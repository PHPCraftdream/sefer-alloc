use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};

use super::BitmapCut;

/// A fixed high-water scan. One cut is taken per word, including empty words.
#[must_use = "finish the fixed scan or persist the remaining cursor"]
pub(crate) struct BitmapScan<'a> {
    pub(super) pending: core::slice::Iter<'a, AtomicU64>,
    pub(super) classes: &'a [AtomicU8],
    pub(super) next_word: usize,
    pub(super) end_word: usize,
}

impl<'a> BitmapScan<'a> {
    pub(crate) fn next_cut(&mut self) -> Option<BitmapCut<'a>> {
        if self.next_word == self.end_word {
            return None;
        }
        let word = self.next_word;
        self.next_word += 1;
        let pending = self.pending.next().unwrap_or_else(|| std::process::abort());
        let bits = pending.swap(0, Ordering::AcqRel);
        let class_start = word * 64;
        Some(BitmapCut {
            // Empty cuts never read classes; non-empty cuts borrow one word.
            classes: if bits == 0 {
                &[]
            } else {
                &self.classes[class_start..class_start + 64]
            },
            word,
            bits,
        })
    }
}
