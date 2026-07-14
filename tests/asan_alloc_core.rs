//! AddressSanitizer-driven stress harness for the `alloc-core` segment substrate.
//!
//! ## Why this exists / what ASan adds
//!
//! The project already runs Miri (strict-provenance UB), Loom (concurrency
//! model-check), TSan (real-thread data races), Kani (proofs) and proptest.
//! AddressSanitizer covers a complementary dimension: it instruments the
//! allocator's OWN raw-pointer memory operations (the `os` mmap/munmap seam,
//! the intrusive free-list `node` seam, the realloc `copy_nonoverlapping`
//! move legs) with byte-granularity shadow memory and catches out-of-bounds
//! access, use-after-free, and invalid free on memory that escapes a mapping.
//!
//! ## CRITICAL: AllocCore is driven DIRECTLY, never as `#[global_allocator]`
//!
//! ASan installs its own process-wide allocator interceptors. Installing
//! `SeferAlloc` (or `AllocCore`) as the `#[global_allocator]` would clash
//! with those interceptors — every Rust allocation would be double-instrumented
//! and the ASan shadow would fight the allocator's own mmap'd segments. So
//! this harness constructs an owned `AllocCore` and drives its `alloc` /
//! `dealloc` / `realloc` / `alloc_zeroed` entry points DIRECTLY. The
//! allocator's segments are its own mmap'd regions (NOT serviced by the system
//! `malloc` ASan intercepts); ASan still shadows that address space, so an
//! access that escapes a released mapping is flagged. (See the script header
//! in `scripts/asan.mjs` and the verification note below for the exact class
//! of bug ASan can and cannot see inside a self-hosted mmap arena.)
//!
//! ## How to run
//!
//! This is a normal test file — `cargo test --features alloc-core --test
//! asan_alloc_core` runs it uninstrumented (it is a valid stress test on its
//! own). The `scripts/asan.mjs` runner compiles + runs it under
//! AddressSanitizer on a nightly toolchain via WSL:
//!
//! ```text
//! node scripts/asan.mjs
//! npm run asan
//! ```
//!
//! It is deliberately NOT part of `npm run check` (Phase-5 / nightly tier, per
//! CLAUDE.md "Speed: short scenario by default").

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;
use std::ptr;

use sefer_alloc::{AllocCore, SegmentLayout};

// ---------------------------------------------------------------------------
// Small alloc → write → realloc grow/shrink → dealloc (the hot path).
// ---------------------------------------------------------------------------

#[test]
fn asan_small_alloc_write_realloc_dealloc() {
    let mut a = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(64, 8).unwrap();
    let ptr = a.alloc(layout);
    assert!(!ptr.is_null());

    // Write + read-back through the allocator-returned pointer: ASan shadows
    // these accesses against the segment mapping.
    // SAFETY: `ptr` is valid for `layout.size()` bytes per AllocCore's M1.
    unsafe {
        for b in 0..64 {
            ptr.add(b).write(0xA5 ^ (b as u8));
        }
        for b in 0..64 {
            assert_eq!(ptr.add(b).read(), 0xA5 ^ (b as u8), "byte {b} mismatch");
        }
    }

    // Grow realloc — exercises the copy leg (copy_nonoverlapping over the
    // segment payload), ASan's highest-value surface here.
    let grown = a.realloc(ptr, layout, 512);
    assert!(!grown.is_null());
    // SAFETY: `grown` valid for 512 bytes; first 64 preserved.
    unsafe {
        for b in 0..64 {
            assert_eq!(
                grown.add(b).read(),
                0xA5 ^ (b as u8),
                "prefix byte {b} lost"
            );
        }
    }
    // Shrink realloc back down.
    let big = Layout::from_size_align(512, 8).unwrap();
    let shrunk = a.realloc(grown, big, 32);
    assert!(!shrunk.is_null());
    // SAFETY: `shrunk` valid for 32 bytes; first 32 preserved.
    unsafe {
        for b in 0..32 {
            assert_eq!(
                shrunk.add(b).read(),
                0xA5 ^ (b as u8),
                "shrink byte {b} lost"
            );
        }
    }
    a.dealloc(shrunk, Layout::from_size_align(32, 8).unwrap());
}

// ---------------------------------------------------------------------------
// Mixed sizes & alignments across the small size-class table.
// ---------------------------------------------------------------------------

#[test]
fn asan_mixed_sizes_aligns() {
    let mut a = AllocCore::new().unwrap();
    let mut live = Vec::new();
    for align in [1usize, 2, 4, 8, 16, 32] {
        for size in [1usize, 3, 7, 15, 31, 63, 127, 255, 511, 1023] {
            let layout = Layout::from_size_align(size, align).unwrap();
            let ptr = a.alloc(layout);
            assert!(!ptr.is_null(), "alloc({size},{align}) null");
            assert_eq!((ptr as usize) % align, 0, "not aligned");
            // SAFETY: valid for `size`.
            unsafe {
                ptr::write_bytes(ptr, 0x3C, size);
                assert_eq!(ptr.add(size - 1).read(), 0x3C, "tail byte {size}");
            }
            live.push((ptr, layout));
        }
    }
    for (ptr, layout) in live {
        a.dealloc(ptr, layout);
    }
}

// ---------------------------------------------------------------------------
// Large dedicated-segment lifecycle: alloc, write, free. Freeing a large
// segment releases its OS reservation (os::release_segment → munmap). This is
// the surface ASan catches use-after-free on: after the segment is released,
// its virtual pages are unmapped and a subsequent access faults under ASan.
// ---------------------------------------------------------------------------

#[test]
fn asan_large_segment_lifecycle() {
    let mut a = AllocCore::new().unwrap();
    // Strictly above SMALL_MAX → the dedicated-segment (large) path.
    let size = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    let layout = Layout::from_size_align(size, 4096).unwrap();
    let ptr = a.alloc(layout);
    assert!(!ptr.is_null());
    assert_eq!((ptr as usize) % 4096, 0);
    // SAFETY: valid for `size`; write head + tail only (multi-MiB sweep is
    // pointless cost under the sanitizer).
    unsafe {
        ptr::write_bytes(ptr, 0x77, 4096);
        assert_eq!(ptr.add(0).read(), 0x77);
        ptr::write_bytes(ptr.add(size - 256), 0x77, 256);
        assert_eq!(ptr.add(size - 1).read(), 0x77);
    }
    a.dealloc(ptr, layout);
    // After dealloc the large segment's reservation is released (munmap). We do
    // NOT touch `ptr` here — that would be the use-after-free the ASan runner's
    // injected-bug proof exercises. This clean test only validates the lifecycle
    // is UB-free.
}

// ---------------------------------------------------------------------------
// alloc_zeroed contract under the sanitizer.
// ---------------------------------------------------------------------------

#[test]
fn asan_alloc_zeroed_is_zero() {
    let mut a = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(512, 16).unwrap();
    let ptr = a.alloc_zeroed(layout);
    assert!(!ptr.is_null());
    // SAFETY: zeroed allocation, valid for 512 bytes.
    unsafe {
        for b in 0..512 {
            assert_eq!(ptr.add(b).read(), 0, "byte {b} not zero");
        }
    }
    a.dealloc(ptr, layout);
}

// ---------------------------------------------------------------------------
// Churn: many alloc/dealloc cycles. Validates the free-list reuse path stays
// ASan-clean (no stale shadow from a recycled block).
// ---------------------------------------------------------------------------

#[test]
fn asan_churn_reuse() {
    let mut a = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(96, 8).unwrap();
    for _ in 0..2000 {
        let ptr = a.alloc(layout);
        assert!(!ptr.is_null());
        // SAFETY: valid for 96.
        unsafe {
            ptr::write_bytes(ptr, 0xE9, 96);
            assert_eq!(ptr.add(95).read(), 0xE9);
        }
        a.dealloc(ptr, layout);
    }
}

// ---------------------------------------------------------------------------
// Realloc chain across size classes (small → small, small → large, large →
// small). Each move leg copies bytes the sanitizer shadows.
// ---------------------------------------------------------------------------

#[test]
fn asan_realloc_chain_across_classes() {
    let mut a = AllocCore::new().unwrap();
    let start = Layout::from_size_align(128, 8).unwrap();
    let mut ptr = a.alloc(start);
    assert!(!ptr.is_null());
    let mut cur_size = 128usize;
    let cur_align = 8usize;
    // SAFETY: prime the block with a known pattern.
    unsafe {
        for b in 0..cur_size {
            ptr.add(b).write((b as u8).wrapping_mul(5));
        }
    }
    // Grow past SMALL_MAX into the dedicated-segment path, then shrink back.
    for &next in &[
        256usize,
        1024,
        SegmentLayout::SMALL_MAX + SegmentLayout::PAGE,
        2048,
        64,
    ] {
        let old_layout = Layout::from_size_align(cur_size, cur_align).unwrap();
        let new = a.realloc(ptr, old_layout, next);
        assert!(!new.is_null());
        let keep = cur_size.min(next);
        // SAFETY: `new` valid for `next`; first `keep` bytes preserved.
        unsafe {
            for b in 0..keep {
                assert_eq!(
                    new.add(b).read(),
                    (b as u8).wrapping_mul(5),
                    "realloc chain byte {b} (size {cur_size}->{next}) lost"
                );
            }
        }
        // Re-establish the pattern over the whole new extent for the next leg.
        // SAFETY: `new` valid for `next`.
        unsafe {
            for b in 0..next {
                new.add(b).write((b as u8).wrapping_mul(5));
            }
        }
        ptr = new;
        cur_size = next;
    }
    a.dealloc(ptr, Layout::from_size_align(cur_size, cur_align).unwrap());
}

// ---------------------------------------------------------------------------
// Many live small allocations, then reverse-order free (LIFO reuse). Catches a
// free-list corruption that only surfaces under ordering pressure.
// ---------------------------------------------------------------------------

#[test]
fn asan_many_live_then_reverse_free() {
    let mut a = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(48, 8).unwrap();
    let mut ptrs = Vec::new();
    for _ in 0..512 {
        let ptr = a.alloc(layout);
        assert!(!ptr.is_null());
        // SAFETY: valid for 48.
        unsafe {
            ptr::write_bytes(ptr, 0x1F, 48);
        }
        ptrs.push(ptr);
    }
    // Reverse free to exercise a different reuse ordering than forward churn.
    while let Some(ptr) = ptrs.pop() {
        a.dealloc(ptr, layout);
    }
}
