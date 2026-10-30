// expect: rule requires ["Acquire"]
impl HeapIngress {
    fn notify(&self, slot: u32) {
        let s = slot as usize;
        self.mask[s / 64].fetch_or(1 << (s % 64), Ordering::Relaxed);
        self.summary.fetch_or(1 << (s / 64), Ordering::Release);
    }

    pub(crate) fn take(&self, mut each: impl FnMut(usize)) {
        if self.summary.load(Ordering::Relaxed) == 0 {
            return;
        }
        let mut words = self.summary.swap(0, Ordering::Relaxed);
        while words != 0 {
            let w = words.trailing_zeros() as usize;
            words &= words - 1;
            let mut bits = self.mask[w].swap(0, Ordering::Acquire);
            while bits != 0 {
                let j = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                each(w * 64 + j);
            }
        }
    }
}
