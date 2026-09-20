use super::{SegmentTable, FREE_LIST_CAPACITY, HASH_CAPACITY, MAX_SEGMENTS, SEGMENT_SHIFT};

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
}

#[doc(hidden)]
impl SegmentHashHarness {
    /// Build an EMPTY hash table over heap-owned backing storage. The slot
    /// registry `count` is 0: the harness exercises the hash helpers
    /// directly and never calls `register`/`unregister`/`recycle`.
    pub fn new() -> Self {
        let mut slots: Vec<*mut u8> = vec![core::ptr::null_mut(); MAX_SEGMENTS];
        let mut hash: Vec<*mut u8> = vec![core::ptr::null_mut(); HASH_CAPACITY];
        let mut free_list: Vec<u32> = vec![0u32; FREE_LIST_CAPACITY];
        let mut free_top: Vec<u32> = vec![0u32; 1];
        // `from_primordial` performs no memory operation — it only stores the
        // pointers. The Vecs' heap allocations are stable across the move into
        // `Self` and are never reallocated, so the stored pointers remain valid.
        let table = SegmentTable::from_primordial(
            slots.as_mut_ptr(),
            0,
            hash.as_mut_ptr(),
            free_list.as_mut_ptr(),
            free_top.as_mut_ptr(),
        );
        Self {
            table,
            _slots: slots,
            _hash: hash,
            _free_list: free_list,
            _free_top: free_top,
        }
    }

    /// Insert `base` (a synthetic nonzero SEGMENT-aligned pointer value; never
    /// dereferenced). Precondition: `base` is not already present and the load
    /// factor is ≤ 50%.
    pub fn insert(&mut self, base: *mut u8) {
        self.table.hash_insert(base);
    }

    /// Remove `base` via backward-shift deletion. Defensive no-op if `base` is
    /// not present.
    pub fn remove(&mut self, base: *mut u8) {
        self.table.hash_remove(base);
    }

    /// O(1)-average membership test for `base`.
    pub fn contains(&self, base: *mut u8) -> bool {
        self.table.hash_contains(base)
    }

    /// A synthetic, distinct, nonzero, SEGMENT-aligned pointer VALUE whose
    /// `hash_index` is exactly `index` (mod `HASH_CAPACITY`). The value is
    /// never dereferenced — it is only stored/compared by the hash table.
    /// Adding `HASH_CAPACITY` before shifting keeps every value nonzero (so it
    /// is never confused with the `null_mut()` empty marker) for any `index`.
    pub fn base_for_index(index: usize) -> *mut u8 {
        core::ptr::without_provenance_mut::<u8>((index + HASH_CAPACITY) << SEGMENT_SHIFT)
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
