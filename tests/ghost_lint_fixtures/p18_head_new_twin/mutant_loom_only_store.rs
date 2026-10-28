// expect: twin
impl StackHead {
    #[cfg(not(loom))]
    pub const fn new() -> Self {
        Self { head: Atomic::new(0) }
    }

    #[cfg(loom)]
    pub fn new() -> Self {
        let s = Self { head: Atomic::new(0) };
        s.head.store(0, Ordering::Relaxed);
        s
    }
}
