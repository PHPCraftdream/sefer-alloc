// expect: ghost macro argument writes
impl Entry {
    fn release(&self) {
        crate::ghost::ghost_assert!("route-pin/last-ref-free", self.refs.fetch_sub(1, Ordering::AcqRel) >= 1);
    }
}
