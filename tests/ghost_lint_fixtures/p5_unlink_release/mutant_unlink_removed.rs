// expect: expected 1 matching site(s), found 0
impl RouteDirectory {
    fn remove(&self, entry: &EntryHandle) -> bool {
        let key = entry.key();
        let Some(mut guard) = self.lock(key) else {
            return false;
        };
        let _retired = guard.remove(key, entry.ptr());
        drop(guard);
        true
    }
}
