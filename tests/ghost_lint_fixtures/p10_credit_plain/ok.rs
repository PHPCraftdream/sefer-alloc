pub(crate) struct SegmentHeader {
    pub live_count: u32,
    pub decommitted: u32,
}

impl SegmentMeta {
    fn set_live_count(&mut self, value: u32) {
        Node::write_u32(Node::offset(self.base, off) as *mut u32, value);
    }

    pub(crate) fn inc_live(&mut self) {
        self.add_live(1);
    }

    pub(crate) fn add_live(&mut self, n: u32) {
        let v = self.live_count_of();
        self.set_live_count(v + n);
    }

    pub(crate) fn dec_live(&mut self) -> u32 {
        self.sub_live(1)
    }

    pub(crate) fn sub_live(&mut self, n: u32) -> u32 {
        let v = self.live_count_of();
        self.set_live_count(v - n);
        v - n
    }
}

impl AllocCore {
    fn try_drain_freelist_batch(&mut self, base: *mut u8, k: usize) {
        meta.add_live(k as u32);
    }

    fn try_carve_batch(&mut self, base: *mut u8, n: usize) {
        meta.add_live(n as u32);
    }

    fn pop_free(&mut self, base: *mut u8) {
        meta.inc_live();
    }

    fn carve_block(&mut self, base: *mut u8) {
        meta.inc_live();
    }

    fn flush_run(&mut self, base: *mut u8, accepted_count: usize) {
        SegmentMeta::new(base).sub_live(accepted_count as u32);
    }

    fn dealloc_small(&mut self, base: *mut u8) {
        SegmentMeta::new(base).dec_live();
    }

    pub(crate) fn reclaim_sidecar_record(base: *mut u8, offset: u32, class: u8) -> bool {
        meta.dec_live();
        true
    }
}

impl SmallSidecar {
    pub(crate) fn publish(&self, offset: u32) -> bool {
        self.bitmap().publish(offset)
    }
}
