//! Bounded, non-authoritative FREE-slot hints; ownership still requires the slot-state CAS.

use core::mem::{align_of, size_of};
use core::sync::atomic::{AtomicU64, Ordering};

use super::MAX_HEAPS;

const WORDS: usize = MAX_HEAPS / 64;
const TAKE_ATTEMPTS: usize = 2 * WORDS;
const _: () = assert!(MAX_HEAPS == WORDS * 64 && WORDS <= 64);
const _: () = assert!(align_of::<FreeSlotHint>() == 64 && size_of::<FreeSlotHint>() == 576);

#[repr(align(64))]
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

    pub fn publish(&self, idx: usize) {
        let word = idx / 64;
        self.words[word].fetch_or(1u64 << (idx % 64), Ordering::Release);
        self.summary.fetch_or(1u64 << word, Ordering::Release);
    }

    #[cold]
    #[inline(never)]
    pub fn take_nonempty(&self) -> Option<usize> {
        for _ in 0..TAKE_ATTEMPTS {
            let summary = self.summary.load(Ordering::Acquire);
            if summary == 0 {
                return None;
            }
            let word_index = summary.trailing_zeros() as usize;
            let word = self.words[word_index].load(Ordering::Acquire);
            if word == 0 {
                self.clear_summary_recheck(word_index);
                continue;
            }
            let bit = 1u64 << word.trailing_zeros();
            let old = self.words[word_index].fetch_and(!bit, Ordering::AcqRel);
            if old & bit == 0 {
                continue;
            }
            if old == bit {
                self.clear_summary_recheck(word_index);
            }
            return Some(word_index * 64 + bit.trailing_zeros() as usize);
        }
        None
    }

    pub fn forget(&self, idx: usize) {
        self.words[idx / 64].fetch_and(!(1u64 << (idx % 64)), Ordering::AcqRel);
    }

    pub fn summary_nonempty(&self) -> bool {
        self.summary.load(Ordering::Acquire) != 0
    }

    #[cfg(feature = "bench-internals")]
    #[doc(hidden)]
    pub fn dbg_popcount(&self) -> u32 {
        self.words
            .iter()
            .map(|word| word.load(Ordering::Acquire).count_ones())
            .sum()
    }

    fn clear_summary_recheck(&self, word: usize) {
        self.summary.fetch_and(!(1u64 << word), Ordering::AcqRel);
        if self.words[word].load(Ordering::Acquire) != 0 {
            self.summary.fetch_or(1u64 << word, Ordering::Release);
        }
    }
}

pub(crate) static FREE_SLOTS: FreeSlotHint = FreeSlotHint::new();
