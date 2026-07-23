//! [`OwnedSidecar`] — the owner-only lazily-materialised sidecar primitive
//! (R14-9, task #294).
//!
//! ## Why this exists
//!
//! Before this task, `alloc_core::os` (the `SegmentDirectory` sidecar) and
//! `alloc_core::large_cache_extended` (the `LargeCacheExtension` sidecar)
//! each hand-rolled the SAME three-step pattern independently:
//!
//! 1. reserve a `leak_zeroed_pages`-backed span sized (and rounded up) for
//!    one `T`;
//! 2. `ptr::write` a real, compiler-constructed `T` into it (so the sidecar's
//!    initial state is never just "OS-zeroed bytes reinterpreted as `T`" —
//!    see `large_cache_extended`'s own module doc, R14-1/task #286, for why
//!    that reinterpretation is unsound in general even though it happens to
//!    work today for a `[Option<CachedLarge>; N]` payload);
//! 3. dereference the resulting `*mut T` as `&'static T` / `&'static mut T`
//!    through an `unsafe fn` boundary (never a safe wrapper — two safe calls
//!    back to back would materialise aliasing `&'static`/`&'static mut`
//!    references with no `unsafe` token at either call site, real UB under
//!    Stacked/Tree Borrows the type system cannot catch on its own).
//!
//! `os::deref_directory_sidecar[_mut]` predates this discipline: it OS-zeroes
//! `SegmentDirectory` in place (relying on the `numa-aware` `node_ids` field's
//! own explicit `init_node_ids()` re-init immediately afterward for the one
//! field where all-zero is NOT a valid state) and exposes safe `fn`s with a
//! prose-only caller contract instead of `unsafe fn` signatures — the exact
//! "safe fn returns `&'static mut` from `*mut`" gap R14-1 already closed for
//! `large_cache_extended`. This module generalises R14-1's fix into one
//! reusable owner-only primitive and both `os.rs`/`segment_directory.rs` and
//! `large_cache_extended.rs` are migrated onto it, closing the gap for
//! `SegmentDirectory` too and removing the duplicated reserve/init/deref
//! boilerplate.
//!
//! ## Scope: owner-only, NOT the `dirty_by_class`/`PerClassDirty` shape
//!
//! `PerClassDirty` (`alloc_core::dirty_by_class`) is published CROSS-THREAD
//! (any thread's remote free can be the first to materialise it), so it needs
//! a CAS-publish state machine (`racy_ptr_cell::RacyPtrCell`) — a genuinely
//! different concern (concurrent race over WHO materialises) from this
//! module's (a single owning thread's typed init + deref discipline once the
//! pointer already exists). `RacyPtrCell` is itself an independently
//! loom-verified seam crate (`crates/racy-ptr-cell`); folding its CAS
//! protocol into this primitive would either weaken that verification
//! surface or duplicate it. `PerClassDirty` keeps `RacyPtrCell` for
//! publication and is NOT migrated onto this module's `reserve`/`deref`/
//! `deref_mut` at all — two independent reasons:
//!
//! 1. its payload is all-`AtomicU64`, for which all-zero IS a valid initial
//!    state (no niche/padding concerns — unlike `CachedLarge`'s bag of bare
//!    pointer/integer fields), so it needs neither [`reserve`]'s `ptr::write`
//!    nor [`reserve_zeroed_with`]'s fixup closure;
//! 2. it is NEVER dereferenced as `&mut` anywhere in this crate (every
//!    mutation goes through `fetch_or`/`swap` on the atomics themselves), so
//!    the aliasing hazard [`deref`]/[`deref_mut`]'s `unsafe fn` boundary
//!    exists to guard against does not apply — `dirty_by_class.rs`'s
//!    `ensure_per_class_dirty`/`get_per_class_dirty` stay ordinary safe `fn`s
//!    (see that module's own audit note for the full argument).
//!
//! ## API shape
//!
//! `AllocCore` stores each sidecar as a plain `*mut T` field (not
//! `OwnedSidecar<T>` itself): the field's `null` value IS the "not yet
//! materialised" sentinel that `AllocCore::new`/`Drop`/every call site
//! already branches on, and `AllocCore` needs `Copy`-free but `Drop`-free raw
//! storage it can leave null without running a destructor. Wrapping that in
//! an `Option<OwnedSidecar<T>>` would only rename the same null check without
//! removing it. So this module exposes free functions operating on `*mut T`
//! directly — [`reserve`] (produces the pointer, typed-init included) and
//! [`deref`]/[`deref_mut`] (the `unsafe fn` boundary) — rather than a
//! handle type callers must additionally manage.

// Named `unsafe` seam (tier 1): the two documented reasons to hold `unsafe`
// here are (1) `ptr::write`-constructing a fully-typed `T` into the
// freshly-reserved, not-yet-observed page(s) [`reserve`] gets from
// `aligned_vmem::leak_zeroed_pages` — non-null, page-aligned, valid for
// `size_of::<T>()` bytes, and not yet aliased by any other reference, so a
// plain overwrite is sound; and (2) dereferencing the resulting `*mut T` /
// `*const T` as `&'static T` / `&'static mut T` in [`deref`]/[`deref_mut`] —
// sound because the pointer is only ever produced by [`reserve`] (thus
// always pointing at a value typed-initialised by this same module) and the
// caller's owner-only single-writer discipline (documented per call site;
// mirrors `AllocCore`'s "neither `Send` nor `Sync`" invariant) rules out any
// concurrent aliasing writer/reader.
#![allow(unsafe_code)]

use core::ptr;

/// Byte size to reserve for one `T` via [`aligned_vmem::leak_zeroed_pages`]:
/// `size_of::<T>()` rounded up to a multiple of `aligned_vmem::PAGE` (that
/// function's own size contract). A zero-sized `T` still reserves one page —
/// `leak_zeroed_pages(0)` returns `None`, and no sidecar `T` in this crate is
/// actually zero-sized, but the rounding stays total or every caller.
#[must_use]
const fn sidecar_size<T>() -> usize {
    let raw = core::mem::size_of::<T>();
    if raw == 0 {
        aligned_vmem::PAGE
    } else {
        let page = aligned_vmem::PAGE;
        (raw + page - 1) & !(page - 1)
    }
}

/// Reserve a fresh OS-backed span and `ptr::write` `value` into it, returning
/// the resulting `*mut T` — the ONE function that determines a sidecar's
/// initial state. Returns `None` only on OOM (sidecar OOM is NOT allocator
/// OOM; callers treat a `None` here as "the mechanism stays off", never as a
/// hard failure).
///
/// The returned pointer is:
/// - non-null, `PAGE`-aligned (>= `align_of::<T>()` for every realistic
///   sidecar `T` in this crate — all well under one page's alignment);
/// - valid for `size_of::<T>()` bytes;
/// - typed-initialised with EXACTLY `value` (never a reinterpreted all-zero
///   pattern — see the module doc's R14-1 rationale);
/// - leaked for the process lifetime (never freed; `aligned_vmem::
///   leak_zeroed_pages` never releases its reservation).
///
/// Callers store the returned pointer (typically in an `AllocCore` field)
/// and access it ONLY through [`deref`]/[`deref_mut`] thereafter.
#[must_use]
// The only current caller is `large_cache_extended::reserve_large_cache_extension`
// (gated on `large-cache-extended`); `segment_directory`'s sidecar uses
// `reserve_zeroed_with` instead (see that function's own doc for why). A
// build with `alloc-segment-directory` but not `large-cache-extended` would
// otherwise warn this unused — harmless (not part of the CI feature matrix:
// `""`, `experimental`, `--all-features`), silenced for `cargo-hack`-style
// per-feature builds.
#[cfg_attr(not(feature = "large-cache-extended"), allow(dead_code))]
pub(crate) fn reserve<T>(value: T) -> Option<*mut T> {
    let base = aligned_vmem::leak_zeroed_pages(sidecar_size::<T>())?;
    let ptr = base.as_ptr().cast::<T>();
    // SAFETY: `ptr` is non-null, `PAGE`-aligned, and valid for
    // `size_of::<T>()` bytes (`sidecar_size::<T>() >= size_of::<T>()` by
    // construction above) — freshly reserved by `leak_zeroed_pages` and not
    // yet observed by any other reference, so `ptr::write` may construct a
    // fully-typed value there without reading or dropping whatever bytes were
    // already present (a plain overwrite, exactly `ptr::write`'s contract).
    unsafe {
        ptr::write(ptr, value);
    }
    Some(ptr)
}

/// Reserve a fresh OS-backed span, leave it OS-zeroed, then run `fixup` on it
/// **in place** (`&mut T`, no stack copy of `T`) to repair any field(s) for
/// which all-zero bytes are NOT already a valid `T` state, returning the
/// resulting `*mut T`.
///
/// This is [`reserve`]'s sibling for a sidecar `T` where:
/// - all-zero bytes ARE a valid state for MOST fields (e.g. a bitmap of
///   plain `u64`/`AtomicU64` words, where all-zero means "every bit clear" —
///   a real, intentional initial state, not a reinterpreted coincidence), so
///   a full `ptr::write` of a stack-built `T` would be pure waste; but
/// - `T` is large enough (tens of KiB, e.g. `SegmentDirectory` under
///   `numa-aware`) that moving a whole `T` through a by-value function
///   parameter risks an avoidable stack copy `reserve` does not need to pay,
///   AND
/// - one or more specific fields are NOT valid at all-zero (e.g.
///   `SegmentDirectory::node_ids` under `numa-aware`: `0` is a real OS node
///   id, so the zeroed table would misread as "node 0 already claimed bucket
///   0" — see that field's own doc comment) and need an explicit, narrow
///   in-place repair.
///
/// `fixup` receives `&mut T` over the freshly OS-zeroed (and, under `miri`,
/// explicitly-zeroed — see `aligned_vmem::leak_zeroed_pages`) span and must
/// bring every field that is NOT valid at all-zero to a valid state. Fields
/// `fixup` does not touch are left at all-zero, which the caller of this
/// function attests (by choosing this constructor over [`reserve`]) is
/// already a valid `T` state for those fields.
///
/// Returns `None` only on OOM, exactly like [`reserve`].
#[must_use]
pub(crate) fn reserve_zeroed_with<T>(fixup: impl FnOnce(&mut T)) -> Option<*mut T> {
    let base = aligned_vmem::leak_zeroed_pages(sidecar_size::<T>())?;
    let ptr = base.as_ptr().cast::<T>();
    // SAFETY: `ptr` is non-null, `PAGE`-aligned, and valid for
    // `size_of::<T>()` bytes (same construction as `reserve` above). The
    // OS-zeroed bytes are the caller-attested valid initial state for every
    // field `fixup` does not touch (that attestation is this function's
    // documented contract, upheld by every call site in this crate — see the
    // module doc). `&mut *ptr` is sound: freshly reserved, not yet observed
    // by any other reference, so no aliasing.
    let value: &mut T = unsafe { &mut *ptr };
    fixup(value);
    Some(ptr)
}

/// Dereference a sidecar pointer produced by [`reserve`] (or
/// [`reserve_zeroed_with`]) as `&'static T`.
///
/// `unsafe fn`, not a safe function with a prose-only caller contract: two
/// safe-looking calls to this (or [`deref_mut`]) back to back would otherwise
/// materialise aliasing `&'static`/`&'static mut` references with no
/// `unsafe` token at either call site — real UB under Stacked/Tree Borrows
/// the type system could not catch. Requiring `unsafe` at the call site
/// forces every caller to locally justify why the aliasing rule below is
/// upheld.
///
/// # Safety
///
/// - `p` must be non-null and was returned by a prior call to [`reserve`] or
///   [`reserve_zeroed_with`] for this same `T` (so it points at a value this
///   module itself brought to a fully valid `T` state, either via
///   `ptr::write` or via OS-zero-plus-attested-fixup).
/// - The calling thread must be the sole owner of the sidecar for the
///   duration of the returned reference's use (single-writer discipline —
///   e.g. `AllocCore` being neither `Send` nor `Sync`), so no concurrent
///   writer can race this read.
/// - The `&'static` returned must not be held live across any call that may
///   produce a `&'static mut T` to the SAME sidecar (i.e. [`deref_mut`]) —
///   callers must not let the two borrows overlap.
#[inline]
pub(crate) unsafe fn deref<T>(p: *const T) -> &'static T {
    debug_assert!(!p.is_null(), "sidecar::deref: null pointer");
    // SAFETY: caller contract above establishes non-null, properly aligned,
    // valid-for-`size_of::<T>()`-bytes, typed-initialised (via [`reserve`]'s
    // `ptr::write`), leaked for the process lifetime, and free of any live
    // aliasing `&mut`. `&'static` is sound because the allocation outlives
    // any reference (leaked, never freed).
    unsafe { &*p }
}

/// Dereference a sidecar pointer produced by [`reserve`] (or
/// [`reserve_zeroed_with`]) as `&'static mut T`.
///
/// `unsafe fn` for the same reason as [`deref`]: a safe wrapper here would
/// let ordinary safe code produce aliasing `&'static mut` references with no
/// `unsafe` token anywhere, which is UB regardless of the owner-only
/// discipline that makes it data-race-free.
///
/// # Safety
///
/// - `p` must be non-null and was returned by a prior call to [`reserve`] or
///   [`reserve_zeroed_with`] for this same `T`.
/// - The calling thread must be the sole owner of the sidecar for the
///   duration of the returned reference's use, so no concurrent reader or
///   writer can race this access.
/// - No other reference (shared or mutable) to this sidecar may be live for
///   the duration of the returned `&'static mut`'s use.
#[inline]
pub(crate) unsafe fn deref_mut<T>(p: *mut T) -> &'static mut T {
    debug_assert!(!p.is_null(), "sidecar::deref_mut: null pointer");
    // SAFETY: caller contract above establishes non-null, properly aligned,
    // valid-for-`size_of::<T>()`-bytes, typed-initialised, leaked for the
    // process lifetime, and free of any other live reference. `&'static mut`
    // is sound because the allocation outlives any reference (leaked, never
    // freed) and the owner-only discipline rules out aliasing.
    unsafe { &mut *p }
}
