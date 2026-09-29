use core::sync::atomic::Ordering;

use super::{BitmapCut, SidecarBitmap};

/// A fixed high-water scan. One cut is taken per word, including empty words.
#[must_use = "finish the fixed scan or persist the remaining cursor"]
pub(crate) struct BitmapScan<'a> {
    pub(super) bitmap: SidecarBitmap<'a>,
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
        let bits = self.bitmap.pending[word].swap(0, Ordering::AcqRel);
        Some(BitmapCut {
            classes: self.bitmap.classes,
            word,
            bits,
        })
    }
}
