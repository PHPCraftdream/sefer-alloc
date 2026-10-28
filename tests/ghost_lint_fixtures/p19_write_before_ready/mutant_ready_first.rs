// expect: is not preceded
fn heap_ptr_impl(_context: FallbackInitContext) -> *mut HeapCore {
    loop {
        if INIT_STATE.load(Ordering::Acquire) == FallbackInitState::Ready as u8 {
            return addr_of_mut!(FALLBACK) as *mut HeapCore;
        }
        let won = INIT_STATE
            .compare_exchange(
                FallbackInitState::Uninit as u8,
                FallbackInitState::Initializing as u8,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok();
        if won {
            // Ghost: exactly one initialiser wins per published heap.
            crate::ghost::ghost_event!("fallback/init-won", 0, 1);
            let mut init_guard = InitStateGuard::armed();
            match HeapCore::new(OWNER_ID_FALLBACK) {
                Some(hc) => {
                    INIT_STATE.store(FallbackInitState::Ready as u8, Ordering::Release);
                    unsafe { (addr_of_mut!(FALLBACK) as *mut HeapCore).write(hc) };
                    init_guard.disarm();
                    return addr_of_mut!(FALLBACK) as *mut HeapCore;
                }
                None => {
                    INIT_STATE.store(FallbackInitState::Uninit as u8, Ordering::Release);
                    init_guard.disarm();
                    return core::ptr::null_mut();
                }
            }
        }
        while INIT_STATE.load(Ordering::Acquire) == FallbackInitState::Initializing as u8 {
            spin_loop();
        }
    }
}

pub(crate) fn try_with_heap<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut HeapCore) -> R,
{
    if INIT_STATE.load(Ordering::Acquire) != FallbackInitState::Ready as u8 {
        return None;
    }
    let heap = addr_of_mut!(FALLBACK) as *mut HeapCore;
    let _guard = LockGuard::try_acquire()?;
    Some(f(unsafe { &mut *heap }))
}

impl LockGuard {
    fn acquired() -> Self {
        // Ghost: one holder of LOCK at a time; every acquire is released once.
        crate::ghost::ghost_event!("fallback/held", 0, 1);
        LockGuard
    }

    fn try_acquire() -> Option<Self> {
        LOCK.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()?;
        Some(Self::acquired())
    }

    fn acquire() -> Option<Self> {
        loop {
            if LOCK
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return Some(Self::acquired());
            }
            while LOCK.load(Ordering::Relaxed) {
                yield_now();
            }
        }
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        crate::ghost::ghost_event!("fallback/held", 0, -1);
        LOCK.store(false, Ordering::Release);
    }
}

impl Drop for InitStateGuard {
    fn drop(&mut self) {
        if self.armed {
            INIT_STATE.store(FallbackInitState::Uninit as u8, Ordering::Release);
        }
    }
}
