//! [`Region`] — a handle-addressed store of `T` backed by `slotmap`.

use crate::Handle;

/// A handle-addressed store of `T`.
///
/// A thin typed membrane over `slotmap::SlotMap<slotmap::DefaultKey, T>`:
/// values live in `slotmap`'s dense, cache-friendly, always-compact backing
/// store, and every operation delegates to `slotmap` while exposing only typed
/// [`Handle<T>`] values (raw `DefaultKey`s never escape). All operations are
/// `O(1)`.
///
/// ## Invariants upheld
///
/// - **I1 — resolution:** a fresh handle resolves via [`get`](Self::get) to the
///   inserted value until it is [`remove`](Self::remove)d.
/// - **I2 — tombstone:** after `remove(h)`, `get(h)` is `None` forever and a
///   second `remove(h)` is a no-op `None`.
/// - **I3 — no ABA:** a stale handle — one whose slot has since been reused —
///   never resolves to a live value. `slotmap`'s `DefaultKey` carries a
///   generation that is bumped on removal, so the old handle fails the version
///   check and yields `None`.
/// - **I4 — accounting:** [`len`](Self::len) equals the number of live entries
///   and [`is_empty`](Self::is_empty) agrees.
/// - **I5 — drop-once:** every live value is dropped exactly once — on
///   `remove` (returned to the caller) or on `Region` drop — never twice,
///   never leaked. `slotmap` owns the storage and therefore the drops.
///
/// ## Generation saturation
///
/// Version saturation (a slot whose generation would have to wrap) is
/// `slotmap`'s responsibility: `DefaultKey` retires such a slot rather than
/// wrapping a generation into alias, so a handle can never alias a future value
/// — the classic generational-arena ABA caveat stays closed at the
/// astronomically rare cost of one slot per `2^32` reuses. There is no
/// hand-rolled retirement code in this crate.
pub struct Region<T> {
    inner: slotmap::SlotMap<slotmap::DefaultKey, T>,
}

impl<T> Region<T> {
    /// Creates an empty region that allocates nothing until first use.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: slotmap::SlotMap::new(),
        }
    }

    /// Creates an empty region with space pre-reserved for `capacity` entries.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: slotmap::SlotMap::with_capacity(capacity),
        }
    }

    /// Number of live values (I4).
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether the region holds no live values (I4).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Current value-storage capacity, in entries.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    /// Inserts `value`, returning a fresh handle that resolves to it (I1).
    pub fn insert(&mut self, value: T) -> Handle<T> {
        Handle::from_key(self.inner.insert(value))
    }

    /// Borrows the value for `handle`, or `None` if the handle is stale or
    /// removed (I1, I2, I3).
    #[must_use]
    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        self.inner.get(handle.key)
    }

    /// Mutably borrows the value for `handle`, or `None` if stale/removed.
    #[must_use]
    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        self.inner.get_mut(handle.key)
    }

    /// Whether `handle` currently resolves to a live value.
    #[must_use]
    pub fn contains(&self, handle: Handle<T>) -> bool {
        self.inner.contains_key(handle.key)
    }

    /// Removes and returns the value for `handle`, or `None` if it is already
    /// stale/removed. After this, `handle` resolves to `None` forever (I2).
    pub fn remove(&mut self, handle: Handle<T>) -> Option<T> {
        self.inner.remove(handle.key)
    }

    /// Iterates the live values in dense (cache-friendly) order. The order is
    /// unspecified and changes as elements are removed.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.inner.values()
    }

    /// Mutably iterates the live values in dense order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.inner.values_mut()
    }

    /// Removes every value, invalidating all outstanding handles, while
    /// retaining allocated capacity. The region is reusable afterwards.
    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

impl<T> Default for Region<T> {
    fn default() -> Self {
        Self::new()
    }
}
