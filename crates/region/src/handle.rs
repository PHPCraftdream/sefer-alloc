//! [`Handle`] — the typed, copyable reference to a value in a [`Region`].
//!
//! [`Region`]: crate::Region

use core::marker::PhantomData;
use core::num::NonZeroU64;

/// An opaque, copyable reference to a value stored in a [`Region`].
///
/// A handle wraps a `slotmap::DefaultKey` (an index plus a generation) and a
/// `region_id` that identifies which `Region` instance the handle belongs to.
/// It is `Copy` and unconditionally `Send + Sync` regardless of `T` — it owns
/// no `T`, it only names one. The `PhantomData<fn() -> T>` keeps the handle
/// *typed* (so a `Handle<A>` cannot be passed to a `Region<B>`) while staying
/// covariant in `T` and free of any drop/auto-trait obligations.
///
/// The `region_id: NonZeroU64` field ensures that handles from different
/// `Region` instances never collide even if they have the same `key`. Using
/// `NonZeroU64` preserves the niche optimization for `Option<Handle<T>>`.
///
/// [`Region`]: crate::Region
#[repr(C)]
pub struct Handle<T> {
    /// Crate-visible so [`Region`](crate::Region) can build and read a handle,
    /// never exposed publicly.
    pub(crate) key: slotmap::DefaultKey,
    pub(crate) region_id: NonZeroU64,
    _ty: PhantomData<fn() -> T>,
}

impl<T> Handle<T> {
    /// Crate-internal constructor wrapping a raw slotmap key and region ID.
    pub(crate) fn from_key_and_region(key: slotmap::DefaultKey, region_id: NonZeroU64) -> Self {
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

// Comparison order: first by `key`, then by `region_id`.
// This matches the `Hash` impl (which also hashes key first).
// Handles from different regions (different `region_id`) will never
// compare equal per `PartialEq`, but they still have a consistent
// total order — useful for sorting/`BTreeMap` even though `HashMap`
// is the more common use case.
impl<T> PartialOrd for Handle<T> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Handle<T> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match self.key.cmp(&other.key) {
            core::cmp::Ordering::Equal => self.region_id.cmp(&other.region_id),
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
