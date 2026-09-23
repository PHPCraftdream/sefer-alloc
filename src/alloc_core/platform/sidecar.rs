//! [`sidecar`] — the owner-only lazily-materialised sidecar primitive
//! (R14-9, task #294; reservation-owning since R2-12).
//!
//! ## Why this exists
//!
//! Before this task, `alloc_core::os` (the `SegmentDirectory` sidecar) and
//! `alloc_core::large_cache_extended` (the `LargeCacheExtension` sidecar)
//! each hand-rolled the SAME three-step pattern independently:
//!
//! 1. reserve a zeroed OS span sized (and rounded up) for one `T`;
//! 2. `ptr::write` a real, compiler-constructed `T` into it (so the sidecar's
//!    initial state is never just "OS-zeroed bytes reinterpreted as `T`" —
//!    see `large_cache_extended`'s own module doc, R14-1/task #286, for why
//!    that reinterpretation is unsound in general even though it happens to
//!    work today for a `[Option<CachedLarge>; N]` payload);
//! 3. dereference the resulting `*mut T` through an `unsafe fn` boundary
//!    (never a safe wrapper — two safe calls back to back would materialise
//!    aliasing `&`/`&mut` references with no `unsafe` token at either call
//!    site, real UB under Stacked/Tree Borrows the type system cannot catch
//!    on its own).
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
//! ## R2-12: the sidecar OWNS its VM reservation — never leaked
//!
//! Up to and including R14-9, step 1 used `aligned_vmem::leak_zeroed_pages`:
//! the backing span was deliberately leaked for the process lifetime and
//! never released. That was only defensible for the bounded population of
//! process-lifetime registry heaps (`HeapCore`'s `AllocCore` is never
//! dropped, so the per-heap cost is bounded by `MAX_HEAPS`); it did NOT hold
//! for the public standalone `AllocCore::new`/drop path, where repeated
//! create/destroy cycles that materialise a sidecar accumulated unrecoverable
//! VM spans without bound (R2-12).
//!
//! Since R2-12, [`reserve`]/[`reserve_zeroed_with`] reserve through
//! `aligned_vmem::reserve_aligned` and return the span wrapped in an
//! [`AccountedSidecar`] — a kind-tagged RAII token holding the
//! `aligned_vmem::Reservation`. The owning `AllocCore` stores the token
//! alongside the sidecar pointer and dropping the core drops the token,
//! releasing the span back to the OS exactly once (the release is counted by
//! `platform::sidecar_stats`' process-wide reservation/release counters, the
//! R2-12 acceptance oracle). The only spans still reserved for the process
//! lifetime are the explicitly-sanctioned process-global ones: registry
//! heaps' sidecars (never dropped, bounded by `MAX_HEAPS`) and the
//! cross-thread-published sidecars (`dirty_by_class`'s `PerClassDirty`,
//! `registry::bootstrap`'s `HeapOverflowSidecar`), each of which documents
//! its own process-global justification at its reservation site.
//!
//! Because the span can now be released while the process lives, the
//! references [`deref`]/[`deref_mut`] hand out are OWNER-TIED, not
//! `'static`: the caller passes the owning value it borrowed the pointer
//! from and the returned reference's lifetime is bounded by that owner, so
//! the type system structurally prevents a reference from outliving the
//! span's release.
//!
//! ## Scope: owner-only, NOT the `dirty_by_class`/`PerClassDirty` shape
//!
//! `PerClassDirty` (`alloc_core::dirty_by_class`) is published CROSS-THREAD
//! (any thread's remote free can be the first to materialise it), so it needs
//! a CAS-publish state machine (`once_ptr_cell::OncePtrCell`) — a genuinely
//! different concern (concurrent race over WHO materialises) from this
//! module's (a single owning thread's typed init + deref discipline once the
//! pointer already exists). `OncePtrCell` is itself an independently
//! loom-verified seam crate (`crates/once-ptr-cell`); folding its CAS
//! protocol into this primitive would either weaken that verification
//! surface or duplicate it. `PerClassDirty` keeps `OncePtrCell` for
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
//! (The R2-12 ownership fix does not change this: `PerClassDirty` is
//! materialised by whichever cross-thread producer wins the CAS race, so no
//! single owner could hold a release token — it stays a documented
//! process-global leak, bounded by `MAX_HEAPS` slots. See that module's own
//! R2-12 note.)
//!
//! ## API shape
//!
//! `AllocCore` stores each sidecar as a plain `*mut T` field (not
//! [`AccountedSidecar`] itself): the field's `null` value IS the "not yet
//! materialised" sentinel that `AllocCore::new`/`Drop`/every call site
//! already branches on, and `AllocCore` needs `Copy`-free but `Drop`-free raw
//! storage it can leave null without running a destructor. Wrapping that in
//! an `Option<...>` would only rename the same null check without removing
//! it. So this module exposes free functions operating on `*mut T` directly
//! — [`reserve`] (produces the pointer + token, typed-init included) and
//! [`deref`]/[`deref_mut`] (the `unsafe fn` boundary) — plus the token type
//! the caller stores alongside the pointer. The pairing invariant (token is
//! `Some` iff the pointer is non-null) is maintained at the two
//! materialisation sites and consumed by `AllocCore`'s own `Drop`; see the
//! field docs in `alloc_core::alloc_core`.

// Named `unsafe` seam (tier 1): the four documented reasons to hold
// `unsafe` here are (1) `ptr::write`-constructing a fully-typed `T` into the
// freshly-reserved, not-yet-observed page(s) [`AccountedSidecar::reserve`]
// gets from `aligned_vmem::reserve_aligned` — non-null, page-aligned, valid
// for `size_of::<T>()` bytes, and not yet aliased by any other reference, so
// a plain overwrite is sound; (2) handing the caller-supplied `fixup` closure
// in [`reserve_zeroed_with`] a raw `*mut T` over the freshly OS-zeroed (not
// yet fully-valid) span — NEVER a `&mut T` (R17-1, task #318: a `&mut T`
// requires `T` be fully valid at the moment the reference is created, which
// does not hold here for a generic `T` before `fixup` has run; the raw
// pointer sidesteps that requirement, and `fixup` itself is contractually
// required to repair the not-yet-valid fields through raw-pointer writes
// only, never by materialising a reference over the not-yet-valid value) —
// sound only because the caller's `# Safety` contract attests every field
// `fixup` leaves untouched is already valid at all-zero and that `fixup`
// upholds the same no-reference-over-invalid-`T` discipline, an attestation
// this function cannot check itself; (3) under `miri`, explicitly zeroing
// the freshly-reserved span in [`AccountedSidecar::reserve`] — sound because
// the reservation is exclusively owned and not yet observed by any other
// reference (miri's `std::alloc` fallback does not zero; every real OS
// backend hands back zeroed anonymous pages, the same discipline
// `aligned_vmem::leak_zeroed_pages` documents); and (4) dereferencing the
// resulting `*mut T` / `*const T` as `&'a T` / `&'a mut T` in [`deref`]/[`deref_mut`]
// — sound because the pointer is only ever produced by
// [`reserve`] or [`reserve_zeroed_with`] (thus always pointing at a value
// brought to a fully valid `T` state by this same module, or by `fixup`
// under that same module's contract), the caller's owner-only
// single-writer discipline (documented per call site; mirrors
// `AllocCore`'s "neither `Send` nor `Sync`" invariant) rules out any
// concurrent aliasing writer/reader, and the owner-tied lifetime plus the
// owner's held [`AccountedSidecar`] (whose `Drop` releases the span only
// after the owner's own `Drop` body has finished) guarantee no returned
// reference can outlive the span.
#![allow(unsafe_code)]

use core::ptr;

/// Byte size to reserve for one `T`: `size_of::<T>()` rounded up to a
/// multiple of `aligned_vmem::PAGE` (the reservation size contract). A
/// zero-sized `T` still reserves one page — `reserve_aligned` rejects zero
/// sizes, and no sidecar `T` in this crate is actually zero-sized, but the
/// rounding stays total for every caller.
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

/// R2-12: which owner-sidecar a reservation belongs to — the key the
/// process-wide reservation/release accounting
/// (`platform::sidecar_stats`) is broken out by. Exactly one of the two
/// directory flavors is live per build (`numa-aware` selects
/// [`SidecarKind::NumaDirectory`], whose span is multiplied by
/// `NODE_BITMAPS`); the extension flavor is live only under
/// `large-cache-extended`.
// The enum is inert (variants never constructed) in builds with neither
// `alloc-segment-directory` nor `large-cache-extended` — e.g. `hardened
// medium-classes internals` (R23-5, task #374) — without being compiled out;
// the same rationale as the `#[cfg_attr(not(feature = ...), allow(dead_code))]`
// pattern on this module's reserve/deref fns, applied item-level because the
// live/dead axis here is a cross-product of two independent features.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SidecarKind {
    /// The `SegmentDirectory` sidecar under non-`numa-aware` builds.
    Directory,
    /// The `SegmentDirectory` sidecar under `numa-aware` (per-node bitmaps).
    NumaDirectory,
    /// The `LargeCacheExtension` sidecar (`large-cache-extended`).
    LargeCacheExtension,
}

/// R2-12: the accounted, owner-held VM reservation backing one lazily-
/// materialised owner-only sidecar. RAII: dropping the token releases the
/// whole OS reservation exactly once (via `aligned_vmem::Reservation`'s own
/// `Drop`) and records the release in the process-wide sidecar accounting.
/// `AllocCore` stores one token per materialised sidecar, paired with the
/// sidecar's raw pointer field (token `Some` iff pointer non-null — see the
/// "API shape" section of the module doc), so the span's lifetime is exactly
/// the owning core's lifetime. Inert in builds with neither
/// `alloc-segment-directory` nor `large-cache-extended` — same allow
/// rationale as [`SidecarKind`].
#[allow(dead_code)]
pub(crate) struct AccountedSidecar {
    kind: SidecarKind,
    vm: aligned_vmem::Reservation,
}

#[allow(dead_code)]
impl AccountedSidecar {
    /// Reserve `size` bytes (must be a non-zero multiple of
    /// `aligned_vmem::PAGE` — [`sidecar_size`] produces that) of anonymous
    /// virtual memory for one sidecar of `kind`, returning the accounted
    /// token. The span is all-zero: every real OS backend hands back zeroed
    /// anonymous pages, and under `miri` (whose `std::alloc` fallback does
    /// NOT zero) the span is explicitly zeroed here — the same guarantee
    /// `aligned_vmem::leak_zeroed_pages` documented and provided before
    /// R2-12. Returns `None` only on OOM (sidecar OOM is NOT allocator OOM;
    /// callers treat a `None` as "the mechanism stays off", never as a hard
    /// failure); a `None` records nothing in the accounting.
    pub(crate) fn reserve(kind: SidecarKind, size: usize) -> Option<Self> {
        let vm = aligned_vmem::reserve_aligned(size, aligned_vmem::PAGE)?;
        #[cfg(miri)]
        // SAFETY: `vm.as_ptr()` is the non-null, page-aligned base of a
        // fresh, exclusively-owned reservation of `vm.len()` bytes; nothing
        // else references it yet, so writing zeros is sound.
        unsafe {
            core::ptr::write_bytes(vm.as_ptr(), 0, vm.len());
        }
        crate::alloc_core::platform::sidecar_stats::record_sidecar_reservation(kind);
        Some(Self { kind, vm })
    }

    /// The usable base pointer of the reserved span (non-null, `PAGE`-aligned,
    /// valid for the reserved size until the token is dropped).
    pub(crate) fn ptr(&self) -> *mut u8 {
        self.vm.as_ptr()
    }
}

impl Drop for AccountedSidecar {
    fn drop(&mut self) {
        // The actual OS release happens when the inner `aligned_vmem::
        // Reservation` field drops immediately after this body; the release
        // event is recorded here so the accounting's reserve/release balance
        // is observable exactly at the token's drop point.
        crate::alloc_core::platform::sidecar_stats::record_sidecar_release(self.kind);
    }
}

/// Reserve a fresh OS-backed span and `ptr::write` `value` into it, returning
/// the resulting `*mut T` and its owned [`AccountedSidecar`] — the ONE
/// function that determines a sidecar's initial state. Returns `None` only on
/// OOM (sidecar OOM is NOT allocator OOM; callers treat a `None` here as "the
/// mechanism stays off", never as a hard failure).
///
/// The returned pointer is:
/// - non-null, `PAGE`-aligned (>= `align_of::<T>()` for every realistic
///   sidecar `T` in this crate — all well under one page's alignment);
/// - valid for `size_of::<T>()` bytes;
/// - typed-initialised with EXACTLY `value` (never a reinterpreted all-zero
///   pattern — see the module doc's R14-1 rationale);
/// - owned by the returned [`AccountedSidecar`]: the span lives until the
///   caller drops the token (for this crate's callers, with the owning
///   `AllocCore` — R2-12), NOT for the process lifetime.
///
/// Callers store the returned pointer (typically in an `AllocCore` field)
/// alongside the token, and access the pointer ONLY through [`deref`]/[`deref_mut`]
/// thereafter.
#[must_use]
// The only current caller is `large_cache_extended::reserve_large_cache_extension`
// (gated on `large-cache-extended`); `segment_directory`'s sidecar uses
// `reserve_zeroed_with` instead (see that function's own doc for why). A
// build with `alloc-segment-directory` but not `large-cache-extended` would
// otherwise warn this unused — harmless (not part of the CI feature matrix:
// `""`, `experimental`, `--all-features`), silenced for `cargo-hack`-style
// per-feature builds.
#[cfg_attr(not(feature = "large-cache-extended"), allow(dead_code))]
pub(crate) fn reserve<T>(kind: SidecarKind, value: T) -> Option<(*mut T, AccountedSidecar)> {
    // Formalises the doc comment above: the reservation only guarantees
    // `PAGE`-alignment, so any `T` whose `align_of` exceeds one page would
    // silently receive an under-aligned pointer. Fails at monomorphization
    // time (compile error), not at runtime, for any such `T`.
    const { assert!(core::mem::align_of::<T>() <= aligned_vmem::PAGE) };
    let sidecar = AccountedSidecar::reserve(kind, sidecar_size::<T>())?;
    let ptr = sidecar.ptr().cast::<T>();
    // SAFETY: `ptr` is non-null, `PAGE`-aligned, and valid for
    // `size_of::<T>()` bytes (`sidecar_size::<T>() >= size_of::<T>()` by
    // construction above) — freshly reserved by `AccountedSidecar::reserve`
    // and not yet observed by any other reference, so `ptr::write` may
    // construct a fully-typed value there without reading or dropping
    // whatever bytes were already present (a plain overwrite, exactly
    // `ptr::write`'s contract).
    unsafe {
        ptr::write(ptr, value);
    }
    Some((ptr, sidecar))
}

/// Reserve a fresh OS-backed span, leave it OS-zeroed, then run `fixup` on it
/// **in place** (a raw `*mut T`, no stack copy of `T`) to repair any field(s)
/// for which all-zero bytes are NOT already a valid `T` state, returning the
/// resulting `*mut T` and its owned [`AccountedSidecar`].
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
/// `fixup` receives a raw `*mut T` over the freshly OS-zeroed (and, under
/// `miri`, explicitly-zeroed — see [`AccountedSidecar::reserve`]) span
/// and must bring every field that is NOT valid at all-zero to a valid
/// state, writing through raw-pointer operations (e.g.
/// `core::ptr::addr_of_mut!((*p).field).write(...)`) WITHOUT EVER
/// materialising a `&`/`&mut T` (or a `&`/`&mut` to any field of the
/// not-yet-fully-valid `T`) — see the `# Safety` section below for why (R17-1,
/// task #318). Fields `fixup` does not touch are left at all-zero, which the
/// caller of this function attests (by choosing this constructor over
/// [`reserve`]) is already a valid `T` state for those fields.
///
/// Returns `None` only on OOM, exactly like [`reserve`].
///
/// The only current caller is `os::reserve_directory_sidecar` (gated on
/// `alloc-segment-directory`). A build with that feature off — e.g.
/// `hardened medium-classes` (R23-5, task #374) — would otherwise warn this
/// unused; harmless (not part of the CI feature matrix: `""`, `experimental`,
/// `--all-features`, all of which either have the feature on or exercise this
/// function through `--all-features`), silenced for `cargo-hack`-style
/// per-feature builds — same pattern as [`reserve`]'s own
/// `#[cfg_attr]` above.
///
/// `unsafe fn`, not a safe function with a prose-only caller contract: `T` is
/// generic and not every `T` is valid at all-zero (e.g. an enum without a
/// zero discriminant, `&U`, `NonNull<U>`, a function pointer, a `bool` byte
/// other than 0/1) — before `fixup` has run, the freshly OS-zeroed span may
/// not hold a valid `T` at all. Rust requires a `&mut T` be fully valid at
/// the instant the reference is created, so materialising one over those
/// bytes would itself be immediate UB regardless of what `fixup` does
/// afterward — this is why `fixup` is handed a raw `*mut T` rather than a
/// `&mut T` (R17-1, task #318: the pre-fix implementation DID materialise a
/// `&mut T` here, which was unsound for a general `T` even though the one
/// concrete `T` used in this crate today, `SegmentDirectory`, happens to be
/// valid at all-zero in every field `fixup` does not touch). Requiring
/// `unsafe` at the call site forces the caller to locally justify both that
/// every field `fixup` leaves untouched is valid at all-zero AND that
/// `fixup` itself never materialises a reference over the not-yet-valid `T`.
///
/// # Safety
///
/// - Every field of `T` NOT written by `fixup` must be valid when its bytes
///   are all-zero (e.g. a bitmap of plain `u64`/`AtomicU64` words, where
///   all-zero means "every bit clear" — a real, intentional state, not a
///   coincidence). `fixup` itself is trusted to repair the remaining
///   field(s) to a valid state before returning.
/// - `fixup` must repair those field(s) using ONLY raw-pointer writes (e.g.
///   `core::ptr::addr_of_mut!((*p).field).write(...)`, or an equivalent
///   `write`/`write_unaligned` through a raw pointer derived without going
///   through a reference) — it must NOT construct a `&T`/`&mut T` over `*p`
///   or over any field of `*p`, whether directly or via an intermediate
///   reference, before every field of `*p` is known to hold a valid value.
/// - The returned pointer must be treated exactly like one returned by
///   [`reserve`]: stored once (alongside its [`AccountedSidecar`]), then
///   accessed only through [`deref`]/[`deref_mut`] under the same owner-only
///   single-writer discipline.
#[must_use]
#[cfg_attr(not(feature = "alloc-segment-directory"), allow(dead_code))]
pub(crate) unsafe fn reserve_zeroed_with<T>(
    kind: SidecarKind,
    fixup: impl FnOnce(*mut T),
) -> Option<(*mut T, AccountedSidecar)> {
    // Formalises the doc comment above: the reservation only guarantees
    // `PAGE`-alignment, so any `T` whose `align_of` exceeds one page would
    // silently receive an under-aligned pointer. Fails at monomorphization
    // time (compile error), not at runtime, for any such `T`.
    const { assert!(core::mem::align_of::<T>() <= aligned_vmem::PAGE) };
    let sidecar = AccountedSidecar::reserve(kind, sidecar_size::<T>())?;
    let ptr = sidecar.ptr().cast::<T>();
    // No `unsafe` block needed here: `ptr` is handed to `fixup` as a raw
    // pointer, never dereferenced by this function itself. `ptr` is
    // non-null, `PAGE`-aligned, and valid for `size_of::<T>()` bytes (same
    // construction as `reserve` above) — freshly reserved, not yet observed
    // by any other reference. The caller's `# Safety` contract (this
    // function's own docs) requires `fixup` to bring every not-valid-at-zero
    // field to a valid state using raw-pointer writes only, never by
    // materialising a reference over the not-yet-valid value.
    fixup(ptr);
    Some((ptr, sidecar))
}

/// Dereference a sidecar pointer produced by [`reserve`] (or
/// [`reserve_zeroed_with`]) as `&T`, with the lifetime of the
/// `owner` reference the caller already holds (R2-12: OWNER-TIED, not
/// `'static` — the span is released when the owner's [`AccountedSidecar`]
/// drops, so a `'static` reference would be unsound).
///
/// `unsafe fn`, not a safe function with a prose-only caller contract: two
/// safe-looking calls to this (or [`deref_mut`]) back to back would otherwise
/// materialise aliasing `&`/`&mut` references with no
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
///   `ptr::write` or via OS-zero-plus-attested-fixup), and the owning
///   [`AccountedSidecar`] for this span must not have been dropped (for this
///   crate's callers: the owning `AllocCore` must still be alive — the
///   `owner` reference passed here enforces exactly that bound).
/// - The calling thread must be the sole owner of the sidecar for the
///   duration of the returned reference's use (single-writer discipline —
///   e.g. `AllocCore` being neither `Send` nor `Sync`), so no concurrent
///   writer can race this read.
/// - The returned `&T` must not be held live across any call that may
///   produce a `&'a mut T` to the SAME sidecar (i.e. [`deref_mut`]) —
///   callers must not let the two borrows overlap.
///
/// Two independent consumer groups call this: `alloc_core_small.rs`'s
/// `directory`/`directory_mut`/`maybe_materialize_directory` and
/// `alloc_core_core_diag.rs`'s `dbg_rebuild_directory` (all gated on
/// `alloc-segment-directory`), and `large_cache_extended.rs`'s
/// `deref_large_cache_extension`/`deref_large_cache_extension_mut` (gated on
/// `large-cache-extended`) — either feature alone keeps this function used;
/// a build with BOTH off — e.g. `hardened medium-classes` (R23-5, task #374)
/// — would otherwise warn this unused; harmless (not part of the CI feature
/// matrix), silenced for `cargo-hack`-style per-feature builds — same
/// pattern as [`reserve`]'s own `#[cfg_attr]` above.
#[inline]
#[cfg_attr(
    not(any(feature = "alloc-segment-directory", feature = "large-cache-extended")),
    allow(dead_code)
)]
pub(crate) unsafe fn deref<T, O>(p: *const T, _owner: &O) -> &T {
    debug_assert!(!p.is_null(), "sidecar::deref: null pointer");
    // SAFETY: caller contract above establishes non-null, properly aligned,
    // valid-for-`size_of::<T>()`-bytes, typed-initialised (via [`reserve`]'s
    // `ptr::write`), alive for `'a` (the owner holds the sidecar's
    // [`AccountedSidecar`], so the span cannot be released while `owner` is
    // live), and free of any live aliasing `&mut`. The owner-tied lifetime
    // makes it unrepresentable for the returned reference to outlive the
    // span's release.
    unsafe { &*p }
}

/// Dereference a sidecar pointer produced by [`reserve`] (or
/// [`reserve_zeroed_with`]) as `&mut T`, with the lifetime of the
/// `owner` reference the caller already holds (R2-12: OWNER-TIED, not
/// `'static` — same reasoning as [`deref`]).
///
/// `unsafe fn` for the same reason as [`deref`]: a safe wrapper here would
/// let ordinary safe code produce aliasing `&mut` references with no
/// `unsafe` token anywhere, which is UB regardless of the owner-only
/// discipline that makes it data-race-free.
///
/// # Safety
///
/// - `p` must be non-null and was returned by a prior call to [`reserve`] or
///   [`reserve_zeroed_with`] for this same `T`, and the owning
///   [`AccountedSidecar`] for this span must not have been dropped (the
///   `owner` reference bounds this).
/// - The calling thread must be the sole owner of the sidecar for the
///   duration of the returned reference's use, so no concurrent reader or
///   writer can race this access.
/// - No other reference (shared or mutable) to this sidecar may be live for
///   the duration of the returned `&mut`'s use.
///
/// Same two independent consumer groups as [`deref`] (see its doc comment):
/// `alloc-segment-directory` and `large-cache-extended`, either sufficient
/// alone.
#[inline]
#[cfg_attr(
    not(any(feature = "alloc-segment-directory", feature = "large-cache-extended")),
    allow(dead_code)
)]
// `clippy::mut_from_ref` is a false positive here: the returned `&mut T`
// does NOT derive its mutability from the shared `_owner` reference (whose
// pointee is the owning struct, not this sidecar's span) — it derives from
// the caller-attested-exclusive raw pointer `p`, whose exclusivity this
// `unsafe fn`'s `# Safety` contract above governs. `_owner` is a pure
// LIFETIME BINDER (R2-12: the returned reference must not outlive the
// owner that holds the sidecar's `AccountedSidecar` reservation token) and
// is never dereferenced; an `&mut O` owner would additionally break the
// legitimate call sites that hold the returned `&mut SegmentDirectory`
// while passing `&self.table` as a disjoint second argument.
#[allow(clippy::mut_from_ref)]
pub(crate) unsafe fn deref_mut<T, O>(p: *mut T, _owner: &O) -> &mut T {
    debug_assert!(!p.is_null(), "sidecar::deref_mut: null pointer");
    // SAFETY: caller contract above establishes non-null, properly aligned,
    // valid-for-`size_of::<T>()`-bytes, typed-initialised, alive for `'a`
    // (the owner holds the sidecar's [`AccountedSidecar`]), and free of any
    // other live reference. The owner-tied lifetime makes it
    // unrepresentable for the returned reference to outlive the span's
    // release, and the owner-only discipline rules out aliasing.
    unsafe { &mut *p }
}
