#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use std::sync::mpsc;

use sefer_alloc::global::reclaimer::MaintenanceScheduler;
use sefer_alloc::registry::HeapCore;

#[test]
fn busy_fallback_is_skipped_then_next_pass_acquires_it() {
    assert!(HeapCore::dbg_with_fallback_for_test(|_| ()).is_some());
    let (held_tx, held_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let holder = std::thread::spawn(move || {
        HeapCore::dbg_with_fallback_for_test(|_| {
            held_tx.send(()).expect("announce held lock");
            release_rx.recv().expect("release lock");
        })
        .expect("fallback ready");
    });
    held_rx.recv().expect("lock acquired by holder");

    let busy = MaintenanceScheduler::visit_once(
        |_| panic!("no registry heap was claimed"),
        |_| panic!("busy fallback must not call visitor"),
    );
    assert!(!busy.slot_leased);
    assert!(!busy.fallback_locked);

    release_tx.send(()).expect("release holder");
    holder.join().expect("holder completed");
    let mut called = false;
    let idle = MaintenanceScheduler::visit_once(
        |_| panic!("no registry heap was claimed"),
        |_| called = true,
    );
    assert!(!idle.slot_leased);
    assert!(idle.fallback_locked);
    assert!(called);
}
