//! `LargeCacheConfig::route_pin_cache_slots`: resolution (rounding, clamping,
//! default) and the effect on the foreign-free path of a real
//! `#[global_allocator]` built with `.route_pin_cache_slots(8)`.
//!
//! The effect test frees one block per owner segment in turn over 16 segments
//! whose indices are pairwise distinct modulo 32 but pigeonhole-collide
//! modulo 8: a cache that really has 8 slots cannot keep more than 7 of them
//! hot (hit rate <= 7/16), while a 32-slot cache (the default) keeps all 16
//! (hit rate ~15/16). The 5/8 bound separates the two.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals"
))]

use std::alloc::{GlobalAlloc, Layout};
use std::thread;

use sefer_alloc::registry::segment_route::RoutePin;
use sefer_alloc::{LargeCacheConfig, SeferAlloc};

const SEGMENT: usize = 4 * 1024 * 1024;
const BASES: usize = 16;
const ROUNDS: usize = 16;
const CONFIG: LargeCacheConfig = LargeCacheConfig::new().route_pin_cache_slots(8);

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::with_config(CONFIG);

fn slots_of(requested: Option<usize>) -> usize {
    let cfg = match requested {
        Some(n) => LargeCacheConfig::new().route_pin_cache_slots(n),
        None => LargeCacheConfig::new(),
    };
    cfg.dbg_resolved_route_pin_cache_slots()
}

#[test]
fn slots_resolve_to_a_power_of_two_between_4_and_64() {
    assert_eq!(slots_of(None), 32, "default");
    for (requested, resolved) in [
        (0, 4),
        (1, 4),
        (3, 4),
        (4, 4),
        (5, 4),
        (7, 4),
        (8, 8),
        (9, 8),
        (15, 8),
        (16, 16),
        (31, 16),
        (32, 32),
        (33, 32),
        (63, 32),
        (64, 64),
        (65, 64),
        (usize::MAX, 64),
    ] {
        assert_eq!(slots_of(Some(requested)), resolved, "requested {requested}");
    }
}

/// `BASES` bases with pairwise distinct `(base / SEGMENT) % 32`, `ROUNDS`
/// blocks each; everything else issued on the way is freed on this thread.
fn blocks_over_distinct_slots(layout: Layout) -> Vec<Vec<usize>> {
    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut surplus = Vec::new();
    for _ in 0..400_000 {
        // SAFETY: valid nonzero Layout; null is checked right away.
        let p = unsafe { GLOBAL.alloc(layout) };
        assert!(!p.is_null(), "allocation failed");
        let addr = p.expose_provenance();
        let base = addr & !(SEGMENT - 1);
        let slot = (base / SEGMENT) % 32;
        let known = groups.iter().position(|(b, _)| *b == base);
        let fresh_slot = groups.iter().all(|(b, _)| (b / SEGMENT) % 32 != slot);
        let keep = match known {
            Some(i) if groups[i].1.len() < ROUNDS => {
                groups[i].1.push(addr);
                true
            }
            Some(_) => false,
            None if groups.len() < BASES && fresh_slot => {
                groups.push((base, vec![addr]));
                true
            }
            None => false,
        };
        if !keep {
            surplus.push(addr);
        }
        if groups.len() == BASES && groups.iter().all(|(_, v)| v.len() == ROUNDS) {
            break;
        }
    }
    assert!(
        groups.len() == BASES && groups.iter().all(|(_, v)| v.len() == ROUNDS),
        "could not cover {BASES} bases with distinct slot indices"
    );
    for addr in surplus {
        // SAFETY: live allocation issued above with this Layout, freed once.
        unsafe { GLOBAL.dealloc(core::ptr::with_exposed_provenance_mut::<u8>(addr), layout) };
    }
    groups.into_iter().map(|(_, v)| v).collect()
}

#[test]
fn configured_slot_count_bounds_the_cache_reach() {
    let layout = Layout::from_size_align(1024, 8).unwrap();
    // The owner (this thread) binds a heap with CONFIG and publishes the count.
    let groups = blocks_over_distinct_slots(layout);
    let order: Vec<usize> = (0..ROUNDS)
        .flat_map(|r| groups.iter().map(move |g| g[r]))
        .collect();
    let (h0, m0, ..) = RoutePin::dbg_route_pin_cache_totals();
    // The producer only frees: it never binds a heap, yet adopts the count.
    let slots = thread::spawn(move || {
        for addr in order {
            // SAFETY: unique live allocation, exact original Layout, freed once.
            unsafe { GLOBAL.dealloc(core::ptr::with_exposed_provenance_mut::<u8>(addr), layout) };
        }
        RoutePin::dbg_route_pin_cache_slots()
    })
    .join()
    .expect("producer panicked");
    let (h1, m1, ..) = RoutePin::dbg_route_pin_cache_totals();
    let (hits, misses) = (h1 - h0, m1 - m0);
    assert_eq!(
        slots, 8,
        "the producer's cache adopted the configured slot count"
    );
    let frees = (BASES * ROUNDS) as u64;
    assert!(
        hits * 8 < frees * 5,
        "8 slots cannot hold {BASES} colliding bases: {hits} hits / {misses} misses of {frees}"
    );
    assert!(misses >= BASES as u64, "first touch of each base misses");
}
