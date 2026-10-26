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
  ...['regression_large_align_no_segment_exhaustion', 'regression_page_aligned_no_segment_exhaustion',
    'regression_realloc_cross_class_shrink', 'stress_boundary_sweep']
    .map((test) => green('miri-alloc-core', test, 'alloc-core')),
  green('miri-alloc-core', 'regression_r6_t22_foreign_window_key_miri', 'exact-span-large internals'),
  ...['regression_magazine_oracles', 'regression_bump_direct_refill']
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
  ...['sb', 'tb'].map((borrows) => green('miri-core', 'b4_r1_pin_cache_miri', 'alloc-global internals bench-internals', {
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
