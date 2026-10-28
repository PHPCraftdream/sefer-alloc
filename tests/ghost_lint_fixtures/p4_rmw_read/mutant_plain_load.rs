// expect: found 0
impl SaturationHint {
    pub fn is_saturated(&self) -> bool {
        let word = self.0.load(Ordering::Acquire);
        word != DISABLED && word & 1 != 0
    }

    pub fn publish_claimable(&self) {
        let _ = self
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| Some((old + 2) & !1));
    }

    pub fn try_mark_saturated(&self, snapshot: u64) {
        if snapshot != DISABLED && snapshot & 1 == 0 {
            let _ = self.0.compare_exchange(
                snapshot,
                snapshot | 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
    }
}
