// expect: not in the catalog table
impl ThpCadenceCell {
    pub(crate) fn note_disagree(&self) -> u32 {
        let previous = self.state.fetch_and(!Self::RUN_MASK, Ordering::SeqCst);
        Self::run_of(previous)
    }
}
