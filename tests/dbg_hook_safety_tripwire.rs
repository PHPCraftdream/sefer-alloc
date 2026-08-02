//! Structural tripwire against the R25-1 / R29-7 / R29-8 / R30-1 bug class:
//! a **safe**, crate-public `pub fn dbg_*` hook that mutates allocator state
//! WITHOUT being `pub unsafe fn` and WITHOUT being gated behind
//! `bench-internals`.
//!
//! ## Why this test exists — and why it was widened (R30-2)
//!
//! Three separate instances of the same soundness hole survived across four
//! rounds because no prior audit searched for **safe** `fn`s of this shape —
//! they searched for hooks *already* marked `unsafe fn` with a
//! production-satisfied gate (see the CLAUDE.md "Benchmark-only `dbg_*`
//! hooks..." rule). R29-9 (task #440) converted that rule into a
//! compile-time-checked invariant, but its scanner ONLY matched hooks whose
//! signature TEXT contained `*mut`/`*const` — its own commit message claimed
//! this "closes the bug class for good." **That claim was too strong.** R30-1
//! (task #450) found and fixed a CONFIRMED soundness bug
//! (`AllocCore::dbg_decomp_full_cycle`, a zero-argument, no-raw-pointer,
//! `&mut self -> bool` hook that could dangle the `small_cur` bump-carve
//! cursor) that was STRUCTURALLY INVISIBLE to the R29-9 scanner, because
//! nothing in that hook's signature contains `*mut`/`*const` at all — the
//! danger is in what `&mut self` lets the body reach, not in any parameter
//! shape. R30-2 (task #451) is the honest correction: the bug class recurred
//! through a shape the original scanner didn't model, so the policy is now
//! **shape-independent** — every crate-public `dbg_*` hook is enumerated
//! regardless of its parameter/return shape (raw pointer, zero-arg `&mut
//! self`, `usize`/index-keyed, or an integer-encoded-address return), and
//! must appear in exactly one reviewed bucket below.
//!
//! ## The two mechanical rules (R30-2 redesign)
//!
//! (a) Every crate-public `dbg_*` hook (any signature shape) must be
//!     `bench-internals`-gated UNLESS it is explicitly allowlisted as a pure
//!     observer (reads state, never mutates allocator metadata, no
//!     soundness-relevant side effect).
//! (b) Every safe (non-`unsafe fn`) hook that MUTATES allocator state must be
//!     individually allowlisted with a short one-line invariant justification
//!     explaining why it's safe despite mutating — independent of whether it
//!     happens to take a pointer parameter.
//!
//! `pub unsafe fn` hooks stay covered by the ORIGINAL raw-pointer-focused
//! reasoning (the caller opts into `unsafe`, so the compiler-enforced
//! boundary already exists) — this test now merely enumerates them under the
//! `Unsafe` category for completeness so the total set is provably
//! exhaustive, not because they need a new safety argument.
//!
//! - **R25-1** (task #395): `HeapCore::dbg_overflow_bitmap_clear_pass` — a
//!   safe `pub fn` that derived a segment base from an arbitrary caller
//!   pointer and wrote allocator metadata (`clear_magazine`) at the derived
//!   offset, gated only on `alloc-global + fastbin` (both in `production`).
//! - **R29-7** (task #438, commit `3deb244`): `dbg_restore_local_for_test` —
//!   a safe `pub fn` that installed a caller-supplied raw pointer as this
//!   thread's live TLS `LOCAL` binding with zero validation.
//! - **R29-8** (task #439, commit `1bfa1d2`): `dbg_force_decommit_retain_for`
//!   — a safe `pub fn` that decommitted a segment's payload pages through a
//!   caller pointer whose segment base was containment-checked but whose
//!   `live_count == 0` precondition was not.
//! - **R30-1** (task #450, commit `25433c3`): `AllocCore::dbg_decomp_full_cycle`
//!   / `dbg_decomp_reserve_and_keep` / `dbg_decomp_release` — safe,
//!   `bench-internals`-gated (so not actually exploitable outside a
//!   measurement build) hooks whose bodies called `reserve_small_segment()`,
//!   unconditionally publishing the freshly reserved segment as the live
//!   `small_cur` bump-carve cursor, then released that same segment — leaving
//!   `small_cur` dangling. Zero raw-pointer parameters; invisible to R29-9's
//!   scanner by construction. Fixed by routing through a new cursor-free
//!   `reserve_small_segment_impl`.
//! - **R31-4** (task #467): `dbg_decomp_release` (both `AllocCore`'s and
//!   `HeapCore`'s delegation) was, until this round, `unsafe fn(&mut self,
//!   base: *mut u8)` — its raw-pointer precondition ("must be a live base
//!   returned by the paired `dbg_decomp_reserve_and_keep`") was enforced
//!   only by caller discipline plus a `debug_assert!` (compiled out in
//!   `--release`). Retrofitted onto a typed, move-consumed
//!   `ReservedSmallSegment` handle (`src/alloc_core/reserved_small_segment.rs`)
//!   — unforgeable (private field, `pub(super)` constructor) and
//!   double-release is now a COMPILE ERROR (E0382), not a runtime hazard.
//!   Both `dbg_decomp_release` entries moved out of `UNSAFE_HOOKS` below
//!   (they are safe `fn`s now, `bench-internals`-gated, so they need no
//!   `SAFE_MUTATORS` entry either — gated hooks aren't in either allowlist).
//! - **R31-15** (task #486, CONFIRMED P0): R31-4 closed unforgeability and
//!   double-release but NOT owner-binding — a handle minted by
//!   `dbg_decomp_reserve_and_keep` on one `AllocCore` could still be handed
//!   to `dbg_decomp_release` on a DIFFERENT `AllocCore`, both safe API
//!   calls, mutating the wrong heap's pool/directory/`SegmentTable` state.
//!   Both `dbg_decomp_release` entries (`AllocCore`'s and `HeapCore`'s
//!   delegation) moved BACK into `UNSAFE_HOOKS` below — `unsafe fn` again,
//!   with a documented `# Safety` contract, PLUS a structural release-build
//!   owner-id check (`ReservedSmallSegment::owner_id` compared against
//!   `AllocCore::dbg_reservation_owner_id`) as a second, non-`debug_assert!`
//!   layer. See `src/alloc_core/reserved_small_segment.rs`'s module doc
//!   ("Owner-binding" section) for the full writeup.
//!
//! ## What "safe by construction" means here (classification A)
//!
//! The majority of the pointer-taking allowlist entries follow this pattern:
//! the caller-supplied raw pointer is used ONLY to compute a segment base via
//! address masking (`os::segment_base_of_ptr` — pure arithmetic, no
//! dereference); that base is then verified against the allocator's own
//! live-segment registry (`contains_base_ro` / `segment_bases().any(..)` —
//! value comparison, no dereference of the base); and ONLY IF that check
//! passes does the function read a field from the segment's OWN header.
//! Because the containment check proves the base is a genuinely live,
//! mapped, owned segment, the read target is not attacker/caller-controlled
//! — only the question "is this address one of mine" is.
//!
//! Non-pointer-taking entries (the R30-2 addition) instead fall into one of:
//! a **pure observer** (reads a field/counter, no mutation at all); a
//! **delegate to the real production code path** (the hook calls the exact
//! same internal function the ordinary alloc/free/decay/trim/drain path
//! calls — sound because it's not a bypass, it's the production logic
//! invoked directly instead of through its usual caller); a **bounded
//! heuristic/policy knob** (mutates a value with no correctness
//! implication — e.g. a decay-timer config, a NUMA cache hint, a diagnostic
//! high-water counter — where a wrong value degrades performance, never
//! soundness); or a **self-contained CAS-guarded probe** (only touches state
//! it just atomically proved was in a specific starting state, and restores
//! it before returning).
//!
//! ## Doc-only guard
//!
//! Reads source text, never links the crate, so it runs in every feature
//! configuration (mirrors `tests/no_stale_doc_references.rs`'s technique).

use std::fs;
use std::path::{Path, PathBuf};

/// Collect every `*.rs` file under `dir` recursively (mirrors the helper in
/// `tests/no_stale_doc_references.rs`).
fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Normalise an absolute source path to a forward-slashed relative path from
/// the crate root, e.g. `src/alloc_core/alloc_core_core_diag.rs`.
fn rel_id(abs: &Path) -> String {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rel = abs.strip_prefix(manifest).unwrap_or(abs);
    rel.to_string_lossy().replace('\\', "/")
}

// ── Classification lists ──────────────────────────────────────────────────
//
// Every crate-public `dbg_*` hook — ANY signature shape — that is (1) safe
// (not `pub unsafe fn`) and (2) NOT `bench-internals`-gated MUST appear in
// exactly one of PURE_OBSERVERS or SAFE_MUTATORS below. `pub unsafe fn`
// hooks are enumerated separately in UNSAFE_HOOKS purely so the accounting
// is exhaustive (found == every classified hook, no silent gaps) — they do
// not need a safety argument here because the caller already opts into
// `unsafe`.
//
// ID format: `"relative/path.rs::fn_name"`.

/// (A) Pure observers — safe, non-`bench-internals`-gated, read allocator
/// state only, no mutation, no soundness-relevant side effect. No per-entry
/// justification needed beyond "read-only" (verified during classification
/// by reading each body).
const PURE_OBSERVERS: &[&str] = &[
    "crates/racy-ptr-cell/src/lib.rs::dbg_is_ready",
    "crates/ring-mpsc/src/lib.rs::dbg_cursors",
    "crates/ring-mpsc/src/lib.rs::dbg_is_marked",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_foreign_or_unroutable_frees",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_segments_reserved_total",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_segments_released_total",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_node_id_for",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_page_map_class_for",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_table_count",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_contains_base",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_hash_remove_max_scan_steps",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_hash_contains_only",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_segment_id_of",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_kind_byte_of",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_kind_at_tag",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_large_size_of",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_span_usable_of",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_reserved_capacity_of",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_block_size",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_small_class_count",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_layout_class_for",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_large_zero_pass_count",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_small_zero_pass_count",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_payload_virgin_for",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_hits",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_stale_hits",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_fallback_scans",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_words_examined",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_dirty_segments_drained",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_wasted_dirty_drains",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_opt_h_attempts",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_opt_h_hits",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_promotion_count",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_promotion_bytes_sum",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_promotion_bytes_min",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_promotion_bytes_max",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_promotion_bytes_hist",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_full_scan_slots_examined",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_authoritative_miss",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_miss_self_heal",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_rescue_oom_avoided",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_is_materialised",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_get_bit",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_get_bit_for_node",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_node_bitmaps",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_node_bucket_for",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_active_bits_for_bucket",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_get_bit_bucket",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_materialize_threshold",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_max_segments",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_words_per_class",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_miss_streak_for_class",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_miss_full_scan_period",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_sidecar_oom_latch",
    "src/alloc_core/alloc_core_large_cache.rs::dbg_large_cache_extended_slot_sizes",
    "src/alloc_core/alloc_core_large_cache.rs::dbg_large_cache_extension_materialised",
    "src/alloc_core/alloc_core_large_cache.rs::dbg_large_cache_total_slots",
    "src/alloc_core/alloc_core_large_cache.rs::dbg_decay_config",
    "src/alloc_core/alloc_core_large_cache.rs::dbg_large_cache_used",
    "src/alloc_core/alloc_core_large_cache.rs::dbg_large_cache_hits",
    "src/alloc_core/alloc_core_large_cache.rs::dbg_large_cache_slot_sizes",
    "src/alloc_core/alloc_core_large_cache.rs::dbg_large_cache_occupied_bits",
    "src/alloc_core/alloc_core_large_cache.rs::dbg_large_cache_budget",
    "src/alloc_core/alloc_core_large_cache.rs::dbg_large_cache_mode",
    "src/alloc_core/alloc_core.rs::dbg_cached_numa_node",
    "src/alloc_core/alloc_core_small_diag.rs::dbg_freelist_head_for",
    "src/alloc_core/alloc_core_small_diag.rs::dbg_is_free_for",
    "src/alloc_core/alloc_core_small_diag.rs::dbg_committed_payload_end_for",
    "src/alloc_core/alloc_core_small_diag.rs::dbg_grow_commit_count",
    "src/alloc_core/alloc_core_small_diag.rs::dbg_grow_chunk",
    "src/alloc_core/alloc_core_small_diag.rs::dbg_lazy_first_chunk",
    "src/alloc_core/alloc_core_small_pool.rs::dbg_decommit_count",
    "src/alloc_core/alloc_core_small_pool.rs::dbg_live_count_for",
    "src/alloc_core/alloc_core_small_pool.rs::dbg_pooled_count",
    "src/alloc_core/alloc_core_small_pool.rs::dbg_pool_cap",
    "src/alloc_core/alloc_core_small_pool.rs::dbg_is_decommitted_for",
    "src/alloc_core/remote_free_ring.rs::dbg_cursors",
    "src/global/fallback.rs::dbg_fallback_lock_acquisitions",
    "src/registry/bootstrap.rs::dbg_slot_state",
    "src/registry/bootstrap.rs::dbg_slot_generation",
    "src/registry/bootstrap.rs::dbg_chunk_is_materialised",
    "src/registry/heap_registry.rs::dbg_slot_initialised",
    "src/registry/heap_overflow.rs::dbg_sidecar_is_materialised",
    "src/registry/heap_core.rs::dbg_cached_numa_node",
    "src/registry/heap_core_diag.rs::dbg_owner_id_for",
    "src/registry/heap_core_diag.rs::dbg_class_for",
    "src/registry/heap_core_diag.rs::dbg_refill_n_for_class",
    "src/registry/heap_core_diag.rs::dbg_directory_bit_for_ptr",
    "src/registry/heap_core_diag.rs::dbg_pooled_count",
    "src/registry/heap_core_diag.rs::dbg_segment_base_of_ptr",
    "src/registry/heap_core_diag.rs::dbg_live_count_for",
    "src/registry/heap_core_diag.rs::dbg_sidecar_oom_latch",
    "src/registry/heap_core_diag.rs::dbg_tcache_virgin_mask",
    "src/registry/heap_core_diag.rs::dbg_tcache_count",
    "src/registry/heap_core_diag.rs::dbg_hardened_large_noop_count",
    "src/registry/heap_core_diag.rs::dbg_contains_base",
    "src/registry/heap_core_diag.rs::dbg_hash_contains_only",
    "src/registry/heap_core_diag.rs::dbg_last_stamped_segment",
    "src/registry/heap_core_diag.rs::dbg_kind_at_tag",
    "src/registry/heap_core_diag.rs::dbg_table_count",
];

/// (B) Safe mutators — safe, non-`bench-internals`-gated, DO mutate
/// allocator/ring state. Each entry's reason is the one-line invariant
/// justification for why the mutation cannot cause unsoundness (bounds
/// check, delegation to the identical production code path, or a
/// correctness-inert policy/heuristic knob). A `[DEBUG_ASSERT ONLY]` tag
/// flags an entry whose bound is a `debug_assert!` (compiled out in
/// `--release`) rather than a release-surviving guard — reviewed and
/// accepted because the class of corruption it bounds is itself only ring
/// bookkeeping (lost/miscounted entries), never a wrong pointer handed to a
/// caller, but flagged explicitly so a future reviewer does not have to
/// re-derive that distinction from scratch.
const SAFE_MUTATORS: &[(&str, &str)] = &[
    (
        "crates/racy-ptr-cell/src/lib.rs::dbg_rollback_reenterable",
        "entry CAS proves the cell UNINIT before touching it; restores original state before returning",
    ),
    (
        "crates/ring-mpsc/src/lib.rs::dbg_reserve_unpublished",
        "generic test-infra ring (no allocator metadata); documented quiescent-ring single-writer test contract",
    ),
    (
        "crates/ring-mpsc/src/lib.rs::dbg_set_cursors",
        "generic test-infra ring (no allocator metadata); documented quiescent-ring single-writer test contract",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_reset_hash_remove_max_scan_steps",
        "resets a diagnostic high-water counter only; no allocator metadata, no soundness relevance",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_find_segment_with_free",
        "calls the real production find_segment_with_free lookup directly; same invariant envelope as its normal caller",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_force_clear_bit",
        "directory bits are a soft self-healing cache with an always-available O(S) fallback scan; a stale-clear bit costs a slower scan, never UB or a wrong pointer",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_rescue_scan",
        "invokes the real OOM-rescue scan production code path directly; not a bypass",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_reset_miss_streak",
        "zeroes a heuristic miss-streak counter; wrong value only changes re-validation cadence, never correctness",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_set_miss_streak_for_class",
        "sets a heuristic miss-streak counter; wrong value only changes re-validation cadence, never correctness",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_rebuild_directory",
        "re-derives the directory bitmap from the authoritative SegmentTable/BinTable state, not an arbitrary write",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_force_sidecar_oom_latch",
        "sets the coarse-only OOM latch to a state production code already handles as a defined fallback path",
    ),
    (
        "src/alloc_core/alloc_core_large_cache.rs::dbg_force_decay_tick",
        "rewinds the decay timer then calls the real maybe_decay_large_cache production path; eviction logic itself unchanged",
    ),
    (
        "src/alloc_core/alloc_core_large_cache.rs::dbg_set_decay_config",
        "overwrites decay-heuristic tuning knobs only; no allocator-metadata/pointer correctness implication",
    ),
    (
        "src/alloc_core/alloc_core_large_cache.rs::dbg_set_large_cache_budget",
        "overwrites a byte-budget policy field; wrong value changes eviction aggressiveness, never soundness",
    ),
    (
        "src/alloc_core/alloc_core.rs::dbg_current_node_cached",
        "calls the real production current_node_cached NUMA-cache-populate path directly; not a bypass",
    ),
    (
        "src/alloc_core/alloc_core.rs::dbg_invalidate_numa_node_cache",
        "calls the real production invalidate_numa_node_cache; NUMA cache is a soft hint, not correctness metadata",
    ),
    (
        "src/alloc_core/alloc_core_small_diag.rs::dbg_carve_batch",
        "calls the real production carve_batch path; identical bump/bitmap mutation to an ordinary alloc_small carve",
    ),
    (
        "src/alloc_core/alloc_core_small_diag.rs::dbg_arm_commit_fail",
        "arms a process-global fault injector for future OS commit_pages calls; simulates an already-handled production error path",
    ),
    (
        "src/alloc_core/alloc_core_small_diag.rs::dbg_arm_commit_fail_at",
        "arms the Nth-call fault injector; same justification as dbg_arm_commit_fail",
    ),
    (
        "src/alloc_core/alloc_core_small_pool.rs::dbg_drain_small_pool",
        "calls the real production drain_small_pool teardown-trim primitive directly (also called from trim_for_recycle)",
    ),
    (
        "src/alloc_core/alloc_core_small_reclaim.rs::dbg_force_coarse_dirty_bit_for",
        "reconstructs a bitmap state the real sidecar-OOM push can already produce; consumers re-validate via ring/bitmap, so a spurious bit costs a wasted scan, never UB",
    ),
    (
        "src/alloc_core/alloc_core_small_reclaim.rs::dbg_drain_all_rings",
        "walks every segment via the table's own base_at (not a caller pointer) and drains via the real production reclaim logic",
    ),
    (
        "src/alloc_core/alloc_core_small_reclaim.rs::dbg_drain_all_rings_checked",
        "same as dbg_drain_all_rings, with an explicit predicate closure; still table-owned bases only",
    ),
    (
        "src/alloc_core/remote_free_ring.rs::dbg_set_cursors",
        "[DEBUG_ASSERT ONLY] directly overwrites a REAL segment's RemoteFreeRing head/tail cursors; documented quiescent-ring precondition is not runtime-enforced in release, but misuse can only corrupt the ring's own bookkeeping (lost/misdirected entries), never dereference a caller pointer or write outside the ring's own cursor words",
    ),
    (
        "src/alloc_core/remote_free_ring.rs::dbg_advance_head_only",
        "F10 (task #502) [DEBUG_ASSERT ONLY] directly overwrites a REAL segment's RemoteFreeRing head cursor only (the inverse of dbg_set_cursors, which also resets cached_head); same quiescent-ring precondition, same bounded blast radius as dbg_set_cursors (corrupts only this ring's own bookkeeping, never a caller pointer or memory outside the ring's own cursor word) -- see that hook's own doc comment for the full soundness argument this one shares",
    ),
    (
        "src/global/fallback.rs::dbg_panic_in_with_heap_releases_lock",
        "drives a real panic through the production with_heap/LockGuard RAII path; exercises, does not bypass, existing safe machinery",
    ),
    (
        "src/global/sefer_alloc.rs::dbg_trim_current_thread",
        "calls the real production trim_for_recycle on the current thread's own heap (also called from AbandonGuard::drop)",
    ),
    (
        "src/global/tls_heap.rs::dbg_teardown_then_resolve_is_fallback",
        "poisons then restores TLS LOCAL via the real mark_local_torn fn, bracketed within the call; net-zero from caller's perspective",
    ),
    (
        "src/global/tls_heap.rs::dbg_teardown_then_resolve_is_foreign_no_bind",
        "same poison/resolve/restore bracket as dbg_teardown_then_resolve_is_fallback",
    ),
    (
        "src/registry/bootstrap.rs::dbg_rollback_chunk_sentinel_reenterable",
        "entry CAS proves the chunk cell UNINIT before touching it; restores to null before returning",
    ),
    (
        "src/registry/heap_registry.rs::dbg_claim_then_simulate_oom",
        "claims a real slot via the production pick_slot/CAS/push_back_after_oom path; reproduces, does not invent, the real post-OOM rollback state",
    ),
    (
        "src/registry/heap_core_tcache.rs::dbg_flush_all",
        "calls the real production flush_all_tcache path used by ordinary teardown",
    ),
    (
        "src/registry/heap_overflow.rs::dbg_rollback_sidecar_sentinel_for_test",
        "panics if the sidecar pointer is not already null on entry (self-checked precondition); drives only the real rollback sequence",
    ),
    (
        "src/registry/heap_overflow.rs::dbg_reserve_unpublished_for_test",
        "[DEBUG_ASSERT ONLY] advances a REAL HeapOverflow ring's tail without publishing; bound to the inline tier is a debug_assert (compiled out in release) not a release-surviving guard, but misuse only corrupts this ring's own tail bookkeeping/occupancy accounting, never a caller-visible pointer",
    ),
    (
        "src/registry/heap_overflow.rs::dbg_fill_and_drain_inline_tier_for_test",
        "pushes/drains via the real production push()/drain() functions only; push validates internally",
    ),
    (
        "src/registry/heap_core.rs::dbg_populate_numa_cache_for_test",
        "calls the real production current_node_cached path via AllocCore; NUMA cache is a soft hint",
    ),
    (
        "src/registry/heap_core_diag.rs::dbg_drain_all_rings",
        "delegates to AllocCore's real production drain over table-owned bases only",
    ),
    (
        "src/registry/heap_core_diag.rs::dbg_force_sidecar_oom_latch",
        "delegates to AllocCore::dbg_force_sidecar_oom_latch; same bounded degrade-to-safe-fallback justification",
    ),
    (
        "src/registry/heap_core_diag.rs::dbg_drain_small_pool",
        "delegates to the real production AllocCore::drain_small_pool teardown-trim primitive (same as trim_for_recycle)",
    ),
];

/// (C) `pub unsafe fn dbg_*` hooks. Enumerated for exhaustive accounting only
/// — the caller's `unsafe` block is the safety boundary, not this list. Kept
/// separate from PURE_OBSERVERS/SAFE_MUTATORS so a hook cannot silently
/// satisfy this test by being declared `unsafe` without ALSO being reviewed
/// for whether `unsafe` is even the right tool (that review is CLAUDE.md's
/// per-item `# Safety` doc requirement, enforced by human review at land
/// time, not by this test).
const UNSAFE_HOOKS: &[&str] = &[
    "src/alloc_core/alloc_core_core_diag.rs::dbg_stamp_segment_id",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_stamp_kind_byte",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_unregister",
    "src/alloc_core/alloc_core_core_diag.rs::dbg_recycle",
    "src/alloc_core/alloc_core_small_diag.rs::dbg_drain_freelist_batch",
    "src/alloc_core/alloc_core_small_diag.rs::dbg_alloc_bitmap_bytes_for",
    "src/alloc_core/alloc_core_small_diag.rs::dbg_magazine_bitmap_bytes_for",
    "src/alloc_core/alloc_core_small_diag.rs::dbg_corrupt_freelist_head_next",
    "src/alloc_core/alloc_core_small_diag.rs::dbg_payload_start_for",
    "src/alloc_core/alloc_core_small_pool.rs::dbg_decomp_decommit_payload",
    "src/alloc_core/alloc_core_small_pool.rs::dbg_decomp_recommit_payload",
    "src/alloc_core/alloc_core_small_pool.rs::dbg_decomp_release",
    "src/alloc_core/alloc_core_small_pool.rs::dbg_decomp_win_commit_only",
    "src/alloc_core/alloc_core_small_pool.rs::dbg_decomp_win_release_only",
    "src/alloc_core/alloc_core_small_pool.rs::dbg_force_decommit_retain_for",
    "src/alloc_core/alloc_core_small_reclaim.rs::dbg_push_to_ring",
    "src/global/tls_heap.rs::dbg_restore_local_for_test",
    "src/registry/bootstrap.rs::dbg_slot_preset_generation",
    "src/registry/heap_core_diag.rs::dbg_push_to_ring",
    "src/registry/heap_core_diag.rs::dbg_push_coarse_only_entry",
    "src/registry/heap_core_diag.rs::dbg_dealloc_own_thread_with_base",
    "src/registry/heap_core_diag.rs::dbg_flush_class_only",
    "src/registry/heap_core_diag.rs::dbg_clear_magazine_on_hit",
    "src/registry/heap_core_diag.rs::dbg_decomp_decommit_payload",
    "src/registry/heap_core_diag.rs::dbg_decomp_recommit_payload",
    "src/registry/heap_core_diag.rs::dbg_decomp_release",
    "src/registry/heap_core_diag.rs::dbg_decomp_win_commit_only",
    "src/registry/heap_core_diag.rs::dbg_decomp_win_release_only",
];

/// Heuristic false positives from the raw-pointer text scan retained from
/// R29-9: the signature text contains `*mut`/`*const` but the pointer is NOT
/// a parameter or return type — it appears only in a generic bound or
/// similar non-load-bearing position. Kept as documentation of a known
/// scanner quirk; the R30-2 scanner no longer branches on this text at all
/// (see `scan_file`), so this constant is not consulted by the assertion —
/// retained only so the historical note is not lost.
#[allow(dead_code)]
const HEURISTIC_FALSE_POSITIVES: &[(&str, &str)] = &[(
    "src/alloc_core/alloc_core_small_reclaim.rs::dbg_drain_all_rings_checked",
    "*mut u8 appears only in the generic bound Fn(*mut u8, usize) -> bool, \
         not in any parameter or return type; the function takes a closure &F",
)];

// ── cfg(...) predicate parser ─────────────────────────────────────────────
//
// R30-2 fix for the R29-9 `has_bench_internals_cfg` substring-match gap: a
// SUBSTRING test on `#[cfg(...)]` text would incorrectly accept
// `not(feature = "bench-internals")` (the OPPOSITE of gating) and
// `any(feature = "bench-internals", feature = "always-on")` (a permissive
// OR, not a real gate) as if either genuinely restricted the hook. Neither
// shape is used by any current `dbg_*` hook (verified by grep before this
// fix), so there is no live false-accept today — but the check must be
// fixed structurally, not papered over. This is a small hand-written
// recursive-descent parser for exactly the grammar subset this project
// uses: `feature = "x"`, `all(...)`, `any(...)`, `not(...)`, comma-nested.
// `syn` is not a dev-dependency of this crate (checked before hand-rolling
// this — see Cargo.toml's `[dev-dependencies]`), so this avoids adding one
// just for a test-only predicate check.

/// One node of a parsed `#[cfg(...)]` predicate.
#[derive(Debug, PartialEq, Eq)]
enum CfgExpr {
    Feature(String),
    /// Any other leaf predicate this project doesn't need to reason about
    /// structurally (e.g. `not(miri)`, `target_os = "..."`) — treated as
    /// "does not require bench-internals" by construction, since it is not
    /// the `feature = "bench-internals"` leaf.
    OtherLeaf,
    All(Vec<CfgExpr>),
    Any(Vec<CfgExpr>),
    Not(Box<CfgExpr>),
}

/// Tiny hand-rolled tokenizer + recursive-descent parser for the
/// `#[cfg(...)]` predicate grammar subset used in this crate: identifiers,
/// `=`, string literals, commas, parens. Returns `None` on anything it
/// cannot parse (caller falls back to treating the predicate as opaque,
/// i.e. NOT requiring bench-internals — conservative in the direction of
/// flagging more candidates as at-risk, never fewer).
struct CfgParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> CfgParser<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            bytes: s.as_bytes(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn eat(&mut self, b: u8) -> bool {
        self.skip_ws();
        if self.peek() == Some(b) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn ident(&mut self) -> Option<String> {
        self.skip_ws();
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            None
        } else {
            Some(String::from_utf8_lossy(&self.bytes[start..self.pos]).into_owned())
        }
    }

    fn string_lit(&mut self) -> Option<String> {
        self.skip_ws();
        if self.peek() != Some(b'"') {
            return None;
        }
        self.pos += 1;
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b == b'"' {
                let s = String::from_utf8_lossy(&self.bytes[start..self.pos]).into_owned();
                self.pos += 1;
                return Some(s);
            }
            self.pos += 1;
        }
        None
    }

    /// Parse a single predicate term: `ident(...)` (all/any/not) or
    /// `ident = "value"` (feature/target_os/...) or a bare `ident`.
    fn parse_term(&mut self) -> Option<CfgExpr> {
        self.skip_ws();
        let name = self.ident()?;
        self.skip_ws();
        match self.peek() {
            Some(b'(') => {
                self.pos += 1;
                let mut children = Vec::new();
                loop {
                    self.skip_ws();
                    if self.peek() == Some(b')') {
                        break;
                    }
                    children.push(self.parse_term()?);
                    self.skip_ws();
                    if self.peek() == Some(b',') {
                        self.pos += 1;
                        continue;
                    }
                    break;
                }
                if !self.eat(b')') {
                    return None;
                }
                match name.as_str() {
                    "all" => Some(CfgExpr::All(children)),
                    "any" => Some(CfgExpr::Any(children)),
                    "not" => {
                        // `not(...)` takes exactly one child predicate.
                        let inner = children.into_iter().next()?;
                        Some(CfgExpr::Not(Box::new(inner)))
                    }
                    _ => Some(CfgExpr::OtherLeaf),
                }
            }
            Some(b'=') => {
                self.pos += 1;
                let value = self.string_lit()?;
                if name == "feature" && value == "bench-internals" {
                    Some(CfgExpr::Feature(value))
                } else {
                    Some(CfgExpr::OtherLeaf)
                }
            }
            _ => Some(CfgExpr::OtherLeaf),
        }
    }
}

/// Parse the inside of a `#[cfg(...)]` attribute (the text between the
/// outermost parens). Returns `None` if the text doesn't parse cleanly —
/// callers must treat `None` conservatively (as "not a genuine gate").
fn parse_cfg_inner(inner: &str) -> Option<CfgExpr> {
    let mut p = CfgParser::new(inner);
    let expr = p.parse_term()?;
    p.skip_ws();
    // Allow (and ignore) a single trailing comma before the closing paren
    // some rustfmt styles leave on multi-line `all(...)` bodies — already
    // consumed inside parse_term's loop, so at top level there should be
    // nothing left.
    Some(expr)
}

/// True iff `expr` UNCONDITIONALLY requires `feature = "bench-internals"` —
/// i.e. every satisfying assignment of the predicate has that feature
/// enabled. `all(...)` requires it if ANY child requires it (conjunction:
/// one required child is enough to force the feature on). `any(...)` NEVER
/// counts, even if every branch happens to require it individually, per the
/// task's explicit rule: an `any(...)`-wrapped occurrence is "not a genuine
/// gate" — this project only recognises the bare leaf or an `all(...)`
/// membership as a real gate, refusing to reward the more permissive shape
/// even in the degenerate case, since that shape is never the intended
/// pattern in this codebase and accepting it would reopen exactly the
/// "any(feature = bench-internals, X)" hole the task calls out. `not(...)`
/// never requires it (negation of anything is not the same as requiring the
/// feature).
fn requires_bench_internals(expr: &CfgExpr) -> bool {
    match expr {
        CfgExpr::Feature(name) => name == "bench-internals",
        CfgExpr::OtherLeaf => false,
        CfgExpr::All(children) => children.iter().any(requires_bench_internals),
        CfgExpr::Any(_) => false,
        CfgExpr::Not(_) => false,
    }
}

/// Extract the inner text of the FIRST complete `#[cfg(...)]` block found in
/// `text` and structurally determine whether it unconditionally requires
/// `bench-internals`. If multiple `#[cfg(...)]` attributes are present
/// (stacked attributes, which Rust treats as an implicit `all(...)` across
/// them), ANY one of them requiring `bench-internals` is sufficient — so
/// this scans every `#[cfg(...)]` block in `text` and returns `true` if any
/// one, parsed alone, requires the feature.
///
/// R31-4 (task #467, closing P2-1 filed in `docs/CORRECTNESS_OPEN_ITEMS.md`
/// item 8): matches the literal 6-byte `#[cfg(` — INCLUDING the open paren —
/// not the shorter 5-byte `#[cfg` prefix the R30-2 version used. The 5-byte
/// prefix also matches `#[cfg_attr(`, whose FIRST argument is a *predicate
/// controlling whether the attribute's SECOND argument applies to the item*
/// — not a condition on the item's own existence at all. A hypothetical
/// `#[cfg_attr(feature = "bench-internals", allow(dead_code))]` on a
/// `dbg_*` hook would, under the old 5-byte match, have its cfg-attr's
/// first argument (`feature = "bench-internals", allow(dead_code)`) fed to
/// `parse_cfg_inner`, which parses only the leading term
/// (`feature = "bench-internals"`) and ignores the trailing
/// `, allow(dead_code)` — so `requires_bench_internals` would wrongly
/// return `true`, treating the hook as gated when `cfg_attr` never gates
/// the item's compilation at all. The 6-byte `#[cfg(` match rejects
/// `#[cfg_attr(` outright (its 6th byte is `_`, not `(`), closing the gap.
/// No live hook uses this shape today (confirmed by
/// `no_dbg_hook_cfg_uses_cfg_attr_bench_internals_shape` below) — this was a
/// latent scanner gap, not an active false-accept.
fn has_bench_internals_cfg(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 5 < bytes.len() {
        if &bytes[i..i + 6] == b"#[cfg(" {
            let mut depth = 0i32;
            let mut paren_start = None;
            for j in i..bytes.len() {
                match bytes[j] {
                    b'[' => depth += 1,
                    b'(' => {
                        if paren_start.is_none() && depth == 1 {
                            paren_start = Some(j + 1);
                        }
                        depth += 1;
                    }
                    b']' => depth -= 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                if depth <= 0 && j > i {
                    if let Some(start) = paren_start {
                        // Inner text runs up to the matching `)` that closes
                        // the `cfg(...)` — that's one byte before the final
                        // `]`. Find it by trimming from the end: the last
                        // `)` before the closing `]` at position j.
                        let inner_end = text[..j].rfind(')').unwrap_or(j);
                        if inner_end > start {
                            let inner = &text[start..inner_end];
                            if let Some(expr) = parse_cfg_inner(inner) {
                                if requires_bench_internals(&expr) {
                                    return true;
                                }
                            }
                        }
                    }
                    i = j + 1;
                    break;
                }
            }
            if depth > 0 {
                i += 6;
            }
        } else {
            i += 1;
        }
    }
    false
}

// ── Source-scanning helpers ───────────────────────────────────────────────

/// Extract the full (possibly multi-line) `#[...]` attribute starting at
/// `lines[start]`. Returns the attribute text and the index of its last line.
/// Bracket-depth tracking: `#[cfg(all(` ... `))]` spans 4 lines.
fn extract_full_attribute(lines: &[&str], start: usize) -> (String, usize) {
    let mut text = String::new();
    let mut depth = 0i32;
    let mut end = start;
    for (i, line) in lines.iter().enumerate().skip(start) {
        for ch in line.chars() {
            match ch {
                '[' | '(' => depth += 1,
                ']' | ')' => depth -= 1,
                _ => {}
            }
        }
        text.push_str(line.trim());
        text.push('\n');
        end = i;
        if depth <= 0 {
            break;
        }
    }
    (text, end)
}

/// Collect the function-signature text from `lines[fn_idx]` up to and
/// including the first line containing `{` (the body delimiter).
fn collect_signature(lines: &[&str], fn_idx: usize) -> String {
    let mut sig = String::new();
    for line in lines.iter().skip(fn_idx) {
        sig.push_str(line);
        if line.contains('{') {
            break;
        }
    }
    sig
}

/// Extract the bare function name from a `pub fn dbg_foo(...)` / `pub fn
/// dbg_foo<...>(...)` / `pub unsafe fn dbg_foo(...)` line.
fn extract_fn_name(trimmed_line: &str) -> Option<&str> {
    let rest = trimmed_line
        .strip_prefix("pub unsafe fn ")
        .or_else(|| trimmed_line.strip_prefix("pub fn "))?;
    let end = rest.find(|c: char| c == '(' || c == '<' || c.is_whitespace())?;
    Some(&rest[..end])
}

/// One hook found during the scan, classified by shape.
struct Candidate {
    id: String,
    is_unsafe: bool,
    bench_internals_gated: bool,
}

/// Forward-scan one source file, returning EVERY crate-public `pub fn dbg_*`
/// / `pub unsafe fn dbg_*` — any signature shape, not just pointer-taking
/// ones (R30-2 widening) — along with whether it's `unsafe` and whether the
/// preceding attribute block gates it behind `bench-internals`.
fn scan_file(rel: &str, text: &str) -> Vec<Candidate> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    // Accumulated attribute text for the next item; cleared by doc comments,
    // other items, and blank lines (but NOT by `//` regular comments, which
    // can sit between an attribute block and the fn — see the R29-7 pattern
    // in tls_heap.rs).
    let mut attr_block = String::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();

        if trimmed.starts_with("///") || trimmed.starts_with("//!") {
            attr_block.clear();
            i += 1;
        } else if trimmed.starts_with("#[") {
            let (attr, end) = extract_full_attribute(&lines, i);
            attr_block.push_str(&attr);
            i = end + 1;
        } else if trimmed.starts_with("//") {
            i += 1;
        } else if trimmed.starts_with("pub fn dbg_") || trimmed.starts_with("pub unsafe fn dbg_") {
            let sig = collect_signature(&lines, i);
            let is_unsafe = trimmed.starts_with("pub unsafe fn ");
            if let Some(name) = extract_fn_name(trimmed) {
                out.push(Candidate {
                    id: format!("{rel}::{name}"),
                    is_unsafe,
                    bench_internals_gated: has_bench_internals_cfg(&attr_block),
                });
            }
            let _ = sig; // signature text is no longer used for shape filtering
            attr_block.clear();
            i += 1;
        } else {
            // Blank lines, other items, code — resets the attribute accumulator.
            attr_block.clear();
            i += 1;
        }
    }
    out
}

#[test]
fn safe_dbg_hooks_match_reviewed_allowlist() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));

    // Collect source trees (same scope as the README unsafe-inventory grep).
    let mut files = Vec::new();
    rs_files(&manifest.join("src"), &mut files);
    rs_files(&manifest.join("crates"), &mut files);
    assert!(!files.is_empty(), "no source files found");

    // Enumerate every crate-public dbg_* hook, bucketed by (unsafe?, gated?).
    let mut found_unsafe: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut found_safe_ungated: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    let mut gated_seen: Vec<String> = Vec::new();
    for file in &files {
        let rel = rel_id(file);
        let text =
            fs::read_to_string(file).unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        for c in scan_file(&rel, &text) {
            if c.is_unsafe {
                found_unsafe.insert(c.id);
            } else if c.bench_internals_gated {
                gated_seen.push(c.id);
            } else {
                found_safe_ungated.insert(c.id);
            }
        }
    }

    // Build the expected safe-ungated set from PURE_OBSERVERS + SAFE_MUTATORS.
    let mut expected_safe_ungated: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    for id in PURE_OBSERVERS {
        assert!(
            expected_safe_ungated.insert((*id).to_string()),
            "duplicate ID in PURE_OBSERVERS: {id}"
        );
    }
    for (id, _reason) in SAFE_MUTATORS {
        assert!(
            expected_safe_ungated.insert((*id).to_string()),
            "duplicate ID across PURE_OBSERVERS/SAFE_MUTATORS: {id}"
        );
    }

    let expected_unsafe: std::collections::BTreeSet<String> =
        UNSAFE_HOOKS.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(
        expected_unsafe.len(),
        UNSAFE_HOOKS.len(),
        "duplicate ID in UNSAFE_HOOKS"
    );

    // Diff both buckets.
    let extras_safe: Vec<_> = found_safe_ungated
        .difference(&expected_safe_ungated)
        .cloned()
        .collect();
    let missing_safe: Vec<_> = expected_safe_ungated
        .difference(&found_safe_ungated)
        .cloned()
        .collect();
    let extras_unsafe: Vec<_> = found_unsafe.difference(&expected_unsafe).cloned().collect();
    let missing_unsafe: Vec<_> = expected_unsafe.difference(&found_unsafe).cloned().collect();

    assert!(
        extras_safe.is_empty()
            && missing_safe.is_empty()
            && extras_unsafe.is_empty()
            && missing_unsafe.is_empty(),
        "dbg_* hook safety tripwire mismatch.\n\n\
         This test enforces CLAUDE.md's benchmark-hook rule (R30-2 widened, \
         shape-independent form): every crate-public `dbg_*` hook must be \
         `bench-internals`-gated UNLESS it is a reviewed pure observer or an \
         individually-justified safe mutator; every `pub unsafe fn dbg_*` \
         must be accounted for exhaustively.\n\n\
         NEW unaccounted-for SAFE, non-bench-internals-gated hooks (review \
         and classify into PURE_OBSERVERS or SAFE_MUTATORS):\n  {}\n\n\
         Listed SAFE hooks no longer found ungated in source (removed, \
         renamed, became unsafe, or became bench-internals-gated — update \
         the lists):\n  {}\n\n\
         NEW unaccounted-for UNSAFE hooks (add to UNSAFE_HOOKS):\n  {}\n\n\
         Listed UNSAFE hooks no longer found (removed, renamed, or became \
         safe — update UNSAFE_HOOKS):\n  {}\n\n\
         (For reference, {} hooks were detected as bench-internals-gated \
         and correctly excluded from the at-risk set.)",
        extras_safe.join("\n  "),
        missing_safe.join("\n  "),
        extras_unsafe.join("\n  "),
        missing_unsafe.join("\n  "),
        gated_seen.len(),
    );
}

// ── Non-vacuity self-check (R30-2) ────────────────────────────────────────
//
// Proves the widened scanner actually catches the R30-1 shape: a
// zero-argument, no-raw-pointer, safe, state-mutating hook that is neither
// bench-internals-gated nor allowlisted. Feeds `scan_file` + the allowlist
// diff logic a SCRATCH in-memory fixture (never touching real `src/`) that
// mimics `AllocCore::dbg_decomp_full_cycle`'s pre-fix shape:
// `pub fn dbg_decomp_full_cycle(&mut self) -> bool { ... }` with no cfg
// attribute at all. The old R29-9 scanner (text-scanning for `*mut`/`*const`
// in the signature) would have silently skipped this fixture, exactly as it
// silently skipped the real bug. The widened scanner must flag it as a
// found-but-unexpected safe/ungated hook.
#[test]
fn widened_scanner_catches_r30_1_shape_zero_arg_mutator() {
    let synthetic_source = r#"
impl AllocCore {
    /// Synthetic stand-in for the pre-fix R30-1 shape: no cfg gate, no raw
    /// pointer anywhere in the signature, mutates `self` state.
    pub fn dbg_decomp_full_cycle_SCRATCH_FIXTURE(&mut self) -> bool {
        let base = self.reserve_small_segment();
        self.release_or_pool_empty_segment(base);
        true
    }
}
"#;

    let candidates = scan_file("SCRATCH_FIXTURE.rs", synthetic_source);
    let hit = candidates
        .iter()
        .find(|c| c.id == "SCRATCH_FIXTURE.rs::dbg_decomp_full_cycle_SCRATCH_FIXTURE");
    let hit = hit.expect(
        "widened scanner failed to find the synthetic zero-arg mutating dbg_* hook at all — \
         non-vacuity check failed",
    );
    assert!(
        !hit.is_unsafe,
        "synthetic fixture must be classified as safe (it is `pub fn`, not `pub unsafe fn`)"
    );
    assert!(
        !hit.bench_internals_gated,
        "synthetic fixture has no #[cfg(...)] at all and must not be seen as gated"
    );

    // Confirm it is NOT in either allowlist (it never should be — it's a
    // scratch fixture id that deliberately doesn't exist in real source),
    // meaning it would surface as an "extras_safe" mismatch in the real
    // assertion above — i.e. the tripwire fires.
    let allowlisted = PURE_OBSERVERS.contains(&hit.id.as_str())
        || SAFE_MUTATORS.iter().any(|(id, _)| *id == hit.id);
    assert!(
        !allowlisted,
        "synthetic fixture id unexpectedly present in an allowlist — test fixture bug"
    );

    // This is the actual counterfactual: confirm the OLD (pre-R30-2) rule —
    // "only signatures whose text contains *mut/*const" — would have missed
    // this fixture, i.e. that the fixture's signature genuinely contains
    // neither substring. If this ever fails, the fixture stopped being
    // representative of the R30-1 shape and must be revised.
    assert!(
        !synthetic_source.contains("*mut") && !synthetic_source.contains("*const"),
        "fixture must NOT contain *mut/*const — otherwise it no longer represents \
         the R30-1 shape the old scanner would have missed"
    );
}

// ── cfg-predicate parser unit checks (R30-2) ──────────────────────────────
//
// Confirms `has_bench_internals_cfg` correctly rejects the two shapes the
// old substring check would have wrongly accepted, and still accepts the
// genuine gate shapes already used by real hooks in this crate.
#[test]
fn cfg_parser_rejects_negated_and_optional_or_bench_internals() {
    // Genuine gates — must be accepted.
    assert!(has_bench_internals_cfg(
        r#"#[cfg(feature = "bench-internals")]"#
    ));
    assert!(has_bench_internals_cfg(
        r#"#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]"#
    ));
    assert!(has_bench_internals_cfg(
        r#"#[cfg(all(
            feature = "alloc-global",
            feature = "fastbin",
            feature = "bench-internals"
        ))]"#
    ));
    // Nested all(all(...)) still requires it.
    assert!(has_bench_internals_cfg(
        r#"#[cfg(all(feature = "x", all(feature = "bench-internals", feature = "y")))]"#
    ));

    // The two shapes this fix specifically targets — must be REJECTED. The
    // OLD substring check (`attr.contains("\"bench-internals\"")`) would
    // have wrongly accepted both.
    assert!(
        !has_bench_internals_cfg(r#"#[cfg(not(feature = "bench-internals"))]"#),
        "not(feature = \"bench-internals\") is the OPPOSITE of gating and must be rejected"
    );
    assert!(
        !has_bench_internals_cfg(
            r#"#[cfg(any(feature = "bench-internals", feature = "always-on"))]"#
        ),
        "any(...) is a permissive OR, not a genuine unconditional gate, and must be rejected"
    );
    // any(...) where EVERY branch happens to require it is still rejected —
    // this project does not special-case that degenerate case (see the
    // requires_bench_internals doc comment for why).
    assert!(!has_bench_internals_cfg(
        r#"#[cfg(any(feature = "bench-internals", all(feature = "bench-internals", feature = "x")))]"#
    ));

    // A predicate that merely MENTIONS the feature string in a non-gating
    // position (e.g. inside an unrelated leaf this parser treats opaquely)
    // must not be accepted either. `target_os = "bench-internals"` is a
    // nonsensical but illustrative adversarial case for the parser's leaf
    // handling — it is a leaf, not the specific `feature = "bench-internals"`
    // leaf, so it must be rejected.
    assert!(!has_bench_internals_cfg(
        r#"#[cfg(target_os = "bench-internals")]"#
    ));

    // No #[cfg] at all.
    assert!(!has_bench_internals_cfg(""));
    assert!(!has_bench_internals_cfg("// just a comment\n"));
}

// ── P2-1 fix (R31-4, task #467) ───────────────────────────────────────────
//
// `docs/reviews/2026-07-30-r30-full-review.md` §5 P2-1 (filed as
// `docs/CORRECTNESS_OPEN_ITEMS.md` item 8): the R30-2 `has_bench_internals_cfg`
// matched the bare 5-byte prefix `#[cfg`, which ALSO matches `#[cfg_attr(`.
// `cfg_attr`'s first argument is a predicate controlling whether the
// attribute's SECOND argument applies to the item — it is not a condition
// on the item's own compilation at all, unlike `cfg(...)`. Feeding that
// first argument to `parse_cfg_inner` (as the old 5-byte match did) could
// make a `#[cfg_attr(feature = "bench-internals", allow(dead_code))]`-
// decorated hook look "gated" when it compiles unconditionally.

/// Direct unit proof that the OLD 5-byte-prefix scanner would have wrongly
/// accepted a `#[cfg_attr(feature = "bench-internals", ...)]` shape, and
/// the NEW 6-byte `#[cfg(` scanner correctly rejects it. This is the
/// counterfactual for the fix: reproduces the review's claim mechanically
/// inside this crate's own test suite rather than trusting the review's
/// external `rustc` extraction.
#[test]
fn has_bench_internals_cfg_rejects_cfg_attr_shape() {
    let cfg_attr_text = r#"#[cfg_attr(feature = "bench-internals", allow(dead_code))]"#;

    // The FIX: the current (6-byte `#[cfg(`-matching) scanner must NOT treat
    // this as a genuine gate — cfg_attr's condition does not gate the
    // item's existence, only whether `allow(dead_code)` is additionally
    // applied to it.
    assert!(
        !has_bench_internals_cfg(cfg_attr_text),
        "#[cfg_attr(feature = \"bench-internals\", ...)] must NOT be treated as a genuine \
         #[cfg(feature = \"bench-internals\")] gate — cfg_attr's first argument controls \
         whether its SECOND argument applies, not whether the item compiles at all"
    );

    // Counterfactual proving this is a real fix, not a vacuous assertion:
    // confirm the text genuinely starts with the 5-byte prefix the OLD
    // scanner matched on (so a reader can verify the OLD scanner would have
    // matched here and then mis-parsed `cfg_attr`'s first argument as if it
    // were `cfg`'s own predicate).
    assert!(
        cfg_attr_text.starts_with("#[cfg"),
        "fixture must start with the old scanner's 5-byte match prefix, or this test doesn't \
         exercise the actual regression"
    );
    assert!(
        !cfg_attr_text.starts_with("#[cfg("),
        "fixture's 6th byte must NOT be '(' (it's '_', from cfg_attr) — otherwise this fixture \
         no longer distinguishes the old 5-byte match from the new 6-byte match"
    );

    // A genuine #[cfg(...)] gate must still be accepted (regression guard
    // against a fix that overcorrects and rejects everything).
    assert!(has_bench_internals_cfg(
        r#"#[cfg(feature = "bench-internals")]"#
    ));
}

/// End-to-end proof via the REAL `scan_file` classifier (not just the
/// `has_bench_internals_cfg` unit): a synthetic `dbg_*` hook decorated with
/// `#[cfg_attr(feature = "bench-internals", ...)]` — the exact adversarial
/// shape the task describes — must be classified as UNGATED (i.e. it would
/// surface as an "extras_safe" tripwire failure requiring allowlisting, the
/// correct conservative behavior), never silently accepted as if genuinely
/// gated.
#[test]
fn scan_file_treats_cfg_attr_bench_internals_hook_as_ungated() {
    let synthetic_source = r#"
impl AllocCore {
    /// Synthetic stand-in for a hypothetical hook using cfg_attr instead of
    /// a genuine cfg gate — P2-1's adversarial shape.
    #[cfg_attr(feature = "bench-internals", allow(dead_code))]
    pub fn dbg_cfg_attr_SCRATCH_FIXTURE(&mut self) -> bool {
        true
    }
}
"#;

    let candidates = scan_file("SCRATCH_FIXTURE.rs", synthetic_source);
    let hit = candidates
        .iter()
        .find(|c| c.id == "SCRATCH_FIXTURE.rs::dbg_cfg_attr_SCRATCH_FIXTURE")
        .expect("scanner failed to find the synthetic cfg_attr-decorated hook at all");

    assert!(!hit.is_unsafe, "fixture is `pub fn`, not `pub unsafe fn`");
    assert!(
        !hit.bench_internals_gated,
        "a #[cfg_attr(feature = \"bench-internals\", ...)]-decorated hook must be classified as \
         UNGATED — cfg_attr's condition does not gate the item's compilation, so treating it as \
         gated would silently widen a production build's safe surface without the tripwire \
         noticing"
    );
}

/// Confirms no CURRENT `dbg_*` hook in the crate actually uses the
/// `#[cfg_attr(feature = "bench-internals", ...)]` shape — this was a
/// latent parser gap (per the review, no live instance), and this test
/// keeps that true going forward (it would fail the moment a hook adopted
/// the shape, forcing a human to look at it rather than silently trusting a
/// non-gating cfg_attr).
#[test]
fn no_dbg_hook_cfg_uses_cfg_attr_bench_internals_shape() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rs_files(&manifest.join("src"), &mut files);
    rs_files(&manifest.join("crates"), &mut files);

    let mut offenders = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).unwrap_or_default();
        if text.contains("cfg_attr") && text.contains("\"bench-internals\"") {
            // Coarse pre-filter (cheap); confirm precisely via the same
            // attr-block/hook-scan walk the other structural tests use.
            let lines: Vec<&str> = text.lines().collect();
            let mut attr_block = String::new();
            let mut i = 0;
            while i < lines.len() {
                let trimmed = lines[i].trim_start();
                if trimmed.starts_with("///") || trimmed.starts_with("//!") {
                    attr_block.clear();
                    i += 1;
                } else if trimmed.starts_with("#[") {
                    let (attr, end) = extract_full_attribute(&lines, i);
                    attr_block.push_str(&attr);
                    i = end + 1;
                } else if trimmed.starts_with("//") {
                    i += 1;
                } else if trimmed.starts_with("pub fn dbg_")
                    || trimmed.starts_with("pub unsafe fn dbg_")
                {
                    if attr_block.contains("cfg_attr") && attr_block.contains("\"bench-internals\"")
                    {
                        if let Some(name) = extract_fn_name(trimmed) {
                            offenders.push(format!("{}::{name}: {attr_block}", rel_id(file)));
                        }
                    }
                    attr_block.clear();
                    i += 1;
                } else {
                    attr_block.clear();
                    i += 1;
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "found dbg_* hook(s) using #[cfg_attr(..., \"bench-internals\", ...)] — this shape does \
         NOT gate the item's compilation the way #[cfg(feature = \"bench-internals\")] does; \
         review and convert to a genuine #[cfg(...)] gate:\n  {}",
        offenders.join("\n  ")
    );
}

/// Confirms no CURRENT `dbg_*` hook in the crate actually uses either of the
/// two shapes the old substring check would have wrongly accepted — this
/// was a latent parser weakness, not a live false-accept, and this test
/// keeps that true going forward (it would fail the moment a hook adopted
/// either shape, forcing a human to look at it rather than silently trusting
/// a permissive gate).
#[test]
fn no_dbg_hook_cfg_uses_negated_or_optional_or_bench_internals_shape() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rs_files(&manifest.join("src"), &mut files);
    rs_files(&manifest.join("crates"), &mut files);

    let mut offenders = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        let mut attr_block = String::new();
        let mut i = 0;
        while i < lines.len() {
            let trimmed = lines[i].trim_start();
            if trimmed.starts_with("///") || trimmed.starts_with("//!") {
                attr_block.clear();
                i += 1;
            } else if trimmed.starts_with("#[") {
                let (attr, end) = extract_full_attribute(&lines, i);
                attr_block.push_str(&attr);
                i = end + 1;
            } else if trimmed.starts_with("//") {
                i += 1;
            } else if trimmed.starts_with("pub fn dbg_")
                || trimmed.starts_with("pub unsafe fn dbg_")
            {
                // A hook "mentions" bench-internals if the feature string
                // appears anywhere in its attribute text but the STRUCTURAL
                // parser (has_bench_internals_cfg) says it is NOT genuinely
                // gated — i.e. the only occurrences are inside a `not(...)`
                // or a non-unconditional `any(...)` branch. This mirrors
                // exactly the distinction the old R29-9 substring check
                // could not draw.
                let mentions_feature = attr_block.contains("\"bench-internals\"");
                let structurally_gated = has_bench_internals_cfg(&attr_block);
                if mentions_feature && !structurally_gated {
                    if let Some(name) = extract_fn_name(trimmed) {
                        offenders.push(format!("{}::{name}: {attr_block}", rel_id(file)));
                    }
                }
                attr_block.clear();
                i += 1;
            } else {
                attr_block.clear();
                i += 1;
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "found dbg_* hook(s) whose #[cfg] mentions bench-internals inside a not(...) or \
         any(...) shape — these need individual review, not blind trust in the substring \
         match:\n  {}",
        offenders.join("\n  ")
    );
}
