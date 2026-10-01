#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]
//! R11 P4-1: chunked shard index vs a set model under random one-shard churn
//! (splits, merges, duplicates, absent keys between and beyond blocks).

use std::collections::BTreeMap;

use sefer_alloc::registry::segment_route::{
    RouteDirectory, RouteError, RouteKind, RouteRegistration,
};

const SEGMENT: usize = 4 * 1024 * 1024;
const BASE_SEGMENT: usize = 1 << 16;
#[cfg(not(miri))]
const KEYS: usize = 700;
#[cfg(miri)]
const KEYS: usize = 150;
#[cfg(not(miri))]
const ROUNDS: usize = 40;
#[cfg(miri)]
const ROUNDS: usize = 4;
#[cfg(not(miri))]
const OPS: usize = 400;
#[cfg(miri)]
const OPS: usize = 200;

fn ptr_of(key: usize) -> *mut u8 {
    core::ptr::without_provenance_mut(key)
}

fn check(
    directory: &RouteDirectory,
    keys: &[usize],
    live: &BTreeMap<usize, RouteRegistration<'_>>,
) {
    for &key in keys {
        let pin = directory.lookup(ptr_of(key));
        assert_eq!(pin.is_some(), live.contains_key(&key), "key {key:#x}");
        if let (Some(pin), Some(route)) = (pin, live.get(&key)) {
            assert_eq!(pin.incarnation(), route.incarnation());
        }
    }
    assert_eq!(directory.live_route_census_for_test(), (0, 0, live.len()));
}

#[test]
fn chunked_shard_matches_set_model_under_random_churn() {
    let directory = RouteDirectory::new();
    let keys: Vec<usize> = (BASE_SEGMENT..)
        .map(|v| v * SEGMENT)
        .filter(|&k| RouteDirectory::shard_index_for_test(k) == 0)
        .take(KEYS)
        .collect();
    let mut live: BTreeMap<usize, RouteRegistration<'_>> = BTreeMap::new();
    let mut rng = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    for round in 0..ROUNDS {
        // Grow-heavy rounds then shrink-heavy rounds force splits and merges.
        let grow = round % 4 < 2;
        for _ in 0..OPS {
            let key = keys[(next() % KEYS as u64) as usize];
            let insert = (next() % 100) < if grow { 80 } else { 20 };
            if insert {
                let res =
                    directory.register(ptr_of(key), SEGMENT, ptr_of(key), 1, RouteKind::Large);
                match (live.contains_key(&key), res) {
                    (false, Ok(route)) => {
                        live.insert(key, route);
                    }
                    (true, Err(RouteError::Duplicate)) => {}
                    (present, other) => panic!("present={present} got {:?}", other.err()),
                }
            } else {
                live.remove(&key);
            }
        }
        check(&directory, &keys, &live);
    }
    live.clear();
    check(&directory, &keys, &live);
}

#[test]
fn old_pin_survives_unlink_and_final_free_after_block_merge() {
    let directory = RouteDirectory::new();
    let keys: Vec<usize> = (BASE_SEGMENT..)
        .map(|v| v * SEGMENT)
        .filter(|&k| RouteDirectory::shard_index_for_test(k) == 0)
        .take(KEYS)
        .collect();
    let routes: Vec<_> = keys
        .iter()
        .map(|&k| {
            directory
                .register(ptr_of(k), SEGMENT, ptr_of(k), 7, RouteKind::Large)
                .unwrap()
        })
        .collect();
    let pins: Vec<_> = keys
        .iter()
        .map(|&k| directory.lookup(ptr_of(k)).unwrap())
        .collect();
    drop(routes);
    for &k in &keys {
        assert!(directory.lookup(ptr_of(k)).is_none());
    }
    assert_eq!(directory.live_route_census_for_test(), (0, 0, 0));
    for pin in &pins {
        assert_eq!(pin.owner(), 7);
    }
    drop(pins);
}
