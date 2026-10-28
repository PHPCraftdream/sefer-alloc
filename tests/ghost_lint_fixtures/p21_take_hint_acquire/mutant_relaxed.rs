// expect: rule requires
impl ExactShard {
    fn acquire_unless_exiting(&self) -> bool {
        let mut spins = 0u32;
        while self
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            yield_now();
        }
        // Ghost: one holder per lock word; every acquire is released once.
        crate::ghost::ghost_event!("exact-shard/held", core::ptr::from_ref(self).addr(), 1);
        true
    }

    fn release(&self) {
        crate::ghost::ghost_event!("exact-shard/held", core::ptr::from_ref(self).addr(), -1);
        self.lock.store(false, Ordering::Release);
    }

    pub(super) fn insert(&self, addr: usize, size: usize) -> InsertOutcome {
        if !self.acquire_unless_exiting() {
            return InsertOutcome::Exiting;
        }
        let inner = unsafe { &mut *self.inner.get() };
        let (out, retired) = inner.insert(Self::hash(addr), addr);
        self.live_hint.store(inner.live, Ordering::Release);
        self.release();
        out
    }

    pub(super) fn peek(&self, addr: usize) -> Option<usize> {
        if self.live_hint.load(Ordering::Acquire) == 0 {
            return None;
        }
        if !self.acquire_unless_exiting() {
            return None;
        }
        let inner = unsafe { &*self.inner.get() };
        let r = inner.find(Self::hash(addr), addr);
        self.release();
        r
    }

    pub(super) fn take(&self, addr: usize) -> Option<usize> {
        if self.live_hint.load(Ordering::Relaxed) == 0 {
            return None;
        }
        if !self.acquire_unless_exiting() {
            return None;
        }
        let inner = unsafe { &mut *self.inner.get() };
        let r = inner.find(Self::hash(addr), addr);
        self.live_hint.store(inner.live, Ordering::Release);
        self.release();
        r
    }
}
