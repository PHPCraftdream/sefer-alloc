//! Reduced Loom model of FREE-slot hint spill and bounded bitmap fallback.
//! Three slots, two releasers, one claimer and maintenance; bitmap dimensions
//! are abstracted. Production FREE_SLOTS remains 576-byte cache-line aligned.
//! State CAS alone grants ownership; hints/bitmap are discovery only.
#![cfg(all(loom, feature = "alloc-global", feature = "internals"))]

use loom::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};
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
    count: AtomicU64,
    init: [bool; 3],
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
            count: AtomicU64::new(3),
            // Slots start LIVE and initialised; the ctor-OOM model overrides.
            init: [true, true, true],
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

impl Registry {
    /// R6-T03 model: a ctor-OOM slot released into the bitmap must not
    /// monopolise discovery — the recovery scan still reaches any
    /// initialised FREE slot.
    fn ctor_oom_claim(&self, saw: &AtomicUsize) {
        let hinted = self.hint.swap(NO_HINT, Ordering::AcqRel) as usize;
        let candidate = if hinted < 3 {
            Some(hinted)
        } else {
            self.take_nonempty(true)
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
                if self.init[slot] {
                    return; // initialised heap won
                }
                // Constructor OOM: give the slot back, publish into the
                // BITMAP only — never the hint.
                assert!(self.state[slot]
                    .compare_exchange(LIVE, FREE, Ordering::Release, Ordering::Relaxed)
                    .is_ok());
                assert_eq!(
                    self.owners[slot].fetch_sub(1, Ordering::Relaxed),
                    1,
                    "OOM path drops ownership"
                );
                self.publish_word(slot);
                // Recovery scan prefers initialised FREE slots.
                if let Some(i) = (0..3)
                    .filter(|&i| i != slot)
                    .find(|&i| self.init[i] && self.state[i].load(Ordering::Acquire) == FREE)
                {
                    saw.store(1, Ordering::Relaxed);
                    if self.state[i]
                        .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        assert_eq!(
                            self.owners[i].fetch_add(1, Ordering::Relaxed),
                            0,
                            "exclusive state-CAS ownership"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn ctor_oom_bitmap_slot_does_not_starve_initialised_recovery() {
    bounded(|| {
        // slot 0: FREE, never initialised, discoverable only via the bitmap
        // (R6-T03 `push_back_after_oom`); slot 1: LIVE, initialised.
        let mut reg = Registry::new();
        reg.init[0] = false;
        let r = Arc::new(reg);
        r.state[0].store(FREE, Ordering::Relaxed);
        r.publish_word(0);
        r.hint.store(NO_HINT, Ordering::Relaxed);
        let saw = Arc::new(AtomicUsize::new(0));

        let rel = Arc::clone(&r);
        let releaser = thread::spawn(move || {
            // release(1) with the production swap shape: hint := 1, spill displaced.
            rel.release(1, LIVE, false);
        });

        let cl = Arc::clone(&r);
        let saw2 = Arc::clone(&saw);
        let claimer = thread::spawn(move || {
            cl.ctor_oom_claim(&saw2);
        });

        releaser.join().unwrap();
        claimer.join().unwrap();

        // Exclusive ownership everywhere.
        for slot in 0..3 {
            assert!(r.owners[slot].load(Ordering::Relaxed) <= 1);
        }
        // The claimer never ends up owning the UNINITIALISED heap.
        assert_eq!(
            r.owners[0].load(Ordering::Relaxed),
            0,
            "uninitialised slot is never handed out"
        );
        // If the recovery scan observed slot 1 FREE, the claimer must own it:
        // the initialised FREE heap is reachable (the pre-R6-T03 shape — OOM
        // slot re-takes the hint and the claim gives up — fails this).
        if saw.load(Ordering::Acquire) == 1 {
            assert_eq!(
                r.owners[1].load(Ordering::Relaxed),
                1,
                "initialised FREE heap reachable"
            );
        }
        // Quiescent invariant: an initialised FREE slot is discoverable via
        // hint or bitmap; the OOM slot never sits in the hint.
        let hint = r.hint.load(Ordering::Relaxed);
        let word = r.word.load(Ordering::Acquire) & MASK;
        assert!(
            hint != 0,
            "uninitialised OOM slot never monopolises the hint"
        );
        if r.state[1].load(Ordering::Acquire) == FREE {
            assert!(
                hint == 1 || word & (1 << 1) != 0,
                "initialised FREE slot discoverable"
            );
        }
    });
}

// ---------------------------------------------------------------------------
// R6-T53: the chunk-OOM recovery slice of `claim_impl`. A bump-minted index
// whose chunk reservation failed is fuelled into the hint (spilling any
// displaced hint into the FREE set) and stays retryable; the recovery scan
// skips the failed index and never hands the unmaterialised slot out.
// ---------------------------------------------------------------------------

/// A minted index with no materialised chunk behind it (`slot_or_none` → None).
const UNMATERIALISED: u8 = 3;

impl Registry {
    /// `bump_count`: mint the next index below the cap.
    fn bump_index(&self) -> Option<usize> {
        let mut count = self.count.load(Ordering::Acquire);
        loop {
            if count as usize >= 3 {
                return None;
            }
            match self.count.compare_exchange_weak(
                count,
                count + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(count as usize),
                Err(actual) => count = actual,
            }
        }
    }

    fn materialised(&self, idx: usize) -> bool {
        self.state[idx].load(Ordering::Acquire) != UNMATERIALISED
    }

    /// `scan_claim_recovery_free(reg, excluded)`: one pass over the
    /// high-water range, skipping the failed index; a materialised FREE slot
    /// wins, preferring initialised heaps.
    fn recovery_scan(&self, excluded: usize, saw: &AtomicUsize) -> Option<usize> {
        let count = (self.count.load(Ordering::Acquire) as usize).min(3);
        let mut uninitialised = None;
        for idx in 0..count {
            if idx == excluded || !self.materialised(idx) {
                continue;
            }
            if self.state[idx].load(Ordering::Acquire) == FREE {
                if self.init[idx] {
                    saw.store(1, Ordering::Relaxed);
                    return Some(idx);
                }
                uninitialised.get_or_insert(idx);
            }
        }
        uninitialised
    }

    /// The chunk-OOM slice: the failed minted index is fuelled into the hint
    /// (retryable), then one recovery pass runs with the failed index
    /// excluded. The mutant hands the minted index out without
    /// `slot_or_none`'s materialisation gate; the violation is observed by
    /// the caller's post-join oracle, not here.
    fn claim_chunk_oom(&self, failed: usize, mutant: bool, saw: &AtomicUsize) {
        if mutant {
            // MUTANT: no `slot_or_none` gate — the fuelled minted index is
            // used as a heap candidate and takes the slot that has no
            // materialised chunk behind it.
            assert_eq!(
                self.owners[failed].fetch_add(1, Ordering::Relaxed),
                0,
                "exclusive state-CAS ownership"
            );
            return;
        }
        let prev = self.hint.swap(failed as u64, Ordering::Relaxed);
        if (prev as usize) < 3 {
            self.publish_word(prev as usize);
        }
        if let Some(idx) = self.recovery_scan(failed, saw) {
            if self.state[idx]
                .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                assert_eq!(
                    self.owners[idx].fetch_add(1, Ordering::Relaxed),
                    0,
                    "exclusive state-CAS ownership"
                );
            }
        }
    }
}

#[test]
fn chunk_oom_minted_index_is_retryable_but_never_handed_out() {
    bounded(|| {
        // Slot 0 exists and is LIVE (`count = 1`); the bump mints index 1 and
        // its chunk reservation fails, leaving it unmaterialised. Slot 0 is
        // freed concurrently, so the recovery pass and the plain claim race
        // for the one initialised heap.
        let mut reg = Registry::new();
        reg.count.store(1, Ordering::Relaxed);
        reg.state[1].store(UNMATERIALISED, Ordering::Relaxed);
        reg.init[1] = false;
        reg.release(0, LIVE, false);
        let r = Arc::new(reg);
        let saw = Arc::new(AtomicUsize::new(0));

        let b = Arc::clone(&r);
        // The mint: `bump_count` publishes index 1 (count 1 → 2); the chunk
        // reservation then fails, leaving the slot unmaterialised.
        let bumper = thread::spawn(move || {
            b.bump_index();
        });

        let m = Arc::clone(&r);
        let saw1 = Arc::clone(&saw);
        let recovery = thread::spawn(move || m.claim_chunk_oom(1, false, &saw1));
        let c = Arc::clone(&r);
        let claimer = thread::spawn(move || c.claim());
        bumper.join().unwrap();
        recovery.join().unwrap();
        claimer.join().unwrap();

        for slot in 0..3 {
            assert!(r.owners[slot].load(Ordering::Relaxed) <= 1);
        }
        assert_eq!(
            r.owners[1].load(Ordering::Relaxed),
            0,
            "unmaterialised minted index is never handed out"
        );
        assert_eq!(
            r.owners[0].load(Ordering::Relaxed),
            1,
            "the initialised FREE heap is claimed by exactly one discovery path"
        );
    });
}

#[test]
#[should_panic(expected = "unmaterialised minted index is never handed out")]
fn negative_chunk_oom_minted_index_hands_out_without_materialisation() {
    bounded(|| {
        let mut reg = Registry::new();
        reg.count.store(1, Ordering::Relaxed);
        reg.state[1].store(UNMATERIALISED, Ordering::Relaxed);
        reg.init[1] = false;
        let r = Arc::new(reg);
        let saw = Arc::new(AtomicUsize::new(0));
        let m = Arc::clone(&r);
        let claimant = thread::spawn(move || m.claim_chunk_oom(1, true, &saw));
        claimant.join().unwrap();
        // Oracle: the high-water does not cover the unmaterialised minted
        // index, so no ownership may have been granted through it. The
        // mutant granted ownership above, so this fires in the model's only
        // schedule — the #[should_panic] expectation is genuinely exercised.
        assert_eq!(
            r.owners[1].load(Ordering::Relaxed),
            0,
            "unmaterialised minted index is never handed out"
        );
    });
}
