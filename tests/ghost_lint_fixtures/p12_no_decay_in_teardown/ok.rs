impl HeapCore {
    fn refill_magazine_slow(&mut self, c: usize) -> *mut u8 {
        if self.depot_may_reload(c) && self.depot_reload_tick(c) {
            return self.finish_magazine_refill(c, TCACHE_CAP);
        }
        self.depot_tick();
        self.refill(c)
    }
    fn park_overflow(&mut self, c: usize) -> bool {
        self.tcache.classes[c].count = 0;
        self.depot_tick();
        true
    }
    fn reload_overflow_tick(&mut self, c: usize) -> bool {
        if !self.reload_overflow(c) {
            return false;
        }
        self.depot_tick();
        true
    }
    pub(crate) fn flush_all_tcache(&mut self) {
        loop {
            if !self.depot_reload(c) {
                break;
            }
        }
        self.depot_release_overflow_pages();
    }
}
