impl Drop for Lease {
    fn drop(&mut self) {
        let _ = self.slot.cas_state(LIVE, FREE, Ordering::Release, Ordering::Relaxed);
    }
}
