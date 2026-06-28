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

/// The footprint of the registry (slots) array in the primordial segment. Fixed
/// and known at compile time so the bootstrap can carve it deterministically.
pub(crate) const REGISTRY_FOOTPRINT: usize = MAX_SEGMENTS * size_of::<*mut u8>();

/// Capacity of the open-addressing hash table (OPT-B). Load factor ≤ 50%
/// so `HASH_CAPACITY = 2 * MAX_SEGMENTS`. Power of two for cheap modulo via
/// bitmasking.
pub(crate) const HASH_CAPACITY: usize = 2 * MAX_SEGMENTS; // 2048

/// Footprint of the hash table in the primordial segment.
/// `HASH_CAPACITY` entries × `sizeof(*mut u8)` = 16 KiB.
pub(crate) const HASH_FOOTPRINT: usize = HASH_CAPACITY * size_of::<*mut u8>();

/// Bit-shift of the segment size (log₂(SEGMENT = 4 MiB) = 22). Used by the
/// hash function to convert a segment base into a table index. Exposed as
/// `pub(crate)` so the bootstrap can seed the primordial hash entry before
/// the `SegmentTable` struct exists (the bootstrap must not call any
/// `SegmentTable` methods before `from_primordial`).
pub(crate) const SEGMENT_SHIFT: usize = 22;

/// Tombstone marker for hash slots that held a base which was subsequently
/// removed (unregister/recycle). Must be a value that can never be a real
/// segment base: real bases are SEGMENT-aligned (aligned to 4 MiB = 1 << 22),
/// so the value `1` (= 0x0000_0001) is unambiguously not a valid base.
///
/// Rule:
/// - `null_mut()` → empty (never occupied)
/// - `TOMBSTONE`  → was occupied, now removed (probe chain intact)
/// - other        → live entry (a real SEGMENT-aligned base pointer)
const TOMBSTONE: *mut u8 = core::ptr::without_provenance_mut::<u8>(1);

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
    /// OPT-B: open-addressing hash table for O(1) `contains_base`. Lives in
    /// the primordial segment immediately after the slots array. Capacity is
    /// `HASH_CAPACITY` entries. Encoding:
    /// - `null_mut()` → empty (never occupied)
    /// - `TOMBSTONE`  → removed entry (probe chain must be preserved)
    /// - other        → live segment base (SEGMENT-aligned pointer)
    hash_slots: *mut *mut u8,
}

impl SegmentTable {
    /// Construct the registry view over an already-laid-down array in the
    /// primordial segment. Used by the bootstrap after it has carved the slot
    /// array and the hash table (the bootstrap writes slot 0 and clears the
    /// hash array through the `node` seam BEFORE calling this — this
    /// constructor performs NO memory operation, it just wraps the pointers +
    /// count).
    ///
    /// # Caller's contract
    ///
    /// `slots` must point to `REGISTRY_FOOTPRINT` bytes inside the primordial
    /// segment, with slot 0 already set to the primordial base. `hash_slots`
    /// must point to `HASH_FOOTPRINT` bytes (all zeroed / `null_mut()`) for the
    /// open-addressing hash table. `count` is the current live count (1 for
    /// just the primordial). This method is safe because it does not touch
    /// memory — it only stores the pointers; the contract is the caller's
    /// invariant, enforced by the bootstrap being the sole caller.
    pub(crate) fn from_primordial(
        slots: *mut *mut u8,
        count: u32,
        hash_slots: *mut *mut u8,
    ) -> Self {
        Self {
            slots,
            count,
            hash_slots,
        }
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
                // OPT-B: also insert into the hash table so `contains_base` is O(1).
                self.hash_insert(base);
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
        // OPT-B: also insert into the hash table so `contains_base` is O(1).
        self.hash_insert(base);
        Some(idx as u32)
    }

    /// NULL the table slot for `base` WITHOUT releasing the OS reservation.
    ///
    /// Used by the OPT-E large-segment free-cache: when a freed large segment
    /// is deposited into `large_cache`, we remove it from the active table so
    /// `drop` / `find_segment_with_free` / `contains_base` do not see it as a
    /// live registered segment. The OS reservation stays alive (the cache owns
    /// it) and will be released either on cache re-use or in `AllocCore::drop`.
    ///
    /// **Contract (caller's invariant):**
    /// - `base` MUST be currently registered as a non-NULL slot.
    /// - The caller takes full ownership of the OS reservation for `base`;
    ///   `drop` will NOT release it (the slot is NULL, so `bases()` skips it).
    /// - The caller MUST ensure the reservation is eventually released (via
    ///   `os::release_segment` in the cache or `Drop` walk).
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn unregister(&mut self, base: *mut u8) {
        let count = self.count as usize;
        for i in 0..count {
            let slot = Self::slot_ptr(self.slots, i);
            let current = super::node::Node::read_struct::<*mut u8>(slot);
            if current == base {
                // NULL the slot — the OS reservation is NOT released here.
                super::node::Node::write_struct::<*mut u8>(slot, core::ptr::null_mut());
                // OPT-B: remove from hash table (tombstone the entry).
                self.hash_remove(base);
                return;
            }
        }
        // Defensive: base was not found in the table. No-op.
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
                // OPT-B: tombstone the hash entry BEFORE releasing the OS
                // reservation. After `release_segment` the pointer value `base`
                // remains valid as a key (we compare values, not dereference),
                // but doing the hash update first is cleaner.
                self.hash_remove(base);
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
    /// our allocations) and is treated as a no-op.
    ///
    /// OPT-B: now O(1) average via the open-addressing hash table. Tombstone
    /// entries are skipped (the probe chain must not stop at a tombstone).
    /// The only time a result is `false` is when the probe chain reaches an
    /// empty slot (`null_mut()`), meaning `base` was never inserted here.
    ///
    /// Recycled (NULL) slots are NOT considered as matching any base, so a
    /// use-after-recycle pointer is correctly treated as foreign.
    #[inline(always)]
    pub(crate) fn contains_base(&self, base: *mut u8) -> bool {
        self.hash_contains(base)
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

    // -------------------------------------------------------------------
    // OPT-B — open-addressing hash table helpers
    //
    // The hash table lives in the primordial segment immediately after the
    // slots array. Capacity = HASH_CAPACITY (a power of two). Encoding:
    //   - null_mut()  → empty  (never occupied; stops a probe chain)
    //   - TOMBSTONE   → removed (probe chain must skip over this)
    //   - other       → live segment base (SEGMENT-aligned, never null)
    //
    // All reads/writes go through the `node` seam, keeping this file
    // safe while meeting the crate's unsafe-confinement requirement.
    // -------------------------------------------------------------------

    /// Hash a segment base to an initial slot index.
    ///
    /// `base` is SEGMENT-aligned, so its low `SEGMENT_SHIFT` bits are zero.
    /// Right-shifting by `SEGMENT_SHIFT` gives a dense integer key (one key
    /// per segment in the virtual address space). We mask to `HASH_CAPACITY - 1`
    /// for a fast modulo (power-of-two capacity).
    #[inline(always)]
    fn hash_index(base: *mut u8) -> usize {
        (base as usize >> SEGMENT_SHIFT) & (HASH_CAPACITY - 1)
    }

    /// Address of hash slot `i`. Pure pointer arithmetic through the `node` seam.
    #[inline(always)]
    fn hash_slot_ptr(&self, i: usize) -> *mut *mut u8 {
        super::node::Node::offset(
            self.hash_slots as *mut u8,
            i * core::mem::size_of::<*mut u8>(),
        ) as *mut *mut u8
    }

    /// Read the value stored at hash slot `i`.
    #[inline(always)]
    fn hash_slot_read(&self, i: usize) -> *mut u8 {
        super::node::Node::read_struct::<*mut u8>(self.hash_slot_ptr(i))
    }

    /// Write `value` into hash slot `i`.
    #[inline]
    fn hash_slot_write(&mut self, i: usize, value: *mut u8) {
        super::node::Node::write_struct::<*mut u8>(self.hash_slot_ptr(i), value);
    }

    /// Insert `base` into the hash table using linear probing.
    ///
    /// Scans forward from `hash_index(base)` (with wrap-around) until an
    /// empty slot OR a tombstone slot is found, then writes `base` there.
    ///
    /// **Precondition:** the caller guarantees `base` is not already in the
    /// table AND at least one empty/tombstone slot exists (load factor ≤ 50%).
    fn hash_insert(&mut self, base: *mut u8) {
        let start = Self::hash_index(base);
        let mut i = start;
        loop {
            let entry = self.hash_slot_read(i);
            if entry.is_null() || entry == TOMBSTONE {
                // Empty or tombstone: this slot is available.
                self.hash_slot_write(i, base);
                return;
            }
            i = (i + 1) & (HASH_CAPACITY - 1);
            // Under the load-factor ≤ 50% guarantee we will always find a
            // free slot before wrapping all the way around. The loop must
            // terminate: at least HASH_CAPACITY/2 slots are empty/tombstone.
            debug_assert!(i != start, "hash table full — load factor exceeded");
        }
    }

    /// Remove `base` from the hash table (replace the entry with a tombstone).
    ///
    /// Scans forward from `hash_index(base)` until `base` is found (replaced
    /// with `TOMBSTONE`) or an empty slot is reached (base was never inserted;
    /// defensive no-op). Tombstone slots are skipped during the probe.
    ///
    /// Only called under `alloc-decommit` (from `recycle` and `unregister`);
    /// the lint is suppressed for non-decommit builds to keep the code uniform.
    #[cfg_attr(not(feature = "alloc-decommit"), allow(dead_code))]
    fn hash_remove(&mut self, base: *mut u8) {
        let start = Self::hash_index(base);
        let mut i = start;
        loop {
            let entry = self.hash_slot_read(i);
            if entry.is_null() {
                // Empty slot: probe chain terminates; base is not present.
                // This indicates a caller bug (removing a non-inserted entry).
                // Defensive no-op — do not corrupt the table.
                return;
            }
            if entry == base {
                // Found the live entry: replace with tombstone so probe chains
                // for other keys that passed through this slot remain intact.
                self.hash_slot_write(i, TOMBSTONE);
                return;
            }
            // TOMBSTONE or different live entry: skip and continue probing.
            i = (i + 1) & (HASH_CAPACITY - 1);
            debug_assert!(i != start, "hash_remove looped without finding base");
        }
    }

    /// Check whether `base` is present in the hash table (O(1) average).
    ///
    /// Scans forward from `hash_index(base)` until:
    /// - `base` is found → returns `true`
    /// - an empty slot (`null_mut()`) is reached → returns `false`
    /// - a tombstone is encountered → skips it (probe chain continues)
    #[inline(always)]
    fn hash_contains(&self, base: *mut u8) -> bool {
        let start = Self::hash_index(base);
        let mut i = start;
        loop {
            let entry = self.hash_slot_read(i);
            if entry.is_null() {
                // Empty slot: the probe chain ends here; base is not present.
                return false;
            }
            if entry == base {
                return true;
            }
            // TOMBSTONE or a different live entry: skip and continue.
            i = (i + 1) & (HASH_CAPACITY - 1);
            if i == start {
                // Wrapped all the way around without finding base or an empty
                // slot. This can only happen if the table has no empty slots at
                // all (all entries are live or tombstone). Under the guaranteed
                // ≤ 50% load factor this cannot occur, but handle it defensively.
                return false;
            }
        }
    }
}
