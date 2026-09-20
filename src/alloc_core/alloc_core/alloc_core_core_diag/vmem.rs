//! `internals`-gated vmem bench-internals forwarders of [`AllocCore`] — the
//! Unix/Windows exact-reserve counter reads and the reset hook (mechanical
//! split of the former flat `alloc_core_core_diag.rs`; pure code movement,
//! no behavior changed).

use crate::alloc_core::alloc_core::AllocCore;

/// Sol-F1 (task #563): `internals`-gated — every other `dbg_*` diagnostic
/// hook in this file. See this file's module doc for the full rationale.
#[cfg(feature = "internals")]
impl AllocCore {
    /// MEASUREMENT-ONLY (task #504, F11 step 1): process-wide count of
    /// `aligned_vmem::try_reserve_aligned_exact` attempts (Unix only — always
    /// 0 on Windows/miri). Denominator over
    /// [`dbg_unix_exact_reserve_hits`](Self::dbg_unix_exact_reserve_hits).
    /// See `aligned_vmem::UNIX_EXACT_RESERVE_ATTEMPTS`'s own doc for the full
    /// rationale (settling `docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`
    /// F11's "coin flip … nothing anywhere counts this" Unix fast-path
    /// question with a real number). Relaxed load — diagnostic only. Reads 0
    /// unless `bench-internals` is on.
    #[doc(hidden)]
    #[cfg(feature = "bench-internals")]
    #[must_use]
    pub fn dbg_unix_exact_reserve_attempts() -> u64 {
        aligned_vmem::unix_exact_reserve_attempts()
    }

    /// MEASUREMENT-ONLY (task #504, F11 step 1): the numerator complement of
    /// [`dbg_unix_exact_reserve_attempts`](Self::dbg_unix_exact_reserve_attempts)
    /// — count of those attempts that succeeded (already `align`-aligned, no
    /// fallback over-reserve+trim needed). Reads 0 unless `bench-internals`
    /// is on.
    #[doc(hidden)]
    #[cfg(feature = "bench-internals")]
    #[must_use]
    pub fn dbg_unix_exact_reserve_hits() -> u64 {
        aligned_vmem::unix_exact_reserve_hits()
    }

    /// MEASUREMENT-ONLY (task #504, F11 step 1): process-wide count of
    /// `aligned_vmem::win_reserve_commit` calls (Windows only — always 0 on
    /// Unix/miri). The sum of the single-call fast path (`align <= 64 KiB` and
    /// `commit_len == size`) and the two-call traditional path (everything
    /// else). See
    /// [`WINDOWS_RESERVE_COMMIT_SINGLE_CALLS`] and
    /// [`WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS`] in `aligned_vmem` for the
    /// split-path breakdown, and `aligned_vmem::windows_reserve_commit_single_calls()`
    /// / `aligned_vmem::windows_reserve_commit_two_call_pairs()` for the
    /// individual counters. Reads 0 unless `bench-internals` is on.
    #[doc(hidden)]
    #[cfg(feature = "bench-internals")]
    #[must_use]
    pub fn dbg_windows_reserve_commit_calls() -> u64 {
        aligned_vmem::windows_reserve_commit_calls()
    }

    /// MEASUREMENT-ONLY (task #859): process-wide count of `aligned_vmem::win_reserve_commit`
    /// fast-path calls that used a single syscall (Windows only — always 0 on
    /// Unix/miri). The fast path applies when `align <= 64 KiB` (the Windows allocation granularity — the Windows page size is 4 KiB)
    /// and `commit_len == size`, so `VirtualAlloc(MEM_RESERVE | MEM_COMMIT)` issues one syscall with both
    /// flags. Reads 0 unless `bench-internals` is on.
    #[doc(hidden)]
    #[cfg(feature = "bench-internals")]
    #[must_use]
    pub fn dbg_windows_reserve_commit_single_calls() -> u64 {
        aligned_vmem::windows_reserve_commit_single_calls()
    }

    /// MEASUREMENT-ONLY (task #859): process-wide count of `aligned_vmem::win_reserve_commit`
    /// two-call-path calls (Windows only — always 0 on Unix/miri). The traditional path
    /// applies when `align > 64 KiB` or `commit_len != size`, requiring two syscalls: one `VirtualAlloc(MEM_RESERVE)`
    /// followed by `VirtualAlloc(MEM_COMMIT)` on the aligned region. Reads 0 unless
    /// `bench-internals` is on.
    #[doc(hidden)]
    #[cfg(feature = "bench-internals")]
    #[must_use]
    pub fn dbg_windows_reserve_commit_two_call_pairs() -> u64 {
        aligned_vmem::windows_reserve_commit_two_call_pairs()
    }

    /// MEASUREMENT-ONLY (task #504, F11 step 1): forwards to
    /// `aligned_vmem::reset_bench_internals_counters()`, which as of round-6
    /// (task #882) resets SIX counters total, not four: the four this crate
    /// exposes its own forwarders for
    /// ([`dbg_unix_exact_reserve_attempts`](Self::dbg_unix_exact_reserve_attempts),
    /// [`dbg_unix_exact_reserve_hits`](Self::dbg_unix_exact_reserve_hits),
    /// [`dbg_windows_reserve_commit_single_calls`](Self::dbg_windows_reserve_commit_single_calls),
    /// [`dbg_windows_reserve_commit_two_call_pairs`](Self::dbg_windows_reserve_commit_two_call_pairs)),
    /// plus two macOS-decommit-oracle counters
    /// (`aligned_vmem::unix_madvise_attempts`/`unix_madvise_successes`) this
    /// crate does not currently forward. Test/bench hook only — mirrors
    /// [`dbg_reset_contains_base_tier1_counters`](Self::dbg_reset_contains_base_tier1_counters)'s
    /// established reset-hook convention.
    #[doc(hidden)]
    #[cfg(feature = "bench-internals")]
    pub fn dbg_reset_vmem_bench_internals_counters() {
        aligned_vmem::reset_bench_internals_counters();
    }
}
