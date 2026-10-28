impl FreeSlotHint {
    pub fn publish(&self, idx: usize) {
        let word = idx / 64;
        self.words[word].fetch_or(1u64 << (idx % 64), Ordering::Release);
        self.summary.fetch_or(1u64 << word, Ordering::Release);
    }

    pub fn take_nonempty(&self) -> Option<usize> {
        for _ in 0..TAKE_ATTEMPTS {
            let summary = self.summary.load(Ordering::Acquire);
            let word_index = summary.trailing_zeros() as usize;
            let word = self.words[word_index].load(Ordering::Acquire);
            let bit = 1u64 << word.trailing_zeros();
            let old = self.words[word_index].fetch_and(!bit, Ordering::AcqRel);
            if old & bit == 0 {
                continue;
            }
            return Some(word_index * 64);
        }
        None
    }

    pub fn forget(&self, idx: usize) {
        self.words[idx / 64].fetch_and(!(1u64 << (idx % 64)), Ordering::AcqRel);
    }

    fn clear_summary_recheck(&self, word: usize) {
        self.summary.fetch_and(!(1u64 << word), Ordering::AcqRel);
        if self.words[word].load(Ordering::Acquire) != 0 {
            self.summary.fetch_or(1u64 << word, Ordering::Release);
        }
    }
}
