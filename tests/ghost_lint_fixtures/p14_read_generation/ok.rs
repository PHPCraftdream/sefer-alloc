impl<T> AtomicSlot<T> {
    pub(crate) fn read_with<R>(&self, expected_gen: u32, guard: &Guard, f: impl FnOnce(&T) -> R) -> Option<R> {
        let g1 = self.generation.load(Ordering::Acquire);
        if g1 != expected_gen {
            return None;
        }
        let shared: Shared<'_, T> = self.value.load(Ordering::Acquire, guard);
        let g2 = self.generation.load(Ordering::Acquire);
        if g2 != g1 {
            return None;
        }
        Some(f(shared))
    }

    pub(crate) fn install(&self, value: T) -> u32 {
        self.value.store(owned, Ordering::Release);
        self.generation.load(Ordering::Acquire)
    }

    pub(crate) fn try_evict_at(&self, expected_gen: u32, guard: &Guard) -> EvictOutcome {
        let cas = self.generation.compare_exchange(
            expected_gen,
            next,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        let old: Shared<'_, T> = self.value.swap(Shared::null(), Ordering::AcqRel, guard);
        EvictOutcome::Evicted
    }
}
