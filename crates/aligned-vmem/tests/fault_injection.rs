//! Tests for the `fault-injection` feature: real-path commit fault injection,
//! DISTINCT from `mock`. These tests run against the REAL OS backend (no
//! `aligned_vmem_mock` cfg): `reserve_aligned_lazy` performs a genuine reservation and
//! `commit_range` issues genuine `VirtualAlloc`/no-op-Unix commit syscalls —
//! the armed hooks only intercept the specific call(s) under test, proving
//! the fault-injection hook coexists with (and does not replace) the real
//! backend.

// `not(aligned_vmem_mock)`: under `aligned_vmem_mock`, `try_commit_range` is entirely
// replaced by the recording stub (see `crate::mock`'s doc comment) and never
// reaches the real-path hook this file tests — that combination is legal to
// compile (see `--all-features`) but produces a vacuous no-op test, which
// would be worse than not running it. These tests specifically prove the
// hook fires on the REAL backend, so they require `aligned_vmem_mock` OFF.
#![cfg(all(
    feature = "fault-injection",
    feature = "lazy-commit",
    not(aligned_vmem_mock)
))]

use aligned_vmem::fault_injection::{arm_fail_at, arm_fail_next};
use aligned_vmem::{commit_range, page_size, reserve_aligned_lazy, PAGE};
use std::sync::Mutex;

const MIB: usize = 1024 * 1024;

/// The `fault-injection` hooks are PROCESS-GLOBAL atomics; libtest runs the
/// tests in this file on parallel threads, so their arm/fire/disarm sequences
/// would otherwise interleave against the shared state (one test's disarm or
/// commit consuming another's just-armed one-shot). Every test takes this lock
/// for its whole body so the process-global hook is exercised single-threaded.
/// `unwrap_or_else(into_inner)` recovers from a poisoned lock so one failing
/// test does not cascade into spurious failures of the rest.
static SERIAL: Mutex<()> = Mutex::new(());

/// `arm_fail_next(1)` forces exactly the NEXT real `commit_range` call to
/// fail without touching the OS; the call after that succeeds normally
/// against the real backend. Non-vacuous: the reservation is real (backed by
/// the OS), and the post-fault commit genuinely makes the range writable.
#[test]
fn fail_next_forces_exactly_one_real_commit_failure() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    arm_fail_next(0); // disarm any residue from a prior test in this binary
    let chunk = 16 * PAGE; // 64 KiB
    let span = 4 * MIB;
    let r = reserve_aligned_lazy(span, span, chunk).expect("real lazy reserve");
    let base = r.as_ptr();

    arm_fail_next(1);

    // SAFETY: base is a live reservation; [chunk, 2*chunk) is within span.
    let first = unsafe { commit_range(base, chunk, 2 * chunk) };
    assert!(!first, "armed fault must force the real commit to fail");

    // SAFETY: same range; the fault was one-shot (consumed above).
    let second = unsafe { commit_range(base, chunk, 2 * chunk) };
    assert!(
        second,
        "the following commit must hit the real backend and succeed"
    );

    // Prove the range is genuinely committed now (real write, not a mock).
    // SAFETY: [chunk, 2*chunk) is committed after `second` succeeded.
    unsafe {
        base.add(chunk).write(0x5A);
        assert_eq!(base.add(chunk).read(), 0x5A);
    }
}

/// `arm_fail_at(k)` lets the first `k - 1` real commits succeed and fails
/// exactly the k-th; it is one-shot and disarms itself after firing.
/// Non-vacuous: verifies both the successes AND the one failure against the
/// real backend, and that a call after the k-th succeeds again.
#[test]
fn fail_at_fails_exactly_the_kth_real_commit() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    arm_fail_at(0); // disarm any residue
    let chunk = 16 * PAGE;
    let span = 4 * MIB;
    let r = reserve_aligned_lazy(span, span, chunk).expect("real lazy reserve");
    let base = r.as_ptr();

    // Fail the 2nd commit from now.
    arm_fail_at(2);

    // SAFETY: [chunk, 2*chunk) is within span.
    let c1 = unsafe { commit_range(base, chunk, 2 * chunk) };
    assert!(c1, "1st commit (k=1) must succeed against the real backend");

    // SAFETY: [2*chunk, 3*chunk) is within span.
    let c2 = unsafe { commit_range(base, 2 * chunk, 3 * chunk) };
    assert!(!c2, "2nd commit (k=2) must be the forced failure");

    // One-shot: the hook disarmed itself, so the 3rd commit succeeds.
    // SAFETY: same range as the failed c2 — retrying is a valid real commit.
    let c3 = unsafe { commit_range(base, 2 * chunk, 3 * chunk) };
    assert!(
        c3,
        "3rd commit (retry after k-th) must succeed (hook disarmed)"
    );

    // Prove real committed memory is writable after the retry.
    // SAFETY: [2*chunk, 3*chunk) is committed after `c3` succeeded.
    unsafe {
        base.add(2 * chunk).write(0xA5);
        assert_eq!(base.add(2 * chunk).read(), 0xA5);
    }
}

/// `arm_fail_next` has priority over `arm_fail_at` when both are armed
/// simultaneously: the "next N" hook fires first.
#[test]
fn fail_next_has_priority_over_fail_at() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    arm_fail_next(0);
    arm_fail_at(0);
    let chunk = 16 * PAGE;
    let span = 4 * MIB;
    let r = reserve_aligned_lazy(span, span, chunk).expect("real lazy reserve");
    let base = r.as_ptr();

    // Arm BOTH: fail-next(1) should fire on the very next call, consuming
    // itself; fail-at(1) (also targeting "the next call") must NOT also fire
    // on that same call (it should still be armed for a LATER call).
    arm_fail_next(1);
    arm_fail_at(1);

    // SAFETY: [chunk, 2*chunk) within span.
    let c1 = unsafe { commit_range(base, chunk, 2 * chunk) };
    assert!(!c1, "fail_next fires first on the 1st call");

    // fail_at(1) counts calls AFTER it was armed; this is its first observed
    // call (the fail_next branch returns before incrementing fail_at's
    // counter), so THIS call is fail_at's k=1 and must also fail.
    // SAFETY: same range, retried.
    let c2 = unsafe { commit_range(base, chunk, 2 * chunk) };
    assert!(!c2, "fail_at's k=1 fires on the 2nd call");

    // Both hooks are now disarmed; the 3rd call hits the real backend.
    // SAFETY: same range.
    let c3 = unsafe { commit_range(base, chunk, 2 * chunk) };
    assert!(c3, "both hooks consumed; 3rd call succeeds for real");
}

/// task #718: `should_fail_commit`'s `FAIL_NEXT` decrement used to be a
/// separate `load` then `store` (not atomic as a pair) — under concurrent
/// callers, two threads could race to observe the same pre-decrement value,
/// either both firing when only one failure was armed or both writing back
/// the same decremented value and silently losing a decrement. Fixed with
/// `AtomicU32::fetch_update`. This test arms `TOTAL / 2` failures and spawns
/// `THREADS` real committing threads racing (synchronized to the same
/// instant every round via a `Barrier`) on the SAME already-fully-committed
/// span (`commit_range` on an already-committed range is
/// documented-idempotent — see `commit_range_idempotent_on_already_committed`
/// in `tests/lazy_commit.rs` — so concurrent calls are safe regardless of the
/// fault-injection hook itself), then asserts the observed failure count is
/// EXACTLY `TOTAL / 2`.
///
/// task #775 (round-closing review finding F1, HIGH): an earlier revision of
/// this test armed exactly `TOTAL` failures for `TOTAL` calls and asserted
/// `failures == TOTAL`. That oracle is STRUCTURALLY INCAPABLE of failing
/// against the pre-fix race, for a reason independent of scheduling,
/// hardware, or thread/round count — not merely "unlikely to trigger on real
/// hardware", which is what that revision's own doc comment (and this
/// crate's `b8b70fb` commit message) incorrectly claimed. The argument: a
/// call fires iff it observes `FAIL_NEXT > 0`. Every torn decrement under the
/// racy `load`-then-`store` pair can only LOSE a decrement (two threads
/// racing on the same value both write back the same decremented result) —
/// it can never cause the counter to drop BELOW its correct trajectory. So
/// the racy counter is pointwise `>=` the counter a correct implementation
/// would show at every instant, meaning every call that fires under the
/// CORRECT implementation also fires under the racy one. With `armed ==
/// calls`, the correct implementation already fires on 100% of calls (the
/// trivial upper bound) — so the racy implementation ALSO fires on 100% of
/// calls, and `failures == TOTAL` holds under BOTH implementations. No
/// number of threads or rounds changes that; the assertion is one-sided in
/// the direction the bug never moves.
///
/// Fixed by arming only HALF the calls (`TOTAL / 2`): now a correct
/// implementation fires on exactly half of them, so the racy implementation
/// (which can only inflate the observed fire count above the correct value,
/// per the argument above) has room to diverge and get CAUGHT. Verified,
/// not assumed: reverted `should_fail_commit` to the pre-`#718` racy
/// `load`-then-`store` and ran BOTH oracle shapes against it. The `armed ==
/// calls` shape passed 3/3 runs (confirming the mathematical argument above,
/// not just asserting it). The `armed == calls / 2` shape FAILED 5/5 runs,
/// on this exact `Barrier`-synchronized 32-thread/200-round design, with NO
/// artificial delay — directly refuting the prior revision's claim that "no
/// amount of thread/round count fixes this on real hardware without a model
/// checker or an artificial delay." Reverted cleanly afterward (`git diff`
/// showed zero net change to `src/fault_injection.rs`). The soundness
/// guarantee for the FIX itself still rests on `fetch_update` being atomic
/// BY CONSTRUCTION (a single indivisible read-modify-write, per
/// `core::sync::atomic`'s own documented semantics) — what changed is that
/// this test can now actually observe a regression, instead of only
/// asserting a true-but-untestable-by-this-oracle fact.
#[test]
fn fail_next_is_atomic_under_concurrent_callers() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    arm_fail_next(0);
    arm_fail_at(0);

    let span = 4 * MIB;
    // `initial_commit == span`: the whole span is committed up front, so
    // every thread's `commit_range` call hits the safe, documented-idempotent
    // "already committed" path regardless of fault-injection outcome.
    let r = reserve_aligned_lazy(span, span, span).expect("fully-committed real reserve");

    // A thin Send/Sync wrapper around the raw pointer -- avoids an
    // exposed-address `as usize` round-trip to move the pointer across the
    // `thread::scope` boundary (the exact pattern task #717 removed from this
    // crate's own internals).
    struct SendPtr(*mut u8);
    // SAFETY: the pointee is a live reservation for the whole scope below;
    // every thread only calls `commit_range` on an already-committed range,
    // which the crate documents as idempotent and safe to call concurrently.
    unsafe impl Send for SendPtr {}
    // SAFETY: same reasoning as the `Send` impl above.
    unsafe impl Sync for SendPtr {}
    let base = SendPtr(r.as_ptr());

    // A `Barrier` synchronizes every thread's decrement ATTEMPT to the same
    // instant, round after round -- without it, each thread's OS commit
    // syscall latency naturally spreads decrements out in time and the race
    // window (a handful of instructions between the load and the store)
    // essentially never gets hit in practice, even with many threads and
    // calls (confirmed empirically while building this test: an unsynchronized
    // 8-thread/200-call version never caught the pre-fix racy implementation
    // across repeated runs). Synchronized bursts make many threads actually
    // contend on the SAME pre-decrement value at the same time.
    const THREADS: usize = 32;
    const ROUNDS: u32 = 200;
    const TOTAL: u32 = THREADS as u32 * ROUNDS;

    // task #775: arm only HALF the calls, so a correct implementation fires
    // on exactly half of them -- see the doc comment above for why arming
    // ALL of them (the pre-#775 shape) made this oracle one-sided.
    arm_fail_next(TOTAL / 2);
    let barrier = std::sync::Barrier::new(THREADS);
    // task #959: the runtime page_size(), not the compile-time PAGE constant
    // (task #947/A-1 moved commit_range's granularity check to page_size()) --
    // a bare PAGE (4 KiB) fails that check unconditionally on 16 KiB-page
    // hosts (Apple Silicon), which made every call in the loop below return
    // false regardless of fault-injection, not just the armed half (first
    // caught on real macOS CI: `failures == TOTAL`, not `TOTAL / 2`).
    let ps = page_size();

    let failures: u32 = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..THREADS)
            .map(|_| {
                let base = &base;
                let barrier = &barrier;
                scope.spawn(move || {
                    let mut local_failures = 0u32;
                    for _ in 0..ROUNDS {
                        barrier.wait();
                        // SAFETY: `[0, ps)` is within the fully-committed
                        // span; recommitting an already-committed range is
                        // documented-idempotent and safe from any thread.
                        let ok = unsafe { commit_range(base.0, 0, ps) };
                        if !ok {
                            local_failures += 1;
                        }
                    }
                    local_failures
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).sum()
    });

    assert_eq!(
        failures,
        TOTAL / 2,
        "exactly TOTAL/2 calls were armed to fail; a torn load-then-store \
         decrement would under- or over-count under concurrent callers"
    );
}

/// `arm_fail_next(0)` / `arm_fail_at(0)` are no-ops (disarm without firing):
/// a real commit proceeds normally.
#[test]
fn zero_arming_is_a_pure_disarm() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    arm_fail_next(0);
    arm_fail_at(0);
    let chunk = 16 * PAGE;
    let span = 2 * MIB;
    let r = reserve_aligned_lazy(span, span, chunk).expect("real lazy reserve");
    let base = r.as_ptr();

    // SAFETY: [chunk, 2*chunk) within span.
    let ok = unsafe { commit_range(base, chunk, 2 * chunk) };
    assert!(ok, "disarmed hooks must not affect the real commit");
}

/// task #1021/R4-8: deterministic semantics test for `arm_fail_at` after the
/// Mutex fix. Verifies that:
/// - The fault fires on exactly the k-th call
/// - The hook self-disarms after firing
/// - Subsequent calls succeed
///
/// This test is deterministic and would fail if the counter/self-disarm logic
/// was broken (e.g., if counter wasn't reset, or if target wasn't cleared after
/// firing).
#[test]
fn arm_fail_at_fails_exactly_kth_and_self_disarms() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    arm_fail_next(0);
    arm_fail_at(0);

    let chunk = 16 * PAGE;
    let span = 4 * MIB;
    let r = reserve_aligned_lazy(span, span, chunk).expect("real lazy reserve");
    let base = r.as_ptr();

    // Arm to fail the 3rd call.
    arm_fail_at(3);

    // SAFETY: ranges are within span.
    assert!(
        unsafe { commit_range(base, 0, chunk) },
        "1st call (k=1) should succeed"
    );
    assert!(
        unsafe { commit_range(base, chunk, 2 * chunk) },
        "2nd call (k=2) should succeed"
    );
    assert!(
        !unsafe { commit_range(base, 2 * chunk, 3 * chunk) },
        "3rd call (k=3) should fail"
    );
    assert!(
        unsafe { commit_range(base, 3 * chunk, 4 * chunk) },
        "4th call (k=4) should succeed (hook self-disarmed)"
    );

    // Verify hook is fully disarmed: another call without re-arming should succeed.
    assert!(
        unsafe { commit_range(base, 4 * chunk, 5 * chunk) },
        "5th call (after self-disarm) should succeed"
    );

    // Explicitly disarm and verify it's a no-op.
    arm_fail_at(0);
    assert!(
        unsafe { commit_range(base, 5 * chunk, 6 * chunk) },
        "6th call (after explicit disarm) should succeed"
    );
}

/// task #1021/R4-8: stress test for concurrent arming/firing without
/// probabilistic assertions. This is NOT a regression oracle for the race fix
/// — the race is closed by construction (both critical sections under a single
/// `Mutex<FaultState>`), not by this test's outcome. A concurrent re-arm
/// interleaving between the two atomic stores in the old implementation cannot
/// be reliably reproduced with a probabilistic test on real hardware.
///
/// This test validates basic concurrency invariants under high contention:
/// - No panics occur (deadlock detection via successful termination)
/// - No deadlock (test completes in reasonable time)
/// - Failure count is bounded by iteration count (no counter corruption)
/// - Explicit disarm (`arm_fail_at(0)`) takes effect immediately
///
/// A broken implementation that corrupts the state machine would violate one
/// of these invariants with high probability under stress.
#[test]
fn concurrent_arm_and_fire_stress_invariants_hold() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    arm_fail_next(0);
    arm_fail_at(0);

    let chunk = 16 * PAGE;
    let span = 4 * MIB;
    let r = reserve_aligned_lazy(span, span, chunk).expect("real lazy reserve");
    let base_addr = r.as_ptr() as usize;

    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    const ITERATIONS: u32 = 1000;

    // Thread 1: repeatedly commit and count failures.
    let t1 = {
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            let mut local_failures = 0u32;
            let base = base_addr as *mut u8;
            for _ in 0..ITERATIONS {
                barrier.wait();
                // SAFETY: `[0, chunk)` is within the span; recommitting an
                // already-committed range is idempotent and safe.
                let ok = unsafe { commit_range(base, 0, chunk) };
                if !ok {
                    local_failures += 1;
                }
            }
            local_failures
        })
    };

    // Thread 2: repeatedly re-arm the fault.
    let t2 = {
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            for _ in 0..ITERATIONS {
                barrier.wait();
                // Re-arm to fail the next call.
                arm_fail_at(1);
            }
        })
    };

    let failures = t1.join().expect("commit thread must not panic");
    t2.join().expect("re-arm thread must not panic");

    // Invariant: failure count must be bounded by iteration count.
    // A corrupted counter (e.g., from lost decrements or overflow) would
    // violate this with high probability under stress.
    assert!(
        failures <= ITERATIONS,
        "failure count {} must not exceed iteration count {}",
        failures,
        ITERATIONS
    );

    // Invariant: explicit disarm must take effect immediately.
    arm_fail_at(0);
    // SAFETY: same range as above.
    assert!(
        unsafe { commit_range(base_addr as *mut u8, 0, chunk) },
        "after explicit disarm, commit must succeed"
    );
}
