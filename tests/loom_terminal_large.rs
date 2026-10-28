//! Actual-source loom model of the Large phase word: the production
//! `LargeReservationState` (`src/alloc_core/large/reservation_state.rs`) and
//! the production phase codec (`src/alloc_core/segment/segment_header/
//! terminal_words.rs`) compiled onto loom atomics. Replaces the earlier shadow
//! model, so a mutant in either production file reddens this test.
//! Run with `RUSTFLAGS="--cfg loom" cargo test --release --features
//! "alloc-core alloc-xthread tagged-index-stack/loom" --test loom_terminal_large`.
//!
//! # Seams (import- and declaration-level; no production statement changed)
//! - `reservation_state.rs` takes `AtomicU64`/`Ordering` from `super`; the
//!   `alloc_core::large` module below re-exports loom's.
//! - `terminal_words.rs` needs `fatal`, `Node`, `SegmentHeader`, `SegmentMeta`
//!   and `align_up_const`: compile-only shims. They back the header-word
//!   accessors (`init_*_terminal`, `large_state_atomic`), which no test calls;
//!   the codec (`pack_large_state`, `large_phase`, `large_generation`,
//!   `next_large_generation`) is the real code. `alloc-decommit` is off in
//!   this row, so the reuse helpers that bind a header word are compiled out.
//!
//! # Oracles
//! - Of two racing terminal publications of one LIVE generation exactly one
//!   wins; of the owner's racing claims exactly one consumes it.
//! - From CONSUMING, cache and release are exclusive.
//! - Ghost ledger: the production `large/pending` events (+1 per successful
//!   `publish_pending`, -1 per successful `claim_pending`) sum to zero once
//!   the word left PENDING, so every terminal publication is claimed exactly
//!   once.
#![cfg(loom)]

use loom::sync::Arc;
use loom::thread;

// Atomic seam: the included `reservation_state.rs` resolves these via `super`.
pub(crate) use loom::sync::atomic::{AtomicU64, Ordering};

// The included production file records `crate::ghost::ghost_event!`; cfg(loom)
// is a ghost build, so the events are recorded in every schedule.
#[allow(dead_code, unused_imports)]
#[path = "../src/ghost/mod.rs"]
mod ghost;

#[allow(dead_code)]
mod alloc_core {
    pub(crate) mod fatal {
        pub fn fatal(msg: &'static [u8]) -> ! {
            panic!("fatal: {}", String::from_utf8_lossy(msg))
        }
    }

    // Compile-only: backs the header-word accessors of `terminal_words.rs`,
    // which no test calls.
    pub(crate) mod node {
        pub(crate) struct Node;
        impl Node {
            pub(crate) fn init_atomic_u32_at(_base: *mut u8, _off: usize, _value: u32) {
                unreachable!("compile-only shim")
            }
            pub(crate) fn init_atomic_u64_at(_base: *mut u8, _off: usize, _value: u64) {
                unreachable!("compile-only shim")
            }
            pub(crate) fn atomic_u64_at<'a>(
                _base: *mut u8,
                _off: usize,
            ) -> &'a core::sync::atomic::AtomicU64 {
                unreachable!("compile-only shim")
            }
        }
    }

    pub(crate) mod segment_header {
        pub(crate) struct SegmentHeader {
            _word: u64,
        }
        pub(crate) struct SegmentMeta {
            pub base: *mut u8,
        }
        pub(crate) const fn align_up_const(n: usize, a: usize) -> usize {
            (n + a - 1) & !(a - 1)
        }
        #[path = "../../../src/alloc_core/segment/segment_header/terminal_words.rs"]
        mod terminal_words;
        pub(crate) use terminal_words::{
            large_generation, large_phase, next_large_generation, pack_large_state, LargePhase,
        };
    }

    pub(crate) mod large {
        pub(crate) use crate::{AtomicU64, Ordering};
        #[path = "../../../src/alloc_core/large/reservation_state.rs"]
        pub(crate) mod reservation_state;
    }
}

use alloc_core::large::reservation_state::LargeReservationState;
use alloc_core::segment_header::{pack_large_state, LargePhase};

const GENERATION: u64 = 11;

fn word(phase: LargePhase) -> u64 {
    pack_large_state(phase, GENERATION)
}

/// This execution's `large/pending` ledger for `state` (the ghost table is
/// process global, so the harness reads deltas from a per-execution snapshot).
fn ledger(state: &AtomicU64) -> i64 {
    ghost::count("large/pending", core::ptr::from_ref(state).addr() as u64)
}

#[test]
fn producer_owner_cut_and_duplicate_admission() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let state = Arc::new(AtomicU64::new(word(LargePhase::Live)));
        let at_start = ledger(&state);
        let first = Arc::clone(&state);
        let a =
            thread::spawn(move || LargeReservationState::new(&first).publish_pending(GENERATION));
        let second = Arc::clone(&state);
        let b =
            thread::spawn(move || LargeReservationState::new(&second).publish_pending(GENERATION));
        let owner = Arc::clone(&state);
        let c = thread::spawn(move || LargeReservationState::new(&owner).claim_pending());

        let admitted = usize::from(a.join().unwrap()) + usize::from(b.join().unwrap());
        let first_claim = c.join().unwrap();
        let late_claim = LargeReservationState::new(&state).claim_pending();
        assert_eq!(admitted, 1, "exactly one terminal publication wins");
        assert_eq!(
            usize::from(first_claim.is_some()) + usize::from(late_claim.is_some()),
            1,
            "exactly one owner claim consumes it"
        );
        assert_eq!(first_claim.or(late_claim), Some(GENERATION));
        assert_eq!(state.load(Ordering::Acquire), word(LargePhase::Consuming));
        assert_eq!(
            ledger(&state) - at_start,
            0,
            "every terminal publication is claimed exactly once"
        );
    });
}

#[test]
fn owner_cache_or_release_is_exclusive() {
    loom::model(|| {
        let state = Arc::new(AtomicU64::new(word(LargePhase::Live)));
        let at_start = ledger(&state);
        assert!(LargeReservationState::new(&state).publish_pending(GENERATION));
        let owner = Arc::clone(&state);
        let a = thread::spawn(move || {
            let state = LargeReservationState::new(&owner);
            state
                .claim_pending()
                .is_some_and(|generation| state.cache_consumed(generation))
        });
        let duplicate = Arc::clone(&state);
        let b = thread::spawn(move || {
            let state = LargeReservationState::new(&duplicate);
            state
                .claim_pending()
                .is_some_and(|generation| state.release_consumed(generation))
        });
        assert_ne!(a.join().unwrap(), b.join().unwrap());
        let last = state.load(Ordering::Acquire);
        assert!(last == word(LargePhase::Cached) || last == word(LargePhase::Released));
        assert_eq!(
            ledger(&state) - at_start,
            0,
            "the publication is claimed exactly once"
        );
    });
}
