// expect: twin
impl OncePtrCell {
    #[cfg(not(loom))]
    pub const fn new() -> Self {
        Self { ptr: Atomic::new(0) }
    }

    #[cfg(loom)]
    pub fn new() -> Self {
        let s = Self { ptr: Atomic::new(0) };
        s.ptr.store(0, Ordering::Relaxed);
        s
    }
}
