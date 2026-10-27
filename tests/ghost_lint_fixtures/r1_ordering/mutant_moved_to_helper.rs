// expect: expected 1 matching site(s), found 0
impl Drop for Lease {
    fn drop(&mut self) {
        self.release();
    }
}
impl Lease {
    fn release(&mut self) {
        let _ = self.slot.cas_state(LIVE, FREE, Ordering::Release, Ordering::Relaxed);
    }
}
