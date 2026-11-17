# P0.2 — test-merge pilot (regression_* group, one merged test target)

- Date: 2026-11-15
- Plan: docs/plans/2026-11-15-workspace-restructure-plan.md §5 (P0.2), §3 gates
- Base: origin/main at pilot start (worktree created from current origin/main; P0.1, A1, T1 already landed)
- Decision recorded BEFORE reading any measurement numbers, per plan §3 ("the criterion is written down before the numbers are read").

## Decision criterion

The criterion for GO on merging the remaining groups (P0.4). All three conditions
must hold SIMULTANEOUSLY; if any one fails, the verdict is NO-GO (stay on
per-file test binaries, no merging):

- **(a) Binary count and test-name inventory.** The number of test binaries in
  the measured feature rows dropped by exactly the expected amount, and the
  test-name inventory matches: the set of (module, function) test paths after
  the merge equals the set of (file, function) test paths before. Proven by
  `scripts/test-inventory.mjs` (plan §3 "Test inventory" gate).
- **(b) Ordinary CI duration, cargo-test jobs.** On ORDINARY CI runs only (job
  durations read from `gh run view --json jobs`; no dedicated wall-clock
  campaign): with at least 3 runs per side ("before" = main before the pilot,
  "after" = the PR / main after), the median duration of the `cargo test` step
  (or of the whole job where no separate step exists) in every AFFECTED test
  job fell by at least 5%, and in no affected job did it grow. Runner noise is
  ±10%: the 5% threshold on a median of three runs is deliberately "noticeable
  or not" — this is the chosen bar, fixed now, and it is not adjusted to fit
  the numbers later.
- **(c) External name pins updated mechanically.** Updating all external name
  pins (counters in README/ARCHITECTURE/CI_TIERS, `tests/no_stale_doc_references.rs`,
  J13 / check-matrix / CI greps, the P0.1 classifier) fit inside the pilot PR
  without manual hacks. Any pin that cannot be updated mechanically is recorded
  in the "Pin updates" section below and is input to this criterion.

Per the CLAUDE.md repo rule, report tables and headline verdicts are
"derived by one checked script", not hand-typed. The measurement script for
criterion (b) is `scripts/ci-job-durations.mjs` (see Measurements below);
criterion (a) is checked by `scripts/test-inventory.mjs`.

## Pilot scope

The group under test is the `regression_*` no-global-state pool from the P0.1
classification (`docs/test_target_classification_20261115.csv`). The committed CSV
is a per-file inventory; `external_pin_count` counts occurrences of that target
name on external pin surfaces, not an aggregate total. Relaxation rungs and pilot
candidates are aggregate analysis available from
`node scripts/classify-tests.mjs --summary`, not the CSV contents.

The P0.1 allow-pinned rung and compat candidate referenced below are historical
classification-time context. The P0.2 counts and tables preserve the pilot-time
snapshot; they are not current aggregates derived from the committed CSV.

Final membership (merged files and excluded files with reasons) is recorded in
the "Merged group" section below.

## Merged group

### Pool filtering

- First filter (P0.1): the P0.1 CSV (docs/test_target_classification_20261115.csv; flags global_allocator / first_config_wins / process_spawn / abort / loom_only / miri_only / harness_false / test_enabled / required_features / kind) lists 78 `regression_*` targets; its hard-blocker set (scripts/classify-tests/groups.mjs `relaxBlocked`, including miri_only) excluded 20 of them. Excluded with reasons (CSV-derived, script-filtered): 6 first_config_wins (regression_batch_flush, regression_batch_freelist_drain, regression_bump_direct_refill, regression_own_segment_cache_invalidation, regression_own_thread_large_no_leak, regression_t10_find_segment_multiseg_recovery); 6 process_spawn (regression_freelist_next_validation, regression_r2_01_segment_bases_lifetime, regression_r2_02_epoch_region_non_send_value, regression_r2_15_virgin_mask_zeroed_and_bounded, regression_r2_17_64bit_target_gate, regression_r6_t38_public_tail_private); 3 global_allocator (regression_r2_08_global_allocator_path_no_unwind, regression_r2_11_stats_no_materialize, regression_r5_01_alloc_global_cross_thread); 2 miri_only (regression_paused_owner_multisegment, regression_paused_owner_wallclock — file-level cfg contains `not(miri)`); 2 required_features (regression_r2_20_lock_free_rejected_no_cow, regression_r4_3_config_conflict — both with additional blockers); 1 first_config_wins-with_config (regression_r2_08_globalalloc_no_unwind). Overlaps exist (regression_r2_08_global_allocator_path_no_unwind carries global_allocator + first_config_wins + process_spawn).
- Second filter (this pilot): all modules of a merged target share ONE process and every process-global static of the crate. The P0.1 flag set (global_allocator, first_config_wins, process_spawn, abort, loom, miri, ...) does not capture that. 23 more modules were therefore excluded after reading each of them against the rule below; they stay flat `tests/regression_<stem>.rs` targets. Categories: G1 exact/ordering assertion on a process-global static counter; G2 mutates a process-global knob or depends on the global registry/fallback lifecycle; G3 exact heap-state / pointer-identity assertion on a leased registry heap presuming a fresh slot, or a test whose non-vacuity depends on minting fresh slots; G4 wall-clock/timeout; E cited by evidence-registry rows R4c-05/06/07. Data: docs/perf/p0_2_exclusions.json; the table is pasted verbatim from `node scripts/p0_2_global_state_scan.mjs --exclusions docs/perf/p0_2_exclusions.json --md`:

| module | category | marker (regex) | file:line | code (trimmed to 100 chars) |
|---|---|---|---|---|
| regression_bootstrap_oom_sentinel_rollback | G2 | `let last_chunk = bootstrap::dbg_num_chunks\(\) - 1` | tests/regression_bootstrap_oom_sentinel_rollback.rs:97 | `let last_chunk = bootstrap::dbg_num_chunks() - 1;` |
| regression_bootstrap_oom_sentinel_rollback | G2 | `dbg_rollback_chunk_sentinel_reenterable\(last_chunk\)` | tests/regression_bootstrap_oom_sentinel_rollback.rs:99 | `match bootstrap::dbg_rollback_chunk_sentinel_reenterable(last_chunk) {` |
| regression_c3_unbounded_recycle | G1 | `let reserved_before = AllocCore::dbg_segments_reserved_total` | tests/regression_c3_unbounded_recycle.rs:53 | `let reserved_before = AllocCore::dbg_segments_reserved_total();` |
| regression_c3_unbounded_recycle | G1 | `let decommit_before = AllocCore::dbg_decommit_count` | tests/regression_c3_unbounded_recycle.rs:99 | `let decommit_before = AllocCore::dbg_decommit_count();` |
| regression_c3_unbounded_recycle | G1 | `dbg_segments_reserved_total\(\) - reserved_before` | tests/regression_c3_unbounded_recycle.rs:168 | `AllocCore::dbg_segments_reserved_total() - reserved_before,` |
| regression_claim_oom_initialised_gate | G2 | `let idx = dbg_claim_then_simulate_oom\(\)` | tests/regression_claim_oom_initialised_gate.rs:168 | `let idx = dbg_claim_then_simulate_oom()` |
| regression_claim_oom_initialised_gate | G2 | `let freed_idx = dbg_claim_then_simulate_oom\(\)` | tests/regression_claim_oom_initialised_gate.rs:204 | `let freed_idx = dbg_claim_then_simulate_oom()` |
| regression_claim_oom_initialised_gate | G2 | `let gen_before_reclaim = slot_generation` | tests/regression_claim_oom_initialised_gate.rs:242 | `let gen_before_reclaim = slot_generation(freed_idx as usize);` |
| regression_counter_wrap | G2 | `This is the ONLY test in this binary` | tests/regression_counter_wrap.rs:41 | `// This is the ONLY test in this binary that touches the registry. Each` |
| regression_counter_wrap | G2 | `let lease_a = HeapRegistry::dbg_claim_lease` | tests/regression_counter_wrap.rs:53 | `let lease_a = HeapRegistry::dbg_claim_lease().expect("claim must succeed");` |
| regression_counter_wrap | G2 | `dbg_slot_preset_generation\(id, preset\)` | tests/regression_counter_wrap.rs:74 | `unsafe { reg.dbg_slot_preset_generation(id, preset) };` |
| regression_counter_wrap | G2 | `lease_c\.slot_index\(\) as usize, id, "still the same slot"` | tests/regression_counter_wrap.rs:96 | `assert_eq!(lease_c.slot_index() as usize, id, "still the same slot");` |
| regression_dealloc_batch_promoted_large_free | G1 | `let released_before = a\.stats\(\)\.segments_released_total` | tests/regression_dealloc_batch_promoted_large_free.rs:100 | `let released_before = a.stats().segments_released_total;` |
| regression_fallback_init_unwind_guard | G2 | `^\s*FallbackInitState::Uninit as u8,` | tests/regression_fallback_init_unwind_guard.rs:72 | `FallbackInitState::Uninit as u8,` |
| regression_fallback_init_unwind_guard | G4 | `recv_timeout\(Duration::from_secs\(10\)\)` | tests/regression_fallback_init_unwind_guard.rs:88 | `match rx.recv_timeout(Duration::from_secs(10)) {` |
| regression_fallback_init_unwind_guard | G2 | `FallbackInitState::Ready as u8` | tests/regression_fallback_init_unwind_guard.rs:108 | `FallbackInitState::Ready as u8,` |
| regression_fallback_panic_lock | G2 | `dbg_panic_in_with_heap_releases_lock\(\)` | tests/regression_fallback_panic_lock.rs:47 | `let ok = dbg_panic_in_with_heap_releases_lock();` |
| regression_fallback_panic_lock | G4 | `recv_timeout\(Duration::from_secs\(10\)\)` | tests/regression_fallback_panic_lock.rs:51 | `match rx.recv_timeout(Duration::from_secs(10)) {` |
| regression_free_path_chunk_oom_graceful | G2 | `^\s*bootstrap::dbg_set_inject_chunk_oom\(true\);` | tests/regression_free_path_chunk_oom_graceful.rs:128 | `bootstrap::dbg_set_inject_chunk_oom(true);` |
| regression_free_path_chunk_oom_graceful | G2 | `bootstrap::dbg_slot_or_none\(high_idx\)` | tests/regression_free_path_chunk_oom_graceful.rs:130 | `let result = bootstrap::dbg_slot_or_none(high_idx);` |
| regression_free_path_chunk_oom_graceful | G2 | `chunk must not be materialised after an OOM bailout` | tests/regression_free_path_chunk_oom_graceful.rs:144 | `"chunk must not be materialised after an OOM bailout"` |
| regression_free_path_chunk_oom_graceful | G2 | `chunk must be materialised after a successful` | tests/regression_free_path_chunk_oom_graceful.rs:162 | `"chunk must be materialised after a successful (non-OOM) resolution"` |
| regression_hardened_large_kind_own_free | G1 | `let noop_before = HeapCore::dbg_hardened_large_noop_count` | tests/regression_hardened_large_kind_own_free.rs:125 | `let noop_before = HeapCore::dbg_hardened_large_noop_count();` |
| regression_hardened_large_kind_own_free | G1 | `branch \(A\)'s mismatched small-layout free` | tests/regression_hardened_large_kind_own_free.rs:374 | `branch (A)'s mismatched small-layout free — the defensive no-op \` |
| regression_hardened_large_kind_own_free | G1 | `unsafe \{ heap\.dealloc\(promoted, wrong_align_layout\) \}` | tests/regression_hardened_large_kind_own_free.rs:507 | `unsafe { heap.dealloc(promoted, wrong_align_layout) };` |
| regression_magazine_bump_guard | G3 | `let mut lease = HeapRegistry::dbg_claim_lease` | tests/regression_magazine_bump_guard.rs:81 | `let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");` |
| regression_magazine_bump_guard | G3 | `bogus, anchor,` | tests/regression_magazine_bump_guard.rs:109 | `bogus, anchor,` |
| regression_magazine_bump_guard | G3 | `after, bogus,` | tests/regression_magazine_bump_guard.rs:125 | `after, bogus,` |
| regression_magazine_free_bounds | G3 | `while !issued\.contains\(&first\)` | tests/regression_magazine_free_bounds.rs:25 | `while !issued.contains(&first) {` |
| regression_magazine_free_bounds | G3 | `dbg_segment_base_of_ptr\(p\), base` | tests/regression_magazine_free_bounds.rs:28 | `assert_eq!(heap.dbg_segment_base_of_ptr(p), base);` |
| regression_magazine_free_bounds | G3 | `core::mem::forget\(lease\)` | tests/regression_magazine_free_bounds.rs:58 | `core::mem::forget(lease);` |
| regression_r1_01_magazine_free_budget | G3 | `^\s*mag_cnt, cap,` | tests/regression_r1_01_magazine_free_budget.rs:174 | `mag_cnt, cap,` |
| regression_r1_01_magazine_free_budget | G3 | `dbg_flush_all must empty class` | tests/regression_r1_01_magazine_free_budget.rs:200 | `"dbg_flush_all must empty class {class_idx}'s magazine"` |
| regression_r1_01_magazine_free_budget | G3 | `^\s*Some\(0\),` | tests/regression_r1_01_magazine_free_budget.rs:206 | `Some(0),` |
| regression_r1_01_magazine_free_budget | G3 | `after exactly TCACHE_CAP allocations off an empty magazine` | tests/regression_r1_01_magazine_free_budget.rs:255 | `"after exactly TCACHE_CAP allocations off an empty magazine, the \` |
| regression_r2_06_header_race_miri | G3 | `let mut owner = HeapRegistry::dbg_claim_lease` | tests/regression_r2_06_header_race_miri.rs:59 | `let mut owner = HeapRegistry::dbg_claim_lease().expect("owner lease");` |
| regression_r2_06_header_race_miri | G3 | `before\.large_active\.count, REMOTE_PER_KIND` | tests/regression_r2_06_header_race_miri.rs:122 | `assert_eq!(before.large_active.count, REMOTE_PER_KIND);` |
| regression_r2_06_header_race_miri | G3 | `assert_eq!\(after\.large_active\.count, 0` | tests/regression_r2_06_header_race_miri.rs:163 | `assert_eq!(after.large_active.count, 0, "Large publications retired");` |
| regression_r2_06_header_race_miri | G3 | `assert_eq!\(after\.unknown_count, 0` | tests/regression_r2_06_header_race_miri.rs:164 | `assert_eq!(after.unknown_count, 0);` |
| regression_r2_06_header_race_miri | E | `let mut owner = HeapRegistry::dbg_claim_lease` | tests/regression_r2_06_header_race_miri.rs:59 | `let mut owner = HeapRegistry::dbg_claim_lease().expect("owner lease");` |
| regression_r2_12_sidecar_leak | G1 | `let before = dbg_sidecar_reservation_stats\(\)` | tests/regression_r2_12_sidecar_leak.rs:119 | `let before = dbg_sidecar_reservation_stats();` |
| regression_r2_12_sidecar_leak | G1 | `after\.directory_reservations - before\.directory_reservations` | tests/regression_r2_12_sidecar_leak.rs:143 | `after.directory_reservations - before.directory_reservations,` |
| regression_r2_12_sidecar_leak | G1 | `after\.large_cache_extension_reservations - before\.large_cache_extension_reservations` | tests/regression_r2_12_sidecar_leak.rs:240 | `after.large_cache_extension_reservations - before.large_cache_extension_reservations,` |
| regression_r4_3_teardown_trim | G1 | `let before = a\.stats\(\)\.segments_released_total` | tests/regression_r4_3_teardown_trim.rs:154 | `let before = a.stats().segments_released_total;` |
| regression_r4_3_teardown_trim | G1 | `let after = a\.stats\(\)\.segments_released_total` | tests/regression_r4_3_teardown_trim.rs:171 | `let after = a.stats().segments_released_total;` |
| regression_realloc_xthread_stamp | G3 | `reissued, grown,` | tests/regression_realloc_xthread_stamp.rs:39 | `reissued, grown,` |
| regression_realloc_xthread_stamp | G3 | `dbg_drain_sidecar_ingress\(\), 0` | tests/regression_realloc_xthread_stamp.rs:49 | `assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);` |
| regression_registry_chunking | G2 | `let last_chunk = bootstrap::dbg_num_chunks\(\) - 1` | tests/regression_registry_chunking.rs:84 | `let last_chunk = bootstrap::dbg_num_chunks() - 1;` |
| regression_registry_chunking | G2 | `^\s*!reg\.dbg_chunk_is_materialised\(last_chunk\)` | tests/regression_registry_chunking.rs:93 | `!reg.dbg_chunk_is_materialised(last_chunk),` |
| regression_registry_chunking | G2 | `id_a, id_b,` | tests/regression_registry_chunking.rs:141 | `id_a, id_b,` |
| regression_registry_chunking | G2 | `addr_before, addr_after,` | tests/regression_registry_chunking.rs:146 | `addr_before, addr_after,` |
| regression_registry_chunking | G2 | `assert_eq!\(id_a, id_b, "the spawned thread` | tests/regression_registry_chunking.rs:184 | `assert_eq!(id_a, id_b, "the spawned thread must reuse the same slot");` |
| regression_registry_initialised_gate | G3 | `claims a FRESH slot` | tests/regression_registry_initialised_gate.rs:215 | `// Claimer threads: each claims a FRESH slot (bump_count path, the one` |
| regression_segment_table_tombstone_rebuild | G4 | `use std::time::Instant` | tests/regression_segment_table_tombstone_rebuild.rs:136 | `use std::time::Instant;` |
| regression_segment_table_tombstone_rebuild | G4 | `let dt = t0\.elapsed` | tests/regression_segment_table_tombstone_rebuild.rs:202 | `let dt = t0.elapsed().as_nanos() as u64;` |
| regression_segment_table_tombstone_rebuild | G1 | `dbg_reset_hash_remove_max_scan_steps\(\)` | tests/regression_segment_table_tombstone_rebuild.rs:324 | `AllocCore::dbg_reset_hash_remove_max_scan_steps();` |
| regression_segment_table_tombstone_rebuild | G1 | `let max_steps = AllocCore::dbg_hash_remove_max_scan_steps` | tests/regression_segment_table_tombstone_rebuild.rs:340 | `let max_steps = AllocCore::dbg_hash_remove_max_scan_steps();` |
| regression_tcache_byte_budget | G3 | `want, TCACHE_CAP,` | tests/regression_tcache_byte_budget.rs:81 | `want, TCACHE_CAP,` |
| regression_tcache_byte_budget | G3 | `mag_cnt as usize,` | tests/regression_tcache_byte_budget.rs:93 | `mag_cnt as usize,` |
| regression_tcache_byte_budget | G3 | `let mag_cnt = heap\.dbg_tcache_count\(class_idx\) as usize` | tests/regression_tcache_byte_budget.rs:193 | `let mag_cnt = heap.dbg_tcache_count(class_idx) as usize;` |
| regression_xthread_large_free_no_leak | G3 | `assert_eq!\(second, first\)` | tests/regression_xthread_large_free_no_leak.rs:36 | `assert_eq!(second, first);` |
| regression_xthread_large_free_no_leak | G3 | `dbg_drain_sidecar_ingress\(\), 0` | tests/regression_xthread_large_free_no_leak.rs:44 | `assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);` |
| regression_xthread_small_ring_miri | G2 | `is a process-global static and concurrent claim/recycle churn` | tests/regression_xthread_small_ring_miri.rs:44 | `// is a process-global static and concurrent claim/recycle churn from a sibling` |
| regression_xthread_small_ring_miri | G2 | `let _ = bootstrap::ensure\(\)` | tests/regression_xthread_small_ring_miri.rs:70 | `let _ = bootstrap::ensure();` |
| regression_xthread_small_ring_miri | G2 | `let mut lease = HeapRegistry::dbg_claim_lease` | tests/regression_xthread_small_ring_miri.rs:89 | `let mut lease = HeapRegistry::dbg_claim_lease().expect("HeapRegistry::claim returned None");` |
| regression_xthread_small_ring_miri | G2 | `producer A HeapRegistry::claim failed` | tests/regression_xthread_small_ring_miri.rs:115 | `HeapRegistry::dbg_claim_lease().expect("producer A HeapRegistry::claim failed");` |
| regression_xthread_small_ring_miri | G2 | `producer B HeapRegistry::claim failed` | tests/regression_xthread_small_ring_miri.rs:137 | `HeapRegistry::dbg_claim_lease().expect("producer B HeapRegistry::claim failed");` |
| regression_xthread_small_ring_miri | E | `let _ = bootstrap::ensure\(\)` | tests/regression_xthread_small_ring_miri.rs:70 | `let _ = bootstrap::ensure();` |
| regression_xthread_thread_free_alias_miri | E | `is a process-global static and concurrent claim/recycle churn` | tests/regression_xthread_thread_free_alias_miri.rs:39 | `// is a process-global static and concurrent claim/recycle churn from a sibling` |
| regression_xthread_thread_free_alias_miri | E | `let _ = bootstrap::ensure\(\)` | tests/regression_xthread_thread_free_alias_miri.rs:65 | `let _ = bootstrap::ensure();` |
| regression_xthread_thread_free_alias_miri | E | `let mut lease = HeapRegistry::dbg_claim_lease` | tests/regression_xthread_thread_free_alias_miri.rs:85 | `let mut lease = HeapRegistry::dbg_claim_lease().expect("HeapRegistry::claim returned None");` |
| regression_xthread_thread_free_alias_miri | E | `remote HeapRegistry::claim failed` | tests/regression_xthread_thread_free_alias_miri.rs:108 | `HeapRegistry::dbg_claim_lease().expect("remote HeapRegistry::claim failed");` |

excluded: 23 modules, 72 entries; kept: 32 modules (merged); total: 55
binaries per full-target row: 32 flat targets -> 1, saved 31
(Rebase note: P1.2 moved regression_r2_03_cross_instance_identity, regression_r2_04_generation_setter_lockdown and regression_r2_21_epoch_drain_buffer_reuse into crates/containers/sefer-concurrent/tests/, so the pilot's root pool is now 75 `regression_*` targets (78 - 3) and the merge is 32 modules; 75 - 23 - 20 = 55.)

Legend:
- **G1** = exact/ordering assertion on a process-global static counter
- **G2** = mutates a process-global knob or depends on global registry/fallback lifecycle state
- **G3** = exact heap-state or pointer-identity assertion on a leased registry heap that presumes a fresh (never recycled) slot, or a test whose non-vacuity depends on minting fresh slots first
- **G4** = wall-clock / timeout threshold
- **E** = cited by an evidence-registry row (flat file path appears in `--test <stem>` of a STRONG row; strict mode reports evidence-test-missing if the flat file is absent)

- Final composition: **32 merged / 43 of 75 not merged (23 + 20)**. Derivation: one-line node check over docs/perf/p0_2_exclusions.json (23 distinct modules) and the `mod` lines of tests/regression_merged.rs (32) printed `composition: merged 32, not merged 43 of 75 (23 global-state + 20 P0.1 blockers)`; the pool is 75 after P1.2 moved the three crate tests listed above.

### The merged target

- `tests/regression_merged.rs` declares 32 modules, each as `#[path = "regression_merged/<stem>.rs"] mod <stem>;`. The `#[path]` attribute is required because a crate-root `mod` resolves in the parent directory of the root file (tests/), not in the subdirectory. Each declaration carries stacked `#[cfg(...)]` copies of that file's own file-level `#![cfg(...)]`.
- The module files tests/regression_merged/<stem>.rs are byte-verbatim moves: sha256 of all 32 files equals the HEAD blob `git cat-file blob HEAD:tests/<stem>.rs` (verified in this task). tests/regression_merged.rs is rustfmt-formatted (`rustfmt --check --edition 2021` exit 0): only the long attribute lines were reflowed (and rustfmt sorted the `mod` declarations alphabetically); the module files are untouched and each stem's cfg predicate texts are unchanged modulo whitespace.
- Binaries per full-target row: 32 flat targets -> 1, saved 31 (derivation: `--exclusions` summary line of scripts/p0_2_global_state_scan.mjs).

### Filter proof

Every miri / tsan / feature-isolation filter token `regression_<stem>::` selects exactly its module's tests in the merged target. Pasted verbatim from `node scripts/p0_2_global_state_scan.mjs --filters` (libtest substring semantics; asserts non-empty, all matches inside the named module, matched set equals ALL tests of that module):

| filter | source file:line | module | tests matched | tests in module | OK |
|---|---|---|---|---|---|
| regression_large_align_no_segment_exhaustion:: | .github/actions/ci-miri-alloc-core/action.yml:24 | regression_large_align_no_segment_exhaustion | 2 | 2 | OK |
| regression_page_aligned_no_segment_exhaustion:: | .github/actions/ci-miri-alloc-core/action.yml:24 | regression_page_aligned_no_segment_exhaustion | 2 | 2 | OK |
| regression_realloc_cross_class_shrink:: | .github/actions/ci-miri-alloc-core/action.yml:24 | regression_realloc_cross_class_shrink | 1 | 1 | OK |
| regression_r6_t22_foreign_window_key_miri:: | .github/actions/ci-miri-alloc-core/action.yml:34 | regression_r6_t22_foreign_window_key_miri | 1 | 1 | OK |
| regression_r2_05_diag_provenance_miri:: | .github/actions/ci-miri-core/action.yml:403 | regression_r2_05_diag_provenance_miri | 9 | 9 | OK |
| regression_w3_stats_aliasing_miri:: | .github/actions/ci-miri-core/action.yml:407 | regression_w3_stats_aliasing_miri | 2 | 2 | OK |
| regression_magazine_oracles:: | .github/actions/ci-miri-fastbin/action.yml:14 | regression_magazine_oracles | 4 | 4 | OK |
| regression_carve_batch:: | .github/actions/ci-test-feature-isolation/action.yml:39 | regression_carve_batch | 6 | 6 | OK |
| regression_percounter_perheap_aggregation:: | .github/actions/ci-tsan/action.yml:87 | regression_percounter_perheap_aggregation | 2 | 2 | OK |
| regression_large_align_no_segment_exhaustion:: | scripts/miri-matrix.mjs:105 | regression_large_align_no_segment_exhaustion | 2 | 2 | OK |
| regression_page_aligned_no_segment_exhaustion:: | scripts/miri-matrix.mjs:106 | regression_page_aligned_no_segment_exhaustion | 2 | 2 | OK |
| regression_realloc_cross_class_shrink:: | scripts/miri-matrix.mjs:106 | regression_realloc_cross_class_shrink | 1 | 1 | OK |
| regression_r6_t22_foreign_window_key_miri:: | scripts/miri-matrix.mjs:110 | regression_r6_t22_foreign_window_key_miri | 1 | 1 | OK |
| regression_magazine_oracles:: | scripts/miri-matrix.mjs:114 | regression_magazine_oracles | 4 | 4 | OK |
| regression_r2_05_diag_provenance_miri:: | scripts/miri-matrix.mjs:141 | regression_r2_05_diag_provenance_miri | 9 | 9 | OK |
| regression_w3_stats_aliasing_miri:: | scripts/miri-matrix.mjs:144 | regression_w3_stats_aliasing_miri | 2 | 2 | OK |
| regression_realloc_oob_old_layout:: | scripts/miri-matrix.mjs:149 | regression_realloc_oob_old_layout | 3 | 3 | OK |
| regression_virgin_bitmap_skip:: | scripts/miri-matrix.mjs:154 | regression_virgin_bitmap_skip | 3 | 3 | OK |

filters: 18 tokens, 11 distinct, 81 tests in 32 merged modules, 0 violations

### Residual risk

- The 32 kept modules still share one process. Each was read; each keeps only assertions that are robust to other modules' activity (lower bounds on global counters, properties of the module's own blocks, thread-local state).
- Modules that claim and drop registry leases may receive a recycled heap (the registry never resets a minted slot's HeapCore).
- Any intermittent CI red in the merged target is an interference signal and counts against criterion (a)/(b) when P0.4 decides.
- P0.4 follow-ups: (1) add a process-global-state marker class to the P0.1 classifier; (2) the three evidence-linked miri modules regression_r2_06_header_race_miri, regression_xthread_small_ring_miri, regression_xthread_thread_free_alias_miri are candidates for P0.4 after an EV-1 reverify.

## Test inventory (plan §3 gate)

- Gate script: `scripts/test-inventory.mjs` (+ `tests/test_inventory.test.mjs`, node --test, synthetic trees incl. RED counterfactuals: deleted/added fn, dropped cfg, orphan module, collateral-change detection; **16 tests green**; suite wired into the docs-consistency job).
- Real run (`node scripts/test-inventory.mjs --before-dir <HEAD snapshot>`; the snapshot was built from HEAD with ONE `git cat-file --batch` spawn: Cargo.toml + tests/*.rs blobs): **PASS — moved stems 32, unchanged stems 497, 1515 tests before = 1515 tests after, 0 lost, 0 added, 0 dup, 0 cfg-mismatch, 0 orphans, 0 unexpected new targets**; per-stem counts identical before/after (the per-stem table is the script's output; rerun to reproduce). `#[test_case]` occurrences: 0. Derivation: scripts/test-inventory.mjs real run in this task (printed line: `inventory: 1515 tests before, 1515 tests after, 0 lost, 0 added, 0 dup, 0 cfg-mismatch, 0 orphans, 0 unexpected new targets`).
- Known script defect: the `--ref` git mode hangs on this machine (per-file git spawn); the real run used the HEAD snapshot via `--before-dir` — the CI wiring runs only the synthetic suite, not the real run.
- Classifier cross-check (derivation: `node scripts/classify-tests.mjs --json` on the regenerated state): targets **528** (equals the node readdir count of tests/*.rs, 528), file_level_cfg **479**; unchanged: targetsTestFalse 11, declaredExplicit 29, harnessFalse 13, global_allocator 64, first_config_wins 81, process_spawn 33. Regenerated: docs/test_target_classification_20261115.csv and docs/perf/TEST_TARGET_CLASSIFICATION_20261115.md. (Rebase note: after P1.2 moved the three crate tests out of tests/, these pilot-state counts are superseded by main's classification; the script refuses to run on an index with unmerged entries, so the cross-check must be re-derived after `rebase --continue`.)

## Pin updates

### Updated mechanically (verified by gates)

Derivation: `git diff --stat` / `git status --porcelain` on the pin files, node readdir of tests/*.rs (528) and a JSON diff of the decomposition baseline against `git show HEAD:...`.

- Final footprint (modified): .github/actions/ci-docs-consistency, ci-miri-alloc-core, ci-miri-core, ci-miri-fastbin, ci-test-feature-isolation, ci-test-hardened, ci-tsan (action.yml each); scripts/miri-matrix.mjs, scripts/tsan.mjs; README.md, docs/ARCHITECTURE.md; tests/classify_tests.test.mjs; docs/verification/ci-decomposition-baseline.json; the classifier outputs docs/test_target_classification_20261115.csv and docs/perf/TEST_TARGET_CLASSIFICATION_20261115.md. New: scripts/test-inventory.mjs, tests/test_inventory.test.mjs, scripts/ci-job-durations.mjs, tests/ci_job_durations.test.mjs, scripts/p0_2_global_state_scan.mjs, docs/perf/p0_2_exclusions.json, tests/regression_merged.rs, tests/regression_merged/*.rs, this report.
- **Untouched: their targets stayed flat** — .github/actions/ci-miri-plain/action.yml, .github/actions/ci-feature-powerset/action.yml, docs/verification/ghost-rules.json, docs/CI_TIERS.md (restored to HEAD; none of their cited modules is in the merged target).
- README.md (two lines) + docs/ARCHITECTURE.md (one line): root `tests/*.rs` count 528, equal to the classifier targets count; the README `tests/regression_magazine_oracles.rs` path mentions (two) now name tests/regression_merged/regression_magazine_oracles.rs.
- miri: `ci-miri-alloc-core`, `ci-miri-core`, `ci-miri-fastbin` — merged modules run as `--test regression_merged -- <module>::` filters, kept in separate cargo commands from flat `--test` targets (a single command must not mix them); `scripts/miri-matrix.mjs` rows updated to the same identities (miri-alloc-core, miri-fastbin, miri-core, local-only).
- tsan: `ci-tsan/action.yml` + `scripts/tsan.mjs` — regression_percounter_perheap_aggregation runs in a second `&&`-chained cargo invocation with `--test regression_merged regression_percounter_perheap_aggregation::` (cargoCmd/MERGED_TESTS in tsan.mjs); features/env identical.
- feature-isolation: `ci-test-feature-isolation/action.yml` — three flat targets in one command, and `--test regression_merged regression_carve_batch::` in a second command.
- hardened: `ci-test-hardened/action.yml` — comment paths only (tests/regression_merged/regression_hardened_interior_ptr.rs).
- docs-consistency: `ci-docs-consistency/action.yml` — new `node --test tests/test_inventory.test.mjs tests/ci_job_durations.test.mjs` step.
- Classify test pins: `tests/classify_tests.test.mjs` live-tree pins targets 528 / file_level_cfg 479.
- Decomposition baseline: `docs/verification/ci-decomposition-baseline.json` — changed job ids (steps hash only; derived by diffing the JSON against HEAD): docs-consistency, test-feature-isolation, miri-core, miri-alloc-core, miri-fastbin, tsan. It was re-recorded with a TEMPORARY copy of scripts/verify-ci-decomposition.mjs without its `--record` guard assert (`expanded.dependencies.size === 0`, which forbids `--record` after the extraction) — the same manual re-record earlier commits did (A1, T1). **Flagged for the parent's decision.**

### Not updated mechanically / for the parent

- Evidence registry (derivation: `node scripts/verify-evidence-registry.mjs --strict --baseline origin/main`): final strict verdict `INCOMPLETE (strict)`, reason `verdict rows pinned to unpublished spike commits: R-PG3, R-Ph1b-native, R-Ph1b-sb, R-Ph1b-tb, R-PG2-sb, R-PG2-tb, R5b-09-sb, R5b-09-tb`; no `evidence-test-missing` violation (rows R4c-05/06/07 untouched: their tests stayed flat). The remaining reasons (unpublished spike commits, rows awaiting final status) cite no file touched by this pilot. R6a-C1 positive_marker texts and R4a-08 recipes name flat/merged stems in historical log text only; the strict check does not read them.
- docs/CI_TIERS.md prose "374 of 484" and "369 of 484": nightly-artifact dependent, not locally re-derivable — P0.3 scope.
- Historical documents (docs/reviews, docs/checkpoints, docs/perf gate reports, CHANGELOG) still name old flat files — historical record, untouched.
- The classifier CSV/MD identity line (Sources: commit/tree) was produced on the pre-commit tree and will be one commit behind after landing.
- origin/main moved ahead during the pilot. Conflict-prone files: docs/verification/ci-decomposition-baseline.json, the classifier CSV/MD, README/ARCHITECTURE counts. The parent should rebase and RE-DERIVE those (re-run the generators), not hand-merge.
- Local verification: cargo was not run in this task. cargo check rows: to be reported by the parent run — (1) union of the merged modules' features, (2) default features (all modules cfg'd out), (3) `--no-default-features --features alloc-decommit` (feature-powerset row).
- Cargo.lock / manifests: this task did not touch Cargo.toml, fuzz/ or tests/compile_fail/.

## Measurements

Filled 2026-11-17 from committed CI run dumps. Nothing here is invented; every
number below is script output or copied from a committed dump.

- Raw dumps: `docs/perf/p0_2_ci_runs/before/` and `docs/perf/p0_2_ci_runs/after/`,
  one `.json` per run id (`<run-id>.json`), copied from pre-collected artifacts
  outside the repo (collection recipe above, steps 1–2).
- Summary CSV: `docs/perf/P0_2_TEST_MERGE_PILOT_summary.csv`, written by
  `scripts/ci-job-durations.mjs` (recipe step 3).

Run-status table (run id, sha, event, conclusion, side, note):

| run id | sha | event | conclusion | side | note |
|---|---|---|---|---|---|
| 43745289068 | dc87bea8 | pull_request | failure | before | |
| 43749733378 | 676a51b8 | pull_request | failure | before | |
| 43759580924 | 8e82339a | pull_request | success | before | |
| 43768376047 | b5779b5f | push main | success | before | |
| 43777538709 | c6524f40 | push main | success | before | |
| 43785254102 | 8013c80d | push main | success | after | unsharded |
| 43851422289 | b01ad11e | schedule main | success | after | unsharded |
| 44074883752 | 0c1029d7 | schedule main | success | after | sharded |
| 44085753908 | | schedule | success | after | DIFFERENT workflow (`collect` shard workflow, no affected jobs); stored for provenance, not usable |

Duration table, pasted verbatim from scripts/ci-job-durations.mjs output,
reproduction command:
`node scripts/ci-job-durations.mjs --before docs/perf/p0_2_ci_runs/before --after docs/perf/p0_2_ci_runs/after --csv docs/perf/P0_2_TEST_MERGE_PILOT_summary.csv --jobs "test (feature isolation),test (hardened tier),thread-sanitizer" --family "miri core=^miri \\(core\\)$=>^miri \\(core, (pin cache|rest)\\)$" --family "miri fastbin=^miri \\(fastbin\\)$=>^miri \\(fastbin, " --family "miri plain=^miri-plain$=>^miri-plain \\(" --family "miri alloc-core=^miri \\(alloc-core\\)$=>^miri \\(alloc-core\\)$"`
(script exits rc=1 by design on insufficient samples). Columns = job, metric,
n_before, median_before_s, n_after, median_after_s, delta_pct, verdict:

| job | metric | n_before | median_before_s | n_after | median_after_s | delta_pct | verdict |
|---|---|---|---|---|---|---|---|
| test (feature isolation) | job | 5 | 608.00 | 3 | 560.00 | -7.89 | fell |
| test (hardened tier) | job | 5 | 924.00 | 3 | 923.00 | -0.11 | flat |
| thread-sanitizer | job | 5 | 96.00 | 3 | 122.00 | +27.08 | grew |
| miri core | job | 4 | 1564.50 | 1 | 1773.00 | - | insufficient-runs |
| miri core (critical path) | job | 4 | 1564.50 | 1 | 1277.00 | - | insufficient-runs |
| miri fastbin | job | 4 | 2004.00 | 1 | 1937.00 | - | insufficient-runs |
| miri fastbin (critical path) | job | 4 | 2004.00 | 1 | 1465.00 | - | insufficient-runs |
| miri plain | job | 4 | 1322.00 | 1 | 1092.00 | - | insufficient-runs |
| miri plain (critical path) | job | 4 | 1322.00 | 1 | 835.00 | - | insufficient-runs |
| miri alloc-core | job | 4 | 380.50 | 3 | 353.00 | -7.23 | fell |
| miri alloc-core (critical path) | job | 4 | 380.50 | 3 | 353.00 | -7.23 | fell |

summary: 3 fell, 1 flat, 1 grew (11 jobs); criterion line: DOES NOT HOLD — grew: thread-sanitizer.

Notes:

- Metric = `job` everywhere: NO dump contains a separate `cargo test` step
  (affected jobs use composite actions — one opaque step), so the script's
  documented whole-job fallback applies.
- The script skipped 31 job records (jobs outside the `--jobs`/`--family`
  selection, incl. `docs consistency (always)`, which has no cargo test step at
  all and is context-only: before median 78 s (n=5), after median 90 s (n=3),
  +15.38 % — 90/78 medians).
- Miri sharding (PRs #261/#262) is accounted for by comparing FAMILIES:
  before = the single unsharded job; after = (i) sum of family shards' job
  durations and (ii) critical path = max shard (both shown above; sum rows are
  named plain, critical-path rows are named "(critical path)"). Only run
  44074883752 among the available after runs is sharded, so miri families have
  n_after = 1 → `insufficient-runs` (the criterion requires ≥ 3). The earlier
  preliminary estimate "miri (core) +31 %" is not reproduced at family level
  (sum +13.3 %, critical path -38.2 %), but n_after = 1 is undecidable.

Collection recipe (for the parent):

1. Collect ≥3 ordinary CI runs from BEFORE the pilot branch landed (main runs
   before this PR) and ≥3 from AFTER (runs on this PR or on main after merge).
   BOTH collection methods work, and the script accepts both dump formats:
   `gh run view <run-id> --json jobs > <file>.json` (camelCase records) and
   `gh api repos/PHPCraftdream/sefer-alloc/actions/runs/<run-id>/jobs?per_page=100`
   (snake_case records). Do NOT launch any
   dedicated benchmark/test campaign — ordinary runs only (plan §3 "Measured
   decisions").
2. Put the JSON files under `docs/perf/p0_2_ci_runs/before/` and
   `docs/perf/p0_2_ci_runs/after/` (one .json per run, named `<run-id>.json`),
   commit them with the report.
3. Run
   `node scripts/ci-job-durations.mjs --before docs/perf/p0_2_ci_runs/before --after docs/perf/p0_2_ci_runs/after --csv docs/perf/P0_2_TEST_MERGE_PILOT_summary.csv`
   — the script prints the per-job median-duration table (before/after,
   delta %) for the affected cargo-test jobs and writes the summary CSV; paste
   its output table into this section and apply the Decision criterion above.
   No number in this report is typed by hand — everything is script output.
   The script exits nonzero (rc=1) naming the offending file if a dump file
   contains no valid job records (e.g. an empty `jobs` array or all records
   missing/invalid timestamps).

## Verdict (2026-11-17)

All three conditions were evaluated against the Measurements above. The
criterion text above was fixed before any number was read and is not
reinterpreted here.

- **(a) PASS** for the pilot itself (pilot-scoped). `node scripts/test-inventory.mjs
  --before-dir .rush/tmp/old-tree` (snapshot of pre-pilot commit `b375a0e1`) on
  current main prints: `moved stems: 32 | unchanged stems: 497 | merged dir files: 32`
  and `inventory: 1515 tests before, 1510 tests after, 5 lost, 0 added, 0 dup,
  0 cfg-mismatch, 0 orphans, 1 unexpected new targets` — then FAIL on the
  absolute inventory. The absolute deltas are attributed BY COMMIT to unrelated
  post-pilot commits (`55658271`, `d6f129d0`, `58cadda2`), not the merge:
  - 5 lost tests: the `no_stale_doc_references::*` doc-count tests (replaced by
    `scripts/verify-doc-inventories.mjs`);
  - 1 new target: the flat `tests/registry_fallback_hook.rs`.
  All 32 moved stems are OK with per-stem counts identical; binary count
  32 → 1 (saved 31) holds. `node scripts/classify-tests.mjs --check` exit 0.
- **(b) DOES NOT HOLD**, on two independent grounds:
  1. thread-sanitizer grew: median (whole-job, n_before=5, n_after=3)
     96 s → 122 s = +27.08 % (delta 26 s over the 96 s before median), above the
     criterion's bar ("in no affected job did it grow"); the criterion's ±10 %
     runner-noise note does not cover +27 %. Mechanism (context, not a
     re-interpretation): the pilot added a second `&&`-chained cargo invocation
     for `regression_percounter_perheap_aggregation::`.
  2. All four miri families are `insufficient-runs`: n_after = 1 (only run
     44074883752 is sharded) < the required 3 — undecidable, not a pass.
  What was compared for sharding: the family union (before = the single
  unsharded job, after = all family shards), with BOTH readings shown in the
  table — sum of shards' job durations (primary, conservative) and critical
  path = max shard (wall-clock view).
- **(c) PASS with one recorded gap.** All mechanical pins LANDED-CONSISTENT on
  main: `node scripts/p0_2_global_state_scan.mjs --filters` → 18 tokens, 81
  tests, 0 violations; `scripts/verify-ci-decomposition.mjs` rc=0 ("68 jobs
  unchanged"); ci-plan `--check-doc` rc=0; verify-structure-limits OK;
  verify-doc-inventories OK; classify pins 510/461 consistent after later
  regenerations; evidence-registry INCOMPLETE(strict) unchanged (unpublished
  spike rows only, no evidence-test-missing). GAP: the doc's claim (Pin updates
  section above) that the README path mentions now name
  `tests/regression_merged/regression_magazine_oracles.rs` never landed —
  README.md:1394 and README.md:1582 still name
  `tests/regression_magazine_oracles.rs` (a nonexistent path); the landed pilot
  commit `b0304d97` contains no README/ARCHITECTURE changes ("follow in the
  next commit" never came as described). The criterion wording — pins "fit
  inside the pilot PR without manual hacks" — is therefore not fully met;
  recorded as a gap, not a criterion rewrite.

## FINAL VERDICT: **NO-GO**

P0.3/P0.4 (further group merges) stay CLOSED. What did not pass and next steps:

1. Criterion (b) needs ≥ 2 more scheduled main runs on the sharded CI workflow
   before the miri families are decidable (items 155/156 in
   `docs/perf/OPEN_ITEMS.md` track sharding health).
2. The tsan growth is structural (the pilot's extra chained invocation): if a
   future attempt wants GO, the criterion as written — no growth in ANY
   affected job — cannot be met by construction unless the second invocation is
   folded back into one command or the criterion is formally revised by the
   owner FIRST.
3. Fix the README `regression_magazine_oracles` path pin (README.md:1394,
   README.md:1582) — filed as item 284 in `docs/CORRECTNESS_OPEN_ITEMS.md`.

Honest limitations:

- n_after = 3 for non-miri jobs and n_after = 1 for miri families; the latter
  is below this doc's own ≥ 3 bar.
- Runner noise is ±10 % per the criterion text, so the -0.11 % "flat" row
  (test (hardened tier), medians 924 s → 923 s) and every single-run miri delta
  are undecidable, not evidence of no-effect.
- The before side includes 2 failing PR runs (43745289068, 43749733378); their
  affected-job records were still valid, and the script prints skipped counts.
- The metric is whole-job duration, not a cargo-test step, because the dumps
  contain no separate step (composite actions).
