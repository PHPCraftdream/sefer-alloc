//! A failed first reservation of chunk 1 must leave fallback allocation and
//! every minted registry index usable. This is one test process: the global
//! OOM switch and high-water mark cannot interfere with another test here.

#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::{GlobalAlloc, Layout};
use std::sync::{Arc, Barrier};

use sefer_alloc::registry::bootstrap;
use sefer_alloc::registry::heap_registry::HeapRegistry;
use sefer_alloc::registry::HeapCore;
use sefer_alloc::SeferAlloc;

struct ChunkOomGuard;

impl Drop for ChunkOomGuard {
    fn drop(&mut self) {
        bootstrap::dbg_set_inject_chunk_oom(false);
    }
}

#[test]
fn claim_chunk_oom_falls_back_and_preserves_minted_indices() {
    let reg = bootstrap::ensure();
    assert_eq!(bootstrap::count_for_test(), 0);

    // Keep chunk 0's 64 slots LIVE, so the next real claim must first touch
    // chunk 1. No churn or pressure is needed to reproduce this boundary.
    let mut held = Vec::with_capacity(64);
    for expected in 0..64 {
        let heap = HeapRegistry::claim();
        assert!(!heap.is_null());
        // SAFETY: claim returned a LIVE, initialised HeapCore until recycle.
        assert_eq!(unsafe { (*heap).id() }, expected);
        held.push(heap as usize);
    }
    assert_eq!(bootstrap::count_for_test(), 64);
    assert!(!reg.dbg_chunk_is_materialised(1));

    let layout = Layout::from_size_align(128, 16).unwrap();
    let reserved = HeapCore::dbg_with_fallback_for_test(|heap| {
        let block = heap.alloc(layout);
        assert!(!block.is_null());
        // SAFETY: this exact block/layout pair was just allocated by heap.
        unsafe { heap.dealloc(block, layout) };
        block as usize
    })
    .expect("fallback heap must already be initialised");

    let allocator = SeferAlloc::new();
    let barrier = Arc::new(Barrier::new(3));
    let _oom_guard = ChunkOomGuard;
    bootstrap::dbg_set_inject_chunk_oom(true);
    let blocks = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                let allocator = &allocator;
                scope.spawn(move || {
                    barrier.wait();
                    // SAFETY: layout is valid; the returned pointer is kept
                    // live until the parent deallocates it with this layout.
                    let block = unsafe { allocator.alloc(layout) };
                    assert!(!block.is_null(), "fallback must serve chunk OOM");
                    (
                        block as usize,
                        sefer_alloc::global::tls_heap::current_for_trim().is_none(),
                    )
                })
            })
            .collect();
        barrier.wait();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(blocks.iter().all(|(_, unbound)| *unbound));
    assert_ne!(blocks[0].0, blocks[1].0);
    assert!(blocks.iter().any(|(block, _)| *block == reserved));
    for (block, _) in blocks {
        // SAFETY: each block was allocated above with this allocator/layout
        // and has not yet been deallocated.
        unsafe { allocator.dealloc(block as *mut u8, layout) };
    }

    assert!(!reg.dbg_chunk_is_materialised(1));
    let minted = bootstrap::count_for_test();
    assert!(
        (65..=66).contains(&minted),
        "two claimers may share one index"
    );
    bootstrap::dbg_set_inject_chunk_oom(false);

    // Hold each successful claim LIVE. Every index minted during the OOM
    // race must be recovered before count is allowed to grow again.
    let mut recovered = Vec::new();
    for _ in 64..minted {
        let heap = HeapRegistry::claim();
        assert!(!heap.is_null());
        // SAFETY: claim returned a LIVE, initialised HeapCore.
        let idx = unsafe { (*heap).id() };
        assert!((64..minted).contains(&idx));
        recovered.push(heap as usize);
    }
    assert_eq!(bootstrap::count_for_test(), minted);
    assert!(reg.dbg_chunk_is_materialised(1));
    let mut recovered_ids = recovered
        .iter()
        .map(|&heap| {
            // SAFETY: each pointer is a distinct LIVE claim above.
            unsafe { (*(heap as *mut HeapCore)).id() }
        })
        .collect::<Vec<_>>();
    recovered_ids.sort_unstable();
    assert_eq!(recovered_ids, (64..minted).collect::<Vec<_>>());
    for heap in recovered {
        // SAFETY: this LIVE pointer has not been recycled yet.
        unsafe { HeapRegistry::recycle(heap as *mut _) };
    }
    for heap in held {
        // SAFETY: this LIVE pointer has not been recycled yet.
        unsafe { HeapRegistry::recycle(heap as *mut _) };
    }
}
