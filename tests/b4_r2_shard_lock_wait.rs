//! B4-R2 — `ShardLock` parks in the OS after the bounded spin phase
//! (WaitOnAddress on Windows, futex on Linux) and wakes the whole queue on a
//! contended unlock.
//!
//! The lock module is included by path (the `#[path]` convention of
//! `tests/r13_shard_guard_auto_traits.rs`), so these tests exercise the real
//! production source. `DBG_OS_WAIT_BLOCKS` is the R30-8 path-activation
//! oracle: it increments once per blocking `wait_contended` call, proving a
//! waiter actually left the bounded spin phase into the OS wait (red against
//! the old pure spin lock, and red-on-value if the wait path never runs).
//!
//! No crate features are required: the file is standalone. On non-OS-wait
//! targets (macOS, miri, loom) only the liveness halves of the assertions
//! run — the unchanged spin+yield tier is the accepted implementation there,
//! so there is no OS-wait path to activate. Every test owns its lock and its
//! flags, so tests tolerate parallel `cargo test` execution.

#[path = "../src/registry/segment_route/shard_lock.rs"]
mod shard_lock;

use core::cell::Cell;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use core::time::Duration;
use shard_lock::ShardLock;

/// Mirrors the module's "OS wait available" predicate without loom feature
/// plumbing: a plain `cargo test` build here never sets `loom`.
const HAS_OS_WAIT: bool = cfg!(all(any(windows, target_os = "linux"), not(miri)));

/// Scoped holder: takes the lock, announces HELD, parks until RELEASE (the
/// `tests/con01_exit_stranded_lock.rs` hold pattern).
fn park_holder<'env>(
    scope: &'env std::thread::Scope<'env, '_>,
    lock: &'env ShardLock<usize>,
    held: &'env AtomicBool,
    release: &'env AtomicBool,
) {
    scope.spawn(move || {
        let _guard = lock.lock();
        held.store(true, Ordering::Release);
        while !release.load(Ordering::Acquire) {
            std::thread::park_timeout(Duration::from_millis(20));
        }
    });
}

fn wait_held(held: &AtomicBool) {
    while !held.load(Ordering::Acquire) {
        std::thread::yield_now();
    }
}

/// Oracle read; the counter only exists on OS-wait targets (cfg(test) +
/// predicate), so non-OS-wait builds compile this to a constant.
fn os_blocks() -> usize {
    #[cfg(any(
        all(windows, not(miri), not(loom)),
        all(
            target_os = "linux",
            any(
                target_arch = "x86_64",
                target_arch = "aarch64",
                target_arch = "riscv64"
            ),
            not(miri),
            not(loom)
        )
    ))]
    {
        shard_lock::DBG_OS_WAIT_BLOCKS.load(Ordering::Relaxed)
    }
    #[cfg(not(any(
        all(windows, not(miri), not(loom)),
        all(
            target_os = "linux",
            any(
                target_arch = "x86_64",
                target_arch = "aarch64",
                target_arch = "riscv64"
            ),
            not(miri),
            not(loom)
        )
    )))]
    {
        0
    }
}

/// Test 1. Path activation + liveness: under a parked holder, every waiter
/// leaves the bounded spin phase into the OS wait (oracle delta >= 1) and
/// every waiter acquires exactly once after release.
#[test]
fn parked_holder_waiters_activate_os_wait_and_all_acquire() {
    let lock = ShardLock::new(0usize);
    let held = AtomicBool::new(false);
    let release = AtomicBool::new(false);
    let go = AtomicBool::new(false);
    std::thread::scope(|scope| {
        park_holder(scope, &lock, &held, &release);
        for _ in 0..3 {
            scope.spawn(|| {
                // GO gates the waiters to AFTER the settle sleep below, so a
                // waiter under parallel-load scheduling cannot start so late
                // that the holder already released (it would then acquire
                // fast-path and legitimately have an oracle delta of 0).
                while !go.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
                // Each waiter measures its own delta on the process-wide
                // oracle counter, so parallel tests cannot corrupt it.
                let before = os_blocks();
                let mut guard = lock
                    .lock_unless_exiting(|| false, || panic!("giveup without exit"))
                    .expect("acquires when not exiting");
                let after = os_blocks();
                // Path-activation oracle: red-by-absence against the old
                // pure spin lock (it never blocks), red-on-value if the wait
                // path never runs.
                if HAS_OS_WAIT {
                    assert!(
                        after - before >= 1,
                        "waiter never entered the OS wait: delta {after}-{before}"
                    );
                }
                *guard += 1;
            });
        }
        wait_held(&held);
        // Let the system settle, then release the waiters: from here each of
        // them has a full second of held lock ahead, far more than the
        // microseconds its 64 spin rounds need, so every waiter reaches the
        // OS wait (delta >= 1) before the release.
        std::thread::sleep(Duration::from_millis(300));
        go.store(true, Ordering::Release);
        std::thread::sleep(Duration::from_millis(1000));
        release.store(true, Ordering::Release);
    });
    assert_eq!(
        *lock.lock(),
        3,
        "every waiter must have acquired exactly once"
    );
}

/// GetThreadTimes, filetime units (100 ns) — test-local FFI, miri is gated
/// by the `#[cfg(windows)]` on the whole test (miri cannot run this FFI).
#[cfg(windows)]
mod thread_cpu {
    #[link(name = "kernel32")]
    extern "system" {
        pub fn GetThreadTimes(
            handle: *mut core::ffi::c_void,
            creation: *mut i64,
            exit_time: *mut i64,
            kernel: *mut i64,
            user: *mut i64,
        ) -> i32;
    }

    // Pseudo-handle for the calling thread (never closed).
    #[link(name = "kernel32")]
    extern "system" {
        pub fn GetCurrentThread() -> *mut core::ffi::c_void;
    }

    /// Sum of kernel+user CPU time for the CALLING thread, in 100 ns units.
    pub fn own_cpu_100ns() -> u64 {
        let (mut c, mut e, mut k, mut u) = (0i64, 0i64, 0i64, 0i64);
        // SAFETY: all four out-pointers are valid i64 locals for the call;
        // GetCurrentThread returns the constant pseudo-handle for the calling
        // thread, which GetThreadTimes accepts; the call only reads them.
        let ok = unsafe { GetThreadTimes(GetCurrentThread(), &mut c, &mut e, &mut k, &mut u) };
        assert_eq!(ok, 1, "GetThreadTimes failed");
        (k as u64).wrapping_add(u as u64)
    }
}

/// Test 2. RED-ON-REVERT: a waiter parked on the OS wait burns (almost) no
/// CPU while the holder sits parked for ~400 ms. A spinning/yielding waiter
/// burns ≈wall CPU; the OS-wait waiter spends microseconds per 4 ms re-arm
/// round. Fails if someone reverts to the pure spin+yield lock.
#[cfg(windows)]
#[test]
fn waiters_do_not_burn_cpu_while_holder_parks() {
    let lock = ShardLock::new(0usize);
    let held = AtomicBool::new(false);
    let release = AtomicBool::new(false);
    std::thread::scope(|scope| {
        park_holder(scope, &lock, &held, &release);
        let waiter = scope.spawn(|| {
            let before_cpu = thread_cpu::own_cpu_100ns();
            let guard = lock
                .lock_unless_exiting(|| false, || panic!("giveup without exit"))
                .expect("acquires when not exiting");
            let after_cpu = thread_cpu::own_cpu_100ns();
            drop(guard);
            // 100 ns units -> ms.
            (after_cpu - before_cpu) as f64 / 10_000.0
        });
        wait_held(&held);
        let wall = std::time::Instant::now();
        std::thread::sleep(Duration::from_millis(400));
        release.store(true, Ordering::Release);
        let waiter_cpu_ms = waiter.join().expect("waiter thread");
        assert!(
            wall.elapsed() >= Duration::from_millis(350),
            "holder window too short for the CPU-burn assertion"
        );
        assert!(
            waiter_cpu_ms < 150.0,
            "waiter burned {waiter_cpu_ms:.1} ms CPU while parked — spin lock reverted?"
        );
    });
}

/// Test 3. Unwinding while holding drops the guard: the release store + wake
/// run, and every waiter queued behind the panicking holder acquires
/// afterwards.
#[test]
fn unwind_while_holding_releases_and_wakes_waiters() {
    let lock = ShardLock::new(0usize);
    let held = AtomicBool::new(false);
    let release = AtomicBool::new(false);
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {})); // keep test output clean
    std::thread::scope(|scope| {
        scope.spawn(|| {
            // The guard is owned INSIDE the panicking closure, so it drops
            // during the unwind that catch_unwind catches — exactly the
            // unwinding-holder case (Drop store + wake must run).
            let _ = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
                let _guard = lock.lock();
                held.store(true, Ordering::Release);
                panic!("unwind while holding");
            }));
            while !release.load(Ordering::Acquire) {
                std::thread::park_timeout(Duration::from_millis(20));
            }
        });
        for _ in 0..2 {
            scope.spawn(|| {
                let mut guard = lock
                    .lock_unless_exiting(|| false, || panic!("giveup without exit"))
                    .expect("acquires when not exiting");
                *guard += 1;
            });
        }
        wait_held(&held);
        std::thread::sleep(Duration::from_millis(300));
        release.store(true, Ordering::Release);
    });
    std::panic::set_hook(prev_hook);
    assert_eq!(*lock.lock(), 2, "both waiters acquired after the unwind");
}

/// Test 4. Exit refusal is checked BEFORE the first blocking wait: with the
/// holder still parked, `exiting() == true` returns None and fires giveup
/// immediately, with no dependence on the release.
#[test]
fn exit_refusal_returns_promptly_while_holder_holds() {
    let lock = ShardLock::new(0usize);
    let held = AtomicBool::new(false);
    let release = AtomicBool::new(false);
    let gaveup = AtomicBool::new(false);
    std::thread::scope(|scope| {
        park_holder(scope, &lock, &held, &release);
        wait_held(&held);
        let result = lock.lock_unless_exiting(|| true, || gaveup.store(true, Ordering::Release));
        assert!(result.is_none(), "exit refusal must return None");
        assert!(gaveup.load(Ordering::Acquire), "giveup must have run");
        release.store(true, Ordering::Release);
    });
}

/// Test 5. Behavior preservation under deep contention (the
/// `tests/r6_t26_cas_spin_ttas_stress.rs` shape, harder): 4 threads x 512
/// alternating rounds plus a sleeper thread that sleeps inside the critical
/// section every ~16 rounds, forcing waiters into the OS-wait phase
/// repeatedly. Mutual exclusion and the exact payload sum must hold.
#[test]
fn stress_mutex_holds_with_deep_contention() {
    static IN_SHARD: AtomicUsize = AtomicUsize::new(0);
    const ROUNDS: usize = 512;
    const THREADS: usize = 4;
    let lock = ShardLock::new(0u64);
    std::thread::scope(|scope| {
        for _ in 0..THREADS {
            scope.spawn(|| {
                for _ in 0..ROUNDS {
                    {
                        let mut guard = lock.lock();
                        let prev = IN_SHARD.swap(1, Ordering::AcqRel);
                        assert_eq!(prev, 0, "mutual exclusion violated");
                        let seen = *guard;
                        *guard = seen.wrapping_add(1);
                        IN_SHARD.store(0, Ordering::Release);
                    }
                    let gave_up = Cell::new(false);
                    let guard = lock.lock_unless_exiting(|| false, || gave_up.set(true));
                    assert!(!gave_up.get(), "refused without exit");
                    let mut guard = guard.expect("acquires when not exiting");
                    let prev = IN_SHARD.swap(1, Ordering::AcqRel);
                    assert_eq!(prev, 0, "mutual exclusion violated");
                    let seen = *guard;
                    *guard = seen.wrapping_add(1);
                    IN_SHARD.store(0, Ordering::Release);
                }
            });
        }
        // Sleeper: every 16th iteration holds the lock for 1 ms inside the
        // critical section, so contending workers run out of spin budget and
        // park in the OS wait repeatedly.
        let mut sleeper_increments = 0u64;
        for round in 0..ROUNDS {
            let mut guard = lock.lock();
            let prev = IN_SHARD.swap(1, Ordering::AcqRel);
            assert_eq!(prev, 0, "mutual exclusion violated");
            if round % 16 == 0 {
                std::thread::sleep(Duration::from_millis(1));
            }
            let seen = *guard;
            *guard = seen.wrapping_add(1);
            sleeper_increments += 1;
            IN_SHARD.store(0, Ordering::Release);
            drop(guard);
        }
        assert_eq!(sleeper_increments, ROUNDS as u64);
    });
    assert_eq!(
        *lock.lock(),
        (THREADS * ROUNDS * 2 + ROUNDS) as u64,
        "payload sum mismatch: two holders raced on the plain counter"
    );
    assert_eq!(IN_SHARD.load(Ordering::Relaxed), 0);
}
