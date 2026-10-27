// expect: outside a branch
impl Drop for LeaseReleaseGuard {
    fn drop(&mut self) {
        let Some(mut lease) = self.lease.take() else {
            return;
        };
        mark_local_torn();
        if lease.slot_index() == 0 {
            drop(lease);
            return;
        }
        lease.core().trim_for_recycle();
        drop(lease);
    }
}
