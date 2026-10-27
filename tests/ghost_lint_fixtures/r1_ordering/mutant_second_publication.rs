// expect: expected 1 matching site(s), found 2
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = self.slot.cas_state(LIVE, FREE, Ordering::Release, Ordering::Relaxed);
        let _ = self.slot.cas_state(FREE, FREE, Ordering::Release, Ordering::Relaxed);
    }
}
