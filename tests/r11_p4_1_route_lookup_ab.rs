#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]
//! R11 P4-1 directional lookup-cost probe (not a gate). Entry point:
//! `RouteDirectory::lookup` on a private directory, single thread, release
//! build, all routes hashed to one shard. Run:
//! `cargo test --release --features "production internals bench-internals"
//!  --test r11_p4_1_route_lookup_ab -- --ignored --nocapture`

use std::time::Instant;

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};

const SEGMENT: usize = 4 * 1024 * 1024;
const BASE_SEGMENT: usize = 1 << 16;
const LOOKUPS: usize = 1_000_000;
const SAMPLES: usize = 7;

#[test]
#[ignore = "directional timing probe"]
fn lookup_cost_one_shard() {
    for n in [16usize, 64, 256, 1024, 4096] {
        let directory = RouteDirectory::new();
        let keys: Vec<usize> = (BASE_SEGMENT..)
            .map(|v| v * SEGMENT)
            .filter(|&k| RouteDirectory::shard_index_for_test(k) == 0)
            .take(n)
            .collect();
        let _routes: Vec<_> = keys
            .iter()
            .map(|&k| {
                let p = core::ptr::without_provenance_mut::<u8>(k);
                directory
                    .register(p, SEGMENT, p, 1, RouteKind::Large)
                    .unwrap()
            })
            .collect();
        let mut samples = Vec::new();
        for _ in 0..SAMPLES {
            let mut state = 0x2545_F491_4F6C_DD1Du64;
            let start = Instant::now();
            let mut hits = 0usize;
            for _ in 0..LOOKUPS {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let key = keys[(state % n as u64) as usize];
                hits += usize::from(
                    directory
                        .lookup(core::ptr::without_provenance_mut(key))
                        .is_some(),
                );
            }
            assert_eq!(hits, LOOKUPS);
            samples.push(start.elapsed().as_nanos() as f64 / LOOKUPS as f64);
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "lookup n={n}: median {:.1} ns/lookup (total ns / {LOOKUPS} lookups, median of {SAMPLES}); min {:.1} max {:.1}",
            samples[SAMPLES / 2],
            samples[0],
            samples[SAMPLES - 1]
        );
    }
}
