// expect: is not preceded
fn finish_bind(heap: *mut HeapCore, lease: HeapLease) -> *mut HeapCore {
    if LOCAL.try_with(|c| c.set(heap)).is_err() {
        drop(lease);
        return core::ptr::null_mut();
    }
    let armed = GUARD.try_with(|g| g.arm(lease)).is_ok();
    if !armed {
        drop(lease);
        let _ = LOCAL.try_with(|c| c.set(core::ptr::null_mut()));
        return core::ptr::null_mut();
    }
    heap
}
