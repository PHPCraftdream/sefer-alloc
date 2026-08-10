//! [`Region`] — a handle-addressed store of `T` backed by `slotmap`.

use crate::Handle;
use core::num::NonZeroUsize;
use core::sync::atomic::{AtomicUsize, Ordering};

static NEXT_REGION_ID: AtomicUsize = AtomicUsize::new(1);

/// A handle-addressed store of `T`.
///
/// A thin typed membrane over `slotmap::SlotMap<slotmap::DefaultKey, T>`.
/// `SlotMap` keeps values in a contiguous slot array resolved by a single
/// indirection (the lookup/churn axis it was benchmarked to win; see
/// <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/BENCHMARKS.md>), but it leaves tombstone holes after removals — it is
/// NOT always-compact, and iteration walks the slot array skipping holes
/// (~30 % slower than a `DenseSlotMap`, which packs live values for dense
/// iteration). Every operation delegates to `slotmap` while exposing only typed
/// [`Handle<T>`] values (raw `DefaultKey`s never escape as usable values
/// through the API — Debug output renders the underlying key for diagnostics
/// only, it cannot be turned back into a functioning handle through this crate's
/// public surface). Individual lookup and
/// removal are `O(1)`; insertion is amortized `O(1)` (may reallocate the slot
/// array on growth); iteration and [`clear`](Self::clear) are linear in the
/// slot-array length; [`reserve`](Self::reserve) may reallocate.
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
/// - **I6 — instance isolation:** a [`Handle<T>`] resolves only through the
///   `Region<T>` instance that minted it. Every accessor
///   ([`get`](Self::get), [`get_mut`](Self::get_mut), [`remove`](Self::remove),
///   [`contains`](Self::contains)) stamps its `region_id` at construction and
///   checks it before touching the backing slotmap; a handle from a
///   *different* `Region<T>` is rejected exactly like a stale handle (`None`/
///   `false`), even when its raw `DefaultKey` collides with a live key in
///   this region (this grows `Handle<T>`'s size versus the pre-0.2.0 layout
///   by one `NonZeroUsize` field — 16 bytes total on a 64-bit host, smaller
///   on a 32-bit host, since the added field is pointer-width, not a fixed
///   8 bytes — see `tests/handle_static_asserts.rs`). `region_id` is minted
///   from a process-wide counter (`NEXT_REGION_ID`, `AtomicUsize`) that is
///   incremented once per `Region::new`/`with_capacity` call and never
///   reused; it saturates after `2^{pointer_width} - 1` `Region`
///   constructions in one process (`usize::MAX` on the host's pointer
///   width) and panics on overflow — see the `# Panics` sections on
///   [`new`](Self::new) and [`with_capacity`](Self::with_capacity). On a
///   64-bit host this is a theoretical guard only. On a **32-bit host**
///   (e.g. `thumbv7em-none-eabi`, `i686-*`) the bound is `2^32 - 1` (about
///   4.29 billion), which is *reachable*, not just theoretical, for a
///   long-lived 32-bit server or embedded process that mints a fresh
///   `Region` per request/session over its lifetime rather than reusing
///   one — the same honest register as the I2/I3 generation-wrap
///   disclosure below, just a much larger and process-lifetime-scoped
///   count rather than a per-slot reuse count.
///
/// ## Generation saturation
///
/// `slotmap::DefaultKey` uses a 32-bit generation counter stored alongside each
/// slot. The exact encoding (odd = occupied, even = vacant), the LIFO freelist
/// behavior, and the measured "~12 seconds" bound for `2^31 - 1` insert/remove
/// cycles on a hot slot are **implementation details of the resolved
/// slotmap 1.1.1 snapshot** — slotmap 1.x reserves the right to change these.
///
/// In the current version: `SlotMap::insert` sets the low bit on reuse
/// (`version | 1`); `SlotMap::remove` advances it past that with
/// `version.wrapping_add(1)` (odd -> even). So one full occupy/free cycle of a
/// slot advances its generation by 2, and after approximately `2^31` such cycles
/// the generation wraps around to its starting value, and a sufficiently stale
/// handle may then resolve to (or remove) a different live value that now
/// occupies the same slot.
///
/// This is a **logic/aliasing issue, not memory unsafety** — `slotmap` guarantees
/// that its internal data structure never becomes corrupt, even when a handle wraps.
/// The worst case for reaching wrap quickly is a hot single-slot churn pattern
/// (repeatedly inserting and removing at the same slot index while nothing else
/// is live). This was empirically confirmed on slotmap 1.1.1: a tight insert/remove
/// loop on one slot for `2^31 - 1` cycles took ~12 seconds on one development
/// machine in release mode; treat this as an order-of-magnitude sense for that
/// version, not a guaranteed bound for all slotmap 1.x.
///
/// Applications that need a stronger guarantee (e.g. to reuse handles without
/// ever risking alias) must add their own wrapper layer that tracks generation
/// wrap on a hot slot; cross-instance confusion is already handled by I6 and
/// needs no wrapper.
pub struct Region<T> {
    region_id: NonZeroUsize,
    inner: slotmap::SlotMap<slotmap::DefaultKey, T>,
}

impl<T> Region<T> {
    /// Creates an empty region that allocates nothing until first use.
    ///
    /// # Panics
    ///
    /// Panics if the process-wide `region_id` counter has been exhausted —
    /// i.e. this would be the `2^{pointer_width}`-th `Region` constructed
    /// (via `new`/`with_capacity`/`Default`) in this process. See the I6
    /// doc block above for the exhaustion bound and why it is reachable,
    /// not just theoretical, on a 32-bit host.
    #[must_use]
    pub fn new() -> Self {
        Self {
            region_id: NonZeroUsize::new(NEXT_REGION_ID.fetch_add(1, Ordering::Relaxed))
                .expect("region_id overflow"),
            inner: slotmap::SlotMap::new(),
        }
    }

    /// Creates an empty region with space pre-reserved for `capacity` entries.
    ///
    /// # Panics
    ///
    /// Panics if `capacity > 2^32 - 3` (slotmap's maximum live-entry limit is
    /// `2^32 - 2`; reserving for sentinel gives `2^32 - 3`) — this is the
    /// guard that actually fires for any out-of-domain `capacity`, on both
    /// 32-bit and 64-bit hosts; on 64-bit this is a theoretical guard only
    /// (realistic workloads never approach this limit), but on a 32-bit host
    /// it is reachable. Also panics (as any `Vec`-backed container does) for
    /// any `capacity` whose slot array would exceed `isize::MAX` bytes —
    /// roughly `usize::MAX / size_of::<Slot<T>>()`; allocation failure beyond
    /// that aborts rather than panicking. Also panics if the process-wide
    /// `region_id` counter has been exhausted — see [`new`](Self::new)'s
    /// `# Panics` section and the I6 doc block above.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        // Reject capacity that would overflow slotmap's limit: max live entries is 2^32 - 2.
        // With one sentinel slot, the maximum reserve is 2^32 - 3.
        const SLOTMAP_MAX_RESERVE: usize = ((1u64 << 32) - 3) as usize;
        if capacity > SLOTMAP_MAX_RESERVE {
            panic!(
                "Region::with_capacity: capacity {} exceeds slotmap limit {}",
                capacity, SLOTMAP_MAX_RESERVE
            );
        }
        // Defense-in-depth, not a guard that can currently fire: the domain
        // check above already rejects any `capacity > SLOTMAP_MAX_RESERVE`
        // (2^32 - 3), so by this point `capacity + 1 <= 2^32 - 2`, which is
        // below `usize::MAX` on every supported target — `usize::MAX` is
        // `2^32 - 1` on the narrowest (32-bit) target this crate supports,
        // and far larger on 64-bit. This `checked_add(1)` therefore cannot
        // overflow today; it stays only so a future change to
        // `SLOTMAP_MAX_RESERVE` (e.g. if a future slotmap version raises or
        // removes its live-entry limit) can't silently reintroduce an
        // overflow here without a guard already in place to catch it.
        capacity
            .checked_add(1)
            .expect("Region::with_capacity: capacity overflow");
        Self {
            region_id: NonZeroUsize::new(NEXT_REGION_ID.fetch_add(1, Ordering::Relaxed))
                .expect("region_id overflow"),
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
    ///
    /// Note: the underlying `slotmap` provides no shrink/compact operation of any kind.
    /// Capacity — and therefore per-sweep iteration cost — is permanently bounded BELOW
    /// by the historical high-water mark of live entries. The only way to reclaim that
    /// cost is to build a fresh `Region` and re-insert (which invalidates every
    /// outstanding handle from the old one).
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
    /// reallocations.
    ///
    /// # Panics
    ///
    /// Panics if `len() + additional` overflows `usize`, in both debug and
    /// release builds — checked up front, before delegating to `slotmap`.
    /// Panics if `len() + additional > 2^32 - 2` (slotmap's maximum live-entry limit).
    /// Additionally panics (as any `Vec`-backed container does) for any
    /// `len() + additional` whose slot array would exceed `isize::MAX` bytes
    /// — roughly `usize::MAX / size_of::<Slot<T>>()`; allocation failure
    /// beyond that aborts rather than panicking.
    pub fn reserve(&mut self, additional: usize) {
        const SLOTMAP_MAX_LIVE: usize = ((1u64 << 32) - 2) as usize;
        let target = self
            .inner
            .len()
            .checked_add(additional)
            .expect("Region::reserve: capacity overflow");
        if target > SLOTMAP_MAX_LIVE {
            panic!(
                "Region::reserve: target {} exceeds slotmap limit {}",
                target, SLOTMAP_MAX_LIVE
            );
        }
        self.inner.reserve(additional);
    }

    /// Inserts `value`, returning a fresh handle that resolves to it (I1).
    ///
    /// # Panics
    ///
    /// Panics if the backing `slotmap` is full (2^32 - 2 live entries).
    #[must_use]
    pub fn insert(&mut self, value: T) -> Handle<T> {
        Handle::from_key_and_region(self.inner.insert(value), self.region_id)
    }

    /// Borrows the value for `handle`, or `None` if the handle is stale or
    /// removed (I1, I2, I3).
    #[must_use]
    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        if handle.region_id != self.region_id {
            return None;
        }
        self.inner.get(handle.key)
    }

    /// Mutably borrows the value for `handle`, or `None` if stale/removed.
    #[must_use]
    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        if handle.region_id != self.region_id {
            return None;
        }
        self.inner.get_mut(handle.key)
    }

    /// Whether `handle` currently resolves to a live value.
    #[must_use]
    pub fn contains(&self, handle: Handle<T>) -> bool {
        if handle.region_id != self.region_id {
            return false;
        }
        self.inner.contains_key(handle.key)
    }

    /// Removes and returns the value for `handle`, or `None` if it is already
    /// stale/removed. After this, `handle` resolves to `None` for roughly
    /// `2^31` reuse cycles of that slot (I2 — see the struct-level doc for
    /// the generation-wrap caveat).
    pub fn remove(&mut self, handle: Handle<T>) -> Option<T> {
        if handle.region_id != self.region_id {
            return None;
        }
        self.inner.remove(handle.key)
    }

    /// Iterates the live values. The order is unspecified and changes as
    /// elements are removed. Walks the underlying `SlotMap`'s slot array,
    /// skipping tombstone holes — so this is NOT cache-dense over live values
    /// (a `DenseSlotMap`-backed store would be); see
    /// <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/BENCHMARKS.md>.
    ///
    /// Note: iteration cost is proportional to the slot-array length, not to
    /// the live-value count. Since the underlying `slotmap` provides no shrink
    /// operation, the slot-array length is permanently bounded below by the
    /// historical high-water mark of live entries — a post-churn region with
    /// many holes pays iteration cost proportional to that high-water mark,
    /// even if few values remain live. See `capacity()`'s documentation for
    /// the full permanence semantics.
    ///
    /// The returned iterator implements `ExactSizeIterator`, `FusedIterator`,
    /// and `Clone`.
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            inner: self.inner.values(),
        }
    }

    /// Mutably iterates the live values (same non-dense order caveat as
    /// [`iter`](Self::iter)).
    ///
    /// The returned iterator implements `ExactSizeIterator` and `FusedIterator`.
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            inner: self.inner.values_mut(),
        }
    }

    /// Removes every value, invalidating all outstanding handles, while
    /// retaining allocated capacity. The region is reusable afterwards.
    ///
    /// Note: `clear` does NOT shrink the underlying slot array; the capacity
    /// remains at the historical high-water mark of live entries. See
    /// `capacity()`'s documentation for the full permanence semantics.
    ///
    /// If a value's `Drop` impl panics mid-`clear`, the clear is partial:
    /// the region stays fully consistent and reusable after unwinding, but
    /// the exact set of survivors depends on the underlying `slotmap` version's
    /// unwind cleanup (slotmap 1.x reserves the right to change this). What is
    /// guaranteed is that: (1) no value is dropped twice, (2) no value is leaked
    /// by the region itself (caller-side `mem::forget` of removed values is
    /// outside this guarantee), and (3) the region's internal accounting remains
    /// correct. See `tests/clear_partial_under_panic.rs`, which documents what
    /// the CURRENT slotmap version actually does -- an observation of the
    /// present dependency, not a stable contract this crate promises.
    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

impl<T> Default for Region<T> {
    /// # Panics
    ///
    /// Panics under the same condition as [`Region::new`] (process-wide
    /// `region_id` counter exhaustion) — this delegates to `new`.
    fn default() -> Self {
        Self::new()
    }
}

/// Note: the `region_id` field shown by this impl is minted from a
/// process-wide counter and is therefore NOT stable across separate runs or
/// processes (its value depends on how many other `Region`/`SyncRegion`
/// instances the process happened to construct first) — do not rely on it in
/// snapshot/golden-output tests.
impl<T> core::fmt::Debug for Region<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Region")
            .field("region_id", &self.region_id)
            .field("len", &self.len())
            .field("capacity", &self.capacity())
            .finish()
    }
}

// Note: `IntoIterator for Region<T>` (consuming by value) is deliberately NOT
// implemented. The underlying `slotmap::SlotMap::into_iter()` yields
// `(DefaultKey, T)` pairs, and exposing `slotmap::DefaultKey` would break
// this crate's encapsulation (raw keys must never escape through the public
// API). Iteration by reference (`&Region<T>` and `&mut Region<T>`) is
// provided below.

impl<'a, T> IntoIterator for &'a Region<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut Region<T> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// Iterator over the live values in a [`Region<T>`], returned by
/// [`Region::iter`] and `IntoIterator for &Region<T>`.
///
/// A thin wrapper over `slotmap`'s own values iterator — kept as a distinct
/// named type (rather than re-exporting `slotmap`'s type directly) so this
/// crate's public API surface never names a `slotmap` type, matching the
/// rest of this crate's encapsulation of its backing store.
pub struct Iter<'a, T> {
    inner: slotmap::basic::Values<'a, slotmap::DefaultKey, T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<T> ExactSizeIterator for Iter<'_, T> {
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<T> core::iter::FusedIterator for Iter<'_, T> {}

impl<T> Clone for Iter<'_, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> core::fmt::Debug for Iter<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Iter").field("len", &self.len()).finish()
    }
}

/// Mutable iterator over the live values in a [`Region<T>`], returned by
/// [`Region::iter_mut`] and `IntoIterator for &mut Region<T>`.
///
/// Same encapsulation rationale as [`Iter`] — not `Clone` (a mutable
/// iterator cannot be duplicated without aliasing `&mut` references).
pub struct IterMut<'a, T> {
    inner: slotmap::basic::ValuesMut<'a, slotmap::DefaultKey, T>,
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<T> ExactSizeIterator for IterMut<'_, T> {
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<T> core::iter::FusedIterator for IterMut<'_, T> {}

impl<T> core::fmt::Debug for IterMut<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("IterMut").field("len", &self.len()).finish()
    }
}
