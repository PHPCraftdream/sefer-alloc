impl HeapCore {
    fn refill_magazine_slow(&mut self, c: usize) -> *mut u8 {
        if self.depot_may_reload(c) && self.depot_reload_tick(c) {
            return self.finish_magazine_refill(c, TCACHE_CAP);
        }
        self.refill(c)
    }
    fn refill_magazine_slow_virgin(&mut self, c: usize) -> (*mut u8, bool) {
        if self.depot_may_reload(c) && self.depot_reload_tick(c) {
            return (self.finish_magazine_refill(c, TCACHE_CAP), false);
        }
        self.refill_virgin(c)
    }
    pub(crate) fn flush_all_tcache(&mut self) {
        loop {
            if !self.depot_reload(c) {
                break;
            }
        }
    }
}
