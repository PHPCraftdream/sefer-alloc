//! J13b C4: bounded multi-thread stress of one shared budget ledger.
#![cfg(all(
    feature = "internals",
    feature = "alloc-decommit",
    not(feature = "numa-aware")
))]

use sefer_alloc::alloc_core::policy::large_thp_runtime::ThpBudgetLedger;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

const MIB: usize = 1024 * 1024;
const BUDGET: usize = 64 * MIB;
const THREADS: usize = 8;
const ITERS: usize = 20_000;
const HOLD: usize = 4;

#[test]
fn shared_ledger_never_exceeds_budget() {
    let ledger = Arc::new(ThpBudgetLedger::new());
    let barrier = Arc::new(Barrier::new(THREADS));
    let granted = Arc::new(AtomicU64::new(0));
    let refused = Arc::new(AtomicU64::new(0));
    let handles: Vec<_> = (0..THREADS)
        .map(|t| {
            let ledger = Arc::clone(&ledger);
            let barrier = Arc::clone(&barrier);
            let granted = Arc::clone(&granted);
            let refused = Arc::clone(&refused);
            thread::spawn(move || {
                let mut seed = 0x1234_5678_9ABC_DEF1u64 ^ ((t as u64 + 1) << 32);
                let mut held: Vec<usize> = Vec::new();
                barrier.wait();
                for _ in 0..ITERS {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    let want = 8 * MIB;
                    let got = ledger.try_reserve(want, BUDGET);
                    if got == 0 {
                        refused.fetch_add(1, Ordering::Relaxed);
                    } else {
                        granted.fetch_add(1, Ordering::Relaxed);
                        assert!(got <= want && got & (2 * MIB - 1) == 0);
                        assert!(ledger.used() <= BUDGET);
                        held.push(got);
                    }
                    if held.len() >= HOLD || (seed & 3 == 0 && !held.is_empty()) {
                        ledger.give_back(held.pop().unwrap());
                    }
                }
                for b in held {
                    ledger.give_back(b);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(ledger.used(), 0);
    assert!(granted.load(Ordering::Relaxed) > 0);
    // A sequential schedule may legitimately never exhaust the budget.
    assert_eq!(
        granted.load(Ordering::Relaxed) + refused.load(Ordering::Relaxed),
        (THREADS * ITERS) as u64
    );
}
