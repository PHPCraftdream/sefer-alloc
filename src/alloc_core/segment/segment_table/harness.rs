use super::{
    ActiveKindIndex, SegmentTable, FREE_LIST_CAPACITY, HASH_CAPACITY, MAX_SEGMENTS, SEGMENT_SHIFT,
};
use crate::alloc_core::segment_header::SegmentKind;

// -----------------------------------------------------------------------
// R4-8/N3 — TEST-ONLY harness for direct exercise of the open-addressing
// hash operations with SYNTHETIC SEGMENT-aligned bases.
//
// `SegmentTable` is `pub(crate)` and its hash helpers (`hash_insert`/
// `hash_remove`/`hash_contains`) are private, so the integration-test
// property test (`tests/segment_table_backshift_proptest.rs`) cannot reach
// them directly. This `#[doc(hidden)] pub` harness owns its own backing
// storage and re-exposes the three hash operations, so the test drives the
// EXACT backward-shift deletion code path with full control over which hash
// indices are occupied — including the cyclic wrap-around past
// `HASH_CAPACITY-1 → 0`, which is untestable through the real `AllocCore`
// API because the OS hands out segment bases whose hash indices never
// deterministically straddle the table boundary.
//
// The harness bases are SYNTHETIC pointer VALUES: they are stored in and
// compared by the hash table but NEVER dereferenced, so any nonzero value is
// safe. `SegmentHashHarness::base_for_index(h)` yields a distinct nonzero
// SEGMENT-aligned value whose `hash_index` is exactly `h`.
//
// `pub` (not `pub(crate)`) only because `alloc_core` itself is
// `#[doc(hidden)]` (see `lib.rs`): the public surface is test-only (the
// `#[doc(hidden)]` re-export in `mod.rs`), reachable by the isolated backshift
// property test. Nothing here is stable public API.
// -----------------------------------------------------------------------

/// Test-only handle to a `SegmentTable`'s hash operations with synthetic
/// (non-dereferenced) bases. See the module-level comment above.
#[doc(hidden)]
pub struct SegmentHashHarness {
    table: SegmentTable,
    // Owns the carved arrays so they outlive the `SegmentTable` view. Sized
    // once at construction and never grown, so the raw pointers handed to
    // `from_primordial` stay valid for the harness's lifetime.
    _slots: Vec<*mut u8>,
    _hash: Vec<*mut u8>,
    _free_list: Vec<u32>,
    _free_top: Vec<u32>,
    _active_kind: Vec<u64>,
}

#[doc(hidden)]
impl SegmentHashHarness {
    /// Logical membership fixture; synthetic hash roots are never read.
    pub fn active_set(&mut self, slot: usize, large: bool) {
        ActiveKindIndex::set(
            self.table.active_kind,
            slot,
            if large {
                SegmentKind::Large
            } else {
                SegmentKind::Small
            },
        );
    }

    pub fn active_clear(&mut self, slot: usize, large: bool) {
        ActiveKindIndex::clear(
            self.table.active_kind,
            slot,
            if large {
                SegmentKind::Large
            } else {
                SegmentKind::Small
            },
        );
    }

    pub fn active_next(&self, from: usize, large: bool) -> Option<usize> {
        ActiveKindIndex::next(
            self.table.active_kind,
            if large {
                SegmentKind::Large
            } else {
                SegmentKind::Small
            },
            from,
        )
    }

    /// Rejection occurs before a synthetic root could be dereferenced.
    pub fn unknown_registration_is_unchanged(&mut self) -> bool {
        let count = self.table.count();
        let top = self._free_top[0];
        let slot = self._slots[1];
        let base = Self::base_for_index(1);
        self.table.register(base, 1, SegmentKind::Unknown).is_none()
            && self.table.count() == count
            && self._free_top[0] == top
            && self._slots[1] == slot
            && self.active_next(0, false) == Some(0)
            && self.active_next(0, true).is_none()
    }

    /// Build an EMPTY hash table over heap-owned backing storage. Slot 0 is
    /// a synthetic primordial root; hash-only tests never register it.
    pub fn new() -> Self {
        let mut slots: Vec<*mut u8> = vec![core::ptr::null_mut(); MAX_SEGMENTS];
        slots[0] = Self::base_for_index(0);
        let mut hash: Vec<*mut u8> = vec![core::ptr::null_mut(); HASH_CAPACITY];
        let mut free_list: Vec<u32> = vec![0u32; FREE_LIST_CAPACITY];
        let mut free_top: Vec<u32> = vec![0u32; 1];
        let mut active_kind = vec![0u64; ActiveKindIndex::FOOTPRINT / 8];
        let active_kind_ptr = active_kind.as_mut_ptr().cast::<ActiveKindIndex>();
        ActiveKindIndex::init_in_place(active_kind_ptr);
        // `from_primordial` performs no memory operation — it only stores the
        // pointers. The Vecs' heap allocations are stable across the move into
        // `Self` and are never reallocated, so the stored pointers remain valid.
        let table = SegmentTable::from_primordial(
            slots.as_mut_ptr(),
            1,
            hash.as_mut_ptr(),
            free_list.as_mut_ptr(),
            free_top.as_mut_ptr(),
            active_kind_ptr,
        );
        Self {
            table,
            _slots: slots,
            _hash: hash,
            _free_list: free_list,
            _free_top: free_top,
            _active_kind: active_kind,
        }
    }

    /// Insert `base` (a synthetic nonzero SEGMENT-aligned pointer value; never
    /// dereferenced). Precondition: `base` is not already present and the load
    /// factor is ≤ 50%.
    pub fn insert(&mut self, base: *mut u8) {
        self.table.hash_insert(base);
    }

    /// Register an address key whose canonical root has independent provenance.
    /// Only one such entry may be live in this harness at a time.
    pub fn insert_root_for_key(&mut self, key: *mut u8, root: *mut u8) {
        assert!(!root.is_null());
        assert_eq!(key.addr() & ((1 << SEGMENT_SHIFT) - 1), 0);
        self._slots[0] = root;
        self.table.hash_insert_identity(key, 0);
    }

    /// Evict the keyed entry and its root before reusing the same address key.
    pub fn remove_root_for_key(&mut self, key: *mut u8, root: *mut u8) {
        assert_eq!(self._slots[0], root);
        self.table.hash_remove(key);
        self.table.own_cache_clear(root);
        self._slots[0] = Self::base_for_index(0);
    }

    /// Model an invalid hash identity with no stored root; no pointer is read.
    pub fn hide_root_for_test(&mut self, root: *mut u8) {
        assert_eq!(self._slots[0], root);
        self._slots[0] = core::ptr::null_mut();
    }

    /// Remove `base` via backward-shift deletion. Defensive no-op if `base` is
    /// not present.
    pub fn remove(&mut self, base: *mut u8) {
        self.table.hash_remove(base);
        self.table.own_cache_clear(base);
    }

    /// O(1)-average membership test for `base`.
    pub fn contains(&self, base: *mut u8) -> bool {
        self.table.hash_contains(base)
    }

    /// Exercise the cache-fill path with an address-only lookup key.
    pub fn contains_cached(&mut self, key: *mut u8) -> bool {
        self.table.contains_base(key)
    }

    /// Return the table's canonical entry, including on a cache hit.
    pub fn canonical(&self, key: *mut u8) -> Option<*mut u8> {
        self.table.canonical_base_of(key)
    }

    /// A synthetic, distinct, nonzero, SEGMENT-aligned pointer VALUE whose
    /// `hash_index` is exactly `index` (mod `HASH_CAPACITY`). The value is
    /// never dereferenced — it is only stored/compared by the hash table.
    /// Adding `HASH_CAPACITY` before shifting keeps every value nonzero (so it
    /// is never confused with the `null_mut()` empty marker) for any `index`.
    pub fn base_for_index(index: usize) -> *mut u8 {
        let high = (index + HASH_CAPACITY) & !(HASH_CAPACITY - 1);
        let low = (index ^ (high >> 13) ^ (high >> 26)) & (HASH_CAPACITY - 1);
        core::ptr::without_provenance_mut::<u8>((high | low) << SEGMENT_SHIFT)
    }

    /// The hash-table capacity (`HASH_CAPACITY`), re-exposed for the property
    /// test so it can size universes and target the wrap boundary.
    pub const CAPACITY: usize = HASH_CAPACITY;
}

impl Default for SegmentHashHarness {
    fn default() -> Self {
        Self::new()
    }
}
