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
//!   [`MAX_SEGMENTS`] — the **high-water mark** of ever-simultaneously-live
//!   segments, not a hard per-workload cap. Under `alloc-decommit`, empty
//!   segments are recycled: their slot is set to NULL and reused by a future
//!   `register`, so the effective live-segment count is unbounded (task #60
//!   — slot-recycle variant B).
//! - `segment_count` is the **high-water mark** of slots ever written.  It
//!   never decreases.  A NULL entry in `slots[0..count)` is a **recyclable
//!   slot** — the OS reservation for that segment has already been released
//!   (by [`recycle`](SegmentTable::recycle)); the slot is available for the
//!   next [`register`](SegmentTable::register) call.
//! - `drop` walks only non-NULL slots and frees each OS reservation. NULL
//!   slots are already freed and skipped.
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
//!
//! ## Slot encoding (task #60 — NULL = recyclable)
//!
//! - A **live slot** holds the segment base pointer (non-NULL, SEGMENT-aligned).
//! - A **recyclable slot** holds `null_mut()`. The corresponding OS reservation
//!   has already been released; the virtual address is no longer valid.
//! - [`register`] scans left-to-right for a recyclable slot first, reusing it
//!   before appending. This keeps `count` at its high-water mark and never
//!   wastes live slots.
//! - [`recycle`] finds the slot for a given base, releases the OS reservation,
//!   then writes NULL. These two operations happen in `decommit_empty_segment`
//!   as a unit, so there is never a window where the OS reservation is released
//!   but the slot is still non-NULL (which would cause `drop` to double-free).

use core::mem::size_of;

/// Maximum number of simultaneously live segments the registry can hold WITHOUT
/// recycling. Each live large/huge allocation consumes one segment slot; each
/// small segment can serve thousands of small allocations. Under `alloc-decommit`
/// the effective limit is unbounded: empty segments are recycled (their slot is
/// NULLed) and reused by future `register` calls. Without `alloc-decommit` this
/// is the hard cap (append-only).
pub(crate) const MAX_SEGMENTS: usize = 1024;

/// The footprint of the registry array in the primordial segment. Fixed and
/// known at compile time so the bootstrap can carve it deterministically.
pub(crate) const REGISTRY_FOOTPRINT: usize = MAX_SEGMENTS * size_of::<*mut u8>();

/// A self-hosted segment registry: a fixed-capacity array of segment-base
/// pointers plus a high-water count, carved from the primordial segment.
///
/// The registry does NOT own the segments it lists — ownership lives with
/// [`super::AllocCore`] (which holds the owning [`super::os::Segment`]
/// handles). The registry is the *index* over them, used by drop/census; the
/// hot path resolves owners via `segment_of(ptr)` (no registry lookup).
///
/// Under `alloc-decommit`, recycled slots hold `null_mut()` — the OS
/// reservation for those segments has been released. [`bases`](Self::bases)
/// filters them out; [`register`] reuses them before appending.
pub(crate) struct SegmentTable {
    /// Pointer to the first slot of the registry array (lives in the
    /// primordial segment's payload). `MAX_SEGMENTS` entries.
    slots: *mut *mut u8,
    /// High-water mark: the number of slots that have EVER been written
    /// (including currently-NULL recyclable slots). Segments 0 (the
    /// primordial) is always at index 0 and is never recycled.
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

    /// Register a new segment base. Returns its assigned `segment_id` (the
    /// index it was placed at), or `None` if the table is full (all slots are
    /// live and count == MAX_SEGMENTS — only possible without `alloc-decommit`
    /// or under an extreme large-allocation storm).
    ///
    /// **Slot-recycle scan (task #60, variant B):** before appending, scans
    /// slots `[0, count)` left-to-right for a NULL (recyclable) slot. If one
    /// is found the new base is written there and that index is returned (NO
    /// increment of `count` — the slot is already within the live window). This
    /// lifts the 1024-segment cap under `alloc-decommit`: as long as some
    /// slots are recycled, `register` never returns `None`.
    ///
    /// no-panic (Phase 11 GlobalAlloc face): returns `None` so the caller
    /// returns null (graceful OOM) rather than aborting.
    pub(crate) fn register(&mut self, base: *mut u8) -> Option<u32> {
        // Phase 60 — scan for a recyclable (NULL) slot first.
        // Linear scan through the live window; skips slot 0 (the primordial,
        // never recycled) but searches the whole [0, count) range for any NULL
        // left by a prior `recycle` call. The primordial base is never NULL so
        // slot 0 is safely visited (the comparison fails quickly).
        let count = self.count as usize;
        for i in 0..count {
            let slot = Self::slot_ptr(self.slots, i);
            let current = super::node::Node::read_struct::<*mut u8>(slot);
            if current.is_null() {
                // Reuse this slot: write the new base, return the index without
                // bumping count (the slot is already in the live window).
                super::node::Node::write_struct::<*mut u8>(slot, base);
                return Some(i as u32);
            }
        }
        // No recyclable slot found — try to append.
        let idx = count;
        if idx >= MAX_SEGMENTS {
            return None;
        }
        let slot = Self::slot_ptr(self.slots, idx);
        super::node::Node::write_struct::<*mut u8>(slot, base);
        self.count += 1;
        Some(idx as u32)
    }

    /// Mark the slot for `base` as recyclable (NULL) and release the segment's
    /// OS reservation. Called from `decommit_empty_segment` AFTER the segment's
    /// payload has been decommitted and its metadata reset.
    ///
    /// **Contract (caller's invariant):**
    /// - The segment at `base` MUST be a live, empty, non-primordial `Small`
    ///   segment whose `live_count == 0`.
    /// - The segment's OS reservation MUST NOT have been released yet. This
    ///   function releases it internally (reads `(reservation, reservation_len)`
    ///   from the header, calls `os::release_segment`, then NULLs the slot).
    ///   After this call the virtual address `base` is invalid — the caller
    ///   MUST NOT dereference it.
    /// - Must be called on the owner thread only (serialized by the
    ///   single-threaded `AllocCore` discipline).
    ///
    /// **Why OS release happens here (not in the caller):** if we released the
    /// OS segment and then NULLed the slot in two separate steps, a crash
    /// between them would leave the slot non-NULL with an invalid virtual
    /// address; `drop` would then read a released mapping. By doing both
    /// atomically in one function (on the owner thread) we guarantee the slot
    /// is NULLed before anything else can observe the OS release.
    ///
    /// If `base` is not found in the table (shouldn't happen under the correct
    /// invariant) this is a no-op — a defensive guard, not a panic.
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn recycle(&mut self, base: *mut u8) {
        // Read the reservation info from the segment BEFORE releasing. The
        // metadata pages (which host the header at offset 0) are NEVER
        // decommitted — only the payload is — so the header is still readable.
        let hdr = super::segment_header::SegmentHeader::read_at(base);
        let reservation = hdr.reservation;
        let reservation_len = hdr.reservation_len;
        // Find the slot that holds `base` and write NULL. Linear scan O(count)
        // — acceptable since this is the cold decommit path, not the hot alloc
        // path. We compare pointer VALUES only; we never dereference `base`
        // after the OS release below, so the order matters: NULL the slot
        // AFTER reading the header and AFTER the OS release.
        let count = self.count as usize;
        for i in 0..count {
            let slot = Self::slot_ptr(self.slots, i);
            let current = super::node::Node::read_struct::<*mut u8>(slot);
            if current == base {
                // Release the OS reservation. After this, `base` is invalid
                // (unmapped). We do NOT dereference `base` after this point.
                super::os::release_segment(reservation, reservation_len);
                // NULL the slot so `register` can reuse it and `drop` skips it.
                super::node::Node::write_struct::<*mut u8>(slot, core::ptr::null_mut());
                return;
            }
        }
        // Defensive: `base` was not found. This indicates a bug in the caller
        // (double-recycle or never-registered). Release the OS reservation
        // anyway to avoid a leak, but don't corrupt the table.
        super::os::release_segment(reservation, reservation_len);
    }

    /// The high-water mark: the number of slots ever written (including
    /// currently-NULL recyclable slots). The number of LIVE (non-NULL)
    /// segments is `self.bases().count()`.
    #[allow(dead_code)] // Substrate introspection; tests / Phase 9 use it.
    pub(crate) fn count(&self) -> u32 {
        self.count
    }

    /// Whether `base` is one of our registered, LIVE (non-NULL) segment bases.
    /// Used by the defensive foreign-pointer check in `dealloc`: a pointer
    /// whose computed segment base is NOT in this set is foreign (not one of
    /// our allocations) and is treated as a no-op. O(segments) — acceptable
    /// for the defensive path; the hot path (known-live pointer) skips this
    /// via the magic check.
    ///
    /// Recycled (NULL) slots are NOT considered as matching any base, so a
    /// use-after-recycle pointer is correctly treated as foreign.
    pub(crate) fn contains_base(&self, base: *mut u8) -> bool {
        for b in self.bases() {
            if b == base {
                return true;
            }
        }
        false
    }

    /// Iterate over all **live** (non-NULL) registered segment bases
    /// (read-only). Skips NULL slots that were recycled by a prior
    /// [`recycle`](Self::recycle) call. Used by:
    /// - `AllocCore::drop` to collect every live segment's OS reservation for
    ///   release. NULL slots are already released — skipping them prevents
    ///   double-free.
    /// - `find_segment_with_free` to scan segments for a free block.
    /// - `contains_base` (defensive dealloc check).
    pub(crate) fn bases(&self) -> impl Iterator<Item = *mut u8> {
        let slots = self.slots;
        let n = self.count as usize;
        (0..n)
            .map(move |i| {
                let slot = Self::slot_ptr(slots, i) as *const *mut u8;
                super::node::Node::read_struct::<*mut u8>(slot)
            })
            .filter(|&p| !p.is_null())
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Address of slot `i` within the registry array. Pure pointer arithmetic
    /// through the `node` seam.
    #[inline]
    fn slot_ptr(slots: *mut *mut u8, i: usize) -> *mut *mut u8 {
        super::node::Node::offset(slots as *mut u8, i * core::mem::size_of::<*mut u8>())
            as *mut *mut u8
    }
}
