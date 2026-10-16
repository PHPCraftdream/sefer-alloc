// Seam: the descriptor array lives in raw `System` memory (never the global
// allocator: re-entrancy) and is accessed through raw pointers under the
// shard spinlock.
#![allow(unsafe_code)]

use super::insert_outcome::InsertOutcome;
use crate::alloc_core::fatal::fatal;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::alloc::{GlobalAlloc, Layout, System};

const EMPTY: usize = 0;
const TOMB: usize = usize::MAX;
const INIT_CAP: usize = 64;

#[repr(C)]
#[derive(Clone, Copy)]
struct Slot {
    addr: usize,
    size: usize,
    align: usize,
    generation: u64,
}

/// Open-addressed (linear probe, tombstones) array; `cap` is a power of two.
struct Inner {
    slots: *mut Slot,
    cap: usize,
    /// Live + tombstone slots.
    used: usize,
    live: usize,
}

fn array_layout(cap: usize) -> Layout {
    // cap is a small power of two; Slot is 32 bytes, align 8.
    Layout::from_size_align(cap * core::mem::size_of::<Slot>(), 8)
        .unwrap_or_else(|_| fatal(b"exact-shard: descriptor layout overflow\n"))
}

impl Inner {
    /// First probe index for hash `h` (low 6 bits pick the shard).
    fn start(&self, h: u64) -> usize {
        ((h >> 6) as usize) & (self.cap - 1)
    }

    /// Rebuild into a fresh array (tombstones dropped). Fallible. On success
    /// returns the retired old array to free AFTER the shard lock is released
    /// (no `System.dealloc` under the lock).
    fn rehash(&mut self) -> Result<Option<(*mut Slot, usize)>, ()> {
        let new_cap = if self.cap == 0 {
            INIT_CAP
        } else if self.live * 2 >= self.cap {
            self.cap * 2
        } else {
            self.cap
        };
        // SAFETY: non-zero size layout; zeroed memory is valid `EMPTY` slots.
        let fresh = unsafe { System.alloc_zeroed(array_layout(new_cap)) } as *mut Slot;
        if fresh.is_null() {
            return Err(());
        }
        let old = self.slots;
        let old_cap = self.cap;
        self.slots = fresh;
        self.cap = new_cap;
        self.used = 0;
        let mut i = 0;
        while i < old_cap {
            // SAFETY: i < old_cap within the old array.
            let s = unsafe { old.add(i).read() };
            if s.addr != EMPTY && s.addr != TOMB {
                self.place(ExactShard::hash(s.addr), s);
            }
            i += 1;
        }
        if !old.is_null() {
            return Ok(Some((old, old_cap)));
        }
        Ok(None)
    }

    /// Place a known-unique slot into the first free position.
    fn place(&mut self, h: u64, s: Slot) {
        let mask = self.cap - 1;
        let mut i = self.start(h);
        loop {
            // SAFETY: i <= mask < cap.
            let cur = unsafe { self.slots.add(i).read() };
            if cur.addr == EMPTY || cur.addr == TOMB {
                if cur.addr == EMPTY {
                    self.used += 1;
                }
                // SAFETY: as above.
                unsafe { self.slots.add(i).write(s) };
                return;
            }
            i = (i + 1) & mask;
        }
    }

    fn insert(&mut self, h: u64, s: Slot) -> (InsertOutcome, Option<(*mut Slot, usize)>) {
        let mut retired = None;
        if (self.used + 1) * 4 > self.cap * 3 {
            match self.rehash() {
                Ok(old) => retired = old,
                Err(()) => return (InsertOutcome::Oom, None),
            }
        }
        let mask = self.cap - 1;
        let mut i = self.start(h);
        let mut first_free = usize::MAX;
        loop {
            // SAFETY: i <= mask < cap.
            let cur = unsafe { self.slots.add(i).read() };
            if cur.addr == EMPTY {
                let at = if first_free == usize::MAX {
                    self.used += 1;
                    i
                } else {
                    first_free
                };
                // SAFETY: `at` is a probed in-bounds index.
                unsafe { self.slots.add(at).write(s) };
                self.live += 1;
                return (InsertOutcome::Inserted, retired);
            }
            if cur.addr == TOMB {
                if first_free == usize::MAX {
                    first_free = i;
                }
            } else if cur.addr == s.addr {
                return (InsertOutcome::Duplicate, retired);
            }
            i = (i + 1) & mask;
        }
    }

    /// Index of the live slot for `addr`.
    fn find(&self, h: u64, addr: usize) -> Option<usize> {
        if self.cap == 0 {
            return None;
        }
        let mask = self.cap - 1;
        let mut i = self.start(h);
        loop {
            // SAFETY: i <= mask < cap.
            let cur = unsafe { self.slots.add(i).read() };
            if cur.addr == EMPTY {
                return None;
            }
            if cur.addr == addr {
                return Some(i);
            }
            i = (i + 1) & mask;
        }
    }
}

/// One shard: spinlock + descriptor array. `live_hint` allows a lock-free
/// "shard is empty" answer, sound because an address being freed was
/// registered before the free (happens-before through the caller's own
/// hand-off of the pointer).
pub(super) struct ExactShard {
    lock: AtomicBool,
    live_hint: AtomicUsize,
    inner: UnsafeCell<Inner>,
}

// SAFETY: `inner` is only touched while `lock` is held.
unsafe impl Sync for ExactShard {}

impl ExactShard {
    pub(super) const fn new() -> Self {
        Self {
            lock: AtomicBool::new(false),
            live_hint: AtomicUsize::new(0),
            inner: UnsafeCell::new(Inner {
                slots: core::ptr::null_mut(),
                cap: 0,
                used: 0,
                live: 0,
            }),
        }
    }

    /// Hash of an exact address (low 6 bits: shard, rest: probe start).
    pub(super) fn hash(addr: usize) -> u64 {
        ((addr as u64) >> 3).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 24
    }

    /// Unconditional spin acquire; only the test hook below holds the lock
    /// this way (production paths use `acquire_unless_exiting`).
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    fn acquire(&self) {
        while self
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
    }

    /// Spin, then `yield_now`, then refuse once the process is exiting: a
    /// holder killed by `ExitProcess` never releases the lock.
    fn acquire_unless_exiting(&self) -> bool {
        let mut spins = 0u32;
        while self
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            if spins < 64 {
                spins += 1;
                core::hint::spin_loop();
            } else if crate::global::process_exit_in_progress() {
                crate::global::fallback::record_exit_lock_giveup();
                return false;
            } else {
                std::thread::yield_now();
            }
        }
        true
    }

    /// Test-only: run a closure while holding this shard's raw spin lock.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    pub(super) fn dbg_with_locked_for_test(&self, f: impl FnOnce()) {
        self.acquire();
        f();
        self.release();
    }

    fn release(&self) {
        self.lock.store(false, Ordering::Release);
    }

    pub(super) fn insert(
        &self,
        addr: usize,
        size: usize,
        align: usize,
        generation: u64,
    ) -> InsertOutcome {
        if !self.acquire_unless_exiting() {
            return InsertOutcome::Exiting;
        }
        // SAFETY: lock held, exclusive access.
        let inner = unsafe { &mut *self.inner.get() };
        let (out, retired) = inner.insert(
            Self::hash(addr),
            Slot {
                addr,
                size,
                align,
                generation,
            },
        );
        self.live_hint.store(inner.live, Ordering::Release);
        self.release();
        if let Some((old, old_cap)) = retired {
            // SAFETY: the array retired by `rehash`, freed only after the
            // lock is released; `array_layout(old_cap)` is its exact Layout.
            unsafe { System.dealloc(old as *mut u8, array_layout(old_cap)) };
        }
        out
    }

    /// Descriptor `(size, align, generation)` for `addr`, if live.
    pub(super) fn peek(&self, addr: usize) -> Option<(usize, usize, u64)> {
        if self.live_hint.load(Ordering::Acquire) == 0 {
            return None;
        }
        // Refusal during exit reads as "not ours": the caller leaks safely.
        if !self.acquire_unless_exiting() {
            return None;
        }
        // SAFETY: lock held.
        let inner = unsafe { &*self.inner.get() };
        let r = inner.find(Self::hash(addr), addr).map(|i| {
            // SAFETY: find returned an in-bounds index.
            let s = unsafe { inner.slots.add(i).read() };
            (s.size, s.align, s.generation)
        });
        self.release();
        r
    }

    /// Unlink and return the descriptor for `addr`.
    pub(super) fn take(&self, addr: usize) -> Option<(usize, usize, u64)> {
        if self.live_hint.load(Ordering::Acquire) == 0 {
            return None;
        }
        // Refusal during exit reads as "not ours": the caller leaks safely.
        if !self.acquire_unless_exiting() {
            return None;
        }
        // SAFETY: lock held.
        let inner = unsafe { &mut *self.inner.get() };
        let r = inner.find(Self::hash(addr), addr).map(|i| {
            // SAFETY: find returned an in-bounds index.
            let s = unsafe { inner.slots.add(i).read() };
            // SAFETY: same index; leave a tombstone.
            unsafe {
                inner.slots.add(i).write(Slot {
                    addr: TOMB,
                    size: 0,
                    align: 0,
                    generation: 0,
                })
            };
            inner.live -= 1;
            (s.size, s.align, s.generation)
        });
        self.live_hint.store(inner.live, Ordering::Release);
        self.release();
        r
    }
}
