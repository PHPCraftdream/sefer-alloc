//! G6d (`docs/GHOST_CHECKS_PLAN.md`, P21): actual-source loom model of the
//! exact-object shard (`src/global/exact_object/exact_shard.rs`, opt-in
//! `exact-object-proto`). The real file takes its sync primitives from
//! `super`; this root binds them to loom:
//! - `AtomicBool` / `AtomicUsize`: const-constructible stand-ins (the shard is
//!   built by `const fn new`) whose loom atomic is created on first use;
//! - `UnsafeCell`: the production API (`get() -> *mut T`) over the real value,
//!   recording a loom mutable access at every `get()`, so two critical
//!   sections without a happens-before edge between them are a loom
//!   causality violation;
//! - `spin_loop` / `yield_now`: loom's, so a waiter lets the holder run.
//!
//! Each test touches every primitive on the main thread first (an insert),
//! so their creation happens-before every racing access.
//!
//! Proven on the production code: the shard lock (CAS-weak Acquire, Release
//! store) orders every critical section, racing insert / take keep the
//! table exact, and the ghost ledger `exact-shard/held` (+1 per acquire, -1
//! per release) returns to zero: every acquire is released exactly once.
#![cfg(loom)]
#![allow(dead_code)]

use std::sync::OnceLock;

pub(crate) use loom::sync::atomic::Ordering;

pub(crate) struct AtomicBool {
    init: bool,
    cell: OnceLock<loom::sync::atomic::AtomicBool>,
}

impl AtomicBool {
    pub(crate) const fn new(init: bool) -> Self {
        Self {
            init,
            cell: OnceLock::new(),
        }
    }
    fn a(&self) -> &loom::sync::atomic::AtomicBool {
        self.cell
            .get_or_init(|| loom::sync::atomic::AtomicBool::new(self.init))
    }
    pub(crate) fn load(&self, order: Ordering) -> bool {
        self.a().load(order)
    }
    pub(crate) fn store(&self, value: bool, order: Ordering) {
        self.a().store(value, order)
    }
    pub(crate) fn compare_exchange_weak(
        &self,
        current: bool,
        new: bool,
        success: Ordering,
        failure: Ordering,
    ) -> Result<bool, bool> {
        self.a()
            .compare_exchange_weak(current, new, success, failure)
    }
}

pub(crate) struct AtomicUsize {
    init: usize,
    cell: OnceLock<loom::sync::atomic::AtomicUsize>,
}

impl AtomicUsize {
    pub(crate) const fn new(init: usize) -> Self {
        Self {
            init,
            cell: OnceLock::new(),
        }
    }
    fn a(&self) -> &loom::sync::atomic::AtomicUsize {
        self.cell
            .get_or_init(|| loom::sync::atomic::AtomicUsize::new(self.init))
    }
    pub(crate) fn load(&self, order: Ordering) -> usize {
        self.a().load(order)
    }
    pub(crate) fn store(&self, value: usize, order: Ordering) {
        self.a().store(value, order)
    }
}

/// The production `UnsafeCell` API with loom access tracking: `get()` records
/// one mutable access on a loom cell before handing out the raw pointer.
pub(crate) struct UnsafeCell<T> {
    value: core::cell::UnsafeCell<T>,
    tracker: OnceLock<loom::cell::UnsafeCell<()>>,
}

impl<T> UnsafeCell<T> {
    pub(crate) const fn new(value: T) -> Self {
        Self {
            value: core::cell::UnsafeCell::new(value),
            tracker: OnceLock::new(),
        }
    }
    pub(crate) fn get(&self) -> *mut T {
        self.tracker
            .get_or_init(|| loom::cell::UnsafeCell::new(()))
            .with_mut(|_| ());
        self.value.get()
    }
}

pub(crate) use loom::hint::spin_loop;
pub(crate) use loom::thread::yield_now;

#[allow(dead_code, unused_imports)]
#[path = "../src/ghost/mod.rs"]
mod ghost;

mod alloc_core {
    pub(crate) mod fatal {
        pub fn fatal(msg: &'static [u8]) -> ! {
            panic!("fatal: {}", String::from_utf8_lossy(msg))
        }
    }
}

mod global {
    /// The model never exits.
    pub fn process_exit_in_progress() -> bool {
        false
    }
    pub(crate) mod fallback {
        pub fn record_exit_lock_giveup() {}
    }
}

// Mounted at the root: `exact_shard.rs` resolves `super::*` here.
#[path = "../src/global/exact_object/exact_shard.rs"]
mod exact_shard;
#[path = "../src/global/exact_object/insert_outcome.rs"]
mod insert_outcome;

use exact_shard::ExactShard;
use insert_outcome::InsertOutcome;
use loom::sync::Arc;
use loom::thread;

/// Bounded exploration: the shard lock spins (64 `spin_loop`s, then
/// `yield_now`), so unbounded preemption explodes the branch count.
fn bounded(f: impl Fn() + Sync + Send + 'static) {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.max_branches = 100_000;
    builder.check(f);
}

const A: usize = 0x1000;
const B: usize = 0x2000;

/// This execution's `exact-shard/held` ledger for `shard` (keyed by the
/// shard's address; the ghost table is process global, so read deltas).
fn ledger(shard: &ExactShard) -> i64 {
    ghost::count("exact-shard/held", core::ptr::from_ref(shard).addr() as u64)
}

#[test]
fn racing_insert_and_take_keep_the_shard_exact() {
    bounded(|| {
        let shard = Arc::new(ExactShard::new());
        assert!(matches!(shard.insert(A, 16, 8, 1), InsertOutcome::Inserted));
        let at_start = ledger(&shard);
        let inserter = {
            let shard = Arc::clone(&shard);
            thread::spawn(move || shard.insert(B, 32, 8, 2))
        };
        assert_eq!(shard.take(A), Some((16, 8, 1)), "A is taken once");
        assert!(matches!(inserter.join().unwrap(), InsertOutcome::Inserted));
        assert_eq!(shard.take(A), None, "A is gone");
        assert_eq!(shard.peek(B), Some((32, 8, 2)), "B is fully inserted");
        assert_eq!(
            ledger(&shard) - at_start,
            0,
            "every shard lock acquire is released exactly once"
        );
    });
}

#[test]
fn racing_inserts_of_one_address_admit_one() {
    bounded(|| {
        let shard = Arc::new(ExactShard::new());
        assert!(matches!(shard.insert(B, 8, 8, 1), InsertOutcome::Inserted));
        let at_start = ledger(&shard);
        let other = {
            let shard = Arc::clone(&shard);
            thread::spawn(move || shard.insert(A, 16, 8, 2))
        };
        let mine = shard.insert(A, 16, 8, 3);
        let outcomes = [other.join().unwrap(), mine];
        let inserted = outcomes
            .iter()
            .filter(|o| matches!(o, InsertOutcome::Inserted))
            .count();
        let duplicate = outcomes
            .iter()
            .filter(|o| matches!(o, InsertOutcome::Duplicate))
            .count();
        assert_eq!(
            (inserted, duplicate),
            (1, 1),
            "one insert wins, one sees it"
        );
        assert!(matches!(shard.take(A), Some((16, 8, 2 | 3))));
        assert_eq!(shard.take(A), None);
        assert_eq!(
            ledger(&shard) - at_start,
            0,
            "every shard lock acquire is released exactly once"
        );
    });
}
