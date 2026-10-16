//! R6-T39 / PF-7 guard: `FREE_PARK_CAP[c]` (the table the free side and, after
//! PF-7, the refill side read) must stay equal to the documented formula
//! `refill_n = clamp(REFILL_BYTE_BUDGET / block_size(c), 1, TCACHE_CAP)`
//! (`src/registry/heap_core/state/tcache.rs:63` `TCACHE_CAP = 16`,
//! `:116` `REFILL_BYTE_BUDGET = 64 * 1024`).
//!
//! The PRIMARY oracle below pins CONCRETE expected values for representative
//! classes, derived BY HAND from the table literal in
//! `src/alloc_core/platform/size_classes.rs` (`SIZE_CLASS_TABLE`, built by
//! `size_classes::build_table` from the 40-class geometric run + `EXTRAS`
//! at :118 — 49 entries; `production` does NOT enable `medium-classes`, so
//! the indices below are the ones CI's production-feature test jobs see).
//! This catches DRIFT OF THE TABLE ITSELF (a changed `MIN_BLOCK`, growth
//! ratio, or `EXTRAS` list silently moving block sizes). It does NOT pin the
//! formula: if `REFILL_BYTE_BUDGET`, `TCACHE_CAP`, or the clamp ever change,
//! the formula changes and these literals must be re-derived and re-audited
//! by hand — a formula change will fail this test, and that failure is the
//! audit trigger, not proof of a bug.
//!
//! The `dbg_refill_n_for_class` equality loop is kept as a SECONDARY
//! assertion (table == formula for every class), but it is intentionally not
//! the primary oracle: it is tautological against a formula-driven table
//! (both sides share the same code path).
//!
//! Pinned entries (index → block_size → cap = clamp(65536 / bs, 1, 16)):
//! - class 0  (`SIZE_CLASS_TABLE[0]`  = 16):      65536/16    = 4096 → 16
//!   (byte budget exceeds any per-refill need; clamped at `TCACHE_CAP`)
//! - class 25 (`SIZE_CLASS_TABLE[25]` = 4096):    65536/4096  = 16 → 16
//!   (exactly at the upper clamp boundary)
//! - class 26 (`SIZE_CLASS_TABLE[26]` = 4640):    65536/4640  = 14.12… → 14
//!   (fractional branch: byte budget divides the refill)
//! - class 30 (`SIZE_CLASS_TABLE[30]` = 8192):    65536/8192  = 8
//! - class 48 (`SIZE_CLASS_TABLE[48]` = 258752):  65536/258752 = 0 → 1
//!   (largest small class; clamped at the floor)
//!
//! Feature gates: `alloc-global + fastbin` (tcache table + accessors) and
//! `internals` (`dbg_claim_lease`).
//!
//! Derivation note for reviewers re-pinning by hand: the geometric run is
//! `next = round_up(cur * 5 / 4, 16)` from `cur = 16` (every term is a
//! multiple of 4, so `ceil` is exact), merged with the sorted `EXTRAS`
//! (`256 … 16384`, `size_classes.rs:118`). The merged table's first 31
//! entries are:
//! 16, 32, 48, 64, 80, 112, 144, 192, 240, 256, 304, 384, 480, 512, 608,
//! 768, 960, 1024, 1200, 1504, 1888, 2048, 2368, 2960, 3712, 4096, 4640,
//! 5808, 6144, 7264, 8192 — giving classes 0, 25, 26, 30 above; the 49th
//! and last entry (class 48) is 258752. Cross-checked against the
//! `size-classes` crate's own pinned fixtures
//! (`crates/size-classes/tests/common/mod.rs`: classes 6/14/18/22/36 =
//! 144/608/1200/2368/17760, `SEFER_MAX` = 258752, `SEFER_L` = 16173).

#![cfg(all(feature = "alloc-global", feature = "fastbin", feature = "internals"))]

use sefer_alloc::registry::{bootstrap, HeapRegistry};

#[test]
fn free_park_cap_pins_concrete_table_entries() {
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim heap");
    let heap = lease.core();

    // (class index, expected cap) — see the derivation in the header comment.
    let pinned: [(usize, usize); 5] = [(0, 16), (25, 16), (26, 14), (30, 8), (48, 1)];
    for &(c, expected) in &pinned {
        assert_eq!(
            heap.dbg_free_park_cap(c),
            Some(expected),
            "FREE_PARK_CAP[{c}] drifted from the hand-derived table literal"
        );
    }

    // SECONDARY oracle: the table still equals the live formula for every
    // class (this alone was the old, tautological assertion).
    let mut c = 0usize;
    while let Some(cap) = heap.dbg_free_park_cap(c) {
        assert_eq!(
            cap,
            heap.dbg_refill_n_for_class(c),
            "FREE_PARK_CAP[{c}] drifted from refill_n_for_class(block_size({c}))"
        );
        assert!(
            (1..=16).contains(&cap),
            "cap out of 1..=TCACHE_CAP for class {c}"
        );
        c += 1;
    }
    // The 49-entry default table. `medium-classes` builds (e.g. the CI
    // `hardened medium-classes internals` job) APPEND six exact classes
    // AFTER index 48 (all > 258752), so the pinned indices and values above
    // are identical there too — only the total count differs.
    assert_eq!(
        c,
        sefer_alloc::SegmentLayout::SIZE_CLASS_TABLE.len(),
        "unexpected small-class count for this build"
    );
}
