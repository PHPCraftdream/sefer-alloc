//! R7-A5 miri target: strict-provenance validation of the directory path.
//!
//! ## Why a BELOW-threshold test
//!
//! Materialising the directory sidecar requires `table.count() >= 32`, meaning
//! 32+ segments (each 4 MiB OS reservation = 4 MiB `std::alloc` allocation
//! under miri). At miri's interpreted-execution speed, that is impractically
//! slow (tens of minutes for the alloc loop alone). The sidecar reservation
//! (`os::reserve_directory_sidecar`) and deref (`os::deref_directory_sidecar`)
//! use the SAME pattern as `reserve_aligned` + raw-pointer deref that the
//! existing miri matrix entries already validate (e.g. `decommit_miri_cycle`,
//! `region_invariants`).
//!
//! This test instead validates the ALLOC/DEALLOC path under the
//! `alloc-segment-directory` feature with the directory NOT materialised
//! (below-threshold). This is the code path that adds the `try_materialise`
//! check on every segment register + the A2 `publish_nonempty`/
//! `publish_empty` helpers (guarded by `directory_sidecar.is_null()` early
//! return). Under miri strict-provenance, this catches UB in:
//!
//! 1. The `directory_sidecar` null-pointer check itself (provenance of the
//!    null `*mut SegmentDirectory` stored in `AllocCore`).
//! 2. The `SegmentTable` slot/base lookup + segment-header reads exercised by
//!    `try_materialise_directory` (which runs on every register, even below
//!    threshold — it just returns early).
//! 3. The `BinTable` head-read path in the A2 publish helpers (called on every
//!    free, even when the directory is absent — the null check early-returns).
//!
//! ## Limitation (documented per A5 task)
//!
//! The full directory-materialised path (sidecar reserve → rebuild → lookup →
//! set/clear bits) is NOT exercised under miri in this test due to the
//! segment-count cost. It is structurally identical to the existing miri-
//! validated reserve+deref pattern (heap_overflow sidecar, registry chunk),
//! and the directory's bit operations are pure safe Rust (`u64` arithmetic).
//! The above-threshold path is covered by native tests (`segment_directory_
//! a1.rs`, `_a2.rs`, `_a3.rs`, `_a5.rs`, `_a5_proptest.rs`).

#![cfg(feature = "alloc-segment-directory")]

use std::alloc::Layout;

use sefer_alloc::AllocCore;

/// Below-threshold alloc/dealloc under `alloc-segment-directory`: the
/// directory mechanism code is compiled in (feature ON) but the sidecar is
/// NOT materialised (table.count < 32). Exercises the null-pointer guard
/// path in `try_materialise_directory` and the A2 publish helpers. Miri
/// strict-provenance validates the pointer arithmetic is clean.
#[test]
fn below_threshold_alloc_dealloc_provenance_clean() {
    let mut core = AllocCore::new().unwrap();

    let threshold = AllocCore::dbg_directory_materialize_threshold();
    assert!(
        core.dbg_table_count() < threshold,
        "fresh core must be below threshold"
    );
    assert!(
        !core.dbg_directory_is_materialised(),
        "directory must not be materialised"
    );

    let sizes: &[usize] = &[16, 64, 256, 1024];
    let layouts: Vec<Layout> = sizes
        .iter()
        .map(|&s| Layout::from_size_align(s, 1).unwrap())
        .collect();

    // Alloc/free cycle with multiple classes — exercises the A2 publish
    // helpers' null-pointer guard and the segment-table lookups.
    let mut live: Vec<(*mut u8, Layout)> = Vec::new();
    for round in 0..3 {
        // Allocate.
        for l in &layouts {
            for _ in 0..5 {
                let p = core.alloc(*l);
                assert!(!p.is_null(), "alloc returned null in round {round}");
                live.push((p, *l));
            }
        }
        // Free half.
        let n = live.len() / 2;
        for _ in 0..n {
            let (p, l) = live.pop().unwrap();
            unsafe { core.dealloc(p, l) };
        }
    }

    // Directory must still be absent.
    assert!(
        !core.dbg_directory_is_materialised(),
        "directory should not materialise for small workloads"
    );

    // Clean up.
    for (p, l) in live {
        unsafe { core.dealloc(p, l) };
    }
}
