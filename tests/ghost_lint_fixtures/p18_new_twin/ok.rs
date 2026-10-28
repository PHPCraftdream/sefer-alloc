impl OncePtrCell {
    #[cfg(not(loom))]
    pub const fn new() -> Self {
        Self { ptr: Atomic::new(0) }
    }

    #[cfg(loom)]
    pub fn new() -> Self {
        Self { ptr: Atomic::new(0) }
    }
}
