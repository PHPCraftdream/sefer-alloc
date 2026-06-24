//! Byte tier tests over `ByteRegion` and `ByteAllocator` (Phase 4, `byte`).
//!
//! FAST tests per the short-scenario policy — small sizes and counts so the
//! suite (and miri over it) finishes quickly. Cover:
//!
//! 1. **alloc / write / read-back / dealloc** across several size classes and
//!    a large (system-fallback) allocation.
//! 2. **alignment** — every returned pointer satisfies the requested `Layout`
//!    align.
//! 3. **`alloc_zeroed`** — returns all-zero memory.
//! 4. **free-list reuse** — dealloc then alloc the same class reuses space
//!    (chunk count does not grow unboundedly under churn).
//! 5. **`GlobalAlloc` round-trip** via direct `GlobalAlloc::alloc`/`dealloc`
//!    through `ByteAllocator`.
//!
//! These tests exercise raw pointers; every `unsafe` block has a `// SAFETY:`
//! comment justifying the dereference against an in-bounds allocation.

#![cfg(feature = "byte")]

use std::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;

use sefer_alloc::{ByteAllocator, ByteRegion};

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

/// Write `byte` to every byte of `len`, then read back and assert, through a
/// raw pointer returned by `ByteRegion`. Confirms the full range is owned and
/// writable.
///
/// # Safety (internal)
///
/// The caller guarantees `ptr` is valid for `len` bytes.
unsafe fn fill_and_check(ptr: *mut u8, len: usize, byte: u8) {
    // SAFETY: caller guarantees `ptr` is valid for `len` bytes.
    unsafe { std::ptr::write_bytes(ptr, byte, len) };
    // SAFETY: same validity; read back each byte we just wrote.
    for i in 0..len {
        assert_eq!(
            unsafe { ptr.add(i).read() },
            byte,
            "byte {i} did not read back the value just written"
        );
    }
}

/// Asserts `ptr` is non-null and aligned to `align`.
fn assert_aligned(ptr: *mut u8, align: usize) {
    assert!(!ptr.is_null(), "allocation must not return null");
    let addr = ptr as usize;
    assert_eq!(
        addr & (align - 1),
        0,
        "pointer {ptr:#p} (addr {addr:#x}) is not aligned to {align}"
    );
}

// ---------------------------------------------------------------------------
// 1. alloc / write / read-back / dealloc across classes + large fallback.
// ---------------------------------------------------------------------------

/// Allocates each size class, writes the full range, reads it back, deallocs.
/// Also exercises a large (system-fallback) allocation.
#[test]
fn alloc_write_read_dealloc_across_classes_and_large() {
    let mut region = ByteRegion::new();

    // Each size class in turn: pick a size <= the class and an align that the
    // class satisfies.
    let cases: &[(usize, usize)] = &[
        // (size, align)
        (1, 1),
        (7, 8),
        (13, 8),
        (24, 16),
        (60, 32),
        (100, 64),
        (200, 128),
        (400, 256),
        (900, 512),
        (1000, 1024),
    ];

    let mut ptrs = Vec::new();
    for &(size, align) in cases {
        let layout = Layout::from_size_align(size, align).unwrap();
        let ptr = region.alloc(layout);
        assert!(!ptr.is_null(), "alloc({size}, align {align}) must succeed");
        assert_aligned(ptr, align);
        // SAFETY: `ptr` was just allocated for `size` bytes by `region.alloc`.
        unsafe { fill_and_check(ptr, size, 0xA5) };
        ptrs.push((ptr, layout));
    }

    // Large (system fallback): bigger than the largest class (1024).
    let big_size = 4096;
    let big_layout = Layout::from_size_align(big_size, 16).unwrap();
    let big_ptr = region.alloc(big_layout);
    assert!(
        !big_ptr.is_null(),
        "large alloc must succeed via system fallback"
    );
    assert_aligned(big_ptr, 16);
    // SAFETY: `big_ptr` allocated for `big_size` bytes.
    unsafe { fill_and_check(big_ptr, big_size, 0x5A) };

    // Dealloc everything. These must not panic and must route correctly
    // (in-arena vs system).
    for (ptr, layout) in &ptrs {
        // SAFETY: each `*ptr` was returned by `region.alloc(*layout)` above and
        // not yet freed.
        unsafe { region.dealloc(*ptr, *layout) };
    }
    // SAFETY: `big_ptr` was returned by `region.alloc(big_layout)` above.
    unsafe { region.dealloc(big_ptr, big_layout) };

    // Touch the region so the optimiser doesn't elide it.
    black_box(&region);
}

// ---------------------------------------------------------------------------
// 2. alignment for every returned pointer.
// ---------------------------------------------------------------------------

/// Every returned pointer satisfies the requested align, across powers of two
/// and non-trivial alignments, and also for reused (freed-then-realloc'd) blocks.
#[test]
fn every_pointer_satisfies_alignment() {
    let mut region = ByteRegion::new();

    for align in [1usize, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024] {
        // Allocate several blocks of each align to also exercise reuse.
        let mut ptrs = Vec::new();
        for _ in 0..8 {
            let layout = Layout::from_size_align(align, align).unwrap();
            let ptr = region.alloc(layout);
            assert_aligned(ptr, align);
            ptrs.push((ptr, layout));
        }
        for (ptr, layout) in &ptrs {
            // SAFETY: `*ptr` was returned by `region.alloc(*layout)` above.
            unsafe { region.dealloc(*ptr, *layout) };
        }
        // Re-allocate the same class: reused blocks must STILL be aligned.
        for _ in 0..8 {
            let layout = Layout::from_size_align(align, align).unwrap();
            let ptr = region.alloc(layout);
            assert_aligned(ptr, align);
            // SAFETY: `ptr` was just returned by `region.alloc(layout)`.
            unsafe { region.dealloc(ptr, layout) };
        }
    }

    black_box(&region);
}

// ---------------------------------------------------------------------------
// 3. alloc_zeroed returns zeroed memory.
// ---------------------------------------------------------------------------

/// `alloc_zeroed` returns memory filled with zeroes across several classes and
/// a large fallback allocation.
#[test]
fn alloc_zeroed_returns_zeroed_memory() {
    let mut region = ByteRegion::new();

    let cases: &[(usize, usize)] = &[
        (8, 8),
        (64, 8),
        (256, 16),
        (1024, 32),
        (5000, 8), // large fallback
    ];

    for &(size, align) in cases {
        let layout = Layout::from_size_align(size, align).unwrap();
        let ptr = region.alloc_zeroed(layout);
        assert!(!ptr.is_null(), "alloc_zeroed must succeed");
        assert_aligned(ptr, align);
        // SAFETY: `ptr` allocated for `size` bytes via alloc_zeroed.
        for i in 0..size {
            assert_eq!(
                unsafe { ptr.add(i).read() },
                0,
                "alloc_zeroed byte {i} (size {size}, align {align}) is not zero"
            );
        }
        // SAFETY: `ptr` was returned by `region.alloc_zeroed(layout)` above.
        unsafe { region.dealloc(ptr, layout) };
    }

    black_box(&region);
}

// ---------------------------------------------------------------------------
// 4. free-list reuse: churn does not grow chunks unboundedly.
// ---------------------------------------------------------------------------

/// Under steady-state churn (alloc then dealloc the same class repeatedly), the
/// region must reuse freed blocks rather than growing chunks forever. We assert
/// the chunk count stays small after many dealloc/alloc cycles of one class.
#[test]
fn free_list_reuse_caps_growth_under_churn() {
    // Each block is 64B, a chunk is 64 KiB => ~1024 blocks/chunk. Allocate
    // enough to force at least one chunk, then churn.
    const N: usize = 1000;

    let mut region = ByteRegion::new();
    let layout = Layout::from_size_align(64, 8).unwrap();

    // Steady-state churn: alloc N, dealloc N, many times. With reuse, the chunk
    // count must NOT grow each iteration.
    let mut last_chunk_count = usize::MAX;
    for _ in 0..20 {
        let mut ptrs = Vec::with_capacity(N);
        for _ in 0..N {
            let ptr = region.alloc(layout);
            assert!(!ptr.is_null());
            ptrs.push(ptr);
        }
        for ptr in &ptrs {
            // SAFETY: each `*ptr` was returned by `region.alloc(layout)` above
            // in this iteration and not yet freed.
            unsafe { region.dealloc(*ptr, layout) };
        }
        let chunks = region.chunk_count();
        // After the first iteration the working set is established; subsequent
        // iterations must reuse, so chunk count must not grow unboundedly.
        // Allow at most a small budget of chunks.
        assert!(
            chunks <= 2,
            "churn should reuse free blocks; chunk count grew to {chunks}"
        );
        last_chunk_count = chunks;
    }
    // Final chunk count is bounded (and stable after warmup).
    assert!(last_chunk_count <= 2);

    black_box(&region);
}

// ---------------------------------------------------------------------------
// 5. GlobalAlloc round-trip via ByteAllocator (direct call, NOT installed).
// ---------------------------------------------------------------------------

/// Exercises `ByteAllocator`'s `unsafe impl GlobalAlloc` by calling
/// `GlobalAlloc::alloc`/`dealloc`/`alloc_zeroed`/`realloc` directly. This does
/// NOT install it as the global allocator (that would be dangerous in a test
/// binary); it only verifies the impl round-trips correctly.
#[test]
fn global_alloc_round_trip_direct() {
    let alloc = ByteAllocator::new();

    // alloc / write / dealloc
    let layout = Layout::from_size_align(48, 8).unwrap();
    // SAFETY: `ByteAllocator` implements `GlobalAlloc`; this is the documented
    // way to call it. The returned pointer is valid for `layout.size()` bytes.
    let ptr = unsafe { alloc.alloc(layout) };
    assert!(!ptr.is_null());
    assert_aligned(ptr, 8);
    // SAFETY: `ptr` valid for 48 bytes.
    unsafe { std::ptr::write_bytes(ptr, 0x7E, 48) };
    // SAFETY: dealloc with the same layout and a valid pointer (GlobalAlloc
    // contract).
    unsafe { alloc.dealloc(ptr, layout) };

    // alloc_zeroed
    let zlayout = Layout::from_size_align(128, 16).unwrap();
    // SAFETY: as above.
    let zptr = unsafe { alloc.alloc_zeroed(zlayout) };
    assert!(!zptr.is_null());
    assert_aligned(zptr, 16);
    // SAFETY: `zptr` valid for 128 bytes; verify zeroed.
    for i in 0..128 {
        assert_eq!(unsafe { zptr.add(i).read() }, 0, "zeroed byte {i} nonzero");
    }
    // SAFETY: dealloc valid pointer with same layout.
    unsafe { alloc.dealloc(zptr, zlayout) };

    // realloc grow + shrink round trip
    let start = Layout::from_size_align(32, 8).unwrap();
    // SAFETY: valid GlobalAlloc call.
    let p1 = unsafe { alloc.alloc(start) };
    assert!(!p1.is_null());
    // SAFETY: write a sentinel into the first 32 bytes.
    unsafe { std::ptr::write_bytes(p1, 0xAB, 32) };
    // Grow to 256.
    // SAFETY: `p1` is a valid allocation of `start`, not yet deallocated.
    let p2 = unsafe { alloc.realloc(p1, start, 256) };
    assert!(!p2.is_null());
    // SAFETY: first 32 bytes copied over; verify sentinel survived the move.
    for i in 0..32 {
        assert_eq!(
            unsafe { p2.add(i).read() },
            0xAB,
            "realloc preserved byte {i}"
        );
    }
    // Shrink back to 16.
    // SAFETY: `p2` valid for 256 bytes (post-grow); shrinking to 16 is fine.
    let p3 = unsafe { alloc.realloc(p2, Layout::from_size_align(256, 8).unwrap(), 16) };
    assert!(!p3.is_null());
    // SAFETY: first 16 bytes still carry the sentinel.
    for i in 0..16 {
        assert_eq!(
            unsafe { p3.add(i).read() },
            0xAB,
            "shrink preserved byte {i}"
        );
    }
    // SAFETY: final dealloc with the layout matching the last realloc's size.
    unsafe { alloc.dealloc(p3, Layout::from_size_align(16, 8).unwrap()) };

    black_box(&alloc);
}
