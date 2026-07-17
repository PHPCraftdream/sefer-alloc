//! Real-path commit fault injection (feature `fault-injection`).
//!
//! Distinct from [`crate::mock`]: `mock` replaces the *entire* backend for
//! commit/decommit/recommit (and short-circuits reservations only for the
//! scripted-failure case) with a thread-local recording stub — a consumer
//! that needs the REAL OS backend under test (real segment reservations, real
//! commit accounting, real page-fault behaviour) cannot use it. This module
//! changes nothing about which backend runs: [`try_commit_range`] always
//! calls the real per-OS `commit_range_impl`. It only splices two armed
//! checks in front of that call so a test can deterministically force a
//! specific call to report `VmemError::last_os_error()` instead of touching
//! the OS — simulating commit-charge exhaustion at an exact point in a real
//! allocation sequence.
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
//! Process-wide `Relaxed` atomics (not thread-local): the intended caller is
//! a single-writer allocator under test (owner-only discipline) where the
//! arming test thread and the committing thread are the same thread, so no
//! cross-thread ordering is required — this matches the hook's prior home in
//! `sefer-alloc`'s `os.rs` exactly.
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
pub fn arm_fail_at(k: u32) {
    FAIL_AT_COUNTER.store(0, Ordering::Relaxed);
    FAIL_AT_TARGET.store(k, Ordering::Relaxed);
}

/// Internal: consult both hooks for the current real commit call. Returns
/// `true` if this call should be forced to fail. Called once per real commit
/// attempt, immediately before the OS syscall.
pub(crate) fn should_fail_commit() -> bool {
    let next = FAIL_NEXT.load(Ordering::Relaxed);
    if next > 0 {
        FAIL_NEXT.store(next - 1, Ordering::Relaxed);
        return true;
    }
    let target = FAIL_AT_TARGET.load(Ordering::Relaxed);
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
