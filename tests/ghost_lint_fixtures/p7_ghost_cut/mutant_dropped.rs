// expect: no ghost_assert!/ghost_event! with id sidecar/cut-bits
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
            pending.swap(0, Ordering::AcqRel)
        };
        Some(BitmapCut { bits })
    }
}
