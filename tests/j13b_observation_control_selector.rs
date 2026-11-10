//! J13b: integration tests for the large-THP observation control selector
//! (`LargeThpControlSelector`, `bounded_mix`).
//!
//! The module under test is intentionally NOT wired into the crate tree
//! (see its own mod.rs); these tests include it through a `#[path]`
//! attribute instead of a crate path — no production integration is
//! pretended.
//!
//! Coverage: the exact one-control-per-complete-aligned-cohort property
//! across seeds including extremes, request-time visibility of the drawn
//! control inside a PARTIAL cohort (at most one — zero is NOT
//! guaranteed), the exact 1/8 rate over a long stream (share-zero
//! exploration cannot die out), same-seed determinism, reseed consumed
//! at the START of the next cohort-opening call (pending SURVIVES the
//! cohort-completing request), reseed changing the draws at MATCHED
//! ordinals (a mutant that ignores the seed is caught), two controls
//! inside one sliding 8-request window being possible WITHOUT violating
//! the per-cohort contract (fixture located by an exhaustive bounded
//! scan of the public `bounded_mix` primitive and verified against the
//! selector's real decisions), decision records processed under three
//! different FREE-PROCESSING ORDERS with the selector state untouched,
//! selector independence (no shared global state, no thread/business
//! identity), totality of the pure `decision_at` function at the `u64`
//! ordinal wrap boundary PLUS a STATEFUL wrap test driving the real
//! counter from ordinal `u64::MAX - 7` through the exact `MAX -> 0`
//! wrap (catching `saturating_add` and plain-`+1` mutants) with the
//! armed reseed consumed exactly by the ordinal-0 opening call, and
//! totality/injectivity of `bounded_mix` including the extreme stream
//! index.
//!
//! Oracle discipline: cohort-control counts are black-box counts over
//! the selector's own decisions (no formula re-derivation); partial-
//! cohort and sliding-window fixtures are located by scanning the
//! public primitive and then checked against the selector's real
//! decisions; every comparison assertion is justified inline (the
//! injectivity argument from `bounded_mix`'s bijection contract, or an
//! explicitly-commented statistical bound whose failure would itself
//! be the broken-decorrelator defect the test exists to catch).

#[allow(dead_code)]
#[path = "../src/alloc_core/config/policy/large_thp_observation/mod.rs"]
mod large_thp_observation;

use large_thp_observation::bounded_mix::bounded_mix;
use large_thp_observation::large_thp_control_selector::LargeThpControlSelector;

/// Feed `n` eligible fresh requests, returning the number of controls.
fn controls_in_first_n(selector: &mut LargeThpControlSelector, n: u64) -> u64 {
    let mut controls = 0;
    for _ in 0..n {
        if selector.classify_eligible_request() {
            controls += 1;
        }
    }
    controls
}

/// Per-cohort control counts for the first `cohorts` complete cohorts.
fn cohort_control_counts(selector: &mut LargeThpControlSelector, cohorts: u64) -> Vec<u64> {
    let mut counts = Vec::new();
    for _ in 0..cohorts {
        let mut in_cohort = 0;
        for _ in 0..LargeThpControlSelector::COHORT_LEN {
            if selector.classify_eligible_request() {
                in_cohort += 1;
            }
        }
        counts.push(in_cohort);
    }
    counts
}

#[test]
fn one_control_per_complete_aligned_cohort_across_seeds() {
    // Exact fixed-cohort oracle: a complete cohort of 8 eligible fresh
    // requests holds EXACTLY one control — counted black-box from the
    // decisions, for every seed (including 0 and u64::MAX).
    for seed in [0u64, 1, 42, 0xFFFF_FFFF, u64::MAX] {
        let counts = cohort_control_counts(&mut LargeThpControlSelector::new(seed), 64);
        assert_eq!(counts.len(), 64);
        for (cohort, count) in counts.iter().enumerate() {
            assert_eq!(
                *count, 1,
                "seed {seed}: cohort {cohort} must hold exactly one control"
            );
        }
    }
}

#[test]
fn partial_cohort_shows_at_most_one_request_time_control() {
    // The decision happens AT REQUEST TIME: the cohort's drawn control
    // position may lie anywhere in 0..8, so a PARTIAL prefix of 1..7
    // requests holds AT MOST one control — exactly one iff the drawn
    // position lies inside the prefix; ZERO is NOT guaranteed (a control
    // drawn at position 0 is visible from the first request of its
    // cohort). The expected count per (cohort, prefix) pair is located
    // by the PUBLIC primitive (the same fixture pattern as the
    // sliding-window test) and then checked against the selector's real
    // decisions. Across the 17 scanned cohorts the drawn positions vary;
    // the fixture would only degenerate if EVERY drawn position were 7 —
    // a ~(1/8)^17 coincidence whose occurrence would itself signal a
    // broken mixer.
    for cohort in 0u64..=16 {
        let drawn = bounded_mix(0xABCD, cohort) % LargeThpControlSelector::COHORT_LEN;
        for r in 1u64..8 {
            let mut selector = LargeThpControlSelector::new(0xABCD);
            let controls = controls_in_first_n(
                &mut selector,
                cohort * LargeThpControlSelector::COHORT_LEN + r,
            );
            let expected = cohort + u64::from(drawn < r);
            assert_eq!(
                controls, expected,
                "cohort {cohort} prefix {r}: the control drawn at position {drawn} is visible iff inside the prefix"
            );
            assert_eq!(
                selector.requests_seen(),
                cohort * LargeThpControlSelector::COHORT_LEN + r
            );
        }
    }
}

#[test]
fn long_stream_rate_is_exactly_one_eighth_exploration_never_dies_out() {
    // Share-zero exploration: the selector has no share input and no
    // learner, so the same selection continues forever. Over 10_000
    // eligible fresh requests there are EXACTLY 1_250 controls (one per
    // complete cohort) — the rate cannot decay, adapt, or die out.
    let mut selector = LargeThpControlSelector::new(7);
    let controls = controls_in_first_n(&mut selector, 10_000);
    assert_eq!(controls, 1_250);
    assert_eq!(selector.requests_seen(), 10_000);
    assert_eq!(selector.cohort_index(), 1_250);
}

#[test]
fn same_seed_determinism() {
    // Two fresh selectors with the same seed produce element-wise
    // identical decisions and identical observer values (the decision is
    // a pure function of (seed, request ordinal)).
    let mut a = LargeThpControlSelector::new(0x5EED);
    let mut b = LargeThpControlSelector::new(0x5EED);
    for i in 0..256u64 {
        assert_eq!(
            a.classify_eligible_request(),
            b.classify_eligible_request(),
            "request {i}"
        );
        assert_eq!(a.requests_seen(), b.requests_seen());
        assert_eq!(a.cohort_index(), b.cohort_index());
        assert_eq!(a.effective_seed(), b.effective_seed());
        assert_eq!(a.has_pending_reseed(), b.has_pending_reseed());
    }
}

#[test]
fn reseed_is_consumed_at_the_start_of_the_next_cohorts_first_request() {
    // Consumption semantics (parent-confirmed): `schedule_reseed` ARMS
    // the seed; it is consumed at the START of the next
    // `classify_eligible_request` call, which opens the next cohort. So
    // after the request that completes the current cohort the armed seed
    // is STILL pending and the effective seed is still the old one; the
    // in-flight cohort completed under the seed it opened with. The NEXT
    // call applies the new seed before deciding, so its decision matches
    // a fresh new-seed reference at the same cohort index.
    const OLD: u64 = 100;
    const NEW: u64 = 200;

    let mut old_ref = LargeThpControlSelector::new(OLD);
    let mut old_ref_cohort = [false; 8];
    for slot in old_ref_cohort.iter_mut() {
        *slot = old_ref.classify_eligible_request();
    }

    let mut a = LargeThpControlSelector::new(OLD);
    for _ in 0..3 {
        let _ = a.classify_eligible_request();
    }
    a.schedule_reseed(NEW);
    assert!(a.has_pending_reseed());
    assert_eq!(
        a.effective_seed(),
        OLD,
        "a scheduled reseed must not apply mid-cohort"
    );

    // Complete the in-flight cohort; its remaining decisions must match
    // the old-seed reference positions 3..8.
    for slot in old_ref_cohort[3..].iter() {
        assert_eq!(a.classify_eligible_request(), *slot);
    }
    // Cohort complete — the armed seed is STILL pending (consumed only
    // at the START of the next call) and the old seed still governs.
    assert!(
        a.has_pending_reseed(),
        "the armed seed survives the cohort-completing request"
    );
    assert_eq!(
        a.effective_seed(),
        OLD,
        "the old seed governs until the next call starts"
    );

    // The next call consumes the armed seed first, then decides: its
    // decision matches a fresh NEW-seed reference at the same cohort
    // index (cohort 1), and the pending flag is gone afterwards.
    let mut new_ref = LargeThpControlSelector::new(NEW);
    let _ = controls_in_first_n(&mut new_ref, LargeThpControlSelector::COHORT_LEN);
    let a_decision = a.classify_eligible_request();
    let ref_decision = new_ref.classify_eligible_request();
    assert_eq!(a_decision, ref_decision, "first post-boundary request");
    assert!(
        !a.has_pending_reseed(),
        "the armed seed was consumed by the cohort-opening call"
    );
    assert_eq!(a.effective_seed(), NEW);
    for i in 1..8 {
        let a_decision = a.classify_eligible_request();
        let ref_decision = new_ref.classify_eligible_request();
        assert_eq!(a_decision, ref_decision, "post-reseed request {i}");
    }
}

#[test]
fn reseed_changes_draws_at_matched_ordinals() {
    // Counterfactual: a mutant that records `schedule_reseed` in
    // `effective_seed` but never lets the armed seed influence the
    // decisions produces a stream identical to the un-reseeded one. With
    // MATCHED ordinals (both selectors have classified the same number
    // of requests before the comparison starts), any difference is
    // attributable to the seed alone. The previous version of this test
    // compared streams offset by one cohort, which proves nothing about
    // the seed. Failure probability on a correct mixer: seed 1 vs seed 0
    // folding identically at every one of the 4096 scanned cohorts is
    // ~(1/8)^4096.
    let mut reseeded = LargeThpControlSelector::new(0);
    let _ = controls_in_first_n(&mut reseeded, LargeThpControlSelector::COHORT_LEN);
    reseeded.schedule_reseed(1);
    let _ = controls_in_first_n(&mut reseeded, LargeThpControlSelector::COHORT_LEN);
    assert_eq!(reseeded.effective_seed(), 1);
    assert!(!reseeded.has_pending_reseed());

    let mut plain = LargeThpControlSelector::new(0);
    let _ = controls_in_first_n(&mut plain, 2 * LargeThpControlSelector::COHORT_LEN);
    assert_eq!(
        reseeded.requests_seen(),
        plain.requests_seen(),
        "matched ordinals are the premise of the comparison"
    );
    let mut any_diff = false;
    for _ in 0..4096 * LargeThpControlSelector::COHORT_LEN {
        if reseeded.classify_eligible_request() != plain.classify_eligible_request() {
            any_diff = true;
            break;
        }
    }
    assert!(
        any_diff,
        "a reseed whose draws stay identical to the un-reseeded stream at matched ordinals is a mutant that ignores the seed"
    );
}

#[test]
fn sliding_window_of_8_can_hold_two_controls_without_contract_violation() {
    // The amendment is explicit: an arbitrary window of 8 requests may
    // contain 2 controls (one per adjacent cohort). The fixture is found
    // by an exhaustive bounded scan of the PUBLIC primitive — adjacent
    // cohorts c, c+1 of seed 0 where the second cohort's drawn position
    // is SMALLER than the first's (28/64 of pairs qualify; 16_383 pairs
    // are scanned) — then verified against the selector's actual
    // decisions: the 8-request window starting at the first control
    // holds exactly that pair, while every cohort involved still holds
    // exactly one control. No hard sliding-window cap exists; the hard
    // cap is per complete cohort only.
    const SEED: u64 = 0;
    let mut fixture = None;
    for c in 1u64..16_384 {
        let p1 = bounded_mix(SEED, c) % LargeThpControlSelector::COHORT_LEN;
        let p2 = bounded_mix(SEED, c + 1) % LargeThpControlSelector::COHORT_LEN;
        if p2 < p1 {
            fixture = Some((c, p1, p2));
            break;
        }
    }
    let (c, p1, p2) = fixture
        .expect("adjacent cohorts with a descending control pair must exist within the scan");

    let mut selector = LargeThpControlSelector::new(SEED);
    let total = (c + 2) * LargeThpControlSelector::COHORT_LEN;
    let mut decisions = Vec::with_capacity(total as usize);
    for _ in 0..total {
        decisions.push(selector.classify_eligible_request());
    }
    let control_at = |i: u64| decisions[i as usize];

    let first = c * LargeThpControlSelector::COHORT_LEN + p1;
    let second = (c + 1) * LargeThpControlSelector::COHORT_LEN + p2;
    assert!(
        control_at(first),
        "fixture: the first control must be a control"
    );
    assert!(
        control_at(second),
        "fixture: the second control must be a control"
    );
    assert!(
        second - first <= 7,
        "both controls must lie within one 8-request window"
    );
    let window_controls: u64 = (first..first + 8).filter(|&i| control_at(i)).count() as u64;
    assert_eq!(
        window_controls, 2,
        "the straddling window holds exactly the two adjacent-cohort controls"
    );
    for cohort in c - 1..=c + 1 {
        let controls: u64 = (cohort * 8..cohort * 8 + 8)
            .filter(|&i| control_at(i))
            .count() as u64;
        assert_eq!(
            controls, 1,
            "cohort {cohort} still holds exactly one control"
        );
    }
}

#[test]
fn decisions_are_immutable_to_free_processing_order_and_observers() {
    // Decisions are made AT REQUEST TIME and recorded as plain values
    // keyed to their request ordinal; the selector exposes no free input,
    // so the order in which the caller processes frees cannot change any
    // classification. The test processes the recorded decisions under
    // three different free-processing orders and pins: (a) the control
    // count per free order is identical, (b) the selector's state is
    // untouched by free processing, (c) read-only observers do not
    // consume the stream, and (d) after "freeing everything" the stream
    // continues exactly as the pure (seed, ordinal) function dictates.
    let mut busy = LargeThpControlSelector::new(0xC0FFEE);
    let mut by_ordinal = Vec::with_capacity(64);
    for i in 0..64u64 {
        by_ordinal.push(busy.classify_eligible_request());
        let _ = busy.effective_seed();
        let _ = busy.has_pending_reseed();
        let _ = busy.cohort_index();
        assert_eq!(
            busy.requests_seen(),
            i + 1,
            "read-only observers must not consume the request stream"
        );
    }
    let request_time_controls: u64 = by_ordinal.iter().filter(|&&c| c).count() as u64;
    assert_eq!(
        request_time_controls, 8,
        "64 requests are 8 complete cohorts: exactly 8 controls"
    );

    let mut strided: Vec<u64> = (0..64).step_by(2).collect();
    let odds: Vec<u64> = (1..64).step_by(2).collect();
    strided.extend(odds.into_iter().rev());
    let orders = [
        (0..64).collect::<Vec<u64>>(),
        (0..64).rev().collect::<Vec<u64>>(),
        strided,
    ];
    for order in orders.iter() {
        let mut freed_controls = 0u64;
        for &ordinal in order.iter() {
            if by_ordinal[ordinal as usize] {
                freed_controls += 1;
            }
            assert_eq!(
                busy.requests_seen(),
                64,
                "free processing must not touch the selector"
            );
        }
        assert_eq!(
            freed_controls, request_time_controls,
            "the control count is identical under every free-processing order"
        );
    }

    let mut clean = LargeThpControlSelector::new(0xC0FFEE);
    for _ in 0..64 {
        let _ = clean.classify_eligible_request();
    }
    for i in 0..16 {
        assert_eq!(
            busy.classify_eligible_request(),
            clean.classify_eligible_request(),
            "post-free request {i}"
        );
    }
}

#[test]
fn selectors_are_independent_no_global_state() {
    // Two same-seed selectors classify identically from their own cohort
    // 0 regardless of construction order or how far another instance has
    // advanced — nothing global ties the stream to anything outside the
    // instance (no thread/task/business identity exists on the API).
    let mut reference = LargeThpControlSelector::new(123);
    let mut first16 = [false; 16];
    for slot in first16.iter_mut() {
        *slot = reference.classify_eligible_request();
    }

    let mut advanced = LargeThpControlSelector::new(123);
    let _ = controls_in_first_n(&mut advanced, 5);

    let mut fresh = LargeThpControlSelector::new(123);
    for (i, expected) in first16.iter().enumerate() {
        assert_eq!(fresh.classify_eligible_request(), *expected, "request {i}");
    }
    // The advanced selector continues the SAME pure stream at its own
    // ordinal — no reset, no interference from the fresh instance.
    for (i, expected) in first16.iter().enumerate().skip(5) {
        assert_eq!(
            advanced.classify_eligible_request(),
            *expected,
            "request {i}"
        );
    }
}

#[test]
fn decision_function_is_total_at_the_ordinal_wrap_boundary() {
    // The no-reseed stream repeats with period 2^64 requests = 2^61
    // cohorts; the pure `decision_at` function IS that stream's
    // definition, so the wrap boundary is exercised directly at the top
    // of the `u64` ordinal domain — no multi-year loop, no private
    // state. Assertions: totality (no panic) and determinism at boundary
    // ordinals; raw primitive injectivity at the extreme stream index
    // (distinct seeds differ by a nonzero pre-mix sum, and the finalizer
    // is a bijection — decisions may fold across seeds, raw draws may
    // not).
    for ordinal in [u64::MAX, u64::MAX - 1, u64::MAX - 7, u64::MAX - 8, 0] {
        let a = LargeThpControlSelector::decision_at(0x1234_5678, ordinal);
        let b = LargeThpControlSelector::decision_at(0x1234_5678, ordinal);
        assert_eq!(a, b, "ordinal {ordinal}: deterministic at the boundary");
    }
    assert_ne!(
        bounded_mix(0, u64::MAX),
        bounded_mix(1, u64::MAX),
        "primitive injectivity at the extreme stream index"
    );
}

#[test]
fn extreme_seeds_are_total_and_the_primitive_is_injective_per_stream() {
    // Totality at extremes: zero seed, u32::MAX and u64::MAX all classify
    // total streams with exactly one control per complete cohort.
    for seed in [0u64, u64::from(u32::MAX), u64::MAX] {
        let counts = cohort_control_counts(&mut LargeThpControlSelector::new(seed), 2);
        assert_eq!(counts, vec![1, 1], "seed {seed}");
    }
    // `bounded_mix` contract pins used by the sliding-window fixture
    // search: distinct input sums finalize to distinct outputs (the
    // finalizer is a bijection), so the same stream index distinguishes
    // seeds and the same seed distinguishes stream indices — including
    // at the top of the stream domain.
    assert_ne!(bounded_mix(0, 0), bounded_mix(1, 0));
    assert_ne!(bounded_mix(0, 0), bounded_mix(0, 1));
    for s in [0u64, 1, u64::MAX] {
        assert_ne!(bounded_mix(s, 0), bounded_mix(s, 1));
        assert_ne!(bounded_mix(s, u64::MAX - 1), bounded_mix(s, u64::MAX));
    }
    assert_eq!(
        bounded_mix(0xDEAD, 77),
        bounded_mix(0xDEAD, 77),
        "pure function: same inputs, same output"
    );
}

#[test]
fn stateful_stream_wraps_the_u64_counter_through_the_real_last_cohort() {
    // REAL stateful wrap coverage (the complement to the pure
    // `decision_at` boundary check): the selector is placed at ordinal
    // `u64::MAX - 7` through the `cfg(test)`-only `with_requests_seen`
    // constructor, and the LAST cohort of 8 requests is driven with
    // real `classify_eligible_request` calls. Pins: every decision
    // matches the INDEPENDENT public `bounded_mix` oracle at cohort
    // `2^61 - 1`; exactly one control in the cohort; `requests_seen`
    // wraps `u64::MAX -> 0` EXACTLY (a `saturating_add` mutant stalls
    // at MAX and fails this pin; a plain `+ 1` mutant panics under
    // overflow checks in CI); the armed reseed is NOT consumed by the
    // boundary cohort (locals 1..7 open no cohort) and is consumed
    // exactly by the call that opens ordinal 0, whose decision then
    // follows the NEW seed.
    const OLD: u64 = 0x11C0_FFEE;
    const NEW: u64 = 0x22FA_11ED;
    let start = u64::MAX - 7;
    let cohort_max = start / LargeThpControlSelector::COHORT_LEN;
    let mut sel = LargeThpControlSelector::with_requests_seen(OLD, start);
    assert_eq!(sel.requests_seen(), start);
    assert_eq!(sel.cohort_index(), cohort_max);

    let mut controls = 0u64;
    for local in 0u64..LargeThpControlSelector::COHORT_LEN {
        if local == 1 {
            // Arm AFTER the cohort-opening call: locals 1..7 open no
            // cohort, so the armed seed must stay pending past the wrap.
            sel.schedule_reseed(NEW);
        }
        let decision = sel.classify_eligible_request();
        let oracle = local == bounded_mix(OLD, cohort_max) % LargeThpControlSelector::COHORT_LEN;
        assert_eq!(
            decision, oracle,
            "boundary cohort local {local}: decision must match the public bounded_mix oracle"
        );
        controls += u64::from(decision);
    }
    assert_eq!(
        controls, 1,
        "the last cohort before the wrap holds exactly one control"
    );
    // THE wrap pin: the counter crossed the boundary EXACTLY.
    assert_eq!(
        sel.requests_seen(),
        0,
        "requests_seen must wrap u64::MAX -> 0 (saturating_add would stall at MAX; plain +1 panics in CI)"
    );
    assert_eq!(sel.cohort_index(), 0, "the next request lands in cohort 0");
    // The armed reseed survived the boundary cohort (locals 1..7 open no
    // cohort) and the old seed still governs until consumption.
    assert!(
        sel.has_pending_reseed(),
        "the armed seed must survive the wrap — no cohort-opening call happened since it was armed"
    );
    assert_eq!(sel.effective_seed(), OLD);

    // The next call opens ordinal 0: it consumes the armed seed FIRST,
    // then decides under NEW.
    let first_new = sel.classify_eligible_request();
    assert_eq!(
        first_new,
        bounded_mix(NEW, 0).is_multiple_of(LargeThpControlSelector::COHORT_LEN),
        "post-wrap request must follow the armed NEW seed"
    );
    assert!(!sel.has_pending_reseed());
    assert_eq!(sel.effective_seed(), NEW);
    assert_eq!(sel.requests_seen(), 1);
    // The rest of the post-wrap cohort continues under NEW; exactly one
    // control across its 8 requests.
    let mut controls_new = u64::from(first_new);
    for local in 1u64..LargeThpControlSelector::COHORT_LEN {
        let decision = sel.classify_eligible_request();
        let oracle = local == bounded_mix(NEW, 0) % LargeThpControlSelector::COHORT_LEN;
        assert_eq!(decision, oracle, "post-wrap cohort local {local}");
        controls_new += u64::from(decision);
    }
    assert_eq!(
        controls_new, 1,
        "the post-wrap cohort holds exactly one control"
    );
}
