//! Regression test (task #656): `bench-scale-tool`'s `-- <substring>` filter
//! selects every workload whose `<bin>::<id>` or `<id>` CONTAINS the given
//! substring. The first version of `benches/region_bench.rs` named its two
//! workload families `region/*` and `sync_region/*` — since `"sync_region/insert"`
//! literally contains `"region/insert"` as a substring, filtering for
//! `region/insert` alone always also matched `sync_region/insert`, making it
//! impossible to run/time the single-threaded workload in isolation.
//!
//! This is a doc-only guard: it reads `benches/region_bench.rs` as source
//! text (never links the crate, mirroring `tests/dbg_hook_safety_tripwire.rs`'s
//! `scan_file` technique) and extracts every workload id string — the first
//! argument to `.bench(` / `.bench_batched(` — then asserts no id is a
//! substring of any OTHER id. This runs in every feature configuration and
//! needs no `bench-scale-tool`/`captrack` dev-dependency to be built.
//!
//! Non-vacuity: reverting the id rename (`st/` -> `region/`, `sync/` ->
//! `sync_region/`) reproduces the original collision and fails this test —
//! verified manually before landing the fix.

use std::fs;
use std::path::Path;

/// Extract every workload id string literal from `benches/region_bench.rs`:
/// the first `"..."` argument to a `.bench(` or `.bench_batched(` call.
fn extract_workload_ids(source: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = source;
    for marker in [".bench(", ".bench_batched("] {
        let mut cursor = rest;
        while let Some(pos) = cursor.find(marker) {
            let after_call = &cursor[pos + marker.len()..];
            let quote_start = after_call
                .find('"')
                .expect("bench call must be followed by a string literal id");
            let after_quote = &after_call[quote_start + 1..];
            let quote_end = after_quote
                .find('"')
                .expect("workload id string literal must be closed");
            ids.push(after_quote[..quote_end].to_string());
            cursor = &after_call[quote_start + 1 + quote_end + 1..];
        }
    }
    let _ = &mut rest; // silence unused-mut-style lint noise; `rest` is only a loop seed above
    ids
}

#[test]
fn workload_ids_never_substring_collide() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let bench_path = manifest.join("benches").join("region_bench.rs");
    let source = fs::read_to_string(&bench_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", bench_path.display()));

    let ids = extract_workload_ids(&source);
    assert!(
        ids.len() >= 8,
        "expected at least 8 workload ids in benches/region_bench.rs, found {}: {ids:?}",
        ids.len()
    );

    let mut collisions = Vec::new();
    for (i, a) in ids.iter().enumerate() {
        for (j, b) in ids.iter().enumerate() {
            if i != j && b.contains(a.as_str()) {
                collisions.push(format!(
                    "\"{a}\" is a substring of \"{b}\" — `-- {a}` would also match `{b}`, \
                     so `{a}` can never be run in isolation via bench-scale-tool's filter"
                ));
            }
        }
    }
    assert!(
        collisions.is_empty(),
        "workload id substring collision(s) found in benches/region_bench.rs:\n{}",
        collisions.join("\n")
    );
}
