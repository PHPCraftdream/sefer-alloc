//! L4 (task #25) — the fallback spinlock survives a panic inside `with_heap`.
//!
//! ## The hole this guards
//!
//! `fallback::with_heap` used to acquire the spinlock, run the closure, then
//! release it with a plain `release_lock()` call — with NO RAII. A panic inside
//! the closure would skip the release and leave `LOCK == true` FOREVER, so every
//! subsequent pre-TLS / teardown allocation routed to the fallback would spin in
//! `acquire_lock` indefinitely (a permanent deadlock instead of a clean abort).
//! No-panic is a project invariant (HeapCore never panics), but the RAII
//! `LockGuard` makes `with_heap` panic-safe at zero cost.
//!
//! ## What this test does
//!
//! The `#[doc(hidden)]` hook `dbg_panic_in_with_heap_releases_lock`:
//!   1. calls `with_heap` with a closure that PANICS (caught via `catch_unwind`),
//!   2. then calls `with_heap` again with a normal closure,
//!
//! returning `true` iff step 2 COMPLETED (lock not wedged). We run the hook on a
//! watchdog thread with a bounded join timeout: before the guard, step 2 would
//! hang forever, so a regression surfaces as a TIMEOUT here rather than a literal
//! forever-hang in the test process.
//!
//! ## Counterfactual (RED without the guard)
//!
//! Revert `with_heap` to the pre-L4 `acquire_lock(); f(); release_lock();` shape
//! (no `LockGuard`): the panicking first `with_heap` leaves `LOCK == true`, the
//! second `with_heap` spins forever, the watchdog join times out and this test
//! fails. With the guard, step 2 returns immediately and the test passes.

#![cfg(all(feature = "alloc-global", feature = "std"))]

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use sefer_alloc::global::dbg_panic_in_with_heap_releases_lock;

#[test]
fn fallback_lock_not_wedged_by_panic() {
    // Run the hook on a dedicated thread and require it to finish within a
    // generous bound. A wedged lock (regression) never sends → we time out.
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        // The hook itself installs `catch_unwind`, so the panicking closure does
        // not abort this thread; a non-wedged lock lets it return `true`.
        let ok = dbg_panic_in_with_heap_releases_lock();
        let _ = tx.send(ok);
    });

    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(true) => {
            handle.join().expect("watchdog thread panicked");
        }
        Ok(false) => panic!(
            "fallback panic-safety hook reported failure — the first `with_heap` \
             did not panic as expected (test hook broken)"
        ),
        Err(_) => panic!(
            "FALLBACK LOCK WEDGED: a panic inside `with_heap` left `LOCK == true`, \
             so the next `with_heap` spun forever (RAII LockGuard missing / not \
             releasing on unwind)"
        ),
    }
}
