impl SegmentIngress {
    pub(crate) fn after_publish<'h>(&self, heap: impl FnOnce() -> Option<&'h HeapIngress>) {
        #[cfg(loom)]
        fence(Ordering::SeqCst);
        if self.state.load(Ordering::SeqCst) == ARMED {
            self.notify(heap());
        }
    }

    fn notify(&self, heap: Option<&HeapIngress>) {
        if self
            .state
            .compare_exchange(ARMED, NOTIFIED, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        heap.unwrap().notify(self.slot);
    }

    pub(crate) fn arm(&self) {
        self.state.store(ARMED, Ordering::Release);
        fence(Ordering::SeqCst);
    }
}
