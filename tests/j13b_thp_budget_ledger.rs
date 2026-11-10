//! J13b C4: single-thread behaviour of the advice budget ledger against a
//! model of outstanding charges.
#![cfg(all(
    feature = "internals",
    feature = "alloc-decommit",
    not(feature = "numa-aware")
))]

use sefer_alloc::alloc_core::policy::large_thp_runtime::{ThpAdvice, ThpBudgetLedger};

const MIB: usize = 1024 * 1024;
const G: usize = ThpAdvice::GRANULE;

#[test]
fn reserve_floors_to_granule() {
    let l = ThpBudgetLedger::new();
    assert_eq!(l.try_reserve(5 * MIB, 64 * MIB), 4 * MIB);
    assert_eq!(l.used(), 4 * MIB);
    assert_eq!(l.try_reserve(G - 1, 64 * MIB), 0);
    assert_eq!(l.used(), 4 * MIB);
}

#[test]
fn zero_want_and_zero_budget() {
    let l = ThpBudgetLedger::new();
    assert_eq!(l.try_reserve(0, 64 * MIB), 0);
    assert_eq!(l.try_reserve(8 * MIB, 0), 0);
    assert_eq!(l.used(), 0);
}

#[test]
fn exact_exhaustion_and_restore() {
    let l = ThpBudgetLedger::new();
    let budget = 6 * MIB;
    assert_eq!(l.try_reserve(4 * MIB, budget), 4 * MIB);
    assert_eq!(l.try_reserve(4 * MIB, budget), 2 * MIB);
    assert_eq!(l.used(), budget);
    assert_eq!(l.try_reserve(4 * MIB, budget), 0);
    assert_eq!(l.used(), budget);
    l.give_back(2 * MIB);
    assert_eq!(l.used(), 4 * MIB);
    assert_eq!(l.try_reserve(8 * MIB, budget), 2 * MIB);
    l.give_back(0);
    assert_eq!(l.used(), budget);
    l.give_back(4 * MIB);
    l.give_back(2 * MIB);
    assert_eq!(l.used(), 0);
}

#[test]
fn budget_below_granule_never_reserves() {
    let l = ThpBudgetLedger::new();
    assert_eq!(l.try_reserve(8 * MIB, G - 4096), 0);
    assert_eq!(l.used(), 0);
}

#[test]
fn huge_values_do_not_overflow() {
    let l = ThpBudgetLedger::new();
    let got = l.try_reserve(usize::MAX, 10 * MIB);
    assert_eq!(got, 10 * MIB);
    assert_eq!(
        l.try_reserve(usize::MAX, usize::MAX),
        usize::MAX / G * G - 10 * MIB
    );
}

#[test]
fn model_random_sequences() {
    let mut x = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for round in 0..20 {
        let budget = (1 + (next() % 16) as usize) * G + (next() % 4096) as usize;
        let l = ThpBudgetLedger::new();
        let mut out: Vec<usize> = Vec::new();
        let mut sum = 0usize;
        for _ in 0..2000 {
            if out.is_empty() || next() % 3 != 0 {
                let want = (next() % (6 * G as u64)) as usize;
                let got = l.try_reserve(want, budget);
                assert_eq!(got & (G - 1), 0, "round {round}");
                assert!(got <= want);
                let left = budget - sum;
                assert_eq!(got, want.min(left) / G * G);
                if got != 0 {
                    out.push(got);
                    sum += got;
                }
            } else {
                let i = (next() % out.len() as u64) as usize;
                let b = out.swap_remove(i);
                l.give_back(b);
                sum -= b;
            }
            assert_eq!(l.used(), sum);
            assert!(l.used() <= budget);
        }
        for b in out {
            l.give_back(b);
        }
        assert_eq!(l.used(), 0);
    }
}
