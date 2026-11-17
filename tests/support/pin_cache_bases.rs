//! Test support for `tests/b4_r1_pin_cache.rs`: allocate through the real global
//! allocator until a set of segment bases is covered.

use std::alloc::{GlobalAlloc, Layout};
use std::ptr::without_provenance_mut;
use std::thread;
use std::time::{Duration, Instant};

use sefer_alloc::alloc_core::segment_header::OWNER_ID_FALLBACK;
use sefer_alloc::registry::segment_route::RouteDirectory;
use sefer_alloc::SeferAlloc;

const SEGMENT: usize = 4 * 1024 * 1024;

fn base_of(addr: usize) -> usize {
    addr & !(SEGMENT - 1)
}

/// Allocate blocks of `layout` through `global` until `bases` segment bases are
/// covered with `count` blocks each; returns the kept blocks grouped by base.
/// With `slot_mod != 0` only bases with pairwise distinct
/// `(base / SEGMENT) % slot_mod` are kept (congruent ones evict each other).
/// Returns the kept blocks grouped by base plus the surplus (both still
/// live; the caller frees or keeps them).
fn allocate_once(
    global: &SeferAlloc,
    layout: Layout,
    count: usize,
    bases: usize,
    slot_mod: usize,
) -> (Vec<(usize, Vec<usize>)>, Vec<usize>) {
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
    // NOT freed here: the caller decides whether to keep or discard the
    // attempt after the ownership check below.
    (map, surplus)
}

/// Bounded wait before retrying after a fallback-owned attempt: preceding
/// test threads still hold their registry slots (their TLS `Drop` runs at
/// thread exit); without `alloc-decommit` slot recycle timing differs, so a
/// retry may need to wait for those leases to release.
const RETRY_DEADLINE: Duration = Duration::from_secs(60);
const RETRY_SLEEP: Duration = Duration::from_millis(20);

/// Allocate blocks of `layout` through `global` until `bases` segment bases
/// are covered with `count` blocks each; returns the kept blocks grouped by
/// base. With `slot_mod != 0` only bases with pairwise distinct
/// `(base / SEGMENT) % slot_mod` are kept (congruent ones evict each other).
/// Surplus blocks are freed on the owner thread before returning.
///
/// Determinism guard: every kept base is verified to be owned by a REGISTRY
/// route (not the `OWNER_ID_FALLBACK` heap). A fallback-owned base makes the
/// foreign-free path handle the free synchronously through the fallback hook
/// (`routing.rs` `cacheable == false`) and never inserts a cached pin, which
/// silently voids the refs oracles of the callers. If any kept base is
/// fallback-owned, ALL blocks of the attempt are freed on this thread, the
/// helper waits (bounded) for the preceding test threads' leases/slots to
/// release, and the whole allocation is retried. Bounded retries do not
/// weaken any oracle: the retry completes before any caller captures probes,
/// counters or totals (the helper returns first), and the forget-drop mutant
/// coverage is untouched — that mutant is detected AFTER the worker join,
/// far downstream of this setup.
pub fn allocate_across_bases_with(
    global: &SeferAlloc,
    layout: Layout,
    count: usize,
    bases: usize,
    slot_mod: usize,
) -> Vec<(usize, Vec<usize>)> {
    let deadline = Instant::now() + RETRY_DEADLINE;
    loop {
        let (map, surplus) = allocate_once(global, layout, count, bases, slot_mod);
        let fallback = map.iter().any(|(base, _)| {
            // SAFETY: `lookup` reads only the address.
            match RouteDirectory::global().lookup(without_provenance_mut::<u8>(*base)) {
                Some(pin) => pin.owner() == OWNER_ID_FALLBACK as usize,
                None => false,
            }
        });
        if !fallback {
            // Free the surplus on the owner thread: own-thread frees, no
            // pin-cache interaction.
            for addr in surplus {
                // SAFETY: live allocation issued above, exact original
                // Layout, freed once on the owning thread.
                unsafe {
                    global.dealloc(core::ptr::with_exposed_provenance_mut::<u8>(addr), layout)
                };
            }
            return map;
        }
        // Fallback-owned bases: free EVERYTHING from this attempt on this
        // thread (kept + surplus), then give the registry time to recycle
        // slots before retrying.
        for (_, kept) in &map {
            for addr in kept {
                // SAFETY: live allocation issued above, exact original
                // Layout, freed once on the owning thread.
                unsafe {
                    global.dealloc(core::ptr::with_exposed_provenance_mut::<u8>(*addr), layout)
                };
            }
        }
        for addr in surplus {
            // SAFETY: live allocation issued above, exact original Layout,
            // freed once on the owning thread.
            unsafe { global.dealloc(core::ptr::with_exposed_provenance_mut::<u8>(addr), layout) };
        }
        assert!(
            Instant::now() < deadline,
            "allocations landed on the OWNER_ID_FALLBACK heap: preceding test threads \
             still hold their leased registry slots (without alloc-decommit slot \
             recycle timing differs); the refs oracle requires registry-owned \
             (cacheable) routes"
        );
        thread::sleep(RETRY_SLEEP);
    }
}
