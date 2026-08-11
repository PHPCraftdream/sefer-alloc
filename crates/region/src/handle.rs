//! [`Handle`] — the typed, copyable reference to a value in a [`Region`].
//!
//! [`Region`]: crate::Region

use core::marker::PhantomData;
use core::num::NonZeroUsize;

/// An opaque, copyable reference to a value stored in a [`Region`].
///
/// A handle wraps a `slotmap::DefaultKey` (an index plus a generation) and a
/// `region_id` that identifies which `Region` instance the handle belongs to.
/// It is `Copy` and unconditionally `Send + Sync` regardless of `T` — it owns
/// no `T`, it only names one. The `PhantomData<fn() -> T>` keeps the handle
/// *typed* (so a `Handle<A>` cannot be passed to a `Region<B>`) while staying
/// covariant in `T` and free of any drop/auto-trait obligations.
///
/// The `region_id: NonZeroUsize` field ensures that handles from different
/// `Region` instances never collide even if they have the same `key`. Using
/// `NonZeroUsize` preserves the niche optimization for `Option<Handle<T>>`.
/// `region_id` is pointer-width (not a fixed 64 bits) so this type stays
/// buildable on no_std targets without 64-bit atomics (e.g.
/// `thumbv7em-none-eabi`, `i686-*`) — every such target
/// still has pointer-width atomics.
///
/// [`Region`]: crate::Region
#[repr(C)]
pub struct Handle<T> {
    /// Declared before `key` so the struct's own field order matches
    /// `Ord`'s comparison order below — if a future refactor ever swaps the
    /// hand-written `Ord`/`PartialOrd` impls for `#[derive(PartialOrd, Ord)]`
    /// (which compares fields in declaration order), this keeps that
    /// substitution field-order-neutral instead of silently reordering
    /// comparisons. Layout-neutral either way: `size_of::<Handle<T>>()` and
    /// its alignment are unaffected by this field's position (verified by
    /// `tests/handle_static_asserts.rs`, which pins the size on both
    /// pointer widths).
    pub(crate) region_id: NonZeroUsize,
    /// Crate-visible so [`Region`](crate::Region) can build and read a handle,
    /// never exposed publicly.
    pub(crate) key: slotmap::DefaultKey,
    _ty: PhantomData<fn() -> T>,
}

impl<T> Handle<T> {
    /// Crate-internal constructor wrapping a raw slotmap key and region ID.
    pub(crate) fn from_key_and_region(key: slotmap::DefaultKey, region_id: NonZeroUsize) -> Self {
        Self {
            key,
            region_id,
            _ty: PhantomData,
        }
    }
}

// Hand-written impls: a handle is "a slotmap key", so these must hold for
// *every* `T`, not only `T: Clone`/`Eq`/… that `#[derive]` would (wrongly)
// require. They delegate to the inner `key` and hold unconditionally in `T`.
impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Handle<T> {}
impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.region_id == other.region_id
    }
}
impl<T> Eq for Handle<T> {}
impl<T> core::hash::Hash for Handle<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
        self.region_id.hash(state);
    }
}

// Comparison order: first by `region_id`, then by `key`.
//
// This is a deliberate design choice (not a `Hash`-impl-parity default —
// field order in `Hash` has no bearing on `Ord`/`Eq` consistency, and any
// order here is equally valid for that purpose): ordering by `region_id`
// first means handles group by their owning `Region` under `sort()` /
// inside a `BTreeMap<Handle<T>, V>` / `BTreeSet<Handle<T>>`, so a caller
// holding handles from several `Region`s can range-scan just one Region's
// slice contiguously (e.g. `handles.sort()` then `.partition_point(..)` /
// a `BTreeMap` range bounded by that Region's own handles). Ordering by
// `key` first (the alternative) would instead interleave handles from
// different Regions whenever their raw `DefaultKey`s happen to compare
// close together — which is common, since the first insert into any fresh
// `Region` tends to produce the same key — defeating exactly the grouping
// a `BTreeMap`/sorted-`Vec` user would reasonably want.
//
// Handles from different regions (different `region_id`) will never
// compare equal per `PartialEq`, but they still have a consistent total
// order — useful for sorting/`BTreeMap` even though `HashMap` is the more
// common use case.
/// `Ord`/`PartialOrd` provide a total order consistent with [`Eq`], suitable
/// for storing `Handle<T>` in a `BTreeMap`/`BTreeSet` or sorting a `Vec` of
/// them. The *relative* order between two particular handles — including
/// whether handles from different [`Region`](crate::Region)s group together
/// or interleave — is an unspecified implementation detail (currently:
/// group by `region_id`, tie-break by `key`) and may change in any release.
/// Do not depend on it for anything beyond "a total order exists".
impl<T> PartialOrd for Handle<T> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Handle<T> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match self.region_id.cmp(&other.region_id) {
            core::cmp::Ordering::Equal => self.key.cmp(&other.key),
            ordering => ordering,
        }
    }
}

impl<T> core::fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Handle")
            .field("key", &self.key)
            .field("region_id", &self.region_id)
            .finish()
    }
}
