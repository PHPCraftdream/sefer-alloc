//! Reduced Loom model for FREE-slot spill, bitmap summary, and scan fallback.
//! Three slots abstract the production 576-byte FREE_SLOTS representation.
//! Hints and bits discover FREE slots; only state CAS grants ownership.
#![cfg(all(loom, feature = "alloc-global", feature = "internals"))]

use loom::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use loom::sync::Arc;
use loom::thread;

const FREE: u8 = 0;
const LIVE: u8 = 1;
const MAINT: u8 = 2;
const EMPTY: u64 = 3;
const MASK: u64 = 7;

struct Pool {
    state: [AtomicU8; 3],
    hint: AtomicU64,
    word: AtomicU64,
    summary: AtomicU64,
    owners: [AtomicU8; 3],
}

impl Pool {
    fn new() -> Self {
        Self {
            state: [
                AtomicU8::new(LIVE),
                AtomicU8::new(LIVE),
                AtomicU8::new(FREE),
            ],
            hint: AtomicU64::new(2),
            word: AtomicU64::new(0),
            summary: AtomicU64::new(0),
            owners: [AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0)],
        }
    }

    fn publish_word(&self, slot: usize) {
        self.word.fetch_or(1 << slot, Ordering::Release);
        self.summary.fetch_or(1, Ordering::Release);
    }

    fn release(&self, slot: usize, spill: bool) {
        assert!(self.state[slot]
            .compare_exchange(LIVE, FREE, Ordering::Release, Ordering::Relaxed)
            .is_ok());
        let displaced = if spill {
            self.hint.swap(slot as u64, Ordering::Relaxed)
        } else {
            self.hint.store(slot as u64, Ordering::Relaxed);
            EMPTY
        };
        if displaced < 3 {
            self.publish_word(displaced as usize);
        }
    }

    fn take_nonempty(&self, check_old: bool) -> Option<usize> {
        for _ in 0..3 {
            if self.summary.load(Ordering::Acquire) == 0 {
                return None;
            }
            let word = self.word.load(Ordering::Acquire) & MASK;
            if word == 0 {
                self.summary.fetch_and(!1, Ordering::AcqRel);
                if self.word.load(Ordering::Acquire) & MASK != 0 {
                    self.summary.fetch_or(1, Ordering::Release);
                }
                continue;
            }
            let bit = 1u64 << word.trailing_zeros();
            let old = self.word.fetch_and(!bit, Ordering::AcqRel);
            if check_old && old & bit == 0 {
                continue;
            }
            if old == bit {
                self.summary.fetch_and(!1, Ordering::AcqRel);
                if self.word.load(Ordering::Acquire) & MASK != 0 {
                    self.summary.fetch_or(1, Ordering::Release);
                }
            }
            return Some(bit.trailing_zeros() as usize);
        }
        None
    }

    fn pick(&self) -> (Option<usize>, Source) {
        let hinted = self.hint.swap(EMPTY, Ordering::AcqRel) as usize;
        if hinted < 3 {
            return (Some(hinted), Source::Hint);
        }
        if self.summary.load(Ordering::Acquire) != 0 {
            if let Some(slot) = self.take_nonempty(true) {
                return (Some(slot), Source::Bitmap);
            }
        }
        // The fixed-cap scan is the model's backstop for empty/quiet hints.
        let scan = self
            .state
            .iter()
            .position(|s| s.load(Ordering::Acquire) == FREE);
        (scan, Source::Scan)
    }

    fn claim(&self) {
        let (candidate, source) = self.pick();
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
                if source == Source::Scan {
                    self.word.fetch_and(!(1 << slot), Ordering::AcqRel);
                }
            }
        }
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

    fn check(&self) {
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
                    "FREE heap discoverable through hint or bitmap"
                );
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Hint,
    Bitmap,
    Scan,
}

fn model(f: impl Fn() + Send + Sync + 'static) {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.max_branches = 2000;
    builder.check(f);
}

#[test]
fn three_slot_two_release_claim_maintenance_preserve_availability_and_unique_ownership() {
    model(|| {
        let pool = Arc::new(Pool::new());
        let a = Arc::clone(&pool);
        let release_a = thread::spawn(move || a.release(0, true));
        let b = Arc::clone(&pool);
        let release_b = thread::spawn(move || b.release(1, true));
        let c = Arc::clone(&pool);
        let claim = thread::spawn(move || c.claim());
        let m = Arc::clone(&pool);
        let maintenance = thread::spawn(move || m.maintain());
        release_a.join().unwrap();
        release_b.join().unwrap();
        claim.join().unwrap();
        maintenance.join().unwrap();
        pool.check();
    });
}

#[test]
fn quiet_summary_uses_bounded_state_scan() {
    model(|| {
        let pool = Arc::new(Pool::new());
        pool.hint.store(EMPTY, Ordering::Relaxed);
        pool.word.store(0, Ordering::Relaxed);
        pool.summary.store(0, Ordering::Relaxed);
        pool.state[0].store(FREE, Ordering::Release);
        let p = Arc::clone(&pool);
        thread::spawn(move || p.claim()).join().unwrap();
        assert_eq!(
            pool.state[0].load(Ordering::Acquire),
            LIVE,
            "scan backstop claims quiet FREE slot"
        );
        assert_eq!(pool.owners[0].load(Ordering::Relaxed), 1);
    });
}

#[test]
fn summary_clear_recheck_preserves_concurrent_bit_publication() {
    model(|| {
        let pool = Arc::new(Pool::new());
        pool.word.store(0, Ordering::Relaxed);
        pool.summary.store(1, Ordering::Relaxed);
        let c = Arc::clone(&pool);
        let clear = thread::spawn(move || {
            if c.word.load(Ordering::Acquire) == 0 {
                c.summary.fetch_and(!1, Ordering::AcqRel);
                if c.word.load(Ordering::Acquire) != 0 {
                    c.summary.fetch_or(1, Ordering::Release);
                }
            }
        });
        let p = Arc::clone(&pool);
        let publish = thread::spawn(move || p.publish_word(1));
        clear.join().unwrap();
        publish.join().unwrap();
        assert!(
            pool.word.load(Ordering::Acquire) == 0 || pool.summary.load(Ordering::Acquire) & 1 != 0,
            "nonempty bitmap has summary"
        );
    });
}
