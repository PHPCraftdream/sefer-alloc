// expect: anchor (after) not found
impl ThpCadenceCell {
    pub(crate) fn note_agree(&self) -> bool {
        let mut current = self.state.load(Ordering::Relaxed);
        let mut attempts = 0;
        while attempts < Self::MAX_ATTEMPTS {
            attempts += 1;
            let run = Self::run_of(current);
            if run >= LargeThpCadence::RUN_MAX {
                return true;
            }
            let next = (current & !Self::RUN_MASK) | u64::from(LargeThpCadence::next_run(run, true));
            match self
                .state
                .compare_exchange(0, next, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
        false
    }
}
