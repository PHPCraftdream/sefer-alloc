// expect: outside a branch matching
impl Drop for EntryHandle {
    fn drop(&mut self) {
        let entry = self.entry();
        free_sidecar(entry);
        let _ = entry.refs.fetch_sub(1, Ordering::AcqRel);
    }
}
