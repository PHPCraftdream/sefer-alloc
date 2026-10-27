// expect: only ["HeapCore::free_magazine_full", "HeapCore::flush_all_tcache"] may do this
impl HeapCore {
    fn free_magazine_full(&mut self, c: usize) {
        unsafe { self.core.flush_magazine_internal(c, &self.tcache.classes[c].slots[0..FLUSH_N]) };
    }
    pub(crate) fn flush_all_tcache(&mut self) {
        let n = self.tcache.classes[0].count as usize;
        unsafe { self.core.flush_magazine_internal(0, &self.tcache.classes[0].slots[0..n]) };
    }
    pub fn dealloc_batch(&mut self, c: usize) {
        unsafe { self.core.flush_magazine_internal(c, &stage[..staged]) };
    }
}
