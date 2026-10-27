// expect: rule requires ["Release"]
impl RouteDirectory {
    fn remove(&self, entry: &EntryHandle) -> bool {
        let key = entry.key();
        let Some(mut guard) = self.lock(key) else {
            return false;
        };
        let _retired = guard.remove(key, entry.ptr());
        entry.entry().linked.store(false, Ordering::Relaxed);
        drop(guard);
        true
    }
}
