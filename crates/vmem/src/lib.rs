//! `aligned-vmem` — cross-platform **aligned anonymous virtual memory**.
//!
//! Reserve a span of `size` bytes whose base is aligned to an arbitrary
//! power-of-two `align`, commit/decommit its pages, and release it — directly
//! through the OS (`mmap`/`munmap`/`madvise` on Unix, `VirtualAlloc`/
//! `VirtualFree` on Windows), with **no file-mapping machinery** and **no
//! dependencies**. Under [miri](https://github.com/rust-lang/miri) it falls
//! back to `std::alloc` so consumers stay miri-testable.
//!
//! This is the OS aperture extracted from
//! [`sefer-alloc`](https://crates.io/crates/sefer-alloc). It is the one crate
//! whose *entire purpose* is the `unsafe` OS calls — every `unsafe` block
//! carries a `// SAFETY:` proof, and a safe API is exposed on top.
//!
//! # Why not `region` / `memmap2` / `mmap-rs`?
//!
//! Those crates are oriented around **file mappings** and **page-protection**.
//! `aligned-vmem` does one different thing: hand you an *anonymous* span whose
//! **base is aligned to a power of two you choose** (e.g. 2 MiB / 4 MiB for an
//! allocator's segments) via the classic over-reserve + trim technique, plus
//! page-granularity decommit/recommit so you can return physical memory to the
//! OS while keeping the address-space reservation. If you are building an
//! allocator, an arena, or a slab and need "give me a 4 MiB-aligned 4 MiB
//! span", this is the small focused tool.
//!
//! # Example
//!
//! ```
//! use aligned_vmem::{reserve_aligned, release};
//!
//! // Reserve 4 MiB aligned to 4 MiB.
//! let span = 4 * 1024 * 1024;
//! let r = reserve_aligned(span, span).expect("OOM");
//! let base = r.as_ptr();
//! assert_eq!(base as usize % span, 0); // base is `span`-aligned
//!
//! // SAFETY: `base` is valid for `r.len()` bytes; we own it exclusively.
//! unsafe { base.write(0xAB); assert_eq!(base.read(), 0xAB); }
//!
//! // RAII release on drop, or take the parts for manual self-hosted release:
//! let (raw, raw_len, raw_align) = r.into_parts();
//! // SAFETY: the triple came from `into_parts` and is released exactly once.
//! unsafe { release(raw, raw_len, raw_align) };
//! ```
//!
//! # Alignment contract
//!
//! `align` must be a power of two and at least [`page_size`]. `size` must be a
//! non-zero multiple of [`page_size`] (so decommit ranges land on page
//! boundaries). Violations return `None` rather than panicking.

#![allow(unsafe_code)]
#![deny(missing_docs)]

use core::ptr::NonNull;

/// The page size this crate assumes for decommit/recommit granularity: 4 KiB,
/// the smallest unit both `mmap` and `VirtualAlloc` will commit/decommit on the
/// platforms this crate targets. Decommit/recommit offsets must be multiples of
/// this value.
pub const PAGE: usize = 1 << 12;

/// Return the page size used for [`decommit`] / [`recommit`] granularity.
///
/// Currently a compile-time constant ([`PAGE`] = 4 KiB); exposed as a function
/// so a future version can query the OS without a breaking change.
#[must_use]
#[inline]
pub fn page_size() -> usize {
    PAGE
}

/// An owning handle to one aligned span of anonymous virtual memory.
///
/// `as_ptr()` is non-null, aligned to the `align` requested at reservation, and
/// valid for `len()` bytes for the lifetime of this handle. The span is **not**
/// initialised. Dropping the handle returns the whole underlying OS reservation
/// to the OS exactly once.
///
/// For a self-hosted allocator that records `(reservation, reservation_len)` in
/// its own metadata rather than keeping a `Vec<Reservation>`, use
/// [`into_parts`](Self::into_parts) to take the raw reservation (suppressing the
/// `Drop`) and release it later with [`release`].
///
/// `Reservation` is `Send` (the span is owned exclusively) but not `Sync`
/// (writes through the raw pointer are unsynchronised — that is the caller's
/// concern).
pub struct Reservation {
    base: NonNull<u8>,
    len: usize,
    reservation: NonNull<u8>,
    reservation_len: usize,
    /// The alignment requested at reservation time. Carried so the `Drop` /
    /// [`release`] path can reconstruct the exact `Layout` under miri (the
    /// native `munmap` / `VirtualFree` paths ignore it). See [`into_parts`].
    align: usize,
}

impl Reservation {
    /// The aligned usable base of this span. Non-null, valid for [`len`](Self::len)
    /// bytes, aligned to the `align` requested at reservation time.
    #[must_use]
    #[inline]
    pub fn as_ptr(&self) -> *mut u8 {
        self.base.as_ptr()
    }

    /// The number of usable bytes at [`as_ptr`](Self::as_ptr).
    #[must_use]
    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the usable span is empty (always `false` — [`reserve_aligned`]
    /// rejects zero sizes; provided for lint-friendliness).
    #[must_use]
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The start of the underlying OS reservation (may sit below
    /// [`as_ptr`](Self::as_ptr) due to the over-reserve + trim technique).
    #[must_use]
    #[inline]
    pub fn reservation_ptr(&self) -> *mut u8 {
        self.reservation.as_ptr()
    }

    /// The full size of the underlying OS reservation.
    #[must_use]
    #[inline]
    pub const fn reservation_len(&self) -> usize {
        self.reservation_len
    }

    /// Consume the handle WITHOUT releasing the OS reservation, returning the
    /// `(reservation_ptr, reservation_len, align)` the caller must later hand to
    /// [`release`] exactly once. Use this when your allocator records the
    /// reservation in its own self-hosted metadata instead of relying on
    /// `Drop`.
    ///
    /// `align` is the alignment originally requested; the native release paths
    /// ignore it, but it is required for the miri fallback to reconstruct the
    /// exact `Layout`. A self-hosting allocator that always uses one alignment
    /// can pass that constant to [`release`] instead of storing this value.
    #[must_use]
    pub fn into_parts(self) -> (*mut u8, usize, usize) {
        let parts = (self.reservation.as_ptr(), self.reservation_len, self.align);
        core::mem::forget(self);
        parts
    }

    /// Wrap a pre-existing OS reservation (e.g. one obtained from
    /// `VirtualAllocExNuma` or another platform-specific allocator that
    /// `reserve_aligned` does not call directly) in a [`Reservation`] handle.
    ///
    /// The handle then participates in the normal RAII lifecycle: on `Drop`
    /// (or via [`release`]) the underlying reservation is returned to the OS
    /// using the platform-appropriate release routine
    /// (`VirtualFree(MEM_RELEASE)` on Windows, `munmap` on Unix,
    /// `std::alloc::dealloc` on miri).
    ///
    /// This is the inverse of [`into_parts`](Self::into_parts) and exists for
    /// the cross-crate handoff pattern: a sibling crate (`numa-shim` on
    /// Windows) issues a platform-specific reservation call that `aligned-vmem`
    /// itself does not wrap, then adopts the result via this constructor so
    /// downstream code can hold a uniform [`Reservation`] regardless of which
    /// syscall produced it.
    ///
    /// # Safety
    ///
    /// All five values must describe a **live, exclusively-owned OS
    /// reservation** compatible with `aligned-vmem`'s release path:
    ///
    /// - `base` is the *aligned usable* start; non-null, valid for `len` bytes,
    ///   aligned to `align`.
    /// - `len` is the usable span size, a non-zero multiple of [`PAGE`].
    /// - `reservation` is the *underlying OS reservation* start (often equal
    ///   to `base`, but may be lower under the over-reserve + trim technique).
    /// - `reservation_len` is the full size of the OS reservation, a non-zero
    ///   multiple of [`PAGE`], `reservation_len >= len + (base - reservation)`.
    /// - `align` is a power of two `>= PAGE` and matches the alignment the OS
    ///   reservation was created with. The native release paths
    ///   (`VirtualFree` / `munmap`) ignore it; the miri fallback uses it to
    ///   reconstruct the exact `Layout`.
    ///
    /// The reservation must be released **exactly once** — by dropping this
    /// handle, or by extracting via `into_parts` and calling [`release`]
    /// manually. Constructing two `Reservation` handles over the same OS
    /// reservation is undefined behaviour (double release).
    ///
    /// On Windows specifically, the reservation MUST have been created with
    /// `MEM_RESERVE | MEM_COMMIT` so `VirtualFree(MEM_RELEASE)` accepts it.
    /// (`VirtualAllocExNuma(..., MEM_RESERVE | MEM_COMMIT, ...)` satisfies
    /// this — that is the intended call site.)
    #[must_use]
    pub unsafe fn from_raw_parts(
        base: *mut u8,
        len: usize,
        reservation: *mut u8,
        reservation_len: usize,
        align: usize,
    ) -> Self {
        // The contract is enforced by the caller's `unsafe`. We only assert
        // the non-null invariant: a null pointer here would corrupt the
        // `Drop` path which would then call `release_reservation(null, ...)`.
        // In a well-formed call this branch is dead.
        let base_nn = NonNull::new(base).expect("from_raw_parts: base must be non-null");
        let res_nn =
            NonNull::new(reservation).expect("from_raw_parts: reservation must be non-null");
        Self {
            base: base_nn,
            len,
            reservation: res_nn,
            reservation_len,
            align,
        }
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        // SAFETY: `self.reservation` was returned by `reserve_aligned` and is
        // valid for `self.reservation_len` bytes; this handle owns it
        // exclusively (no aliasing — `Reservation` is `Send` but not `Sync`).
        // Dropping returns the entire reservation to the OS exactly once.
        unsafe { release_reservation(self.reservation, self.reservation_len, self.align) };
    }
}

// SAFETY (Send): a `Reservation` owns its OS reservation exclusively; moving it
// to another thread moves ownership of every byte, leaving no aliasing on the
// origin thread. The memory is plain uninitialised bytes (no `Rc`/`Cell`/TLS
// affinity).
unsafe impl Send for Reservation {}

/// Reserve `size` bytes of anonymous virtual memory whose base is aligned to
/// `align`, via the over-reserve + trim technique.
///
/// - `align` must be a power of two `>=` [`PAGE`].
/// - `size` must be a non-zero multiple of [`PAGE`].
///
/// Returns `None` on a contract violation or if the OS refuses the reservation
/// (OOM) — never panics, so it is safe to call from inside a `GlobalAlloc`
/// implementation.
#[must_use]
pub fn reserve_aligned(size: usize, align: usize) -> Option<Reservation> {
    if size == 0 || !align.is_power_of_two() || align < PAGE || !size.is_multiple_of(PAGE) {
        return None;
    }
    reserve_aligned_raw(size, align).map(|(base, reservation, reservation_len)| Reservation {
        base,
        len: size,
        reservation,
        reservation_len,
        align,
    })
}

/// Release a whole OS reservation obtained from [`Reservation::into_parts`].
///
/// # Safety
///
/// `reservation`, `reservation_len` and `align` must be the three values
/// returned by [`Reservation::into_parts`] (or, for a self-hosting caller that
/// always uses one alignment, that same alignment constant), and the
/// reservation must be released **exactly once**. A double release is a
/// contract violation. The native (`munmap` / `VirtualFree`) paths ignore
/// `align`; it is consulted only by the miri fallback to reconstruct the exact
/// `Layout`.
pub unsafe fn release(reservation: *mut u8, reservation_len: usize, align: usize) {
    let nn = match NonNull::new(reservation) {
        Some(n) => n,
        None => return,
    };
    // SAFETY: forwarded from the caller's contract above.
    unsafe { release_reservation(nn, reservation_len, align) };
}

/// Decommit pages `[base + start, base + end)`: return their physical backing
/// to the OS while keeping the address-space reservation alive. Re-access after
/// decommit produces fresh zero-filled pages (after [`recommit`] on Windows;
/// implicitly on Unix).
///
/// `start` and `end` must be multiples of [`PAGE`] and within the span. A
/// no-op if the range is empty.
///
/// # Safety
///
/// `base` must be the [`as_ptr`](Reservation::as_ptr) of a live reservation,
/// and `[base+start, base+end)` must contain no data the caller still needs —
/// its contents are discarded.
pub unsafe fn decommit(base: *mut u8, start: usize, end: usize) {
    if start >= end || !start.is_multiple_of(PAGE) || !end.is_multiple_of(PAGE) {
        return;
    }
    // SAFETY: forwarded from the caller's contract; the per-OS routine touches
    // only kernel page-state, never the bytes.
    unsafe { decommit_pages_impl(base, start, end) };
}

/// Recommit pages `[base + start, base + end)` previously passed to
/// [`decommit`]. On Windows this re-commits physical pages
/// (`VirtualAlloc(MEM_COMMIT)`); on Unix re-access is implicit so this is a
/// no-op.
///
/// `start` and `end` must be multiples of [`PAGE`] and within the span.
///
/// Returns `true` if the range is now committed (or the call was a well-formed
/// no-op — empty range), and `false` if the OS refused to commit the pages
/// (commit-charge exhaustion / true OOM). On `false` the caller MUST NOT write
/// into `[base+start, base+end)`: the pages are still merely reserved, and a
/// write would fault (`STATUS_ACCESS_VIOLATION` on Windows). Never panics, so
/// it is safe to call from inside a `GlobalAlloc` implementation.
///
/// A contract violation on the offsets (misaligned, or `start >= end`) returns
/// `true` as a no-op — no pages are touched, matching the pre-fallible
/// behaviour. Only a genuine OS commit failure yields `false`.
///
/// # Safety
///
/// `base` must be the [`as_ptr`](Reservation::as_ptr) of a live reservation
/// whose `[base+start, base+end)` range was previously decommitted.
#[must_use]
pub unsafe fn recommit(base: *mut u8, start: usize, end: usize) -> bool {
    if start >= end || !start.is_multiple_of(PAGE) || !end.is_multiple_of(PAGE) {
        return true;
    }
    // SAFETY: forwarded from the caller's contract.
    unsafe { recommit_pages_impl(base, start, end) }
}

// ---------------------------------------------------------------------------
// Windows path: VirtualAlloc / VirtualFree. Raw bindings declared locally so
// the crate has NO winapi/windows-sys dependency. std always links kernel32.
// ---------------------------------------------------------------------------

#[cfg(all(windows, not(miri)))]
fn reserve_aligned_raw(size: usize, align: usize) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    let over = size.checked_add(align)?;
    // PERF-PASS-1 (task #49, G4/A3): two-step reserve-then-commit instead of
    // reserve+commit-the-whole-over-allocation-then-trim. The old path
    // committed `over` (up to 2x `size`) bytes via
    // `MEM_RESERVE | MEM_COMMIT`, then `MEM_DECOMMIT`-trimmed the head/tail —
    // a transient 2x commit-charge spike and page-table population for pages
    // discarded microseconds later, plus 3 syscalls total. `MEM_RESERVE`
    // alone (no commit) reserves address space without touching the commit
    // charge or the page tables; only the exact `size`-byte aligned span is
    // then committed — 2 syscalls, zero over-commit. The release path is
    // unchanged: `VirtualFree(region, 0, MEM_RELEASE)` releases the WHOLE
    // reservation regardless of which sub-range was committed, so trimming is
    // no longer needed at all (not even a decommit-trim — the head/tail bytes
    // are simply never committed in the first place).
    let region = unsafe {
        // SAFETY: `VirtualAlloc(NULL, over, MEM_RESERVE, PAGE_READWRITE)`
        // reserves (but does not commit) `over` bytes of address space,
        // returning the base (granularity-aligned) or NULL on OOM/refusal. We
        // check for NULL immediately.
        let p = winapi_virtual_reserve(over);
        NonNull::new(p as *mut u8)?
    };
    let region_ptr = region.as_ptr();
    let region_addr = region_ptr as usize;
    let base_addr = align_up_addr(region_addr, align);
    debug_assert!(base_addr + size <= region_addr + over);
    let base = unsafe {
        // SAFETY: `base_addr` is non-null (>= region_addr) and within the
        // reserved `over`-byte region; aligned to `align`.
        NonNull::new_unchecked(base_addr as *mut u8)
    };
    // SAFETY: `[base_addr, base_addr+size)` is within the just-reserved
    // `over`-byte region (asserted above); `MEM_COMMIT` commits exactly this
    // aligned sub-range, matching the fallible-recommit convention this crate
    // already uses (`recommit_pages_impl`, task-referenced commit 617518f):
    // check `VirtualAlloc`'s return for NULL rather than assuming success.
    let committed = unsafe { winapi_virtual_commit(base_addr as *mut u8, size) };
    if committed.is_null() {
        // Commit failed (commit-charge exhaustion): mirror the reserve path's
        // existing "never panic, return None on OOM" contract. The
        // reservation itself succeeded, so it must still be released before
        // reporting failure — otherwise this leaks the address-space
        // reservation (not physical memory, since nothing was committed, but
        // a leaked VA range nonetheless).
        unsafe {
            // SAFETY: `region` was returned by the `VirtualAlloc(MEM_RESERVE)`
            // call immediately above and has not been released yet; releasing
            // it here (before ever handing it to a caller) cannot double-free.
            winapi_virtual_release(region_ptr);
        }
        return None;
    }
    Some((base, region, over))
}

#[cfg(all(windows, not(miri)))]
unsafe fn release_reservation(reservation: NonNull<u8>, _reservation_len: usize, _align: usize) {
    // SAFETY: `reservation` was returned by a prior `VirtualAlloc(.., MEM_RESERVE,
    // ..)` (PERF-PASS-1, task #49: reserve-only, no longer MEM_COMMIT — see
    // `reserve_aligned_raw` above) with an inner aligned sub-range separately
    // committed via `MEM_COMMIT`. `VirtualFree(.., 0, MEM_RELEASE)` releases
    // the ENTIRE region reserved in that one `VirtualAlloc` call regardless of
    // which (if any) sub-range was subsequently committed — `dwSize` MUST be 0
    // in this mode. This path is intentionally UNCHANGED by the reserve/commit
    // split: it always released the whole reservation, independent of commit
    // state, so it stays correct without modification.
    winapi_virtual_release(reservation.as_ptr());
}

#[cfg(all(windows, not(miri)))]
unsafe fn decommit_pages_impl(base: *mut u8, start: usize, end: usize) {
    let len = end - start;
    // SAFETY: caller guarantees `[base+start, base+start+len)` is within a
    // committed reservation; `MEM_DECOMMIT` returns the physical pages.
    let addr = unsafe { base.add(start) };
    unsafe { winapi_virtual_decommit(addr, len) };
}

#[cfg(all(windows, not(miri)))]
unsafe fn recommit_pages_impl(base: *mut u8, start: usize, end: usize) -> bool {
    let len = end - start;
    // SAFETY: caller guarantees `[base+start, +len)` is within an address-space
    // reservation owned by them; `MEM_COMMIT` re-commits the physical pages.
    // `VirtualAlloc(MEM_COMMIT)` returns the base address on success or NULL when
    // the system cannot back the commit (commit-charge exhaustion). We MUST
    // surface that NULL — writing into a reserved-but-uncommitted page faults
    // (`STATUS_ACCESS_VIOLATION`). Unlike the reserve path we do not need the
    // returned pointer's value (the range is already at a fixed address); only
    // its non-NULL-ness matters.
    let addr = unsafe { base.add(start) };
    let committed = unsafe {
        VirtualAlloc(
            addr as *mut core::ffi::c_void,
            len,
            MEM_COMMIT,
            PAGE_READWRITE,
        )
    };
    !committed.is_null()
}

#[cfg(all(windows, not(miri)))]
extern "system" {
    fn VirtualAlloc(
        lp_address: *mut core::ffi::c_void,
        dw_size: usize,
        fl_allocation_type: u32,
        fl_protect: u32,
    ) -> *mut core::ffi::c_void;
    fn VirtualFree(lp_address: *mut core::ffi::c_void, dw_size: usize, dw_free_type: u32) -> i32;
}

#[cfg(all(windows, not(miri)))]
const MEM_COMMIT: u32 = 0x0000_1000;
#[cfg(all(windows, not(miri)))]
const MEM_RESERVE: u32 = 0x0000_2000;
#[cfg(all(windows, not(miri)))]
const MEM_DECOMMIT: u32 = 0x0000_4000;
#[cfg(all(windows, not(miri)))]
const MEM_RELEASE: u32 = 0x0000_8000;
#[cfg(all(windows, not(miri)))]
const PAGE_READWRITE: u32 = 0x04;

#[cfg(all(windows, not(miri)))]
unsafe fn winapi_virtual_reserve(over: usize) -> *mut core::ffi::c_void {
    // PERF-PASS-1 (task #49, G4/A3): `MEM_RESERVE` only — no `MEM_COMMIT`.
    // SAFETY: non-zero `over` + documented reserve-only + RW protection flags
    // (the protection flags apply once the sub-range is later committed).
    VirtualAlloc(core::ptr::null_mut(), over, MEM_RESERVE, PAGE_READWRITE)
}

#[cfg(all(windows, not(miri)))]
unsafe fn winapi_virtual_commit(addr: *mut u8, len: usize) -> *mut core::ffi::c_void {
    // PERF-PASS-1 (task #49, G4/A3): commit exactly the aligned `[addr,
    // addr+len)` sub-range within an already-reserved region.
    // SAFETY: caller (`reserve_aligned_raw`) guarantees `[addr, addr+len)` is
    // within a region just reserved via `winapi_virtual_reserve`.
    VirtualAlloc(
        addr as *mut core::ffi::c_void,
        len,
        MEM_COMMIT,
        PAGE_READWRITE,
    )
}

#[cfg(all(windows, not(miri)))]
unsafe fn winapi_virtual_decommit(addr: *mut u8, len: usize) {
    // SAFETY: caller guarantees `[addr, addr+len)` is within a committed region.
    VirtualFree(addr as *mut core::ffi::c_void, len, MEM_DECOMMIT);
}

#[cfg(all(windows, not(miri)))]
unsafe fn winapi_virtual_release(addr: *mut u8) {
    // SAFETY: caller guarantees `addr` is the base of a region reserved via
    // `VirtualAlloc(.., MEM_RESERVE, ..)` (PERF-PASS-1: reserve-only, with an
    // inner sub-range separately `MEM_COMMIT`ted — see `reserve_aligned_raw`);
    // `MEM_RELEASE` + size 0 releases the entire reservation regardless of
    // commit state.
    VirtualFree(addr as *mut core::ffi::c_void, 0, MEM_RELEASE);
}

// ---------------------------------------------------------------------------
// Unix path: mmap / munmap / madvise. Raw bindings declared locally — no libc
// dependency.
// ---------------------------------------------------------------------------

#[cfg(all(unix, not(miri)))]
fn reserve_aligned_raw(size: usize, align: usize) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    // PERF-PASS-1 (task #49, G4/A3): try an EXACT-size `mmap(size)` first (1
    // syscall). Linux's top-down mmap placement heuristic (and, in the
    // decommit->recycle->re-reserve cycle, the kernel handing back the same
    // hole just `munmap`ped) often returns an address that already satisfies
    // `align` for a whole-segment-sized request, especially once the process
    // has done a few of these reservations. If the returned address happens
    // to already be `align`-aligned, use it directly. This mirrors mimalloc's
    // own opportunistic-alignment trick.
    if let Some(exact) = try_reserve_aligned_exact(size, align) {
        return Some(exact);
    }
    // Fallback: NOT aligned (or the exact-size mmap failed for a reason
    // unrelated to alignment, e.g. transient OOM at this exact size — retried
    // below with the larger allocation is a legitimate reattempt, not
    // incorrect). Over-reserve `size + align` and trim head/tail exactly as
    // before this pass — functionally identical to the pre-existing
    // behavior, so this path is a strict fallback: never worse than today,
    // worst case `1 (failed exact attempt, if it returned an unaligned
    // mapping) + 1 (munmap of that mapping) + 1 (over-reserve mmap) + up to 2
    // (trim munmaps)` = up to 5 syscalls, same ceiling the review names.
    let over = size.checked_add(align)?;
    let region_ptr = unsafe {
        // SAFETY: `mmap(NULL, over, RW, PRIVATE|ANON, -1, 0)` requests an
        // anonymous private mapping of `over` bytes; the kernel chooses the
        // (page-aligned) address or returns MAP_FAILED.
        let p = libc_mmap(over);
        if p.is_null() {
            return None;
        }
        p
    };
    let region_addr = region_ptr as usize;
    let base_addr = align_up_addr(region_addr, align);
    let base = unsafe {
        // SAFETY: `base_addr` is non-null (>= region_addr) and `align`-aligned.
        NonNull::new_unchecked(base_addr as *mut u8)
    };
    let head = base_addr - region_addr;
    let tail_start = base_addr + size;
    let tail_len = (region_addr + over) - tail_start;
    if head > 0 {
        unsafe {
            // SAFETY: `[region_addr, region_addr + head)` is within the freshly
            // mapped region; `munmap` returns it to the kernel.
            libc_munmap(region_ptr as *mut u8, head);
        }
    }
    if tail_len > 0 {
        unsafe {
            // SAFETY: `[tail_start, tail_start + tail_len)` is within the region
            // and after the usable span; `munmap` returns it.
            libc_munmap(tail_start as *mut u8, tail_len);
        }
    }
    Some((base, base, size))
}

/// PERF-PASS-1 (task #49, G4/A3): attempt the 1-syscall exact-size mmap fast
/// path. Returns `Some((base, base, size))` (matching the over-reserve path's
/// return shape, where `reservation == base` because there is no head/tail to
/// keep track of) if the kernel happened to hand back an already-aligned
/// address; returns `None` (after `munmap`-ing the unaligned mapping, if one
/// was obtained) so the caller can fall back to the over-reserve path. A
/// `None` here is NOT necessarily an OOM signal — it may just mean "not
/// aligned" — so the caller must retry via the fallback, not propagate `None`
/// as final failure.
#[cfg(all(unix, not(miri)))]
fn try_reserve_aligned_exact(
    size: usize,
    align: usize,
) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    let region_ptr = unsafe {
        // SAFETY: `mmap(NULL, size, RW, PRIVATE|ANON, -1, 0)` requests an
        // anonymous private mapping of exactly `size` bytes; the kernel
        // chooses the (page-aligned) address or returns MAP_FAILED (mapped to
        // null by `libc_mmap`).
        let p = libc_mmap(size);
        if p.is_null() {
            return None;
        }
        p
    };
    let region_addr = region_ptr as usize;
    if !region_addr.is_multiple_of(align) {
        // Not aligned — this exact-size mapping is useless for the caller's
        // alignment contract. Give it back to the kernel immediately so the
        // fallback's over-reserve attempt doesn't compete with it for address
        // space, then signal "try the fallback" via `None`.
        unsafe {
            // SAFETY: `region_ptr` was just returned by `mmap` above with
            // length `size`, and is unmapped here exactly once (this whole
            // mapping is being discarded, not partially trimmed).
            libc_munmap(region_ptr as *mut u8, size);
        }
        return None;
    }
    let base = unsafe {
        // SAFETY: `region_ptr` is non-null (checked above) and now proven
        // `align`-aligned.
        NonNull::new_unchecked(region_ptr as *mut u8)
    };
    // `reservation == base` and `reservation_len == size`: there is no
    // head/tail trim in this path, so the entire mapping IS the usable span,
    // identical in shape to the over-reserve path's post-trim invariant.
    Some((base, base, size))
}

#[cfg(all(unix, not(miri)))]
unsafe fn release_reservation(reservation: NonNull<u8>, reservation_len: usize, _align: usize) {
    // SAFETY: on unix the head/tail are unmapped so `reservation` IS the start of
    // the remaining mapping of length `reservation_len`; `munmap` returns it.
    libc_munmap(reservation.as_ptr(), reservation_len);
}

#[cfg(all(unix, not(miri)))]
unsafe fn decommit_pages_impl(base: *mut u8, start: usize, end: usize) {
    // PLATFORM NOTE (XNU/macOS honesty): on Linux `MADV_DONTNEED` is eager —
    // pages are dropped immediately and the next access is guaranteed to
    // zero-fill. On macOS/XNU (and the *BSDs) `MADV_DONTNEED` is ADVISORY and
    // LAZY: it does NOT carry Linux's zero-fill-on-next-access guarantee, and
    // RSS reclamation is best-effort, not prompt. sefer-alloc's CORRECTNESS is
    // unaffected — every `alloc_zeroed` path zeroes explicitly (`Node::zero` in
    // the callers), so nothing relies on the kernel zeroing decommitted pages.
    // Only the RSS-reclaim timing differs on Darwin.
    let len = end - start;
    // SAFETY: caller guarantees `[base+start, +len)` is within a live mapping;
    // `madvise(MADV_DONTNEED)` discards the backing pages (on Linux re-access
    // zero-fills; on XNU/*BSD the hint is lazy — see the platform note above).
    let addr = unsafe { base.add(start) };
    unsafe { libc_madvise_dontneed(addr, len) };
}

#[cfg(all(unix, not(miri)))]
unsafe fn recommit_pages_impl(_base: *mut u8, _start: usize, _end: usize) -> bool {
    // On unix, re-access after MADV_DONTNEED is implicit — the kernel provides
    // fresh zeroed pages on demand. No syscall needed, and this path physically
    // cannot fail (no eager commit to refuse), so always report success.
    true
}

#[cfg(all(unix, not(miri)))]
const PROT_READ: i32 = 0x1;
#[cfg(all(unix, not(miri)))]
const PROT_WRITE: i32 = 0x2;
#[cfg(all(unix, not(miri)))]
const MAP_PRIVATE: i32 = 0x02;
// MAP_ANON value differs across BSD vs Linux. macOS / *BSD use 0x1000,
// Linux uses 0x20. Wrong value silently turns mmap into a file-backed
// mapping attempt (with fd=-1 → EBADF → MAP_FAILED → reserve_aligned
// returns None). Tested in CI's macOS job.
#[cfg(all(unix, not(miri), target_os = "linux"))]
const MAP_ANON: i32 = 0x20;
#[cfg(all(
    unix,
    not(miri),
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly",
    )
))]
const MAP_ANON: i32 = 0x1000;
#[cfg(all(unix, not(miri)))]
const MAP_FAILED: usize = usize::MAX;
#[cfg(all(unix, not(miri)))]
const MADV_DONTNEED: i32 = 4;

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
    fn madvise(addr: *mut core::ffi::c_void, length: usize, advice: i32) -> i32;
}

#[cfg(all(unix, not(miri)))]
unsafe fn libc_mmap(len: usize) -> *mut core::ffi::c_void {
    // SAFETY: `mmap(NULL, len, RW, PRIVATE|ANON, -1, 0)` — anonymous private
    // mapping; the kernel chooses the address.
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
    // SAFETY: caller guarantees `[addr, addr+len)` was returned by `mmap` and is
    // unmapped exactly once.
    let _ = munmap(addr as *mut core::ffi::c_void, len);
}

#[cfg(all(unix, not(miri)))]
unsafe fn libc_madvise_dontneed(addr: *mut u8, len: usize) {
    // SAFETY: caller guarantees `[addr, addr+len)` is within a live mmap region.
    let _ = madvise(addr as *mut core::ffi::c_void, len, MADV_DONTNEED);
}

// ---------------------------------------------------------------------------
// Miri aperture: miri cannot execute raw FFI, so fall back to `std::alloc` with
// the requested alignment. Sound because under miri the consumer is NOT the
// global allocator — the host allocator backs the test harness.
// ---------------------------------------------------------------------------

#[cfg(miri)]
fn reserve_aligned_raw(size: usize, align: usize) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    use std::alloc::Layout;
    let layout = Layout::from_size_align(size, align).ok()?;
    let ptr = unsafe {
        // SAFETY: `layout` has non-zero size and power-of-two alignment; under
        // miri the consumer is not the global allocator, so no reentrancy.
        std::alloc::alloc(layout)
    };
    let base = NonNull::new(ptr)?;
    Some((base, base, size))
}

#[cfg(miri)]
unsafe fn release_reservation(reservation: NonNull<u8>, reservation_len: usize, align: usize) {
    use std::alloc::Layout;
    // SAFETY: `reservation` was returned by `std::alloc::alloc` with exactly
    // `Layout::from_size_align(reservation_len, align)` in `reserve_aligned_raw`
    // (the `align` is threaded through `Reservation`/`into_parts`/`release` so
    // the reconstructed layout matches the allocation), and is freed once.
    let layout = Layout::from_size_align(reservation_len, align).expect("release: invalid layout");
    std::alloc::dealloc(reservation.as_ptr(), layout);
}

#[cfg(miri)]
unsafe fn decommit_pages_impl(_base: *mut u8, _start: usize, _end: usize) {
    // Miri models no RSS; decommit is a no-op (pages stay accessible — the
    // caller already proved nothing live remains in the range).
}

#[cfg(miri)]
unsafe fn recommit_pages_impl(_base: *mut u8, _start: usize, _end: usize) -> bool {
    // Miri: decommit was a no-op, so recommit is too — always succeeds.
    true
}

/// Round `addr` up to the next multiple of `align` (a power of two).
#[cfg(not(miri))]
fn align_up_addr(addr: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    let mask = align - 1;
    (addr + mask) & !mask
}
