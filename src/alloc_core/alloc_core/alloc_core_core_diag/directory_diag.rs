//! `internals`-gated directory-sidecar introspection of [`AllocCore`] — the
//! R7-A1 bitmap/bucket probes, miss-streak seams, the rebuild hook, and the
//! class-aware-dirty latch seams (mechanical split of the former flat
//! `alloc_core_core_diag.rs`; pure code movement, no behavior changed).

use crate::alloc_core::alloc_core::AllocCore;
use crate::alloc_core::directory_stats;

/// Sol-F1 (task #563): `internals`-gated — every other `dbg_*` diagnostic
/// hook in this file. See this file's module doc for the full rationale.
#[cfg(feature = "internals")]
impl AllocCore {
    /// R9-8 (task #230): count of forced OOM-rescue scans that found a real
    /// free block the directory had hidden (and thus avoided a spurious OOM).
    /// Distinguished from `dbg_directory_miss_self_heal` (the periodic
    /// re-validation's routine self-heals). Reads 0 unless `alloc-stats` is on
    /// and `alloc-segment-directory` is active.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_rescue_oom_avoided() -> u64 {
        directory_stats::DIRECTORY_RESCUE_OOM_AVOIDED.load(core::sync::atomic::Ordering::Relaxed)
    }

    // ── R7-A1: directory sidecar introspection ────────────────────────────

    /// R7-A1: whether the per-class segment directory sidecar has been
    /// materialised for this `AllocCore`. `true` iff the sidecar pointer is
    /// non-null (the threshold was crossed and the OS reservation succeeded).
    /// Always returns `false` when `alloc-segment-directory` is OFF.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_is_materialised(&self) -> bool {
        #[cfg(feature = "alloc-segment-directory")]
        {
            !self.directory_sidecar.is_null()
        }
        #[cfg(not(feature = "alloc-segment-directory"))]
        {
            false
        }
    }

    /// R7-A1: read a single bit from the materialised directory sidecar.
    /// Returns `None` if the directory is not materialised (below threshold,
    /// feature OFF, or sidecar OOM). Returns `Some(true/false)` otherwise.
    ///
    /// R11-6: under `numa-aware`, this ORs across ALL node buckets (returns
    /// `true` if ANY node has the bit set for this class/slot). For per-node
    /// verification use `dbg_directory_get_bit_for_node`.
    ///
    /// Test-only — lets integration tests verify the rebuilt bitmap matches
    /// the actual `BinTable` state.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_get_bit(&self, class_idx: usize, slot_idx: usize) -> Option<bool> {
        #[cfg(feature = "alloc-segment-directory")]
        {
            self.directory().map(|dir| {
                let word = slot_idx / 64;
                let bit_mask = 1u64 << (slot_idx % 64);
                (0..crate::alloc_core::segment_directory::NODE_BITMAPS)
                    .any(|nb| dir.class_nonempty_by_node[nb][class_idx][word] & bit_mask != 0)
            })
        }
        #[cfg(not(feature = "alloc-segment-directory"))]
        {
            let _ = (class_idx, slot_idx);
            None
        }
    }

    /// R11-6 TEST-ONLY: read a single bit from a SPECIFIC node bucket. Returns
    /// `None` if the directory is not materialised. Used by the NUMA
    /// local-first/foreign-fallback test and the per-node oracle to verify
    /// that bits are placed in the correct node bucket.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-segment-directory", feature = "numa-aware"))]
    #[must_use]
    pub fn dbg_directory_get_bit_for_node(
        &self,
        node_id: u32,
        class_idx: usize,
        slot_idx: usize,
    ) -> Option<bool> {
        self.directory()
            .map(|dir| dir.get_bit(node_id, class_idx, slot_idx))
    }

    /// R11-6 TEST-ONLY: return the number of node buckets in the directory
    /// (`NODE_BITMAPS`). Used by the NUMA oracle to iterate all buckets.
    #[doc(hidden)]
    #[cfg(feature = "alloc-segment-directory")]
    #[must_use]
    pub fn dbg_directory_node_bitmaps() -> usize {
        crate::alloc_core::segment_directory::NODE_BITMAPS
    }

    /// R13-2 (task #272) TEST-ONLY: read the CURRENT read-only bucket index
    /// `node_id` maps to (mirrors `SegmentDirectory::node_bucket` — does NOT
    /// register a new bucket). Returns `crate::alloc_core::segment_directory::MAX_NODES`
    /// (the shared unknown-bucket index) for a node that has never claimed a
    /// bucket, OR whose bucket was freed by the R13-2 reuse mechanism because
    /// every bit it ever set went back to 0. Returns `None` if the directory
    /// is not materialised. Lets the bucket-slot-reuse migration test observe
    /// directly whether a node id lands in a REAL per-node bucket or has
    /// fallen back to the shared unknown bucket, without depending on which
    /// class/slot bits happen to be set at the moment of the check.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-segment-directory", feature = "numa-aware"))]
    #[must_use]
    pub fn dbg_directory_node_bucket_for(&self, node_id: u32) -> Option<usize> {
        self.directory().map(|dir| dir.node_bucket_ro(node_id))
    }

    /// R13-2 (task #272) TEST-ONLY: read the current value of the R13-2
    /// active-bit counter for real-node bucket index `bucket` (`[0,
    /// MAX_NODES)`). Returns `None` if the directory is not materialised or
    /// `bucket >= MAX_NODES` (the shared unknown bucket has no counter — see
    /// `SegmentDirectory::active_bits_by_node`'s doc comment). Lets a test
    /// verify the counter reaches exactly 0 (and no lower/higher) when a
    /// bucket's node is driven fully idle, independent of the derived
    /// `dbg_directory_node_bucket_for` bucket-index observation.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-segment-directory", feature = "numa-aware"))]
    #[must_use]
    pub fn dbg_directory_active_bits_for_bucket(&self, bucket: usize) -> Option<u32> {
        self.directory().and_then(|dir| {
            if bucket < crate::alloc_core::segment_directory::MAX_NODES {
                Some(dir.active_bits_by_node[bucket])
            } else {
                None
            }
        })
    }

    /// R11-6 TEST-ONLY: read the bit for `(node_bucket, class_idx, slot_idx)`
    /// directly by bucket index (not node_id). Used by the NUMA oracle to
    /// iterate all buckets and compare incremental vs rebuild per-bucket.
    #[doc(hidden)]
    #[cfg(feature = "alloc-segment-directory")]
    #[must_use]
    pub fn dbg_directory_get_bit_bucket(
        &self,
        bucket: usize,
        class_idx: usize,
        slot_idx: usize,
    ) -> Option<bool> {
        self.directory().map(|dir| {
            let word = slot_idx / 64;
            let bit_mask = 1u64 << (slot_idx % 64);
            dir.class_nonempty_by_node[bucket][class_idx][word] & bit_mask != 0
        })
    }

    /// R11-6 TEST-ONLY: directly invoke `find_segment_with_free(class_idx)`,
    /// BYPASSING `alloc_small`'s step-1 `pop_free(small_cur)` fast path. This
    /// forces the directory-driven lookup (or the linear scan fallback) to be
    /// the deciding factor, without depending on incidental `small_cur` state.
    ///
    /// Returns `Some(base)` (a segment-base pointer whose `BinTable[class_idx]`
    /// is non-empty) or `None` (no segment has a free block for this class).
    ///
    /// Used by `tests/segment_directory_numa.rs`'s local-first /
    /// foreign-fallback test to exercise the directory's node-bucket scan order
    /// in isolation, rather than depending on which segment `small_cur` points
    /// at after a mixed-node workload (which could make the decisive alloc
    /// resolve via `pop_free(small_cur)` before the directory is ever
    /// consulted — making the test vacuous with respect to the bucket order).
    ///
    /// `#[doc(hidden)] pub` per the established test-only-export pattern
    /// (CLAUDE.md "File and module structure" sanctioned exception 1). Not
    /// stable public API.
    #[doc(hidden)]
    pub fn dbg_find_segment_with_free(&mut self, class_idx: usize) -> Option<*mut u8> {
        self.find_segment_with_free(class_idx)
    }

    /// R8-2 (task #215) TEST-ONLY: directly clear a single bit in the
    /// materialised directory sidecar, BYPASSING the normal invariant (which
    /// only clears a bit in response to a real non-empty→empty transition).
    /// This is a test tool to SIMULATE directory drift — manufacture a
    /// directory bit that is stale-clear while the underlying `BinTable` still
    /// has a free block — so the R8-2 self-heal path (periodic re-validation
    /// full scan finds a segment the directory missed) can be exercised by a
    /// test. No real production code path should ever call this: every other
    /// call site of `publish_empty` is gated on an actual BinTable head
    /// transition to `FREE_LIST_NULL`. Returns `true` if the directory was
    /// materialised (and the bit cleared), `false` otherwise.
    #[doc(hidden)]
    pub fn dbg_directory_force_clear_bit(&mut self, class_idx: usize, slot_idx: usize) -> bool {
        #[cfg(feature = "alloc-segment-directory")]
        {
            if self.directory_sidecar.is_null() {
                return false;
            }
            // R11-6: derive the base from the table so publish_empty routes
            // to the correct node bucket (or clears all buckets if null).
            let base = self.table.base_at(slot_idx);
            self.publish_empty(base, class_idx, slot_idx);
            true
        }
        #[cfg(not(feature = "alloc-segment-directory"))]
        {
            let _ = (class_idx, slot_idx);
            false
        }
    }

    /// R9-8 (task #230) TEST-ONLY: invoke the OOM-rescue scan directly for
    /// `class_idx` — a faithful mirror of what `alloc_small`'s step-4 `None`
    /// branch (and the magazine-refill equivalent) does right before surfacing
    /// an OOM to the user. Runs ONE forced O(S) linear scan that bypasses the
    /// R8-2 directory-trust fast path; if it finds a real free block the
    /// directory had hidden, self-heals the bit (inside the scan) and bumps
    /// `DIRECTORY_RESCUE_OOM_AVOIDED`, returning the segment base. Returns
    /// `None` if nothing was found (genuine OOM) or the directory is off /
    /// not materialised / `numa-aware`.
    ///
    /// This exists because reaching a real OOM (`MAX_SEGMENTS` table full or OS
    /// reservation failure) is impractical in a unit test (would require
    /// ~1024 live 4 MiB segments). The hook lets a test exercise the EXACT
    /// rescue code path the production OOM branches call, against a
    /// manufactured directory drift, without driving the table to capacity.
    #[doc(hidden)]
    pub fn dbg_directory_rescue_scan(&mut self, class_idx: usize) -> Option<*mut u8> {
        #[cfg(all(feature = "alloc-segment-directory", not(feature = "numa-aware")))]
        {
            if self.directory_sidecar.is_null() {
                return None;
            }
            let seg = self.find_segment_with_free_forced(class_idx);
            if seg.is_some() {
                #[cfg(feature = "alloc-stats")]
                crate::alloc_core::directory_stats::DIRECTORY_RESCUE_OOM_AVOIDED
                    .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            }
            seg
        }
        #[cfg(not(all(feature = "alloc-segment-directory", not(feature = "numa-aware"))))]
        {
            let _ = class_idx;
            None
        }
    }

    /// R7-A1: the materialisation threshold constant (test introspection).
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_materialize_threshold() -> u32 {
        #[cfg(feature = "alloc-segment-directory")]
        {
            crate::alloc_core::segment_directory::DIRECTORY_MATERIALIZE_THRESHOLD
        }
        #[cfg(not(feature = "alloc-segment-directory"))]
        {
            0
        }
    }

    /// TEST-ONLY (`docs/CORRECTNESS_OPEN_ITEMS.md` item 143): the
    /// process-wide count of OS segment reservations the KERNEL REFUSED —
    /// see [`os::SEGMENTS_RESERVE_FAILED_TOTAL`](crate::alloc_core::os::SEGMENTS_RESERVE_FAILED_TOTAL)
    /// for the full rationale.
    ///
    /// A capacity test fills until `alloc` returns null, but null is
    /// ambiguous: it means EITHER the `SegmentTable` ran out of slots (the
    /// ceiling such a test is actually asserting) OR the OS declined to back
    /// another mapping because the machine is under memory pressure — a
    /// system-wide condition no test can control. Reading this counter's
    /// DELTA across the fill loop disambiguates the two: zero means every
    /// null came from the allocator's own bookkeeping, so the count is a
    /// valid assertion; non-zero means the environment cut the run short and
    /// the count says nothing about the ceiling.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_segments_reserve_failed_total() -> u64 {
        crate::alloc_core::os::SEGMENTS_RESERVE_FAILED_TOTAL
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R15-1 (task #303) TEST-ONLY: `segment_directory::WORDS_PER_CLASS`
    /// (`= MAX_SEGMENTS / 64`) — the per-class word count of the
    /// `class-aware-dirty` sidecar (`PerClassDirty`) and, by construction,
    /// also the word count of the coarse per-segment `dirty_segments`
    /// bitmap (`registry::heap_slot::DIRTY_BITMAP_WORDS` mirrors this same
    /// formula independently — see that constant's doc comment). Added so
    /// measurement harnesses (`examples/r13_9_class_aware_dirty_sidecar_rss.rs`)
    /// can report the REAL sidecar footprint instead of a hardcoded literal
    /// that silently goes stale whenever `MAX_SEGMENTS` changes (exactly
    /// what happened across the R14-7 raise: the example's own comment and
    /// printed line hardcoded `16`, the pre-raise value, and did not notice
    /// the post-raise value is `64`).
    #[cfg(feature = "alloc-segment-directory")]
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_words_per_class() -> usize {
        crate::alloc_core::segment_directory::WORDS_PER_CLASS
    }

    /// R8-2 (task #215) / R9-8 (task #230) TEST-ONLY: reset the per-instance
    /// `directory_miss_streak` counters to 0 for ALL classes. The streak is
    /// internal optimisation state (now PER-CLASS since R9-8) that
    /// `push_past_threshold` (and any other alloc sequence) leaves in an
    /// unknown residual value; tests that need to assert on the periodic
    /// re-validation boundary behaviour call this to put the streaks in a known
    /// state before driving misses. No production code path touches the streak
    /// outside `find_segment_with_free_impl`'s directory-miss branch.
    #[doc(hidden)]
    pub fn dbg_directory_reset_miss_streak(&mut self) {
        #[cfg(feature = "alloc-segment-directory")]
        {
            for c in self.directory_miss_streak.iter_mut() {
                *c = 0;
            }
        }
    }

    /// R9-8 (task #230) TEST-ONLY: read the per-instance `directory_miss_streak`
    /// counter for a SINGLE class — lets a test assert a specific class's streak
    /// value directly (the per-class decoupling proof checks that one class's
    /// misses do NOT advance another class's streak).
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_miss_streak_for_class(&self, class_idx: usize) -> u32 {
        #[cfg(feature = "alloc-segment-directory")]
        {
            self.directory_miss_streak
                .get(class_idx)
                .copied()
                .map_or(0, |v| v as u32)
        }
        #[cfg(not(feature = "alloc-segment-directory"))]
        {
            let _ = class_idx;
            0
        }
    }

    /// R9-8 (task #230) TEST-ONLY: SET the per-instance `directory_miss_streak`
    /// counter for a SINGLE class to `value`. Lets a test place a class's streak
    /// at an arbitrary point (e.g. `period - 1`) WITHOUT driving polluting
    /// carves — used by the periodic-self-heal test to position class_x one miss
    /// shy of the re-validation boundary. No production code path sets the
    /// streak outside `find_segment_with_free_impl`'s directory-miss branch.
    #[doc(hidden)]
    pub fn dbg_directory_set_miss_streak_for_class(&mut self, class_idx: usize, value: u8) {
        #[cfg(feature = "alloc-segment-directory")]
        {
            if let Some(c) = self.directory_miss_streak.get_mut(class_idx) {
                *c = value;
            }
        }
        #[cfg(not(feature = "alloc-segment-directory"))]
        {
            let _ = (class_idx, value);
        }
    }

    /// R8-2 (task #215): the periodic re-validation full-scan period constant
    /// (test introspection) — the streak length after which a genuine
    /// directory miss runs the full linear scan as a re-validation pass.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_miss_full_scan_period() -> u32 {
        #[cfg(feature = "alloc-segment-directory")]
        {
            crate::alloc_core::segment_directory::DIRECTORY_MISS_FULL_SCAN_PERIOD
        }
        #[cfg(not(feature = "alloc-segment-directory"))]
        {
            0
        }
    }

    /// R7-A1 TEST-ONLY: re-run the full rebuild of the directory sidecar
    /// from the current `SegmentTable` state. Returns `true` if the
    /// directory was materialised (and thus rebuilt), `false` if it has not
    /// been materialised yet. This lets a test: (a) allocate enough to
    /// cross the threshold (directory materialised with all-zero BinTable
    /// heads), (b) free some blocks (creating non-empty BinTable entries),
    /// (c) call this to rebuild, (d) verify the bits match.
    #[doc(hidden)]
    #[allow(unsafe_code)] // R14-9 (task #294): calls the `unsafe fn sidecar::deref_mut`
                          // boundary right after `ptr` is proven non-null (produced
                          // only by `reserve_directory_sidecar`, fully initialised).
                          // `AllocCore`'s owner-only discipline (neither `Send` nor
                          // `Sync`) rules out a concurrent writer, and no other
                          // reference to the sidecar is live across this call.
    pub fn dbg_rebuild_directory(&mut self) -> bool {
        #[cfg(feature = "alloc-segment-directory")]
        {
            let ptr = self.directory_sidecar;
            if ptr.is_null() {
                return false;
            }
            // Obtain a `&mut SegmentDirectory` from the raw pointer — the
            // same dereference `directory_mut()` does, but done BEFORE
            // borrowing `self.table` so the borrow checker sees two
            // disjoint borrows (the sidecar memory is heap-allocated, not
            // a field of `self`).
            //
            // SAFETY: `ptr` is non-null (checked above) and was produced by
            // `reserve_directory_sidecar`, so it points at a value fully
            // initialised by that constructor. `AllocCore`'s owner-only
            // discipline rules out a concurrent writer, and no other
            // reference to this sidecar is live across this call.
            let dir = unsafe { crate::alloc_core::sidecar::deref_mut(ptr, &*self) };
            // Zero out all bits first, then rebuild from scratch.
            // R11-6: iterate all node buckets.
            for nb in 0..crate::alloc_core::segment_directory::NODE_BITMAPS {
                for c in 0..crate::alloc_core::size_classes::SMALL_CLASS_COUNT {
                    for w in 0..crate::alloc_core::segment_directory::WORDS_PER_CLASS {
                        dir.class_nonempty_by_node[nb][c][w] = 0;
                    }
                }
            }
            // R13-2 (task #272): the bit storage above was zeroed by a RAW
            // field write (not `clear_bit`), so the per-bucket active-bit
            // counters `set_bit` maintains below (via `rebuild_from_table`)
            // must be zeroed in lock-step here — otherwise `set_bit`'s
            // "was this bit previously 0" check stays correct (the bits
            // really are 0), but the stale non-zero counts left over from
            // before this reset would make an otherwise-correct re-derivation
            // under-report emptiness, or in the worst case never reach 0 and
            // so never free a bucket that IS now fully idle. `node_ids`
            // itself is deliberately NOT reset (see the comment below) — only
            // the derived counters that track occupancy of the bits, not the
            // node->bucket identity mapping.
            #[cfg(feature = "numa-aware")]
            {
                dir.active_bits_by_node = [0; crate::alloc_core::segment_directory::MAX_NODES];
            }
            // R12-2: deliberately do NOT reset `node_ids` here. The dense
            // node-id -> bucket mapping is established ONCE, at first
            // materialisation (`maybe_materialize_directory`'s call to
            // `reserve_directory_sidecar`, whose `sidecar::reserve_zeroed_with`
            // fixup runs `init_node_ids_raw`, followed by `rebuild_from_table`),
            // and is APPEND-ONLY from then on (new nodes may still claim free
            // slots via
            // `node_bucket_mut`, but existing claims never move) — exactly
            // matching the incremental `set_bit`/`clear_bit` path's
            // discipline. Resetting `node_ids` here and re-deriving
            // "first-seen in TABLE-SLOT order" would silently pick a
            // DIFFERENT node->bucket assignment than "first-seen in
            // REAL-TIME class-transition order" (segment N being created
            // before segment M does not imply N's class transitions
            // empty->non-empty before M's — a segment fully consumed by the
            // time of materialisation contributes NO bits and registers NO
            // bucket until it is later freed into). Resetting here broke the
            // §7.3 item 1 per-bucket oracle
            // (`segment_directory_numa::per_node_oracle_holds_after_mixed_node_workload`):
            // it compared this test-only "rebuild from scratch" against
            // live incremental state bucket-for-bucket, and a reassigned
            // mapping made an otherwise-correct bit appear in the "wrong"
            // bucket. Preserving `node_ids` across rebuilds keeps bucket
            // identity stable, so only the BITS are re-derived (which is
            // the whole point of a self-healing rebuild) — matching what
            // the production self-heal call sites
            // (`publish_empty`/`sync_directory_for_segment_classes`) already
            // do (they never touch `node_ids` either).
            dir.rebuild_from_table(&self.table);
            true
        }
        #[cfg(not(feature = "alloc-segment-directory"))]
        {
            false
        }
    }

    /// R13-1 (task #271) TEST-ONLY: force-trip this heap's coarse-only latch
    /// (`registry::heap_slot::HeapSlotRemote::sidecar_oom_latch`) WITHOUT
    /// actually driving the process to OOM. Reaching a genuine
    /// `ensure_per_class_dirty` OOM in a test would require exhausting
    /// virtual memory (impractical and non-deterministic — the same
    /// rationale [`dbg_directory_rescue_scan`](Self::dbg_directory_rescue_scan)
    /// documents for its own OOM-adjacent scenario). This hook stores `true`
    /// directly into the SAME `&'static AtomicBool` handle the real
    /// `apply_resolved_dirty_bit` (G1 apply phase) sidecar-OOM branch writes (`Release`, matching the
    /// production write's ordering), letting a test deterministically
    /// reconstruct "a producer already observed sidecar OOM at least once for
    /// this heap" and then assert `drain_dirty_segments`'s consumer-side
    /// behaviour after that point. Returns `true` if the latch handle was
    /// bound (i.e. `class-aware-dirty` is on and this `AllocCore` has been
    /// claimed through the registry), `false` otherwise (no-op).
    #[doc(hidden)]
    pub fn dbg_force_sidecar_oom_latch(&mut self) -> bool {
        #[cfg(feature = "class-aware-dirty")]
        {
            match self.sidecar_oom_latch {
                Some(latch) => {
                    latch.store(true, core::sync::atomic::Ordering::Release);
                    true
                }
                None => false,
            }
        }
        #[cfg(not(feature = "class-aware-dirty"))]
        {
            false
        }
    }

    /// R13-1 (task #271) TEST-ONLY: read this heap's coarse-only latch
    /// (`registry::heap_slot::HeapSlotRemote::sidecar_oom_latch`). Returns
    /// `None` if the latch handle is not bound (`class-aware-dirty` off, or
    /// this `AllocCore` was never claimed through the registry), `Some(bool)`
    /// otherwise. `Acquire` load — matches production
    /// `drain_dirty_segments`'s own read exactly (R14-2, task #287: that
    /// read was promoted from `Relaxed` to `Acquire` this task, closing a
    /// divergence three independent Round 13 reviews found against this
    /// field's doc comment and the loom model, both of which had always
    /// documented/used `Acquire`).
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_sidecar_oom_latch(&self) -> Option<bool> {
        #[cfg(feature = "class-aware-dirty")]
        {
            self.sidecar_oom_latch
                .map(|latch| latch.load(core::sync::atomic::Ordering::Acquire))
        }
        #[cfg(not(feature = "class-aware-dirty"))]
        {
            None
        }
    }
}
