impl Shim {
    fn init(&self) -> Option<NonNull<T>> {
        loop {
            let p = self.ptr.load(Ordering::Acquire);
            match self.ptr.compare_exchange(null, sentinel, Ordering::Acquire, Ordering::Relaxed) {
                Ok(_) => match init() {
                    Some(ptr) => {
                        self.ptr.store(raw, Ordering::Release);
                        return Some(ptr);
                    }
                    None => {
                        self.ptr.store(null, Ordering::Release);
                        return None;
                    }
                },
                Err(_) => loop {
                    let p = self.ptr.load(Ordering::Acquire);
                    break;
                },
            }
        }
    }
}

impl Real {
    fn init(&self) -> Option<NonNull<T>> {
        loop {
            let p = self.ptr.load(Ordering::Acquire);
            match self.ptr.compare_exchange(null, sentinel, Ordering::Acquire, Ordering::Relaxed) {
                Ok(_) => match init() {
                    Some(ptr) => {
                        self.ptr.store(raw, Ordering::Release);
                        return Some(ptr);
                    }
                    None => {
                        self.ptr.store(null, Ordering::Release);
                        return None;
                    }
                },
                Err(_) => loop {
                    let p = self.ptr.load(Ordering::Acquire);
                    break;
                },
            }
        }
    }
}
