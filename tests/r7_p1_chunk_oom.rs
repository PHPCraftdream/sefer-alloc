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
use sefer_alloc::registry::heap_registry::{
    dbg_bump_count_without_materialising, dbg_slot_initialised, HeapRegistry,
};
use sefer_alloc::registry::heap_slot::{STATE_EMPTY, STATE_LIVE};
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
        let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
        assert_eq!(lease.core().id(), expected);
        held.push(lease);
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
    let raced_count = bootstrap::count_for_test();
    assert!(
        (65..=66).contains(&raced_count),
        "two claimers may share one index"
    );

    // Model one claimant delayed between bump and chunk resolution. This
    // guarantees an older unhinted OOM index for either worker schedule.
    let delayed = dbg_bump_count_without_materialising().expect("delayed minted index");
    assert_eq!(delayed, raced_count);
    assert!(!bootstrap::dbg_slot_or_none(delayed as usize));
    let minted = bootstrap::count_for_test();
    assert_eq!(minted, raced_count + 1);
    assert!(!reg.dbg_chunk_is_materialised(1));
    bootstrap::dbg_set_inject_chunk_oom(false);

    let mut hinted = HeapRegistry::dbg_claim_lease().expect("claim");
    // SAFETY: hinted is this test's exclusively owned LIVE claim.
    let hinted_id = hinted.core().id();
    assert!((64..raced_count).contains(&hinted_id));
    assert_eq!(bootstrap::count_for_test(), minted, "hint precedes bump");
    assert!(reg.dbg_chunk_is_materialised(1));

    // Independent slot census: materializing the chunk did not initialize
    // or claim the displaced minted indices.
    let mut pending: Vec<_> = (64..minted).filter(|&idx| idx != hinted_id).collect();
    assert!(!pending.is_empty());
    for &idx in &pending {
        assert_eq!(reg.dbg_slot_state(idx as usize), STATE_EMPTY);
        assert_eq!(reg.dbg_slot_generation(idx as usize), 0);
        assert!(!dbg_slot_initialised(idx));
    }

    let mut fresh = HeapRegistry::dbg_claim_lease().expect("claim");
    // SAFETY: fresh is a distinct LIVE claim retained through the cold scan.
    assert_eq!(fresh.core().id(), minted);
    assert_eq!(bootstrap::count_for_test(), minted + 1);
    for &idx in &pending {
        assert_eq!(reg.dbg_slot_state(idx as usize), STATE_EMPTY);
        assert!(!dbg_slot_initialised(idx));
    }

    // Reach the logical cap with numeric indices only. The cold scanner
    // must recover every displaced index in the already materialized chunk.
    for expected in minted + 1..bootstrap::MAX_HEAPS as u32 {
        assert_eq!(dbg_bump_count_without_materialising(), Some(expected));
    }
    assert_eq!(dbg_bump_count_without_materialising(), None);
    assert_eq!(bootstrap::count_for_test(), bootstrap::MAX_HEAPS as u32);
    let mut recovered: Vec<_> = vec![hinted, fresh];
    let pending_count = pending.len();
    for _ in 0..pending_count {
        let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
        // SAFETY: each successful claim is retained LIVE until cleanup.
        let idx = lease.core().id();
        let position = pending
            .iter()
            .position(|&expected| expected == idx)
            .expect("cold claim must recover a displaced minted index");
        pending.remove(position);
        recovered.push(lease);
        assert_eq!(bootstrap::count_for_test(), bootstrap::MAX_HEAPS as u32);
    }
    assert!(pending.is_empty(), "no minted OOM index was lost");
    for idx in 64..=minted {
        assert_eq!(reg.dbg_slot_state(idx as usize), STATE_LIVE);
        assert_eq!(reg.dbg_slot_generation(idx as usize), 1);
        assert!(dbg_slot_initialised(idx));
    }
    for chunk in 2..bootstrap::dbg_num_chunks() {
        assert!(
            !reg.dbg_chunk_is_materialised(chunk),
            "numeric cap must not allocate chunks"
        );
    }
    drop(recovered);
    drop(held);
}
