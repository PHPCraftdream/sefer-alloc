//! Actual-source model of the large-THP budget ledger: the production
//! `ThpBudgetLedger` (`src/alloc_core/config/policy/large_thp_runtime/
//! thp_budget_ledger.rs`) compiled onto loom atomics, together with the REAL
//! pure advice geometry (`thp_advice.rs`, `thp_advice_outcome.rs`, the
//! `large_thp_*` pure files) — no stubbed logic anywhere.
//! Run with `RUSTFLAGS="--cfg loom" cargo test --release --features
//! "alloc-core alloc-xthread tagged-index-stack/loom" --test loom_thp_ledger`.
//!
//! # Seams (import- and declaration-level; no production statement changed)
//! - `thp_budget_ledger.rs` takes `AtomicU64`/`Ordering` from `super`, the
//!   crate root here, which re-exports loom's.
//! - Included production files resolve their `crate::` paths through a nested
//!   `mod alloc_core` tree that re-exports the root-mounted real files under
//!   the exact module paths the sources use.
//! - No included file carries a feature gate (the `mod.rs` gating does not
//!   apply to `#[path]` includes), so this model needs no `alloc-decommit`.
//!
//! # Oracles
//! - With an unconditional `fetch_add` reserve the grant sum would exceed the
//!   budget; the bounded CAS reserve keeps it at or under the budget — this
//!   model is the conservation proof.
//! - Every grant is a positive multiple of the advice granule; after every
//!   thread returns exactly its own grant the ledger is empty again.
#![cfg(loom)]

use loom::sync::Arc;
use loom::thread;

// Atomic seam: the included `thp_budget_ledger.rs` resolves these via `super`.
pub(crate) use loom::sync::atomic::{AtomicU64, Ordering};

// Root-mounted production files (a `#[path]` inside a nested inline module
// resolves through a `tests/<module>/` directory that does not exist).
#[allow(dead_code)]
#[path = "../src/alloc_core/config/policy/large_thp/large_thp_curve.rs"]
mod large_thp_curve;
#[allow(dead_code)]
#[path = "../src/alloc_core/config/policy/large_thp/large_thp_gradient.rs"]
mod large_thp_gradient;
#[allow(dead_code)]
#[path = "../src/alloc_core/config/policy/large_thp/large_thp_mode.rs"]
mod large_thp_mode;
#[allow(dead_code)]
#[path = "../src/alloc_core/config/policy/large_thp/large_thp_request_group.rs"]
mod large_thp_request_group;
#[allow(dead_code)]
#[path = "../src/alloc_core/config/policy/large_thp/large_thp_share.rs"]
mod large_thp_share;
#[allow(dead_code)]
#[path = "../src/alloc_core/config/policy/large_thp_runtime/thp_advice.rs"]
mod thp_advice;
#[allow(dead_code)]
#[path = "../src/alloc_core/config/policy/large_thp_runtime/thp_advice_outcome.rs"]
mod thp_advice_outcome;
#[path = "../src/alloc_core/config/policy/large_thp_runtime/thp_budget_ledger.rs"]
mod thp_budget_ledger;
#[allow(dead_code)]
#[path = "../src/alloc_core/config/policy/large_thp_runtime/thp_tuple.rs"]
mod thp_tuple;

// The production sources resolve `crate::alloc_core::config::policy::…`
// paths; re-export the root-mounted real files under exactly those paths.
mod alloc_core {
    pub(crate) mod config {
        pub(crate) mod policy {
            pub(crate) mod large_thp {
                pub(crate) use crate::large_thp_curve::LargeThpCurve;
                pub(crate) use crate::large_thp_gradient::LargeThpGradient;
                pub(crate) use crate::large_thp_mode::LargeThpMode;
                pub(crate) use crate::large_thp_request_group::LargeThpRequestGroup;
                pub(crate) use crate::large_thp_share::LargeThpShare;
            }
            pub(crate) mod large_thp_runtime {
                pub(crate) mod thp_advice {
                    pub(crate) use crate::thp_advice::ThpAdvice;
                }
                pub(crate) mod thp_advice_outcome {
                    pub(crate) use crate::thp_advice_outcome::ThpAdviceOutcome;
                }
                pub(crate) use crate::thp_tuple::ThpTuple;
            }
        }
    }
}

use alloc_core::config::policy::large_thp_runtime::thp_advice::ThpAdvice;
use thp_budget_ledger::ThpBudgetLedger;

const BUDGET: usize = 8 * 1024 * 1024;
const WANT: usize = 6 * 1024 * 1024;

#[test]
fn racing_reserves_never_overshoot_budget() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ledger = Arc::new(ThpBudgetLedger::new());

        let first_ledger = Arc::clone(&ledger);
        let first = thread::spawn(move || first_ledger.try_reserve(WANT, BUDGET));
        let second_ledger = Arc::clone(&ledger);
        let second = thread::spawn(move || second_ledger.try_reserve(WANT, BUDGET));

        let first_grant = first.join().unwrap();
        let second_grant = second.join().unwrap();

        // Every non-zero grant is a granule-multiple of advised bytes.
        assert!(first_grant % ThpAdvice::GRANULE == 0 && second_grant % ThpAdvice::GRANULE == 0);
        assert!(first_grant <= WANT && second_grant <= WANT);
        // Conservation: racing 6 MiB wants against an 8 MiB budget can never
        // hand out more than the budget (an unconditional fetch_add would).
        assert!(first_grant + second_grant <= BUDGET);
        assert_eq!(ledger.used(), first_grant + second_grant);
        ledger.give_back(first_grant);
        assert_eq!(ledger.used(), second_grant);
        ledger.give_back(second_grant);
        assert_eq!(ledger.used(), 0, "every granted byte is given back");
    });
}

#[test]
fn reserve_sequence_pins_exact_values() {
    loom::model(|| {
        let ledger = ThpBudgetLedger::new();
        // Budget 0 means unlimited: never charged, always 0 here.
        assert_eq!(ledger.try_reserve(WANT, 0), 0);
        assert_eq!(ledger.used(), 0);

        assert_eq!(ledger.try_reserve(WANT, BUDGET), WANT);
        assert_eq!(ledger.used(), WANT);
        // The 2 MiB remainder is granted as the granule floor.
        assert_eq!(ledger.try_reserve(WANT, BUDGET), 2 * 1024 * 1024);
        assert_eq!(ledger.used(), BUDGET);
        // The budget is exhausted: no advice, no charge.
        assert_eq!(ledger.try_reserve(WANT, BUDGET), 0);
        assert_eq!(ledger.used(), BUDGET);

        ledger.give_back(2 * 1024 * 1024);
        assert_eq!(ledger.used(), WANT);
        ledger.give_back(WANT);
        assert_eq!(ledger.used(), 0);
        // Returning zero is a no-op.
        ledger.give_back(0);
        assert_eq!(ledger.used(), 0);
    });
}
