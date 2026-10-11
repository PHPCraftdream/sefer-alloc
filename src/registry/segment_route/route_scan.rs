use crate::alloc_core::remote_bitmap::BitmapScan;

use super::RouteCut;

/// Owner-held bounded scan. Finish it or persist it outside the reservation.
#[must_use = "finish or persist the bounded scan"]
pub struct RouteScan<'a> {
    pub(super) inner: BitmapScan<'a>,
}

impl<'a> RouteScan<'a> {
    #[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
    pub(crate) fn summary_words_visited(&self) -> u64 {
        self.inner.summary_words_visited()
    }

    pub fn next_cut(&mut self) -> Option<RouteCut<'a>> {
        self.inner.next_cut().map(|inner| RouteCut { inner })
    }
}
