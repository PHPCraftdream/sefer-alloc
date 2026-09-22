//! Diagnostics / measurement-only probes for [`HeapCore`] — second half of
//! the mechanical split of the former flat `heap_core_diag.rs` (the first
//! half — inspection hooks and ring-push / coarse-only simulation — lives in
//! the sibling `diag` module).
//!
//! This file holds the `impl HeapCore { .. }` block for the promotion /
//! hardened-defensive-noop counters (`dbg_promotion_compiled`,
//! `dbg_hardened_large_noop_count`), the `contains_base` family, the `unsafe`
//! delegation wrappers (`dbg_dealloc_own_thread_with_base`,
//! `dbg_flush_class_only`, `dbg_clear_magazine_on_hit`), and the
//! `dbg_decomp_*` family. Pure code-movement sibling; no behavior changed.

use crate::registry::heap_core::HeapCore;

// Referenced ONLY by the `alloc-global` + `fastbin` + `bench-internals`-gated
// `dbg_dealloc_own_thread_with_base` hook in this file, so it carries that
// identical predicate — the same file-head cfg-import convention as
// `heap_core/free/realloc.rs`. Ungated, it would be an unused-import warning
// in every configuration without `fastbin` / `bench-internals` (unlike the
// `diag` sibling, whose import list carries the former flat file's warning
// history verbatim).
//
// Task #2000: `os` / `SegmentMeta` (the OTHER former consumers of this same
// gate) were removed — `dbg_clear_magazine_on_hit` now calls the shared
// `HeapCore::clear_magazine_on_issue` instead of inlining its own
// `os::segment_base_of_ptr` + `SegmentMeta::new(..).magazine_bitmap()` copy.
#[cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "bench-internals"
))]
use core::alloc::Layout;

impl HeapCore {
    /// R16-5 (task #315) TEST-ONLY: `true` iff `HeapCore::realloc`'s
    /// Small/medium->Large promotion mechanism (`try_promote_to_large`,
    /// `free/realloc.rs`) is compiled into THIS build — i.e. the exact
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
    /// [`HARDENED_LARGE_NOOP_COUNT`](crate::registry::heap_core::free::dealloc::HARDENED_LARGE_NOOP_COUNT)'s
    /// doc comment for exactly which two branches (`free/dealloc_own_base.rs`'s
    /// branch (A) mismatch case and branch (B)) share this one counter and
    /// why. Relaxed load — diagnostic only. Reads 0 unless `alloc-stats` is
    /// on (the increment sites are gated); the accessor itself is always
    /// compiled (gated on `alloc-core`, same as the static) so callers need
    /// no `#[cfg]`.
    #[doc(hidden)]
    #[cfg(feature = "alloc-core")]
    #[must_use]
    pub fn dbg_hardened_large_noop_count() -> u64 {
        crate::registry::heap_core::free::dealloc::HARDENED_LARGE_NOOP_COUNT
            .load(core::sync::atomic::Ordering::Relaxed)
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
    /// (`heap_core_xthread`) itself calls (`self.core.contains_base(base)`)
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
    /// skipping the Tier-1 `OWN_CACHE_SIZE`-entry `own_cache` check that `dbg_contains_base`
    /// above (mirroring the real `dealloc_routing` call) always tries first.
    ///
    /// **Why this hook exists instead of just constructing a >`OWN_CACHE_SIZE`-segment
    /// workload for the existing `dbg_contains_base` hook:** `own_cache`'s
    /// hit/miss behaviour is keyed by `(base >> SEGMENT_SHIFT) & (OWN_CACHE_SIZE - 1)` — a
    /// function of the segment's OS-assigned virtual address, which
    /// `mmap`/`VirtualAlloc` chooses, not this allocator. A workload that
    /// allocates more than `OWN_CACHE_SIZE` distinct segments does not portably guarantee a Tier-2
    /// hit: the OS could still lay the segments out so their cache indices
    /// never collide inside one deterministic-iai run. This hook sidesteps
    /// that non-determinism by calling the Tier-2 probe directly — see
    /// `SegmentTable::dbg_hash_contains_only`'s doc comment for the full
    /// argument. Still the SAME production `hash_contains` routine
    /// `contains_base` itself falls through to on every real Tier-1 miss —
    /// not an alternate/bypass implementation.
    ///
    /// Sol-F1 (task #563): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_hash_contains_only`] moved behind `internals` (see
    /// that method's file, `alloc_core_core_diag.rs`, module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-global",
        feature = "alloc-xthread",
        feature = "internals"
    ))]
    #[must_use]
    pub fn dbg_hash_contains_only(&self, base: *mut u8) -> bool {
        self.core.dbg_hash_contains_only(base)
    }

    /// MEASUREMENT-ONLY (R32-10, task #501, F2): thin delegation to
    /// [`AllocCore::dbg_contains_base_tier1_hits`] — exposed at the
    /// `HeapCore` level (mirroring `dbg_hash_contains_only`'s existing
    /// delegation pattern in this file) so a macro-bench workload driven
    /// through the real `HeapCore`/`#[global_allocator]` entry point can read
    /// back the process-wide Tier-1 hit/miss split without reaching into
    /// `alloc_core`-internal modules. Process-wide (a bare associated fn, no
    /// `&self`), matching `dbg_maybe_decay_guard_passed_count`'s existing
    /// convention for other process-wide diagnostic counters in this crate.
    /// Reads 0 unless `bench-internals` is on.
    ///
    /// Sol-F1 (task #563): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_contains_base_tier1_hits`] moved behind `internals`
    /// (see that method's file, `alloc_core_core_diag.rs`, module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-global",
        feature = "bench-internals",
        feature = "internals"
    ))]
    #[must_use]
    pub fn dbg_contains_base_tier1_hits() -> u64 {
        crate::alloc_core::AllocCore::dbg_contains_base_tier1_hits()
    }

    /// MEASUREMENT-ONLY (R32-10, task #501, F2): the Tier-2-fallback
    /// complement of [`HeapCore::dbg_contains_base_tier1_hits`]. See
    /// [`AllocCore::dbg_contains_base_tier1_misses`].
    ///
    /// Sol-F1 (task #563): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_contains_base_tier1_misses`] moved behind
    /// `internals` (see that method's file, `alloc_core_core_diag.rs`,
    /// module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-global",
        feature = "bench-internals",
        feature = "internals"
    ))]
    #[must_use]
    pub fn dbg_contains_base_tier1_misses() -> u64 {
        crate::alloc_core::AllocCore::dbg_contains_base_tier1_misses()
    }

    /// MEASUREMENT-ONLY (R32-10, task #501, F2): thin delegation to
    /// [`AllocCore::dbg_reset_contains_base_tier1_counters`], exposed at the
    /// `HeapCore` level for the same reason as the two accessors above.
    ///
    /// Sol-F1 (task #563): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_reset_contains_base_tier1_counters`] moved behind
    /// `internals` (see that method's file, `alloc_core_core_diag.rs`,
    /// module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-global",
        feature = "bench-internals",
        feature = "internals"
    ))]
    pub fn dbg_reset_contains_base_tier1_counters() {
        crate::alloc_core::AllocCore::dbg_reset_contains_base_tier1_counters();
    }

    /// MEASUREMENT-ONLY (R23-3, task #372): thin delegation to
    /// [`dealloc_own_thread_with_base`](crate::registry::heap_core::HeapCore::dealloc_own_thread_with_base),
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
    /// overflow-arm call (`free/dealloc_own_base.rs`'s magazine-overflow branch of
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
    /// `clear_magazine` block (`src/registry/heap_core/alloc/hot.rs`, the RAD-5 E4
    /// block that runs on EVERY magazine hit under `production`) standalone on
    /// `issued`, so `benches/perf_gate_iai.rs` can isolate its combined Ir cost
    /// (`segment_base_of_ptr(issued)` + `SegmentMeta::new(base).magazine_bitmap
    /// ().clear_magazine(off)`) — the ALLOC-side sub-mechanism R3's
    /// honest-reject (`docs/perf/IAI_BASELINE.md`) flagged as "never isolated"
    /// (R3 rejected DEFERRING the clear for correctness reasons but admitted "no
    /// iai baseline was taken; there is nothing to measure"). See
    /// `docs/perf/R29_10_ALLOC_HIT_CLEAR_MAGAZINE_ISOLATION_GATE.md`.
    ///
    /// Task #2000: like `dbg_flush_class_only`, this hook now delegates to
    /// one callable production function, [`HeapCore::clear_magazine_on_issue`]
    /// — before that task, the three lines were straight-line code inside the
    /// magazine-hit branch with no callable function to delegate to, so the
    /// faithful isolation was an exact textual copy instead; extracting the
    /// shared helper closed that gap, so this hook calls it directly and
    /// cannot drift from what production actually runs. `issued` carries the
    /// identical value the production block already receives at its call
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
        // Task #2000: calls the SAME shared step the production magazine-hit
        // arms now call (`HeapCore::clear_magazine_on_issue`,
        // `heap_core/alloc/hot.rs`) instead of an independent textual copy —
        // strictly stronger than the former "byte-for-byte copy" contract
        // (a shared call site cannot drift; a textual copy could).
        // SAFETY: forwarded from this caller's identical `# Safety` contract —
        // `issued` is a live block in an owned segment, exactly as production
        // assumes at the magazine-hit call site.
        let _ = Self::clear_magazine_on_issue(issued);
    }

    // ── R29-3 (task #434) — segment-lifecycle decomposition delegation ──────
    //
    // Thin delegation to the `AllocCore`-level hooks in
    // `alloc_core_small_pool.rs`, so `examples/r29_3_*` and
    // `benches/perf_gate_iai.rs` can reach them through the `HeapCore`
    // `#[doc(hidden)]` surface (the established pattern — same visibility
    // discipline as every other `dbg_*` in this file).

    /// R29-3 delegation — see [`AllocCore::dbg_decomp_full_cycle`].
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// method moved behind `internals` (`alloc_core_small_pool.rs`'s module
    /// doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
    pub fn dbg_decomp_full_cycle(&mut self) -> bool {
        self.core.dbg_decomp_full_cycle()
    }

    /// R29-3 delegation — see [`AllocCore::dbg_decomp_os_roundtrip`].
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// method moved behind `internals` (`alloc_core_small_pool.rs`'s module
    /// doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
    pub fn dbg_decomp_os_roundtrip() -> bool {
        crate::alloc_core::AllocCore::dbg_decomp_os_roundtrip()
    }

    /// R29-3 delegation — see [`AllocCore::dbg_decomp_reserve_and_keep`].
    ///
    /// R31-4 (task #467): forwards [`crate::alloc_core::ReservedSmallSegment`]
    /// instead of a bare `*mut u8` — see that type's module doc.
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// method moved behind `internals` (`alloc_core_small_pool.rs`'s module
    /// doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
    pub fn dbg_decomp_reserve_and_keep(
        &mut self,
    ) -> Option<crate::alloc_core::ReservedSmallSegment> {
        self.core.dbg_decomp_reserve_and_keep()
    }

    /// R29-3 delegation — see [`AllocCore::dbg_decomp_release`].
    ///
    /// R31-15 (task #486): [`AllocCore::dbg_decomp_release`] is `unsafe fn`
    /// again (R31-4's move-consuming signature closed double-release but
    /// not owner-binding — a cross-`AllocCore` release was still safe-
    /// reachable UB; see that function's doc comment for the full
    /// writeup). This delegation forwards the identical `# Safety`
    /// contract.
    ///
    /// # Safety
    ///
    /// Same contract as [`AllocCore::dbg_decomp_release`]: `handle` must
    /// have been produced by a paired `dbg_decomp_reserve_and_keep` call on
    /// THIS SAME `HeapCore`'s underlying `AllocCore`, and the segment must
    /// still be live/unreleased.
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// method moved behind `internals` (`alloc_core_small_pool.rs`'s module
    /// doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
    #[allow(unsafe_code)] // R31-15: unsafe fn boundary, forwarded contract.
    pub unsafe fn dbg_decomp_release(&mut self, handle: crate::alloc_core::ReservedSmallSegment) {
        // SAFETY: forwarded from this caller's identical `# Safety` contract.
        unsafe { self.core.dbg_decomp_release(handle) };
    }

    /// R29-3 delegation — see [`AllocCore::dbg_decomp_decommit_payload`].
    ///
    /// # Safety
    ///
    /// Same contract as [`AllocCore::dbg_decomp_decommit_payload`].
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// method moved behind `internals` (`alloc_core_small_pool.rs`'s module
    /// doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
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
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// method moved behind `internals` (`alloc_core_small_pool.rs`'s module
    /// doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
    #[allow(unsafe_code)] // R31-6: unsafe fn boundary, forwarded contract.
    pub unsafe fn dbg_decomp_recommit_payload(base: *mut u8) -> bool {
        // SAFETY: forwarded from this caller's identical `# Safety` contract.
        unsafe { crate::alloc_core::AllocCore::dbg_decomp_recommit_payload(base) }
    }

    /// R29-3 delegation — see [`AllocCore::dbg_decomp_payload_range`].
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// method moved behind `internals` (`alloc_core_small_pool.rs`'s module
    /// doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
    pub fn dbg_decomp_payload_range() -> (usize, usize) {
        crate::alloc_core::AllocCore::dbg_decomp_payload_range()
    }

    /// R29-3 delegation — see [`AllocCore::dbg_decomp_page_size`].
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// method moved behind `internals` (`alloc_core_small_pool.rs`'s module
    /// doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
    pub fn dbg_decomp_page_size() -> usize {
        crate::alloc_core::AllocCore::dbg_decomp_page_size()
    }

    // ── task #504 (F11 step 1) — `aligned_vmem` reservation path-activation
    // counter delegation ──────────────────────────────────────────────────
    //
    // Thin delegation to the `AllocCore`-level hooks in
    // `alloc_core_core_diag.rs`, mirroring the R29-3 decomposition
    // delegation cluster above. `alloc-decommit` is NOT required here
    // (unlike the decomp cluster) — these counters observe every ordinary
    // segment reservation, not just the decommit/recommit cycle.

    /// task #504 delegation — see [`AllocCore::dbg_unix_exact_reserve_attempts`].
    ///
    /// Sol-F1 (task #563): additionally gated `internals` — the delegated
    /// method moved behind `internals` (`alloc_core_core_diag.rs`'s module
    /// doc).
    #[doc(hidden)]
    #[cfg(all(feature = "bench-internals", feature = "internals"))]
    pub fn dbg_unix_exact_reserve_attempts() -> u64 {
        crate::alloc_core::AllocCore::dbg_unix_exact_reserve_attempts()
    }

    /// task #504 delegation — see [`AllocCore::dbg_unix_exact_reserve_hits`].
    ///
    /// Sol-F1 (task #563): additionally gated `internals` — the delegated
    /// method moved behind `internals` (`alloc_core_core_diag.rs`'s module
    /// doc).
    #[doc(hidden)]
    #[cfg(all(feature = "bench-internals", feature = "internals"))]
    pub fn dbg_unix_exact_reserve_hits() -> u64 {
        crate::alloc_core::AllocCore::dbg_unix_exact_reserve_hits()
    }

    /// task #504 delegation — see [`AllocCore::dbg_windows_reserve_commit_calls`].
    ///
    /// Sol-F1 (task #563): additionally gated `internals` — the delegated
    /// method moved behind `internals` (`alloc_core_core_diag.rs`'s module
    /// doc).
    #[doc(hidden)]
    #[cfg(all(feature = "bench-internals", feature = "internals"))]
    pub fn dbg_windows_reserve_commit_calls() -> u64 {
        crate::alloc_core::AllocCore::dbg_windows_reserve_commit_calls()
    }

    /// task #504 delegation — see [`AllocCore::dbg_reset_vmem_bench_internals_counters`].
    ///
    /// Sol-F1 (task #563): additionally gated `internals` — the delegated
    /// method moved behind `internals` (`alloc_core_core_diag.rs`'s module
    /// doc).
    #[doc(hidden)]
    #[cfg(all(feature = "bench-internals", feature = "internals"))]
    pub fn dbg_reset_vmem_bench_internals_counters() {
        crate::alloc_core::AllocCore::dbg_reset_vmem_bench_internals_counters();
    }

    // ── task #504 (F11 step 2) — Windows reserve-vs-commit split delegation ─

    /// task #504 delegation — see [`AllocCore::dbg_decomp_win_reserve_only`].
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// method moved behind `internals` (`alloc_core_small_pool.rs`'s module
    /// doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
    pub fn dbg_decomp_win_reserve_only() -> Option<(*mut u8, *mut u8, usize)> {
        crate::alloc_core::AllocCore::dbg_decomp_win_reserve_only()
    }

    /// task #504 delegation — see [`AllocCore::dbg_decomp_win_commit_only`].
    ///
    /// # Safety
    ///
    /// Same contract as [`AllocCore::dbg_decomp_win_commit_only`].
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// method moved behind `internals` (`alloc_core_small_pool.rs`'s module
    /// doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
    #[must_use]
    #[allow(unsafe_code)] // task #504: unsafe fn boundary, forwarded contract.
    pub unsafe fn dbg_decomp_win_commit_only(base: *mut u8) -> bool {
        // SAFETY: forwarded from this caller's identical `# Safety` contract.
        unsafe { crate::alloc_core::AllocCore::dbg_decomp_win_commit_only(base) }
    }

    /// task #504 delegation — see [`AllocCore::dbg_decomp_win_release_only`].
    ///
    /// # Safety
    ///
    /// Same contract as [`AllocCore::dbg_decomp_win_release_only`].
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// method moved behind `internals` (`alloc_core_small_pool.rs`'s module
    /// doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
    #[allow(unsafe_code)] // task #504: unsafe fn boundary, forwarded contract.
    pub unsafe fn dbg_decomp_win_release_only(reservation_ptr: *mut u8, reservation_len: usize) {
        // SAFETY: forwarded from this caller's identical `# Safety` contract.
        unsafe {
            crate::alloc_core::AllocCore::dbg_decomp_win_release_only(
                reservation_ptr,
                reservation_len,
            )
        };
    }
}
