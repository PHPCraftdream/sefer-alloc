# Integration-test classification report (plan 2026-11-15)

Script: `scripts/classify-tests.mjs` (modules under `scripts/classify-tests/`).

Sources: commit `7ba0a5d93d5864332d73c7fa0d40455d3558c48a`, tree `a7951c80755fb5bdb8d89c8af7761445723072ce`; tracked inputs clean.

## First-config-wins markers (derived from src/)

| marker | first src/ hit (file:line) | provenance |
| --- | --- | --- |
| with_config | src/global/alloc_stats.rs:168 | src/global/sefer_alloc/core.rs:277 |
| live_config_matches | src/alloc_core/alloc_core/lifecycle.rs:5 | src/alloc_core/alloc_core/lifecycle.rs:254 + src/registry/heap_core/core.rs:388 |
| claim_lease_with_config | src/alloc_core/alloc_core/bootstrap.rs:185 | src/registry/heap_core/core.rs (derive output) |
| current_for_alloc_with_config | src/global/sefer_alloc/core.rs:4 | src/global/tls_heap.rs:411 |
| bind_slow_tagged_with_config | src/global/sefer_alloc/core.rs:339 | src/global/tls_heap.rs:491 |
| new_with_config | src/alloc_core/alloc_core/alloc_core_impl.rs:159 | src/registry/heap_core/core.rs:316 (+ claim.rs) |
| heap_ptr_with_config | src/global/fallback.rs:184 | src/global/fallback.rs:188 |
| config_conflicts_total | src/global/fallback.rs:429 | src/registry/heap_registry/counters.rs:99 |
| config_conflicts | src/global/alloc_stats.rs:183 | src/global/alloc_stats.rs:183 (AllocStats field) |
| fallback_config_conflicts | src/global/sefer_alloc/core.rs:244 | src/global/sefer_alloc/core.rs:372 |
| thp_tuple_conflicts | src/global/alloc_stats.rs:193 | src/global/sefer_alloc/diag.rs (alloc_stats.rs:193 field family) |

Dropped from the marker list: `publish_pending` (src/alloc_core/segment/segment_route/route_protocol/large_state.rs:25) — the Large-reservation route sidecar publication CAS (LIVE(g)->PENDING(g)), not a first-config-wins API. `claim_with_config` auto-dropped (no src/ hit).

## Pin surfaces scanned

| kind | files |
| --- | --- |
| ci_action | 56 |
| ci_workflow | 8 |
| doc | 1381 |
| doc_json | 2 |
| script | 285 |

Note: the classification suite's own output artifacts (`docs/perf/TEST_TARGET_CLASSIFICATION_*.md`, `docs/test_target_classification_*.csv`) are excluded from the doc pin surface: scanning the report's own echo would pin the targets it lists, making the classification non-idempotent (self-reference, not a pre-existing pin).

Targets externally pinned by name: **475** of 510.

## Plan hypotheses (plan §1) vs naive grep vs measured

| metric | plan (2026-11-15) | naive grep | measured | verdict |
| --- | --- | --- | --- | --- |
| tests/ .rs files | 562 | 510 | 510 | corrected |
| file-level cfg | 514 | 461 | 461 | corrected |
| global_allocator | 79 | 76 | 63 | corrected |
| first_config_wins | 111 | 24 | 79 | corrected |
| process_spawn | 33 | 32 | 33 | confirmed |
| loom_only (plan definition: loom mentions) | 21 | 45 | 32 | corrected |
| harness=false (all harness=false lines in Cargo.toml) | 52 | 52 | 13 | corrected |
| [[test]] tables | 29 | 26 | 26 | corrected |

Note: the plan's §1 numbers are grep approximations over overlapping sets — the `naive grep` column reproduces them over raw text (see the reconciliation table for exactly which files differ and why). Example: global_allocator plan 79 = naive grep 79; measured 64 because 15 files match only in comments/strings.
## Definitions

| term | meaning |
| --- | --- |
| target | entry under tests/ that Cargo builds as a test target (explicit [[test]] or autodiscovered) |
| non-target | tests/ entry Cargo does not build (fixture dirs, support modules, .mjs scripts, regressions file) |
| test_enabled=false | target with `test = false` in its [[test]] table |
| harness_false | target with `harness = false` in its [[test]] table (custom main) |
| declared_explicit | target declared via an explicit [[test]] table |
| required_features | required-features list of the [[test]] table (space-joined) |
| file_level_cfg | file-level #![cfg(...)] or #![cfg_attr(...)] predicate before the first item |
| cfg_features / cfg_keys | feature names / cfg keys referenced by the file-level cfg |
| global_allocator | file installs a #[global_allocator] |
| first_config_wins | first-config-wins API markers present (comma-joined; see marker table) |
| process_spawn | uses std::process::Command / current_exe / CARGO_BIN_EXE / spawn |
| abort | calls std::process::abort / libc::abort |
| should_panic / catch_unwind | informational: #[should_panic] / catch_unwind usage (never a merge blocker) |
| loom_only | strict: loom-gated by file-level cfg OR uses loom:: APIs |
| loom_mentions | informational: any bare `loom` word in stripped source |
| miri_only | file-level cfg mentions miri |
| kani_gated | file-level cfg mentions kani |
| main_fn | source defines `fn main(` |
| externally_pinned | target NAME appears in .github workflows/actions, scripts/*.mjs, docs markdown or the docs/verification/*.json catalog |
| mergeable | passes ALL strict merge rules (see unmergeable_reasons for violations) |
| unmergeable_reasons | full list of violated strict rules, semicolon-joined slugs |
| group | (theme\|features\|file-cfg) key slug; only mergeable targets are grouped |

## Flag-set intersections (pairwise, over all targets)

| set (size) \ in | global_allocator (63) | first_config_wins (79) | process_spawn (33) | loom_only (32) | miri_only (50) | kani_gated (0) | harness_false (13) | test_disabled (11) | required_features (26) | file_cfg (461) | externally_pinned (475) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **global_allocator** (63) | 63 | 14 | 4 | 1 | 11 | 0 | 11 | 11 | 12 | 52 | 61 |
| **first_config_wins** (79) | 14 | 79 | 3 | 1 | 26 | 0 | 0 | 0 | 2 | 79 | 75 |
| **process_spawn** (33) | 4 | 3 | 33 | 0 | 3 | 0 | 1 | 0 | 2 | 25 | 33 |
| **loom_only** (32) | 1 | 1 | 0 | 32 | 1 | 0 | 0 | 0 | 0 | 30 | 31 |
| **miri_only** (50) | 11 | 26 | 3 | 1 | 50 | 0 | 0 | 0 | 4 | 50 | 50 |
| **kani_gated** (0) | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| **harness_false** (13) | 11 | 0 | 1 | 0 | 0 | 0 | 13 | 11 | 13 | 0 | 13 |
| **test_disabled** (11) | 11 | 0 | 0 | 0 | 0 | 0 | 11 | 11 | 11 | 0 | 11 |
| **required_features** (26) | 12 | 2 | 2 | 0 | 4 | 0 | 13 | 11 | 26 | 12 | 26 |
| **file_cfg** (461) | 52 | 79 | 25 | 30 | 50 | 0 | 0 | 0 | 12 | 461 | 428 |
| **externally_pinned** (475) | 61 | 75 | 33 | 31 | 50 | 0 | 13 | 11 | 26 | 428 | 475 |

## Non-target inventory (1204 entries; 38 of them are top-level entries of tests/)

Top-level accounting: tests/ has **548** top-level entries; 510 are targets; the remaining **38** are non-targets. Fixture directories additionally contribute deeper files to the inventory below (kind `directory-fixture` / `support-module` / `other-file`).

| path | kind |
| --- | --- |
| tests/_shared | directory-fixture |
| tests/_shared/j13b_advice_assertions.rs | support-module |
| tests/alloc_core_differential.proptest-regressions | proptest-regressions |
| tests/ci_decomposition_contracts.mjs | mjs-script |
| tests/ci_effective_workflow_fixtures.mjs | mjs-script |
| tests/ci_job_durations.test.mjs | mjs-script |
| tests/ci_selection_history.test.mjs | mjs-script |
| tests/ci_ubuntu_apt_sources.mjs | mjs-script |
| tests/classify_tests.test.mjs | mjs-script |
| tests/compile_fail | directory-fixture |
| tests/compile_fail/r14_positive_probe | directory-fixture |
| tests/compile_fail/r14_positive_probe/src | directory-fixture |
| tests/compile_fail/r14_positive_probe/src/main.rs | support-module |
| tests/compile_fail/r2_01_segment_bases_outlives_owner | directory-fixture |
| tests/compile_fail/r2_01_segment_bases_outlives_owner/Cargo.lock | other-file |
| tests/compile_fail/r2_01_segment_bases_outlives_owner/Cargo.toml | other-file |
| tests/compile_fail/r2_01_segment_bases_outlives_owner/src | directory-fixture |
| tests/compile_fail/r2_01_segment_bases_outlives_owner/src/main.rs | support-module |
| tests/compile_fail/r2_02_epoch_region_non_send_value | directory-fixture |
| tests/compile_fail/r2_02_epoch_region_non_send_value/Cargo.lock | other-file |
| tests/compile_fail/r2_02_epoch_region_non_send_value/Cargo.toml | other-file |
| tests/compile_fail/r2_02_epoch_region_non_send_value/src | directory-fixture |
| tests/compile_fail/r2_02_epoch_region_non_send_value/src/main.rs | support-module |
| tests/compile_fail/r6_t38_public_tail_private | directory-fixture |
| tests/compile_fail/r6_t38_public_tail_private/Cargo.lock | other-file |
| tests/compile_fail/r6_t38_public_tail_private/Cargo.toml | other-file |
| tests/compile_fail/r6_t38_public_tail_private/src | directory-fixture |
| tests/compile_fail/r6_t38_public_tail_private/src/main.rs | support-module |
| tests/compile_fail/route_publication_requires_unsafe | directory-fixture |
| tests/compile_fail/route_publication_requires_unsafe/Cargo.lock | other-file |
| tests/compile_fail/route_publication_requires_unsafe/Cargo.toml | other-file |
| tests/compile_fail/route_publication_requires_unsafe/src | directory-fixture |
| tests/compile_fail/route_publication_requires_unsafe/src/main.rs | support-module |
| tests/compile_fail/route_registration_owner_not_sync | directory-fixture |
| tests/compile_fail/route_registration_owner_not_sync/src | directory-fixture |
| tests/compile_fail/route_registration_owner_not_sync/src/main.rs | support-module |
| tests/compile_fail/shard_guard_send_only_payload_not_sync | directory-fixture |
| tests/compile_fail/shard_guard_send_only_payload_not_sync/src | directory-fixture |
| tests/compile_fail/shard_guard_send_only_payload_not_sync/src/main.rs | support-module |
| tests/compile_fail/small_sidecar_shared_issue_not_callable | directory-fixture |
| tests/compile_fail/small_sidecar_shared_issue_not_callable/src | directory-fixture |
| tests/compile_fail/small_sidecar_shared_issue_not_callable/src/main.rs | support-module |
| tests/compile_fail/small_sidecar_shared_prepare_not_callable | directory-fixture |
| tests/compile_fail/small_sidecar_shared_prepare_not_callable/src | directory-fixture |
| tests/compile_fail/small_sidecar_shared_prepare_not_callable/src/main.rs | support-module |
| tests/evidence_reverify_runner_selftest.mjs | mjs-script |
| tests/evidence_reverify_support_selftest.mjs | mjs-script |
| tests/g1_big_study_parser.mjs | mjs-script |
| tests/g1_big_study_receipt.mjs | mjs-script |
| tests/g1_big_study_runner.mjs | mjs-script |
| tests/g1_widening_count.test.mjs | mjs-script |
| tests/ghost_extraction_policy_fixtures.mjs | mjs-script |
| tests/ghost_extraction_policy.test.mjs | mjs-script |
| tests/ghost_layer_restructuring_policy_fixtures.mjs | mjs-script |
| tests/ghost_lint_fixtures | directory-fixture |
| tests/ghost_lint_fixtures/nl3_owner_model_bind | directory-fixture |
| tests/ghost_lint_fixtures/nl3_owner_model_bind/mutant_claim_ordering.rs | support-module |
| tests/ghost_lint_fixtures/nl3_owner_model_bind/mutant_drop_ordering.rs | support-module |
| tests/ghost_lint_fixtures/nl3_owner_model_bind/mutant_release_before_trim.rs | support-module |
| tests/ghost_lint_fixtures/nl3_owner_model_bind/ok.rs | support-module |
| tests/ghost_lint_fixtures/nl3_owner_model_bind/rule.json | other-file |
| tests/ghost_lint_fixtures/p1_claim_empty_init | directory-fixture |
| tests/ghost_lint_fixtures/p1_claim_empty_init/mutant_relaxed_success.rs | support-module |
| tests/ghost_lint_fixtures/p1_claim_empty_init/ok.rs | support-module |
| tests/ghost_lint_fixtures/p1_claim_empty_init/rule.json | other-file |
| tests/ghost_lint_fixtures/p1_claim_free_live | directory-fixture |
| tests/ghost_lint_fixtures/p1_claim_free_live/mutant_relaxed_both.rs | support-module |
| tests/ghost_lint_fixtures/p1_claim_free_live/mutant_release_success.rs | support-module |
| tests/ghost_lint_fixtures/p1_claim_free_live/ok.rs | support-module |
| tests/ghost_lint_fixtures/p1_claim_free_live/rule.json | other-file |
| tests/ghost_lint_fixtures/p1_claim_publish_live | directory-fixture |
| tests/ghost_lint_fixtures/p1_claim_publish_live/mutant_relaxed_success.rs | support-module |
| tests/ghost_lint_fixtures/p1_claim_publish_live/ok.rs | support-module |
| tests/ghost_lint_fixtures/p1_claim_publish_live/rule.json | other-file |
| tests/ghost_lint_fixtures/p1_claim_recover_init | directory-fixture |
| tests/ghost_lint_fixtures/p1_claim_recover_init/mutant_relaxed_success.rs | support-module |
| tests/ghost_lint_fixtures/p1_claim_recover_init/ok.rs | support-module |
| tests/ghost_lint_fixtures/p1_claim_recover_init/rule.json | other-file |
| tests/ghost_lint_fixtures/p1_drop_lost_cas_fatal | directory-fixture |
| tests/ghost_lint_fixtures/p1_drop_lost_cas_fatal/mutant_fatal_on_ok.rs | support-module |
| tests/ghost_lint_fixtures/p1_drop_lost_cas_fatal/mutant_fatal_unguarded.rs | support-module |
| tests/ghost_lint_fixtures/p1_drop_lost_cas_fatal/ok.rs | support-module |
| tests/ghost_lint_fixtures/p1_drop_lost_cas_fatal/rule.json | other-file |
| tests/ghost_lint_fixtures/p1_drop_release | directory-fixture |
| tests/ghost_lint_fixtures/p1_drop_release/mutant_acqrel.rs | support-module |
| tests/ghost_lint_fixtures/p1_drop_release/mutant_no_cas.rs | support-module |
| tests/ghost_lint_fixtures/p1_drop_release/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p1_drop_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p1_drop_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p1_model_bind | directory-fixture |
| tests/ghost_lint_fixtures/p1_model_bind/mutant_claim_release.rs | support-module |
| tests/ghost_lint_fixtures/p1_model_bind/mutant_drop_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p1_model_bind/mutant_unbound.rs | support-module |
| tests/ghost_lint_fixtures/p1_model_bind/ok.rs | support-module |
| tests/ghost_lint_fixtures/p1_model_bind/rule.json | other-file |
| tests/ghost_lint_fixtures/p1_oom_rollback_release | directory-fixture |
| tests/ghost_lint_fixtures/p1_oom_rollback_release/mutant_relaxed_success.rs | support-module |
| tests/ghost_lint_fixtures/p1_oom_rollback_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p1_oom_rollback_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p10_credit_issue | directory-fixture |
| tests/ghost_lint_fixtures/p10_credit_issue_one | directory-fixture |
| tests/ghost_lint_fixtures/p10_credit_issue_one/mutant_reclaim_issues.rs | support-module |
| tests/ghost_lint_fixtures/p10_credit_issue_one/ok.rs | support-module |
| tests/ghost_lint_fixtures/p10_credit_issue_one/rule.json | other-file |
| tests/ghost_lint_fixtures/p10_credit_issue/mutant_producer_issue.rs | support-module |
| tests/ghost_lint_fixtures/p10_credit_issue/ok.rs | support-module |
| tests/ghost_lint_fixtures/p10_credit_issue/rule.json | other-file |
| tests/ghost_lint_fixtures/p10_credit_plain | directory-fixture |
| tests/ghost_lint_fixtures/p10_credit_plain/mutant_atomic.rs | support-module |
| tests/ghost_lint_fixtures/p10_credit_plain/ok.rs | support-module |
| tests/ghost_lint_fixtures/p10_credit_plain/rule.json | other-file |
| tests/ghost_lint_fixtures/p10_credit_retire | directory-fixture |
| tests/ghost_lint_fixtures/p10_credit_retire_batch | directory-fixture |
| tests/ghost_lint_fixtures/p10_credit_retire_batch/mutant_producer_retire.rs | support-module |
| tests/ghost_lint_fixtures/p10_credit_retire_batch/ok.rs | support-module |
| tests/ghost_lint_fixtures/p10_credit_retire_batch/rule.json | other-file |
| tests/ghost_lint_fixtures/p10_credit_retire/mutant_producer_retire.rs | support-module |
| tests/ghost_lint_fixtures/p10_credit_retire/ok.rs | support-module |
| tests/ghost_lint_fixtures/p10_credit_retire/rule.json | other-file |
| tests/ghost_lint_fixtures/p10_credit_store | directory-fixture |
| tests/ghost_lint_fixtures/p10_credit_store/mutant_direct_store.rs | support-module |
| tests/ghost_lint_fixtures/p10_credit_store/ok.rs | support-module |
| tests/ghost_lint_fixtures/p10_credit_store/rule.json | other-file |
| tests/ghost_lint_fixtures/p10_model_bind | directory-fixture |
| tests/ghost_lint_fixtures/p10_model_bind/mutant_cut_unbound.rs | support-module |
| tests/ghost_lint_fixtures/p10_model_bind/mutant_publish_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p10_model_bind/ok.rs | support-module |
| tests/ghost_lint_fixtures/p10_model_bind/rule.json | other-file |
| tests/ghost_lint_fixtures/p10_release_after_empty | directory-fixture |
| tests/ghost_lint_fixtures/p10_release_after_empty/mutant_unchecked_release.rs | support-module |
| tests/ghost_lint_fixtures/p10_release_after_empty/ok.rs | support-module |
| tests/ghost_lint_fixtures/p10_release_after_empty/rule.json | other-file |
| tests/ghost_lint_fixtures/p11_clear_sole_path | directory-fixture |
| tests/ghost_lint_fixtures/p11_clear_sole_path/mutant_scan_clears.rs | support-module |
| tests/ghost_lint_fixtures/p11_clear_sole_path/ok.rs | support-module |
| tests/ghost_lint_fixtures/p11_clear_sole_path/rule.json | other-file |
| tests/ghost_lint_fixtures/p11_consume_before_finish_bounded | directory-fixture |
| tests/ghost_lint_fixtures/p11_consume_before_finish_bounded/mutant_finish_first.rs | support-module |
| tests/ghost_lint_fixtures/p11_consume_before_finish_bounded/ok.rs | support-module |
| tests/ghost_lint_fixtures/p11_consume_before_finish_bounded/rule.json | other-file |
| tests/ghost_lint_fixtures/p11_consume_before_finish_drain | directory-fixture |
| tests/ghost_lint_fixtures/p11_consume_before_finish_drain/mutant_no_consume.rs | support-module |
| tests/ghost_lint_fixtures/p11_consume_before_finish_drain/ok.rs | support-module |
| tests/ghost_lint_fixtures/p11_consume_before_finish_drain/rule.json | other-file |
| tests/ghost_lint_fixtures/p11_consume_before_finish_scan | directory-fixture |
| tests/ghost_lint_fixtures/p11_consume_before_finish_scan/mutant_finish_first.rs | support-module |
| tests/ghost_lint_fixtures/p11_consume_before_finish_scan/ok.rs | support-module |
| tests/ghost_lint_fixtures/p11_consume_before_finish_scan/rule.json | other-file |
| tests/ghost_lint_fixtures/p11_model_bind | directory-fixture |
| tests/ghost_lint_fixtures/p11_model_bind/mutant_cut_unbound.rs | support-module |
| tests/ghost_lint_fixtures/p11_model_bind/mutant_publish_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p11_model_bind/ok.rs | support-module |
| tests/ghost_lint_fixtures/p11_model_bind/rule.json | other-file |
| tests/ghost_lint_fixtures/p11_plain_large | directory-fixture |
| tests/ghost_lint_fixtures/p11_plain_large/mutant_atomic.rs | support-module |
| tests/ghost_lint_fixtures/p11_plain_large/ok.rs | support-module |
| tests/ghost_lint_fixtures/p11_plain_large/rule.json | other-file |
| tests/ghost_lint_fixtures/p11_plain_small | directory-fixture |
| tests/ghost_lint_fixtures/p11_plain_small/mutant_atomic.rs | support-module |
| tests/ghost_lint_fixtures/p11_plain_small/ok.rs | support-module |
| tests/ghost_lint_fixtures/p11_plain_small/rule.json | other-file |
| tests/ghost_lint_fixtures/p11_set_sole_path | directory-fixture |
| tests/ghost_lint_fixtures/p11_set_sole_path/mutant_scan_sets.rs | support-module |
| tests/ghost_lint_fixtures/p11_set_sole_path/ok.rs | support-module |
| tests/ghost_lint_fixtures/p11_set_sole_path/rule.json | other-file |
| tests/ghost_lint_fixtures/p12_bind_once | directory-fixture |
| tests/ghost_lint_fixtures/p12_bind_once/mutant_rebind_in_reload.rs | support-module |
| tests/ghost_lint_fixtures/p12_bind_once/ok.rs | support-module |
| tests/ghost_lint_fixtures/p12_bind_once/rule.json | other-file |
| tests/ghost_lint_fixtures/p12_fast_park_from_full_magazine | directory-fixture |
| tests/ghost_lint_fixtures/p12_fast_park_from_full_magazine/mutant_drain_parks.rs | support-module |
| tests/ghost_lint_fixtures/p12_fast_park_from_full_magazine/ok.rs | support-module |
| tests/ghost_lint_fixtures/p12_fast_park_from_full_magazine/rule.json | other-file |
| tests/ghost_lint_fixtures/p12_fast_reload_from_miss | directory-fixture |
| tests/ghost_lint_fixtures/p12_fast_reload_from_miss/mutant_drain_fast_reloads.rs | support-module |
| tests/ghost_lint_fixtures/p12_fast_reload_from_miss/ok.rs | support-module |
| tests/ghost_lint_fixtures/p12_fast_reload_from_miss/rule.json | other-file |
| tests/ghost_lint_fixtures/p12_flush_reloads_parked | directory-fixture |
| tests/ghost_lint_fixtures/p12_flush_reloads_parked/mutant_no_reload.rs | support-module |
| tests/ghost_lint_fixtures/p12_flush_reloads_parked/mutant_reload_before_flush.rs | support-module |
| tests/ghost_lint_fixtures/p12_flush_reloads_parked/ok.rs | support-module |
| tests/ghost_lint_fixtures/p12_flush_reloads_parked/rule.json | other-file |
| tests/ghost_lint_fixtures/p12_model_bind | directory-fixture |
| tests/ghost_lint_fixtures/p12_model_bind/mutant_drop_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p12_model_bind/mutant_no_trim.rs | support-module |
| tests/ghost_lint_fixtures/p12_model_bind/mutant_release_first.rs | support-module |
| tests/ghost_lint_fixtures/p12_model_bind/ok.rs | support-module |
| tests/ghost_lint_fixtures/p12_model_bind/rule.json | other-file |
| tests/ghost_lint_fixtures/p12_no_decay_in_teardown | directory-fixture |
| tests/ghost_lint_fixtures/p12_no_decay_in_teardown/mutant_decay_in_drain.rs | support-module |
| tests/ghost_lint_fixtures/p12_no_decay_in_teardown/ok.rs | support-module |
| tests/ghost_lint_fixtures/p12_no_decay_in_teardown/rule.json | other-file |
| tests/ghost_lint_fixtures/p12_overflow_release_after_drain | directory-fixture |
| tests/ghost_lint_fixtures/p12_overflow_release_after_drain/mutant_no_release.rs | support-module |
| tests/ghost_lint_fixtures/p12_overflow_release_after_drain/mutant_release_before_drain.rs | support-module |
| tests/ghost_lint_fixtures/p12_overflow_release_after_drain/ok.rs | support-module |
| tests/ghost_lint_fixtures/p12_overflow_release_after_drain/rule.json | other-file |
| tests/ghost_lint_fixtures/p12_overflow_reset_after_release | directory-fixture |
| tests/ghost_lint_fixtures/p12_overflow_reset_after_release/mutant_no_reset.rs | support-module |
| tests/ghost_lint_fixtures/p12_overflow_reset_after_release/mutant_reset_before_decommit.rs | support-module |
| tests/ghost_lint_fixtures/p12_overflow_reset_after_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p12_overflow_reset_after_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p12_overflow_tick_reload | directory-fixture |
| tests/ghost_lint_fixtures/p12_overflow_tick_reload/mutant_teardown_reload_ticks.rs | support-module |
| tests/ghost_lint_fixtures/p12_overflow_tick_reload/ok.rs | support-module |
| tests/ghost_lint_fixtures/p12_overflow_tick_reload/rule.json | other-file |
| tests/ghost_lint_fixtures/p12_owner_only_access | directory-fixture |
| tests/ghost_lint_fixtures/p12_owner_only_access/mutant_diag_reads_depot.rs | support-module |
| tests/ghost_lint_fixtures/p12_owner_only_access/ok.rs | support-module |
| tests/ghost_lint_fixtures/p12_owner_only_access/rule.json | other-file |
| tests/ghost_lint_fixtures/p12_segment_tick_on_refill | directory-fixture |
| tests/ghost_lint_fixtures/p12_segment_tick_on_refill/mutant_drain_ticks.rs | support-module |
| tests/ghost_lint_fixtures/p12_segment_tick_on_refill/ok.rs | support-module |
| tests/ghost_lint_fixtures/p12_segment_tick_on_refill/rule.json | other-file |
| tests/ghost_lint_fixtures/p12_ticking_reload_on_refill | directory-fixture |
| tests/ghost_lint_fixtures/p12_ticking_reload_on_refill/mutant_drain_ticks.rs | support-module |
| tests/ghost_lint_fixtures/p12_ticking_reload_on_refill/ok.rs | support-module |
| tests/ghost_lint_fixtures/p12_ticking_reload_on_refill/rule.json | other-file |
| tests/ghost_lint_fixtures/p14_evict_cas | directory-fixture |
| tests/ghost_lint_fixtures/p14_evict_cas/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p14_evict_cas/ok.rs | support-module |
| tests/ghost_lint_fixtures/p14_evict_cas/rule.json | other-file |
| tests/ghost_lint_fixtures/p14_evict_swap | directory-fixture |
| tests/ghost_lint_fixtures/p14_evict_swap/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p14_evict_swap/ok.rs | support-module |
| tests/ghost_lint_fixtures/p14_evict_swap/rule.json | other-file |
| tests/ghost_lint_fixtures/p14_install_release | directory-fixture |
| tests/ghost_lint_fixtures/p14_install_release/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p14_install_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p14_install_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p14_model_bind | directory-fixture |
| tests/ghost_lint_fixtures/p14_model_bind/mutant_cas_drifted.rs | support-module |
| tests/ghost_lint_fixtures/p14_model_bind/mutant_swap_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p14_model_bind/ok.rs | support-module |
| tests/ghost_lint_fixtures/p14_model_bind/rule.json | other-file |
| tests/ghost_lint_fixtures/p14_read_generation | directory-fixture |
| tests/ghost_lint_fixtures/p14_read_generation/mutant_no_recheck.rs | support-module |
| tests/ghost_lint_fixtures/p14_read_generation/mutant_relaxed_recheck.rs | support-module |
| tests/ghost_lint_fixtures/p14_read_generation/ok.rs | support-module |
| tests/ghost_lint_fixtures/p14_read_generation/rule.json | other-file |
| tests/ghost_lint_fixtures/p14_read_value | directory-fixture |
| tests/ghost_lint_fixtures/p14_read_value/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p14_read_value/ok.rs | support-module |
| tests/ghost_lint_fixtures/p14_read_value/rule.json | other-file |
| tests/ghost_lint_fixtures/p15_bind_cas | directory-fixture |
| tests/ghost_lint_fixtures/p15_bind_cas/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p15_bind_cas/ok.rs | support-module |
| tests/ghost_lint_fixtures/p15_bind_cas/rule.json | other-file |
| tests/ghost_lint_fixtures/p15_claim_cas | directory-fixture |
| tests/ghost_lint_fixtures/p15_claim_cas/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p15_claim_cas/ok.rs | support-module |
| tests/ghost_lint_fixtures/p15_claim_cas/rule.json | other-file |
| tests/ghost_lint_fixtures/p15_death_release | directory-fixture |
| tests/ghost_lint_fixtures/p15_death_release/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p15_death_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p15_death_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p15_model_bind | directory-fixture |
| tests/ghost_lint_fixtures/p15_model_bind/mutant_cas_drifted.rs | support-module |
| tests/ghost_lint_fixtures/p15_model_bind/mutant_swap_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p15_model_bind/ok.rs | support-module |
| tests/ghost_lint_fixtures/p15_model_bind/rule.json | other-file |
| tests/ghost_lint_fixtures/p15_prune_acquire | directory-fixture |
| tests/ghost_lint_fixtures/p15_prune_acquire/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p15_prune_acquire/ok.rs | support-module |
| tests/ghost_lint_fixtures/p15_prune_acquire/rule.json | other-file |
| tests/ghost_lint_fixtures/p15_release_store | directory-fixture |
| tests/ghost_lint_fixtures/p15_release_store/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p15_release_store/ok.rs | support-module |
| tests/ghost_lint_fixtures/p15_release_store/rule.json | other-file |
| tests/ghost_lint_fixtures/p17_get_twin | directory-fixture |
| tests/ghost_lint_fixtures/p17_get_twin/mutant_relaxed_in_twin.rs | support-module |
| tests/ghost_lint_fixtures/p17_get_twin/ok.rs | support-module |
| tests/ghost_lint_fixtures/p17_get_twin/rule.json | other-file |
| tests/ghost_lint_fixtures/p17_init_twin | directory-fixture |
| tests/ghost_lint_fixtures/p17_init_twin/mutant_missing_rollback_in_twin.rs | support-module |
| tests/ghost_lint_fixtures/p17_init_twin/mutant_relaxed_publish_in_twin.rs | support-module |
| tests/ghost_lint_fixtures/p17_init_twin/ok.rs | support-module |
| tests/ghost_lint_fixtures/p17_init_twin/rule.json | other-file |
| tests/ghost_lint_fixtures/p17_rollback_twin | directory-fixture |
| tests/ghost_lint_fixtures/p17_rollback_twin/mutant_relaxed_in_twin.rs | support-module |
| tests/ghost_lint_fixtures/p17_rollback_twin/ok.rs | support-module |
| tests/ghost_lint_fixtures/p17_rollback_twin/rule.json | other-file |
| tests/ghost_lint_fixtures/p18_head_new_twin | directory-fixture |
| tests/ghost_lint_fixtures/p18_head_new_twin/mutant_loom_only_store.rs | support-module |
| tests/ghost_lint_fixtures/p18_head_new_twin/ok.rs | support-module |
| tests/ghost_lint_fixtures/p18_head_new_twin/rule.json | other-file |
| tests/ghost_lint_fixtures/p18_new_twin | directory-fixture |
| tests/ghost_lint_fixtures/p18_new_twin/mutant_loom_only_store.rs | support-module |
| tests/ghost_lint_fixtures/p18_new_twin/ok.rs | support-module |
| tests/ghost_lint_fixtures/p18_new_twin/rule.json | other-file |
| tests/ghost_lint_fixtures/p19_ghost_acquired | directory-fixture |
| tests/ghost_lint_fixtures/p19_ghost_acquired/mutant_dropped.rs | support-module |
| tests/ghost_lint_fixtures/p19_ghost_acquired/ok.rs | support-module |
| tests/ghost_lint_fixtures/p19_ghost_acquired/rule.json | other-file |
| tests/ghost_lint_fixtures/p19_ghost_init | directory-fixture |
| tests/ghost_lint_fixtures/p19_ghost_init/mutant_dropped.rs | support-module |
| tests/ghost_lint_fixtures/p19_ghost_init/ok.rs | support-module |
| tests/ghost_lint_fixtures/p19_ghost_init/rule.json | other-file |
| tests/ghost_lint_fixtures/p19_ghost_release | directory-fixture |
| tests/ghost_lint_fixtures/p19_ghost_release/mutant_dropped.rs | support-module |
| tests/ghost_lint_fixtures/p19_ghost_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p19_ghost_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p19_init_cas | directory-fixture |
| tests/ghost_lint_fixtures/p19_init_cas/mutant_acqrel.rs | support-module |
| tests/ghost_lint_fixtures/p19_init_cas/ok.rs | support-module |
| tests/ghost_lint_fixtures/p19_init_cas/rule.json | other-file |
| tests/ghost_lint_fixtures/p19_lock_acquire | directory-fixture |
| tests/ghost_lint_fixtures/p19_lock_acquire/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p19_lock_acquire/ok.rs | support-module |
| tests/ghost_lint_fixtures/p19_lock_acquire/rule.json | other-file |
| tests/ghost_lint_fixtures/p19_lock_try | directory-fixture |
| tests/ghost_lint_fixtures/p19_lock_try/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p19_lock_try/ok.rs | support-module |
| tests/ghost_lint_fixtures/p19_lock_try/rule.json | other-file |
| tests/ghost_lint_fixtures/p19_publish_release | directory-fixture |
| tests/ghost_lint_fixtures/p19_publish_release/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p19_publish_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p19_publish_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p19_ready_acquire | directory-fixture |
| tests/ghost_lint_fixtures/p19_ready_acquire/mutant_relaxed_fast.rs | support-module |
| tests/ghost_lint_fixtures/p19_ready_acquire/ok.rs | support-module |
| tests/ghost_lint_fixtures/p19_ready_acquire/rule.json | other-file |
| tests/ghost_lint_fixtures/p19_rollback_release | directory-fixture |
| tests/ghost_lint_fixtures/p19_rollback_release/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p19_rollback_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p19_rollback_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p19_try_ready_acquire | directory-fixture |
| tests/ghost_lint_fixtures/p19_try_ready_acquire/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p19_try_ready_acquire/ok.rs | support-module |
| tests/ghost_lint_fixtures/p19_try_ready_acquire/rule.json | other-file |
| tests/ghost_lint_fixtures/p19_try_requires_ready | directory-fixture |
| tests/ghost_lint_fixtures/p19_try_requires_ready/mutant_unchecked.rs | support-module |
| tests/ghost_lint_fixtures/p19_try_requires_ready/ok.rs | support-module |
| tests/ghost_lint_fixtures/p19_try_requires_ready/rule.json | other-file |
| tests/ghost_lint_fixtures/p19_unlock_release | directory-fixture |
| tests/ghost_lint_fixtures/p19_unlock_release/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p19_unlock_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p19_unlock_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p19_write_before_ready | directory-fixture |
| tests/ghost_lint_fixtures/p19_write_before_ready/mutant_ready_first.rs | support-module |
| tests/ghost_lint_fixtures/p19_write_before_ready/ok.rs | support-module |
| tests/ghost_lint_fixtures/p19_write_before_ready/rule.json | other-file |
| tests/ghost_lint_fixtures/p2_claim_acqrel | directory-fixture |
| tests/ghost_lint_fixtures/p2_claim_acqrel/mutant_relaxed_failure.rs | support-module |
| tests/ghost_lint_fixtures/p2_claim_acqrel/mutant_release_success.rs | support-module |
| tests/ghost_lint_fixtures/p2_claim_acqrel/ok.rs | support-module |
| tests/ghost_lint_fixtures/p2_claim_acqrel/rule.json | other-file |
| tests/ghost_lint_fixtures/p2_drop_lost_cas_fatal | directory-fixture |
| tests/ghost_lint_fixtures/p2_drop_lost_cas_fatal/mutant_fatal_on_ok.rs | support-module |
| tests/ghost_lint_fixtures/p2_drop_lost_cas_fatal/ok.rs | support-module |
| tests/ghost_lint_fixtures/p2_drop_lost_cas_fatal/rule.json | other-file |
| tests/ghost_lint_fixtures/p2_drop_release | directory-fixture |
| tests/ghost_lint_fixtures/p2_drop_release/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p2_drop_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p2_drop_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p2_model_bind | directory-fixture |
| tests/ghost_lint_fixtures/p2_model_bind/mutant_claim_release.rs | support-module |
| tests/ghost_lint_fixtures/p2_model_bind/mutant_recycle_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p2_model_bind/ok.rs | support-module |
| tests/ghost_lint_fixtures/p2_model_bind/rule.json | other-file |
| tests/ghost_lint_fixtures/p20_rollback_release | directory-fixture |
| tests/ghost_lint_fixtures/p20_rollback_release/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p20_rollback_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p20_rollback_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p20_running_acquire | directory-fixture |
| tests/ghost_lint_fixtures/p20_running_acquire/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p20_running_acquire/ok.rs | support-module |
| tests/ghost_lint_fixtures/p20_running_acquire/rule.json | other-file |
| tests/ghost_lint_fixtures/p20_running_release | directory-fixture |
| tests/ghost_lint_fixtures/p20_running_release/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p20_running_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p20_running_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p20_running_under_control | directory-fixture |
| tests/ghost_lint_fixtures/p20_running_under_control/mutant_unlocked.rs | support-module |
| tests/ghost_lint_fixtures/p20_running_under_control/ok.rs | support-module |
| tests/ghost_lint_fixtures/p20_running_under_control/rule.json | other-file |
| tests/ghost_lint_fixtures/p20_start_cas | directory-fixture |
| tests/ghost_lint_fixtures/p20_start_cas/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p20_start_cas/ok.rs | support-module |
| tests/ghost_lint_fixtures/p20_start_cas/rule.json | other-file |
| tests/ghost_lint_fixtures/p20_start_wait_acquire | directory-fixture |
| tests/ghost_lint_fixtures/p20_start_wait_acquire/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p20_start_wait_acquire/ok.rs | support-module |
| tests/ghost_lint_fixtures/p20_start_wait_acquire/rule.json | other-file |
| tests/ghost_lint_fixtures/p21_acquire_cas | directory-fixture |
| tests/ghost_lint_fixtures/p21_acquire_cas/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p21_acquire_cas/ok.rs | support-module |
| tests/ghost_lint_fixtures/p21_acquire_cas/rule.json | other-file |
| tests/ghost_lint_fixtures/p21_ghost_acquire | directory-fixture |
| tests/ghost_lint_fixtures/p21_ghost_acquire/mutant_dropped.rs | support-module |
| tests/ghost_lint_fixtures/p21_ghost_acquire/ok.rs | support-module |
| tests/ghost_lint_fixtures/p21_ghost_acquire/rule.json | other-file |
| tests/ghost_lint_fixtures/p21_ghost_release | directory-fixture |
| tests/ghost_lint_fixtures/p21_ghost_release/mutant_dropped.rs | support-module |
| tests/ghost_lint_fixtures/p21_ghost_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p21_ghost_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p21_hint_acquire | directory-fixture |
| tests/ghost_lint_fixtures/p21_hint_acquire/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p21_hint_acquire/ok.rs | support-module |
| tests/ghost_lint_fixtures/p21_hint_acquire/rule.json | other-file |
| tests/ghost_lint_fixtures/p21_insert_hint_before_release | directory-fixture |
| tests/ghost_lint_fixtures/p21_insert_hint_before_release/mutant_after_release.rs | support-module |
| tests/ghost_lint_fixtures/p21_insert_hint_before_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p21_insert_hint_before_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p21_insert_under_lock | directory-fixture |
| tests/ghost_lint_fixtures/p21_insert_under_lock/mutant_unlocked.rs | support-module |
| tests/ghost_lint_fixtures/p21_insert_under_lock/ok.rs | support-module |
| tests/ghost_lint_fixtures/p21_insert_under_lock/rule.json | other-file |
| tests/ghost_lint_fixtures/p21_peek_under_lock | directory-fixture |
| tests/ghost_lint_fixtures/p21_peek_under_lock/mutant_unlocked.rs | support-module |
| tests/ghost_lint_fixtures/p21_peek_under_lock/ok.rs | support-module |
| tests/ghost_lint_fixtures/p21_peek_under_lock/rule.json | other-file |
| tests/ghost_lint_fixtures/p21_release_store | directory-fixture |
| tests/ghost_lint_fixtures/p21_release_store/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p21_release_store/ok.rs | support-module |
| tests/ghost_lint_fixtures/p21_release_store/rule.json | other-file |
| tests/ghost_lint_fixtures/p21_take_hint_acquire | directory-fixture |
| tests/ghost_lint_fixtures/p21_take_hint_acquire/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p21_take_hint_acquire/ok.rs | support-module |
| tests/ghost_lint_fixtures/p21_take_hint_acquire/rule.json | other-file |
| tests/ghost_lint_fixtures/p21_take_hint_before_release | directory-fixture |
| tests/ghost_lint_fixtures/p21_take_hint_before_release/mutant_after_release.rs | support-module |
| tests/ghost_lint_fixtures/p21_take_hint_before_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p21_take_hint_before_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p21_take_under_lock | directory-fixture |
| tests/ghost_lint_fixtures/p21_take_under_lock/mutant_unlocked.rs | support-module |
| tests/ghost_lint_fixtures/p21_take_under_lock/ok.rs | support-module |
| tests/ghost_lint_fixtures/p21_take_under_lock/rule.json | other-file |
| tests/ghost_lint_fixtures/p23_arm_before_drain | directory-fixture |
| tests/ghost_lint_fixtures/p23_arm_before_drain/mutant_drain_first.rs | support-module |
| tests/ghost_lint_fixtures/p23_arm_before_drain/ok.rs | support-module |
| tests/ghost_lint_fixtures/p23_arm_before_drain/rule.json | other-file |
| tests/ghost_lint_fixtures/p23_arm_fence | directory-fixture |
| tests/ghost_lint_fixtures/p23_arm_fence/mutant_acqrel.rs | support-module |
| tests/ghost_lint_fixtures/p23_arm_fence/ok.rs | support-module |
| tests/ghost_lint_fixtures/p23_arm_fence/rule.json | other-file |
| tests/ghost_lint_fixtures/p23_arm_release | directory-fixture |
| tests/ghost_lint_fixtures/p23_arm_release/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p23_arm_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p23_arm_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p23_arm_then_fence | directory-fixture |
| tests/ghost_lint_fixtures/p23_arm_then_fence/mutant_fence_first.rs | support-module |
| tests/ghost_lint_fixtures/p23_arm_then_fence/ok.rs | support-module |
| tests/ghost_lint_fixtures/p23_arm_then_fence/rule.json | other-file |
| tests/ghost_lint_fixtures/p23_check_seqcst | directory-fixture |
| tests/ghost_lint_fixtures/p23_check_seqcst/mutant_acquire.rs | support-module |
| tests/ghost_lint_fixtures/p23_check_seqcst/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p23_check_seqcst/ok.rs | support-module |
| tests/ghost_lint_fixtures/p23_check_seqcst/rule.json | other-file |
| tests/ghost_lint_fixtures/p23_mask_before_summary | directory-fixture |
| tests/ghost_lint_fixtures/p23_mask_before_summary/mutant_summary_first.rs | support-module |
| tests/ghost_lint_fixtures/p23_mask_before_summary/ok.rs | support-module |
| tests/ghost_lint_fixtures/p23_mask_before_summary/rule.json | other-file |
| tests/ghost_lint_fixtures/p23_notify_cas | directory-fixture |
| tests/ghost_lint_fixtures/p23_notify_cas/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p23_notify_cas/ok.rs | support-module |
| tests/ghost_lint_fixtures/p23_notify_cas/rule.json | other-file |
| tests/ghost_lint_fixtures/p23_rmw_before_check | directory-fixture |
| tests/ghost_lint_fixtures/p23_rmw_before_check/mutant_check_first.rs | support-module |
| tests/ghost_lint_fixtures/p23_rmw_before_check/ok.rs | support-module |
| tests/ghost_lint_fixtures/p23_rmw_before_check/rule.json | other-file |
| tests/ghost_lint_fixtures/p23_summary_release | directory-fixture |
| tests/ghost_lint_fixtures/p23_summary_release/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p23_summary_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p23_summary_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p23_take_mask_acquire | directory-fixture |
| tests/ghost_lint_fixtures/p23_take_mask_acquire/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p23_take_mask_acquire/ok.rs | support-module |
| tests/ghost_lint_fixtures/p23_take_mask_acquire/rule.json | other-file |
| tests/ghost_lint_fixtures/p23_take_summary_acquire | directory-fixture |
| tests/ghost_lint_fixtures/p23_take_summary_acquire/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p23_take_summary_acquire/ok.rs | support-module |
| tests/ghost_lint_fixtures/p23_take_summary_acquire/rule.json | other-file |
| tests/ghost_lint_fixtures/p24_claim_before_words | directory-fixture |
| tests/ghost_lint_fixtures/p24_claim_before_words/mutant_word_before_claim.rs | support-module |
| tests/ghost_lint_fixtures/p24_claim_before_words/ok.rs | support-module |
| tests/ghost_lint_fixtures/p24_claim_before_words/rule.json | other-file |
| tests/ghost_lint_fixtures/p24_claim_cas | directory-fixture |
| tests/ghost_lint_fixtures/p24_claim_cas/mutant_acquire_only.rs | support-module |
| tests/ghost_lint_fixtures/p24_claim_cas/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p24_claim_cas/mutant_second_cas.rs | support-module |
| tests/ghost_lint_fixtures/p24_claim_cas/ok.rs | support-module |
| tests/ghost_lint_fixtures/p24_claim_cas/rule.json | other-file |
| tests/ghost_lint_fixtures/p24_loser_needs_published | directory-fixture |
| tests/ghost_lint_fixtures/p24_loser_needs_published/mutant_any_err.rs | support-module |
| tests/ghost_lint_fixtures/p24_loser_needs_published/mutant_publishing_reads.rs | support-module |
| tests/ghost_lint_fixtures/p24_loser_needs_published/mutant_winner_reads.rs | support-module |
| tests/ghost_lint_fixtures/p24_loser_needs_published/ok.rs | support-module |
| tests/ghost_lint_fixtures/p24_loser_needs_published/rule.json | other-file |
| tests/ghost_lint_fixtures/p24_no_spin_primitive | directory-fixture |
| tests/ghost_lint_fixtures/p24_no_spin_primitive/mutant_spin_in_reader.rs | support-module |
| tests/ghost_lint_fixtures/p24_no_spin_primitive/mutant_spin_loser.rs | support-module |
| tests/ghost_lint_fixtures/p24_no_spin_primitive/ok.rs | support-module |
| tests/ghost_lint_fixtures/p24_no_spin_primitive/rule.json | other-file |
| tests/ghost_lint_fixtures/p24_publish_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p24_publish_inventory/mutant_extra_word.rs | support-module |
| tests/ghost_lint_fixtures/p24_publish_inventory/mutant_published_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p24_publish_inventory/mutant_spin_loser.rs | support-module |
| tests/ghost_lint_fixtures/p24_publish_inventory/mutant_weak_cas.rs | support-module |
| tests/ghost_lint_fixtures/p24_publish_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p24_publish_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p24_published_release | directory-fixture |
| tests/ghost_lint_fixtures/p24_published_release/mutant_acquire.rs | support-module |
| tests/ghost_lint_fixtures/p24_published_release/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p24_published_release/mutant_second_store.rs | support-module |
| tests/ghost_lint_fixtures/p24_published_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p24_published_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p24_reader_acquire | directory-fixture |
| tests/ghost_lint_fixtures/p24_reader_acquire/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p24_reader_acquire/mutant_spin_reader.rs | support-module |
| tests/ghost_lint_fixtures/p24_reader_acquire/ok.rs | support-module |
| tests/ghost_lint_fixtures/p24_reader_acquire/rule.json | other-file |
| tests/ghost_lint_fixtures/p24_reader_needs_published | directory-fixture |
| tests/ghost_lint_fixtures/p24_reader_needs_published/mutant_inverted.rs | support-module |
| tests/ghost_lint_fixtures/p24_reader_needs_published/mutant_relaxed_guard.rs | support-module |
| tests/ghost_lint_fixtures/p24_reader_needs_published/mutant_unchecked.rs | support-module |
| tests/ghost_lint_fixtures/p24_reader_needs_published/ok.rs | support-module |
| tests/ghost_lint_fixtures/p24_reader_needs_published/rule.json | other-file |
| tests/ghost_lint_fixtures/p24_word_loads_relaxed | directory-fixture |
| tests/ghost_lint_fixtures/p24_word_loads_relaxed/mutant_acquire_word.rs | support-module |
| tests/ghost_lint_fixtures/p24_word_loads_relaxed/mutant_four_words.rs | support-module |
| tests/ghost_lint_fixtures/p24_word_loads_relaxed/ok.rs | support-module |
| tests/ghost_lint_fixtures/p24_word_loads_relaxed/rule.json | other-file |
| tests/ghost_lint_fixtures/p24_word_stores_relaxed | directory-fixture |
| tests/ghost_lint_fixtures/p24_word_stores_relaxed/mutant_missing_word.rs | support-module |
| tests/ghost_lint_fixtures/p24_word_stores_relaxed/mutant_release_word.rs | support-module |
| tests/ghost_lint_fixtures/p24_word_stores_relaxed/ok.rs | support-module |
| tests/ghost_lint_fixtures/p24_word_stores_relaxed/rule.json | other-file |
| tests/ghost_lint_fixtures/p24_words_before_published | directory-fixture |
| tests/ghost_lint_fixtures/p24_words_before_published/mutant_word_after_published.rs | support-module |
| tests/ghost_lint_fixtures/p24_words_before_published/mutant_word_missing.rs | support-module |
| tests/ghost_lint_fixtures/p24_words_before_published/ok.rs | support-module |
| tests/ghost_lint_fixtures/p24_words_before_published/rule.json | other-file |
| tests/ghost_lint_fixtures/p24_words_sole_writer | directory-fixture |
| tests/ghost_lint_fixtures/p24_words_sole_writer/mutant_load_words_writes.rs | support-module |
| tests/ghost_lint_fixtures/p24_words_sole_writer/mutant_read_writes.rs | support-module |
| tests/ghost_lint_fixtures/p24_words_sole_writer/ok.rs | support-module |
| tests/ghost_lint_fixtures/p24_words_sole_writer/rule.json | other-file |
| tests/ghost_lint_fixtures/p25_cas_after_observe | directory-fixture |
| tests/ghost_lint_fixtures/p25_cas_after_observe/mutant_blind_expected.rs | support-module |
| tests/ghost_lint_fixtures/p25_cas_after_observe/mutant_no_observation.rs | support-module |
| tests/ghost_lint_fixtures/p25_cas_after_observe/ok.rs | support-module |
| tests/ghost_lint_fixtures/p25_cas_after_observe/rule.json | other-file |
| tests/ghost_lint_fixtures/p25_checked_release_sole_path | directory-fixture |
| tests/ghost_lint_fixtures/p25_checked_release_sole_path/mutant_bypass.rs | support-module |
| tests/ghost_lint_fixtures/p25_checked_release_sole_path/ok.rs | support-module |
| tests/ghost_lint_fixtures/p25_checked_release_sole_path/rule.json | other-file |
| tests/ghost_lint_fixtures/p25_give_back_fetch_sub | directory-fixture |
| tests/ghost_lint_fixtures/p25_give_back_fetch_sub/mutant_acquire.rs | support-module |
| tests/ghost_lint_fixtures/p25_give_back_fetch_sub/mutant_seqcst.rs | support-module |
| tests/ghost_lint_fixtures/p25_give_back_fetch_sub/mutant_store_instead.rs | support-module |
| tests/ghost_lint_fixtures/p25_give_back_fetch_sub/ok.rs | support-module |
| tests/ghost_lint_fixtures/p25_give_back_fetch_sub/rule.json | other-file |
| tests/ghost_lint_fixtures/p25_large_release_sole_path | directory-fixture |
| tests/ghost_lint_fixtures/p25_large_release_sole_path/mutant_direct_release.rs | support-module |
| tests/ghost_lint_fixtures/p25_large_release_sole_path/mutant_free_fn_release.rs | support-module |
| tests/ghost_lint_fixtures/p25_large_release_sole_path/ok.rs | support-module |
| tests/ghost_lint_fixtures/p25_large_release_sole_path/rule.json | other-file |
| tests/ghost_lint_fixtures/p25_reserve_bounded | directory-fixture |
| tests/ghost_lint_fixtures/p25_reserve_bounded/mutant_other_bound.rs | support-module |
| tests/ghost_lint_fixtures/p25_reserve_bounded/mutant_unbounded_loop.rs | support-module |
| tests/ghost_lint_fixtures/p25_reserve_bounded/mutant_while_true.rs | support-module |
| tests/ghost_lint_fixtures/p25_reserve_bounded/ok.rs | support-module |
| tests/ghost_lint_fixtures/p25_reserve_bounded/rule.json | other-file |
| tests/ghost_lint_fixtures/p25_reserve_cas_relaxed | directory-fixture |
| tests/ghost_lint_fixtures/p25_reserve_cas_relaxed/mutant_acqrel.rs | support-module |
| tests/ghost_lint_fixtures/p25_reserve_cas_relaxed/mutant_fetch_add_reserve.rs | support-module |
| tests/ghost_lint_fixtures/p25_reserve_cas_relaxed/mutant_seqcst.rs | support-module |
| tests/ghost_lint_fixtures/p25_reserve_cas_relaxed/mutant_store_reserve.rs | support-module |
| tests/ghost_lint_fixtures/p25_reserve_cas_relaxed/ok.rs | support-module |
| tests/ghost_lint_fixtures/p25_reserve_cas_relaxed/rule.json | other-file |
| tests/ghost_lint_fixtures/p25_reserve_load_relaxed | directory-fixture |
| tests/ghost_lint_fixtures/p25_reserve_load_relaxed/mutant_acquire.rs | support-module |
| tests/ghost_lint_fixtures/p25_reserve_load_relaxed/mutant_seqcst.rs | support-module |
| tests/ghost_lint_fixtures/p25_reserve_load_relaxed/ok.rs | support-module |
| tests/ghost_lint_fixtures/p25_reserve_load_relaxed/rule.json | other-file |
| tests/ghost_lint_fixtures/p25_return_needs_checked_success | directory-fixture |
| tests/ghost_lint_fixtures/p25_return_needs_checked_success/mutant_error_branch.rs | support-module |
| tests/ghost_lint_fixtures/p25_return_needs_checked_success/mutant_ignored_result.rs | support-module |
| tests/ghost_lint_fixtures/p25_return_needs_checked_success/ok.rs | support-module |
| tests/ghost_lint_fixtures/p25_return_needs_checked_success/rule.json | other-file |
| tests/ghost_lint_fixtures/p25_unmap_before_give_back | directory-fixture |
| tests/ghost_lint_fixtures/p25_unmap_before_give_back/mutant_give_back_first.rs | support-module |
| tests/ghost_lint_fixtures/p25_unmap_before_give_back/mutant_no_unmap.rs | support-module |
| tests/ghost_lint_fixtures/p25_unmap_before_give_back/ok.rs | support-module |
| tests/ghost_lint_fixtures/p25_unmap_before_give_back/rule.json | other-file |
| tests/ghost_lint_fixtures/p25_used_relaxed | directory-fixture |
| tests/ghost_lint_fixtures/p25_used_relaxed/mutant_acquire.rs | support-module |
| tests/ghost_lint_fixtures/p25_used_relaxed/mutant_seqcst.rs | support-module |
| tests/ghost_lint_fixtures/p25_used_relaxed/ok.rs | support-module |
| tests/ghost_lint_fixtures/p25_used_relaxed/rule.json | other-file |
| tests/ghost_lint_fixtures/p26_diagnostic_counter_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p26_diagnostic_counter_inventory/mutant_load_store.rs | support-module |
| tests/ghost_lint_fixtures/p26_diagnostic_counter_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p26_diagnostic_counter_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p26_ordinal_before_decision | directory-fixture |
| tests/ghost_lint_fixtures/p26_ordinal_before_decision/mutant_decide_before_ticket.rs | support-module |
| tests/ghost_lint_fixtures/p26_ordinal_before_decision/mutant_variable_seed.rs | support-module |
| tests/ghost_lint_fixtures/p26_ordinal_before_decision/ok.rs | support-module |
| tests/ghost_lint_fixtures/p26_ordinal_before_decision/rule.json | other-file |
| tests/ghost_lint_fixtures/p26_ordinal_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p26_ordinal_inventory/mutant_load_instead_of_rmw.rs | support-module |
| tests/ghost_lint_fixtures/p26_ordinal_inventory/mutant_second_rmw.rs | support-module |
| tests/ghost_lint_fixtures/p26_ordinal_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p26_ordinal_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p26_ordinal_probe | directory-fixture |
| tests/ghost_lint_fixtures/p26_ordinal_probe/mutant_fetch_add.rs | support-module |
| tests/ghost_lint_fixtures/p26_ordinal_probe/mutant_seqcst_load.rs | support-module |
| tests/ghost_lint_fixtures/p26_ordinal_probe/ok.rs | support-module |
| tests/ghost_lint_fixtures/p26_ordinal_probe/rule.json | other-file |
| tests/ghost_lint_fixtures/p26_telemetry_load_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p26_telemetry_load_inventory/mutant_acquire_load.rs | support-module |
| tests/ghost_lint_fixtures/p26_telemetry_load_inventory/mutant_extra_load.rs | support-module |
| tests/ghost_lint_fixtures/p26_telemetry_load_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p26_telemetry_load_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p26_ticket_log_after_decision | directory-fixture |
| tests/ghost_lint_fixtures/p26_ticket_log_after_decision/mutant_log_before_decision.rs | support-module |
| tests/ghost_lint_fixtures/p26_ticket_log_after_decision/ok.rs | support-module |
| tests/ghost_lint_fixtures/p26_ticket_log_after_decision/rule.json | other-file |
| tests/ghost_lint_fixtures/p26_ticket_log_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p26_ticket_log_inventory/mutant_cursor_load_store.rs | support-module |
| tests/ghost_lint_fixtures/p26_ticket_log_inventory/mutant_missing_level_store.rs | support-module |
| tests/ghost_lint_fixtures/p26_ticket_log_inventory/mutant_seqcst_level_store.rs | support-module |
| tests/ghost_lint_fixtures/p26_ticket_log_inventory/mutant_seqcst_store.rs | support-module |
| tests/ghost_lint_fixtures/p26_ticket_log_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p26_ticket_log_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p26_ticket_log_reset_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p26_ticket_log_reset_inventory/mutant_fetch_add.rs | support-module |
| tests/ghost_lint_fixtures/p26_ticket_log_reset_inventory/mutant_seqcst_store.rs | support-module |
| tests/ghost_lint_fixtures/p26_ticket_log_reset_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p26_ticket_log_reset_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p26_ticket_log_snapshot_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p26_ticket_log_snapshot_inventory/mutant_cursor_seqcst_load.rs | support-module |
| tests/ghost_lint_fixtures/p26_ticket_log_snapshot_inventory/mutant_extra_slot_load.rs | support-module |
| tests/ghost_lint_fixtures/p26_ticket_log_snapshot_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p26_ticket_log_snapshot_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p27_cas_after_observe | directory-fixture |
| tests/ghost_lint_fixtures/p27_cas_after_observe/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p27_cas_after_observe/ok.rs | support-module |
| tests/ghost_lint_fixtures/p27_cas_after_observe/rule.json | other-file |
| tests/ghost_lint_fixtures/p27_diagnostic_load_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p27_diagnostic_load_inventory/mutant_acquire.rs | support-module |
| tests/ghost_lint_fixtures/p27_diagnostic_load_inventory/mutant_extra_load.rs | support-module |
| tests/ghost_lint_fixtures/p27_diagnostic_load_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p27_diagnostic_load_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p27_snapshot_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p27_snapshot_inventory/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p27_snapshot_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p27_snapshot_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p27_update_bounded | directory-fixture |
| tests/ghost_lint_fixtures/p27_update_bounded/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p27_update_bounded/ok.rs | support-module |
| tests/ghost_lint_fixtures/p27_update_bounded/rule.json | other-file |
| tests/ghost_lint_fixtures/p27_update_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p27_update_inventory/mutant_second_load.rs | support-module |
| tests/ghost_lint_fixtures/p27_update_inventory/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p27_update_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p27_update_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_begin | directory-fixture |
| tests/ghost_lint_fixtures/p28_begin/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_begin/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_begin/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_boundary | directory-fixture |
| tests/ghost_lint_fixtures/p28_boundary/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_boundary/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_boundary/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_checked_release_marker | directory-fixture |
| tests/ghost_lint_fixtures/p28_checked_release_marker/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_checked_release_marker/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_checked_release_marker/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_failure | directory-fixture |
| tests/ghost_lint_fixtures/p28_failure_read | directory-fixture |
| tests/ghost_lint_fixtures/p28_failure_read/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_failure_read/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_failure_read/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_failure/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_failure/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_failure/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_fi_complete | directory-fixture |
| tests/ghost_lint_fixtures/p28_fi_complete/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_fi_complete/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_fi_complete/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_fi_reserve | directory-fixture |
| tests/ghost_lint_fixtures/p28_fi_reserve/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_fi_reserve/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_fi_reserve/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_fi_seed | directory-fixture |
| tests/ghost_lint_fixtures/p28_fi_seed/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_fi_seed/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_fi_seed/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_legacy_release_marker | directory-fixture |
| tests/ghost_lint_fixtures/p28_legacy_release_marker/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_legacy_release_marker/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_legacy_release_marker/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_prefix_read | directory-fixture |
| tests/ghost_lint_fixtures/p28_prefix_read/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_prefix_read/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_prefix_read/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_publish_field1_order | directory-fixture |
| tests/ghost_lint_fixtures/p28_publish_field1_order/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_publish_field1_order/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_publish_field1_order/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_publish_field2_order | directory-fixture |
| tests/ghost_lint_fixtures/p28_publish_field2_order/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_publish_field2_order/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_publish_field2_order/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_publish_field3_order | directory-fixture |
| tests/ghost_lint_fixtures/p28_publish_field3_order/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_publish_field3_order/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_publish_field3_order/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_publish_field4_order | directory-fixture |
| tests/ghost_lint_fixtures/p28_publish_field4_order/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_publish_field4_order/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_publish_field4_order/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_publish_field5_order | directory-fixture |
| tests/ghost_lint_fixtures/p28_publish_field5_order/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_publish_field5_order/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_publish_field5_order/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_publish_field6_order | directory-fixture |
| tests/ghost_lint_fixtures/p28_publish_field6_order/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_publish_field6_order/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_publish_field6_order/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_publish_order | directory-fixture |
| tests/ghost_lint_fixtures/p28_publish_order/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_publish_order/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_publish_order/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_read_order | directory-fixture |
| tests/ghost_lint_fixtures/p28_read_order/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_read_order/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_read_order/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_release_hint | directory-fixture |
| tests/ghost_lint_fixtures/p28_release_hint/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_release_hint/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_release_hint/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_reservation | directory-fixture |
| tests/ghost_lint_fixtures/p28_reservation/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_reservation/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_reservation/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_slot_publish | directory-fixture |
| tests/ghost_lint_fixtures/p28_slot_publish/mutant_missing_payload.rs | support-module |
| tests/ghost_lint_fixtures/p28_slot_publish/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_slot_publish/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_slot_publish/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_slot_read | directory-fixture |
| tests/ghost_lint_fixtures/p28_slot_read/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_slot_read/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_slot_read/rule.json | other-file |
| tests/ghost_lint_fixtures/p28_started_read | directory-fixture |
| tests/ghost_lint_fixtures/p28_started_read/mutant_violation.rs | support-module |
| tests/ghost_lint_fixtures/p28_started_read/ok.rs | support-module |
| tests/ghost_lint_fixtures/p28_started_read/rule.json | other-file |
| tests/ghost_lint_fixtures/p29_agree_bounded | directory-fixture |
| tests/ghost_lint_fixtures/p29_agree_bounded/mutant_unbounded_loop.rs | support-module |
| tests/ghost_lint_fixtures/p29_agree_bounded/ok.rs | support-module |
| tests/ghost_lint_fixtures/p29_agree_bounded/rule.json | other-file |
| tests/ghost_lint_fixtures/p29_agree_cas_after_load | directory-fixture |
| tests/ghost_lint_fixtures/p29_agree_cas_after_load/mutant_constant_expected.rs | support-module |
| tests/ghost_lint_fixtures/p29_agree_cas_after_load/ok.rs | support-module |
| tests/ghost_lint_fixtures/p29_agree_cas_after_load/rule.json | other-file |
| tests/ghost_lint_fixtures/p29_agree_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p29_agree_inventory/mutant_blind_store.rs | support-module |
| tests/ghost_lint_fixtures/p29_agree_inventory/mutant_seqcst_cas.rs | support-module |
| tests/ghost_lint_fixtures/p29_agree_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p29_agree_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p29_disagree_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p29_disagree_inventory/mutant_bounded_cas_reset.rs | support-module |
| tests/ghost_lint_fixtures/p29_disagree_inventory/mutant_seqcst_reset.rs | support-module |
| tests/ghost_lint_fixtures/p29_disagree_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p29_disagree_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p29_level_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p29_level_inventory/mutant_second_load.rs | support-module |
| tests/ghost_lint_fixtures/p29_level_inventory/mutant_seqcst_load.rs | support-module |
| tests/ghost_lint_fixtures/p29_level_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p29_level_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p29_level_read_guarded | directory-fixture |
| tests/ghost_lint_fixtures/p29_level_read_guarded/mutant_unconditional_level.rs | support-module |
| tests/ghost_lint_fixtures/p29_level_read_guarded/ok.rs | support-module |
| tests/ghost_lint_fixtures/p29_level_read_guarded/rule.json | other-file |
| tests/ghost_lint_fixtures/p29_run_clear_sole_path | directory-fixture |
| tests/ghost_lint_fixtures/p29_run_clear_sole_path/mutant_second_clearer.rs | support-module |
| tests/ghost_lint_fixtures/p29_run_clear_sole_path/ok.rs | support-module |
| tests/ghost_lint_fixtures/p29_run_clear_sole_path/rule.json | other-file |
| tests/ghost_lint_fixtures/p3_early_release_on_exit | directory-fixture |
| tests/ghost_lint_fixtures/p3_early_release_on_exit/mutant_no_early_exit.rs | support-module |
| tests/ghost_lint_fixtures/p3_early_release_on_exit/mutant_other_guard.rs | support-module |
| tests/ghost_lint_fixtures/p3_early_release_on_exit/ok.rs | support-module |
| tests/ghost_lint_fixtures/p3_early_release_on_exit/rule.json | other-file |
| tests/ghost_lint_fixtures/p3_guard_lease_cell | directory-fixture |
| tests/ghost_lint_fixtures/p3_guard_lease_cell/mutant_bare_lease.rs | support-module |
| tests/ghost_lint_fixtures/p3_guard_lease_cell/mutant_refcell.rs | support-module |
| tests/ghost_lint_fixtures/p3_guard_lease_cell/ok.rs | support-module |
| tests/ghost_lint_fixtures/p3_guard_lease_cell/rule.json | other-file |
| tests/ghost_lint_fixtures/p3_local_dropless | directory-fixture |
| tests/ghost_lint_fixtures/p3_local_dropless/mutant_lazy_init.rs | support-module |
| tests/ghost_lint_fixtures/p3_local_dropless/mutant_not_thread_local.rs | support-module |
| tests/ghost_lint_fixtures/p3_local_dropless/mutant_refcell.rs | support-module |
| tests/ghost_lint_fixtures/p3_local_dropless/ok.rs | support-module |
| tests/ghost_lint_fixtures/p3_local_dropless/rule.json | other-file |
| tests/ghost_lint_fixtures/p3_model_bind | directory-fixture |
| tests/ghost_lint_fixtures/p3_model_bind/mutant_claim_release.rs | support-module |
| tests/ghost_lint_fixtures/p3_model_bind/mutant_drop_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p3_model_bind/mutant_unbound.rs | support-module |
| tests/ghost_lint_fixtures/p3_model_bind/ok.rs | support-module |
| tests/ghost_lint_fixtures/p3_model_bind/rule.json | other-file |
| tests/ghost_lint_fixtures/p3_rollback_unpublish_first | directory-fixture |
| tests/ghost_lint_fixtures/p3_rollback_unpublish_first/mutant_no_unpublish.rs | support-module |
| tests/ghost_lint_fixtures/p3_rollback_unpublish_first/mutant_release_first.rs | support-module |
| tests/ghost_lint_fixtures/p3_rollback_unpublish_first/ok.rs | support-module |
| tests/ghost_lint_fixtures/p3_rollback_unpublish_first/rule.json | other-file |
| tests/ghost_lint_fixtures/p3_torn_before_release | directory-fixture |
| tests/ghost_lint_fixtures/p3_torn_before_release/mutant_torn_after_exit_check.rs | support-module |
| tests/ghost_lint_fixtures/p3_torn_before_release/mutant_torn_last.rs | support-module |
| tests/ghost_lint_fixtures/p3_torn_before_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p3_torn_before_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p3_torn_before_trim | directory-fixture |
| tests/ghost_lint_fixtures/p3_torn_before_trim/mutant_torn_in_branch.rs | support-module |
| tests/ghost_lint_fixtures/p3_torn_before_trim/mutant_trim_first.rs | support-module |
| tests/ghost_lint_fixtures/p3_torn_before_trim/ok.rs | support-module |
| tests/ghost_lint_fixtures/p3_torn_before_trim/rule.json | other-file |
| tests/ghost_lint_fixtures/p3_trim_before_release | directory-fixture |
| tests/ghost_lint_fixtures/p3_trim_before_release/mutant_early_unguarded.rs | support-module |
| tests/ghost_lint_fixtures/p3_trim_before_release/mutant_release_first.rs | support-module |
| tests/ghost_lint_fixtures/p3_trim_before_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p3_trim_before_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p4_bump_count_cas | directory-fixture |
| tests/ghost_lint_fixtures/p4_bump_count_cas/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p4_bump_count_cas/ok.rs | support-module |
| tests/ghost_lint_fixtures/p4_bump_count_cas/rule.json | other-file |
| tests/ghost_lint_fixtures/p4_claim_model_bind | directory-fixture |
| tests/ghost_lint_fixtures/p4_claim_model_bind/mutant_unbound.rs | support-module |
| tests/ghost_lint_fixtures/p4_claim_model_bind/mutant_weakened.rs | support-module |
| tests/ghost_lint_fixtures/p4_claim_model_bind/ok.rs | support-module |
| tests/ghost_lint_fixtures/p4_claim_model_bind/rule.json | other-file |
| tests/ghost_lint_fixtures/p4_forget_acqrel | directory-fixture |
| tests/ghost_lint_fixtures/p4_forget_acqrel/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p4_forget_acqrel/ok.rs | support-module |
| tests/ghost_lint_fixtures/p4_forget_acqrel/rule.json | other-file |
| tests/ghost_lint_fixtures/p4_free_set_model_bind | directory-fixture |
| tests/ghost_lint_fixtures/p4_free_set_model_bind/mutant_unbound.rs | support-module |
| tests/ghost_lint_fixtures/p4_free_set_model_bind/mutant_weakened.rs | support-module |
| tests/ghost_lint_fixtures/p4_free_set_model_bind/ok.rs | support-module |
| tests/ghost_lint_fixtures/p4_free_set_model_bind/rule.json | other-file |
| tests/ghost_lint_fixtures/p4_free_slots_model_bind | directory-fixture |
| tests/ghost_lint_fixtures/p4_free_slots_model_bind/mutant_unbound.rs | support-module |
| tests/ghost_lint_fixtures/p4_free_slots_model_bind/mutant_weakened.rs | support-module |
| tests/ghost_lint_fixtures/p4_free_slots_model_bind/ok.rs | support-module |
| tests/ghost_lint_fixtures/p4_free_slots_model_bind/rule.json | other-file |
| tests/ghost_lint_fixtures/p4_hint_take_spill | directory-fixture |
| tests/ghost_lint_fixtures/p4_hint_take_spill/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p4_hint_take_spill/ok.rs | support-module |
| tests/ghost_lint_fixtures/p4_hint_take_spill/rule.json | other-file |
| tests/ghost_lint_fixtures/p4_mark_cas | directory-fixture |
| tests/ghost_lint_fixtures/p4_mark_cas/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p4_mark_cas/mutant_store.rs | support-module |
| tests/ghost_lint_fixtures/p4_mark_cas/ok.rs | support-module |
| tests/ghost_lint_fixtures/p4_mark_cas/rule.json | other-file |
| tests/ghost_lint_fixtures/p4_publish_acqrel | directory-fixture |
| tests/ghost_lint_fixtures/p4_publish_acqrel/mutant_release.rs | support-module |
| tests/ghost_lint_fixtures/p4_publish_acqrel/ok.rs | support-module |
| tests/ghost_lint_fixtures/p4_publish_acqrel/rule.json | other-file |
| tests/ghost_lint_fixtures/p4_publish_summary | directory-fixture |
| tests/ghost_lint_fixtures/p4_publish_summary/mutant_missing.rs | support-module |
| tests/ghost_lint_fixtures/p4_publish_summary/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p4_publish_summary/ok.rs | support-module |
| tests/ghost_lint_fixtures/p4_publish_summary/rule.json | other-file |
| tests/ghost_lint_fixtures/p4_publish_word | directory-fixture |
| tests/ghost_lint_fixtures/p4_publish_word/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p4_publish_word/ok.rs | support-module |
| tests/ghost_lint_fixtures/p4_publish_word/rule.json | other-file |
| tests/ghost_lint_fixtures/p4_recheck_republish | directory-fixture |
| tests/ghost_lint_fixtures/p4_recheck_republish/mutant_no_recheck.rs | support-module |
| tests/ghost_lint_fixtures/p4_recheck_republish/mutant_unconditional.rs | support-module |
| tests/ghost_lint_fixtures/p4_recheck_republish/ok.rs | support-module |
| tests/ghost_lint_fixtures/p4_recheck_republish/rule.json | other-file |
| tests/ghost_lint_fixtures/p4_rmw_read | directory-fixture |
| tests/ghost_lint_fixtures/p4_rmw_read/mutant_plain_load.rs | support-module |
| tests/ghost_lint_fixtures/p4_rmw_read/ok.rs | support-module |
| tests/ghost_lint_fixtures/p4_rmw_read/rule.json | other-file |
| tests/ghost_lint_fixtures/p4_selector_model_bind | directory-fixture |
| tests/ghost_lint_fixtures/p4_selector_model_bind/mutant_unbound.rs | support-module |
| tests/ghost_lint_fixtures/p4_selector_model_bind/mutant_weakened.rs | support-module |
| tests/ghost_lint_fixtures/p4_selector_model_bind/ok.rs | support-module |
| tests/ghost_lint_fixtures/p4_selector_model_bind/rule.json | other-file |
| tests/ghost_lint_fixtures/p4_spill_model_bind | directory-fixture |
| tests/ghost_lint_fixtures/p4_spill_model_bind/mutant_unbound.rs | support-module |
| tests/ghost_lint_fixtures/p4_spill_model_bind/mutant_weakened.rs | support-module |
| tests/ghost_lint_fixtures/p4_spill_model_bind/ok.rs | support-module |
| tests/ghost_lint_fixtures/p4_spill_model_bind/rule.json | other-file |
| tests/ghost_lint_fixtures/p4_summary_clear_acqrel | directory-fixture |
| tests/ghost_lint_fixtures/p4_summary_clear_acqrel/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p4_summary_clear_acqrel/ok.rs | support-module |
| tests/ghost_lint_fixtures/p4_summary_clear_acqrel/rule.json | other-file |
| tests/ghost_lint_fixtures/p4_take_acqrel | directory-fixture |
| tests/ghost_lint_fixtures/p4_take_acqrel/mutant_release.rs | support-module |
| tests/ghost_lint_fixtures/p4_take_acqrel/ok.rs | support-module |
| tests/ghost_lint_fixtures/p4_take_acqrel/rule.json | other-file |
| tests/ghost_lint_fixtures/p5_decrement_before_free | directory-fixture |
| tests/ghost_lint_fixtures/p5_decrement_before_free/mutant_free_first.rs | support-module |
| tests/ghost_lint_fixtures/p5_decrement_before_free/ok.rs | support-module |
| tests/ghost_lint_fixtures/p5_decrement_before_free/rule.json | other-file |
| tests/ghost_lint_fixtures/p5_drop_acqrel | directory-fixture |
| tests/ghost_lint_fixtures/p5_drop_acqrel/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p5_drop_acqrel/mutant_release_only.rs | support-module |
| tests/ghost_lint_fixtures/p5_drop_acqrel/ok.rs | support-module |
| tests/ghost_lint_fixtures/p5_drop_acqrel/rule.json | other-file |
| tests/ghost_lint_fixtures/p5_free_sole_path | directory-fixture |
| tests/ghost_lint_fixtures/p5_free_sole_path/mutant_remove_frees.rs | support-module |
| tests/ghost_lint_fixtures/p5_free_sole_path/ok.rs | support-module |
| tests/ghost_lint_fixtures/p5_free_sole_path/rule.json | other-file |
| tests/ghost_lint_fixtures/p5_ghost_drop | directory-fixture |
| tests/ghost_lint_fixtures/p5_ghost_drop/mutant_check_removed.rs | support-module |
| tests/ghost_lint_fixtures/p5_ghost_drop/ok.rs | support-module |
| tests/ghost_lint_fixtures/p5_ghost_drop/rule.json | other-file |
| tests/ghost_lint_fixtures/p5_ghost_pin | directory-fixture |
| tests/ghost_lint_fixtures/p5_ghost_pin/mutant_check_removed.rs | support-module |
| tests/ghost_lint_fixtures/p5_ghost_pin/ok.rs | support-module |
| tests/ghost_lint_fixtures/p5_ghost_pin/rule.json | other-file |
| tests/ghost_lint_fixtures/p5_last_ref_free | directory-fixture |
| tests/ghost_lint_fixtures/p5_last_ref_free/mutant_free_before_check.rs | support-module |
| tests/ghost_lint_fixtures/p5_last_ref_free/mutant_wrong_count.rs | support-module |
| tests/ghost_lint_fixtures/p5_last_ref_free/ok.rs | support-module |
| tests/ghost_lint_fixtures/p5_last_ref_free/rule.json | other-file |
| tests/ghost_lint_fixtures/p5_model_bind | directory-fixture |
| tests/ghost_lint_fixtures/p5_model_bind/mutant_model_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p5_model_bind/mutant_unlink_drifted.rs | support-module |
| tests/ghost_lint_fixtures/p5_model_bind/ok.rs | support-module |
| tests/ghost_lint_fixtures/p5_model_bind/rule.json | other-file |
| tests/ghost_lint_fixtures/p5_pin_acqrel | directory-fixture |
| tests/ghost_lint_fixtures/p5_pin_acqrel/mutant_relaxed_success.rs | support-module |
| tests/ghost_lint_fixtures/p5_pin_acqrel/ok.rs | support-module |
| tests/ghost_lint_fixtures/p5_pin_acqrel/rule.json | other-file |
| tests/ghost_lint_fixtures/p5_unlink_release | directory-fixture |
| tests/ghost_lint_fixtures/p5_unlink_release/mutant_relaxed_unlink.rs | support-module |
| tests/ghost_lint_fixtures/p5_unlink_release/mutant_unlink_removed.rs | support-module |
| tests/ghost_lint_fixtures/p5_unlink_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p5_unlink_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p6_ghost_drop | directory-fixture |
| tests/ghost_lint_fixtures/p6_ghost_drop/mutant_dropped.rs | support-module |
| tests/ghost_lint_fixtures/p6_ghost_drop/ok.rs | support-module |
| tests/ghost_lint_fixtures/p6_ghost_drop/rule.json | other-file |
| tests/ghost_lint_fixtures/p6_ghost_evict | directory-fixture |
| tests/ghost_lint_fixtures/p6_ghost_evict/mutant_dropped.rs | support-module |
| tests/ghost_lint_fixtures/p6_ghost_evict/ok.rs | support-module |
| tests/ghost_lint_fixtures/p6_ghost_evict/rule.json | other-file |
| tests/ghost_lint_fixtures/p6_ghost_insert | directory-fixture |
| tests/ghost_lint_fixtures/p6_ghost_insert/mutant_insert_dropped.rs | support-module |
| tests/ghost_lint_fixtures/p6_ghost_insert/ok.rs | support-module |
| tests/ghost_lint_fixtures/p6_ghost_insert/rule.json | other-file |
| tests/ghost_lint_fixtures/p6_ghost_publish | directory-fixture |
| tests/ghost_lint_fixtures/p6_ghost_publish/mutant_all_dropped.rs | support-module |
| tests/ghost_lint_fixtures/p6_ghost_publish/mutant_unlink_dropped.rs | support-module |
| tests/ghost_lint_fixtures/p6_ghost_publish/ok.rs | support-module |
| tests/ghost_lint_fixtures/p6_ghost_publish/rule.json | other-file |
| tests/ghost_lint_fixtures/p6_ghost_sweep | directory-fixture |
| tests/ghost_lint_fixtures/p6_ghost_sweep/mutant_dropped.rs | support-module |
| tests/ghost_lint_fixtures/p6_ghost_sweep/ok.rs | support-module |
| tests/ghost_lint_fixtures/p6_ghost_sweep/rule.json | other-file |
| tests/ghost_lint_fixtures/p6_insert_non_fallback | directory-fixture |
| tests/ghost_lint_fixtures/p6_insert_non_fallback/mutant_unguarded.rs | support-module |
| tests/ghost_lint_fixtures/p6_insert_non_fallback/ok.rs | support-module |
| tests/ghost_lint_fixtures/p6_insert_non_fallback/rule.json | other-file |
| tests/ghost_lint_fixtures/p6_insert_sole_path | directory-fixture |
| tests/ghost_lint_fixtures/p6_insert_sole_path/mutant_fast_path_insert.rs | support-module |
| tests/ghost_lint_fixtures/p6_insert_sole_path/ok.rs | support-module |
| tests/ghost_lint_fixtures/p6_insert_sole_path/rule.json | other-file |
| tests/ghost_lint_fixtures/p6_tls_decl | directory-fixture |
| tests/ghost_lint_fixtures/p6_tls_decl/mutant_global_cell.rs | support-module |
| tests/ghost_lint_fixtures/p6_tls_decl/mutant_loom_twin_unsafe_cell.rs | support-module |
| tests/ghost_lint_fixtures/p6_tls_decl/mutant_twin_dropped.rs | support-module |
| tests/ghost_lint_fixtures/p6_tls_decl/ok.rs | support-module |
| tests/ghost_lint_fixtures/p6_tls_decl/rule.json | other-file |
| tests/ghost_lint_fixtures/p7_cut_acqrel | directory-fixture |
| tests/ghost_lint_fixtures/p7_cut_acqrel/mutant_load_store.rs | support-module |
| tests/ghost_lint_fixtures/p7_cut_acqrel/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p7_cut_acqrel/ok.rs | support-module |
| tests/ghost_lint_fixtures/p7_cut_acqrel/rule.json | other-file |
| tests/ghost_lint_fixtures/p7_ghost_cut | directory-fixture |
| tests/ghost_lint_fixtures/p7_ghost_cut/mutant_dropped.rs | support-module |
| tests/ghost_lint_fixtures/p7_ghost_cut/ok.rs | support-module |
| tests/ghost_lint_fixtures/p7_ghost_cut/rule.json | other-file |
| tests/ghost_lint_fixtures/p7_ghost_published | directory-fixture |
| tests/ghost_lint_fixtures/p7_ghost_published/mutant_dropped.rs | support-module |
| tests/ghost_lint_fixtures/p7_ghost_published/ok.rs | support-module |
| tests/ghost_lint_fixtures/p7_ghost_published/rule.json | other-file |
| tests/ghost_lint_fixtures/p7_issue_release | directory-fixture |
| tests/ghost_lint_fixtures/p7_issue_release/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p7_issue_release/ok.rs | support-module |
| tests/ghost_lint_fixtures/p7_issue_release/rule.json | other-file |
| tests/ghost_lint_fixtures/p7_publish_acqrel | directory-fixture |
| tests/ghost_lint_fixtures/p7_publish_acqrel/mutant_acqrel.rs | support-module |
| tests/ghost_lint_fixtures/p7_publish_acqrel/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p7_publish_acqrel/ok.rs | support-module |
| tests/ghost_lint_fixtures/p7_publish_acqrel/rule.json | other-file |
| tests/ghost_lint_fixtures/p7_publish_requires_issue | directory-fixture |
| tests/ghost_lint_fixtures/p7_publish_requires_issue/mutant_unchecked.rs | support-module |
| tests/ghost_lint_fixtures/p7_publish_requires_issue/mutant_wrong_check.rs | support-module |
| tests/ghost_lint_fixtures/p7_publish_requires_issue/ok.rs | support-module |
| tests/ghost_lint_fixtures/p7_publish_requires_issue/rule.json | other-file |
| tests/ghost_lint_fixtures/p8_drop_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p8_drop_inventory/mutant_acquire.rs | support-module |
| tests/ghost_lint_fixtures/p8_drop_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p8_drop_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p8_drop_walks_presence | directory-fixture |
| tests/ghost_lint_fixtures/p8_drop_walks_presence/mutant_full_scan.rs | support-module |
| tests/ghost_lint_fixtures/p8_drop_walks_presence/ok.rs | support-module |
| tests/ghost_lint_fixtures/p8_drop_walks_presence/rule.json | other-file |
| tests/ghost_lint_fixtures/p8_encoded_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p8_encoded_inventory/mutant_relaxed_load.rs | support-module |
| tests/ghost_lint_fixtures/p8_encoded_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p8_encoded_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p8_init_before_publish | directory-fixture |
| tests/ghost_lint_fixtures/p8_init_before_publish/mutant_publish_first.rs | support-module |
| tests/ghost_lint_fixtures/p8_init_before_publish/ok.rs | support-module |
| tests/ghost_lint_fixtures/p8_init_before_publish/rule.json | other-file |
| tests/ghost_lint_fixtures/p8_issue_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p8_issue_inventory/mutant_relaxed_store.rs | support-module |
| tests/ghost_lint_fixtures/p8_issue_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p8_issue_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p8_no_dense_in_prod | directory-fixture |
| tests/ghost_lint_fixtures/p8_no_dense_in_prod/mutant_dense_call.rs | support-module |
| tests/ghost_lint_fixtures/p8_no_dense_in_prod/ok.rs | support-module |
| tests/ghost_lint_fixtures/p8_no_dense_in_prod/rule.json | other-file |
| tests/ghost_lint_fixtures/p8_prepare_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p8_prepare_inventory/mutant_new_op.rs | support-module |
| tests/ghost_lint_fixtures/p8_prepare_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p8_prepare_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p8_prepared_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p8_prepared_inventory/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/p8_prepared_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p8_prepared_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p8_promote_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p8_promote_inventory/mutant_relaxed_presence.rs | support-module |
| tests/ghost_lint_fixtures/p8_promote_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p8_promote_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p8_publish_before_presence | directory-fixture |
| tests/ghost_lint_fixtures/p8_publish_before_presence/mutant_presence_first.rs | support-module |
| tests/ghost_lint_fixtures/p8_publish_before_presence/ok.rs | support-module |
| tests/ghost_lint_fixtures/p8_publish_before_presence/rule.json | other-file |
| tests/ghost_lint_fixtures/p8_uses_class_leaves | directory-fixture |
| tests/ghost_lint_fixtures/p8_uses_class_leaves/mutant_elsewhere.rs | support-module |
| tests/ghost_lint_fixtures/p8_uses_class_leaves/mutant_gone.rs | support-module |
| tests/ghost_lint_fixtures/p8_uses_class_leaves/ok.rs | support-module |
| tests/ghost_lint_fixtures/p8_uses_class_leaves/rule.json | other-file |
| tests/ghost_lint_fixtures/p9_begin_reuse_guard | directory-fixture |
| tests/ghost_lint_fixtures/p9_begin_reuse_guard/mutant_unchecked.rs | support-module |
| tests/ghost_lint_fixtures/p9_begin_reuse_guard/ok.rs | support-module |
| tests/ghost_lint_fixtures/p9_begin_reuse_guard/rule.json | other-file |
| tests/ghost_lint_fixtures/p9_begin_reuse_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p9_begin_reuse_inventory/mutant_relaxed_cas.rs | support-module |
| tests/ghost_lint_fixtures/p9_begin_reuse_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p9_begin_reuse_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p9_claim_live_guard | directory-fixture |
| tests/ghost_lint_fixtures/p9_claim_live_guard/mutant_wrong_phase.rs | support-module |
| tests/ghost_lint_fixtures/p9_claim_live_guard/ok.rs | support-module |
| tests/ghost_lint_fixtures/p9_claim_live_guard/rule.json | other-file |
| tests/ghost_lint_fixtures/p9_claim_live_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p9_claim_live_inventory/mutant_relaxed_load.rs | support-module |
| tests/ghost_lint_fixtures/p9_claim_live_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p9_claim_live_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p9_claim_pending_guard | directory-fixture |
| tests/ghost_lint_fixtures/p9_claim_pending_guard/mutant_unchecked.rs | support-module |
| tests/ghost_lint_fixtures/p9_claim_pending_guard/ok.rs | support-module |
| tests/ghost_lint_fixtures/p9_claim_pending_guard/rule.json | other-file |
| tests/ghost_lint_fixtures/p9_claim_pending_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p9_claim_pending_inventory/mutant_relaxed_load.rs | support-module |
| tests/ghost_lint_fixtures/p9_claim_pending_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p9_claim_pending_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p9_ghost_claim | directory-fixture |
| tests/ghost_lint_fixtures/p9_ghost_claim/mutant_dropped.rs | support-module |
| tests/ghost_lint_fixtures/p9_ghost_claim/ok.rs | support-module |
| tests/ghost_lint_fixtures/p9_ghost_claim/rule.json | other-file |
| tests/ghost_lint_fixtures/p9_ghost_publish | directory-fixture |
| tests/ghost_lint_fixtures/p9_ghost_publish/mutant_dropped.rs | support-module |
| tests/ghost_lint_fixtures/p9_ghost_publish/ok.rs | support-module |
| tests/ghost_lint_fixtures/p9_ghost_publish/rule.json | other-file |
| tests/ghost_lint_fixtures/p9_publish_pending_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p9_publish_pending_inventory/mutant_relaxed_success.rs | support-module |
| tests/ghost_lint_fixtures/p9_publish_pending_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p9_publish_pending_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/p9_transition_inventory | directory-fixture |
| tests/ghost_lint_fixtures/p9_transition_inventory/mutant_blind_store.rs | support-module |
| tests/ghost_lint_fixtures/p9_transition_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/p9_transition_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/r1_ordering | directory-fixture |
| tests/ghost_lint_fixtures/r1_ordering/mutant_ambiguous_fn.rs | support-module |
| tests/ghost_lint_fixtures/r1_ordering/mutant_moved_to_helper.rs | support-module |
| tests/ghost_lint_fixtures/r1_ordering/mutant_relaxed.rs | support-module |
| tests/ghost_lint_fixtures/r1_ordering/mutant_second_publication.rs | support-module |
| tests/ghost_lint_fixtures/r1_ordering/ok.rs | support-module |
| tests/ghost_lint_fixtures/r1_ordering/rule.json | other-file |
| tests/ghost_lint_fixtures/r10_decl | directory-fixture |
| tests/ghost_lint_fixtures/r10_decl/mutant_init.rs | support-module |
| tests/ghost_lint_fixtures/r10_decl/mutant_missing.rs | support-module |
| tests/ghost_lint_fixtures/r10_decl/mutant_type.rs | support-module |
| tests/ghost_lint_fixtures/r10_decl/ok.rs | support-module |
| tests/ghost_lint_fixtures/r10_decl/rule.json | other-file |
| tests/ghost_lint_fixtures/r2_inventory | directory-fixture |
| tests/ghost_lint_fixtures/r2_inventory/mutant_extra_op.rs | support-module |
| tests/ghost_lint_fixtures/r2_inventory/mutant_missing_store.rs | support-module |
| tests/ghost_lint_fixtures/r2_inventory/ok.rs | support-module |
| tests/ghost_lint_fixtures/r2_inventory/rule.json | other-file |
| tests/ghost_lint_fixtures/r3_nested_before | directory-fixture |
| tests/ghost_lint_fixtures/r3_nested_before/mutant_after_first.rs | support-module |
| tests/ghost_lint_fixtures/r3_nested_before/ok.rs | support-module |
| tests/ghost_lint_fixtures/r3_nested_before/rule.json | other-file |
| tests/ghost_lint_fixtures/r3_sequence | directory-fixture |
| tests/ghost_lint_fixtures/r3_sequence/mutant_drain_in_branch.rs | support-module |
| tests/ghost_lint_fixtures/r3_sequence/mutant_release_first.rs | support-module |
| tests/ghost_lint_fixtures/r3_sequence/ok.rs | support-module |
| tests/ghost_lint_fixtures/r3_sequence/rule.json | other-file |
| tests/ghost_lint_fixtures/r4_guard | directory-fixture |
| tests/ghost_lint_fixtures/r4_guard/mutant_free_before_check.rs | support-module |
| tests/ghost_lint_fixtures/r4_guard/mutant_wrong_count.rs | support-module |
| tests/ghost_lint_fixtures/r4_guard/ok.rs | support-module |
| tests/ghost_lint_fixtures/r4_guard/rule.json | other-file |
| tests/ghost_lint_fixtures/r5_sole_path | directory-fixture |
| tests/ghost_lint_fixtures/r5_sole_path/mutant_remote_writes_depot.rs | support-module |
| tests/ghost_lint_fixtures/r5_sole_path/ok.rs | support-module |
| tests/ghost_lint_fixtures/r5_sole_path/rule.json | other-file |
| tests/ghost_lint_fixtures/r6_ghost_coverage | directory-fixture |
| tests/ghost_lint_fixtures/r6_ghost_coverage/mutant_check_removed.rs | support-module |
| tests/ghost_lint_fixtures/r6_ghost_coverage/mutant_other_id.rs | support-module |
| tests/ghost_lint_fixtures/r6_ghost_coverage/ok.rs | support-module |
| tests/ghost_lint_fixtures/r6_ghost_coverage/rule.json | other-file |
| tests/ghost_lint_fixtures/r7_twin | directory-fixture |
| tests/ghost_lint_fixtures/r7_twin/mutant_extra_op_in_prod.rs | support-module |
| tests/ghost_lint_fixtures/r7_twin/mutant_relaxed_only_in_loom.rs | support-module |
| tests/ghost_lint_fixtures/r7_twin/ok.rs | support-module |
| tests/ghost_lint_fixtures/r7_twin/rule.json | other-file |
| tests/ghost_lint_fixtures/r8_model_bind | directory-fixture |
| tests/ghost_lint_fixtures/r8_model_bind/mutant_model_drifted.rs | support-module |
| tests/ghost_lint_fixtures/r8_model_bind/mutant_tag_missing.rs | support-module |
| tests/ghost_lint_fixtures/r8_model_bind/ok.rs | support-module |
| tests/ghost_lint_fixtures/r8_model_bind/rule.json | other-file |
| tests/ghost_lint_fixtures/r9_hygiene | directory-fixture |
| tests/ghost_lint_fixtures/r9_hygiene/mutant_ghost_field.rs | support-module |
| tests/ghost_lint_fixtures/r9_hygiene/mutant_ghost_path_in_prod.rs | support-module |
| tests/ghost_lint_fixtures/r9_hygiene/mutant_write_in_check.rs | support-module |
| tests/ghost_lint_fixtures/r9_hygiene/ok.rs | support-module |
| tests/ghost_lint_fixtures/r9_hygiene/rule.json | other-file |
| tests/ghost_runtime_history_fixtures.mjs | mjs-script |
| tests/ghost_runtime_prefix_fixtures.mjs | mjs-script |
| tests/iai_nolto_summary.test.mjs | mjs-script |
| tests/j13_limited_report_fixtures.mjs | mjs-script |
| tests/j13b_artifacts.test.mjs | mjs-script |
| tests/j13b_build_seal_refusal.mjs | mjs-script |
| tests/j13b_native_facts_check.mjs | mjs-script |
| tests/j13b_request_refusal.mjs | mjs-script |
| tests/j13b_schema.test.mjs | mjs-script |
| tests/j13b_static_reference.test.mjs | mjs-script |
| tests/layering_lint.test.mjs | mjs-script |
| tests/module_graph.test.mjs | mjs-script |
| tests/r11_p3_small_sidecar_issue_oom | directory-fixture |
| tests/r11_p3_small_sidecar_issue_oom/support.rs | support-module |
| tests/r6_sidecar_bitmap | directory-fixture |
| tests/r6_sidecar_bitmap/child_process.rs | support-module |
| tests/regression_merged | directory-fixture |
| tests/regression_merged/regression_alloc_zeroed_fresh.rs | support-module |
| tests/regression_merged/regression_carve_batch.rs | support-module |
| tests/regression_merged/regression_dealloc_metadata_region_guard.rs | support-module |
| tests/regression_merged/regression_exact_256_class.rs | support-module |
| tests/regression_merged/regression_fastbin_aligned_roundtrip.rs | support-module |
| tests/regression_merged/regression_flush_class_unsafe_boundary.rs | support-module |
| tests/regression_merged/regression_hardened_interior_ptr.rs | support-module |
| tests/regression_merged/regression_inplace_large_realloc.rs | support-module |
| tests/regression_merged/regression_large_align_no_segment_exhaustion.rs | support-module |
| tests/regression_merged/regression_large_cache_multi_size_cycle.rs | support-module |
| tests/regression_merged/regression_large_cache_span_usable_stable.rs | support-module |
| tests/regression_merged/regression_magazine_oracles.rs | support-module |
| tests/regression_merged/regression_magazine_scan_bounds.rs | support-module |
| tests/regression_merged/regression_magic_at_atomic_load.rs | support-module |
| tests/regression_merged/regression_page_aligned_no_segment_exhaustion.rs | support-module |
| tests/regression_merged/regression_percounter_perheap_aggregation.rs | support-module |
| tests/regression_merged/regression_r2_05_diag_provenance_miri.rs | support-module |
| tests/regression_merged/regression_r2_05_diag_provenance.rs | support-module |
| tests/regression_merged/regression_r2_3_dbg_accessors_membership_guard.rs | support-module |
| tests/regression_merged/regression_r3a_stats_walk_compiled_out.rs | support-module |
| tests/regression_merged/regression_r4_2_null_base_foreign_guard.rs | support-module |
| tests/regression_merged/regression_r4_2_refill_return_clamp.rs | support-module |
| tests/regression_merged/regression_r6_t22_foreign_window_key_miri.rs | support-module |
| tests/regression_merged/regression_realloc_cross_class_shrink.rs | support-module |
| tests/regression_merged/regression_realloc_foreign_pointer_null.rs | support-module |
| tests/regression_merged/regression_realloc_heap_oob.rs | support-module |
| tests/regression_merged/regression_realloc_inplace_global.rs | support-module |
| tests/regression_merged/regression_realloc_oob_old_layout.rs | support-module |
| tests/regression_merged/regression_resolver_branch_collapse.rs | support-module |
| tests/regression_merged/regression_stamp_metadata_unsafe_boundary.rs | support-module |
| tests/regression_merged/regression_virgin_bitmap_skip.rs | support-module |
| tests/regression_merged/regression_w3_stats_aliasing_miri.rs | support-module |
| tests/support | directory-fixture |
| tests/support/alloc_core_differential_native_seeds.txt | other-file |
| tests/support/j13b_cadence_math.rs | support-module |
| tests/support/j13b_learning_math.rs | support-module |
| tests/support/j13b_peak_epoch_witness.rs | support-module |
| tests/support/pin_cache_bases.rs | support-module |
| tests/support/r11_box_cap_gate.rs | support-module |
| tests/support/r11_box_cap_reissue.rs | support-module |
| tests/support/r6_bounded_large_cache.rs | support-module |
| tests/support/r8_global_box_witness.rs | support-module |
| tests/support/r8_native_os_mapping.rs | support-module |
| tests/support/tagged_index_stack_compile_fail.rs | support-module |
| tests/test_inventory.test.mjs | mjs-script |
| tests/test_selection_impact.test.mjs | mjs-script |
| tests/verify_miri_target_wiring_quoting.test.mjs | mjs-script |

## Groups (top 10 of 2 by saved links; total saved links 0)

| group key | members | saved links | member names |
| --- | --- | --- | --- |
| j13b\|-\|- | 1 | 0 | j13b_observation_residency_plan |
| registry\|-\|- | 1 | 0 | registry_fallback_hook |

Full member lists: see the `--csv` output (column `group`).

## Pilot candidate

Group key: `j13b|-|-`

Members:

- `j13b_observation_residency_plan`

Saved links: **0**.

By construction of the strict rule, no member is name-pinned externally. But ANY merge of targets invalidates the repo-level published COUNT claims below, which would need updating in the same change:

| file | line | text | numbers |
| --- | --- | --- | --- |
| docs/ARCHITECTURE.md | 509 | Flat root-level Rust inventories: **510 integration test files** (`tests/*.rs`), | 510 |
| docs/ARCHITECTURE.md | 518 | \| Unit / integration tests \| Construction, edge cases, invariants; snapshot acceptance pending \| `tests/*.rs` (510 files) \| | 510 |
| docs/ARCHITECTURE.md | 521 | \| loom \| Bounded interleavings; snapshot execution pending \| **Root (27 files):** `tests/loom_active_kind_index.rs`, `tests/loom_depot_recycle_handoff.rs`, `tests/loom_b4_r1_pin_cache.rs`, `tests/loom_b4_scan_skip_empty_publish.rs`, `tests/loom_exact_shard_actual.rs`, `tests/loom_fallback_actual.rs`, `tests/loom_g1_ingress_exact.rs`, `tests/loom_r8_maintenance_lease.rs`, `tests/loom_r11_ph4a_heap_lease.rs`, `tests/loom_r11_registry_claim.rs`, `tests/loom_r6_selector_model.rs`, `tests/loom_registry_free_slots.rs`, `tests/loom_registry_hints_actual.rs`, `tests/loom_r19_registry_free_set.rs`, `tests/loom_r20_heap_reuse_spill.rs`, `tests/loom_route_pin_unlink.rs`, `tests/loom_r11_ph4b_publish_recycle_drain.rs`, `tests/loom_sidecar_bitmap_actual.rs`, `tests/loom_sidecar_summary.rs`, `tests/loom_terminal_large.rs`, `tests/loom_terminal_owner_drain.rs`, `tests/loom_thp_ledger.rs`, `tests/loom_thp_learning_cell.rs`, `tests/loom_thp_cadence_cell.rs`, `tests/loom_thp_tuple_cell.rs`, `tests/loom_nl3_remote_reclaim.rs`, `tests/loom_nl3_owner_teardown.rs`; **member suites:** `crates/primitives/once-ptr-cell/tests/loom_once_ptr_cell.rs`, `crates/primitives/tagged-index-stack/tests/loom_aba.rs`, `crates/containers/sefer-concurrent/tests/loom_epoch.rs`, `crates/containers/sefer-concurrent/tests/loom_sharded.rs`, `crates/containers/sefer-concurrent/tests/loom_r11_epoch_false_full.rs` \| | 27 |
| docs/CI_TIERS.md | 180 | (a change of the segment-directory scan selects 369 of 484 tests), so the saving comes from peripheral and test-only | 369/484 |
| docs/perf/b4_ablate/G1_K7_A6_VERDICT.md | 27 | (1606 files). Campaign driver output: `a6/campaign.log`. | 1606 |
| docs/perf/G1_DECISION_CRITERION.md | 81 | \| Selected-test count for a representative peripheral change \| tests \| after P1.5 strictly fewer than the current all-target selection \| current data point: a segment-directory scan change selects 369 of 484 tests (`docs/CI_TIERS.md`, "Test selection" section; first map run: 374 of 484 mapped under `--all-features`) \| | 369/484 |
| docs/perf/R14_6_ADAPTIVE_RESERVED_CAPACITY_GATE.md | 352 | - `docs/perf/_raw_r14_6_*.log` (14 files): iai baseline/treatment, R12-4 | 14 |
| docs/plans/2026-11-15-workspace-restructure-plan.md | 37 | a segment-directory scan change selects 369 of 484 tests) because one crate holds all of the | 369/484 |
| docs/reviews/2026-07-09-maintainability-architecture-review.md | 235 | (103 files)», фактически 117. | 103 |
| docs/reviews/2026-07-17-deep-audit/08-test-verification-coverage.md | 3 | Scope: `tests/` (169 integration test files), `crates/*/tests/`, verification | 169 |
| docs/reviews/2026-07-26-r19-r21-readonly-review.md | 346 | То же касается ручного `tests/*.rs (221 files)` в ARCHITECTURE: точное число | 221 |
| docs/reviews/2026-08-06-size-classes-publish-readiness-review.md | 210 | `cargo package -p size-classes --list` (10 files): | 10 |
| docs/reviews/2026-08-06-tagged-index-stack-publish-readiness-review.md | 315 | ### F4 — 12 of 16 tests are CI-dead | 12/16 |
| docs/reviews/2026-08-11-sefer-region-code-quality-review.md | 5 | `tests/` (11 files), `benches/` (5 harnesses), `examples/contended_reads.rs`, `Cargo.toml`, | 11 |
| docs/reviews/2026-08-25-093114-numa-shim-publication-audit-oh.md | 33 | - Full crate re-read from scratch with no assumption that prior remediation worked: `src/lib.rs` (all 2326 lines, in three passes), all 10 integration test files, `benches/numa_bench.rs`, `Cargo.toml`, `README.md`, `CHANGELOG.md` (all 367 lines), and every `numa-shim-*` job in `.github/workflows/ci.yml`. | 10 |
| docs/reviews/2026-10-01-refactor-ph1b-exact-object-proto-receipt.md | 90 | - `tests/no_stale_doc_references.rs` (production internals): 30 passed, 3 failed — все три счётчика документов, которые я по инструкции не правил: `architecture_test_file_count_matches_reality` (321 файл тестов, нужен токен `(321 files`), `verification_inventory_matches_docs` (README: `**321 integration test files**`), `readme_unsafe_inventory_counts_match_reality` (README: tier-1 теперь 28, токен `**28** tier-1 module-level seams`). Остальное (включая `lib_rs_seam_inventory_matches_canonical_grep` и `cargo_toml_alloc_global_panic_contract_is_accurate`) зелёное после добавления bullet'ов в `src/lib.rs` и размещения feature после `alloc-global`. | 321 |
| docs/reviews/2026-10-05-ph4a-heap-lease-receipt.md | 89 | - README.md:1373 `**330 integration test files**` → `**331 integration test files**` | 330/331 |
| docs/reviews/2026-10-05-ph4a-heap-lease-receipt.md | 90 | - README.md:1383 `tests/*.rs (330 files)` → `tests/*.rs (331 files)` | 330/331 |
| docs/reviews/2026-10-05-ph4a-heap-lease-receipt.md | 91 | - docs/ARCHITECTURE.md:489 `tests/*.rs (330 files)` → `tests/*.rs (331 files)` | 330/331 |
| docs/reviews/2026-10-08-src-review-oxx-round-17-code-quality.md | 487 | doc(hidden)  : grep -rnE '#\[doc\(hidden\)\]' src/ \| wc -l                     -> 455 (67 files) | 67 |
| README.md | 1626 | needs its acceptance run. The present tree contains **510 integration test files**, | 510 |
| README.md | 1636 | \| Unit / integration tests \| Construction, edge cases, end-to-end behaviour \| `tests/*.rs` (510 files) \| | 510 |
| README.md | 1637 | \| Examples \| Executable soak, burn-in, RSS, and macro verification harnesses \| `examples/*.rs` (107 files) \| | 107 |
| README.md | 1638 | \| Benches \| Reproducible performance and gate harnesses \| `benches/*.rs` (34 files) \| | 34 |
| README.md | 1640 | \| `loom` \| Bounded protocol interleavings; see `scripts/loom.mjs` for selected configurations \| **Root (27 files):** `tests/loom_active_kind_index.rs`, `tests/loom_depot_recycle_handoff.rs`, `tests/loom_b4_r1_pin_cache.rs`, `tests/loom_b4_scan_skip_empty_publish.rs`, `tests/loom_exact_shard_actual.rs`, `tests/loom_fallback_actual.rs`, `tests/loom_g1_ingress_exact.rs`, `tests/loom_r8_maintenance_lease.rs`, `tests/loom_r11_ph4a_heap_lease.rs`, `tests/loom_r11_registry_claim.rs`, `tests/loom_r6_selector_model.rs`, `tests/loom_registry_free_slots.rs`, `tests/loom_registry_hints_actual.rs`, `tests/loom_r19_registry_free_set.rs`, `tests/loom_r20_heap_reuse_spill.rs`, `tests/loom_route_pin_unlink.rs`, `tests/loom_r11_ph4b_publish_recycle_drain.rs`, `tests/loom_sidecar_bitmap_actual.rs`, `tests/loom_sidecar_summary.rs`, `tests/loom_terminal_large.rs`, `tests/loom_terminal_owner_drain.rs`, `tests/loom_thp_ledger.rs`, `tests/loom_thp_learning_cell.rs`, `tests/loom_thp_cadence_cell.rs`, `tests/loom_thp_tuple_cell.rs`, `tests/loom_nl3_remote_reclaim.rs`, `tests/loom_nl3_owner_teardown.rs`; **member suites:** `crates/primitives/once-ptr-cell/tests/loom_once_ptr_cell.rs`, `crates/primitives/tagged-index-stack/tests/loom_aba.rs`, `crates/containers/sefer-concurrent/tests/loom_epoch.rs`, `crates/containers/sefer-concurrent/tests/loom_sharded.rs`, `crates/containers/sefer-concurrent/tests/loom_r11_epoch_false_full.rs` \| | 27 |

j13 hash pins: scripts/j13_limited_report.mjs, scripts/j13_report.mjs, scripts/j13b_artifacts.mjs, scripts/j13b_static_reference.mjs, scripts/j13b_zero_cost_receipt.mjs hash src/ files; tests/ files hash-pinned: **none (expected)**.

### File-hash pin inventory (derived from hash-table shapes in j13 scripts)

| path | script | zone |
| --- | --- | --- |
| crates/core/sefer-policy/src/policy/large_thp_runtime/thp_runtime.rs | scripts/j13b_static_reference.mjs | src/other |
| src/alloc_core/alloc_core/mem/advise_fresh_large.rs | scripts/j13b_static_reference.mjs | src/other |

## Relaxation ladder

| rung | description | pilot-eligible | mergeable files | groups | total saved links |
| --- | --- | --- | --- | --- | --- |
| strict | strict rules (see report definitions) | no | 2 | 2 | 0 |
| compat | uniform required-features + uniform file-level cfg in the group key; hard blockers apply; externally-pinned excluded | yes | 30 | 26 | 4 |
| looser-features | additionally allow non-uniform required-features (still pinned-excluded) | no | 30 | 26 | 4 |
| looser-cfg | additionally allow non-uniform file-level cfg | no | 30 | 12 | 18 |
| allow-pinned | additionally allow externally-pinned names | no | 302 | 73 | 229 |

### Pilot eligibility per rung

Strict mergeable pool: **2** files in 2 groups (total saved links 0).

| rung | pilot-eligible | why | pilot key | pilot members | pilot saved links |
| --- | --- | --- | --- | --- | --- |
| strict | no | not eligible: strict rules block any file with required-features or a file-level cfg, so the pool cannot form uniform groups | j13b\|-\|- | 1 | 0 |
| compat | yes | pilot-eligible: uniform required-features and uniform file-level cfg in the group key, hard blockers apply, externally-pinned names excluded | r20\|-\|all(feature = "alloc-global", feature = "internals") | 3 | 2 |
| looser-features | no | not eligible unless group required-features become uniform: members may mix differing required-features | r20\|all(feature = "alloc-global", feature = "internals") | 3 | 2 |
| looser-cfg | no | not eligible unless group file-level cfg becomes uniform: members may mix differing file-level cfgs | r10 | 7 | 6 |
| allow-pinned | no | NOT pilot-eligible: members are externally name-pinned; listed for scale only | r6 | 42 | 41 |

`compat` rung pilot candidate (the pilot candidate of the classification):

Group key: `r20|-|all(feature = "alloc-global", feature = "internals")`; members: 3; saved links: **2**.

- `r20_free_slot_hint_layout`
- `r20_free_slot_hint_unit`
- `r20_heap_reuse_selector`

Pilot eligibility per rung is derived from each rung's relaxation rules; no go/no-go verdict is expressed.

## Grep reconciliation (naive baseline vs measured)

Naive baseline patterns (fixed, over RAW text — intentionally naive, these are the greps plan §1 used): global_allocator `/#\[global_allocator\]/`; first_config_wins `\b(with_config|config_conflicts|thp_tuple)\b`; process_spawn `(process::Command|current_exe|CARGO_BIN_EXE_)`; loom `\bloom\b`; miri `\bmiri\b`; file_level_cfg lines matching `^\s*#!\[cfg`. Baseline-only reason codes: comment-only, string-literal, item-level-attr-only (matched code, but not before the first item), marker-list (matched word is not a derived marker). Measured-only reason codes: multi-line-attr, cfg_attr-form, bare-Command-import (true positive via `use std::process::{Command, ...}` + bare `Command::new`, outside the naive pattern).

### global_allocator — naive 76 vs measured 63

Pattern: `/\#\[global_allocator\]/ on raw text`

In baseline but not measured (13):

| path | reason |
| --- | --- |
| tests/asan_alloc_core.rs | comment-only |
| tests/dealloc_only_no_bind_torn.rs | comment-only |
| tests/dealloc_only_no_bind.rs | comment-only |
| tests/global_alloc.rs | comment-only |
| tests/no_panic_doc_accuracy.rs | comment-only |
| tests/oxx_r2_03_dealloc_batch_current_for_dealloc.rs | comment-only |
| tests/r11_ph3b_kind_aware_entries.rs | comment-only |
| tests/r25_4_dealloc_batch_multi_flush_oracle.rs | comment-only |
| tests/r6_t01_owner_stamp_removed_routing.rs | comment-only |
| tests/regression_r2_08_globalalloc_no_unwind.rs | comment-only |
| tests/stats_reflects_activity.rs | comment-only |
| tests/stress_concurrent_boundaries.rs | comment-only |
| tests/tls_heap_teardown_torn_sentinel.rs | comment-only |

Measured but not in baseline (0):

(none)

### first_config_wins — naive 24 vs measured 79

Pattern: `\b(with_config|config_conflicts|thp_tuple)\b on raw text`

In baseline but not measured (5):

| path | reason |
| --- | --- |
| tests/j13b_advice_capture_native.rs | marker-list |
| tests/loom_thp_ledger.rs | string-literal |
| tests/loom_thp_tuple_cell.rs | comment-only |
| tests/r14_4_promotion_free_correctness.rs | comment-only |
| tests/sefer_alloc_examples.rs | comment-only |

Measured but not in baseline (60):

| path | reason |
| --- | --- |
| tests/alloc_core_batch.rs | marker-list |
| tests/b4_class_cursor_invalidation.rs | marker-list |
| tests/decommit_miri_cycle.rs | marker-list |
| tests/decommit_soak.rs | marker-list |
| tests/g1_k7_depot_overflow_config.rs | marker-list |
| tests/j13b_cadence_native.rs | marker-list |
| tests/j13b_control_cache_reuse.rs | marker-list |
| tests/j13b_control_concurrent_ordinal.rs | marker-list |
| tests/j13b_control_dense.rs | marker-list |
| tests/j13b_control_installed_global.rs | marker-list |
| tests/j13b_control_invalidation.rs | marker-list |
| tests/j13b_control_moving_realloc.rs | marker-list |
| tests/j13b_control_oom_shrink_invalidation.rs | marker-list |
| tests/j13b_control_premise_fresh.rs | marker-list |
| tests/j13b_control_premise_recovery.rs | marker-list |
| tests/j13b_control_remote_unknown.rs | marker-list |
| tests/j13b_control_reserved_capacity_invalidation.rs | marker-list |
| tests/j13b_control_sampler_recovery.rs | marker-list |
| tests/j13b_control_sparse.rs | marker-list |
| tests/j13b_control_ticket_log_capture_bounds.rs | marker-list |
| tests/j13b_learning_runtime.rs | marker-list |
| tests/j13b_owned_block_installed.rs | marker-list |
| tests/j13b_static_reference.rs | marker-list |
| tests/j13b_thp_always_unlimited.rs | marker-list |
| tests/j13b_thp_auto_seam.rs | marker-list |
| tests/j13b_thp_budget_conservation.rs | marker-list |
| tests/j13b_thp_default_off_identity.rs | marker-list |
| tests/j13b_thp_header_charge_carry.rs | marker-list |
| tests/j13b_thp_injected_pre_syscall.rs | marker-list |
| tests/j13b_thp_min_zero.rs | marker-list |
| tests/j13b_thp_numa_off.rs | marker-list |
| tests/j13b_thp_off_explicit.rs | marker-list |
| tests/j13b_thp_publish_concurrent.rs | marker-list |
| tests/j13b_thp_publish_first_wins.rs | marker-list |
| tests/j13b_thp_publish_same_tuple.rs | marker-list |
| tests/j13b_thp_realloc_charge.rs | marker-list |
| tests/j13b_thp_registration_failure.rs | marker-list |
| tests/j13b_thp_release_failure_retains_charge.rs | marker-list |
| tests/j13b_thp_remote_free_reclaim.rs | marker-list |
| tests/j13b_thp_tail_trim_budget.rs | marker-list |
| tests/j13b_thp_tail_trim_extent.rs | marker-list |
| tests/j13b_thp_unpublished_default.rs | marker-list |
| tests/large_cache_budget.rs | marker-list |
| tests/large_cache_config_knobs.rs | marker-list |
| tests/large_cache_decay.rs | marker-list |
| tests/large_cache_extended_budget_before_materialization.rs | marker-list |
| tests/large_cache_mode.rs | marker-list |
| tests/profile.rs | marker-list |
| tests/r10_active_kind_small_recycle.rs | marker-list |
| tests/r8_6_decommit_boundary.rs | marker-list |
| tests/regression_batch_flush.rs | marker-list |
| tests/regression_batch_freelist_drain.rs | marker-list |
| tests/regression_bump_direct_refill.rs | marker-list |
| tests/regression_own_segment_cache_invalidation.rs | marker-list |
| tests/regression_own_thread_large_no_leak.rs | marker-list |
| tests/regression_t10_find_segment_multiseg_recovery.rs | marker-list |
| tests/segment_directory_a2.rs | marker-list |
| tests/segment_directory_a5.rs | marker-list |
| tests/segment_table_recycle.rs | marker-list |
| tests/small_segment_pool.rs | marker-list |

### process_spawn — naive 32 vs measured 33

Pattern: `(process::Command|current_exe|CARGO_BIN_EXE_) on raw text`

In baseline but not measured (0):

(none)

Measured but not in baseline (1):

| path | reason |
| --- | --- |
| tests/tagged_index_stack_ab_runner_scratch_guard.rs | bare-Command-import |

### loom_tree — naive 45 vs measured 32

Pattern: `\bloom\b on raw text`

In baseline but not measured (13):

| path | reason |
| --- | --- |
| tests/b4_scan_rmw_zero_word_skips_swap.rs | comment-only |
| tests/b4_scan_zero_word_skip_keeps_bits.rs | comment-only |
| tests/class_leaves_publication_miri.rs | comment-only |
| tests/global_alloc_installed.rs | comment-only |
| tests/no_stale_doc_references.rs | string-literal |
| tests/r11_ph4b_lease_miri.rs | comment-only |
| tests/r6_sidecar_bitmap.rs | comment-only |
| tests/r6_t26_cas_spin_ttas_stress.rs | comment-only |
| tests/registry_basic.rs | comment-only |
| tests/regression_r2_01_segment_bases_lifetime.rs | comment-only |
| tests/regression_r6_t38_public_tail_private.rs | comment-only |
| tests/sidecar_granule_index_exhaustive.rs | comment-only |
| tests/unsafe_allow_placement.rs | comment-only |

Measured but not in baseline (0):

(none)

### miri_tree — naive 119 vs measured 50

Pattern: `\bmiri\b on raw text`

In baseline but not measured (69):

| path | reason |
| --- | --- |
| tests/alloc_core_differential.rs | item-level-attr-only |
| tests/alloc_core_invariants.rs | comment-only |
| tests/alloc_zeroed_fresh_large_skip.rs | item-level-attr-only |
| tests/alloc_zeroed_virgin_small_skip.rs | comment-only |
| tests/b4_r1_pin_cache_miri.rs | comment-only |
| tests/b4_scan_rmw_zero_word_skips_swap.rs | comment-only |
| tests/b4_scan_zero_word_skip_keeps_bits.rs | comment-only |
| tests/class_leaves_publication_miri.rs | comment-only |
| tests/decommit_miri_cycle.rs | comment-only |
| tests/decommit_soak.rs | comment-only |
| tests/decomp_hooks_forced_page.rs | item-level-attr-only |
| tests/heap_core_tcache_decommit.rs | item-level-attr-only |
| tests/heap_invariants.rs | comment-only |
| tests/heap_soak.rs | comment-only |
| tests/j13b_adaptive_windows_native.rs | item-level-attr-only |
| tests/j13b_advice_capture_native.rs | item-level-attr-only |
| tests/j13b_advice_windows_native.rs | item-level-attr-only |
| tests/j13b_native_identity.rs | item-level-attr-only |
| tests/j13b_os_probe.rs | item-level-attr-only |
| tests/j13b_owner_training_native.rs | item-level-attr-only |
| tests/large_cache.rs | comment-only |
| tests/lazy_initial_commit_forced_page.rs | comment-only |
| tests/loom_b4_scan_skip_empty_publish.rs | comment-only |
| tests/loom_sidecar_bitmap_actual.rs | comment-only |
| tests/miri_global_box_acceptance.rs | item-level-attr-only |
| tests/miri_r1_01_cap_frame.rs | comment-only |
| tests/nl3_safe_xthread_churn_integrity.rs | comment-only |
| tests/nl7_safe_large_realloc_integrity.rs | comment-only |
| tests/no_stale_doc_references.rs | comment-only |
| tests/numa_seam.rs | comment-only |
| tests/numa_segment_id.rs | comment-only |
| tests/oxx_r2_02_realloc_lazy_commit_frontier.rs | comment-only |
| tests/r11_box_cap_gate_w2.rs | item-level-attr-only |
| tests/r11_box_cap_gate_w3.rs | item-level-attr-only |
| tests/r11_exact_object_proto.rs | item-level-attr-only |
| tests/r11_p3_small_sidecar_routes.rs | item-level-attr-only |
| tests/r11_p4_1_route_shard_model.rs | item-level-attr-only |
| tests/r12_1_directory_scan_no_aliasing.rs | comment-only |
| tests/r13_3_magazine_virgin_hit_skips_zero.rs | item-level-attr-only |
| tests/r13_shard_guard_auto_traits.rs | item-level-attr-only |
| tests/r14_7_max_segments_ceiling.rs | comment-only |
| tests/r14_sidecar_owner_capability_negative.rs | item-level-attr-only |
| tests/r6_b4_sidecar_registration_zero_state.rs | item-level-attr-only |
| tests/r6_route_directory_model.rs | item-level-attr-only |
| tests/r6_sidecar_bitmap.rs | comment-only |
| tests/r8_6_decommit_boundary.rs | comment-only |
| tests/r8_global_box_provenance.rs | item-level-attr-only |
| tests/r8_large_alignment.rs | item-level-attr-only |
| tests/r8_terminal_global.rs | item-level-attr-only |
| tests/r9_diagnostic_reader_kinds.rs | item-level-attr-only |
| tests/region_invariants.rs | comment-only |
| tests/regression_batch_freelist_drain.rs | item-level-attr-only |
| tests/regression_bump_direct_refill.rs | comment-only |
| tests/regression_c3_unbounded_recycle.rs | item-level-attr-only |
| tests/regression_r2_15_virgin_mask_zeroed_and_bounded.rs | item-level-attr-only |
| tests/regression_registry_initialised_gate.rs | comment-only |
| tests/regression_segment_table_tombstone_rebuild.rs | comment-only |
| tests/regression_xthread_small_ring_miri.rs | comment-only |
| tests/regression_xthread_thread_free_alias_miri.rs | comment-only |
| tests/route_pin_last_ref_miri.rs | comment-only |
| tests/segment_directory_a5_miri.rs | comment-only |
| tests/segment_state_reconciliation_dual_enumeration.rs | comment-only |
| tests/segment_state_reconciliation_oracle.rs | item-level-attr-only |
| tests/segment_table_hash.rs | item-level-attr-only |
| tests/segment_table_o1.rs | item-level-attr-only |
| tests/segment_table_recycle.rs | comment-only |
| tests/sidecar_granule_index_exhaustive.rs | comment-only |
| tests/small_segment_pool.rs | item-level-attr-only |
| tests/stress_boundary_sweep.rs | item-level-attr-only |

Measured but not in baseline (0):

(none)

### file_level_cfg — naive 461 vs measured 461

Pattern: `lines matching ^\s*#!\[cfg on raw text (line-anchored)`

In baseline but not measured (0):

(none)

Measured but not in baseline (0):

(none)

## How to reproduce

```sh
node scripts/classify-tests.mjs --json
node scripts/classify-tests.mjs --csv
node scripts/classify-tests.mjs --report
```

The script writes nothing itself; all output goes to stdout. Deterministic: identical runs on an identical tree produce byte-identical output. Requires Node >= 24, zero npm dependencies, no cargo, no network.
