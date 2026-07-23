//! R13-7 (task #277, EXPERIMENTAL `large-cache-extended`):
//! [`LargeCacheExtension`] — a lazily-materialised sidecar that widens
//! `AllocCore`'s large-segment free-cache from the fixed 8 base slots
//! (`LARGE_CACHE_SLOTS`, `alloc_core.rs`) to `8 + LARGE_CACHE_EXTENDED_SLOTS`
//! (8 + 32 = 40 total).
//!
//! ## Why this exists
//!
//! `docs/perf/R13_6_EXACT_SPAN_RESERVED_CAPACITY_PRODUCTION_GATE.md` §4
//! measured that under `exact-span-large` (R12-3), distinct Large request
//! sizes stop aliasing onto the same rounded-to-`SEGMENT` `usable` value —
//! each distinct size now occupies its OWN cache slot instead of sharing one.
//! A working set of 16+ distinct Large sizes can exhaust the fixed 8 slots,
//! forcing every eviction-then-refill cycle back onto the OS round-trip
//! (µs-level) that the cache exists to avoid. This sidecar gives such
//! workloads headroom to keep more DISTINCT sizes resident, while the
//! byte-budget (`large_cache_budget_bytes`) remains the PRIMARY control on
//! total cached RSS — the extension only relaxes the SLOT-COUNT ceiling, not
//! the byte ceiling: a small budget still bounds the cache tightly regardless
//! of how many slots exist.
//!
//! ## Design choice: lazy sidecar, not size-bucketed
//!
//! Two designs were on the table (see task brief): (a) a lazily-materialised
//! flat extension array, (b) a size-bucketed cache (distinct slot ranges per
//! size band). (a) was chosen because:
//!
//! - It reuses the EXACT matching/eviction algorithms already proven correct
//!   for the base 8 slots (best-fit scan by `usable_size` within
//!   `LARGE_CACHE_SIZE_FACTOR`, FIFO-oldest-`seq` eviction) — a size-bucketed
//!   design would need bucket-boundary policy (how many buckets, what
//!   ranges, what happens at a boundary-straddling best-fit query) that adds
//!   real design surface for a benefit ((b)'s only edge over (a) is avoiding
//!   an O(slots) linear scan) that does not matter here: every touch site is
//!   already the COLD large-alloc/dealloc slow path, never the hot small-object
//!   path, so O(40) vs O(a-few-buckets) is noise (see `LARGE_CACHE_SLOTS`'s
//!   own doc: the existing O(LARGE_CACHE_SLOTS) scan is "cheap" at 8 and stays
//!   cheap at 40 for the same reason).
//! - It mirrors an ALREADY-ESTABLISHED codebase pattern for exactly this
//!   shape of problem — a fixed inline array that is enough for the common
//!   case, backed by a lazily-materialised, `leak_zeroed_pages`-reserved
//!   sidecar that only costs anything once the workload genuinely needs it
//!   (`directory_sidecar`/`SegmentDirectory` in `segment_directory.rs`, and
//!   `dirty_by_class`/`PerClassDirty` in `dirty_by_class.rs`). Reusing the
//!   pattern means zero-overhead-when-off is automatic (same argument those
//!   two modules' docs already make) and reviewers already know the shape.
//!
//! ## Owner-only — plain `*mut`, NOT `RacyPtrCell`
//!
//! Unlike `dirty_by_class`'s `PerClassDirty` (written by ANY cross-thread
//! producer via a remote free, hence `RacyPtrCell`'s CAS-publish protocol),
//! the large-cache is exactly like `directory_sidecar`/`SegmentDirectory`:
//! touched ONLY by this `AllocCore`'s owning thread (`alloc_large`/`dealloc`/
//! `reclaim_large_segment` all run on the single owning thread). `AllocCore`
//! is deliberately NEITHER `Send` NOR `Sync` (see the "NOTE: `AllocCore` is
//! intentionally NOT `Send` (nor `Sync`)" comment above its `Drop` impl in
//! `alloc_core.rs`) — the safety of a plain, non-atomic `*mut
//! LargeCacheExtension` here does not come from a `Send`/`Sync` marker at
//! all, it comes from the stronger owner-only single-thread discipline: an
//! `AllocCore` value never crosses a thread boundary in the first place
//! (Phase 8 is single-threaded by construction), so there is no concurrent
//! producer to race even in principle. A plain `*mut LargeCacheExtension`,
//! materialised lazily and dereferenced through the shared owner-only
//! `alloc_core::sidecar` primitive (R14-9, task #294; originally established
//! by `os::deref_directory_sidecar[_mut]`, generalised into `sidecar::deref[_mut]`
//! and reused by both), is the correct-weight primitive here — no atomics, no
//! CAS, no loom coverage needed for the publish step (there is no concurrent
//! publisher to race).
//!
//! ## Sizing
//!
//! `LARGE_CACHE_EXTENDED_SLOTS = 32` gives 8 (base, always resident) + 32
//! (lazy) = 40 total slots — inside the "32-64 entries" range the task brief
//! names, chosen at the low end: each slot is `size_of::<CachedLarge>()`
//! bytes (six machine words), and the sidecar is reserved via
//! `leak_zeroed_pages` (whole-PAGE granularity) only once a workload's
//! working set of DISTINCT Large sizes genuinely exceeds 8 — a rare
//! configuration outside the exact-span-large-plus-wide-working-set scenario
//! this task targets. 32 extra slots comfortably covers the "16-32 distinct
//! Large sizes" judge workload from the task brief (up to 40 total) without
//! reserving for a much larger N speculatively; a future task can raise this
//! constant if a wider working set is demonstrated to need it (the same
//! "measure first" posture `LARGE_CACHE_SLOTS` itself documents: it grew
//! from 2 to 8 only after task D1 demonstrated the need).
//!
//! `large-cache-extended` is EXPERIMENTAL: opt-in, additive over
//! `alloc-decommit`, NOT part of `production`. With the feature OFF, this
//! module does not exist in the binary, `AllocCore`'s field list, `Drop`, and
//! every large-cache algorithm are BYTE-FOR-BYTE IDENTICAL to before this
//! task (behavioural counterpart verified in
//! `tests/large_cache_extended_off_no_overflow_capacity.rs`).

// Named `unsafe` seam: the SOLE documented reason to hold `unsafe` here is
// dereferencing a materialised sidecar pointer as `&'static
// LargeCacheExtension` / `&'static mut LargeCacheExtension` in
// `deref_large_cache_extension[_mut]` — sound because the pointer is only
// ever produced by `reserve_large_cache_extension` in this same module (via
// `sidecar::reserve`, which typed-initialises it — see that function's own
// safety contract) and the owner-only single-writer discipline (`AllocCore`
// is neither `Send` nor `Sync`; see its own doc comment) rules out any
// concurrent aliasing writer/reader. (R14-9, task #294: the typed-init
// `ptr::write` step itself now lives in `alloc_core::sidecar::reserve`,
// shared with `os.rs`'s directory-sidecar reservation — this module no
// longer duplicates it.)
#![allow(unsafe_code)]

use super::alloc_core::CachedLarge;

/// Number of additional slots held in the lazily-materialised extension
/// sidecar, on top of the `LARGE_CACHE_SLOTS` (8) base array. See the module
/// doc's "Sizing" section for the rationale.
pub(crate) const LARGE_CACHE_EXTENDED_SLOTS: usize = 32;

/// The lazily-materialised large-cache extension sidecar. See the module doc
/// for the full design. Owner-only (no atomics — mirrors `SegmentDirectory`,
/// not `PerClassDirty`).
pub(crate) struct LargeCacheExtension {
    pub(super) slots: [Option<CachedLarge>; LARGE_CACHE_EXTENDED_SLOTS],
}

/// Reserve and construct a [`LargeCacheExtension`] sidecar via direct OS VM
/// reservation (M5-clean — `AllocCore`'s alloc path must not recurse into
/// `std::alloc`/`Vec`/`Box`; see `alloc_core.rs`'s module doc). Returns
/// `Some(ptr)` on success (valid for the process lifetime, EXPLICITLY
/// typed-initialised via [`super::sidecar::reserve`] — the OS-zeroed bytes
/// `leak_zeroed_pages` hands back are never trusted as-is to already encode
/// an all-`None` `[Option<CachedLarge>; _]`; whether `Option`'s all-zero
/// bytes form a valid niche encoding for a given payload type is an
/// UNSPECIFIED, rustc-version- and layout-dependent detail of `repr(Rust)`,
/// not a language guarantee — see `CachedLarge`'s own layout in
/// `alloc_core.rs`, a bag of bare `*mut u8`/`usize`/`u64` fields with no
/// reserved niche the compiler is obligated to place there. `sidecar::reserve`
/// writes a real, compiler-constructed `[None; LARGE_CACHE_EXTENDED_SLOTS]`
/// value into the reserved page(s) before the pointer is returned/published,
/// so every `deref_large_cache_extension[_mut]` call thereafter reads a value
/// the compiler itself produced as `None`, not a reinterpreted zero pattern),
/// or `None` on OOM (sidecar OOM is NOT allocator OOM — the extension
/// mechanism simply stays off for this heap and the cache is capped at the 8
/// base slots, exactly as if this feature did not exist).
///
/// Uses [`super::sidecar::reserve`] (full typed move-in), not
/// [`super::sidecar::reserve_zeroed_with`]: `LargeCacheExtension` is small
/// (`LARGE_CACHE_EXTENDED_SLOTS` * `size_of::<Option<CachedLarge>>()`, well
/// under 2 KiB), so moving a stack-built value through the by-value
/// constructor is cheap, and — unlike `SegmentDirectory` — EVERY field needs
/// real typed initialisation (all-zero is not an attested-valid `Option<CachedLarge>`
/// state), so there is no all-zero-valid subset for a `reserve_zeroed_with`
/// fixup to leave untouched.
#[must_use]
pub(crate) fn reserve_large_cache_extension() -> Option<*mut LargeCacheExtension> {
    super::sidecar::reserve(LargeCacheExtension {
        slots: [const { None }; LARGE_CACHE_EXTENDED_SLOTS],
    })
}

/// Dereference a materialised extension sidecar pointer as
/// `&LargeCacheExtension`. Thin forwarder to [`super::sidecar::deref`]
/// (R14-9, task #294) — kept as a named, `LargeCacheExtension`-typed function
/// so call sites read the same as before the sidecar primitive was
/// extracted.
///
/// `unsafe fn` for the same reason [`super::sidecar::deref`] itself is: two
/// safe-looking calls to this (or [`deref_large_cache_extension_mut`]) back
/// to back would otherwise materialise aliasing `&'static`/`&'static mut`
/// references with no `unsafe` token at either call site — real UB under
/// Stacked/Tree Borrows that the type system could not catch.
///
/// # Safety
///
/// - `p` must be non-null and was returned by [`reserve_large_cache_extension`]
///   (so it points at a value [`super::sidecar::reserve`] typed-initialised).
/// - The calling thread is the sole owner (`AllocCore` is neither `Send` nor
///   `Sync`; see its doc comment in `alloc_core.rs`), so no concurrent
///   writer can race this read.
/// - The `&'static` returned must not be held live across any call that may
///   produce a `&mut LargeCacheExtension` to the SAME sidecar (i.e.
///   [`deref_large_cache_extension_mut`]) — callers must not let the two
///   borrows overlap.
#[inline]
pub(crate) unsafe fn deref_large_cache_extension(
    p: *const LargeCacheExtension,
) -> &'static LargeCacheExtension {
    // SAFETY: forwarded verbatim from this function's own caller contract
    // above, which is `super::sidecar::deref`'s contract specialised to
    // `LargeCacheExtension`.
    unsafe { super::sidecar::deref(p) }
}

/// Dereference a materialised extension sidecar pointer as
/// `&mut LargeCacheExtension`. Thin forwarder to
/// [`super::sidecar::deref_mut`] — see [`deref_large_cache_extension`]'s doc
/// for why this stays a named wrapper.
///
/// `unsafe fn` for the same reason as [`deref_large_cache_extension`].
///
/// # Safety
///
/// - `p` must be non-null and was returned by [`reserve_large_cache_extension`]
///   (so it points at a value [`super::sidecar::reserve`] typed-initialised).
/// - The calling thread is the sole owner (`AllocCore` is neither `Send` nor
///   `Sync`), so no concurrent reader or writer can race this access.
/// - No other reference (shared or mutable) to this sidecar may be live for
///   the duration of the returned `&'static mut`'s use.
#[inline]
pub(crate) unsafe fn deref_large_cache_extension_mut(
    p: *mut LargeCacheExtension,
) -> &'static mut LargeCacheExtension {
    // SAFETY: forwarded verbatim from this function's own caller contract
    // above, which is `super::sidecar::deref_mut`'s contract specialised to
    // `LargeCacheExtension`.
    unsafe { super::sidecar::deref_mut(p) }
}
