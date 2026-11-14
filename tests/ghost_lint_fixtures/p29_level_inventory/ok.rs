impl ThpCadenceCell {
    pub(crate) fn level(&self) -> u32 {
        let word = self.state.load(Ordering::Relaxed);
        LargeThpCadence::level_of(Self::run_of(word))
    }
}
