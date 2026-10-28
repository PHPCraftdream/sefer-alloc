impl StackHead {
    #[cfg(not(loom))]
    pub const fn new() -> Self {
        Self { head: Atomic::new(0) }
    }

    #[cfg(loom)]
    pub fn new() -> Self {
        Self { head: Atomic::new(0) }
    }
}
