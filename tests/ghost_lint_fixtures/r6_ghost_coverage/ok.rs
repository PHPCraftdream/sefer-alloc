impl Drop for EntryHandle {
    fn drop(&mut self) {
        let entry = self.entry();
        let prev = entry.refs.fetch_sub(1, Ordering::AcqRel);
        crate::ghost::ghost_assert!("route-pin/last-ref-free", prev >= 1, "refs underflow");
        if prev == 1 {
            free_sidecar(entry);
        }
    }
}
