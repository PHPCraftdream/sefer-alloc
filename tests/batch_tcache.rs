//! R10-7 (Part 2) — correctness tests for the tcache-aware batch API
//! (`SeferAlloc::alloc_batch` / `dealloc_batch`).
//!
//! These tests prove the batch produces the SAME observable result as N
//! individual scalar `alloc`/`dealloc` calls: every returned block is a valid,
//! aligned, non-aliasing live allocation that can be freed either as a batch
//! (`dealloc_batch`) or one-by-one (scalar `dealloc`), and a batch cycle leaves
//! the heap usable for subsequent allocations (no corruption / leak on the
//! steady-state path). Per project convention: tests live in `tests/`, not
//! inline.
//!
//! The tcache-aware design is exercised across the regimes that matter:
//! - N ≤ `TCACHE_CAP` (16): the magazine-drain fast path alone serves the batch.
//! - N > `TCACHE_CAP`: the drain exhausts the magazine and the `refill_class_bump`
//!   remainder path fills the rest (the genuinely new code).
//! - repeated cycles: the warm steady state (magazine drained then refilled by
//!   the frees) the bench measures.
//!
//! `batch-api` feature gate (R10-7 follow-up): the `alloc_batch`/`dealloc_batch`
//! surface is gated behind the `batch-api` Cargo feature (NOT part of
//! `production`); this test file requires it alongside `alloc-global`.

#![cfg(all(feature = "alloc-global", feature = "batch-api"))]

use std::alloc::{GlobalAlloc, Layout};
use std::collections::HashSet;

use sefer_alloc::SeferAlloc;

// Each file in `tests/` is its own binary, so installing the global allocator
// here is isolated from the rest of the suite.
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

/// Sizes spanning several small size classes (16 B, 64 B, 256 B) plus a
/// larger one (1024 B). All align 8.
const SIZES: &[usize] = &[16, 64, 256, 1024];

/// Batch sizes: 1 (degenerate), the magazine cap boundary (8/16/17), a
/// realistic bulk (32/64), and a value that forces several magazine
/// drain+refill oscillations (200).
const BATCH_NS: &[usize] = &[1, 8, 16, 17, 32, 64, 200];

/// Allocate `n` blocks of `layout` via `alloc_batch` and assert every returned
/// slot is non-null, `layout.align()`-aligned, and distinct from every other
/// slot in the batch. Returns the filled slice (length == `n` on success).
unsafe fn alloc_batch_valid(n: usize, layout: Layout) -> Vec<*mut u8> {
    let mut buf: Vec<*mut u8> = vec![std::ptr::null_mut(); n];
    let filled = unsafe { GLOBAL.alloc_batch(layout, &mut buf) };
    assert_eq!(filled, n, "alloc_batch under-filled (n={n})");
    let mut seen: HashSet<usize> = HashSet::new();
    for (i, &p) in buf.iter().enumerate() {
        assert!(!p.is_null(), "alloc_batch returned null at [{i}] (n={n})");
        assert_eq!(
            p as usize % layout.align(),
            0,
            "alloc_batch ptr not layout-aligned at [{i}] (n={n})",
        );
        assert!(
            seen.insert(p as usize),
            "alloc_batch issued duplicate ptr at [{i}]"
        );
    }
    buf
}

/// Write a unique 8-byte pattern into each block's first word and verify it is
/// independently readable — proves the blocks are usable, non-overlapping
/// storage (an aliasing/corruption bug would clobber a neighbour's pattern).
unsafe fn write_distinct_patterns(blocks: &[*mut u8]) {
    for (i, &p) in blocks.iter().enumerate() {
        let pat = 0xDEAD_BEEF_0000_0000u64 | (i as u64);
        // SAFETY: `p` is a freshly allocated block of `layout` (size >= 16 for
        // every tested size), so the first 8 bytes are in bounds and writable.
        unsafe { std::ptr::write_volatile(p.cast::<u64>(), pat) };
    }
    // Re-read in a second pass so a later write clobbering an earlier block's
    // word is actually observed (write_volatile alone does not force a re-read).
    for (i, &p) in blocks.iter().enumerate() {
        let pat = 0xDEAD_BEEF_0000_0000u64 | (i as u64);
        // SAFETY: same in-bounds justification as above.
        let got = unsafe { std::ptr::read_volatile(p.cast::<u64>()) };
        assert_eq!(
            got, pat,
            "alloc_batch block [{i}] was clobbered (aliasing?)"
        );
    }
}

// ---------------------------------------------------------------------------
// Core: alloc_batch returns valid, aligned, distinct, writable blocks at every
// (size, n) — including n > TCACHE_CAP (the refill-remainder path).
// ---------------------------------------------------------------------------

#[test]
fn alloc_batch_valid_all_size_n() {
    for &size in SIZES {
        let layout = Layout::from_size_align(size, 8).unwrap();
        for &n in BATCH_NS {
            // SAFETY: `layout` is a valid non-zero Layout; we free every block
            // via dealloc_batch at the end.
            let blocks = unsafe {
                let b = alloc_batch_valid(n, layout);
                write_distinct_patterns(&b);
                b
            };
            // SAFETY: every entry of `blocks` came from alloc_batch above with
            // `layout`; each is freed exactly once here.
            unsafe { GLOBAL.dealloc_batch(layout, &blocks) };
        }
    }
}

// ---------------------------------------------------------------------------
// Cross-compatibility: blocks from alloc_batch freed via SCALAR dealloc (one
// by one), and blocks from scalar alloc freed via dealloc_batch. Proves the
// batch and scalar paths share one consistent free-list / magazine substrate.
// ---------------------------------------------------------------------------

#[test]
fn alloc_batch_freed_via_scalar_dealloc() {
    let layout = Layout::from_size_align(64, 8).unwrap();
    for &n in &[17, 64] {
        // SAFETY: alloc_batch fills valid blocks; we free each via scalar dealloc.
        let blocks = unsafe { alloc_batch_valid(n, layout) };
        for &p in &blocks {
            // SAFETY: `p` was allocated by `GLOBAL` with `layout` above; freed once.
            unsafe { GLOBAL.dealloc(p, layout) };
        }
    }
}

#[test]
fn scalar_alloc_freed_via_dealloc_batch() {
    let layout = Layout::from_size_align(64, 8).unwrap();
    for &n in &[17, 64] {
        let mut blocks: Vec<*mut u8> = Vec::with_capacity(n);
        for _ in 0..n {
            // SAFETY: layout is valid; GLOBAL never returns null here.
            let p = unsafe { GLOBAL.alloc(layout) };
            assert!(!p.is_null());
            blocks.push(p);
        }
        // SAFETY: every entry was allocated by GLOBAL with `layout`; freed once.
        unsafe { GLOBAL.dealloc_batch(layout, &blocks) };
    }
}

// ---------------------------------------------------------------------------
// No-aliasing between a batch and a concurrent scalar allocation: after a
// batch is live, a scalar alloc must NOT hand back a pointer still in the
// batch (would be a double-issue). Then freeing both sets leaves the heap sane.
// ---------------------------------------------------------------------------

#[test]
fn batch_and_scalar_do_not_alias() {
    let layout = Layout::from_size_align(128, 8).unwrap();
    let n = 64;
    // SAFETY: valid layout; both sets freed at the end.
    let batch = unsafe { alloc_batch_valid(n, layout) };
    // While the batch is live, do a scalar alloc and check it is distinct.
    // SAFETY: valid layout.
    let scalar = unsafe { GLOBAL.alloc(layout) };
    assert!(!scalar.is_null());
    assert!(
        !batch.contains(&scalar),
        "scalar alloc handed back a pointer still live in the batch",
    );
    // SAFETY: both sets allocated by GLOBAL with `layout`; freed once.
    unsafe {
        GLOBAL.dealloc(scalar, layout);
        GLOBAL.dealloc_batch(layout, &batch);
    }
}

// ---------------------------------------------------------------------------
// Warm steady state: repeated alloc_batch + dealloc_batch cycles must not
// corrupt the heap. After K cycles, a fresh allocation must still succeed and
// be usable — the bench's exact pattern (drain-then-refill oscillation).
// ---------------------------------------------------------------------------

#[test]
fn batch_cycle_warm_steady_state() {
    let layout = Layout::from_size_align(48, 8).unwrap();
    for &n in &[8, 17, 64] {
        for _ in 0..50 {
            // SAFETY: valid layout; freed each cycle via dealloc_batch.
            let blocks = unsafe {
                let b = alloc_batch_valid(n, layout);
                write_distinct_patterns(&b);
                b
            };
            // SAFETY: every entry from the alloc_batch above; freed once.
            unsafe { GLOBAL.dealloc_batch(layout, &blocks) };
        }
        // After the cycles, a fresh scalar alloc must still work (heap sane).
        // SAFETY: valid layout.
        let p = unsafe { GLOBAL.alloc(layout) };
        assert!(!p.is_null(), "heap unusable after {n}-batch cycles");
        // SAFETY: `p` allocated above; freed once.
        unsafe { GLOBAL.dealloc(p, layout) };
    }
}

// ---------------------------------------------------------------------------
// dealloc_batch skips nulls defensively (matching the scalar dealloc contract
// and `flush_class`'s null-skip), so a partial fill freed through it is safe.
// ---------------------------------------------------------------------------

#[test]
fn dealloc_batch_skips_nulls() {
    let layout = Layout::from_size_align(32, 8).unwrap();
    // SAFETY: valid layout.
    let mut blocks = unsafe { alloc_batch_valid(32, layout) };
    // Punch some holes (null out a few, but keep the pointers for separate free).
    let mut saved: Vec<*mut u8> = Vec::new();
    for i in (0..blocks.len()).step_by(3) {
        saved.push(blocks[i]);
        blocks[i] = std::ptr::null_mut();
    }
    // SAFETY: nulls are skipped; non-null entries freed once. Then free the
    // saved ones individually.
    unsafe {
        GLOBAL.dealloc_batch(layout, &blocks);
        for &p in &saved {
            GLOBAL.dealloc(p, layout);
        }
    }
}

// ---------------------------------------------------------------------------
// Mixed sizes in one process: several size classes exercised back-to-back
// confirms the per-class magazine drain + refill routing keys correctly on
// the classified class (not a stale class from a prior call).
// ---------------------------------------------------------------------------

#[test]
fn batch_mixed_size_classes_back_to_back() {
    // SAFETY: each layout is valid; each batch freed via dealloc_batch.
    unsafe {
        for &size in SIZES {
            let layout = Layout::from_size_align(size, 8).unwrap();
            let b = alloc_batch_valid(33, layout); // 33 > TCACHE_CAP → refill path
            write_distinct_patterns(&b);
            GLOBAL.dealloc_batch(layout, &b);
        }
    }
}
