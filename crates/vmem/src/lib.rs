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
// legitimately unused. This used to be a crate-wide `allow(dead_code)`, which
// made the whole crate structurally unable to report ANY unused item under
// `--all-features` (task #646/F8). Narrowed to per-item `#[cfg_attr(feature =
// "mock", allow(dead_code))]` on exactly the helpers confirmed (by building
// `--features mock,lazy-commit,huge-pages,fault-injection` on Windows, Unix
// (`--target x86_64-unknown-linux-gnu`) and miri (`--cfg miri`)) to go dead
// under `mock` alone: the per-OS `decommit_pages_impl` / `recommit_pages_impl`
// / `commit_range_impl` / `reserve_aligned_lazy_raw` trio-plus-one on each
// platform, plus the Windows-only `winapi_virtual_decommit` +
// `MEM_DECOMMIT` and the Unix-only `libc_madvise` + `madv_free_advice` +
// `MADV_DONTNEED` + `MADV_FREE` (all only reachable from the real decommit
// path, which `mock` bypasses).
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

// ---------------------------------------------------------------------------
// bench-internals: path-activation counters (task #504, F11 step 1).
// ---------------------------------------------------------------------------
//
// Two independent questions, one instrument each:
//
// - Unix: `unix_reserve` tries an EXACT-size `mmap` first
//   (`try_reserve_aligned_exact`) and only falls through to the over-reserve
//   + trim path on a miss (wrong alignment). The survey
//   (`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` F11) computed this
//   fast path is a net syscall LOSS below a 50% hit rate but noted "nothing
//   anywhere counts this" — `UNIX_EXACT_RESERVE_HITS`/`_ATTEMPTS` settle it
//   with a real number instead of the theoretical bound.
// - Windows: `win_reserve_commit` unconditionally issues 2 syscalls per
//   segment (one `MEM_RESERVE` + one `MEM_COMMIT`), over-reserving `size +
//   align` and never trimming (Windows cannot partially release a
//   `MEM_RESERVE` region). `WINDOWS_RESERVE_COMMIT_CALLS` counts these call
//   PAIRS for parity/comparison against the Unix hit-rate story — there is no
//   fast/slow-path split to measure on Windows today (that is exactly what
//   step 3, a `VirtualAlloc2` prototype, would introduce), so this is a
//   simple activation count, not a hit rate.
//
// `AtomicU64` storage, always compiled (like sefer-alloc's own `dbg_*`
// counters); increments gated on `bench-internals` so a plain build carries
// zero extra instructions. Relaxed — diagnostic only, no ordering obligation.

#[cfg(feature = "bench-internals")]
use core::sync::atomic::AtomicU64;

/// `bench-internals`: total number of `try_reserve_aligned_exact` attempts
/// (Unix only — always 0 on Windows/miri; that internal helper is private and
/// platform-gated, so it is named here in code font rather than linked).
/// Denominator for [`UNIX_EXACT_RESERVE_HITS`]. See the module-level
/// "bench-internals" section doc above.
#[cfg(feature = "bench-internals")]
pub static UNIX_EXACT_RESERVE_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of `try_reserve_aligned_exact` attempts that
/// succeeded (the `mmap` landed already `align`-aligned, no fallback
/// over-reserve+trim needed). Numerator over
/// [`UNIX_EXACT_RESERVE_ATTEMPTS`]. See the module-level "bench-internals"
/// section doc above.
#[cfg(feature = "bench-internals")]
pub static UNIX_EXACT_RESERVE_HITS: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: total number of `win_reserve_commit` calls (Windows
/// only — always 0 on Unix/miri; that internal helper is private and
/// platform-gated, so it is named here in code font rather than linked).
/// Each call issues exactly 2 syscalls
/// (`VirtualAlloc(MEM_RESERVE)` + `VirtualAlloc(MEM_COMMIT)`, plus a possible
/// third best-effort retry on a `huge-pages` commit failure — not counted
/// here, see that call site). See the module-level "bench-internals" section
/// doc above.
#[cfg(feature = "bench-internals")]
pub static WINDOWS_RESERVE_COMMIT_CALLS: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: relaxed snapshot of [`UNIX_EXACT_RESERVE_ATTEMPTS`].
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[must_use]
pub fn unix_exact_reserve_attempts() -> u64 {
    UNIX_EXACT_RESERVE_ATTEMPTS.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of [`UNIX_EXACT_RESERVE_HITS`].
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[must_use]
pub fn unix_exact_reserve_hits() -> u64 {
    UNIX_EXACT_RESERVE_HITS.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of [`WINDOWS_RESERVE_COMMIT_CALLS`].
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[must_use]
pub fn windows_reserve_commit_calls() -> u64 {
    WINDOWS_RESERVE_COMMIT_CALLS.load(Ordering::Relaxed)
}

/// `bench-internals`: reset all three counters
/// ([`UNIX_EXACT_RESERVE_ATTEMPTS`], [`UNIX_EXACT_RESERVE_HITS`],
/// [`WINDOWS_RESERVE_COMMIT_CALLS`]) to 0. Test/bench hook only — lets a
/// measurement window start from a clean count instead of accumulating
/// across the whole process lifetime, mirroring sefer-alloc's established
/// `dbg_reset_*` convention.
#[cfg(feature = "bench-internals")]
pub fn reset_bench_internals_counters() {
    UNIX_EXACT_RESERVE_ATTEMPTS.store(0, Ordering::Relaxed);
    UNIX_EXACT_RESERVE_HITS.store(0, Ordering::Relaxed);
    WINDOWS_RESERVE_COMMIT_CALLS.store(0, Ordering::Relaxed);
}

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
    // Guard against an OS returning 0, a non-power-of-two, or (task #714) a
    // value smaller than PAGE (4 KiB) -- the OS page size is never smaller
    // than PAGE on any target this crate supports, so a queried value below
    // it indicates `query_os_page_size()` read the wrong sysconf(3)
    // parameter entirely (exactly the failure mode a wrong `_SC_PAGESIZE`
    // constant on an untested target produces: a plausible-looking
    // power-of-two answer to a DIFFERENT question). A hostile/broken value
    // would otherwise corrupt every rounding computation downstream. Fall
    // back to PAGE.
    let value = if queried >= PAGE && queried.is_power_of_two() {
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
        since = "0.2.0",
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
    /// **Not the inverse of [`into_parts`](Self::into_parts)** (task #719: an
    /// earlier revision of this doc claimed exactly that, which is
    /// misleading — [`into_parts`](Self::into_parts) returns only 3 of the 5
    /// fields this constructor requires, discarding `base` and `len`
    /// entirely, so `r.into_parts()` cannot be fed straight back into
    /// `from_raw_parts` to reconstruct `r`). [`into_parts`](Self::into_parts)'s
    /// true structural complement is [`release`], whose signature is exactly
    /// the 3-tuple `into_parts` returns
    /// (`reservation_ptr, reservation_len, align`) — that is the intended
    /// matched pair for "take ownership out of RAII, then give it back to
    /// the OS manually". `from_raw_parts` is a separate, more general
    /// constructor for the cross-crate handoff pattern: a sibling crate
    /// (`numa-shim` on Windows) issues a platform-specific reservation call
    /// that `aligned-vmem` itself does not wrap, then adopts the result via
    /// this constructor — it needs `base`/`len` too because the adopted
    /// reservation's usable span need not start at the OS reservation's own
    /// base (the over-reserve + trim technique this crate itself uses
    /// internally is exactly that shape).
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
        // task #719: validate the documented `align`/`reservation_len`
        // contract HERE, at the unsafe call site, rather than leaving it to
        // surface later as a panic inside `Drop::drop` (via the miri
        // backend's `Layout::from_size_align(reservation_len,
        // align).expect(...)` in `release_reservation`) -- a panic reachable
        // from `Drop` is far more dangerous than one at construction time:
        // if this `Reservation` is ever dropped while ANOTHER panic is
        // already unwinding the stack, Rust aborts the whole process on the
        // second panic. Every other construction path in this crate already
        // produces a valid `(align, reservation_len)` pair by construction
        // (validated at each public entry point), so this check is specific
        // to the caller-supplied values `from_raw_parts` accepts. Violating
        // the documented contract is already undefined behaviour per this
        // function's own `# Safety` section; panicking immediately here
        // converts a silently-deferred hazard into a loud, attributable
        // failure at the actual point of misuse.
        //
        // task #776 (F2 revision -- round-closing review finding F7): the
        // original check validated only `align`, but `Layout::from_size_align`
        // also fails when `reservation_len` overflows `isize::MAX` once
        // rounded up to `align` -- an `align`-only check left that half of
        // the SAME Drop-reachable-panic hazard open (e.g.
        // `from_raw_parts(b, PAGE, r, usize::MAX, PAGE)` still constructed
        // successfully and still panicked inside `Drop` under miri). Checking
        // `Layout::from_size_align(...).is_ok()` directly covers both halves
        // of the documented contract in one call, matching exactly what
        // `release_reservation`'s miri backend will later attempt.
        assert!(
            align.is_power_of_two()
                && align >= PAGE
                && std::alloc::Layout::from_size_align(reservation_len, align).is_ok(),
            "Reservation::from_raw_parts: align must be a power of two >= PAGE, and \
             (reservation_len, align) must form a valid Layout; got align={align}, \
             reservation_len={reservation_len}"
        );
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
/// `align` (exact-size mmap fast path on Unix, over-reserve + trim fallback
/// on an alignment miss; Windows always over-reserves and never trims).
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

    // task #713: `reserve_aligned_raw` now captures its own `VmemError`
    // immediately at the point of failure (before any cleanup FFI); this
    // just propagates it rather than re-deriving a possibly-stale one here.
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
///
/// **Platform divergence, not just a data-loss concern:** on Windows,
/// `MEM_DECOMMIT` genuinely unmaps the pages, so a **write to `[base+start,
/// base+end)` before [`recommit`] is a hard `STATUS_ACCESS_VIOLATION`
/// crash**, not a soft re-fault. On Linux, `MADV_DONTNEED` keeps the mapping
/// resident and transparently re-faults a fresh zero page on next write, so
/// the same code that is safe on Linux can crash on Windows. This exact
/// divergence already crashed an in-repo consumer that assumed the Linux
/// semantics — see `docs/CORRECTNESS_OPEN_ITEMS.md` item 6 (filed 2026-07-30)
/// for the incident record and status.
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
/// no-op — empty range, `start == end`), and `false` if the OS refused to
/// commit the pages (commit-charge exhaustion / true OOM) OR the offsets
/// violated the contract below. On `false` the caller MUST NOT write into
/// `[base+start, base+end)`. Never panics. For the cause use [`try_recommit`].
///
/// # Safety
///
/// `base` must be the [`as_ptr`](Reservation::as_ptr) of a live reservation
/// whose `[base+start, base+end)` range was previously decommitted.
/// `start`/`end` must be multiples of [`PAGE`] with `start <= end` — a
/// violation returns `false` (task #712: an earlier version of this function
/// clamped a contract violation to the WRITE-PERMITTING `true` sentinel,
/// which already caused a real crash — see `docs/CORRECTNESS_OPEN_ITEMS.md`
/// for the incident this class of bug produces on Windows).
#[must_use]
pub unsafe fn recommit(base: *mut u8, start: usize, end: usize) -> bool {
    // SAFETY: forwarded from the caller's contract.
    unsafe { try_recommit(base, start, end).is_ok() }
}

/// Fallible [`recommit`]: `Ok(())` if the range is now committed (or was a
/// well-formed no-op), `Err(VmemError::invalid_argument())` if the offsets
/// violated the contract (misaligned, or `start > end`), `Err(VmemError)`
/// carrying the OS cause on genuine commit failure.
///
/// # Safety
///
/// Same as [`recommit`].
pub unsafe fn try_recommit(base: *mut u8, start: usize, end: usize) -> Result<(), VmemError> {
    if start == end {
        return Ok(());
    }
    if start > end || !start.is_multiple_of(PAGE) || !end.is_multiple_of(PAGE) {
        return Err(VmemError::invalid_argument());
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
/// `start` and `end` must be multiples of [`PAGE`] with `start <= end`. A
/// genuinely empty range (`start == end`) is a no-op returning `true`; any
/// other contract violation (misaligned, or `start > end`) returns `false`
/// (task #712: an earlier version of this function clamped a contract
/// violation to the WRITE-PERMITTING `true` sentinel, which already caused a
/// real crash — see `docs/CORRECTNESS_OPEN_ITEMS.md` for the incident this
/// class of bug produces on Windows).
///
/// Returns `true` if the range is now committed, `false` if the OS refused
/// (commit-charge exhaustion / true OOM) OR the offsets violated the contract
/// above. On `false` the caller MUST NOT write into the range. Never panics.
/// For the cause use [`try_commit_range`].
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
///
/// **Concurrent calls are safe** (task #776, F14): multiple threads may call
/// `commit_range` concurrently on ranges within the SAME reservation, whether
/// the ranges overlap or not — `VirtualAlloc(MEM_COMMIT)` (Windows) is itself
/// thread-safe and idempotent, and the Unix/miri backends are no-ops (the
/// entire span is already committed eagerly on those platforms). This does
/// NOT relax the range/liveness contract above; it only states that issuing
/// several legal calls from different threads at once is not itself a new
/// hazard.
#[must_use]
#[cfg(feature = "lazy-commit")]
pub unsafe fn commit_range(base: *mut u8, start: usize, end: usize) -> bool {
    // SAFETY: forwarded from the caller's contract.
    unsafe { try_commit_range(base, start, end).is_ok() }
}

/// Fallible [`commit_range`]: `Ok(())` on success (or a genuinely empty
/// no-op, `start == end`), `Err(VmemError::invalid_argument())` if the
/// offsets violated the contract (misaligned, or `start > end`),
/// `Err(VmemError)` carrying the OS cause on genuine commit failure.
///
/// # Safety
///
/// Same as [`commit_range`].
#[cfg(feature = "lazy-commit")]
pub unsafe fn try_commit_range(base: *mut u8, start: usize, end: usize) -> Result<(), VmemError> {
    if start == end {
        return Ok(());
    }
    if start > end || !start.is_multiple_of(PAGE) || !end.is_multiple_of(PAGE) {
        return Err(VmemError::invalid_argument());
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
            // task #713: this is a SIMULATED failure — no real syscall ran,
            // so `VmemError::last_os_error()` would read whatever `errno`/
            // `GetLastError` happens to be lying around from unrelated prior
            // code, not a cause tied to this call at all.
            // `os_refusal_unknown_code()` states plainly that the OS refused
            // with no (real) code to report, instead of manufacturing a
            // misleading one.
            return Err(VmemError::os_refusal_unknown_code());
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

    // task #713: both `raw` branches now capture their own `VmemError`
    // immediately at the point of failure; this just propagates it.
    raw.map(|(base, reservation, reservation_len)| Reservation {
        base,
        len: size,
        reservation,
        reservation_len,
        align,
    })
}

// ---------------------------------------------------------------------------
// Huge / large pages (feature `huge-pages`).
// ---------------------------------------------------------------------------

/// Reserve `size` bytes aligned to `align`, requesting OS **large / huge
/// pages** (Linux `MAP_HUGETLB` + `MADV_HUGEPAGE`, Windows `MEM_LARGE_PAGES`).
/// Currently a **no-op on macOS and other non-Linux Unix** — it falls back to
/// an ordinary reservation, identical to [`reserve_aligned`].
///
/// Large pages reduce TLB pressure for big allocator segments. The request is
/// **best-effort**: if the OS refuses large pages (none configured, no
/// privilege), the reservation transparently falls back to ordinary pages, so
/// this never fails purely because huge pages are unavailable — it fails only
/// on a genuine reservation error (OOM) or a contract violation.
///
/// Base/align/size contract is otherwise identical to [`reserve_aligned`],
/// **except on Linux with `huge-pages` enabled** (task #776, F3): `size` and
/// `align` must BOTH additionally be multiples of the Linux huge-page size
/// (2 MiB) — a request that only satisfies `reserve_aligned`'s own weaker
/// `PAGE`-multiple contract is rejected with `VmemError::invalid_argument()`
/// before any syscall runs, even though such a request could previously
/// succeed there via the documented ordinary-page fallback (task #714 added
/// this rejection to close a real `munmap` mapping leak — see that task's
/// own commit for the full reasoning; the trade-off is a stricter contract
/// in exchange for a provably-correct trim). For the failure cause use
/// [`try_reserve_aligned_huge`].
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

    // task #713: `reserve_aligned_huge_raw` now captures its own `VmemError`
    // immediately at the point of failure; this just propagates it.
    reserve_aligned_huge_raw(size, align).map(|(base, reservation, reservation_len)| Reservation {
        base,
        len: size,
        reservation,
        reservation_len,
        align,
    })
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
fn reserve_aligned_raw(
    size: usize,
    align: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    win_reserve_commit(size, align, size, 0)
}

/// Windows over-reserve + commit helper shared by the eager, lazy and huge
/// paths. Reserves `size + align` bytes, finds the aligned base, and commits
/// `commit_len` bytes (with `extra_flags` OR-ed into `MEM_COMMIT`, e.g.
/// `MEM_LARGE_PAGES`). Returns the aligned base, the reservation base and the
/// full reservation length. On commit failure the whole reservation is
/// released and `Err` returned.
///
/// task #713: every `Err` here carries a [`VmemError`] captured IMMEDIATELY
/// after the syscall that produced it, before any cleanup FFI call that could
/// clobber `GetLastError` — a fit-computation failure (not a real OS refusal)
/// maps to [`VmemError::invalid_argument`] rather than a stale/irrelevant
/// error code.
#[cfg(all(windows, not(miri)))]
fn win_reserve_commit(
    size: usize,
    align: usize,
    commit_len: usize,
    extra_commit_flags: u32,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    let over = size
        .checked_add(align)
        .ok_or_else(VmemError::invalid_argument)?;
    let region = unsafe {
        // SAFETY: `VirtualAlloc(NULL, over, MEM_RESERVE, PAGE_READWRITE)`
        // reserves (but does not commit) `over` bytes of address space,
        // returning the base or NULL on OOM/refusal. NULL is checked below.
        let p = winapi_virtual_reserve(over);
        match NonNull::new(p as *mut u8) {
            Some(n) => n,
            // Nothing was reserved; no cleanup needed, so capturing here is
            // already the immediate-capture the task requires.
            None => return Err(VmemError::last_os_error()),
        }
    };
    let region_ptr = region.as_ptr();
    // task #717: `.addr()` reads the address without exposing provenance
    // (strict-provenance-legal); the paired `.with_addr()` below reconstructs
    // `base` carrying `region_ptr`'s OWN provenance (valid for the whole
    // `over`-byte reservation) at the computed aligned address, instead of
    // the previous `base_addr as *mut u8` cast, which manufactured a pointer
    // with no established provenance at all (contradicted the README's
    // documented "no exposed-address `as usize` round-trips" guarantee).
    let region_addr = region_ptr.addr();
    let fits = align_up_addr(region_addr, align).and_then(|a| {
        let end = a.checked_add(size)?;
        let region_end = region_addr.checked_add(over)?;
        (end <= region_end).then_some(a)
    });
    let base_addr = match fits {
        Some(a) => a,
        None => {
            // Not an OS refusal — an internal fit-computation failure (should
            // not occur given `over = size + align`); do not read errno here.
            // SAFETY: `region` was returned by the `MEM_RESERVE` call above and
            // has not been released yet; releasing before handing to a caller
            // cannot double-free.
            unsafe { winapi_virtual_release(region_ptr) };
            return Err(VmemError::invalid_argument());
        }
    };
    // SAFETY: `base_addr >= region_addr`, within the reserved region, aligned;
    // `region_ptr.with_addr` carries `region_ptr`'s provenance to the new
    // address, so `base` is a valid derived pointer into the live reservation.
    let base = unsafe { NonNull::new_unchecked(region_ptr.with_addr(base_addr)) };
    // SAFETY: `[base_addr, base_addr+commit_len)` is within the just-reserved
    // region (`commit_len <= size`, validated by callers); `MEM_COMMIT` commits
    // exactly this aligned sub-range. NULL indicates commit-charge exhaustion.
    let committed = unsafe {
        VirtualAlloc(
            base.as_ptr().cast(),
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
                VirtualAlloc(base.as_ptr().cast(), commit_len, MEM_COMMIT, PAGE_READWRITE)
            };
            if !plain.is_null() {
                #[cfg(feature = "bench-internals")]
                WINDOWS_RESERVE_COMMIT_CALLS.fetch_add(1, Ordering::Relaxed);
                return Ok((base, region, over));
            }
            // Capture immediately after the FINAL failing syscall (the plain
            // retry), before the cleanup release below.
            let err = VmemError::last_os_error();
            // SAFETY: `region` reserved above, not yet handed out — release once.
            unsafe { winapi_virtual_release(region_ptr) };
            return Err(err);
        }
        // Capture immediately after the failing commit, before cleanup.
        let err = VmemError::last_os_error();
        // SAFETY: `region` reserved above, not yet handed out — release once.
        unsafe { winapi_virtual_release(region_ptr) };
        return Err(err);
    }
    #[cfg(feature = "bench-internals")]
    WINDOWS_RESERVE_COMMIT_CALLS.fetch_add(1, Ordering::Relaxed);
    Ok((base, region, over))
}

#[cfg(all(windows, not(miri)))]
unsafe fn release_reservation(reservation: NonNull<u8>, _reservation_len: usize, _align: usize) {
    // SAFETY: `reservation` was returned by a prior `VirtualAlloc(.., MEM_RESERVE,
    // ..)` with an inner aligned sub-range separately committed. `VirtualFree(..,
    // 0, MEM_RELEASE)` releases the ENTIRE region regardless of commit state.
    winapi_virtual_release(reservation.as_ptr());
}

#[cfg(all(windows, not(miri)))]
// mock (task #646/F8): bypassed by the recording backend, unused when `mock`
// alone is enabled without a real decommit call site reachable.
#[cfg_attr(feature = "mock", allow(dead_code))]
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
// mock (task #646/F8): see decommit_pages_impl above.
#[cfg_attr(feature = "mock", allow(dead_code))]
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
// mock (task #646/F8): see decommit_pages_impl above; `try_commit_range`'s
// real-path branch is compiled out under `mock`, so this never gets called.
#[cfg_attr(feature = "mock", allow(dead_code))]
unsafe fn commit_range_impl(base: *mut u8, start: usize, end: usize) -> Result<(), VmemError> {
    // Same MEM_COMMIT call as recommit (idempotent on Windows).
    // SAFETY: forwarded from the caller's contract.
    unsafe { recommit_pages_impl(base, start, end) }
}

#[cfg(all(windows, not(miri), feature = "lazy-commit"))]
// mock (task #646/F8): `try_reserve_aligned_lazy`'s real-path branch is
// compiled out under `mock`, so this never gets called.
#[cfg_attr(feature = "mock", allow(dead_code))]
fn reserve_aligned_lazy_raw(
    size: usize,
    align: usize,
    initial_commit: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    win_reserve_commit(size, align, initial_commit, 0)
}

#[cfg(all(windows, not(miri), feature = "huge-pages"))]
fn reserve_aligned_huge_raw(
    size: usize,
    align: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
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
// mock (task #646/F8): only consumed by winapi_virtual_decommit below, which
// itself is unused under `mock`.
#[cfg_attr(feature = "mock", allow(dead_code))]
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
// mock (task #646/F8): see decommit_pages_impl above.
#[cfg_attr(feature = "mock", allow(dead_code))]
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
fn reserve_aligned_raw(
    size: usize,
    align: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    unix_reserve(size, align, false)
}

/// Unix reservation shared by the eager and huge paths. When `huge` is `true`
/// the exact-size fast path and over-reserve fallback both request
/// `MAP_HUGETLB` (Linux) and fall back to ordinary pages if the huge mapping
/// fails.
///
/// task #713: every `Err` here carries a [`VmemError`] captured IMMEDIATELY
/// after the syscall that produced it, before any cleanup `munmap` call that
/// could clobber `errno` — a fit-computation failure (not a real OS refusal)
/// maps to [`VmemError::invalid_argument`] rather than a stale/irrelevant
/// error code. The exact-size fast path's own failure cause is discarded
/// (this function always falls through to the over-reserve attempt
/// regardless of why the fast path failed — unchanged control flow from
/// before this task); the final returned error is always the over-reserve
/// attempt's own.
///
/// task #714 (rust-intel audit MEDIUM §F1): on Linux, `huge` requests
/// `MAP_HUGETLB`, and Linux's `mmap(2)` "Huge TLB mappings" section requires
/// BOTH `munmap(2)`'s `addr` and `length` to be multiples of the huge page
/// size (`man 2 mmap`: "the length ... must also be huge page aligned" for
/// `MAP_HUGETLB`; the kernel additionally guarantees an anonymous
/// `MAP_HUGETLB` mapping with `addr == NULL` starts at a huge-page-aligned
/// address). This function's over-reserve/head-tail-trim path (below)
/// previously trimmed a hugetlb mapping down to a caller-supplied `size` at
/// ordinary [`PAGE`] granularity, which is misaligned for a hugetlb mapping
/// unless `size` happens to already be huge-page-aligned — the resulting
/// `munmap` calls fail `EINVAL` (silently discarded by this function's own
/// `let _ = munmap(...)` cleanup calls), leaking the ENTIRE untrimmed
/// mapping (plus its pinned physical huge pages) on every affected
/// reservation AND on every subsequent [`release`].
///
/// REASONED-FROM-SPEC, NOT empirically verified (per this task's own
/// instruction: no hugetlb-configured host is in this project's CI). Fixed
/// by requiring `size` AND `align` to both be multiples of
/// [`LINUX_HUGE_PAGE_SIZE`] before attempting a Linux huge-page reservation
/// at all — with both huge-page-aligned, `over = size + align` is also
/// huge-page-aligned, the kernel-guaranteed huge-page-aligned `region_addr`
/// makes `head` provably `0` (`align_up_addr` of an already-aligned address
/// to an aligned `align` is a no-op), and `tail_len` (the difference of two
/// huge-page-aligned addresses) is provably huge-page-aligned too — so every
/// `munmap` this function can still reach is provably conformant, not merely
/// less likely to misalign. A caller that does not supply huge-page-aligned
/// `size`/`align` gets a clean, documented [`VmemError::invalid_argument`]
/// instead of a silent leak.
#[cfg(all(unix, not(miri)))]
fn unix_reserve(
    size: usize,
    align: usize,
    huge: bool,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    #[cfg(all(target_os = "linux", feature = "huge-pages"))]
    if huge
        && (!size.is_multiple_of(LINUX_HUGE_PAGE_SIZE)
            || !align.is_multiple_of(LINUX_HUGE_PAGE_SIZE))
    {
        return Err(VmemError::invalid_argument());
    }
    if let Ok(exact) = try_reserve_aligned_exact(size, align, huge) {
        return Ok(exact);
    }
    let over = size
        .checked_add(align)
        .ok_or_else(VmemError::invalid_argument)?;
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
                    // Nothing was mapped; no cleanup needed, so capturing
                    // here is already the immediate-capture the task requires.
                    return Err(VmemError::last_os_error());
                }
                p2
            } else {
                return Err(VmemError::last_os_error());
            }
        } else {
            p
        }
    };
    // task #717: `.addr()`/`.with_addr()` (strict-provenance) replace the
    // previous `as usize` / `as *mut u8` round-trip — see win_reserve_commit's
    // matching comment above for the full reasoning.
    let region_addr = region_ptr.addr();
    let fits = align_up_addr(region_addr, align).and_then(|a| {
        let tail_start = a.checked_add(size)?;
        let region_end = region_addr.checked_add(over)?;
        (tail_start <= region_end).then_some((a, tail_start, region_end))
    });
    let (base_addr, tail_start, region_end) = match fits {
        Some(t) => t,
        None => {
            // Not an OS refusal — an internal fit-computation failure (should
            // not occur given `over = size + align`); do not read errno here.
            // SAFETY: `region_ptr` was returned by `mmap` above; releasing the
            // whole `over`-byte mapping before handing to a caller is sound.
            unsafe { libc_munmap(region_ptr.cast(), over) };
            return Err(VmemError::invalid_argument());
        }
    };
    // SAFETY: `base_addr >= region_addr` and `align`-aligned; `with_addr`
    // carries `region_ptr`'s provenance (valid for the whole `over`-byte
    // mapping) to the computed address.
    let base = unsafe { NonNull::new_unchecked(region_ptr.with_addr(base_addr).cast::<u8>()) };
    let head = base_addr - region_addr;
    let tail_len = region_end - tail_start;
    if head > 0 {
        // SAFETY: `[region_addr, region_addr+head)` is within the mapping.
        unsafe { libc_munmap(region_ptr.cast(), head) };
    }
    if tail_len > 0 {
        // SAFETY: `[tail_start, tail_start+tail_len)` is within the mapping;
        // `with_addr` carries `region_ptr`'s provenance to `tail_start`.
        unsafe { libc_munmap(region_ptr.with_addr(tail_start).cast(), tail_len) };
    }
    #[cfg(feature = "huge-pages")]
    if huge {
        // SAFETY: `base` is the start of a live `size`-byte mapping; a
        // best-effort `MADV_HUGEPAGE` hint touches only kernel metadata.
        unsafe { libc_madvise_hugepage(base.as_ptr(), size) };
    }
    Ok((base, base, size))
}

/// 1-syscall exact-size mmap fast path (see the 0.1 doc). `huge` requests
/// `MAP_HUGETLB`.
///
/// task #713: a genuine `mmap` failure captures its [`VmemError`]
/// IMMEDIATELY, before returning. An alignment miss (the exact address `mmap`
/// handed back doesn't satisfy `align`) is NOT an OS refusal — it maps to
/// [`VmemError::invalid_argument`] instead of a stale/irrelevant error code.
/// Either way, [`unix_reserve`] discards this function's own error and always
/// falls through to the over-reserve path on any failure (unchanged control
/// flow from before this task) — the error value here is not observable by
/// any public caller today, but is still captured correctly for future
/// callers / diagnostics.
#[cfg(all(unix, not(miri)))]
fn try_reserve_aligned_exact(
    size: usize,
    align: usize,
    huge: bool,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    #[cfg(feature = "bench-internals")]
    UNIX_EXACT_RESERVE_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    let region_ptr = unsafe {
        // SAFETY: anonymous private mapping of exactly `size` bytes.
        let p = libc_mmap(size, huge);
        if p.is_null() {
            // Nothing was mapped; no cleanup needed, so capturing here is
            // already the immediate-capture the task requires.
            return Err(VmemError::last_os_error());
        }
        p
    };
    // task #776 (F8): `.addr()` reads the address without exposing
    // provenance, completing the strict-provenance discipline task #717
    // applied to `unix_reserve`'s slow path -- this is the Unix FAST path
    // (tried first by every reservation), so it is the higher-traffic site.
    let region_addr = region_ptr.addr();
    if !region_addr.is_multiple_of(align) {
        // SAFETY: `region_ptr` was just mapped with length `size`; unmap once.
        unsafe { libc_munmap(region_ptr.cast(), size) };
        return Err(VmemError::invalid_argument());
    }
    #[cfg(feature = "bench-internals")]
    UNIX_EXACT_RESERVE_HITS.fetch_add(1, Ordering::Relaxed);
    // SAFETY: non-null and proven `align`-aligned.
    let base = unsafe { NonNull::new_unchecked(region_ptr as *mut u8) };
    #[cfg(feature = "huge-pages")]
    if huge {
        // SAFETY: `base` is a live `size`-byte mapping; hint-only.
        unsafe { libc_madvise_hugepage(base.as_ptr(), size) };
    }
    Ok((base, base, size))
}

#[cfg(all(unix, not(miri)))]
unsafe fn release_reservation(reservation: NonNull<u8>, reservation_len: usize, _align: usize) {
    // SAFETY: on unix `reservation` IS the start of the remaining mapping of
    // length `reservation_len`; `munmap` returns it.
    libc_munmap(reservation.as_ptr(), reservation_len);
}

#[cfg(all(unix, not(miri)))]
// mock (task #646/F8): bypassed by the recording backend, unused when `mock`
// alone is enabled without a real decommit call site reachable.
#[cfg_attr(feature = "mock", allow(dead_code))]
unsafe fn decommit_pages_impl(base: *mut u8, start: usize, end: usize, kind: DecommitKind) {
    let len = end - start;
    // task #719: missing SAFETY comment (the Windows sibling has one for the
    // identical `base.add(start)` offset above its own commit call).
    // SAFETY: caller guarantees `[base+start, +len)` is within a live
    // reservation owned by them, so the offset stays in-bounds.
    let addr = unsafe { base.add(start) };
    match kind {
        // SAFETY: caller guarantees `[base+start, +len)` is within a live
        // mapping; `madvise` touches only kernel page-state.
        DecommitKind::Eager => unsafe { libc_madvise(addr, len, MADV_DONTNEED) },
        DecommitKind::Lazy => unsafe { libc_madvise(addr, len, madv_free_advice()) },
    }
}

#[cfg(all(unix, not(miri)))]
// mock (task #646/F8): see decommit_pages_impl above.
#[cfg_attr(feature = "mock", allow(dead_code))]
unsafe fn recommit_pages_impl(_base: *mut u8, _start: usize, _end: usize) -> Result<(), VmemError> {
    // On unix, re-access after MADV_DONTNEED is implicit — fresh zeroed pages on
    // demand. No syscall, cannot fail.
    Ok(())
}

#[cfg(all(unix, not(miri), feature = "lazy-commit"))]
// mock (task #646/F8): `try_commit_range`'s real-path branch is compiled out
// under `mock`, so this never gets called.
#[cfg_attr(feature = "mock", allow(dead_code))]
unsafe fn commit_range_impl(_base: *mut u8, _start: usize, _end: usize) -> Result<(), VmemError> {
    // Unix: pages are already accessible (eager mmap). Always succeeds.
    Ok(())
}

#[cfg(all(unix, not(miri), feature = "lazy-commit"))]
// mock (task #646/F8): `try_reserve_aligned_lazy`'s real-path branch is
// compiled out under `mock`, so this never gets called.
#[cfg_attr(feature = "mock", allow(dead_code))]
fn reserve_aligned_lazy_raw(
    size: usize,
    align: usize,
    _initial_commit: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    reserve_aligned_raw(size, align)
}

#[cfg(all(unix, not(miri), feature = "huge-pages"))]
fn reserve_aligned_huge_raw(
    size: usize,
    align: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    unix_reserve(size, align, true)
}

/// Select the lazy-decommit `madvise` advice for this platform.
/// Linux: `MADV_FREE`; macOS: `MADV_FREE_REUSABLE`; other Unix: `MADV_DONTNEED`.
#[cfg(all(unix, not(miri)))]
// mock (task #646/F8): only caller is decommit_pages_impl above, itself
// unused under `mock`.
#[cfg_attr(feature = "mock", allow(dead_code))]
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
/// task #714: the default Linux huge page size (`mmap(2)`'s "Huge TLB
/// mappings" section; `/proc/meminfo`'s `Hugepagesize:` on a default
/// configuration). `MAP_HUGETLB` without an explicit size-encoding flag
/// (`MAP_HUGE_2MB`/`MAP_HUGE_1GB` etc., which this crate does not use) always
/// requests the system's DEFAULT huge page size, which is 2 MiB on every
/// mainstream x86_64/aarch64 Linux configuration this crate's own platform
/// support targets. `unix_reserve` requires `size`/`align` to be multiples of
/// this before attempting a huge-page reservation at all, so every `munmap`
/// it can still reach is provably huge-page-aligned (see `unix_reserve`'s own
/// doc for the full reasoning) — REASONED-FROM-SPEC, not empirically
/// verified on a real hugetlb-configured host (none is in this project's CI).
#[cfg(all(unix, not(miri), target_os = "linux", feature = "huge-pages"))]
const LINUX_HUGE_PAGE_SIZE: usize = 2 * 1024 * 1024;
#[cfg(all(unix, not(miri)))]
const MAP_FAILED: usize = usize::MAX;
#[cfg(all(unix, not(miri)))]
// mock (task #646/F8): only consumed by decommit_pages_impl / madv_free_advice
// above, both unused under `mock`.
#[cfg_attr(feature = "mock", allow(dead_code))]
const MADV_DONTNEED: i32 = 4;
/// Linux `MADV_FREE` (lazy reclaim under pressure).
#[cfg(all(unix, not(miri), target_os = "linux"))]
// mock (task #646/F8): see MADV_DONTNEED above.
#[cfg_attr(feature = "mock", allow(dead_code))]
const MADV_FREE: i32 = 8;
/// macOS `MADV_FREE_REUSABLE` (lazy reclaim; page reusable).
#[cfg(all(unix, not(miri), any(target_os = "macos", target_os = "ios")))]
const MADV_FREE_REUSABLE: i32 = 7;
/// Linux `MADV_HUGEPAGE` (transparent-huge-page hint).
#[cfg(all(unix, not(miri), target_os = "linux", feature = "huge-pages"))]
const MADV_HUGEPAGE: i32 = 14;
// task #714 (rust-intel audit MEDIUM §F1): `_SC_PAGESIZE`'s numeric value is
// NOT portable across the `unix` family — it is an index into each OS's own
// `sysconf(3)` name table, not a POSIX-wide constant. The prior version of
// this constant used the Darwin value (29) for macOS/iOS and the Linux value
// (30) for every OTHER `unix` target, INCLUDING all four BSDs this crate's
// own `MAP_ANON` cfg list above already supports — on FreeBSD/DragonFly/
// NetBSD/OpenBSD, `sysconf(30)` queries an unrelated `sysconf` parameter, not
// the page size, and if that unrelated value happened to be a power of two
// it would silently pass `page_size()`'s validation guard and poison the
// decommit-offset rounding callers are told to base on `page_size()`
// (`:227-230`).
//
// REASONED-FROM-SPEC, NOT empirically verified (per this task's own
// instruction: none of the four BSDs run in this project's CI) — values
// cited from each OS's own `sys/unistd.h` `_SC_*` name table:
// - FreeBSD `sys/unistd.h`: `_SC_PAGESIZE` = 47 (`_SC_PAGE_SIZE` is a
//   `#define` alias for the same value). DragonFly BSD forked from FreeBSD
//   and has NOT renumbered this table; DragonFly's own `sys/unistd.h`
//   confirms the same 47.
// - NetBSD `sys/unistd.h`: `_SC_PAGESIZE` = 28 (`_SC_PAGE_SIZE` aliases it).
// - OpenBSD `sys/unistd.h`: `_SC_PAGESIZE` = 28 (same table position as
//   NetBSD; OpenBSD forked from NetBSD's 4.4BSD-derived `unistd.h`).
#[cfg(all(unix, not(miri)))]
const _SC_PAGESIZE: i32 = {
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos"
    ))]
    {
        29
    }
    #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
    {
        47
    }
    #[cfg(any(target_os = "netbsd", target_os = "openbsd"))]
    {
        28
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd",
    )))]
    {
        // Linux and most other unices use 30 for _SC_PAGESIZE / _SC_PAGE_SIZE.
        30
    }
};

// task #719: `offset`'s type hardcodes `i64` for POSIX `off_t`, which is
// NOT a portable assumption in general -- `off_t`'s width is platform- (and
// sometimes build-config-) dependent, e.g. 32-bit on some 32-bit Linux
// targets without large-file-support flags, while BSDs/macOS define it as a
// 64-bit `int64_t` even in 32-bit builds. `i64` is correct for every target
// this crate actually builds/tests on today (this session verified
// x86_64-unknown-{linux-gnu,freebsd,netbsd} plus native Windows/macOS are
// all 64-bit-off_t targets) -- narrowing this further (e.g. per-OS
// `target_pointer_width`-conditional typing) is deferred until this crate
// gains a real 32-bit Unix target in its own supported/tested set, per
// CLAUDE.md's "don't design for hypothetical future requirements". `mmap`
// is ALWAYS called with a literal `0` offset here (anonymous mappings only,
// no file descriptor) -- there is no code path where a wrong-width `off_t`
// could silently truncate a real value; the residual risk is purely an ABI
// shape mismatch on a target this crate does not currently support.
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
    // task #776 (F8): `.addr()` for the same reason as the fast-path fix
    // above -- a comparison-only read, not a round-trip.
    if p.addr() == MAP_FAILED {
        core::ptr::null_mut()
    } else {
        p
    }
}

#[cfg(all(unix, not(miri)))]
unsafe fn libc_munmap(addr: *mut u8, len: usize) {
    // SAFETY: caller guarantees `[addr, addr+len)` was mmap'd and is unmapped once.
    // task #719: the `-1`/`EINVAL` return is deliberately discarded, not
    // merely overlooked — every caller of this function already establishes
    // page/huge-page alignment before calling (task #714 closed the one case
    // where that was NOT true, a silent whole-mapping leak); given that
    // precondition, a genuine `munmap` failure here would indicate a bug in
    // this crate's own alignment bookkeeping, not a recoverable runtime
    // condition the caller could act on (`release`/`decommit`'s public
    // signatures are infallible `()`/`bool`, so there is no channel to
    // surface it through even if we wanted to). The failure mode on error is
    // a leaked mapping, never memory unsafety — the memory stays validly
    // mapped, just not returned to the OS.
    let _ = munmap(addr as *mut core::ffi::c_void, len);
}

#[cfg(all(unix, not(miri)))]
// mock (task #646/F8): only caller is decommit_pages_impl above, itself
// unused under `mock`.
#[cfg_attr(feature = "mock", allow(dead_code))]
unsafe fn libc_madvise(addr: *mut u8, len: usize, advice: i32) {
    // SAFETY: caller guarantees `[addr, addr+len)` is within a live mmap region.
    // task #719: the return value is deliberately discarded -- `madvise`
    // failing here means the OS did not reclaim the pages (the mapping stays
    // exactly as valid and readable/writable as before the call), not a
    // memory-safety concern; [`decommit`]/[`decommit_lazy`]'s own public
    // contracts already document decommit as an OS-cooperative hint whose
    // failure mode is "the physical pages were not actually returned", never
    // a dangling/invalid mapping.
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

/// task #713: a bad `(size, align)` `Layout` combination is a caller contract
/// violation, not an OS refusal — maps to [`VmemError::invalid_argument`]. A
/// genuine `std::alloc::alloc` failure (null return) has no real
/// `errno`/`GetLastError` to read under miri; `VmemError::last_os_error()`
/// correctly yields [`VmemError::os_refusal_unknown_code`] here rather than a
/// misleading `code 0`.
#[cfg(miri)]
fn reserve_aligned_raw(
    size: usize,
    align: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    use std::alloc::Layout;
    let layout = Layout::from_size_align(size, align).map_err(|_| VmemError::invalid_argument())?;
    // SAFETY: `layout` has non-zero size and pow2 align; under miri the consumer
    // is not the global allocator, so no reentrancy.
    let ptr = unsafe { std::alloc::alloc(layout) };
    match NonNull::new(ptr) {
        Some(base) => Ok((base, base, size)),
        None => Err(VmemError::last_os_error()),
    }
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
// mock (task #646/F8): bypassed by the recording backend, unused when `mock`
// alone is enabled without a real decommit call site reachable.
#[cfg_attr(feature = "mock", allow(dead_code))]
unsafe fn decommit_pages_impl(_base: *mut u8, _start: usize, _end: usize, _kind: DecommitKind) {
    // Miri models no RSS; decommit is a no-op.
}

#[cfg(miri)]
// mock (task #646/F8): see decommit_pages_impl above.
#[cfg_attr(feature = "mock", allow(dead_code))]
unsafe fn recommit_pages_impl(_base: *mut u8, _start: usize, _end: usize) -> Result<(), VmemError> {
    Ok(())
}

#[cfg(all(miri, feature = "lazy-commit"))]
// mock (task #646/F8): `try_commit_range`'s real-path branch is compiled out
// under `mock`, so this never gets called.
#[cfg_attr(feature = "mock", allow(dead_code))]
unsafe fn commit_range_impl(_base: *mut u8, _start: usize, _end: usize) -> Result<(), VmemError> {
    Ok(())
}

#[cfg(all(miri, feature = "lazy-commit"))]
// mock (task #646/F8): `try_reserve_aligned_lazy`'s real-path branch is
// compiled out under `mock`, so this never gets called.
#[cfg_attr(feature = "mock", allow(dead_code))]
fn reserve_aligned_lazy_raw(
    size: usize,
    align: usize,
    _initial_commit: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    reserve_aligned_raw(size, align)
}

#[cfg(all(miri, feature = "huge-pages"))]
fn reserve_aligned_huge_raw(
    size: usize,
    align: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
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
// task #719: this was a blanket `#[allow(dead_code)]`, suppressing the lint
// in EVERY feature config (not just under `mock`, where it is genuinely
// unused) -- exactly the crate-wide-suppression hazard task #646/F8 already
// narrowed every other dead-code allow in this file away from (see the
// module doc above). `DecommitKind` was missed from that pass. Narrowed to
// match the established per-item pattern.
#[cfg_attr(feature = "mock", allow(dead_code))]
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
