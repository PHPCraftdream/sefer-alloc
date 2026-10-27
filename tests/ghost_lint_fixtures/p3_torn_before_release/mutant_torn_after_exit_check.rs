// expect: is not preceded
impl Drop for LeaseReleaseGuard {
    fn drop(&mut self) {
        let Some(mut lease) = self.lease.take() else {
            return;
        };
        if super::process_exit_in_progress() {
            drop(lease);
            return;
        }
        mark_local_torn();
        lease.core().trim_for_recycle();
        drop(lease);
    }
}
