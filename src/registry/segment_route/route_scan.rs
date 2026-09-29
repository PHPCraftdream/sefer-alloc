use crate::alloc_core::remote_bitmap::BitmapScan;

use super::RouteCut;

/// Owner-held bounded scan. Finish it or persist it outside the reservation.
#[must_use = "finish or persist the bounded scan"]
pub struct RouteScan<'a> {
    pub(super) inner: BitmapScan<'a>,
}

impl<'a> RouteScan<'a> {
    pub fn next_cut(&mut self) -> Option<RouteCut<'a>> {
        self.inner.next_cut().map(|inner| RouteCut { inner })
    }
}
