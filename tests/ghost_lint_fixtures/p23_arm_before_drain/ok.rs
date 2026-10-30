impl AllocCore {
    fn drain_notified_slot(&mut self, i: usize, class_idx: usize) -> Option<*mut u8> {
        let base = self.table.base_at(i);
        if base.is_null() {
            return None;
        }
        self.table.arm_small_route(i, base);
        match self.drain_segment_sidecar(base) {
            _ => {}
        }
        (SegmentMeta::new(base).bin_table().head(class_idx) != FREE_LIST_NULL).then_some(base)
    }
}
