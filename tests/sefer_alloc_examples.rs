//! Runnable versions of the `SeferAlloc` doc-comment examples (task T1b).
//!
//! Each test here is the real, executed counterpart of a snippet that used to
//! live as a `no_run` doctest in `src/lib.rs` / `src/global/sefer_alloc.rs`.
//! The crate now has zero compiled doctests (see `CLAUDE.md` "No doctests"):
//! the doc comments carry non-executed `text` illustrations and point readers
//! here (or to the feature-specific test files) for the runnable form.
//!
//! This file holds the front-page examples — the simple `SeferAlloc::new()`
//! global install and the `stats()` monitoring snapshot. The `with_config`
//! install example lives in `tests/sefer_alloc_with_config.rs`; the
//! `Region` example is covered by `tests/region_invariants.rs`.

#![cfg(feature = "alloc-global")]

use sefer_alloc::SeferAlloc;

// Simple-form installation — mirrors the `SeferAlloc` type-level doc example:
//     #[global_allocator]
//     static GLOBAL: SeferAlloc = SeferAlloc::new();
//
// Each file in `tests/` is its own binary, so installing the global allocator
// here is isolated from the rest of the suite.
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

/// Allocations routed through the freshly installed global allocator succeed
/// and round-trip correctly (the `SeferAlloc::new()` install example).
#[test]
fn new_global_allocator_serves_allocs() {
    let b = Box::new(0xABCD_u64);
    assert_eq!(*b, 0xABCD);

    let mut v: Vec<u8> = Vec::with_capacity(1024);
    v.resize(1024, 0x5A);
    assert_eq!(v.len(), 1024);
    assert_eq!(v[0], 0x5A);
    assert_eq!(v[1023], 0x5A);
}

/// `stats()` returns a readable snapshot: the `segments_live` derivation and
/// the cache / cross-thread counters are all accessible, and the live-count
/// derivation is sound (`reserved >= released`). This is the executed form of
/// the `stats()` / process-wide monitoring doc examples.
#[test]
fn stats_snapshot_fields_are_readable() {
    // Touch the allocator so its counters are non-trivial.
    let mut v: Vec<u8> = Vec::with_capacity(256);
    v.resize(256, 0x11);
    drop(v);

    let stats = GLOBAL.stats();

    // The doc example's `segments_live` derivation — must not underflow and
    // must respect reserved >= released (a sound allocator never releases more
    // segments than it has reserved).
    let segments_live = stats
        .segments_reserved_total
        .saturating_sub(stats.segments_released_total);
    let _ = segments_live;
    assert!(
        stats.segments_reserved_total >= stats.segments_released_total,
        "reserved ({}) must be >= released ({})",
        stats.segments_reserved_total,
        stats.segments_released_total
    );

    // Process-wide monitoring contract: these fields exist and are readable
    // regardless of which feature flags populated them.
    let _ = stats.tcache_hits;
    let _ = stats.ring_overflows;
}
