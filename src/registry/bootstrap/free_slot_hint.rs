//! Bounded, non-authoritative FREE-slot bitmap; only a slot-state CAS grants ownership.

use core::sync::atomic::{AtomicU64, Ordering};

use super::MAX_HEAPS;

const WORDS: usize = MAX_HEAPS / 64;
const TAKE_ATTEMPTS: usize = 2 * WORDS;
const _: () = assert!(MAX_HEAPS == WORDS * 64 && WORDS <= 64);

/// Idempotent FREE-slot hints with lowest-index selection in quiescence.
/// Publications follow the slot's transition to FREE; stale hints are allowed.
#[doc(hidden)]
pub struct FreeSlotHint {
    summary: AtomicU64,
    words: [AtomicU64; WORDS],
}

impl Default for FreeSlotHint {
    fn default() -> Self {
        Self::new()
    }
}

impl FreeSlotHint {
    pub const fn new() -> Self {
        Self {
            summary: AtomicU64::new(0),
            words: [const { AtomicU64::new(0) }; WORDS],
        }
    }

    /// Publish an existing slot after its successful transition to FREE.
    pub fn publish(&self, idx: usize) {
        let w = idx / 64;
        self.words[w].fetch_or(1u64 << (idx % 64), Ordering::Release);
        self.summary.fetch_or(1u64 << w, Ordering::Release);
    }

    /// Consume a candidate in at most 128 attempts, without granting ownership.
    pub fn take(&self) -> Option<usize> {
        for _ in 0..TAKE_ATTEMPTS {
            let summary = self.summary.load(Ordering::Acquire);
            if summary == 0 {
                return None;
            }
            let w = summary.trailing_zeros() as usize;
            let word = self.words[w].load(Ordering::Acquire);
            if word == 0 {
                self.clear_summary_recheck(w);
                continue;
            }
            let bit = 1u64 << word.trailing_zeros();
            let old = self.words[w].fetch_and(!bit, Ordering::AcqRel);
            if old & bit == 0 {
                continue;
            }
            if old == bit {
                self.clear_summary_recheck(w);
            }
            return Some(w * 64 + bit.trailing_zeros() as usize);
        }
        None
    }

    /// Clear only this slot's bit after a successful FREE-to-LIVE claim.
    /// A stale summary bit is cleaned by the next take.
    pub fn forget(&self, idx: usize) {
        self.words[idx / 64].fetch_and(!(1u64 << (idx % 64)), Ordering::AcqRel);
    }

    /// Read-only: is this slot's hint bit currently set?
    pub fn contains(&self, idx: usize) -> bool {
        idx < MAX_HEAPS && self.words[idx / 64].load(Ordering::Acquire) & (1u64 << (idx % 64)) != 0
    }

    fn clear_summary_recheck(&self, w: usize) {
        self.summary.fetch_and(!(1u64 << w), Ordering::AcqRel);
        if self.words[w].load(Ordering::Acquire) != 0 {
            self.summary.fetch_or(1u64 << w, Ordering::Release);
        }
    }
}
