#![allow(deprecated)]
//! [`ShardedHandle<T>`] — the typed, copyable reference into a
//! [`ShardedRegion<T>`] (Phase 7a, `experimental`).
//!
//! A `ShardedHandle` pairs an [`EpochHandle<T>`] with the id of the shard that
//! minted it, so [`ShardedRegion`](crate::concurrent::ShardedRegion) can route a
//! read/remove back to the owning shard without scanning. The shard id is a
//! `u16` — `ShardedRegion` caps the shard count at `u16::MAX` for that reason.
//!
//! Like [`EpochHandle<T>`], this is `Copy` and unconditionally `Send + Sync`
//! regardless of `T`: it names a slot but owns no `T`. The
//! `PhantomData<fn() -> T>` keeps the handle *typed* and covariant.
//!
//! [`EpochHandle<T>`]: crate::concurrent::EpochHandle

use core::marker::PhantomData;

use crate::concurrent::EpochHandle;

/// An opaque, copyable reference to a value stored in a
/// [`ShardedRegion`](crate::concurrent::ShardedRegion).
///
/// It is a `(shard, EpochHandle)` pair: the shard routes the operation to the
/// owning [`EpochRegion`](crate::concurrent::EpochRegion), and the inner
/// [`EpochHandle`] addresses the slot within it. All invariants of
/// [`EpochHandle`] carry over unchanged (I1–I3); the shard id is what makes a
/// handle *shard-local* — a handle minted in shard A carries `shard == A` and
/// is routed exclusively to shard A, so it can never resolve against shard B's
/// slot table (the multi-shard differential property asserted in
/// `tests/sharded.rs`).
#[deprecated(
    since = "0.1.0",
    note = "concurrent regions are legacy/research-tier; use the production allocator stack (`alloc-xthread`) for cross-thread allocation needs"
)]
pub struct ShardedHandle<T> {
    /// Crate-visible so [`ShardedRegion`](crate::concurrent::ShardedRegion)
    /// can build a handle on insert and route on read/remove.
    #[allow(clippy::missing_docs_in_private_items)]
    pub(crate) shard: u16,
    /// Crate-visible; the inner per-shard handle.
    #[allow(clippy::missing_docs_in_private_items)]
    pub(crate) inner: EpochHandle<T>,
    _ty: PhantomData<fn() -> T>,
}

impl<T> ShardedHandle<T> {
    /// Crate-internal constructor from a shard id and an inner `EpochHandle`.
    pub(crate) fn new(shard: u16, inner: EpochHandle<T>) -> Self {
        Self {
            shard,
            inner,
            _ty: PhantomData,
        }
    }

    /// The shard this handle was minted in (0-based). Exposed for diagnostics
    /// and tests; routing uses the crate-visible field directly.
    #[must_use]
    pub fn shard(&self) -> u16 {
        self.shard
    }
}

// Hand-written impls: a handle is "a shard id + an EpochHandle", so these must
// hold for *every* `T`, not only `T: Clone`/`Eq`/… that `#[derive]` would
// (wrongly) require. They inspect only the shard id and the (index,
// generation) integers inside the inner handle, and hold unconditionally in
// `T` — mirroring `EpochHandle`'s own hand-written trait impls.
impl<T> Clone for ShardedHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for ShardedHandle<T> {}
impl<T> PartialEq for ShardedHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.shard == other.shard && self.inner == other.inner
    }
}
impl<T> Eq for ShardedHandle<T> {}
impl<T> core::hash::Hash for ShardedHandle<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.shard.hash(state);
        self.inner.hash(state);
    }
}
impl<T> core::fmt::Debug for ShardedHandle<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ShardedHandle")
            .field("shard", &self.shard)
            .field("inner", &self.inner)
            .finish()
    }
}
