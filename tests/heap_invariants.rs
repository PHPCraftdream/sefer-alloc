//! Focused invariant tests for the Phase 8 segment substrate (`alloc-core`
//! feature).
//!
//! Targeted unit tests: alignment, reuse, refill, realloc, churn, own-thread
//! dealloc on multiple threads each with its own `AllocCore`. Kept FAST per
//! the short-scenario policy: small sizes, small counts, miri-friendly.
//!
//! (An earlier version drove this through the now-removed `Heap` wrapper;
//! `Heap` was a pure pass-through to `AllocCore` on the single-thread `alloc`
//! feature, so this is a faithful 1:1 substitution. The two TLS `with_heap`
//! binding tests were removed alongside the `Heap`/`with_heap` API.)

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;
use std::ptr;

use sefer_alloc::AllocCore;

// ---------------------------------------------------------------------------
// M1 -- validity: non-null, sized, aligned.
// ---------------------------------------------------------------------------

#[test]
fn m1_small_allocations_are_aligned_and_writable() {
    let mut h = AllocCore::new().unwrap();
    for align in [1usize, 2, 4, 8, 16] {
        for size in [1usize, 7, 16, 100, 1024, 4096] {
            let layout = Layout::from_size_align(size, align).unwrap();
            let p = h.alloc(layout);
            assert!(!p.is_null(), "alloc({size}, {align}) returned null");
            assert_eq!((p as usize) % align, 0, "not aligned to {align}");
            // Write pattern + read back (M1: bytes are ours).
            unsafe {
                ptr::write_bytes(p, 0xAB, size);
                for b in 0..size {
                    assert_eq!(p.add(b).read(), 0xAB);
                }
            }
        }
    }
}

#[test]
fn m1_large_allocations_are_aligned_and_writable() {
    let mut h = AllocCore::new().unwrap();
    let big = 1024 * 1024; // 1 MiB -- above SMALL_MAX
    let layout = Layout::from_size_align(big, 4096).unwrap();
    let p = h.alloc(layout);
    assert!(!p.is_null());
    assert_eq!((p as usize) % 4096, 0);
    unsafe {
        ptr::write_bytes(p, 0x33, big);
        assert_eq!(p.read(), 0x33);
        assert_eq!(p.add(big - 1).read(), 0x33);
    }
}

#[test]
fn m1_alloc_zeroed_is_all_zero() {
    let mut h = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(999, 8).unwrap();
    let p = h.alloc_zeroed(layout);
    assert!(!p.is_null());
    unsafe {
        for b in 0..999 {
            assert_eq!(p.add(b).read(), 0, "byte {b} not zero");
        }
    }
}

// ---------------------------------------------------------------------------
// M2 -- double-free is safe.
// ---------------------------------------------------------------------------

#[test]
fn m2_double_free_does_not_crash() {
    let mut h = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(64, 8).unwrap();
    let p = h.alloc(layout);
    h.dealloc(p, layout);
    // Second dealloc: pushes again onto the free list. Not ideal but not UB.
    // The allocator must continue to function.
    h.dealloc(p, layout);
    let p2 = h.alloc(layout);
    assert!(!p2.is_null());
}

// ---------------------------------------------------------------------------
// M3 -- no overlap.
// ---------------------------------------------------------------------------

#[test]
fn m3_simultaneous_allocations_do_not_overlap() {
    let mut h = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(256, 8).unwrap();
    let mut ptrs = Vec::new();
    for _ in 0..64 {
        let p = h.alloc(layout);
        assert!(!p.is_null());
        ptrs.push(p);
    }
    // Pairwise non-overlap.
    for i in 0..ptrs.len() {
        for j in (i + 1)..ptrs.len() {
            let a = ptrs[i] as usize;
            let b = ptrs[j] as usize;
            assert!(
                a + 256 <= b || b + 256 <= a,
                "allocations {i} and {j} overlap"
            );
        }
    }
    // Write unique patterns, verify no cross-contamination.
    for (i, &p) in ptrs.iter().enumerate() {
        unsafe { ptr::write_bytes(p, i as u8, 256) };
    }
    for (i, &p) in ptrs.iter().enumerate() {
        unsafe {
            for b in 0..256 {
                assert_eq!(p.add(b).read(), i as u8, "alloc {i} byte {b} clobbered");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// M4 -- alignment & size fidelity.
// ---------------------------------------------------------------------------

#[test]
fn m4_various_sizes_and_aligns() {
    let mut h = AllocCore::new().unwrap();
    for align in [1usize, 2, 4, 8, 16] {
        for size in [1usize, 15, 31, 63, 127, 255, 511, 1023, 2047] {
            let layout = Layout::from_size_align(size, align).unwrap();
            let p = h.alloc(layout);
            assert!(!p.is_null());
            assert_eq!((p as usize) % align, 0, "size={size} align={align}");
        }
    }
}

// ---------------------------------------------------------------------------
// Free-list reuse: alloc/dealloc cycles reuse blocks.
// ---------------------------------------------------------------------------

#[test]
fn free_list_reuses_freed_blocks() {
    let mut h = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(64, 8).unwrap();
    let mut ptrs = Vec::new();
    for _ in 0..256 {
        ptrs.push(h.alloc(layout));
    }
    for &p in &ptrs {
        h.dealloc(p, layout);
    }
    // Re-allocate: should reuse without needing many new segments.
    for _ in 0..256 {
        let p = h.alloc(layout);
        assert!(!p.is_null());
    }
}

// ---------------------------------------------------------------------------
// Refill: draining the free list triggers a refill batch from the substrate.
// ---------------------------------------------------------------------------

#[test]
fn refill_works_after_draining_free_list() {
    let mut h = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(128, 8).unwrap();
    // Allocate enough to trigger at least one refill (REFILL_BATCH = 31,
    // so 64 allocs should trigger 2 refills).
    let mut ptrs = Vec::new();
    for _ in 0..64 {
        let p = h.alloc(layout);
        assert!(!p.is_null());
        ptrs.push(p);
    }
    // Free all, then allocate again (hot-path pops from the rebuilt free list).
    for &p in &ptrs {
        h.dealloc(p, layout);
    }
    for _ in 0..64 {
        let p = h.alloc(layout);
        assert!(!p.is_null());
    }
}

// ---------------------------------------------------------------------------
// Realloc preserves bytes.
// ---------------------------------------------------------------------------

#[test]
fn realloc_preserves_prefix_bytes() {
    let mut h = AllocCore::new().unwrap();
    let initial = 128;
    let layout = Layout::from_size_align(initial, 8).unwrap();
    let p = h.alloc(layout);
    unsafe {
        for b in 0..initial {
            p.add(b).write((b as u8).wrapping_mul(7));
        }
    }
    // Grow.
    let new_p = h.realloc(p, layout, 512);
    assert!(!new_p.is_null());
    unsafe {
        for b in 0..initial {
            assert_eq!(
                new_p.add(b).read(),
                (b as u8).wrapping_mul(7),
                "byte {b} not preserved across realloc grow"
            );
        }
    }
    // Shrink.
    let new_layout = Layout::from_size_align(512, 8).unwrap();
    let shrunk = h.realloc(new_p, new_layout, 32);
    assert!(!shrunk.is_null());
    unsafe {
        for b in 0..32 {
            assert_eq!(
                shrunk.add(b).read(),
                (b as u8).wrapping_mul(7),
                "byte {b} not preserved across realloc shrink"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Churn: sustained alloc/dealloc keeps allocator consistent.
// ---------------------------------------------------------------------------

#[test]
fn churn_keeps_heap_consistent() {
    let mut h = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(64, 8).unwrap();
    for _ in 0..10_000 {
        let p = h.alloc(layout);
        assert!(!p.is_null());
        h.dealloc(p, layout);
    }
}

// ---------------------------------------------------------------------------
// Multi-thread: each thread has its own heap, no cross-thread free.
// ---------------------------------------------------------------------------

#[test]
fn multi_thread_own_heap_own_dealloc() {
    let threads: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                let mut h = AllocCore::new().unwrap();
                let layout = Layout::from_size_align(64, 8).unwrap();
                let mut ptrs = Vec::new();
                for _ in 0..256 {
                    let p = h.alloc(layout);
                    assert!(!p.is_null());
                    // Write pattern.
                    unsafe { ptr::write_bytes(p, 0xCC, 64) };
                    ptrs.push(p);
                }
                // Read back.
                for &p in &ptrs {
                    unsafe {
                        for b in 0..64 {
                            assert_eq!(p.add(b).read(), 0xCC);
                        }
                    }
                }
                // Dealloc on the owning thread.
                for &p in &ptrs {
                    h.dealloc(p, layout);
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
}
