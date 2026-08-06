//! Capacity telemetry probe for `Region<T>`'s `slotmap::SlotMap` backing
//! (task #656, using `captrack` — <https://github.com/PHPCraftdream/captrack>).
//!
//! `slotmap::SlotMap` is not one of captrack's natively-wrapped collection
//! types (`Vec`/`HashMap`/`VecDeque`/etc. — see captrack's own README), so
//! this drives its public registry API directly
//! (`captrack::registry::record_creation`/`record_sample`) instead of the
//! `CapInspect` trait or the `t*!` macros, which assume one of the built-in
//! wrapped types. Each workload below picks a synthetic, fixed
//! `(name, line=0, column=0)` location — real source `file!()`/`line!()`
//! would collapse every workload in this file to the same call-site key,
//! since they'd all be recorded from the same physical call site inside the
//! shared helper; a distinct `name` string per workload keeps them separate
//! entries in the dump.
//!
//! Ignored by default — this is a manual measurement probe, not a
//! correctness test, and writes a file under Cargo's own per-test scratch
//! directory (`CARGO_TARGET_TMPDIR`, NOT a hand-rolled relative `target/`
//! path — this workspace overrides `CARGO_TARGET_DIR` to a shared location
//! outside the repo, so a literal `"target/..."` string would silently
//! create an untracked stray directory under `crates/region/` instead of
//! landing in the real, already-gitignored build-artifact tree). Run
//! explicitly:
//! ```text
//! cargo test -p sefer-region --test captrack_probe -- --ignored --nocapture
//! ```
//! then inspect the printed path to `captrack-region-probe.json`.

use sefer_region::Region;

/// One (name, file, line, column) location, using the workload name as both
/// the display label and the registry key's `file` field (line/column fixed
/// at 0 — see module doc for why).
fn loc(name: &'static str) -> (&'static str, &'static str, u32, u32) {
    (name, name, 0, 0)
}

#[test]
#[ignore = "manual capacity-telemetry probe, not a correctness test — see module doc"]
fn region_capacity_under_churn() {
    // Workload 1: steady growth, no removal. SlotMap::capacity() sampled
    // every 100 inserts plus a final sample — shows the growth curve shape
    // (geometric doubling vs. linear) rather than just the endpoint.
    {
        let (name, file, line, column) = loc("region/steady_insert_1000");
        captrack::registry::record_creation(name, file, line, column);
        let mut r: Region<u64> = Region::new();
        for i in 0..1000u64 {
            r.insert(i);
            if i % 100 == 0 {
                captrack::registry::record_sample(file, line, column, r.capacity());
            }
        }
        captrack::registry::record_sample(file, line, column, r.capacity());
    }

    // Workload 2: churn — insert 1000, remove every other one (freeing
    // slots onto slotmap's internal free list), then insert 500 more.
    // `Region::reserve`'s own doc comment claims re-inserts after a churn
    // reuse freed slots rather than growing unboundedly — this measures
    // that claim rather than trusting it: capacity after the refill should
    // NOT exceed capacity right after the removal pass.
    {
        let (name, file, line, column) = loc("region/churn_half_remove_refill");
        captrack::registry::record_creation(name, file, line, column);
        let mut r: Region<u64> = Region::new();
        let handles: Vec<_> = (0..1000u64).map(|i| r.insert(i)).collect();
        for (i, h) in handles.iter().enumerate() {
            if i % 2 == 0 {
                r.remove(*h);
            }
        }
        let cap_after_remove = r.capacity();
        captrack::registry::record_sample(file, line, column, cap_after_remove);
        for i in 0..500u64 {
            r.insert(i);
        }
        let cap_after_refill = r.capacity();
        captrack::registry::record_sample(file, line, column, cap_after_refill);
        assert!(
            cap_after_refill <= cap_after_remove,
            "refilling freed slots should not grow capacity past the post-removal high-water \
             mark: after_remove={cap_after_remove}, after_refill={cap_after_refill}"
        );
    }

    // Workload 3: with_capacity pre-sized vs. default new() for the
    // identical insert pattern — checks whether pre-sizing avoids any
    // intermediate reallocation (the final capacity should be no larger
    // than exactly what was requested, unlike workload 1's organic growth).
    {
        let (name, file, line, column) = loc("region/with_capacity_1000_presized");
        captrack::registry::record_creation(name, file, line, column);
        let mut r: Region<u64> = Region::with_capacity(1000);
        let cap_before_insert = r.capacity();
        for i in 0..1000u64 {
            r.insert(i);
        }
        captrack::registry::record_sample(file, line, column, r.capacity());
        assert!(
            cap_before_insert >= 1000,
            "with_capacity(1000) must reserve at least 1000 slots up front, got {cap_before_insert}"
        );
    }

    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("captrack-region-probe.json");
    captrack::dump_capacity_stats(&out).expect("dump capacity stats");
    println!("captrack report written to {}", out.display());
}
