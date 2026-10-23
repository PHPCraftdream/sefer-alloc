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

    /// Возвращает только offset; класс и LIVE проверяет owner reclaim.
    /// Producer отвергает never-issued гранулу по start gate. Если бит всё же
    /// повреждён, reclaim сохраняет fatal-проверки state/start/geometry.
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
