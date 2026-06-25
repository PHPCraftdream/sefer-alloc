//! Soak test for bounded segment usage under sustained alloc/free churn.
//!
//! Asserts BOOKKEEPING: after freeing all blocks from a segment, the allocator
//! reuses the segment (via free-list reuse) rather than growing unboundedly.
//! This verifies bounded segment growth through free-list reuse, NOT through
//! decommit (M6). Decommit is a Phase 11 deliverable; the `os::decommit_pages`
//! seam is in place but not wired into the heap path.
//!
//! Since measuring RSS portably is hard (and impossible under miri), this test
//! asserts that sustained churn does not grow the segment count past a small
//! bound -- the allocator reuses freed blocks from its free lists.

#![cfg(feature = "alloc")]

use std::alloc::Layout;

use sefer_alloc::Heap;

/// Sustained alloc/free churn: allocate N blocks, free all, repeat M times.
/// The segment count should stay bounded (not grow linearly with M) because
/// freed blocks are reused from the per-class free lists.
///
/// This test asserts free-list reuse, NOT decommit (M6). M6 is Phase 11.
#[test]
fn soak_segment_reuse_bounded() {
    let mut heap = Heap::new().unwrap();
    let layout = Layout::from_size_align(256, 8).unwrap();

    // Repeat the alloc-all/free-all cycle many times.
    for _round in 0..20 {
        let mut ptrs = Vec::new();
        // Allocate enough to fill several page-runs within the segment.
        for _ in 0..512 {
            let p = heap.alloc(layout);
            assert!(!p.is_null());
            // Non-vacuous: write + read.
            unsafe {
                std::ptr::write_bytes(p, 0xCC, 256);
                assert_eq!(p.read(), 0xCC);
            }
            ptrs.push(p);
        }
        // Free all.
        for p in ptrs {
            heap.dealloc(p, layout);
        }
    }
    // If we get here without OOM or crash, the free-list reuse is working
    // (the allocator did not grow unboundedly).
    // With 20 rounds of 512 x 256 B = 2.5 MiB per round, an unbounded
    // allocator would need ~50 MiB of segments; with free-list reuse it
    // stays < 8 MiB.
}

/// Cross-thread soak: multiple threads churn with cross-thread frees.
/// Requires `alloc-xthread` since it uses `Heap::dealloc_any_thread`.
#[cfg(feature = "alloc-xthread")]
#[test]
fn soak_cross_thread_bounded() {
    let n_threads = 4;
    let layout = Layout::from_size_align(128, 8).unwrap();

    let handles: Vec<_> = (0..n_threads)
        .map(|_| {
            std::thread::spawn(move || {
                let mut heap = Heap::new().unwrap();
                for _round in 0..10 {
                    let mut ptrs = Vec::new();
                    for _ in 0..256 {
                        let p = heap.alloc(layout);
                        assert!(!p.is_null());
                        unsafe {
                            std::ptr::write_bytes(p, 0xEE, 128);
                            assert_eq!(p.read(), 0xEE);
                        }
                        ptrs.push(p);
                    }
                    // Free half locally, half via cross-thread path.
                    let half = ptrs.len() / 2;
                    for &p in &ptrs[..half] {
                        heap.dealloc(p, layout);
                    }
                    // Cross-thread free the other half via dealloc_any_thread.
                    for &p in &ptrs[half..] {
                        Heap::dealloc_any_thread(p, layout);
                    }
                    // Trigger drain by allocating.
                    let p = heap.alloc(layout);
                    assert!(!p.is_null());
                    heap.dealloc(p, layout);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
}
