//! Focused member-local allocation-contract test for `sefer-alloc-core`, so
//! the crate's own CI rows have a test target to run (root-crate coverage
//! stays in the root crate's `tests/alloc_core_invariants.rs`).
//!
//! Self-contained: std + `sefer_alloc_core` only, no dev-dependencies. Kept
//! FAST: small sizes, one allocation.

// The plain `cargo test -p sefer-alloc-core` row compiles this to ZERO tests
// on purpose; the `--all-features` row runs it (turns on `alloc-core`).
#![cfg(feature = "alloc-core")]
// The package denies `unsafe_code`, but the contract under test includes the
// `unsafe fn realloc`/`dealloc` entry points; the test honors their
// documented contracts (live allocation, matching layout, freed exactly
// once).
#![allow(unsafe_code)]

use sefer_alloc_core::alloc_core::alloc_core::AllocCore;

use std::alloc::Layout;

/// One end-to-end contract slice: a valid allocation is written, grown via
/// `realloc` with the prefix preserved across the transfer, and released
/// exactly once with the matching layout.
///
/// Soundness discipline (the whole point of keeping this to ONE test):
/// - every pointer checked non-null AND correctly aligned right after each
///   transfer (`alloc`, `realloc` grow);
/// - the `realloc` old pointer is consumed on the non-null return and never
///   reused or re-freed (per the `realloc` `# Safety` contract);
/// - the final block is deallocated exactly once, with a layout whose
///   size/align exactly match the live allocation (512 bytes, align 8);
/// - no double-free probe, no pointer→usize roundtrip, no read of memory the
///   allocator may leave uninitialized.
#[test]
fn alloc_write_grow_realloc_preserves_prefix_then_single_dealloc() {
    let mut a = AllocCore::new().expect("AllocCore::new failed");

    // 1. Valid allocation: non-null, aligned, writable.
    let initial = 128usize;
    let old_layout = Layout::from_size_align(initial, 8).unwrap();
    let ptr = a.alloc(old_layout);
    assert!(!ptr.is_null(), "alloc returned null");
    assert_eq!(ptr.addr() % 8, 0, "alloc ptr not 8-aligned");

    // SAFETY: `ptr` is a fresh non-null allocation valid for `initial`
    // bytes; we write only that range.
    unsafe {
        for b in 0..initial {
            ptr.add(b).write((b as u8).wrapping_mul(7));
        }
    }

    // 2. Grow via realloc: prefix must survive the transfer.
    let grown_size = 512usize;
    // SAFETY: `ptr` is the exact start of a currently-live allocation made
    // with `old_layout` (size/align match); on the non-null return below the
    // old pointer is consumed by this call and never touched again.
    let grown = unsafe { a.realloc(ptr, old_layout, grown_size) };
    assert!(!grown.is_null(), "realloc grow returned null");
    assert_eq!(grown.addr() % 8, 0, "realloc ptr not 8-aligned");

    // SAFETY: `grown` is the non-null realloc result, valid for
    // `grown_size` bytes; we read only the preserved `initial` prefix.
    unsafe {
        for b in 0..initial {
            assert_eq!(
                grown.add(b).read(),
                (b as u8).wrapping_mul(7),
                "byte {b} not preserved across realloc grow"
            );
        }
    }

    // 3. Release exactly once, with the matching live layout (the block is
    // now `grown_size` bytes at align 8).
    let live_layout = Layout::from_size_align(grown_size, 8).unwrap();
    // SAFETY: `grown` is the live allocation start with exactly the
    // `live_layout` size/align; this is its single deallocation.
    unsafe { a.dealloc(grown, live_layout) };
}
