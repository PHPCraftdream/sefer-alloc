#![allow(deprecated)]
//! [`LockFreeHandle`] — the typed, copyable reference into a [`LockFreeRegion`].
//!
//! [`LockFreeRegion`]: crate::concurrent::LockFreeRegion

use core::marker::PhantomData;

/// An opaque, copyable reference to a value stored in a
/// [`LockFreeRegion`](crate::concurrent::LockFreeRegion).
///
/// Like [`Handle<T>`](crate::Handle), this wraps an index plus a generation and
/// is `Copy` and unconditionally `Send + Sync` regardless of `T` — it names a
/// slot but owns no `T`. The `PhantomData<fn() -> T>` keeps the handle *typed*
/// (a `LockFreeHandle<A>` cannot be passed to a `LockFreeRegion<B>`) while
/// staying covariant in `T` and free of any drop/auto-trait obligations.
///
/// Unlike the slotmap-backed `Handle<T>`, the index/generation here index into
/// this tier's own paged slot table (see [`LockFreeRegion`](crate::concurrent::LockFreeRegion)),
/// so the two handle types are intentionally distinct.
///
/// R2-03 (independent src review round 2, task #2005): carries the minting
/// region's `region_id` — a never-reused, monotonically-assigned identity
/// (see `LockFreeRegion`'s own field) — so a handle from a DIFFERENT
/// `LockFreeRegion<T>` instance of the SAME `T` is rejected before any slot
/// lookup, even when its raw `(index, generation)` happens to numerically
/// coincide with a slot in the wrong region (which it routinely does: every
/// freshly-created region's first insert claims index 0 at generation 0).
/// Mirrors the identical fix on [`EpochHandle`](crate::concurrent::EpochHandle)
/// — see that type's doc for the empirically-confirmed cross-instance
/// hazard this closes.
#[deprecated(
    since = "0.1.0",
    note = "concurrent regions are legacy/research-tier; use the production allocator stack (`alloc-xthread`) for cross-thread allocation needs"
)]
pub struct LockFreeHandle<T> {
    /// Crate-visible so [`LockFreeRegion`](crate::concurrent::LockFreeRegion)
    /// can build and read a handle. The minting region's identity (R2-03);
    /// see the struct doc.
    #[allow(clippy::missing_docs_in_private_items)]
    pub(crate) region_id: u64,
    /// Crate-visible so [`LockFreeRegion`](crate::concurrent::LockFreeRegion)
    /// can build and read a handle. Global slot index into the page table.
    #[allow(clippy::missing_docs_in_private_items)]
    pub(crate) index: u32,
    /// Crate-visible; the generation the handle was minted at. A stale
    /// generation never resolves (I3 — no ABA).
    #[allow(clippy::missing_docs_in_private_items)]
    pub(crate) generation: u32,
    _ty: PhantomData<fn() -> T>,
}

impl<T> LockFreeHandle<T> {
    /// Crate-internal constructor from a minting region's id, a raw index,
    /// and a generation.
    pub(crate) fn new(region_id: u64, index: u32, generation: u32) -> Self {
        Self {
            region_id,
            index,
            generation,
            _ty: PhantomData,
        }
    }
}

// Hand-written impls: a handle is "a region id + an index + a generation", so
// these must hold for *every* `T`, not only `T: Clone`/`Eq`/… that `#[derive]`
// would (wrongly) require. They inspect only the integer fields and hold
// unconditionally in `T`.
impl<T> Clone for LockFreeHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for LockFreeHandle<T> {}
impl<T> PartialEq for LockFreeHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.region_id == other.region_id
            && self.index == other.index
            && self.generation == other.generation
    }
}
impl<T> Eq for LockFreeHandle<T> {}
impl<T> core::hash::Hash for LockFreeHandle<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.region_id.hash(state);
        self.index.hash(state);
        self.generation.hash(state);
    }
}
impl<T> core::fmt::Debug for LockFreeHandle<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LockFreeHandle")
            .field("region_id", &self.region_id)
            .field("index", &self.index)
            .field("generation", &self.generation)
            .finish()
    }
}
