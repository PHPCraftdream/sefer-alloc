// expect: only ["HeapCore::depot_segment_tick", "HeapCore::park_overflow", "HeapCore::reload_overflow_tick"]
impl HeapCore {
    fn refill_magazine_slow(&mut self, c: usize) -> *mut u8 {
        if self.depot_may_reload(c) && self.depot_reload_tick(c) {
            return self.finish_magazine_refill(c, TCACHE_CAP);
        }
        if self.depot_may_tick(c) {
            self.depot_segment_tick(c);
        }
        self.refill(c)
    }
    fn depot_segment_tick(&mut self, c: usize) {
        let Some(depot) = self.depot.as_deref_mut() else {
            return;
        };
        if depot.ovf.bytes() == 0 {
            depot.ovf.disarm(c);
            return;
        }
        self.depot_tick();
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
            self.depot_tick();
            if !self.depot_reload(c) {
                break;
            }
        }
        self.depot_release_overflow_pages();
    }
}
