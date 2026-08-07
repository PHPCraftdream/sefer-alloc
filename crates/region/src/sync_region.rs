//! [`SyncRegion`] — the safe concurrent default: a `Region` behind an `RwLock`.

use std::sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{Handle, Region};

/// A thread-safe wrapper around [`Region<T>`] — the trusted concurrent baseline.
///
/// This is a coarse-grained `std::sync::RwLock<Region<T>>` with an ergonomic
/// guard-based API: multiple readers (`read`) or one writer (`write`) at a time.
/// It is the *always-shippable* concurrent answer: correct under any interleaving
/// because every mutation serialises through the lock. Finer-grained or lock-free
/// alternatives are out of scope for this crate.
///
/// The wrapper stays `#![forbid(unsafe_code)]`: all interior mutability comes
/// from `std`'s `RwLock`. Use [`read`](Self::read) / [`write`](Self::write) for
/// multi-operation transactions (the borrows tie to the guard), or the
/// one-shot convenience methods ([`insert`](Self::insert),
/// [`remove`](Self::remove), …) which take `&self` and lock internally.
///
/// ## Poisoning policy
///
/// A panic while a guard is held poisons the `RwLock`. A poisoned `Region` is
/// still structurally valid — no broken memory invariants: `slotmap` keeps the
/// slot store generational and consistent regardless of a panicked op, so we
/// **recover from poison** rather than propagate it. Every accessor uses
/// `RwLockReadGuard`/`RwLockWriteGuard` recovery (`PoisonError::into_inner`),
/// handing back the intact inner `Region` and letting callers continue. This
/// keeps a panic in one thread from bricking the region for all others.
///
/// **Poison recovery guarantees container integrity only, not operation completion.**
/// The recovered `Region` has no memory corruption, but an interrupted operation
/// may have left partial effects visible: a panicking `T::Drop` during `clear()`
/// leaves later values live (a partial clear, not a full one), and a panicked
/// multi-op `write()` transaction leaves whatever partial effects it already
/// applied. Callers whose `T` carries cross-value invariants, or whose
/// multi-op transactions need all-or-nothing semantics, must implement their
/// own signaling — this crate provides none beyond what's documented here.
///
/// ## Reentrancy
///
/// [`get_cloned`](Self::get_cloned) runs `T::clone`, and [`clear`](Self::clear) runs each
/// `T::Drop`, while the internal lock is held. If `T`'s `Clone` or `Drop` implementation
/// re-enters the same `SyncRegion` (directly or transitively), the thread deadlocks or
/// panics per `std::sync::RwLock`'s documented same-thread reacquisition behavior.
/// Even non-reentrant but slow `Clone`/`Drop` delays every other user: `clear` holds
/// the write lock across its entire linear sweep, while `get_cloned` holds the read lock
/// across the clone (readers are unaffected by the latter, but writers block). Never
/// call a one-shot convenience method (or a nested `read`/`write`) while the calling
/// thread already holds a read/write guard from the same `SyncRegion` — the one-shots
/// lock internally and the nested acquisition deadlocks (`std`'s `RwLock` is not reentrant;
/// even read-after-read can block behind a queued writer, since the platform's priority
/// policy is unspecified).
///
/// ## Contended reads
///
/// Under multi-threaded read contention, the one-shot convenience methods
/// ([`get_cloned`](Self::get_cloned), [`contains`](Self::contains),
/// [`len`](Self::len), [`is_empty`](Self::is_empty)) anti-scale: each call pays a
/// shared-cache-line lock acquisition that dominates the nanosecond-scale lookup,
/// resulting in a ~4× aggregate throughput loss going from 1 to 8 reader threads
/// on a 16-CPU host. Batching multiple reads under one held [`read`](Self::read)
/// guard restores flat scaling at ~30× the one-shot aggregate at 8 threads.
/// This is inherent `RwLock` physics for a nanosecond-scale critical section,
/// not a defect in the lock implementation.
pub struct SyncRegion<T> {
    inner: RwLock<Region<T>>,
}

impl<T> SyncRegion<T> {
    /// Creates an empty region that allocates nothing until first use.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Region::new()),
        }
    }

    /// Creates an empty region with space pre-reserved for `capacity` entries.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: RwLock::new(Region::with_capacity(capacity)),
        }
    }

    /// Locks for shared read, returning a guard that hands out `&Region<T>`.
    ///
    /// Multiple readers may hold the guard concurrently. Recovers from poison
    /// (see the [poisoning policy](Self#poisoning-policy)). Returns `std`'s
    /// own guard type directly — a deliberate, stable API commitment; migrating
    /// the internal lock implementation in the future would be a breaking change.
    pub fn read(&self) -> RwLockReadGuard<'_, Region<T>> {
        self.inner.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// Locks for exclusive write, returning a guard that hands out `&mut Region<T>`.
    ///
    /// Blocks all other readers and writers until dropped. Recovers from poison
    /// (see the [poisoning policy](Self#poisoning-policy)). Returns `std`'s
    /// own guard type directly — a deliberate, stable API commitment; migrating
    /// the internal lock implementation in the future would be a breaking change.
    pub fn write(&self) -> RwLockWriteGuard<'_, Region<T>> {
        self.inner.write().unwrap_or_else(PoisonError::into_inner)
    }

    /// Inserts `value`, returning a fresh handle that resolves to it (I1).
    ///
    /// One-shot convenience that locks for write internally. For a transaction
    /// that does several ops under one lock, use [`write`](Self::write) instead.
    ///
    /// # Panics
    ///
    /// Panics if the backing `slotmap` is full (2^32 - 2 live entries).
    pub fn insert(&self, value: T) -> Handle<T> {
        self.write().insert(value)
    }

    /// Removes and returns the value for `handle`, or `None` if stale/removed.
    ///
    /// One-shot convenience that locks for write internally. The write guard is
    /// released before the removed value is dropped by the caller, so a reentrant
    /// `Drop` on the removed value is safe against the deadlock class described
    /// in the [reentrancy section](Self#reentrancy).
    pub fn remove(&self, handle: Handle<T>) -> Option<T> {
        self.write().remove(handle)
    }

    /// Whether `handle` currently resolves to a live value.
    ///
    /// One-shot convenience that locks for read internally. Note that under
    /// concurrency a `true` result may be stale by the time the caller acts on it;
    /// acting on a stale handle can only ever produce `None` at the point of use,
    /// never resolve to a wrong live value within roughly `2^31` reuse cycles of
    /// that slot. Callers who need an atomic check-then-act should use
    /// [`write`](Self::write) instead of two separate lock acquisitions.
    #[must_use]
    pub fn contains(&self, handle: Handle<T>) -> bool {
        self.read().contains(handle)
    }

    /// Number of live values (I4).
    ///
    /// One-shot convenience that locks for read internally. Note that under
    /// concurrency the count is a momentary snapshot, not a stable property.
    #[must_use]
    pub fn len(&self) -> usize {
        self.read().len()
    }

    /// Whether the region holds no live values (I4).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.read().is_empty()
    }

    /// Removes every value, invalidating all outstanding handles.
    ///
    /// One-shot convenience that locks for write internally.
    /// If a value's `Drop` impl panics mid-`clear`, the clear is partial:
    /// values already visited (including the panicking one) are removed and
    /// dropped, but later values remain live and correctly accounted. The region
    /// itself stays fully consistent and reusable after unwinding.
    pub fn clear(&self) {
        self.write().clear();
    }

    /// Clones the value for `handle` out without leaving the caller holding a guard,
    /// or `None` if stale/removed. One-shot convenience that locks for read internally.
    ///
    /// Prefer this over [`read`](Self::read) when you only need a by-value copy
    /// and don't want to hold the guard across other work. Note that the `T::clone`
    /// call itself runs under the read lock (this is unavoidable due to borrowing
    /// semantics), so for expensive-Clone payloads every call extends the lock hold
    /// by the full clone duration and delays any writer arriving during that window
    /// by up to that much (measured ~1.5–1.8 ms worst-case writer stall for a 4 MiB
    /// payload). For such payloads, store `Arc<T>` instead so the "clone" is a cheap
    /// refcount bump.
    pub fn get_cloned(&self, handle: Handle<T>) -> Option<T>
    where
        T: Clone,
    {
        self.read().get(handle).cloned()
    }
}

impl<T> Default for SyncRegion<T> {
    fn default() -> Self {
        Self::new()
    }
}
