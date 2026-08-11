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
use std::time::Instant;

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
    // see docs/CORRECTNESS_OPEN_ITEMS.md for the tracking entry. A
    // previously-distributed crate-local `bench-iters.txt` was removed in
    // task #820 as misleading (its header falsely claimed the harness read it,
    // when in fact the harness has no way to be pointed at any crate-local
    // file). All JIT-calibration workloads self-heal on a fresh run, and the
    // workspace-level `bench-iters.txt` is the canonical source when running
    // within a workspace.
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

    // Wrong-region handle: a handle minted by a SECOND, distinct `Region`
    // (its own `region_id` from the same process-wide `NEXT_REGION_ID`
    // counter), passed to `.get()` on the FIRST region. Since F2 (task
    // #802), every accessor checks `region_id` before touching the
    // slotmap, so this isolates the cost of the rejecting path the
    // shipping code takes on a cross-Region handle -- distinct from
    // `st/get_stale` (same region, tombstoned slot).
    {
        let mut r: Region<u64> = Region::new();
        let _handles: Vec<Handle<u64>> = (0..PREPOPULATE).map(|i| r.insert(i)).collect();
        let mut other: Region<u64> = Region::new();
        let foreign = other.insert(42u64);
        h.bench("st/get_wrong_region", move || {
            black_box(r.get(black_box(foreign)));
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

    // ── Multi-threaded contention: Region::new() vs. the shared NEXT_REGION_ID
    // counter ──────────────────────────────────────────────────────────────
    //
    // bench_scale_tool::Harness is single-threaded only (verified in this
    // session against other crates in this workspace), so contention
    // throughput is measured manually with real threads -- the same pattern
    // already established in
    // `crates/tagged-index-stack/benches/tagged_index_stack_bench.rs`'s
    // "Multi-threaded contention workloads" section: N threads (capped),
    // each spinning a fixed-duration loop, summed ops/sec printed to stdout
    // after `h.run()` (not through the bench-iters.txt manifest, which is
    // specific to the single-threaded fixed-iteration Harness model).
    //
    // `Region::new()` mints its `region_id` from a single process-wide
    // `AtomicUsize` (`NEXT_REGION_ID`, `region.rs`) via `fetch_add`. This
    // workload measures how that one shared atomic scales as concurrently
    // running threads each construct fresh `Region<u64>`s (and immediately
    // drop them) as fast as possible -- the F2 domain-identity redesign's
    // only cross-thread shared state.
    println!("\n=== Multi-threaded contention benchmarks ===\n");

    let num_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8); // Cap at 8 for consistent benchmarking across machines

    println!(
        "Using {} threads (based on available_parallelism, capped at 8)",
        num_threads
    );

    const DURATION_SECS: u64 = 1;

    let start = Instant::now();
    let mut ops_per_thread = Vec::with_capacity(num_threads);

    std::thread::scope(|s| {
        let mut handles = Vec::with_capacity(num_threads);
        for _ in 0..num_threads {
            let handle = s.spawn(move || {
                let mut ops = 0u64;
                while start.elapsed().as_secs() < DURATION_SECS {
                    let r: Region<u64> = Region::new();
                    black_box(&r);
                    ops += 1;
                }
                ops
            });
            handles.push(handle);
        }
        // Join all threads AFTER they've all started.
        for handle in handles {
            ops_per_thread.push(handle.join().unwrap());
        }
    });

    let total_ops: u64 = ops_per_thread.iter().sum();
    let total_ops_per_sec = total_ops / DURATION_SECS;
    println!(
        "contention/region_new: {} ops/sec total ({} threads, {} sec)",
        total_ops_per_sec, num_threads, DURATION_SECS
    );
    println!("  Per-thread breakdown: {:?}\n", ops_per_thread);

    println!("=== All contention benchmarks complete ===");
}
