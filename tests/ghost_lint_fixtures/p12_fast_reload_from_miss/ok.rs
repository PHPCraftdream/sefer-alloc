impl HeapCore {
    fn refill_magazine_miss(&mut self, c: usize) -> *mut u8 {
        if let Some(depot) = self.depot.as_deref_mut() {
            let class = &mut self.tcache.classes[c];
            if let Some(more) = depot.ovf.reload_simple(c, &mut class.slots) {
                class.ovf = more as u8;
                return self.finish_magazine_refill(c, TCACHE_CAP);
            }
        }
        self.refill_magazine_slow(c)
    }
    pub(crate) fn flush_all_tcache(&mut self) {
        loop {
            if !self.depot_reload(c) {
                break;
            }
        }
    }
}
