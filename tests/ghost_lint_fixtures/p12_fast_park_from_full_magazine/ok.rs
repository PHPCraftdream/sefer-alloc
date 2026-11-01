impl HeapCore {
    fn free_magazine_park(&mut self, c: usize, cnt: usize, off: u32, root: *mut u8, block: *mut u8) {
        if let Some(depot) = self.depot.as_deref_mut() {
            let class = &mut self.tcache.classes[c];
            if depot.ovf.park_simple(c, &class.slots, budget) {
                class.ovf = 1;
                return;
            }
        }
        self.free_magazine_full(c, cnt, off, root, block);
    }
    pub(crate) fn flush_all_tcache(&mut self) {
        loop {
            if !self.depot_reload(c) {
                break;
            }
        }
    }
}
