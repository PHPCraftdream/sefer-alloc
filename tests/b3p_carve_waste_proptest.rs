//! Ph3c step 1′ (B3 per-class carve cursors) — carve-waste proptest oracle.
//!
//! Randomly interleaves ALL small classes (the full `SIZE_CLASS_TABLE`,
//! including > 4 KiB span classes) through `AllocCore` with no frees, and
//! asserts the run-tail loss oracle per class: for every class the bytes
//! TAKEN from the shared bump frontier by fresh `RUN_LEAVES[class]`-sub-leaf
//! runs minus the bytes actually issued in blocks is at most ONE partial run
//! (`RUN_LEAVES[class] * 4096`). The single-segment guard (segment-table
//! count must not grow) keeps the oracle exact: with one segment each class
//! can leave at most one exhausted tail run unaccounted.
//!
//! Oracle hooks: `b3p_run_bytes` (incremented at every fresh-run take in
//! `carve_block`/`try_carve_batch`), `b3p_run_leaves`; see
//! `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs`.
#![cfg(all(feature = "production", feature = "internals", feature = "bench-internals"))]

use proptest::prelude::*;
use sefer_alloc::{AllocCore, SegmentLayout};

// Called only from inside `proptest!`'s generated harness.
#[allow(dead_code)]
fn runner(n_allocs: usize) {
    let classes = SegmentLayout::SIZE_CLASS_TABLE;
    let n_classes = classes.len();
    // Counters are process-wide statics: snapshot per case, compare deltas.
    let run_bytes_before: Vec<u64> =
        (0..n_classes).map(sefer_alloc::b3p_run_bytes).collect();
    let mut core = AllocCore::new().expect("AllocCore::new");
    let table_base = core.dbg_table_count();
    let mut issued: Vec<u64> = vec![0; n_classes];
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(n_allocs);

    let mut rng_state = n_allocs as u64 ^ 0x9E37_79B9_7F4A_7C15;
    let mut next_rand = move || {
        // xorshift64: deterministic per-case interleaving.
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        rng_state
    };

    let budget: u64 = 2 * 1024 * 1024; // stay comfortably inside one 4 MiB segment
    let mut total: u64 = 0;
    for _ in 0..n_allocs {
        let class = (next_rand() as usize) % n_classes;
        let bs = classes[class];
        if total + bs as u64 > budget {
            break;
        }
        let layout = core::alloc::Layout::from_size_align(bs, 8).unwrap();
        // SAFETY: non-zero size, valid alignment; freed when `core` drops.
        let ptr = core.alloc(layout);
        assert!(!ptr.is_null(), "alloc null (class {class}, bs {bs})");
        ptrs.push(ptr);
        issued[class] += bs as u64;
        total += bs as u64;
    }

    // Single-segment guard: no small-segment reserve may have happened,
    // otherwise the per-class oracle bound would need a segment factor.
    assert_eq!(
        core.dbg_table_count(),
        table_base,
        "case with {n_allocs} allocs reserved a second segment"
    );

    // Oracle: per class, run bytes taken − issued bytes ≤ one partial run.
    for (class, &issued_bytes) in issued.iter().enumerate() {
        let taken = sefer_alloc::b3p_run_bytes(class) - run_bytes_before[class];
        let bound = sefer_alloc::b3p_run_leaves(class) as u64 * 4096;
        assert!(
            taken.saturating_sub(issued_bytes) <= bound,
            "class {class} (bs {}): run_bytes {taken} − issued {issued_bytes} > one run ({bound})",
            classes[class]
        );
    }
    assert!(!ptrs.is_empty());
}

#[test]
fn carve_waste_oracle_single_segment() {
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]
        fn prop_carve_waste(n_allocs in 64usize..1024) {
            runner(n_allocs);
        }
    }
}
