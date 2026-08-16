//! `aligned-vmem` — cross-platform **aligned anonymous virtual memory**.
//!
//! Reserve a span of `size` bytes whose base is aligned to an arbitrary
//! power-of-two `align`, commit/decommit its pages, and release it — directly
//! through the OS (`mmap`/`munmap`/`madvise` on Unix, `VirtualAlloc`/
//! `VirtualFree` on Windows), with **no file-mapping machinery** and **no
//! dependencies**. Under [miri](https://github.com/rust-lang/miri) it falls
//! back to `std::alloc` so consumers stay miri-testable. A consumer that
//! installs itself as `#[global_allocator]` cannot use this crate under miri,
//! because the miri backend routes allocations through the global allocator and
//! would create a reentrancy hazard (the same class of issue `numa-shim` hit
//! in #777).
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
//! allocator's segments). On 32-bit Unix, first tries an ordinary exact-size
//! `mmap` and checks whether the kernel happened to place it at an
//! `align`-aligned address (fast path; hit rate depends on the OS's placement
//! heuristics, not on any hint this crate passes); on a miss (wrong
//! alignment), over-reserves `size + align` bytes and keeps the full mapping.
//! On 64-bit Unix, the exact-size fast path is compiled out entirely (see the
//! module-level "bench-internals" section below and [`reserve_aligned`]'s own
//! rustdoc), so every reservation always over-reserves `size + align` bytes in
//! one `mmap` call. On Windows, uses one syscall (fast path
//! for `align <= 64 KiB`, over-reserving nothing — base == region) or two
//! syscalls (over-reserving `size + align` and keeping the full mapping). The
//! `Reservation::reservation_ptr` / `reservation_len` fields expose the full
//! reservation; `Reservation::as_ptr` / `len` expose the aligned usable span,
//! plus page-granularity decommit/recommit so you can hint the OS to return
//! physical memory while keeping the address-space reservation (on Linux and
//! Windows this is guaranteed to return physical backing; on the Darwin family
//! — macOS/iOS/tvOS/watchOS — and the BSDs, this reclaim is advisory-only and
//! provides no zero-fill guarantee, see [`decommit`]'s Darwin caveat). If you are building an
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
//! assert_eq!(base.addr() % span, 0); // base is `span`-aligned
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
//! must round to `page_size()`, not `PAGE`, because `madvise(2)` rejects
//! the entire call (all-or-nothing) when `addr` is not a multiple of the
//! real page size.

#![allow(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]
// Under `mock` the real platform syscalls (decommit/recommit/commit_range) are
// bypassed by the recording backend, so their per-OS `*_impl` helpers become
// legitimately unused. This used to be a crate-wide `allow(dead_code)`, which
// made the whole crate structurally unable to report ANY unused item under
// `--all-features` (task #646/F8). Narrowed to per-item
// `#[cfg_attr(aligned_vmem_mock, allow(dead_code))]` on exactly the helpers
// confirmed (by building `RUSTFLAGS="--cfg aligned_vmem_mock" cargo build
// --features lazy-commit,huge-pages,fault-injection` on Windows, Unix
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
// specific combination, on the single item it affects.
//
// Structural alternative considered and deferred for a future major release:
// reorganize the three backends as separate `#[cfg]`-selected private modules
// (`os_windows` / `os_unix` / `os_miri`) with one shared private signature,
// allowing `mock` to be a fourth module selected by the same `#[cfg]` mechanism.
// That would eliminate every `#[cfg_attr(aligned_vmem_mock, allow(dead_code))]`
// attribute, but is a larger refactor than this crate's 0.2.0 release should
// carry. The current partial-replacement shape (mock replaces decommit/recommit/
// commit_range but not reserve/release) is explicitly chosen.

use core::ptr::NonNull;
use core::sync::atomic::{AtomicUsize, Ordering};

pub mod error;
pub use error::VmemError;

#[cfg(aligned_vmem_mock)]
#[cfg_attr(docsrs, doc(cfg(aligned_vmem_mock)))]
pub mod mock;

#[cfg(feature = "fault-injection")]
#[cfg_attr(docsrs, doc(cfg(feature = "fault-injection")))]
pub mod fault_injection;

/// The minimum page size this crate assumes for decommit/recommit granularity:
/// 4 KiB, the smallest unit both `mmap` and `VirtualAlloc` will commit/decommit
/// on the platforms this crate targets.
///
/// Decommit/recommit offsets must be multiples of the runtime [`page_size()`];
/// this constant is only the guaranteed lower bound. `page_size()` may be larger
/// (e.g. 16 KiB on Apple Silicon macOS), so callers computing decommit offsets
/// must round to `page_size()`, not `PAGE`.
///
/// # Naming
///
/// This constant was named `PAGE` in the 0.1.0 release. The name is misleading
/// because it is not the actual page size on all platforms (e.g., 16 KiB on Apple
/// Silicon macOS, 64 KiB on some Linux configurations). For new code, prefer
/// [`MIN_PAGE`] instead, which more accurately describes what this constant
/// represents: the *minimum* granularity the crate assumes, not the platform's
/// page size.
pub const PAGE: usize = 1 << 12;

/// Alias for [`PAGE`] under a name that doesn't imply "the OS page size".
///
/// Prefer this name in new code — it makes explicit that the value is the
/// *minimum* decommit/recommit granularity, not necessarily the actual OS page
/// size (which may be larger — see [`page_size`]).
pub const MIN_PAGE: usize = PAGE;

/// Cache for [`page_size`]. `0` means "not yet queried"; a real page size is
/// always a non-zero power of two so `0` is an unambiguous sentinel.
static PAGE_SIZE_CACHE: AtomicUsize = AtomicUsize::new(0);

// ---------------------------------------------------------------------------
// bench-internals: path-activation counters (task #504, F11 step 1).
// ---------------------------------------------------------------------------
//
// Three independent questions, one instrument family each:
//
// - Unix: on 32-bit, `unix_reserve` tries an EXACT-size `mmap` first
//   (`try_reserve_aligned_exact`) and only falls through to the over-reserve
//   path on a miss (wrong alignment), keeping the full `size + align` mapping.
//   The fast path costs 1 syscall on a hit (mmap) vs 3 on a miss (mmap +
//   munmap + mmap for the over-reserve). Expected cost = `p*1 + (1-p)*3 =
//   3 - 2p` syscalls vs a flat 1 without the fast path. On 64-bit, the fast
//   path is disabled entirely — `unix_reserve` always over-reserves (1 syscall)
//   because the expected cost exceeds 1 for every hit rate p < 1 (the break-even
//   is 100%), and address-space economy (the fast path's only benefit) is not
//   a concern on 64-bit. On 32-bit, the fast path remains enabled for VA economy,
//   despite the same syscall-cost disadvantage, because 32-bit address space is
//   scarce. At real hit rates (34.4%-56.7%, see `reserve_aligned`'s own
//   rustdoc "Cost on Unix fast-path miss" note) this would be 87%-131% MORE
//   syscall traffic on 64-bit if the fast path were kept.
//   `UNIX_EXACT_RESERVE_HITS`/`_ATTEMPTS` settle the real hit rate.
// - Windows: `win_reserve_commit` issues reserve+commit in either
//   one syscall (the fast path for `align <= WIN_ALLOCATION_GRANULARITY` on a full-span commit
//   (`commit_len == size`), or `align <= GetLargePageMinimum()` for large-page requests,
//   over-reserving nothing — base == region)
//   or two syscalls (all other cases — `align > WIN_ALLOCATION_GRANULARITY` for ordinary
//   requests, `align > GetLargePageMinimum()` for large-page requests, or a partial initial
//   commit (`commit_len != size`) — over-reserving `size + align` only when
//   `align > WIN_ALLOCATION_GRANULARITY` or the fast-reserve sub-path's own alignment check
//   misses; Windows cannot partially release a `MEM_RESERVE` region).
//   `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` and `WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS`
//   count each path separately for parity/comparison against the Unix
//   hit-rate story.
// - macOS decommit oracle (round-6, task #882): `libc_madvise` discards
//   `madvise`'s return value by design (task #719), so nothing distinguished
//   "the syscall succeeded but Darwin's semantics didn't reclaim the pages"
//   from "the syscall itself failed" for item 48's root-cause question.
//   `UNIX_MADVISE_ATTEMPTS`/`UNIX_MADVISE_SUCCESSES` settle it with a real
//   number — see those statics' own docs.
// - Windows decommit failure path (round-3): `VirtualFree(MEM_DECOMMIT)`
//   failure/failure tracking distinguishes "the syscall was attempted but
//   failed" from "it was never attempted at all". `WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS`/`_FAILURES`
//   settle this, and `UNIX_MUNMAP_FAILURES` provides the Unix counterpart.
//
// `AtomicU64` storage, increments gated on `bench-internals` so a plain build
// carries zero extra instructions (storage itself is also gated, not compiled
// without the feature). Relaxed — diagnostic only, no ordering obligation.

#[cfg(feature = "bench-internals")]
use core::sync::atomic::AtomicU64;

/// `bench-internals`: total number of exact-size `mmap` attempts across this
/// crate's two distinct exact-size fast paths, sharing one counter for unified
/// hit-rate tracking:
/// - `try_reserve_aligned_exact` (32-bit Unix only — always 0 on Windows/miri
///   AND on 64-bit Unix outside the huge-page case below, since task #944's
///   finding P-1 gated that internal helper to `target_pointer_width = "32"`;
///   it is private and platform-gated, so it is named here in code font
///   rather than linked).
/// - The Linux/Android huge-page exact-size path (II-4, 2026-08-16 audit
///   finding), which uses an exact-size `mmap` with `MAP_HUGETLB` when
///   `align == LINUX_HUGE_PAGE_SIZE` (2 MiB) — this one DOES fire on 64-bit
///   Linux/Android, since the huge-page path is not gated to 32-bit. It lives
///   in the exact-size huge-path block in `unix_reserve`, a different code
///   path from `try_reserve_aligned_exact`, but is counted here rather than
///   under a separate static.
///
/// This counter increments BEFORE the `mmap` call, so it includes both alignment
/// misses and OS-level failures (e.g., OOM, MAP_HUGETLB refused). The hit-rate
/// ratio `UNIX_EXACT_RESERVE_HITS / UNIX_EXACT_RESERVE_ATTEMPTS` therefore
/// measures the combined success rate of both alignment and OS availability.
///
/// Note that the denominator conflates two different failure kinds with
/// different syscall costs: an alignment miss costs 3 syscalls (mmap + munmap +
/// mmap for the over-reserve), while an OS refusal costs only 1 (the initial
/// mmap fails, no munmap runs afterward). Denominator for
/// [`UNIX_EXACT_RESERVE_HITS`]. See the module-level "bench-internals"
/// section doc above.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static UNIX_EXACT_RESERVE_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of exact-size `mmap` attempts (counted in
/// [`UNIX_EXACT_RESERVE_ATTEMPTS`]) that succeeded -- the `mmap` landed
/// already `align`-aligned, so no over-reserve fallback was needed. Covers
/// both source paths that static's doc describes: `try_reserve_aligned_exact`
/// (32-bit Unix only) and the Linux/Android huge-page exact-size path (II-4,
/// 2026-08-16 audit finding, `align == LINUX_HUGE_PAGE_SIZE`, fires on
/// 64-bit Linux/Android too).
///
/// Numerator over [`UNIX_EXACT_RESERVE_ATTEMPTS`]. See the module-level
/// "bench-internals" section doc above.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static UNIX_EXACT_RESERVE_HITS: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of successful `win_reserve_commit` calls that took the
/// single-call fast path (Windows only — always 0 on Unix/miri; that internal helper
/// is private and platform-gated, so it is named here in code font rather than linked).
/// Each call issues 1 syscall (`VirtualAlloc(MEM_RESERVE | MEM_COMMIT)`),
/// which the fast path uses when `align <= WIN_ALLOCATION_GRANULARITY` and `commit_len == size`.
/// When a large-page request fails, a best-effort retry with ordinary pages
/// issues a second syscall but is still counted as 1 in this counter — see the
/// retry fallback code in `win_reserve_commit`. See the module-level
/// "bench-internals" section doc above.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static WINDOWS_RESERVE_COMMIT_SINGLE_CALLS: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of successful `win_reserve_commit` calls that took the
/// two-call path (Windows only — always 0 on Unix/miri; that internal helper
/// is private and platform-gated, so it is named here in code font rather than linked).
/// Each call issues exactly 2 syscalls
/// (`VirtualAlloc(MEM_RESERVE)` + `VirtualAlloc(MEM_COMMIT)`, plus a possible
/// third best-effort retry on a `huge-pages` commit failure — not counted
/// here, see that call site). This path is used when `align > WIN_ALLOCATION_GRANULARITY` or when
/// `commit_len != size` (the lazy-commit case). See the module-level
/// "bench-internals" section doc above.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: total number of `madvise(2)` calls issued by
/// `libc_madvise` (Unix only — always 0 on Windows/miri; that internal
/// helper is private, so it is named here in code font rather than linked).
/// Covers every `madvise` call site reachable through `decommit_pages_impl`
/// (both `DecommitKind::Eager` — `MADV_DONTNEED` — and
/// `DecommitKind::Lazy` — `MADV_FREE`/`MADV_FREE_REUSABLE`/`MADV_DONTNEED`
/// fallback). Added by task #882 as the empirical oracle for
/// <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>
/// item 48: `libc_madvise` itself discards
/// `madvise`'s return value by design (task #719 — a failure there is not a
/// memory-safety concern), so nothing else in the crate can currently tell
/// apart "the syscall itself failed" from "the syscall succeeded but Darwin's
/// advisory-only `MADV_DONTNEED` semantics did not reclaim the pages" — the
/// two competing root-cause hypotheses for item 48's macOS zero-fill gap.
/// Denominator for [`UNIX_MADVISE_SUCCESSES`].
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static UNIX_MADVISE_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of [`UNIX_MADVISE_ATTEMPTS`] that returned `0`
/// (success) from `madvise(2)`, as opposed to `-1` (failure, `errno` set).
/// See [`UNIX_MADVISE_ATTEMPTS`]'s doc for the root-cause question this
/// settles. Numerator over [`UNIX_MADVISE_ATTEMPTS`].
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static UNIX_MADVISE_SUCCESSES: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of `munmap` calls that failed (Unix only —
/// always 0 on Windows/miri). `munmap` failures indicate a backend
/// bookkeeping problem that can turn into a silent leak/un-freed RSS
/// with zero visibility. The crate's public API is infallible (by design),
/// so failures are currently silently ignored; this counter provides at
/// least diagnostic visibility into the failure rate. Added to address
/// the finding documented in `docs/reviews/2026-08-16-aligned-vmem-fxx-prerelease-audit.md`
/// item P2-6.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static UNIX_MUNMAP_FAILURES: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of `VirtualFree(..., MEM_DECOMMIT)` calls that
/// failed (Windows only — always 0 on Unix/miri). `VirtualFree(MEM_DECOMMIT)`
/// failures indicate a backend bookkeeping problem that can turn into a
/// silent leak/un-freed RSS with zero visibility. The crate's public API is
/// infallible (by design), so failures are currently silently ignored; this
/// counter provides at least diagnostic visibility into the failure rate.
/// Added to address the finding documented in
/// `docs/reviews/2026-08-16-aligned-vmem-fxx-prerelease-audit.md` item P2-6.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of `VirtualFree(..., MEM_RELEASE)` calls that
/// failed (Windows only — always 0 on Unix/miri). `VirtualFree(MEM_RELEASE)`
/// failures indicate a backend bookkeeping problem that can turn into a
/// silent leak/un-freed RSS with zero visibility. For correctly created internal
/// reservations, a release failure usually indicates a bookkeeping defect. For
/// `unsafe from_raw_parts` and external allocator handoff, a failure turns into a
/// silent leak at Drop. The crate's public API is infallible (by design), so
/// failures are currently silently ignored; this counter provides at least
/// diagnostic visibility into the failure rate. Added to address the finding
/// documented in `docs/reviews/2026-08-16-aligned-vmem-independent-prerelease-audit-r4.md`
/// item R4-7.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static WINDOWS_VIRTUALFREE_RELEASE_FAILURES: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of `VirtualFree(MEM_DECOMMIT)` attempts made.
/// Denominator for [`WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES`]. Mirrors the Unix
/// `UNIX_MADVISE_ATTEMPTS`/`UNIX_MADVISE_SUCCESSES` pair pattern — lets tests
/// distinguish "genuinely succeeded" from "never attempted".
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: relaxed snapshot of the internal
/// `UNIX_EXACT_RESERVE_ATTEMPTS` counter (private storage; this accessor is
/// the public read surface). Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn unix_exact_reserve_attempts() -> u64 {
    UNIX_EXACT_RESERVE_ATTEMPTS.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of the internal
/// `UNIX_EXACT_RESERVE_HITS` counter (private storage; this accessor is the
/// public read surface). Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn unix_exact_reserve_hits() -> u64 {
    UNIX_EXACT_RESERVE_HITS.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of the sum of the internal
/// `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` and
/// `WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS` counters (both paths combined;
/// private storage, this accessor is the public read surface).
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn windows_reserve_commit_calls() -> u64 {
    WINDOWS_RESERVE_COMMIT_SINGLE_CALLS.load(Ordering::Relaxed)
        + WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of the internal
/// `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` counter (single-call path count;
/// private storage, this accessor is the public read surface).
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn windows_reserve_commit_single_calls() -> u64 {
    WINDOWS_RESERVE_COMMIT_SINGLE_CALLS.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of the internal
/// `WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS` counter (two-call path count;
/// private storage, this accessor is the public read surface).
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn windows_reserve_commit_two_call_pairs() -> u64 {
    WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of the internal `UNIX_MADVISE_ATTEMPTS`
/// counter (private storage; this accessor is the public read surface).
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn unix_madvise_attempts() -> u64 {
    UNIX_MADVISE_ATTEMPTS.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of the internal `UNIX_MADVISE_SUCCESSES`
/// counter (private storage; this accessor is the public read surface).
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn unix_madvise_successes() -> u64 {
    UNIX_MADVISE_SUCCESSES.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of `WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS`.
/// Denominator for `windows_virtualfree_decommit_failures()`. Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn windows_virtualfree_decommit_attempts() -> u64 {
    WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of `UNIX_MUNMAP_FAILURES`.
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn unix_munmap_failures() -> u64 {
    UNIX_MUNMAP_FAILURES.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of `WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES`.
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn windows_virtualfree_decommit_failures() -> u64 {
    WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of `WINDOWS_VIRTUALFREE_RELEASE_FAILURES`.
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn windows_virtualfree_release_failures() -> u64 {
    WINDOWS_VIRTUALFREE_RELEASE_FAILURES.load(Ordering::Relaxed)
}

/// `bench-internals`: reset all ten counters (`UNIX_EXACT_RESERVE_ATTEMPTS`,
/// `UNIX_EXACT_RESERVE_HITS`, `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS`,
/// `WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS`, `UNIX_MADVISE_ATTEMPTS`,
/// `UNIX_MADVISE_SUCCESSES` -- all private storage, read via their
/// respective accessor functions above -- plus `UNIX_MUNMAP_FAILURES`,
/// `WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS`,
/// `WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES`, and
/// `WINDOWS_VIRTUALFREE_RELEASE_FAILURES`) to 0. Test/bench hook only —
/// lets a measurement window start from a clean count instead of accumulating
/// across the whole process lifetime, mirroring sefer-alloc's established
/// `dbg_reset_*` convention.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
pub fn reset_bench_internals_counters() {
    UNIX_EXACT_RESERVE_ATTEMPTS.store(0, Ordering::Relaxed);
    UNIX_EXACT_RESERVE_HITS.store(0, Ordering::Relaxed);
    WINDOWS_RESERVE_COMMIT_SINGLE_CALLS.store(0, Ordering::Relaxed);
    WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS.store(0, Ordering::Relaxed);
    UNIX_MADVISE_ATTEMPTS.store(0, Ordering::Relaxed);
    UNIX_MADVISE_SUCCESSES.store(0, Ordering::Relaxed);
    UNIX_MUNMAP_FAILURES.store(0, Ordering::Relaxed);
    WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS.store(0, Ordering::Relaxed);
    WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES.store(0, Ordering::Relaxed);
    WINDOWS_VIRTUALFREE_RELEASE_FAILURES.store(0, Ordering::Relaxed);
}

/// Validate a queried OS page size, falling back to PAGE if the value is invalid.
///
/// This function is pure and has no OS dependencies, making it directly testable.
/// It guards against:
/// - A queried value of 0
/// - A non-power-of-two value
/// - A value smaller than PAGE (4 KiB), which indicates `query_os_page_size()`
///   read the wrong sysconf(3) parameter entirely (e.g., a wrong `_SC_PAGESIZE`
///   constant on an untested target).
///
/// The OS page size is never smaller than PAGE on any target this crate supports,
/// so a queried value below it indicates a broken query. A hostile/broken value
/// would otherwise corrupt every rounding computation downstream, so we fall back
/// to the safe default PAGE.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[inline]
#[must_use]
pub fn validate_page_size(queried: usize) -> usize {
    validate_page_size_impl(queried)
}

/// Internal implementation of page size validation.
#[inline]
#[must_use]
fn validate_page_size_impl(queried: usize) -> usize {
    if queried >= PAGE && queried.is_power_of_two() {
        queried
    } else {
        PAGE
    }
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
/// multiples gets a crate-level silent skip (the call returns without any
/// effect), because `decommit`/`decommit_lazy` validate against `page_size()`
/// before reaching the OS. Even at the OS level, madvise(2) rejects the entire
/// call (all-or-nothing) when `addr` is not a multiple of the real page size.
/// Use this value (not [`PAGE`]) to round decommit offsets.
#[must_use]
#[inline]
pub fn page_size() -> usize {
    let cached = PAGE_SIZE_CACHE.load(Ordering::Relaxed);
    if cached != 0 {
        return cached;
    }
    let queried = query_os_page_size();
    let value = validate_page_size_impl(queried);
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
    debug_assert!(
        info.dw_allocation_granularity as usize >= WIN_ALLOCATION_GRANULARITY,
        "OS-reported allocation granularity ({}) is smaller than the hardcoded constant ({}); \
         this would break the single-call fast path's alignment guarantee",
        info.dw_allocation_granularity,
        WIN_ALLOCATION_GRANULARITY
    );
    // NOTE: This debug_assert fires only when `query_os_page_size()` is called,
    // which happens on the cold path (decommit/decommit_lazy) — since task #897
    // removed the `align > page_size() &&` conjunct, the reserve fast path no
    // longer consults `page_size()` at all. It does NOT fire on the Windows
    // single-call reservation fast path, which uses `WIN_ALLOCATION_GRANULARITY`
    // directly.
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
/// valid for `len()` bytes for the lifetime of this handle **except ranges the
/// caller has decommitted (via the free functions or the safe methods) and
/// not yet recommitted — on Windows such pages are unmapped until `recommit`**.
/// The span is **not** initialised. Dropping the handle returns the whole
/// underlying OS reservation to the OS exactly once.
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
    /// Whether OS large/huge pages were actually granted for this reservation.
    /// True if `reserve_aligned_huge` succeeded in obtaining large pages on
    /// Linux (`MAP_HUGETLB`) or Windows (`MEM_LARGE_PAGES` when the OS grants
    /// the request). False if the request fell back to ordinary pages.
    ///
    /// This flag is the "best-effort" observable: a caller can detect whether
    /// the huge-page feature actually engaged, rather than receiving only an
    /// indistinguishable `Ok(Reservation)` on every fallback path.
    ///
    /// **Windows limitation (task #848 single-call fast path):** on Windows,
    /// this flag is `true` only when ALL of the following hold:
    /// 1. The fast-path condition `align <= GetLargePageMinimum()` is satisfied
    ///    (typically `align <= 2 MiB` on x86_64)
    /// 2. `size` is a multiple of the system's large-page minimum
    /// 3. The calling process has `SeLockMemoryPrivilege` granted AND has
    ///    **enabled** it via `AdjustTokenPrivileges` (the crate does not do
    ///    this for you — a process with the privilege granted but not
    ///    enabled fails exactly like an unprivileged one and silently falls
    ///    back to ordinary pages)
    ///
    /// NOTE: The widened fast-path condition (II-3, 2026-08-16 audit finding) expanded
    /// the single-call ATTEMPT window from `align <= 64 KiB` to `align <= GetLargePageMinimum()`,
    /// but on an unprivileged host the actual paths that SUCCEED (pass the post-call alignment
    /// check) are typically still limited. When large pages are NOT granted (unprivileged),
    /// `VirtualAlloc`'s alignment guarantee is only 64 KiB; in practice it typically does NOT
    /// happen to land on the requested alignment, so the post-call check fails and the fast
    /// path falls through to the two-call path. Practically, this means `is_huge == true` only
    /// for shapes where large pages are actually granted, which requires all three conditions
    /// above to hold.
    ///
    /// If any of these conditions fail, the function falls back to ordinary
    /// pages and this flag is `false`. On Windows, large pages (`MEM_LARGE_PAGES`)
    /// are only ever requested and possibly granted via the single-call fast path;
    /// the two-call path never requests large pages, so
    /// `granted_huge` is always `false` for a reservation that takes it.
    granted_huge: bool,
}

impl core::fmt::Debug for Reservation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Reservation")
            .field("base", &self.base.as_ptr())
            .field("len", &self.len)
            .field("reservation", &self.reservation.as_ptr())
            .field("reservation_len", &self.reservation_len)
            .field("align", &self.align)
            .field("granted_huge", &self.granted_huge)
            .finish()
    }
}

impl Reservation {
    /// The aligned usable base of this span. Non-null, valid for [`len`](Self::len)
    /// bytes **except ranges that have been decommitted and not yet recommitted —
    /// on Windows such pages are unmapped until `recommit`**, aligned to the `align`
    /// requested at reservation time.
    ///
    /// Returns `*mut u8` (rather than the std convention of `*const T` from
    /// `&self`) because a raw pointer carries no borrow obligation in this
    /// crate's model, and the span is exclusively owned by this `Reservation`
    /// handle. The mutability reflects ownership, not mutability of the
    /// borrow itself.
    #[must_use]
    #[inline]
    pub fn as_ptr(&self) -> *mut u8 {
        self.base.as_ptr()
    }

    /// The number of usable bytes at [`as_ptr`](Self::as_ptr).
    #[must_use]
    #[inline]
    #[allow(clippy::len_without_is_empty)]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// The start of the underlying OS reservation (may sit below
    /// [`as_ptr`](Self::as_ptr) because the reservation is over-reserved
    /// to achieve alignment and the full mapping is kept).
    #[must_use]
    #[inline]
    pub fn reservation_ptr(&self) -> *mut u8 {
        self.reservation.as_ptr()
    }

    /// The **requested/logical** span length of this reservation.
    ///
    /// **This value is NOT necessarily the actual OS reservation size** — at least
    /// three paths under-report the true VA span the OS mapped:
    ///
    /// - **Windows single-call fast path** (`align <= 64 KiB`): this returns
    ///   `commit_len` (which equals `size`), not the rounded-up VA reservation
    ///   size. Windows rounds VA reservations up to the 64 KiB allocation
    ///   granularity internally, so `reserve_aligned(4096, 4096)` reports
    ///   `reservation_len() == 4096` while actually consuming 64 KiB of address
    ///   space.
    /// - **Windows two-call path's fast-reserve sub-path** (`align <= 64 KiB`
    ///   via `reserve_aligned_lazy`): when the candidate `VirtualAlloc(NULL,
    ///   size, MEM_RESERVE)` happens to be aligned, this returns `size` directly,
    ///   not the rounded-up 64 KiB granularity. The underlying reservation still
    ///   consumes a 64 KiB-granular region.
    /// - **Any page-rounding `mmap` where the OS page size exceeds the requested
    ///   granularity** — e.g. Apple-Silicon macOS's 16 KiB pages, or 64 KiB on
    ///   some Linux configurations (see [`MIN_PAGE`]'s doc above): `mmap` rounds
    ///   `length` up to the page size, so `reserve_aligned(PAGE, PAGE)` on a 16
    ///   KiB-page host actually maps a full 16 KiB page while this returns
    ///   `4096`.
    ///
    /// Both cases are harmless for correctness (`VirtualFree(base, 0,
    /// MEM_RELEASE)` ignores the length argument; `munmap` rounds its length
    /// argument up to the page size the same way `mmap` did, so `release`
    /// still unmaps the whole underlying mapping) — but the return value is
    /// not a portable measure of the true reservation size.
    #[must_use]
    #[inline]
    pub const fn reservation_len(&self) -> usize {
        self.reservation_len
    }

    // Historical note (task #848, #921): the Windows single-call fast path
    // (align <= WIN_ALLOCATION_GRANULARITY, typically 64 KiB; widens to
    // GetLargePageMinimum() when requesting large pages) and the two-call
    // path's fast-reserve sub-path (align <= WIN_ALLOCATION_GRANULARITY
    // via reserve_aligned_lazy) are the primary under-report cases for
    // this method; the page-rounding mmap case is the third. These are
    // documented in the method's rustdoc above without task-number references.

    /// The alignment requested at reservation time.
    #[must_use]
    #[inline]
    pub const fn align(&self) -> usize {
        self.align
    }

    /// Whether OS large/huge pages were actually granted for this reservation.
    ///
    /// Returns `true` if the reservation successfully obtained large/huge pages
    /// from the OS (Linux `MAP_HUGETLB` or Windows `MEM_LARGE_PAGES`), and `false`
    /// if it fell back to ordinary pages or was not a huge-page request.
    ///
    /// This is the "best-effort" observable: a caller using `reserve_aligned_huge`
    /// can now detect whether the huge-page feature actually engaged, rather than
    /// receiving only an indistinguishable `Ok(Reservation)` on every fallback.
    ///
    /// **Windows limitation (task #848 single-call fast path):** on Windows,
    /// this returns `true` only when ALL of the following hold:
    /// 1. The fast-path condition `align <= GetLargePageMinimum()` is satisfied
    ///    (typically `align <= 2 MiB` on x86_64)
    /// 2. `size` is a multiple of the system's large-page minimum
    /// 3. The calling process has `SeLockMemoryPrivilege` granted AND has
    ///    **enabled** it via `AdjustTokenPrivileges` (the crate does not do
    ///    this for you — a process with the privilege granted but not
    ///    enabled fails exactly like an unprivileged one and silently falls
    ///    back to ordinary pages)
    ///
    /// NOTE: The widened fast-path condition (II-3, 2026-08-16 audit finding) expanded
    /// the single-call ATTEMPT window from `align <= 64 KiB` to `align <= GetLargePageMinimum()`,
    /// but on an unprivileged host the actual paths that SUCCEED (pass the post-call alignment
    /// check) are typically still limited. When large pages are NOT granted (unprivileged),
    /// `VirtualAlloc`'s alignment guarantee is only 64 KiB; in practice it typically does NOT
    /// happen to land on the requested alignment, so the post-call check fails and the fast
    /// path falls through to the two-call path. Practically, this means `is_huge() == true` only
    /// for shapes where large pages are actually granted, which requires all three conditions
    /// above to hold.
    ///
    /// If any of these conditions fail, the function falls back to ordinary pages
    /// and this flag is `false`. On Windows, large pages (`MEM_LARGE_PAGES`)
    /// are only ever requested and possibly granted via the single-call fast path;
    /// the two-call path never requests large pages, so
    /// `is_huge()` is always `false` for a reservation that takes it. See
    /// [`reserve_aligned_huge`]'s rustdoc for details.
    ///
    /// **Note:** reservations adopted via [`from_raw_parts`](Self::from_raw_parts)
    /// report whatever `granted_huge` value the caller passed to that constructor,
    /// which the caller is responsible for getting right (see that constructor's
    /// `# Safety` section).
    #[must_use]
    #[inline]
    pub const fn is_huge(&self) -> bool {
        self.granted_huge
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
    ///
    /// **Warning:** This method returns a raw tuple. Consider using
    /// [`into_reservation_parts`](Self::into_reservation_parts) instead, which
    /// returns a named struct that prevents accidentally swapping `len` and `align`.
    #[must_use]
    pub fn into_parts(self) -> (*mut u8, usize, usize) {
        let parts = (self.reservation.as_ptr(), self.reservation_len, self.align);
        core::mem::forget(self);
        parts
    }

    /// Consume the handle WITHOUT releasing the OS reservation, returning the
    /// [`ReservationParts`] struct the caller must later hand to [`release_parts`]
    /// exactly once. Use this when your allocator records the reservation in its
    /// own self-hosted metadata instead of relying on `Drop`.
    ///
    /// This method is the typed, named alternative to [`into_parts`](Self::into_parts);
    /// it prevents the footgun of accidentally swapping `len` and `align`, which
    /// would be undefined behavior on the native backend and cause leaks or crashes
    /// on the Unix backend.
    ///
    /// **WARNING:** This method discards `base`, `len`, and `granted_huge`. To
    /// reconstruct a full `Reservation` via [`from_raw_parts`](Self::from_raw_parts),
    /// you MUST preserve these three fields separately alongside the returned
    /// `ReservationParts`. If you omit `granted_huge`, the reconstructed reservation
    /// will incorrectly report `is_huge() == false` even if the original used huge
    /// pages, which can lead to incorrect decommit-availability decisions.
    ///
    /// For backwards compatibility with code that already uses the tuple form,
    /// you can call [`ReservationParts::as_tuple`] to get a raw tuple.
    #[must_use]
    pub fn into_reservation_parts(self) -> ReservationParts {
        let parts = ReservationParts {
            ptr: self.reservation.as_ptr(),
            len: self.reservation_len,
            align: self.align,
        };
        // Same suppression as `into_parts` -- without this, `self` would run
        // its normal `Drop` (which now also releases the OS reservation) at
        // the end of this function, and the returned `ReservationParts`
        // would describe already-freed memory: a guaranteed double-free the
        // moment the caller follows this method's own contract and passes
        // it to `release_parts`.
        core::mem::forget(self);
        parts
    }

    /// Consume the handle WITHOUT releasing the OS reservation, returning a
    /// full [`ReservationFullParts`] struct containing all six fields needed to
    /// reconstruct the original `Reservation` via [`from_raw_parts`](Self::from_raw_parts).
    ///
    /// This is the lossless round-trip alternative to [`into_reservation_parts`](Self::into_reservation_parts):
    /// it preserves `base`, `len`, and `granted_huge` in addition to the underlying
    /// reservation metadata, eliminating the risk of silent huge-page status loss
    /// or usable-span information loss.
    ///
    /// Use this when you need to temporarily extract all reservation state for
    /// later reconstruction, such as in a custom allocator that persists metadata
    /// across restarts or hands off reservations between components.
    #[must_use]
    pub fn into_full_parts(self) -> ReservationFullParts {
        let parts = ReservationFullParts {
            base: self.base.as_ptr(),
            len: self.len,
            reservation: self.reservation.as_ptr(),
            reservation_len: self.reservation_len,
            align: self.align,
            granted_huge: self.granted_huge,
        };
        core::mem::forget(self);
        parts
    }

    /// Decommit pages `[start, end)` within this reservation.
    ///
    /// This is the safe, bounds-checked alternative to the free [`decommit`]
    /// function for callers already holding a `Reservation`. It delegates to
    /// the underlying implementation with `self.as_ptr()` as base and
    /// automatically ensures `[start, end)` is within the reservation's usable span.
    ///
    /// Hint the OS to return the physical backing of `[start, end)` while keeping the
    /// address-space reservation alive. On Linux and Windows this is guaranteed to
    /// return physical backing and zero-fill on next access (Linux `MADV_DONTNEED`,
    /// Windows `MEM_DECOMMIT`). On the Darwin family (macOS/iOS/tvOS/watchOS) and the
    /// four BSDs (FreeBSD/DragonFly/NetBSD/OpenBSD), this is a best-effort hint with no
    /// zero-fill or reclaim guarantee — the physical pages may remain resident and
    /// old data may be observed after a decommit+recommit roundtrip.
    ///
    /// `start` and `end` must be multiples of the runtime page size ([`page_size()`]).
    /// A no-op if the range is empty, is out of bounds (`end > self.len()`), or
    /// if the offsets violate the page-size multiple contract (the same
    /// silent-skip behavior as the free [`decommit`] function).
    ///
    /// See [`decommit`] for platform divergence notes (Windows crashes on write
    /// before recommit, Linux does not), huge-page incompatibility, and Darwin
    /// zero-fill caveats.
    pub fn decommit(&self, start: usize, end: usize) {
        // Bounds check: the range must be within the reservation's usable span.
        if end > self.len() {
            return;
        }
        // SAFETY: `self.as_ptr()` is a valid reservation base, and we've just
        // verified `[start, end)` is within `self.len()`. The free function's
        // own contract (multiples of page_size(), etc.) is validated inside it.
        unsafe { decommit(self.as_ptr(), start, end) };
    }

    /// Lazy decommit variant: hint the OS it MAY reclaim `[start, end)` under memory
    /// pressure, cheaper than [`Self::decommit`] (Linux `MADV_FREE`, macOS/iOS
    /// `MADV_FREE_REUSABLE`, FreeBSD/DragonFly `MADV_FREE`, NetBSD/OpenBSD
    /// `MADV_FREE`, other Unix (including tvOS/watchOS) falls back to `MADV_DONTNEED`;
    /// Windows falls back to the eager [`Self::decommit`] path, which has no lazy equivalent).
    ///
    /// This is the safe, bounds-checked alternative to the free [`decommit_lazy`]
    /// function for callers already holding a `Reservation`. It delegates to the
    /// underlying implementation with `self.as_ptr()` as base and automatically
    /// ensures `[start, end)` is within the reservation's usable span.
    ///
    /// See [`decommit_lazy`] for the platform-specific cost inversion on macOS/iOS
    /// (this variant actually drops RSS immediately there, unlike the eager path)
    /// and other caveats.
    pub fn decommit_lazy(&self, start: usize, end: usize) {
        // Bounds check: the range must be within the reservation's usable span.
        if end > self.len() {
            return;
        }
        // SAFETY: same safety argument as `decommit` above.
        unsafe { decommit_lazy(self.as_ptr(), start, end) };
    }

    /// Recommit pages `[start, end)` previously passed to [`Self::decommit`].
    ///
    /// This is the safe, bounds-checked alternative to the free [`recommit`]
    /// function for callers already holding a `Reservation`. It delegates to
    /// the underlying implementation with `self.as_ptr()` as base and automatically
    /// ensures `[start, end)` is within the reservation's usable span.
    ///
    /// Returns `true` if the range is now committed (or the call was a well-formed
    /// no-op — empty range, `start == end`), and `false` if the OS refused to
    /// commit the pages (commit-charge exhaustion / true OOM) OR the offsets
    /// violated the contract below. On `false` the caller MUST NOT write into
    /// `[start, end)`. Never panics. For the cause use [`Self::try_recommit`].
    ///
    /// `start` and `end` must be multiples of the runtime page size ([`page_size()`]).
    /// A well-formed no-op (empty range, `start == end`) returns `true`; any
    /// other contract violation (misaligned, or `start > end`, or `end > self.len()`)
    /// returns `false`.
    #[must_use]
    pub fn recommit(&self, start: usize, end: usize) -> bool {
        // Bounds check: the range must be within the reservation's usable span.
        if end > self.len() {
            return false;
        }
        // SAFETY: `self.as_ptr()` is a valid reservation base, and we've just
        // verified `[start, end)` is within `self.len()`. The free function's
        // own contract (multiples of page_size(), etc.) is validated inside it.
        unsafe { recommit(self.as_ptr(), start, end) }
    }

    /// Fallible [`Self::recommit`]: `Ok(())` if the range is now committed
    /// (or was a well-formed no-op), `Err(VmemError::invalid_argument())` if the
    /// offsets violated the contract (misaligned, or `start > end`, or `end > self.len()`),
    /// `Err(VmemError)` carrying the OS cause on genuine commit failure.
    ///
    /// This is the safe, bounds-checked alternative to the free [`try_recommit`]
    /// function for callers already holding a `Reservation`.
    pub fn try_recommit(&self, start: usize, end: usize) -> Result<(), VmemError> {
        // Bounds check: the range must be within the reservation's usable span.
        if end > self.len() {
            return Err(VmemError::invalid_argument());
        }
        // SAFETY: same safety argument as `recommit` above.
        unsafe { try_recommit(self.as_ptr(), start, end) }
    }

    /// Commit pages `[start, end)` within this reservation.
    ///
    /// This is the safe, bounds-checked alternative to the free [`commit_range`]
    /// function for callers already holding a `Reservation`. It delegates to
    /// the underlying implementation with `self.as_ptr()` as base and automatically
    /// ensures `[start, end)` is within the reservation's usable span.
    ///
    /// After a [`reserve_aligned_lazy`] call that left some pages reserved-but-uncommitted,
    /// `commit_range` commits exactly the requested sub-range so it becomes writable.
    ///
    /// Returns `true` if the range is now committed, `false` if the OS refused
    /// (commit-charge exhaustion / true OOM) OR the offsets violated the contract
    /// above. On `false` the caller MUST NOT write into the range. Never panics.
    /// For the cause use [`Self::try_commit_range`].
    ///
    /// `start` and `end` must be multiples of the runtime page size ([`page_size()`]).
    /// A well-formed no-op (empty range, `start == end`) returns `true`; any
    /// other contract violation (misaligned, or `start > end`, or `end > self.len()`)
    /// returns `false`.
    #[must_use]
    #[cfg(feature = "lazy-commit")]
    #[cfg_attr(docsrs, doc(cfg(feature = "lazy-commit")))]
    pub fn commit_range(&self, start: usize, end: usize) -> bool {
        // Bounds check: the range must be within the reservation's usable span.
        if end > self.len() {
            return false;
        }
        // SAFETY: same safety argument as `recommit` above.
        unsafe { commit_range(self.as_ptr(), start, end) }
    }

    /// Fallible [`Self::commit_range`]: `Ok(())` on success (or was a well-formed no-op),
    /// `Err(VmemError::invalid_argument())` if the offsets violated the contract
    /// (misaligned, or `start > end`, or `end > self.len()`), `Err(VmemError)` carrying
    /// the OS cause on genuine commit failure.
    ///
    /// This is the safe, bounds-checked alternative to the free [`try_commit_range`]
    /// function for callers already holding a `Reservation`.
    #[cfg(feature = "lazy-commit")]
    #[cfg_attr(docsrs, doc(cfg(feature = "lazy-commit")))]
    pub fn try_commit_range(&self, start: usize, end: usize) -> Result<(), VmemError> {
        // Bounds check: the range must be within the reservation's usable span.
        if end > self.len() {
            return Err(VmemError::invalid_argument());
        }
        // SAFETY: same safety argument as `recommit` above.
        unsafe { try_commit_range(self.as_ptr(), start, end) }
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
    /// This is **not** the inverse of [`into_parts`](Self::into_parts): that
    /// method returns only 3 of the 6 fields this constructor requires
    /// (`reservation_ptr, reservation_len, align`), discarding `base`, `len`,
    /// and `granted_huge` entirely. [`into_parts`](Self::into_parts)'s true structural complement
    /// is [`release`], whose signature is exactly the 3-tuple `into_parts`
    /// returns — that is the intended matched pair for "take ownership out of
    /// RAII, then give it back to the OS manually". `from_raw_parts` is a
    /// separate, more general constructor for the cross-crate handoff pattern:
    /// a sibling crate (`numa-shim` on Windows) issues a platform-specific
    /// reservation call that `aligned-vmem` itself does not wrap, then adopts
    /// the result via this constructor — it needs `base`/`len` too because the
    /// adopted reservation's usable span need not start at the OS reservation's
    /// own base (this crate over-reserves `size + align` and keeps the full
    /// mapping whenever the exact-size fast path misses, or on Windows when
    /// `align > 64 KiB`, which is exactly that shape).
    ///
    /// # Safety
    ///
    /// All six values must describe a **live, exclusively-owned OS
    /// reservation** compatible with `aligned-vmem`'s release path:
    ///
    /// - `base` is the *aligned usable* start; non-null, valid for `len` bytes,
    ///   aligned to `align` AND to the runtime [`page_size()`] (not just the
    ///   compile-time `PAGE`). On systems with non-4 KiB pages (e.g., 16 KiB on
    ///   Apple Silicon), passing a 4 KiB-aligned `base` will cause `decommit`,
    ///   `decommit_lazy`, or `munmap` calls to fail silently or return `EINVAL`.
    /// - `len` is the usable span size, a non-zero multiple of both [`PAGE`] and
    ///   the runtime [`page_size()`].
    /// - `reservation` is the *underlying OS reservation* start (often equal
    ///   to `base`, but may be lower because the reservation is over-reserved
    ///   to achieve alignment and the full mapping is kept). It must be aligned
    ///   to the runtime [`page_size()`].
    /// - `reservation_len` on Unix and under miri MUST be the full length of
    ///   the underlying OS mapping/allocation — Unix's `release` passes it
    ///   directly to `munmap`, and miri's `release` passes it as the exact
    ///   `Layout` size to `dealloc`, so an undersized value leaks memory (Unix)
    ///   or is undefined behavior (miri). On Windows, `VirtualFree(MEM_RELEASE)`
    ///   ignores this value, so it is advisory there — reporting the value
    ///   `Reservation::reservation_len` would report for an equivalent
    ///   reservation is sufficient on Windows only. It must in all cases be a
    ///   non-zero multiple of both `PAGE` and the runtime [`page_size()`] with
    ///   `reservation_len >= len + (base - reservation)`; both are asserted at
    ///   construction.
    /// - `align` is a power of two `>= PAGE` and matches the alignment the OS
    ///   reservation was created with.
    /// - `granted_huge` MUST accurately reflect whether the OS actually
    ///   granted huge pages for this reservation. Pass `true` only if the
    ///   reservation was obtained via a huge-page allocation (e.g.
    ///   `reserve_aligned_huge`) and the OS confirmed the grant (via
    ///   `Reservation::is_huge()` or equivalent platform-specific detection).
    ///   If you pass an incorrect value, `Reservation::is_huge()` will report
    ///   an incorrect value, and any decommit-availability decision you make
    ///   based on that wrong `is_huge()` result will be incorrect (on huge
    ///   pages, `decommit` is a silent no-op — RSS does not drop and reads
    ///   return the old data, not a crash or undefined behavior). If you cannot
    ///   determine whether the OS granted huge pages, you MUST pass `false` and
    ///   use `reserve_aligned` instead.
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
        granted_huge: bool,
    ) -> Self {
        // Historical notes (task #719, #776, #916):
        //
        // - task #719: validate the documented `align`/`reservation_len` contract
        //   HERE, at the unsafe call site, rather than leaving it to surface later
        //   as a panic inside `Drop::drop` (via the miri backend's
        //   `Layout::from_size_align(reservation_len, align).expect(...)` in
        //   `release_reservation`) -- a panic reachable from `Drop` is far more
        //   dangerous than one at construction time: if this `Reservation` is ever
        //   dropped while ANOTHER panic is already unwinding the stack, Rust
        //   aborts the whole process on the second panic. Every other construction
        //   path in this crate already produces a valid `(align, reservation_len)`
        //   pair by construction (validated at each public entry point), so this
        //   check is specific to the caller-supplied values `from_raw_parts`
        //   accepts. Violating the documented contract is already undefined
        //   behaviour per this function's own `# Safety` section; panicking
        //   immediately here converts a silently-deferred hazard into a loud,
        //   attributable failure at the actual point of misuse.
        //
        // - task #776 (F2 revision -- round-closing review finding F7): the
        //   original check validated only `align`, but `Layout::from_size_align`
        //   also fails when `reservation_len` overflows `isize::MAX` once rounded
        //   up to `align` -- an `align`-only check left that half of the SAME
        //   Drop-reachable-panic hazard open (e.g. `from_raw_parts(b, PAGE, r,
        //   usize::MAX, PAGE)` still constructed successfully and still panicked
        //   inside `Drop` under miri). The explicit `reservation_len != 0 &&
        //   reservation_len.is_multiple_of(PAGE)` checks enforce the documented
        //   nonzero/page-multiple invariants, while `Layout::from_size_align(...).
        //   is_ok()` catches overflow cases.
        //
        // - task #916 (H2C3): the comment above previously claimed these checks
        //   "cover all documented contract violations immediately at the call
        //   site" -- this was false. Four documented invariants were uncheckable
        //   from the arguments alone (pointer validity, liveness, exclusivity,
        //   and exact-once release), but three MORE were cheaply checkable and
        //   were NOT checked:
        //   - `len` must be a non-zero multiple of `PAGE` (documented, not checked)
        //   - `base` must be aligned to `align` (documented, not checked)
        //   - `reservation <= base` (documented, now checked below via `base_addr >= res_addr`)
        //   - `reservation_len >= len + (base - reservation)` (documented, not checked)
        //   All four are now checked explicitly below, leaving only the genuinely
        //   uncheckable invariants (pointer validity, liveness, exclusivity) as
        //   unchecked caller responsibilities.
        let base_nn = NonNull::new(base).expect("from_raw_parts: base must be non-null");
        let res_nn =
            NonNull::new(reservation).expect("from_raw_parts: reservation must be non-null");
        let base_addr = base.addr();
        let res_addr = reservation.addr();
        assert!(
            align.is_power_of_two()
                && align >= PAGE
                && reservation_len != 0
                && reservation_len.is_multiple_of(PAGE)
                && len != 0
                && len.is_multiple_of(PAGE)
                && base_addr >= res_addr
                && base_addr.is_multiple_of(align)
                && len
                    .checked_add(base_addr - res_addr)
                    .is_some_and(|required| reservation_len >= required)
                && std::alloc::Layout::from_size_align(reservation_len, align).is_ok(),
            "Reservation::from_raw_parts: \
             align must be a power of two >= PAGE; \
             reservation_len must be non-zero and a multiple of PAGE; \
             len must be non-zero and a multiple of PAGE; \
             base must be >= reservation; \
             base must be aligned to align; \
             reservation_len must be >= len + (base - reservation); \
             (reservation_len, align) must form a valid Layout; \
             got align={align}, reservation_len={reservation_len}, len={len}, \
             base={base:?}, reservation={reservation:?}"
        );
        Self {
            base: base_nn,
            len,
            reservation: res_nn,
            reservation_len,
            align,
            granted_huge,
        }
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        // Record the release for mock observers (RAII path visibility).
        #[cfg(aligned_vmem_mock)]
        mock::record(mock::Call::Release {
            reservation: self.reservation.as_ptr().addr(),
            reservation_len: self.reservation_len,
        });
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

/// The components returned by [`Reservation::into_reservation_parts`].
///
/// A named structure (instead of a raw tuple) prevents the footgun of
/// accidentally swapping the `len` and `align` fields, which would be
/// undefined behavior on the native backend and cause leaks or crashes
/// on the Unix backend.
///
/// `ReservationParts::new` closes the `release_parts` round-trip (release a
/// reservation you only have the parts for). Reconstructing a full
/// `Reservation` via `from_raw_parts` additionally requires the usable `base`,
/// `len`, and `granted_huge` fields, which the caller must record separately —
/// `ReservationParts` alone is insufficient whenever the reservation was
/// over-reserved for alignment or when huge-page status must be preserved.
/// If you omit `granted_huge`, the reconstructed reservation will incorrectly
/// report `is_huge() == false` even if the original reservation used huge pages.
#[non_exhaustive]
#[derive(Debug, PartialEq, Eq)]
pub struct ReservationParts {
    /// The base pointer of the reservation (from [`Reservation::reservation_ptr`]).
    pub ptr: *mut u8,
    /// The length of the reservation in bytes (from [`Reservation::reservation_len`]).
    pub len: usize,
    /// The alignment requested at reservation time.
    pub align: usize,
}

impl ReservationParts {
    /// Construct a `ReservationParts` from its component fields.
    ///
    /// This closes the `release_parts` round-trip (release a reservation you
    /// only have the parts for). Reconstructing a full `Reservation` via
    /// `from_raw_parts` additionally requires the usable `base`, `len`, and
    /// `granted_huge` fields, which the caller must record separately —
    /// `ReservationParts` alone is insufficient whenever the reservation was
    /// over-reserved for alignment or when huge-page status must be preserved.
    #[must_use]
    #[inline]
    pub const fn new(ptr: *mut u8, len: usize, align: usize) -> Self {
        Self { ptr, len, align }
    }

    /// Convert this struct back into a raw tuple compatible with [`release`].
    ///
    /// This method exists only for backwards compatibility with code that
    /// already uses the tuple form. New code should use [`release_parts`] instead.
    #[must_use]
    #[inline]
    pub const fn as_tuple(self) -> (*mut u8, usize, usize) {
        (self.ptr, self.len, self.align)
    }
}

/// The full components returned by [`Reservation::into_full_parts`].
///
/// This struct contains ALL six fields needed to reconstruct a `Reservation`
/// via [`Reservation::from_raw_parts`], eliminating the risk of metadata loss
/// during round-trip. Unlike [`ReservationParts`], it preserves `base`, `len`,
/// and `granted_huge` in addition to the underlying reservation metadata.
///
/// This is the lossless round-trip alternative to [`ReservationParts`]. Use it
/// when you need to temporarily extract all reservation state for later
/// reconstruction.
#[non_exhaustive]
#[derive(Debug, PartialEq, Eq)]
pub struct ReservationFullParts {
    /// The aligned usable start pointer (from [`Reservation::as_ptr`]).
    pub base: *mut u8,
    /// The usable span size in bytes (from [`Reservation::len`]).
    pub len: usize,
    /// The underlying OS reservation start (from [`Reservation::reservation_ptr`]).
    pub reservation: *mut u8,
    /// The length of the reservation in bytes (from [`Reservation::reservation_len`]).
    pub reservation_len: usize,
    /// The alignment requested at reservation time.
    pub align: usize,
    /// Whether the OS granted huge pages for this reservation (from [`Reservation::is_huge`]).
    pub granted_huge: bool,
}

impl ReservationFullParts {
    /// Construct a `ReservationFullParts` from its component fields.
    ///
    /// This is the inverse of [`Reservation::into_full_parts`]. All six fields
    /// are required to reconstruct a complete `Reservation` with no metadata loss.
    #[must_use]
    #[inline]
    pub const fn new(
        base: *mut u8,
        len: usize,
        reservation: *mut u8,
        reservation_len: usize,
        align: usize,
        granted_huge: bool,
    ) -> Self {
        Self {
            base,
            len,
            reservation,
            reservation_len,
            align,
            granted_huge,
        }
    }

    /// Reconstruct a `Reservation` from these parts.
    ///
    /// This is a convenience wrapper around [`Reservation::from_raw_parts`]
    /// that forwards all six fields. The same safety requirements apply.
    ///
    /// # Safety
    ///
    /// All six fields must satisfy the same invariants as documented for
    /// [`Reservation::from_raw_parts`]. See that function's `# Safety` section
    /// for full details.
    #[must_use]
    pub unsafe fn into_reservation(self) -> Reservation {
        // SAFETY: Delegated to the caller — same contract as `from_raw_parts`.
        unsafe {
            Reservation::from_raw_parts(
                self.base,
                self.len,
                self.reservation,
                self.reservation_len,
                self.align,
                self.granted_huge,
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Reserve
// ---------------------------------------------------------------------------

/// Reserve `size` bytes of anonymous virtual memory whose base is aligned to
/// `align`.
///
/// - `align` must be a power of two `>=` [`PAGE`].
/// - `size` must be a non-zero multiple of [`PAGE`].
///
/// On 32-bit Unix, first tries an ordinary exact-size `mmap` and checks
/// whether the kernel happened to place it at an `align`-aligned address
/// (fast path; hit rate depends on the OS's placement heuristics, not on any
/// hint this crate passes); on a miss (wrong alignment), over-reserves
/// `size + align` bytes and keeps the full mapping. On 64-bit Unix, the fast
/// path is compiled out (`target_pointer_width = "32"` — see task #944,
/// finding P-1), so every reservation always over-reserves `size + align`
/// bytes in one `mmap` call. On Windows, uses one syscall (fast path
/// for `align <= 64 KiB`, over-reserving nothing — base == region) or two
/// syscalls (over-reserving `size + align` and keeping the full mapping). The `Reservation::reservation_ptr` / `reservation_len` fields
/// expose the full reservation; `Reservation::as_ptr` / `len` expose the
/// aligned usable span.
///
/// **Cost on 32-bit Unix fast-path miss:** the reservation holds `size + align`
/// bytes of virtual address space for its lifetime (measured hit rate: 34.4% at
/// 64 KiB align, 46.7% at 1 MiB, 56.7% at 4 MiB — commit `35d51e6`, task #849;
/// measured on WSL2/Linux, x86_64; 30-run aggregate; scope: 32-bit only — the
/// hit rate is kernel- and ASLR-dependent and is not expected to transfer to
/// other Unix platforms). **On 64-bit Unix these numbers do not apply**: the
/// fast path never runs, so every reservation pays the "miss" cost of
/// `size + align` bytes held for the reservation's lifetime, unconditionally.
///
/// Returns `None` on a contract violation or if the OS refuses the reservation
/// (OOM) — never panics, so it is safe to call from inside a `GlobalAlloc`
/// implementation. For the failure cause use [`try_reserve_aligned`].
#[must_use]
pub fn reserve_aligned(size: usize, align: usize) -> Option<Reservation> {
    try_reserve_aligned(size, align).ok()
}

/// A contract violation (bad `size`/`align`) returns
/// [`VmemError::invalid_argument`] without touching the OS.
fn validate_size_align(size: usize, align: usize) -> Result<(), VmemError> {
    if size == 0 || !align.is_power_of_two() || align < PAGE || !size.is_multiple_of(PAGE) {
        return Err(VmemError::invalid_argument());
    }
    // Reject size/align combinations that would overflow `size + align`
    // internally (e.g. on the two-call path), to ensure consistent
    // classification as `invalid_argument` across all platforms rather than
    // an OS-specific refusal.
    let Some(sum) = size.checked_add(align) else {
        return Err(VmemError::invalid_argument());
    };
    // task #957 (fxx-3.3): `checked_add` above only rejects overflow past
    // `usize::MAX`, but `Layout::from_size_align` (consulted later for the
    // resulting `reservation_len`, e.g. in `release`'s G-1 assert and in
    // `from_raw_parts`'s equivalent check) additionally requires the size to
    // fit within `isize::MAX` once rounded up to `align` -- a `sum` between
    // `isize::MAX` and `usize::MAX` passes `checked_add` but would later fail
    // that `Layout` construction, turning what should be an immediate,
    // attributable `invalid_argument` here into a deferred assert/panic
    // downstream. Reject it here instead, for the same reason `checked_add`
    // is checked above: a contract violation should be classified
    // consistently and immediately, before any OS or `Layout` call.
    if sum > isize::MAX as usize {
        return Err(VmemError::invalid_argument());
    }
    Ok(())
}

/// Private helper: validate `initial_commit` for lazy reservations.
/// Only called from `try_reserve_aligned_lazy`, which is itself gated on
/// `lazy-commit` -- dead code when that feature is off.
#[cfg_attr(not(feature = "lazy-commit"), allow(dead_code))]
fn validate_initial_commit(initial_commit: usize, size: usize) -> Result<(), VmemError> {
    if initial_commit == 0 || !initial_commit.is_multiple_of(PAGE) || initial_commit > size {
        return Err(VmemError::invalid_argument());
    }
    Ok(())
}

/// Private struct for raw reservation results from backend functions.
/// Named to prevent transposing `base` and `reservation` (both `NonNull<u8>`).
///
/// This is call-site convenience only: the backend functions themselves still
/// return unnamed tuples, and the struct is constructed only at the call sites
/// via `.map()`. This helps at the call site but does NOT eliminate the
/// transposition risk entirely — the two `NonNull<u8>` tuple elements are still
/// unnamed at the backend layer.
struct RawReservation {
    /// The aligned usable base of the reservation.
    base: NonNull<u8>,
    /// The underlying OS reservation start (may be lower than `base`).
    reservation: NonNull<u8>,
    /// Full reservation length in bytes.
    reservation_len: usize,
    /// Whether large/huge pages were granted (Linux `MAP_HUGETLB` / Windows `MEM_LARGE_PAGES`).
    granted_huge: bool,
}

/// Private helper: finish a reservation from a raw backend result.
fn finish_reservation(
    size: usize,
    align: usize,
    raw: Result<RawReservation, VmemError>,
) -> Result<Reservation, VmemError> {
    raw.map(|r| Reservation {
        base: r.base,
        len: size,
        reservation: r.reservation,
        reservation_len: r.reservation_len,
        align,
        granted_huge: r.granted_huge,
    })
}

/// Private helper: finish a reservation from a raw backend result (4-tuple).
/// Only called from `try_reserve_aligned_huge`, which is itself gated on
/// `huge-pages` -- dead code when that feature is off.
#[cfg_attr(not(feature = "huge-pages"), allow(dead_code))]
fn finish_reservation_huge(
    size: usize,
    align: usize,
    raw: Result<(NonNull<u8>, NonNull<u8>, usize, bool), VmemError>,
) -> Result<Reservation, VmemError> {
    raw.map(
        |(base, reservation, reservation_len, granted_huge)| Reservation {
            base,
            len: size,
            reservation,
            reservation_len,
            align,
            granted_huge,
        },
    )
}

/// Fallible [`reserve_aligned`]: returns a [`VmemError`] carrying the OS cause
/// (`errno` / `GetLastError`) on failure instead of a bare `None`.
///
/// A contract violation (bad `size`/`align`) returns
/// [`VmemError::invalid_argument`] without touching the OS.
pub fn try_reserve_aligned(size: usize, align: usize) -> Result<Reservation, VmemError> {
    validate_size_align(size, align)?;
    // Mock fault-injection: honour a scripted reserve failure first.
    #[cfg(aligned_vmem_mock)]
    if let Some(e) = mock::take_reserve_fault() {
        mock::record(mock::Call::Reserve { size, align });
        return Err(e);
    }
    #[cfg(aligned_vmem_mock)]
    mock::record(mock::Call::Reserve { size, align });

    // task #713: `reserve_aligned_raw` now captures its own `VmemError`
    // immediately at the point of failure (before any cleanup FFI); this
    // just propagates it rather than re-deriving a possibly-stale one here.
    finish_reservation(
        size,
        align,
        reserve_aligned_raw(size, align).map(|(base, reservation, reservation_len)| {
            RawReservation {
                base,
                reservation,
                reservation_len,
                granted_huge: false,
            }
        }),
    )
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
///
/// If `reservation` is null, this function returns early and does nothing
/// (the call is a no-op). The mock recorder is also skipped in this case,
/// so a `mock`-based test's expected call log may desync if it expects a
/// record for a null pointer.
///
/// # Panics
///
/// Panics if `reservation` is non-null and `(reservation_len, align)` violates
/// the documented contract above: `reservation_len` must be non-zero and a
/// multiple of [`PAGE`], `align` must be a power of two `>= PAGE`, and the
/// pair must form a valid [`std::alloc::Layout`]. The assert runs before
/// `mock::record`, so under the `aligned_vmem_mock` cfg a contract-violating
/// call panics before it is ever recorded in the mock call log — it does not
/// appear as a `Release` entry.
///
/// A null `reservation` is unaffected by this: it remains the documented
/// no-op above and is not a panic path.
pub unsafe fn release(reservation: *mut u8, reservation_len: usize, align: usize) {
    // Historical note (task #947/G-1): before this assert existed, this doc
    // comment used to claim "the native (`munmap`/`VirtualFree`) paths ignore
    // `align`" — which was true in the sense that a contract-violating call
    // would silently "succeed" (no crash, no error) on those native backends;
    // only the `miri` fallback path (which reconstructs a `Layout` from
    // `reservation_len`/`align` to call back into `std::alloc`) would panic on
    // the same bad input, with a bare, uninformative `.expect()` message. That
    // divergence is now closed: this function validates the contract up front
    // and panics with a descriptive message on **every** backend, not only
    // under `miri`. The assert runs before `mock::record`, so under the
    // `aligned_vmem_mock` cfg a contract-violating call panics before it is
    // ever recorded in the mock call log — it does not appear as a `Release`
    // entry.
    //
    // The checked invariants are a subset of `from_raw_parts`'s checks because
    // `release` receives only `(reservation_len, align)` (not the full
    // `(base, len, reservation, reservation_len, align)` tuple), so the bounds
    // between `base` and `reservation` are uncheckable here — we validate what
    // we can and keep the same informative message style.
    if reservation.is_null() {
        return;
    }
    assert!(
        reservation_len != 0
            && reservation_len.is_multiple_of(PAGE)
            && align.is_power_of_two()
            && align >= PAGE
            && std::alloc::Layout::from_size_align(reservation_len, align).is_ok(),
        "release: \
         reservation_len must be non-zero and a multiple of PAGE; \
         align must be a power of two >= PAGE; \
         (reservation_len, align) must form a valid Layout; \
         got reservation_len={reservation_len}, align={align}"
    );

    let nn = NonNull::new(reservation).expect("checked non-null above");
    #[cfg(aligned_vmem_mock)]
    mock::record(mock::Call::Release {
        reservation: reservation.addr(),
        reservation_len,
    });
    // SAFETY: forwarded from the caller's contract above.
    unsafe { release_reservation(nn, reservation_len, align) };
}

/// Release a reservation obtained from [`Reservation::into_reservation_parts`].
///
/// This is the typed alternative to [`release`]: it takes a [`ReservationParts`]
/// struct instead of raw parameters, preventing accidental swapping of `len` and
/// `align` (which would cause undefined behavior on the native backend and leaks
/// or crashes on Unix).
///
/// For backwards compatibility with code that uses the raw tuple form, you can
/// convert a `ReservationParts` to a tuple via [`ReservationParts::as_tuple`] and
/// call [`release`].
///
/// # Safety
///
/// `parts.ptr` must be a reservation obtained from [`Reservation::into_reservation_parts`]
/// (or the raw [`Reservation::into_parts`]) and must be live. The reservation must be released
/// exactly once.
pub unsafe fn release_parts(parts: ReservationParts) {
    let ReservationParts {
        ptr: reservation,
        len: reservation_len,
        align,
    } = parts;
    // Delegate to the existing release function.
    unsafe { release(reservation, reservation_len, align) };
}

// ---------------------------------------------------------------------------
// Decommit / recommit
// ---------------------------------------------------------------------------

/// Decommit pages `[base + start, base + end)`: hint the OS to return
/// their physical backing while keeping the address-space reservation alive.
/// On Linux and Windows this is guaranteed to return physical backing and
/// zero-fill on next access (Linux `MADV_DONTNEED`, Windows `MEM_DECOMMIT`).
/// On the Darwin family (macOS/iOS/tvOS/watchOS) and the four BSDs
/// (FreeBSD/DragonFly/NetBSD/OpenBSD), this is a best-effort hint with no
/// zero-fill or reclaim guarantee — the physical pages may remain resident and
/// old data may be observed after a decommit+recommit roundtrip.
///
/// `start` and `end` must be multiples of [`page_size()`] and within the span.
/// A no-op if the range is empty.
///
/// # Safety
///
/// `base` must be the [`as_ptr`](Reservation::as_ptr) of a live reservation,
/// and `[base+start, base+end)` must contain no data the caller still needs —
/// its contents are discarded.
///
/// **No fallible form:** this entry point is intentionally infallible. The `()`
/// return carries no write-permitting sentinel to misuse, so silently skipping
/// on a contract violation is safe. A `try_decommit` return could be added in a
/// future additive API decision if consumers need error propagation, but the
/// README already argues for the safety of silent no-op on this shape.
///
/// **Platform divergence, not just a data-loss concern:** on Windows,
/// `MEM_DECOMMIT` genuinely unmaps the pages, so a **write to `[base+start,
/// base+end)` before [`recommit`] is a hard `STATUS_ACCESS_VIOLATION`
/// crash**, not a soft re-fault. On Linux, `MADV_DONTNEED` keeps the mapping
/// resident and transparently re-faults a fresh zero page on next write, so
/// the same code that is safe on Linux can crash on Windows. This exact
/// divergence already crashed an in-repo consumer that assumed the Linux
/// semantics — see
/// <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>
/// item 6 (filed 2026-07-30) for the incident record and status.
///
/// **Huge-page incompatibility (task #843 V4):** on both Windows and Linux,
/// decommit **does not work** on huge-page reservations (those returned by
/// [`reserve_aligned_huge`] with [`Reservation::is_huge`] == `true`).
/// On Windows, `VirtualFree` with `MEM_DECOMMIT` fails on large-page regions.
/// On Linux, `MADV_DONTNEED`/`MADV_FREE` on a `MAP_HUGETLB` mapping is accepted
/// only at huge-page granularity, so any [`page_size()`]-granular offset gets
/// `EINVAL` and does nothing. The behavior is therefore indistinguishable from
/// a silent no-op: the caller's RSS does not decrease, and subsequent reads
/// return the old (stale) data rather than zeroed pages. Use [`reserve_aligned`]
/// instead if you need decommit functionality.
///
/// **Darwin zero-fill gap (confirmed as a real, failing-test-level gap by
/// this crate's first real-macOS CI run, 2026-08-13 — the underlying hazard
/// was already known repo-wide since Round 9, see
/// <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>
/// item 48):** `MADV_DONTNEED` on Darwin and the four BSDs (FreeBSD/DragonFly/
/// NetBSD/OpenBSD) is advisory-only for anonymous memory — unlike Linux, it does
/// not reliably unmap the physical pages, so a decommit + [`recommit`] roundtrip
/// on these OS families (macOS/iOS/tvOS/watchOS — all share XNU and the same
/// `MADV_DONTNEED` semantics, not just macOS — plus the four BSDs which use
/// identical `MADV_DONTNEED` semantics) can observe the OLD data still resident
/// instead of a fresh zero page. This is the same "indistinguishable
/// from a silent no-op" shape as the huge-page case above, but for ORDINARY
/// (non-huge) reservations on Darwin and the BSDs specifically. See
/// <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>
/// for the open item; no fix is implemented
/// yet (the real fix needs re-`mmap`(`MAP_FIXED`) over the range, a larger
/// change deserving its own review round). Note: this caveat applies only to
/// the EAGER `decommit` path (which uses `MADV_DONTNEED` on all Unix); the
/// lazy `decommit_lazy` path uses `MADV_FREE`-family advice on Darwin/BSDs and
/// DOES free pages on those platforms.
pub unsafe fn decommit(base: *mut u8, start: usize, end: usize) {
    let ps = page_size();
    if start >= end || !start.is_multiple_of(ps) || !end.is_multiple_of(ps) {
        return;
    }
    #[cfg(aligned_vmem_mock)]
    mock::record(mock::Call::Decommit {
        base: base.addr(),
        start,
        end,
    });
    #[cfg(not(aligned_vmem_mock))]
    // SAFETY: forwarded from the caller's contract; the per-OS routine touches
    // only kernel page-state, never the bytes.
    unsafe {
        decommit_pages_impl(base, start, end, DecommitKind::Eager)
    };
}

/// Lazy decommit variant: hint the OS it MAY reclaim `[base+start, base+end)`
/// under memory pressure, cheaper than [`decommit`] (Linux `MADV_FREE`,
/// macOS/iOS `MADV_FREE_REUSABLE`, FreeBSD/DragonFly `MADV_FREE`,
/// NetBSD/OpenBSD `MADV_FREE`, other Unix (including tvOS/watchOS) falls
/// back to `MADV_DONTNEED`; Windows falls back to the eager [`decommit`]
/// path, which has no lazy equivalent).
///
/// Unlike [`decommit`], on Linux the pages are NOT necessarily zeroed on next
/// access if the kernel has not yet reclaimed them (a write before reclamation
/// keeps the old contents and cancels the free) — so this is appropriate only
/// for memory whose contents the caller no longer needs but has not yet
/// overwritten. Cheaper reclaim; the kernel takes pages only under pressure.
/// **This benign-re-fault story is Linux-only: on Windows this call is the
/// eager [`decommit`] path (see the summary above), where a write into the
/// range before [`recommit`] is a hard `STATUS_ACCESS_VIOLATION` crash, not a
/// re-fault** — see [`decommit`]'s platform-divergence paragraph above for the
/// incident this already caused.
///
/// **On macOS/iOS specifically, the cost ordering above is INVERTED, on the
/// RSS axis only** — see [`decommit`]'s Darwin caveat: eager `decommit`'s
/// `MADV_DONTNEED` is a no-op there (drops nothing), while this lazy variant's
/// `MADV_FREE_REUSABLE` DOES drop the physical footprint immediately (not just
/// "under pressure"). Neither call zero-fills on next access on macOS/iOS —
/// that half of the non-guarantee is unchanged from the eager path. On
/// tvOS/watchOS this function falls back to the same `MADV_DONTNEED` as
/// [`decommit`] (see the "other Unix" case in the summary above — the arm
/// that excludes macOS/iOS specifically, not "other Unix" in a general
/// sense), so there it IS a true no-op like the eager path, on both axes.
/// This tvOS/watchOS fallback is this crate's current `madv_free_advice` cfg
/// coverage (REASONED-FROM-SPEC, not verified on tvOS/watchOS hardware or a
/// tvOS/watchOS build target -- neither is available to this crate's CI):
/// `MADV_FREE_REUSABLE`'s numeric value is defined by XNU, the kernel all
/// four Darwin targets share, so it MAY work identically there too; but
/// tvOS/watchOS's userspace sandbox restrictions are unverified for this
/// specific advice value, so this is a plausible widening candidate, not an
/// established fact (see `madv_free_advice`'s doc and
/// <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>
/// item 48's S9 note, which must agree with this wording -- keep both in
/// sync if either changes).
///
/// **No fallible form:** this entry point is intentionally infallible, for the
/// same safety rationale as [`decommit`]. The `()` return carries no
/// write-permitting sentinel, so silently skipping on a contract violation is
/// safe. A `try_decommit_lazy` could be added as a future additive API decision.
///
/// `start`/`end` contract and safety are identical to [`decommit`].
///
/// # Safety
///
/// Same as [`decommit`].
pub unsafe fn decommit_lazy(base: *mut u8, start: usize, end: usize) {
    let ps = page_size();
    if start >= end || !start.is_multiple_of(ps) || !end.is_multiple_of(ps) {
        return;
    }
    #[cfg(aligned_vmem_mock)]
    mock::record(mock::Call::DecommitLazy {
        base: base.addr(),
        start,
        end,
    });
    #[cfg(not(aligned_vmem_mock))]
    // SAFETY: forwarded from the caller's contract; the per-OS routine touches
    // only kernel page-state, never the bytes.
    unsafe {
        decommit_pages_impl(base, start, end, DecommitKind::Lazy)
    };
}

/// Recommit pages `[base + start, base + end)` previously passed to
/// [`decommit`]. On Windows this re-commits physical pages
/// (`VirtualAlloc(MEM_COMMIT)`); on Unix re-access is implicit so this is a
/// no-op. On the Darwin family (macOS/iOS/tvOS/watchOS) specifically, whether
/// re-access reads back zeroed pages or the pre-decommit contents is not
/// guaranteed either way — see [`decommit`]'s Darwin caveat for why.
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
/// `start`/`end` must be multiples of the runtime page size ([`page_size()`])
/// with `start <= end` — a violation returns `false` (task #712: an earlier
/// version of this function clamped a contract violation to the WRITE-PERMITTING
/// `true` sentinel, which already caused a real crash — see
/// <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>
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
    let ps = page_size();
    if start > end || !start.is_multiple_of(ps) || !end.is_multiple_of(ps) {
        return Err(VmemError::invalid_argument());
    }
    if start == end {
        return Ok(());
    }
    #[cfg(aligned_vmem_mock)]
    {
        mock::record(mock::Call::Recommit {
            base: base.addr(),
            start,
            end,
        });
        mock::take_commit_fault().map_or(Ok(()), Err)
    }
    #[cfg(not(aligned_vmem_mock))]
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
/// `start` and `end` must be multiples of the runtime page size ([`page_size()`])
/// with `start <= end`. A well-formed no-op (empty range, `start == end`)
/// returns `true`; any other contract violation (misaligned, or `start > end`)
/// returns `false` (task #712: an earlier version of this function clamped a
/// contract violation to the WRITE-PERMITTING `true` sentinel, which already
/// caused a real crash — see
/// <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>
/// for the incident this class of bug produces on Windows).
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
#[cfg_attr(docsrs, doc(cfg(feature = "lazy-commit")))]
pub unsafe fn commit_range(base: *mut u8, start: usize, end: usize) -> bool {
    // SAFETY: forwarded from the caller's contract.
    unsafe { try_commit_range(base, start, end).is_ok() }
}

/// Fallible [`commit_range`]: `Ok(())` on success (or was a well-formed no-op),
/// `Err(VmemError::invalid_argument())` if the offsets violated the contract
/// (misaligned, or `start > end`), `Err(VmemError)` carrying the OS cause on
/// genuine commit failure.
///
/// # Safety
///
/// Same as [`commit_range`].
#[cfg(feature = "lazy-commit")]
#[cfg_attr(docsrs, doc(cfg(feature = "lazy-commit")))]
pub unsafe fn try_commit_range(base: *mut u8, start: usize, end: usize) -> Result<(), VmemError> {
    let ps = page_size();
    if start > end || !start.is_multiple_of(ps) || !end.is_multiple_of(ps) {
        return Err(VmemError::invalid_argument());
    }
    if start == end {
        return Ok(());
    }
    #[cfg(aligned_vmem_mock)]
    {
        mock::record(mock::Call::CommitRange {
            base: base.addr(),
            start,
            end,
        });
        mock::take_commit_fault().map_or(Ok(()), Err)
    }
    #[cfg(not(aligned_vmem_mock))]
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
#[cfg_attr(docsrs, doc(cfg(feature = "lazy-commit")))]
pub fn reserve_aligned_lazy(
    size: usize,
    align: usize,
    initial_commit: usize,
) -> Option<Reservation> {
    try_reserve_aligned_lazy(size, align, initial_commit).ok()
}

/// Fallible [`reserve_aligned_lazy`].
#[cfg(feature = "lazy-commit")]
#[cfg_attr(docsrs, doc(cfg(feature = "lazy-commit")))]
pub fn try_reserve_aligned_lazy(
    size: usize,
    align: usize,
    initial_commit: usize,
) -> Result<Reservation, VmemError> {
    validate_size_align(size, align)?;
    validate_initial_commit(initial_commit, size)?;
    #[cfg(aligned_vmem_mock)]
    if let Some(e) = mock::take_reserve_fault() {
        mock::record(mock::Call::ReserveLazy {
            size,
            align,
            initial_commit,
        });
        return Err(e);
    }
    #[cfg(aligned_vmem_mock)]
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
    #[cfg(aligned_vmem_mock)]
    let raw = reserve_aligned_raw(size, align);
    #[cfg(not(aligned_vmem_mock))]
    let raw = reserve_aligned_lazy_raw(size, align, initial_commit);

    // task #713: both `raw` branches now capture their own `VmemError`
    // immediately at the point of failure; this just propagates it.
    finish_reservation(
        size,
        align,
        raw.map(|(base, reservation, reservation_len)| RawReservation {
            base,
            reservation,
            reservation_len,
            granted_huge: false,
        }),
    )
}

// ---------------------------------------------------------------------------
// Huge / large pages (feature `huge-pages`).
// ---------------------------------------------------------------------------

/// Reserve `size` bytes aligned to `align`, requesting OS **large / huge
/// pages** (Linux `MAP_HUGETLB`, Windows `MEM_LARGE_PAGES`).
/// Currently a **no-op on macOS and other non-Linux Unix** — it falls back to
/// an ordinary reservation, identical to [`reserve_aligned`].
///
/// **Transparent-huge-page hinting (Linux `MADV_HUGEPAGE`) is not used:** it
/// cannot affect an already-explicitly-huge `MAP_HUGETLB` mapping (the pages
/// are already huge), so issuing it would be a wasted syscall. This crate's
/// strategy is the explicit hugetlbfs path only.
///
/// Large pages reduce TLB pressure for big allocator segments. The request is
/// **best-effort**: if the OS refuses large pages (none configured, no
/// privilege), the reservation transparently falls back to ordinary pages, so
/// this never fails purely because huge pages are unavailable — it fails only
/// on a genuine reservation error (OOM) or a contract violation.
///
/// To detect whether huge pages were actually granted (as opposed to having
/// fallen back to ordinary pages), use the returned [`Reservation::is_huge`]
/// method.
///
/// Base/align/size contract is otherwise identical to [`reserve_aligned`],
/// **except on Linux with `huge-pages` enabled**: `size` and `align` must BOTH
/// additionally be multiples of the Linux huge-page size (2 MiB) — a request
/// that only satisfies `reserve_aligned`'s own weaker `PAGE`-multiple contract
/// is rejected with `VmemError::invalid_argument()` before any syscall runs,
/// even though such a request could previously succeed there via the documented
/// ordinary-page fallback. For the failure cause use
/// [`try_reserve_aligned_huge`].
///
/// **Windows limitation:** on Windows, this function returns a reservation with
/// [`Reservation::is_huge`] == `true` only when ALL of the following hold:
/// 1. The fast-path condition `align <= GetLargePageMinimum()` is satisfied
///    (typically `align <= 2 MiB` on x86_64)
/// 2. `size` is a multiple of the system's large-page minimum
/// 3. The calling process has `SeLockMemoryPrivilege` granted AND has
///    **enabled** it via `AdjustTokenPrivileges` (the crate does not do
///    this for you — a process with the privilege granted but not enabled
///    fails exactly like an unprivileged one and silently falls back to
///    ordinary pages)
///
/// NOTE: The widened fast-path condition (II-3, 2026-08-16 audit finding) expanded
/// the single-call ATTEMPT window from `align <= 64 KiB` to `align <= GetLargePageMinimum()`,
/// but on an unprivileged host the actual paths that SUCCEED (pass the post-call alignment
/// check) are typically still limited. When large pages are NOT granted (unprivileged),
/// `VirtualAlloc`'s alignment guarantee is only 64 KiB; in practice it typically does NOT
/// happen to land on the requested alignment, so the post-call check fails and the fast
/// path falls through to the two-call path. Practically, this means `is_huge() == true` only
/// for shapes where large pages are actually granted, which requires all three conditions
/// above to hold.
///
/// **Extra-syscall cost on unprivileged hosts:** For the widened align range
/// (`64 KiB < align <= GetLargePageMinimum()`), when large pages are requested but
/// not granted (e.g., unprivileged process, or `SeLockMemoryPrivilege` not enabled),
/// the code attempts `VirtualAlloc` with `MEM_LARGE_PAGES` (fails), retries without
/// it (succeeds with ordinary pages), and if that retry's base doesn't happen to
/// satisfy the requested alignment, the whole thing is released and falls through to
/// the two-call path. This means an unprivileged reservation in this align range
/// can cost up to 2 extra `VirtualAlloc` calls + 1 `VirtualFree` before reaching
/// the two-call path, versus before the II-3 change (which would have gone straight
/// to the two-call path for `align > 64 KiB`). This is a real, measurable behavior
/// change, not a correctness bug — the widening genuinely expands the single-call
/// attempt window, and unprivileged processes pay the extra-syscall cost for shapes
/// that now attempt but fail the fast path.
///
/// If any of these conditions fail, the function falls back to ordinary
/// pages and returns a reservation with [`Reservation::is_huge`] == `false`.
/// On Windows, large pages (`MEM_LARGE_PAGES`) are only ever requested and
/// possibly granted via the single-call fast path; the two-call path never requests
/// large pages, so the result never has
/// [`Reservation::is_huge`] == `true`.
///
/// **Decommit incompatibility:** on both Windows and Linux, [`decommit`] and
/// [`decommit_lazy`] **do not work** on huge-page reservations. On Windows,
/// `VirtualFree` with `MEM_DECOMMIT` fails on large-page regions. On Linux,
/// `MADV_DONTNEED`/`MADV_FREE` on a `MAP_HUGETLB` mapping is accepted only at
/// huge-page granularity, so any [`page_size()`]-granular offset gets `EINVAL`
/// and does nothing. The behavior is therefore indistinguishable from a silent
/// no-op: the caller's RSS does not decrease, and subsequent reads return the
/// old (stale) data rather than zeroed pages. Use [`reserve_aligned`] instead
/// if you need decommit functionality.
// Historical notes (task #776, #714, #848, #843):
//
// - task #776, F3: Linux huge-page request additionally requires both size
//   and align to be multiples of the Linux huge-page size (2 MiB), rejecting
//   PAGE-multiple requests that `reserve_aligned` accepts. This was added to
//   close a real `munmap` mapping leak (task #714); the trade-off is a
//   stricter contract in exchange for provable correctness.
//
// - task #848: Windows single-call fast path is the only
//   path that can grant large pages on Windows; the two-call path never
//   requests them. (For large-page requests, the fast-path condition is
//   `align <= GetLargePageMinimum()`, typically 2 MiB; for ordinary requests,
//   it is `align <= WIN_ALLOCATION_GRANULARITY`, 64 KiB.)
//
// - task #843, V4: decommit does not work on huge-page reservations on either
//   platform (Windows: VirtualFree fails; Linux: MADV_DONTNEED/MADV_FREE
//   requires huge-page granularity).
#[must_use]
#[cfg(feature = "huge-pages")]
#[cfg_attr(docsrs, doc(cfg(feature = "huge-pages")))]
pub fn reserve_aligned_huge(size: usize, align: usize) -> Option<Reservation> {
    try_reserve_aligned_huge(size, align).ok()
}

/// Fallible [`reserve_aligned_huge`].
#[cfg(feature = "huge-pages")]
#[cfg_attr(docsrs, doc(cfg(feature = "huge-pages")))]
pub fn try_reserve_aligned_huge(size: usize, align: usize) -> Result<Reservation, VmemError> {
    validate_size_align(size, align)?;
    #[cfg(aligned_vmem_mock)]
    if let Some(e) = mock::take_reserve_fault() {
        mock::record(mock::Call::ReserveHuge { size, align });
        return Err(e);
    }
    #[cfg(aligned_vmem_mock)]
    mock::record(mock::Call::ReserveHuge { size, align });

    // task #713: `reserve_aligned_huge_raw` now captures its own `VmemError`
    // immediately at the point of failure; this just propagates it.
    finish_reservation_huge(size, align, reserve_aligned_huge_raw(size, align))
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
///   `size` is accepted; a zero `size` returns `None`). On some platforms
///   (e.g. macOS with 16 KiB pages, or 64 KiB Windows allocation granularity),
///   the OS may round further beyond `PAGE`, so the actual granularity
///   consumed can exceed `PAGE`.
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
    win_reserve_commit(size, align, size, 0).map(
        |(base, reservation, reservation_len, _granted_huge)| (base, reservation, reservation_len),
    )
}

/// Windows over-reserve + commit helper shared by the eager, lazy and huge
/// paths. Takes two execution paths:
///
/// **Single-call fast path** (`align <= GetLargePageMinimum() && commit_len == size` for
/// large-page requests, `align <= WIN_ALLOCATION_GRANULARITY && commit_len == size` for
/// ordinary requests): reserves and commits `commit_len` bytes in one `VirtualAlloc` call with
/// `MEM_RESERVE | MEM_COMMIT | extra_commit_flags` (e.g., `MEM_LARGE_PAGES`).
/// If the initial call fails with `extra_commit_flags != 0`, it retries without
/// the extra flags (ordinary-page fallback). Returns `(base, base, commit_len, huge_granted)`
/// — the fourth element indicates whether the huge-page request actually succeeded
/// (true only when `extra_commit_flags` was nonzero AND the initial attempt succeeded
/// without falling back to ordinary pages).
///
/// **Two-call path** (all other cases): reserves address space in a first call,
/// then commits `commit_len` bytes with plain `MEM_COMMIT` (no extra flags applied).
/// The reserve size is conditional: when `align <= WIN_ALLOCATION_GRANULARITY`,
/// the fast-reserve optimization attempts to reserve exactly `size` bytes and
/// uses it if the result happens to already satisfy alignment; otherwise, it
/// reserves `size + align` bytes to guarantee an aligned base can be found.
/// Returns `(base, region, over, false)` — the fourth element is always `false`
/// because the two-call path never requests `MEM_LARGE_PAGES` (Windows rejects
/// it on pre-reserved regions anyway). On commit failure the whole reservation
/// is released and `Err` returned.
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
) -> Result<(NonNull<u8>, NonNull<u8>, usize, bool), VmemError> {
    // V21 (task #848): for align <= 64 KiB, use a single combined
    // VirtualAlloc(NULL, size, MEM_RESERVE | MEM_COMMIT [| extra_flags])
    // call instead of two calls. VirtualAlloc(NULL, ...) already returns
    // a base aligned to WIN_ALLOCATION_GRANULARITY (64 KiB on all supported
    // Windows targets), so the alignment contract is satisfied by construction.
    //
    // II-3 (2026-08-16 audit finding): when requesting large pages
    // (extra_commit_flags includes MEM_LARGE_PAGES), widen the fast-path
    // condition to use GetLargePageMinimum() instead of WIN_ALLOCATION_GRANULARITY.
    // A granted large-page allocation is naturally aligned to at least the
    // large-page minimum (typically 2 MiB on Windows), so it will already
    // satisfy alignments up to that minimum. The unconditional alignment check
    // below guarantees correctness even if large pages are not granted.
    //
    // NOTE: GetLargePageMinimum() returns 0 on systems/CPU that do not support
    // large pages at all (Microsoft documentation). Since align >= PAGE > 0 always,
    // the comparison align <= 0 is always false for a positive align, meaning the
    // fast path becomes unreachable on such hosts. This is a safe degenerate case:
    // we fall through correctly to the two-call path, same as when align > threshold.
    // No special-case code is needed; the existing threshold comparison handles it.
    //
    // Historical note: a ~4.6 µs / ~33% reduction claim was made in the original
    // V21 commit, inherited from pre-#848 measurement (R32_13) of the OLD two-call
    // path. That claim has NOT been re-measured for the current single-call fast
    // path code. The claim should be treated as an unverified hypothesis, not a
    // validated benchmark result.
    //
    // `commit_len == size` is REQUIRED, not just an optimization detail: a
    // single VirtualAlloc(.., MEM_RESERVE | MEM_COMMIT, ..) call reserves AND
    // commits the SAME byte range -- there is no way to reserve `size` bytes
    // while committing only a smaller `commit_len` in one call. The lazy-commit
    // path (`reserve_aligned_lazy` -> `commit_range` later) calls this function
    // with `commit_len < size` by design (reserve the full span up front,
    // commit incrementally). Taking the single-call path there would silently
    // shrink the actual reservation to `commit_len` bytes, breaking every
    // later `commit_range` call past that point -- confirmed concretely: a
    // targeted repro (align=4 KiB, size=64 KiB, initial_commit=4 KiB) showed
    // the returned `reservation_len` was only 4096, not 65536, and the
    // follow-up `commit_range` past `initial_commit` failed. Guarding on
    // `commit_len == size` keeps the fast path to exactly the case it's sound
    // for (the eager `reserve_aligned`/`reserve_aligned_huge` callers, which
    // always pass `commit_len == size`) and routes the lazy-commit caller
    // through the unchanged two-call path below.
    let fast_path_align_threshold = if extra_commit_flags != 0 {
        // When requesting large pages, the threshold is the large-page minimum.
        // See the GetLargePageMinimum()==0 degenerate-case note above this function.
        unsafe { GetLargePageMinimum() }
    } else {
        WIN_ALLOCATION_GRANULARITY
    };
    if align <= fast_path_align_threshold && commit_len == size {
        // Single-call path: reserve+commit together.
        // Track whether huge pages were actually granted; initialized from the
        // request flag, but may be cleared if the retry fallback succeeds.
        let mut huge_granted = extra_commit_flags != 0;
        let base = unsafe {
            // SAFETY: `VirtualAlloc(NULL, commit_len, MEM_RESERVE | MEM_COMMIT
            // | extra_commit_flags, PAGE_READWRITE)` reserves and commits in one
            // syscall, returning the base or NULL on OOM/refusal. NULL is checked
            // below.
            let p = VirtualAlloc(
                core::ptr::null_mut(),
                commit_len,
                MEM_RESERVE | MEM_COMMIT | extra_commit_flags,
                PAGE_READWRITE,
            );
            match NonNull::new(p as *mut u8) {
                Some(n) => n,
                None => {
                    if extra_commit_flags != 0 {
                        // Best-effort retry: try without extra_commit_flags (e.g.
                        // MEM_LARGE_PAGES). This matches the two-call path's fallback
                        // behavior. On success, `huge_granted` is cleared because the
                        // retry succeeded with ordinary pages, not the original large-page
                        // request.
                        // SAFETY: fresh anonymous reserve+commit at a kernel-chosen
                        // address; NULL is checked below.
                        let plain = VirtualAlloc(
                            core::ptr::null_mut(),
                            commit_len,
                            MEM_RESERVE | MEM_COMMIT,
                            PAGE_READWRITE,
                        );
                        match NonNull::new(plain as *mut u8) {
                            Some(n) => {
                                huge_granted = false; // Fallback to ordinary pages
                                n
                            }
                            None => return Err(VmemError::last_os_error()),
                        }
                    } else {
                        return Err(VmemError::last_os_error());
                    }
                }
            }
        };
        // task #917 (finding H2C6, Windows analogue of Unix task #897/finding U1):
        // this check is UNCONDITIONAL. The fast-path's premise (VirtualAlloc(NULL, ...)
        // returns a base aligned to WIN_ALLOCATION_GRANULARITY) is REASONED from Microsoft
        // documentation but never verified at the point of use. The only verification is
        // a debug_assert in query_os_page_size() which compiles out of --release and
        // is not even called by this fast path (it lives on the cold decommit path).
        // If WIN_ALLOCATION_GRANULARITY were wrong (unlikely but theoretically possible
        // on a future Windows version), this check would catch it and fall through to
        // the two-call path, guaranteeing the documented alignment contract. Deliberately
        // a real runtime check, not a debug_assert: release builds are exactly where
        // an unverified constant matters (CLAUDE.md's R26-4 rule: debug_assert compiles
        // out of --release).
        // task #921/V-6: this check applies to BOTH the initial allocation AND any
        // retry fallback - we never return without verifying alignment, even on the
        // retry path that strips extra_commit_flags (e.g. MEM_LARGE_PAGES).
        if !base.as_ptr().addr().is_multiple_of(align) {
            // SAFETY: `base` was just allocated with VirtualAlloc(MEM_RESERVE | MEM_COMMIT)
            // and has not been released yet; releasing before handing to a caller prevents
            // a leak.
            unsafe { winapi_virtual_release(base.as_ptr()) };
            // Fall through to the two-call path below.
        } else {
            #[cfg(feature = "bench-internals")]
            WINDOWS_RESERVE_COMMIT_SINGLE_CALLS.fetch_add(1, Ordering::Relaxed);
            // Single-call path: base == region (no over-reserve).
            // Return (base, base, commit_len, huge_granted).
            // NOTE: huge_granted reflects which VirtualAlloc call actually succeeded:
            // if the retry fallback was taken, huge_granted is false (ordinary pages);
            // otherwise it is true only when extra_commit_flags (e.g. MEM_LARGE_PAGES)
            // was requested AND the initial attempt succeeded. We do not query the
            // actual grant at runtime, but this correctly tracks the observable
            // difference between "large-page request succeeded" vs "ordinary-page
            // fallback".
            return Ok((base, base, commit_len, huge_granted));
        }
    }

    // Two-call path (align > WIN_ALLOCATION_GRANULARITY for ordinary requests,
    // align > GetLargePageMinimum() for large-page requests, or a partial initial commit,
    // or single-call alignment check failed).
    // task #921/V-32: when align <= WIN_ALLOCATION_GRANULARITY, try a fast-reserve
    // path: VirtualAlloc(NULL, size, MEM_RESERVE, ...) may return a base already
    // aligned to the requested alignment, avoiding the size+align over-reserve overhead.
    // If it's not aligned, we release it and fall through to the over-reserve path.
    let (region, over) = if align <= WIN_ALLOCATION_GRANULARITY {
        let candidate = unsafe {
            // SAFETY: `VirtualAlloc(NULL, size, MEM_RESERVE, PAGE_READWRITE)`
            // reserves (but does not commit) `size` bytes of address space,
            // returning the base or NULL on OOM/refusal. NULL is checked below.
            let p = winapi_virtual_reserve(size);
            match NonNull::new(p as *mut u8) {
                Some(n) => n,
                // Nothing was reserved; no cleanup needed, so capturing here is
                // already the immediate-capture the task requires.
                None => return Err(VmemError::last_os_error()),
            }
        };
        let candidate_ptr = candidate.as_ptr();
        // Check if the reserved region happens to already be aligned to `align`.
        // VirtualAlloc(NULL, ...) returns a base aligned to WIN_ALLOCATION_GRANULARITY
        // (64 KiB), so this check often succeeds for `align <= 64 KiB` cases.
        if candidate_ptr.addr().is_multiple_of(align) {
            // Fast-reserve succeeded: use `size` directly, no over-reserve needed.
            // The aligned base equals the region base (no offset).
            (candidate, size)
        } else {
            // Aligned candidate won't work; release it and fall through to the
            // size+align over-reserve path below.
            // SAFETY: `candidate` was just reserved with `MEM_RESERVE` and has not
            // been released yet; releasing before falling back prevents a leak.
            unsafe { winapi_virtual_release(candidate_ptr) };
            // Continue to the over = size + align path.
            let over = size
                .checked_add(align)
                .ok_or_else(VmemError::invalid_argument)?;
            let region = unsafe {
                // SAFETY: same as the reserve call above, for `over` bytes.
                let p = winapi_virtual_reserve(over);
                match NonNull::new(p as *mut u8) {
                    Some(n) => n,
                    None => return Err(VmemError::last_os_error()),
                }
            };
            (region, over)
        }
    } else {
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
        (region, over)
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
    let committed =
        unsafe { VirtualAlloc(base.as_ptr().cast(), commit_len, MEM_COMMIT, PAGE_READWRITE) };
    if committed.is_null() {
        // Capture immediately after the failing commit, before cleanup.
        let err = VmemError::last_os_error();
        // SAFETY: `region` reserved above, not yet handed out — release once.
        unsafe { winapi_virtual_release(region_ptr) };
        return Err(err);
    }
    #[cfg(feature = "bench-internals")]
    WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS.fetch_add(1, Ordering::Relaxed);
    // task #921/V-7: the two-call path never requests MEM_LARGE_PAGES (always plain
    // MEM_COMMIT), so granted_huge is always false here. Only the single-call fast path
    // (align <= GetLargePageMinimum() for large-page requests, align <= WIN_ALLOCATION_GRANULARITY
    // otherwise) can grant huge pages.
    // NOTE: MEM_LARGE_PAGES on a pre-reserved (not pre-committed-with-large-pages) region
    // is empirically always rejected by Windows, so requesting it would be a guaranteed
    // wasted syscall anyway.
    Ok((base, region, over, false))
}

#[cfg(all(windows, not(miri)))]
unsafe fn release_reservation(reservation: NonNull<u8>, _reservation_len: usize, _align: usize) {
    // SAFETY: `reservation` was returned by a prior `VirtualAlloc(.., MEM_RESERVE, ..)`
    // with an inner aligned sub-range separately committed. `VirtualFree(.., 0,
    // MEM_RELEASE)` releases the entire MEM_RESERVE region regardless of commit state.
    unsafe { winapi_virtual_release(reservation.as_ptr()) };
}

#[cfg(all(windows, not(miri)))]
// mock (task #646/F8): bypassed by the recording backend, unused when `mock`
// alone is enabled without a real decommit call site reachable.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
unsafe fn decommit_pages_impl(base: *mut u8, start: usize, end: usize, _kind: DecommitKind) {
    // task #957 (NUM-1): guard the `end - start` subtraction below against an
    // inverted range (caller contract violation) so a debug build panics with
    // an attributable message rather than the subtraction silently wrapping.
    debug_assert!(
        start <= end,
        "decommit_pages_impl: start ({start}) must be <= end ({end})"
    );
    let len = end - start;
    // Windows has no lazy `MADV_FREE` equivalent — both eager and lazy map to
    // `MEM_DECOMMIT`.
    // SAFETY: caller guarantees `[base+start, +len)` is within a MEM_RESERVEd
    // region (not necessarily committed); `MEM_DECOMMIT` returns the physical pages,
    // and decommitting an already-uncommitted sub-range is a defined safe no-op.
    let addr = unsafe { base.add(start) };
    unsafe { winapi_virtual_decommit(addr, len) };
}

#[cfg(all(windows, not(miri)))]
// mock (task #646/F8): see decommit_pages_impl above.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
unsafe fn recommit_pages_impl(base: *mut u8, start: usize, end: usize) -> Result<(), VmemError> {
    // task #957 (NUM-1): guard the `end - start` subtraction below against an
    // inverted range (caller contract violation) so a debug build panics with
    // an attributable message rather than the subtraction silently wrapping.
    debug_assert!(
        start <= end,
        "recommit_pages_impl: start ({start}) must be <= end ({end})"
    );
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
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
unsafe fn commit_range_impl(base: *mut u8, start: usize, end: usize) -> Result<(), VmemError> {
    // Same MEM_COMMIT call as recommit (idempotent on Windows).
    // SAFETY: forwarded from the caller's contract.
    unsafe { recommit_pages_impl(base, start, end) }
}

#[cfg(all(windows, not(miri), feature = "lazy-commit"))]
// mock (task #646/F8): `try_reserve_aligned_lazy`'s real-path branch is
// compiled out under `mock`, so this never gets called.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
fn reserve_aligned_lazy_raw(
    size: usize,
    align: usize,
    initial_commit: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    win_reserve_commit(size, align, initial_commit, 0).map(
        |(base, reservation, reservation_len, _granted_huge)| (base, reservation, reservation_len),
    )
}

#[cfg(all(windows, not(miri), feature = "huge-pages"))]
fn reserve_aligned_huge_raw(
    size: usize,
    align: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize, bool), VmemError> {
    // Windows large pages work via the single-call fast path (task #848):
    // MEM_LARGE_PAGES is issued in a combined MEM_RESERVE | MEM_COMMIT call.
    // The fast-path condition is widened (2026-08-16 audit finding II-3) to
    // attempt the single-call path for any `align` up to GetLargePageMinimum()
    // (typically 2 MiB), not just the 64 KiB WIN_ALLOCATION_GRANULARITY. A
    // granted large-page allocation is naturally aligned to at least the
    // large-page minimum, so it satisfies alignments up to that threshold.
    // The unconditional post-call alignment check guarantees correctness
    // even if large pages are not granted (the allocation then uses ordinary
    // pages, which have the 64 KiB WIN_ALLOCATION_GRANULARITY guarantee).
    //
    // Even when the fast-path condition is satisfied, large-page allocation
    // requires:
    // 1. size is a multiple of the system's large-page minimum
    // 2. The process has SeLockMemoryPrivilege granted AND has enabled it
    //    via AdjustTokenPrivileges (this crate does not do this for you --
    //    granted-but-not-enabled fails exactly like unprivileged)
    // If either fails, the allocation falls back to ordinary pages and
    // granted_huge is false.
    //
    // This widening narrows (but does not eliminate) the platform gap versus
    // Linux's `align >= 2 MiB` requirement: the overlap is now in the 2 MiB
    // neighborhood (where both platforms CAN attempt a huge grant, though
    // Windows still needs privilege to actually succeed), not at 4 MiB
    // (which exceeds GetLargePageMinimum() and can never be huge on Windows).
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
    fn GetLargePageMinimum() -> usize;
}

/// Mirrors the Windows `SYSTEM_INFO` struct — only `dwPageSize` is read.
///
/// `Default` is all-zeroes (null for the two address fields);
/// `GetSystemInfo` overwrites the fields it defines.
#[cfg(all(windows, not(miri)))]
#[repr(C)]
#[derive(Default)]
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
const MEM_COMMIT: u32 = 0x0000_1000;
#[cfg(all(windows, not(miri)))]
const MEM_RESERVE: u32 = 0x0000_2000;
#[cfg(all(windows, not(miri)))]
const WIN_ALLOCATION_GRANULARITY: usize = 65536; // 64 KiB - VirtualAlloc alignment guarantee
#[cfg(all(windows, not(miri)))]
// mock (task #646/F8): only consumed by winapi_virtual_decommit below, which
// itself is unused under `mock`.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
const MEM_DECOMMIT: u32 = 0x0000_4000;
#[cfg(all(windows, not(miri)))]
const MEM_RELEASE: u32 = 0x0000_8000;
#[cfg(all(windows, not(miri), feature = "huge-pages"))]
const MEM_LARGE_PAGES: u32 = 0x2000_0000;
#[cfg(all(windows, not(miri)))]
const PAGE_READWRITE: u32 = 0x04;

#[cfg(all(windows, not(miri)))]
unsafe fn winapi_virtual_reserve(over: usize) -> *mut core::ffi::c_void {
    // SAFETY: `VirtualAlloc` with `MEM_RESERVE` only reserves address space without
    // commit; null base is documented for this usage and safe for any valid size.
    unsafe { VirtualAlloc(core::ptr::null_mut(), over, MEM_RESERVE, PAGE_READWRITE) }
}

#[cfg(all(windows, not(miri)))]
// mock (task #646/F8): see decommit_pages_impl above.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
unsafe fn winapi_virtual_decommit(addr: *mut u8, len: usize) {
    // SAFETY: caller guarantees `[addr, addr+len)` is within a MEM_RESERVEd region;
    // decommitting an already-uncommitted sub-range is a defined safe no-op per the Windows API contract.
    // task #921/V-8: the return value is deliberately discarded. A failure here would
    // indicate a bug in this crate's own bookkeeping (not a recoverable external condition),
    // and the failure mode is a leak, never unsafety. The failure is known to be reachable
    // in practice (e.g. the huge-page decommit case documented in `decommit`'s rustdoc), so
    // this is not a theoretical concern.
    //
    // task P2-6 (2026-08-16 audit finding): increment the failure counter
    // under `bench-internals` so at least diagnostic visibility exists. The
    // counter is gated on the feature and the increment is a single relaxed
    // fetch_add — zero overhead when the feature is off.
    //
    // Finding C-12 (2026-08-16 audit): add an attempts counter mirroring the
    // Unix `UNIX_MADVISE_ATTEMPTS`/`UNIX_MADVISE_SUCCESSES` pair, letting tests
    // distinguish "genuinely succeeded" from "never attempted".
    #[cfg(feature = "bench-internals")]
    WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    // SAFETY: `VirtualFree` with `MEM_DECOMMIT` is safe for any address/len within a `MEM_RESERVE`d region;
    // decommitting an already-uncommitted sub-range is a defined safe no-op per the Windows API contract.
    let ret = unsafe { VirtualFree(addr as *mut core::ffi::c_void, len, MEM_DECOMMIT) };
    #[cfg(feature = "bench-internals")]
    if ret == 0 {
        WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    #[cfg(not(feature = "bench-internals"))]
    let _ = ret;
}

#[cfg(all(windows, not(miri)))]
unsafe fn winapi_virtual_release(addr: *mut u8) {
    // SAFETY: caller guarantees `addr` is the base of a `MEM_RESERVE` region;
    // `MEM_RELEASE` + size 0 releases the entire reservation.
    // task #921/V-8: the return value is deliberately discarded. A failure here would
    // indicate a bug in this crate's own bookkeeping (not a recoverable external condition),
    // and the failure mode is a leak, never unsafety (the mapping stays valid, just not
    // returned to the OS).
    //
    // task R4-7 (2026-08-16 audit finding): increment the failure counter
    // under `bench-internals` so at least diagnostic visibility exists. The
    // counter is gated on the feature and the increment is a single relaxed
    // fetch_add — zero overhead when the feature is off.
    // SAFETY: `VirtualFree` with `MEM_RELEASE` and size 0 is safe for the base of a `MEM_RESERVE` region.
    let ret = unsafe { VirtualFree(addr as *mut core::ffi::c_void, 0, MEM_RELEASE) };
    #[cfg(feature = "bench-internals")]
    if ret == 0 {
        WINDOWS_VIRTUALFREE_RELEASE_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    #[cfg(not(feature = "bench-internals"))]
    let _ = ret;
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
    unix_reserve(size, align, false).map(|(base, reservation, reservation_len, _granted_huge)| {
        (base, reservation, reservation_len)
    })
}

/// Unix reservation shared by the eager and huge paths. When `huge` is `true`
/// the exact-size fast path and over-reserve fallback both request
/// `MAP_HUGETLB` (Linux) and fall back to ordinary pages if the huge mapping
/// fails.
///
/// Returns `(base, reservation, reservation_len, granted_huge)` where
/// `granted_huge` is `true` iff `huge` was `true` and the huge-page request
/// actually succeeded.
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
/// address). A non-huge-aligned `size` makes `over = size + align`
/// non-huge-aligned too, so the whole-mapping `munmap` in
/// `release_reservation` would fail `EINVAL`, leaking the ENTIRE mapping
/// (plus its pinned physical huge pages) on every affected reservation AND
/// on every subsequent [`release`].
///
/// REASONED-FROM-SPEC, NOT empirically verified (per this task's own
/// instruction: no hugetlb-configured host is in this project's CI). Fixed
/// by requiring `size` AND `align` to both be multiples of
/// [`LINUX_HUGE_PAGE_SIZE`] before attempting a Linux huge-page reservation
/// at all — with both huge-page-aligned, `over = size + align` is also
/// huge-page-aligned, so the whole-mapping `munmap` calls this function makes
/// (release_reservation unmaps the entire `reservation_len` span) are
/// provably conformant. The head offset `align_up_addr(region_addr, align) -
/// region_addr` is NOT zero in general for `align > LINUX_HUGE_PAGE_SIZE`
/// (e.g. with `align = 4 MiB` and a kernel-guaranteed 2-MiB-aligned region,
/// the offset is 2 MiB), but this no longer matters for correctness because
/// the entire over-reserve mapping is kept and released as a single unit
/// rather than being trimmed with head/tail munmap calls (task #842).
/// A caller that does not supply huge-page-aligned `size`/`align` gets a
/// clean, documented [`VmemError::invalid_argument`] instead of a silent leak.
#[cfg(all(unix, not(miri)))]
fn unix_reserve(
    size: usize,
    align: usize,
    huge: bool,
) -> Result<(NonNull<u8>, NonNull<u8>, usize, bool), VmemError> {
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        feature = "huge-pages"
    ))]
    if huge
        && (!size.is_multiple_of(LINUX_HUGE_PAGE_SIZE)
            || !align.is_multiple_of(LINUX_HUGE_PAGE_SIZE))
    {
        return Err(VmemError::invalid_argument());
    }
    // II-4 (2026-08-16 audit finding): Linux exact-size huge-page fast path
    // for `align == LINUX_HUGE_PAGE_SIZE` (2 MiB). The kernel guarantees an
    // anonymous MAP_HUGETLB mapping with addr == NULL starts at a huge-page-
    // aligned address (see the doc comment above), so an exact-size mmap
    // satisfies the alignment contract with zero over-reserve when
    // `align == LINUX_HUGE_PAGE_SIZE`. This avoids charging `size + align`
    // against the scarce hugetlb pool.
    //
    // This block handles ONLY the genuine-huge-page-granted case and returns
    // early on success; on any failure (mmap refusal, or -- defensively --
    // the kernel's alignment guarantee not holding) it falls through to the
    // general over-reserve path below instead of re-implementing its own
    // ordinary-page fallback. That path already correctly tracks
    // `granted_huge = false` on a huge-to-ordinary fallback (see its own
    // `libc_mmap(over, huge)` retry below) -- duplicating that logic here
    // previously produced a real bug (caught by zero-trust review): an
    // earlier version of this block retried `libc_mmap(size, false)` inline
    // and unconditionally returned `granted_huge = true` even when that
    // ordinary-page retry was the one that actually succeeded, which is
    // both a false `is_huge()` claim (the same class of bug task #943/W-1
    // fixed for Windows) and paired with an alignment guarantee that does
    // NOT hold for a plain (non-`MAP_HUGETLB`) mapping.
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        feature = "huge-pages"
    ))]
    if huge && align == LINUX_HUGE_PAGE_SIZE {
        // Track exact-size attempt (bench-internals oracle for II-4 fast path).
        #[cfg(feature = "bench-internals")]
        UNIX_EXACT_RESERVE_ATTEMPTS.fetch_add(1, Ordering::Relaxed);

        // SAFETY: anonymous private MAP_HUGETLB mapping of exactly `size` bytes.
        let p = unsafe { libc_mmap(size, true) };
        if !p.is_null() {
            let region_addr = p.addr();
            // Real runtime check (not debug_assert!): the kernel's huge-page-
            // alignment guarantee is unverified behavior this crate is
            // trusting, not self-evident arithmetic -- release builds are
            // exactly where an unverified assumption matters (same reasoning
            // as the Windows H2C6 / Unix U1 checks elsewhere in this file).
            if region_addr.is_multiple_of(align) {
                // Track exact-size hit (bench-internals oracle for II-4 fast path).
                #[cfg(feature = "bench-internals")]
                UNIX_EXACT_RESERVE_HITS.fetch_add(1, Ordering::Relaxed);

                // SAFETY: non-null and just verified aligned.
                let base = unsafe { NonNull::new_unchecked(p as *mut u8) };
                return Ok((base, base, size, true));
            }
            // Kernel's alignment guarantee did not hold (should not happen in
            // practice); release this mapping and fall through below.
            // SAFETY: `p` was returned by `mmap` above and not yet handed to
            // a caller; releasing before falling through prevents a leak.
            unsafe { libc_munmap(p.cast(), size) };
        }
        // Huge mmap failed, or its alignment guarantee didn't hold: fall
        // through to the general over-reserve path below.
    }
    // 32-bit only: try exact-size mmap first for address-space economy.
    // On 64-bit this is a net syscall loss (the fast path costs 1 syscall on a
    // hit, 3 on a miss, vs a flat 1 syscall for the over-reserve path), and
    // address-space economy is not a concern on 64-bit.
    //
    // R3-1/R4-2 fix: skip 32-bit generic exact path when huge-page exact-size fast path
    // was already tried above (lines 2707-2738). Without this check, a 32-bit host
    // with hugetlb pool == 0 would call `try_reserve_aligned_exact(size, align, huge=true)`
    // after the specialized huge-exact path just failed with the same MAP_HUGETLB call,
    // causing `UNIX_EXACT_RESERVE_ATTEMPTS` to be incremented twice for one logical reserve.
    #[cfg(target_pointer_width = "32")]
    {
        #[cfg(all(any(target_os = "linux", target_os = "android"), feature = "huge-pages"))]
        let huge_exact_already_tried = huge && align == LINUX_HUGE_PAGE_SIZE;
        #[cfg(not(all(any(target_os = "linux", target_os = "android"), feature = "huge-pages")))]
        let huge_exact_already_tried = false;

        if !huge_exact_already_tried {
            if let Ok((base, reservation, reservation_len, granted_huge)) =
                try_reserve_aligned_exact(size, align, huge)
            {
                return Ok((base, reservation, reservation_len, granted_huge));
            }
        }
    }
    let over = size
        .checked_add(align)
        .ok_or_else(VmemError::invalid_argument)?;
    // Track whether huge pages were actually granted; assigned in each branch
    // below (a bare pointer, not a tuple, must be the unsafe block's own tail
    // expression -- `region_ptr` is used as a raw pointer immediately after).
    let granted_huge;
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
                granted_huge = false; // Fallback to ordinary pages
                p2
            } else {
                return Err(VmemError::last_os_error());
            }
        } else {
            granted_huge = HUGE_SUPPORTED && huge; // Huge pages requested and actually supported
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
        (tail_start <= region_end).then_some(a)
    });
    let base_addr = match fits {
        Some(a) => a,
        None => {
            // Not an OS refusal — an internal fit-computation failure (should
            // not occur given `over = size + align`); do not read errno here.
            // SAFETY: `region_ptr` was returned by `mmap` above; releasing the
            // whole `over`-byte mapping before handing to a caller is sound.
            unsafe { libc_munmap(region_ptr.cast(), over) };
            return Err(VmemError::invalid_argument());
        }
    };
    // Symmetry with `try_reserve_aligned_exact`'s unconditional alignment check:
    // that check is a runtime check (not debug_assert) because it guards against
    // an unverified `_SC_PAGESIZE` constant producing a misaligned base on BSDs.
    // Here, the arithmetic `align_up_addr(region_addr, align)` is self-evidently
    // correct with no unverified-constant dependency, so a debug_assert is sufficient.
    debug_assert!(
        base_addr.is_multiple_of(align),
        "over-reserve base_addr must be align-aligned"
    );
    // SAFETY: `base_addr >= region_addr` and `align`-aligned; `with_addr`
    // carries `region_ptr`'s provenance (valid for the whole `over`-byte
    // mapping) to the computed address.
    let base = unsafe { NonNull::new_unchecked(region_ptr.with_addr(base_addr).cast::<u8>()) };
    // Keep the entire over-reserve mapping as the reservation, exactly as
    // the Windows backend does. This removes the `munmap` trim calls and
    // makes V1's alignment bug structurally impossible: there is exactly one
    // `munmap` at `region_ptr` (provably page-aligned by `mmap`'s contract)
    // instead of two at potentially misaligned offsets. Cost: up to `align`
    // bytes of untouched VA held for the reservation's lifetime (no RSS).
    Ok((
        base,
        // SAFETY: `region_ptr` is confirmed non-null above (both the `p` and
        // `p2` paths null-check before reaching here); this cast preserves
        // `region_ptr`'s own provenance (valid for the whole `over`-byte
        // mapping), unlike `base` above which is `with_addr`'d to a
        // sub-range of it.
        unsafe { NonNull::new_unchecked(region_ptr.cast::<u8>()) },
        over,
        granted_huge,
    ))
}

/// 1-syscall exact-size mmap fast path. `huge` requests
/// `MAP_HUGETLB`.
///
/// Returns `(base, reservation, reservation_len, granted_huge)` where
/// `granted_huge` is `true` iff `huge` was `true` and the mapping succeeded.
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
///
/// NOTE: This function is only used on 32-bit targets for address-space
/// economy. On 64-bit, `unix_reserve` always over-reserves (1 syscall)
/// because the fast path is a net syscall loss at all hit rates.
#[cfg(all(unix, not(miri), target_pointer_width = "32"))]
fn try_reserve_aligned_exact(
    size: usize,
    align: usize,
    huge: bool,
) -> Result<(NonNull<u8>, NonNull<u8>, usize, bool), VmemError> {
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
    // task #897 (finding U1): this check is UNCONDITIONAL -- it used to be
    // guarded by `align > page_size() &&`, reasoned as follows: when
    // `align <= page_size()`, `mmap` always returns page-aligned addresses,
    // so the check below is provably false and can be skipped. That
    // reasoning is correct only if `page_size()` is <= the REAL OS page
    // size. `page_size()` accepts any power-of-two value >= `PAGE` read
    // from `sysconf(_SC_PAGESIZE)` (see its own guard comment above,
    // "a plausible-looking power-of-two answer to a DIFFERENT question") --
    // a wrong `_SC_PAGESIZE` constant that happens to return a power-of-two
    // ABOVE the real page size would be silently accepted and cached
    // process-wide (docs/CORRECTNESS_OPEN_ITEMS.md item 43's still-open BSD
    // half: FreeBSD/DragonFly/NetBSD/OpenBSD constants are REASONED-FROM-SPEC,
    // not hardware-verified). If that ever happens, every `align` in
    // `(real_page_size, page_size()]` would silently skip this check, and
    // `try_reserve_aligned_exact` could return a base that is NOT aligned to
    // `align` -- violating `Reservation::as_ptr()`'s documented alignment
    // guarantee with no error and no diagnostic. The `align > page_size()`
    // conjunct measured zero syscalls saved (measurement #849: 480/480 hits
    // in page-size mode, i.e. it only ever removed a dead branch, never
    // avoided real work), so there is no performance cost to always running
    // the check. Deliberately a real runtime check, not a `debug_assert!`:
    // release builds are exactly where an unverified `_SC_PAGESIZE` constant
    // would matter (CLAUDE.md's R26-4 rule: `debug_assert!` compiles out of
    // `--release`).
    if !region_addr.is_multiple_of(align) {
        // SAFETY: `region_ptr` was just mapped with length `size`; unmap once.
        unsafe { libc_munmap(region_ptr.cast(), size) };
        return Err(VmemError::invalid_argument());
    }
    #[cfg(feature = "bench-internals")]
    UNIX_EXACT_RESERVE_HITS.fetch_add(1, Ordering::Relaxed);
    // SAFETY: non-null and proven `align`-aligned.
    let base = unsafe { NonNull::new_unchecked(region_ptr as *mut u8) };
    // `granted_huge` reflects what was actually requested AND what the OS supports.
    // On non-Linux Unix the `huge` flag is silently ignored, so we report false.
    // This is correct because `MAP_HUGETLB` fails the WHOLE `mmap` call when
    // 2 MiB hugetlb pages are unavailable (an all-or-nothing kernel behavior),
    // so "the caller asked for huge and mmap succeeded" implies a grant.
    Ok((base, base, size, HUGE_SUPPORTED && huge))
}

#[cfg(all(unix, not(miri)))]
unsafe fn release_reservation(reservation: NonNull<u8>, reservation_len: usize, _align: usize) {
    // SAFETY: on unix `reservation` is the start of the remaining mapping of length
    // `reservation_len`; `libc_munmap` requires the address/len to be a live mmap'd
    // region being unmapped once, which this call satisfies.
    unsafe { libc_munmap(reservation.as_ptr(), reservation_len) };
}

#[cfg(all(unix, not(miri)))]
// mock (task #646/F8): bypassed by the recording backend, unused when `mock`
// alone is enabled without a real decommit call site reachable.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
unsafe fn decommit_pages_impl(base: *mut u8, start: usize, end: usize, kind: DecommitKind) {
    // task #957 (NUM-1): guard the `end - start` subtraction below against an
    // inverted range (caller contract violation) so a debug build panics with
    // an attributable message rather than the subtraction silently wrapping.
    debug_assert!(
        start <= end,
        "decommit_pages_impl: start ({start}) must be <= end ({end})"
    );
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
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
unsafe fn recommit_pages_impl(_base: *mut u8, _start: usize, _end: usize) -> Result<(), VmemError> {
    // On Linux, re-access after MADV_DONTNEED is implicit — the kernel
    // actually unmaps the physical pages, so the next write re-faults a
    // fresh zero page. No syscall, cannot fail, on Linux.
    //
    // CAVEAT (confirmed as a real, failing-test-level gap by this crate's
    // first real-macOS CI run, 2026-08-13 -- the underlying hazard was
    // already known repo-wide since Round 9, see
    // docs/CORRECTNESS_OPEN_ITEMS.md item 48): this does
    // NOT hold on the Darwin family (macOS/iOS/tvOS/watchOS). `madvise(MADV_DONTNEED)`
    // on Darwin is advisory only for anonymous memory and does not reliably
    // unmap/zero the pages, so a `decommit` + `recommit` roundtrip on Darwin
    // can observe the OLD data still resident — `decommit`'s "return physical
    // backing to the OS" promise is silently unmet on Darwin, the same shape
    // as the already-documented huge-page no-op above `decommit`'s own doc
    // comment. No fix implemented here (a real fix needs re-`mmap`(MAP_FIXED)
    // over the range on Darwin, a bigger change deserving its own review round); this
    // comment and the test scoping below are the honest interim state.
    Ok(())
}

#[cfg(all(unix, not(miri), feature = "lazy-commit"))]
// mock (task #646/F8): `try_commit_range`'s real-path branch is compiled out
// under `mock`, so this never gets called.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
unsafe fn commit_range_impl(_base: *mut u8, _start: usize, _end: usize) -> Result<(), VmemError> {
    // Unix: pages are already accessible (eager mmap). Always succeeds.
    Ok(())
}

#[cfg(all(unix, not(miri), feature = "lazy-commit"))]
// mock (task #646/F8): `try_reserve_aligned_lazy`'s real-path branch is
// compiled out under `mock`, so this never gets called.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
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
) -> Result<(NonNull<u8>, NonNull<u8>, usize, bool), VmemError> {
    unix_reserve(size, align, true)
}

/// Select the lazy-decommit `madvise` advice for this platform.
/// Linux: `MADV_FREE`; macOS/iOS: `MADV_FREE_REUSABLE`; FreeBSD/DragonFly:
/// `MADV_FREE` (5); NetBSD/OpenBSD: `MADV_FREE` (6). tvOS/watchOS are routed
/// to the `MADV_DONTNEED` fallback below (the `cfg` arms below match only
/// `any(target_os = "macos", target_os = "ios")`, not tvOS/watchOS — see
/// [`decommit_lazy`]'s own public doc, which documents this fallback as
/// current behavior). `MADV_FREE_REUSABLE` would be a plausible future
/// widening for tvOS/watchOS (same XNU kernel as macOS/iOS, so the numeric
/// advice value is shared), not current behavior — REASONED-FROM-SPEC only,
/// not verified on those targets, and not what this function actually does
/// today. REASONED-FROM-SPEC for all BSD constants — this crate has no BSD
/// CI runner to empirically verify on; values are from each OS's own
/// `sys/mman.h` headers (FreeBSD/DragonFly: 5, NetBSD/OpenBSD: 6).
#[cfg(all(unix, not(miri)))]
// mock (task #646/F8): only caller is decommit_pages_impl above, itself
// unused under `mock`.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
#[inline]
fn madv_free_advice() -> i32 {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        MADV_FREE
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        MADV_FREE_REUSABLE
    }
    #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
    {
        MADV_FREE_BSD_5
    }
    #[cfg(any(target_os = "netbsd", target_os = "openbsd"))]
    {
        MADV_FREE_BSD_6
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd",
    )))]
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
// task #893 (T6, round-7 review): `MAP_ANON`/`MAP_HUGETLB` below are gated on
// `target_os = "linux"` or `target_os = "android"`, but their numeric VALUES are `target_arch`-
// dependent, not just OS-dependent — the same class of non-portability the
// `_SC_PAGESIZE` table below (task #714) documents at length. `0x20` /
// `0x40000` are `asm-generic/mman-common.h`'s values, correct on every
// mainstream Linux architecture this crate targets (x86, x86_64, aarch64,
// arm, riscv, powerpc — every Linux architecture that uses
// `asm-generic/mman-common.h`, which is all of them except MIPS, Alpha,
// PA-RISC and Xtensa; NOT an exhaustive tier-1/tier-2 roster — s390x and
// loongarch64 are tier-2 and also use `asm-generic/mman-common.h`, just not
// named in this parenthetical -- round 7, task #895/TC9). They are WRONG on MIPS:
// `arch/mips/include/uapi/asm/mman.h` defines `MAP_ANONYMOUS = 0x0800` and
// `MAP_HUGETLB = 0x80000`; `0x20` on MIPS is actually the IRIX-compat
// `MAP_RENAME`, which Linux ignores — so on `mips*-unknown-linux-*` (tier 3;
// no such target is installed here or in this project's CI) `libc_mmap`
// would issue `mmap(..., MAP_PRIVATE, -1, 0)` with no anonymous flag set,
// `fd = -1` causes `EBADF`, `mmap` returns `MAP_FAILED`, and every
// `reserve_aligned` call fails closed with no diagnostic pointing at the
// wrong constant (Alpha, PA-RISC and Xtensa diverge similarly, but none of
// those is a current Rust target at all). Android's bionic libc runs on the
// Linux kernel, so the Linux values apply directly. REASONED-FROM-SPEC, not executed —
// no MIPS target is available to verify this against real hardware.
#[cfg(all(unix, not(miri), any(target_os = "linux", target_os = "android")))]
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

/// task #918 (finding H2C7): Compile-time error for Unix targets without a
/// `MAP_ANON` definition. Several real `cfg(unix)` targets (e.g. Android
/// `aarch64-linux-android`, illumos `x86_64-unknown-illumos`, Solaris
/// `x86_64-pc-solaris`) set `unix` but do NOT match either of the two
/// `MAP_ANON` cfg arms above (Linux or Darwin/BSD). Without this guard,
/// `libc_mmap` fails with a bare `error[E0425]: cannot find value MAP_ANON
/// in this scope` — fails closed (no unsoundness), but with an unattributable
/// compiler error rather than a clear diagnostic naming the actual reason
/// (unsupported target) and pointing at how to add support.
///
/// Adding support for a new Unix target requires:
/// 1. Confirming the target's `MAP_ANON` (or `MAP_ANONYMOUS`) constant value
///    from its libc headers or OS documentation.
/// 2. Adding a new `#[cfg(...)]` arm for that `target_os` with the correct
///    constant value, following the pattern of the two existing arms above.
#[cfg(all(
    unix,
    not(miri),
    not(any(target_os = "linux", target_os = "android")),
    not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly",
    ))
))]
compile_error!(
    "aligned-vmem does not currently support this Unix target because its \
     `MAP_ANON` constant is not defined. Adding support requires confirming \
     the target's `MAP_ANON` (or `MAP_ANONYMOUS`) value and adding a new \
     `#[cfg(...)]` arm for this `target_os` following the pattern in the file. \
     See the comment above this compile_error! for details."
);

// task #1017 (finding R4-1): Compile-time error for MIPS targets.
//
// MIPS (both `mips` and `mips64`) uses different `MAP_ANON`/`MAP_HUGETLB`
// constant values than the `asm-generic/mman-common.h` values this crate
// hardcodes for Linux: MIPS defines `MAP_ANONYMOUS = 0x0800` and
// `MAP_HUGETLB = 0x80000`, while this crate uses `0x20` and `0x40000`
// respectively (the `asm-generic` values). With the wrong constants,
// every `reserve_aligned` call fails closed at runtime with `EBADF`
// (invalid file descriptor) because `libc_mmap` issues `mmap(..., MAP_PRIVATE, -1, 0)`
// with no anonymous flag properly set, but the failure is silent (no diagnostic
// points to the constant error). Rather than compile a buildable-but-broken crate,
// we fail compilation with a clear diagnostic. This is a release decision:
// adding support requires adding a `#[cfg(any(target_arch = "mips", target_arch = "mips64"))]`
// arm with the correct MIPS-specific constant values. See `docs/CORRECTNESS_OPEN_ITEMS.md`.
#[cfg(all(
    unix,
    not(miri),
    any(target_arch = "mips", target_arch = "mips64")
))]
compile_error!(
    "aligned-vmem does not support MIPS: MAP_ANON/MAP_HUGETLB constant values \
     differ from the values this crate hardcodes, causing every reservation to \
     fail with EBADF at runtime with no diagnostic. See docs/CORRECTNESS_OPEN_ITEMS.md \
     for the release decision record."
);

/// Linux `MAP_HUGETLB` (request huge pages at mmap time).
///
/// Same architecture caveat as `MAP_ANON` above: `0x40000` is correct on
/// x86/x86_64/aarch64/arm/riscv/powerpc, wrong on MIPS (`0x80000` per
/// `arch/mips/include/uapi/asm/mman.h`) — see the note above `MAP_ANON` for
/// the full rationale and failure mode. Android's bionic libc runs on the
/// Linux kernel, so the Linux value applies directly; Android is covered by
/// the same `target_os = "linux"` arm.
#[cfg(all(
    unix,
    not(miri),
    any(target_os = "linux", target_os = "android"),
    feature = "huge-pages"
))]
const MAP_HUGETLB: i32 = 0x40000;

/// Linux `MAP_HUGE_2MB` (request 2 MiB huge pages at mmap time).
///
/// This flag explicitly requests the 2 MiB huge page size, overriding the
/// system's configured default huge-page size (set via the kernel boot
/// parameter `default_hugepagesz=`). Without this flag, plain `MAP_HUGETLB`
/// requests the system's default huge page size, which can be 1 GiB on
/// HPC/database-tuned hosts — a mismatch that would cause a silent leak
/// because this crate's `LINUX_HUGE_PAGE_SIZE` constant and validation logic
/// assume 2 MiB. The value is taken from the Linux kernel's
/// `include/uapi/linux/mman.h`: `MAP_HUGE_2MB = (21 << MAP_HUGE_SHIFT)` where
/// `MAP_HUGE_SHIFT = 26`. (REASONED-FROM-SPEC: this fix has NOT been empirically
/// verified on a real host with a non-2-MiB default `default_hugepagesz`, because
/// no such host exists in this project's CI.)
///
/// Note: The `MAP_HUGE_*` size encoding (the bits above `MAP_HUGE_SHIFT`)
/// was introduced in Linux 3.8 (2013); on older kernels these bits are not
/// interpreted by the `MAP_HUGETLB` path and the system's default huge-page size
/// is used instead. Android's bionic libc runs on the Linux kernel, so the
/// Linux value applies directly; Android is covered by the same
/// `target_os = "linux"` arm.
#[cfg(all(
    unix,
    not(miri),
    any(target_os = "linux", target_os = "android"),
    feature = "huge-pages"
))]
const MAP_HUGE_2MB: i32 = 21 << 26;

/// task #852 (W2): `HUGE_SUPPORTED` is true only on Linux or Android with the
/// `huge-pages` feature enabled. Non-Linux Unix (macOS, iOS, BSD, etc.) do NOT
/// support `MAP_HUGETLB` — the `libc_mmap` function silently ignores the `huge`
/// parameter on those platforms. This constant is used to ensure `granted_huge`
/// reports the ACTUAL grant, not just the request. Android's bionic libc runs on
/// the Linux kernel, so Android is covered by the same `target_os = "linux"`
/// arm and gets huge-page support when the feature is enabled.
#[cfg(all(
    unix,
    not(miri),
    any(target_os = "linux", target_os = "android"),
    feature = "huge-pages"
))]
const HUGE_SUPPORTED: bool = true;
#[cfg(all(
    unix,
    not(miri),
    not(all(
        any(target_os = "linux", target_os = "android"),
        feature = "huge-pages"
    ))
))]
const HUGE_SUPPORTED: bool = false;

/// task #909: the Linux huge page size this crate explicitly requests via
/// `MAP_HUGE_2MB` (not the system's configured default). Before this fix,
/// the crate used plain `MAP_HUGETLB` and relied on a now-falsified premise
/// that "the default is always 2 MiB on mainstream x86_64/aarch64 Linux" —
/// that premise was proved wrong by task #909's independent review finding H1,
/// which showed the default is independently configurable via the kernel boot
/// parameter `default_hugepagesz=` and can be 1 GiB on HPC/database-tuned hosts.
/// The fix (task #909) makes the crate explicitly request 2 MiB huge pages via
/// `MAP_HUGE_2MB`, making this constant correct by construction — if 2 MiB
/// huge pages are not configured/available on the host, the mmap call fails
/// cleanly (returns null → the crate's normal OOM/error path), rather than
/// silently succeeding with a mismatched size and leaking on munmap. `unix_reserve`
/// requires `size`/`align` to be multiples of this before attempting a huge-page
/// reservation at all, so every `munmap` it can still reach is provably
/// huge-page-aligned (see `unix_reserve`'s own doc for the full reasoning) —
/// REASONED-FROM-SPEC, not empirically verified on a real hugetlb-configured host
/// with a non-2-MiB default `default_hugepagesz` (none is in this project's CI).
/// Android's bionic libc runs on the Linux kernel, so the Linux value applies
/// directly; Android is covered by the same `target_os = "linux"` arm.
#[cfg(all(
    unix,
    not(miri),
    any(target_os = "linux", target_os = "android"),
    feature = "huge-pages"
))]
const LINUX_HUGE_PAGE_SIZE: usize = 2 * 1024 * 1024;
#[cfg(all(unix, not(miri)))]
const MAP_FAILED: usize = usize::MAX;
#[cfg(all(unix, not(miri)))]
// mock (task #646/F8): only consumed by decommit_pages_impl / madv_free_advice
// above, both unused under `mock`.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
const MADV_DONTNEED: i32 = 4;
/// Linux `MADV_FREE` (lazy reclaim under pressure).
#[cfg(all(unix, not(miri), any(target_os = "linux", target_os = "android")))]
// mock (task #646/F8): see MADV_DONTNEED above.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
const MADV_FREE: i32 = 8;
/// macOS/iOS `MADV_FREE_REUSABLE` (lazy reclaim; page reusable).
#[cfg(all(unix, not(miri), any(target_os = "macos", target_os = "ios")))]
const MADV_FREE_REUSABLE: i32 = 7;
/// FreeBSD/DragonFly `MADV_FREE` (lazy reclaim; advisory).
/// REASONED-FROM-SPEC — no BSD CI runner to empirically verify on; value from
/// `sys/mman.h` (both OSes use 5).
#[cfg(all(unix, not(miri), any(target_os = "freebsd", target_os = "dragonfly")))]
// mock (task #646/F8): see MADV_DONTNEED above.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
const MADV_FREE_BSD_5: i32 = 5;
/// NetBSD/OpenBSD `MADV_FREE` (lazy reclaim; advisory).
/// REASONED-FROM-SPEC — no BSD CI runner to empirically verify on; value from
/// `sys/mman.h` (both OSes use 6).
#[cfg(all(unix, not(miri), any(target_os = "netbsd", target_os = "openbsd")))]
// mock (task #646/F8): see MADV_DONTNEED above.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
const MADV_FREE_BSD_6: i32 = 6;
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
// (see `UNIX_EXACT_RESERVE_HITS`'s own doc comment above for the counter
// this hazard would have silently corrupted).
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
//
// task #951 (independent review finding 2.1): task #944/U-2 wired Android
// (bionic libc) into `MAP_ANON`/`MAP_HUGETLB`/`HUGE_SUPPORTED`/
// `LINUX_HUGE_PAGE_SIZE`/`MADV_FREE`/`libc_mmap`/`unix_reserve`'s huge
// guard, but missed this table — Android fell through to the `not(any(...))` fallback below and
// silently got the glibc value 30. Bionic does NOT share glibc's
// `confname.h` numbering (same portability hazard the BSD comment above
// already documents for a different OS family). EMPIRICALLY VERIFIED for
// this task (not merely reasoned-from-spec, unlike the BSD entries above,
// none of which run in this project's CI): fetched
// https://android.googlesource.com/platform/bionic/+/refs/heads/main/libc/include/bits/sysconf.h
// directly (AOSP `main` branch, `platform/bionic` repo) on 2026-08-15,
// which defines `#define _SC_PAGESIZE 0x0027` (= 39) and
// `#define _SC_PAGE_SIZE 0x0028` (= 40 — a DIFFERENT slot from
// `_SC_PAGESIZE` on bionic, unlike every other OS in this table where the
// two names alias the same value; callers must use `_SC_PAGESIZE`'s 39,
// not 40). glibc's value 30 lands on an unrelated `_SC_XOPEN_*`-range slot
// under bionic's table, which is why the wrong value silently poisons
// `page_size()`'s fallback path (returns a non-power-of-two or an
// unrelated small integer, fails `validate_page_size_impl`, and falls
// back to `PAGE` = 4096) instead of tripping the BSD-style "wrong but
// power-of-two" poison scenario the task #714 comment above warns about.
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
    #[cfg(target_os = "android")]
    {
        39
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
        target_os = "android",
    )))]
    {
        // glibc/musl Linux use 30 for _SC_PAGESIZE / _SC_PAGE_SIZE. This is
        // NOT a safe generalization to "most other unices" — that framing
        // has already been wrong twice (the four BSDs above, task #714;
        // Android/bionic above, task #951) — so treat 30 as the
        // glibc/musl-Linux value specifically, not a portable unix default.
        30
    }
};

// task #719, #914: `offset`'s type is conditionally-typed based on the
// target's actual `off_t` width. POSIX `off_t`'s width is platform-dependent:
// - 32-bit Linux (glibc) and Android (bionic) default to a 32-bit `off_t` without
//   `_FILE_OFFSET_BITS=64`. Since this crate's `mmap` is ALWAYS called with
//   offset=0 (anonymous mappings only, no file descriptor), a 32-bit `off_t`
//   is correct on these targets -- there is no code path where a wrong-width
//   `off_t` could silently truncate a real value.
// - 32-bit Linux with musl uses a 64-bit `off_t` unconditionally (musl defines
//   `off_t` as 64-bit on every architecture; `mmap64` is an alias of `mmap`).
// - Every other Unix target (64-bit pointer width, OR 32-bit BSD/Darwin) uses
//   a 64-bit `off_t`. BSDs and macOS define `off_t` as a 64-bit `int64_t` even
//   in 32-bit builds; this includes x86_64-unknown-{freebsd,netbsd,openbsd},
//   i686-unknown-{freebsd,openbsd}, and x86_64-apple-darwin.
// - Android's bionic is REASONED-FROM-SPEC: not empirically verified in this
//   project's CI, but bionic inherits the same 32-bit-off_t-on-32-bit-default
//   behavior as glibc on 32-bit targets without large-file-support flags.
//
// The two-arm `OffT` type alias below classifies targets as follows:
// - The `i32` arm matches 32-bit glibc and bionic targets only.
// - The `i64` catch-all arm matches everything else, including 32-bit musl,
//   all 64-bit targets, and all 32-bit BSD/Darwin targets.
#[cfg(all(
    unix,
    not(miri),
    target_pointer_width = "32",
    any(target_os = "linux", target_os = "android"),
    not(target_env = "musl")
))]
type OffT = i32;

#[cfg(all(
    unix,
    not(miri),
    not(all(
        target_pointer_width = "32",
        any(target_os = "linux", target_os = "android"),
        not(target_env = "musl")
    ))
))]
type OffT = i64;

#[cfg(all(unix, not(miri)))]
extern "C" {
    fn mmap(
        addr: *mut core::ffi::c_void,
        length: usize,
        prot: i32,
        flags: i32,
        fd: i32,
        offset: OffT,
    ) -> *mut core::ffi::c_void;
    fn munmap(addr: *mut core::ffi::c_void, length: usize) -> i32;
    fn madvise(addr: *mut core::ffi::c_void, length: usize, advice: i32) -> i32;
    fn sysconf(name: i32) -> core::ffi::c_long;
}

#[cfg(all(unix, not(miri)))]
unsafe fn libc_mmap(len: usize, huge: bool) -> *mut core::ffi::c_void {
    #[cfg_attr(
        not(all(
            any(target_os = "linux", target_os = "android"),
            feature = "huge-pages"
        )),
        allow(unused_mut)
    )]
    let mut flags = MAP_PRIVATE | MAP_ANON;
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        feature = "huge-pages"
    ))]
    if huge {
        flags |= MAP_HUGETLB | MAP_HUGE_2MB;
    }
    let _ = huge; // silence unused on non-linux / no huge-pages builds

    // SAFETY: anonymous private mapping; kernel chooses the address.
    let p = unsafe {
        mmap(
            core::ptr::null_mut(),
            len,
            PROT_READ | PROT_WRITE,
            flags,
            -1,
            0,
        )
    };
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
    //
    // task P2-6 (2026-08-16 audit finding): increment the failure counter
    // under `bench-internals` so at least diagnostic visibility exists. The
    // counter is gated on the feature and the increment is a single relaxed
    // fetch_add — zero overhead when the feature is off.
    let ret = unsafe { munmap(addr as *mut core::ffi::c_void, len) };
    #[cfg(feature = "bench-internals")]
    if ret != 0 {
        UNIX_MUNMAP_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    #[cfg(not(feature = "bench-internals"))]
    let _ = ret;
}

#[cfg(all(unix, not(miri)))]
// mock (task #646/F8): only caller is decommit_pages_impl above, itself
// unused under `mock`.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
unsafe fn libc_madvise(addr: *mut u8, len: usize, advice: i32) {
    // SAFETY: caller guarantees `[addr, addr+len)` is within a live mmap region.
    // task #719: the return value is deliberately discarded -- `madvise`
    // failing here means the OS did not reclaim the pages (the mapping stays
    // exactly as valid and readable/writable as before the call), not a
    // memory-safety concern; [`decommit`]/[`decommit_lazy`]'s own public
    // contracts already document decommit as an OS-cooperative hint whose
    // failure mode is "the physical pages were not actually returned", never
    // a dangling/invalid mapping.
    //
    // task #882: under `bench-internals` ONLY, also record whether the
    // syscall itself succeeded (returned `0`) or failed (returned `-1`) into
    // `UNIX_MADVISE_ATTEMPTS`/`UNIX_MADVISE_SUCCESSES` -- see those statics'
    // docs for why (settling item 48's H1-vs-H2 root-cause question). The
    // discard above is unconditional and unchanged for every non-bench build;
    // this is a read of the same return value the plain build throws away,
    // not a new syscall or a change to what `libc_madvise` returns to its
    // caller (still `()` either way), so it is zero-cost when the feature is
    // off.
    // SAFETY: `madvise` is safe for any address/len within a live mmap region; advice is a valid constant.
    let ret = unsafe { madvise(addr as *mut core::ffi::c_void, len, advice) };
    #[cfg(feature = "bench-internals")]
    {
        UNIX_MADVISE_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
        if ret == 0 {
            UNIX_MADVISE_SUCCESSES.fetch_add(1, Ordering::Relaxed);
        }
    }
    #[cfg(not(feature = "bench-internals"))]
    let _ = ret;
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
        Some(base) => Ok((base, base, size)), // Never huge under miri
        None => Err(VmemError::last_os_error()),
    }
}

#[cfg(miri)]
unsafe fn release_reservation(reservation: NonNull<u8>, reservation_len: usize, align: usize) {
    use std::alloc::Layout;
    // SAFETY: `reservation` was returned by `std::alloc::alloc` with exactly
    // this layout in `reserve_aligned_raw`; freed once.
    let layout = Layout::from_size_align(reservation_len, align).expect("release: invalid layout");
    // SAFETY: `reservation` was returned by `std::alloc::alloc` with exactly this layout.
    unsafe { std::alloc::dealloc(reservation.as_ptr(), layout) };
}

// Unsupported target: no `reserve_aligned_raw` / `release_reservation` implementation.
//
// The crate currently provides backends only for:
// - Windows (not miri): uses `VirtualAlloc` / `VirtualFree`.
// - Unix (not miri): uses `mmap` / `munmap`.
// - Miri: uses `std::alloc` for testing.
//
// This target has `std` but matches none of the above (e.g. `wasm32-wasip1`,
// `x86_64-fortanix-unknown-sgx`). Adding support requires a new
// `reserve_aligned_raw` / `release_reservation` implementation for this
// target family.
#[cfg(all(not(windows), not(unix), not(miri)))]
compile_error!(
    "aligned-vmem does not currently support this target because no \
     `reserve_aligned_raw` / `release_reservation` implementation exists \
     for it. The crate provides backends only for Windows, Unix, and miri. \
     Adding support requires implementing those two functions for this \
     target family."
);

#[cfg(miri)]
// mock (task #646/F8): bypassed by the recording backend, unused when `mock`
// alone is enabled without a real decommit call site reachable.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
unsafe fn decommit_pages_impl(_base: *mut u8, _start: usize, _end: usize, _kind: DecommitKind) {
    // Miri models no RSS; decommit is a no-op.
}

#[cfg(miri)]
// mock (task #646/F8): see decommit_pages_impl above.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
unsafe fn recommit_pages_impl(_base: *mut u8, _start: usize, _end: usize) -> Result<(), VmemError> {
    Ok(())
}

#[cfg(all(miri, feature = "lazy-commit"))]
// mock (task #646/F8): `try_commit_range`'s real-path branch is compiled out
// under `mock`, so this never gets called.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
unsafe fn commit_range_impl(_base: *mut u8, _start: usize, _end: usize) -> Result<(), VmemError> {
    Ok(())
}

#[cfg(all(miri, feature = "lazy-commit"))]
// mock (task #646/F8): `try_reserve_aligned_lazy`'s real-path branch is
// compiled out under `mock`, so this never gets called.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
fn reserve_aligned_lazy_raw(
    size: usize,
    align: usize,
    _initial_commit: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    reserve_aligned_raw(size, align)
        .map(|(base, reservation, reservation_len)| (base, reservation, reservation_len))
}

#[cfg(all(miri, feature = "huge-pages"))]
fn reserve_aligned_huge_raw(
    size: usize,
    align: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize, bool), VmemError> {
    // Miri has no huge pages; ordinary allocation is observably identical.
    reserve_aligned_raw(size, align).map(|(base, reservation, reservation_len)| {
        (base, reservation, reservation_len, false) // Never huge under miri
    })
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
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
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
