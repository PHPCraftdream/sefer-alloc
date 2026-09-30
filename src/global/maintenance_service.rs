//! Explicit, process-lifetime ownerless maintenance executor.
//!
//! Lock order: CONTROL is used only for startup/test acknowledgements, never
//! around a registry lease or fallback lock, and never around thread spawning.
//! The worker holds at most one heap lease or the fallback try-lock at a time.
//! Startup's std allocations may use SeferAlloc: startup is not called from
//! GlobalAlloc, and the allocation path never calls startup. Registry/fallback
//! bootstrap is OS-backed and therefore does not recursively allocate via std.
//!
//! The worker is intentionally detached for the process lifetime. There is no
//! shutdown/restart: unwinding or returning aborts instead of silently losing an
//! activated guarantee. OS process exit stops the worker; unloading the code or
//! forking an activated multithreaded process is not supported by this contract.

#[cfg(all(feature = "internals", feature = "bench-internals"))]
use core::sync::atomic::AtomicBool;
#[cfg(feature = "internals")]
use core::sync::atomic::AtomicU64;
use core::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::Duration;

use super::{fallback, MaintenanceStartError};
use crate::registry::HeapRegistry;

const IDLE: u8 = 0;
const STARTING: u8 = 1;
const RUNNING: u8 = 2;
const SLOT_BUDGET: usize = 32;
const PERIOD: Duration = Duration::from_millis(10);
static STATE: AtomicU8 = AtomicU8::new(IDLE);
static CONTROL: Mutex<()> = Mutex::new(());
static CHANGED: Condvar = Condvar::new();

#[cfg(feature = "internals")]
static PASSES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "internals")]
static FALLBACK_VISITS: AtomicU64 = AtomicU64::new(0);
#[cfg(all(feature = "internals", feature = "bench-internals"))]
static FAIL_START: AtomicBool = AtomicBool::new(false);
#[cfg(all(feature = "internals", feature = "bench-internals"))]
static START_PAUSE: AtomicBool = AtomicBool::new(false);
#[cfg(all(feature = "internals", feature = "bench-internals"))]
static START_PAUSED: AtomicBool = AtomicBool::new(false);
#[cfg(all(feature = "internals", feature = "bench-internals"))]
static FAIL_WORKER: AtomicU8 = AtomicU8::new(0);

/// Internal process-global executor. Test-only methods are not stable API.
#[doc(hidden)]
pub struct MaintenanceService;

// No worker exists while this rollback guard is armed. In particular a spawn
// error leaves activation inactive and retryable, not stuck in STARTING.
struct StartingGuard;
impl Drop for StartingGuard {
    fn drop(&mut self) {
        STATE.store(IDLE, Ordering::Release);
    }
}

// An unexpected return is as fatal as an unwind. No formatting, logging or
// allocation is performed by this guard, including under panic = unwind.
struct WorkerGuard;
impl Drop for WorkerGuard {
    fn drop(&mut self) {
        std::process::abort();
    }
}

impl MaintenanceService {
    pub(crate) fn start() -> Result<(), MaintenanceStartError> {
        match STATE.compare_exchange(IDLE, STARTING, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {}
            Err(RUNNING) => return Ok(()),
            Err(_) => return Err(MaintenanceStartError::InProgress),
        }
        let rollback = StartingGuard;
        #[cfg(all(feature = "internals", feature = "bench-internals"))]
        {
            if START_PAUSE.load(Ordering::Acquire) {
                let mut guard = Self::control();
                START_PAUSED.store(true, Ordering::Release);
                CHANGED.notify_all();
                while START_PAUSE.load(Ordering::Acquire) {
                    guard = CHANGED
                        .wait(guard)
                        .unwrap_or_else(|_| std::process::abort());
                }
                START_PAUSED.store(false, Ordering::Release);
            }
        }
        // No locks/lease/TLS mutation are held. std's closure/thread metadata
        // allocation is an ordinary top-level call to the selected allocator,
        // whose bootstrap cannot call this function or spawn another worker.
        let handle = Self::spawn_worker().map_err(MaintenanceStartError::Spawn)?;
        core::mem::forget(rollback);
        // fire-and-forget: detached by design, guarded by terminal abort on
        // every unexpected worker exit. All state/code must live until exit.
        drop(handle);
        let mut guard = Self::control();
        while STATE.load(Ordering::Acquire) != RUNNING {
            guard = CHANGED
                .wait(guard)
                .unwrap_or_else(|_| std::process::abort());
        }
        Ok(())
    }

    fn spawn_worker() -> std::io::Result<std::thread::JoinHandle<()>> {
        // Fault injection shares exactly the OS-error rollback/propagation
        // branch in start(), rather than bypassing it with a separate result.
        #[cfg(all(feature = "internals", feature = "bench-internals"))]
        if FAIL_START.swap(false, Ordering::AcqRel) {
            return Err(std::io::Error::from(std::io::ErrorKind::WouldBlock));
        }
        std::thread::Builder::new().spawn(Self::worker)
    }

    pub(crate) fn running() -> bool {
        STATE.load(Ordering::Acquire) == RUNNING
    }

    fn control() -> MutexGuard<'static, ()> {
        // Poison means an unexpected control-plane panic: never recover a
        // requested guarantee by silently accepting potentially broken state.
        CONTROL.lock().unwrap_or_else(|_| std::process::abort())
    }

    fn worker() {
        let _terminal = WorkerGuard;
        {
            let _guard = Self::control();
            STATE.store(RUNNING, Ordering::Release);
            CHANGED.notify_all();
        }
        let mut cursor = 0;
        loop {
            #[cfg(all(feature = "internals", feature = "bench-internals"))]
            match FAIL_WORKER.load(Ordering::Acquire) {
                1 => return,
                2 => panic!("injected maintenance worker unwind"),
                _ => {}
            }
            let _ = HeapRegistry::maintenance_pass(&mut cursor, SLOT_BUDGET);
            // Uninitialized or paused fallback: skip, never wait or materialize
            // it. A later periodic pass retries independently of notifications.
            let fallback_visited =
                fallback::try_with_heap(|core| core.trim_for_recycle()).is_some();
            #[cfg(feature = "internals")]
            {
                // Test acknowledgements occur only AFTER all leases/locks have
                // gone. Waiting tests do not claim a heap or drive maintenance.
                let _guard = Self::control();
                if fallback_visited {
                    FALLBACK_VISITS.fetch_add(1, Ordering::Relaxed);
                }
                PASSES.fetch_add(1, Ordering::Release);
                CHANGED.notify_all();
            }
            #[cfg(not(feature = "internals"))]
            let _ = fallback_visited;
            // Scheduling cadence, not a takeover timeout or wall-clock SLA.
            // Notifications are unnecessary for liveness; spurious wakes are
            // harmless because every iteration performs a finite bounded pass.
            std::thread::park_timeout(PERIOD);
        }
    }

    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn passes_for_test() -> u64 {
        PASSES.load(Ordering::Acquire)
    }

    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn fallback_visits_for_test() -> u64 {
        FALLBACK_VISITS.load(Ordering::Acquire)
    }

    /// Wait for a real completed worker pass; a timeout is only a test failure
    /// bound, never evidence that any reclaim happened. No allocator call or
    /// artificial worker wakeup is used to obtain the acknowledgement.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn wait_after_for_test(before: u64, timeout: Duration) -> bool {
        let guard = Self::control();
        let (guard, _) = CHANGED
            .wait_timeout_while(guard, timeout, |_| PASSES.load(Ordering::Acquire) <= before)
            .unwrap_or_else(|_| std::process::abort());
        let completed = PASSES.load(Ordering::Acquire) > before;
        drop(guard);
        completed
    }

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn fail_next_start_for_test() {
        FAIL_START.store(true, Ordering::Release);
    }

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn pause_start_for_test(pause: bool) {
        let _guard = Self::control();
        START_PAUSE.store(pause, Ordering::Release);
        CHANGED.notify_all();
    }

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn wait_start_paused_for_test(timeout: Duration) -> bool {
        let guard = Self::control();
        let (guard, _) = CHANGED
            .wait_timeout_while(guard, timeout, |_| !START_PAUSED.load(Ordering::Acquire))
            .unwrap_or_else(|_| std::process::abort());
        let paused = START_PAUSED.load(Ordering::Acquire);
        drop(guard);
        paused
    }

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn fail_worker_for_test(unwind: bool) {
        FAIL_WORKER.store(if unwind { 2 } else { 1 }, Ordering::Release);
    }
}
