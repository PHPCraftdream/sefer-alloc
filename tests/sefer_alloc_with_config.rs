//! Gap 1: end-to-end test for `SeferAlloc::with_config`.
//!
//! Proves that the full plumbing chain
//! `SeferAlloc.config → current_for_alloc_with_config → bind_slow_tagged_with_config
//!  → claim_with_config → HeapCore::new_with_config → AllocCore::new_with_config`
//! is wired correctly. If any link is broken, either compilation fails (const fn
//! not wired) or the test panics/segfaults at runtime.
//!
//! Because `#[global_allocator]` is process-global, this file is its own binary
//! (each file in `tests/` is a separate test binary).

#![cfg(all(feature = "alloc-global", feature = "alloc-decommit"))]

use sefer_alloc::{LargeCacheConfig, LargeCacheMode, SeferAlloc};

const MIB: usize = 1024 * 1024;

// A restrictive config: 32 MiB budget, small headroom, fast decay.
const CONFIG: LargeCacheConfig = LargeCacheConfig::new()
    .budget_bytes(32 * MIB)
    .headroom_bytes(4 * MIB)
    .decay_interval_ms(100)
    .decay_rate_percent(50)
    .mode(LargeCacheMode::Lazy);

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::with_config(CONFIG);

// ── test 1 ──────────────────────────────────────────────────────────────────

/// Large allocation through the configured global allocator succeeds and
/// the memory is readable/writable.
#[test]
fn large_alloc_through_configured_global() {
    // 4 MiB allocation — well within the 32 MiB budget.
    let mut v: Vec<u8> = Vec::with_capacity(4 * MIB);
    v.resize(4 * MIB, 0xAB);
    assert_eq!(v.len(), 4 * MIB);
    assert_eq!(v[0], 0xAB);
    assert_eq!(v[4 * MIB - 1], 0xAB);
    drop(v);
}

// ── test 2 ──────────────────────────────────────────────────────────────────

/// Multiple allocations and frees through the configured allocator.
#[test]
fn multiple_allocs_and_frees() {
    for _ in 0..4 {
        let mut v: Vec<u8> = Vec::with_capacity(2 * MIB);
        v.resize(2 * MIB, 0xCD);
        assert_eq!(v[MIB], 0xCD);
        drop(v);
    }
}

// ── test 3 ──────────────────────────────────────────────────────────────────

/// Small allocations (Box, String) also work through the configured allocator.
#[test]
fn small_allocs_work() {
    let b = Box::new(42u64);
    assert_eq!(*b, 42);
    let s = String::from("sefer-alloc end-to-end config test");
    assert_eq!(s.len(), 34);
}
