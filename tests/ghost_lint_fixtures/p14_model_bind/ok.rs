// ghost-bind: epoch/evict-cas
const EVICT_CAS: (Ordering, Ordering) = (Ordering::AcqRel, Ordering::Acquire);

impl Slot {
    fn install(&self, value: usize) {
        // ghost-bind: epoch/install-release
        self.value.store(value, Ordering::Release);
    }
    fn evict(&self, expected_gen: u64) {
        if self.generation.compare_exchange(expected_gen, expected_gen + 1, EVICT_CAS.0, EVICT_CAS.1).is_ok() {
            // ghost-bind: epoch/evict-swap
            self.value.swap(VACANT, Ordering::AcqRel);
        }
    }
    fn read_with(&self, expected_gen: u64) -> Option<usize> {
        // ghost-bind: epoch/read-generation
        let g1 = self.generation.load(Ordering::Acquire);
        // ghost-bind: epoch/read-value
        let v = self.value.load(Ordering::Acquire);
        // ghost-bind: epoch/read-generation
        let g2 = self.generation.load(Ordering::Acquire);
        Some(v)
    }
}
