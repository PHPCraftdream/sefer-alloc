// expect: does not match
thread_local! {
    /// The cached raw pointer to this thread's heap.
    static LOCAL: Cell<*mut HeapCore> = const { Cell::new(core::ptr::null_mut()) };

    /// The thread-exit lease-release guard.
    static GUARD: LeaseReleaseGuard = const { LeaseReleaseGuard::new() };
}

struct LeaseReleaseGuard {
    /// The typed lease.
    lease: Option<HeapLease>,
}
