//! [`SyncRegion`] — the safe concurrent default: a `Region` behind an `RwLock`.

use std::sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{Handle, Region};

/// A thread-safe wrapper around [`Region<T>`] — the trusted concurrent baseline.
///
/// This is a coarse-grained `std::sync::RwLock<Region<T>>` with an ergonomic
/// guard-based API: multiple readers (`read`) or one writer (`write`) at a time.
/// It is the *always-shippable* concurrent answer: correct under any interleaving
/// because every mutation serialises through the lock. Lock-free tiers
/// (Phase 3b) are a later opt-in upgrade for read-mostly hot paths; until those
/// land and clear loom/TSan, this is the default for shared mutable regions.
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
/// dense store generational and consistent regardless of a panicked op, so we
/// **recover from poison** rather than propagate it. Every accessor uses
/// `RwLockReadGuard`/`RwLockWriteGuard` recovery (`PoisonError::into_inner`),
/// handing back the intact inner `Region` and letting callers continue. This
/// keeps a panic in one thread from bricking the region for all others.
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
    /// (see the [poisoning policy](Self#poisoning-policy)).
    pub fn read(&self) -> RwLockReadGuard<'_, Region<T>> {
        self.inner
            .read()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Locks for exclusive write, returning a guard that hands out `&mut Region<T>`.
    ///
    /// Blocks all other readers and writers until dropped. Recovers from poison
    /// (see the [poisoning policy](Self#poisoning-policy)).
    pub fn write(&self) -> RwLockWriteGuard<'_, Region<T>> {
        self.inner
            .write()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Inserts `value`, returning a fresh handle that resolves to it (I1).
    ///
    /// One-shot convenience that locks for write internally. For a transaction
    /// that does several ops under one lock, use [`write`](Self::write) instead.
    pub fn insert(&self, value: T) -> Handle<T> {
        self.write().insert(value)
    }

    /// Removes and returns the value for `handle`, or `None` if stale/removed.
    ///
    /// One-shot convenience that locks for write internally.
    pub fn remove(&self, handle: Handle<T>) -> Option<T> {
        self.write().remove(handle)
    }

    /// Whether `handle` currently resolves to a live value.
    ///
    /// One-shot convenience that locks for read internally.
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
    pub fn clear(&self) {
        self.write().clear();
    }

    /// Clones the value for `handle` out without holding a guard, or `None` if
    /// stale/removed. One-shot convenience that locks for read internally.
    ///
    /// Prefer this over [`read`](Self::read) when you only need a by-value copy
    /// and don't want to hold the guard across other work.
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
