// VER-03: one source for local execution and manual CI wiring verification.
// EXPECTED-RED strings below are verbatim CI grep expressions (card 164).
const green = (job, test, features = '', extra = {}) => ({
  job, test, features, model: 'strict', borrows: 'sb', args: [], expect: 'GREEN', ...extra,
});
const paused = (features, borrows) => green('miri-core', 'miri_global_box_acceptance', features, {
  borrows, args: ['--', 'paused'], expect: 'EXPECTED-RED', card: 164,
  marker: '[miri_global_box_acceptance] COMPLETE paused_terminal_owner_retirement',
  redMessage: borrows === 'sb'
    ? 'Undefined Behavior: not granting access to tag .* weakly protected'
    : 'Undefined Behavior: write access through .* is forbidden',
  redSite: {
    frame0: '^[[:space:]]+0: sefer_alloc::alloc_core::platform::node::Node::write_next',
    file: 'at .*node\\.rs:',
    caller: 'reclaim_sidecar_record|flush_run|dealloc_small',
  },
});
const narrow = (borrows) => green('miri-core', 'miri_global_box_acceptance', 'alloc-global internals bench-internals', {
  borrows, args: ['--', 'narrow'], marker: '[miri_global_box_acceptance] COMPLETE narrow_two_rounds',
});
const r101 = (borrows) => green('miri-core', 'miri_r1_01_cap_frame', 'alloc-global fastbin internals bench-internals', {
  borrows, marker: '[miri_r1_01_cap_frame] COMPLETE r1_01_cap_frame',
});
const depot = (borrows) => green('miri-core', 'miri_depot_cycle', 'alloc-global fastbin internals bench-internals', {
  borrows, marker: '[miri_depot_cycle] COMPLETE depot_cycle',
});
const plainFeatures = 'alloc-global alloc-xthread alloc-decommit internals bench-internals';
export const MIRI_TARGETS = [
  green('miri-core', 'region_invariants'),
  green('miri-core', 'advice_event_stream', 'bench-internals fault-injection', {
    pkg: 'aligned-vmem',
    marker: 'test cursor_wrap_never_turns_failed_capture_into_an_empty_success ... ok',
  }),
  green('miri-core', 'advice_event_replay', 'bench-internals', {
    pkg: 'aligned-vmem',
    marker: 'test unknown_release_and_partial_advice_never_yield_zero_or_repaired_rows ... ok',
  }),
  green('miri-core', 'j13b_advice_capture_native', 'production alloc-stats internals bench-internals lazy-commit-fault-injection', {
    marker: 'test native_advice_capture_rejects_unsupported_hosts_before_payload_claims ... ok',
  }),
  green('miri-core', 'j13b_advice_windows_native', 'production alloc-stats internals bench-internals lazy-commit-fault-injection', {
    marker: 'test advice_windows_reject_models_before_clocks_capture_or_payload ... ok',
  }),
  green('miri-core', 'j13b_native_identity', '', {
    marker: 'test process_facts_reject_models_before_files_commands_or_payload ... ok',
  }),
  green('miri-core', 'j13b_build_seal', '', {
    marker: 'test duplicate_or_escaped_compiled_keys_cannot_silently_override ... ok',
  }),
  green('miri-core', 'j13b_artifact_request', '', {
    marker: 'test duplicate_decoded_keys_and_escaped_aliases_are_rejected ... ok',
  }),
  green('miri-core', 'advice_window', 'bench-internals', {
    pkg: 'aligned-vmem',
    marker: 'test historical_cumulative_peak_is_not_the_next_window_peak ... ok',
  }),
  green('miri-core', 'advice_lifetime_ledger', 'bench-internals', {
    pkg: 'aligned-vmem',
    marker: 'test unsupported_release_preserves_entries_and_never_recovers_via_retry_or_reuse ... ok',
  }),
  ...['sb', 'tb'].map((borrows) => green('miri-core', 'j13b_window_kernel', '', {
    borrows,
    marker: 'test four_workers_execute_and_all_joins_precede_an_error_result ... ok',
  })),
  ...['sb', 'tb'].map((borrows) => green('miri-core', 'j13b_payload_actors', '', {
    borrows,
    marker: 'test persistent_actors_transfer_both_directions_on_actual_threads ... ok',
  })),
  green('miri-core', 'j13b_owner_training_native', 'production internals bench-internals lazy-commit-fault-injection', {
    marker: 'test unsupported_training_is_rejected_before_payload_claims ... ok',
  }),
  green('miri-core', 'j13b_adaptive_windows_native', 'production internals bench-internals', {
    marker: 'test adaptive_windows_reject_model_or_unsupported_os_before_payloads ... ok',
  }),
  green('miri-core', 'j13b_os_probe', '', {
    marker: 'test unsupported_os_or_model_is_rejected_before_any_probe ... ok',
  }),
  ...['sb', 'tb'].map((borrows) => green('miri-core', 'j13b_owned_block', '', {
    borrows,
    marker: 'test ownership_transfers_both_directions_and_is_freed_on_the_actual_destination ... ok',
  })),
  narrow('sb'),
  narrow('tb'),
  r101('sb'),
  r101('tb'),
  depot('sb'),
  depot('tb'),
  paused('alloc-global internals bench-internals', 'sb'),
  paused('alloc-global internals bench-internals', 'tb'),
  paused('production internals bench-internals', 'sb'),
  paused('production internals bench-internals', 'tb'),
  green('miri-core', 'narrow_domain_unchecked_storage', '', { pkg: 'tagged-index-stack' }),
  green('miri-core', 'decommit_miri_cycle', 'alloc-core alloc-decommit internals'),
  green('miri-core', 'segment_directory_a5_miri', 'alloc-segment-directory internals'),
  // Ghost-checks G1: the ghost macros and shadow table under miri (cfg(miri) is a ghost build).
  green('miri-core', 'ghost_infra', 'internals'),
  // Ghost-checks G2 (P5 route pins, item 186): last-reference free vs a concurrent pin drop on the real directory.
  green('miri-core', 'route_pin_last_ref_miri', 'alloc-global internals'),
  ...['regression_large_align_no_segment_exhaustion', 'regression_page_aligned_no_segment_exhaustion',
    'regression_realloc_cross_class_shrink', 'stress_boundary_sweep']
    .map((test) => green('miri-alloc-core', test, 'alloc-core')),
  green('miri-alloc-core', 'regression_r6_t22_foreign_window_key_miri', 'exact-span-large internals'),
  // NL-3: bounded cross-thread safe churn (cfg(miri) caps the sequence inside the test).
  ...['regression_magazine_oracles', 'regression_bump_direct_refill', 'nl3_safe_xthread_churn_integrity',
    'nl7_safe_large_realloc_integrity']
    .map((test) => green('miri-fastbin', test, 'alloc-global alloc-xthread alloc-decommit fastbin internals')),
  ...['regression_xthread_large_free_no_leak', 'regression_xthread_thread_free_alias_miri',
    'regression_xthread_small_ring_miri']
    .map((test) => green('miri-plain', test, plainFeatures, { model: 'plain' })),
  green('miri-plain', 'b4_r1_pin_cache_miri', plainFeatures, { model: 'plain' }),
  // b4-r1 round 2 (P2): the same cell under narrow provenance — REAL Box
  // ownership transported via a Send wrapper, cache-hit activation oracle
  // (hits delta >= 1 reds a "publish_cached always Miss" mutant), gated
  // pending-bit delivery and TLS thread-exit release. plain stays because
  // the file is plain-compatible (no strict-provenance requirement) and its
  // preemption=0.5 coverage of the start gate schedule is retained.
  // The TreeBorrows cell runs in its own job (miri-core-tb): it is the longest step of the workflow.
  ...['sb', 'tb'].map((borrows) => green(borrows === 'tb' ? 'miri-core-tb' : 'miri-core', 'b4_r1_pin_cache_miri', 'alloc-global internals bench-internals', {
    borrows, args: ['--', 'narrow'],
    marker: '[b4_r1_pin_cache_miri] COMPLETE narrow',
  })),
  green('miri-plain', 'r6_terminal_owner_drain', 'alloc-global alloc-xthread internals bench-internals', {
    model: 'plain', args: ['--', '--exact', 'requested_sizes_one_through_seven_retire_once_and_reissue'],
    marker: 'test requested_sizes_one_through_seven_retire_once_and_reissue ... ok',
  }),
  green('miri-core', 'r11_ph4a_lease_miri', 'alloc-global alloc-xthread internals'),
  green('miri-core', 'r11_ph4b_lease_miri', 'alloc-global alloc-xthread internals'),
  green('miri-core', 'regression_r2_05_diag_provenance_miri', 'alloc-core internals'),
  green('miri-core', 'regression_w3_stats_aliasing_miri', 'std'),
  green('miri-core', 'class_leaves_publication_miri', 'alloc-core'),
  green('miri-plain', 'regression_r2_06_header_race_miri', plainFeatures, { model: 'plain' }),
  green('local-only', 'regression_realloc_oob_old_layout', 'alloc-core', {
    expect: 'WAIVED', card: 190,
    reason: 'Local-only until bounded Miri runtime is measured; no CI runtime receipt yet.',
  }),
  green('local-only', 'regression_virgin_bitmap_skip', 'alloc-core internals', {
    expect: 'WAIVED', card: 190,
    reason: 'Local-only until Miri runtime is measured; under cfg(miri) zero-skip itself is disabled.',
  }),
];
