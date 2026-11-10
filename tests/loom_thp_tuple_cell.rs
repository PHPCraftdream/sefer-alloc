//! Actual-source model of the write-once large-THP tuple cell: the production
//! `ThpTupleCell` (`src/alloc_core/config/policy/large_thp_runtime/
//! thp_tuple_cell.rs`) compiled onto loom atomics, together with the REAL
//! pure codec and gradient files (`thp_tuple.rs`, `large_thp_*.rs`) — no
//! stubbed logic anywhere.
//! Run with `RUSTFLAGS="--cfg loom" cargo test --release --features
//! "alloc-core alloc-xthread tagged-index-stack/loom" --test loom_thp_tuple_cell`.
//!
//! # Seams (import- and declaration-level; no production statement changed)
//! - `thp_tuple_cell.rs` takes `AtomicU64`/`AtomicU8`/`Ordering` from `super`,
//!   the crate root here, which re-exports loom's.
//! - Included production files resolve their `crate::` paths through a nested
//!   `mod alloc_core` tree that re-exports the root-mounted real files under
//!   the exact module paths the sources use.
//! - No included file carries a feature gate (the `mod.rs` gating does not
//!   apply to `#[path]` includes), so this model needs no `alloc-decommit`.
//!
//! # Oracles
//! - A reader never observes a torn tuple: its result is exactly one of the
//!   published candidates or `ThpTuple::DEFAULT` (no third value ever).
//! - Of two racing publishers of distinct tuples exactly one is admitted; the
//!   final `read()` equals the admitted winner's tuple exactly.
//! - A loser that meets `PUBLISHING` returns immediately — the model
//!   finishing under every interleaving is the no-spin evidence.
#![cfg(loom)]

use loom::sync::Arc;
use loom::thread;

// Atomic seam: the included `thp_tuple_cell.rs` resolves these via `super`.
pub(crate) use loom::sync::atomic::{AtomicU64, AtomicU8, Ordering};

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
#[path = "../src/alloc_core/config/policy/large_thp/large_thp_share.rs"]
mod large_thp_share;
#[allow(dead_code)]
#[path = "../src/alloc_core/config/policy/large_thp_runtime/thp_tuple.rs"]
mod thp_tuple;
#[path = "../src/alloc_core/config/policy/large_thp_runtime/thp_tuple_cell.rs"]
mod thp_tuple_cell;

// The production sources resolve `crate::alloc_core::config::policy::…`
// paths; re-export the root-mounted real files under exactly those paths.
mod alloc_core {
    pub(crate) mod config {
        pub(crate) mod policy {
            pub(crate) mod large_thp {
                pub(crate) use crate::large_thp_curve::LargeThpCurve;
                pub(crate) use crate::large_thp_gradient::LargeThpGradient;
                pub(crate) use crate::large_thp_mode::LargeThpMode;
            }
            pub(crate) mod large_thp_runtime {
                pub(crate) use crate::thp_tuple::ThpTuple;
            }
        }
    }
}

use alloc_core::config::policy::large_thp::{LargeThpGradient, LargeThpMode};
use alloc_core::config::policy::large_thp_runtime::ThpTuple;
use thp_tuple_cell::ThpTupleCell;

fn read_with_publication_witness(cell: &ThpTupleCell) -> ThpTuple {
    // A Relaxed witness adds no word synchronization. After observing state 2,
    // a subsequent load of the same atomic cannot observe an earlier state.
    let published = cell.state.load(Ordering::Relaxed) == 2;
    let observed = cell.read();
    if published {
        assert_ne!(
            observed,
            ThpTuple::DEFAULT,
            "published non-default candidate must not decode to the fallback"
        );
    }
    observed
}

// Two non-default candidates differ in minimum, budget and EMA rates;
// shared packed words are still nonzero, distinguishing stale initialization.
fn tuple_a() -> ThpTuple {
    ThpTuple {
        mode: LargeThpMode::Always,
        min_bytes: 16 * 1024 * 1024,
        budget_bytes: 4 * 1024 * 1024,
        gradient: LargeThpGradient::CAUTIOUS,
    }
}

fn tuple_b() -> ThpTuple {
    ThpTuple {
        mode: LargeThpMode::Always,
        min_bytes: 32 * 1024 * 1024,
        budget_bytes: 8 * 1024 * 1024,
        gradient: LargeThpGradient::EAGER,
    }
}

#[test]
fn two_publishers_distinct_tuples_reader_never_torn() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let cell = Arc::new(ThpTupleCell::new());
        let a_tuple = tuple_a();
        let b_tuple = tuple_b();

        let a_cell = Arc::clone(&cell);
        let a = thread::spawn(move || a_cell.publish(&a_tuple));
        let b_cell = Arc::clone(&cell);
        let b = thread::spawn(move || b_cell.publish(&b_tuple));
        let r_cell = Arc::clone(&cell);
        let reader = thread::spawn(move || read_with_publication_witness(&r_cell));

        let admitted_a = a.join().unwrap();
        let admitted_b = b.join().unwrap();
        let observed = reader.join().unwrap();
        let final_read = cell.read();

        // Never torn: the reader sees exactly one candidate or the default.
        assert!(
            observed == tuple_a() || observed == tuple_b() || observed == ThpTuple::DEFAULT,
            "reader observed a value that is neither candidate nor default"
        );
        // Exactly one distinct-tuple publication is admitted.
        assert_eq!(
            usize::from(admitted_a) + usize::from(admitted_b),
            1,
            "exactly one of two distinct-tuple publishers wins"
        );
        // The final read is the admitted winner's tuple, exactly.
        let winner = if admitted_a { tuple_a() } else { tuple_b() };
        assert_eq!(final_read, winner, "cell holds exactly the winner's tuple");
    });
}

#[test]
fn two_publishers_same_tuple_overcount_contract() {
    loom::model(|| {
        let cell = Arc::new(ThpTupleCell::new());
        let a_tuple = tuple_a();

        let first_cell = Arc::clone(&cell);
        let first = thread::spawn(move || first_cell.publish(&a_tuple));
        let second_cell = Arc::clone(&cell);
        let second = thread::spawn(move || second_cell.publish(&a_tuple));
        let r_cell = Arc::clone(&cell);
        let reader = thread::spawn(move || read_with_publication_witness(&r_cell));

        let first_ok = first.join().unwrap();
        let second_ok = second.join().unwrap();
        let observed = reader.join().unwrap();

        // Write-once: the cell ends with exactly A.
        assert_eq!(cell.read(), a_tuple);
        // Conflict accounting: the loser's `false` is the (over-counted, in
        // the same-tuple race) conflict; 0 or 1 of the two calls reports it.
        let conflicts = usize::from(!first_ok) + usize::from(!second_ok);
        assert!(conflicts <= 1, "at most one same-tuple publisher conflicts");
        // Reader is never torn: A or DEFAULT, nothing else.
        assert!(
            observed == a_tuple || observed == ThpTuple::DEFAULT,
            "reader observed a value that is neither A nor default"
        );
    });
}

#[test]
fn reader_during_publication_sees_default_or_winner() {
    loom::model(|| {
        let cell = Arc::new(ThpTupleCell::new());
        let a_tuple = tuple_a();

        let p_cell = Arc::clone(&cell);
        let publisher = thread::spawn(move || p_cell.publish(&a_tuple));
        let r_cell = Arc::clone(&cell);
        let reader = thread::spawn(move || read_with_publication_witness(&r_cell));

        let admitted = publisher.join().unwrap();
        let observed = reader.join().unwrap();

        // Exactly two legal observations: the winner or the default. A reader
        // landing during PUBLISHING sees DEFAULT (one Acquire load, no spin).
        assert!(
            observed == a_tuple || observed == ThpTuple::DEFAULT,
            "reader observed a third value during publication"
        );
        assert!(admitted, "the single publisher is always admitted");
        assert_eq!(cell.read(), a_tuple, "after join the cell holds exactly A");
    });
}
