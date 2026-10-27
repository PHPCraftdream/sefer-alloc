// expect: dominating position
impl Core {
    fn trim_for_recycle(&mut self, slot: &Slot) {
        let _ = slot.cas_state(LIVE, FREE, Ordering::Release, Ordering::Relaxed);
        self.flush_all_tcache();
    }
}
