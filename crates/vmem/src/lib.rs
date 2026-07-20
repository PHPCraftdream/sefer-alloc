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
//! # Fallible vs infallible API (0.2)
//!
//! Every reservation/commit entry point has two forms:
//! - the historical infallible form returning `Option`/`bool`
//!   ([`reserve_aligned`], [`recommit`], …), and
//! - a `try_*` form returning [`Result<_, VmemError>`] whose error carries the
//!   OS `errno` / `GetLastError` cause ([`try_reserve_aligned`],
//!   [`try_recommit`], …).
//!
//! The infallible forms forward to the `try_*` forms and discard the cause, so
//! both stay in perfect lockstep.
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
//! `align` must be a power of two and at least [`PAGE`]. `size` must be a
//! non-zero multiple of [`PAGE`] (so decommit ranges land on page boundaries).
//! Violations return `None` / `Err(VmemError::invalid_argument())` rather than
//! panicking.
//!
//! # Page size ([`page_size`])
//!
//! [`PAGE`] (4 KiB) is the crate's *minimum decommit granularity* — the
//! validation constant. [`page_size`] returns the **actual OS page size**
//! queried once via `sysconf(_SC_PAGESIZE)` (Unix) / `GetSystemInfo` (Windows).
//! On Apple Silicon macOS this is 16 KiB; callers computing decommit offsets
//! must round to `page_size()`, not `PAGE`, to avoid partial decommits.

#![allow(unsafe_code)]
#![deny(missing_docs)]
// Under `mock` the real platform syscalls (decommit/recommit/commit_range) are
// bypassed by the recording backend, so their per-OS `*_impl` helpers become
// legitimately unused. Suppress dead-code only in that configuration; the code
// must still compile everywhere.
#![cfg_attr(feature = "mock", allow(dead_code))]
// `fault_injection`'s hook is only consulted from `try_commit_range`, which is
// itself gated on `lazy-commit`. A caller who enables `fault-injection`
// without `lazy-commit` gets a compiled-but-unreachable hook (harmless — the
// feature is additive and test-only); suppress dead-code only in that
// specific combination.
#![cfg_attr(
    all(feature = "fault-injection", not(feature = "lazy-commit")),
    allow(dead_code)
)]

use core::ptr::NonNull;
use core::sync::atomic::{AtomicUsize, Ordering};

pub mod error;
pub use error::VmemError;

#[cfg(feature = "mock")]
pub mod mock;

#[cfg(feature = "fault-injection")]
pub mod fault_injection;

/// The minimum page size this crate assumes for decommit/recommit granularity:
/// 4 KiB, the smallest unit both `mmap` and `VirtualAlloc` will commit/decommit
/// on the platforms this crate targets. Decommit/recommit offsets passed to the
/// validation in [`decommit`] / [`recommit`] must be multiples of this value.
///
/// This is a compile-time constant (the *minimum*); the real OS page size may
/// be larger — query it with [`page_size`].
pub const PAGE: usize = 1 << 12;

/// Cache for [`page_size`]. `0` means "not yet queried"; a real page size is
/// always a non-zero power of two so `0` is an unambiguous sentinel.
static PAGE_SIZE_CACHE: AtomicUsize = AtomicUsize::new(0);

/// Return the OS page size in bytes, querying the OS once and caching the
/// result.
///
/// Uses `sysconf(_SC_PAGESIZE)` on Unix and `GetSystemInfo` on Windows; under
/// miri (or if the OS query returns a nonsensical value) it falls back to
/// [`PAGE`] (4 KiB). The value is cached in a process-wide atomic after the
/// first call, so repeated calls are a single relaxed load.
///
/// **Correctness:** on Apple Silicon macOS the page size is 16 KiB, and on some
/// Linux configurations 64 KiB. A caller that decommits at 4 KiB-but-not-page
/// multiples would silently do partial work; use this value (not [`PAGE`]) to
/// round decommit offsets.
#[must_use]
pub fn page_size() -> usize {
    let cached = PAGE_SIZE_CACHE.load(Ordering::Relaxed);
    if cached != 0 {
        return cached;
    }
    let queried = query_os_page_size();
    // Guard against an OS returning 0 or a non-power-of-two (never observed in
    // practice, but a hostile/broken value would corrupt every rounding
    // computation downstream). Fall back to PAGE.
    let value = if queried != 0 && queried.is_power_of_two() {
        queried
    } else {
        PAGE
    };
    PAGE_SIZE_CACHE.store(value, Ordering::Relaxed);
    value
}

#[cfg(all(unix, not(miri)))]
fn query_os_page_size() -> usize {
    // SAFETY: `sysconf(_SC_PAGESIZE)` takes an integer name and returns a
    // `c_long` (the page size, or -1 on error). No pointers involved.
    let v = unsafe { sysconf(_SC_PAGESIZE) };
    if v <= 0 {
        0
    } else {
        v as usize
    }
}

#[cfg(all(windows, not(miri)))]
fn query_os_page_size() -> usize {
    // SAFETY: `GetSystemInfo` fills the caller-provided `SYSTEM_INFO`; the
    // struct is stack-allocated and fully written by the call.
    let mut info = SystemInfo::default();
    unsafe { GetSystemInfo(&mut info) };
    info.dw_page_size as usize
}

#[cfg(miri)]
fn query_os_page_size() -> usize {
    // Miri has no real OS page; use the crate's constant granularity.
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
    /// would return `true`.
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
    /// itself does not wrap, then adopts the result via this constructor.
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
    ///   reservation was created with.
    ///
    /// The reservation must be released **exactly once** — by dropping this
    /// handle, or by extracting via `into_parts` and calling [`release`]
    /// manually. Constructing two `Reservation` handles over the same OS
    /// reservation is undefined behaviour (double release).
    ///
    /// On Windows specifically, the reservation MUST have been created with
    /// `MEM_RESERVE | MEM_COMMIT` so `VirtualFree(MEM_RELEASE)` accepts it.
    #[must_use]
    pub unsafe fn from_raw_parts(
        base: *mut u8,
        len: usize,
        reservation: *mut u8,
        reservation_len: usize,
        align: usize,
    ) -> Self {
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

// ---------------------------------------------------------------------------
// Reserve
// ---------------------------------------------------------------------------

/// Reserve `size` bytes of anonymous virtual memory whose base is aligned to
/// `align`, via the over-reserve + trim technique.
///
/// - `align` must be a power of two `>=` [`PAGE`].
/// - `size` must be a non-zero multiple of [`PAGE`].
///
/// Returns `None` on a contract violation or if the OS refuses the reservation
/// (OOM) — never panics, so it is safe to call from inside a `GlobalAlloc`
/// implementation. For the failure cause use [`try_reserve_aligned`].
#[must_use]
pub fn reserve_aligned(size: usize, align: usize) -> Option<Reservation> {
    try_reserve_aligned(size, align).ok()
}

/// Fallible [`reserve_aligned`]: returns a [`VmemError`] carrying the OS cause
/// (`errno` / `GetLastError`) on failure instead of a bare `None`.
///
/// A contract violation (bad `size`/`align`) returns
/// [`VmemError::invalid_argument`] without touching the OS.
pub fn try_reserve_aligned(size: usize, align: usize) -> Result<Reservation, VmemError> {
    if size == 0 || !align.is_power_of_two() || align < PAGE || !size.is_multiple_of(PAGE) {
        return Err(VmemError::invalid_argument());
    }
    // Mock fault-injection: honour a scripted reserve failure first.
    #[cfg(feature = "mock")]
    if let Some(e) = mock::take_reserve_fault() {
        mock::record(mock::Call::Reserve { size, align });
        return Err(e);
    }
    #[cfg(feature = "mock")]
    mock::record(mock::Call::Reserve { size, align });

    match reserve_aligned_raw(size, align) {
        Some((base, reservation, reservation_len)) => Ok(Reservation {
            base,
            len: size,
            reservation,
            reservation_len,
            align,
        }),
        None => Err(VmemError::last_os_error()),
    }
}

/// Release a whole OS reservation obtained from [`Reservation::into_parts`].
///
/// # Safety
///
/// `reservation`, `reservation_len` and `align` must be the three values
/// returned by [`Reservation::into_parts`] (or, for a self-hosting caller that
/// always uses one alignment, that same alignment constant), and the
/// reservation must be released **exactly once**. The native (`munmap` /
/// `VirtualFree`) paths ignore `align`; it is consulted only by the miri
/// fallback to reconstruct the exact `Layout`.
pub unsafe fn release(reservation: *mut u8, reservation_len: usize, align: usize) {
    let nn = match NonNull::new(reservation) {
        Some(n) => n,
        None => return,
    };
    #[cfg(feature = "mock")]
    mock::record(mock::Call::Release {
        reservation: reservation as usize,
        reservation_len,
    });
    // SAFETY: forwarded from the caller's contract above.
    unsafe { release_reservation(nn, reservation_len, align) };
}

// ---------------------------------------------------------------------------
// Decommit / recommit
// ---------------------------------------------------------------------------

/// Decommit pages `[base + start, base + end)`: return their physical backing
/// to the OS while keeping the address-space reservation alive (Linux
/// `MADV_DONTNEED`, Windows `MEM_DECOMMIT`). Re-access after decommit produces
/// fresh zero-filled pages (after [`recommit`] on Windows; implicitly on Unix).
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
    #[cfg(feature = "mock")]
    mock::record(mock::Call::Decommit {
        base: base as usize,
        start,
        end,
    });
    #[cfg(not(feature = "mock"))]
    // SAFETY: forwarded from the caller's contract; the per-OS routine touches
    // only kernel page-state, never the bytes.
    unsafe {
        decommit_pages_impl(base, start, end, DecommitKind::Eager)
    };
}

/// Lazy decommit variant: hint the OS it MAY reclaim `[base+start, base+end)`
/// under memory pressure, cheaper than [`decommit`] (Linux `MADV_FREE`, macOS
/// `MADV_FREE_REUSABLE`, other Unix falls back to `MADV_DONTNEED`; Windows falls
/// back to the eager [`decommit`] path, which has no lazy equivalent).
///
/// Unlike [`decommit`], on Linux the pages are NOT necessarily zeroed on next
/// access if the kernel has not yet reclaimed them (a write before reclamation
/// keeps the old contents and cancels the free) — so this is appropriate only
/// for memory whose contents the caller no longer needs but has not yet
/// overwritten. Cheaper reclaim; the kernel takes pages only under pressure.
///
/// `start`/`end` contract and safety are identical to [`decommit`].
///
/// # Safety
///
/// Same as [`decommit`].
pub unsafe fn decommit_lazy(base: *mut u8, start: usize, end: usize) {
    if start >= end || !start.is_multiple_of(PAGE) || !end.is_multiple_of(PAGE) {
        return;
    }
    #[cfg(feature = "mock")]
    mock::record(mock::Call::DecommitLazy {
        base: base as usize,
        start,
        end,
    });
    #[cfg(not(feature = "mock"))]
    // SAFETY: forwarded from the caller's contract; the per-OS routine touches
    // only kernel page-state, never the bytes.
    unsafe {
        decommit_pages_impl(base, start, end, DecommitKind::Lazy)
    };
}

/// Recommit pages `[base + start, base + end)` previously passed to
/// [`decommit`]. On Windows this re-commits physical pages
/// (`VirtualAlloc(MEM_COMMIT)`); on Unix re-access is implicit so this is a
/// no-op.
///
/// Returns `true` if the range is now committed (or the call was a well-formed
/// no-op — empty range), and `false` if the OS refused to commit the pages
/// (commit-charge exhaustion / true OOM). On `false` the caller MUST NOT write
/// into `[base+start, base+end)`. Never panics. For the cause use
/// [`try_recommit`].
///
/// A contract violation on the offsets (misaligned, or `start >= end`) returns
/// `true` as a no-op.
///
/// # Safety
///
/// `base` must be the [`as_ptr`](Reservation::as_ptr) of a live reservation
/// whose `[base+start, base+end)` range was previously decommitted.
#[must_use]
pub unsafe fn recommit(base: *mut u8, start: usize, end: usize) -> bool {
    // SAFETY: forwarded from the caller's contract.
    unsafe { try_recommit(base, start, end).is_ok() }
}

/// Fallible [`recommit`]: `Ok(())` if the range is now committed (or was a
/// well-formed no-op), `Err(VmemError)` carrying the OS cause on commit
/// failure.
///
/// # Safety
///
/// Same as [`recommit`].
pub unsafe fn try_recommit(base: *mut u8, start: usize, end: usize) -> Result<(), VmemError> {
    if start >= end || !start.is_multiple_of(PAGE) || !end.is_multiple_of(PAGE) {
        return Ok(());
    }
    #[cfg(feature = "mock")]
    {
        mock::record(mock::Call::Recommit {
            base: base as usize,
            start,
            end,
        });
        mock::take_commit_fault().map_or(Ok(()), Err)
    }
    #[cfg(not(feature = "mock"))]
    // SAFETY: forwarded from the caller's contract.
    unsafe {
        recommit_pages_impl(base, start, end)
    }
}

// ---------------------------------------------------------------------------
// Incremental commit (feature `lazy-commit`).
// ---------------------------------------------------------------------------

/// Commit pages `[base + start, base + end)` within an existing reservation.
///
/// This is the incremental-commit building block: after a
/// [`reserve_aligned_lazy`] call that left some pages reserved-but-uncommitted,
/// `commit_range` commits exactly the requested sub-range so it becomes
/// writable. On Windows this issues `VirtualAlloc(MEM_COMMIT)`; on Unix and
/// under miri the pages are already accessible, so this is a no-op that always
/// returns `true`.
///
/// `start` and `end` must be multiples of [`PAGE`] and `start < end`. A
/// contract violation is a no-op returning `true`.
///
/// Returns `true` if the range is now committed, `false` if the OS refused
/// (commit-charge exhaustion / true OOM). On `false` the caller MUST NOT write
/// into the range. Never panics. For the cause use [`try_commit_range`].
///
/// # Difference from [`recommit`]
///
/// [`recommit`] re-commits pages that were PREVIOUSLY committed and then
/// decommitted via [`decommit`]. `commit_range` commits pages that were NEVER
/// committed (reserved via the lazy path). The underlying Windows syscall is
/// the same; the semantic intent differs.
///
/// # Safety
///
/// `base` must be the [`as_ptr`](Reservation::as_ptr) of a live reservation,
/// and `[base+start, base+end)` must fall within that reservation's usable span
/// (i.e. `end <= len`). The range must be currently reserved but not yet
/// committed (or already committed — recommitting is harmless on Windows).
#[must_use]
#[cfg(feature = "lazy-commit")]
pub unsafe fn commit_range(base: *mut u8, start: usize, end: usize) -> bool {
    // SAFETY: forwarded from the caller's contract.
    unsafe { try_commit_range(base, start, end).is_ok() }
}

/// Fallible [`commit_range`]: `Ok(())` on success (or well-formed no-op),
/// `Err(VmemError)` carrying the OS cause on commit failure.
///
/// # Safety
///
/// Same as [`commit_range`].
#[cfg(feature = "lazy-commit")]
pub unsafe fn try_commit_range(base: *mut u8, start: usize, end: usize) -> Result<(), VmemError> {
    if start >= end || !start.is_multiple_of(PAGE) || !end.is_multiple_of(PAGE) {
        return Ok(());
    }
    #[cfg(feature = "mock")]
    {
        mock::record(mock::Call::CommitRange {
            base: base as usize,
            start,
            end,
        });
        mock::take_commit_fault().map_or(Ok(()), Err)
    }
    #[cfg(not(feature = "mock"))]
    {
        // Real-path fault injection (feature `fault-injection`, DISTINCT from
        // `mock`): consult the armed hooks immediately before the real
        // syscall. When neither hook is armed this is two relaxed loads that
        // branch-predict not-taken — negligible on the production path, and
        // compiled out entirely when the feature is off.
        #[cfg(feature = "fault-injection")]
        if fault_injection::should_fail_commit() {
            return Err(VmemError::last_os_error());
        }
        // SAFETY: forwarded from the caller's contract.
        unsafe { commit_range_impl(base, start, end) }
    }
}

/// Reserve `size` bytes of anonymous virtual memory whose base is aligned to
/// `align`, committing ONLY the first `initial_commit` bytes — the rest is
/// reserved but NOT committed (on Windows; on Unix/miri ALL pages are committed,
/// matching the eager path).
///
/// See [`reserve_aligned`] for the base/align contract. `initial_commit` must
/// be a non-zero multiple of [`PAGE`] and `<= size`; violations return `None`.
///
/// The returned [`Reservation`] frees the ENTIRE VA reservation on drop
/// regardless of how much was committed. For the failure cause use
/// [`try_reserve_aligned_lazy`].
#[must_use]
#[cfg(feature = "lazy-commit")]
pub fn reserve_aligned_lazy(
    size: usize,
    align: usize,
    initial_commit: usize,
) -> Option<Reservation> {
    try_reserve_aligned_lazy(size, align, initial_commit).ok()
}

/// Fallible [`reserve_aligned_lazy`].
#[cfg(feature = "lazy-commit")]
pub fn try_reserve_aligned_lazy(
    size: usize,
    align: usize,
    initial_commit: usize,
) -> Result<Reservation, VmemError> {
    if size == 0
        || !align.is_power_of_two()
        || align < PAGE
        || !size.is_multiple_of(PAGE)
        || initial_commit == 0
        || !initial_commit.is_multiple_of(PAGE)
        || initial_commit > size
    {
        return Err(VmemError::invalid_argument());
    }
    #[cfg(feature = "mock")]
    if let Some(e) = mock::take_reserve_fault() {
        mock::record(mock::Call::ReserveLazy {
            size,
            align,
            initial_commit,
        });
        return Err(e);
    }
    #[cfg(feature = "mock")]
    mock::record(mock::Call::ReserveLazy {
        size,
        align,
        initial_commit,
    });

    // Under `mock` the OS partial-commit is bypassed: `commit_range` records-
    // and-returns without touching the OS, so a genuinely partially-committed
    // Windows reservation would leave the tail unwritable and fault when the
    // consumer's mocked "commit" is a no-op. Chain to the EAGER (fully
    // committed) backend instead, so the returned span is entirely usable while
    // the mock still records the `ReserveLazy` call for assertion.
    #[cfg(feature = "mock")]
    let raw = reserve_aligned_raw(size, align);
    #[cfg(not(feature = "mock"))]
    let raw = reserve_aligned_lazy_raw(size, align, initial_commit);

    match raw {
        Some((base, reservation, reservation_len)) => Ok(Reservation {
            base,
            len: size,
            reservation,
            reservation_len,
            align,
        }),
        None => Err(VmemError::last_os_error()),
    }
}

// ---------------------------------------------------------------------------
// Huge / large pages (feature `huge-pages`).
// ---------------------------------------------------------------------------

/// Reserve `size` bytes aligned to `align`, requesting OS **large / huge
/// pages** (Linux `MAP_HUGETLB`, macOS best-effort `MADV_HUGEPAGE`, Windows
/// `MEM_LARGE_PAGES`).
///
/// Large pages reduce TLB pressure for big allocator segments. The request is
/// **best-effort**: if the OS refuses large pages (none configured, no
/// privilege), the reservation transparently falls back to ordinary pages, so
/// this never fails purely because huge pages are unavailable — it fails only
/// on a genuine reservation error (OOM) or a contract violation.
///
/// Base/align/size contract is identical to [`reserve_aligned`]. For the
/// failure cause use [`try_reserve_aligned_huge`].
#[must_use]
#[cfg(feature = "huge-pages")]
pub fn reserve_aligned_huge(size: usize, align: usize) -> Option<Reservation> {
    try_reserve_aligned_huge(size, align).ok()
}

/// Fallible [`reserve_aligned_huge`].
#[cfg(feature = "huge-pages")]
pub fn try_reserve_aligned_huge(size: usize, align: usize) -> Result<Reservation, VmemError> {
    if size == 0 || !align.is_power_of_two() || align < PAGE || !size.is_multiple_of(PAGE) {
        return Err(VmemError::invalid_argument());
    }
    #[cfg(feature = "mock")]
    if let Some(e) = mock::take_reserve_fault() {
        mock::record(mock::Call::ReserveHuge { size, align });
        return Err(e);
    }
    #[cfg(feature = "mock")]
    mock::record(mock::Call::ReserveHuge { size, align });

    match reserve_aligned_huge_raw(size, align) {
        Some((base, reservation, reservation_len)) => Ok(Reservation {
            base,
            len: size,
            reservation,
            reservation_len,
            align,
        }),
        None => Err(VmemError::last_os_error()),
    }
}

// ---------------------------------------------------------------------------
// leak_zeroed_pages: static-lifetime OS-zeroed sidecar.
// ---------------------------------------------------------------------------

/// Reserve `size` bytes of **zero-initialised** anonymous virtual memory and
/// **leak** it for the process lifetime, returning the base pointer.
///
/// Folds the leaked-zeroed-sidecar pattern (used by allocators for pre-main
/// bookkeeping structures that must not route through the very allocator they
/// implement) into one helper:
///
/// - `size` is rounded up to a multiple of [`PAGE`] internally (any non-zero
///   `size` is accepted; a zero `size` returns `None`).
/// - the span is guaranteed all-zero on every backend, INCLUDING the miri
///   fallback (`std::alloc` does not zero; this helper zeroes explicitly under
///   miri), so the returned memory is a valid all-zero initial state.
/// - the reservation is `mem::forget`-leaked: it lives for the process lifetime
///   and is never released.
///
/// Returns `None` on OOM or a zero `size`. The returned pointer is non-null,
/// [`PAGE`]-aligned, and valid for the rounded-up size for the whole process
/// lifetime. Because the reservation is leaked, the returned pointer may be
/// safely turned into a `&'static` by the caller (subject to the caller's own
/// aliasing discipline).
#[must_use]
pub fn leak_zeroed_pages(size: usize) -> Option<NonNull<u8>> {
    if size == 0 {
        return None;
    }
    let rounded = size.checked_add(PAGE - 1)? & !(PAGE - 1);
    let reservation = reserve_aligned(rounded, PAGE)?;
    let base = reservation.as_ptr();

    // Under miri, `reserve_aligned` falls back to `std::alloc`, which does NOT
    // zero the bytes; every real OS backend hands back zeroed pages. Zero
    // explicitly under miri so the all-zero initial-state guarantee holds on
    // every backend.
    #[cfg(miri)]
    // SAFETY: `base` is a fresh, exclusively-owned reservation of `rounded`
    // bytes; nothing else references it yet, so writing zeros is sound.
    unsafe {
        core::ptr::write_bytes(base, 0, rounded);
    }

    // Leak: the sidecar lives for the process lifetime, never released.
    core::mem::forget(reservation);

    // SAFETY: `base` is the non-null `as_ptr` of a successful reservation.
    Some(unsafe { NonNull::new_unchecked(base) })
}

// ===========================================================================
// Windows path: VirtualAlloc / VirtualFree. Raw bindings declared locally so
// the crate has NO winapi/windows-sys dependency. std always links kernel32.
// ===========================================================================

#[cfg(all(windows, not(miri)))]
fn reserve_aligned_raw(size: usize, align: usize) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    win_reserve_commit(size, align, size, 0)
}

/// Windows over-reserve + commit helper shared by the eager, lazy and huge
/// paths. Reserves `size + align` bytes, finds the aligned base, and commits
/// `commit_len` bytes (with `extra_flags` OR-ed into `MEM_COMMIT`, e.g.
/// `MEM_LARGE_PAGES`). Returns the aligned base, the reservation base and the
/// full reservation length. On commit failure the whole reservation is
/// released and `None` returned.
#[cfg(all(windows, not(miri)))]
fn win_reserve_commit(
    size: usize,
    align: usize,
    commit_len: usize,
    extra_commit_flags: u32,
) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    let over = size.checked_add(align)?;
    let region = unsafe {
        // SAFETY: `VirtualAlloc(NULL, over, MEM_RESERVE, PAGE_READWRITE)`
        // reserves (but does not commit) `over` bytes of address space,
        // returning the base or NULL on OOM/refusal. NULL is checked below.
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
            // SAFETY: `region` was returned by the `MEM_RESERVE` call above and
            // has not been released yet; releasing before handing to a caller
            // cannot double-free.
            unsafe { winapi_virtual_release(region_ptr) };
            return None;
        }
    };
    // SAFETY: `base_addr >= region_addr`, within the reserved region, aligned.
    let base = unsafe { NonNull::new_unchecked(base_addr as *mut u8) };
    // SAFETY: `[base_addr, base_addr+commit_len)` is within the just-reserved
    // region (`commit_len <= size`, validated by callers); `MEM_COMMIT` commits
    // exactly this aligned sub-range. NULL indicates commit-charge exhaustion.
    let committed = unsafe {
        VirtualAlloc(
            base_addr as *mut core::ffi::c_void,
            commit_len,
            MEM_COMMIT | extra_commit_flags,
            PAGE_READWRITE,
        )
    };
    if committed.is_null() {
        if extra_commit_flags != 0 {
            // Best-effort large pages: retry the commit with ordinary pages.
            // SAFETY: same range within the same live reservation.
            let plain = unsafe {
                VirtualAlloc(
                    base_addr as *mut core::ffi::c_void,
                    commit_len,
                    MEM_COMMIT,
                    PAGE_READWRITE,
                )
            };
            if !plain.is_null() {
                return Some((base, region, over));
            }
        }
        // SAFETY: `region` reserved above, not yet handed out — release once.
        unsafe { winapi_virtual_release(region_ptr) };
        return None;
    }
    Some((base, region, over))
}

#[cfg(all(windows, not(miri)))]
unsafe fn release_reservation(reservation: NonNull<u8>, _reservation_len: usize, _align: usize) {
    // SAFETY: `reservation` was returned by a prior `VirtualAlloc(.., MEM_RESERVE,
    // ..)` with an inner aligned sub-range separately committed. `VirtualFree(..,
    // 0, MEM_RELEASE)` releases the ENTIRE region regardless of commit state.
    winapi_virtual_release(reservation.as_ptr());
}

#[cfg(all(windows, not(miri)))]
unsafe fn decommit_pages_impl(base: *mut u8, start: usize, end: usize, _kind: DecommitKind) {
    let len = end - start;
    // Windows has no lazy `MADV_FREE` equivalent — both eager and lazy map to
    // `MEM_DECOMMIT`.
    // SAFETY: caller guarantees `[base+start, +len)` is within a committed
    // reservation; `MEM_DECOMMIT` returns the physical pages.
    let addr = unsafe { base.add(start) };
    unsafe { winapi_virtual_decommit(addr, len) };
}

#[cfg(all(windows, not(miri)))]
unsafe fn recommit_pages_impl(base: *mut u8, start: usize, end: usize) -> Result<(), VmemError> {
    let len = end - start;
    // SAFETY: caller guarantees `[base+start, +len)` is within a reservation
    // owned by them; `MEM_COMMIT` re-commits the physical pages. NULL indicates
    // commit-charge exhaustion.
    let addr = unsafe { base.add(start) };
    let committed = unsafe {
        VirtualAlloc(
            addr as *mut core::ffi::c_void,
            len,
            MEM_COMMIT,
            PAGE_READWRITE,
        )
    };
    if committed.is_null() {
        Err(VmemError::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(all(windows, not(miri), feature = "lazy-commit"))]
unsafe fn commit_range_impl(base: *mut u8, start: usize, end: usize) -> Result<(), VmemError> {
    // Same MEM_COMMIT call as recommit (idempotent on Windows).
    // SAFETY: forwarded from the caller's contract.
    unsafe { recommit_pages_impl(base, start, end) }
}

#[cfg(all(windows, not(miri), feature = "lazy-commit"))]
fn reserve_aligned_lazy_raw(
    size: usize,
    align: usize,
    initial_commit: usize,
) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    win_reserve_commit(size, align, initial_commit, 0)
}

#[cfg(all(windows, not(miri), feature = "huge-pages"))]
fn reserve_aligned_huge_raw(
    size: usize,
    align: usize,
) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    win_reserve_commit(size, align, size, MEM_LARGE_PAGES)
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
    fn GetSystemInfo(lp_system_info: *mut SystemInfo);
}

/// Mirrors the Windows `SYSTEM_INFO` struct — only `dwPageSize` is read.
#[cfg(all(windows, not(miri)))]
#[repr(C)]
struct SystemInfo {
    w_processor_architecture: u16,
    w_reserved: u16,
    dw_page_size: u32,
    lp_minimum_application_address: *mut core::ffi::c_void,
    lp_maximum_application_address: *mut core::ffi::c_void,
    dw_active_processor_mask: usize,
    dw_number_of_processors: u32,
    dw_processor_type: u32,
    dw_allocation_granularity: u32,
    w_processor_level: u16,
    w_processor_revision: u16,
}

#[cfg(all(windows, not(miri)))]
impl Default for SystemInfo {
    fn default() -> Self {
        // Zeroed; `GetSystemInfo` overwrites the fields it defines.
        Self {
            w_processor_architecture: 0,
            w_reserved: 0,
            dw_page_size: 0,
            lp_minimum_application_address: core::ptr::null_mut(),
            lp_maximum_application_address: core::ptr::null_mut(),
            dw_active_processor_mask: 0,
            dw_number_of_processors: 0,
            dw_processor_type: 0,
            dw_allocation_granularity: 0,
            w_processor_level: 0,
            w_processor_revision: 0,
        }
    }
}

#[cfg(all(windows, not(miri)))]
const MEM_COMMIT: u32 = 0x0000_1000;
#[cfg(all(windows, not(miri)))]
const MEM_RESERVE: u32 = 0x0000_2000;
#[cfg(all(windows, not(miri)))]
const MEM_DECOMMIT: u32 = 0x0000_4000;
#[cfg(all(windows, not(miri)))]
const MEM_RELEASE: u32 = 0x0000_8000;
#[cfg(all(windows, not(miri), feature = "huge-pages"))]
const MEM_LARGE_PAGES: u32 = 0x2000_0000;
#[cfg(all(windows, not(miri)))]
const PAGE_READWRITE: u32 = 0x04;

#[cfg(all(windows, not(miri)))]
unsafe fn winapi_virtual_reserve(over: usize) -> *mut core::ffi::c_void {
    // SAFETY: `MEM_RESERVE` only — reserve address space without commit.
    VirtualAlloc(core::ptr::null_mut(), over, MEM_RESERVE, PAGE_READWRITE)
}

#[cfg(all(windows, not(miri)))]
unsafe fn winapi_virtual_decommit(addr: *mut u8, len: usize) {
    // SAFETY: caller guarantees `[addr, addr+len)` is within a committed region.
    VirtualFree(addr as *mut core::ffi::c_void, len, MEM_DECOMMIT);
}

#[cfg(all(windows, not(miri)))]
unsafe fn winapi_virtual_release(addr: *mut u8) {
    // SAFETY: caller guarantees `addr` is the base of a `MEM_RESERVE` region;
    // `MEM_RELEASE` + size 0 releases the entire reservation.
    VirtualFree(addr as *mut core::ffi::c_void, 0, MEM_RELEASE);
}

// ===========================================================================
// Unix path: mmap / munmap / madvise. Raw bindings declared locally — no libc
// dependency.
// ===========================================================================

#[cfg(all(unix, not(miri)))]
fn reserve_aligned_raw(size: usize, align: usize) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    unix_reserve(size, align, false)
}

/// Unix reservation shared by the eager and huge paths. When `huge` is `true`
/// the exact-size fast path and over-reserve fallback both request
/// `MAP_HUGETLB` (Linux) and fall back to ordinary pages if the huge mapping
/// fails.
#[cfg(all(unix, not(miri)))]
fn unix_reserve(
    size: usize,
    align: usize,
    huge: bool,
) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    if let Some(exact) = try_reserve_aligned_exact(size, align, huge) {
        return Some(exact);
    }
    let over = size.checked_add(align)?;
    let region_ptr = unsafe {
        // SAFETY: `mmap(NULL, over, RW, PRIVATE|ANON, -1, 0)` — anonymous
        // private mapping; the kernel chooses the address or returns MAP_FAILED
        // (mapped to null by `libc_mmap`).
        let p = libc_mmap(over, huge);
        if p.is_null() {
            // Retry without huge pages if the huge request was the cause.
            if huge {
                // SAFETY: same call, ordinary pages.
                let p2 = libc_mmap(over, false);
                if p2.is_null() {
                    return None;
                }
                p2
            } else {
                return None;
            }
        } else {
            p
        }
    };
    let region_addr = region_ptr as usize;
    let fits = align_up_addr(region_addr, align).and_then(|a| {
        let tail_start = a.checked_add(size)?;
        let region_end = region_addr.checked_add(over)?;
        (tail_start <= region_end).then_some((a, tail_start, region_end))
    });
    let (base_addr, tail_start, region_end) = match fits {
        Some(t) => t,
        None => {
            // SAFETY: `region_ptr` was returned by `mmap` above; releasing the
            // whole `over`-byte mapping before handing to a caller is sound.
            unsafe { libc_munmap(region_ptr as *mut u8, over) };
            return None;
        }
    };
    // SAFETY: `base_addr >= region_addr` and `align`-aligned.
    let base = unsafe { NonNull::new_unchecked(base_addr as *mut u8) };
    let head = base_addr - region_addr;
    let tail_len = region_end - tail_start;
    if head > 0 {
        // SAFETY: `[region_addr, region_addr+head)` is within the mapping.
        unsafe { libc_munmap(region_ptr as *mut u8, head) };
    }
    if tail_len > 0 {
        // SAFETY: `[tail_start, tail_start+tail_len)` is within the mapping.
        unsafe { libc_munmap(tail_start as *mut u8, tail_len) };
    }
    #[cfg(feature = "huge-pages")]
    if huge {
        // SAFETY: `base` is the start of a live `size`-byte mapping; a
        // best-effort `MADV_HUGEPAGE` hint touches only kernel metadata.
        unsafe { libc_madvise_hugepage(base.as_ptr(), size) };
    }
    Some((base, base, size))
}

/// 1-syscall exact-size mmap fast path (see the 0.1 doc). `huge` requests
/// `MAP_HUGETLB`.
#[cfg(all(unix, not(miri)))]
fn try_reserve_aligned_exact(
    size: usize,
    align: usize,
    huge: bool,
) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    let region_ptr = unsafe {
        // SAFETY: anonymous private mapping of exactly `size` bytes.
        let p = libc_mmap(size, huge);
        if p.is_null() {
            return None;
        }
        p
    };
    let region_addr = region_ptr as usize;
    if !region_addr.is_multiple_of(align) {
        // SAFETY: `region_ptr` was just mapped with length `size`; unmap once.
        unsafe { libc_munmap(region_ptr as *mut u8, size) };
        return None;
    }
    // SAFETY: non-null and proven `align`-aligned.
    let base = unsafe { NonNull::new_unchecked(region_ptr as *mut u8) };
    #[cfg(feature = "huge-pages")]
    if huge {
        // SAFETY: `base` is a live `size`-byte mapping; hint-only.
        unsafe { libc_madvise_hugepage(base.as_ptr(), size) };
    }
    Some((base, base, size))
}

#[cfg(all(unix, not(miri)))]
unsafe fn release_reservation(reservation: NonNull<u8>, reservation_len: usize, _align: usize) {
    // SAFETY: on unix `reservation` IS the start of the remaining mapping of
    // length `reservation_len`; `munmap` returns it.
    libc_munmap(reservation.as_ptr(), reservation_len);
}

#[cfg(all(unix, not(miri)))]
unsafe fn decommit_pages_impl(base: *mut u8, start: usize, end: usize, kind: DecommitKind) {
    let len = end - start;
    let addr = unsafe { base.add(start) };
    match kind {
        // SAFETY: caller guarantees `[base+start, +len)` is within a live
        // mapping; `madvise` touches only kernel page-state.
        DecommitKind::Eager => unsafe { libc_madvise(addr, len, MADV_DONTNEED) },
        DecommitKind::Lazy => unsafe { libc_madvise(addr, len, madv_free_advice()) },
    }
}

#[cfg(all(unix, not(miri)))]
unsafe fn recommit_pages_impl(_base: *mut u8, _start: usize, _end: usize) -> Result<(), VmemError> {
    // On unix, re-access after MADV_DONTNEED is implicit — fresh zeroed pages on
    // demand. No syscall, cannot fail.
    Ok(())
}

#[cfg(all(unix, not(miri), feature = "lazy-commit"))]
unsafe fn commit_range_impl(_base: *mut u8, _start: usize, _end: usize) -> Result<(), VmemError> {
    // Unix: pages are already accessible (eager mmap). Always succeeds.
    Ok(())
}

#[cfg(all(unix, not(miri), feature = "lazy-commit"))]
fn reserve_aligned_lazy_raw(
    size: usize,
    align: usize,
    _initial_commit: usize,
) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    reserve_aligned_raw(size, align)
}

#[cfg(all(unix, not(miri), feature = "huge-pages"))]
fn reserve_aligned_huge_raw(
    size: usize,
    align: usize,
) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    unix_reserve(size, align, true)
}

/// Select the lazy-decommit `madvise` advice for this platform.
/// Linux: `MADV_FREE`; macOS: `MADV_FREE_REUSABLE`; other Unix: `MADV_DONTNEED`.
#[cfg(all(unix, not(miri)))]
#[inline]
fn madv_free_advice() -> i32 {
    #[cfg(target_os = "linux")]
    {
        MADV_FREE
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        MADV_FREE_REUSABLE
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "ios")))]
    {
        MADV_DONTNEED
    }
}

#[cfg(all(unix, not(miri)))]
const PROT_READ: i32 = 0x1;
#[cfg(all(unix, not(miri)))]
const PROT_WRITE: i32 = 0x2;
#[cfg(all(unix, not(miri)))]
const MAP_PRIVATE: i32 = 0x02;
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
/// Linux `MAP_HUGETLB` (request huge pages at mmap time).
#[cfg(all(unix, not(miri), target_os = "linux", feature = "huge-pages"))]
const MAP_HUGETLB: i32 = 0x40000;
#[cfg(all(unix, not(miri)))]
const MAP_FAILED: usize = usize::MAX;
#[cfg(all(unix, not(miri)))]
const MADV_DONTNEED: i32 = 4;
/// Linux `MADV_FREE` (lazy reclaim under pressure).
#[cfg(all(unix, not(miri), target_os = "linux"))]
const MADV_FREE: i32 = 8;
/// macOS `MADV_FREE_REUSABLE` (lazy reclaim; page reusable).
#[cfg(all(unix, not(miri), any(target_os = "macos", target_os = "ios")))]
const MADV_FREE_REUSABLE: i32 = 7;
/// Linux `MADV_HUGEPAGE` (transparent-huge-page hint).
#[cfg(all(unix, not(miri), target_os = "linux", feature = "huge-pages"))]
const MADV_HUGEPAGE: i32 = 14;
#[cfg(all(unix, not(miri)))]
const _SC_PAGESIZE: i32 = {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        29
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        // Linux and most other unices use 30 for _SC_PAGESIZE / _SC_PAGE_SIZE.
        30
    }
};

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
    fn sysconf(name: i32) -> core::ffi::c_long;
}

#[cfg(all(unix, not(miri)))]
unsafe fn libc_mmap(len: usize, huge: bool) -> *mut core::ffi::c_void {
    #[cfg_attr(
        not(all(target_os = "linux", feature = "huge-pages")),
        allow(unused_mut)
    )]
    let mut flags = MAP_PRIVATE | MAP_ANON;
    #[cfg(all(target_os = "linux", feature = "huge-pages"))]
    if huge {
        flags |= MAP_HUGETLB;
    }
    let _ = huge; // silence unused on non-linux / no huge-pages builds
                  // SAFETY: anonymous private mapping; kernel chooses the address.
    let p = mmap(
        core::ptr::null_mut(),
        len,
        PROT_READ | PROT_WRITE,
        flags,
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
    // SAFETY: caller guarantees `[addr, addr+len)` was mmap'd and is unmapped once.
    let _ = munmap(addr as *mut core::ffi::c_void, len);
}

#[cfg(all(unix, not(miri)))]
unsafe fn libc_madvise(addr: *mut u8, len: usize, advice: i32) {
    // SAFETY: caller guarantees `[addr, addr+len)` is within a live mmap region.
    let _ = madvise(addr as *mut core::ffi::c_void, len, advice);
}

#[cfg(all(unix, not(miri), target_os = "linux", feature = "huge-pages"))]
unsafe fn libc_madvise_hugepage(addr: *mut u8, len: usize) {
    // SAFETY: caller guarantees `[addr, addr+len)` is within a live mmap region;
    // `MADV_HUGEPAGE` is a best-effort hint (errors ignored).
    let _ = madvise(addr as *mut core::ffi::c_void, len, MADV_HUGEPAGE);
}

#[cfg(all(unix, not(miri), not(target_os = "linux"), feature = "huge-pages"))]
unsafe fn libc_madvise_hugepage(_addr: *mut u8, _len: usize) {
    // Non-Linux Unix: no transparent-huge-page madvise; the mmap fallback
    // already yielded ordinary pages. No-op.
}

// ===========================================================================
// Miri aperture: miri cannot execute raw FFI, so fall back to `std::alloc`.
// ===========================================================================

#[cfg(miri)]
fn reserve_aligned_raw(size: usize, align: usize) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    use std::alloc::Layout;
    let layout = Layout::from_size_align(size, align).ok()?;
    // SAFETY: `layout` has non-zero size and pow2 align; under miri the consumer
    // is not the global allocator, so no reentrancy.
    let ptr = unsafe { std::alloc::alloc(layout) };
    let base = NonNull::new(ptr)?;
    Some((base, base, size))
}

#[cfg(miri)]
unsafe fn release_reservation(reservation: NonNull<u8>, reservation_len: usize, align: usize) {
    use std::alloc::Layout;
    // SAFETY: `reservation` was returned by `std::alloc::alloc` with exactly
    // this layout in `reserve_aligned_raw`; freed once.
    let layout = Layout::from_size_align(reservation_len, align).expect("release: invalid layout");
    std::alloc::dealloc(reservation.as_ptr(), layout);
}

#[cfg(miri)]
unsafe fn decommit_pages_impl(_base: *mut u8, _start: usize, _end: usize, _kind: DecommitKind) {
    // Miri models no RSS; decommit is a no-op.
}

#[cfg(miri)]
unsafe fn recommit_pages_impl(_base: *mut u8, _start: usize, _end: usize) -> Result<(), VmemError> {
    Ok(())
}

#[cfg(all(miri, feature = "lazy-commit"))]
unsafe fn commit_range_impl(_base: *mut u8, _start: usize, _end: usize) -> Result<(), VmemError> {
    Ok(())
}

#[cfg(all(miri, feature = "lazy-commit"))]
fn reserve_aligned_lazy_raw(
    size: usize,
    align: usize,
    _initial_commit: usize,
) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    reserve_aligned_raw(size, align)
}

#[cfg(all(miri, feature = "huge-pages"))]
fn reserve_aligned_huge_raw(
    size: usize,
    align: usize,
) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    // Miri has no huge pages; ordinary allocation is observably identical.
    reserve_aligned_raw(size, align)
}

// ---------------------------------------------------------------------------
// Shared helpers.
// ---------------------------------------------------------------------------

/// Discriminates the eager (`MADV_DONTNEED` / `MEM_DECOMMIT`) vs lazy
/// (`MADV_FREE`) decommit paths. Threaded into `decommit_pages_impl` so both
/// [`decommit`] and [`decommit_lazy`] share one platform routine.
#[derive(Clone, Copy)]
#[allow(dead_code)] // unused under the `mock` feature (syscalls bypassed)
enum DecommitKind {
    Eager,
    Lazy,
}

/// Round `addr` up to the next multiple of `align` (a power of two).
/// Returns `None` on overflow instead of wrapping.
#[cfg(not(miri))]
fn align_up_addr(addr: usize, align: usize) -> Option<usize> {
    debug_assert!(align.is_power_of_two());
    let mask = align - 1;
    addr.checked_add(mask).map(|sum| sum & !mask)
}
