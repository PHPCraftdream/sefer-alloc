impl Core {
    fn trim_for_recycle(&mut self, slot: &Slot) {
        self.flush_all_tcache();
        if self.torn {
            let _ = slot.cas_state(LIVE, FREE, Ordering::Release, Ordering::Relaxed);
        }
    }
}
