//! Reduced shadow of bootstrap/free_slot_hint.rs and heap_registry/claim.rs.
//! Two materialised slots, one bitmap word, two bounded take attempts (2 * WORDS).
//! All protocol atomics/orderings mirror production; oracle atoms are independent.
//! No saturation/bump/scan, OOM, payload, chunk allocation, or unbounded retries.
#![cfg(all(loom, feature = "alloc-global", feature = "internals"))]

use loom::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use loom::sync::Arc;
use loom::thread;

const FREE: u8 = 0;
const LIVE: u8 = 1;
const MAINT: u8 = 2;
const NO_HINT: u64 = 2;

struct FreeSet {
    word: AtomicU64,
    summary: AtomicU64,
}

impl FreeSet {
    fn new(word: u64, summary: u64) -> Self {
        Self {
            word: AtomicU64::new(word),
            summary: AtomicU64::new(summary),
        }
    }

    fn publish(&self, idx: usize) {
        self.word.fetch_or(1 << idx, Ordering::Release);
        self.summary.fetch_or(1, Ordering::Release);
    }

    fn clear_summary_recheck(&self, recheck: bool) {
        self.summary.fetch_and(!1, Ordering::AcqRel);
        if recheck && self.word.load(Ordering::Acquire) != 0 {
            self.summary.fetch_or(1, Ordering::Release);
        }
    }

    fn take(&self, check_old: bool) -> Option<usize> {
        for _ in 0..2 {
            if self.summary.load(Ordering::Acquire) == 0 {
                return None;
            }
            let word = self.word.load(Ordering::Acquire);
            if word == 0 {
                self.clear_summary_recheck(true);
                continue;
            }
            let bit = 1u64 << word.trailing_zeros();
            let old = self.word.fetch_and(!bit, Ordering::AcqRel);
            if check_old && old & bit == 0 {
                continue;
            }
            if old == bit {
                self.clear_summary_recheck(true);
            }
            return Some(bit.trailing_zeros() as usize);
        }
        None
    }

    fn forget(&self, idx: usize) {
        self.word.fetch_and(!(1 << idx), Ordering::AcqRel);
    }
}

struct Registry {
    states: [AtomicU8; 2],
    hint: AtomicU64,
    set: FreeSet,
    // Not candidate-based: only successful state CAS winners enter this oracle.
    owners: [AtomicU8; 2],
}

impl Registry {
    fn new() -> Self {
        Self {
            states: [AtomicU8::new(LIVE), AtomicU8::new(FREE)],
            hint: AtomicU64::new(1),
            set: FreeSet::new(2, 1),
            owners: [AtomicU8::new(0), AtomicU8::new(0)],
        }
    }

    fn publish(&self, idx: usize) {
        self.hint.store(idx as u64, Ordering::Relaxed);
        self.set.publish(idx);
    }

    fn release(&self, idx: usize, from: u8) {
        assert!(self.states[idx]
            .compare_exchange(from, FREE, Ordering::Release, Ordering::Relaxed)
            .is_ok());
    }

    fn enter(&self, idx: usize) {
        assert_eq!(
            self.owners[idx].fetch_add(1, Ordering::Relaxed),
            0,
            "unique state-CAS owners"
        );
    }

    fn claim(&self) {
        // A lost hint CAS retries selector: hint -> free-set -> state CAS.
        // Two selector passes suffice for this reduced finite scenario.
        for _ in 0..2 {
            let hint = self.hint.swap(NO_HINT, Ordering::AcqRel);
            let idx = if hint < NO_HINT {
                Some(hint as usize)
            } else {
                self.set.take(true)
            };
            let Some(idx) = idx else { return };
            let observed = self.states[idx].load(Ordering::Acquire);
            if observed == FREE
                && self.states[idx]
                    .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
                self.enter(idx);
                self.set.forget(idx);
                // Keep this owner LIVE until model quiescence.
                return;
            }
        }
    }

    fn maintenance(&self) {
        if self.states[1]
            .compare_exchange(FREE, MAINT, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.enter(1);
            assert_eq!(
                self.owners[1].fetch_sub(1, Ordering::Relaxed),
                1,
                "unique state-CAS owners"
            );
            self.release(1, MAINT);
            self.publish(1);
        }
    }

    fn assert_available(&self) {
        let bits = self.set.word.load(Ordering::Acquire);
        let hint = self.hint.load(Ordering::Relaxed);
        for idx in 0..2 {
            if self.states[idx].load(Ordering::Acquire) == FREE {
                assert!(
                    bits & (1 << idx) != 0 || hint == idx as u64,
                    "quiescent FREE has bit or hint"
                );
            }
            assert!(
                self.owners[idx].load(Ordering::Relaxed) <= 1,
                "unique state-CAS owners"
            );
        }
    }
}

fn bounded_model(f: impl Fn() + Send + Sync + 'static) {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.max_branches = 1000;
    builder.check(f);
}

#[test]
fn free_set_recycle_claim_maintenance_preserve_availability_and_owners() {
    bounded_model(|| {
        let reg = Arc::new(Registry::new());
        let r = Arc::clone(&reg);
        let recycle = thread::spawn(move || {
            r.release(0, LIVE);
            r.publish(0);
        });
        let r = Arc::clone(&reg);
        let claim = thread::spawn(move || r.claim());
        let r = Arc::clone(&reg);
        let maintenance = thread::spawn(move || r.maintenance());
        recycle.join().unwrap();
        claim.join().unwrap();
        maintenance.join().unwrap();
        reg.assert_available();
    });
}

fn summary_model(recheck: bool) {
    bounded_model(move || {
        // A stale summary with an empty word is a legitimate take precursor.
        let set = Arc::new(FreeSet::new(0, 1));
        let s = Arc::clone(&set);
        let clear = thread::spawn(move || {
            if s.word.load(Ordering::Acquire) == 0 {
                s.clear_summary_recheck(recheck);
            }
        });
        let s = Arc::clone(&set);
        let publish = thread::spawn(move || s.publish(1));
        clear.join().unwrap();
        publish.join().unwrap();
        assert!(
            set.word.load(Ordering::Acquire) == 0 || set.summary.load(Ordering::Acquire) & 1 != 0,
            "quiescent nonzero word has summary"
        );
    });
}

#[test]
fn free_set_publish_clear_recheck_preserves_summary() {
    summary_model(true);
}

#[test]
#[should_panic(expected = "quiescent nonzero word has summary")]
fn negative_free_set_no_summary_recheck() {
    summary_model(false);
}

fn consumption_model(check_old: bool) {
    bounded_model(move || {
        // No publication/forget/state recycling: each initial bit is one token.
        // This oracle detects duplicate consumption, NOT duplicate ownership.
        let set = Arc::new(FreeSet::new(3, 1));
        let states = Arc::new([AtomicU8::new(FREE), AtomicU8::new(FREE)]);
        let consume = |s: Arc<FreeSet>, states: Arc<[AtomicU8; 2]>| {
            thread::spawn(move || {
                let candidate = s.take(check_old);
                let owner = candidate.filter(|&idx| {
                    states[idx]
                        .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                });
                (candidate, owner)
            })
        };
        let a = consume(Arc::clone(&set), Arc::clone(&states));
        let b = consume(Arc::clone(&set), Arc::clone(&states));
        let (a, owner_a) = a.join().unwrap();
        let (b, owner_b) = b.join().unwrap();
        // Even the broken take cannot defeat the independent ownership CAS.
        assert!(
            owner_a.is_none() || owner_b.is_none() || owner_a != owner_b,
            "unique state-CAS owners"
        );
        assert!(
            a.is_none() || b.is_none() || a != b,
            "unique bitmap token consumption"
        );
    });
}

#[test]
fn free_set_take_consumes_each_bit_once() {
    consumption_model(true);
}

#[test]
#[should_panic(expected = "unique bitmap token consumption")]
fn negative_free_set_take_ignores_old_bit() {
    consumption_model(false);
}

#[test]
#[should_panic(expected = "quiescent FREE has bit or hint")]
fn negative_free_set_publish_before_state_cas() {
    bounded_model(|| {
        let reg = Arc::new(Registry::new());
        // Isolate lost availability: slot 1 is LIVE, no unrelated candidates.
        reg.states[1].store(LIVE, Ordering::Relaxed);
        reg.hint.store(NO_HINT, Ordering::Relaxed);
        reg.set.word.store(0, Ordering::Relaxed);
        reg.set.summary.store(0, Ordering::Relaxed);
        let r = Arc::clone(&reg);
        let recycle = thread::spawn(move || {
            r.publish(0); // MUTANT: consumed while state is still LIVE.
            r.release(0, LIVE); // No post-CAS publication.
        });
        let r = Arc::clone(&reg);
        let claim = thread::spawn(move || r.claim());
        recycle.join().unwrap();
        claim.join().unwrap();
        reg.assert_available();
    });
}
