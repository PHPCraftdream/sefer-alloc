// expect: no ghost_assert!/ghost_event! with id route-pin/no-resurrection
impl EntryHandle {
    fn pin_from_array(ptr: *mut Entry) -> Self {
        let entry = unsafe { &*ptr };
        match entry
            .refs
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
        {
            Ok(_) => {}
            Err(_) => fatal(b"directory: pin reference count overflow\n"),
        }
        Self { ptr: AtomicPtr::new(ptr) }
    }
}
