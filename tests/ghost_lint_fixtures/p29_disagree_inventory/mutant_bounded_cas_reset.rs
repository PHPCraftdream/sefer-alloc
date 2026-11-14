// expect: not in the catalog table
impl ThpCadenceCell {
    pub(crate) fn note_disagree(&self) -> u32 {
        let mut current = self.state.load(Ordering::Relaxed);
        let mut attempts = 0;
        while attempts < Self::MAX_ATTEMPTS {
            attempts += 1;
            match self.state.compare_exchange(
                current,
                current & !Self::RUN_MASK,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(previous) => return Self::run_of(previous),
                Err(actual) => current = actual,
            }
        }
        0
    }
}
