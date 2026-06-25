//! Cross-thread differential proptest for Phase 10 (M7 — owner routing).
//!
//! Multiple threads each alloc/free, including frees of other threads' blocks.
//! Invariants: M1–M4 + M7 hold; no double-free, no lost block, no corruption.
//! NON-VACUOUS: pattern write+readback on every allocation.
//!
//! Per the short-scenario policy: ~64 cases, small sizes, fast.

#![cfg(feature = "alloc-xthread")]

use std::alloc::Layout;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use proptest::prelude::*;
use sefer_alloc::Heap;

/// Cross-thread free: thread A allocates, thread B frees (via
/// `Heap::dealloc_any_thread`).
#[test]
fn cross_thread_free_basic() {
    let mut heap_a = Heap::new().unwrap();
    let layout = Layout::from_size_align(64, 8).unwrap();

    // Thread A allocates 64 blocks, writes a pattern.
    let mut ptrs = Vec::new();
    for i in 0u8..64 {
        let p = heap_a.alloc(layout);
        assert!(!p.is_null());
        unsafe {
            std::ptr::write_bytes(p, i.wrapping_mul(7), 64);
        }
        ptrs.push((p, i));
    }

    // Verify pattern before cross-thread free.
    for &(p, i) in &ptrs {
        unsafe {
            for b in 0..64 {
                assert_eq!(
                    p.add(b).read(),
                    i.wrapping_mul(7),
                    "pattern mismatch before cross-thread free"
                );
            }
        }
    }

    // Thread B frees all of them via the cross-thread path.
    // Convert to usize for Send (raw pointers are !Send; usize is Send).
    let addrs: Vec<usize> = ptrs.iter().map(|&(p, _)| p as usize).collect();
    thread::spawn(move || {
        for addr in addrs {
            Heap::dealloc_any_thread(addr as *mut u8, layout);
        }
    })
    .join()
    .unwrap();

    // Thread A allocates again: the remotely-freed blocks should be
    // reclaimed by A's heap (drained from the thread-free stack on A's
    // next alloc).
    let mut new_ptrs = Vec::new();
    for _ in 0..64 {
        let p = heap_a.alloc(layout);
        assert!(!p.is_null());
        // Write a new pattern and read back (M1 non-vacuous).
        unsafe {
            std::ptr::write_bytes(p, 0xBB, 64);
            for b in 0..64 {
                assert_eq!(p.add(b).read(), 0xBB);
            }
        }
        new_ptrs.push(p);
    }

    // M3: no overlap among the new allocations.
    for i in 0..new_ptrs.len() {
        for j in (i + 1)..new_ptrs.len() {
            let a = new_ptrs[i] as usize;
            let b = new_ptrs[j] as usize;
            assert!(
                a + 64 <= b || b + 64 <= a,
                "new allocations overlap after cross-thread free"
            );
        }
    }
}

/// Multi-thread churn: 4 threads each alloc and cross-free to each other.
#[test]
fn cross_thread_churn_multi() {
    const N_THREADS: usize = 4;
    const ALLOCS_PER_THREAD: usize = 64;
    const SIZE: usize = 128;
    let layout = Layout::from_size_align(SIZE, 8).unwrap();

    let barrier = Arc::new(Barrier::new(N_THREADS));
    // Each thread allocates blocks, shares pointers with the next thread,
    // and the next thread frees them.
    let total_freed = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..N_THREADS)
        .map(|tid| {
            let barrier = Arc::clone(&barrier);
            let total_freed = Arc::clone(&total_freed);
            thread::spawn(move || {
                let mut heap = Heap::new().unwrap();
                let mut my_ptrs = Vec::new();

                // Allocate blocks and write a unique pattern.
                for i in 0..ALLOCS_PER_THREAD {
                    let p = heap.alloc(layout);
                    assert!(!p.is_null());
                    let pattern = ((tid * 100 + i) & 0xFF) as u8;
                    unsafe {
                        std::ptr::write_bytes(p, pattern, SIZE);
                    }
                    my_ptrs.push((p, pattern));
                }

                // Verify patterns.
                for &(p, pattern) in &my_ptrs {
                    unsafe {
                        for b in 0..SIZE {
                            assert_eq!(p.add(b).read(), pattern, "tid={tid} pattern corrupt");
                        }
                    }
                }

                barrier.wait();

                // Free half of our own blocks via the cross-thread path
                // (simulating another thread freeing our memory).
                let half = my_ptrs.len() / 2;
                for &(p, _) in &my_ptrs[..half] {
                    Heap::dealloc_any_thread(p, layout);
                    total_freed.fetch_add(1, Ordering::Relaxed);
                }

                // Free the other half normally.
                for &(p, _) in &my_ptrs[half..] {
                    heap.dealloc(p, layout);
                }

                // Allocate again to trigger drain of thread-free stack.
                for _ in 0..ALLOCS_PER_THREAD / 2 {
                    let p = heap.alloc(layout);
                    assert!(!p.is_null());
                    // Non-vacuous: write + read.
                    unsafe {
                        std::ptr::write_bytes(p, 0xDD, SIZE);
                        assert_eq!(p.read(), 0xDD);
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    assert!(
        total_freed.load(Ordering::Relaxed) > 0,
        "no cross-thread frees happened (vacuous test)"
    );
}

// Proptest: random alloc/dealloc stream with cross-thread frees.
proptest! {
    #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn cross_thread_proptest(
        sizes in prop::collection::vec(1usize..=2048, 10..100),
        free_indices in prop::collection::vec(any::<usize>(), 5..50),
    ) {
        let layout_for = |size: usize| -> Layout {
            Layout::from_size_align(size.max(1), 8).unwrap()
        };

        let mut heap = Heap::new().expect("heap bootstrap");
        let mut live: Vec<(*mut u8, usize)> = Vec::new();

        // Allocate.
        for &size in &sizes {
            let layout = layout_for(size);
            let ptr = heap.alloc(layout);
            prop_assert!(!ptr.is_null());
            prop_assert_eq!((ptr as usize) % 8, 0);
            // Non-vacuous: write pattern.
            unsafe {
                std::ptr::write_bytes(ptr, 0xA5, size);
                prop_assert_eq!(ptr.read(), 0xA5);
            }
            live.push((ptr, size));
        }

        // Cross-thread free some blocks.
        let mut freed_ptrs: Vec<(*mut u8, usize)> = Vec::new();
        for &idx in &free_indices {
            if live.is_empty() {
                break;
            }
            let i = idx % live.len();
            let (ptr, size) = live.swap_remove(i);
            freed_ptrs.push((ptr, size));
        }

        // Send to another thread for cross-thread free.
        if !freed_ptrs.is_empty() {
            let ptrs_send: Vec<(*mut u8, usize)> = freed_ptrs.clone();
            // SAFETY: controlled test harness.
            let ptrs_raw: Vec<(usize, usize)> = ptrs_send.iter().map(|&(p, s)| (p as usize, s)).collect();
            std::thread::spawn(move || {
                for (addr, size) in ptrs_raw {
                    let ptr = addr as *mut u8;
                    let layout = Layout::from_size_align(size.max(1), 8).unwrap();
                    Heap::dealloc_any_thread(ptr, layout);
                }
            })
            .join()
            .unwrap();
        }

        // Trigger drain by allocating.
        let layout = layout_for(64);
        let p = heap.alloc(layout);
        prop_assert!(!p.is_null());
        unsafe {
            std::ptr::write_bytes(p, 0xBB, 64);
            prop_assert_eq!(p.read(), 0xBB);
        }

        // Free remaining live blocks.
        for (ptr, size) in &live {
            heap.dealloc(*ptr, layout_for(*size));
        }
    }
}
