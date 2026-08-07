//! [`Region`] — a handle-addressed store of `T` backed by `slotmap`.

use crate::Handle;

/// A handle-addressed store of `T`.
///
/// A thin typed membrane over `slotmap::SlotMap<slotmap::DefaultKey, T>`.
/// `SlotMap` keeps values in a contiguous slot array resolved by a single
/// indirection (the lookup/churn axis it was benchmarked to win; see
/// `docs/BENCHMARKS.md`), but it leaves tombstone holes after removals — it is
/// NOT always-compact, and iteration walks the slot array skipping holes
/// (~30 % slower than a `DenseSlotMap`, which packs live values for dense
/// iteration). Every operation delegates to `slotmap` while exposing only typed
/// [`Handle<T>`] values (raw `DefaultKey`s never escape). Individual lookup,
/// insertion, and removal are `O(1)`; iteration and [`clear`](Self::clear) are
/// linear in the slot-array length; [`reserve`](Self::reserve) may reallocate.
///
/// ## Invariants upheld
///
/// - **I1 — resolution:** a fresh handle resolves via [`get`](Self::get) to the
///   inserted value until it is [`remove`](Self::remove)d.
/// - **I2 — tombstone:** after `remove(h)`, `get(h)` returns `None` for
///   roughly `2^31` reuse cycles of that slot (a stale handle that has
///   survived that many insert/remove cycles may wrap and spuriously
///   resolve to a later value). A second `remove(h)` is a no-op `None`.
/// - **I3 — no ABA:** a stale handle — one whose slot has since been reused —
///   does not resolve to a live value for roughly `2^31` reuse cycles of
///   that slot. `slotmap`'s `DefaultKey` carries a 32-bit generation (odd =
///   occupied, even = vacant): insert sets the low bit, remove increments via
///   `wrapping_add(1)`, so a full occupy/free cycle advances the generation
///   by 2 — after `2^31` such cycles it wraps and a very old handle may alias
///   a later value. Memory safety is never affected — `slotmap` guarantees
///   this even after wrap.
/// - **I4 — accounting:** [`len`](Self::len) equals the number of live entries
///   and [`is_empty`](Self::is_empty) agrees.
/// - **I5 — drop-once:** every live value is dropped exactly once — on
///   `remove` (returned to the caller) or on `Region` drop — never twice,
///   never leaked. `slotmap` owns the storage and therefore the drops.
///
/// ## Generation saturation
///
/// `slotmap::DefaultKey` uses a 32-bit generation counter stored alongside each
/// slot, where an odd value means occupied and an even value means vacant.
/// `SlotMap::insert` sets the low bit on reuse (`version | 1`); `SlotMap::remove`
/// advances it past that with `version.wrapping_add(1)` (odd -> even). So one
/// full occupy/free cycle of a slot advances its generation by 2, and after
/// approximately `2^31` such cycles the generation wraps around to its starting
/// value, and a sufficiently stale handle may then resolve to (or remove) a
/// different live value that now occupies the same slot.
///
/// This is a **logic/aliasing issue, not memory unsafety** — `slotmap` guarantees
/// that its internal data structure never becomes corrupt, even when a handle wraps.
/// The worst case for reaching wrap quickly is a hot single-slot churn pattern
/// (repeatedly inserting and removing at the same slot index while nothing else
/// is live), because `slotmap`'s freelist is LIFO. This was empirically confirmed:
/// a tight insert/remove loop on one slot for `2^31 - 1` cycles took ~12 seconds
/// on one development machine in release mode; treat this as an order-of-magnitude
/// sense, not a guaranteed bound.
///
/// There is **no slot-retirement code** in this crate. Applications that need a
/// stronger guarantee (e.g. to reuse handles without ever risking alias) must add
/// their own wrapper layer that tracks generation saturation.
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

    /// Reserves capacity for at least `additional` more insertions.
    ///
    /// Does nothing if the backing store already has room. After a churn that
    /// removes entries, the freed slots live on the free list, so re-inserting
    /// reuses existing capacity and does not grow unboundedly (the backing
    /// stays bounded by the high-water mark of live entries). Delegates to
    /// `slotmap`'s `reserve`; may allocate more than asked to avoid frequent
    /// reallocations. Panics if the new allocation size overflows `usize`.
    pub fn reserve(&mut self, additional: usize) {
        self.inner.reserve(additional);
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

    /// Iterates the live values. The order is unspecified and changes as
    /// elements are removed. Walks the underlying `SlotMap`'s slot array,
    /// skipping tombstone holes — so this is NOT cache-dense over live values
    /// (a `DenseSlotMap`-backed store would be); see `docs/BENCHMARKS.md`.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.inner.values()
    }

    /// Mutably iterates the live values (same non-dense order caveat as
    /// [`iter`](Self::iter)).
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
