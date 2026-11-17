#[path = "harness.rs"]
#[cfg(feature = "internals")]
mod harness;
#[path = "hash.rs"]
mod hash;

use super::issue_transaction::{IssueRun, IssueTransaction};

#[cfg(feature = "alloc-global")]
use super::route_slots::RouteSlots;
use super::table_constants::*;
use super::*;
#[cfg(feature = "alloc-global")]
use crate::alloc_core::platform::fatal::fatal;
use crate::segment::segment_header::{SegmentHeader, SegmentKind};
#[cfg(feature = "internals")]
#[doc(hidden)]
pub use harness::SegmentHashHarness;

/// R6-T11 — sentinel key for an EMPTY own_cache slot. `usize::MAX` is not
/// SEGMENT-aligned, so it can never equal a real lookup key (real keys are
/// SEGMENT-aligned addresses); a hit is therefore a single key comparison,
/// with no separate `!is_null()` root check.
const OWN_CACHE_EMPTY_KEY: usize = usize::MAX;
const LAST_EMPTY_KEY: usize = usize::MAX;

const _: () = assert!(
    OWN_CACHE_SIZE.is_power_of_two(),
    "OWN_CACHE_SIZE must be a power of two -- SegmentTable::cache_index masks with OWN_CACHE_SIZE - 1"
);

/// A self-hosted segment registry: a fixed-capacity array of segment-base
/// pointers plus a high-water count, carved from the primordial segment.
///
/// The registry does NOT own the segments it lists — ownership lives with
/// [`crate::alloc_core::AllocCore`] (which holds the owning [`crate::alloc_core::os::Segment`]
/// handles). The registry is the *index* over them, used by drop/census; the
/// hot path resolves owners via `segment_of(ptr)` (no registry lookup).
///
/// Under `alloc-decommit`, recycled slots hold `null_mut()` — the OS
/// reservation for those segments has been released. [`bases`](Self::bases)
/// filters them out; [`register`](Self::register) reuses them before appending.
/// Fixed-capacity registry; its primordial backing storage is validated at construction.
pub struct SegmentTable {
    #[cfg(feature = "alloc-global")]
    pub(super) routes: Option<RouteSlots>,
    /// Pointer to the first slot of the registry array (lives in the
    /// primordial segment's payload). `MAX_SEGMENTS` entries.
    slots: *mut *mut u8,
    /// Plain owner-only index in the primordial metadata window.
    pub(super) active_kind: *mut ActiveKindIndex,
    /// PERF-P2 (Э3) — a tiny fixed-size direct-mapped cache of segment bases
    /// returned by a won `hash_find` probe. It is an
    /// inline struct field (NOT primordial-resident memory), initialised to
    /// all-EMPTY entries in `from_primordial`.
    ///
    /// ## Invariant (the correctness keystone — a stale hit is UB / M2 breach)
    ///
    /// Each `own_cache[i]` entry is a `(key, root)` pair: an EMPTY slot is
    /// `(OWN_CACHE_EMPTY_KEY, null_mut())`; a filled slot is a biased Large
    /// payload key (which need not equal its reservation root) paired with
    /// an allocator-origin pointer CURRENTLY registered and live in the hash
    /// table. A matching entry carries the same live-membership guarantee as
    /// `hash_find(key)`.
    /// This invariant is preserved
    /// STRUCTURALLY: the ONLY places that fill the cache are won probes, and
    /// the ONLY places that can remove a base from the hash (`unregister`,
    /// `recycle`) clear the matching cache slot in the SAME function,
    /// immediately after `hash_remove`. You cannot drop a base from the table
    /// without passing through code that also evicts it from the cache.
    pub(super) own_cache: [(usize, *mut u8); OWN_CACHE_SIZE],
    /// N9 — a ONE-entry "last resolved segment" memo consulted BEFORE
    /// `own_cache`/hash lookup. Holds `(key.addr(), root)` for the most
    /// recent exact-base resolution (root.addr() == key.addr() only —
    /// biased Large payload keys are NEVER cached here, unlike `own_cache`).
    /// Hit cost is a single address compare; miss cost is unchanged. N9-v3:
    /// filled ONLY from an exact-base `own_cache` HIT, never from a won
    /// `hash_find` probe (on miss-heavy work the memo would not hit between
    /// misses, so the probe-side fill was dead stores); invalidated in
    /// `own_cache_clear`, i.e. in lockstep with `own_cache` on every
    /// `unregister`/`recycle`, so a stale hit is structurally impossible.
    /// EMPTY is encoded as `last_key == LAST_EMPTY_KEY` (usize::MAX): real
    /// segment bases are SEGMENT-aligned and a null lookup key is 0, so an
    /// EMPTY entry can never be hit — with a SINGLE compare, no extra branch.
    pub(super) last_key: usize,
    pub(super) last_root: *mut u8,

    /// High-water mark: the number of slots that have EVER been written
    /// (including currently-NULL recyclable slots). Segments 0 (the
    /// primordial) is always at index 0 and is never recycled.
    count: u32,
    /// OPT-B: open-addressing hash table for O(1) `contains_base`. Lives in
    /// the primordial segment immediately after the slots array. Capacity is
    /// `HASH_CAPACITY` entries. Two-state encoding (backward-shift deletion
    /// in `hash_remove` — R4-8/N3 — never leaves tombstones):
    /// - `null_mut()` → empty (terminates a probe chain)
    /// - other        → SEGMENT-aligned numeric key plus slot id+1 in low bits
    pub(super) hash_slots: *mut *mut u8,
    /// Task #135 (Part 1): a stack of recycled (NULL) slot indices, carved in
    /// the primordial segment immediately after the hash table. `FREE_LIST_CAPACITY`
    /// (= `MAX_SEGMENTS`) `u32` entries; only `[0, free_top)` are meaningful.
    /// `unregister`/`recycle` push the just-vacated index here (O(1));
    /// `register` pops from here first (O(1)) before falling back to append.
    pub(super) free_list: *mut u32,
    /// Pointer to the free-list's top-of-stack counter (a single `u32` carved
    /// right after `free_list`'s `FREE_LIST_CAPACITY` entries). The number of
    /// valid (push-order) entries currently on the free-list stack.
    pub(super) free_top: *mut u32,
}

impl SegmentTable {
    /// Construct the registry view over an already-laid-down array in the
    /// primordial segment. Used by the bootstrap after it has carved the slot
    /// array, hash table, and active-kind index. This only wraps pointers.
    ///
    /// # Caller's contract
    ///
    /// `slots` must point to `REGISTRY_FOOTPRINT` bytes inside the primordial
    /// segment, with slot 0 already set to the primordial base. `hash_slots`
    /// must point to `HASH_FOOTPRINT` bytes (all zeroed / `null_mut()`) for the
    /// open-addressing hash table. `active_kind` must point to initialized
    /// 1040-byte storage with Small bit 0 set. `count` is the current live
    /// count (1 for just the primordial). This method does not touch
    /// memory — it only stores the pointers; bootstrap and the test harness
    /// establish the contract before calling.
    /// # Safety
    ///
    /// Every pointer must reference correctly aligned, writable storage with
    /// capacity required by the table constants. Slot zero and the hash entry
    /// must describe the live primordial segment; the free-list top and active
    /// index must be initialized. The table backing storage must remain mapped
    /// and owner-exclusive for the table's lifetime; `count` is in
    /// `1..=MAX_SEGMENTS`.
    #[allow(unsafe_code)]
    pub unsafe fn from_primordial(
        slots: *mut *mut u8,
        count: u32,
        hash_slots: *mut *mut u8,
        free_list: *mut u32,
        free_top: *mut u32,
        active_kind: *mut ActiveKindIndex,
    ) -> Self {
        Self {
            #[cfg(feature = "alloc-global")]
            routes: None,
            slots,
            active_kind,
            // PERF-P2: the direct-mapped own-segment cache starts EMPTY (all
            // keys `OWN_CACHE_EMPTY_KEY`). It only ever fills from a won
            // `hash_contains` probe.
            own_cache: [(OWN_CACHE_EMPTY_KEY, core::ptr::null_mut()); OWN_CACHE_SIZE],
            // N9: the last-segment memo starts EMPTY (never matches — see
            // LAST_EMPTY_KEY above).
            last_key: LAST_EMPTY_KEY,
            last_root: core::ptr::null_mut(),
            count,
            hash_slots,
            free_list,
            free_top,
        }
    }

    /// # Safety
    ///
    /// The table and primordial storage must be valid, mapped, and exclusively
    /// owner-accessed. `primordial` must be slot zero; routing must not already
    /// have been attached.
    #[allow(unsafe_code)]
    #[cfg(feature = "alloc-global")]
    pub unsafe fn attach_owner(&mut self, owner: u32, primordial: *mut u8) -> Option<()> {
        if self.routes.is_some() || self.count != 1 || self.base_at(0) != primordial {
            return None;
        }
        self.routes = Some(RouteSlots::new(
            owner,
            primordial,
            crate::alloc_core::os::SEGMENT,
        )?);
        Some(())
    }

    /// Whether this instance attached terminal descriptors before first issue.
    /// Standalone owner-local cores never attach them. Once attached, missing
    /// per-segment descriptors remain a broken invariant, not a local-mode hint.
    #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
    #[inline]
    pub fn is_routed(&self) -> bool {
        self.routes.is_some()
    }

    pub fn close_routes(&mut self) {
        #[cfg(feature = "alloc-global")]
        if let Some(routes) = &mut self.routes {
            routes.close_all();
        }
    }

    /// # Safety
    ///
    /// `base` must be mapped, live, and readable as a segment header; the
    /// table must be initialized and exclusively owner-accessed.
    #[allow(unsafe_code)]
    #[cfg_attr(not(debug_assertions), inline(always))]
    pub unsafe fn issue_small(&self, base: *mut u8, offset: u32, class_idx: usize) {
        #[cfg(feature = "alloc-global")]
        if let Some(routes) = &self.routes {
            let class = u8::try_from(class_idx)
                .unwrap_or_else(|_| fatal(b"segment_table: small class index exceeds u8\n"));
            let index = SegmentHeader::segment_id_at(base) as usize;
            if self.base_at(index) != base {
                fatal(b"segment_table: issue base is not the segment base\n");
            }
            routes.issue_small(index, base, offset, class);
        }
        #[cfg(not(feature = "alloc-global"))]
        let _ = (base, offset, class_idx);
    }

    /// # Safety
    ///
    /// `base` must be mapped, live, and readable as a segment header; the
    /// table must be initialized and exclusively owner-accessed.
    #[allow(unsafe_code)]
    #[cfg_attr(not(debug_assertions), inline(always))]
    pub unsafe fn prepare_small_issue(
        &self,
        base: *mut u8,
        offset: u32,
        class_idx: usize,
    ) -> Option<IssueTransaction> {
        #[cfg(feature = "alloc-global")]
        if let Some(routes) = &self.routes {
            let class = u8::try_from(class_idx)
                .unwrap_or_else(|_| fatal(b"segment_table: small class index exceeds u8\n"));
            let index = SegmentHeader::segment_id_at(base) as usize;
            if self.base_at(index) != base {
                fatal(b"segment_table: prepare base is not the segment base\n");
            }
            return routes.prepare_small_issue(index, base, offset, class);
        }
        let _ = (base, offset, class_idx);
        Some(IssueTransaction::unrouted(base, offset))
    }

    /// # Safety
    ///
    /// `base` must be mapped and live and route backing must remain valid;
    /// the table must be initialized and exclusively owner-accessed.
    #[allow(unsafe_code)]
    #[cfg_attr(not(debug_assertions), inline(always))]
    pub unsafe fn prepare_small_issue_run(&self, base: *mut u8, class_idx: usize) -> IssueRun<'_> {
        #[cfg(feature = "alloc-global")]
        if let Some(routes) = &self.routes {
            let class = u8::try_from(class_idx)
                .unwrap_or_else(|_| fatal(b"segment_table: small class index exceeds u8\n"));
            let index = SegmentHeader::segment_id_at(base) as usize;
            if self.base_at(index) != base {
                fatal(b"segment_table: prepare base is not the segment base\n");
            }
            let inner = routes.prepare_small_issue_run(base, class);
            return IssueRun::routed(index, base, inner);
        }
        let _ = (base, class_idx);
        IssueRun::unrouted(base)
    }

    /// # Safety
    ///
    /// The table and routed metadata must remain valid and owner-exclusive;
    /// `index` must be in range and identify the supplied live segment base.
    #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
    #[allow(unsafe_code)]
    #[inline]
    pub unsafe fn scan_small_route_from(
        &self,
        index: usize,
        base: *mut u8,
        high_water: usize,
        word: usize,
    ) -> Option<crate::alloc_core::segment_route::RouteScan<'_>> {
        self.routes
            .as_ref()?
            .scan_small_from(index, base, high_water, word)
    }

    /// # Safety
    ///
    /// The table and routed metadata must remain valid and owner-exclusive;
    /// `index` must be in range and identify a live routed Large segment.
    #[allow(unsafe_code)]
    #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
    pub unsafe fn claim_large_route(&self, index: usize, base: *mut u8) -> bool {
        self.routes
            .as_ref()
            .is_some_and(|routes| routes.claim_large_pending(index, base))
    }

    /// Register a new segment base. Returns its assigned `segment_id` (the
    /// index it was placed at), or `None` if the table is full or routed
    /// registration fails. In either failure case, no table slot is changed
    /// and the caller still owns the reservation for rollback.
    ///
    /// **O(1) slot-recycle (task #135, Part 1 — supersedes the task #60 linear
    /// scan):** pops a recyclable slot index off the free-list stack (O(1)) if
    /// one is available; the new base is written there and that index is
    /// returned (NO increment of `count` — the slot is already within the live
    /// window). Only when the free-list is empty does `register` append past
    /// the current high-water mark. This lifts the fixed `MAX_SEGMENTS` cap
    /// under `alloc-decommit`: as long as some slots are recycled, `register`
    /// never returns `None`.
    ///
    /// no-panic (Phase 11 GlobalAlloc face): returns `None` so the caller
    /// returns null (graceful OOM) rather than aborting.
    /// # Safety
    ///
    /// When `kind` is Small or Large, `base` must identify an exclusively owned,
    /// live mapped reservation of at least `len` bytes. For `register_payload`,
    /// `payload` must be inside that reservation. Storage stays mapped until
    /// removal and table mutation is serialized. Unsupported kinds are rejected
    /// before any pointer-derived access.
    #[allow(unsafe_code)]
    pub unsafe fn register(&mut self, base: *mut u8, len: usize, kind: SegmentKind) -> Option<u32> {
        // SAFETY: this internal operation is gated by the same validated live-reservation contract as register_payload.
        unsafe { self.register_payload(base, len, kind, base) }
    }

    /// # Safety
    ///
    /// `base` must identify a live, registered reservation with readable
    /// metadata and the table must remain owner-exclusive.
    #[allow(unsafe_code)]
    pub unsafe fn register_payload(
        &mut self,
        base: *mut u8,
        len: usize,
        kind: SegmentKind,
        payload: *mut u8,
    ) -> Option<u32> {
        if !matches!(kind, SegmentKind::Small | SegmentKind::Large) {
            return None;
        }
        let key = crate::alloc_core::os::segment_base_of_ptr(payload);
        let has_free = crate::alloc_core::node::Node::read_u32(self.free_top) != 0;
        if !has_free && self.count as usize >= MAX_SEGMENTS {
            return None;
        }
        #[cfg(not(feature = "alloc-global"))]
        let _ = len;
        #[cfg(feature = "alloc-global")]
        let target = self.next_register_slot(has_free);
        #[cfg(feature = "alloc-global")]
        let route = if let Some(routes) = &mut self.routes {
            let needed = self.count as usize + usize::from(!has_free);
            Some(routes.prepare(needed, base, len, kind, payload, target)?)
        } else {
            None
        };
        // O(1): pop a recycled slot index, if the free-list has one.
        if let Some(i) = self.free_list_pop() {
            let slot = Self::slot_ptr(self.slots, i as usize);
            // Defensive: the free-list invariant guarantees this slot is
            // currently NULL (see `free_list_push`'s contract) — reuse it.
            crate::alloc_core::node::Node::write_struct::<*mut u8>(slot, base);
            // OPT-B: also insert into the hash table so `contains_base` is O(1).
            self.hash_insert_identity(key, i);
            #[cfg(feature = "alloc-global")]
            if let Some(route) = route {
                self.routes
                    .as_mut()
                    .unwrap_or_else(|| fatal(b"segment_table: routed table missing routes\n"))
                    .put(i as usize, route);
            }
            ActiveKindIndex::set(self.active_kind, i as usize, kind);
            return Some(i);
        }
        // No recyclable slot — append.
        let idx = self.count as usize;
        if idx >= MAX_SEGMENTS {
            return None;
        }
        let slot = Self::slot_ptr(self.slots, idx);
        crate::alloc_core::node::Node::write_struct::<*mut u8>(slot, base);
        self.count += 1;
        // OPT-B: also insert into the hash table so `contains_base` is O(1).
        self.hash_insert_identity(key, idx as u32);
        #[cfg(feature = "alloc-global")]
        if let Some(route) = route {
            self.routes
                .as_mut()
                .unwrap_or_else(|| fatal(b"segment_table: routed table missing routes\n"))
                .put(idx, route);
        }
        ActiveKindIndex::set(self.active_kind, idx, kind);
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
    /// same §11/§33 field-atomic discipline as `kind_at`) instead of scanning the
    /// table for a matching base pointer. The header at `base` is still valid
    /// here (this is the pre-decommit/pre-release call site — see the
    /// contract above), so the read is safe.
    ///
    /// A standalone core defensively no-ops on a mismatched `segment_id`.
    /// A routed heap scans for the actual slot instead: returning without
    /// removing its route could let the caller unmap while it stays published.
    /// If no slot owns the base, it aborts before the caller can release it.
    #[cfg_attr(
        not(any(feature = "alloc-decommit", feature = "alloc-xthread")),
        allow(dead_code)
    )]
    /// # Safety
    ///
    /// `base` must be mapped, live, and readable as a segment header; the
    /// table must remain initialized and exclusively owner-accessed.
    #[allow(unsafe_code)]
    pub unsafe fn unregister(&mut self, base: *mut u8) {
        let id = crate::alloc_core::segment_header::SegmentHeader::segment_id_at(base);
        let slot_id = if self.base_at(id as usize) == base {
            id as usize
        } else {
            // Standalone cores retain the existing defensive no-op. A routed
            // heap must resolve the true slot before its caller can unmap.
            #[cfg(feature = "alloc-global")]
            {
                if self.routes.is_some() {
                    (0..self.count as usize)
                        .find(|&i| self.base_at(i) == base)
                        .unwrap_or_else(|| fatal(b"segment_table: unregister of unowned base\n"))
                } else {
                    return;
                }
            }
            #[cfg(not(feature = "alloc-global"))]
            return;
        };
        let slot = Self::slot_ptr(self.slots, slot_id);
        let header = SegmentHeader::read_at(base);
        let key = crate::alloc_core::os::segment_base_of_ptr(
            crate::alloc_core::node::Node::offset(base, header.payload_offset),
        );
        #[cfg(feature = "alloc-global")]
        if let Some(routes) = &mut self.routes {
            routes.remove(slot_id);
        }
        ActiveKindIndex::clear(self.active_kind, slot_id, header.kind);
        // NULL the slot — the OS reservation is NOT released here.
        crate::alloc_core::node::Node::write_struct::<*mut u8>(slot, core::ptr::null_mut());
        // OPT-B: remove from hash table via backward-shift deletion (R4-8/N3:
        // no tombstone, no rebuild — see `hash_remove`).
        self.hash_remove(key);
        // PERF-P2 (Э3): `base` is leaving the table — it MUST NOT remain
        // cached. A stale cache slot surviving removal would let a future
        // `contains_base` HIT on an unregistered/recycled/unmapped base and
        // route a foreign or freed pointer as own-thread (UB / M2 breach).
        // Co-located with `hash_remove` in the SAME function so invalidation
        // is structurally complete.
        self.own_cache_clear(base);
        // Task #135: push the just-vacated index onto the free-list so a
        // future `register` can reuse it in O(1). Only a slot that held
        // `base` reaches here,
        // so this can never push the same index twice for a single logical
        // unregister/recycle (the free-list-duplicate invariant).
        self.free_list_push(slot_id as u32);
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
    /// If `base` is a genuinely registered segment whose stamped
    /// `segment_id` no longer resolves to its own slot (a corrupted/stale
    /// `id` — a caller bug, or genuine memory corruption), the slot is
    /// located by a bounded linear scan over `slots[0..count)` and the main
    /// path still runs (`base` is unique, so at most one slot can hold it).
    /// If `base` is not a member of the table AT ALL (never registered, or
    /// already recycled — a double-recycle), NOTHING is released: see
    /// oxx R2-06 below for why.
    ///
    /// ## oxx R2-06 (independent src review round 2): membership is checked
    /// BEFORE the header is read, and NEVER through `slots[segment_id]` alone
    /// (previously: `unregister`'s sibling guard, this being its `recycle`
    /// analogue).
    ///
    /// The prior version of this function read `SegmentHeader::read_at(base)`
    /// UNCONDITIONALLY, before any membership check, then treated
    /// `slots[segment_id] != base` as the ONLY defended-against anomaly. Two
    /// problems: (a) a double-recycle target is already unmapped — reading
    /// its header there is itself a fault, not a graceful defensive path;
    /// (b) even when the header read happened to succeed (e.g. a corrupted
    /// `segment_id` on an otherwise-live, still-mapped segment), the old
    /// defensive tail released the OS reservation from the UNVERIFIED header
    /// bytes it just read and left `slots[]` untouched — leaving that
    /// segment's real slot dangling (still non-NULL, pointing at now-unmapped
    /// memory), which `AllocCore::drop`/scan paths would later walk into a
    /// use-after-unmap or a second `release_segment` of the same reservation.
    ///
    /// The fix reorders the checks: [`hash_find`](Self::hash_find) is a pure
    /// address-VALUE lookup — it never dereferences `base`'s memory — so it
    /// can safely answer "is `base` a live member of this table" even when
    /// `base` is unmapped or was never ours. Only once that membership is
    /// confirmed do we read the header (now known-safe: a hash member is, by
    /// the table's own invariant, a segment we mapped and have not yet
    /// released). The real slot is then found — O(1) via `segment_id` in the
    /// common case, or by the bounded linear scan above when `segment_id` is
    /// wrong but `base` is still a genuine member — and the FULL normal path
    /// runs against it (hash/cache eviction, OS release, slot NULL,
    /// free-list push), so no slot is ever left dangling. If `hash_find`
    /// reports NO membership at all, the header is never read and the
    /// reservation is never released: we would be releasing bytes read from
    /// an unverified/unmapped address, exactly the hazard this fix closes.
    /// The cost is a leak (the OS reservation is never returned), not
    /// UB/a double-free — recorded via
    /// [`SEGMENT_RECYCLE_UNVERIFIED_BASE_TOTAL`] and flagged via that counter
    /// only: the free path never panics. (This branch is reachable only via caller corruption
    /// or a genuine double-recycle bug — never through a normal
    /// decommit → recycle call — so no test in this crate is expected to
    /// drive it; a corrupted-but-still-registered `segment_id` instead
    /// drives the linear-scan-found path above, which is exercised by
    /// `tests/segment_table_recycle.rs::recycle_defensive_tail_evicts_hash_and_cache`).
    /// The linear scan itself only ever runs in this anomalous branch — the
    /// common `segment_id`-resolves-directly case above stays O(1).
    ///
    /// oxx R2-06: membership FIRST, keyed on `base`'s ADDRESS only — no
    /// dereference of `base` happens here, so this is safe even if `base`
    /// is already unmapped (double-recycle) or was never registered.
    /// # Safety
    ///
    /// `base` must be a mapped, live, empty Small segment with readable header
    /// and valid reservation metadata; table mutation must be owner-exclusive.
    #[cfg(feature = "alloc-decommit")]
    #[allow(unsafe_code)]
    pub unsafe fn recycle(&mut self, base: *mut u8) {
        if self.hash_find(base).is_none() {
            // `base` is not a live member of this table. We deliberately do
            // NOT read its header (it may be unmapped, foreign, or garbage)
            // and therefore do NOT release anything — releasing an
            // unverified (reservation, reservation_len) pair read from
            // untrusted memory is exactly the hazard this fix closes. This
            // is a leak, never UB.
            SEGMENT_RECYCLE_UNVERIFIED_BASE_TOTAL
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return;
        }
        // `base` is confirmed live in the hash table, so (by the table's own
        // invariant — the ONLY way to leave the hash is `hash_remove`, always
        // paired with a real release) its metadata pages are still mapped.
        // Safe to read the header now. The metadata pages (which host the
        // header at offset 0) are NEVER decommitted — only the payload is.
        let hdr = crate::alloc_core::segment_header::SegmentHeader::read_at(base);
        let reservation = hdr.reservation;
        let reservation_len = hdr.reservation_len;
        let id = hdr.segment_id as usize;
        // Fast path: the stamped `segment_id` resolves directly to `base`'s
        // slot (O(1); `base_at` bounds-checks `id` and returns null on
        // out-of-range, which never equals a real segment base).
        let slot_id = if self.base_at(id) == base {
            Some(id)
        } else {
            // Anomalous: `base` IS a live hash member (checked above) but its
            // stamped `segment_id` does not resolve to its own slot — a
            // corrupted/stale `id` (caller bug, or memory corruption; the
            // same threat model `unregister`'s sibling guard defends
            // against). `base` is unique, so a bounded linear scan over
            // `slots[0..count)` finds its real slot. Only this anomalous
            // branch pays the O(count) cost — the fast path above is
            // unaffected.
            (0..self.count as usize).find(|&i| self.base_at(i) == base)
        };
        let Some(slot_id) = slot_id else {
            // Hash member with no `slots[]` entry: the hash/slots invariant
            // is broken. Leak rather than panic on the free path; the
            // segment stays mapped, so its hash entry cannot route to
            // unmapped memory.
            SEGMENT_RECYCLE_UNVERIFIED_BASE_TOTAL
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return;
        };
        let slot = Self::slot_ptr(self.slots, slot_id);
        #[cfg(feature = "alloc-global")]
        if let Some(routes) = &mut self.routes {
            routes.remove(slot_id);
        }
        ActiveKindIndex::clear(self.active_kind, slot_id, hdr.kind);
        // OPT-B: remove the hash entry (backward-shift deletion) BEFORE
        // releasing the OS reservation.
        self.hash_remove(base);
        // PERF-P2 (Э3): evict `base` from the direct-mapped cache BEFORE
        // releasing the OS reservation. After `release_segment` the virtual
        // address `base` is unmapped; a stale cache slot still holding it
        // would let a later free of a pointer whose computed base equals
        // this recycled base HIT the cache and be (catastrophically) routed
        // as own-thread → write to unmapped/recycled memory (UB / M2
        // breach). Co-located with `hash_remove` in the SAME function →
        // structural invalidation.
        self.own_cache_clear(base);
        // Release the OS reservation. After this, `base` is invalid
        // (unmapped). We do NOT dereference `base` after this point.
        crate::alloc_core::os::release_segment(reservation, reservation_len);
        // NULL the slot so `register` can reuse it and `drop` skips it.
        crate::alloc_core::node::Node::write_struct::<*mut u8>(slot, core::ptr::null_mut());
        // Push the vacated index onto the free-list (O(1) reuse by a future
        // `register`). This index is pushed at most once per logical
        // recycle (the slot held exactly one live base, now evicted above).
        self.free_list_push(slot_id as u32);
    }

    /// The high-water mark: the number of slots ever written (including
    /// currently-NULL recyclable slots). The number of LIVE (non-NULL)
    /// segments is `self.bases().count()`.
    pub fn count(&self) -> u32 {
        self.count
    }

    /// Whether `base` is one of our registered, LIVE (non-NULL) segment bases.
    /// Used by the defensive foreign-pointer check in `dealloc`: a pointer
    /// whose computed segment base is NOT in this set is foreign (not one of
    /// our allocations) and is treated as a no-op.
    ///
    /// OPT-B: now O(1) average via the open-addressing hash table. The only
    /// time a result is `false` is when the probe chain reaches an empty slot
    /// (`null_mut()`), meaning `base` was never inserted here (backward-shift
    /// deletion — R4-8/N3 — leaves clean empty slots, never tombstones, so an
    /// empty slot unambiguously terminates the probe).
    ///
    /// Recycled (NULL) slots are NOT considered as matching any base, so a
    /// use-after-recycle pointer is correctly treated as foreign.
    ///
    /// PERF-P2 (Э3): checks the tiny direct-mapped own-segment cache FIRST. A
    /// cache HIT (numeric key matches and the stored root is non-null) returns
    /// `true` immediately — the cache holds ONLY roots proven present (filled
    /// from a won probe) and is evicted in lockstep with every hash removal
    /// (`unregister`/`recycle`), so a hit carries the exact `hash_contains ==
    /// true` guarantee (registered + live + mapped). A MISS falls through to
    /// the full `hash_find`; on a probe hit we FILL the cache slot
    /// (remember-proven) and return `true`; on a probe miss we return `false`
    /// WITHOUT filling (the cache never holds an absent base). Requires `&mut
    /// self` for the fill; all hot free-path callers (`dealloc`,
    /// `dealloc_routing`, `realloc`) already hold `&mut`.
    // R6-T16: live under `alloc-global`/internals/bench builds only.
    #[cfg_attr(
        not(any(feature = "alloc-global", feature = "internals")),
        allow(dead_code)
    )]
    #[inline(always)]
    pub fn contains_base(&mut self, base: *mut u8) -> bool {
        self.canonical_base_of_mut(base).is_some()
    }

    /// Owner lookup with cache fill; the cache retains only allocator roots.
    #[inline(always)]
    pub fn canonical_base_of_mut(&mut self, key: *mut u8) -> Option<*mut u8> {
        // N9: single-entry memo FIRST — one address compare on the hit path,
        // nothing new on the miss path. The stored root carries the table's
        // own provenance (same guarantee as `own_cache`), never the lookup
        // key. Eviction is co-located with `own_cache` in `own_cache_clear`.
        // Single compare: `last_key == LAST_EMPTY_KEY` encodes EMPTY, which
        // neither a real SEGMENT-aligned base nor a null key can match.
        if self.last_key == key.addr() {
            // The memo is part of Tier-1: count its hits with the own_cache hits.
            #[cfg(feature = "bench-internals")]
            crate::alloc_core::alloc_core::tier1_bump(
                &crate::alloc_core::alloc_core::CONTAINS_BASE_TIER1_HITS,
            );
            return Some(self.last_root);
        }
        let index = Self::cache_index(key);
        let (cached_key, cached_root) = self.own_cache[index];
        if cached_key == key.addr() {
            #[cfg(feature = "bench-internals")]
            crate::alloc_core::alloc_core::tier1_bump(
                &crate::alloc_core::alloc_core::CONTAINS_BASE_TIER1_HITS,
            );
            // N9-v3: arm the memo on an exact-base own_cache hit only.
            if cached_root.addr() == key.addr() {
                self.last_key = key.addr();
                self.last_root = cached_root;
            }
            return Some(cached_root);
        }
        #[cfg(feature = "bench-internals")]
        crate::alloc_core::alloc_core::tier1_bump(
            &crate::alloc_core::alloc_core::CONTAINS_BASE_TIER1_MISSES,
        );
        let root = self.hash_find(key)?;
        // N9-v3: a won probe fills `own_cache` only; the memo is armed by the
        // next exact-base own_cache hit.
        self.own_cache[index] = (key.addr(), root);
        Some(root)
    }
    /// TEST-ONLY (N9-v3): does the memo hold this base? (EMPTY can't match.)
    #[cfg(feature = "bench-internals")]
    pub fn memo_holds(&self, key: *mut u8) -> bool {
        self.last_key == key.addr()
    }
    /// R2-05 (independent src review round 2, task #2007): like
    /// [`contains_base_ro`](Self::contains_base_ro), but returns the STORED,
    /// canonical segment-base pointer — the exact `*mut u8` THIS table
    /// registered (carrying the allocator's own provenance over the
    /// segment) — instead of a bool.
    ///
    /// # Why this exists
    ///
    /// `base` is used only as a lookup KEY here: its ADDRESS is compared
    /// against every stored entry, never dereferenced by this method itself.
    /// A caller that derives `base` from an arbitrary/caller-supplied
    /// pointer (e.g. a `dbg_*` diagnostic accessor's `ptr` argument) and then
    /// confirms `contains_base_ro(base)` has only proven that SOME live
    /// segment happens to share `base`'s ADDRESS — not that `base` itself
    /// has valid provenance over that segment's memory. Under Rust's
    /// strict-provenance model, matching an address does not grant
    /// provenance: safe code can construct a pointer with the same address
    /// as a live allocation but zero provenance over it (e.g. via
    /// `ptr::without_provenance_mut`, or arithmetic on an unrelated
    /// allocation that happens to land on the same address), and
    /// dereferencing such a pointer is undefined behavior even though the
    /// address is "correct". Reading allocator metadata through `base`
    /// itself after only an address-membership check is exactly that hazard.
    ///
    /// The fix is to read allocator metadata through the pointer THIS
    /// method returns (the table's own stored entry, which was written by
    /// [`register`](Self::register) from a pointer the allocator itself
    /// derived and therefore genuinely has provenance over the segment) —
    /// never through the caller-supplied `base` that was only used as the
    /// lookup key. `own_cache` carries the same guarantee: its only fill
    /// stores the pointer returned by `hash_find`, not the lookup key.
    /// This remains true after a caller-derived fill, hit, or collision.
    #[inline(always)]
    pub fn canonical_base_of(&self, base: *mut u8) -> Option<*mut u8> {
        // N9: same read-only memo check as `canonical_base_of_mut` (no fill
        // possible under `&self`; the miss path is unchanged).
        // Single compare: EMPTY encoding can never match (see mut variant).
        if self.last_key == base.addr() {
            return Some(self.last_root);
        }
        let idx = Self::cache_index(base);
        let (cached_key, cached_root) = self.own_cache[idx];
        if cached_key == base.addr() {
            return Some(cached_root);
        }
        self.hash_find(base)
    }

    /// PERF-P2 (Э3): direct-mapped cache index for `base`. Segments are
    /// SEGMENT-aligned so the low `SEGMENT_SHIFT` bits are zero; shift them out
    /// first to get a dense per-segment key, then mask to `OWN_CACHE_SIZE`.
    #[inline(always)]
    fn cache_index(base: *mut u8) -> usize {
        Self::index_of(base.addr())
    }

    /// Evict every cache entry holding this canonical root. A biased payload
    /// key may map to a different index than its root, so eviction compares
    /// stored roots, not the numeric lookup keys. Called from
    /// `unregister`/`recycle` in lockstep with `hash_remove`.
    ///
    /// ## Register-reuse reasoning (why `register` does NOT touch the cache)
    ///
    /// A non-null pair holds a key and its live, stored root. Registration
    /// does not fill the cache; a won hash probe does. Unregister/recycle
    /// remove the hash entry and clear its root before that address can be
    /// reused, so neither a stale key nor a stale root can survive reuse.
    #[cfg_attr(
        not(any(feature = "alloc-decommit", feature = "alloc-xthread")),
        allow(dead_code)
    )]
    #[inline]
    pub(super) fn own_cache_clear(&mut self, base: *mut u8) {
        for slot in self.own_cache.iter_mut() {
            if slot.1 == base {
                *slot = (OWN_CACHE_EMPTY_KEY, core::ptr::null_mut());
            }
        }
        // N9: the last-segment memo is invalidated in lockstep with
        // `own_cache` — it only ever holds an exact-base (root == key)
        // entry, so comparing the stored root against the evicted `base`
        // is exact, never over-broad.
        if self.last_root == base {
            self.last_key = LAST_EMPTY_KEY;
            self.last_root = core::ptr::null_mut();
        }
    }

    /// Iterate over all **live** (non-NULL) registered segment bases
    /// (read-only). Skips NULL slots that were recycled by a prior
    /// [`recycle`](Self::recycle) call. Used by:
    /// - `AllocCore::drop` to collect every live segment's OS reservation for
    ///   release. NULL slots are already released — skipping them prevents
    ///   double-free.
    /// - cold census and diagnostics.
    ///
    /// R2-01 (task #2003): `+ '_` binds the returned iterator to `&self`'s
    /// lifetime, closing a real UAF — see `base_at`'s doc comment (below)
    /// for why an interleaved-mutation caller must use `base_at` instead of
    /// holding this iterator across a `recycle(...)` call regardless.
    #[allow(unsafe_code)]
    pub fn bases(&self) -> impl Iterator<Item = *mut u8> + '_ {
        let slots = self.slots;
        let n = self.count as usize;
        (0..n)
            .map(move |i| {
                let slot = Self::slot_ptr(slots, i) as *const *mut u8;
                crate::alloc_core::node::Node::read_struct::<*mut u8>(slot)
            })
            .filter(|&p| !p.is_null())
    }

    /// Read the slot at index `i` directly, without going through the
    /// `bases()` iterator. Returns `null_mut()` for a recycled (NULL) slot or
    /// an out-of-range index — the caller distinguishes "recycled" from
    /// "out of range" via `count()` if needed.
    ///
    /// **Why this exists (task #126; corrected by R2-01/task #2003):**
    /// `bases()`'s returned `impl Iterator` now (post-`+ '_'`, task #2003)
    /// captures `&self`'s lifetime, which conflicts with an interleaved
    /// `&mut self.table.recycle(...)` call (needed by `find_segment_with_free`
    /// to recycle segments that empty out mid-scan) — holding a live
    /// `bases()` iterator across such a call is correctly a borrow-check
    /// error. (Before task #2003's fix, `bases()`'s return type did NOT
    /// actually capture that lifetime under edition 2021's RPIT elision
    /// rules — the closure only closes over `Copy` data, so nothing forced
    /// the capture — making this comment's premise inaccurate at the time it
    /// was written; the conclusion below was always the right engineering
    /// call regardless.) `base_at` sidesteps needing `bases()` here at all:
    /// each call is a self-contained pointer read with no returned borrow,
    /// so the caller can freely interleave `base_at(i)` reads with
    /// `recycle(...)` calls in the same index-driven loop — no pre-collect
    /// buffer needed, and no bound on how many segments can be recycled in
    /// one scan.
    #[allow(unsafe_code)]
    #[inline(always)]
    pub fn base_at(&self, i: usize) -> *mut u8 {
        if i >= self.count as usize {
            return core::ptr::null_mut();
        }
        let slot = Self::slot_ptr(self.slots, i) as *const *mut u8;
        crate::alloc_core::node::Node::read_struct::<*mut u8>(slot)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Address of slot `i` within the registry array. Pure pointer arithmetic
    /// through the `node` seam.
    #[inline]
    fn slot_ptr(slots: *mut *mut u8, i: usize) -> *mut *mut u8 {
        crate::alloc_core::node::Node::offset(slots as *mut u8, i * core::mem::size_of::<*mut u8>())
            as *mut *mut u8
    }
}
