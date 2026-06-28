//! [`Handle`] — the typed, copyable reference to a value in a [`Region`].
//!
//! [`Region`]: crate::Region

use core::marker::PhantomData;

/// An opaque, copyable reference to a value stored in a [`Region`].
///
/// A handle wraps a `slotmap::DefaultKey` (an index plus a generation) and is
/// `Copy` and unconditionally `Send + Sync` regardless of `T` — it owns no `T`,
/// it only names one. The `PhantomData<fn() -> T>` keeps the handle *typed* (so
/// a `Handle<A>` cannot be passed to a `Region<B>`) while staying covariant in
/// `T` and free of any drop/auto-trait obligations.
///
/// [`Region`]: crate::Region
pub struct Handle<T> {
    /// Crate-visible so [`Region`](crate::Region) can build and read a handle,
    /// never exposed publicly.
    pub(crate) key: slotmap::DefaultKey,
    _ty: PhantomData<fn() -> T>,
}

impl<T> Handle<T> {
    /// Crate-internal constructor wrapping a raw slotmap key.
    pub(crate) fn from_key(key: slotmap::DefaultKey) -> Self {
        Self {
            key,
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
        self.key == other.key
    }
}
impl<T> Eq for Handle<T> {}
impl<T> core::hash::Hash for Handle<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}
impl<T> core::fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Handle").field("key", &self.key).finish()
    }
}
