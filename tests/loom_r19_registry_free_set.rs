//! Reduced Loom model of FREE-slot hint spill and bounded bitmap fallback.
//! Three slots, two releasers, one claimer and maintenance; bitmap dimensions
//! are abstracted. Production FREE_SLOTS remains 576-byte cache-line aligned.
//! State CAS alone grants ownership; hints/bitmap are discovery only.
#![cfg(all(loom, feature = "alloc-global", feature = "internals"))]

use loom::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use loom::sync::Arc;
use loom::thread;

const FREE: u8 = 0;
const LIVE: u8 = 1;
const MAINT: u8 = 2;
const NO_HINT: u64 = 3;
const MASK: u64 = 7;

struct Registry {
    state: [AtomicU8; 3],
    hint: AtomicU64,
    word: AtomicU64,
    summary: AtomicU64,
    owners: [AtomicU8; 3],
}

impl Registry {
    fn new() -> Self {
        Self {
            state: [
                AtomicU8::new(LIVE),
                AtomicU8::new(LIVE),
                AtomicU8::new(LIVE),
            ],
            hint: AtomicU64::new(NO_HINT),
            word: AtomicU64::new(0),
            summary: AtomicU64::new(0),
            owners: [AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0)],
        }
    }

    fn publish_word(&self, slot: usize) {
        self.word.fetch_or(1 << slot, Ordering::Release);
        self.summary.fetch_or(1, Ordering::Release);
    }

    fn release(&self, slot: usize, from: u8, store_instead_of_swap: bool) {
        assert!(self.state[slot]
            .compare_exchange(from, FREE, Ordering::Release, Ordering::Relaxed)
            .is_ok());
        if store_instead_of_swap {
            self.hint.store(slot as u64, Ordering::Relaxed);
        } else {
            let displaced = self.hint.swap(slot as u64, Ordering::Relaxed);
            if displaced < 3 {
                self.publish_word(displaced as usize);
            }
        }
    }

    fn clear_summary_recheck(&self) {
        self.summary.fetch_and(!1, Ordering::AcqRel);
        if self.word.load(Ordering::Acquire) & MASK != 0 {
            self.summary.fetch_or(1, Ordering::Release);
        }
    }

    fn take_nonempty(&self, check_old: bool) -> Option<usize> {
        for _ in 0..3 {
            if self.summary.load(Ordering::Acquire) == 0 {
                return None;
            }
            let bits = self.word.load(Ordering::Acquire) & MASK;
            if bits == 0 {
                self.clear_summary_recheck();
                continue;
            }
            let bit = 1u64 << bits.trailing_zeros();
            let old = self.word.fetch_and(!bit, Ordering::AcqRel);
            if check_old && old & bit == 0 {
                continue;
            }
            if old == bit {
                self.clear_summary_recheck();
            }
            return Some(bit.trailing_zeros() as usize);
        }
        None
    }

    fn publish_before_or_after_free_cas(&self, early: bool) {
        if early {
            self.publish_word(0);
            assert!(self.state[0]
                .compare_exchange(LIVE, FREE, Ordering::Release, Ordering::Relaxed)
                .is_ok());
        } else {
            assert!(self.state[0]
                .compare_exchange(LIVE, FREE, Ordering::Release, Ordering::Relaxed)
                .is_ok());
            self.publish_word(0);
        }
    }

    fn consume_published_candidate(&self) {
        if let Some(slot) = self.take_nonempty(true) {
            let _ =
                self.state[slot].compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire);
        }
    }

    fn claim(&self) {
        let hinted = self.hint.swap(NO_HINT, Ordering::AcqRel) as usize;
        let (candidate, from_scan) = if hinted < 3 {
            (Some(hinted), false)
        } else if self.summary.load(Ordering::Acquire) != 0 {
            match self.take_nonempty(true) {
                Some(slot) => (Some(slot), false),
                None => (self.scan_free(), true),
            }
        } else {
            (self.scan_free(), true)
        };
        if let Some(slot) = candidate {
            if self.state[slot]
                .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                assert_eq!(
                    self.owners[slot].fetch_add(1, Ordering::Relaxed),
                    0,
                    "exclusive state-CAS ownership"
                );
                if from_scan {
                    self.word.fetch_and(!(1 << slot), Ordering::AcqRel);
                }
            }
        }
    }

    fn scan_free(&self) -> Option<usize> {
        self.state
            .iter()
            .position(|state| state.load(Ordering::Acquire) == FREE)
    }

    fn maintain(&self) {
        if self.state[2]
            .compare_exchange(FREE, MAINT, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            assert_eq!(
                self.owners[2].fetch_add(1, Ordering::Relaxed),
                0,
                "exclusive state-CAS ownership"
            );
            assert_eq!(
                self.owners[2].fetch_sub(1, Ordering::Relaxed),
                1,
                "maintenance releases ownership"
            );
            assert!(self.state[2]
                .compare_exchange(MAINT, FREE, Ordering::Release, Ordering::Relaxed)
                .is_ok());
            let displaced = self.hint.swap(2, Ordering::Relaxed);
            if displaced < 3 {
                self.publish_word(displaced as usize);
            }
        }
    }

    fn assert_available(&self) {
        let hint = self.hint.load(Ordering::Relaxed);
        let word = self.word.load(Ordering::Acquire) & MASK;
        for slot in 0..3 {
            assert!(
                self.owners[slot].load(Ordering::Relaxed) <= 1,
                "exclusive state-CAS ownership"
            );
            if self.state[slot].load(Ordering::Acquire) == FREE {
                assert!(
                    hint == slot as u64 || word & (1 << slot) != 0,
                    "quiescent FREE slot is discoverable"
                );
            }
        }
    }
}

fn bounded(f: impl Fn() + Send + Sync + 'static) {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.max_branches = 2000;
    builder.check(f);
}

#[test]
fn two_releasers_spill_hint_claim_and_maintenance_preserve_availability() {
    bounded(|| {
        let registry = Arc::new(Registry::new());
        registry.state[2].store(FREE, Ordering::Relaxed);
        registry.hint.store(2, Ordering::Relaxed);
        let a = Arc::clone(&registry);
        let first = thread::spawn(move || a.release(0, LIVE, false));
        let b = Arc::clone(&registry);
        let second = thread::spawn(move || b.release(1, LIVE, false));
        let c = Arc::clone(&registry);
        let claim = thread::spawn(move || c.claim());
        let m = Arc::clone(&registry);
        let maintenance = thread::spawn(move || m.maintain());
        first.join().unwrap();
        second.join().unwrap();
        claim.join().unwrap();
        maintenance.join().unwrap();
        registry.assert_available();
    });
}

fn spill_model(store_mutant: bool) {
    bounded(move || {
        let registry = Registry::new();
        registry.hint.store(0, Ordering::Relaxed);
        assert!(registry.state[0]
            .compare_exchange(LIVE, FREE, Ordering::Release, Ordering::Relaxed)
            .is_ok());
        registry.release(1, LIVE, store_mutant);
        registry.hint.store(NO_HINT, Ordering::Relaxed);
        assert!(
            registry.take_nonempty(true).is_some(),
            "displaced hint is preserved in FREE_SLOTS"
        );
    });
}

#[test]
fn release_swap_spills_displaced_hint_into_bitmap() {
    spill_model(false);
}

#[test]
#[should_panic(expected = "displaced hint is preserved in FREE_SLOTS")]
fn negative_release_store_loses_displaced_hint() {
    spill_model(true);
}

fn publication_model(publish_early: bool) {
    bounded(move || {
        let registry = Arc::new(Registry::new());
        let producer = Arc::clone(&registry);
        let release =
            thread::spawn(move || producer.publish_before_or_after_free_cas(publish_early));
        let consumer = Arc::clone(&registry);
        let claim = thread::spawn(move || consumer.consume_published_candidate());
        release.join().unwrap();
        claim.join().unwrap();
        registry.assert_available();
    });
}

#[test]
fn free_cas_precedes_bitmap_publication() {
    publication_model(false);
}

#[test]
#[should_panic(expected = "quiescent FREE slot is discoverable")]
fn negative_bitmap_publication_before_free_cas_loses_availability() {
    publication_model(true);
}

fn consumption_model(ignore_old: bool) {
    bounded(move || {
        let registry = Arc::new(Registry::new());
        registry.state[0].store(FREE, Ordering::Relaxed);
        registry.word.store(1, Ordering::Relaxed);
        registry.summary.store(1, Ordering::Relaxed);
        let take = |r: Arc<Registry>| thread::spawn(move || r.take_nonempty(!ignore_old));
        let a = take(Arc::clone(&registry));
        let b = take(Arc::clone(&registry));
        let a = a.join().unwrap();
        let b = b.join().unwrap();
        assert!(
            a.is_none() || b.is_none() || a != b,
            "one bit has one candidate consumer"
        );
        // Mutant duplicates candidate values, not ownership: retain the real CAS.
        let first = registry.state[0]
            .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
            .is_ok();
        let second = registry.state[0]
            .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
            .is_ok();
        assert!(
            !(first && second),
            "state CAS still prevents duplicate ownership"
        );
    });
}

#[test]
fn take_checks_previous_bit_once() {
    consumption_model(false);
}

#[test]
#[should_panic(expected = "one bit has one candidate consumer")]
fn negative_take_ignores_old_bit_duplicate_candidate_not_ownership() {
    consumption_model(true);
}
