//! Regression test for `SyncRegion::remove` reentrant Drop safety.
//!
//! `SyncRegion::remove` releases the write guard before returning `T`,
//! so a reentrant `Drop` implementation of the returned value does NOT
//! deadlock on the `SyncRegion`'s lock. This test verifies that property
//! with explicit timeout protection against a deadlocking regression.
//!
//! `SyncRegion` is `std`-only -- this whole file is a no-op without it.

#![cfg(feature = "std")]

use sefer_region::{Handle, SyncRegion};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

/// A value whose `Drop` re-enters the same `SyncRegion` by removing its
/// own (by-then-stale) handle -- the strongest form of this test: the
/// entry recursively targets itself, so if the write guard were still
/// held when `Drop` ran, this would deadlock immediately. Holds the
/// `SyncRegion` via `Arc` (not `&SyncRegion`) so the value's own lifetime
/// does not have to be provably shorter than the `SyncRegion`'s -- the
/// value lives INSIDE the `SyncRegion` it references, which a plain
/// borrow cannot express without upsetting the borrow checker.
struct ReentrantDrop {
    sync_region: Arc<SyncRegion<ReentrantDrop>>,
    handle: Option<Handle<ReentrantDrop>>,
    dropped_flag: Arc<AtomicBool>,
}

impl Drop for ReentrantDrop {
    fn drop(&mut self) {
        self.dropped_flag.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle {
            // This must NOT deadlock: the write guard from the `remove()`
            // call that is dropping `self` was already released before
            // this `Drop` ran. By this point `h` is stale (the entry has
            // already been removed), so this resolves to `None` (I2) --
            // the point of the call is that it returns at all.
            let _ = self.sync_region.remove(h);
        }
    }
}

#[test]
fn remove_releases_guard_before_drop() {
    // Run the deadlock-sensitive operation in a separate thread with timeout.
    // A real regression (guard held across drop) would deadlock; without timeout,
    // the entire test suite would hang forever.
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        let sync_region: Arc<SyncRegion<ReentrantDrop>> = Arc::new(SyncRegion::new());
        let dropped_flag = Arc::new(AtomicBool::new(false));

        let value = ReentrantDrop {
            sync_region: Arc::clone(&sync_region),
            handle: None, // no handle yet -- filled in below, once known
            dropped_flag: Arc::clone(&dropped_flag),
        };
        let handle = sync_region.insert(value);

        // Point the value at its own handle so `Drop` can re-enter on itself.
        if let Some(inner) = sync_region.write().get_mut(handle) {
            inner.handle = Some(handle);
        }

        // Remove the value:
        // 1. Acquire write guard.
        // 2. `Region::remove(handle)` extracts the value (no Drop yet).
        // 3. Release write guard (the guard is a temporary of the tail
        //    expression, dropped before `removed` is returned to the caller).
        // 4. Return the value to the caller as `removed`.
        let removed = sync_region.remove(handle);
        assert!(removed.is_some(), "remove should have returned the value");

        // Drop the returned value explicitly -- this is where `Drop::drop`
        // (and the reentrant `remove` call inside it) actually fires.
        drop(removed);
        assert!(dropped_flag.load(Ordering::SeqCst), "Drop should have run");

        // If we reach here at all, no deadlock occurred (a real regression
        // would hang this thread, which recv_timeout will catch below).

        // Confirm the SyncRegion is still fully usable afterward.
        let second = sync_region.insert(ReentrantDrop {
            sync_region: Arc::clone(&sync_region),
            handle: None,
            dropped_flag: Arc::clone(&dropped_flag),
        });
        assert!(sync_region.contains(second));
        assert_eq!(sync_region.len(), 1);

        // Break the reference cycle by removing the second value.
        // Without this, the `Arc<SyncRegion>` in the value holds a strong
        // reference to the region, and the region holds the value, creating
        // a cycle that is never dropped. Verify RAII completion by asserting
        // `Arc::try_unwrap` succeeds.
        sync_region.remove(second);
        let _ = Arc::try_unwrap(sync_region).expect("all strong references dropped");
        tx.send(()).expect("channel send failed");
    });

    // Timeout after 5 seconds -- the operation should complete in milliseconds.
    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(()) => {
            handle.join().expect("thread panicked");
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("DEADLOCK REGRESSION: remove() did NOT release guard before Drop");
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("thread panicked without sending result");
        }
    }
}
