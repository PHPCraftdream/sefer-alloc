//! Diagnostics / test-only hooks for [`HeapCore`] (mechanical split of
//! `heap_core.rs`, task R4-10).
//!
//! This file holds the `impl HeapCore { .. }` block for the inspection
//! test hooks (`dbg_owner_id_for`, `dbg_tcache_count`, etc.).
//! Pure code-movement sibling of `heap_core.rs`; no behavior changed.

use core::alloc::Layout;

use crate::alloc_core::os;
use crate::alloc_core::segment_header::SegmentMeta;
use core::sync::atomic::Ordering;

use super::heap_core::HeapCore;

impl HeapCore {
    /// TEST-ONLY (P4): read the `owner_id` stamped in the segment header of
    /// the segment that contains `ptr`. Returns `None` if `ptr` is not in a
    /// segment owned by this heap's substrate. Used by
    /// `tests/heap_core_tcache_stamp.rs` to verify the stamp-hoist wrote
    /// the correct ownership.
    #[doc(hidden)]
    #[cfg(feature = "alloc-global")]
    pub fn dbg_owner_id_for(&self, ptr: *mut u8) -> Option<u32> {
        use crate::alloc_core::segment_header::{unpack_owner_id, SegmentMeta};
        let base = os::segment_base_of_ptr(ptr);
        if !self.core.segment_bases().any(|b| b == base) {
            return None;
        }
        let owner_atomic = SegmentMeta::new(base).owner_state_atomic();
        let word = owner_atomic.load(Ordering::Relaxed);
        Some(unpack_owner_id(word))
    }

    /// TEST-ONLY (P4): the cached `last_stamped_segment` base, or null if
    /// no segment has been stamped yet. Allows tests to observe whether the
    /// stamp-cache was updated without re-stamping.
    #[doc(hidden)]
    #[cfg(feature = "alloc-global")]
    pub fn dbg_last_stamped_segment(&self) -> *mut u8 {
        self.last_stamped_segment
    }

    /// TEST-ONLY (R12-6): the decoded `SegmentKind` of `ptr`'s segment, as a
    /// small tag (`0` = Primordial, `1` = Small, `2` = Large, `3` = Unknown)
    /// — thin delegation to [`AllocCore::dbg_kind_at_tag`]. Exposed at the
    /// `HeapCore` level (mirroring `dbg_owner_id_for`/`dbg_live_count_for`'s
    /// existing delegation pattern in this file) so a `tests/` integration
    /// test can distinguish the PRIMORDIAL segment from an ordinary `Small`
    /// segment without reaching into crate-internal modules — needed because
    /// the primordial segment is never pool/release-eligible (see
    /// `AllocCore::dec_live_and_maybe_decommit`'s own Primordial exclusion),
    /// so a test constructing many distinct `Small` target segments must be
    /// able to positively exclude it, not merely infer it from allocation
    /// order.
    #[doc(hidden)]
    #[cfg(feature = "alloc-global")]
    #[must_use]
    pub fn dbg_kind_at_tag(&self, ptr: *mut u8) -> u8 {
        self.core.dbg_kind_at_tag(ptr)
    }

    /// TEST-ONLY (P7): read the magazine count for class `c`. Widened to
    /// `u16` at this test-only boundary (task #53 shrank the internal
    /// storage to `u8` — see `PerClass::count` — but keeps this accessor's
    /// return type stable for existing callers).
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub fn dbg_tcache_count(&self, c: usize) -> u16 {
        self.tcache.classes[c].count as u16
    }

    /// TEST-ONLY (R26-5, task #414): `true` iff `ptr` is currently sitting in
    /// one of this heap's magazine slots for class `c` — i.e.
    /// `self.tcache.classes[c].slots[i] == ptr` for some
    /// `i < classes[c].count`. This is the exact "magazine-resident"
    /// predicate the `dealloc_batch` first-warm policy produces for the first
    /// `TCACHE_CAP` accepted blocks (see `heap_core_dealloc_batch.rs`'s doc).
    /// Exposed so a `tests/` integration test can verify, per individual
    /// block, whether a block freed via `dealloc_batch` ended up
    /// magazine-resident (still live per the D1 invariant, poppable on the
    /// next same-class `alloc`) versus genuinely flushed to the free list —
    /// distinguishing "184 blocks correctly freed" from a cancelling pair
    /// (one leaked + one double-processed) that nets to the same aggregate
    /// `live_count` delta but corrupts two individual blocks' states.
    /// Read-only linear scan of `slots[0..count]` (`count <= TCACHE_CAP ==
    /// 16`, so bounded); does NOT touch allocator metadata, so it is a plain
    /// safe `fn` (same category as `dbg_tcache_count`), NOT `unsafe`.
    /// `bench-internals`-gated (R27-7/task #425: no production caller → R25-10
    /// sub-rule 2).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "bench-internals"
    ))]
    #[must_use]
    pub fn dbg_tcache_contains(&self, c: usize, ptr: *mut u8) -> bool {
        let cls = &self.tcache.classes[c];
        cls.slots[..cls.count as usize].contains(&ptr)
    }

    /// TEST-ONLY (R13-3, task #273): read the raw
    /// [`PerClass::virgin_mask`](super::tcache::PerClass) bitmask for class
    /// `c` — bit `i` set ⟺ `slots[i]` (for `i < dbg_tcache_count(c)`) is
    /// currently tracked as a genuinely virgin (never-before-served, zero-skippable)
    /// magazine-resident block. Lets a test directly observe WHICH resident
    /// slots the R13-3 magazine-plumbing marked virgin, instead of assuming
    /// "every retained block from a miss-triggering refill is virgin" — that
    /// assumption is FALSE whenever the free-drain-first policy
    /// (`refill_class_bump_impl`'s non-negotiable source order) reclaims
    /// existing free blocks ahead of a bump-carve within the same refill
    /// (see `tests/r13_3_magazine_virgin_hit_skips_zero.rs`'s doc for the
    /// scenario that discovered this).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "virgin-zero-skip"
    ))]
    #[must_use]
    pub fn dbg_tcache_virgin_mask(&self, c: usize) -> u16 {
        self.tcache.classes[c].virgin_mask
    }

    /// TEST-ONLY (task D3): resolve the size class index for `layout`, the
    /// same classification `alloc` uses to index `tcache.classes[c].slots`/
    /// `.count`.
    /// Delegates to [`AllocCore::dbg_layout_class_for`]; exposed at the
    /// `HeapCore` level because `core` is `pub(crate)` and external
    /// integration tests only see `HeapCore`/`HeapRegistry`.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub fn dbg_class_for(&self, layout: Layout) -> Option<usize> {
        self.core.dbg_layout_class_for(layout)
    }

    /// TEST-ONLY (task D3): the per-class refill amount `alloc`'s
    /// magazine-miss path actually uses for class `c` — i.e.
    /// `super::tcache::refill_n_for_class(SizeClasses::block_size(c))`, the
    /// exact expression `alloc` evaluates. Lets a test assert the byte-budget
    /// clamp fired for a given class without duplicating (and risking
    /// drifting from) the formula.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub fn dbg_refill_n_for_class(&self, c: usize) -> usize {
        super::tcache::refill_n_for_class(crate::alloc_core::size_classes::SizeClasses::block_size(
            c,
        ))
    }

    /// TEST-ONLY (task R2/#154): push `ptr`'s segment-relative offset — packed
    /// with `class_idx` — into its segment's `RemoteFreeRing`, exactly as a
    /// cross-thread freer's `dealloc_routing` Variant-2 push would. Thin
    /// delegation to [`AllocCore::dbg_push_to_ring`]; exposed at the `HeapCore`
    /// level so the ring↔magazine residual-limit pinning test
    /// (`tests/regression_xthread_double_free_residual.rs`) can simulate a
    /// remote free while driving the magazine through `HeapCore`. Returns
    /// `false` if the ring was full or `ptr` is not one of this heap's segments.
    /// Zero production impact: `#[doc(hidden)]`, test-only, delegates to an
    /// existing hook.
    ///
    /// # Safety
    ///
    /// `unsafe fn` (R6-MS-4) for exactly the same reason as
    /// [`AllocCore::dbg_push_to_ring`]: this thin delegation is the producer
    /// side of the cross-thread free simulation, and a safe wrapper would leave
    /// the round5 `memory_safety_review` R5-MS-4 stale-note→double-issue chain
    /// open through `HeapCore` (the residual tests reach the seam through this
    /// very wrapper). The caller must honour the identical contract: `ptr` is a
    /// live block in a segment owned by this heap; this push is at most one
    /// logical remote free (no `dealloc`/`flush_class`/`alloc`-re-issue of `ptr`
    /// between this push and the consuming drain); and `class_idx` is the
    /// block's actual allocated class. See the delegated fn's `# Safety` section
    /// for the full rationale and the defensive-guard caveats.
    ///
    /// R24-6 (task #384) note: this hook's `alloc-xthread` gate is a subset of
    /// `production`'s feature list, so it IS reachable from a plain
    /// `--features production` build — deliberately NOT moved behind
    /// `bench-internals` like its two younger siblings
    /// (`dbg_dealloc_own_thread_with_base`, `dbg_push_coarse_only_entry`):
    /// this is the oldest hook in this file (R6-MS-4) and has ~20 existing
    /// callers across the whole `alloc-xthread` test suite, so re-gating it
    /// would be a disproportionate diff for a doc-precision concern. See
    /// README.md's "Where unsafe lives" R24-6 note for the full rationale.
    /// It remains, like its siblings, `#[doc(hidden)]`, test/measurement-only,
    /// and excluded from any "changes production behavior" claim.
    #[doc(hidden)]
    #[cfg(feature = "alloc-xthread")]
    #[allow(unsafe_code)] // R6-MS-4: `unsafe fn` boundary (delegation to the unsafe producer).
    pub unsafe fn dbg_push_to_ring(&self, ptr: *mut u8, class_idx: usize) -> bool {
        // SAFETY (R6-MS-4): this method carries the identical `# Safety`
        // contract as the delegated `AllocCore::dbg_push_to_ring` and is itself
        // `unsafe fn`, so the obligation is forwarded to THIS caller verbatim.
        unsafe { self.core.dbg_push_to_ring(ptr, class_idx) }
    }

    /// TEST-ONLY (task R2/#154): drain every owned segment's `RemoteFreeRing`
    /// into its `BinTable`, exactly as the alloc slow path's lazy drain does,
    /// but unconditionally. Task #164: routes through the same magazine
    /// predicate as the production drain, so tests exercise the real
    /// decision path.
    #[doc(hidden)]
    #[cfg(feature = "alloc-xthread")]
    pub fn dbg_drain_all_rings(&mut self) {
        // Task #164: split borrow — `&self.tcache` (read) + `&mut self.core`
        // (write) are disjoint fields of HeapCore.
        //
        // RAD-5 (E4) GO/NO-GO EXPERIMENT: the magazine predicate is now the
        // O(1) bitmap probe, matching the production `refill_magazine_slow`
        // predicate — see that function's identical replacement for the
        // rationale. `class_idx` is unused by the probe (residency is keyed
        // by offset, not class) but kept in the closure signature to match
        // the `dbg_drain_all_rings_checked`/`reclaim_offset_checked` `F: Fn(*mut
        // u8, usize) -> bool` contract.
        #[cfg(feature = "fastbin")]
        {
            self.core.dbg_drain_all_rings_checked(&|ptr, _class_idx| {
                let pbase = os::segment_base_of_ptr(ptr);
                let poff = (ptr as usize - pbase as usize) as u32;
                SegmentMeta::new(pbase)
                    .magazine_bitmap()
                    .is_in_magazine(poff)
            });
        }
        #[cfg(not(feature = "fastbin"))]
        self.core.dbg_drain_all_rings();
    }

    /// TEST-ONLY (Mechanism 2, task #51): force-drain this heap's
    /// empty-small-segment hysteresis pool (release + recycle every pooled
    /// segment). Forwards to `AllocCore::drain_small_pool` — the production
    /// teardown-trim primitive (see [`trim_for_recycle`](Self::trim_for_recycle)).
    /// Used by decommit tests that run through the `SeferAlloc`/`HeapRegistry` face
    /// (where `claim_with_config` cannot reliably disable the pool on a reused
    /// slot) to deterministically observe the decommit that a pooled segment
    /// would otherwise absorb. Returns the number of segments drained.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_drain_small_pool(&mut self) -> usize {
        self.core.drain_small_pool()
    }

    /// TEST-ONLY (R11-2): read a single directory bit for the segment that
    /// contains `ptr`, resolved via the segment header's `segment_id` (so the
    /// caller does not need crate-internal access to compute `slot_idx`).
    /// Returns `None` if the directory is not materialised or `ptr` is foreign
    /// (a foreign `ptr`'s segment-aligned base is not owned by this `AllocCore`,
    /// so the containment guard below returns `None` before any header read).
    /// Thin delegation to `AllocCore::dbg_directory_get_bit` — exposed at the
    /// `HeapCore` level so integration tests driving cross-thread frees through
    /// `HeapCore::dealloc` can observe whether `drain_heap_overflow` synced the
    /// directory after reclaiming an overflow entry.
    #[doc(hidden)]
    #[cfg(feature = "alloc-segment-directory")]
    #[must_use]
    pub fn dbg_directory_bit_for_ptr(&self, ptr: *mut u8, class_idx: usize) -> Option<bool> {
        use crate::alloc_core::segment_header::SegmentHeader;
        let base = os::segment_base_of_ptr(ptr);
        // R29-17 (task #448): containment guard BEFORE the segment_id_at read.
        // segment_id_at dereferences the segment header (`Node::read_u32`), so
        // a null/foreign/arbitrary `ptr` whose segment-aligned base is unmapped
        // would read unmapped memory (crash) or arbitrary mapped memory
        // (garbage sid). Mirrors the `dbg_owner_id_for` guard shape in this
        // file (return `None` on a foreign base) rather than the assert-panic
        // shape of `dbg_segment_id_of` in `alloc_core_core_diag.rs`: this fn
        // already returns `Option<bool>` and documents "ptr is foreign → None",
        // and the single existing caller passes a genuinely live block.
        if !self.core.segment_bases().any(|b| b == base) {
            return None;
        }
        let sid = SegmentHeader::segment_id_at(base) as usize;
        self.core.dbg_directory_get_bit(class_idx, sid)
    }

    /// TEST-ONLY (R11-2): the number of empty small segments currently
    /// retained in this heap's hysteresis pool. Thin delegation to
    /// `AllocCore::dbg_pooled_count` — exposed at the `HeapCore` level so
    /// integration tests can assert that a segment emptied via an overflow-ring
    /// reclaim was actually pooled (not left as an ordinary registered
    /// segment by the pre-R11-2 bug that dropped the pool/release signal).
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub fn dbg_pooled_count(&self) -> usize {
        self.core.dbg_pooled_count()
    }

    /// TEST-ONLY (R26-1, task #410): the resolved runtime pool cap for this
    /// heap — thin delegation to `AllocCore::dbg_pool_cap`, exposed at the
    /// `HeapCore` level (mirroring `dbg_pooled_count`'s existing delegation
    /// pattern in this file) so the R26-1 subprocess-per-arm RSS probe can
    /// self-verify each claimed heap actually resolved to the requested
    /// `pool_segments` config (the direct proof the R25-5 RSS-axis bug —
    /// registry-slot reuse silently keeping an earlier arm's cap — is gone).
    /// Read-only `&self` accessor returning a `usize`; does NOT touch
    /// allocator metadata through a raw pointer, so it is a plain safe `fn`
    /// (same category as `dbg_pooled_count`), NOT an `unsafe fn`.
    /// `bench-internals`-gated (R27-7/task #425: no production caller → R25-10
    /// sub-rule 2).
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[must_use]
    pub fn dbg_pool_cap(&self) -> usize {
        self.core.dbg_pool_cap()
    }

    /// R29-4 (task #435) MEASUREMENT-ONLY: thin delegation to
    /// [`AllocCore::dbg_segment_state_reconciliation`] — exposed at the
    /// `HeapCore` level (mirroring `dbg_pooled_count`'s / `dbg_pool_cap`'s
    /// existing delegation pattern in this file) so the R29-4 probe can
    /// snapshot the per-state segment accounting from a claimed heap.
    /// Returns a [`SegmentStateReconciliation`] that classifies every
    /// registered segment into exactly one state. Read-only `&self`;
    /// does NOT mutate allocator state. `bench-internals`-gated
    /// (no production caller → R25-10 sub-rule 2).
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[must_use]
    pub fn dbg_segment_state_reconciliation(
        &self,
    ) -> crate::alloc_core::SegmentStateReconciliation {
        self.core.dbg_segment_state_reconciliation()
    }

    /// R29-13 (task #444) MEASUREMENT-ONLY: thin delegation to
    /// [`AllocCore::dbg_large_cache_used`] — exposed at the `HeapCore` level
    /// (mirroring `dbg_pooled_count`'s/`dbg_pool_cap`'s existing delegation
    /// pattern in this file) so the R29-13 large-cache retention probe can
    /// read the current running sum of cached large-span bytes for a claimed
    /// heap. Read-only `&self`; does NOT mutate allocator state.
    /// `bench-internals`-gated (no production caller → R25-10 sub-rule 2).
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[must_use]
    pub fn dbg_large_cache_used(&self) -> usize {
        self.core.dbg_large_cache_used()
    }

    /// R31-3 (task #466) MEASUREMENT-ONLY: thin delegation to
    /// [`AllocCore::dbg_large_cache_budget`] — exposed at the `HeapCore`
    /// level (same delegation pattern as `dbg_large_cache_used` above) so
    /// the R31-3 multi-heap RSS gate can self-verify each claimed heap's
    /// resolved large-cache byte budget (`None` = unbounded, base cache;
    /// `Some(256 MiB)` = `large-cache-extended`'s R17-9 finite default)
    /// matches the expected value for the build under test, per the R26-4
    /// config-sweep evidence rule (read back from the allocator's own
    /// diagnostic surface, not assumed). Read-only `&self`; does NOT mutate
    /// allocator state. `bench-internals`-gated (no production caller →
    /// R25-10 sub-rule 2).
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[must_use]
    pub fn dbg_large_cache_budget(&self) -> Option<usize> {
        self.core.dbg_large_cache_budget()
    }

    /// R30-6 (task #455) MEASUREMENT-ONLY: thin delegation to
    /// [`AllocCore::dbg_large_cache_hits`] — exposed at the `HeapCore` level
    /// (same delegation pattern as `dbg_large_cache_used` immediately above)
    /// so the R30-6 large-cache headroom BENEFIT-side A/B probe can read this
    /// heap's own large-cache hit counter directly (without going through the
    /// process-wide `large_cache_hits_total` aggregator, which would mix in
    /// every other concurrently-claimed heap's hits and defeat a per-thread
    /// before/after delta). Read-only `&self`; does NOT mutate allocator
    /// state.
    ///
    /// R31-4 (task #467, closing P2-2 filed in
    /// `docs/CORRECTNESS_OPEN_ITEMS.md` item 8): gated
    /// `all(alloc-decommit, bench-internals)`, matching this SAME file's
    /// four sibling `HeapCore`-level measurement delegations
    /// (`dbg_pool_cap`, `dbg_segment_state_reconciliation`,
    /// `dbg_large_cache_used`, `dbg_large_cache_slot_sizes`) — this hook has
    /// no production caller, so CLAUDE.md's benchmark-hook rule 2 ("a hook
    /// with no production caller MUST default to `bench-internals`-gating")
    /// applies, the same rule those four siblings already cite. Previously
    /// gated `alloc-decommit` alone (justified at the time as "matching
    /// `AllocCore::dbg_large_cache_hits`'s own gate exactly") — that
    /// justification was the wrong comparison: the DELEGATED method's
    /// pre-existing gate is not the rule for a NEW `HeapCore`-level hook.
    /// This widened this hook's presence in every plain `production` build
    /// (`alloc-decommit` alone is inside `production`) until now. Both
    /// current callers (`examples/r30_6_large_cache_headroom_ab_gate.rs`,
    /// `examples/r31_1_large_cache_headroom_crossing_regime_gate.rs`) already
    /// require `bench-internals` in their `required-features`
    /// (`Cargo.toml`), so this tightening breaks neither.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[must_use]
    pub fn dbg_large_cache_hits(&self) -> u64 {
        self.core.dbg_large_cache_hits()
    }

    /// R29-13 (task #444) MEASUREMENT-ONLY: thin delegation to
    /// [`AllocCore::dbg_large_cache_slot_sizes`] — exposed at the `HeapCore`
    /// level (same pattern as `dbg_large_cache_used` above) so the R29-13
    /// probe can count how many of the 8 base large-cache slots are
    /// currently occupied for a claimed heap. Read-only `&self`; does NOT
    /// mutate allocator state. `bench-internals`-gated (no production caller
    /// → R25-10 sub-rule 2).
    ///
    /// Return-array length is `8` — the base large-cache slot count
    /// (`LARGE_CACHE_SLOTS`, `pub(super)` inside `alloc_core` and therefore
    /// not nameable from `registry`); hardcoded here rather than re-exported,
    /// matching [`AllocCore::dbg_large_cache_slot_sizes`]'s own public
    /// signature verbatim.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[must_use]
    pub fn dbg_large_cache_slot_sizes(&self) -> [Option<usize>; 8] {
        self.core.dbg_large_cache_slot_sizes()
    }

    /// R31-3 (task #466) MEASUREMENT-ONLY: thin delegation to
    /// [`AllocCore::dbg_large_cache_extended_slot_sizes`] — exposed at the
    /// `HeapCore` level (same pattern as `dbg_large_cache_slot_sizes` above)
    /// so the R31-3 multi-heap RSS gate can count how many of the 32
    /// extension-sidecar slots are currently occupied for a claimed heap.
    /// Read-only `&self`; does NOT mutate allocator state.
    /// `bench-internals`-gated (no production caller → R25-10 sub-rule 2),
    /// additionally gated on `large-cache-extended` (matching the delegated
    /// `AllocCore` method's own gate — the sidecar does not exist otherwise).
    ///
    /// Return-array length is `32` — `LARGE_CACHE_EXTENDED_SLOTS`,
    /// `pub(crate)` inside `alloc_core` and therefore not nameable from
    /// `registry`; hardcoded here rather than re-exported, matching
    /// [`AllocCore::dbg_large_cache_extended_slot_sizes`]'s own public
    /// signature verbatim.
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "large-cache-extended"
    ))]
    #[must_use]
    pub fn dbg_large_cache_extended_slot_sizes(&self) -> [Option<usize>; 32] {
        self.core.dbg_large_cache_extended_slot_sizes()
    }

    /// R31-3 (task #466) MEASUREMENT-ONLY: thin delegation to
    /// [`AllocCore::dbg_large_cache_extension_materialised`] — exposed at the
    /// `HeapCore` level (same pattern as `dbg_large_cache_slot_sizes` above)
    /// so the R31-3 multi-heap RSS gate can confirm, per claimed heap,
    /// whether the extension sidecar actually materialised (the
    /// mechanism-activation proof this gate's workload — 16 distinct Large
    /// sizes, overflowing the base 8 — is meant to trigger). Read-only
    /// `&self`; does NOT mutate allocator state. `bench-internals`-gated (no
    /// production caller → R25-10 sub-rule 2), additionally gated on
    /// `large-cache-extended` (matching the delegated `AllocCore` method's
    /// own gate).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "large-cache-extended"
    ))]
    #[must_use]
    pub fn dbg_large_cache_extension_materialised(&self) -> bool {
        self.core.dbg_large_cache_extension_materialised()
    }

    /// R29-13 (task #444) MEASUREMENT-ONLY: thin delegation to
    /// [`AllocCore::dbg_decay_config`] — exposed at the `HeapCore` level (same
    /// pattern as `dbg_large_cache_used` above) so the R29-13 probe can read
    /// back the RESOLVED `(decay_rate_bp, decay_interval_ms, headroom_bytes)`
    /// large-cache decay config for a claimed heap — the self-verification
    /// proof that a `LargeCacheConfig::new().headroom_bytes(n)` construction
    /// actually resolved to `n`, not assumed (per the config-sweep evidence
    /// rule, CLAUDE.md's R26-4 entry). Read-only `&self`; does NOT mutate
    /// allocator state. `bench-internals`-gated (no production caller →
    /// R25-10 sub-rule 2).
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[must_use]
    pub fn dbg_decay_config(&self) -> (u32, u64, usize) {
        self.core.dbg_decay_config()
    }

    /// R29-13 (task #444) MEASUREMENT-ONLY: thin delegation to
    /// [`AllocCore::dbg_force_decay_tick`] — exposed at the `HeapCore` level
    /// (same pattern as `dbg_large_cache_used` above) so the R29-13 probe can
    /// force a large-cache decay tick (bypassing the wall-clock interval)
    /// without waiting, to demonstrate the retained cache is reclaimable via
    /// repeated forced ticks even though pure idle never reclaims it.
    /// `&mut self` (mutates decay-tick bookkeeping and, when a tick fires,
    /// evicts cached spans). `bench-internals`-gated (no production caller →
    /// R25-10 sub-rule 2).
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_force_decay_tick(&mut self) {
        self.core.dbg_force_decay_tick();
    }

    /// TEST-ONLY (R11-2): resolve the base address of the segment that
    /// contains `ptr`. Thin delegation to `alloc_core::os::segment_base_of_ptr`
    /// — exposed at the `HeapCore` level because `alloc_core::os` is
    /// `pub(crate)` and integration tests in `tests/` only see the crate's
    /// true `pub` surface. Lets a test verify two pointers share a segment
    /// (a same-segment sanity check on the test's own construction) without
    /// reaching into crate-internal modules.
    #[doc(hidden)]
    #[cfg(feature = "alloc-global")]
    #[must_use]
    pub fn dbg_segment_base_of_ptr(&self, ptr: *mut u8) -> *mut u8 {
        os::segment_base_of_ptr(ptr)
    }

    /// TEST-ONLY (R11-2): the owner-only `live_count` of `ptr`'s segment, or
    /// `None` if `ptr` is foreign / not small/primordial. Thin delegation to
    /// `AllocCore::dbg_live_count_for` — exposed at the `HeapCore` level so an
    /// integration test can drive a segment down to EXACTLY zero live blocks
    /// (reading the exact remaining count at each step) without guessing how
    /// many blocks a given segment holds.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub fn dbg_live_count_for(&self, ptr: *mut u8) -> Option<u32> {
        self.core.dbg_live_count_for(ptr)
    }

    /// TEST-ONLY (R26-5, task #414): whether `ptr`'s block is currently marked
    /// FREE (on a free list) in its segment's alloc bitmap — the M2
    /// double-free bit. `true` ⟺ the block's storage is on a free list and
    /// available for reuse; `false` ⟺ the block is currently ALLOCATED
    /// (handed out, or magazine-resident — a magazine-resident block counts
    /// as live/allocated from the bitmap's perspective, since the magazine
    /// push does not call `dec_live`). Thin delegation to
    /// [`AllocCore::dbg_is_free_for`] — exposed at the `HeapCore` level
    /// (mirroring `dbg_live_count_for`'s / `dbg_kind_at_tag`'s existing
    /// delegation pattern in this file) so a `tests/` integration test can
    /// inspect the FINAL per-block allocation state of individual blocks
    /// after a batched `dealloc_batch`, not just the aggregate `live_count`
    /// delta. Read-only `&self` accessor returning a `bool`; reads one
    /// segment-bitmap bit for a pointer whose segment base is validated by
    /// the delegated `contains_base_ro` check, so it is a plain safe `fn`
    /// (same category as `dbg_live_count_for` / `dbg_kind_at_tag`), NOT an
    /// `unsafe fn`.
    /// `bench-internals`-gated (R27-7/task #425: no production caller → R25-10
    /// sub-rule 2).
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "bench-internals"))]
    #[must_use]
    pub fn dbg_is_free_for(&self, ptr: *mut u8) -> bool {
        self.core.dbg_is_free_for(ptr)
    }

    /// TEST-ONLY (R13-1, task #271): force-trip this heap's coarse-only
    /// latch, simulating "a producer already observed sidecar OOM at least
    /// once for this heap" WITHOUT actually driving the process to OOM. Thin
    /// delegation to `AllocCore::dbg_force_sidecar_oom_latch` — exposed at
    /// the `HeapCore` level because `core` is `pub(crate)` and integration
    /// tests in `tests/` only see `HeapCore`/`HeapRegistry`. Returns `true`
    /// if the latch handle was bound (this heap has been claimed through the
    /// registry and `class-aware-dirty` is on), `false` otherwise.
    #[doc(hidden)]
    #[cfg(feature = "class-aware-dirty")]
    pub fn dbg_force_sidecar_oom_latch(&mut self) -> bool {
        self.core.dbg_force_sidecar_oom_latch()
    }

    /// TEST-ONLY (R13-1, task #271): read this heap's coarse-only latch.
    /// Thin delegation to `AllocCore::dbg_sidecar_oom_latch` — see that
    /// function's doc comment. Returns `None` if the latch handle is not
    /// bound.
    #[doc(hidden)]
    #[cfg(feature = "class-aware-dirty")]
    #[must_use]
    pub fn dbg_sidecar_oom_latch(&self) -> Option<bool> {
        self.core.dbg_sidecar_oom_latch()
    }

    /// TEST-ONLY (R13-1, task #271): push `ptr`'s segment-relative offset
    /// into its segment's `RemoteFreeRing` (exactly like
    /// [`dbg_push_to_ring`](Self::dbg_push_to_ring)) AND set ONLY the coarse
    /// per-segment dirty bit for that segment, deliberately WITHOUT setting
    /// any per-class bit — reconstructing the exact on-heap state a real
    /// sidecar-OOM cross-thread free leaves behind, without needing a real
    /// OOM. Thin delegation to `AllocCore::dbg_push_to_ring` +
    /// `AllocCore::dbg_force_coarse_dirty_bit_for` — exposed at the
    /// `HeapCore` level so a `tests/` integration test can construct a
    /// "coarse-only entry" scenario through the real production ring/bitmap
    /// machinery. Returns `true` iff BOTH the ring push and the coarse-bit
    /// set succeeded.
    ///
    /// # Safety
    ///
    /// Carries the identical contract as
    /// [`dbg_push_to_ring`](Self::dbg_push_to_ring) (this is that same
    /// producer operation, plus an additional bitmap-only side effect that
    /// carries no extra memory-safety obligation of its own — the coarse bit
    /// is read-only metadata to `drain_dirty_segments`, never dereferenced as
    /// a pointer).
    // R24-6 (task #384): gated additionally on `bench-internals` so this
    // `unsafe fn` measurement/test-only hook is NOT reachable from plain
    // `--features production` (its prior gate — `alloc-xthread` +
    // `alloc-segment-directory` + `class-aware-dirty` — is fully satisfied by
    // `production`'s feature list on its own). Its one caller,
    // `tests/class_aware_dirty_oom_latch.rs`, now requires `bench-internals`
    // too. See the `bench-internals` feature doc in `Cargo.toml` for the full
    // rationale and why the sibling `dbg_push_to_ring` (R6-MS-4) was NOT
    // moved here.
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-xthread",
        feature = "alloc-segment-directory",
        feature = "class-aware-dirty",
        feature = "bench-internals"
    ))]
    #[allow(unsafe_code)] // R13-1: `unsafe fn` boundary, mirrors `dbg_push_to_ring`.
    pub unsafe fn dbg_push_coarse_only_entry(&self, ptr: *mut u8, class_idx: usize) -> bool {
        // SAFETY: identical contract to `dbg_push_to_ring`, forwarded to
        // THIS caller verbatim.
        let pushed = unsafe { self.core.dbg_push_to_ring(ptr, class_idx) };
        let coarse = self.core.dbg_force_coarse_dirty_bit_for(ptr);
        pushed && coarse
    }

    /// R16-5 (task #315) TEST-ONLY: `true` iff `HeapCore::realloc`'s
    /// Small/medium->Large promotion mechanism (`try_promote_to_large`,
    /// `heap_core_free.rs`) is compiled into THIS build — i.e. the exact
    /// `#[cfg]` predicate gating both `try_promote_to_large` and its call
    /// site (R15-3, task #305) evaluates `true`.
    ///
    /// Exists so `tests/r14_4_promotion_move_leg_reduction.rs`'s own
    /// hand-mirrored `HAS_PROMOTION` constant (which duplicates this exact
    /// predicate as a manually-kept-in-sync `cfg!`-derived `const bool`,
    /// since the real predicate is private to `src/`) can assert equality
    /// against the REAL compiled-in state at test run time, rather than
    /// silently drifting out of sync if a future change to the `src/`-side
    /// predicate is not mirrored into the test file — the review finding
    /// (P3-2, Round 15) this closes.
    #[doc(hidden)]
    #[must_use]
    pub const fn dbg_promotion_compiled() -> bool {
        cfg!(all(
            feature = "medium-classes",
            any(
                not(feature = "exact-span-large"),
                all(
                    feature = "large-reserved-capacity",
                    not(feature = "numa-aware")
                )
            )
        ))
    }

    /// TEST/DIAGNOSTIC (R22-12, task #363): process-wide count of `hardened`
    /// defensive no-ops fired on a Large-kind own-thread `dealloc` — see
    /// [`HARDENED_LARGE_NOOP_COUNT`](super::heap_core_free::HARDENED_LARGE_NOOP_COUNT)'s
    /// doc comment for exactly which two branches (`heap_core_free.rs`'s
    /// branch (A) mismatch case and branch (B)) share this one counter and
    /// why. Relaxed load — diagnostic only. Reads 0 unless `alloc-stats` is
    /// on (the increment sites are gated); the accessor itself is always
    /// compiled (gated on `alloc-core`, same as the static) so callers need
    /// no `#[cfg]`.
    #[doc(hidden)]
    #[cfg(feature = "alloc-core")]
    #[must_use]
    pub fn dbg_hardened_large_noop_count() -> u64 {
        super::heap_core_free::HARDENED_LARGE_NOOP_COUNT.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// MEASUREMENT-ONLY (R22-17, task #368): thin delegation to
    /// `AllocCore::contains_base` (`self.core.table.contains_base`'s
    /// `pub(crate)` wrapper), exposed at the `HeapCore` level so
    /// `benches/perf_gate_iai.rs` can measure the OWN-THREAD segment-ownership
    /// probe's instruction cost IN ISOLATION — i.e. without the surrounding
    /// alloc/dealloc bookkeeping that a full `HeapCore::dealloc` call would mix
    /// in. `base` must already be a segment-aligned base (the same value
    /// `os::segment_base_of_ptr` would produce); this hook does not compute
    /// it, so a caller measuring the FULL cost of the check (base computation
    /// + probe) should time `dbg_segment_base_of_ptr` and this call together.
    ///
    /// Exactly mirrors what `HeapCore::dealloc_routing`
    /// (`heap_core_xthread.rs`) itself calls (`self.core.contains_base(base)`)
    /// — same function, same cache-then-hash-probe behavior, same cost. This
    /// is NOT an alternate/bypass implementation of the check; it is the
    /// production check itself, exposed read-only for isolated timing. No
    /// production call site is changed by adding this hook.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
    pub fn dbg_contains_base(&mut self, base: *mut u8) -> bool {
        self.core.contains_base(base)
    }

    /// MEASUREMENT-ONLY (R23-3, task #372): thin delegation to
    /// `AllocCore::dbg_hash_contains_only`, exposed at the `HeapCore` level so
    /// `benches/perf_gate_iai.rs` can measure Tier-2's (the 8192-slot
    /// open-addressing probe) instruction cost IN ISOLATION, unconditionally
    /// skipping the Tier-1 4-entry `own_cache` check that `dbg_contains_base`
    /// above (mirroring the real `dealloc_routing` call) always tries first.
    ///
    /// **Why this hook exists instead of just constructing a >4-segment
    /// workload for the existing `dbg_contains_base` hook:** `own_cache`'s
    /// hit/miss behaviour is keyed by `(base >> SEGMENT_SHIFT) & 3` — a
    /// function of the segment's OS-assigned virtual address, which
    /// `mmap`/`VirtualAlloc` chooses, not this allocator. A workload that
    /// allocates 5+ distinct segments does not portably guarantee a Tier-2
    /// hit: the OS could still lay the segments out so their cache indices
    /// never collide inside one deterministic-iai run. This hook sidesteps
    /// that non-determinism by calling the Tier-2 probe directly — see
    /// `SegmentTable::dbg_hash_contains_only`'s doc comment for the full
    /// argument. Still the SAME production `hash_contains` routine
    /// `contains_base` itself falls through to on every real Tier-1 miss —
    /// not an alternate/bypass implementation.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
    #[must_use]
    pub fn dbg_hash_contains_only(&self, base: *mut u8) -> bool {
        self.core.dbg_hash_contains_only(base)
    }

    /// MEASUREMENT-ONLY (R23-3, task #372): thin delegation to
    /// [`dealloc_own_thread_with_base`](super::heap_core_free::HeapCore::dealloc_own_thread_with_base),
    /// exposed so `benches/perf_gate_iai.rs` can isolate the free path's
    /// POST-ROUTING body — the M2 double-free oracle checks (in-magazine
    /// bitmap probe + flushed/alloc-bitmap probe) and the magazine push
    /// itself — from the ROUTING prefix (`segment_base_of_ptr` +
    /// `contains_base`) that R22-17/R23-1 already isolated. This is the SAME
    /// production own-thread free body `dealloc_routing` calls once ownership
    /// is confirmed — not an alternate/bypass implementation; `base` must
    /// already be the correct segment-aligned base for `ptr` (the same value
    /// `dbg_segment_base_of_ptr` would produce), exactly as the real
    /// `dealloc_routing` caller already has it in hand from its own
    /// `contains_base` check.
    ///
    /// Per this file's other `dbg_push_to_ring`-style hooks, this is an
    /// `unsafe fn`: it forwards the identical [`HeapCore::dealloc`]
    /// caller-pointer contract (`ptr` is null or a live, exactly-once-freed
    /// start pointer previously returned by this heap's `alloc` for the same
    /// `layout`; `base` is that pointer's true segment base) — the same
    /// contract `dealloc_own_thread_with_base` itself inherits from being
    /// reachable only via the `unsafe fn dealloc`/`dealloc_routing` chain. No
    /// new safety reasoning is introduced beyond what that existing chain
    /// already carries.
    ///
    /// # Safety
    ///
    /// The caller must uphold the same [`GlobalAlloc::dealloc`] contract
    /// documented on [`HeapCore::dealloc`]'s `# Safety` section for `ptr`/
    /// `layout`, AND `base` must equal `os::segment_base_of_ptr(ptr)` for a
    /// segment this heap owns (the same precondition `dealloc_routing`
    /// already establishes via its `contains_base` check before reaching this
    /// body in production).
    // R24-6 (task #384): gated additionally on `bench-internals` so this
    // `unsafe fn` measurement-only hook is NOT reachable from plain
    // `--features production` (its prior gate — `alloc-global` + `fastbin` —
    // is fully satisfied by `production`'s feature list on its own). Its one
    // caller, `benches/perf_gate_iai.rs`, now requires `bench-internals` too
    // (added to that bench target's `required-features` in `Cargo.toml`; CI's
    // `perf-gate.yml` invocation passes it explicitly). See the
    // `bench-internals` feature doc in `Cargo.toml` for the full rationale.
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "bench-internals"
    ))]
    #[inline(always)]
    #[allow(unsafe_code)] // R23-3: `unsafe fn` boundary, mirrors `dbg_push_to_ring`/`HeapCore::dealloc`.
    pub unsafe fn dbg_dealloc_own_thread_with_base(
        &mut self,
        ptr: *mut u8,
        layout: Layout,
        base: *mut u8,
    ) {
        // SAFETY: this method carries the identical `# Safety` contract as
        // `HeapCore::dealloc`/`dealloc_own_thread_with_base`, forwarded to
        // THIS caller verbatim.
        self.dealloc_own_thread_with_base(ptr, layout, base);
    }

    /// R28-1 (task #430) MEASUREMENT-ONLY: run `AllocCore::flush_class`
    /// standalone on `blocks`, exposing the ONE overflow sub-cost R24-2 §5.1
    /// flagged as "NOT cleanly isolable without a hook that calls
    /// `flush_class` standalone" (the ~470 Ir non-isolable remainder inside
    /// one magazine-overflow event — `flush_class` + the 8-pointer compaction
    /// shift + the final push, R24-2's §1.3/§4.4). This hook isolates JUST
    /// `flush_class`'s own cost, leaving compaction + final push as a
    /// (smaller) separate remainder — see
    /// `docs/perf/R28_1_FLUSH_CLASS_ISOLATION_GATE.md` for the full
    /// decomposition and the isolation arithmetic.
    ///
    /// Delegates to [`AllocCore::flush_class`] verbatim — production's exact
    /// overflow-arm call (`heap_core_free.rs`'s magazine-overflow branch of
    /// `dealloc_own_thread_with_base`), not an alternate/bypass
    /// implementation. `class_idx`/`blocks` carry the identical contract as
    /// the delegated call: every entry is the live start pointer of a
    /// currently-magazine-resident block of size class `class_idx`, freed at
    /// most once across the call. Following R24-2's own documented
    /// disclosure of why this is the "exact Heisenberg risk" a naive
    /// standalone call would hit: this hook does NOT itself re-derive
    /// magazine state (it does not touch `self.tcache` at all) — the caller
    /// (the bench arm) is responsible for constructing a `blocks` slice that
    /// mirrors exactly what production's overflow arm passes in (already
    /// bitmap-cleared via `dbg_overflow_bitmap_clear_pass`'s former loop, now
    /// inlined at the call site — see the bench arm's own doc comment) and
    /// for NOT relying on the surrounding `HeapCore`'s magazine/BinTable
    /// invariants being intact afterward (`blocks` are returned to the
    /// substrate for real; a subsequent alloc of the SAME class from the SAME
    /// heap instance is a fresh carve/pop, not a re-issue of a still-resident
    /// magazine entry).
    ///
    /// # Safety
    ///
    /// The caller must uphold [`AllocCore::flush_class`]'s `# Safety`
    /// contract verbatim for `class_idx`/`blocks`: every non-null entry is
    /// the exact start pointer of a currently-LIVE small-class allocation of
    /// size class `class_idx` owned by this heap's substrate, not an interior
    /// or foreign pointer, and each entry is freed **at most once** across
    /// this call (a duplicate entry, or a block already on the free list, is
    /// contract UB — the per-block M2 guards inside `flush_run` degrade
    /// several such cases benignly at runtime, but that is defence-in-depth,
    /// not a substitute for honouring the contract). Null entries are
    /// permitted and skipped.
    // `bench-internals`-gated from creation (CLAUDE.md's benchmark-hook
    // rule): this `unsafe fn` derives allocator metadata writes from a
    // caller-controlled raw-pointer slice with zero validation beyond
    // `flush_class`'s own per-block M2 guards, so it must never be reachable
    // from a plain `--features production` build — its gate (`alloc-global`
    // + `fastbin` + `bench-internals`) matches `dbg_dealloc_own_thread_with_
    // base`'s exact precedent above, and its ONE caller
    // (`benches/perf_gate_iai.rs`) requires `bench-internals` on the whole
    // bench target already (`Cargo.toml`'s `required-features`).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "bench-internals"
    ))]
    #[inline(always)]
    #[allow(unsafe_code)] // R28-1: `unsafe fn` boundary, mirrors `dbg_dealloc_own_thread_with_base` above.
    pub unsafe fn dbg_flush_class_only(&mut self, class_idx: usize, blocks: &[*mut u8]) {
        // SAFETY: this method carries the identical `# Safety` contract as
        // the delegated `AllocCore::flush_class`, forwarded to THIS caller
        // verbatim.
        unsafe { self.core.flush_class(class_idx, blocks) };
    }

    /// R29-10 (task #441) MEASUREMENT-ONLY: run the EXACT production alloc-hit
    /// `clear_magazine` block (`src/registry/heap_core_alloc.rs`, the RAD-5 E4
    /// block that runs on EVERY magazine hit under `production`) standalone on
    /// `issued`, so `benches/perf_gate_iai.rs` can isolate its combined Ir cost
    /// (`segment_base_of_ptr(issued)` + `SegmentMeta::new(base).magazine_bitmap
    /// ().clear_magazine(off)`) — the ALLOC-side sub-mechanism R3's
    /// honest-reject (`docs/perf/IAI_BASELINE.md`) flagged as "never isolated"
    /// (R3 rejected DEFERRING the clear for correctness reasons but admitted "no
    /// iai baseline was taken; there is nothing to measure"). See
    /// `docs/perf/R29_10_ALLOC_HIT_CLEAR_MAGAZINE_ISOLATION_GATE.md`.
    ///
    /// Unlike `dbg_flush_class_only` (which delegates to one callable production
    /// function), this hook INLINES the production block byte-for-byte: in
    /// production the three lines are straight-line code inside the
    /// magazine-hit branch, not a callable function, so the faithful isolation
    /// is an exact textual copy of that straight-line block (no
    /// alternate/bypass implementation, no extra bookkeeping). `issued` carries
    /// the identical value the production block already receives at its call
    /// site (`self.tcache.classes[c].slots[new_cnt]` — the just-popped
    /// magazine-resident block).
    ///
    /// # Safety
    ///
    /// `issued` must be the exact start pointer of a currently-live allocation
    /// residing in a segment owned by this heap's substrate — the same
    /// precondition the production magazine-hit block already relies on. That
    /// block re-derives the segment base via `os::segment_base_of_ptr(issued)`
    /// and writes the magazine-residency bitmap at that derived base with ZERO
    /// validation beyond the pointer's own segment-alignment, so a foreign,
    /// null, interior, or already-recycled `issued` is contract UB: the derived
    /// `base` may be unmapped (crash) or may alias an unrelated segment's bitmap
    /// (silent metadata corruption). The individual primitives composed here
    /// (`segment_base_of_ptr` / `SegmentMeta::new` / `magazine_bitmap` /
    /// `clear_magazine`) are each safe `pub(crate)` fns, but their COMBINATION
    /// derives an unchecked metadata write from a raw pointer — the exact shape
    /// CLAUDE.md's benchmark-hook rule (the R25-1 fix for
    /// `dbg_overflow_bitmap_clear_pass`) requires to be `pub unsafe fn` with a
    /// documented `# Safety` contract rather than a safe `pub fn`. The sole
    /// caller is the bench arm, which constructs `issued` as a
    /// freshly-freed-into-the-magazine live block.
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "bench-internals"
    ))]
    #[inline(always)]
    #[allow(unsafe_code)] // R29-10: `unsafe fn` boundary, mirrors `dbg_flush_class_only` above.
    pub unsafe fn dbg_clear_magazine_on_hit(&self, issued: *mut u8) {
        // Byte-for-byte copy of the production magazine-hit clear block
        // (`heap_core_alloc.rs`'s RAD-5 E4 lines), so the isolated Ir is the
        // real in-context cost, not an invented mechanism.
        // SAFETY: forwarded from this caller's identical `# Safety` contract —
        // `issued` is a live block in an owned segment, exactly as production
        // assumes at the magazine-hit call site.
        let base = os::segment_base_of_ptr(issued);
        let off = (issued as usize - base as usize) as u32;
        SegmentMeta::new(base).magazine_bitmap().clear_magazine(off);
    }

    // ── R29-3 (task #434) — segment-lifecycle decomposition delegation ──────
    //
    // Thin delegation to the `AllocCore`-level hooks in
    // `alloc_core_small_pool.rs`, so `examples/r29_3_*` and
    // `benches/perf_gate_iai.rs` can reach them through the `HeapCore`
    // `#[doc(hidden)]` surface (the established pattern — same visibility
    // discipline as every other `dbg_*` in this file).

    /// R29-3 delegation — see [`AllocCore::dbg_decomp_full_cycle`].
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_full_cycle(&mut self) -> bool {
        self.core.dbg_decomp_full_cycle()
    }

    /// R29-3 delegation — see [`AllocCore::dbg_decomp_os_roundtrip`].
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_os_roundtrip() -> bool {
        crate::alloc_core::AllocCore::dbg_decomp_os_roundtrip()
    }

    /// R29-3 delegation — see [`AllocCore::dbg_decomp_reserve_and_keep`].
    ///
    /// R31-4 (task #467): forwards [`crate::alloc_core::ReservedSmallSegment`]
    /// instead of a bare `*mut u8` — see that type's module doc.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_reserve_and_keep(
        &mut self,
    ) -> Option<crate::alloc_core::ReservedSmallSegment> {
        self.core.dbg_decomp_reserve_and_keep()
    }

    /// R29-3 delegation — see [`AllocCore::dbg_decomp_release`].
    ///
    /// R31-4 (task #467): forwards the handle by value; no `unsafe` needed
    /// any more — see [`AllocCore::dbg_decomp_release`]'s doc comment for
    /// why the move-consuming signature replaces the former `unsafe fn`
    /// contract.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_release(&mut self, handle: crate::alloc_core::ReservedSmallSegment) {
        self.core.dbg_decomp_release(handle);
    }

    /// R29-3 delegation — see [`AllocCore::dbg_decomp_decommit_payload`].
    ///
    /// # Safety
    ///
    /// Same contract as [`AllocCore::dbg_decomp_decommit_payload`].
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[allow(unsafe_code)] // R29-3: unsafe fn boundary, forwarded contract.
    pub unsafe fn dbg_decomp_decommit_payload(base: *mut u8) {
        // SAFETY: forwarded from this caller's identical `# Safety` contract.
        unsafe { crate::alloc_core::AllocCore::dbg_decomp_decommit_payload(base) };
    }

    /// R31-6 (task #469) delegation — see [`AllocCore::dbg_decomp_recommit_payload`].
    ///
    /// # Safety
    ///
    /// Same contract as [`AllocCore::dbg_decomp_recommit_payload`].
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[allow(unsafe_code)] // R31-6: unsafe fn boundary, forwarded contract.
    pub unsafe fn dbg_decomp_recommit_payload(base: *mut u8) -> bool {
        // SAFETY: forwarded from this caller's identical `# Safety` contract.
        unsafe { crate::alloc_core::AllocCore::dbg_decomp_recommit_payload(base) }
    }

    /// R29-3 delegation — see [`AllocCore::dbg_decomp_payload_range`].
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_payload_range() -> (usize, usize) {
        crate::alloc_core::AllocCore::dbg_decomp_payload_range()
    }

    /// R29-3 delegation — see [`AllocCore::dbg_decomp_page_size`].
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_page_size() -> usize {
        crate::alloc_core::AllocCore::dbg_decomp_page_size()
    }
}
