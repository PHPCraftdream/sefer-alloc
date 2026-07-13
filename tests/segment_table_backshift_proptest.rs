//! R4-8/N3 — correctness backstop for `SegmentTable`'s backward-shift hash
//! deletion.
//!
//! `SegmentTable`'s open-addressing hash now deletes entries with backward-shift
//! deletion (Knuth TAOCP §6.4, Algorithm R) instead of tombstoning + periodic
//! rebuild. This eliminates tombstones (and the N3 rebuild tail-latency spike)
//! but the shift-eligibility check is subtle: a bug there corrupts the probe
//! chain so a later `hash_contains` of a LIVE base reports `false` (a false
//! negative that would route a foreign free as own-thread — UB / M2 breach).
//!
//! This file is the property-test backstop for that change. It drives a random
//! sequence of insert/remove/contains against the REAL hash operations (via the
//! `SegmentHashHarness` test seam, which exposes `hash_insert`/`hash_remove`/
//! `hash_contains` over heap-owned backing storage) and an oracle
//! `HashSet<usize>` model, asserting FULL membership agreement after every
//! operation — for every base in the universe, both positive (live → found) and
//! negative (absent → not found).
//!
//! ## Why a harness, not the `AllocCore` public API
//!
//! Through `AllocCore` the hash is only reachable via segment register/
//! unregister, which reserve 4 MiB OS segments per distinct base and — crucially
//! — hand out virtual addresses whose hash indices never deterministically
//! straddle the table's cyclic wrap boundary (`HASH_CAPACITY-1 → 0`). The
//! harness uses SYNTHETIC SEGMENT-aligned pointer values (never dereferenced),
//! so the test can target ANY hash index, including a cluster that wraps the
//! table end — exactly the edge case the shift-eligibility condition is hardest
//! to get right for. This is the one place that wrap-around is reachable.
//!
//! Per the short-scenario policy (`CLAUDE.md`): ~64 cases.

#![cfg(feature = "alloc-core")]

use std::collections::HashSet;

use proptest::prelude::*;
use sefer_alloc::alloc_core::SegmentHashHarness;

/// A hash index in the table's universe. Kept as `usize` for clean modular
/// arithmetic against `SegmentHashHarness::CAPACITY`.
type Idx = usize;

/// Operation applied to both the harness and the oracle model. `usize` selects
/// a member of the universe.
#[derive(Clone, Debug)]
enum Op {
    Insert(Idx),
    Remove(Idx),
}

/// Generate a universe: a contiguous (wrapping) range of hash indices. A
/// contiguous range forms a DENSE cluster when fully inserted, which maximises
/// the shift work backward-shift deletion must do on a mid-cluster removal —
/// the exact scenario the eligibility check must survive. The range may straddle
/// the table's wrap boundary (`CAPACITY-1 → 0`), which is the edge case the
/// `AllocCore` API cannot reach.
fn universe_strategy() -> impl Strategy<Value = (Idx, Vec<Idx>)> {
    let cap = SegmentHashHarness::CAPACITY;
    (0..cap, 1usize..=(cap / 4)).prop_map(move |(start, len)| {
        let members: Vec<Idx> = (0..len).map(|k| (start + k) % cap).collect();
        (start, members)
    })
}

/// Operation stream: random insert/remove of universe members.
fn ops_strategy() -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(
        prop_oneof![
            any::<usize>().prop_map(Op::Insert),
            any::<usize>().prop_map(Op::Remove),
        ],
        0..400,
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn backshift_hash_matches_oracle_model(
        (_start, universe) in universe_strategy(),
        raw_ops in ops_strategy(),
    ) {
        let mut h = SegmentHashHarness::new();
        let mut model: HashSet<usize> = HashSet::new();
        let cap = SegmentHashHarness::CAPACITY;

        for op in raw_ops {
            // Map the op's raw index onto a universe member.
            let member = if universe.is_empty() { 0 } else { universe[op_index(&op) % universe.len()] };
            let base = SegmentHashHarness::base_for_index(member);

            match op {
                Op::Insert(_) => {
                    // `hash_insert`'s precondition is "base not already
                    // present" (matching real usage: a segment base is
                    // registered exactly once). Guard the harness call so we
                    // never double-insert; the oracle's `HashSet::insert` is
                    // idempotent regardless.
                    if !model.contains(&member) {
                        h.insert(base);
                    }
                    model.insert(member);
                }
                Op::Remove(_) => {
                    h.remove(base);
                    model.remove(&member);
                }
            }

            // FULL membership agreement after EVERY op: for every universe
            // member, the harness and the oracle agree. This catches both false
            // negatives (a live base reported absent — the backshift corruption
            // signature) and false positives (a removed base still present).
            for &m in &universe {
                let b = SegmentHashHarness::base_for_index(m);
                let harness_says = h.contains(b);
                let model_says = model.contains(&m);
                prop_assert_eq!(
                    harness_says, model_says,
                    "membership disagreement after op {:?} on member {} \
                     (universe start idx, cap={}): harness={}, model={}",
                    op, m, cap, harness_says, model_says
                );
            }

            // Also check a few hash indices OUTSIDE the universe: they must
            // always be absent (the harness must never invent a live entry).
            for outside in [cap / 2, (cap / 2 + 1) % cap, (cap / 2 + 2) % cap] {
                if !universe.contains(&outside) {
                    let b = SegmentHashHarness::base_for_index(outside);
                    prop_assert!(
                        !h.contains(b),
                        "harness reports a live entry for an index {} that was \
                         never inserted",
                        outside
                    );
                }
            }
        }
    }
}

/// Extract the inner selector from an `Op` (both variants carry one `usize`).
fn op_index(op: &Op) -> usize {
    match op {
        Op::Insert(i) | Op::Remove(i) => *i,
    }
}

// ===========================================================================
// Targeted deterministic edge cases for the shift-eligibility condition.
// These fix specific shapes so a bug in the cyclic interval test is caught
// even if the randomised case above happens not to sample that exact shape.
// ===========================================================================

/// A dense contiguous cluster (indices 0..16) survives a mid-cluster removal:
/// every other member stays findable, the removed one becomes absent.
#[test]
fn backshift_dense_cluster_mid_removal() {
    let mut h = SegmentHashHarness::new();
    let members: Vec<usize> = (0..16).collect();
    for &m in &members {
        h.insert(SegmentHashHarness::base_for_index(m));
    }
    // Remove a middle entry — forces a multi-entry backward shift.
    h.remove(SegmentHashHarness::base_for_index(5));
    assert!(
        !h.contains(SegmentHashHarness::base_for_index(5)),
        "removed entry still present"
    );
    for &m in &members {
        if m == 5 {
            continue;
        }
        assert!(
            h.contains(SegmentHashHarness::base_for_index(m)),
            "live entry {} vanished after mid-cluster removal of 5 \
             (probe chain corrupted by backward-shift)",
            m
        );
    }
}

/// A cluster that WRAPS the table boundary (`CAPACITY-1 → 0`) survives removal.
/// This is the wrap-around case the `AllocCore` API cannot reach and the
/// subtlest case for the cyclic shift-eligibility test.
#[test]
fn backshift_wraparound_cluster_removal() {
    let cap = SegmentHashHarness::CAPACITY;
    let mut h = SegmentHashHarness::new();
    // Cluster straddling the wrap boundary: ..., cap-3, cap-2, cap-1, 0, 1, 2.
    let members: Vec<usize> = vec![cap - 3, cap - 2, cap - 1, 0, 1, 2];
    for &m in &members {
        h.insert(SegmentHashHarness::base_for_index(m));
    }
    // Remove the entry at the boundary itself (cap-1).
    h.remove(SegmentHashHarness::base_for_index(cap - 1));
    assert!(
        !h.contains(SegmentHashHarness::base_for_index(cap - 1)),
        "removed wrap-boundary entry still present"
    );
    for &m in &members {
        if m == cap - 1 {
            continue;
        }
        assert!(
            h.contains(SegmentHashHarness::base_for_index(m)),
            "live wrap-cluster entry {} vanished after removing cap-1",
            m
        );
    }

    // Remove an entry on the post-wrap side (index 1) and re-check.
    h.remove(SegmentHashHarness::base_for_index(1));
    assert!(!h.contains(SegmentHashHarness::base_for_index(1)));
    for &m in &members {
        if m == cap - 1 || m == 1 {
            continue;
        }
        assert!(
            h.contains(SegmentHashHarness::base_for_index(m)),
            "live entry {} vanished after removing post-wrap index 1",
            m
        );
    }
}

/// Repeated insert/remove churn of a FIXED universe at high load factor must
/// never corrupt membership, regardless of removal order. This is the
/// deterministic counter-shape to the randomised proptest: it exhaustively
/// removes and re-inserts every member, in order, many times.
#[test]
fn backshift_churn_fixed_universe_all_orders() {
    let mut h = SegmentHashHarness::new();
    // 64 contiguous indices — a dense cluster, well under the 50% load factor.
    let members: Vec<usize> = (0..64).collect();
    for &m in &members {
        h.insert(SegmentHashHarness::base_for_index(m));
    }
    // Remove every member one at a time; after each removal every SURVIVOR must
    // remain present and every removed one absent.
    for (k, &removed) in members.iter().enumerate() {
        h.remove(SegmentHashHarness::base_for_index(removed));
        for (j, &m) in members.iter().enumerate() {
            let present = h.contains(SegmentHashHarness::base_for_index(m));
            if j <= k {
                // Already removed.
                assert!(
                    !present,
                    "removed entry {} re-appeared mid-churn (k={})",
                    m, k
                );
            } else {
                assert!(
                    present,
                    "survivor {} vanished after removing {} (k={}) — \
                     backward-shift corrupted the probe chain",
                    m, removed, k
                );
            }
        }
    }
    // Table should now be fully empty of the universe.
    for &m in &members {
        assert!(!h.contains(SegmentHashHarness::base_for_index(m)));
    }

    // Re-insert everything (exercises re-population after a fully-shifted-out
    // cluster) and verify all present.
    for &m in &members {
        h.insert(SegmentHashHarness::base_for_index(m));
    }
    for &m in &members {
        assert!(h.contains(SegmentHashHarness::base_for_index(m)));
    }
}

/// Removing a base that was never inserted is a defensive no-op: it must not
/// corrupt any live entry's probe chain.
#[test]
fn backshift_remove_absent_is_noop() {
    let mut h = SegmentHashHarness::new();
    h.insert(SegmentHashHarness::base_for_index(3));
    h.insert(SegmentHashHarness::base_for_index(4));
    // Remove something never inserted.
    h.remove(SegmentHashHarness::base_for_index(10));
    assert!(h.contains(SegmentHashHarness::base_for_index(3)));
    assert!(h.contains(SegmentHashHarness::base_for_index(4)));
    assert!(!h.contains(SegmentHashHarness::base_for_index(10)));
}
