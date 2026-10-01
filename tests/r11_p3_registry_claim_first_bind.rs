//! Compact real GlobalAlloc first-bind path; workers stay bound concurrently.
#![cfg(all(feature = "alloc-global", feature = "internals"))]

use core::alloc::{GlobalAlloc, Layout};
use std::sync::{Arc, Barrier};

use sefer_alloc::registry::bootstrap;
use sefer_alloc::registry::heap_slot::STATE_FREE;
use sefer_alloc::SeferAlloc;

#[test]
fn simultaneous_first_binds_take_distinct_fresh_slots() {
    const WORKERS: usize = 16;
    let allocator = SeferAlloc::new();
    let layout = Layout::from_size_align(128, 16).expect("valid layout");
    let entered = Arc::new(Barrier::new(WORKERS + 1));
    let release = Arc::new(Barrier::new(WORKERS + 1));
    let (ids, high_water, first_chunk, second_chunk) = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..WORKERS)
            .map(|_| {
                let entered = Arc::clone(&entered);
                let release = Arc::clone(&release);
                let allocator = &allocator;
                scope.spawn(move || {
                    // SAFETY: valid layout; block is deallocated below with
                    // the same allocator/layout before the thread exits.
                    let block = unsafe { allocator.alloc(layout) };
                    assert!(!block.is_null());
                    let heap = sefer_alloc::global::tls_heap::current_for_trim()
                        .expect("GlobalAlloc allocation must bind TLS");
                    // SAFETY: this thread retains its LIVE TLS claim.
                    let id = unsafe { (*heap).id() };
                    entered.wait();
                    release.wait();
                    // SAFETY: block was allocated above and remains live.
                    unsafe { allocator.dealloc(block, layout) };
                    id
                })
            })
            .collect();
        entered.wait();
        let high_water = bootstrap::count_for_test();
        let first_chunk = bootstrap::ensure().dbg_chunk_is_materialised(0);
        let second_chunk = bootstrap::ensure().dbg_chunk_is_materialised(1);
        release.wait();
        let ids = workers
            .into_iter()
            .map(|worker| worker.join().expect("worker"))
            .collect::<Vec<_>>();
        (ids, high_water, first_chunk, second_chunk)
    });
    assert_eq!(high_water, WORKERS as u32);
    assert!(first_chunk);
    assert!(!second_chunk);
    let mut sorted = ids;
    sorted.sort_unstable();
    assert_eq!(sorted, (0..WORKERS as u32).collect::<Vec<_>>());
    for id in sorted {
        assert_eq!(bootstrap::ensure().dbg_slot_state(id as usize), STATE_FREE);
    }
}
