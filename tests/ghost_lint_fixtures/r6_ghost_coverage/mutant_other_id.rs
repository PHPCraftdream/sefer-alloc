// expect: no ghost_assert!/ghost_event! with id route-pin/last-ref-free
impl Drop for EntryHandle {
    fn drop(&mut self) {
        let entry = self.entry();
        let prev = entry.refs.fetch_sub(1, Ordering::AcqRel);
        crate::ghost::ghost_assert!("route-pin/other", prev >= 1);
        if prev == 1 {
            free_sidecar(entry);
        }
    }
}
