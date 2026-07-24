//! [`Segment`] — the **OS segment aperture**: SEGMENT-aligned raw memory spans
//! handed up to the safe Cartographer.
//!
//! This module is now a **thin wrapper** over the `aligned-vmem` crate
//! (`crates/vmem`), which carries all the platform-specific OS syscalls
//! (`mmap`/`munmap`/`madvise` on Unix, `VirtualAlloc`/`VirtualFree` on
//! Windows). The `unsafe` live in `aligned_vmem`; this file exposes a safe
//! interface that matches the original `os.rs` contract so the rest of
//! `alloc_core` is unchanged.
//!
//! ## What changed vs. the old os.rs
//!
//! | Item | Old | New |
//! |------|-----|-----|
//! | `SEGMENT`, `PAGE`, `segment_base_of`, `segment_base_of_ptr` | defined here | defined here (unchanged) |
//! | `Segment { base, len, reservation, reservation_len }` | full impl | thin wrapper over `aligned_vmem::Reservation` |
//! | `release_segment(reservation, reservation_len)` | direct OS call | `aligned_vmem::release(ptr, len, SEGMENT)` |
//! | `decommit_pages` / `recommit_pages` | direct OS calls | `aligned_vmem::decommit` / `aligned_vmem::recommit` |
//!
//! ## Miri aperture
//!
//! `aligned-vmem` already contains the miri fallback (`std::alloc` with the
//! requested alignment); no miri-specific code is needed here.

// The crate is `#![deny(unsafe_code)]` with `alloc-core` on; this is one of
// the documented `unsafe` seams. `allow` lifts the crate-level `deny` for
// this file only so we can call `aligned_vmem::release` (an `unsafe fn`).
#![allow(unsafe_code)]

use core::ptr::NonNull;
use core::sync::atomic::{AtomicU64, Ordering};

use aligned_vmem as vmem;

/// Process-wide count of successful OS segment reservations (every
/// [`Segment::reserve`] success, plus NUMA-pinned reservations via
/// `numa::reserve_aligned_on_node`, which bypasses `Segment::reserve` but
/// still releases through [`release_segment`] below). Monotonic —
/// increment-only, relaxed. Diagnostic only: exposed process-wide via
/// `SeferAlloc::stats()` (`AllocStats::segments_reserved_total`) so a
/// consumer can watch `segments_reserved_total - segments_released_total`
/// (the live segment count) without walking any per-heap `SegmentTable`.
///
/// A monotonic pair (reserved/released totals) was chosen over a single
/// balanced live-count atomic: every increment/decrement pair would need to
/// be threaded through every segment-owning code path (small heap,
/// large-cache, decommit recycle, cross-thread reclaim) and a single missed
/// decrement anywhere silently desyncs the counter forever. Two
/// increment-only counters can never desync — worst case a path is missed
/// and BOTH totals under-count, which is self-evident (reserved stops
/// growing while segments keep flowing) rather than silently wrong.
pub(crate) static SEGMENTS_RESERVED_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Process-wide count of successful OS segment releases (every
/// [`release_segment`] call with a non-null reservation). Monotonic,
/// relaxed. See [`SEGMENTS_RESERVED_TOTAL`].
pub(crate) static SEGMENTS_RELEASED_TOTAL: AtomicU64 = AtomicU64::new(0);

/// The segment size and alignment, in bytes. 4 MiB — mimalloc's default. Every
/// [`Segment`] handed up by this module is aligned to a multiple of this value,
/// so [`crate::alloc_core::segment_of`] can find an allocation's owning segment
/// header in O(1) by masking the low bits of its address.
///
/// This is exposed (read-only) as [`super::SegmentLayout::SEGMENT`].
pub(crate) const SEGMENT: usize = 1 << 22;

/// A page size used by the page-granularity `PageMap`. 4 KiB — the smallest
/// unit both `mmap` (unix) and `VirtualAlloc` (windows) will commit/decommit.
/// Kept independent of [`SEGMENT`] so the page tables stay small (1024 pages
/// per segment) while the alignment mask is the segment mask.
///
/// Re-exported from `aligned_vmem::PAGE` for a single source of truth.
pub(crate) const PAGE: usize = vmem::PAGE;

/// A conservative compile-time upper bound on every realistic real-world OS
/// page size. 64 KiB covers the page sizes `aligned_vmem::page_size()` can
/// actually return at runtime — 4 KiB (x86-64 default), 16 KiB (Apple Silicon
/// macOS), and 64 KiB (some Linux/aarch64 configs). Being a power of two,
/// alignment to it implies alignment to every smaller power-of-two page size.
///
/// This is a compile-time *superset* bound, used in place of the runtime
/// `aligned_vmem::page_size()` wherever a decommit/recommit boundary offset
/// (`Layout::small_meta_end` / `Layout::primordial_meta_end`) must be
/// const-evaluated: those are `const fn` and cannot call the runtime query,
/// but they MUST land on a real-page boundary on every platform, because
/// `madvise`/`VirtualFree` silently round non-page-aligned offsets to the
/// nearest real page (M6 would silently reclaim the wrong byte range — see
/// `docs/reviews/2026-07-17-deep-audit/10-platform-portability.md`). Using
/// `PAGE` (4 KiB) here was the latent bug: a 16 KiB-page machine got a
/// boundary mid-page. Cost: at most ~64 KiB of extra committed-but-unused
/// metadata slack per segment in the worst case (negligible against
/// [`SEGMENT`] = 4 MiB).
pub(crate) const MAX_REALISTIC_PAGE_SIZE: usize = 1 << 16;

/// Convert an address to the SEGMENT-aligned base it falls within.
///
/// Pure safe arithmetic — this is part of the Cartographer and lives outside
/// the `unsafe` seam logically, but is so tightly coupled to [`SEGMENT`] that
/// it is defined here next to the constant. Re-exported via
/// [`super::SegmentLayout`].
#[must_use]
pub(crate) const fn segment_base_of(addr: usize) -> usize {
    addr & !(SEGMENT - 1)
}

/// Convert a raw pointer to the SEGMENT-aligned base pointer it falls within,
/// **preserving provenance**.
///
/// This is the strict-provenance–clean equivalent of the old
/// `segment_base_of(ptr as usize) as *mut u8` idiom: `ptr as usize` strips
/// provenance (exposed-address cast, forbidden under `-Zmiri-strict-provenance`),
/// while `ptr.map_addr(|a| a & !(SEGMENT - 1))` rounds the address down within
/// the same provenance domain — sound under both permissive and strict-provenance
/// models. The returned pointer carries the same provenance as `ptr` and points
/// to the SEGMENT-aligned base of the segment that contains `ptr`.
///
/// `ptr` MUST lie within a segment owned by this allocator (the Cartographer's
/// invariant); the result is the base of that segment.
#[must_use]
#[inline(always)]
pub(crate) fn segment_base_of_ptr(ptr: *mut u8) -> *mut u8 {
    ptr.map_addr(|a| a & !(SEGMENT - 1))
}

/// A owning handle to one SEGMENT-aligned span of raw memory.
///
/// `base` is non-null, aligned to `SEGMENT`, and valid for `len` bytes for the
/// lifetime of this `Segment`. Dropping the `Segment` returns the whole
/// underlying OS reservation to the OS exactly once. The span is **not**
/// initialised — callers must not read uninitialised bytes (matching the
/// `GlobalAlloc::alloc` contract).
///
/// Internally wraps [`aligned_vmem::Reservation`]; exposes the same API as the
/// original `Segment` type so all call sites are unchanged.
///
/// `Segment` is `Send` (but not `Sync`): the span is owned exclusively by the
/// sending thread. `&Segment` grants only read access to the metadata.
pub struct Segment(vmem::Reservation);

impl Segment {
    /// Reserve a SEGMENT-aligned span of `len` bytes from the OS.
    ///
    /// `len` is rounded UP to a multiple of `SEGMENT` (a span is always whole
    /// segments). Returns `None` only on OOM or if `len == 0`.
    #[must_use]
    pub(crate) fn reserve(len: usize) -> Option<Self> {
        if len == 0 {
            return None;
        }
        let n_segments = len.div_ceil(SEGMENT);
        let usable = n_segments * SEGMENT;
        let reservation = vmem::reserve_aligned(usable, SEGMENT)?;
        SEGMENTS_RESERVED_TOTAL.fetch_add(1, Ordering::Relaxed);
        Some(Segment(reservation))
    }

    /// R12-3 (EXPERIMENTAL, feature `exact-span-large`): reserve a
    /// SEGMENT-ALIGNED span of EXACTLY `len` bytes — unlike [`reserve`](Self::reserve),
    /// `len` is NOT rounded up to a whole number of `SEGMENT`s. The caller
    /// (`alloc_large_slow`) is responsible for `len` already being a non-zero
    /// multiple of the OS page size (`aligned_vmem::PAGE`) — `reserve_aligned`'s
    /// own contract requirement — typically `round_up(header + size, PAGE)`.
    ///
    /// This exists so a Large/huge dedicated segment can reserve+commit only
    /// the physically-needed span instead of a minimum of one whole `SEGMENT`
    /// (4 MiB) regardless of the requested size. The base stays `SEGMENT`
    /// (4 MiB) aligned — only the span's byte length shrinks — so
    /// [`segment_base_of_ptr`] continues to resolve the correct segment base
    /// for an exact-span segment exactly as it does for a whole-segment one.
    /// `crates/vmem::reserve_aligned(size, align)` already supports
    /// `size != align` on every backend (see the `exact-span-large` feature
    /// doc in `Cargo.toml` for the full backend-support argument); this
    /// constructor is a thin non-rounding sibling of [`reserve`](Self::reserve),
    /// not a new OS-level capability.
    ///
    /// Returns `None` only on OOM, if `len == 0`, or if `len` is not a
    /// multiple of the OS page size (forwarded from `vmem::reserve_aligned`'s
    /// contract).
    ///
    /// `not(numa-aware)`: mirrors [`reserve_lazy`](Self::reserve_lazy)'s own
    /// gating — the sole caller (`alloc_large_slow`'s `not(numa-aware)` OS
    /// reservation arm) only takes this branch when `numa-aware` is off; the
    /// numa-aware arm calls `numa::reserve_aligned_on_node` directly instead,
    /// which already forwards `usable` unrounded regardless of this feature
    /// (see the `exact-span-large` feature doc in `Cargo.toml`). Gating the
    /// definition here too (rather than leaving it reachable-but-uncalled)
    /// keeps `--all-features` (which enables `numa-aware` alongside
    /// `exact-span-large`) free of a dead-code warning.
    ///
    /// R12-4: also excluded when `large-reserved-capacity` is on — that
    /// feature's `alloc_large_slow` arm always calls
    /// [`reserve_capacity_exact`](Self::reserve_capacity_exact) instead (a
    /// strict superset of what this constructor does), so `reserve_exact`
    /// would otherwise be genuinely unreachable-but-compiled under that
    /// feature combination, same dead-code rationale as the `numa-aware`
    /// exclusion above.
    #[must_use]
    #[cfg(all(
        feature = "exact-span-large",
        not(feature = "numa-aware"),
        not(feature = "large-reserved-capacity")
    ))]
    pub(crate) fn reserve_exact(len: usize) -> Option<Self> {
        if len == 0 {
            return None;
        }
        let reservation = vmem::reserve_aligned(len, SEGMENT)?;
        SEGMENTS_RESERVED_TOTAL.fetch_add(1, Ordering::Relaxed);
        Some(Segment(reservation))
    }

    /// R12-4 (EXPERIMENTAL, feature `large-reserved-capacity`): reserve a
    /// SEGMENT-ALIGNED span of EXACTLY `reserved_len` bytes — like
    /// [`reserve_exact`](Self::reserve_exact) — but commit only the first
    /// `initial_commit` bytes; the remainder of `reserved_len` stays
    /// reserved-but-uncommitted (Windows lazy-commit path; falls back to the
    /// eager, fully-committed path on Unix/miri, matching
    /// `aligned_vmem::reserve_aligned_lazy`'s own fallback — same portability
    /// contract as [`reserve_lazy`](Self::reserve_lazy)).
    ///
    /// This is [`reserve_lazy`](Self::reserve_lazy)'s ARBITRARY-length sibling
    /// (that constructor is hardcoded to a whole `SEGMENT`), combined with
    /// [`reserve_exact`](Self::reserve_exact)'s non-rounding contract: the
    /// caller (`alloc_large_slow`) picks `reserved_len` as a geometric multiple
    /// of the page-rounded request (capped, see the `large-reserved-capacity`
    /// feature doc in `Cargo.toml`) and `initial_commit` as the page-rounded
    /// request itself, so a later growing `realloc` that still fits within
    /// `reserved_len` can commit just the missing tail
    /// ([`super::commit_pages`]) instead of moving the allocation.
    ///
    /// `initial_commit` must be a non-zero multiple of `PAGE` and
    /// `<= reserved_len`; `reserved_len` must itself be a non-zero multiple of
    /// `PAGE` (forwarded from `vmem::reserve_aligned_lazy`'s contract).
    /// Returns `None` on OOM or a contract violation.
    ///
    /// `not(numa-aware)`: mirrors [`reserve_lazy`](Self::reserve_lazy) /
    /// [`reserve_exact`](Self::reserve_exact)'s own gating — the sole caller
    /// (`alloc_large_slow`'s `not(numa-aware)` OS reservation arm) only takes
    /// this branch when `numa-aware` is off; the numa-aware arm always uses
    /// the eager `numa::reserve_aligned_on_node` path (NUMA reservations are
    /// not disturbed by the lazy path, same exclusion as the small-segment
    /// lazy-commit path). Gating the definition here too (rather than leaving
    /// it reachable-but-uncalled) keeps `--all-features` free of a dead-code
    /// warning.
    #[must_use]
    #[cfg(all(feature = "large-reserved-capacity", not(feature = "numa-aware")))]
    pub(crate) fn reserve_capacity_exact(
        reserved_len: usize,
        initial_commit: usize,
    ) -> Option<Self> {
        if reserved_len == 0 {
            return None;
        }
        let reservation = vmem::reserve_aligned_lazy(reserved_len, SEGMENT, initial_commit)?;
        SEGMENTS_RESERVED_TOTAL.fetch_add(1, Ordering::Relaxed);
        Some(Segment(reservation))
    }

    /// Reserve a SEGMENT-aligned, exactly-`SEGMENT`-sized span from the OS,
    /// committing only the first `initial_commit` bytes — the rest stays
    /// reserved-but-uncommitted (Windows lazy-commit path; falls back to the
    /// eager, fully-committed path on Unix/miri, matching
    /// `aligned_vmem::reserve_aligned_lazy`'s own fallback).
    ///
    /// R7-B6 (primordial lazy commit): the primordial-segment analogue of
    /// [`reserve_small_segment`](super::alloc_core_small)'s lazy branch,
    /// factored here so `bootstrap::primordial` can call it without
    /// duplicating the raw `aligned_vmem` plumbing outside this seam.
    /// `initial_commit` must be a non-zero multiple of `PAGE` and
    /// `<= SEGMENT`; the caller (`bootstrap::primordial`) upholds this via a
    /// `debug_assert!` mirroring `reserve_small_segment`'s identical
    /// contract. Returns `None` on OOM or a contract violation.
    ///
    /// `not(numa-aware)`: the sole caller (`bootstrap::primordial`) only
    /// takes this branch when `numa-aware` is off, mirroring
    /// `reserve_small_segment`'s own NUMA exclusion (P2 gate: NUMA
    /// reservations must not be disturbed by the lazy path) — see that call
    /// site's doc for the full rationale. Gating the definition here too
    /// (rather than leaving it reachable-but-uncalled) keeps `--all-features`
    /// (which enables `numa-aware` alongside `alloc-lazy-commit`) free of a
    /// dead-code warning.
    ///
    /// R12-9 (task #260): gated on `primordial-lazy-commit` specifically —
    /// this is the constructor `bootstrap::primordial` alone calls, so it is
    /// the one call site that must NOT compile when only
    /// `small-segment-lazy-commit` is enabled (else it would be unreachable
    /// dead code in that configuration).
    #[must_use]
    #[cfg(all(feature = "primordial-lazy-commit", not(feature = "numa-aware")))]
    pub(crate) fn reserve_lazy(initial_commit: usize) -> Option<Self> {
        let reservation = vmem::reserve_aligned_lazy(SEGMENT, SEGMENT, initial_commit)?;
        SEGMENTS_RESERVED_TOTAL.fetch_add(1, Ordering::Relaxed);
        Some(Segment(reservation))
    }

    /// The SEGMENT-aligned usable base of this span, as a `*mut u8`. Non-null,
    /// valid for [`len`](Self::len) bytes, aligned to `SEGMENT`.
    #[must_use]
    pub(crate) fn as_ptr(&self) -> *mut u8 {
        self.0.as_ptr()
    }

    /// The number of usable bytes at [`as_ptr`](Self::as_ptr). A multiple of
    /// `SEGMENT` for a [`reserve`](Self::reserve)/[`reserve_lazy`](Self::reserve_lazy)
    /// span; for a [`reserve_exact`](Self::reserve_exact) span (feature
    /// `exact-span-large`) it is only guaranteed to be a multiple of the OS
    /// page size.
    #[must_use]
    #[allow(dead_code)] // Substrate API; Phase 9+ heaps read it.
    pub(crate) const fn len(&self) -> usize {
        self.0.len()
    }

    /// The start of the OS reservation (may extend below [`as_ptr`](Self::as_ptr)
    /// due to the over-reserve + trim technique). Recorded in the segment
    /// header so `AllocCore::drop` can release the whole reservation.
    #[must_use]
    pub(crate) fn reservation(&self) -> NonNull<u8> {
        // SAFETY: `aligned_vmem::Reservation::reservation_ptr()` is always
        // non-null — it was returned by the OS (or `std::alloc` under miri)
        // and is non-null by the `reserve_aligned` contract.
        unsafe { NonNull::new_unchecked(self.0.reservation_ptr()) }
    }

    /// The full size of the OS reservation (head + usable + tail).
    #[must_use]
    pub(crate) const fn reservation_len(&self) -> usize {
        self.0.reservation_len()
    }
}

// `Send` is already implemented on `aligned_vmem::Reservation`; `Segment` is a
// newtype over one `Reservation`, so it AUTO-derives `Send` — no `unsafe impl`
// needed (task #21 / review L1). The former `unsafe impl Send for Segment`
// only restated the auto-impl, but froze it: had a future edit added a
// `!Send` field (e.g. a `Cell<..>` diagnostic), the manual impl would have
// silently kept `Segment: Send` — a lie — where the auto-impl would honestly
// drop it. This compile-time assert documents the intent (Segment must stay
// `Send`, its sole use is exclusive-ownership transfer to another thread) AND
// enforces it: adding a `!Send` field makes THIS line fail to compile with a
// clear "`Segment: Send` is not satisfied" error, instead of a silent unsound
// bless.
const _: () = {
    fn assert_send<T: Send>() {}
    let _ = assert_send::<Segment>;
};

// NOTE: `Segment` is intentionally NOT `Sync` (same as before). Writes into
// the span happen via raw pointers in the `node` seam under the single-threaded
// Phase 8 invariant; no synchronisation for concurrent writes.

/// Release a whole OS reservation. Thin wrapper over
/// [`aligned_vmem::release`].
///
/// # Contract (caller's invariant — not enforced by the type system)
///
/// `reservation` must be a pointer previously returned by
/// [`Segment::reserve`] (specifically its [`Segment::reservation`]) and not
/// yet released. `reservation_len` must be the matching length. Must be called
/// exactly once per reservation.
pub(crate) fn release_segment(reservation: *mut u8, reservation_len: usize) {
    if reservation.is_null() {
        return;
    }
    // SAFETY: the caller's contract (documented above) guarantees `reservation`
    // was returned by `aligned_vmem::reserve_aligned(_, SEGMENT)` and is freed
    // exactly once. The `align` argument matches the original reservation.
    unsafe { vmem::release(reservation, reservation_len, SEGMENT) };
    SEGMENTS_RELEASED_TOTAL.fetch_add(1, Ordering::Relaxed);
}

/// Relaxed snapshot of [`SEGMENTS_RESERVED_TOTAL`]. Diagnostic only.
#[must_use]
pub(crate) fn segments_reserved_total() -> u64 {
    SEGMENTS_RESERVED_TOTAL.load(Ordering::Relaxed)
}

/// Relaxed snapshot of [`SEGMENTS_RELEASED_TOTAL`]. Diagnostic only.
#[must_use]
pub(crate) fn segments_released_total() -> u64 {
    SEGMENTS_RELEASED_TOTAL.load(Ordering::Relaxed)
}

/// Decommit the payload pages of a segment: return their physical backing to
/// the OS while keeping the address-space reservation alive. Thin wrapper over
/// [`aligned_vmem::decommit`].
///
/// `base` is the SEGMENT-aligned base. We decommit `[base + start_offset,
/// base + end_offset)` — typically the payload region past the metadata. The
/// offsets MUST be page-aligned and within the segment.
#[cfg(feature = "alloc-decommit")]
pub(crate) fn decommit_pages(base: *mut u8, start_offset: usize, end_offset: usize) {
    // SAFETY: `base` is the base of a live segment owned by this allocator.
    // The caller guarantees no live blocks exist in the range, and offsets are
    // page-aligned. `aligned_vmem::decommit` validates only the offset
    // alignment and `start < end` (NOT that the range lies within the segment —
    // that is this caller's invariant), and is a no-op under miri.
    unsafe { vmem::decommit(base, start_offset, end_offset) };
}

// ── R7-A1: directory sidecar VM reservation ────────────────────────────────
//
// The `SegmentDirectory` sidecar is materialized via the shared owner-only
// sidecar primitive (`alloc_core::sidecar`, R14-9/task #294), which itself
// wraps `aligned_vmem::leak_zeroed_pages` (M5-clean: direct OS syscall, no
// `std::alloc`/`Box`/`Vec`; the reservation is leaked for the process
// lifetime — same discipline as `RegistryChunk` / `HeapOverflowSidecar`).
// Owner-only: the pointer lives in `AllocCore` and only the owning thread
// ever dereferences it (no cross-thread race, no CAS protocol needed —
// simpler than the `HeapOverflow` sidecar, which IS cross-thread).

/// Reserve and construct a [`SegmentDirectory`] sidecar. Returns `Some(ptr)`
/// on success (the pointer is valid for the process lifetime, a fully valid
/// initial state), or `None` on OOM (sidecar OOM is NOT allocator OOM — the
/// mechanism simply stays off and the linear scan fallback is used).
///
/// Uses [`sidecar::reserve_zeroed_with`] rather than [`sidecar::reserve`]:
/// `SegmentDirectory`'s bitmap fields (`class_nonempty_by_node`,
/// `active_bits_by_node`) are valid at all-zero (every bit/count clear is a
/// real, intentional initial state), but under `numa-aware` the `node_ids`
/// table is NOT (`0` is a real OS node id, so leaving it OS-zeroed would
/// misread as "node 0 already claimed bucket 0" — see that field's own doc
/// comment); the fixup closure runs the same `init_node_ids()` repair the
/// pre-R14-9 code ran as a separate step right after reservation. Moving the
/// whole (up to ~56 KiB under `numa-aware`) `SegmentDirectory` through a
/// by-value [`sidecar::reserve`] call would risk an avoidable stack copy;
/// the in-place fixup avoids it.
///
/// The caller stores the pointer in `AllocCore::directory_sidecar` and
/// dereferences it via [`sidecar::deref`] / [`sidecar::deref_mut`].
#[cfg(feature = "alloc-segment-directory")]
pub(crate) fn reserve_directory_sidecar() -> Option<*mut super::segment_directory::SegmentDirectory>
{
    // SAFETY: `SegmentDirectory`'s bitmap fields (`class_nonempty_by_node`,
    // `active_bits_by_node`) are the only fields `init_node_ids` leaves
    // untouched, and both are valid at all-zero (every bit/count clear is a
    // real, intentional initial state — "no class non-empty yet" / "zero
    // active bits yet"). The one field that is NOT valid at all-zero
    // (`node_ids` under `numa-aware`: `0` is a real OS node id) is exactly
    // the field `init_node_ids` repairs before this function's caller can
    // observe the pointer. Same owner-only single-writer discipline as every
    // other sidecar in this module (the pointer is stored in
    // `AllocCore::directory_sidecar` and dereferenced only via
    // `sidecar::deref`/`deref_mut`).
    unsafe {
        super::sidecar::reserve_zeroed_with(
            super::segment_directory::SegmentDirectory::init_node_ids,
        )
    }
}

/// Read ONE `class_nonempty_by_node[node_bucket][class_idx]` word-array out
/// of the directory sidecar BY VALUE, without materialising any `&`/`&mut
/// SegmentDirectory` reference (task #252 / R12-1).
///
/// # Why this exists
///
/// `find_segment_with_free_impl`'s directory-driven scan (in
/// `alloc_core_small.rs`) used to hold a live `&'static SegmentDirectory`
/// (from [`deref_directory_sidecar`]) across calls to
/// `validate_directory_candidate`, which can itself call
/// [`deref_directory_sidecar_mut`] (via `publish_empty` /
/// `sync_directory_for_segment_classes`) on the SAME allocation while the
/// shared reference was still lexically live. That is aliasing UB under
/// Stacked/Tree Borrows — `&T` and `&mut T` simultaneously live over one
/// allocation — regardless of the fact that the single-threaded owner
/// discipline makes it data-race-free. This accessor breaks the shared
/// reference's lifetime: it copies the one word-array the scan loop needs
/// into a local `[u64; WORDS_PER_CLASS]` and returns, so no reference to the
/// sidecar survives past this call. The scan loop iterates over the local
/// copy; any candidate found is re-validated (base non-null, kind, BinTable
/// head) by `validate_directory_candidate` before use, so a value read
/// slightly stale (relative to a mutation `validate_directory_candidate`
/// performs on a LATER candidate in the same word) is exactly the same
/// "candidate, not fact" contract the directory already documents (see the
/// R7-A3 module doc: every set bit is validated, never trusted blindly).
///
/// # Safety (caller contract — upheld by `AllocCore` owner-only discipline)
///
/// `p` must be non-null and was returned by [`reserve_directory_sidecar`].
/// The calling thread is the sole owner (`AllocCore` is single-writer), so a
/// racing writer is impossible; this function itself never overlaps its own
/// raw read with any live reference because it materialises none.
#[cfg(feature = "alloc-segment-directory")]
#[inline]
pub(crate) fn read_directory_class_words(
    p: *const super::segment_directory::SegmentDirectory,
    node_bucket: usize,
    class_idx: usize,
) -> [u64; super::segment_directory::WORDS_PER_CLASS] {
    debug_assert!(!p.is_null(), "read_directory_class_words: null pointer");
    // SAFETY: `p` is non-null, PAGE-aligned, valid for
    // `size_of::<SegmentDirectory>()` bytes, leaked for the process lifetime
    // (same validity contract as `deref_directory_sidecar`). `addr_of!`
    // forms a raw pointer to the target field WITHOUT creating any
    // intermediate `&SegmentDirectory`, and `.read()` performs a single
    // valid, properly-aligned, non-overlapping copy of a plain-`u64` array
    // (no interior padding/niche/Drop concerns) into an owned local — no
    // reference to the sidecar escapes this function.
    unsafe {
        let field = core::ptr::addr_of!((*p).class_nonempty_by_node[node_bucket][class_idx]);
        field.read()
    }
}

/// R12-2: read the calling thread's own directory bucket index for `node_id`
/// OUT OF the directory sidecar BY VALUE, without materialising any
/// `&SegmentDirectory` reference — same discipline as
/// [`read_directory_class_words`] (task #252 / R12-1), for the same reason:
/// this is called once, right before the scan-order bucket list is built,
/// and must not leave any reference live across the loop that follows (which
/// calls `validate_directory_candidate`, itself able to materialise a
/// `&mut SegmentDirectory` on the same allocation to self-heal a stale bit).
///
/// Read-only lookup (does NOT register a new node): mirrors
/// `SegmentDirectory::node_bucket`, not `node_bucket_mut`. A node id with no
/// claimed bucket yet (e.g. the calling thread's own node has no segments
/// registered) resolves to the unknown bucket — correct, since there is
/// nothing local to prefer yet.
///
/// # Safety (caller contract — upheld by `AllocCore` owner-only discipline)
///
/// `p` must be non-null and was returned by [`reserve_directory_sidecar`].
/// The calling thread is the sole owner (`AllocCore` is single-writer), so a
/// racing writer is impossible; this function itself never overlaps its own
/// raw read with any live reference because it materialises none.
#[cfg(all(feature = "alloc-segment-directory", feature = "numa-aware"))]
#[inline]
pub(crate) fn read_directory_node_bucket(
    p: *const super::segment_directory::SegmentDirectory,
    node_id: u32,
) -> usize {
    debug_assert!(!p.is_null(), "read_directory_node_bucket: null pointer");
    if node_id == super::segment_header::NO_NODE_RAW {
        return super::segment_directory::MAX_NODES;
    }
    // SAFETY: `p` is non-null, PAGE-aligned, valid for
    // `size_of::<SegmentDirectory>()` bytes, leaked for the process lifetime
    // (same validity contract as `deref_directory_sidecar`). `addr_of!` forms
    // a raw pointer to the `node_ids` field WITHOUT creating any intermediate
    // `&SegmentDirectory`, and `.read()` performs a single valid, properly-
    // aligned, non-overlapping copy of a plain-`u32` array (no interior
    // padding/niche/Drop concerns) into an owned local — no reference to the
    // sidecar escapes this function.
    let node_ids: [u32; super::segment_directory::MAX_NODES] = unsafe {
        let field = core::ptr::addr_of!((*p).node_ids);
        field.read()
    };
    node_ids
        .iter()
        .position(|&n| n == node_id)
        .unwrap_or(super::segment_directory::MAX_NODES)
}

/// Commit a sub-range within a segment whose payload was only partially
/// committed (the lazy-commit path). Thin wrapper over
/// [`aligned_vmem::commit_range`].
///
/// Returns `true` if the range is now committed, `false` if the OS refused.
/// On `false` the caller MUST NOT write into the range.
///
/// ## Fault injection (CRATE-P2 follow-up: absorbed into vmem)
///
/// The B2/B4 "fail the next N" / "fail the k-th" commit-failure fault
/// injectors used to live here as sefer-local `COMMIT_FAIL_*` atomics. They
/// have been absorbed into `aligned_vmem::fault_injection` (feature
/// `aligned-vmem/fault-injection`, pulled in additively by
/// `alloc-lazy-commit`): the hook now lives on vmem's REAL commit path
/// (`try_commit_range`, checked immediately before the real syscall), so
/// this function needs no fault-injection logic of its own — arming is done
/// directly against `aligned_vmem::fault_injection::{arm_fail_next,
/// arm_fail_at}` (see `AllocCore::dbg_arm_commit_fail` /
/// `dbg_arm_commit_fail_at`). This is DISTINCT from vmem's `mock` feature,
/// which replaces the whole backend (sefer needs the real segment
/// reservation + real commit accounting under test, so it cannot build with
/// `mock`).
///
/// ## Difference from [`recommit_pages`]
///
/// [`recommit_pages`] re-commits pages that were PREVIOUSLY committed and then
/// decommitted via [`decommit_pages`]. `commit_pages` commits pages that were
/// NEVER committed in the first place (reserved via the lazy path). The
/// underlying Windows syscall is the same (`VirtualAlloc(MEM_COMMIT)`), but
/// the semantic intent differs.
///
/// R12-4: also compiled under `large-reserved-capacity`, which reuses this
/// exact wrapper to commit the missing tail of a Large segment's
/// `reserved_capacity` on a growing in-place `realloc` — same underlying
/// `aligned_vmem::commit_range` primitive as the small-segment lazy-commit
/// path, just a different caller.
///
/// R12-9 (task #260): the B2 grow-on-carve caller in `carve_block`/
/// `carve_batch` (`alloc_core_small.rs`) is SHARED between the primordial
/// segment and ordinary small segments, so this wrapper is gated on `any(..)`
/// of the two split lazy-commit sub-features (either one's initial partial
/// reservation can leave a frontier below `SEGMENT` that a later carve grows
/// past), not on either sub-feature alone.
#[must_use]
#[cfg(any(
    feature = "primordial-lazy-commit",
    feature = "small-segment-lazy-commit",
    feature = "large-reserved-capacity"
))]
pub(crate) fn commit_pages(base: *mut u8, start_offset: usize, end_offset: usize) -> bool {
    // SAFETY: `base` is the base of a live segment owned by this allocator.
    // The caller guarantees `[base + start_offset, base + end_offset)` is
    // within the segment's VA reservation and currently reserved-but-
    // uncommitted (or already committed — idempotent). `aligned_vmem::
    // commit_range` validates only the offset alignment and `start < end`.
    // The fault-injection check (when armed) happens INSIDE `commit_range`,
    // immediately before the real syscall.
    unsafe { vmem::commit_range(base, start_offset, end_offset) }
}

/// Recommit previously-decommitted pages within a segment. Thin wrapper over
/// [`aligned_vmem::recommit`].
///
/// Returns `true` if the range is now committed (writes into it are safe), and
/// `false` if the OS refused the commit (commit-charge exhaustion / true OOM).
/// On `false` the caller MUST NOT write into the range and MUST leave the
/// segment marked decommitted — this is an honest OOM, propagated as a null
/// carve, never a fault or panic (`sefer_alloc` OOM contract).
#[must_use]
#[cfg(feature = "alloc-decommit")]
// B3: under `small-segment-lazy-commit` the recommit path in
// carve_block/carve_batch is replaced by a lazy clear-decommitted-flag (the
// initial chunk is already committed), so this function has no callers. It
// IS called when `alloc-decommit` is ON but `small-segment-lazy-commit` is
// OFF. (`primordial-lazy-commit` alone does not affect this: only `Small`
// segments are ever decommitted — the primordial segment is structurally
// excluded from the decommit/pool lifecycle, see `dec_live_and_maybe_decommit`
// — so this function's reachability tracks `small-segment-lazy-commit`
// specifically, not the `primordial-lazy-commit` sibling. R12-9, task #260.)
#[cfg_attr(feature = "small-segment-lazy-commit", allow(dead_code))]
pub(crate) fn recommit_pages(base: *mut u8, start_offset: usize, end_offset: usize) -> bool {
    // SAFETY: `base` is the base of a live segment owned by this allocator,
    // and `[base + start_offset, base + end_offset)` was previously decommitted.
    // `aligned_vmem::recommit` validates only the offset alignment and
    // `start < end` (NOT range containment — that is this caller's invariant),
    // and is a no-op that reports success under miri.
    unsafe { vmem::recommit(base, start_offset, end_offset) }
}
