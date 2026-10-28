// expect: rule requires ["Release"]
impl<'a> SidecarBitmap<'a> {
    pub(crate) fn issue(&self, offset: u32, class: u8) -> bool {
        match self.classes {
            ClassMap::Dense(classes) => {
                classes[granule].store(class + 1, Ordering::Relaxed);
                true
            }
            ClassMap::Leaves(leaves) => leaves.issue(granule, class),
        }
    }

    pub(crate) fn publish(&self, offset: u32) -> bool {
        if self.classes.encoded(granule) == 0 {
            return false;
        }
        self.pending[granule / 64].fetch_or(1 << (granule % 64), Ordering::AcqRel);
        crate::ghost::ghost_event!("sidecar/published", core::ptr::from_ref(&self.pending[granule / 64]).addr(), 1);
        true
    }
}
