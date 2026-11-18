use crate::alloc_core::remote_bitmap::BitmapCut;

use super::RouteRecord;

/// Detached bits must be consumed or persisted before backing reclamation.
#[must_use = "consume or persist every detached bit"]
pub struct RouteCut<'a> {
    pub(super) inner: BitmapCut<'a>,
}

impl RouteCut<'_> {
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn pop(&mut self) -> Option<RouteRecord> {
        self.inner.pop().map(|record| RouteRecord {
            offset: record.offset,
            class: record.class,
        })
    }
}
