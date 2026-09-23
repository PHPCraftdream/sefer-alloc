//! R2-14 runtime oracle: the DUAL-ENUMERATION fix in
//! `AllocCore::dbg_segment_state_reconciliation`
//! (`src/alloc_core/small/alloc_core_small_pool/mod.rs`, the
//! `bench-internals` + `internals` method inside the `alloc-decommit`-gated
//! module).
//!
//! # Why this file exists (the three false claims the fix corrects)
//!
//! The pre-R2-14 instrument carried three defects, each of which made its
//! published accounting wrong in a way no existing test could catch:
//!
//! 1. **`large_cached` was structurally dead.** Cached Large segments were
//!    classified from TABLE slots via `hdr.magic == 0`. But a large-cache
//!    deposit UNREGISTERS its segment first (`alloc_core/mem/mod.rs`'s
//!    own-thread dealloc branch; `alloc_core/large/alloc_core_large.rs`'s
//!    remote-reclaim branch both call `table.unregister(base)` BEFORE zeroing
//!    the magic), so a cached entry is NEVER in the table and
//!    `large_cached.count` could only ever read 0. R29-4's own gate report
//!    (`docs/perf/R29_4_SEGMENT_STATE_RECONCILIATION_GATE.md` §1, item 7)
//!    recorded the opposite — "a large-cache-held segment DOES appear in
//!    `segment_bases()`" — as fact; the fix proved it FALSE and added the
//!    second enumeration over the combined base+extension cache index space
//!    (`large_cache_scan_bound()` / `large_cache_slot_get(idx)`).
//! 2. **The claimed identity was arithmetically false.** The instrument
//!    documented `sum(per_state.count) + unknown_count == table.count()`.
//!    The walk SKIPS NULL (recycled) slots while `SegmentTable::count()` is a
//!    HIGH-WATER mark (slots ever written, including recycled holes), so any
//!    heap that had recycled even one slot violated the claim. The fix
//!    publishes the two quantities that make the identity close —
//!    `table_high_water` and `table_recycled_null_slots` — and restates it as
//!    `total.count + unknown_count + table_recycled_null_slots
//!    == table_high_water + large_cached.count`.
//! 3. **Small-segment committed bytes ignored the lazy-commit frontier.**
//!    Primordial / small_active / small_pooled / small_empty_orphan always
//!    reported the full `SEGMENT`. Under either lazy-commit policy the real
//!    commit charge is the `committed_payload_end` frontier (set at
//!    bootstrap / reservation, advanced by grow-on-carve), so the instrument
//!    over-reported by up to ~3.75 MiB per lazily-committed segment.
//!
//! # Oracle design (CLAUDE.md's R26-4 config-evidence + R30-8
//! # path-activation disciplines)
//!
//! Every arm keeps an INDEPENDENT LEDGER — what the test itself allocated,
//! plus the pre-existing `dbg_*_for` / header-field SEAM accessors
//! (`dbg_span_usable_of`, `dbg_reserved_capacity_of`,
//! `dbg_committed_payload_end_for`, `dbg_table_count`, `dbg_contains_base`,
//! `dbg_live_count_for`, `dbg_is_decommitted_for`) — and never derives an
//! expectation from the reconciliation getter under test. Each arm asserts
//! `unknown_count == 0` and the corrected identity
//! `total.count + unknown_count + table_recycled_null_slots
//! == table_high_water + large_cached.count`.
//!
//! The `small_decommitted_retained` formula is deliberately NOT re-pinned
//! here — `tests/segment_state_reconciliation_oracle.rs` already owns it in
//! both policy worlds (task #1081 / #1087), and R2-14 left that branch's
//! formula byte-for-byte unchanged.
//!
//! # Arms, and what each one falsifies about the pre-fix instrument
//!
//! | arm | state driven | pre-fix failure |
//! |---|---|---|
//! | `recycled_hole_...` | 1 NULL (recycled) table slot under a high-water of 4 | the two new fields do not exist (the file cannot compile against the old instrument) AND the old identity claim `total.count + unknown_count == table.count()` reads 3 + 0 == 4 — FALSE in exactly this state |
//! | `cached_large_...` | one deposited Large span | `large_cached.count` reads 0 (defect 1) — fails on EVERY host and cfg; the deposit is invisible to the table walk |
//! | `extension_slot_...` | 8 base slots full + a 9th deposit in the materialised extension | same defect 1, now across the EXTENSION half of the combined index space: pre-fix the second enumeration did not exist at all |
//! | `primordial_..._lazy_frontier` | a fresh heap's primordial under `primordial-lazy-commit` on genuine-Windows-lazy | `primordial.committed_bytes` reads the full `SEGMENT` instead of the frontier (defect 3) |
//! | `primordial_..._eager` | a fresh heap with `primordial-lazy-commit` OFF | pins the eager contract value `SEGMENT` (the fix must not overcorrect) |
//! | `small_active_..._partial_frontier` | a second small segment carved partway under `small-segment-lazy-commit` on genuine-Windows-lazy | `small_active.committed_bytes` reads the full `SEGMENT` instead of the advanced frontier (defect 3) |
//!
//! # cfg gating (read this before editing an arm)
//!
//! `production` CONTAINS `primordial-lazy-commit` but NOT
//! `small-segment-lazy-commit`, `large-cache-extended` or `numa-aware`, so
//! the four verification invocations land in four DIFFERENT policy worlds.
//! The lazy arms are therefore split by what the reservation ACTUALLY does,
//! not by what the feature is named:
//!
//! - `primordial-lazy-commit` (or `small-segment-lazy-commit`) is a genuine
//!   PARTIAL commit only on `all(windows, not(miri),
//!   not(feature = "numa-aware"))` — `bootstrap::primordial` /
//!   `reserve_small_segment` stamp the frontier there, and stamp `SEGMENT`
//!   everywhere else (`numa-aware`'s P2 gate keeps reservations eager; Unix
//!   and miri `mmap`/`alloc` the whole span up front, R8-5).
//! - `--all-features` turns `numa-aware` ON, so a naive
//!   `#[cfg(feature = "primordial-lazy-commit")]`-only lazy arm would
//!   compile there and FAIL its own `< SEGMENT` assertion. Each lazy feature
//!   therefore gets a strict arm (genuine Windows lazy) AND an
//!   eager-reality arm (feature on, platform/feature forces eager) — the
//!   union of the two cfgs is exactly the feature's own cfg, so the strict
//!   assertion only ever runs where it is true.

#![cfg(all(
    feature = "internals",
    feature = "alloc-decommit",
    feature = "bench-internals"
))]

use std::alloc::Layout;

use sefer_alloc::alloc_core::SegmentStateReconciliation;
use sefer_alloc::{AllocCore, SegmentLayout};

// ---------------------------------------------------------------------------
// Shared machinery — compiled under every configuration that satisfies the
// file gate (each helper below has at least one live caller in each of the
// four verification invocations, so none is ever dead code).
// ---------------------------------------------------------------------------

/// The corrected R2-14 identity, asserted verbatim in EVERY arm:
/// every live table slot (`table_high_water` minus
/// `table_recycled_null_slots`) is classified exactly once (or counted as
/// `unknown_count`), and `large_cached` adds the separately-enumerated cache
/// entries. The pre-R2-14 claim `sum(per_state.count) + unknown_count ==
/// table.count()` was FALSE — see this file's module doc, defect 2.
fn assert_dual_enumeration_identity(rec: &SegmentStateReconciliation, arm: &str) {
    assert_eq!(
        rec.total.count + rec.unknown_count + rec.table_recycled_null_slots,
        rec.table_high_water + rec.large_cached.count,
        "{arm}: corrected identity — total.count + unknown_count + \
         table_recycled_null_slots must equal table_high_water + \
         large_cached.count"
    );
    // `total` is the sum of the seven per-state accounts, INCLUDING
    // `large_cached` (R2-14 moved it from "never populated" into the fold).
    let sum_of_states = rec.primordial.count
        + rec.small_pooled.count
        + rec.small_active.count
        + rec.small_empty_orphan.count
        + rec.small_decommitted_retained.count
        + rec.large_active.count
        + rec.large_cached.count;
    assert_eq!(
        rec.total.count, sum_of_states,
        "{arm}: total.count must equal the sum of the seven per-state counts \
         (large_cached included)"
    );
    let committed_sum = rec.primordial.committed_bytes
        + rec.small_pooled.committed_bytes
        + rec.small_active.committed_bytes
        + rec.small_empty_orphan.committed_bytes
        + rec.small_decommitted_retained.committed_bytes
        + rec.large_active.committed_bytes
        + rec.large_cached.committed_bytes;
    assert_eq!(
        rec.total.committed_bytes, committed_sum,
        "{arm}: total.committed_bytes must include every per-state commit charge"
    );
    let reserved_sum = rec.primordial.reserved_bytes
        + rec.small_pooled.reserved_bytes
        + rec.small_active.reserved_bytes
        + rec.small_empty_orphan.reserved_bytes
        + rec.small_decommitted_retained.reserved_bytes
        + rec.large_active.reserved_bytes
        + rec.large_cached.reserved_bytes;
    assert_eq!(
        rec.total.reserved_bytes, reserved_sum,
        "{arm}: total.reserved_bytes must include every per-state reservation"
    );
}

/// The config- and platform-honest committed-payload bytes of a Small or
/// Primordial segment: the `committed_payload_end` FRONTIER when either
/// lazy-commit policy feature is compiled in (the independent ledger value),
/// else the whole `SEGMENT` (eager backends commit the entire reservation at
/// reserve time, and the frontier accessor does not exist there).
#[cfg_attr(
    not(any(
        feature = "primordial-lazy-commit",
        feature = "small-segment-lazy-commit"
    )),
    allow(unused_variables)
)]
fn small_frontier_bytes(ac: &AllocCore, ptr: *mut u8) -> u64 {
    #[cfg(any(
        feature = "primordial-lazy-commit",
        feature = "small-segment-lazy-commit"
    ))]
    let frontier = ac
        .dbg_committed_payload_end_for(ptr)
        .expect("ptr must be an owned Small/Primordial segment") as u64;
    #[cfg(not(any(
        feature = "primordial-lazy-commit",
        feature = "small-segment-lazy-commit"
    )))]
    let frontier = SegmentLayout::SEGMENT as u64;
    frontier
}

/// A layout that classifies as Large under EVERY Small/Large boundary this
/// crate ships: 8 MiB is past `SMALL_MAX` both without the medium classes
/// (~253 KiB) and under `medium-classes-wide` (1.75 MiB), so the arm does not
/// silently degrade into a Small allocation under `--all-features`.
fn large_layout() -> Layout {
    Layout::from_size_align(8 * 1024 * 1024, 8).expect("8 MiB / 8 is a valid layout")
}

// ---------------------------------------------------------------------------
// Arm 1 — a recycled (NULL) table slot under an elevated high-water mark.
// ---------------------------------------------------------------------------

/// Recycle ONE table slot while the high-water mark stays elevated, then
/// check the reconciliation sees the hole honestly.
///
/// State driven: primordial (slot 0) + two fresh registered empty Small
/// segments (via `dbg_decomp_reserve_and_keep`, which registers + initialises
/// but never publishes `small_cur` — the R30-1-correct measurement route) +
/// one Large registration freed under a ZERO cache budget, so the deposit is
/// declined and `dealloc`'s Large branch unregisters + releases the
/// reservation: the slot NULLs, the high-water does not move.
///
/// Counterfactual: pre-fix, `table_high_water` / `table_recycled_null_slots`
/// did not exist, so this file cannot even compile against the old
/// instrument — and the old claimed identity `total.count + unknown_count ==
/// table.count()` reads 3 + 0 == 4 in exactly this state, i.e. it was FALSE
/// whenever a heap had ever recycled a slot.
#[test]
fn recycled_hole_leaves_high_water_and_a_counted_null_slot() {
    let mut ac = AllocCore::new().expect("AllocCore::new must survive bootstrap");

    // Independent ledger anchor: one live block in the primordial segment,
    // so the frontier seam has a reachable pointer.
    let prim = ac.alloc(Layout::from_size_align(16, 8).expect("16/8 layout"));
    assert!(!prim.is_null(), "the primordial must serve a 16-byte block");

    // ── Reserve TWO fresh, registered, EMPTY Small segments. ───────────────
    let h1 = ac
        .dbg_decomp_reserve_and_keep()
        .expect("first small-segment reservation must succeed");
    let h2 = ac
        .dbg_decomp_reserve_and_keep()
        .expect("second small-segment reservation must succeed");
    let b1 = h1.dbg_base();
    let b2 = h2.dbg_base();
    assert_ne!(b1, b2, "two reservations must yield two distinct segments");

    // Precondition evidence: both segments are genuinely EMPTY and not yet
    // decommitted — the exact pre-state `small_empty_orphan` requires.
    assert_eq!(
        ac.dbg_live_count_for(b1),
        Some(0),
        "precondition: a freshly reserved segment must read live_count == 0"
    );
    assert_eq!(
        ac.dbg_live_count_for(b2),
        Some(0),
        "precondition: a freshly reserved segment must read live_count == 0"
    );
    assert_eq!(
        ac.dbg_is_decommitted_for(b1),
        Some(false),
        "precondition: a freshly reserved segment must not be decommitted"
    );
    assert_eq!(
        ac.dbg_is_decommitted_for(b2),
        Some(false),
        "precondition: a freshly reserved segment must not be decommitted"
    );

    // ── One Large registration, freed under a ZERO cache budget: the deposit
    //    is declined (`large_cache_deposit_budget_infeasible`), so dealloc's
    //    Large branch unregisters + releases — the table slot NULLs while the
    //    high-water mark stays. This is the deterministic recycle path (the
    //    empty-small-segment path would POOL the segment under the default
    //    cap of 4, keeping its slot live instead).
    let large = large_layout();
    let lp = ac.alloc(large);
    assert!(!lp.is_null(), "an 8 MiB Large allocation must succeed");
    ac.dbg_set_large_cache_budget(Some(0));
    // SAFETY: `lp` was returned by the matching `ac.alloc(large)` above, is
    // live, and is freed exactly once here.
    unsafe { ac.dealloc(lp, large) };
    assert!(
        !ac.dbg_contains_base(lp),
        "activation: the freed Large must be unregistered (slot NULLed)"
    );

    // ── Snapshot, then RELEASE both reserved handles BEFORE any load-bearing
    //    assertion (the handle's leak-detecting `Drop` would fire a SECOND
    //    panic during a failed assertion's unwind — the hazard
    //    tests/segment_state_reconciliation_oracle.rs documents).
    let rec = ac.dbg_segment_state_reconciliation();
    let expected_orphan_committed = small_frontier_bytes(&ac, b1) + small_frontier_bytes(&ac, b2);
    // SAFETY: both handles were minted by THIS `ac`'s
    // `dbg_decomp_reserve_and_keep` above and have not been released in the
    // interim — the exact pairing `# Safety` requires.
    unsafe { ac.dbg_decomp_release(h1) };
    unsafe { ac.dbg_decomp_release(h2) };

    assert_eq!(rec.unknown_count, 0, "no corrupt segment headers");
    assert_eq!(
        rec.table_high_water,
        ac.dbg_table_count() as usize,
        "the snapshot's high-water must equal the table's own high-water seam"
    );
    assert_eq!(
        rec.table_high_water, 4,
        "primordial + 2 reserved small + 1 large slot = 4 slots ever written"
    );
    assert_eq!(
        rec.table_recycled_null_slots, 1,
        "the freed Large's slot must be counted as a NULL recycled hole"
    );
    assert_eq!(rec.primordial.count, 1, "the heap's own primordial segment");
    assert_eq!(
        rec.small_empty_orphan.count, 2,
        "both reserved segments are registered, empty, not pooled, not \
         small_cur, not decommitted — the orphan state"
    );
    assert_eq!(
        rec.large_active.count, 0,
        "the large segment was unregistered before any snapshot"
    );
    assert_eq!(
        rec.large_cached.count, 0,
        "a declined deposit never reaches the cache (budget was 0)"
    );
    assert_eq!(
        rec.small_empty_orphan.committed_bytes, expected_orphan_committed,
        "R2-14 defect 3: orphan committed bytes must be the frontier — \
         pre-fix this read the full SEGMENT per segment"
    );
    assert_eq!(
        rec.primordial.committed_bytes,
        small_frontier_bytes(&ac, prim),
        "R2-14 defect 3: the primordial's committed bytes must be its \
         frontier — pre-fix this read the full SEGMENT"
    );
    assert_dual_enumeration_identity(&rec, "recycled_hole");
}

// ---------------------------------------------------------------------------
// Arm 2 — one Large span deposited into the cache.
// ---------------------------------------------------------------------------

/// Deposit ONE Large segment into the cache (alloc + free under an unbounded
/// budget) and check the second enumeration finds it with the right bytes.
///
/// Independent ledger: the span's `span_usable` / `reserved_capacity` are
/// read from the LIVE header seams (`dbg_span_usable_of` /
/// `dbg_reserved_capacity_of`) BEFORE the free — a deposit unregisters the
/// segment first, so after the free there is no header left to read.
///
/// Counterfactual: pre-fix `large_cached.count` always read 0 (defect 1), so
/// `count == 1` fails on EVERY host and EVERY cfg against the unfixed
/// instrument. The committed-bytes equality additionally pins that deposits
/// keep their pages COMMITTED (no decommit on deposit), and the reserved
/// equality pins that the reported reservation is the real OS reservation
/// descriptor (the over-reserve+trim span), not the R12-4 `reserved_capacity`
/// VA target.
#[test]
fn cached_large_is_enumerated_with_usable_and_reservation_bytes() {
    let mut ac = AllocCore::new().expect("AllocCore::new must survive bootstrap");
    // Isolate the slot/budget behaviour from the config defaults: under
    // `large-cache-extended` the DEFAULT budget is a finite 256 MiB, under
    // the plain path it is unbounded — `None` makes both identical.
    ac.dbg_set_large_cache_budget(None);

    let large = large_layout();
    let lp = ac.alloc(large);
    assert!(!lp.is_null(), "an 8 MiB Large allocation must succeed");

    // Independent ledger, from the header seams while the segment is still
    // registered (post-free the deposit unregisters it first).
    let usable = ac.dbg_span_usable_of(lp);
    let reserved_capacity = ac.dbg_reserved_capacity_of(lp);
    assert!(
        usable >= large.size(),
        "the physical span must cover the request ({usable} < {})",
        large.size()
    );

    // SAFETY: `lp` was returned by the matching `ac.alloc(large)` above, is
    // live, and is freed exactly once here.
    unsafe { ac.dealloc(lp, large) };

    let rec = ac.dbg_segment_state_reconciliation();

    assert_eq!(rec.unknown_count, 0, "no corrupt segment headers");
    assert_eq!(
        rec.large_cached.count, 1,
        "R2-14 defect 1: the deposit must be enumerated by the second pass — \
         pre-fix this read 0 on every host (cached entries are never in the \
         table)"
    );
    assert_eq!(
        rec.large_active.count, 0,
        "a deposit unregisters its segment, so the table walk must not also \
         count it as active"
    );
    assert_eq!(
        rec.large_cached.committed_bytes, usable as u64,
        "deposits keep their pages COMMITTED, so the commit charge is the \
         carried-forward usable span ({usable})"
    );
    assert!(
        rec.large_cached.reserved_bytes >= usable as u64
            && rec.large_cached.reserved_bytes >= reserved_capacity as u64,
        "the OS reservation must at least cover the committed usable span and \
         the reserved-capacity target"
    );
    // Where the reservation is the ordinary over-reserve+trim span (the
    // deterministic head + committed + tail of a 64-bit, non-miri, non-NUMA
    // backend), pin it exactly: the descriptor is one SEGMENT LONGER than the
    // reserved-capacity target (which is `usable` without
    // `large-reserved-capacity`, and the geometric capacity target with it) —
    // deliberately NOT the R12-4 VA-capacity figure the fix's doc calls out.
    #[cfg(all(not(miri), target_pointer_width = "64", not(feature = "numa-aware")))]
    assert_eq!(
        rec.large_cached.reserved_bytes,
        (reserved_capacity + SegmentLayout::SEGMENT) as u64,
        "the OS reservation descriptor is the over-reserve+trim span \
         (reserved capacity + one SEGMENT of head/tail), NOT the \
         reserved_capacity VA target"
    );
    assert_eq!(
        rec.table_recycled_null_slots, 1,
        "the deposit's unregistered slot must be counted as a NULL hole"
    );
    assert_eq!(
        rec.table_high_water, 2,
        "primordial + the (now unregistered) large slot"
    );
    assert_eq!(rec.primordial.count, 1, "the heap's own primordial segment");
    assert_dual_enumeration_identity(&rec, "cached_large");
}

// ---------------------------------------------------------------------------
// Arm 3 — the extension half of the combined cache index space.
// ---------------------------------------------------------------------------

/// Nine same-sized Large allocations fill the base 8 cache slots and one
/// extension slot. Compute the size AT RUNTIME from this build's actual
/// Small/Large boundary: under `medium-classes-wide` the maximum Small class
/// is 1.75 MiB, so a fixed 4 MiB request could silently become Small.
/// All allocations are live before any are freed, so no cache hit can consume
/// a slot during the deposit loop; distinct sizes are unnecessary.
#[cfg(feature = "large-cache-extended")]
fn large_test_sizes() -> Vec<usize> {
    let segment = SegmentLayout::SEGMENT;
    let small_max_class = AllocCore::dbg_small_class_count() - 1;
    let small_max = AllocCore::dbg_block_size(small_max_class);
    let n = (2 * small_max).div_ceil(segment).max(1) * segment;
    vec![n; 9]
}

/// Fill the 8 base cache slots, force a 9th deposit so the extension
/// sidecar materialises, and check the second enumeration covers the
/// COMBINED index space (base slots 0..8 plus extension slots 8..40).
///
/// Counterfactual: pre-fix the second enumeration did not exist at all, so
/// `large_cached.count` read 0 — this fails on EVERY host and cfg. The
/// extension-specific half of the claim (the scan reaching past
/// `LARGE_CACHE_SLOTS` into the lazily-materialised sidecar) is only
/// exercised under `large-cache-extended`, which is exactly this arm's gate.
#[cfg(feature = "large-cache-extended")]
#[test]
fn extension_slots_are_enumerated_into_large_cached() {
    let mut ac = AllocCore::new().expect("AllocCore::new must survive bootstrap");
    ac.dbg_set_large_cache_budget(None); // isolate slot count from budget

    assert!(
        !ac.dbg_large_cache_extension_materialised(),
        "extension must start unmaterialised"
    );
    assert_eq!(
        ac.dbg_large_cache_total_slots(),
        8,
        "the combined scan bound must be 8 before any overflow"
    );

    let sizes = large_test_sizes();
    // Independent ledger: per-span `span_usable` read from the LIVE header
    // seam (a deposit unregisters the segment first).
    let mut usable_sum = 0u64;
    let mut reserved_capacity_sum = 0u64;
    let mut live: Vec<(*mut u8, Layout)> = Vec::with_capacity(sizes.len());
    for &bytes in &sizes {
        let l = Layout::from_size_align(bytes, 8).expect("size is a valid layout");
        let p = ac.alloc(l);
        assert!(
            !p.is_null(),
            "alloc of {bytes} bytes failed — a genuinely memory-starved host, \
             not the expected case"
        );
        usable_sum += ac.dbg_span_usable_of(p) as u64;
        reserved_capacity_sum += ac.dbg_reserved_capacity_of(p) as u64;
        live.push((p, l));
    }

    for (p, l) in live {
        // SAFETY: `p` was returned by the matching `ac.alloc(l)` above, is
        // live, and is freed exactly once here.
        unsafe { ac.dealloc(p, l) };
    }

    assert!(
        ac.dbg_large_cache_extension_materialised(),
        "the 9th distinct deposit must materialise the extension sidecar"
    );
    assert_eq!(
        ac.dbg_large_cache_total_slots(),
        40,
        "the combined scan bound must grow to 8 + 32 once materialised"
    );

    let rec = ac.dbg_segment_state_reconciliation();

    assert_eq!(rec.unknown_count, 0, "no corrupt segment headers");
    assert_eq!(
        rec.large_cached.count,
        sizes.len(),
        "the second enumeration must cover the COMBINED index space: all 9 \
         deposits (8 base + 1 extension) enumerated"
    );
    assert!(
        rec.large_cached.count >= 9,
        "at least 9 cached spans must be visible (the extension one is only \
         reachable past the base 8)"
    );
    assert_eq!(
        rec.large_cached.committed_bytes, usable_sum,
        "every deposit keeps its pages committed — the sum of the recorded \
         usable spans"
    );
    #[cfg(all(not(miri), target_pointer_width = "64", not(feature = "numa-aware")))]
    assert_eq!(
        rec.large_cached.reserved_bytes,
        reserved_capacity_sum + (sizes.len() as u64) * (SegmentLayout::SEGMENT as u64),
        "each reservation is the over-reserve+trim span (capacity + SEGMENT)"
    );
    #[cfg(not(all(not(miri), target_pointer_width = "64", not(feature = "numa-aware"))))]
    assert!(
        rec.large_cached.reserved_bytes >= usable_sum
            && rec.large_cached.reserved_bytes >= reserved_capacity_sum,
        "each reservation must at least cover its committed usable span and \
         its reserved-capacity target"
    );
    assert_eq!(
        rec.large_active.count, 0,
        "all nine large segments were freed (unregistered) before the snapshot"
    );
    assert_eq!(
        rec.table_recycled_null_slots,
        sizes.len(),
        "every one of the 9 large registrations was recycled to a NULL slot"
    );
    assert_eq!(
        rec.table_high_water,
        1 + sizes.len(),
        "primordial + 9 large slots ever written"
    );
    assert_eq!(rec.primordial.count, 1, "the heap's own primordial segment");
    assert_dual_enumeration_identity(&rec, "extension_slot");
}

// ---------------------------------------------------------------------------
// Arm 4 — the primordial segment's committed bytes.
// ---------------------------------------------------------------------------

/// A fresh heap under `primordial-lazy-commit` on the ONLY platform where
/// that policy is a genuine partial commit (real Windows, not miri, not
/// numa-aware): the primordial's committed bytes must be the lazy frontier,
/// strictly below the whole SEGMENT.
///
/// Counterfactual: pre-fix this read the full `SEGMENT` (defect 3), so the
/// `< SEGMENT` assertion fails on EVERY host — on a 4 KiB-page Windows host
/// the frontier is short of SEGMENT by ~3.75 MiB (LAZY_FIRST_CHUNK is only
/// the FIRST committed chunk, not the whole segment).
#[cfg(all(
    feature = "primordial-lazy-commit",
    not(feature = "numa-aware"),
    windows,
    not(miri)
))]
#[test]
fn primordial_committed_bytes_are_the_lazy_frontier() {
    let mut ac = AllocCore::new().expect("AllocCore::new must survive bootstrap");

    // Snapshot on a fresh heap — nothing has carved past the bootstrap stamp.
    let rec = ac.dbg_segment_state_reconciliation();

    // Independent arithmetic expectation: the same `lazy_initial_commit`
    // formula `bootstrap::primordial` stamps, recomputed from the public
    // test-only forwarders (page-rounded `primordial_meta_end` +
    // LAZY_FIRST_CHUNK).
    let expected = SegmentLayout::primordial_lazy_initial_commit(aligned_vmem::page_size()) as u64;
    assert!(
        expected < SegmentLayout::SEGMENT as u64,
        "sanity: the lazy initial commit ({expected}) must be strictly below \
         the whole segment ({})",
        SegmentLayout::SEGMENT
    );

    assert_eq!(rec.unknown_count, 0, "no corrupt segment headers");
    assert_eq!(rec.primordial.count, 1, "one primordial segment per heap");
    assert_eq!(
        rec.primordial.committed_bytes, expected,
        "R2-14 defect 3: the primordial's committed bytes must be the lazy \
         frontier ({expected}) — pre-fix this read the full SEGMENT"
    );
    assert!(
        rec.primordial.committed_bytes < SegmentLayout::SEGMENT as u64,
        "the lazy frontier must be strictly partial"
    );
    assert_eq!(
        rec.primordial.reserved_bytes,
        SegmentLayout::SEGMENT as u64,
        "the whole 4 MiB reservation stays reserved either way"
    );

    // Cross-check against the independent frontier SEAM (not the getter
    // under test): one small alloc into the primordial cannot advance the
    // frontier past the initial chunk (the refill batch is a few hundred
    // bytes, far inside LAZY_FIRST_CHUNK).
    let p = ac.alloc(Layout::from_size_align(16, 8).expect("16/8 layout"));
    assert!(!p.is_null(), "the primordial must serve a 16-byte block");
    assert_eq!(
        ac.dbg_committed_payload_end_for(p),
        Some(expected as usize),
        "activation: the frontier seam must agree with the reconciliation \
         figure"
    );
    assert_eq!(rec.table_high_water, 1, "a fresh heap has written one slot");
    assert_eq!(rec.table_recycled_null_slots, 0, "no recycled slots yet");
    assert_dual_enumeration_identity(&rec, "lazy_primordial");
    // SAFETY: `p` was returned by the matching `ac.alloc` above, is live, and
    // is freed exactly once here.
    unsafe {
        ac.dealloc(p, Layout::from_size_align(16, 8).expect("16/8 layout"));
    }
}

/// `primordial-lazy-commit` is ON but the platform/feature combination forces
/// the EAGER reservation — `numa-aware`'s P2 gate (NUMA reservations stay
/// eager), or Unix/miri where `reserve_aligned_lazy` commits the whole span
/// up front (R8-5). `--all-features` turns `numa-aware` on, so this is the
/// arm that runs there; the reconciliation must still report the honest
/// (full-SEGMENT) frontier.
#[cfg(all(
    feature = "primordial-lazy-commit",
    any(feature = "numa-aware", not(windows), miri)
))]
#[test]
fn primordial_committed_bytes_are_segment_when_lazy_policy_is_forced_eager() {
    let mut ac = AllocCore::new().expect("AllocCore::new must survive bootstrap");
    let rec = ac.dbg_segment_state_reconciliation();

    assert_eq!(rec.unknown_count, 0, "no corrupt segment headers");
    assert_eq!(rec.primordial.count, 1, "one primordial segment per heap");
    assert_eq!(
        rec.primordial.committed_bytes,
        SegmentLayout::SEGMENT as u64,
        "under numa-aware / Unix / miri the primordial reservation commits \
         the whole span, so the frontier — and the reconciliation — read SEGMENT"
    );
    assert_eq!(
        rec.primordial.reserved_bytes,
        SegmentLayout::SEGMENT as u64,
        "the whole 4 MiB reservation stays reserved"
    );

    let p = ac.alloc(Layout::from_size_align(16, 8).expect("16/8 layout"));
    assert!(!p.is_null(), "the primordial must serve a 16-byte block");
    assert_eq!(
        ac.dbg_committed_payload_end_for(p),
        Some(SegmentLayout::SEGMENT),
        "activation: the frontier seam must agree with the reconciliation"
    );
    assert_dual_enumeration_identity(&rec, "lazy_primordial_eager_reality");
    // SAFETY: `p` was returned by the matching `ac.alloc` above, is live, and
    // is freed exactly once here.
    unsafe {
        ac.dealloc(p, Layout::from_size_align(16, 8).expect("16/8 layout"));
    }
}

/// The eager counterpart: `primordial-lazy-commit` OFF (which is what every
/// non-`production` invocation of this file's gate selects, e.g. plain
/// `alloc-decommit,internals,bench-internals`). The primordial is reserved
/// with the plain eager `Segment::reserve`, so the whole segment is the
/// honest commit charge — the fix must NOT overcorrect the eager path.
#[cfg(not(feature = "primordial-lazy-commit"))]
#[test]
fn primordial_committed_bytes_are_the_whole_segment_when_eager() {
    let mut ac = AllocCore::new().expect("AllocCore::new must survive bootstrap");
    let rec = ac.dbg_segment_state_reconciliation();

    assert_eq!(rec.unknown_count, 0, "no corrupt segment headers");
    assert_eq!(rec.primordial.count, 1, "one primordial segment per heap");
    assert_eq!(
        rec.primordial.committed_bytes,
        SegmentLayout::SEGMENT as u64,
        "the eager backend commits the whole segment at reservation time"
    );
    assert_eq!(
        rec.primordial.reserved_bytes,
        SegmentLayout::SEGMENT as u64,
        "the whole 4 MiB reservation stays reserved"
    );

    // Under the sibling lazy feature (`small-segment-lazy-commit` alone —
    // `production` is NOT in this arm's cfg, so this combination is only
    // reachable from an explicit mixed invocation) the frontier seam exists
    // and must agree that the primordial is fully committed: bootstrap stamps
    // `SEGMENT` whenever `primordial-lazy-commit` itself is off.
    let p = ac.alloc(Layout::from_size_align(16, 8).expect("16/8 layout"));
    assert!(!p.is_null(), "the primordial must serve a 16-byte block");
    #[cfg(any(
        feature = "primordial-lazy-commit",
        feature = "small-segment-lazy-commit"
    ))]
    assert_eq!(
        ac.dbg_committed_payload_end_for(p),
        Some(SegmentLayout::SEGMENT),
        "activation: the frontier seam must agree that the primordial is \
         fully committed"
    );
    // SAFETY: `p` was returned by the matching `ac.alloc` above, is live,
    // and is freed exactly once here.
    unsafe {
        ac.dealloc(p, Layout::from_size_align(16, 8).expect("16/8 layout"));
    }
    assert_dual_enumeration_identity(&rec, "eager_primordial");
}

// ---------------------------------------------------------------------------
// Arm 5 — a small segment's frontier advanced partway by grow-on-carve.
// ---------------------------------------------------------------------------

/// Drive the carve out of the primordial into a SECOND small segment, then
/// keep carving so the frontier grows past its initial lazy chunk. Returns a
/// pointer INTO the second segment. The allocations are deliberately leaked
/// (mirrors `tests/lazy_commit_frontier.rs`): `AllocCore::drop` releases the
/// whole segment regardless, and the caller only needs the pointer for seam
/// reads.
#[cfg(feature = "small-segment-lazy-commit")]
fn carve_into_second_small_segment(ac: &mut AllocCore) -> *mut u8 {
    let seed = ac.alloc(Layout::from_size_align(16, 8).expect("16/8 layout"));
    assert!(!seed.is_null(), "the primordial must serve a 16-byte block");
    let prim_base = SegmentLayout::segment_base_of(seed as usize);

    let mut second = core::ptr::null_mut();
    for _ in 0..500_000 {
        let p = ac.alloc(Layout::from_size_align(16, 8).expect("16/8 layout"));
        assert!(!p.is_null(), "small alloc must not fail on a healthy heap");
        if SegmentLayout::segment_base_of(p as usize) != prim_base {
            second = p;
            break;
        }
    }
    assert!(
        !second.is_null(),
        "the carve must move to a second small segment — a failure here means \
         the heap never exhausted the primordial, and the arm would be vacuous"
    );

    // Keep carving ~640 KiB into the second segment: more than the initial
    // LAZY_FIRST_CHUNK (256 KiB), so grow-on-carve genuinely commits a
    // GROW_CHUNK and advances the frontier past its initial value.
    for _ in 0..40_000 {
        let p = ac.alloc(Layout::from_size_align(16, 8).expect("16/8 layout"));
        assert!(!p.is_null(), "small alloc must not fail on a healthy heap");
    }
    second
}

/// A fresh second small segment under `small-segment-lazy-commit` on the ONLY
/// platform where that policy is a genuine partial commit (real Windows, not
/// miri, not numa-aware): `small_active.committed_bytes` must equal the
/// advanced frontier (the independent seam value) and be strictly below
/// SEGMENT.
///
/// Counterfactual: pre-fix this read the full `SEGMENT` (defect 3), so the
/// seam-equality AND the `< SEGMENT` assertion both fail on every host —
/// the frontier is short of SEGMENT by up to ~3.75 MiB.
#[cfg(all(
    feature = "small-segment-lazy-commit",
    not(feature = "numa-aware"),
    windows,
    not(miri)
))]
#[test]
fn small_active_committed_bytes_are_the_advanced_frontier() {
    let mut ac = AllocCore::new().expect("AllocCore::new must survive bootstrap");
    let second = carve_into_second_small_segment(&mut ac);

    let frontier = ac
        .dbg_committed_payload_end_for(second)
        .expect("second segment must be owned by this heap");
    let initial = SegmentLayout::small_lazy_initial_commit(aligned_vmem::page_size()) as u64;
    assert!(
        frontier < SegmentLayout::SEGMENT,
        "sanity: a lazily-committed small segment's frontier must be \
         strictly partial ({frontier} vs {})",
        SegmentLayout::SEGMENT
    );
    assert!(
        frontier as u64 > initial,
        "activation: grow-on-carve must have advanced the frontier past the \
         initial chunk ({frontier} vs {initial})"
    );

    let rec = ac.dbg_segment_state_reconciliation();

    assert_eq!(rec.unknown_count, 0, "no corrupt segment headers");
    assert_eq!(rec.primordial.count, 1, "the heap's own primordial segment");
    assert_eq!(
        rec.small_active.count, 1,
        "the second (carving) segment is the only active small segment"
    );
    assert_eq!(
        rec.small_active.committed_bytes, frontier as u64,
        "R2-14 defect 3: small_active committed bytes must be the advanced \
         frontier ({frontier}) — pre-fix this read the full SEGMENT"
    );
    assert!(
        rec.small_active.committed_bytes < SegmentLayout::SEGMENT as u64,
        "the lazy frontier must be strictly partial"
    );
    assert_eq!(
        rec.small_active.reserved_bytes,
        SegmentLayout::SEGMENT as u64,
        "the whole 4 MiB reservation stays reserved"
    );
    assert_eq!(rec.small_pooled.count, 0, "nothing was pooled in this heap");
    assert_eq!(
        rec.small_empty_orphan.count, 0,
        "only two segments exist (primordial + the active one)"
    );
    assert_eq!(rec.table_high_water, 2, "primordial + the second segment");
    assert_eq!(rec.table_recycled_null_slots, 0, "nothing recycled yet");
    assert_dual_enumeration_identity(&rec, "partial_small");
}

/// `small-segment-lazy-commit` is ON but the platform/feature combination
/// forces the EAGER reservation — `numa-aware`'s P2 gate (which
/// `--all-features` turns on), or Unix/miri where `reserve_aligned_lazy`
/// commits the whole span up front (R8-5). The second small segment then
/// starts AND stays at the full-SEGMENT frontier; the reconciliation must
/// still agree with the seam.
#[cfg(all(
    feature = "small-segment-lazy-commit",
    any(feature = "numa-aware", not(windows), miri)
))]
#[test]
fn small_active_committed_bytes_are_segment_when_lazy_policy_is_forced_eager() {
    let mut ac = AllocCore::new().expect("AllocCore::new must survive bootstrap");
    let second = carve_into_second_small_segment(&mut ac);

    let frontier = ac
        .dbg_committed_payload_end_for(second)
        .expect("second segment must be owned by this heap");
    assert_eq!(
        frontier,
        SegmentLayout::SEGMENT,
        "activation: under numa-aware / Unix / miri the small-segment \
         reservation commits the whole span, so the frontier reads SEGMENT"
    );

    let rec = ac.dbg_segment_state_reconciliation();

    assert_eq!(rec.unknown_count, 0, "no corrupt segment headers");
    assert_eq!(rec.small_active.count, 1, "the second (carving) segment");
    assert_eq!(
        rec.small_active.committed_bytes,
        SegmentLayout::SEGMENT as u64,
        "the eager-forced frontier is the whole segment"
    );
    assert_dual_enumeration_identity(&rec, "partial_small_eager_reality");
}
