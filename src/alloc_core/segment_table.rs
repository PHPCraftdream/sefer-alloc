//! [`SegmentTable`] — the global registry of all live segments, SELF-HOSTED in
//! the primordial segment's payload (not a `Vec` / `Box` on the global
//! allocator).
//!
//! This is the keystone of the Phase 8 Membrane Inversion (§1 of
//! `MALLOC_PLAN.md`): the safe slot-table discipline stops *consuming* memory
//! (via `Vec`/`HashSet`) and starts *governing* it. The registry lives inside
//! the very segments it tracks, so the alloc path can mutate it without ever
//! calling the global allocator (M5 — reentrancy-freedom).
//!
//! ## Design
//!
//! - A fixed-capacity array of segment-base pointers, carved from the
//!   primordial segment at a known offset. Capacity is bounded by
//!   [`MAX_SEGMENTS`] — sufficient for the single-threaded Phase 8 surface
//!   (each large/huge allocation is its own segment; small allocations pack
//!   many per segment). Phase 9+ may grow this dynamically (still self-hosted)
//!   if a workload genuinely needs more.
//! - `segment_count` tracks how many entries are live. `drop` walks the table
//!   and frees every segment EXCEPT the primordial (which is freed last, after
//!   the table itself is no longer needed — special-cased by `AllocCore::drop`).
//! - O(1) `segment_of(ptr) = ptr & ~(SEGMENT-1)` lives in [`super::os`] and
//!   yields the segment base; routing then reads the header at offset 0. The
//!   table is only needed for census/drop, not the hot path.
//!
//! ## Safety
//!
//! The table is plain data (a `*mut u8` array + a count) laid down in segment
//! memory; mutation goes through the [`node`](super::node) seam's pointer
//! writes for the slots. The bootstrap (`super::bootstrap`) is the ONLY place
//! that hand-writes the table in place before the safe Cartographer takes over.

use core::mem::size_of;

/// Maximum number of live segments the registry can hold. Each large/huge
/// allocation consumes one segment; small allocations pack many per segment,
/// so this bounds the simultaneous large-allocation count. ~1024 segments ×
/// 4 MiB = 4 GiB of simultaneously-live large allocations — generous for a
/// single-threaded Phase 8 surface. Bumped if a real workload needs more.
pub(crate) const MAX_SEGMENTS: usize = 1024;

/// The footprint of the registry array in the primordial segment. Fixed and
/// known at compile time so the bootstrap can carve it deterministically.
pub(crate) const REGISTRY_FOOTPRINT: usize = MAX_SEGMENTS * size_of::<*mut u8>();

/// A self-hosted segment registry: a fixed-capacity array of segment-base
/// pointers plus a live count, carved from the primordial segment.
///
/// The registry does NOT own the segments it lists — ownership lives with
/// [`super::AllocCore`] (which holds the owning [`super::os::Segment`]
/// handles). The registry is the *index* over them, used by drop/census; the
/// hot path resolves owners via `segment_of(ptr)` (no registry lookup).
pub(crate) struct SegmentTable {
    /// Pointer to the first slot of the registry array (lives in the
    /// primordial segment's payload). `MAX_SEGMENTS` entries.
    slots: *mut *mut u8,
    /// Number of live segments currently registered. Segment 0 (the
    /// primordial) is always registered as index 0.
    count: u32,
}

impl SegmentTable {
    /// Construct the registry view over an already-laid-down array in the
    /// primordial segment. Used by the bootstrap after it has carved the slot
    /// array (the bootstrap writes slot 0 through the `node` seam BEFORE
    /// calling this — this constructor performs NO memory operation, it just
    /// wraps the pointer + count).
    ///
    /// # Caller's contract
    ///
    /// `slots` must point to `REGISTRY_FOOTPRINT` bytes inside the primordial
    /// segment, with slot 0 already set to the primordial base. `count` is the
    /// current live count (1 for just the primordial). This method is safe
    /// because it does not touch memory — it only stores the pointer; the
    /// contract is the caller's invariant, enforced by the bootstrap being the
    /// sole caller.
    pub(crate) fn from_primordial(slots: *mut *mut u8, count: u32) -> Self {
        Self { slots, count }
    }

    /// Register a new segment base. Returns its assigned `segment_id` (which
    /// equals the index it was placed at). Panics if the table is full (a
    /// bounded-capacity invariant — Phase 8 surface is sized so this never
    /// fires for legitimate workloads; future phases grow the table).
    pub(crate) fn register(&mut self, base: *mut u8) -> u32 {
        let idx = self.count as usize;
        assert!(
            idx < MAX_SEGMENTS,
            "SegmentTable full: MAX_SEGMENTS exceeded (bump MAX_SEGMENTS)"
        );
        // The write goes through the `node` seam — this file is pure safe
        // composition. `Node::offset` computes the address (the unsafe `add`
        // lives in the seam); `Node::write_struct` does the proven write.
        let slot = super::node::Node::offset(self.slots as *mut u8, idx * core::mem::size_of::<*mut u8>())
            as *mut *mut u8;
        super::node::Node::write_struct::<*mut u8>(slot, base);
        self.count += 1;
        idx as u32
    }

    /// The current number of live segments (including the primordial).
    #[allow(dead_code)] // Substrate introspection; tests / Phase 9 use it.
    pub(crate) fn count(&self) -> u32 {
        self.count
    }

    /// Whether `base` is one of our registered segment bases. Used by the
    /// defensive foreign-pointer check in `dealloc`: a pointer whose computed
    /// segment base is NOT in this set is foreign (not one of our allocations)
    /// and is treated as a no-op. O(segments) — acceptable for the defensive
    /// path; the hot path (known-live pointer) skips this via the magic check.
    pub(crate) fn contains_base(&self, base: *mut u8) -> bool {
        for b in self.bases() {
            if b == base {
                return true;
            }
        }
        false
    }

    /// Iterate over all registered segment bases (read-only). Used by drop to
    /// free every segment.
    pub(crate) fn bases(&self) -> impl Iterator<Item = *mut u8> {
        let slots = self.slots;
        let n = self.count as usize;
        (0..n).map(move |i| {
            // The read goes through the `node` seam.
            let slot = super::node::Node::offset(slots as *mut u8, i * core::mem::size_of::<*mut u8>())
                as *const *mut u8;
            super::node::Node::read_struct::<*mut u8>(slot)
        })
    }
}
