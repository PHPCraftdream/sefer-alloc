// expect: is not preceded
impl Drop for LeaseReleaseGuard {
    fn drop(&mut self) {
        let Some(mut lease) = self.lease.take() else {
            return;
        };
        mark_local_torn();
        drop(lease);
        lease.core().trim_for_recycle();
        drop(lease);
    }
}
