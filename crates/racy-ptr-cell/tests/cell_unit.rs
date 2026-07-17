//! Native (non-loom) single-threaded unit tests for [`RacyPtrCell`]:
//! sequential correctness of the fast path, init-once, OOM rollback + retry,
//! and the sentinel/null non-leak — the properties that do not need loom's
//! interleaving explorer.

#![cfg(not(loom))]

use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, Ordering};

use racy_ptr_cell::RacyPtrCell;

#[repr(align(4))]
struct Payload {
    marker: u32,
}

fn leak(marker: u32) -> NonNull<Payload> {
    NonNull::from(Box::leak(Box::new(Payload { marker })))
}

#[test]
fn get_is_none_until_initialised() {
    let cell: RacyPtrCell<Payload> = RacyPtrCell::new();
    assert!(cell.get().is_none());
    assert!(!cell.dbg_is_ready());
}

#[test]
fn init_runs_once_then_fast_path() {
    let cell: RacyPtrCell<Payload> = RacyPtrCell::new();
    let calls = AtomicU32::new(0);

    let p1 = cell
        .get_or_try_init(|| {
            calls.fetch_add(1, Ordering::Relaxed);
            Some(leak(0x1111))
        })
        .unwrap();
    // Second call hits the fast path — no second init.
    let p2 = cell
        .get_or_try_init(|| {
            calls.fetch_add(1, Ordering::Relaxed);
            Some(leak(0x2222))
        })
        .unwrap();

    assert_eq!(p1, p2, "same published pointer");
    assert_eq!(calls.load(Ordering::Relaxed), 1, "init ran exactly once");
    assert!(cell.dbg_is_ready());
    // SAFETY: p1 is the leaked, still-live payload.
    assert_eq!(unsafe { p1.as_ref().marker }, 0x1111);

    // get() agrees.
    assert_eq!(cell.get(), Some(p1));

    // Reclaim the leak.
    unsafe { drop(Box::from_raw(p1.as_ptr())) };
}

#[test]
fn oom_rolls_back_and_retry_succeeds() {
    let cell: RacyPtrCell<Payload> = RacyPtrCell::new();

    // First attempt: init returns None (OOM) → rollback → None returned.
    let first = cell.get_or_try_init(|| None);
    assert!(first.is_none(), "OOM attempt returns None");
    assert!(!cell.dbg_is_ready(), "sentinel rolled back to UNINIT");
    assert!(cell.get().is_none());

    // Retry: now succeeds and publishes.
    let p = cell.get_or_try_init(|| Some(leak(0x9999))).unwrap();
    assert!(cell.dbg_is_ready());
    assert_eq!(cell.get(), Some(p));
    // SAFETY: p is the leaked, still-live payload.
    assert_eq!(unsafe { p.as_ref().marker }, 0x9999);
    unsafe { drop(Box::from_raw(p.as_ptr())) };
}
