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

    /// The SEGMENT-aligned usable base of this span, as a `*mut u8`. Non-null,
    /// valid for [`len`](Self::len) bytes, aligned to `SEGMENT`.
    #[must_use]
    pub(crate) fn as_ptr(&self) -> *mut u8 {
        self.0.as_ptr()
    }

    /// The number of usable bytes at [`as_ptr`](Self::as_ptr). Always a
    /// multiple of `SEGMENT`.
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
// The `SegmentDirectory` sidecar is materialized via `aligned_vmem::reserve_aligned`
// (M5-clean: direct OS syscall, no `std::alloc`/`Box`/`Vec`). The reservation
// is leaked via `core::mem::forget` for the process lifetime — same discipline
// as `RegistryChunk` / `HeapOverflowSidecar`. Owner-only: the pointer lives in
// `AllocCore` and only the owning thread ever dereferences it (no cross-thread
// race, no CAS protocol needed — simpler than the `HeapOverflow` sidecar, which
// IS cross-thread).

/// Byte size of one [`SegmentDirectory`], rounded up to a multiple of
/// `aligned_vmem::PAGE` — mirrors `registry_chunk::CHUNK_SIZE`'s identical
/// rounding for the same `reserve_aligned` size-contract reason (`size`
/// must be a non-zero multiple of `PAGE`).
#[cfg(feature = "alloc-segment-directory")]
const DIRECTORY_SIDECAR_SIZE: usize = {
    let raw = core::mem::size_of::<super::segment_directory::SegmentDirectory>();
    if raw == 0 {
        vmem::PAGE
    } else {
        let page = vmem::PAGE;
        (raw + page - 1) & !(page - 1)
    }
};

/// Alignment for the directory sidecar's `reserve_aligned` call.
/// `SegmentDirectory`'s natural alignment is 8 (array of `u64`), well under
/// a page; `reserve_aligned` requires `align >= PAGE`, so we use `PAGE`.
#[cfg(feature = "alloc-segment-directory")]
const DIRECTORY_SIDECAR_ALIGN: usize = vmem::PAGE;

/// Reserve and construct a [`SegmentDirectory`] sidecar via direct OS VM
/// reservation (M5-clean). Returns `Some(ptr)` on success (the pointer is
/// valid for the process lifetime, OS-zeroed, a fully valid initial state),
/// or `None` on OOM (sidecar OOM is NOT allocator OOM — the mechanism simply
/// stays off and the linear scan fallback is used).
///
/// The reservation is leaked via `core::mem::forget` — the sidecar lives for
/// the process lifetime, never released.
///
/// # Safety of the returned pointer
///
/// The returned `*mut SegmentDirectory` is:
/// - non-null, PAGE-aligned (>= `align_of::<SegmentDirectory>()`)
/// - valid for `size_of::<SegmentDirectory>()` bytes
/// - OS-zeroed (all bits zero = every `class_nonempty` bit clear)
/// - uniquely owned (only the calling `AllocCore` holds the pointer)
/// - never freed (leaked for the process lifetime)
///
/// The caller stores it in `AllocCore::directory_sidecar` and dereferences
/// it via [`deref_directory_sidecar`] / [`deref_directory_sidecar_mut`].
#[cfg(feature = "alloc-segment-directory")]
pub(crate) fn reserve_directory_sidecar() -> Option<*mut super::segment_directory::SegmentDirectory>
{
    let reservation = vmem::reserve_aligned(DIRECTORY_SIDECAR_SIZE, DIRECTORY_SIDECAR_ALIGN)?;
    let base = reservation.as_ptr() as *mut super::segment_directory::SegmentDirectory;

    // Under miri, `reserve_aligned` falls back to `std::alloc`, which does
    // NOT zero the bytes. Zero explicitly so the all-zero initial state
    // (every bitmap bit clear) holds under miri too.
    #[cfg(miri)]
    unsafe {
        core::ptr::write_bytes(base as *mut u8, 0, DIRECTORY_SIDECAR_SIZE);
    }

    // Leak the reservation: the sidecar lives for the process lifetime.
    core::mem::forget(reservation);

    Some(base)
}

/// Dereference a materialised directory sidecar pointer as
/// `&SegmentDirectory`. The ONE safe membrane function for shared-ref
/// access to the sidecar (mirrors `bootstrap::deref_overflow_sidecar`'s
/// role for `HeapOverflowSidecar`).
///
/// # Safety (caller contract — upheld by `AllocCore` owner-only discipline)
///
/// `p` must be non-null and was returned by [`reserve_directory_sidecar`].
/// The calling thread is the sole owner (`AllocCore` is single-writer).
#[cfg(feature = "alloc-segment-directory")]
pub(crate) fn deref_directory_sidecar(
    p: *const super::segment_directory::SegmentDirectory,
) -> &'static super::segment_directory::SegmentDirectory {
    debug_assert!(!p.is_null(), "deref_directory_sidecar: null pointer");
    // SAFETY: `p` is non-null, PAGE-aligned, valid for
    // `size_of::<SegmentDirectory>()` bytes, OS-zeroed or rebuild-written,
    // leaked for the process lifetime. The owner-only discipline means no
    // concurrent writer. `&'static` is sound because the allocation outlives
    // any reference.
    unsafe { &*p }
}

/// Dereference a materialised directory sidecar pointer as
/// `&mut SegmentDirectory`. The ONE safe membrane function for mutable
/// access to the sidecar. Owner-only: only the owning thread calls this.
///
/// # Safety (caller contract — upheld by `AllocCore` owner-only discipline)
///
/// `p` must be non-null and was returned by [`reserve_directory_sidecar`].
/// The calling thread is the sole owner (`AllocCore` is single-writer).
/// No other mutable or shared reference to this sidecar may be live.
#[cfg(feature = "alloc-segment-directory")]
pub(crate) fn deref_directory_sidecar_mut(
    p: *mut super::segment_directory::SegmentDirectory,
) -> &'static mut super::segment_directory::SegmentDirectory {
    debug_assert!(!p.is_null(), "deref_directory_sidecar_mut: null pointer");
    // SAFETY: same as `deref_directory_sidecar`, plus: the owner-only
    // discipline guarantees no concurrent reader or writer, so `&mut` is
    // sound. The `'static` lifetime is sound because the allocation outlives
    // any reference (leaked, never freed).
    unsafe { &mut *p }
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
pub(crate) fn recommit_pages(base: *mut u8, start_offset: usize, end_offset: usize) -> bool {
    // SAFETY: `base` is the base of a live segment owned by this allocator,
    // and `[base + start_offset, base + end_offset)` was previously decommitted.
    // `aligned_vmem::recommit` validates only the offset alignment and
    // `start < end` (NOT range containment — that is this caller's invariant),
    // and is a no-op that reports success under miri.
    unsafe { vmem::recommit(base, start_offset, end_offset) }
}
