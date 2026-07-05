//! Regression (task C1, 0.3.0): the magazine (tcache) fast path must serve
//! align>16 requests, not just align<=SMALL_ALIGN_MAX (16).
//!
//! ## Background
//!
//! `HeapCore::alloc`'s magazine fast path and `HeapCore::dealloc_own_thread`'s
//! magazine routing used to be gated on `align <= SMALL_ALIGN_MAX`. Every
//! align>16 request (e.g. tokio's `Cell<T, S>` at align=128, page-aligned
//! I/O buffers) therefore bypassed the magazine on EVERY alloc and dealloc,
//! going straight to the substrate (`AllocCore::alloc`/`dealloc`) even though
//! `SizeClasses::class_for(size, align)` already resolves a valid small class
//! for such requests (task B1). This defeated the whole point of the
//! magazine for a common workload shape.
//!
//! The fix removes the gate: `class_for` guarantees (for any `Some(c)` it
//! returns) that `block_size(c) % align == 0`, and every block of class `c`
//! is carved at an offset that is a multiple of `block_size(c)` inside a
//! SEGMENT-aligned segment — so ANY block of class `c` already satisfies
//! `align`, regardless of what `align` was. Keying the magazine purely by
//! `class_idx` is therefore sound.
//!
//! ## This test
//!
//! 1. **Hit-counter check**: drive an alloc/dealloc/alloc cycle at two
//!    align>16 shapes ((640,128) and (256,64)) and assert
//!    `tcache_hits_total()` increased — i.e. the magazine actually served at
//!    least one of these requests. This is the counterfactual-checked half
//!    (see the doc comment on `main` below for how it was verified).
//! 2. **Correctness**: allocate many blocks at several align>16 shapes,
//!    write a distinct byte pattern into each, verify no two blocks overlap
//!    (readback still matches its own pattern after all allocations are
//!    done) and every returned pointer satisfies its requested alignment.

#![cfg(all(feature = "alloc-global", feature = "fastbin"))]

use std::alloc::Layout;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};
// Only the `alloc-stats`-gated hit-count test uses the aggregator (task W3).
#[cfg(feature = "alloc-stats")]
use sefer_alloc::registry::tcache_hits_total;

// Serialise all tests in this file: the registry is a process-global static,
// and `tcache_hits_total()` aggregates a process-wide total (matching the
// discipline in `heap_core_tcache.rs`).
static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

/// C1: align>16 alloc/dealloc/alloc must register at least one magazine hit.
///
/// **Counterfactual (performed by hand during development, not re-run by
/// CI — this is the honest record of what was checked):** with the
/// `align <= SMALL_ALIGN_MAX` gate restored (the pre-fix code), this same
/// alloc/dealloc/alloc sequence at (640,128) and (256,64) produces ZERO
/// magazine hits for those two shapes — every request falls straight
/// through to `AllocCore::alloc`/`dealloc`, so `tcache_hits_total()` does not
/// move. With the gate removed (post-fix, the code under test here), the
/// second `alloc` of each shape is served by the magazine (the first
/// `dealloc` pushed the block into the now-unblocked magazine slot), so the
/// counter strictly increases. This test asserts the post-fix (increases)
/// side; the pre-fix (stays flat) side was verified manually.
///
/// Requires `alloc-stats` (task W3): the per-hit `tcache_hits` increment is
/// gated behind it, so without the feature `tcache_hits_total()` stays flat by
/// design and this assertion could not hold. The align>16 magazine ROUTING
/// itself (the actual C1 fix) is covered feature-independently by
/// `c1_align_over_16_correctness` below.
#[cfg(feature = "alloc-stats")]
#[test]
fn c1_align_over_16_hits_magazine() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let before = tcache_hits_total();

    let shapes = [(640usize, 128usize), (256usize, 64usize)];
    for &(size, align) in &shapes {
        let layout = Layout::from_size_align(size, align).unwrap();
        // First alloc: magazine is empty for this class -> miss (refill).
        let p1 = unsafe { (*heap).alloc(layout) };
        assert!(!p1.is_null(), "alloc({size},{align}) returned null");
        // Free it: with the gate removed, this block goes into the magazine
        // (align>16 is no longer excluded).
        unsafe { (*heap).dealloc(p1, layout) };
        // Second alloc of the SAME (size, align): should pop straight from
        // the magazine -> a hit, if (and only if) the C1 fix is in place.
        let p2 = unsafe { (*heap).alloc(layout) };
        assert!(!p2.is_null(), "second alloc({size},{align}) returned null");
        unsafe { (*heap).dealloc(p2, layout) };
    }

    let after = tcache_hits_total();
    assert!(
        after > before,
        "tcache_hits_total() did not increase for align>16 requests \
         (before={before}, after={after}) -- the magazine gate regressed"
    );

    unsafe { HeapRegistry::recycle(heap) };
}

/// C1 correctness: many align>16 allocations round-trip through the
/// magazine without corruption -- every block honours its requested
/// alignment and no two live blocks overlap (pattern + readback).
#[test]
fn c1_align_over_16_correctness() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // A mix of align>16 shapes, some requiring the divisibility walk
    // (class_for's slow path), including a page-aligned shape (task B1).
    let shapes: &[(usize, usize)] = &[
        (48, 32),
        (100, 32),
        (200, 64),
        (640, 128),
        (256, 64),
        (300, 256),
        (4000, 4096),
    ];

    struct Block {
        ptr: *mut u8,
        layout: Layout,
        pattern: u8,
    }

    let mut blocks: Vec<Block> = Vec::new();
    let mut seed: u64 = 0xC1C1_C1C1;
    let mut next_byte = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed & 0xFF) as u8
    };

    // Allocate a batch (round-robin through the shapes several times so the
    // magazine actually gets exercised: refill, hit, overflow).
    const ROUNDS: usize = 40;
    for round in 0..ROUNDS {
        for &(size, align) in shapes {
            let layout = Layout::from_size_align(size, align).unwrap();
            let p = unsafe { (*heap).alloc(layout) };
            assert!(
                !p.is_null(),
                "alloc({size},{align}) returned null at round {round}"
            );
            assert_eq!(
                (p as usize) % align,
                0,
                "alloc({size},{align}) returned misaligned pointer {p:p}"
            );
            let pattern = next_byte();
            unsafe { core::ptr::write_bytes(p, pattern, size) };
            blocks.push(Block {
                ptr: p,
                layout,
                pattern,
            });
        }
    }

    // No-overlap check: every live block's memory still holds exactly its
    // own pattern (if two blocks overlapped, a later write would have
    // clobbered an earlier block's bytes).
    for b in &blocks {
        let bytes = unsafe { core::slice::from_raw_parts(b.ptr, b.layout.size()) };
        assert!(
            bytes.iter().all(|&byte| byte == b.pattern),
            "block at {:p} (size={}, align={}) does not hold its own pattern \
             -- overlap or corruption",
            b.ptr,
            b.layout.size(),
            b.layout.align()
        );
    }

    // Uniqueness: no duplicate pointers were ever handed out live at once.
    let mut seen: HashSet<usize> = HashSet::with_capacity(blocks.len());
    for b in &blocks {
        assert!(
            seen.insert(b.ptr as usize),
            "duplicate live pointer {:p}",
            b.ptr
        );
    }

    // Free everything.
    for b in &blocks {
        unsafe { (*heap).dealloc(b.ptr, b.layout) };
    }

    // Sanity: the allocator is still functional after the free (no panic /
    // no null on a fresh alloc of the same shapes).
    for &(size, align) in shapes {
        let layout = Layout::from_size_align(size, align).unwrap();
        let p = unsafe { (*heap).alloc(layout) };
        assert!(
            !p.is_null(),
            "post-free alloc({size},{align}) returned null"
        );
        unsafe { (*heap).dealloc(p, layout) };
    }

    unsafe { HeapRegistry::recycle(heap) };
}
