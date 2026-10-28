impl Shim {
    fn get(&self) -> Option<NonNull<T>> {
        let p = self.ptr.load(Ordering::Acquire);
        if Self::is_ready(p) { Some(p) } else { None }
    }
}

impl Real {
    fn get(&self) -> Option<NonNull<T>> {
        let p = self.ptr.load(Ordering::Acquire);
        if Self::is_ready(p) { Some(p) } else { None }
    }
}
