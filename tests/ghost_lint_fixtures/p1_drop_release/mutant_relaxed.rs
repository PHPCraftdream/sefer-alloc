// expect: rule requires ["Release", "Relaxed"]
impl Drop for HeapLease {
    fn drop(&mut self) {
        if std::thread::panicking() {
            fatal(b"claim: heap lease dropped while panicking\n");
        }
        if self
            .slot
            .cas_state(
                SlotState::Live,
                SlotState::Free,
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .is_err()
        {
            fatal(b"claim: heap lease free rollback cas lost\n");
        }
        let prev = reg.reuse_hint.swap(self.index, Ordering::Relaxed);
    }
}
