//! [`Segment`] — the **OS segment aperture**: the foundation `unsafe` seam that
//! hands SEGMENT-aligned raw memory spans up to the safe Cartographer.
//!
//! This is one of the two confined-`unsafe` modules of the Phase 8 substrate
//! (the other is [`super::node`] — the intrusive free-list seam). The crate is
//! `#![deny(unsafe_code)]` with `alloc-core` on; this file lifts that with
//! `#![allow(unsafe_code)]` so the OS syscalls can run, and the confinement is
//! **enforced by the compiler** — `unsafe` anywhere outside `os` and `node` is
//! a hard error.
//!
//! ## What this module IS and is NOT
//!
//! - IS: a safe-to-use [`Segment { base, len }`] handle that owns one
//!   SEGMENT-aligned span of `len` bytes, carved directly from the OS via
//!   `mmap` (unix) or `VirtualAlloc` (windows). It yields the aligned base as a
//!   `NonNull<u8>` and frees the span on drop. Every `unsafe` block carries a
//!   `// SAFETY:` comment.
//! - IS NOT: a wrapper over `std::alloc`. The whole point of Phase 8 (§2 of
//!   `MALLOC_PLAN.md`) is that `std::alloc` is NEVER on the path — when this
//!   crate becomes the global allocator, calling `std::alloc` from inside an
//!   allocation would recurse infinitely. We call the OS directly.
//!
//! ## Miri aperture (`#[cfg(miri)]`)
//!
//! Miri cannot execute raw FFI (`VirtualAlloc`/`mmap`), so under miri only we
//! fall back to `std::alloc` with SEGMENT alignment. This is sound because
//! under miri we are NOT the global allocator — the test harness uses the host
//! allocator. The M5 reentrancy test (`alloc_core_reentrancy`) runs WITHOUT
//! miri, so it still proves the production path never touches `std::alloc`.
//!
//! ## How SEGMENT-alignment is achieved
//!
//! Both `mmap` (unix) and `VirtualAlloc` (windows) return addresses aligned to
//! the OS allocation granularity (typically 4 KiB / 64 KiB), NOT to our
//! SEGMENT size. To guarantee a SEGMENT-aligned base we use the classic
//! **over-reserve + trim** technique:
//!
//! 1. Reserve `2 * SEGMENT` bytes from the OS.
//! 2. Find the first SEGMENT-aligned address `base` inside that reservation.
//! 3. Return (decommit on windows / `munmap` on unix) the head `[reservation,
//!    base)` and tail `[base + SEGMENT, reservation + 2*SEGMENT)` so their
//!    physical pages do not count against RSS. The address-space window stays
//!    owned until drop, when the whole reservation is released.
//!
//! On windows, partial release of a `MEM_RESERVE`-only region is impossible
//! (`VirtualFree(.., MEM_RELEASE)` releases the *entire* region), so we
//! `MEM_DECOMMIT` the head/tail — physical pages are returned, address space
//! stays reserved until the segment is dropped. On unix `munmap` cleanly
//! unmaps the head and tail.

// The crate is `#![deny(unsafe_code)]` with `alloc-core` on (see `src/lib.rs`);
// this is one of the TWO documented `unsafe` seams of the Phase 8 substrate.
// `allow` lifts the crate-level `deny` for this file only — `unsafe` anywhere
// else in the crate is a hard error, so the confinement is compiler-checked.
#![allow(unsafe_code)]

use core::ptr::NonNull;

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
pub(crate) const PAGE: usize = 1 << 12;

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

/// A owning handle to one SEGMENT-aligned span of raw memory.
///
/// `base` is non-null, aligned to `SEGMENT`, and valid for `len` bytes for the
/// lifetime of this `Segment`. Dropping the `Segment` returns the whole
/// underlying OS reservation to the OS exactly once. The span is **not**
/// initialised — callers must not read uninitialised bytes (matching the
/// `GlobalAlloc::alloc` contract).
///
/// `Segment` is `Send` (but not `Sync`): the span is owned exclusively by the
/// sending thread (Phase 8 is single-threaded; per-thread heaps arrive in
/// Phase 9). `&Segment` grants only read access to the metadata — to obtain a
/// writable pointer into the span, callers go through the [`node`](super::node)
/// seam.
pub struct Segment {
    /// The SEGMENT-aligned base. Non-null, valid for `len` bytes, aligned to
    /// `SEGMENT`.
    base: NonNull<u8>,
    /// Number of *usable* bytes at `base`. This is the requested span size
    /// (typically `SEGMENT`, but large/huge allocations may span multiple
    /// segments — see [`super::AllocCore`]).
    len: usize,
    /// The raw start of the original OS reservation (which may extend below
    /// `base` due to the over-reserve + trim technique). Kept so `Drop` can
    /// release the whole reservation. On unix this equals `base` when the OS
    /// happened to return an aligned address (no trim needed); on windows it
    /// is the original `VirtualAlloc` return and may be `< base`.
    reservation: NonNull<u8>,
    /// The full size of the OS reservation (head + usable + tail). Recorded
    /// for `Drop` to free the right amount.
    reservation_len: usize,
}

impl Segment {
    /// Reserve a SEGMENT-aligned span of `len` bytes from the OS.
    ///
    /// `len` is rounded UP to a multiple of `SEGMENT` (a span is always whole
    /// segments) — large/huge allocations request `len` and get a span whose
    /// usable length is `round_up(len, SEGMENT)`. Returns `None` only on OOM
    /// (the OS refused the reservation). The span is **uninitialised**.
    ///
    /// # Panics
    ///
    /// Panics if `len == 0` (a degenerate request that has no legitimate
    /// caller — every allocation rounds up to at least one segment).
    #[must_use]
    pub(crate) fn reserve(len: usize) -> Option<Self> {
        assert!(len > 0, "Segment::reserve requires len > 0");
        // Round up to a whole number of segments. A span is always an integral
        // number of segments so segment_of(ptr) lands on a real header.
        let n_segments = len.div_ceil(SEGMENT);
        let usable = n_segments * SEGMENT;
        reserve_aligned(usable).map(|(base, reservation, reservation_len)| Self {
            base,
            len: usable,
            reservation,
            reservation_len,
        })
    }

    /// The SEGMENT-aligned usable base of this span, as a `*mut u8`. Non-null,
    /// valid for [`len`](Self::len) bytes, aligned to `SEGMENT`.
    #[must_use]
    pub(crate) fn as_ptr(&self) -> *mut u8 {
        self.base.as_ptr()
    }

    /// The number of usable bytes at [`as_ptr`](Self::as_ptr). Always a
    /// multiple of `SEGMENT`.
    #[must_use]
    #[allow(dead_code)] // Substrate API; Phase 9+ heaps read it.
    pub(crate) const fn len(&self) -> usize {
        self.len
    }

    /// The start of the OS reservation (may extend below [`as_ptr`](Self::as_ptr)
    /// due to the over-reserve + trim technique). Recorded in the segment
    /// header so `AllocCore::drop` can release the whole reservation.
    #[must_use]
    pub(crate) const fn reservation(&self) -> NonNull<u8> {
        self.reservation
    }

    /// The full size of the OS reservation (head + usable + tail).
    #[must_use]
    pub(crate) const fn reservation_len(&self) -> usize {
        self.reservation_len
    }
}

/// Release a whole OS reservation. SAFE entry point (the `unsafe` is internal
/// to this module): `AllocCore::drop` and `AllocCore::dealloc` (for large
/// segments) call this to free a segment whose `(reservation, reservation_len)`
/// was recorded in its header — NO `Vec<Segment>` is needed (the registry IS
/// the ownership list, part of the self-hosting discipline).
///
/// # Contract (caller's invariant — not enforced by the type system)
///
/// `reservation` must be a pointer previously returned by
/// [`Segment::reserve`] (specifically its [`Segment::reservation`]) and not
/// yet released. `reservation_len` must be the matching length. Must be called
/// exactly once per reservation. A second call on an already-released range is
/// a no-op at the OS level (munmap of an unmapped range / VirtualFree on a
/// released region) but is still a contract violation.
pub(crate) fn release_segment(reservation: *mut u8, reservation_len: usize) {
    let nn = match NonNull::new(reservation) {
        Some(n) => n,
        None => return,
    };
    // SAFETY: the caller's contract (documented above) guarantees `reservation`
    // was returned by `Segment::reserve` and is freed exactly once.
    unsafe { release_reservation(nn, reservation_len) };
}

impl Drop for Segment {
    fn drop(&mut self) {
        // SAFETY: `self.reservation` was returned by the OS reserve call in
        // `reserve_aligned` and is valid for `self.reservation_len` bytes; this
        // `Segment` owns it (no aliasing — Phase 8 is single-threaded and
        // `Segment` is `Send` but not `Sync`). Dropping returns the entire
        // reservation to the OS exactly once. We never touch `self.base`
        // afterwards. See `release_reservation` for the per-OS free call.
        unsafe { release_reservation(self.reservation, self.reservation_len) };
    }
}

// SAFETY (Send): a `Segment` owns its OS reservation exclusively; moving it to
// another thread moves ownership of every byte with it, leaving no aliasing
// access on the origin thread. The memory is plain uninitialised bytes (no
// `Rc`/`Cell`/TLS affinity). Phase 8 is single-threaded so this is conservative
// — Phase 9 per-thread heaps will rely on it.
unsafe impl Send for Segment {}

// NOTE: `Segment` is intentionally NOT `Sync`. `&Segment` is a shared borrow of
// the metadata, but the span itself is mutated through raw pointers in the
// `node` seam under the single-threaded Phase 8 invariant; there is no
// synchronisation for concurrent writes. Phase 9–10 will introduce the
// concurrency primitives.

// ---------------------------------------------------------------------------
// Per-OS reserve / release. Each is one small `unsafe` block with a SAFETY
// proof; the public `Segment::reserve` / `Drop` are the only callers.
// ---------------------------------------------------------------------------

/// Reserve `usable` bytes SEGMENT-aligned from the OS via the over-reserve +
/// trim technique. Returns `(base, reservation_start, reservation_len)` where
/// `base` is SEGMENT-aligned and `base..base+usable` lies within
/// `reservation_start..reservation_start+reservation_len`.
///
/// Returns `None` if the OS refuses the reservation (OOM).
///
/// # Safety contract for callers
///
/// The returned `reservation` MUST later be passed to
/// [`release_reservation`] with the same `reservation_len`, exactly once.
#[cfg(all(windows, not(miri)))]
fn reserve_aligned(usable: usize) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    // 1. Reserve + commit `2 * usable` bytes (the over-reserve). `VirtualAlloc`
    //    with `MEM_RESERVE | MEM_COMMIT` both reserves address space AND
    //    commits physical pages in one call.
    let over = usable.checked_mul(2)?;
    let region = unsafe {
        // SAFETY: `VirtualAlloc(NULL, over, MEM_RESERVE | MEM_COMMIT,
        // PAGE_READWRITE)` is the documented way to reserve+commit a region of
        // `over` bytes. It returns the base of the region (64 KiB-aligned by
        // the OS allocation granularity) or NULL on failure. We immediately
        // check for NULL and bail.
        let p = winapi_virtual_alloc(over);
        NonNull::new(p as *mut u8)?
    };
    let region_ptr = region.as_ptr();
    let region_addr = region_ptr as usize;
    // 2. Find the first SEGMENT-aligned address inside the reservation. There
    //    is always one because we reserved `2 * usable` and the alignment
    //    granularity (SEGMENT) divides `usable`, so the aligned address is at
    //    most `SEGMENT - granularity` past `region_addr`.
    let base_addr = align_up_addr(region_addr, SEGMENT);
    debug_assert!(
        base_addr + usable <= region_addr + over,
        "aligned base must lie within the over-reservation"
    );
    let base = unsafe {
        // SAFETY: `base_addr` is a non-null address inside the reserved region
        // and is SEGMENT-aligned. Casting it to `*mut u8` is sound — the memory
        // is reserved+committed for `over` bytes starting at `region_addr`, and
        // `base_addr` is within that range. `NonNull::new_unchecked` is sound
        // because `region_addr` was non-null and `base_addr >= region_addr`.
        NonNull::new_unchecked(base_addr as *mut u8)
    };
    // 3. Decommit the head and tail so their physical pages return to the OS.
    //    The address-space window stays reserved until drop (Windows cannot
    //    partially `MEM_RELEASE`); only the committed physical memory is
    //    returned, which is what bounds RSS.
    let head = base_addr - region_addr;
    let tail_start = base_addr + usable;
    let tail_len = (region_addr + over) - tail_start;
    if head > 0 {
        unsafe {
            // SAFETY: `[region_addr, region_addr + head)` is within the
            // committed region; `MEM_DECOMMIT` returns its physical pages to
            // the OS while keeping the address space reserved. The address
            // stays valid (reserved) until `MEM_RELEASE` on drop.
            winapi_virtual_decommit(region_ptr, head);
        }
    }
    if tail_len > 0 {
        unsafe {
            // SAFETY: `[tail_start, tail_start + tail_len)` is within the
            // committed region; same `MEM_DECOMMIT` contract as the head.
            winapi_virtual_decommit(tail_start as *mut u8, tail_len);
        }
    }
    Some((base, region, over))
}

#[cfg(all(windows, not(miri)))]
unsafe fn release_reservation(reservation: NonNull<u8>, _reservation_len: usize) {
    // SAFETY: `reservation` was returned by a prior `VirtualAlloc(.., MEM_RESERVE
    // | MEM_COMMIT, ..)` and is owned by the dropping `Segment`. `VirtualFree(..,
    // 0, MEM_RELEASE)` releases the ENTIRE region that was reserved in one call
    // — the `dwSize` argument MUST be 0 in this mode (documented Windows
    // requirement). We pass `_reservation_len` only for symmetry with unix; it
    // is unused here.
    winapi_virtual_release(reservation.as_ptr());
}

// Raw Windows API bindings, declared locally so the crate has NO winapi
// dependency (we want zero added deps for Phase 8). The signatures match the
// Windows SDK; `extern "system"` is the stdcall ABI on win32 / the default on
// win64. Linking against `kernel32` is handled by the std crate (it always
// links `kernel32` on windows).
#[cfg(all(windows, not(miri)))]
extern "system" {
    fn VirtualAlloc(
        lpAddress: *mut core::ffi::c_void,
        dwSize: usize,
        flAllocationType: u32,
        flProtect: u32,
    ) -> *mut core::ffi::c_void;
    fn VirtualFree(
        lpAddress: *mut core::ffi::c_void,
        dwSize: usize,
        dwFreeType: u32,
    ) -> i32;
}

#[cfg(all(windows, not(miri)))]
const MEM_RESERVE: u32 = 0x0000_2000;
#[cfg(all(windows, not(miri)))]
const MEM_COMMIT: u32 = 0x0000_1000;
#[cfg(all(windows, not(miri)))]
const MEM_DECOMMIT: u32 = 0x0000_4000;
#[cfg(all(windows, not(miri)))]
const MEM_RELEASE: u32 = 0x0000_8000;
#[cfg(all(windows, not(miri)))]
const PAGE_READWRITE: u32 = 0x04;

#[cfg(all(windows, not(miri)))]
unsafe fn winapi_virtual_alloc(over: usize) -> *mut core::ffi::c_void {
    // SAFETY: caller (reserve_aligned) passes a non-zero `over` and the flags
    // are the documented reserve+commit + read/write protection combination.
    VirtualAlloc(
        core::ptr::null_mut(),
        over,
        MEM_RESERVE | MEM_COMMIT,
        PAGE_READWRITE,
    )
}

#[cfg(all(windows, not(miri)))]
unsafe fn winapi_virtual_decommit(addr: *mut u8, len: usize) {
    // SAFETY: caller guarantees `[addr, addr+len)` is within a committed
    // region owned by us; `MEM_DECOMMIT` returns the physical pages.
    VirtualFree(addr as *mut core::ffi::c_void, len, MEM_DECOMMIT);
}

#[cfg(all(windows, not(miri)))]
unsafe fn winapi_virtual_release(addr: *mut u8) {
    // SAFETY: caller guarantees `addr` is the base of a region previously
    // reserved via `VirtualAlloc(.., MEM_RESERVE|MEM_COMMIT, ..)`; `MEM_RELEASE`
    // with `dwSize == 0` releases the entire reservation.
    VirtualFree(addr as *mut core::ffi::c_void, 0, MEM_RELEASE);
}

// ---------------------------------------------------------------------------
// Unix path: `mmap` / `munmap`. We do NOT use `madvise` in Phase 8 — page-level
// decommit (M6) is a Phase 10 deliverable; here we only reserve aligned spans.
// ---------------------------------------------------------------------------

#[cfg(all(unix, not(miri)))]
fn reserve_aligned(usable: usize) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    let over = usable.checked_mul(2)?;
    // SAFETY: `mmap(NULL, over, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANON, -1,
    // 0)` is the documented way to obtain an anonymous private mapping of `over`
    // bytes. It returns the base address (page-aligned, kernel-chosen) or
    // `MAP_FAILED` on failure.
    let region_ptr = unsafe {
        let p = libc_mmap(over);
        if p.is_null() || p as usize == usize::MAX {
            return None;
        }
        p
    };
    let region_addr = region_ptr as usize;
    let base_addr = align_up_addr(region_addr, SEGMENT);
    let base = unsafe {
        // SAFETY: `base_addr` is non-null (>= region_addr, which is non-null)
        // and SEGMENT-aligned.
        NonNull::new_unchecked(base_addr as *mut u8)
    };
    // Trim head and tail: `munmap` cleanly unmaps each portion.
    let head = base_addr - region_addr;
    let tail_start = base_addr + usable;
    let tail_len = (region_addr + over) - tail_start;
    if head > 0 {
        unsafe {
            // SAFETY: `[region_addr, region_addr + head)` is within the freshly
            // mapped region; `munmap` unmaps it, returning the address space
            // and physical pages to the kernel.
            libc_munmap(region_ptr, head);
        }
    }
    if tail_len > 0 {
        unsafe {
            // SAFETY: `[tail_start, tail_start + tail_len)` is within the region
            // and after the usable span; `munmap` returns it to the kernel.
            libc_munmap(tail_start as *mut u8, tail_len);
        }
    }
    Some((base, base, usable))
}

#[cfg(all(unix, not(miri)))]
unsafe fn release_reservation(reservation: NonNull<u8>, reservation_len: usize) {
    // SAFETY: `reservation` was the `base` returned by `reserve_aligned` (on
    // unix head/tail are unmapped, so `base` IS the start of the remaining
    // mapping of length `usable == reservation_len`). `munmap` returns it.
    libc_munmap(reservation.as_ptr(), reservation_len);
}

#[cfg(all(unix, not(miri)))]
const PROT_READ: i32 = 0x1;
#[cfg(all(unix, not(miri)))]
const PROT_WRITE: i32 = 0x2;
#[cfg(all(unix, not(miri)))]
const MAP_PRIVATE: i32 = 0x02;
#[cfg(all(unix, not(miri)))]
const MAP_ANON: i32 = 0x20;
#[cfg(all(unix, not(miri)))]
const MAP_FAILED: usize = usize::MAX;

#[cfg(all(unix, not(miri)))]
extern "C" {
    fn mmap(
        addr: *mut core::ffi::c_void,
        length: usize,
        prot: i32,
        flags: i32,
        fd: i32,
        offset: i64,
    ) -> *mut core::ffi::c_void;
    fn munmap(addr: *mut core::ffi::c_void, length: usize) -> i32;
}

#[cfg(all(unix, not(miri)))]
unsafe fn libc_mmap(len: usize) -> *mut core::ffi::c_void {
    // SAFETY: see reserve_aligned. `mmap(NULL, len, RW, PRIVATE|ANON, -1, 0)`
    // requests an anonymous private mapping; the kernel chooses the address.
    let p = mmap(
        core::ptr::null_mut(),
        len,
        PROT_READ | PROT_WRITE,
        MAP_PRIVATE | MAP_ANON,
        -1,
        0,
    );
    if (p as usize) == MAP_FAILED {
        core::ptr::null_mut()
    } else {
        p
    }
}

#[cfg(all(unix, not(miri)))]
unsafe fn libc_munmap(addr: *mut u8, len: usize) {
    // SAFETY: caller guarantees `[addr, addr+len)` was returned by `mmap` and
    // not yet unmapped; `munmap` releases it.
    let _ = munmap(addr as *mut core::ffi::c_void, len);
}

// ---------------------------------------------------------------------------
// Miri aperture: under miri we cannot execute raw FFI (VirtualAlloc/mmap), so
// we fall back to `std::alloc` with SEGMENT alignment. This is sound because
// under miri we are NOT the global allocator — the test harness uses the host
// allocator. The M5 reentrancy test runs WITHOUT miri so it still proves the
// production path never touches `std::alloc`.
// ---------------------------------------------------------------------------

#[cfg(miri)]
fn reserve_aligned(usable: usize) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    use std::alloc::Layout;
    let layout = Layout::from_size_align(usable, SEGMENT).ok()?;
    let ptr = unsafe {
        // SAFETY: `layout` has non-zero size (caller asserts `usable > 0`) and
        // SEGMENT alignment (a power of two). `std::alloc::alloc` returns a
        // pointer to `usable` bytes with the requested alignment, or null on
        // OOM. We are NOT the global allocator under miri, so no reentrancy.
        std::alloc::alloc(layout)
    };
    let base = NonNull::new(ptr)?;
    // Under miri the allocation IS the reservation (no over-reserve/trim).
    Some((base, base, usable))
}

#[cfg(miri)]
unsafe fn release_reservation(reservation: NonNull<u8>, reservation_len: usize) {
    use std::alloc::Layout;
    // SAFETY: `reservation` was returned by `std::alloc::alloc` with
    // `Layout::from_size_align(reservation_len, SEGMENT)` in `reserve_aligned`
    // above, and is freed exactly once. Reconstructing the same layout is sound
    // because `reservation_len` is always a positive multiple of SEGMENT.
    let layout = Layout::from_size_align(reservation_len, SEGMENT)
        .expect("release_reservation: invalid layout");
    std::alloc::dealloc(reservation.as_ptr(), layout);
}

/// Decommit the payload pages of a segment: return their physical backing to the
/// OS while keeping the address-space reservation alive (the header stays mapped
/// for owner-routing reads). This is the M6 delivery — memory freed back to
/// empty segments is returned to the OS so steady-state RSS does not grow
/// unboundedly under churn.
///
/// `base` is the SEGMENT-aligned base. We decommit `[base + start_offset,
/// base + end_offset)` — typically the payload region past the metadata. The
/// offsets MUST be page-aligned and within the segment.
///
/// # Safety contract for callers (this fn is safe; the contract is the
/// caller's invariant)
///
/// The segment at `base` must be a live segment owned by this allocator with no
/// live blocks in the decommitted range. After decommit the pages read as zero
/// on re-access (the OS lazily re-provides zeroed pages on demand for the
/// existing reservation).
#[allow(dead_code)] // M6 infrastructure; exercised by the soak test and future heap integration.
#[cfg(all(windows, not(miri)))]
pub(crate) fn decommit_pages(base: *mut u8, start_offset: usize, end_offset: usize) {
    debug_assert!(start_offset.is_multiple_of(PAGE), "start must be page-aligned");
    debug_assert!(end_offset.is_multiple_of(PAGE), "end must be page-aligned");
    debug_assert!(start_offset < end_offset, "empty range");
    let len = end_offset - start_offset;
    if len == 0 {
        return;
    }
    // SAFETY: `[base + start_offset, base + start_offset + len)` is within a
    // committed segment owned by this allocator. The caller guarantees no live
    // blocks exist in the range. `MEM_DECOMMIT` returns the physical pages to
    // the OS while keeping the address-space reservation (the pages become
    // inaccessible until re-committed; re-access triggers a fresh zero-fill
    // commit from the OS).
    unsafe {
        let addr = base.add(start_offset);
        VirtualFree(addr as *mut core::ffi::c_void, len, MEM_DECOMMIT);
    }
}

#[allow(dead_code)] // M6 infrastructure; exercised by the soak test.
#[cfg(all(unix, not(miri)))]
pub(crate) fn decommit_pages(base: *mut u8, start_offset: usize, end_offset: usize) {
    debug_assert!(start_offset.is_multiple_of(PAGE), "start must be page-aligned");
    debug_assert!(end_offset.is_multiple_of(PAGE), "end must be page-aligned");
    debug_assert!(start_offset < end_offset, "empty range");
    let len = end_offset - start_offset;
    if len == 0 {
        return;
    }
    // SAFETY: `[base + start_offset .. base + start_offset + len)` is within a
    // live segment owned by this allocator. The caller guarantees no live blocks
    // exist in the range. `madvise(MADV_DONTNEED)` tells the kernel to discard
    // the physical pages; subsequent accesses get fresh zeroed pages on demand.
    unsafe {
        let addr = base.add(start_offset);
        libc_madvise_dontneed(addr, len);
    }
}

#[allow(dead_code)] // M6 infrastructure; exercised by the soak test.
#[cfg(miri)]
pub(crate) fn decommit_pages(_base: *mut u8, _start_offset: usize, _end_offset: usize) {
    // Under miri we cannot call OS syscalls. Decommit is a no-op (the M6 soak
    // test asserts the bookkeeping, not the RSS measurement, since miri does not
    // model RSS). The pages remain accessible — correctness is unchanged (the
    // caller already proved no live blocks exist).
}

#[cfg(all(unix, not(miri)))]
extern "C" {
    fn madvise(addr: *mut core::ffi::c_void, length: usize, advice: i32) -> i32;
}

#[cfg(all(unix, not(miri)))]
const MADV_DONTNEED: i32 = 4;

#[cfg(all(unix, not(miri)))]
unsafe fn libc_madvise_dontneed(addr: *mut u8, len: usize) {
    // SAFETY: caller guarantees `[addr, addr+len)` is within a live mmap region.
    // `MADV_DONTNEED` discards the backing pages; re-access produces zero-filled
    // pages on demand.
    let _ = madvise(addr as *mut core::ffi::c_void, len, MADV_DONTNEED);
}

/// Recommit previously-decommitted pages within a segment. On windows this is
/// `VirtualAlloc(MEM_COMMIT)` over the range; on unix re-access after
/// `MADV_DONTNEED` is implicit (the kernel re-provides zeroed pages), so this
/// is a no-op. Under miri: no-op (decommit was a no-op).
#[allow(dead_code)] // M6 infrastructure; future heap integration.
#[cfg(all(windows, not(miri)))]
pub(crate) fn recommit_pages(base: *mut u8, start_offset: usize, end_offset: usize) {
    debug_assert!(start_offset.is_multiple_of(PAGE));
    debug_assert!(end_offset.is_multiple_of(PAGE));
    let len = end_offset - start_offset;
    if len == 0 {
        return;
    }
    // SAFETY: `[base + start_offset .. +len)` is within an address-space
    // reservation owned by this allocator (the segment was reserved with
    // MEM_RESERVE|MEM_COMMIT; after MEM_DECOMMIT the reservation persists).
    // MEM_COMMIT re-commits the physical pages.
    unsafe {
        let addr = base.add(start_offset);
        VirtualAlloc(addr as *mut core::ffi::c_void, len, MEM_COMMIT, PAGE_READWRITE);
    }
}

#[allow(dead_code)] // M6 infrastructure.
#[cfg(all(unix, not(miri)))]
pub(crate) fn recommit_pages(_base: *mut u8, _start_offset: usize, _end_offset: usize) {
    // On unix, re-access after MADV_DONTNEED is implicit — the kernel provides
    // fresh zeroed pages on demand. No syscall needed.
}

#[allow(dead_code)] // M6 infrastructure.
#[cfg(miri)]
pub(crate) fn recommit_pages(_base: *mut u8, _start_offset: usize, _end_offset: usize) {
    // Miri: decommit was a no-op, so recommit is too.
}

/// Round `addr` up to the next multiple of `align` (a power of two). Pure safe
/// arithmetic on addresses (no pointer math).
#[cfg(not(miri))]
fn align_up_addr(addr: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two(), "align must be a power of two");
    let mask = align - 1;
    (addr + mask) & !mask
}
