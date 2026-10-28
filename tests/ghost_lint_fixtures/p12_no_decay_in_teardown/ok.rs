impl HeapCore {
    fn refill_magazine_slow(&mut self, c: usize) -> *mut u8 {
        if self.tcache.classes[c].parked != 0 && self.depot_reload(c) {
            self.depot_tick();
            return self.finish_magazine_refill(c, TCACHE_CAP);
        }
        self.depot_tick();
        self.refill(c)
    }
    fn free_magazine_full(&mut self, c: usize, block: *mut u8) {
        if self.depot_may_park(c) && self.depot_park(c) {
            self.depot_tick();
        }
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
