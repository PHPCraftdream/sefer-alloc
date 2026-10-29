impl HeapCore {
    pub(crate) fn flush_all_tcache(&mut self) {
        for c in 0..SMALL_CLASS_COUNT {
            loop {
                let n = self.tcache.classes[c].count as usize;
                if n != 0 {
                    unsafe {
                        self.core
                            .flush_class_internal(c, &self.tcache.classes[c].slots[0..n])
                    };
                    self.tcache.classes[c].count = 0;
                }
                if !self.depot_reload(c) {
                    break;
                }
            }
        }
        self.depot_release_overflow_pages();
    }
}
