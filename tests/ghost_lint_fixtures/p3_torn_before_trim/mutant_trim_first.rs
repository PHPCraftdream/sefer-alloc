// expect: is not preceded
impl Drop for LeaseReleaseGuard {
    fn drop(&mut self) {
        let Some(mut lease) = self.lease.take() else {
            return;
        };
        lease.core().trim_for_recycle();
        mark_local_torn();
        if super::process_exit_in_progress() {
            drop(lease);
            return;
        }
        drop(lease);
    }
}
