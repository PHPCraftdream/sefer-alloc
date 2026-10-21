use crate::alloc_core::size_classes::MIN_BLOCK;

/// Detached bits must be consumed or persisted before the backing is freed.
#[must_use = "consume or persist every detached bit before releasing the segment"]
pub(crate) struct BitmapCut<'a> {
    pub(super) _marker: core::marker::PhantomData<&'a ()>,
    pub(super) word: usize,
    pub(super) bits: u64,
}

impl BitmapCut<'_> {
    pub(crate) const fn is_empty(&self) -> bool {
        self.bits == 0
    }

    /// D1′ phase-1b: the leaf-class map is gone entirely — `pop` returns ONLY
    /// the record offset, and the class is decided from the owner-private
    /// state byte in `AllocCore::reclaim_sidecar_record` (only
    /// `encode(c, LIVE)` is accepted). The former `fatal` on a zero leaf
    /// encoding is gone with it: a never-issued granule's pending bit can no
    /// longer abort the owner, and the sidecar carries only the pending and
    /// start banks.
    pub(crate) fn pop(&mut self) -> Option<u32> {
        if self.bits == 0 {
            return None;
        }
        let bit = self.bits.trailing_zeros() as usize;
        let granule = self.word * 64 + bit;
        self.bits &= self.bits - 1;
        Some((granule * MIN_BLOCK) as u32)
    }
}
