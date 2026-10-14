use super::{AtomicU64, BitmapCut, ClassMap, Ordering};
use crate::alloc_core::os::SEGMENT;
use crate::alloc_core::size_classes::MIN_BLOCK;

const SUMMARY_WORDS: usize = 64;

/// Discovery-only cursor. Fixed scans continue to use `BitmapScan`.
#[must_use = "consume every selected pending cut before releasing the sidecar"]
pub(crate) struct SummaryScan<'a> {
    pending: &'a [AtomicU64],
    classes: ClassMap<'a>,
    start_word: usize,
    end_word: usize,
    summary_word: usize,
    summary_end: usize,
    candidate_summary: usize,
    candidates: u64,
    first_mask: u64,
    last_mask: u64,
}

impl<'a> SummaryScan<'a> {
    pub(crate) const SUMMARY_WORDS: usize = SUMMARY_WORDS;

    pub(crate) fn new(
        pending: &'a [AtomicU64],
        classes: ClassMap<'a>,
        high_water: usize,
        start_word: usize,
    ) -> Option<Self> {
        if high_water > SEGMENT || start_word < SUMMARY_WORDS {
            return None;
        }
        let end_word = high_water.div_ceil(MIN_BLOCK * 64);
        if start_word > end_word || pending.len() != SEGMENT / (MIN_BLOCK * 64) {
            return None;
        }
        let summary_start = start_word / 64;
        let summary_end = end_word.div_ceil(64);
        if start_word == end_word {
            return Some(Self {
                pending,
                classes,
                start_word,
                end_word,
                summary_word: summary_end,
                summary_end,
                candidate_summary: summary_end,
                candidates: 0,
                first_mask: 0,
                last_mask: 0,
            });
        }
        let first_mask = u64::MAX << (start_word % 64);
        let last_mask = if end_word % 64 == 0 {
            u64::MAX
        } else {
            (1u64 << (end_word % 64)) - 1
        };
        Some(Self {
            pending,
            classes,
            start_word,
            end_word,
            summary_word: summary_start,
            summary_end,
            candidate_summary: summary_start,
            candidates: 0,
            first_mask,
            last_mask,
        })
    }

    pub(crate) fn next_cut(&mut self) -> Option<BitmapCut<'a>> {
        loop {
            while self.candidates == 0 {
                if self.summary_word == self.summary_end {
                    return None;
                }
                let s = self.summary_word;
                self.summary_word += 1;
                let mut mask = if s == self.start_word / 64 {
                    self.first_mask
                } else {
                    u64::MAX
                };
                if s + 1 == self.summary_end {
                    mask &= self.last_mask;
                }
                self.candidate_summary = s;
                self.candidates = self.pending[s].swap(0, Ordering::AcqRel) & mask;
                #[cfg(all(
                    feature = "bench-internals",
                    any(r19_sidecar_summary_oracle, r20_prf01_c3_oracle)
                ))]
                super::super::Prf01SidecarOracle::summary_word_visited();
            }
            let bit = self.candidates.trailing_zeros() as usize;
            self.candidates &= self.candidates - 1;
            let word = self.candidate_summary * 64 + bit;
            if word < self.start_word || word >= self.end_word {
                continue;
            }
            let bits = self.pending[word].swap(0, Ordering::AcqRel);
            return Some(BitmapCut {
                classes: self.classes,
                word,
                bits,
            });
        }
    }
}
