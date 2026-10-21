use crate::alloc_core::remote_bitmap::BitmapCut;

/// Detached bits must be consumed or persisted before backing reclamation.
#[must_use = "consume or persist every detached bit"]
pub struct RouteCut<'a> {
    pub(super) inner: BitmapCut<'a>,
}

impl RouteCut<'_> {
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// D1′ phase-1b: the offset ONLY. There is no class carrier left in the
    /// sidecar; the owner decides the class from the state byte in
    /// `AllocCore::reclaim_sidecar_record` (only `encode(c, LIVE)` is
    /// accepted). The former `RouteRecord` type is removed.
    pub fn pop(&mut self) -> Option<u32> {
        self.inner.pop()
    }
}
