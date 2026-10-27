// expect: 2 fns match Lease::drop
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = self.slot.cas_state(LIVE, FREE, Ordering::Release, Ordering::Relaxed);
    }
}
impl Lease {
    fn drop(&mut self) {}
}
