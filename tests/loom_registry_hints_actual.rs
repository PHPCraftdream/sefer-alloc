//! G5 (`docs/GHOST_CHECKS_PLAN.md`, P4): actual-source loom test of the two
//! self-contained registry hints. The REAL `free_slot_hint.rs` and
//! `saturation.rs` are compiled here as modules; they take `AtomicU64` /
//! `Ordering` from `super`, which this root binds to loom. Reduced geometry:
//! `MAX_HEAPS = 128` (two free-set words).
//!
//! Proven on the production code:
//! - a free-set publication racing the taker's summary clear is never lost and
//!   never handed out twice (`clear_summary_recheck` re-publishes, I3);
//! - a saturation verdict is committed only from a snapshot taken after every
//!   completed `publish_claimable` (the CAS in `try_mark_saturated`, I4).
#![cfg(loom)]
#![allow(dead_code)]

use std::sync::OnceLock;

pub(crate) use loom::sync::atomic::Ordering;

/// `MAX_HEAPS` of the included `free_slot_hint.rs`.
pub(crate) const MAX_HEAPS: usize = 128;

/// Const-constructible stand-in for `AtomicU64`: the production hints are
/// built by `const fn new`, which loom atomics cannot be; the loom atomic is
/// created on first use, inside the model execution that owns the value. Each
/// test touches every atomic once on the main thread before spawning, so the
/// creation happens-before every racing access.
pub(crate) struct AtomicU64 {
    init: u64,
    cell: OnceLock<loom::sync::atomic::AtomicU64>,
}

impl AtomicU64 {
    pub(crate) const fn new(init: u64) -> Self {
        Self {
            init,
            cell: OnceLock::new(),
        }
    }
    fn a(&self) -> &loom::sync::atomic::AtomicU64 {
        self.cell
            .get_or_init(|| loom::sync::atomic::AtomicU64::new(self.init))
    }
    pub(crate) fn load(&self, order: Ordering) -> u64 {
        self.a().load(order)
    }
    pub(crate) fn store(&self, value: u64, order: Ordering) {
        self.a().store(value, order)
    }
    pub(crate) fn fetch_or(&self, value: u64, order: Ordering) -> u64 {
        self.a().fetch_or(value, order)
    }
    pub(crate) fn fetch_and(&self, value: u64, order: Ordering) -> u64 {
        self.a().fetch_and(value, order)
    }
    pub(crate) fn compare_exchange(
        &self,
        current: u64,
        new: u64,
        success: Ordering,
        failure: Ordering,
    ) -> Result<u64, u64> {
        self.a().compare_exchange(current, new, success, failure)
    }
    pub(crate) fn fetch_update<F: FnMut(u64) -> Option<u64>>(
        &self,
        set: Ordering,
        fetch: Ordering,
        f: F,
    ) -> Result<u64, u64> {
        self.a().fetch_update(set, fetch, f)
    }
}

#[path = "../src/registry/bootstrap/free_slot_hint.rs"]
mod free_slot_hint;
#[path = "../src/registry/bootstrap/saturation.rs"]
mod saturation;

use free_slot_hint::FreeSlotHint;
use loom::sync::Arc;
use loom::thread;
use saturation::SaturationHint;

/// Drain every index still reachable through the summary.
fn drain(hint: &FreeSlotHint) -> Vec<usize> {
    let mut out = Vec::new();
    while let Some(idx) = hint.take_nonempty() {
        out.push(idx);
    }
    out
}

#[test]
fn publication_racing_summary_clear_is_never_lost_or_duplicated() {
    loom::model(|| {
        let hint = Arc::new(FreeSlotHint::new());
        hint.publish(0);
        hint.publish(64);
        hint.forget(64);
        let taker = {
            let h = hint.clone();
            thread::spawn(move || h.take_nonempty())
        };
        let publisher = {
            let h = hint.clone();
            thread::spawn(move || h.publish(1))
        };
        let taken = taker.join().unwrap();
        publisher.join().unwrap();
        let mut all: Vec<usize> = taken.into_iter().chain(drain(&hint)).collect();
        all.sort_unstable();
        assert_eq!(all, vec![0, 1], "every publication is taken exactly once");
    });
}

#[test]
fn publications_in_two_words_both_stay_reachable() {
    loom::model(|| {
        let hint = Arc::new(FreeSlotHint::new());
        hint.publish(0);
        hint.forget(0);
        hint.publish(64);
        let taker = {
            let h = hint.clone();
            thread::spawn(move || h.take_nonempty())
        };
        let publisher = {
            let h = hint.clone();
            thread::spawn(move || {
                h.publish(3);
                h.publish(65);
            })
        };
        let taken = taker.join().unwrap();
        publisher.join().unwrap();
        let mut all: Vec<usize> = taken.into_iter().chain(drain(&hint)).collect();
        all.sort_unstable();
        assert_eq!(
            all,
            vec![3, 64, 65],
            "every publication is taken exactly once"
        );
    });
}

#[test]
fn saturation_is_committed_only_after_every_completed_publication() {
    loom::model(|| {
        let hint = Arc::new(SaturationHint::new());
        assert_eq!(hint.snapshot(), 0);
        let scanner = {
            let h = hint.clone();
            thread::spawn(move || {
                let snapshot = h.snapshot();
                // The scan found nothing claimable.
                h.try_mark_saturated(snapshot);
                snapshot
            })
        };
        let publisher = {
            let h = hint.clone();
            thread::spawn(move || h.publish_claimable())
        };
        let snapshot = scanner.join().unwrap();
        publisher.join().unwrap();
        if hint.is_saturated() {
            assert!(
                snapshot >= 2,
                "saturated from snapshot {snapshot} taken before the completed publication"
            );
        }
    });
}
