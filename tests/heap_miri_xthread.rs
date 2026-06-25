//! Tiny miri-targeted test for the Phase 10 cross-thread atomic seam.
//!
//! Runs a small 2-thread alloc/free scenario where thread B frees blocks
//! allocated by thread A, exercising the `ThreadFreeStack` push (CAS) and
//! drain (swap) under miri's UB detector. Kept FAST per the short-scenario
//! policy: 4 blocks, 2 threads.
//!
//! # How to run
//!
//! ```sh
//! MIRIFLAGS="-Zmiri-ignore-leaks" cargo +nightly miri test --features alloc-xthread --test heap_miri_xthread
//! ```
//!
//! The `-Zmiri-ignore-leaks` flag is needed because `Heap::drop` under
//! `alloc-xthread` intentionally leaks segments and the Treiber head
//! (abandonment-leak for thread-death soundness). Miri's leak detector
//! would flag this as an error without the flag. The leaks are bounded and
//! documented -- see `src/heap/heap.rs` Drop impl.

#![cfg(feature = "alloc-xthread")]

use std::alloc::Layout;

use sefer_alloc::Heap;

/// 2-thread alloc/free: A allocates, B frees via `dealloc_any_thread`, A
/// re-allocates (triggering drain). Miri checks for UB in the atomic seam.
#[test]
fn miri_cross_thread_basic() {
    let mut heap_a = Heap::new().unwrap();
    let layout = Layout::from_size_align(64, 8).unwrap();

    // A allocates 4 blocks.
    let mut ptrs = Vec::new();
    for i in 0u8..4 {
        let p = heap_a.alloc(layout);
        assert!(!p.is_null());
        // Write a pattern (non-vacuous).
        unsafe {
            std::ptr::write_bytes(p, i.wrapping_add(0xA0), 64);
        }
        ptrs.push(p);
    }

    // B frees all 4 via the cross-thread path.
    let ptrs_raw: Vec<usize> = ptrs.iter().map(|&p| p as usize).collect();
    std::thread::spawn(move || {
        for addr in ptrs_raw {
            let ptr = addr as *mut u8;
            Heap::dealloc_any_thread(ptr, layout);
        }
    })
    .join()
    .unwrap();

    // A allocates again (triggers drain of the thread-free stack).
    for _ in 0..4 {
        let p = heap_a.alloc(layout);
        assert!(!p.is_null());
        // Write + read (non-vacuous).
        unsafe {
            std::ptr::write_bytes(p, 0xBB, 64);
            assert_eq!(p.read(), 0xBB);
        }
    }
}
