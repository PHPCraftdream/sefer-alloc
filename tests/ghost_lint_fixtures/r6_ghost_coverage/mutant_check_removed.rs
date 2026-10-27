// expect: no ghost_assert!/ghost_event! with id route-pin/last-ref-free
impl Drop for EntryHandle {
    fn drop(&mut self) {
        let entry = self.entry();
        if entry.refs.fetch_sub(1, Ordering::AcqRel) == 1 {
            free_sidecar(entry);
        }
    }
}
