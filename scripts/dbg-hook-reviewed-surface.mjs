// Extracted verbatim from scripts/verify-dbg-hook-safety.mjs (P2.1 corrective split).
// Static reviewed-surface tables only; no data line changed.

// Pure observers: read-only, no allocator-state mutation.
export const PURE_OBSERVERS = [
  // PRF01: oracle-only relaxed totals, constant-zero A summary; no pointer/reset/policy.
  "src/registry/heap_core/diag/queries.rs::dbg_prf01_sidecar_oracle",
  // Strict-trim process-wide relaxed totals and this core's independent segment geometry only.
  "src/registry/heap_core/diag/queries.rs::dbg_prf01_strict_words_visited",
  // Strict-trim read-only relaxed counter totals plus owned segment geometry; no reset or policy.
  "src/global/sefer_alloc/diag.rs::dbg_prf01_strict_words_visited",
  // B4: static snapshot of 7 Relaxed counters; impl gated on internals+bench-internals, no instance/pointer use.
  "src/global/sefer_alloc/diag.rs::dbg_b4_collapse_counters",
  // B4 §11: static snapshot of 2 Relaxed cursor counters; impl gated on internals+bench-internals, no instance/pointer use.
  "src/global/sefer_alloc/diag.rs::dbg_b4_cursor_counters",
  // B4 §11 test-only: read-only cursor slot of the current thread's heap; no pointer/mutation.
  "src/global/sefer_alloc/diag.rs::dbg_class_cursor_slot_for_current_thread",
  // B4 §11 test-only: read-only cursor slot of this AllocCore.
  "src/alloc_core/small/alloc_core_small/find_segment/class_cursor.rs::dbg_class_cursor_slot",
  // B4 §11 test-only: read-only cursor slot of this heap's AllocCore.
  "src/registry/heap_core/core.rs::dbg_class_cursor_slot",
  // PRF01: actual type layout only, no allocator instance or pointer access.
  "src/registry/heap_core/diag/queries.rs::dbg_prf01_small_sidecar_size",
  // C1: relaxed successful-lease totals only, no pointers, reset or policy use.
  "src/registry/bootstrap/free_slot_hint.rs::dbg_popcount",
  "src/registry/heap_registry/claim.rs::dbg_registry_claim_sources",
  "src/registry/heap_registry/claim.rs::dbg_registry_free_set_claims",
  // Read-only Acquire popcount of the advisory FREE-slot bitmap.
  "src/registry/heap_registry/claim.rs::dbg_free_slot_hint_popcount",
  // Read-only Relaxed load of the advisory reuse-hint word; no pointer or reset.
  "src/registry/heap_registry/claim.rs::dbg_reuse_hint",
  // C1: numeric results of resolved-default comparison under the claiming lease.
  "src/registry/heap_registry/claim.rs::dbg_registry_default_config_results",
  // R6-T07: pure decision over four scalar arguments; no allocator state.
  "src/global/tls_heap.rs::dbg_saturation_retry_decision",
  // CI-2b: relaxed cumulative System totals and actual slot layout only;
  // no pointer input, metadata access, allocation, mutation, or counter reset.
  "crates/core/sefer-segment/src/segment/segment_route/route_state/small_sidecar.rs::dbg_route_slots_system_totals",
  "crates/core/sefer-segment/src/segment/segment_route/route_state/small_sidecar.rs::dbg_route_slot_bytes",
  // Read-only Relaxed load of the cold-path giveup counter.
  "crates/platform/sefer-platform/src/dbg_exit_lock_giveups.rs::dbg_exit_lock_giveups",
  // Read-only Relaxed descriptor count; the borrowed RoutePin retains its lifetime.
  "crates/core/sefer-segment/src/segment/segment_route/directory/route_directory.rs::dbg_route_pin_refs",
  // Read-only Relaxed load of Entry.refs through the owner-held RouteRegistration's own EntryHandle; no address-keyed lookup, no pointer dereference beyond the already-owned entry.
  "crates/core/sefer-segment/src/segment/segment_route/route_state/registration.rs::dbg_entry_refs_relaxed",
  "crates/platform/numa-shim/src/lib.rs::dbg_current_node_for_cpu",
  "crates/platform/numa-shim/src/lib.rs::dbg_node_resolution_for_cpu",
  "crates/primitives/once-ptr-cell/src/imp.rs::dbg_is_ready",
  "src/alloc_core/alloc_core/alloc_core_core_diag/totals.rs::dbg_foreign_or_unroutable_frees",
  "src/alloc_core/alloc_core/alloc_core_core_diag/totals.rs::dbg_local_reject_frees",
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
  "src/alloc_core/large/large_cache_adaptive.rs::dbg_large_cache_adaptive",
  "src/alloc_core/large/alloc_core_large_cache_eviction.rs::dbg_large_cache_budget",
  "src/alloc_core/large/alloc_core_large_cache_eviction.rs::dbg_large_cache_mode",
  "src/alloc_core/platform/sidecar_stats.rs::dbg_sidecar_reservation_stats",
  "src/alloc_core/alloc_core/state.rs::dbg_cached_numa_node",
  "src/alloc_core/small/alloc_core_small_diag.rs::dbg_freelist_head_for",
  "src/alloc_core/small/alloc_core_small_diag.rs::dbg_is_free_for",
  // B4 (в) F11: read-only state-nibble of one granule via the canonical table root; no mutation.
  "src/alloc_core/small/alloc_core_small_diag.rs::dbg_state_byte_state_for",
  "src/registry/heap_core/diag/queries.rs::dbg_state_byte_state_for",
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
  "src/registry/heap_core/diag/queries.rs::dbg_class_for",
  "src/registry/heap_core/diag/queries.rs::dbg_refill_n_for_class",
  // Pure const-table read (FREE_PARK_CAP[c]); no pointer, no allocation, no state mutation.
  "src/registry/heap_core/diag/queries.rs::dbg_free_park_cap",
  "src/registry/heap_core/diag/queries.rs::dbg_directory_bit_for_ptr",
  "src/registry/heap_core/diag/queries.rs::dbg_pooled_count",
  "src/registry/heap_core/diag/queries.rs::dbg_segment_base_of_ptr",
  "src/registry/heap_core/diag/queries.rs::dbg_live_count_for",
  "src/registry/heap_core/diag/queries.rs::dbg_tcache_virgin_mask",
  "src/registry/heap_core/diag/queries.rs::dbg_tcache_count",
  "src/registry/heap_core/diag/queries.rs::dbg_depot_len",
  // G1-K7: read-only copies of the resolved budget and owner-only counters.
  "src/registry/heap_core/diag/queries.rs::dbg_depot_budget",
  "src/registry/heap_core/diag/queries.rs::dbg_depot_overflow",
  "src/registry/heap_core/diag/queries.rs::dbg_depot_ring",
  "src/registry/heap_core/diag/queries.rs::dbg_depot_ticking",
  // WB-3: read-only copy of the overflow decay / ingress / latch state.
  "src/registry/heap_core/diag/queries.rs::dbg_depot_epoch",
  // G1-K7: pure const resolution of a by-value config.
  "crates/core/sefer-policy/src/large_cache_config.rs::dbg_resolved_depot_budget",
  // CF-2: pure const resolution of a by-value config.
  "crates/core/sefer-policy/src/large_cache_config.rs::dbg_resolved_route_pin_cache_slots",
  "crates/core/sefer-policy/src/large_cache_config.rs::dbg_resolved_huge_page_after_segments",
  // J13b: pure const resolution of a by-value config into a Copy tuple.
  "crates/core/sefer-policy/src/large_cache_config.rs::dbg_resolved_thp_tuple",
  "src/registry/heap_core/diag/diag_probes.rs::dbg_hardened_large_noop_count",
  "src/registry/heap_core/diag/diag_probes.rs::dbg_contains_base",
  "src/registry/heap_core/diag/diag_probes.rs::dbg_hash_contains_only",
  "src/registry/heap_core/diag/queries.rs::dbg_kind_at_tag",
  "src/registry/heap_core/diag/queries.rs::dbg_kind_byte_of",
  // Pure numeric size-class divisibility forwarders (no pointer, no state).
  "crates/core/sefer-segment/src/segment/support/segment_directory/segment_layout.rs::dbg_size_class_divides",
  "crates/core/sefer-segment/src/segment/support/segment_directory/segment_layout.rs::dbg_size_class_divisor",
  // Non-panicking SegmentTable membership probe (canonical_root_for); read-only.
  "src/registry/heap_core/diag/queries.rs::dbg_segment_registered_for",
  "src/registry/heap_core/diag/queries.rs::dbg_table_count",
  "src/registry/bootstrap/ensure.rs::dbg_slot_or_none"
];

// Safe mutators: each retained reviewed invariant justification is authoritative.
export const SAFE_MUTATORS = [
  [
    "src/registry/heap_core/diag/diag_probes.rs::dbg_arm_depot_overflow_faults",
    "G1-K7 test fault: stores Relaxed fail-next counts consulted only before a depot-overflow reservation and (aligned-vmem's existing hook, as dbg_arm_commit_fail) before a real page commit; both take existing OS-refusal paths; no pointer, no metadata write; compiled only with the test-only lazy-commit-fault-injection feature"
  ],
  [
    "src/global/sefer_alloc/diag.rs::dbg_background_maintenance_current_thread",
    "delegates only to the production bounded background_maintenance_step for current_for_trim's current-thread exclusive live HeapCore; no FREE-slot lease or lifecycle mutation; bench-internals + internals gated"
  ],  [
    "crates/core/sefer-segment/src/segment/segment_route/directory/route_directory.rs::dbg_with_shard_of_locked_for_test",
    "bench-internals-only ordinary shard guard; pointer address is only a shard key, never dereferenced; callback receives no protected data; reentry or a stranded holder risks liveness, not memory safety; unwind releases the guard"
  ],
  ["src/registry/bootstrap/registry.rs::dbg_slot_state", "Registry::slot may materialize a chunk and abort on OOM; then reads atomic state, without borrowing the core"],
  ["src/registry/bootstrap/registry.rs::dbg_slot_generation", "Registry::slot may materialize a chunk and abort on OOM; then reads atomic generation, without borrowing the core"],
  ["src/registry/heap_registry/counters.rs::dbg_slot_initialised", "Registry::slot may materialize a chunk and abort on OOM; then reads initialization state, without borrowing the core"],
  [
    "src/registry/heap_registry/claim.rs::dbg_forget_free_slot_hint",
    "clears one bit in the non-authoritative FREE hint bitmap only; slot-state CAS remains the ownership authority, so a missed hint affects selection rather than correctness"
  ],
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
    "crates/primitives/once-ptr-cell/src/imp.rs::dbg_rollback_reenterable",
    "entry CAS is a point-in-time UNINIT check, not mutual exclusion across the whole probe; the final restore is gated on the probe's own postcondition CAS re-winning the cell, so a concurrent get_or_try_init racing in mid-probe is never clobbered"
  ],
  [
    "crates/containers/sefer-region/src/region.rs::dbg_try_mint_region_id",
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
    "src/alloc_core/large/alloc_core_large_cache_eviction.rs::dbg_backdate_decay_timer",
    "changes only inert timer/stride bookkeeping; checked subtraction cannot access pointers or allocator metadata, so arbitrary durations cannot cause UB"
  ],
  [
    "src/alloc_core/large/large_cache_adaptive.rs::dbg_set_large_cache_adaptive",
    "switches the learned retention policy flag only; the cache then behaves as the fixed-capacity extension, never touching pointers or metadata"
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
  ],
  [
    "src/registry/heap_registry/claim.rs::dbg_set_inject_ctor_oom",
    "sets a plain AtomicBool test-injection flag only; no allocator metadata, no raw pointers, no soundness relevance — the flag makes claim_impl's materialise call return None early, simulating an already-handled production OOM path (R6-T03)"
  ],
  [
    "src/alloc_core/large/reservation_state.rs::dbg_set_inject_large_alloc_oom",
    "sets a plain AtomicBool test-injection flag only; alloc_large then returns null before touching metadata, the already-handled production OOM result (R6-T29)"
  ],
  [
    "src/alloc_core/large/reservation_state.rs::dbg_set_inject_biased_reserve_fail",
    "sets a plain AtomicBool test-injection flag only; the first non-NUMA biased reserve reports None, the already-handled production reserve-failure result; the retry stays real (R6-T34)"
  ],
  [
    "src/alloc_core/large/reservation_state.rs::dbg_set_inject_large_register_fail",
    "sets a plain AtomicBool test-injection flag only; the fresh-path Large registration reports failure, the already-handled production table-full result; the reservation is released through the terminal funnel (J13b)"
  ],
  [
    "src/alloc_core/large/reservation_state.rs::dbg_alloc_large_for_test",
    "delegates to the real production AllocCore::alloc_large with a caller size and alignment; takes no pointer, returns a fresh allocation or null; overflow guards are the code under test (R6-T30)"
  ]
];

// Explicit reviewed rows; groups share only a reviewed contract, never a name wildcard.
export const REVIEWED_SURFACE = [
  { id: 'crates/platform/aligned-vmem/src/api/release.rs::release_size_align_is_valid_for_test', kind: 'observer', reason: 'pure release geometry validation over integers; no pointers, mapped storage or OS access', gates: ['fault-injection'] },
  // P1.4: these moved verbatim from `src/alloc_core/platform/os.rs` (formerly
  // `pub(crate)` and therefore invisible to this scanner) to the new
  // `sefer-platform` crate and are now crate-public. The raw-pointer OS-page
  // functions are `pub unsafe fn` with a `# Safety` contract (R25-1 class; the
  // root keeps pub(crate) safe wrappers) and are production unsafe API outside
  // internals/bench-internals, which this inventory deliberately does not list
  // (see the tier-policy skip in verifyHooks). Only `segment_base_of_ptr` (pure
  // address arithmetic, no dereference) is a safe observer row; the gated
  // measurement twin is an unsafe row.
  { id: 'crates/platform/sefer-platform/src/os.rs::segment_base_of_ptr', kind: 'observer', reason: 'SEGMENT-aligned base-of-pointer arithmetic over an allocator-owned segment (Cartographer invariant documented at the definition); no dereference, no OS access', gates: [] },
  { id: 'crates/platform/sefer-platform/src/os.rs::commit_pages_for_measurement', kind: 'unsafe', reason: 'measurement-side twin of commit_pages: same caller contract (# Safety at the definition) over an allocator-owned segment reservation; bench-internals+internals gated', gates: ['bench-internals', 'internals', 'alloc-decommit'] },
  { id: 'src/registry/heap_core/diag/queries.rs::dbg_sidecar_scan_measurement', kind: 'mutator', reason: 'thread-local optional diagnostic counts and original-versus-prefilter scan mode only; no pointers or allocator metadata access', gates: ['bench-internals', 'r18_sidecar_scan_bench'] },
  ...[
    ['crates/core/sefer-segment/src/segment/support/segment_directory/segment_layout.rs', ['small_decommit_start', 'primordial_decommit_start', 'small_lazy_initial_commit', 'primordial_lazy_initial_commit'], 'observer', 'pure geometry forwarders'],
    ['src/alloc_core/small/alloc_core_small_magazine.rs', ['refill_class', 'refill_class_bump', 'refill_class_bump_virgin'], 'mutator', 'delegates to allocation substrate; exclusive mutable core borrow'],
    ['crates/containers/sefer-concurrent/src/epoch/epoch_region.rs', ['_remote_free_queue_buffer_identity_for_tests'], 'observer', 'locked buffer identity only; no dereference permission'],
    ['crates/containers/sefer-concurrent/src/epoch/epoch_region.rs', ['_set_slot_generation_for_tests'], 'mutator', 'vacant-slot generation boundary; mutable region borrow'],
    ['crates/containers/sefer-concurrent/src/sharded/sharded_region.rs', ['_reset_my_shard_binding_for_tests'], 'mutator', 'clears TLS routing cache, not occupied tokens; may consume additional claims'],
    ['crates/containers/sefer-concurrent/src/sharded/sharded_region.rs', ['_remote_free_queue_buffer_identity_for_tests', '_live_token_blocks_for_tests', '_tls_claim_count_for_tests', '_prune_claim_checks_for_tests'], 'observer', 'identity and atomic/TLS count observations'],
    ['src/registry/bootstrap/ensure.rs', ['dbg_num_chunks'], 'observer', 'constant geometry'],
    ['src/registry/heap_core/diag/diag_probes.rs', ['dbg_promotion_compiled', 'dbg_promotion_macro_gate'], 'observer', 'compile-time predicate'],
    ['src/global/maintenance_service.rs', ['passes_for_test', 'fallback_visits_for_test'], 'observer', 'atomic diagnostic counters'],
    ['src/global/maintenance_service.rs', ['wait_after_for_test'], 'mutator', 'locks/waits on production completion condition'],
    ['crates/core/sefer-segment/src/segment/segment_route/directory/route_directory.rs', ['fail_next_registration_for_test'], 'mutator', 'TLS registration failure countdown; handled fallible path'],
    ['crates/core/sefer-segment/src/segment/segment_route/directory/route_directory.rs', ['with_initial_incarnation_for_test'], 'mutator', 'constructs isolated directory with chosen initial incarnation'],
    ['crates/core/sefer-segment/src/segment/segment_route/directory/route_directory.rs', ['retained_pointer_capacity_for_test', 'moved_pointer_cells_for_test', 'shard_index_for_test', 'live_route_census_for_test'], 'observer', 'directory capacity/census under its locks'],
    ['crates/core/sefer-segment/src/segment/segment_route/route_state/small_sidecar.rs', ['class_at_for_test', 'mixed_leaves_for_test', 'system_totals_for_test', 'chunk_totals_for_test'], 'observer', 'sidecar class/counter observations'],
    ['crates/core/sefer-segment/src/segment/segment_route/route_state/registration.rs', ['class_at_global_address_for_test'], 'observer', 'lookup pins independent sidecar before class read'],
    ['crates/core/sefer-segment/src/segment/segment_route/route_protocol/large_state.rs', ['word_for_test'], 'observer', 'atomic terminal state snapshot'],
    ['crates/core/sefer-segment/src/segment/segment_route/route_state/pin.rs', ['pending_for_test'], 'observer', 'pinned sidecar pending state'],
    ['src/alloc_core/platform/numa.rs', ['biased_root_offset_for_test'], 'observer', 'pure checked geometry'],
    ['src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs', ['large_geometry_for_test'], 'observer', 'validated owned header geometry'],
  ].flatMap(([file, names, kind, reason]) => names.map(name => ({ id: `${file}::${name}`, kind, reason, gates: ['internals'] }))),
  ...[
    ['crates/core/sefer-segment/src/segment/segment_table/impl_group/harness.rs', ['attach_routes_for_test', 'register_route_for_test'], 'mutator', 'exclusive owned fixture delegates to real attachment/registration; synthetic address keys never dereferenced and descriptors unlinked on drop'],
    ['src/global/maintenance_service.rs', ['try_fallback_step_for_test', 'fail_next_start_for_test', 'pause_start_for_test', 'wait_start_paused_for_test', 'fail_worker_for_test'], 'mutator', 'bounded maintenance step or explicit startup/worker fault injection; synchronization retained'],
    ['crates/core/sefer-segment/src/segment/segment_route/directory/route_directory.rs', ['fail_next_registrations_for_test'], 'mutator', 'TLS registration failure countdown; handled fallible path'],
    ['crates/core/sefer-segment/src/segment/segment_route/route_state/small_sidecar.rs', ['fail_spill_after_for_test', 'fail_next_spills_for_test', 'fail_prepare_after_for_test'], 'mutator', 'atomic fallible preparation/spill fault injection'],
    ['crates/core/sefer-segment/src/segment/segment_route/route_state/small_sidecar.rs', ['spill_failures_remaining_for_test'], 'observer', 'atomic injection countdown'],
    ['crates/core/sefer-segment/src/segment/segment_route/route_state/small_sidecar.rs', ['owner_state_for_test'], 'unsafe', 'caller owns mapped reservation; raw metadata snapshot'],
    ['src/alloc_core/large/alloc_core_large_cache_eviction.rs', ['terminal_cached_large_state_for_test'], 'observer', 'owner cache terminal snapshot'],
    ['src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs', ['terminal_header_layout_for_test', 'terminal_header_snapshot_for_test', 'terminal_primordial_snapshot_for_test', 'terminal_next_generation_for_test'], 'observer', 'checked header layout/state and generation geometry'],
    ['src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs', ['large_thp_charge_for_test'], 'observer', 'owned Large header field read; kind-checked, no pointer dereference beyond the owned header'],
    ['crates/containers/sefer-concurrent/src/sharded/sharded_region.rs', ['_set_remote_free_hint_for_tests'], 'mutator', 'advisory hint only; real queue remains authoritative'],
    ['crates/core/sefer-segment/src/segment/segment_route/route_state/pin.rs', ['dbg_publish_cached_for_test'], 'unsafe', 'entry into the production cached fast path; the address must be the caller’s CURRENT issued Small/Primordial allocation — publication consumes ownership once, and a bare address never establishes it (publishing a foreign live address forces the owner to reclaim a live instance: UAF by construction)'],
    ['crates/core/sefer-segment/src/segment/segment_route/route_state/pin.rs', ['dbg_cache_insert_for_test'], 'mutator', 'inserts caller-looked-up pin; no publish/reclaim obligation'],
    ['crates/core/sefer-segment/src/segment/segment_route/route_state/pin.rs', ['dbg_route_pin_cache_totals'], 'observer', 'pure Relaxed counter snapshot'],
  ].flatMap(([file, names, kind, reason]) => names.map(name => ({ id: `${file}::${name}`, kind, reason, gates: ['internals', 'bench-internals'] }))),
  ...[
    ['crates/containers/sefer-concurrent/src/lock_free/lock_free_region.rs', ['_forge_handle_for_tests', '_page_arc_clones_for_tests', '_checked_total_slots_for_tests'], 'observer', 'by-value handle/geometry or TLS counter; ordinary operations still validate handles'],
    ['crates/containers/sefer-concurrent/src/lock_free/lock_free_region.rs', ['_reset_page_arc_clones_for_tests'], 'mutator', 'TLS diagnostic counter only'],
  ].flatMap(([file, names, kind, reason]) => names.map(name => ({ id: `${file}::${name}`, kind, reason, gates: ['bench-internals'] }))),
  { id: 'crates/platform/aligned-vmem/src/bench_internals/advice/stream/log.rs::seed_cursor_for_test', kind: 'mutator', reason: 'seeds only the caller-owned synthetic log cursor; no global native-log accessor, mapping access or allocator-policy effect; sticky publication failures remain', gates: ['bench-internals', 'fault-injection'] },
  { id: 'crates/platform/aligned-vmem/src/bench_internals/advice/stream/log.rs::reserve_unpublished_for_test', kind: 'mutator', reason: 'reserves one deliberately incomplete scalar slot in the caller-owned synthetic log; bounded capacity and sticky unsupported closure remain', gates: ['bench-internals', 'fault-injection'] },
  { id: 'crates/platform/aligned-vmem/src/bench_internals/advice/stream/log.rs::complete_reserved_for_test', kind: 'mutator', reason: 'publishes inert words only into a reserved caller-owned synthetic-log slot through the real immutable slot protocol; cannot clear failure or access mappings/native global', gates: ['bench-internals', 'fault-injection'] },
  { id: 'crates/core/sefer-segment/src/support/mod.rs::set_segment_id_at', kind: 'unsafe', reason: 'pub(crate) unsafe internals-gated header segment_id write during owner bootstrap; live aligned owner-exclusive header (# Safety at the definition)', gates: ['internals'] },
  { id: 'crates/core/sefer-segment/src/segment/segment_table/impl_group/harness.rs::hide_root_for_test', kind: 'mutator', reason: 'isolated numeric harness; no dereference; external alloc_core module path requires internals, not a compilation gate', gates: [] },
  { id: 'crates/core/sefer-segment/src/segment/segment_route/directory/route_directory.rs::register', kind: 'unsafe', reason: 'pub unsafe fn: caller guarantees live owned reservation identity for the numeric key; pointer args are never dereferenced and the registration unlinks on drop (# Safety at the definition)', gates: ['internals'] },
  { id: 'crates/core/sefer-segment/src/segment/segment_route/directory/route_directory.rs::lookup', kind: 'observer', reason: 'the real production lookup path; the pointer is only a numeric shard key, never dereferenced', gates: [] },
  { id: 'crates/core/sefer-segment/src/segment/segment_route/route_state/pin.rs::contains_payload', kind: 'observer', reason: 'pure span-geometry comparison of the caller pointer against the pinned entry bounds; no dereference', gates: [] },
  { id: 'src/registry/heap_core/alloc/batch.rs::alloc_batch', kind: 'mutator', reason: 'production batch allocation path; writes issued block pointers into the caller-owned out slice', gates: [] },
  { id: 'crates/core/sefer-segment/src/segment/segment_table/impl_group/harness.rs::insert', kind: 'unsafe', reason: 'pub unsafe fn: inserted synthetic value becomes the harness table canonical root safe lookups return verbatim; caller contract at the # Safety doc', gates: [] },
  { id: 'crates/core/sefer-segment/src/segment/segment_table/impl_group/harness.rs::insert_root_for_key', kind: 'unsafe', reason: 'pub unsafe fn: root must carry valid provenance over its own address (safe lookups return it verbatim); key is an address-only key (# Safety at the definition)', gates: [] },
  { id: 'crates/core/sefer-segment/src/segment/segment_table/impl_group/harness.rs::remove_root_for_key', kind: 'mutator', reason: 'isolated numeric SegmentTable harness; synthetic keys never dereferenced', gates: [] },
  { id: 'crates/core/sefer-segment/src/segment/segment_table/impl_group/harness.rs::remove', kind: 'mutator', reason: 'isolated numeric SegmentTable harness; synthetic keys never dereferenced', gates: [] },
  { id: 'crates/core/sefer-segment/src/segment/segment_table/impl_group/harness.rs::contains', kind: 'observer', reason: 'isolated numeric SegmentTable harness; synthetic keys never dereferenced', gates: [] },
  { id: 'crates/core/sefer-segment/src/segment/segment_table/impl_group/harness.rs::contains_cached', kind: 'observer', reason: 'isolated numeric SegmentTable harness; synthetic keys never dereferenced', gates: [] },
  { id: 'crates/core/sefer-segment/src/segment/segment_table/impl_group/harness.rs::canonical', kind: 'observer', reason: 'isolated numeric SegmentTable harness; synthetic keys never dereferenced', gates: [] },
  // P2.1 extraction: hooks moved verbatim into crates/core/sefer-segment;
  // these were previously private/invisible to this scanner at the old root paths.
  ...[
    ['crates/core/sefer-segment/src/segment/segment_table/impl_group/routing/route_slots_observers.rs', ['arm_small_route'], 'mutator', 're-arms a routed slot small route before its full drain; production drain path, pointer is a numeric key'],
    ['crates/core/sefer-segment/src/segment/segment_table/impl_group/segment_table_impl.rs', ['contains_base', 'canonical_base_of_mut'], 'mutator', 'owner lookup over allocator-owned metadata; pointer keys never dereferenced, cache retains only roots proven present'],
    ['crates/core/sefer-segment/src/segment/segment_table/impl_group/segment_table_impl.rs', ['canonical_base_of', 'memo_holds'], 'observer', 'pure membership/memo probes over the table own stored entries; pointer keys never dereferenced'],
    ['crates/core/sefer-segment/src/support/mod.rs', ['dbg_hash_contains_only'], 'observer', 'hash-only membership probe; pointer keys never dereferenced'],
    ['crates/core/sefer-segment/src/segment/segment_table/impl_group/hash.rs', ['contains_base_ro'], 'observer', 'hash-only membership probe over the table own stored entries; pointer keys never dereferenced'],
  ].flatMap(([file, names, kind, reason]) => names.map(name => ({ id: `${file}::${name}`, kind, reason, gates: [] }))),

  { id: 'crates/platform/aligned-vmem/src/reservation_parts.rs::new', kind: 'observer', reason: 'pure constructor storing the caller-owned reservation pointer and length into a parts struct; no dereference', gates: [] },
  { id: 'crates/platform/aligned-vmem/src/reservation_full_parts.rs::new', kind: 'observer', reason: 'pure field-by-field reconstruction of reservation parts; caller-owned pointers stored, never dereferenced', gates: [] },
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
  ].map(([name, kind, gates]) => ({ id: `crates/primitives/tagged-index-stack/src/imp.rs::${name}`, kind, gates, reason: 'isolated verification stack/model surface; raw link write requires its caller contract' })),
];
