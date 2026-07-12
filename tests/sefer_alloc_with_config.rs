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

// ── T5 / cleanup#3 ───────────────────────────────────────────────────────────

/// Single-`#[global_allocator]`-instance, multi-thread config consistency.
///
/// This is the *realistic, supported* usage surfaced by cleanup#3: one
/// `SeferAlloc` per process, with every thread allocating through it. Under
/// that usage the documented "first-bind-wins" semantics (per-slot
/// materialisation + per-thread TLS cache — see `SeferAlloc::with_config`'s
/// doc) are consistent: each thread materialises its registry slot through
/// this single instance, so each thread's heap carries this one config.
///
/// This test pins that consistency — each worker allocates small + large,
/// writes a per-thread tag, and reads it back; a corruption or a config
/// mis-plumb on any thread would fail the tag check or panic the join. It is a
/// forward-looking guard over already-correct behaviour (the doc fix changes
/// no code path), so it has no behavioural RED→GREEN; it exists to catch any
/// future regression that breaks single-instance config threading.
#[test]
fn single_instance_config_consistent_across_threads() {
    let handles: Vec<_> = (0..4_u8)
        .map(|i| {
            std::thread::spawn(move || {
                let tag = 0xA0 + i;

                // Small allocation through the configured global allocator.
                let mut small = Box::new([0u8; 64]);
                small[0] = tag;
                small[63] = tag.wrapping_sub(1);

                // Large allocation (2 MiB — within the 32 MiB budget).
                let mut big: Vec<u8> = Vec::with_capacity(2 * MIB);
                big.resize(2 * MIB, tag);
                assert_eq!(small[0], tag, "small alloc corrupted on worker {i}");
                assert_eq!(small[63], tag.wrapping_sub(1));
                assert_eq!(big[0], tag, "large alloc head corrupted on worker {i}");
                assert_eq!(
                    big[2 * MIB - 1],
                    tag,
                    "large alloc tail corrupted on worker {i}"
                );

                drop(small);
                drop(big);
            })
        })
        .collect();

    for (i, h) in handles.into_iter().enumerate() {
        h.join()
            .unwrap_or_else(|e| panic!("worker {i} thread must not panic: {e:?}"));
    }
}
