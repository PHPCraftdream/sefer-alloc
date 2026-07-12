//! [`SegmentTable`] — the global registry of all live segments, SELF-HOSTED in
//! the primordial segment's payload (not a `Vec` / `Box` on the global
//! allocator).
//!
//! This is the keystone of the Phase 8 Membrane Inversion (§1 of
//! `ALLOC_PLAN.md`): the safe slot-table discipline stops *consuming* memory
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
/// the effective limit is unbounded: empty segments (including empty small
/// segments) are recycled (their slot is NULLed) and reused by future
/// `register` calls. Without `alloc-decommit`, only the EMPTY small-segment
/// recycle path is disabled; the free-list still recycles any released segment
/// slot (large allocations still recycle their slot on free via
/// `unregister`/`recycle`, unconditionally). Long-running processes with many
/// small-segment carve/decay cycles will pin small-segment slots and eventually
/// hit this cap.
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

/// Capacity of the free-list stack of recyclable slot indices (task #135,
/// Part 1). One `u32` per possible slot — the free-list can never hold more
/// entries than there are slots (`MAX_SEGMENTS`), so this bound is exact (no
/// separate overflow path is needed).
pub(crate) const FREE_LIST_CAPACITY: usize = MAX_SEGMENTS;

/// Footprint of the free-list stack (the index array only; the top-of-stack
/// counter is a separate `u32` field carved right after it — see
/// `Layout::primordial_free_list_off` / `primordial_free_top_off`).
pub(crate) const FREE_LIST_FOOTPRINT: usize = FREE_LIST_CAPACITY * size_of::<u32>();

/// PERF-P2 (eureka Э3) — number of slots in the direct-mapped own-segment
/// cache. A power of two (indexed by masking) kept deliberately tiny (start
/// small, measure before growing). The cache holds ONLY bases proven present
/// by a won `hash_contains` probe (it *remembers proven*, never *asserts*),
/// and every table-mutation path that can remove a base
/// (`unregister`/`recycle`) clears the matching slot IN THE SAME FUNCTION that
/// mutates the hash — so complete invalidation is structural, not a
/// remember-to-invalidate discipline scattered across call sites.
pub(crate) const OWN_CACHE_SIZE: usize = 4;

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
    /// PERF-P2 (Э3) — a tiny fixed-size direct-mapped cache of segment bases
    /// that have been PROVEN present by a won `hash_contains` probe. It is an
    /// inline struct field (NOT primordial-resident memory), zero-initialised
    /// (all `null_mut()`) in `from_primordial`.
    ///
    /// ## Invariant (the correctness keystone — a stale hit is UB / M2 breach)
    ///
    /// `own_cache[i]` is either `null_mut()` (empty) or a base that is
    /// CURRENTLY registered and live in the hash table. A cache HIT
    /// (`own_cache[cache_index(base)] == base`, non-null) therefore carries the
    /// exact same guarantee as `hash_contains(base) == true`: the segment is
    /// registered, live, and mapped by us. This invariant is preserved
    /// STRUCTURALLY: the ONLY places that fill the cache are won probes, and
    /// the ONLY places that can remove a base from the hash (`unregister`,
    /// `recycle`) clear the matching cache slot in the SAME function,
    /// immediately after `hash_remove`. You cannot drop a base from the table
    /// without passing through code that also evicts it from the cache.
    own_cache: [*mut u8; OWN_CACHE_SIZE],
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
    /// Task #135 (Part 1): a stack of recycled (NULL) slot indices, carved in
    /// the primordial segment immediately after the hash table. `FREE_LIST_CAPACITY`
    /// (= `MAX_SEGMENTS`) `u32` entries; only `[0, free_top)` are meaningful.
    /// `unregister`/`recycle` push the just-vacated index here (O(1));
    /// `register` pops from here first (O(1)) before falling back to append.
    free_list: *mut u32,
    /// Pointer to the free-list's top-of-stack counter (a single `u32` carved
    /// right after `free_list`'s `FREE_LIST_CAPACITY` entries). The number of
    /// valid (push-order) entries currently on the free-list stack.
    free_top: *mut u32,
    /// W2 (tombstone-rebuild) — number of TOMBSTONE entries currently present
    /// in `hash_slots`. A PLAIN struct field (NOT primordial-resident memory):
    /// the carved footprint (`slots`/`hash_slots`/`free_list`/`free_top`) is
    /// unchanged; this counter is an inline field of the `SegmentTable` value
    /// held by `AllocCore`, zero-initialised in `from_primordial`.
    ///
    /// ## Why it exists (the perf-metastability it kills)
    ///
    /// Tombstones are written by `hash_remove` and, pre-W2, NEVER converted
    /// back to empty (no backward-shift deletion, no rebuild). Every
    /// register/unregister cycle with a FRESH base (large-cache eviction,
    /// decommit-recycle, ASLR) consumed one empty slot forever, so `#empty`
    /// was monotonically non-increasing. Once `#empty` hit 0 (live ≤
    /// `MAX_SEGMENTS`, tombstones ≥ `MAX_SEGMENTS`), a `hash_contains` of an
    /// ABSENT base — the hot case, since every cross-thread free begins with a
    /// `contains_base` MISS on the caller's own table — probed the ENTIRE
    /// `HASH_CAPACITY` array before returning `false`. A long-running server
    /// degraded to ~`HASH_CAPACITY` metadata loads per cross-thread free. Not
    /// UB — a metastable perf collapse in exactly the DBMS/async profile the
    /// crate targets.
    ///
    /// The counter is maintained EXACTLY (incremented by `hash_remove`,
    /// decremented when `hash_insert` reuses a tombstone slot, reset to 0 by
    /// `rebuild_hash`) so the rebuild trigger can fire deterministically.
    tombstones: u32,
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
        free_list: *mut u32,
        free_top: *mut u32,
    ) -> Self {
        Self {
            slots,
            // PERF-P2: the direct-mapped own-segment cache starts EMPTY (all
            // slots null). It only ever fills from a won `hash_contains` probe.
            own_cache: [core::ptr::null_mut(); OWN_CACHE_SIZE],
            count,
            hash_slots,
            free_list,
            free_top,
            // W2: no tombstones exist in a freshly-carved hash table (the
            // bootstrap zeroed every entry to `null_mut()` = empty).
            tombstones: 0,
        }
    }

    /// Register a new segment base. Returns its assigned `segment_id` (the
    /// index it was placed at), or `None` if the table is full (all slots are
    /// live and count == MAX_SEGMENTS — only possible without `alloc-decommit`
    /// or under an extreme large-allocation storm).
    ///
    /// **O(1) slot-recycle (task #135, Part 1 — supersedes the task #60 linear
    /// scan):** pops a recyclable slot index off the free-list stack (O(1)) if
    /// one is available; the new base is written there and that index is
    /// returned (NO increment of `count` — the slot is already within the live
    /// window). Only when the free-list is empty does `register` append past
    /// the current high-water mark. This lifts the 1024-segment cap under
    /// `alloc-decommit`: as long as some slots are recycled, `register` never
    /// returns `None`.
    ///
    /// no-panic (Phase 11 GlobalAlloc face): returns `None` so the caller
    /// returns null (graceful OOM) rather than aborting.
    pub(crate) fn register(&mut self, base: *mut u8) -> Option<u32> {
        // O(1): pop a recycled slot index, if the free-list has one.
        if let Some(i) = self.free_list_pop() {
            let slot = Self::slot_ptr(self.slots, i as usize);
            // Defensive: the free-list invariant guarantees this slot is
            // currently NULL (see `free_list_push`'s contract) — reuse it.
            super::node::Node::write_struct::<*mut u8>(slot, base);
            // OPT-B: also insert into the hash table so `contains_base` is O(1).
            self.hash_insert(base);
            return Some(i);
        }
        // No recyclable slot — append.
        let idx = self.count as usize;
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
    ///
    /// Un-gated from `alloc-decommit` (0.3.0, task A1): the cross-thread
    /// large-segment reclaim path (`AllocCore::reclaim_large_segment`) needs
    /// to free a table slot for reuse regardless of whether `alloc-decommit`
    /// is enabled — without it, a segment freed by a remote thread could
    /// never be unregistered, permanently pinning a `SegmentTable` slot (and,
    /// pre-fix, the whole segment). The function body is pure safe pointer
    /// arithmetic through the `node`/hash seams — nothing decommit-specific —
    /// so lifting the gate is purely additive.
    ///
    /// **O(1) (task #135, Part 1 — supersedes the linear base-scan):** reads
    /// `segment_id` directly out of the segment's own header (via the
    /// field-specific `segment_id_at` accessor — a single `u32` load, disjoint
    /// from the owner-mutated `bump` field, so this is race-free under the
    /// same §11/§33 discipline as `magic_at`/`kind_at`) instead of scanning the
    /// table for a matching base pointer. The header at `base` is still valid
    /// here (this is the pre-decommit/pre-release call site — see the
    /// contract above), so the read is safe.
    ///
    /// Defensive: if the slot at `segment_id` does not actually hold `base`
    /// (a caller bug, or a stale/corrupt `segment_id`), this is a no-op — the
    /// same defensive posture the old linear scan had for "base not found".
    #[cfg_attr(
        not(any(feature = "alloc-decommit", feature = "alloc-xthread")),
        allow(dead_code)
    )]
    pub(crate) fn unregister(&mut self, base: *mut u8) {
        let id = super::segment_header::SegmentHeader::segment_id_at(base);
        if id as usize >= self.count as usize {
            // Defensive: out-of-range id (corrupt header / caller bug). No-op.
            return;
        }
        let slot = Self::slot_ptr(self.slots, id as usize);
        let current = super::node::Node::read_struct::<*mut u8>(slot);
        if current != base {
            // Defensive: the slot at `id` does not hold `base` — no-op rather
            // than corrupt the table.
            return;
        }
        // NULL the slot — the OS reservation is NOT released here.
        super::node::Node::write_struct::<*mut u8>(slot, core::ptr::null_mut());
        // OPT-B: remove from hash table (tombstone the entry).
        self.hash_remove(base);
        // W2: `hash_remove` just bumped `tombstones`. Amortise the rebuild onto
        // this deletion if the table has crossed the threshold (see
        // `maybe_rebuild_hash`). Rebuild lives on the DELETION path, not the
        // read path, so `contains_base`/`hash_contains` stay branch-free of
        // rebuild logic.
        self.maybe_rebuild_hash();
        // PERF-P2 (Э3): `base` is leaving the table — it MUST NOT remain
        // cached. A stale cache slot surviving removal would let a future
        // `contains_base` HIT on an unregistered/recycled/unmapped base and
        // route a foreign or freed pointer as own-thread (UB / M2 breach).
        // Co-located with `hash_remove` in the SAME function so invalidation
        // is structurally complete.
        self.own_cache_clear(base);
        // Task #135: push the just-vacated index onto the free-list so a
        // future `register` can reuse it in O(1). Guarded by `current != base`
        // above (only a slot that WAS non-NULL and held `base` reaches here),
        // so this can never push the same index twice for a single logical
        // unregister/recycle (the free-list-duplicate invariant).
        self.free_list_push(id);
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
        // Task #135 (Part 1): `segment_id` was already read as part of the
        // full-struct header read above (the header is still fully valid at
        // this point — only the PAYLOAD is decommitted by the caller before
        // `recycle` runs, never the metadata page hosting the header), so no
        // extra read is needed. O(1) slot lookup replaces the old O(count)
        // linear scan.
        let id = hdr.segment_id;
        if (id as usize) < self.count as usize {
            let slot = Self::slot_ptr(self.slots, id as usize);
            let current = super::node::Node::read_struct::<*mut u8>(slot);
            if current == base {
                // OPT-B: tombstone the hash entry BEFORE releasing the OS
                // reservation. After `release_segment` the pointer value `base`
                // remains valid as a key (we compare values, not dereference),
                // but doing the hash update first is cleaner.
                self.hash_remove(base);
                // PERF-P2 (Э3): evict `base` from the direct-mapped cache
                // BEFORE releasing the OS reservation. After `release_segment`
                // the virtual address `base` is unmapped; a stale cache slot
                // still holding it would let a later free of a pointer whose
                // computed base equals this recycled base HIT the cache and be
                // (catastrophically) routed as own-thread → write to unmapped /
                // recycled memory (UB / M2 breach). Co-located with
                // `hash_remove` in the SAME function → structural invalidation.
                self.own_cache_clear(base);
                // Release the OS reservation. After this, `base` is invalid
                // (unmapped). We do NOT dereference `base` after this point.
                super::os::release_segment(reservation, reservation_len);
                // NULL the slot so `register` can reuse it and `drop` skips it.
                super::node::Node::write_struct::<*mut u8>(slot, core::ptr::null_mut());
                // Push the vacated index onto the free-list (O(1) reuse by a
                // future `register`). Guarded by `current == base` above, so
                // this index is pushed at most once per logical recycle.
                self.free_list_push(id);
                // W2: amortise a rebuild onto this deletion if the tombstone
                // count crossed the threshold. Done AFTER the slot is NULLed so
                // `rebuild_hash`'s live-base scan (over `slots[0..count]`) does
                // NOT re-insert the just-recycled `base` (its slot is now
                // NULL and skipped) — the rebuild must observe the table in its
                // post-removal state.
                self.maybe_rebuild_hash();
                return;
            }
        }
        // Defensive: `base` was not found at its stamped `segment_id` slot
        // (corrupt header / double-recycle / never-registered). This
        // indicates a bug in the caller — or a corrupted `segment_id` (the
        // same threat model `unregister`'s sibling guard defends against).
        //
        // L-3 (UBFIX-11): the ORIGINAL defensive tail released the OS
        // reservation here WITHOUT first evicting `base` from the hash table
        // / own-cache, unlike the main (non-defensive) path just above. If
        // `base` happens to still be a genuinely LIVE entry in the hash table
        // (reachable via `hash_index(base)`, which is keyed by the pointer
        // VALUE, not by the corrupt `segment_id`) or the direct-mapped
        // own-cache, that stale entry would survive this release: a later
        // `contains_base(base)` on the now-UNMAPPED address would return
        // `true` (cache hit or hash hit), routing a subsequent free as
        // own-thread and reading/writing unmapped memory.
        //
        // `hash_remove`/`own_cache_clear` key on `base`'s VALUE (via
        // `hash_index`/`cache_index`), never on `id` — so calling them here
        // is safe and correct regardless of what is wrong with the stamped
        // `segment_id`: if `base` is genuinely present in the hash/cache
        // (under its natural probe position, independent of any slot index),
        // it is evicted; if it is not present (e.g. truly never registered),
        // both are already documented no-ops (`hash_remove`'s empty-slot
        // return; `own_cache_clear`'s slot-mismatch skip). This mirrors the
        // main path's exact call order (hash/cache eviction BEFORE the OS
        // release), just without the (untrustworthy, in this branch) slot
        // NULL + free-list push — the `slots[]` array itself is intentionally
        // left untouched here, since we do not know which (if any) index
        // legitimately maps to `base` under the corruption.
        self.hash_remove(base);
        self.own_cache_clear(base);
        // Release the OS reservation anyway to avoid a leak, but don't
        // corrupt the `slots[]` array — we do not know which slot (if any)
        // legitimately corresponds to `base` under this corruption, so
        // NULLing an unrelated slot / pushing a bogus free-list index would
        // be worse than a defensive no-op there.
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
    ///
    /// PERF-P2 (Э3): checks the tiny direct-mapped own-segment cache FIRST. A
    /// cache HIT (`own_cache[cache_index(base)] == base`, non-null) returns
    /// `true` immediately — the cache holds ONLY bases proven present (filled
    /// from a won probe) and is evicted in lockstep with every hash removal
    /// (`unregister`/`recycle`), so a hit carries the exact `hash_contains ==
    /// true` guarantee (registered + live + mapped). A MISS falls through to
    /// the full `hash_contains`; on a probe hit we FILL the cache slot
    /// (remember-proven) and return `true`; on a probe miss we return `false`
    /// WITHOUT filling (the cache never holds an absent base). Requires `&mut
    /// self` for the fill; all hot free-path callers (`dealloc`,
    /// `dealloc_routing`, `realloc`) already hold `&mut`.
    #[inline(always)]
    pub(crate) fn contains_base(&mut self, base: *mut u8) -> bool {
        let idx = Self::cache_index(base);
        // Fast path: proven-present cache hit.
        if self.own_cache[idx] == base && !base.is_null() {
            return true;
        }
        // Miss → full O(1) hash probe. Fill the cache only on a won probe.
        if self.hash_contains(base) {
            self.own_cache[idx] = base;
            true
        } else {
            false
        }
    }

    /// PERF-P2 (Э3): read-only membership test that NEVER touches the cache
    /// (no fill), for `&self` contexts (test-only `dbg_*` accessors, census).
    /// Same result as `contains_base` — it just skips the remember-proven
    /// fill. Kept separate so the read-only surface does not require `&mut`.
    #[inline(always)]
    pub(crate) fn contains_base_ro(&self, base: *mut u8) -> bool {
        let idx = Self::cache_index(base);
        if self.own_cache[idx] == base && !base.is_null() {
            return true;
        }
        self.hash_contains(base)
    }

    /// PERF-P2 (Э3): direct-mapped cache index for `base`. Segments are
    /// SEGMENT-aligned so the low `SEGMENT_SHIFT` bits are zero; shift them out
    /// first to get a dense per-segment key, then mask to `OWN_CACHE_SIZE`.
    #[inline(always)]
    fn cache_index(base: *mut u8) -> usize {
        (base as usize >> SEGMENT_SHIFT) & (OWN_CACHE_SIZE - 1)
    }

    /// PERF-P2 (Э3): evict `base` from the direct-mapped cache if (and only if)
    /// the slot for `base`'s index currently holds exactly `base`. Called from
    /// `unregister`/`recycle` in lockstep with `hash_remove`. A slot holding a
    /// DIFFERENT base (a collision) is left untouched — that other base is
    /// still live, and evicting it would only cost a future miss, never
    /// correctness; but we specifically do NOT clear it because doing so is
    /// unnecessary.
    ///
    /// ## Register-reuse reasoning (why `register` does NOT touch the cache)
    ///
    /// The cache invariant is "a non-null cache slot holds a base currently
    /// present in the hash". A newly-registered base `b` at index `i` finds
    /// `own_cache[i]` either EMPTY (`null` — fine, no stale entry) or holding
    /// some OTHER base `b' != b`. In the latter case `b'` can only be a base
    /// that is ITSELF still live in the hash (every eviction path clears the
    /// slot when its base leaves, so a surviving non-null slot is a live base):
    /// `b'` is not `b`, so a lookup of `b` MISSES the cache and falls to the
    /// hash (correct), and a lookup of `b'` still HITS correctly (b' is
    /// genuinely live). Registering `b` neither creates nor removes a hash
    /// entry for `b'`, so the invariant holds for both.
    ///
    /// It is IMPOSSIBLE for `own_cache[i]` to hold `b` itself at register time:
    /// the cache only fills from a won probe, and `b` could only have won a
    /// probe while previously registered; but between that registration and
    /// this one `b` MUST have been removed via `unregister`/`recycle`, which
    /// clears the slot for `b`. Hence no stale `b` can survive to this
    /// `register`. Therefore `register` needs no cache write — the ONLY hazard
    /// (a stale slot surviving removal) is fully handled by the eviction in
    /// `unregister`/`recycle`. Verified: see the counterfactual regression test
    /// `regression_own_segment_cache_invalidation`.
    #[cfg_attr(
        not(any(feature = "alloc-decommit", feature = "alloc-xthread")),
        allow(dead_code)
    )]
    #[inline]
    fn own_cache_clear(&mut self, base: *mut u8) {
        let idx = Self::cache_index(base);
        if self.own_cache[idx] == base {
            self.own_cache[idx] = core::ptr::null_mut();
        }
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

    /// Read the slot at index `i` directly, without going through the
    /// `bases()` iterator. Returns `null_mut()` for a recycled (NULL) slot or
    /// an out-of-range index — the caller distinguishes "recycled" from
    /// "out of range" via `count()` if needed.
    ///
    /// **Why this exists (task #126):** `bases()`'s returned `impl Iterator`
    /// captures the elided lifetime of `&self` (return-position `impl Trait`
    /// lifetime-capture rule), even though the closure itself only closes over
    /// `Copy` data (`slots`, `n`) and performs no actual borrow of `self`'s
    /// fields after construction. That capture is enough to make the
    /// borrow-checker treat a live `bases()` iterator as holding `&self.table`,
    /// which conflicts with an interleaved `&mut self.table.recycle(...)` call
    /// (needed by `find_segment_with_free` to recycle segments that empty out
    /// mid-scan). `base_at` sidesteps this: each call is a self-contained
    /// pointer read with no returned borrow, so the caller can freely
    /// interleave `base_at(i)` reads with `recycle(...)` calls in the same
    /// index-driven loop — no pre-collect buffer needed, and no bound on how
    /// many segments can be recycled in one scan.
    #[inline(always)]
    pub(crate) fn base_at(&self, i: usize) -> *mut u8 {
        if i >= self.count as usize {
            return core::ptr::null_mut();
        }
        let slot = Self::slot_ptr(self.slots, i) as *const *mut u8;
        super::node::Node::read_struct::<*mut u8>(slot)
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
    // Task #135 (Part 1) — O(1) free-list of recycled slot indices.
    //
    // A plain stack (LIFO) of `u32` slot indices, carved in the primordial
    // segment right after the hash table. Invariant: an index is on the
    // free-list IF AND ONLY IF the corresponding `slots[index]` is currently
    // NULL. `unregister`/`recycle` push (after NULLing the slot); `register`
    // pops (before writing the new base into the slot). Both call sites are
    // guarded so an index is pushed at most once per NULL transition (see
    // their `current == base` / `current != base` checks), so the free-list
    // never holds a duplicate entry and never holds an index whose slot is
    // actually live.
    // -------------------------------------------------------------------

    /// Address of free-list entry `i`. Pure pointer arithmetic through the
    /// `node` seam.
    #[inline(always)]
    fn free_list_slot_ptr(&self, i: usize) -> *mut u32 {
        super::node::Node::offset(self.free_list as *mut u8, i * core::mem::size_of::<u32>())
            as *mut u32
    }

    /// Push `idx` onto the free-list stack. Caller's invariant: `idx`'s slot
    /// was just NULLed (transitioned live → recyclable) and is not already on
    /// the free-list — see the guarded call sites in `unregister`/`recycle`.
    #[inline]
    fn free_list_push(&mut self, idx: u32) {
        let top = super::node::Node::read_u32(self.free_top as *const u32);
        debug_assert!(
            (top as usize) < FREE_LIST_CAPACITY,
            "free-list overflow — more recycled slots than MAX_SEGMENTS"
        );
        let slot = self.free_list_slot_ptr(top as usize);
        super::node::Node::write_u32(slot, idx);
        super::node::Node::write_u32(self.free_top, top + 1);
    }

    /// Pop the most-recently-recycled index off the free-list, or `None` if
    /// it is empty. O(1).
    #[inline]
    fn free_list_pop(&mut self) -> Option<u32> {
        let top = super::node::Node::read_u32(self.free_top as *const u32);
        if top == 0 {
            return None;
        }
        let new_top = top - 1;
        let slot = self.free_list_slot_ptr(new_top as usize);
        let idx = super::node::Node::read_u32(slot);
        super::node::Node::write_u32(self.free_top, new_top);
        Some(idx)
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
                if entry == TOMBSTONE {
                    // W2: reusing a tombstone converts it back to live — the
                    // ONLY tombstone→live transition. Keep `tombstones` exact
                    // (it is otherwise only grown by `hash_remove` and reset by
                    // `rebuild_hash`); an off-by-one here would let the rebuild
                    // trigger fire early or late.
                    self.tombstones -= 1;
                }
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
                // W2: the ONLY live→tombstone transition — bump the exact
                // tombstone count so `unregister`/`recycle` can decide whether
                // the table has accumulated enough tombstones to warrant a
                // rebuild.
                self.tombstones += 1;
                return;
            }
            // TOMBSTONE or different live entry: skip and continue probing.
            i = (i + 1) & (HASH_CAPACITY - 1);
            debug_assert!(i != start, "hash_remove looped without finding base");
        }
    }

    /// W2 — rebuild trigger. Called on the deletion paths (`unregister`,
    /// `recycle`) right after a tombstone was created (and the vacated slot
    /// NULLed). Rebuilds only when tombstones exceed `HASH_CAPACITY / 4`.
    ///
    /// ## Threshold justification (why a quarter)
    ///
    /// A rebuild is O(`HASH_CAPACITY`) (clear the array + re-insert every live
    /// base). If we trigger every time `tombstones > HASH_CAPACITY / 4`, then
    /// between two rebuilds at least `HASH_CAPACITY / 4` *fresh* deletions must
    /// have occurred (each rebuild resets the count to 0, and only a
    /// live→tombstone removal grows it). So the O(`HASH_CAPACITY`) rebuild cost
    /// is amortised over ≥ `HASH_CAPACITY / 4` deletions → **O(1) amortised**
    /// per delete. A quarter (rather than a half) also keeps the *steady-state*
    /// tombstone population ≤ `HASH_CAPACITY / 4`, so `#empty` stays ≥
    /// `HASH_CAPACITY - MAX_SEGMENTS - HASH_CAPACITY/4` = well above zero (with
    /// `HASH_CAPACITY = 2·MAX_SEGMENTS`, that is ≥ `HASH_CAPACITY/4` empty
    /// slots always), which bounds the worst-case `hash_contains` probe length
    /// and kills the metastable collapse this counter exists to prevent. A
    /// smaller fraction would rebuild too often (cost); a larger one lets the
    /// probe chains grow longer before reclaiming — a quarter is the balance.
    #[cfg_attr(
        not(any(feature = "alloc-decommit", feature = "alloc-xthread")),
        allow(dead_code)
    )]
    #[inline]
    fn maybe_rebuild_hash(&mut self) {
        if self.tombstones > (HASH_CAPACITY / 4) as u32 {
            self.rebuild_hash();
        }
    }

    /// W2 — rebuild the open-addressing hash from the authoritative dense slot
    /// registry, eliminating ALL tombstones. Clears `hash_slots` to empty,
    /// re-inserts every LIVE base (`slots[0..count]`, skipping NULL/recycled
    /// slots), resets `tombstones` to 0, and clears `own_cache`.
    ///
    /// This is TRANSPARENT: `contains_base`/`hash_contains` return the exact
    /// same true/false for every base before and after (membership is defined
    /// by the live slot set, which the rebuild reproduces exactly). The only
    /// thing that changes is the probe *positions* — which is precisely why
    /// `own_cache` MUST be reset: it caches `hash_contains`-proven bases, and a
    /// rebuild moves entries, so a stale cache slot could point a probe at the
    /// wrong position. Clearing it is the safe, correct move (it re-fills
    /// lazily from won probes), and matches the eviction discipline of
    /// `unregister`/`recycle`, which clear the cache in lockstep with hash
    /// mutation.
    #[cfg_attr(
        not(any(feature = "alloc-decommit", feature = "alloc-xthread")),
        allow(dead_code)
    )]
    fn rebuild_hash(&mut self) {
        // 1. Clear the whole hash table to empty (null).
        for i in 0..HASH_CAPACITY {
            self.hash_slot_write(i, core::ptr::null_mut());
        }
        // 2. Tombstones are gone — reset the exact count BEFORE re-inserting so
        //    the tombstone→live decrement in `hash_insert` (which cannot fire
        //    now, since every slot is empty) never sees a stale value.
        self.tombstones = 0;
        // 3. Re-insert every LIVE base from the authoritative dense registry.
        //    NULL (recycled) slots are skipped — they are not members. The
        //    just-vacated base (in `recycle`/`unregister`) is already NULL in
        //    `slots`, so it is correctly NOT re-inserted.
        let n = self.count as usize;
        for i in 0..n {
            let slot = Self::slot_ptr(self.slots, i) as *const *mut u8;
            let base = super::node::Node::read_struct::<*mut u8>(slot);
            if !base.is_null() {
                self.hash_insert(base);
            }
        }
        // 4. Reset the proven-present cache: a rebuild moved probe positions, so
        //    every cached (proven) entry may now be stale. Clearing is the
        //    safe, correct move — it re-fills lazily from future won probes.
        self.own_cache = [core::ptr::null_mut(); OWN_CACHE_SIZE];
    }

    /// W2 — TEST-ONLY observability seam: the current exact TOMBSTONE count in
    /// the hash table. Mirrors the `dbg_*` convention used elsewhere in the
    /// substrate (e.g. `count()` exposure via `AllocCore::dbg_table_count`).
    /// Lets the counterfactual regression test observe that the rebuild fires
    /// and keeps tombstones bounded. Zero production impact.
    #[doc(hidden)]
    #[cfg_attr(
        not(any(feature = "alloc-decommit", feature = "alloc-xthread")),
        allow(dead_code)
    )]
    pub(crate) fn dbg_hash_tombstones(&self) -> u32 {
        self.tombstones
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
