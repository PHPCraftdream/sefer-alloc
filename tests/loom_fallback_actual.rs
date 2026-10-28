//! G6d (`docs/GHOST_CHECKS_PLAN.md`, P19): actual-source loom model of the
//! fallback heap's one-time init and its lock (`src/global/fallback.rs`).
//! The real file takes its sync primitives from `super`; this root binds them
//! to loom and stands in for `HeapCore` with a loom cell every closure
//! writes, so a heap access not ordered by the READY publication or by the
//! lock is a loom causality violation.
//!
//! The production state lives in `static`s while loom objects live for one
//! execution, so each atomic stand-in holds the loom atomic of the current
//! execution and is re-created by `epoch()` at the start of every execution,
//! on the main thread, before any racing access (a warm-up execution
//! registers every static first). The statics are process-wide, so every
//! scenario runs inside the one test function below, sequentially.
//!
//! Proven on the production code: racing first callers run exactly one
//! initialiser (ghost `fallback/init-won` +1 per winner) and both reach the
//! one published heap; the lock serialises every closure and is released once
//! per acquire (ghost `fallback/held` returns to zero); `try_with_heap` never
//! reaches a heap that is not READY.
#![cfg(loom)]
#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize as StdUsize, Ordering as StdOrdering};
use std::sync::Mutex;

pub(crate) use loom::sync::atomic::Ordering;

/// Current loom execution, bumped by `epoch()`.
static EPOCH: StdUsize = StdUsize::new(0);
/// Re-creation hooks of every stand-in touched so far.
type Hook = Box<dyn Fn() + Send>;
static REGISTRY: Mutex<Vec<Hook>> = Mutex::new(Vec::new());

/// Start of an execution: re-create every registered loom atomic here.
fn epoch() {
    EPOCH.fetch_add(1, StdOrdering::SeqCst);
    for hook in REGISTRY.lock().unwrap().iter() {
        hook();
    }
}

macro_rules! lazy_atomic {
    ($name:ident, $loom:ty, $value:ty) => {
        pub(crate) struct $name {
            init: $value,
            cell: Mutex<Option<(usize, &'static $loom)>>,
        }

        impl $name {
            pub(crate) const fn new(init: $value) -> Self {
                Self {
                    init,
                    cell: Mutex::new(None),
                }
            }
            /// This execution's loom atomic. Objects of earlier executions are
            /// leaked, never dropped outside the execution that made them.
            fn a(&'static self) -> &'static $loom {
                let epoch = EPOCH.load(StdOrdering::SeqCst);
                let mut cell = self.cell.lock().unwrap();
                if let Some((e, a)) = *cell {
                    if e == epoch {
                        return a;
                    }
                }
                let first = cell.is_none();
                let fresh: &'static $loom = Box::leak(Box::new(<$loom>::new(self.init)));
                *cell = Some((epoch, fresh));
                drop(cell);
                if first {
                    REGISTRY.lock().unwrap().push(Box::new(move || {
                        let _ = self.a();
                    }));
                }
                fresh
            }
            pub(crate) fn load(&'static self, order: Ordering) -> $value {
                self.a().load(order)
            }
            pub(crate) fn store(&'static self, value: $value, order: Ordering) {
                self.a().store(value, order)
            }
            pub(crate) fn compare_exchange(
                &'static self,
                current: $value,
                new: $value,
                success: Ordering,
                failure: Ordering,
            ) -> Result<$value, $value> {
                self.a().compare_exchange(current, new, success, failure)
            }
        }
    };
}

lazy_atomic!(AtomicU8, loom::sync::atomic::AtomicU8, u8);
lazy_atomic!(AtomicBool, loom::sync::atomic::AtomicBool, bool);
lazy_atomic!(AtomicU64, loom::sync::atomic::AtomicU64, u64);

impl AtomicU64 {
    pub(crate) fn fetch_add(&'static self, value: u64, order: Ordering) -> u64 {
        self.a().fetch_add(value, order)
    }
}

pub(crate) use loom::hint::spin_loop;
pub(crate) use loom::thread::yield_now;

#[allow(dead_code, unused_imports)]
#[path = "../src/ghost/mod.rs"]
mod ghost;

/// The model never exits.
pub fn process_exit_in_progress() -> bool {
    false
}

mod alloc_core {
    pub(crate) mod segment_header {
        pub const OWNER_ID_FALLBACK: u32 = 0x7FFF_FFFE;
    }
}

/// `HeapCore::new` calls (harness bookkeeping, std atomics).
static INITS: StdUsize = StdUsize::new(0);

mod registry {
    /// Stand-in heap: one loom cell every closure writes.
    pub struct HeapCore {
        cell: loom::cell::UnsafeCell<u32>,
    }

    impl HeapCore {
        pub fn new(owner: u32) -> Option<Self> {
            assert_eq!(owner, crate::alloc_core::segment_header::OWNER_ID_FALLBACK);
            crate::INITS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Some(Self {
                cell: loom::cell::UnsafeCell::new(0),
            })
        }

        /// One tracked write; returns how many closures touched this heap.
        pub fn touch(&mut self) -> u32 {
            // SAFETY: loom tracks the access; its exclusivity is the property
            // under test.
            self.cell.with_mut(|p| unsafe {
                *p += 1;
                *p
            })
        }
    }
}

#[path = "../src/global/fallback.rs"]
mod fallback;

use loom::thread;

fn bounded(f: impl Fn() + Sync + Send + 'static) {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.max_branches = 100_000;
    builder.check(f);
}

struct Ledger {
    held: i64,
    won: i64,
    inits: usize,
}

impl Ledger {
    fn now() -> Self {
        Self {
            held: ghost::count("fallback/held", 0),
            won: ghost::count("fallback/init-won", 0),
            inits: INITS.load(StdOrdering::SeqCst),
        }
    }
    fn since(&self) -> (i64, i64, usize) {
        let now = Self::now();
        (
            now.held - self.held,
            now.won - self.won,
            now.inits - self.inits,
        )
    }
}

/// Racing first callers: one initialiser, one heap, serialised closures.
fn racing_first_callers() {
    bounded(|| {
        epoch();
        let start = Ledger::now();
        let other = thread::spawn(|| fallback::with_heap(|heap| heap.touch()));
        let mine = fallback::with_heap(|heap| heap.touch());
        let theirs = other.join().unwrap();
        let mut seen = [mine.expect("heap"), theirs.expect("heap")];
        seen.sort_unstable();
        assert_eq!(
            seen,
            [1, 2],
            "both closures ran on the one heap, one after the other"
        );
        assert_eq!(
            start.since(),
            (0, 1, 1),
            "one initialiser, every acquire released"
        );
    });
}

/// `try_with_heap` racing the initialiser sees no heap or the published one.
fn try_racing_init() {
    bounded(|| {
        epoch();
        let start = Ledger::now();
        let init = thread::spawn(|| fallback::with_heap(|heap| heap.touch()));
        let tried = fallback::try_with_heap(|heap| heap.touch());
        let first = init.join().unwrap().expect("heap");
        match tried {
            None => assert_eq!(first, 1),
            Some(n) => {
                let mut seen = [n, first];
                seen.sort_unstable();
                assert_eq!(seen, [1, 2], "a READY try_with_heap shares the one heap");
            }
        }
        let (held, won, inits) = start.since();
        assert_eq!(
            (held, won, inits),
            (0, 1, 1),
            "one initialiser, every acquire released"
        );
    });
}

#[test]
fn fallback_init_and_lock_on_production_source() {
    // Warm-up: registers every static stand-in.
    loom::model(|| {
        epoch();
        let _ = fallback::try_with_heap(|heap| heap.touch());
        let _ = fallback::with_heap(|heap| heap.touch());
        let _ = fallback::lock_acquisitions();
        fallback::record_exit_lock_giveup();
    });
    racing_first_callers();
    try_racing_init();
}
