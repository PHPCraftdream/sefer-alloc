// expect: outside a branch matching
impl Drop for EntryHandle {
    fn drop(&mut self) {
        let entry = self.entry();
        if entry.refs.fetch_sub(1, Ordering::AcqRel) == 2 {
            free_sidecar(entry);
        }
    }
}
