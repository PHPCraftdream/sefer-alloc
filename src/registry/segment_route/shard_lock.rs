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
/// Cap of the exponential `spin_loop()` backoff: `1 << BACKOFF_MAX_PAUSES_LOG2`
/// pauses per TTAS read at most, then the waiter starts yielding.
const BACKOFF_MAX_PAUSES_LOG2: u32 = 6;
/// The process-exit probe (an FFI call on Windows) is checked only once per
/// `EXIT_PROBE_YIELDS` yield rounds, never on every wait iteration. Exit
/// refusal is delayed by at most that many scheduler yields.
const EXIT_PROBE_YIELDS: u32 = 64;

/// Bounded exponential backoff: `spin_loop()` `1 << min(shift, cap)` times.
/// Pure latency device; correctness never depends on it.
fn spin_backoff(shift: u32) {
    for _ in 0..(1u32 << shift.min(BACKOFF_MAX_PAUSES_LOG2)) {
        core::hint::spin_loop();
    }
}

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

    #[allow(dead_code)] // Standalone path-included auto-trait tests use only lock().
    pub(super) fn lock_unless_exiting(
        &self,
        exiting: impl Fn() -> bool,
        giveup: impl Fn(),
    ) -> Option<ShardGuard<'_, T>> {
        let mut spins = 0u32;
        let mut yield_rounds = 0u32;
        loop {
            // Fast path and retry: a single CAS; no read precedes the first
            // attempt, so the uncontended path is unchanged.
            if self
                .locked
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return Some(ShardGuard {
                    lock: self,
                    _unique: PhantomData,
                });
            }
            // TTAS: read-only spin while the lock looks held — the cache line
            // stays shared instead of ping-ponging RMW between waiters.
            while self.locked.load(Ordering::Relaxed) {
                if spins < SPINS_BEFORE_YIELD {
                    spins += 1;
                    spin_backoff(spins);
                } else {
                    yield_rounds = yield_rounds.wrapping_add(1);
                    if yield_rounds & (EXIT_PROBE_YIELDS - 1) == 0 && exiting() {
                        giveup();
                        return None;
                    }
                    std::thread::yield_now();
                }
            }
        }
    }

    // R6-T16: its compiled lib consumers are the bench-gated directory
    // probes; the standalone auto-trait tests include this file in their own
    // crate and do not lint this one.
    #[cfg_attr(not(feature = "bench-internals"), allow(dead_code))]
    pub(super) fn lock(&self) -> ShardGuard<'_, T> {
        let mut spins = 0u32;
        loop {
            if self
                .locked
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return ShardGuard {
                    lock: self,
                    _unique: PhantomData,
                };
            }
            while self.locked.load(Ordering::Relaxed) {
                if spins < SPINS_BEFORE_YIELD {
                    spins += 1;
                    spin_backoff(spins);
                } else {
                    std::thread::yield_now();
                }
            }
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
