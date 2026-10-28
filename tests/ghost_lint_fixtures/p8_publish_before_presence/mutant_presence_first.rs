// expect: is not preceded
impl ClassLeaves {
    pub(crate) fn prepared(&self, granule: usize, class: u8) -> bool {
        self.dense[granule].load(Ordering::Acquire) == class + 1
    }

    pub(crate) fn prepare(&self, granule: usize, class: u8) -> bool {
        self.promote(granule / LEAF_GRANULES)
    }

    fn promote(&self, leaf: usize) -> bool {
        let fresh = unsafe { System.alloc(layout) }.cast::<AtomicU8>();
        #[cfg(not(loom))]
        unsafe {
            core::ptr::write_bytes(fresh, 0, LEAF_GRANULES)
        };
        self.present[leaf / 64].fetch_or(1u64 << (leaf % 64), Ordering::Release);
        self.mixed[leaf].store(fresh, Ordering::Release);
        true
    }

    pub(crate) fn issue(&self, granule: usize, class: u8) -> bool {
        let mixed = self.mixed[leaf].load(Ordering::Acquire);
        unsafe { &*mixed.add(granule % LEAF_GRANULES) }.store(class + 1, Ordering::Release);
        true
    }

    pub(crate) fn encoded(&self, granule: usize) -> u8 {
        let mixed = self.mixed[leaf].load(Ordering::Acquire);
        unsafe { &*mixed.add(granule % LEAF_GRANULES) }.load(Ordering::Acquire)
    }
}

impl Drop for ClassLeaves {
    fn drop(&mut self) {
        for (word, bits) in self.present.iter().enumerate() {
            let mut bits = bits.load(Ordering::Relaxed);
            while bits != 0 {
                let ptr = self.mixed[leaf].load(Ordering::Relaxed);
                unsafe { System.dealloc(ptr.cast(), layout) };
            }
        }
        #[cfg(debug_assertions)]
        for (leaf, slot) in self.mixed.iter().enumerate() {
            let present = self.present[leaf / 64].load(Ordering::Relaxed) != 0;
            debug_assert_eq!(present, !slot.load(Ordering::Relaxed).is_null());
        }
    }
}
