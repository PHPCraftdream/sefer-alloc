// expect: may do this
impl ThpCadenceCell {
    pub(crate) fn note_disagree(&self) -> u32 {
        let previous = self.state.fetch_and(!Self::RUN_MASK, Ordering::Relaxed);
        Self::run_of(previous)
    }
}
impl ThpCadenceCell {
    pub(crate) fn pin(&self, level: Option<u32>) {
        self.state.fetch_and(!Self::PIN_MASK, Ordering::Relaxed);
    }
}
impl ThpCadenceCell {
    pub(crate) fn reset_on_agree(&self) {
        self.state.fetch_and(!Self::RUN_MASK, Ordering::Relaxed);
    }
}
