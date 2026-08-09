//! Tests for the `fault-injection` feature: real-path commit fault injection,
//! DISTINCT from `mock`. These tests run against the REAL OS backend (no
//! `mock` feature): `reserve_aligned_lazy` performs a genuine reservation and
//! `commit_range` issues genuine `VirtualAlloc`/no-op-Unix commit syscalls —
//! the armed hooks only intercept the specific call(s) under test, proving
//! the fault-injection hook coexists with (and does not replace) the real
//! backend.

// `not(feature = "mock")`: under `mock`, `try_commit_range` is entirely
// replaced by the recording stub (see `crate::mock`'s doc comment) and never
// reaches the real-path hook this file tests — that combination is legal to
// compile (see `--all-features`) but produces a vacuous no-op test, which
// would be worse than not running it. These tests specifically prove the
// hook fires on the REAL backend, so they require `mock` OFF.
#![cfg(all(
    feature = "fault-injection",
    feature = "lazy-commit",
    not(feature = "mock")
))]

use aligned_vmem::fault_injection::{arm_fail_at, arm_fail_next};
use aligned_vmem::{commit_range, reserve_aligned_lazy, PAGE};
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
/// `AtomicU32::fetch_update`. This test arms exactly `TOTAL` failures and
/// spawns `THREADS` real committing threads racing (synchronized to the same
/// instant every round via a `Barrier`) on the SAME already-fully-committed
/// span (`commit_range` on an already-committed range is
/// documented-idempotent — see `commit_range_idempotent_on_already_committed`
/// in `tests/lazy_commit.rs` — so concurrent calls are safe regardless of the
/// fault-injection hook itself), then asserts the observed failure count is
/// EXACTLY `TOTAL`.
///
/// Verification-honesty note (task #718): this test does NOT reliably fail
/// against the pre-fix racy implementation, and that was verified, not
/// assumed. Before landing this fix, a temporarily-reverted
/// `should_fail_commit` (the original separate `load` then `store`) was run
/// against two versions of this test: an earlier unsynchronized 8-thread/
/// 200-call-per-thread design (5 runs, all passed) and this `Barrier`-
/// synchronized 32-thread/200-round design (5 more runs, all passed) — 10
/// runs total, zero failures, against genuinely racy code. The reason: the
/// actual
/// race window is a handful of instructions (a `Relaxed` load immediately
/// followed by a `Relaxed` store, no real work between them), while real OS
/// thread wake-up jitter after a `Barrier::wait()` release is typically
/// microseconds — several orders of magnitude wider than the window it would
/// need to land inside. No amount of thread/round count fixes this on real
/// hardware without either a model checker (`loom`, not currently wired into
/// `aligned-vmem` — see the module doc) or injecting an artificial delay into
/// the very code path under test, which would defeat the point. This test's
/// actual value is therefore narrower than "catches a reintroduced race": it
/// proves the FIX is correct and introduces no regression, deadlock, or
/// miscount under real concurrent load (a genuine positive-path guarantee),
/// and it documents the intended exactly-once-per-armed-failure contract as
/// an executable spec. The actual soundness guarantee for the fix itself
/// rests on `fetch_update` being atomic BY CONSTRUCTION (a single indivisible
/// read-modify-write, per `core::sync::atomic`'s own documented semantics),
/// not on this test's ability to have caught the old bug.
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

    arm_fail_next(TOTAL);
    let barrier = std::sync::Barrier::new(THREADS);

    let failures: u32 = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..THREADS)
            .map(|_| {
                let base = &base;
                let barrier = &barrier;
                scope.spawn(move || {
                    let mut local_failures = 0u32;
                    for _ in 0..ROUNDS {
                        barrier.wait();
                        // SAFETY: `[0, PAGE)` is within the fully-committed
                        // span; recommitting an already-committed range is
                        // documented-idempotent and safe from any thread.
                        let ok = unsafe { commit_range(base.0, 0, PAGE) };
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
        failures, TOTAL,
        "exactly {TOTAL} calls were armed to fail; a torn load-then-store \
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
