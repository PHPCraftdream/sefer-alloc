//! Atomic-order model of the Large phase word, without reservation payloads.
//! Run with `RUSTFLAGS="--cfg loom" cargo test --features "alloc-core alloc-xthread" --test loom_terminal_large`.

#![cfg(loom)]

use loom::sync::atomic::{AtomicU64, Ordering};
use loom::sync::Arc;
use loom::thread;

const LIVE: u64 = 2;
const PENDING: u64 = 3;
const CONSUMING: u64 = 4;
const CACHED: u64 = 5;
const RELEASED: u64 = 6;
const GENERATION: u64 = 11;

fn word(phase: u64) -> u64 {
    (GENERATION << 3) | phase
}

fn publish(state: &AtomicU64) -> bool {
    state
        .compare_exchange(
            word(LIVE),
            word(PENDING),
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_ok()
}

fn claim(state: &AtomicU64) -> bool {
    let seen = state.load(Ordering::Acquire);
    seen == word(PENDING)
        && state
            .compare_exchange(seen, word(CONSUMING), Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
}

#[test]
fn producer_owner_cut_and_duplicate_admission() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let state = Arc::new(AtomicU64::new(word(LIVE)));
        let first = Arc::clone(&state);
        let a = thread::spawn(move || publish(&first));
        let second = Arc::clone(&state);
        let b = thread::spawn(move || publish(&second));
        let owner = Arc::clone(&state);
        let c = thread::spawn(move || claim(&owner));

        let admitted = usize::from(a.join().unwrap()) + usize::from(b.join().unwrap());
        let first_claim = usize::from(c.join().unwrap());
        let late_claim = usize::from(claim(&state));
        assert_eq!(admitted, 1);
        assert_eq!(first_claim + late_claim, 1);
        assert_eq!(state.load(Ordering::Acquire), word(CONSUMING));
    });
}

#[test]
fn owner_cache_or_release_is_exclusive() {
    loom::model(|| {
        let state = Arc::new(AtomicU64::new(word(PENDING)));
        let owner = Arc::clone(&state);
        let a = thread::spawn(move || {
            if claim(&owner) {
                owner
                    .compare_exchange(
                        word(CONSUMING),
                        word(CACHED),
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
            } else {
                false
            }
        });
        let duplicate = Arc::clone(&state);
        let b = thread::spawn(move || {
            if claim(&duplicate) {
                duplicate
                    .compare_exchange(
                        word(CONSUMING),
                        word(RELEASED),
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
            } else {
                false
            }
        });
        assert_ne!(a.join().unwrap(), b.join().unwrap());
        assert!(
            matches!(state.load(Ordering::Acquire), x if x == word(CACHED) || x == word(RELEASED))
        );
    });
}
