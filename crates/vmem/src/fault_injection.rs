//! Real-path commit fault injection (feature `fault-injection`).
//!
//! Distinct from `crate::mock` (cfg `aligned_vmem_mock`, not necessarily set
//! alongside `fault-injection`): `mock` replaces the *entire* backend for
//! commit/decommit/recommit (and short-circuits reservations only for the
//! scripted-failure case) with a thread-local recording stub — a consumer
//! that needs the REAL OS backend under test (real segment reservations, real
//! commit accounting, real page-fault behaviour) cannot use it. This module
//! changes nothing about which backend runs: [`crate::try_commit_range`]
//! always calls the real per-OS `commit_range_impl`. It only splices two armed
//! checks in front of that call so a test can deterministically force a
//! specific call to report `VmemError::os_refusal_unknown_code()` (task #713:
//! not `last_os_error()` — no real syscall runs for a simulated fault, so
//! there is no real OS code to report) instead of touching the OS —
//! simulating commit-charge exhaustion at an exact point in a real allocation
//! sequence.
//!
//! Two independent, additive hooks (mirrors the two-tier hook that
//! `sefer-alloc` carried before this crate absorbed it):
//! - [`arm_fail_next`]: the next `n` real commit calls fail.
//! - [`arm_fail_at`]: the k-th real commit call from now (1-based) fails;
//!   one-shot, disarms itself after firing.
//!
//! `arm_fail_next`'s "fail next N" is checked first and has priority; when it
//! is disarmed (0), `arm_fail_at`'s "fail the k-th" is checked. Both may be
//! armed simultaneously.
//!
//! Process-wide atomics (not thread-local): a test typically arms a fault
//! from one thread and triggers the committing call from another (e.g. an
//! `alloc-xthread` reclaim test spawning worker threads while the main test
//! thread stays armed), so this module does NOT assume the arming and
//! committing thread are the same (task #718 -- an earlier revision of this
//! doc claimed exactly that "owner-only discipline" assumption and used
//! `Relaxed` throughout on that basis; the assumption does not hold for
//! multi-threaded consumers, so it is not a safe basis for the ordering
//! choice). Concretely: [`arm_fail_at`] now uses a `Mutex<FaultState>` to
//! serialize arming and disarming, closing the concurrent re-arm race
//! (task #1021/R4-8). `FAIL_NEXT`'s decrement uses [`AtomicU32::fetch_update`
//! (a genuine atomic read-modify-write) instead of a separate load then store,
//! which would otherwise race under concurrent callers and lose or duplicate a
//! decrement.
//!
//! Zero cost when the feature is off: this entire module is compiled out
//! (`#[cfg(feature = "fault-injection")]` on the `mod` declaration in
//! `lib.rs`), and the call sites that consult it are themselves
//! `#[cfg(feature = "fault-injection")]`-gated, so the production path is
//! byte-identical with the feature disabled.

use core::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

/// Fault state protected by a mutex to serialize arm/fire operations and
/// prevent concurrent re-arm races (task #1021/R4-8).
struct FaultState {
    /// Target call number for one-shot failure (0 = disarmed).
    target: u32,
    /// Running count of commit calls since last arming.
    counter: u32,
}

static FAULT_STATE: Mutex<FaultState> = Mutex::new(FaultState { target: 0, counter: 0 });

/// When `> 0`, the next real commit call fails without touching the OS and
/// decrements this counter. `0` disarms. See [`arm_fail_next`].
static FAIL_NEXT: AtomicU32 = AtomicU32::new(0);

/// Arm the "fail the next N real commits" hook. The next `n` calls to the
/// real commit path ([`crate::try_commit_range`] / [`crate::commit_range`])
/// return `Err`/`false` without touching the OS. `n == 0` disarms.
///
/// Uses `Relaxed`, not `Release` like `arm_fail_at`: `FAIL_NEXT` carries no
/// payload to publish across threads, so there is nothing a stronger ordering
/// would protect.
///
/// Checked BEFORE [`arm_fail_at`]'s hook (this hook has priority).
#[cfg_attr(docsrs, doc(cfg(feature = "fault-injection")))]
pub fn arm_fail_next(n: u32) {
    FAIL_NEXT.store(n, Ordering::Relaxed);
}

/// Arm the "fail the k-th real commit from now" hook (1-based, one-shot).
/// The k-th call to the real commit path from now fails; calls already
/// consumed by [`arm_fail_next`] are not counted. All other calls
/// (before and after) succeed normally. After firing, the hook disarms
/// itself. `k == 0` disarms without ever firing.
///
/// Resets the internal call counter, so arming always counts from zero.
/// Checked AFTER [`arm_fail_next`]'s hook.
///
/// task #1021/R4-8: This function now uses a Mutex to serialize arming with
/// the self-disarm in `should_fail_commit`, preventing the concurrent re-arm
/// race where a re-arm between the target reset and counter reset would be
/// lost. The earlier two-atomic approach (`FAIL_AT_COUNTER = 0` then
/// `FAIL_AT_TARGET = k`) had a race window between those stores.
#[cfg_attr(docsrs, doc(cfg(feature = "fault-injection")))]
pub fn arm_fail_at(k: u32) {
    let mut state = FAULT_STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.counter = 0;
    state.target = k;
}

/// Internal: consult both hooks for the current real commit call. Returns
/// `true` if this call should be forced to fail. Called once per real commit
/// attempt, immediately before the OS syscall.
// mock (task #646/F8): `try_commit_range`'s `#[cfg(not(aligned_vmem_mock))]`
// branch — the only call site — is compiled out under `aligned_vmem_mock`, so this goes
// unused whenever the `aligned_vmem_mock` cfg is set alongside `fault-injection`.
// fault-injection (task #925/V-21): `try_commit_range` itself is gated on
// `lazy-commit`, so this is unused when `fault-injection` is enabled without
// `lazy-commit`. Suppressed in both specific combinations.
#[cfg_attr(
    any(
        aligned_vmem_mock,
        all(feature = "fault-injection", not(feature = "lazy-commit"))
    ),
    allow(dead_code)
)]
pub(crate) fn should_fail_commit() -> bool {
    // task #718: `fetch_update` performs the load-check-decrement as one
    // atomic read-modify-write, closing the race a separate `load` then
    // `store` had under concurrent callers (two threads could both observe
    // the same pre-decrement value and either both fire when only one
    // failure was armed, or both write back the same decremented value and
    // silently lose a decrement).
    let fired = FAIL_NEXT
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            // `then_some` evaluates its argument EAGERLY (before the call),
            // so `next - 1` would underflow-panic when `next == 0` even
            // though the resulting `Option` would be `None`; `then` with a
            // closure evaluates lazily, only when `next > 0`.
            (next > 0).then(|| next - 1)
        })
        .is_ok();
    if fired {
        return true;
    }

    // task #1021/R4-8: Use Mutex to prevent concurrent re-arm race.
    // The earlier two-atomic approach had a window between resetting
    // FAIL_AT_TARGET and FAIL_AT_COUNTER where a concurrent arm_fail_at
    // could be lost. Holding the mutex across both stores guarantees
    // atomicity.
    let mut state = FAULT_STATE.lock().unwrap_or_else(|e| e.into_inner());
    if state.target > 0 {
        state.counter += 1;
        let call_number = state.counter; // 1-based
        if call_number == state.target {
            // One-shot: disarm after firing. Both stores happen under the
            // mutex, so no concurrent arm can interleave and be lost.
            state.target = 0;
            state.counter = 0;
            return true;
        }
    }
    false
}
