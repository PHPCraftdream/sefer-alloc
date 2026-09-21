//! R7-A1/R7-A2/R11-6 directory-sidecar machinery — sidecar materialisation,
//! accessors, the empty↔non-empty transition helpers, dirty-segment drain,
//! and the free `directory_node_id_of` router (mechanical split of the former
//! flat `alloc_core_small.rs`; pure code movement, no behavior changed).
//! Every item here is `alloc-segment-directory`-gated.

use crate::alloc_core::os;
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind, SegmentMeta, FREE_LIST_NULL};

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
    /// one OS VM reservation + one full-table-scan rebuild.
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
        let ptr = match os::reserve_directory_sidecar() {
            Some(p) => p,
            None => return, // OOM — mechanism stays off, not an error.
        };
        // One-time rebuild: walk every registered small/primordial segment,
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
        let dir = unsafe { crate::alloc_core::sidecar::deref_mut(ptr) };
        dir.rebuild_from_table(&self.table);

        self.directory_sidecar = ptr;
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
    pub(in crate::alloc_core) fn directory(
        &self,
    ) -> Option<&crate::alloc_core::segment_directory::SegmentDirectory> {
        if self.directory_sidecar.is_null() {
            None
        } else {
            // SAFETY: see the `#[allow(unsafe_code)]` justification above.
            Some(unsafe { crate::alloc_core::sidecar::deref(self.directory_sidecar) })
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
    pub(in crate::alloc_core) fn directory_mut(
        &mut self,
    ) -> Option<&mut crate::alloc_core::segment_directory::SegmentDirectory> {
        if self.directory_sidecar.is_null() {
            None
        } else {
            // SAFETY: see the `#[allow(unsafe_code)]` justification above.
            Some(unsafe { crate::alloc_core::sidecar::deref_mut(self.directory_sidecar) })
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
    /// Clears the corresponding bit in the segment's node bucket.
    ///
    /// If `base` is null (the stale-bit-clearing path in directory validation,
    /// where the segment was already recycled), the node is unknowable so the
    /// bit is cleared across ALL node buckets (`clear_bit_all_nodes`).
    ///
    /// No-op if the directory is not materialised.
    #[inline]
    pub(in crate::alloc_core) fn publish_empty(
        &mut self,
        base: *mut u8,
        class_idx: usize,
        slot_idx: usize,
    ) {
        if let Some(dir) = self.directory_mut() {
            if base.is_null() {
                dir.clear_bit_all_nodes(class_idx, slot_idx);
            } else {
                dir.clear_bit(directory_node_id_of(base), class_idx, slot_idx);
            }
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

    /// R7-A4: drain all dirty segments' remote-free rings. Called at the top
    /// of `find_segment_with_free_impl` BEFORE the directory scan, so the
    /// directory bits reflect the latest cross-thread frees.
    ///
    /// For each dirty word, `swap(0, Acquire)`. For each set bit:
    ///   1. `base_at(slot_idx)` — skip if null (recycled slot, stale bit).
    ///   2. Validate kind is Small/Primordial.
    ///   3. Validate `segment_id_at(base) == slot_idx` (revalidation: a slot
    ///      may have been recycled and reused for a different segment since
    ///      the producer set the bit).
    ///   4. Drain the segment's remote-free ring (REUSING the existing
    ///      drain body from the directory-hit path — P1-compliant).
    ///   5. `sync_directory_for_segment_classes` to publish reclaimed blocks
    ///      into the directory (R8-1: only the classes the drain touched).
    ///   6. Handle decommit/pool hysteresis.
    ///   7. Refresh the `ring_drain_head` cache.
    ///
    /// Increments the `dirty_segments_drained` A0 counter per drained segment.
    ///
    /// R9-6 (class-aware dirty routing judge, measurement-only): additionally
    /// increments `wasted_dirty_drains` for each drained segment whose ring,
    /// once drained, produced ZERO reclaimed blocks of the `class_idx` the
    /// caller is searching for — those are drains that per-(segment,class)
    /// dirty routing would have avoided entirely. This is purely diagnostic
    /// (no algorithmic change); the comparison happens once per successful
    /// drain against the R8-1 `changed_classes` bitmap the loop already
    /// accumulates.
    ///
    /// No-op if `dirty_segments` is not bound (pre-bind window) or the
    /// directory sidecar is not materialised.
    ///
    /// R12-7 stage 2 (`class-aware-dirty`, EXPERIMENTAL): when the feature is
    /// on AND this heap's per-(segment, class) sidecar is already
    /// materialised (`dirty_by_class::get_per_class_dirty` — read-only, never
    /// materialises from this side, see that function's doc comment), the
    /// candidate-segment scan source switches from the shared per-segment
    /// `dirty_segments` bitmap (16 words, ANY class) to `class_idx`'s OWN
    /// `WORDS_PER_CLASS`-word slice of the per-class sidecar — an O(D_class)
    /// scan instead of O(D). The scan-body loop below (validation, ring
    /// drain, directory sync, decommit hysteresis) is BYTE-FOR-BYTE
    /// UNCHANGED regardless of which bitmap fed it: once a candidate segment
    /// is picked, its ENTIRE ring is still drained in one pass exactly as
    /// before, so entries of OTHER classes in that same ring are still
    /// reclaimed and published this same pass — a per-class bit is a VISIT
    /// HINT only, never load-bearing for correctness (see
    /// `dirty_by_class`'s module doc for the full lost-wakeup argument). If
    /// the sidecar is not yet materialised (this heap has never received a
    /// class-routed cross-thread free), the scan falls back to the shared
    /// per-segment bitmap unchanged — identical behaviour to the feature
    /// being off.
    ///
    /// R13-1 (task #271, P0 fix): BEFORE consulting `per_class_words` at all,
    /// this heap's coarse-only latch
    /// (`registry::heap_slot::HeapSlotRemote::sidecar_oom_latch`) is checked.
    /// If it is set, the per-class path is ignored UNCONDITIONALLY for this
    /// heap — even if the sidecar happens to be materialised right now — and
    /// the scan always uses the coarse `dirty_segments` bitmap. This closes a
    /// visibility gap the plain "sidecar not yet materialised -> fall back"
    /// rule above does NOT cover: if producer A's push hit sidecar OOM (set
    /// only the coarse bit for its entry, no per-class bit — the sidecar
    /// genuinely did not exist for that push) and a LATER producer B on the
    /// SAME heap successfully materialises the sidecar afterwards, the
    /// sidecar is materialised NOW (so the plain fallback rule above would no
    /// longer trigger), yet producer A's entry never got a per-class bit —
    /// it would be invisible to a per-class-only scan until the periodic
    /// full-scan fallback or an OOM-rescue scan finds it. The latch makes the
    /// two publication paths mutually exclusive for the lifetime of the
    /// heap slot: once ANY push has ever failed to materialise the sidecar,
    /// this heap is permanently pinned to the coarse scan — always correct,
    /// same fallback behaviour the feature has when OFF, just without the
    /// class-scoped speedup from then on for this one heap. See
    /// `HeapSlotRemote::sidecar_oom_latch`'s doc comment for the full design
    /// and `apply_resolved_dirty_bit`'s doc comment
    /// (`registry::heap_core_xthread`) for the producer-side write.
    ///
    /// R14-2 (task #287, P0 fix): this read is `Acquire`, pairing with the
    /// producer's `Release` store in `apply_resolved_dirty_bit` — this is
    /// the SAME pairing `HeapSlotRemote::sidecar_oom_latch`'s doc comment has
    /// always claimed and the loom model
    /// (`tests/loom_class_aware_dirty.rs::latched_visit_and_drain`) has
    /// always checked. Three independent Round 13 reviews found this
    /// production read was actually `Relaxed` — a real Acquire/Relaxed
    /// three-way divergence against both the field doc and the loom model,
    /// which does NOT prove anything about a `Relaxed` production read (a
    /// loom pass under a strictly stronger ordering than production ships is
    /// not evidence the weaker production ordering is sound). `Acquire` here
    /// is free on x86 (already an implicit acquire load) and removes the
    /// compiler-reordering risk a `Relaxed` load carries in principle (a
    /// hypothetical hoist of this load above an earlier per-class-sidecar
    /// read in the SAME function, on a future refactor or a weak-memory
    /// target). With `Acquire`, the latch establishes real happens-before
    /// from "the push that tripped it" to "this drain's decision to trust
    /// only the coarse bitmap" — matching the doc comment's and the loom
    /// model's claim exactly, closing the divergence rather than
    /// documenting it away.
    #[cfg(feature = "alloc-xthread")]
    pub(in crate::alloc_core) fn drain_dirty_segments<
        #[cfg(feature = "fastbin")] F: Fn(*mut u8, usize) -> bool,
    >(
        &mut self,
        #[cfg_attr(not(feature = "alloc-stats"), allow(unused_variables))] class_idx: usize,
        #[cfg(feature = "fastbin")] is_in_magazine: &F,
    ) {
        let ds = match self.dirty_segments {
            Some(ds) => ds,
            None => return, // Pre-bind: no dirty bitmap.
        };
        // The directory must be materialised for dirty routing to be useful.
        if self.directory_sidecar.is_null() {
            return;
        }
        let small_cur = self.small_cur;

        // R13-1 (task #271, P0 fix), R14-2 (task #287): the coarse-only
        // latch, checked BEFORE resolving the per-class slice at all — see
        // this function's doc comment for the full rationale. `Acquire`:
        // pairs with `apply_resolved_dirty_bit`'s `Release` store
        // (`registry::heap_core_xthread`) — see the doc comment's ordering
        // discussion.
        #[cfg(feature = "class-aware-dirty")]
        let coarse_only_latched: bool = self
            .sidecar_oom_latch
            .is_some_and(|latch| latch.load(core::sync::atomic::Ordering::Acquire));

        // R12-7 stage 2: resolve the class-scoped word slice, if available
        // AND the coarse-only latch has not tripped for this heap.
        // `per_class_words` borrows out of the `'static` sidecar (through the
        // `'static` cell handle `self.dirty_by_class`), so it outlives `self`
        // and there is no borrow conflict with the `&mut self` calls inside
        // the loop below.
        #[cfg(feature = "class-aware-dirty")]
        let per_class_words: Option<&'static [core::sync::atomic::AtomicU64]> =
            if coarse_only_latched {
                None
            } else {
                self.dirty_by_class
                    .and_then(crate::alloc_core::dirty_by_class::get_per_class_dirty)
                    .map(|pc| {
                        let start =
                            class_idx * crate::alloc_core::segment_directory::WORDS_PER_CLASS;
                        let end = start + crate::alloc_core::segment_directory::WORDS_PER_CLASS;
                        &pc.words[start..end]
                    })
            };

        // The per-word iteration: `(base_word_index, dirty_bits_for_that_word)`
        // pairs, drawn from either the class-scoped slice (base index offset
        // by nothing — the slice already starts at the segment-word origin,
        // since `WORDS_PER_CLASS` covers the full `MAX_SEGMENTS` slot space
        // exactly like `ds` does) or the shared per-segment bitmap.
        #[cfg(feature = "class-aware-dirty")]
        let scan_source: &[core::sync::atomic::AtomicU64] =
            per_class_words.unwrap_or(ds.as_slice());
        #[cfg(not(feature = "class-aware-dirty"))]
        let scan_source: &[core::sync::atomic::AtomicU64] = ds.as_slice();

        for (w, ds_word) in scan_source.iter().enumerate() {
            // #1983: a plain Relaxed LOAD-filter before the read-and-clear
            // swap — skip the `lock xchg`-class RMW entirely when the word
            // reads 0. Sound because the swap below is used purely as
            // "read-and-clear": a load that races a producer's concurrent
            // `fetch_or` (setting a bit between this load and the `continue`)
            // just leaves that bit set until a LATER drain — precisely the
            // module's own documented P4 bounded-deferral contract (see
            // `remote_free_ring/mod.rs`'s "a later drain picks it up"). On a
            // non-zero load the swap still runs, unchanged, preserving the
            // Release/Acquire pairing with the producer below.
            if ds_word.load(core::sync::atomic::Ordering::Relaxed) == 0 {
                continue;
            }
            // Acquire: pairs with the producer's Release fetch_or (either the
            // per-segment bit or, under `class-aware-dirty`, the per-class
            // bit — both use the identical Release/Acquire pairing).
            let dirty = ds_word.swap(0, core::sync::atomic::Ordering::Acquire);
            if dirty == 0 {
                continue;
            }
            let mut bits = dirty;
            while bits != 0 {
                let j = bits.trailing_zeros() as usize;
                bits &= bits - 1; // clear lowest set bit
                let slot_idx = w * 64 + j;

                // Validation 1: base must be non-null.
                let base = self.table.base_at(slot_idx);
                if base.is_null() {
                    continue; // Recycled slot, stale dirty bit.
                }

                // Validation 2: must be Small/Primordial.
                if !matches!(
                    SegmentHeader::kind_at(base),
                    SegmentKind::Small | SegmentKind::Primordial
                ) {
                    continue;
                }

                // Validation 3: segment_id must match slot_idx (revalidation
                // against slot recycle — a recycled-then-reused slot may hold
                // a different segment whose segment_id != the old slot_idx
                // the producer saw when setting the bit).
                if SegmentHeader::segment_id_at(base) as usize != slot_idx {
                    continue;
                }

                // REUSE the existing A3/scan drain body (P1-compliant).
                let mut meta_for_ring = SegmentMeta::new(base);
                let ring = meta_for_ring.remote_ring();
                let cached_head = meta_for_ring.ring_drain_head_of();
                if ring.tail_relaxed() != cached_head {
                    #[cfg(feature = "alloc-decommit")]
                    let mut decommit_happened = false;
                    // R8-1 (task #214): accumulate the set of classes this drain
                    // pass touches, so the post-drain directory sync inspects
                    // ONLY those classes (O(popcount)) instead of re-sweeping
                    // all SMALL_CLASS_COUNT classes.
                    let mut changed_classes: u64 = 0;
                    let new_head = ring.drain(|off| {
                        #[cfg(feature = "fastbin")]
                        let reclaimed = Self::reclaim_offset_checked(base, off, &is_in_magazine);
                        #[cfg(not(feature = "fastbin"))]
                        let reclaimed = Self::reclaim_offset(base, off);
                        if reclaimed {
                            #[cfg(feature = "alloc-decommit")]
                            if Self::dec_live_and_maybe_decommit(base, small_cur) {
                                decommit_happened = true;
                            }
                            // R10-3: gate the class bit on `reclaimed` — a
                            // rejected entry never mutated the BinTable for its
                            // class (every early `return false` in
                            // reclaim_offset[_checked] precedes `set_head`/
                            // `mark_free`), so recording it would (a) cause a
                            // spurious directory sync for an unchanged class
                            // and (b) make the R9-6 WASTED_DIRTY_DRAINS metric
                            // under-count: a drain that rejected every entry of
                            // the sought class still looked "not wasted".
                            changed_classes |=
                                1u64 << crate::alloc_core::remote_free_ring::entry_class_idx(off);
                        }
                    });
                    // A2 post-drain directory sync.
                    {
                        let sid = SegmentHeader::segment_id_at(base) as usize;
                        self.sync_directory_for_segment_classes(base, sid, changed_classes);
                    }
                    // R9-6 (class-aware dirty routing judge, measurement-only):
                    // if this drain — triggered by a `find_segment_with_free_impl(class_idx)`
                    // call — produced ZERO reclaimed blocks of the sought class
                    // (the sought class's bit is NOT in `changed_classes`), it
                    // was wasted work from THAT caller's perspective. Per-(segment,
                    // class) dirty routing would have avoided visiting this segment
                    // entirely. Purely diagnostic — no algorithmic effect.
                    #[cfg(feature = "alloc-stats")]
                    if changed_classes & (1u64 << class_idx) == 0 {
                        crate::alloc_core::directory_stats::WASTED_DIRTY_DRAINS
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                    }
                    // P1-b: decommit/pool hysteresis.
                    #[cfg(feature = "alloc-decommit")]
                    if decommit_happened {
                        self.release_or_pool_empty_segment(base);
                        // The segment is now released/pooled; skip the head
                        // refresh (the segment may be unmapped).
                        // R7-A0: count this dirty segment as drained.
                        #[cfg(feature = "alloc-stats")]
                        crate::alloc_core::directory_stats::DIRTY_SEGMENTS_DRAINED
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        continue;
                    }
                    // P1-d: refresh the ring_drain_head cache.
                    meta_for_ring.set_ring_drain_head(new_head);
                }

                // R7-A0: count this dirty segment as drained.
                #[cfg(feature = "alloc-stats")]
                crate::alloc_core::directory_stats::DIRTY_SEGMENTS_DRAINED
                    .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            }
        }
    }
}
