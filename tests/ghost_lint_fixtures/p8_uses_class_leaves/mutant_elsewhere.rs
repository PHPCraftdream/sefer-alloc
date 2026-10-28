// expect: only ["SmallSidecar::bitmap"]
impl SmallSidecar {
    fn bitmap(&self) -> SidecarBitmap<'_> {
        SidecarBitmap::from_leaves(&self.pending, &self.classes)
            .unwrap_or_else(|| fatal(b"small_sidecar: sidecar bitmap leaf layout mismatch\n"))
    }
}

impl<'a> SidecarBitmap<'a> {
    pub(crate) fn from_initialized(pending: &'a [AtomicU64], classes: &'a [AtomicU8]) -> Option<Self> {
        SidecarBitmap::from_leaves(pending, LEAVES)
    }
}
