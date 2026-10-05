//! Diagnostics / test-only hooks for [`HeapCore`] (mechanical split of
//! `heap_core.rs`, task R4-10; further split of the former flat
//! `heap_core_diag.rs`).
//!
//! This file holds the first half of the `impl HeapCore { .. }` block: the
//! inspection test hooks (`dbg_owner_id_for`, `dbg_tcache_count`, etc.)
//! through the ring-push / coarse-only simulation hooks (`dbg_push_to_ring`,
//! `dbg_drain_all_rings`, `dbg_push_coarse_only_entry`). The promotion /
//! hardened-defensive-noop counters, the `contains_base` family, the `unsafe`
//! delegation wrappers, and the `dbg_decomp_*` family live in the sibling
//! `diag_probes` module. Pure code-movement sibling of `heap_core.rs`; no
//! behavior changed.

#[cfg(all(feature = "alloc-global", feature = "fastbin", feature = "internals"))]
use core::alloc::Layout;

use crate::alloc_core::os;
use core::sync::atomic::Ordering;

use crate::registry::heap_core::HeapCore;

impl HeapCore {
    /// TEST-ONLY (P4): read the `owner_id` stamped in the segment header of
    /// the segment that contains `ptr`. Returns `None` if `ptr` is not in a
    /// segment owned by this heap's substrate. Used by
    /// `tests/heap_core_tcache_stamp.rs` to verify the stamp-hoist wrote
    /// the correct ownership.
    ///
    /// Numeric membership resolves the canonical allocator-origin root, including
    /// overaligned Large reservations. This observer never mutates owner caches.
    #[doc(hidden)]
    #[cfg(feature = "alloc-global")]
    pub fn dbg_owner_id_for(&self, ptr: *mut u8) -> Option<u32> {
        use crate::alloc_core::segment_header::{unpack_owner_id, SegmentMeta};
        let base = self.core.canonical_root_for(ptr)?;
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
    ///
    /// Sol-F1 (task #563): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_kind_at_tag`] moved behind `internals` (see that
    /// method's file, `alloc_core_core_diag.rs`, module doc).
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "internals"))]
    #[must_use]
    pub fn dbg_kind_at_tag(&self, ptr: *mut u8) -> u8 {
        self.core.dbg_kind_at_tag(ptr)
    }

    /// R32-9 (task #500) MEASUREMENT-ONLY: thin delegation to
    /// [`AllocCore::dbg_table_count`] — exposed at the `HeapCore` level
    /// (mirroring `dbg_kind_at_tag`'s/`dbg_owner_id_for`'s existing
    /// delegation pattern in this file) so the new `≥64-live-segment`
    /// macro-bench harness (`benches/macro_multiseg_steady_state.rs`,
    /// `examples/r32_9_macro_multiseg_steady_state_ab_gate.rs`) can read
    /// back this heap's segment-table high-water count as its
    /// path-activation oracle: the harness's steady-state workload never
    /// decommits below its target floor, so within the measured window the
    /// high-water count and the true live count coincide (see
    /// `SegmentTable::count`'s own doc: "the number of LIVE (non-NULL)
    /// segments is `self.bases().count()`" — strictly `<=` this value in
    /// general, but equal here because nothing is ever recycled once the
    /// floor is reached). Read-only `&self`; does NOT mutate allocator
    /// state; does NOT derive a base from a caller pointer (same safety
    /// category as `dbg_kind_at_tag`), so a plain safe `fn` is correct, not
    /// `unsafe fn`. Gated on `alloc-global` only (matching this file's
    /// other ungated-beyond-`alloc-global` accessors), because
    /// `AllocCore::dbg_table_count` itself carries no additional feature
    /// gate.
    ///
    /// Sol-F1 (task #563): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_table_count`] moved behind `internals` (see that
    /// method's file, `alloc_core_core_diag.rs`, module doc).
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "internals"))]
    #[must_use]
    pub fn dbg_table_count(&self) -> u32 {
        self.core.dbg_table_count()
    }

    #[doc(hidden)]
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    pub fn dbg_active_kind_census(&self) -> (usize, usize, bool) {
        self.core.dbg_active_kind_census()
    }

    #[doc(hidden)]
    #[cfg(all(
        feature = "internals",
        feature = "bench-internals",
        feature = "alloc-xthread"
    ))]
    pub fn dbg_large_sidecar_slot_inspections() -> u64 {
        crate::alloc_core::AllocCore::dbg_large_sidecar_slot_inspections()
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
    /// `TCACHE_CAP` accepted blocks (see `free/dealloc_batch.rs`'s doc).
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
    /// [`PerClass::virgin_mask`](crate::registry::heap_core::state::tcache::PerClass) bitmask for class
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
    ///
    /// Sol-F1 (task #563): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_layout_class_for`] moved behind `internals` (see
    /// that method's file, `alloc_core_core_diag.rs`, module doc).
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "fastbin", feature = "internals"))]
    pub fn dbg_class_for(&self, layout: Layout) -> Option<usize> {
        self.core.dbg_layout_class_for(layout)
    }

    /// TEST-ONLY (task D3): the per-class refill amount `alloc`'s
    /// magazine-miss path actually uses for class `c` — i.e.
    /// `crate::registry::heap_core::state::tcache::refill_n_for_class(SizeClasses::block_size(c))`, the
    /// exact expression `alloc` evaluates. Lets a test assert the byte-budget
    /// clamp fired for a given class without duplicating (and risking
    /// drifting from) the formula.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub fn dbg_refill_n_for_class(&self, c: usize) -> usize {
        crate::registry::heap_core::state::tcache::refill_n_for_class(
            crate::alloc_core::size_classes::SizeClasses::block_size(c),
        )
    }

    /// TEST-ONLY (Mechanism 2, task #51): force-drain this heap's
    /// empty-small-segment hysteresis pool (release + recycle every pooled
    /// segment). Forwards to `AllocCore::drain_small_pool` — the production
    /// teardown-trim primitive (see [`trim_for_recycle`](Self::trim_for_recycle)).
    /// Used by decommit tests that run through the `SeferAlloc`/`HeapRegistry` face
    /// (where `claim_lease_with_config` cannot reliably disable the pool on a reused
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
    ///
    /// Sol-F1 (task #563): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_directory_get_bit`] moved behind `internals` (see
    /// that method's file, `alloc_core_core_diag.rs`, module doc).
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-segment-directory", feature = "internals"))]
    #[must_use]
    pub fn dbg_directory_bit_for_ptr(&self, ptr: *mut u8, class_idx: usize) -> Option<bool> {
        use crate::alloc_core::segment_header::SegmentHeader;
        let candidate = os::segment_base_of_ptr(ptr);
        // R29-17 (task #448): containment guard BEFORE the segment_id_at read.
        // segment_id_at dereferences the segment header (`Node::read_u32`), so
        // a null/foreign/arbitrary `ptr` whose segment-aligned base is unmapped
        // would read unmapped memory (crash) or arbitrary mapped memory
        // (garbage sid). Mirrors the `dbg_owner_id_for` guard shape in this
        // file (return `None` on a foreign base) rather than the assert-panic
        // shape of `dbg_segment_id_of` in `alloc_core_core_diag.rs`: this fn
        // already returns `Option<bool>` and documents "ptr is foreign → None",
        // and the single existing caller passes a genuinely live block.
        //
        // R2-05 (independent src review round 2, task #2007): the
        // `segment_id_at` read below now goes through the STORED (canonical)
        // segment base `segment_bases()` yields, not `ptr`'s caller-derived
        // address — `.find` (not `.any`) so the matched, canonical `*mut u8`
        // survives past the membership check. See
        // `SegmentTable::canonical_base_of`'s doc for the full rationale.
        let base = self.core.segment_bases().find(|&b| b == candidate)?;
        let sid = SegmentHeader::segment_id_at(base) as usize;
        self.core.dbg_directory_get_bit(class_idx, sid)
    }

    /// TEST-ONLY (R11-2): the number of empty small segments currently
    /// retained in this heap's hysteresis pool. Thin delegation to
    /// `AllocCore::dbg_pooled_count` — exposed at the `HeapCore` level so
    /// integration tests can assert that a segment emptied via an overflow-ring
    /// reclaim was actually pooled (not left as an ordinary registered
    /// segment by the pre-R11-2 bug that dropped the pool/release signal).
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_pooled_count`] moved behind `internals`
    /// (`alloc_core_small_pool.rs`'s module doc).
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "internals"))]
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
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_pool_cap`] moved behind `internals`
    /// (`alloc_core_small_pool.rs`'s module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
    #[must_use]
    pub fn dbg_pool_cap(&self) -> usize {
        self.core.dbg_pool_cap()
    }

    /// R29-4 (task #435) MEASUREMENT-ONLY: thin delegation to
    /// [`AllocCore::dbg_segment_state_reconciliation`] — exposed at the
    /// `HeapCore` level (mirroring `dbg_pooled_count`'s / `dbg_pool_cap`'s
    /// existing delegation pattern in this file) so the R29-4 probe can
    /// snapshot the per-state segment accounting from a claimed heap.
    /// Returns a [`SegmentStateReconciliation`] built from a DUAL
    /// enumeration: every registered segment is classified into exactly
    /// one state, and the heap's occupied large-cache slots are enumerated
    /// separately into `large_cached` (a cache deposit unregisters the
    /// segment first, so cached Large segments are never in the segment
    /// table). Its `committed_bytes` figures are the OS commit charge
    /// implied by each segment's frontier/backend contract, NOT a measured
    /// RSS figure. Read-only `&self`; does NOT mutate allocator state.
    /// `bench-internals`-gated (no production caller → R25-10 sub-rule 2).
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_segment_state_reconciliation`] moved behind
    /// `internals` (`alloc_core_small_pool.rs`'s module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
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
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_large_cache_used`] moved behind `internals`
    /// (`alloc_core_large_cache.rs`'s module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
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
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_large_cache_budget`] moved behind `internals`
    /// (`alloc_core_large_cache.rs`'s module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
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
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_large_cache_hits`] moved behind `internals`
    /// (`alloc_core_large_cache.rs`'s module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
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
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_large_cache_slot_sizes`] moved behind `internals`
    /// (`alloc_core_large_cache.rs`'s module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
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
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_large_cache_extended_slot_sizes`] moved behind
    /// `internals` (`alloc_core_large_cache.rs`'s module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "large-cache-extended",
        feature = "internals"
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
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_large_cache_extension_materialised`] moved behind
    /// `internals` (`alloc_core_large_cache.rs`'s module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "large-cache-extended",
        feature = "internals"
    ))]
    #[must_use]
    pub fn dbg_large_cache_extension_materialised(&self) -> bool {
        self.core.dbg_large_cache_extension_materialised()
    }

    /// R31-4 (task #487) MEASUREMENT-ONLY: thin delegation to
    /// [`AllocCore::dbg_large_cache_total_slots`] — exposed at the `HeapCore`
    /// level (same delegation pattern as `dbg_large_cache_used` above) so a
    /// gate driving the real `#[global_allocator]` (which only ever has a
    /// `HeapCore`, never a bare `AllocCore`, in hand) can read back the
    /// combined base+extension addressable slot count (8 if the extension
    /// has not materialised, 40 once it has) as an in-run materialisation
    /// oracle — the same evidence
    /// `tests/large_cache_extended_narrow_working_set_after_materialization.rs::scan_bound_stays_forty_during_narrow_working_set_phase`
    /// already asserts one layer down, at the `AllocCore` level. Read-only
    /// `&self`; does NOT mutate allocator state. `bench-internals`-gated (no
    /// production caller → R25-10 sub-rule 2). Unlike the two
    /// `large-cache-extended`-gated siblings immediately above, this
    /// delegates a method that is available under plain `alloc-decommit`
    /// (it reads 8 when the extension feature/sidecar is absent), so it is
    /// NOT additionally gated on `large-cache-extended` — matching
    /// [`AllocCore::dbg_large_cache_total_slots`]'s own gate exactly.
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_large_cache_total_slots`] moved behind `internals`
    /// (`alloc_core_large_cache.rs`'s module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
    #[must_use]
    pub fn dbg_large_cache_total_slots(&self) -> usize {
        self.core.dbg_large_cache_total_slots()
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
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_decay_config`] moved behind `internals`
    /// (`alloc_core_large_cache.rs`'s module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
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
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_force_decay_tick`] moved behind `internals`
    /// (`alloc_core_large_cache.rs`'s module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
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
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_live_count_for`] moved behind `internals`
    /// (`alloc_core_small_pool.rs`'s module doc).
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "internals"))]
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
    ///
    /// Sol-F1 (task #563): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_is_free_for`] moved behind `internals` (see that
    /// method's file, `alloc_core_small_diag.rs`, module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-global",
        feature = "bench-internals",
        feature = "internals"
    ))]
    #[must_use]
    pub fn dbg_is_free_for(&self, ptr: *mut u8) -> bool {
        self.core.dbg_is_free_for(ptr)
    }
}
