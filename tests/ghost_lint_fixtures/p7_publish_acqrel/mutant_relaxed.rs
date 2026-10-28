// expect: rule requires ["AcqRel"]
impl<'a> SidecarBitmap<'a> {
    pub(crate) fn issue(&self, offset: u32, class: u8) -> bool {
        match self.classes {
            ClassMap::Dense(classes) => {
                classes[granule].store(class + 1, Ordering::Release);
                true
            }
            ClassMap::Leaves(leaves) => leaves.issue(granule, class),
        }
    }

    pub(crate) fn publish(&self, offset: u32) -> bool {
        if self.classes.encoded(granule) == 0 {
            return false;
        }
        self.pending[granule / 64].fetch_or(1 << (granule % 64), Ordering::Relaxed);
        crate::ghost::ghost_event!("sidecar/published", core::ptr::from_ref(&self.pending[granule / 64]).addr(), 1);
        true
    }
}
