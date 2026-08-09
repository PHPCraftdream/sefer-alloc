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
use slotmap::{DefaultKey, SlotMap};

/// Fixture size for the pre-populated get/iterate benches — large enough
/// that `get`'s single-indirection lookup is not dominated by allocator
/// warm-up noise, small enough that calibration stays fast.
const PREPOPULATE: u64 = 1_000;

fn main() {
    // Task #792/F14, verified against bench-scale-tool 0.1.0's actual
    // source (not just its doc comments): `Harness` exposes NO way to
    // override where its manifest lives -- `manifest_path()` is private to
    // that crate. It walks up from `CARGO_MANIFEST_DIR` for the nearest
    // `[workspace]`-declaring `Cargo.toml`, falling back to
    // `<crate>/../../bench-iters.txt` only when no such ancestor exists
    // (e.g. this crate extracted standalone from a published tarball).
    // `load_manifest` on a missing file returns an empty map (no crash),
    // and each workload self-heals via a 1s JIT calibration pass instead of
    // aborting -- but a plain `cargo bench` run (no `--calibrate` flag)
    // STILL attempts `save_manifest` at the end whenever any workload was
    // JIT-calibrated, which is every workload on a fresh/missing manifest.
    // For a standalone-extracted package this targets a path OUTSIDE the
    // package root -- exactly the exposure F14 flags. There is no fix
    // reachable from THIS crate's own source: the harness's manifest
    // routing lives entirely in the separately-published bench-scale-tool
    // crate, which this workspace does not vendor or patch. Filed as a
    // known residual limitation rather than silently claimed fixed --
    // see docs/CORRECTNESS_OPEN_ITEMS.md for the tracking entry. The
    // sibling `crates/region/benches/bench-iters.txt` (a plain hand-edited
    // copy of the pinned counts, matching bench-scale-tool's own manifest
    // format) is kept for human reference next to the bench source, but
    // the harness has no way to actually be pointed at it.
    let mut h = Harness::new("region_bench", env!("CARGO_MANIFEST_DIR"));

    // ── Region<T> — single-threaded ──────────────────────────────────────

    h.bench_batched("st/insert", Region::<u64>::new, |mut r| {
        black_box(r.insert(black_box(42u64)));
    });

    {
        let mut r: Region<u64> = Region::new();
        let handles: Vec<Handle<u64>> = (0..PREPOPULATE).map(|i| r.insert(i)).collect();
        let mid = handles[(PREPOPULATE / 2) as usize];
        h.bench("st/get_hit", move || {
            black_box(r.get(black_box(mid)));
        });
    }

    {
        let mut r: Region<u64> = Region::new();
        let handles: Vec<Handle<u64>> = (0..PREPOPULATE).map(|i| r.insert(i)).collect();
        let stale = handles[0];
        r.remove(stale);
        h.bench("st/get_stale", move || {
            black_box(r.get(black_box(stale)));
        });
    }

    h.bench_batched(
        "st/remove",
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
            let _ = r.insert(i);
        }
        h.bench("st/iterate", move || {
            let sum: u64 = r.iter().sum();
            black_box(sum);
        });
    }

    // Holey iteration: 50% holes (2,000 insert, remove every other, 1,000 live).
    {
        let mut r: Region<u64> = Region::new();
        let mut handles: Vec<Handle<u64>> = Vec::new();
        // Insert 2,000 values
        for i in 0..2_000 {
            handles.push(r.insert(i));
        }
        // Remove every other one, leaving 1,000 live values with 1,000 holes
        for i in (0..2_000).step_by(2) {
            r.remove(handles[i]);
        }
        h.bench("st/holey_sweep", move || {
            let sum: u64 = r.iter().sum();
            black_box(sum);
        });
    }

    // Sparse iteration: 90% holes (10,000 insert, remove 9,000, 1,000 live).
    {
        let mut r: Region<u64> = Region::new();
        let mut handles: Vec<Handle<u64>> = Vec::new();
        // Insert 10,000 values
        for i in 0..10_000 {
            handles.push(r.insert(i));
        }
        // Remove 9,000, leaving 1,000 live values with 9,000 holes
        for handle in handles.iter().take(9_000) {
            r.remove(*handle);
        }
        h.bench("st/sparse_sweep", move || {
            let sum: u64 = r.iter().sum();
            black_box(sum);
        });
    }

    // Steady-state churn: remove then reinsert the same entry, keeping map size constant.
    // Times ONLY the churn cost, not teardown or cold allocation.
    {
        let mut r: Region<u64> = Region::new();
        let mut handle: Handle<u64> = r.insert(42u64);
        h.bench("st/churn", move || {
            let v = r.remove(handle).unwrap();
            handle = r.insert(v);
        });
    }

    // ── SyncRegion<T> — RwLock-wrapped one-shot convenience methods ──────

    h.bench_batched("sync/insert", SyncRegion::<u64>::new, |sr| {
        black_box(sr.insert(black_box(42u64)));
    });

    {
        let sr: SyncRegion<u64> = SyncRegion::new();
        let handles: Vec<Handle<u64>> = (0..PREPOPULATE).map(|i| sr.insert(i)).collect();
        let mid = handles[(PREPOPULATE / 2) as usize];
        h.bench("sync/get_cloned_hit", move || {
            black_box(sr.get_cloned(black_box(mid)));
        });
    }

    h.bench_batched(
        "sync/remove",
        || {
            let sr: SyncRegion<u64> = SyncRegion::new();
            let handle = sr.insert(1u64);
            (sr, handle)
        },
        |(sr, handle)| {
            black_box(sr.remove(handle));
        },
    );

    // Steady-state churn for SyncRegion: same pattern as st/churn, through the one-shot API.
    {
        let sr: SyncRegion<u64> = SyncRegion::new();
        let mut handle: Handle<u64> = sr.insert(42u64);
        h.bench("sync/churn", move || {
            let v = sr.remove(handle).unwrap();
            handle = sr.insert(v);
        });
    }

    // ── raw slotmap::SlotMap — wrapper-overhead comparison ───────────────
    //
    // `Region<T>` is documented as "a thin typed membrane" that "delegates
    // every operation to slotmap" (region.rs's own module doc). These
    // workloads run the IDENTICAL operations directly against
    // `slotmap::SlotMap<DefaultKey, u64>` (`Region`'s own backing type),
    // with no `Handle<T>` wrapping, so `st/*` vs `raw/*` isolates exactly
    // the cost of the membrane itself — the PhantomData-branded `Handle<T>`
    // and the one-line delegation methods — with the underlying slotmap
    // algorithm held constant.

    h.bench_batched("raw/insert", SlotMap::<DefaultKey, u64>::new, |mut m| {
        black_box(m.insert(black_box(42u64)));
    });

    {
        let mut m: SlotMap<DefaultKey, u64> = SlotMap::new();
        let keys: Vec<DefaultKey> = (0..PREPOPULATE).map(|i| m.insert(i)).collect();
        let mid = keys[(PREPOPULATE / 2) as usize];
        h.bench("raw/get_hit", move || {
            black_box(m.get(black_box(mid)));
        });
    }

    h.bench_batched(
        "raw/remove",
        || {
            let mut m: SlotMap<DefaultKey, u64> = SlotMap::new();
            let key = m.insert(1u64);
            (m, key)
        },
        |(mut m, key)| {
            black_box(m.remove(key));
        },
    );

    // Steady-state churn for raw SlotMap: same pattern as st/churn.
    {
        let mut m: SlotMap<DefaultKey, u64> = SlotMap::new();
        let mut k: DefaultKey = m.insert(42u64);
        h.bench("raw/churn", move || {
            let v = m.remove(k).unwrap();
            k = m.insert(v);
        });
    }

    h.run();
}
