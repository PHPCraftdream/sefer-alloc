impl DepotOverflow {
    pub(crate) fn release_pages(&mut self) {
        if !self.active() {
            self.reserve_failed = false;
            return;
        }
        let mut c = 0;
        while c < DEPOT_CLASSES {
            let have = self.committed[c] as usize;
            if have != 0 {
                let off = self.off[c] as usize;
                crate::alloc_core::os::decommit_depot_overflow(self.base, off, off + have);
            }
            c += 1;
        }
        self.reset_rings();
    }
}
