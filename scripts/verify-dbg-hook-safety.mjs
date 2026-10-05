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
  "src/global/fallback.rs::dbg_init_state",
  "src/registry/bootstrap/registry.rs::dbg_slot_state",
  "src/registry/bootstrap/registry.rs::dbg_slot_generation",
  "src/registry/bootstrap/registry.rs::dbg_chunk_is_materialised",
  "src/registry/heap_registry/counters.rs::dbg_slot_initialised",
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
  [
    "src/registry/heap_registry/claim.rs::dbg_claim_lease",
    "test-only forwarder to the real claim CAS protocol (same claim_impl as HeapRegistry::claim); it returns the typed HeapLease whose Drop is the Release LIVE->FREE publication, takes no pointer and exposes no core access (core() is pub(crate)) -- the only effect is a slot state transition that the lease's own Drop reverses (Ph4a, task #2091)"
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

function rustMask(text) {
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
  return !!expr && ((expr.name === 'feature' && featureImplies(expr.value, feature)) || (expr.name === 'all' && expr.children.some(child => expressionRequires(child, feature))));
}
function cfgInner(attr) { return /^#!?\[\s*cfg\s*\(([\s\S]*)\)\s*\]$/.exec(attr)?.[1]; }
function attributesRequire(attrs, feature) { return attrs.some(attr => { const inner = cfgInner(attr); return inner !== undefined && expressionRequires(parseCfg(inner), feature); }); }
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
function scanFile(id, text) {
  const mask = rustMask(text);
  const token = /#(!?)\s*\[|\bpub\s+(unsafe\s+)?fn\s+(dbg_\w+)\b|\b(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;|[{};]/g;
  const globals = [], scopes = [], hooks = [], modules = [];
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
    } else if (match[3]) {
      hooks.push({ id: `${id}::${match[3]}`, file: id, unsafe: !!match[2], attrs: inherited(), docs: safetyDocs(text, pendingStart ?? match.index) });
    } else if (match[4]) {
      modules.push({ name: match[4], attrs: inherited() }); pending = []; pendingStart = null;
    } else if (match[0] === '{') { scopes.push(pending); pending = []; pendingStart = null; }
    else if (match[0] === '}') { scopes.pop(); pending = []; pendingStart = null; }
    else { pending = []; pendingStart = null; }
  }
  return { globals, hooks, modules };
}
function listRust(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap(entry => { const path = join(dir, entry.name); return entry.isDirectory() ? listRust(path) : entry.isFile() && entry.name.endsWith('.rs') ? [path] : []; });
}
function idFor(path) { return relative(REPO_ROOT, path).split('\\').join('/'); }
function moduleTarget(parent, module, files) {
  const explicit = module.attrs.map(attr => /^#\[\s*path\s*=\s*"([^"\n]+)"\s*\]$/.exec(attr)?.[1]).find(Boolean);
  const dir = dirname(parent);
  if (explicit) { const id = join(dir, explicit).split('\\').join('/'); return files.has(id) ? id : null; }
  const stem = basename(parent, '.rs');
  const base = ['mod', 'lib', 'main'].includes(stem) ? dir : join(dir, stem);
  return [join(base, `${module.name}.rs`), join(base, module.name, 'mod.rs')].map(path => path.split('\\').join('/')).find(path => files.has(path)) ?? null;
}
function unique(ids, label, errors) {
  const result = new Set();
  for (const id of ids) { if (result.has(id)) errors.push(`duplicate ${label}: ${id}`); result.add(id); }
  return result;
}
function verifyHooks(files) {
  const errors = [];
  const expectedSafe = unique([...PURE_OBSERVERS, ...SAFE_MUTATORS.map(([id]) => id)], 'reviewed safe hook', errors);
  for (const [id, reason] of SAFE_MUTATORS) if (!reason.trim()) errors.push(`missing invariant justification: ${id}`);
  const expectedUnsafe = unique(UNSAFE_HOOKS, 'reviewed unsafe hook', errors);
  for (const id of expectedUnsafe) if (expectedSafe.has(id)) errors.push(`safe/unsafe classification overlaps: ${id}`);
  const incoming = new Map();
  for (const [parent, file] of files) for (const module of file.modules) { const target = moduleTarget(parent, module, files); if (target) { if (!incoming.has(target)) incoming.set(target, []); incoming.get(target).push({ parent, attrs: module.attrs }); } }
  function fileRequires(file, feature, seen = new Set()) {
    if (seen.has(file)) return false;
    if (attributesRequire(files.get(file).globals, feature)) return true;
    const edges = incoming.get(file);
    return !!edges?.length && edges.every(edge => attributesRequire(edge.attrs, feature) || fileRequires(edge.parent, feature, new Set([...seen, file])));
  }
  const foundSafe = new Set(), foundUnsafe = new Set();
  let gated = 0;
  for (const file of files.values()) for (const hook of file.hooks) {
    const requires = feature => attributesRequire(hook.attrs, feature) || fileRequires(hook.file, feature);
    for (const attr of hook.attrs) if (invalidBenchAttribute(attr)) errors.push(`non-gating bench cfg/cfg_attr: ${hook.id}: ${attr}`);
    if (hook.unsafe) {
      foundUnsafe.add(hook.id);
      if (!/^\s*\/\/\/\s*# Safety\s*$/m.test(hook.docs)) errors.push(`unsafe hook lacks its # Safety contract: ${hook.id}`);
      if (BENCH_UNSAFE_HOOKS.has(hook.id) && !requires('bench-internals')) errors.push(`measurement-only unsafe hook is not bench-internals-gated: ${hook.id}`);
      if (hook.id === CURRENT_RESERVATION_HOOK && !requires('internals')) errors.push(`reservation observer is not internals-gated: ${hook.id}`);
      if (SIDECAR_UNSAFE_HOOKS.has(hook.id)) for (const feature of ['alloc-global', 'alloc-xthread', 'internals', 'bench-internals']) if (!requires(feature)) errors.push(`terminal producer hook missing ${feature} gate: ${hook.id}`);
    } else if (requires('bench-internals')) gated++;
    else foundSafe.add(hook.id);
  }
  for (const [found, expected, label] of [[foundSafe, expectedSafe, 'safe ungated'], [foundUnsafe, expectedUnsafe, 'unsafe']]) {
    for (const id of found) if (!expected.has(id)) errors.push(`unreviewed ${label} hook: ${id}`);
    for (const id of expected) if (!found.has(id)) errors.push(`stale ${label} allowlist entry (review required, never auto-pruned): ${id}`);
  }
  return { errors, safe: foundSafe.size, unsafe: foundUnsafe.size, gated };
}
function selfCheck() {
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
  const observerFile = 'src/global/sefer_alloc/diag.rs';
  const observer = scanFile(observerFile, '/// # Safety\n/// Caller owns the live allocation.\n#[cfg(feature = "internals")]\npub unsafe fn dbg_current_reservation_for_test(ptr: *mut u8) {}');
  const observerErrors = verifyHooks(new Map([[observerFile, observer]])).errors;
  assert(observerErrors.includes(`measurement-only unsafe hook is not bench-internals-gated: ${CURRENT_RESERVATION_HOOK}`));
  const observerBenchOnly = scanFile(observerFile, '/// # Safety\n/// Caller owns the live allocation.\n#[cfg(feature = "bench-internals")]\npub unsafe fn dbg_current_reservation_for_test(ptr: *mut u8) {}');
  assert(verifyHooks(new Map([[observerFile, observerBenchOnly]])).errors.includes(`reservation observer is not internals-gated: ${CURRENT_RESERVATION_HOOK}`));
}
selfCheck();
if (process.argv.includes('--self-test')) { console.log('[verify-dbg-hook-safety] parser adversarial self-checks PASS'); }
else {
  const files = new Map([...listRust(join(REPO_ROOT, 'src')), ...listRust(join(REPO_ROOT, 'crates'))].map(path => { const id = idFor(path); return [id, scanFile(id, readFileSync(path, 'utf8'))]; }));
  if (!files.size) throw new Error('no Rust source files found');
  const result = verifyHooks(files);
  if (result.errors.length) { console.error(`[verify-dbg-hook-safety] FAIL\n${result.errors.join('\n')}`); process.exitCode = 1; }
  else console.log(`[verify-dbg-hook-safety] PASS: ${result.safe} reviewed safe, ${result.unsafe} reviewed unsafe, ${result.gated} genuinely bench-gated safe hooks`);
}
