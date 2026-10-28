// expect: outside a branch matching
impl<'a> SidecarBitmap<'a> {
    pub(crate) fn publish(&self, offset: u32) -> bool {
        let Some(granule) = Self::granule(offset) else {
            return false;
        };
        if granule == 0 {
            return false;
        }
        self.pending[granule / 64].fetch_or(1 << (granule % 64), Ordering::AcqRel);
        true
    }
}
