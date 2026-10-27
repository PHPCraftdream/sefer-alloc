// expect: rule requires ["AcqRel", "Acquire"]
impl EntryHandle {
    fn pin_from_array(ptr: *mut Entry) -> Self {
        let entry = unsafe { &*ptr };
        match entry
            .refs
            .fetch_update(Ordering::Relaxed, Ordering::Acquire, |n| n.checked_add(1))
        {
            Ok(_old) => crate::ghost::ghost_assert!("route-pin/no-resurrection", _old != 0, "pin on a dead entry"),
            Err(_) => fatal(b"directory: pin reference count overflow\n"),
        }
        Self { ptr: AtomicPtr::new(ptr) }
    }
}
