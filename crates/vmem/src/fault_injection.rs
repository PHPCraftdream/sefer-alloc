//! Real-path commit fault injection (feature `fault-injection`).
//!
//! Distinct from `crate::mock` (feature `mock`, not necessarily enabled
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
//! choice). Concretely: [`arm_fail_at`]'s counter-reset-then-target-store is
//! a payload-then-flag publish (the reset is the "payload", the target store
//! is the "flag" that makes a fresh arming visible) and needs a
//! Release/Acquire pair, not `Relaxed`, to guarantee a reader that observes
//! the flag also observes the payload -- see [`arm_fail_at`] and
//! [`should_fail_commit`]'s doc comments for the exact pairing. [`FAIL_NEXT`]'s
//! decrement uses [`AtomicU32::fetch_update`] (a genuine atomic
//! read-modify-write) instead of a separate load then store, which would
//! otherwise race under concurrent callers and lose or duplicate a
//! decrement.
//!
//! Zero cost when the feature is off: this entire module is compiled out
//! (`#[cfg(feature = "fault-injection")]` on the `mod` declaration in
//! `lib.rs`), and the call sites that consult it are themselves
//! `#[cfg(feature = "fault-injection")]`-gated, so the production path is
//! byte-identical with the feature disabled.

use core::sync::atomic::{AtomicU32, Ordering};

/// When `> 0`, the next real commit call fails without touching the OS and
/// decrements this counter. `0` disarms. See [`arm_fail_next`].
static FAIL_NEXT: AtomicU32 = AtomicU32::new(0);

/// When `> 0`, [`FAIL_AT_COUNTER`] counts real commit calls; when the counter
/// reaches this target, that call fails and the target resets to 0
/// (one-shot). See [`arm_fail_at`].
static FAIL_AT_TARGET: AtomicU32 = AtomicU32::new(0);

/// Running count of real commit calls since the last [`arm_fail_at`] call.
static FAIL_AT_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Arm the "fail the next N real commits" hook. The next `n` calls to the
/// real commit path ([`crate::try_commit_range`] / [`crate::commit_range`])
/// return `Err`/`false` without touching the OS. `n == 0` disarms.
///
/// Checked BEFORE [`arm_fail_at`]'s hook (this hook has priority).
pub fn arm_fail_next(n: u32) {
    FAIL_NEXT.store(n, Ordering::Relaxed);
}

/// Arm the "fail the k-th real commit from now" hook (1-based, one-shot).
/// The k-th call to the real commit path from now fails; all other calls
/// (before and after) succeed normally. After firing, the hook disarms
/// itself. `k == 0` disarms without ever firing.
///
/// Resets the internal call counter, so arming always counts from zero.
/// Checked AFTER [`arm_fail_next`]'s hook.
///
/// task #718: the counter reset is the "payload" and the target store is the
/// "flag" a reader gates on ([`should_fail_commit`] only inspects
/// [`FAIL_AT_COUNTER`] once it has observed [`FAIL_AT_TARGET`] `> 0`) — a
/// `Release` store here, paired with the `Acquire` load there, guarantees a
/// reader that observes a freshly-armed target also observes the zeroed
/// counter, even when the arming and committing calls run on different
/// threads.
pub fn arm_fail_at(k: u32) {
    FAIL_AT_COUNTER.store(0, Ordering::Relaxed);
    FAIL_AT_TARGET.store(k, Ordering::Release);
}

/// Internal: consult both hooks for the current real commit call. Returns
/// `true` if this call should be forced to fail. Called once per real commit
/// attempt, immediately before the OS syscall.
// mock (task #646/F8): `try_commit_range`'s `#[cfg(not(feature = "mock"))]`
// branch — the only call site — is compiled out under `mock`, so this goes
// unused whenever `mock` is enabled alongside `fault-injection`.
#[cfg_attr(feature = "mock", allow(dead_code))]
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
    // task #718: `Acquire` pairs with `arm_fail_at`'s `Release` store on
    // `FAIL_AT_TARGET` — see that function's doc comment.
    let target = FAIL_AT_TARGET.load(Ordering::Acquire);
    if target > 0 {
        let prev = FAIL_AT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let call_number = prev + 1; // 1-based
        if call_number == target {
            // One-shot: disarm after firing.
            FAIL_AT_TARGET.store(0, Ordering::Relaxed);
            FAIL_AT_COUNTER.store(0, Ordering::Relaxed);
            return true;
        }
    }
    false
}
