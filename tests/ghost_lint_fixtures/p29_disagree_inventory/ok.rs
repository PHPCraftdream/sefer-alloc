impl ThpCadenceCell {
    pub(crate) fn note_disagree(&self) -> u32 {
        let previous = self.state.fetch_and(!Self::RUN_MASK, Ordering::Relaxed);
        Self::run_of(previous)
    }
}
