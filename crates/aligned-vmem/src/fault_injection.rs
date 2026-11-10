//! `fault_injection` provides controlled fault seams for real backend paths.
//! The checked-release pre-syscall hook returns before entering the backend;
//! its backend-result surrogate is consumed INSIDE each checked backend and
//! enters that backend's ordinary result classification. The surrogate skips
//! native release and uses an explicit no-code error; it proves classification
//! propagation only, never OS/kernel refusal. The hook mutex is released before
//! any native operation, and native error capture precedes any later hook lock.
//!
//! Distinct from `crate::mock` (cfg `aligned_vmem_mock`, not necessarily set
//! alongside `fault-injection`): `mock` replaces the *entire* backend for
//! commit/decommit/recommit (and short-circuits reservations only for the
//! scripted-failure case) with a thread-local recording stub — a consumer
//! that needs the REAL OS backend under test (real segment reservations, real
//! commit accounting, real page-fault behaviour) cannot use it. This module
//! changes nothing about which backend runs: [`crate::try_commit_range`]
//! calls the real per-OS `commit_range_impl` — **but only when the
//! `aligned_vmem_mock` cfg is NOT set** (task #1106/L1). Under
//! `aligned_vmem_mock`, `try_commit_range`'s real-path branch — the single
//! call site of `should_fail_commit` — is compiled out and replaced by the
//! mock backend, so the commit hooks are not consulted: the mock's own
//! fault script (`crate::mock::fail_next_commit` etc. — a separate mechanism,
//! compiled only under the mock cfg) takes precedence. Arming commit hooks
//! is inert there; checked-release hooks remain active. CI deliberately builds exactly that combination
//! (`.github/workflows/ci.yml` sets the mock cfg alongside the full feature
//! set including `fault-injection`), so the combination is real, not
//! theoretical; the hooks are `allow(dead_code)` in
//! it rather than rejected, because the mock rows exercise the same feature
//! set and a `compile_error!` on the combination would break them. Outside
//! the mock cfg, this module splices two armed checks in front of the real
//! backend call so a test can deterministically force a
//! specific call to report `VmemError::os_refusal_unknown_code()` (task #713:
//! not `last_os_error()` — no real syscall runs for a simulated fault, so
//! there is no real OS code to report) instead of touching the OS —
//! simulating commit-charge exhaustion at an exact point in a real allocation
//! sequence.
//!
//! task #1219 adds a decommit-side sibling hook with exactly ONE call site of
//! its own: [`arm_fail_next_decommit`], consulted from
//! `dispatch_try_decommit` (`api/decommit.rs`) — the single private dispatch
//! point both fallible decommit entry points (the free `try_decommit` and
//! `Reservation::try_decommit`) funnel through — in front of the real
//! `decommit_pages_impl` call. It exists for the same reason and carries the
//! same mock-caveat as the commit-side hooks (inert under
//! `aligned_vmem_mock`, where the call site is compiled out). Only the
//! fail-next tier exists on the decommit side; no `arm_fail_at`-style k-th
//! hook, because no test has needed one — add it by mirroring
//! [`arm_fail_at`]/`FAULT_STATE` if one ever does.
//!
//! task #53 adds two further fail-next hooks,
//! [`arm_fail_next_advise_range`] and [`arm_fail_next_residency_sample`],
//! consulted in front of the real `madvise(MADV_HUGEPAGE)` / `mincore(2)`
//! call in the respective native arm of
//! `Reservation::try_advise_huge_pages_range` (`api/advise_huge_pages.rs`)
//! and `Reservation::try_sample_residency` (`api/residency/sample.rs`). An
//! armed fire yields `VmemError::os_refusal_unknown_code()` — no syscall
//! ran, so there is no real OS code to report (the task-#713 reasoning
//! above). Inert wherever those arms are compiled out (`aligned_vmem_mock`,
//! miri, non-Linux/Android hosts — hence the wider dead-code cfg on the two
//! consult functions). Only the fail-next tier exists (the same #1219
//! rationale). Hook state is `Mutex`-serialized like `FAULT_STATE`
//! (task #1021): arming, one-shot consumption and re-arm are the races to
//! close, and the counters carry no cross-thread payload, so no ordering
//! semantics are involved. The range-advice hook also injects a pre-syscall
//! failure for the whole-span `MADV_NOHUGEPAGE` control operation.
//!
//! The release pre-syscall hook returns before entering the checked backend.
//! A separate backend-result surrogate is consumed INSIDE the checked backend
//! and reaches its ordinary result classification. It skips native release,
//! supplies an explicit no-code error, and proves classification propagation
//! only—not operating-system or kernel refusal.
//!
//! Two independent, additive commit hooks; release hooks are separately
//! documented as pre-syscall and backend-result surrogates above.
//!
//! Multi-threaded commit hook state is process-wide; see its mutex and atomic
//! notes below. The release hook mutex protects independent remaining counts;
//! the lock is not held over native release or OS-error capture.
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

static FAULT_STATE: Mutex<FaultState> = Mutex::new(FaultState {
    target: 0,
    counter: 0,
});

/// When `> 0`, the next real commit call fails without touching the OS and
/// decrements this counter. `0` disarms. See [`arm_fail_next`].
static FAIL_NEXT: AtomicU32 = AtomicU32::new(0);

/// Arm the "fail the next N real commits" hook. The next `n` calls to the
/// real commit path ([`crate::try_commit_range`] / [`crate::commit_range`])
/// return `Err`/`false` without touching the OS. `n == 0` disarms.
/// Inert under the `aligned_vmem_mock` cfg — see the module doc.
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
/// Inert under the `aligned_vmem_mock` cfg — see the module doc.
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
#[allow(deprecated)] // `fetch_update` is deprecated for `try_update`, which needs a newer MSRV.
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

/// When `> 0`, the next real decommit call fails without touching the OS and
/// decrements this counter. `0` disarms. See [`arm_fail_next_decommit`].
///
/// Separate from [`FAIL_NEXT`]: arming the COMMIT hook must not also fire
/// decommits (and vice versa), so the two tiers keep independent state.
static FAIL_NEXT_DECOMMIT: AtomicU32 = AtomicU32::new(0);

/// Arm the "fail the next N real decommits" hook (task #1219). The next `n`
/// calls through the real decommit dispatch point — `dispatch_try_decommit`
/// (`api/decommit.rs`), reached by BOTH fallible decommit entry points, the
/// free [`crate::try_decommit`] and [`crate::Reservation::try_decommit`] —
/// return `Ok(DecommitOutcome::Refused(VmemError::os_refusal_unknown_code()))`
/// without touching the OS. `n == 0` disarms.
///
/// What an armed hook PROVES when it fires, stated precisely because the
/// decommit-side history is full of overclaims: the `Err(e) =>
/// DecommitOutcome::Refused(e)` mapping arm is reachable from both fallible
/// entry points and constructs the outcome carrying exactly the error the
/// backend layer produced. It does NOT prove an OS refusal — no syscall ran;
/// the no-code sentinel is used for the same task-#713 reason as the
/// commit-side hook. What a caller can NOT learn from this hook: whether any
/// real kernel would refuse any real range — that remains untestable
/// deterministically from `tests/` alone (see
/// `decommit_outcome.rs`'s module doc for the avenues rejected).
///
/// Inert under the `aligned_vmem_mock` cfg — see the module doc. Deliberately
/// NOT consulted by the infallible `decommit`/`decommit_lazy`: both discard
/// the backend outcome by signature, so an injected fault there would have
/// nothing observable to affect.
///
/// Uses `Relaxed` for the same reason as [`arm_fail_next`]: the counter
/// carries no payload to publish across threads.
#[cfg_attr(docsrs, doc(cfg(feature = "fault-injection")))]
pub fn arm_fail_next_decommit(n: u32) {
    FAIL_NEXT_DECOMMIT.store(n, Ordering::Relaxed);
}

/// Internal: consult the decommit-side hook for the current real decommit
/// call. Returns `true` if this call should be forced to fail. Called once
/// per real decommit attempt, immediately before the OS syscall — the single
/// call site is `dispatch_try_decommit`'s `#[cfg(not(aligned_vmem_mock))]`
/// branch (`api/decommit.rs`).
// mock (task #646/F8 shape): under `aligned_vmem_mock` that call site is
// compiled out, so this goes unused whenever the cfg is set alongside
// `fault-injection`. Unlike `should_fail_commit` there is NO
// `fault-injection`-without-`lazy-commit` dead combination to suppress:
// `dispatch_try_decommit` is not feature-gated (decommit is core API), so the
// mock cfg is the only combination that orphans this function.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
#[allow(deprecated)] // `fetch_update` is deprecated for `try_update`, which needs a newer MSRV.
pub(crate) fn should_fail_decommit() -> bool {
    // Same `fetch_update` shape and task #718 rationale as `should_fail_commit`:
    // one atomic read-modify-write, so concurrent callers can neither both
    // fire on one armed failure nor silently lose a decrement.
    FAIL_NEXT_DECOMMIT
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            // `then` (lazy), not `then_some` (eager) — see the matching
            // comment in `should_fail_commit` for the underflow trap.
            (next > 0).then(|| next - 1)
        })
        .is_ok()
}

/// Independent counters for pre-syscall and backend-result surrogate hooks.
/// The mutex protects both counts; it is released before any native operation.
struct ReleaseFaultState {
    pre_syscall_remaining: u32,
    backend_result_remaining: u32,
}

static RELEASE_FAULT_STATE: Mutex<ReleaseFaultState> = Mutex::new(ReleaseFaultState {
    pre_syscall_remaining: 0,
    backend_result_remaining: 0,
});

/// Arm the pre-syscall release failure hook. Fails the next `n` checked
/// releases without calling the backend; `n == 0` disarms this hook.
#[cfg_attr(docsrs, doc(cfg(feature = "fault-injection")))]
pub fn arm_fail_next_release(n: u32) {
    RELEASE_FAULT_STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .pre_syscall_remaining = n;
}

/// Arm a backend-result surrogate. Each of the next `n` checked releases
/// performs no release syscall and classifies a synthetic failure result
/// (-1 on Unix, zero on Windows); the reservation is conservatively leaked.
/// `n == 0` disarms this independent hook.
#[cfg_attr(docsrs, doc(cfg(feature = "fault-injection")))]
pub fn arm_fail_next_release_backend_result(n: u32) {
    RELEASE_FAULT_STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .backend_result_remaining = n;
}

/// Consume a pre-syscall failure count. Lock is not held across native calls.
pub(crate) fn should_fail_release() -> bool {
    let mut state = RELEASE_FAULT_STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if state.pre_syscall_remaining > 0 {
        state.pre_syscall_remaining -= 1;
        true
    } else {
        false
    }
}

/// Consume a backend-result surrogate count. Lock is not held across native calls.
pub(crate) fn should_fail_release_backend_result() -> bool {
    let mut state = RELEASE_FAULT_STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if state.backend_result_remaining > 0 {
        state.backend_result_remaining -= 1;
        true
    } else {
        false
    }
}

/// Remaining one-shot failures for the range huge-page-advice hook
/// (task #53). Mutex-serialized per the `FAULT_STATE` (task #1021) pattern
/// — no cross-thread payload, no ordering semantics.
struct FailNextState {
    /// Remaining one-shot failures (0 = disarmed).
    remaining: u32,
}

static ADVISE_RANGE_FAULT_STATE: Mutex<FailNextState> = Mutex::new(FailNextState { remaining: 0 });
static RESIDENCY_SAMPLE_FAULT_STATE: Mutex<FailNextState> =
    Mutex::new(FailNextState { remaining: 0 });

/// Arm the "fail the next N REAL range-advice backend calls" hook
/// (task #53). The next `n` calls through
/// `Reservation::try_advise_huge_pages_range`'s native arm — the real
/// `madvise(MADV_HUGEPAGE)` path — return
/// `Err(VmemError::os_refusal_unknown_code())` without touching the OS.
/// `n == 0` disarms.
///
/// Consulted only in the native Linux/Android arm, so inert under the
/// `aligned_vmem_mock` cfg, under miri, and on non-Linux/Android hosts
/// (call sites compiled out — see the module doc). The same hook is also
/// consulted before the owning `MADV_NOHUGEPAGE` operation's syscall. Only
/// the fail-next tier exists (no k-th hook). The state is Mutex-serialized
/// per the `FAULT_STATE` (task #1021) pattern; no ordering semantics are
/// involved.
#[cfg_attr(docsrs, doc(cfg(feature = "fault-injection")))]
pub fn arm_fail_next_advise_range(n: u32) {
    let mut state = ADVISE_RANGE_FAULT_STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    state.remaining = n;
}

/// Arm the "fail the next N REAL residency-sample backend calls" hook
/// (task #53). The next `n` calls through
/// `Reservation::try_sample_residency`'s native arm — the real `mincore(2)`
/// path — return `Err(VmemError::os_refusal_unknown_code())` without
/// touching the OS. `n == 0` disarms.
///
/// Same native-arm-only consultation and inertness matrix as
/// [`arm_fail_next_advise_range`].
#[cfg_attr(docsrs, doc(cfg(feature = "fault-injection")))]
pub fn arm_fail_next_residency_sample(n: u32) {
    let mut state = RESIDENCY_SAMPLE_FAULT_STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    state.remaining = n;
}

/// Internal: consult the range-advice hook for the current real backend
/// call. Returns `true` if this call should be forced to fail. Called once
/// before the real `madvise` syscall from range-advice and no-THP operations
/// (`api/advise_huge_pages.rs`), only for native Linux/Android without miri
/// or `aligned_vmem_mock`. Check and decrement happen in the same lock
/// critical section; no TOCTOU between check and decrement.
#[cfg_attr(
    any(
        aligned_vmem_mock,
        miri,
        not(any(target_os = "linux", target_os = "android"))
    ),
    allow(dead_code)
)]
pub(crate) fn should_fail_advise_huge_range() -> bool {
    let mut state = ADVISE_RANGE_FAULT_STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if state.remaining > 0 {
        state.remaining -= 1;
        true
    } else {
        false
    }
}

/// Internal: consult the residency-sample hook for the current real backend
/// call. Same shape, same single-critical-section check-and-decrement, and
/// same only-call-sites-in-the-native-arm caveat as
/// [`should_fail_advise_huge_range`] — the call sites are the native arms
/// of `Reservation::try_sample_residency` (`api/residency/sample.rs`).
#[cfg_attr(
    any(
        aligned_vmem_mock,
        miri,
        not(any(target_os = "linux", target_os = "android"))
    ),
    allow(dead_code)
)]
pub(crate) fn should_fail_residency_sample() -> bool {
    let mut state = RESIDENCY_SAMPLE_FAULT_STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if state.remaining > 0 {
        state.remaining -= 1;
        true
    } else {
        false
    }
}
