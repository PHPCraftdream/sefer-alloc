// expect: not in the catalog table
impl ThpCadenceCell {
    pub(crate) fn level(&self) -> u32 {
        let word = self.state.load(Ordering::SeqCst);
        LargeThpCadence::level_of(Self::run_of(word))
    }
}
