// VER-03: one source for local execution and manual CI wiring verification.
// EXPECTED-RED strings below are verbatim CI grep expressions (card 164).
const green = (job, test, features = '', extra = {}) => ({
  job, test, features, model: 'strict', borrows: 'sb', args: [], expect: 'GREEN', ...extra,
});
const paused = (features, borrows) => green('miri-core-rest', 'miri_global_box_acceptance', features, {
  borrows, args: ['--', 'paused'], expect: 'EXPECTED-RED', card: 164,
  marker: '[miri_global_box_acceptance] COMPLETE paused_terminal_owner_retirement',
  redMessage: borrows === 'sb'
    ? 'Undefined Behavior: not granting access to tag .* weakly protected'
    : 'Undefined Behavior: write access through .* is forbidden',
  redSite: {
    frame0: '^[[:space:]]+0: sefer_segment::support::node_impl::Node::write_next$',
    file0: '^[[:space:]]+at crates/core/sefer-segment/src/support/node\\.rs:[0-9]+:[0-9]+(: [0-9]+:[0-9]+)?$',
    frame1: '^[[:space:]]+1: sefer_segment::support::node_impl::RootNode::write_next$',
    file1: '^[[:space:]]+at crates/core/sefer-segment/src/support/node\\.rs:[0-9]+:[0-9]+(: [0-9]+:[0-9]+)?$',
    frame2: '^[[:space:]]+2: sefer_alloc_core::alloc_core::platform::node::Node::write_next$',
    file2: '^[[:space:]]+at crates/core/sefer-alloc-core/src/alloc_core/platform/node\\.rs:[0-9]+:[0-9]+(: [0-9]+:[0-9]+)?$',
    caller: '^[[:space:]]+3: .*::(reclaim_sidecar_record|flush_run|dealloc_small)$',
  },
});
const narrow = (borrows) => green('miri-core-rest', 'miri_global_box_acceptance', 'alloc-global internals bench-internals', {
  borrows, args: ['--', 'narrow'], marker: '[miri_global_box_acceptance] COMPLETE narrow_two_rounds',
});
const r101 = (borrows) => green('miri-core-rest', 'miri_r1_01_cap_frame', 'alloc-global fastbin internals bench-internals', {
  borrows, marker: '[miri_r1_01_cap_frame] COMPLETE r1_01_cap_frame',
});
const depot = (borrows) => green('miri-core-rest', 'miri_depot_cycle', 'alloc-global fastbin internals bench-internals', {
  borrows, marker: '[miri_depot_cycle] COMPLETE depot_cycle',
});
const plainFeatures = 'alloc-global alloc-xthread alloc-decommit internals bench-internals';
export const MIRI_TARGETS = [
  green('miri-sefer-alloc-api-zst', 'miri_zst', '', { pkg: 'sefer-alloc-api' }),
  green('miri-core-rest', 'region_invariants'),
  green('miri-core-rest', 'advice_event_stream', 'bench-internals fault-injection', {
    pkg: 'aligned-vmem',
    marker: 'test cursor_wrap_never_turns_failed_capture_into_an_empty_success ... ok',
  }),
  green('miri-core-rest', 'advice_event_replay', 'bench-internals', {
    pkg: 'aligned-vmem',
    marker: 'test unknown_release_and_partial_advice_never_yield_zero_or_repaired_rows ... ok',
  }),
  green('miri-core-rest', 'j13b_advice_capture_native', 'production alloc-stats internals bench-internals lazy-commit-fault-injection', {
    marker: 'test native_advice_capture_rejects_unsupported_hosts_before_payload_claims ... ok',
  }),
  green('miri-core-rest', 'j13b_advice_windows_native', 'production alloc-stats internals bench-internals lazy-commit-fault-injection', {
    marker: 'test advice_windows_reject_models_before_clocks_capture_or_payload ... ok',
  }),
  green('miri-core-rest', 'j13b_native_identity', '', {
    marker: 'test process_facts_reject_models_before_files_commands_or_payload ... ok',
  }),
  green('miri-core-rest', 'j13b_build_seal', '', {
    marker: 'test duplicate_or_escaped_compiled_keys_cannot_silently_override ... ok',
  }),
  green('miri-core-rest', 'j13b_artifact_request', '', {
    marker: 'test duplicate_decoded_keys_and_escaped_aliases_are_rejected ... ok',
  }),
  green('miri-core-rest', 'advice_window', 'bench-internals', {
    pkg: 'aligned-vmem',
    marker: 'test historical_cumulative_peak_is_not_the_next_window_peak ... ok',
  }),
  green('miri-core-rest', 'advice_lifetime_ledger', 'bench-internals', {
    pkg: 'aligned-vmem',
    marker: 'test unsupported_release_preserves_entries_and_never_recovers_via_retry_or_reuse ... ok',
  }),
  ...['sb', 'tb'].map((borrows) => green('miri-core-rest', 'j13b_window_kernel', '', {
    borrows,
    marker: 'test four_workers_execute_and_all_joins_precede_an_error_result ... ok',
  })),
  ...['sb', 'tb'].map((borrows) => green('miri-core-rest', 'j13b_payload_actors', '', {
    borrows,
    marker: 'test persistent_actors_transfer_both_directions_on_actual_threads ... ok',
  })),
  green('miri-core-rest', 'j13b_owner_training_native', 'production internals bench-internals lazy-commit-fault-injection', {
    marker: 'test unsupported_training_is_rejected_before_payload_claims ... ok',
  }),
  green('miri-core-rest', 'j13b_adaptive_windows_native', 'production internals bench-internals', {
    marker: 'test adaptive_windows_reject_model_or_unsupported_os_before_payloads ... ok',
  }),
  green('miri-core-rest', 'j13b_os_probe', '', {
    marker: 'test unsupported_os_or_model_is_rejected_before_any_probe ... ok',
  }),
  ...['sb', 'tb'].map((borrows) => green('miri-core-rest', 'j13b_owned_block', '', {
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
  green('miri-core-rest', 'narrow_domain_unchecked_storage', '', { pkg: 'tagged-index-stack' }),
  green('miri-core-rest', 'decommit_miri_cycle', 'alloc-core alloc-decommit internals'),
  green('miri-core-rest', 'segment_directory_a5_miri', 'alloc-segment-directory internals'),
  // Ghost-checks G1: the ghost macros and shadow table under miri (cfg(miri) is a ghost build).
  green('miri-core-rest', 'ghost_infra', 'internals'),
  // Ghost-checks G2 (P5 route pins, item 186): last-reference free vs a concurrent pin drop on the real directory.
  green('miri-core-rest', 'route_pin_last_ref_miri', 'alloc-global internals'),
  // P0.2 pilot — only the regression_* tests that are rows below with target
  // `regression_merged` were merged into the single `regression_merged` target
  // (tests/regression_merged.rs + tests/regression_merged/<name>.rs); the `--`
  // name filters keep each row's executed test set identical to the pre-merge
  // per-target run. All other regression_* rows stay flat targets.
  green('miri-alloc-core', 'regression_merged', 'alloc-core', {
    args: ['--', 'regression_large_align_no_segment_exhaustion::',
      'regression_page_aligned_no_segment_exhaustion::', 'regression_realloc_cross_class_shrink::'],
  }),
  green('miri-alloc-core', 'stress_boundary_sweep', 'alloc-core'),
  green('miri-alloc-core', 'regression_merged', 'exact-span-large internals', {
    args: ['--', 'regression_r6_t22_foreign_window_key_miri::'],
  }),
  // NL-3: bounded cross-thread safe churn (cfg(miri) caps the sequence inside the test).
  green('miri-fastbin-oracles', 'regression_merged', 'alloc-global alloc-xthread alloc-decommit fastbin internals', {
    args: ['--', 'regression_magazine_oracles::'],
  }),
  ...['regression_bump_direct_refill', 'nl3_safe_xthread_churn_integrity']
    .map((test) => green('miri-fastbin-integrity-rest', test, 'alloc-global alloc-xthread alloc-decommit fastbin internals')),
  // NL-7 is a single measured ~1330s target (run 43984907030): too heavy to
  // share a shard, but the file has exactly 2 #[test] fns with unknown
  // per-test durations, so it is not further divisible without name filters.
  green('miri-fastbin-integrity-nl7', 'nl7_safe_large_realloc_integrity', 'alloc-global alloc-xthread alloc-decommit fastbin internals'),
  // Sharded by measured target wall time (docs/perf/CI_LONG_JOBS.md): the
  // b4-r1 pin-cache cell (~1041s, run 43984907030) is the single heaviest
  // plain target and gets its own shard; the rest of the plain suite
  // (including the ~8s large free/no-leak regression) is the other shard.
  green('miri-plain-heavy', 'b4_r1_pin_cache_miri', plainFeatures, { model: 'plain' }),
  // Task #223: the sidecar zero-fill/free-backing oracle has a miri-reduced
  // mode (checked budget) and MUST run under miri: only miri exercises the
  // `explicit_zero_fill_words` fill path of the VM-span backend (the miri
  // allocator is System-backed and does not hand back zeroed bytes).
  ...['regression_xthread_large_free_no_leak', 'regression_xthread_thread_free_alias_miri', 'regression_xthread_small_ring_miri']
    .map((test) => green('miri-plain-rest', test, plainFeatures, { model: 'plain' })),
  green('miri-plain-rest', 'p223_small_sidecar_zero_free_backing',
    `production ${plainFeatures}`, { model: 'plain' }),
  // b4-r1 round 2 (P2): the same cell under narrow provenance — REAL Box
  // ownership transported via a Send wrapper, cache-hit activation oracle
  // (hits delta >= 1 reds a "publish_cached always Miss" mutant), gated
  // pending-bit delivery and TLS thread-exit release. plain stays because
  // the file is plain-compatible (no strict-provenance requirement) and its
  // preemption=0.5 coverage of the start gate schedule is retained.
  // The TreeBorrows cell runs in its own job (miri-core-tb): it is the longest step of the workflow.
  ...['sb', 'tb'].map((borrows) => green(borrows === 'tb' ? 'miri-core-tb' : 'miri-core-pin-cache', 'b4_r1_pin_cache_miri', 'alloc-global internals bench-internals', {
    borrows, args: ['--', 'narrow'],
    marker: '[b4_r1_pin_cache_miri] COMPLETE narrow',
  })),
  green('miri-plain-rest', 'r6_terminal_owner_drain', 'alloc-global alloc-xthread internals bench-internals', {
    model: 'plain', args: ['--', '--exact', 'requested_sizes_one_through_seven_retire_once_and_reissue'],
    marker: 'test requested_sizes_one_through_seven_retire_once_and_reissue ... ok',
  }),
  green('miri-core-rest', 'r11_ph4a_lease_miri', 'alloc-global alloc-xthread internals'),
  green('miri-core-rest', 'r11_ph4b_lease_miri', 'alloc-global alloc-xthread internals'),
  green('miri-core-rest', 'regression_merged', 'alloc-core internals', {
    args: ['--', 'regression_r2_05_diag_provenance_miri::'],
  }),
  green('miri-core-rest', 'regression_merged', 'std', {
    args: ['--', 'regression_w3_stats_aliasing_miri::'],
  }),
  green('miri-core-rest', 'class_leaves_publication_miri', 'alloc-core'),
  green('miri-plain-rest', 'regression_r2_06_header_race_miri', plainFeatures, { model: 'plain' }),
  green('local-only', 'regression_merged', 'alloc-core', {
    args: ['--', 'regression_realloc_oob_old_layout::'],
    expect: 'WAIVED', card: 190,
    reason: 'Local-only until bounded Miri runtime is measured; no CI runtime receipt yet.',
  }),
  green('local-only', 'regression_merged', 'alloc-core internals', {
    args: ['--', 'regression_virgin_bitmap_skip::'],
    expect: 'WAIVED', card: 190,
    reason: 'Local-only until Miri runtime is measured; under cfg(miri) zero-skip itself is disabled.',
  }),
];
