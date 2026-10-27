// expect: twins differ at op 0
#[cfg(loom)]
impl Cell {
    fn get(&self) -> usize {
        self.ptr.load(Ordering::Relaxed)
    }
}
#[cfg(not(loom))]
impl Cell {
    fn get(&self) -> usize {
        self.ptr.load(Ordering::Acquire)
    }
}
