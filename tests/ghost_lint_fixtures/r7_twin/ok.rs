#[cfg(loom)]
impl Cell {
    fn get(&self) -> usize {
        self.ptr.load(Ordering::Acquire)
    }
}
#[cfg(not(loom))]
impl Cell {
    fn get(&self) -> usize {
        self.ptr.load(Ordering::Acquire)
    }
}
