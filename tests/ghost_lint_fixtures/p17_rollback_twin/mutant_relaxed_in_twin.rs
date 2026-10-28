// expect: twin
impl Shim {
    fn drop(&mut self) {
        if !self.defused {
            self.ptr.store(core::ptr::null_mut(), Ordering::Relaxed);
        }
    }
}

impl Real {
    fn drop(&mut self) {
        if !self.defused {
            self.ptr.store(core::ptr::null_mut(), Ordering::Release);
        }
    }
}
