// expect: rule requires ["Release", "Relaxed"]
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = self.slot.cas_state(LIVE, FREE, Ordering::Relaxed, Ordering::Relaxed);
    }
}
