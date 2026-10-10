// Inventory includes public const dbg functions, suffix test hooks, injections,
// and the explicitly named deliberate root forwarders below.
// Executable security gate migrated from the R30-2 reviewed hook tripwire.
// Every crate-public dbg_* hook, regardless of signature shape, must be a
// genuinely bench-internals-gated safe hook, a reviewed pure observer, an
// individually justified safe mutator, or an exhaustively reviewed unsafe hook.
// These are the ONE authoritative reviewed tables. Surviving invariant reasons
// are preserved verbatim; only explicitly retired definitions were pruned.
// Unsafe measurement hooks require their real gate and # Safety contract.
// No source-copy test, auto-pruning, wildcard allowlist or cfg_attr exemption.
// Run: node scripts/verify-dbg-hook-safety.mjs

import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { basename, dirname, join, relative } from 'node:path';
import { pathToFileURL } from 'node:url';
import { REPO_ROOT } from './lib.mjs';

// Pure observers: read-only, no allocator-state mutation.
const PURE_OBSERVERS = [
  "crates/numa-shim/src/lib.rs::dbg_current_node_for_cpu",
  "crates/numa-shim/src/lib.rs::dbg_node_resolution_for_cpu",
  "crates/once-ptr-cell/src/imp.rs::dbg_is_ready",
  "src/alloc_core/alloc_core/alloc_core_core_diag/totals.rs::dbg_foreign_or_unroutable_frees",
  "src/alloc_core/alloc_core/alloc_core_core_diag/totals.rs::dbg_segments_reserved_total",
  "src/alloc_core/alloc_core/alloc_core_core_diag/totals.rs::dbg_segments_released_total",
  "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_segments_reserve_failed_total",
  "src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs::dbg_node_id_for",
  "src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs::dbg_page_map_class_for",
  "src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs::dbg_table_count",
  "src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs::dbg_contains_base",
  "src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs::dbg_hash_remove_max_scan_steps",
  "src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs::dbg_recycle_unverified_base_total",
  "src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs::dbg_hash_contains_only",
  "src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs::dbg_segment_id_of",
  "src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs::dbg_kind_byte_of",
  "src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs::dbg_kind_at_tag",
  "src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs::dbg_large_size_of",
  "src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs::dbg_span_usable_of",
  "src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs::dbg_reserved_capacity_of",
  "src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs::dbg_block_size",
  "src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs::dbg_small_class_count",
  "src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs::dbg_layout_class_for",
  "src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs::dbg_large_zero_pass_count",
  "src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs::dbg_small_zero_pass_count",
  "src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs::dbg_payload_virgin_for",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_directory_hits",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_directory_stale_hits",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_directory_fallback_scans",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_directory_words_examined",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_opt_h_attempts",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_opt_h_hits",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_reloc_inplace_large_count",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_reloc_inplace_small_count",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_reloc_fastpath_decline_count",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_promotion_count",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_promotion_bytes_sum",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_promotion_bytes_min",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_promotion_bytes_max",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_promotion_bytes_hist",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_full_scan_slots_examined",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_directory_authoritative_miss",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_directory_miss_self_heal",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_routed_miss_scans",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_routed_miss_scan_drain_created_free",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_routed_miss_scan_bin_already_nonempty",
  "src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs::dbg_routed_miss_scan_nothing",
  "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_directory_rescue_oom_avoided",
  "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_directory_is_materialised",
  "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_directory_get_bit",
  "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_directory_get_bit_for_node",
  "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_directory_node_bitmaps",
  "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_directory_node_bucket_for",
  "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_directory_active_bits_for_bucket",
  "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_directory_get_bit_bucket",
  "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_directory_materialize_threshold",
  "src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs::dbg_max_segments",
  "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_words_per_class",
  "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_directory_miss_streak_for_class",
  "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_directory_miss_full_scan_period",
  "src/alloc_core/large/alloc_core_large_cache.rs::dbg_large_cache_extended_slot_sizes",
  "src/alloc_core/large/alloc_core_large_cache.rs::dbg_large_cache_extension_materialised",
  "src/alloc_core/large/alloc_core_large_cache.rs::dbg_large_cache_total_slots",
  "src/alloc_core/large/alloc_core_large_cache_eviction.rs::dbg_decay_config",
  "src/alloc_core/large/alloc_core_large_cache_eviction.rs::dbg_large_cache_used",
  "src/alloc_core/large/alloc_core_large_cache_eviction.rs::dbg_large_cache_hits",
  "src/alloc_core/large/alloc_core_large_cache_eviction.rs::dbg_large_cache_slot_sizes",
  "src/alloc_core/large/alloc_core_large_cache_eviction.rs::dbg_large_cache_occupied_bits",
  "src/alloc_core/large/alloc_core_large_cache_eviction.rs::dbg_large_cache_budget",
  "src/alloc_core/large/alloc_core_large_cache_eviction.rs::dbg_large_cache_mode",
  "src/alloc_core/platform/sidecar_stats.rs::dbg_sidecar_reservation_stats",
  "src/alloc_core/alloc_core/state.rs::dbg_cached_numa_node",
  "src/alloc_core/small/alloc_core_small_diag.rs::dbg_freelist_head_for",
  "src/alloc_core/small/alloc_core_small_diag.rs::dbg_is_free_for",
  "src/alloc_core/small/alloc_core_small_diag.rs::dbg_committed_payload_end_for",
  "src/alloc_core/small/alloc_core_small_diag.rs::dbg_grow_commit_count",
  "src/alloc_core/small/alloc_core_small_diag.rs::dbg_grow_chunk",
  "src/alloc_core/small/alloc_core_small_diag.rs::dbg_lazy_first_chunk",
  "src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs::dbg_decommit_count",
  "src/alloc_core/small/alloc_core_small_diag.rs::dbg_live_count_for",
  "src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs::dbg_pooled_count",
  "src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs::dbg_pool_cap",
  "src/alloc_core/small/alloc_core_small_pool/decommit.rs::dbg_is_decommitted_for",
  "src/global/fallback.rs::dbg_fallback_lock_acquisitions",
  // Pure production-helper transition on a by-value u32; no shared state,
  // allocator metadata, pointers or scheduler action; internals + bench-internals gated.
  "src/global/fallback.rs::dbg_fallback_lock_spin_transition",
  "src/global/fallback.rs::dbg_init_state",
  "src/registry/bootstrap/registry.rs::dbg_chunk_is_materialised",
  "src/registry/heap_core/core.rs::dbg_cached_numa_node",
  "src/registry/heap_core/diag/queries.rs::dbg_owner_id_for",
  "src/registry/heap_core/diag/queries.rs::dbg_class_for",
  "src/registry/heap_core/diag/queries.rs::dbg_refill_n_for_class",
  "src/registry/heap_core/diag/queries.rs::dbg_directory_bit_for_ptr",
  "src/registry/heap_core/diag/queries.rs::dbg_pooled_count",
  "src/registry/heap_core/diag/queries.rs::dbg_segment_base_of_ptr",
  "src/registry/heap_core/diag/queries.rs::dbg_live_count_for",
  "src/registry/heap_core/diag/queries.rs::dbg_tcache_virgin_mask",
  "src/registry/heap_core/diag/queries.rs::dbg_tcache_count",
  "src/registry/heap_core/diag/diag_probes.rs::dbg_hardened_large_noop_count",
  "src/registry/heap_core/diag/diag_probes.rs::dbg_contains_base",
  "src/registry/heap_core/diag/diag_probes.rs::dbg_hash_contains_only",
  "src/registry/heap_core/diag/queries.rs::dbg_last_stamped_segment",
  "src/registry/heap_core/diag/queries.rs::dbg_kind_at_tag",
  "src/registry/heap_core/diag/queries.rs::dbg_table_count",
  "src/registry/bootstrap/ensure.rs::dbg_slot_or_none"
];

// Safe mutators: each retained reviewed invariant justification is authoritative.
const SAFE_MUTATORS = [
  ["src/registry/bootstrap/registry.rs::dbg_slot_state", "Registry::slot may materialize a chunk and abort on OOM; then reads atomic state, without borrowing the core"],
  ["src/registry/bootstrap/registry.rs::dbg_slot_generation", "Registry::slot may materialize a chunk and abort on OOM; then reads atomic generation, without borrowing the core"],
  ["src/registry/heap_registry/counters.rs::dbg_slot_initialised", "Registry::slot may materialize a chunk and abort on OOM; then reads initialization state, without borrowing the core"],
  [
    "src/registry/heap_registry/claim.rs::dbg_claim_lease",
    "test-only forwarder to real claim CAS; may increment generation and materialize/bind HeapCore on first claim; " +
      "returns a typed HeapLease whose Drop publishes LIVE->FREE; `core(&mut self)` is exclusive and lease-bound; no raw pointer (Ph4a, task #2091)"
  ],
  [
    "src/registry/heap_registry/claim.rs::dbg_claim_lease_with_config",
    "test-only forwarder to the real claim_lease_with_config path (same claim_impl CAS protocol and N2 config-conflict hook as HeapRegistry::claim_with_config); like dbg_claim_lease its exclusive core(&mut self) borrow is tied to the CAS-owned lease; it returns a typed HeapLease (Drop = Release LIVE->FREE), takes a LargeCacheConfig by value and no raw pointers (Ph4c, task #2107)"
  ],
  [
    "src/registry/heap_registry/claim.rs::dbg_try_maintenance",
    "test-only forwarder to HeapRegistry::try_maintenance after the Ph4c surface narrowing made it pub(crate) (ADR addendum section 2.6); same FREE->MAINTENANCE CAS as the production path, returns the typed MaintenanceLease whose Drop restores FREE, takes no pointer; dbg_with_core lends an exclusive &mut HeapCore tied to the won FREE->MAINTENANCE CAS and mutable lease borrow"
  ],
  [
    "src/registry/heap_registry/claim.rs::dbg_with_core",
    "test-only forwarder to MaintenanceLease::with_core after the Ph4c surface narrowing made it pub(crate) (ADR addendum section 2.6); the callback receives the exclusive &mut HeapCore guarded by the lease's won FREE->MAINTENANCE CAS, and the alias adds no access beyond the crate-only original"
  ],
  [
    "crates/once-ptr-cell/src/imp.rs::dbg_rollback_reenterable",
    "entry CAS is a point-in-time UNINIT check, not mutual exclusion across the whole probe; the final restore is gated on the probe's own postcondition CAS re-winning the cell, so a concurrent get_or_try_init racing in mid-probe is never clobbered"
  ],
  [
    "crates/sefer-region/src/region.rs::dbg_try_mint_region_id",
    "test-only forwarder to the real region_id-minting CAS logic (task #813), but it takes an explicit `&AtomicUsize` parameter and never touches the real `NEXT_REGION_ID` process-wide static -- the only thing it mutates is whatever local counter the caller constructed and owns (integration tests use a fresh stack-local AtomicUsize per test), so there is no shared/allocator state and no soundness relevance"
  ],
  [
    "src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs::dbg_reset_hash_remove_max_scan_steps",
    "resets a diagnostic high-water counter only; no allocator metadata, no soundness relevance"
  ],
  [
    "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_find_segment_with_free",
    "calls the real production find_segment_with_free lookup directly; same invariant envelope as its normal caller"
  ],
  [
    "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_directory_force_clear_bit",
    "directory bits are a soft self-healing cache with an always-available O(S) fallback scan; a stale-clear bit costs a slower scan, never UB or a wrong pointer"
  ],
  [
    "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_directory_rescue_scan",
    "invokes the real OOM-rescue scan production code path directly; not a bypass"
  ],
  [
    "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_directory_reset_miss_streak",
    "zeroes a heuristic miss-streak counter; wrong value only changes re-validation cadence, never correctness"
  ],
  [
    "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_directory_set_miss_streak_for_class",
    "sets a heuristic miss-streak counter; wrong value only changes re-validation cadence, never correctness"
  ],
  [
    "src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs::dbg_rebuild_directory",
    "re-derives the directory bitmap from the authoritative SegmentTable/BinTable state, not an arbitrary write"
  ],
  [
    "src/alloc_core/large/alloc_core_large_cache_eviction.rs::dbg_force_decay_tick",
    "rewinds the decay timer then calls the real maybe_decay_large_cache production path; eviction logic itself unchanged"
  ],
  [
    "src/alloc_core/large/alloc_core_large_cache_eviction.rs::dbg_set_decay_config",
    "overwrites decay-heuristic tuning knobs only; no allocator-metadata/pointer correctness implication"
  ],
  [
    "src/alloc_core/large/alloc_core_large_cache_eviction.rs::dbg_set_large_cache_budget",
    "overwrites a byte-budget policy field; wrong value changes eviction aggressiveness, never soundness"
  ],
  [
    "src/alloc_core/alloc_core/state.rs::dbg_current_node_cached",
    "calls the real production current_node_cached NUMA-cache-populate path directly; not a bypass"
  ],
  [
    "src/alloc_core/alloc_core/state.rs::dbg_invalidate_numa_node_cache",
    "calls the real production invalidate_numa_node_cache; NUMA cache is a soft hint, not correctness metadata"
  ],
  [
    "src/alloc_core/small/alloc_core_small_diag.rs::dbg_carve_batch",
    "calls the real production carve_batch path; identical bump/bitmap mutation to an ordinary alloc_small carve"
  ],
  [
    "src/alloc_core/small/alloc_core_small_diag.rs::dbg_arm_commit_fail",
    "arms a process-global fault injector for future OS commit_pages calls; simulates an already-handled production error path"
  ],
  [
    "src/alloc_core/small/alloc_core_small_diag.rs::dbg_arm_commit_fail_at",
    "arms the Nth-call fault injector; same justification as dbg_arm_commit_fail"
  ],
  [
    "src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs::dbg_drain_small_pool",
    "calls the real production drain_small_pool teardown-trim primitive directly (also called from trim_for_recycle)"
  ],
  [
    "src/global/fallback.rs::dbg_panic_in_with_heap_releases_lock",
    "drives a real panic through the production with_heap/LockGuard RAII path; exercises, does not bypass, existing safe machinery"
  ],
  [
    "src/global/tls_heap.rs::dbg_teardown_then_resolve_is_fallback",
    "poisons then restores TLS LOCAL via the real mark_local_torn fn, bracketed within the call; net-zero from caller's perspective"
  ],
  [
    "src/global/tls_heap.rs::dbg_teardown_then_resolve_is_foreign_no_bind",
    "same poison/resolve/restore bracket as dbg_teardown_then_resolve_is_fallback"
  ],
  [
    "src/registry/bootstrap/ensure.rs::dbg_rollback_chunk_sentinel_reenterable",
    "entry CAS proves the chunk cell UNINIT before touching it; restores to null before returning"
  ],
  [
    "src/registry/bootstrap/saturation.rs::dbg_with_word_for_test",
    "constructs only a local atomic hint for boundary models; never touches the process-global registry"
  ],
  [
    "src/registry/heap_registry/counters.rs::dbg_claim_then_simulate_oom",
    "claims a real slot via the production pick_slot/CAS/push_back_after_oom path; reproduces, does not invent, the real post-OOM rollback state"
  ],
  [
    "src/registry/heap_registry/counters.rs::dbg_bump_count_without_materialising",
    "R2-11 (task #2013): delegates to the real production bump_count (a single AtomicU32::fetch_add on Registry::count, the identical op pick_slot/claim always perform); the minted index is never pushed onto free_slots and never claimed, matching count_for_test's already-accepted 'count only ever grows across the suite' cost -- bounded, inert bookkeeping, cannot produce UB or hand out a wrong pointer"
  ],
  [
    "src/registry/heap_core/state/tcache_flush.rs::dbg_flush_all",
    "calls the real production flush_all_tcache path used by ordinary teardown"
  ],
  [
    "src/registry/heap_core/core.rs::dbg_populate_numa_cache_for_test",
    "calls the real production current_node_cached path via AllocCore; NUMA cache is a soft hint"
  ],
  [
    "src/registry/heap_core/diag/queries.rs::dbg_drain_small_pool",
    "delegates to the real production AllocCore::drain_small_pool teardown-trim primitive (same as trim_for_recycle)"
  ],
  [
    "src/registry/bootstrap/ensure.rs::dbg_set_inject_chunk_oom",
    "sets a plain AtomicBool test-injection flag only; no allocator metadata, no raw pointers, no soundness relevance — the flag makes ensure_chunk_slow's closure return None early, simulating an already-handled production OOM path"
  ],
  [
    "src/global/fallback.rs::dbg_set_inject_fallback_init_panic",
    "sets a plain AtomicBool test-injection flag only; no allocator metadata, no raw pointers, no soundness relevance — the flag makes heap_ptr panic before HeapCore::new, simulating an unwind out of the InitStateGuard'd region (R34-17/task #536)"
  ],
  [
    "src/global/fallback.rs::dbg_panic_in_fallback_init_rolls_back",
    "drives a real panic through the production heap_ptr/InitStateGuard RAII path then re-calls heap_ptr to prove the state rolled back; exercises, does not bypass, existing safe machinery (R34-17/task #536)"
  ]
];

// Caller-owned unsafe boundaries, exhaustively accounted for.
const CURRENT_RESERVATION_HOOK = "src/global/sefer_alloc/diag.rs::dbg_current_reservation_for_test";
const UNSAFE_HOOKS = [
  "src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs::dbg_stamp_segment_id",
  "src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs::dbg_stamp_kind_byte",
  "src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs::dbg_unregister",
  "src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs::dbg_recycle",
  "src/alloc_core/small/alloc_core_small_diag.rs::dbg_drain_freelist_batch",
  "src/alloc_core/small/alloc_core_small_diag.rs::dbg_alloc_bitmap_bytes_for",
  "src/alloc_core/small/alloc_core_small_diag.rs::dbg_magazine_bitmap_bytes_for",
  "src/alloc_core/small/alloc_core_small_diag.rs::dbg_corrupt_freelist_head_next",
  "src/alloc_core/small/alloc_core_small_diag.rs::dbg_payload_start_for",
  "src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs::dbg_decomp_decommit_payload",
  "src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs::dbg_decomp_recommit_payload",
  "src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs::dbg_decomp_release",
  "src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs::dbg_decomp_win_commit_only",
  "src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs::dbg_decomp_win_release_only",
  "src/alloc_core/small/alloc_core_small_pool/decommit.rs::dbg_force_decommit_retain_for",
  "src/global/sefer_alloc/global_alloc.rs::dbg_dealloc_while_fallback_lock_held",
  "src/global/tls_heap.rs::dbg_restore_local_for_test",
  "src/registry/bootstrap/registry.rs::dbg_slot_preset_generation",
  "src/registry/heap_core/diag/diag_probes.rs::dbg_dealloc_own_thread_with_base",
  "src/registry/heap_core/diag/diag_probes.rs::dbg_flush_class_only",
  "src/registry/heap_core/diag/diag_probes.rs::dbg_clear_magazine_on_hit",
  "src/registry/heap_core/diag/diag_probes.rs::dbg_decomp_decommit_payload",
  "src/registry/heap_core/diag/diag_probes.rs::dbg_decomp_recommit_payload",
  "src/registry/heap_core/diag/diag_probes.rs::dbg_decomp_release",
  "src/registry/heap_core/diag/diag_probes.rs::dbg_decomp_win_commit_only",
  "src/registry/heap_core/diag/diag_probes.rs::dbg_decomp_win_release_only",
  "src/alloc_core/alloc_core/sidecar_test_hooks.rs::dbg_publish_small_sidecar_free",
  "src/registry/heap_core_xthread/sidecar_drain.rs::dbg_publish_small_sidecar_free",
  "src/registry/heap_core_xthread/sidecar_drain.rs::dbg_reclaim_sidecar_record_for_test",
  CURRENT_RESERVATION_HOOK
];
const SIDECAR_UNSAFE_HOOKS = new Set(UNSAFE_HOOKS.filter(id => /::dbg_(?:publish_small_sidecar_free|reclaim_sidecar_record_for_test)$/.test(id)));
const BENCH_UNSAFE_HOOKS = new Set(UNSAFE_HOOKS.filter(id => /::dbg_(?:decomp_|dealloc_own_thread_with_base|flush_class_only|clear_magazine_on_hit|force_decommit_retain_for|restore_local_for_test|dealloc_while_fallback_lock_held|publish_small_sidecar_free|reclaim_sidecar_record_for_test)/.test(id)));
BENCH_UNSAFE_HOOKS.add(CURRENT_RESERVATION_HOOK);

// Explicit reviewed rows; groups share only a reviewed contract, never a name wildcard.
export const REVIEWED_SURFACE = [
  { id: 'src/registry/heap_core/diag/queries.rs::dbg_sidecar_scan_measurement', kind: 'mutator', reason: 'thread-local optional diagnostic counts and original-versus-prefilter scan mode only; no pointers or allocator metadata access', gates: ['bench-internals', 'r18_sidecar_scan_bench'] },
  ...[
    ['src/alloc_core/segment/segment_layout.rs', ['small_decommit_start', 'primordial_decommit_start', 'small_lazy_initial_commit', 'primordial_lazy_initial_commit'], 'observer', 'pure geometry forwarders'],
    ['src/alloc_core/small/alloc_core_small_magazine.rs', ['refill_class', 'refill_class_bump', 'refill_class_bump_virgin'], 'mutator', 'delegates to allocation substrate; exclusive mutable core borrow'],
    ['src/concurrent/epoch/epoch_region.rs', ['_remote_free_queue_buffer_identity_for_tests'], 'observer', 'locked buffer identity only; no dereference permission'],
    ['src/concurrent/epoch/epoch_region.rs', ['_set_slot_generation_for_tests'], 'mutator', 'vacant-slot generation boundary; mutable region borrow'],
    ['src/concurrent/sharded/sharded_region.rs', ['_reset_my_shard_binding_for_tests'], 'mutator', 'clears TLS routing cache, not occupied tokens; may consume additional claims'],
    ['src/concurrent/sharded/sharded_region.rs', ['_remote_free_queue_buffer_identity_for_tests', '_live_token_blocks_for_tests', '_tls_claim_count_for_tests', '_prune_claim_checks_for_tests'], 'observer', 'identity and atomic/TLS count observations'],
    ['src/registry/bootstrap/ensure.rs', ['dbg_num_chunks'], 'observer', 'constant geometry'],
    ['src/registry/heap_core/diag/diag_probes.rs', ['dbg_promotion_compiled'], 'observer', 'compile-time predicate'],
    ['src/global/maintenance_service.rs', ['passes_for_test', 'fallback_visits_for_test'], 'observer', 'atomic diagnostic counters'],
    ['src/global/maintenance_service.rs', ['wait_after_for_test'], 'mutator', 'locks/waits on production completion condition'],
    ['src/registry/segment_route/directory.rs', ['fail_next_registration_for_test'], 'mutator', 'TLS registration failure countdown; handled fallible path'],
    ['src/registry/segment_route/directory.rs', ['with_initial_incarnation_for_test'], 'mutator', 'constructs isolated directory with chosen initial incarnation'],
    ['src/registry/segment_route/directory.rs', ['retained_pointer_capacity_for_test', 'moved_pointer_cells_for_test', 'shard_index_for_test', 'live_route_census_for_test'], 'observer', 'directory capacity/census under its locks'],
    ['src/registry/segment_route/small_sidecar.rs', ['class_at_for_test', 'mixed_leaves_for_test', 'system_totals_for_test'], 'observer', 'sidecar class/counter observations'],
    ['src/registry/segment_route/registration.rs', ['class_at_global_address_for_test'], 'observer', 'lookup pins independent sidecar before class read'],
    ['src/registry/segment_route/large_state.rs', ['word_for_test'], 'observer', 'atomic terminal state snapshot'],
    ['src/registry/segment_route/pin.rs', ['pending_for_test'], 'observer', 'pinned sidecar pending state'],
    ['src/alloc_core/platform/numa.rs', ['biased_root_offset_for_test'], 'observer', 'pure checked geometry'],
    ['src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs', ['large_geometry_for_test'], 'observer', 'validated owned header geometry'],
  ].flatMap(([file, names, kind, reason]) => names.map(name => ({ id: `${file}::${name}`, kind, reason, gates: ['internals'] }))),
  ...[
    ['src/global/maintenance_service.rs', ['try_fallback_step_for_test', 'fail_next_start_for_test', 'pause_start_for_test', 'wait_start_paused_for_test', 'fail_worker_for_test'], 'mutator', 'bounded maintenance step or explicit startup/worker fault injection; synchronization retained'],
    ['src/registry/segment_route/directory.rs', ['fail_next_registrations_for_test'], 'mutator', 'TLS registration failure countdown; handled fallible path'],
    ['src/registry/segment_route/small_sidecar.rs', ['fail_spill_after_for_test', 'fail_next_spills_for_test', 'fail_prepare_after_for_test'], 'mutator', 'atomic fallible preparation/spill fault injection'],
    ['src/registry/segment_route/small_sidecar.rs', ['spill_failures_remaining_for_test'], 'observer', 'atomic injection countdown'],
    ['src/registry/segment_route/small_sidecar.rs', ['owner_state_for_test'], 'unsafe', 'caller owns mapped reservation; raw metadata snapshot'],
    ['src/alloc_core/large/alloc_core_large_cache_eviction.rs', ['terminal_cached_large_state_for_test'], 'observer', 'owner cache terminal snapshot'],
    ['src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs', ['terminal_header_layout_for_test', 'terminal_header_snapshot_for_test', 'terminal_primordial_snapshot_for_test', 'terminal_next_generation_for_test'], 'observer', 'checked header layout/state and generation geometry'],
    ['src/concurrent/sharded/sharded_region.rs', ['_set_remote_free_hint_for_tests'], 'mutator', 'advisory hint only; real queue remains authoritative'],
  ].flatMap(([file, names, kind, reason]) => names.map(name => ({ id: `${file}::${name}`, kind, reason, gates: ['internals', 'bench-internals'] }))),
  ...[
    ['src/concurrent/lock_free/lock_free_region.rs', ['_forge_handle_for_tests', '_page_arc_clones_for_tests', '_checked_total_slots_for_tests'], 'observer', 'by-value handle/geometry or TLS counter; ordinary operations still validate handles'],
    ['src/concurrent/lock_free/lock_free_region.rs', ['_reset_page_arc_clones_for_tests'], 'mutator', 'TLS diagnostic counter only'],
  ].flatMap(([file, names, kind, reason]) => names.map(name => ({ id: `${file}::${name}`, kind, reason, gates: ['bench-internals'] }))),
  { id: 'src/alloc_core/segment/segment_table/harness.rs::hide_root_for_test', kind: 'mutator', reason: 'isolated numeric harness; no dereference; external alloc_core module path requires internals, not a compilation gate', gates: [] },
  { id: 'src/alloc_core/small/alloc_core_small_magazine.rs::flush_class', kind: 'unsafe', reason: 'exclusive caller-owned live blocks, exactly once; delegates to flush substrate', gates: ['internals'] },
  { id: 'src/registry/bootstrap/ensure.rs::count_for_test', kind: 'mutator', reason: 'ensure may initialize registry; then monotonic count load', gates: [] },
  { id: 'src/registry/heap_registry/maintenance.rs::maintenance_pass_for_test', kind: 'mutator', reason: 'production CAS-owned bounded maintenance pass', gates: ['internals'] },
  ...[
    ['backoff_spin_count_for_test', 'observer', ['loom']],
    ['retry_counts_for_test', 'observer', ['test-model']],
    ['backoff_spin_depths_for_test', 'observer', ['test-model']],
    ['StackHead::with_tag_for_test', 'mutator', ['test-model']],
    ['StackHead::cas_head_for_test', 'mutator', ['loom']],
    ['ArrayIndexStack::cas_head_for_test', 'mutator', ['loom']],
    ['ArrayIndexStack::load_next_for_test', 'observer', ['test-model']],
    ['ArrayIndexStack::store_next_for_test', 'unsafe', ['loom']],
    ['ArrayIndexStack::with_tag_for_test', 'mutator', ['test-model']],
  ].map(([name, kind, gates]) => ({ id: `crates/tagged-index-stack/src/imp.rs::${name}`, kind, gates, reason: 'isolated verification stack/model surface; raw link write requires its caller contract' })),
];
const ROOT_FORWARDERS = [
  'src/alloc_core/small/alloc_core_small_magazine.rs::refill_class',
  'src/alloc_core/small/alloc_core_small_magazine.rs::refill_class_bump',
  'src/alloc_core/small/alloc_core_small_magazine.rs::refill_class_bump_virgin',
  'src/alloc_core/small/alloc_core_small_magazine.rs::flush_class',
  'src/alloc_core/segment/segment_layout.rs::small_decommit_start',
  'src/alloc_core/segment/segment_layout.rs::primordial_decommit_start',
  'src/alloc_core/segment/segment_layout.rs::small_lazy_initial_commit',
  'src/alloc_core/segment/segment_layout.rs::primordial_lazy_initial_commit',
];

export function rustMask(text) {
  const out = text.split('');
  const blank = (a, b) => { for (let j = a; j < b; j++) if (out[j] !== '\n' && out[j] !== '\r') out[j] = ' '; };
  for (let i = 0; i < text.length;) {
    const start = i;
    if (text.startsWith('//', i)) { const end = text.indexOf('\n', i); i = end < 0 ? text.length : end; blank(start, i); }
    else if (text.startsWith('/*', i)) {
      let depth = 1; i += 2;
      while (i < text.length && depth) { if (text.startsWith('/*', i)) { depth++; i += 2; } else if (text.startsWith('*/', i)) { depth--; i += 2; } else i++; }
      if (depth) throw new Error('unterminated Rust block comment');
      blank(start, i);
    } else {
      const raw = /^(?:b)?r(#{0,255})"/.exec(text.slice(i));
      if (raw) { const end = text.indexOf(`"${raw[1]}`, i + raw[0].length); if (end < 0) throw new Error('unterminated Rust raw string'); i = end + 1 + raw[1].length; blank(start, i); }
      else if (text[i] === '"') { i++; while (i < text.length) { if (text[i] === '\\') i += 2; else if (text[i++] === '"') break; } blank(start, i); }
      else { const char = /^'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|[^\r\n])|[^'\\\r\n])'/.exec(text.slice(i)); if (char) { i += char[0].length; blank(start, i); } else i++; }
    }
  }
  return out.join('');
}

// The policy deliberately accepts bare bench-internals and all(...), never
// not(...), any(...) or cfg_attr. Unknown/malformed predicates fail closed.
function parseCfg(inner) {
  const tokens = inner.match(/"(?:\\.|[^"\\])*"|[A-Za-z_][\w-]*|[(),=]|\S/g) ?? [];
  let pos = 0;
  function term() {
    const name = tokens[pos++];
    if (!/^[A-Za-z_][\w-]*$/.test(name ?? '')) throw new Error('invalid cfg identifier');
    if (tokens[pos] === '=') { pos++; const value = tokens[pos++]; if (!value?.startsWith('"')) throw new Error('invalid cfg value'); return { name, value: JSON.parse(value) }; }
    if (tokens[pos] !== '(') return { name };
    pos++; const children = [];
    while (tokens[pos] !== ')') { children.push(term()); if (tokens[pos] !== ',') break; pos++; }
    if (tokens[pos++] !== ')') throw new Error('unterminated cfg');
    if (name === 'not' && children.length !== 1) throw new Error('invalid not arity');
    return { name, children };
  }
  try { const result = term(); if (pos !== tokens.length) return null; return result; } catch { return null; }
}
// Resolve root Cargo implications rather than hardcoding a second feature
// convention: alloc-global already requires alloc-xthread in the manifest.
const featureSection = /^\[features\]\s*\n([\s\S]*?)(?=^\[|(?![\s\S]))/m.exec(readFileSync(join(REPO_ROOT, 'Cargo.toml'), 'utf8'))?.[1];
if (featureSection === undefined) throw new Error('missing root Cargo feature definitions');
const featureGraph = new Map([...featureSection.matchAll(/^\s*([\w-]+)\s*=\s*\[([\s\S]*?)\]/gm)].map(match => [match[1], [...match[2].matchAll(/"([^"]+)"/g)].map(value => value[1])]));
function featureImplies(from, required, seen = new Set()) {
  if (from === required) return true;
  if (seen.has(from)) return false;
  return (featureGraph.get(from) ?? []).some(child => featureGraph.has(child) && featureImplies(child, required, new Set([...seen, from])));
}
function expressionRequires(expr, feature) {
  if (feature === 'loom' || feature === 'r18_sidecar_scan_bench') return !!expr && ((expr.name === feature && !expr.children) || (expr.name === 'all' && expr.children.some(child => expressionRequires(child, feature))));
  if (feature === 'test-model') return !!expr && ((['loom', 'tagged_index_stack_test'].includes(expr.name) && !expr.children) || (expr.name === 'any' && expr.children.length > 0 && expr.children.every(child => expressionRequires(child, feature))) || (expr.name === 'all' && expr.children.some(child => expressionRequires(child, feature))));
  return !!expr && ((expr.name === 'feature' && featureImplies(expr.value, feature)) || (expr.name === 'all' && expr.children.some(child => expressionRequires(child, feature))));
}
function cfgInner(attr) { return /^#!?\[\s*cfg\s*\(([\s\S]*)\)\s*\]$/.exec(attr)?.[1]; }
export function attributesRequire(attrs, feature) { return attrs.some(attr => { const inner = cfgInner(attr); return inner !== undefined && expressionRequires(parseCfg(inner), feature); }); }
function invalidBenchAttribute(attr) {
  if (!/^#!?\[\s*cfg(?:_attr)?\s*\(/.test(attr) || !/feature\s*=\s*"bench-internals"/.test(attr)) return false;
  const inner = cfgInner(attr);
  return inner === undefined || !expressionRequires(parseCfg(inner), 'bench-internals');
}
function safetyDocs(text, offset) {
  const lines = text.slice(0, offset).split(/\r?\n/);
  let first = lines.length - 1;
  while (first >= 0 && (!lines[first].trim() || /^\s*\/\//.test(lines[first]))) first--;
  return lines.slice(first + 1).join('\n');
}
export function scanFile(id, text) {
  const mask = rustMask(text);
  // Rust's canonical order is const unsafe; also inventory unsafe const
  // defensively so a malformed declaration cannot silently hide a hook.
  const token = /#(!?)\s*\[|\bpub\s+(?:(const)\s+)?(unsafe\s+)?(?:const\s+)?fn\s+(\w+)\b|\b(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*([;{])|[{};]/g;
  const globals = [], scopes = [], hooks = [], modules = [], moduleScopes = [];
  let pending = [], pendingStart = null;
  for (let match; (match = token.exec(mask));) {
    const inherited = () => [...globals, ...scopes.flat(), ...pending];
    if (match[0].startsWith('#')) {
      let end = token.lastIndex, depth = 1;
      while (end < mask.length && depth) { if (mask[end] === '[') depth++; else if (mask[end] === ']') depth--; end++; }
      if (depth) throw new Error(`${id}: unterminated attribute`);
      const attr = text.slice(match.index, end);
      if (match[1] === '!' && scopes.length === 0) globals.push(attr); else { pending.push(attr); pendingStart ??= match.index; }
      token.lastIndex = end;
    } else if (match[4]) {
      const name = match[4];
      const impls = [...mask.slice(0, match.index).matchAll(/\bimpl(?:\s*<[^{}]*?>)?\s+(\w+)[^{]*\{/g)];
      const lastImpl = impls.at(-1);
      const implTail = lastImpl ? mask.slice(lastImpl.index, match.index) : '';
      const owner = lastImpl && [...implTail].reduce((n, ch) => n + (ch === '{' ? 1 : ch === '}' ? -1 : 0), 0) > 0 ? lastImpl[1] : null;
      const hookId = `${id}::${id === 'crates/tagged-index-stack/src/imp.rs' && owner ? `${owner}::` : ''}${name}`;
      if (/^dbg_|_for_tests?$|inject/.test(name) || ROOT_FORWARDERS.includes(hookId))
        hooks.push({ id: hookId, name, owner, file: id, unsafe: !!match[3], attrs: inherited(), docs: safetyDocs(text, pendingStart ?? match.index) });
    } else if (match[5]) {
      if (match[6] === '{') { scopes.push(pending); moduleScopes.push(match[5]); }
      else modules.push({ name: match[5], attrs: inherited(), inline: moduleScopes.filter(Boolean) });
      pending = []; pendingStart = null;
    } else if (match[0] === '{') { scopes.push(pending); moduleScopes.push(null); pending = []; pendingStart = null; }
    else if (match[0] === '}') { scopes.pop(); moduleScopes.pop(); pending = []; pendingStart = null; }
    else { pending = []; pendingStart = null; }
  }
  return { globals, hooks, modules };
}
export function listRust(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap(entry => { const path = join(dir, entry.name); return entry.isDirectory() ? listRust(path) : entry.isFile() && entry.name.endsWith('.rs') ? [path] : []; });
}
export function idFor(path) { return relative(REPO_ROOT, path).split('\\').join('/'); }
export function moduleTarget(parent, module, files, roots = new Set()) {
  const explicit = module.attrs.map(attr => /^#\[\s*path\s*=\s*"([^"\n]+)"\s*\]$/.exec(attr)?.[1]).find(Boolean);
  const dir = dirname(parent);
  const stem = basename(parent, '.rs');
  const base = roots.has(parent) || ['mod', 'lib', 'main'].includes(stem) ? dir : join(dir, stem);
  const inline = module.inline ?? [];
  if (explicit) { const id = join(inline.length ? join(base, ...inline) : dir, explicit).split('\\').join('/'); return files.has(id) ? id : null; }
  const moduleBase = join(base, ...inline);
  return [join(moduleBase, `${module.name}.rs`), join(moduleBase, module.name, 'mod.rs')].map(path => path.split('\\').join('/')).find(path => files.has(path)) ?? null;
}
function unique(ids, label, errors) {
  const result = new Set();
  for (const id of ids) { if (result.has(id)) errors.push(`duplicate ${label}: ${id}`); result.add(id); }
  return result;
}
export function effectiveFileGates(files, roots = new Set()) {
  const incoming = new Map();
  for (const [parent, file] of files) for (const module of file.modules) {
    const target = moduleTarget(parent, module, files, roots);
    if (target) { if (!incoming.has(target)) incoming.set(target, []); incoming.get(target).push({ parent, attrs: module.attrs }); }
  }
  function requires(file, feature, seen = new Set()) {
    if (seen.has(file)) return false;
    if (attributesRequire(files.get(file).globals, feature)) return true;
    // A crate root is also compiled independently of any module declaration.
    if (roots.has(file)) return false;
    const edges = incoming.get(file);
    return !!edges?.length && edges.every(edge => attributesRequire(edge.attrs, feature) || requires(edge.parent, feature, new Set([...seen, file])));
  }
  return requires;
}
export function verifyHooks(files, policy = {}) {
  const errors = [];
  const expectedSafe = unique(policy.safe ?? [...PURE_OBSERVERS, ...SAFE_MUTATORS.map(([id]) => id), ...REVIEWED_SURFACE.filter(row => row.kind !== 'unsafe').map(row => row.id)], 'reviewed safe hook', errors);
  for (const [id, reason] of SAFE_MUTATORS) if (!reason.trim()) errors.push(`missing invariant justification: ${id}`);
  const expectedUnsafe = unique(policy.unsafe ?? [...UNSAFE_HOOKS, ...REVIEWED_SURFACE.filter(row => row.kind === 'unsafe').map(row => row.id)], 'reviewed unsafe hook', errors);
  for (const id of expectedUnsafe) if (expectedSafe.has(id)) errors.push(`safe/unsafe classification overlaps: ${id}`);
  unique((policy.surface ?? REVIEWED_SURFACE).map(row => row.id), 'reviewed surface row', errors);
  for (const row of policy.surface ?? REVIEWED_SURFACE) if (!row.reason?.trim()) errors.push(`missing invariant justification: ${row.id}`);
  const requiresFile = effectiveFileGates(files);
  const foundSafe = new Set(), foundUnsafe = new Set();
  let gated = 0;
  for (const file of files.values()) for (const hook of file.hooks) {
    const requires = feature => attributesRequire(hook.attrs, feature) || requiresFile(hook.file, feature);
    const row = (policy.surface ?? REVIEWED_SURFACE).find(row => row.id === hook.id);
    if (row) for (const feature of row.gates) if (!requires(feature)) errors.push(`reviewed hook missing ${feature} gate: ${hook.id}`);
    for (const attr of hook.attrs) if (invalidBenchAttribute(attr)) errors.push(`non-gating bench cfg/cfg_attr: ${hook.id}: ${attr}`);
    if (hook.id === 'src/global/fallback.rs::dbg_fallback_lock_spin_transition') for (const feature of ['internals', 'bench-internals']) if (!requires(feature)) errors.push(`pure transition missing ${feature} gate: ${hook.id}`);
    if (hook.unsafe) {
      foundUnsafe.add(hook.id);
      if (!/^\s*\/\/\/\s*# Safety\s*$/m.test(hook.docs)) errors.push(`unsafe hook lacks its # Safety contract: ${hook.id}`);
      if (BENCH_UNSAFE_HOOKS.has(hook.id) && !requires('bench-internals')) errors.push(`measurement-only unsafe hook is not bench-internals-gated: ${hook.id}`);
      if (hook.id === CURRENT_RESERVATION_HOOK && !requires('internals')) errors.push(`reservation observer is not internals-gated: ${hook.id}`);
      if (SIDECAR_UNSAFE_HOOKS.has(hook.id)) for (const feature of ['alloc-global', 'alloc-xthread', 'internals', 'bench-internals']) if (!requires(feature)) errors.push(`terminal producer hook missing ${feature} gate: ${hook.id}`);
    } else if (requires('bench-internals')) {
      gated++;
      if (expectedSafe.has(hook.id)) foundSafe.add(hook.id);
      else if (row || !/^dbg_/.test(hook.name)) foundSafe.add(hook.id);
    } else foundSafe.add(hook.id);
  }
  for (const [found, expected, label] of [[foundSafe, expectedSafe, 'safe ungated'], [foundUnsafe, expectedUnsafe, 'unsafe']]) {
    for (const id of found) if (!expected.has(id)) errors.push(`unreviewed ${label} hook: ${id}`);
    for (const id of expected) if (!found.has(id)) errors.push(`stale ${label} allowlist entry (review required, never auto-pruned): ${id}`);
  }
  return { errors, safe: foundSafe.size, unsafe: foundUnsafe.size, gated };
}
// Hand-authored oracle: declarations, owners, classifications and required gates
// below are literal expectations, not derived from REVIEWED_SURFACE rows.
function handAuthoredCheck() {
  const check = (file, source, expected) => {
    const parsed = scanFile(file, source);
    assert.deepEqual(parsed.hooks.map(h => [h.name, h.owner, h.unsafe]), expected);
    // A partial fixture necessarily leaves unrelated allowlist entries stale.
    const errors = verifyHooks(new Map([[file, parsed]])).errors.filter(e => !e.startsWith('stale '));
    assert.deepEqual(errors, []);
    return parsed;
  };
  const sidecar = 'src/registry/segment_route/small_sidecar.rs';
  const ownerSource = `impl SmallSidecar {
    /// # Safety
    /// Caller exclusively owns the live mapped reservation.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    pub unsafe fn owner_state_for_test(address: usize, class: usize) -> Option<(usize, u32, u32)> { None }
  }`;
  check(sidecar, ownerSource, [['owner_state_for_test', 'SmallSidecar', true]]);
  const sidecarErrors = source => verifyHooks(new Map([[sidecar, scanFile(sidecar, source)]])).errors;
  assert(sidecarErrors(ownerSource.replace('pub unsafe fn', 'pub fn')).includes(`unreviewed safe ungated hook: ${sidecar}::owner_state_for_test`));
  assert(sidecarErrors(ownerSource.replace('/// # Safety', '/// Contract')).includes(`unsafe hook lacks its # Safety contract: ${sidecar}::owner_state_for_test`));
  assert(sidecarErrors(ownerSource.replace('all(feature = "internals", feature = "bench-internals")', 'feature = "internals"')).includes(`reviewed hook missing bench-internals gate: ${sidecar}::owner_state_for_test`));
  assert(sidecarErrors(ownerSource.replace('all(feature = "internals", feature = "bench-internals")', 'feature = "bench-internals"')).includes(`reviewed hook missing internals gate: ${sidecar}::owner_state_for_test`));

  const chunks = 'src/registry/bootstrap/ensure.rs';
  check(chunks, '#[cfg(feature = "internals")]\npub const fn dbg_num_chunks() -> usize { 4 }', [['dbg_num_chunks', null, false]]);
  assert(verifyHooks(new Map([[chunks, scanFile(chunks, 'pub const fn dbg_num_chunks() -> usize { 4 }')]])).errors.includes(`reviewed hook missing internals gate: ${chunks}::dbg_num_chunks`));
  const probes = 'src/registry/heap_core/diag/diag_probes.rs';
  check(probes, 'impl HeapCore { #[cfg(feature = "internals")] pub const fn dbg_promotion_compiled() -> bool { false } }', [['dbg_promotion_compiled', 'HeapCore', false]]);
  assert(verifyHooks(new Map([[probes, scanFile(probes, 'impl HeapCore { pub const fn dbg_promotion_compiled() -> bool { false } }')]])).errors.includes(`reviewed hook missing internals gate: ${probes}::dbg_promotion_compiled`));

  const magazine = 'src/alloc_core/small/alloc_core_small_magazine.rs';
  const magazineSource = `#[cfg(feature = "internals")]
  impl AllocCore {
    pub fn refill_class(&mut self, class_idx: usize, want: usize, out: &mut [*mut u8]) -> usize { 0 }
    pub fn refill_class_bump(&mut self, class_idx: usize, out: &mut [*mut u8]) -> usize { 0 }
    #[cfg(all(feature = "alloc-xthread", feature = "fastbin", feature = "virgin-zero-skip"))]
    pub fn refill_class_bump_virgin(&mut self, class_idx: usize, out: &mut [*mut u8], virgin_out: &mut u16) -> usize { 0 }
    /// # Safety
    /// Caller owns each live block exactly once.
    pub unsafe fn flush_class(&mut self, class_idx: usize, blocks: &[*mut u8]) {}
  }`;
  check(magazine, magazineSource, [['refill_class', 'AllocCore', false], ['refill_class_bump', 'AllocCore', false], ['refill_class_bump_virgin', 'AllocCore', false], ['flush_class', 'AllocCore', true]]);
  assert.deepEqual(verifyHooks(new Map([[magazine, scanFile(magazine, magazineSource.replace('#[cfg(feature = "internals")]', ''))]])).errors.filter(e => e.startsWith('reviewed hook missing ')), [
    `reviewed hook missing internals gate: ${magazine}::refill_class`,
    `reviewed hook missing internals gate: ${magazine}::refill_class_bump`,
    `reviewed hook missing internals gate: ${magazine}::refill_class_bump_virgin`,
    `reviewed hook missing internals gate: ${magazine}::flush_class`,
  ]);

  const layout = 'src/alloc_core/segment/segment_layout.rs';
  const layoutSource = `#[cfg(feature = "internals")]
  impl SegmentLayout {
    pub fn small_decommit_start() -> usize { 0 }
    pub fn primordial_decommit_start() -> usize { 0 }
    #[cfg(any(feature = "primordial-lazy-commit", feature = "small-segment-lazy-commit"))]
    pub fn small_lazy_initial_commit(page_size: usize) -> usize { page_size }
    #[cfg(any(feature = "primordial-lazy-commit", feature = "small-segment-lazy-commit"))]
    pub fn primordial_lazy_initial_commit(page_size: usize) -> usize { page_size }
  }`;
  check(layout, layoutSource, [['small_decommit_start', 'SegmentLayout', false], ['primordial_decommit_start', 'SegmentLayout', false], ['small_lazy_initial_commit', 'SegmentLayout', false], ['primordial_lazy_initial_commit', 'SegmentLayout', false]]);
  assert.deepEqual(verifyHooks(new Map([[layout, scanFile(layout, layoutSource.replace('#[cfg(feature = "internals")]', ''))]])).errors.filter(e => e.startsWith('reviewed hook missing ')), [
    `reviewed hook missing internals gate: ${layout}::small_decommit_start`,
    `reviewed hook missing internals gate: ${layout}::primordial_decommit_start`,
    `reviewed hook missing internals gate: ${layout}::small_lazy_initial_commit`,
    `reviewed hook missing internals gate: ${layout}::primordial_lazy_initial_commit`,
  ]);

  const epoch = 'src/concurrent/epoch/epoch_region.rs';
  check(epoch, `#[cfg(feature = "internals")] impl EpochRegion<T> {
    pub fn _set_slot_generation_for_tests(&mut self, index: u32, generation: u32) {}
    pub fn _remote_free_queue_buffer_identity_for_tests(&self) -> (usize, usize, usize) { (0, 0, 0) }
  }`, [['_set_slot_generation_for_tests', 'EpochRegion', false], ['_remote_free_queue_buffer_identity_for_tests', 'EpochRegion', false]]);
  assert.deepEqual(verifyHooks(new Map([[epoch, scanFile(epoch, `impl EpochRegion<T> {
    pub fn _set_slot_generation_for_tests(&mut self, index: u32, generation: u32) {}
    pub fn _remote_free_queue_buffer_identity_for_tests(&self) -> (usize, usize, usize) { (0, 0, 0) }
  }`)]] )).errors.filter(e => e.startsWith('reviewed hook missing ')), [
    `reviewed hook missing internals gate: ${epoch}::_set_slot_generation_for_tests`,
    `reviewed hook missing internals gate: ${epoch}::_remote_free_queue_buffer_identity_for_tests`,
  ]);
  const sharded = 'src/concurrent/sharded/sharded_region.rs';
  check(sharded, `#[cfg(feature = "internals")] impl ShardedRegion<T> {
    pub fn _reset_my_shard_binding_for_tests() {}
    pub fn _remote_free_queue_buffer_identity_for_tests(&self, shard: u16) -> Option<(usize, usize, usize)> { None }
    #[cfg(feature = "bench-internals")]
    pub fn _set_remote_free_hint_for_tests(&self, shard: u16, pending: bool) -> bool { false }
  }`, [['_reset_my_shard_binding_for_tests', 'ShardedRegion', false], ['_remote_free_queue_buffer_identity_for_tests', 'ShardedRegion', false], ['_set_remote_free_hint_for_tests', 'ShardedRegion', false]]);
  assert(verifyHooks(new Map([[sharded, scanFile(sharded, '#[cfg(feature = "internals")] impl ShardedRegion<T> { pub fn _set_remote_free_hint_for_tests(&self, shard: u16, pending: bool) -> bool { false } }')]])).errors.includes(`reviewed hook missing bench-internals gate: ${sharded}::_set_remote_free_hint_for_tests`));

  assert.deepEqual(verifyHooks(new Map([[sharded, scanFile(sharded, `impl ShardedRegion<T> {
    pub fn _reset_my_shard_binding_for_tests() {}
    pub fn _remote_free_queue_buffer_identity_for_tests(&self, shard: u16) -> Option<(usize, usize, usize)> { None }
    #[cfg(feature = "bench-internals")]
    pub fn _set_remote_free_hint_for_tests(&self, shard: u16, pending: bool) -> bool { false }
  }`)]] )).errors.filter(e => e.startsWith('reviewed hook missing ')), [
    `reviewed hook missing internals gate: ${sharded}::_reset_my_shard_binding_for_tests`,
    `reviewed hook missing internals gate: ${sharded}::_remote_free_queue_buffer_identity_for_tests`,
    `reviewed hook missing internals gate: ${sharded}::_set_remote_free_hint_for_tests`,
  ]);
  // Actual bootstrap module/file relationship, with no item-level gate.
  const parent = 'src/registry/bootstrap/mod.rs';
  const files = new Map([
    [parent, scanFile(parent, '#[cfg(feature = "internals")] mod ensure;')],
    [chunks, scanFile(chunks, 'pub const fn dbg_num_chunks() -> usize { 4 }')],
  ]);
  assert.deepEqual(verifyHooks(files).errors.filter(e => !e.startsWith('stale ')), []);
  files.set(parent, scanFile(parent, '#[cfg(any(feature = "internals", feature = "production"))] mod ensure;'));
  assert(verifyHooks(files).errors.includes(`reviewed hook missing internals gate: ${chunks}::dbg_num_chunks`));
  files.set(parent, scanFile(parent, '#[cfg(feature = "internals")] mod ensure;'));
  files.set('src/registry/bootstrap/alternate.rs', scanFile('src/registry/bootstrap/alternate.rs', '#[path = "ensure.rs"] mod ensure;'));
  assert(verifyHooks(files).errors.includes(`reviewed hook missing internals gate: ${chunks}::dbg_num_chunks`));

  // const unsafe is used by globalalloc-model; unsafe const is inventoried
  // defensively, not asserted to be a valid Rust qualifier order.
  for (const declaration of ['pub const unsafe fn', 'pub unsafe const fn']) {
    const parsed = scanFile('fixture.rs', `impl Core {\n/// # Safety\n/// Caller owns the allocation.\n${declaration} dbg_const_unsafe() {} }`);
    assert.deepEqual(parsed.hooks.map(h => [h.name, h.unsafe]), [['dbg_const_unsafe', true]]);
    assert.deepEqual(verifyHooks(new Map([['fixture.rs', parsed]]), { safe: [], unsafe: ['fixture.rs::dbg_const_unsafe'], surface: [] }).errors, []);
  }
}
function selfCheck() {
  handAuthoredCheck();
  const fixturePolicy = { safe: [], unsafe: [], surface: [] };
  for (const name of ['dbg_const', 'value_for_test', 'value_for_tests', 'inject_fault']) {
    const file = scanFile('fixture.rs', `impl Core { pub const fn ${name}() {} }`);
    assert.equal(file.hooks.length, 1);
    assert(verifyHooks(new Map([['fixture.rs', file]]), fixturePolicy).errors.includes(`unreviewed safe ungated hook: fixture.rs::${name}`));
  }
  // Structural coverage only: these generated cases share the policy's rows
  // and therefore cannot serve as an independent classification/gating oracle.
  for (const row of REVIEWED_SURFACE) {
    const split = row.id.lastIndexOf('::'), name = row.id.slice(split + 2);
    const qualified = row.id.startsWith('crates/tagged-index-stack/src/imp.rs::');
    const file = qualified ? 'crates/tagged-index-stack/src/imp.rs' : row.id.slice(0, split);
    const owner = qualified && row.id.slice(file.length + 2).includes('::') ? row.id.slice(file.length + 2, split) : 'Core';
    const docs = row.kind === 'unsafe' ? '/// # Safety\n/// Caller owns the mapped allocation.\n' : '';
    const declaration = `${docs}pub ${row.kind === 'unsafe' ? 'unsafe ' : ''}fn ${name}() {}`;
    const skeleton = gates => `${gates}\nimpl ${owner} {\n${declaration}\n}`;
    const attrs = row.gates.map(gate => gate === 'test-model' ? '#[cfg(any(tagged_index_stack_test, loom))]' : gate === 'loom' ? '#[cfg(loom)]' : `#[cfg(feature = "${gate}")]`);
    const policy = { safe: row.kind === 'unsafe' ? [] : [row.id], unsafe: row.kind === 'unsafe' ? [row.id] : [], surface: [row] };
    // Free functions in the model crate must not accidentally acquire an owner.
    const source = gates => qualified && owner === 'Core' ? `${gates}\n${declaration}` : skeleton(gates);
    assert.deepEqual(verifyHooks(new Map([[file, scanFile(file, source(attrs.join('\n')))]]), policy).errors, []);
    for (let index = 0; index < attrs.length; index++) {
      const mutant = attrs.filter((_, i) => i !== index).join('\n');
      assert(verifyHooks(new Map([[file, scanFile(file, source(mutant))]]), policy).errors.includes(`reviewed hook missing ${row.gates[index]} gate: ${row.id}`));
    }
    if (row.kind === 'unsafe') {
      const noDocs = source(attrs.join('\n')).replace(docs, '');
      assert(verifyHooks(new Map([[file, scanFile(file, noDocs)]]), policy).errors.includes(`unsafe hook lacks its # Safety contract: ${row.id}`));
    }
  }
  const duplicatePolicy = { safe: ['fixture.rs::dbg_dup', 'fixture.rs::dbg_dup'], unsafe: [], surface: [] };
  assert(verifyHooks(new Map(), duplicatePolicy).errors.includes('duplicate reviewed safe hook: fixture.rs::dbg_dup'));
  assert(verifyHooks(new Map(), duplicatePolicy).errors.includes('stale safe ungated allowlist entry (review required, never auto-pruned): fixture.rs::dbg_dup'));
  assert(verifyHooks(new Map(), { safe: [], unsafe: ['fixture.rs::dbg_retired'], surface: [] }).errors.includes('stale unsafe allowlist entry (review required, never auto-pruned): fixture.rs::dbg_retired'));
  const duplicateUnsafe = { safe: [], unsafe: ['fixture.rs::dbg_dup', 'fixture.rs::dbg_dup'], surface: [] };
  assert(verifyHooks(new Map(), duplicateUnsafe).errors.includes('duplicate reviewed unsafe hook: fixture.rs::dbg_dup'));
  const duplicateRow = { id: 'fixture.rs::dbg_dup', kind: 'observer', gates: [], reason: 'pure local value' };
  assert(verifyHooks(new Map(), { safe: [], unsafe: [], surface: [duplicateRow, duplicateRow] }).errors.includes('duplicate reviewed surface row: fixture.rs::dbg_dup'));
  const mustGate = ['#[cfg(feature = "bench-internals")]', '#[cfg(all(feature = "x", all(feature = "bench-internals", feature = "y")))]'];
  const mustReject = ['', '#[cfg(not(feature = "bench-internals"))]', '#[cfg(any(feature = "bench-internals", feature = "x"))]', '#[cfg(any(feature = "bench-internals", all(feature = "bench-internals", feature = "x")))]', '#[cfg_attr(feature = "bench-internals", allow(dead_code))]', '#[cfg(target_os = "bench-internals")]', '#[cfg(feature = "bench-internals", nonsense)]'];
  for (const attr of mustGate) assert(attributesRequire([attr], 'bench-internals'), attr);
  for (const attr of mustReject) assert(!attributesRequire([attr], 'bench-internals'), attr);
  const zeroArg = scanFile('fixture.rs', 'impl Core { pub fn dbg_mutate(&mut self) { self.clear(); } }');
  assert.equal(zeroArg.hooks.length, 1); assert(!attributesRequire(zeroArg.hooks[0].attrs, 'bench-internals'));
  assert(verifyHooks(new Map([['fixture.rs', zeroArg]])).errors.includes('unreviewed safe ungated hook: fixture.rs::dbg_mutate'));
  const optional = scanFile('fixture.rs', '#[cfg_attr(feature = "bench-internals", allow(dead_code))]\npub fn dbg_mutate() {}');
  assert(invalidBenchAttribute(optional.hooks[0].attrs[0]));
  const inherited = scanFile('fixture.rs', '#[cfg(all(feature = "x", feature = "bench-internals"))]\nimpl Core { pub fn dbg_mutate() {} }');
  assert(attributesRequire(inherited.hooks[0].attrs, 'bench-internals'));
  assert.equal(scanFile('fixture.rs', '// pub fn dbg_comment() {}\nconst X: &str = r#"pub fn dbg_raw_string() {}"#;\n/* pub fn dbg_block() {} */').hooks.length, 0);
  const safety = scanFile('fixture.rs', '/// # Safety\n/// Must own the allocation.\n#[cfg(feature = "bench-internals")]\npub unsafe fn dbg_publish(ptr: *mut u8) {}');
  assert(safety.hooks[0].unsafe); assert(safety.hooks[0].docs.includes('# Safety'));
  const parent = scanFile('fixture/mod.rs', '#[cfg(feature = "bench-internals")]\nmod child;');
  const child = scanFile('fixture/child.rs', 'pub fn dbg_mutate() {}');
  assert.equal(child.hooks.length, 1);
  assert(!verifyHooks(new Map([['fixture/mod.rs', parent], ['fixture/child.rs', child]])).errors.includes('unreviewed safe ungated hook: fixture/child.rs::dbg_mutate'));
  const alternate = scanFile('fixture/alternate.rs', '#[path = "child.rs"]\nmod child;');
  assert(verifyHooks(new Map([['fixture/mod.rs', parent], ['fixture/alternate.rs', alternate], ['fixture/child.rs', child]])).errors.includes('unreviewed safe ungated hook: fixture/child.rs::dbg_mutate'));
  const fakeSafety = scanFile('fixture.rs', 'const DOC: &str = "/// # Safety";\npub unsafe fn dbg_unsafe() {}');
  assert(verifyHooks(new Map([['fixture.rs', fakeSafety]]), { safe: [], unsafe: ['fixture.rs::dbg_unsafe'], surface: [] }).errors.includes('unsafe hook lacks its # Safety contract: fixture.rs::dbg_unsafe'));
  const observerFile = 'src/global/sefer_alloc/diag.rs';
  const observer = scanFile(observerFile, '/// # Safety\n/// Caller owns the live allocation.\n#[cfg(feature = "internals")]\npub unsafe fn dbg_current_reservation_for_test(ptr: *mut u8) {}');
  const observerErrors = verifyHooks(new Map([[observerFile, observer]])).errors;
  assert(observerErrors.includes(`measurement-only unsafe hook is not bench-internals-gated: ${CURRENT_RESERVATION_HOOK}`));
  const observerBenchOnly = scanFile(observerFile, '/// # Safety\n/// Caller owns the live allocation.\n#[cfg(feature = "bench-internals")]\npub unsafe fn dbg_current_reservation_for_test(ptr: *mut u8) {}');
  assert(verifyHooks(new Map([[observerFile, observerBenchOnly]])).errors.includes(`reservation observer is not internals-gated: ${CURRENT_RESERVATION_HOOK}`));
  const pureFile = 'src/global/fallback.rs';
  const pureId = `${pureFile}::dbg_fallback_lock_spin_transition`;
  const pureHook = scanFile(pureFile, '#[cfg(all(feature = "internals", feature = "bench-internals"))]\nimpl SeferAlloc { pub fn dbg_fallback_lock_spin_transition(spins: u32) -> (u32, bool) { (spins, false) } }');
  assert.equal(pureHook.hooks.length, 1);
  for (const feature of ['internals', 'bench-internals']) assert(attributesRequire(pureHook.hooks[0].attrs, feature));
  for (const gate of ['internals', 'bench-internals']) {
    const remaining = gate === 'internals' ? 'bench-internals' : 'internals';
    const missing = scanFile(pureFile, `#[cfg(feature = "${remaining}")]\npub fn dbg_fallback_lock_spin_transition() {}`);
    assert(verifyHooks(new Map([[pureFile, missing]])).errors.includes(`pure transition missing ${gate} gate: ${pureId}`));
  }
  const pureResult = verifyHooks(new Map([[pureFile, pureHook], ['fixture.rs', zeroArg]]));
  assert.equal(pureResult.safe, 2);
  assert.equal(pureResult.gated, 1);
  assert(!pureResult.errors.some(error => error.includes(pureId)));
  assert(pureResult.errors.includes('unreviewed safe ungated hook: fixture.rs::dbg_mutate'));
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
if (process.argv.includes('--self-test')) { selfCheck(); console.log('[verify-dbg-hook-safety] parser adversarial self-checks PASS'); }
else {
  const files = new Map([...listRust(join(REPO_ROOT, 'src')), ...listRust(join(REPO_ROOT, 'crates'))].map(path => { const id = idFor(path); return [id, scanFile(id, readFileSync(path, 'utf8'))]; }));
  if (!files.size) throw new Error('no Rust source files found');
  const result = verifyHooks(files);
  if (result.errors.length) { console.error(`[verify-dbg-hook-safety] FAIL\n${result.errors.join('\n')}`); process.exitCode = 1; }
  else console.log(`[verify-dbg-hook-safety] PASS: ${result.safe} reviewed safe, ${result.unsafe} reviewed unsafe, ${result.gated} genuinely bench-gated safe hooks`);
}
}
