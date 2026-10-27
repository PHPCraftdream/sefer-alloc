// expect: thread_local is false
thread_local! {

    /// The thread-exit lease-release guard.
    static GUARD: LeaseReleaseGuard = const { LeaseReleaseGuard::new() };
}

struct LeaseReleaseGuard {
    /// The typed lease.
    lease: Cell<Option<HeapLease>>,
}

static LOCAL: Cell<*mut HeapCore> = const { Cell::new(core::ptr::null_mut()) };
