#![allow(deprecated)]
//! [`EpochHandle`] — the typed, copyable reference into an [`EpochRegion`].
//!
//! [`EpochRegion`]: crate::concurrent::EpochRegion

use core::marker::PhantomData;

/// An opaque, copyable reference to a value stored in an
/// [`EpochRegion`](crate::concurrent::EpochRegion).
///
/// Like [`Handle<T>`](crate::Handle), this wraps an index plus a generation and
/// is `Copy` and unconditionally `Send + Sync` regardless of `T` — it names a
/// slot but owns no `T`. The `PhantomData<fn() -> T>` keeps the handle *typed*
/// (an `EpochHandle<A>` cannot be passed to an `EpochRegion<B>`) while staying
/// covariant in `T` and free of any drop/auto-trait obligations.
///
/// Distinct from [`LockFreeHandle<T>`](crate::concurrent::LockFreeHandle) and
/// [`Handle<T>`](crate::Handle): this indexes the epoch tier's own fixed slot
/// table (see [`EpochRegion`](crate::concurrent::EpochRegion)), so the handle
/// types are intentionally distinct.
///
/// R2-03 (independent src review round 2, task #2005): carries the minting
/// region's `region_id` — a never-reused, monotonically-assigned identity
/// (see `EpochRegion`'s own field) — so a handle from a DIFFERENT
/// `EpochRegion<T>` instance of the SAME `T` is rejected before any slot
/// lookup or generation CAS, even when its raw `(index, generation)` happens
/// to numerically coincide with a slot in the wrong region (which it
/// routinely does: every freshly-created region's first insert claims index
/// 0 at generation 0). Confirmed empirically before this field existed: a
/// handle minted by region A, applied to region B's `get_with`, returned
/// region B's real value; applied to region B's `remove`, it won the
/// generation CAS on a slot region B had NEVER installed (vacant at
/// generation 0), reporting a phantom removal and underflowing `len` to
/// `usize::MAX`.
#[deprecated(
    since = "0.1.0",
    note = "concurrent regions are legacy/research-tier; use the production allocator stack (`alloc-xthread`) for cross-thread allocation needs"
)]
pub struct EpochHandle<T> {
    /// Crate-visible so [`EpochRegion`](crate::concurrent::EpochRegion) can
    /// build and read a handle. The minting region's identity (R2-03); see
    /// the struct doc.
    #[allow(clippy::missing_docs_in_private_items)]
    pub(crate) region_id: u64,
    /// Crate-visible so [`EpochRegion`](crate::concurrent::EpochRegion) can
    /// build and read a handle. Index into the boxed slot slice.
    #[allow(clippy::missing_docs_in_private_items)]
    pub(crate) index: u32,
    /// Crate-visible; the generation the handle was minted at. A stale
    /// generation never resolves (I3 — no ABA).
    #[allow(clippy::missing_docs_in_private_items)]
    pub(crate) generation: u32,
    _ty: PhantomData<fn() -> T>,
}

impl<T> EpochHandle<T> {
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
impl<T> Clone for EpochHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for EpochHandle<T> {}
impl<T> PartialEq for EpochHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.region_id == other.region_id
            && self.index == other.index
            && self.generation == other.generation
    }
}
impl<T> Eq for EpochHandle<T> {}
impl<T> core::hash::Hash for EpochHandle<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.region_id.hash(state);
        self.index.hash(state);
        self.generation.hash(state);
    }
}
impl<T> core::fmt::Debug for EpochHandle<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EpochHandle")
            .field("region_id", &self.region_id)
            .field("index", &self.index)
            .field("generation", &self.generation)
            .finish()
    }
}
