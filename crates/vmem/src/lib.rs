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
//! ```text
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
//! Runnable form: `tests/smoke.rs`.
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

    /// Whether the usable span is empty ([`len()`](Self::len) == `0`).
    ///
    /// **Deprecated (task #98 / R4-6):** [`Reservation`] is a non-empty RAII
    /// handle — [`reserve_aligned`] rejects a zero `size`, and the unsafe
    /// [`from_raw_parts`](Self::from_raw_parts) `# Safety` contract likewise
    /// requires a non-zero `len`. So `is_empty` is **always `false`** for every
    /// *valid* `Reservation`: there is no reachable valid state in which it
    /// would return `true`. (The previous test forced `len: 0` through
    /// `from_raw_parts`, i.e. called the unsafe constructor in violation of its
    /// own documented precondition — that proves nothing, since `unsafe` code
    /// that breaks its contract has no defined behaviour to assert against.)
    ///
    /// The method is therefore meaningless and is kept only for semver
    /// compatibility (this crate is independently publishable); prefer an
    /// explicit [`len()`](Self::len) check if a length predicate is ever
    /// needed. If zero-length spans ever become a real, well-defined state with
    /// proper ownership/drop semantics, this deprecation can be lifted — but
    /// that is a larger design decision than a low-risk cleanup.
    #[deprecated(
        note = "Reservation is a non-empty RAII handle; is_empty is always false for any valid instance. Use len() if a length check is needed."
    )]
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
// B0 (R7 Workstream B): incremental-commit foundation.
// ---------------------------------------------------------------------------

/// Commit pages `[base + start, base + end)` within an existing reservation.
///
/// This is the incremental-commit building block: after a
/// [`reserve_aligned_lazy`] call that left some pages reserved-but-uncommitted,
/// `commit_range` commits exactly the requested sub-range so it becomes
/// writable. On Windows this issues `VirtualAlloc(MEM_COMMIT)`; on Unix and
/// under miri the pages are already accessible (lazy-reserve falls back to
/// eager), so this is a no-op that always returns `true`.
///
/// `start` and `end` must be multiples of [`PAGE`] and `start < end`. A
/// contract violation (misaligned offsets or `start >= end`) is a no-op
/// returning `true` (matching [`recommit`]'s convention).
///
/// Returns `true` if the range is now committed, `false` if the OS refused
/// (commit-charge exhaustion / true OOM). On `false` the caller MUST NOT
/// write into the range. Never panics.
///
/// # Difference from [`recommit`]
///
/// [`recommit`] re-commits pages that were PREVIOUSLY committed and then
/// decommitted via [`decommit`]. `commit_range` commits pages that were
/// NEVER committed in the first place (reserved via the lazy path). The
/// underlying Windows syscall is the same (`VirtualAlloc(MEM_COMMIT)`), but
/// the semantic intent differs: `recommit` restores, `commit_range` grows.
/// On non-Windows platforms both are no-ops.
///
/// # Safety
///
/// `base` must be the [`as_ptr`](Reservation::as_ptr) of a live reservation,
/// and `[base+start, base+end)` must fall within that reservation's usable
/// span (i.e. `end <= len`). The range must be currently reserved but not
/// yet committed (or already committed — recommitting an already-committed
/// range is harmless on Windows).
#[must_use]
#[cfg(feature = "alloc-lazy-commit")]
pub unsafe fn commit_range(base: *mut u8, start: usize, end: usize) -> bool {
    if start >= end || !start.is_multiple_of(PAGE) || !end.is_multiple_of(PAGE) {
        return true;
    }
    // SAFETY: forwarded from the caller's contract.
    unsafe { commit_range_impl(base, start, end) }
}

/// Reserve `size` bytes of anonymous virtual memory whose base is aligned to
/// `align`, committing ONLY the first `initial_commit` bytes — the rest is
/// reserved but NOT committed (on Windows; on Unix/miri ALL pages are
/// committed, matching the eager path).
///
/// This is the lazy-commit counterpart of [`reserve_aligned`]: the VA
/// reservation is identical (same base, same length, same alignment, same
/// single-object-freed-once semantics), but on Windows only
/// `[base, base + initial_commit)` is backed by committed physical pages.
/// The remaining `[base + initial_commit, base + size)` range is reserved
/// address space without commit charge — touching those pages before a
/// [`commit_range`] call will fault (`STATUS_ACCESS_VIOLATION`).
///
/// ## Parameters
///
/// - `size`: total usable span (same contract as [`reserve_aligned`]).
/// - `align`: alignment (same contract as [`reserve_aligned`]).
/// - `initial_commit`: how many bytes to commit starting from the aligned
///   base. Must be a non-zero multiple of [`PAGE`] and `<= size`. If these
///   constraints are violated, the function returns `None`.
///
/// ## Drop / release
///
/// The returned [`Reservation`] frees the ENTIRE VA reservation on drop
/// (via `VirtualFree(MEM_RELEASE)` / `munmap` / `std::alloc::dealloc`),
/// regardless of how much was committed. Partial commit does NOT change
/// the release path — the single-reservation-freed-once invariant holds.
///
/// ## Platform behaviour
///
/// | Platform | Behaviour |
/// |----------|-----------|
/// | Windows  | Reserve full VA, commit only `initial_commit` bytes |
/// | Unix     | Eager (all pages committed) — Unix has no commit charge |
/// | Miri     | Eager (all pages committed) — miri cannot model commit |
///
/// Returns `None` on a contract violation or OOM. Never panics.
#[must_use]
#[cfg(feature = "alloc-lazy-commit")]
pub fn reserve_aligned_lazy(
    size: usize,
    align: usize,
    initial_commit: usize,
) -> Option<Reservation> {
    if size == 0
        || !align.is_power_of_two()
        || align < PAGE
        || !size.is_multiple_of(PAGE)
        || initial_commit == 0
        || !initial_commit.is_multiple_of(PAGE)
        || initial_commit > size
    {
        return None;
    }
    reserve_aligned_lazy_raw(size, align, initial_commit).map(
        |(base, reservation, reservation_len)| Reservation {
            base,
            len: size,
            reservation,
            reservation_len,
            align,
        },
    )
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
    // `align_up_addr`/the fit check below are release-mode (not
    // `debug_assert!`-only, L-9d): `region_addr` comes straight from the OS
    // and is not attacker-controlled in practice, but treating a would-be
    // overflow/out-of-range result as an ordinary OOM costs nothing and
    // removes the UB-shaped landmine of trusting unchecked arithmetic on an
    // address.
    let fits = align_up_addr(region_addr, align).and_then(|a| {
        let end = a.checked_add(size)?;
        let region_end = region_addr.checked_add(over)?;
        (end <= region_end).then_some(a)
    });
    let base_addr = match fits {
        Some(a) => a,
        None => {
            unsafe {
                // SAFETY: `region` was returned by the `VirtualAlloc(MEM_RESERVE)`
                // call immediately above and has not been released yet; releasing
                // it here (before ever handing it to a caller) cannot double-free.
                winapi_virtual_release(region_ptr);
            }
            return None;
        }
    };
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

// B0: commit_range — commit a sub-range within an existing reservation.
// Semantically identical to recommit_pages_impl (same VirtualAlloc(MEM_COMMIT)
// call), but exists as a separate function for clarity: recommit restores
// previously-decommitted pages; commit_range grows the committed frontier of a
// lazy reservation. The Windows syscall is the same — MEM_COMMIT is idempotent.
#[cfg(all(windows, not(miri), feature = "alloc-lazy-commit"))]
unsafe fn commit_range_impl(base: *mut u8, start: usize, end: usize) -> bool {
    let len = end - start;
    // SAFETY: caller guarantees `[base+start, +len)` is within an address-space
    // reservation owned by them. `VirtualAlloc(MEM_COMMIT)` commits pages that
    // are currently reserved-but-uncommitted (or already committed — idempotent).
    // Returns the base address on success, NULL on commit-charge exhaustion.
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

// B0: lazy reserve — reserve VA, commit only `initial_commit` bytes.
// The over-reserve + align logic mirrors `reserve_aligned_raw` exactly; only the
// final commit step differs (commit `initial_commit` bytes instead of `size`).
#[cfg(all(windows, not(miri), feature = "alloc-lazy-commit"))]
fn reserve_aligned_lazy_raw(
    size: usize,
    align: usize,
    initial_commit: usize,
) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    let over = size.checked_add(align)?;
    let region = unsafe {
        // SAFETY: `VirtualAlloc(NULL, over, MEM_RESERVE, PAGE_READWRITE)`
        // reserves `over` bytes of address space without committing.
        let p = winapi_virtual_reserve(over);
        NonNull::new(p as *mut u8)?
    };
    let region_ptr = region.as_ptr();
    let region_addr = region_ptr as usize;
    let fits = align_up_addr(region_addr, align).and_then(|a| {
        let end = a.checked_add(size)?;
        let region_end = region_addr.checked_add(over)?;
        (end <= region_end).then_some(a)
    });
    let base_addr = match fits {
        Some(a) => a,
        None => {
            unsafe {
                // SAFETY: `region` was returned by the `VirtualAlloc(MEM_RESERVE)`
                // call immediately above; releasing before handing to a caller.
                winapi_virtual_release(region_ptr);
            }
            return None;
        }
    };
    let base = unsafe {
        // SAFETY: `base_addr` is non-null (>= region_addr), within the reserved
        // region, aligned to `align`.
        NonNull::new_unchecked(base_addr as *mut u8)
    };
    // B0: commit ONLY the initial sub-range [base_addr, base_addr + initial_commit),
    // NOT the full [base_addr, base_addr + size). The rest remains reserved but
    // uncommitted — commit_range will grow it later.
    // SAFETY: `[base_addr, base_addr + initial_commit)` is within the just-reserved
    // region; `initial_commit <= size` (validated by the public API).
    let committed = unsafe { winapi_virtual_commit(base_addr as *mut u8, initial_commit) };
    if committed.is_null() {
        // Commit failed: release the whole reservation (same pattern as
        // reserve_aligned_raw's commit-failure path).
        unsafe {
            // SAFETY: `region` was returned by `VirtualAlloc(MEM_RESERVE)` above.
            winapi_virtual_release(region_ptr);
        }
        return None;
    }
    Some((base, region, over))
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
    // `align_up_addr`/the fit check below are release-mode (not
    // `debug_assert!`-only, L-9d): `region_addr` comes straight from the OS
    // and is not attacker-controlled in practice, but treating a would-be
    // overflow/out-of-range result as an ordinary OOM costs nothing and
    // removes the UB-shaped landmine of trusting unchecked arithmetic on an
    // address.
    let fits = align_up_addr(region_addr, align).and_then(|a| {
        let tail_start = a.checked_add(size)?;
        let region_end = region_addr.checked_add(over)?;
        (tail_start <= region_end).then_some((a, tail_start, region_end))
    });
    let (base_addr, tail_start, region_end) = match fits {
        Some(t) => t,
        None => {
            unsafe {
                // SAFETY: `region_ptr` was returned by the `mmap` call
                // immediately above and has not been trimmed/released yet;
                // releasing the whole `over`-byte mapping here (before ever
                // handing it to a caller) cannot double-free.
                libc_munmap(region_ptr as *mut u8, over);
            }
            return None;
        }
    };
    let base = unsafe {
        // SAFETY: `base_addr` is non-null (>= region_addr) and `align`-aligned.
        NonNull::new_unchecked(base_addr as *mut u8)
    };
    let head = base_addr - region_addr;
    let tail_len = region_end - tail_start;
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

// B0: Unix commit_range — no-op. Unix has no commit/uncommit distinction for
// anonymous mmap'd memory: all pages are demand-paged and accessible. The
// lazy-reserve path falls back to eager on Unix, so commit_range is never
// needed, but the function must still exist so feature-gated callers compile
// on all platforms.
#[cfg(all(unix, not(miri), feature = "alloc-lazy-commit"))]
unsafe fn commit_range_impl(_base: *mut u8, _start: usize, _end: usize) -> bool {
    // Unix: pages are already accessible (eager mmap). Always succeeds.
    true
}

// B0: Unix reserve_aligned_lazy — falls back to the eager path. Unix mmap does
// not have a reserve-without-commit concept for anonymous memory (all pages are
// demand-paged); `initial_commit` is ignored and the full span is mapped.
#[cfg(all(unix, not(miri), feature = "alloc-lazy-commit"))]
fn reserve_aligned_lazy_raw(
    size: usize,
    align: usize,
    _initial_commit: usize,
) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    // Delegate to the eager path — identical observable behaviour.
    reserve_aligned_raw(size, align)
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

// B0: Miri commit_range — no-op. Under miri the lazy-reserve falls back to
// eager (std::alloc, all bytes accessible), so commit_range always succeeds.
#[cfg(all(miri, feature = "alloc-lazy-commit"))]
unsafe fn commit_range_impl(_base: *mut u8, _start: usize, _end: usize) -> bool {
    true
}

// B0: Miri reserve_aligned_lazy — falls back to eager. Miri cannot model
// reserved-but-uncommitted pages; `initial_commit` is ignored.
#[cfg(all(miri, feature = "alloc-lazy-commit"))]
fn reserve_aligned_lazy_raw(
    size: usize,
    align: usize,
    _initial_commit: usize,
) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    reserve_aligned_raw(size, align)
}

/// Round `addr` up to the next multiple of `align` (a power of two).
/// Returns `None` on overflow (the rounded-up value would not fit in
/// `usize`) instead of wrapping — the caller treats this exactly like any
/// other OS-level reservation failure (OOM), never a panic or silent wrap.
#[cfg(not(miri))]
fn align_up_addr(addr: usize, align: usize) -> Option<usize> {
    debug_assert!(align.is_power_of_two());
    let mask = align - 1;
    addr.checked_add(mask).map(|sum| sum & !mask)
}
