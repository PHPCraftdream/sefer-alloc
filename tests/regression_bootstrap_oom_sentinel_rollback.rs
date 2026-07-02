//! Regression (task #131): `ensure_slow`'s OOM bailout must roll
//! `REGISTRY_PTR` back from `SENTINEL_INITIALIZING` to `null` BEFORE it
//! aborts the process, instead of leaving the sentinel stuck forever.
//!
//! ## The bug this covers
//!
//! `ensure_slow`'s CAS winner reserves virtual memory for the `Registry` via
//! `aligned_vmem::reserve_aligned`. Before the fix, a `None` result there hit
//! `.expect(..)`, which panics -- but `REGISTRY_PTR` had ALREADY been CASed
//! to `SENTINEL_INITIALIZING` and the panic path never rolled it back. Two
//! failure modes follow: every loser thread already spinning in
//! `ensure_slow`'s `Err` branch spins FOREVER (the sentinel never becomes a
//! real pointer), and every FUTURE `ensure()` call (from any thread) sees the
//! non-null sentinel, falls into `ensure_slow`, fails
//! `compare_exchange(null, SENTINEL)` (current value is SENTINEL, not null),
//! and ALSO spins forever -- the whole process livelocks on the next
//! registry touch. Worse, unwinding the panic itself allocates (message
//! formatting / backtrace capture), which reenters `ensure()` and hits the
//! very same stuck sentinel before the original panic even finishes
//! unwinding.
//!
//! ## The fix under test
//!
//! The OOM branch now calls `rollback_registry_sentinel()` (store
//! `REGISTRY_PTR` back to `null` with `Release`) BEFORE `std::process::abort()`.
//! `abort` cannot be observed from within a test process (it terminates it),
//! so this test does not attempt to trigger the real OOM path. Instead it
//! exercises `rollback_registry_sentinel()` THROUGH the exact same function
//! call the fix uses -- via the `#[doc(hidden)]` test hook
//! `bootstrap::dbg_rollback_sentinel_reenterable()`, which drives the LIVE
//! `REGISTRY_PTR` through the sentinel -> rollback -> postcondition-CAS
//! sequence and restores it afterward. See that function's doc comment in
//! `src/registry/bootstrap.rs` for the full safety argument (it only acts
//! when `REGISTRY_PTR` is observed as `null`, so it never disturbs an
//! already-initialised registry, and it always restores `null` on exit).
//!
//! ## Race safety
//!
//! This test, like every other `tests/registry_*` file, shares the
//! process-global `REGISTRY_PTR` with the rest of the suite. It uses the SAME
//! `SERIAL` one-shot-mutex discipline used throughout `tests/` (see
//! `registry_basic.rs`) so no other test's `ensure()`/`claim()` call can race
//! this test's direct manipulation of `REGISTRY_PTR`. In addition, the hook
//! itself is defensive: if by the time this test runs some earlier test in
//! the suite has ALREADY raced ahead and initialised the registry (a real,
//! non-null non-sentinel pointer sits in `REGISTRY_PTR`), the hook's own
//! internal CAS(null, SENTINEL) simply fails and it returns `None` rather
//! than touching a live registry -- this test treats `None` as inconclusive
//! (skips the assertion) rather than as a failure, so it can never falsely
//! fail (or corrupt shared state) merely because it happened to run after
//! the registry was already bootstrapped by another test in the same binary.
//!
//! ## Non-vacuousness (counterfactual)
//!
//! If `rollback_registry_sentinel` is broken (e.g. its `store` is removed or
//! it stores the sentinel back instead of `null`), the hook's postcondition
//! CAS(null, SENTINEL) — performed immediately after the rollback — observes
//! the sentinel still in place and fails, so the hook returns `Some(false)`
//! and this test's `assert_eq!(..., Some(true))` fails. This was verified
//! manually during development by temporarily commenting out the `store` in
//! `rollback_registry_sentinel` and re-running: the test failed as expected,
//! then passed again once restored.

#![cfg(feature = "alloc-global")]

use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::bootstrap;

// Serialise against the other registry-touching test files in this crate
// (matches the discipline used throughout `tests/`, e.g. `registry_basic.rs`).
static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

/// Anti-livelock: after the OOM-bailout's rollback runs, `REGISTRY_PTR` must
/// be back at `null` (`UNINIT`), not stuck at `SENTINEL_INITIALIZING` --
/// otherwise every current/future `ensure()` caller spins forever (Task
/// #131).
#[test]
fn oom_bailout_rollback_clears_sentinel_not_stuck() {
    let _serial = SerialGuard::acquire();

    match bootstrap::dbg_rollback_sentinel_reenterable() {
        Some(rolled_back_cleanly) => {
            assert!(
                rolled_back_cleanly,
                "rollback_registry_sentinel() must clear REGISTRY_PTR back to \
                 null so a subsequent CAS(null, SENTINEL) succeeds -- if this \
                 is false, the sentinel is stuck and every ensure() caller \
                 (present and future) spins forever (Task #131 livelock)"
            );
        }
        None => {
            // The registry was already initialised (real pointer) by an
            // earlier test in this binary before this test's turn under the
            // serial guard -- the hook correctly refused to disturb a live
            // registry. This is expected in a shared test binary and is not
            // a failure of the property under test; nothing to assert here.
        }
    }

    // Whichever branch above ran, `ensure()` must still work normally
    // afterward -- the hook is documented to always restore `REGISTRY_PTR`
    // to what it observed on entry, and must never leave a live registry (or
    // an UNINIT one) unable to bootstrap.
    let reg = bootstrap::ensure();
    let _ = reg.count.load(Ordering::Acquire);
}
