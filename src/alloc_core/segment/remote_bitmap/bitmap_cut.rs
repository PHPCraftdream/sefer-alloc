use core::sync::atomic::{AtomicU8, Ordering};

use super::{BitmapRecord, InvalidClass};
use crate::alloc_core::size_classes::{MIN_BLOCK, SMALL_CLASS_COUNT};

/// Detached bits must be consumed or persisted before the backing is freed.
#[must_use = "consume or persist every detached bit before releasing the segment"]
pub(crate) struct BitmapCut<'a> {
    pub(super) classes: &'a [AtomicU8],
    pub(super) word: usize,
    pub(super) bits: u64,
}

impl BitmapCut<'_> {
    pub(crate) const fn is_empty(&self) -> bool {
        self.bits == 0
    }

    /// Read class before reclaim/reissue. Do not unwind or discard a record
    /// before its ledger/reclaim commit. A cut does no work in Drop.
    pub(crate) fn pop(&mut self) -> Option<Result<BitmapRecord, InvalidClass>> {
        if self.bits == 0 {
            return None;
        }
        let bit = self.bits.trailing_zeros() as usize;
        self.bits &= self.bits - 1;
        let granule = self.word * 64 + bit;
        let encoded = self.classes[granule].load(Ordering::Relaxed);
        if encoded == 0 || usize::from(encoded) > SMALL_CLASS_COUNT {
            return Some(Err(InvalidClass));
        }
        Some(Ok(BitmapRecord {
            offset: (granule * MIN_BLOCK) as u32,
            class: encoded - 1,
        }))
    }
}
