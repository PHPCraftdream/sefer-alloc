//! Test support for `tests/b4_r1_pin_cache.rs`: allocate through the real global
//! allocator until a set of segment bases is covered.

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::SeferAlloc;

const SEGMENT: usize = 4 * 1024 * 1024;

fn base_of(addr: usize) -> usize {
    addr & !(SEGMENT - 1)
}

/// Allocate blocks of `layout` through `global` until `bases` segment bases are
/// covered with `count` blocks each; returns the kept blocks grouped by base.
/// With `slot_mod != 0` only bases with pairwise distinct
/// `(base / SEGMENT) % slot_mod` are kept (congruent ones evict each other).
/// Surplus blocks are freed on the owner thread before returning.
pub fn allocate_across_bases_with(
    global: &SeferAlloc,
    layout: Layout,
    count: usize,
    bases: usize,
    slot_mod: usize,
) -> Vec<(usize, Vec<usize>)> {
    let slot_of = |base: usize| {
        if slot_mod == 0 {
            0
        } else {
            (base / SEGMENT) % slot_mod
        }
    };
    let mut map: Vec<(usize, Vec<usize>)> = Vec::new();
    // Allocation index at which each map entry last received a block.
    let mut last_seen: Vec<usize> = Vec::new();
    let mut surplus = Vec::new();
    // A recycled owner heap can hand out a few leftover free-list blocks of old
    // segments first. Such a base never reaches `count` (its segment is not
    // carved any more) and would hold a map entry for good, so an incomplete
    // entry that has received no block for this many allocations is dropped
    // (its blocks join the surplus) when a new base appears.
    const STALE_AFTER: usize = 1024;
    for i in 0..200_000 {
        // SAFETY: valid nonzero Layout; null is checked immediately.
        let p = unsafe { global.alloc(layout) };
        assert!(!p.is_null(), "small allocation failed");
        // Exposed: every consumer rebuilds the pointer via
        // `with_exposed_provenance_mut`.
        let addr = p.expose_provenance();
        match map.iter().position(|(base, _)| *base == base_of(addr)) {
            Some(at) => {
                last_seen[at] = i;
                let v = &mut map[at].1;
                if v.len() < count {
                    v.push(addr);
                } else {
                    surplus.push(addr);
                }
            }
            None => {
                let mut at = 0;
                while at < map.len() {
                    if map[at].1.len() < count && i - last_seen[at] > STALE_AFTER {
                        let (_, stale) = map.remove(at);
                        last_seen.remove(at);
                        surplus.extend(stale);
                    } else {
                        at += 1;
                    }
                }
                let b = base_of(addr);
                let clash = slot_mod != 0 && map.iter().any(|(m, _)| slot_of(*m) == slot_of(b));
                if map.len() < bases && !clash {
                    map.push((b, vec![addr]));
                    last_seen.push(i);
                } else {
                    surplus.push(addr);
                }
            }
        }
        if map.len() == bases && map.iter().all(|(_, v)| v.len() == count) {
            break;
        }
    }
    assert!(
        map.len() == bases && map.iter().all(|(_, v)| v.len() == count),
        "could not cover {bases} bases with {count} blocks each (kept {:?}, surplus {})",
        map.iter()
            .map(|(b, v)| (b / SEGMENT, v.len()))
            .collect::<Vec<_>>(),
        surplus.len()
    );
    // Free the surplus on the owner thread: own-thread frees, no pin-cache
    // interaction.
    for addr in surplus {
        // SAFETY: live allocation issued above, exact original Layout, freed
        // once on the owning thread.
        unsafe { global.dealloc(core::ptr::with_exposed_provenance_mut::<u8>(addr), layout) };
    }
    map
}
