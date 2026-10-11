//! `SegmentTable` unregistration and Large-route parking (pure move out of
//! `segment_table_impl.rs`, which hit the source line cap).

use super::*;

impl SegmentTable {
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
    pub(crate) fn unregister(&mut self, base: *mut u8) {
        // The returned (empty) parked route drops at the end of the statement.
        self.unregister_impl(base, false);
    }

    pub(crate) fn claim_large_owner_live(&self, base: *mut u8) -> bool {
        #[cfg(feature = "alloc-global")]
        if let Some(routes) = &self.routes {
            let id = SegmentHeader::segment_id_at(base) as usize;
            let route = routes.get(id);
            if route.root() != base {
                std::process::abort();
            }
            return route.claim_large_owner_live().is_ok();
        }
        #[cfg(not(feature = "alloc-global"))]
        let _ = base;
        true
    }

    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn finish_large_route_reuse(&self, id: u32, generation: Option<u64>) {
        #[cfg(feature = "alloc-global")]
        if let (Some(routes), Some(generation)) = (&self.routes, generation) {
            let route = routes.get(id as usize);
            if !route.finish_large_reuse_after_reset(generation) {
                std::process::abort();
            }
        }
        #[cfg(not(feature = "alloc-global"))]
        let _ = (id, generation);
    }

    #[cfg_attr(
        not(any(feature = "alloc-decommit", feature = "alloc-xthread")),
        allow(dead_code)
    )]
    pub(crate) fn unregister_parking(&mut self, base: *mut u8) -> super::ParkedRoute {
        self.unregister_impl(base, true)
    }

    fn unregister_impl(&mut self, base: *mut u8, retain_route: bool) -> super::ParkedRoute {
        #[cfg(not(feature = "alloc-global"))]
        let _ = retain_route;
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
                        .unwrap_or_else(|| std::process::abort())
                } else {
                    return super::ParkedRoute::none();
                }
            }
            #[cfg(not(feature = "alloc-global"))]
            return super::ParkedRoute::none();
        };
        let slot = Self::slot_ptr(self.slots, slot_id);
        let header = SegmentHeader::read_at(base);
        let key = crate::alloc_core::os::segment_base_of_ptr(
            crate::alloc_core::node::Node::offset(base, header.payload_offset),
        );
        #[cfg(feature = "alloc-global")]
        let parked = if let Some(routes) = &mut self.routes {
            if retain_route {
                super::ParkedRoute::new(routes.take(slot_id))
            } else {
                routes.remove(slot_id);
                super::ParkedRoute::none()
            }
        } else {
            super::ParkedRoute::none()
        };
        #[cfg(not(feature = "alloc-global"))]
        let parked = super::ParkedRoute::none();
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
        parked
    }
}
