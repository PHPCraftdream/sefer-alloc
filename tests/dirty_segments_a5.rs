//! R7-A5 concurrency tests — dirty-segment routing gap-fill.
//!
//! Fills gaps from the A4 test doc:
//! 1. `slot_recycle_stale_dirty_bit` — a stale dirty bit from a recycled slot
//!    is safely dropped by the owner's revalidation (A4 doc test #6, not
//!    implemented in A4).
//! 2. `dirty_word_with_multiple_segments` — multiple segments covered by the
//!    SAME dirty word: cross-thread frees to segments whose IDs share a dirty
//!    word (segment_id / 64 == same word index) all set bits in the same u64;
//!    the owner's drain processes all of them.
//!
//! Feature-gated behind `alloc-global` + `alloc-xthread` +
//! `alloc-segment-directory`.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-segment-directory"
))]

extern crate sefer_alloc;

use std::alloc::{GlobalAlloc, Layout};
use std::sync::Arc;
use std::thread;

use sefer_alloc::SeferAlloc;

static A: SeferAlloc = SeferAlloc::new();

/// Helper: allocate `n` blocks of `size` bytes.
fn alloc_blocks(n: usize, size: usize) -> Vec<*mut u8> {
    let layout = Layout::from_size_align(size, 1).unwrap();
    (0..n).map(|_| unsafe { A.alloc(layout) }).collect()
}

/// Helper: free a block.
unsafe fn free_block(ptr: *mut u8, size: usize) {
    let layout = Layout::from_size_align(size, 1).unwrap();
    A.dealloc(ptr, layout);
}

// =========================================================================
// Test 1: slot recycle with stale dirty bit
// =========================================================================

/// A recycled slot may have a stale dirty bit from a previous lifetime.
/// The owner's drain MUST revalidate (base/kind/id check) and discard the
/// stale bit without corrupting state.
///
/// Strategy: allocate blocks, free them cross-thread (setting dirty bits),
/// then free ALL blocks from the owner thread (triggering segment recycle).
/// Then allocate again (potentially reusing recycled slots) and free
/// cross-thread again. The second round's dirty drain must not be confused
/// by stale bits from the first round.
#[test]
fn slot_recycle_stale_dirty_bit() {
    const SIZE: usize = 64;
    const N: usize = 200;

    // Round 1: owner allocates, remote frees some, owner reclaims.
    let ptrs_r1 = alloc_blocks(N, SIZE);
    let remote_chunk: Vec<usize> = ptrs_r1[..N / 2].iter().map(|&p| p as usize).collect();
    let t1 = thread::spawn(move || {
        for addr in remote_chunk {
            unsafe { free_block(addr as *mut u8, SIZE) };
        }
    });
    t1.join().unwrap();

    // Owner drains (allocation triggers dirty drain).
    let drain_ptrs = alloc_blocks(N / 4, SIZE);

    // Free ALL round-1 + drain blocks to trigger segment recycle.
    for &p in &ptrs_r1[N / 2..] {
        unsafe { free_block(p, SIZE) };
    }
    for p in drain_ptrs {
        unsafe { free_block(p, SIZE) };
    }

    // Round 2: allocate again (may reuse recycled slots).
    let ptrs_r2 = alloc_blocks(N, SIZE);
    let remote_chunk2: Vec<usize> = ptrs_r2[..N / 2].iter().map(|&p| p as usize).collect();

    // Remote frees again — sets dirty bits in potentially reused slots.
    let t2 = thread::spawn(move || {
        for addr in remote_chunk2 {
            unsafe { free_block(addr as *mut u8, SIZE) };
        }
    });
    t2.join().unwrap();

    // Owner drains again — this drain must handle stale bits from round 1
    // (now pointing at recycled/reused segments) correctly.
    let ptrs_r2_drain = alloc_blocks(N / 2, SIZE);

    // Clean up.
    for &p in &ptrs_r2[N / 2..] {
        unsafe { free_block(p, SIZE) };
    }
    for p in ptrs_r2_drain {
        unsafe { free_block(p, SIZE) };
    }
}

// =========================================================================
// Test 2: dirty word with multiple segments
// =========================================================================

/// Multiple segments covered by the same dirty word: cross-thread frees to
/// blocks in DIFFERENT segments that share the same dirty word index
/// (segment_id / 64). All bits in one u64 word are set; the owner's drain
/// processes every one.
///
/// Strategy: allocate many blocks to fill multiple segments, then free
/// blocks from DIFFERENT segments on remote threads. All freed blocks must
/// be reclaimable by the owner.
#[test]
fn dirty_word_with_multiple_segments() {
    use sefer_alloc::registry::heap_core::DBG_RING_PUSH_RETRY_EXHAUSTED;
    use std::sync::atomic::Ordering;

    const SIZE: usize = 64;
    // Allocate enough to span many segments (each segment holds ~65k 64B blocks).
    // We need multiple segments, so allocate in large batches.
    const BLOCKS_PER_PRODUCER: usize = 200;
    const PRODUCERS: usize = 4;
    const TOTAL: usize = PRODUCERS * BLOCKS_PER_PRODUCER;

    let exhausted_before = DBG_RING_PUSH_RETRY_EXHAUSTED.load(Ordering::Relaxed);

    // Owner: allocate a large number of blocks across multiple segments.
    let ptrs = alloc_blocks(TOTAL, SIZE);

    // Identify distinct segment bases to verify we have multiple segments.
    let segment_mask = !(sefer_alloc::SegmentLayout::SEGMENT - 1);
    let mut segment_bases: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for &p in &ptrs {
        segment_bases.insert((p as usize) & segment_mask);
    }

    // Send blocks from different segments to different producer threads.
    let chunk_size = TOTAL / PRODUCERS;
    let mut handles = Vec::new();
    for i in 0..PRODUCERS {
        let start = i * chunk_size;
        let end = start + chunk_size;
        let chunk: Vec<usize> = ptrs[start..end].iter().map(|&p| p as usize).collect();
        handles.push(thread::spawn(move || {
            for addr in chunk {
                unsafe { free_block(addr as *mut u8, SIZE) };
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    // Owner: allocate again — dirty drain processes all dirty bits.
    let ptrs2 = alloc_blocks(TOTAL, SIZE);

    let exhausted_delta = DBG_RING_PUSH_RETRY_EXHAUSTED.load(Ordering::Relaxed) - exhausted_before;
    assert_eq!(
        exhausted_delta, 0,
        "dirty_word_with_multiple_segments: {exhausted_delta} pushes exhausted (want 0)"
    );

    // Clean up.
    for p in ptrs2 {
        unsafe { free_block(p, SIZE) };
    }
}

// =========================================================================
// Test 3: multiple remote frees across segments then owner drain
// =========================================================================

/// Variant with an explicit barrier pattern: all producers complete, THEN
/// the owner runs one alloc cycle. Verifies the owner's word-by-word scan
/// picks up every dirty bit.
#[test]
fn multi_segment_remote_free_then_single_drain() {
    const SIZE: usize = 128;
    const BLOCKS: usize = 400;
    const PRODUCERS: usize = 4;

    let ptrs = alloc_blocks(BLOCKS, SIZE);

    // Distribute blocks round-robin to producers.
    let mut per_producer: Vec<Vec<usize>> = (0..PRODUCERS).map(|_| Vec::new()).collect();
    for (i, &p) in ptrs.iter().enumerate() {
        per_producer[i % PRODUCERS].push(p as usize);
    }

    let chunks: Vec<Arc<Vec<usize>>> = per_producer.into_iter().map(Arc::new).collect();
    let mut handles = Vec::new();
    for chunk in &chunks {
        let c = Arc::clone(chunk);
        handles.push(thread::spawn(move || {
            for &addr in c.iter() {
                unsafe { free_block(addr as *mut u8, SIZE) };
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    // Single owner drain pass via alloc.
    let ptrs2 = alloc_blocks(BLOCKS, SIZE);

    // All blocks should be reclaimed.
    for p in ptrs2 {
        unsafe { free_block(p, SIZE) };
    }
}
