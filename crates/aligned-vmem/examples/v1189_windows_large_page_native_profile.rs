//! Task #1189 (P3-b, `docs/reviews/2026-08-19-2148-aligned-vmem-publication-audit-Сол-кодекс.md`
//! §"Производительность", P3 second bullet): a Windows-native profile of the
//! existing large-page counters, cross-referenced against perf item 47 (the
//! two doomed-syscall classes on the same `win_reserve_commit` fast path).
//!
//! The report names three speculative-cost events the single-call large-page
//! path can pay: a failed large-page `VirtualAlloc`, the unconditional
//! ordinary-page retry, a release triggered by post-call misalignment, and a
//! fallthrough to the two-call path. Every one of those events already has a
//! dedicated `bench-internals` counter (`WINDOWS_LARGE_PAGE_RETRY_FAILURES`,
//! `WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES`,
//! `WINDOWS_LARGE_PAGE_PLAIN_FALLBACK_SUCCESSES`,
//! `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS`/`_TWO_CALL_PAIRS`) -- what was
//! missing was a REAL RUN on a real Windows host reading them off, which
//! this example does. Measurement-only: this does not change
//! `win_reserve_commit`'s dispatch logic.
//!
//! Run (single process, on Windows):
//! ```text
//! cargo run -p aligned-vmem --release --features "huge-pages bench-internals" \
//!     --example v1189_windows_large_page_native_profile
//! ```
//!
//! # What this measures, and what it deliberately does not
//!
//! This process almost certainly does NOT hold `SeLockMemoryPrivilege`
//! (granting it requires an administrator to edit local security policy and
//! is not this project's CI/dev-host default), so every `VirtualAlloc(..,
//! MEM_LARGE_PAGES)` call in this run is expected to fail outright with
//! `ERROR_PRIVILEGE_NOT_HELD` -- this is the SAME state
//! `tests/huge_pages.rs`'s `reserve_aligned_huge_2mib_still_two_call_path_unprivileged`
//! already documents and asserts a path-activation oracle against. This
//! example does not attempt to acquire the privilege (out of scope: that
//! needs administrator rights this project's dev/CI hosts are not assumed to
//! have); it reports the REAL counter values for the unprivileged regime,
//! which is the regime every downstream `aligned-vmem` consumer runs under
//! by default, and is therefore the regime item 57's P3-b decision (keep the
//! speculative retry, or gate it behind a privilege pre-check) most needs
//! real numbers for.
//!
//! Every `reserve_aligned_huge(2 MiB, 2 MiB)` call below is expected to hit
//! BOTH `WINDOWS_LARGE_PAGE_PLAIN_FALLBACK_SUCCESSES` (the initial
//! `MEM_LARGE_PAGES` attempt fails with `ERROR_PRIVILEGE_NOT_HELD`, and the
//! unconditional ordinary-page retry succeeds) AND
//! `WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES` (the retry's own `base` is then
//! checked against the 2 MiB alignment `win_reserve_commit`'s single-call
//! fast path assumed the ORIGINAL large-page grant would satisfy -- an
//! ordinary `VirtualAlloc` only guarantees 64 KiB alignment, so it lands
//! outside the requested 2 MiB boundary almost every call). Read
//! `src/os/windows.rs`'s `win_reserve_commit`: the alignment check at line
//! ~180 applies to `base` regardless of WHICH of the two VirtualAlloc calls
//! produced it, so both counters legitimately fire for the SAME call --
//! this is not a double-count bug, it is two different questions
//! ("did the large-page request itself succeed?" / "was whatever base we
//! got aligned?") both answered `no` by the same reservation. Neither
//! signal is `WINDOWS_LARGE_PAGE_RETRY_FAILURES` (that needs the
//! ordinary-page retry to ALSO fail, an OOM-adjacent condition this
//! example does not induce).

use aligned_vmem::{
    reserve_aligned_huge, reset_bench_internals_counters, windows_large_page_alignment_failures,
    windows_large_page_plain_fallback_successes, windows_large_page_retry_failures,
    windows_reserve_commit_single_calls, windows_reserve_commit_two_call_pairs,
};

const MIB: usize = 1024 * 1024;

/// Batch-allocate all `iters` reservations before releasing any of them,
/// mirroring `v20_849_unix_exact_reserve_hit_rate.rs`'s own realistic-regime
/// rationale (an alloc/free/alloc/free loop of the same size would let the
/// kernel hand back the just-freed VA gap every time, which is not a
/// representative sample of a reservation-heavy workload).
fn measure(label: &str, size: usize, iters: usize) {
    reset_bench_internals_counters();
    let mut held = Vec::with_capacity(iters);
    let mut successes = 0usize;
    for _ in 0..iters {
        if let Some(r) = reserve_aligned_huge(size, size) {
            held.push(r);
            successes += 1;
        }
    }
    let single = windows_reserve_commit_single_calls();
    let two_call = windows_reserve_commit_two_call_pairs();
    let align_fail = windows_large_page_alignment_failures();
    let retry_fail = windows_large_page_retry_failures();
    let plain_fallback = windows_large_page_plain_fallback_successes();
    drop(held);

    println!(
        "{label}: size={size} iters={iters} successes={successes} \
         single_call={single} two_call_pairs={two_call} \
         large_page_alignment_failures={align_fail} \
         large_page_retry_failures={retry_fail} \
         large_page_plain_fallback_successes={plain_fallback}"
    );
}

fn main() {
    println!(
        "note: this run measures the UNPRIVILEGED regime (no SeLockMemoryPrivilege \
         acquisition attempted) -- see this file's own module doc for why that is \
         the regime item 57's P3-b decision needs real numbers for."
    );

    // GetLargePageMinimum() == 2 MiB on typical x86_64 Windows: the widened
    // single-call fast-path condition (align <= GetLargePageMinimum()) is
    // satisfied here, so this arm exercises the fast path's own failure
    // taxonomy (item 47's two doomed-syscall classes).
    measure(
        "2 MiB (== GetLargePageMinimum on typical x86_64)",
        2 * MIB,
        8,
    );

    // align > GetLargePageMinimum(): the fast-path condition is false, so
    // this arm never attempts the large-page VirtualAlloc at all -- included
    // as the counter-example confirming the fast path is condition-gated,
    // not size-gated, mirroring `reserve_aligned_huge_4mib_still_two_call_path`.
    measure(
        "4 MiB (> GetLargePageMinimum on typical x86_64)",
        4 * MIB,
        8,
    );

    // A larger batch at the 2 MiB regime, to see whether the failure
    // taxonomy's counts stay proportional to attempt count (expected, since
    // privilege state does not change mid-run) or show any surprise
    // (e.g. an intermittent alignment-lucky hit taking the single-call path).
    measure("2 MiB, larger batch", 2 * MIB, 32);
}
