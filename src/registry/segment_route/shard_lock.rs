//! Non-allocating spin lock for the route directory shards.
//!
//! `std::sync::Mutex` boxes a pthread mutex on first lock on some targets
//! (macOS), which re-enters the installed global allocator while a heap is
//! still being constructed. This lock never allocates.
#![allow(unsafe_code)]

use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

const SPINS_BEFORE_YIELD: u32 = 64;

pub(super) struct ShardLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

// SAFETY: access to `value` is serialized by `locked`; `T: Send` lets the
// guard hand `&mut T` to whichever thread holds the lock.
unsafe impl<T: Send> Sync for ShardLock<T> {}
// SAFETY: moving the lock moves the owned `T`.
unsafe impl<T: Send> Send for ShardLock<T> {}

/// Guard for one acquired [`ShardLock`]: exclusive payload access on the
/// single thread that holds it.
///
/// Auto traits, pinned by the `_unique` marker: `Send` requires only
/// `T: Send`; `Sync` additionally requires `T: Sync`, because [`Deref`]
/// hands `&T` to every thread sharing the guard. [`ShardLock`] itself stays
/// `Sync` under `T: Send` alone.
pub(super) struct ShardGuard<'a, T> {
    lock: &'a ShardLock<T>,
    // `&'a mut T` is `Send` iff `T: Send` and `Sync` iff `T: Sync` —
    // exactly the guard's access contract (R13-05).
    _unique: PhantomData<&'a mut T>,
}

impl<T> ShardLock<T> {
    pub(super) const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    pub(super) fn lock(&self) -> ShardGuard<'_, T> {
        let mut spins = 0u32;
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            if spins < SPINS_BEFORE_YIELD {
                spins += 1;
                core::hint::spin_loop();
            } else {
                std::thread::yield_now();
            }
        }
        ShardGuard {
            lock: self,
            _unique: PhantomData,
        }
    }
}

impl<T> Deref for ShardGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the guard holds the lock.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for ShardGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the guard holds the lock exclusively.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for ShardGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}
