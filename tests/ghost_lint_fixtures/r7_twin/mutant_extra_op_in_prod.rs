// expect: twins differ in length
#[cfg(loom)]
impl Cell {
    fn get(&self) -> usize {
        self.ptr.load(Ordering::Acquire)
    }
}
#[cfg(not(loom))]
impl Cell {
    fn get(&self) -> usize {
        self.hits.fetch_add(1, Ordering::Relaxed);
        self.ptr.load(Ordering::Acquire)
    }
}
