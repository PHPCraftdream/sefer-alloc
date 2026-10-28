// expect: expected 3 matching site(s), found 2
impl<'a> BitmapScan<'a> {
    pub(crate) fn next_cut(&mut self) -> Option<BitmapCut<'a>> {
        let pending = self.pending.next().unwrap();
        let (bits, empty, exchanged) = if baseline {
            let bits = pending.swap(0, Ordering::AcqRel);
            (bits, bits == 0, true)
        } else if pending.load(Ordering::Acquire) == 0 {
            (0, true, false)
        } else {
            (pending.swap(0, Ordering::AcqRel), false, true)
        };
        let bits = if pending.load(Ordering::Acquire) == 0 {
            0
        } else {
            pending.load(Ordering::Acquire)
        };
        crate::ghost::ghost_event!("sidecar/cut-bits", core::ptr::from_ref(pending).addr(), bits.count_ones());
        Some(BitmapCut { bits })
    }
}
