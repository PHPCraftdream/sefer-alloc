//! R7-A1/R7-A2/R11-6 directory-sidecar machinery — sidecar materialisation,
//! accessors, the empty↔non-empty transition helpers, dirty-segment drain,
//! and the free `directory_node_id_of` router (mechanical split of the former
//! flat `alloc_core_small.rs`; pure code movement, no behavior changed).
//! Every item here is `alloc-segment-directory`-gated.

use crate::alloc_core::os;
use crate::alloc_core::segment_header::{SegmentMeta, FREE_LIST_NULL};

use crate::alloc_core::alloc_core::AllocCore;

// ── R7-A1: directory sidecar materialisation ────────────────────────────────

/// R11-6: read the NUMA `node_id` from a segment header for directory bitmap
/// routing. Under `numa-aware`, reads `SegmentMeta::node_id_of()`. Under
/// non-`numa-aware`, returns 0 unconditionally (the single-bucket directory
/// ignores the value; `node_id_of` is cfg-gated out). Returns 0 for a null
/// base (callers that clear stale bits handle the null case separately via
/// `clear_bit_all_nodes`).
#[cfg(feature = "alloc-segment-directory")]
#[inline]
fn directory_node_id_of(base: *mut u8) -> u32 {
    #[cfg(feature = "numa-aware")]
    {
        if base.is_null() {
            0
        } else {
            SegmentMeta::new(base).node_id_of()
        }
    }
    #[cfg(not(feature = "numa-aware"))]
    {
        let _ = base;
        0
    }
}

#[cfg(feature = "alloc-segment-directory")]
impl AllocCore {
    /// Check whether the directory sidecar should be materialised and, if
    /// so, reserve it and rebuild the bitmap from the current segment table.
    ///
    /// Called after every successful `table.register()` on the small-segment
    /// path. Fast path (already materialised OR below threshold): one
    /// null-check + one u32 comparison. Slow path (first materialisation):
    /// one OS VM reservation + one full-table-scan rebuild, including after
    /// a cold trim released the previous directory.
    ///
    /// Sidecar OOM is NOT allocator OOM: on reserve failure, the pointer
    /// stays null and the mechanism is simply off (the linear scan fallback
    /// is used, unchanged from today). Never abort.
    #[allow(unsafe_code)] // R14-9 (task #294): calls the `unsafe fn sidecar::deref_mut`
                          // boundary right after `reserve_directory_sidecar` proves
                          // `ptr` non-null and fully initialised. `AllocCore`'s
                          // owner-only discipline (neither `Send` nor `Sync`) rules
                          // out a concurrent writer, and no other reference to the
                          // sidecar is live across this call.
                          // R2-12: the returned reference is OWNER-TIED (`&'a` from `self`), not `'static` — the sidecar's VM span is released when this core drops its `directory_sidecar_vm` token.
    pub(in crate::alloc_core) fn maybe_materialize_directory(&mut self) {
        // Fast path: already materialised.
        if !self.directory_sidecar.is_null() {
            return;
        }
        // Below threshold: not worth materialising yet.
        if self.table.count()
            < crate::alloc_core::segment_directory::DIRECTORY_MATERIALIZE_THRESHOLD
        {
            return;
        }
        // Slow path: reserve the sidecar via direct OS VM (M5-clean).
        // R2-12: the returned token OWNS the sidecar's VM span; it is stored
        // in `self.directory_sidecar_vm` below so the span is released when
        // this core drops (before R2-12 the span was leaked for the process
        // lifetime — unbounded under standalone create/drop churn).
        let (ptr, vm) = match os::reserve_directory_sidecar() {
            Some(pair) => pair,
            None => return, // OOM — mechanism stays off, not an error.
        };
        // Fresh rebuild: walk every registered small/primordial segment,
        // read each class's BinTable head, set the exact class_nonempty bits.
        // The sidecar's bitmap fields were OS-zeroed (all bits clear) and its
        // `node_ids` (numa-aware) already repaired by `reserve_directory_sidecar`
        // (via `sidecar::reserve_zeroed_with`), so only non-empty heads need
        // to be SET here.
        //
        // SAFETY (R14-9, task #294): `ptr` was just returned by
        // `reserve_directory_sidecar` (this function's own call above), so it
        // is non-null and points at a value that constructor brought to a
        // fully valid state. `AllocCore`'s owner-only discipline (neither
        // `Send` nor `Sync`) rules out a concurrent writer, and no other
        // reference to this sidecar is live across this call.
        let dir = unsafe { crate::alloc_core::sidecar::deref_mut(ptr, &*self) };
        dir.rebuild_from_table(&self.table);

        self.directory_sidecar = ptr;
        self.directory_sidecar_vm = Some(vm);
    }

    /// Drop only the owner-private directory on a cold trim. The stable
    /// cross-thread dirty sidecar and its publication state are independent.
    /// A later small-segment registration lazily rebuilds the directory from
    /// the still-live table, including its bitmap and NUMA bucket counters.
    pub(crate) fn release_directory_for_cold_trim(&mut self) {
        self.directory_sidecar = core::ptr::null_mut();
        self.directory_miss_streak.fill(0);
        drop(self.directory_sidecar_vm.take());
    }

    /// Return a shared reference to the materialised directory sidecar, or
    /// `None` if not yet materialised.
    #[inline]
    #[allow(unsafe_code)] // R14-9 (task #294): calls the `unsafe fn sidecar::deref`
                          // boundary. Sound: `self.directory_sidecar` is just proven
                          // non-null (produced only by `reserve_directory_sidecar`,
                          // which fully initialises it), and `AllocCore`'s owner-only
                          // discipline (neither `Send` nor `Sync`) rules out a
                          // concurrent writer. The returned `&SegmentDirectory` does
                          // not outlive this call.
                          // R2-12: the returned reference is OWNER-TIED (`&'a` from `self`), not `'static` — the sidecar's VM span is released when this core drops its `directory_sidecar_vm` token.
    pub(in crate::alloc_core) fn directory(
        &self,
    ) -> Option<&crate::alloc_core::segment_directory::SegmentDirectory> {
        if self.directory_sidecar.is_null() {
            None
        } else {
            // SAFETY: see the `#[allow(unsafe_code)]` justification above.
            Some(unsafe { crate::alloc_core::sidecar::deref(self.directory_sidecar, self) })
        }
    }

    /// Return a mutable reference to the materialised directory sidecar, or
    /// `None` if not yet materialised.
    #[inline]
    #[allow(unsafe_code)] // R14-9 (task #294): calls the `unsafe fn sidecar::deref_mut`
                          // boundary. Sound: `self.directory_sidecar` is just proven
                          // non-null (produced only by `reserve_directory_sidecar`,
                          // fully initialised), `AllocCore`'s owner-only discipline
                          // (neither `Send` nor `Sync`) rules out a concurrent
                          // reader/writer, and no other reference to the sidecar is
                          // live across this call.
                          // R2-12: the returned reference is OWNER-TIED (`&'a` from `self`), not `'static` — the sidecar's VM span is released when this core drops its `directory_sidecar_vm` token.
    pub(in crate::alloc_core) fn directory_mut(
        &mut self,
    ) -> Option<&mut crate::alloc_core::segment_directory::SegmentDirectory> {
        if self.directory_sidecar.is_null() {
            None
        } else {
            // SAFETY: see the `#[allow(unsafe_code)]` justification above.
            Some(unsafe { crate::alloc_core::sidecar::deref_mut(self.directory_sidecar, &*self) })
        }
    }

    // ── R7-A2: centralized empty↔non-empty transition helpers ──────────────
    //
    // These are the SINGLE choke point for directory bitmap maintenance. Every
    // site that mutates a per-class BinTable head calls one of these three
    // helpers to keep the directory in sync. The helpers are cheap no-ops when
    // the sidecar is not materialised (below threshold, OOM-disabled, or
    // feature OFF).
    //
    // R11-6: the helpers derive the segment's NUMA node from `base` and route
    // the bit-set/clear to the correct per-node bucket under `numa-aware`.
    // Under non-`numa-aware` the node dimension is inert (single bucket).

    /// R7-A2 / R11-6: notify the directory that class `class_idx` in segment
    /// slot `slot_idx` (segment base `base`) transitioned from empty to
    /// non-empty (old_head was FREE_LIST_NULL, new_head is not). Sets the
    /// corresponding bit in the segment's node bucket.
    ///
    /// No-op if the directory is not materialised.
    #[inline]
    pub(in crate::alloc_core) fn publish_nonempty(
        &mut self,
        base: *mut u8,
        class_idx: usize,
        slot_idx: usize,
    ) {
        if let Some(dir) = self.directory_mut() {
            dir.set_bit(directory_node_id_of(base), class_idx, slot_idx);
        }
    }

    /// R7-A2 / R11-6: notify the directory that class `class_idx` in segment
    /// slot `slot_idx` (segment base `base`) transitioned from non-empty to
    /// empty (old_head was not FREE_LIST_NULL, new_head is FREE_LIST_NULL).
    /// Clears the corresponding bit in every node bucket. A segment may have
    /// published its bit to the unknown bucket before its node obtained a
    /// dedicated bucket; clearing only the node's current bucket would leave
    /// that old candidate stale indefinitely.
    ///
    /// Clearing across all buckets also handles a recycled null base and bits
    /// left behind by a node's transition from the unknown bucket to a
    /// dedicated one. `clear_bit_all_nodes` keeps each dedicated bucket's
    /// active-bit count in sync.
    ///
    /// No-op if the directory is not materialised.
    #[inline]
    pub(in crate::alloc_core) fn publish_empty(
        &mut self,
        _base: *mut u8,
        class_idx: usize,
        slot_idx: usize,
    ) {
        if let Some(dir) = self.directory_mut() {
            dir.clear_bit_all_nodes(class_idx, slot_idx);
        }
    }

    /// R7-A2: clear ALL class bits for segment slot `slot_idx`. Called on
    /// segment recycle/release so a reused slot does not inherit stale bits
    /// from the old segment lifetime.
    ///
    /// No-op if the directory is not materialised.
    #[inline]
    pub(in crate::alloc_core) fn clear_segment_directory(&mut self, slot_idx: usize) {
        if let Some(dir) = self.directory_mut() {
            dir.clear_slot(slot_idx);
        }
    }

    /// R8-1 (task #214): sync the directory for segment at `base` / `slot_idx`
    /// by inspecting ONLY the classes whose bit is set in `changed_classes` —
    /// the set of classes a ring-drain pass actually touched — instead of
    /// sweeping all `SMALL_CLASS_COUNT` classes. O(popcount(`changed_classes`))
    /// reads instead of O(`SMALL_CLASS_COUNT`). This closes the
    /// O(D × SMALL_CLASS_COUNT) remote-dirty directory-sync regression: a
    /// drain that reclaims blocks of (say) 2 classes does 2 reads, not 49.
    ///
    /// `changed_classes` is accumulated by a ring-drain closure via
    /// `changed_classes |= 1u64 << entry_class_idx(off)` for every drained
    /// entry, so it carries every distinct class the pass touched regardless of
    /// how many entries each class contributed.
    ///
    /// R11-6: derives the node bucket from `base` and routes each set/clear to
    /// the correct per-node bucket.
    ///
    /// No-op if the directory is not materialised or `changed_classes == 0`.
    pub(crate) fn sync_directory_for_segment_classes(
        &mut self,
        base: *mut u8,
        slot_idx: usize,
        changed_classes: u64,
    ) {
        if changed_classes == 0 {
            return;
        }
        if let Some(dir) = self.directory_mut() {
            let node_id = directory_node_id_of(base);
            let meta = SegmentMeta::new(base);
            let bt = meta.bin_table();
            let mut bits = changed_classes;
            while bits != 0 {
                let c = bits.trailing_zeros() as usize;
                bits &= bits - 1; // clear lowest set bit
                if bt.head(c) != FREE_LIST_NULL {
                    dir.set_bit(node_id, c, slot_idx);
                } else {
                    dir.clear_bit(node_id, c, slot_idx);
                }
            }
        }
    }
}
