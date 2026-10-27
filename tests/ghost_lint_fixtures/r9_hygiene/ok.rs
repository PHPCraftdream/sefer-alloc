struct Entry {
    refs: AtomicUsize,
}
impl Entry {
    fn release(&self) {
        let prev = self.refs.fetch_sub(1, Ordering::AcqRel);
        crate::ghost::ghost_assert!("route-pin/last-ref-free", prev >= 1 && self.refs.load(Ordering::Relaxed) < usize::MAX);
    }
}
#[cfg(sefer_ghost)]
fn harness_reset() {
    crate::ghost::reset();
}
