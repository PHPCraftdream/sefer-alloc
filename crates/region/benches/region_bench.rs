//! `bench-scale-tool` fixed-iteration benches for `Region<T>`/`SyncRegion<T>`
//! (task #656). This crate previously had zero benches of its own — see
//! `docs/BENCHMARKS.md` at the workspace root for the historical container-
//! choice comparison, which was produced by the workspace's own
//! `benches/locality.rs`, not by anything inside this crate.
//!
//! Run:
//! ```text
//! cargo bench -p sefer-region --bench region_bench -- --calibrate 1
//! cargo bench -p sefer-region --bench region_bench
//! ```

use std::hint::black_box;

use bench_scale_tool::Harness;
use sefer_region::{Handle, Region, SyncRegion};

/// Fixture size for the pre-populated get/iterate benches — large enough
/// that `get`'s single-indirection lookup is not dominated by allocator
/// warm-up noise, small enough that calibration stays fast.
const PREPOPULATE: u64 = 1_000;

fn main() {
    let mut h = Harness::new("region_bench", env!("CARGO_MANIFEST_DIR"));

    // ── Region<T> — single-threaded ──────────────────────────────────────

    h.bench_batched("region/insert", Region::<u64>::new, |mut r| {
        black_box(r.insert(black_box(42u64)));
    });

    {
        let mut r: Region<u64> = Region::new();
        let handles: Vec<Handle<u64>> = (0..PREPOPULATE).map(|i| r.insert(i)).collect();
        let mid = handles[(PREPOPULATE / 2) as usize];
        h.bench("region/get_hit", move || {
            black_box(r.get(black_box(mid)));
        });
    }

    {
        let mut r: Region<u64> = Region::new();
        let handles: Vec<Handle<u64>> = (0..PREPOPULATE).map(|i| r.insert(i)).collect();
        let stale = handles[0];
        r.remove(stale);
        h.bench("region/get_stale", move || {
            black_box(r.get(black_box(stale)));
        });
    }

    h.bench_batched(
        "region/remove",
        || {
            let mut r: Region<u64> = Region::new();
            let handle = r.insert(1u64);
            (r, handle)
        },
        |(mut r, handle)| {
            black_box(r.remove(handle));
        },
    );

    {
        let mut r: Region<u64> = Region::new();
        for i in 0..PREPOPULATE {
            r.insert(i);
        }
        h.bench("region/iterate", move || {
            let sum: u64 = r.iter().sum();
            black_box(sum);
        });
    }

    // ── SyncRegion<T> — RwLock-wrapped one-shot convenience methods ──────

    h.bench_batched("sync_region/insert", SyncRegion::<u64>::new, |sr| {
        black_box(sr.insert(black_box(42u64)));
    });

    {
        let sr: SyncRegion<u64> = SyncRegion::new();
        let handles: Vec<Handle<u64>> = (0..PREPOPULATE).map(|i| sr.insert(i)).collect();
        let mid = handles[(PREPOPULATE / 2) as usize];
        h.bench("sync_region/get_cloned_hit", move || {
            black_box(sr.get_cloned(black_box(mid)));
        });
    }

    h.bench_batched(
        "sync_region/remove",
        || {
            let sr: SyncRegion<u64> = SyncRegion::new();
            let handle = sr.insert(1u64);
            (sr, handle)
        },
        |(sr, handle)| {
            black_box(sr.remove(handle));
        },
    );

    h.run();
}
