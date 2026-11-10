// expect: rule requires ["Relaxed"]
impl ThpBudgetLedger {
    const MAX_ATTEMPTS: u32 = 32;

    pub fn used(&self) -> usize {
        self.used.load(Ordering::Relaxed) as usize
    }

    pub fn try_reserve(&self, want: usize, budget: usize) -> usize {
        if budget == 0 {
            return 0;
        }
        let mut attempts = 0;
        while attempts < Self::MAX_ATTEMPTS {
            attempts += 1;
            let cur = self.used.load(Ordering::SeqCst);
            let left = (budget as u64).saturating_sub(cur);
            let cap = if (want as u64) < left {
                want as u64
            } else {
                left
            };
            let take = cap / ThpAdvice::GRANULE as u64 * ThpAdvice::GRANULE as u64;
            if take == 0 {
                return 0;
            }
            let Some(next) = cur.checked_add(take) else {
                return 0;
            };
            if self
                .used
                .compare_exchange_weak(cur, next, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return take as usize;
            }
        }
        0
    }

    pub fn give_back(&self, bytes: usize) {
        if bytes == 0 {
            return;
        }
        let prev = self.used.fetch_sub(bytes as u64, Ordering::Relaxed);
        debug_assert!(prev >= bytes as u64);
    }
}
