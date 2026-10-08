# R16 item 84 — magazine flush-root mask gate

**GO — `perf(runtime)` candidate passes the preregistered deterministic gate and accepted native correctness checks.**
No wall-clock or RSS claim. No feature/configuration change, new API/hook or unsafe surface.
Source: perf OPEN_ITEMS item84; round-16 oxx review §4 H2; R12-02 issue-side precedent.

## 1. Change and entry point

Only the root-clear loops in `src/registry/heap_core/free/dealloc_own_base.rs` and `src/registry/heap_core/state/tcache_flush.rs` replace `canonical_block_of(...).abort` with provenance-preserving `crate::alloc_core::os::segment_base_of_ptr(flushed)`.
Debug builds assert equality with `canonical_root_for`. All validation, offsets, bitmap operations/order, counts, compaction, virgin masks and substrate flush stay unchanged.
Small/Primordial roots are SEGMENT-aligned; magazine blocks are allocator-issued; biased Large roots are excluded. The helper uses `map_addr`, not pointer/integer reconstruction.

Overflow bench entry: `HeapCore::dealloc` → owner routing → `dealloc_own_thread_with_base`. Flush-all entry: `HeapCore::dbg_flush_all` → production `flush_all_tcache`, used by trim/recycle.
Standalone `dbg_flush_class_only` is an unchanged substrate control, not the modified loop.
This is HeapCore evidence, not a real-global-allocator throughput claim.

## 2. Immutable identities captured before measurements

| field | value |
|---|---|
| base HEAD | `265a571abc67323792334dc4dc5452fc59328d19` |
| HEAD tree | `75fcf7356bb177cf77a75c9ec293148281eff737` |
| A/C0 sorted input manifest SHA-256 | `3239a60a7630f65cd81f681e1098ad39f11cdd1318b910937e6f280ed6bd590b` |
| B sorted input manifest SHA-256 | `f065f5d4405cfe685b47e513c673f316fc8feb6f0e59dcc69d684cec8bc85789` |
| exact `git diff --ignore-cr-at-eol -- src` SHA-256 | `6b8a75790e0b61c39977d87154eb83831d07111e278db6558e3528538bc4381f` |
| common benchmark diff SHA-256 | `c2950fee308ca87574926b750ae9c8c033dc82884979245273342849da3af0c7` |
| benchmark file SHA-256 | `1bff70774aac20a85a2f77b083618c948905898cd531078ea8620f77620fe1a2` |
| generated iai harness SHA-256 | `b218264ec5acb1592bacd1527c4615300bb179705d55ab411ec07df6fea4504e` |

A src is pinned HEAD, clean before candidate; C0 is identical source at another nested path.
B differs only in the two owned src files. Bench pair is identical in all arms. Exact premeasurement manifests: `target/r16-perf84/{A,C0,B}.identity.json`; patches: `target/r16-perf84/{bench,candidate}.patch`. All frozen bytes reverified after runs.
Durable identity sidecar records manifest-file hashes and five raw/full sanitized-log hashes;
full-log hashes were independently checked against actual saved target logs before finalization.

Compiler rustc 1.98.1 (`48a229ceaefd4985c50990b14116b6d856af0985`, LLVM22.1.8), target x86_64-unknown-linux-gnu; features `production bench-internals internals`; Valgrind3.22.0/runner0.14.2.
CPU Intel i7-11800H; Linux6.18.33.2 WSL2. A1/A2 and B1/B2: two runs each; C0: one. Native debug checks used rustc1.97.0.

## 3. Preregistered gate (unchanged after results)

T=max(10 Ir, max absolute C0−A1 Ir across all fifteen rows); repeat-only A1/A2 fallback
was allowed only if C0 unavailable (not used). All six numeric columns must repeat exactly.
Raw savings n17/n32/flush-all must exceed T; flush-all paired saving must exceed 2T.
Dose residual abs(saving32−2saving17) must not exceed max(10 Ir,(5/100)*2saving17).
That proportional tolerance is 5%: residual numerator / twice-n17-saving denominator.
Negative-control absolute Ir changes ≤T; paired control changes ≤2T; churn cannot regress >T.
Any correctness/activation/identity failure blocks GO; no cherry-picking or averaging.
Cache columns are secondary model signals; adverse movements and same-source envelope disclosed.

## 4. Checked results

<!-- r16_perf84:tables:begin -->
| bench | A Ir | B Ir | B−A Ir | B/A Ir (num/den) | C0−A Ir | A EstCycles | B EstCycles | B/A EstCycles (num/den) |
|---|---:|---:|---:|---|---:|---:|---:|---|
| dealloc_free_only_16b_n16 | 64603 | 64603 | 0 | 1.000000 (64603/64603) | 0 | 152261 | 152265 | 1.000026 (152265/152261) |
| dealloc_free_only_16b_n17 | 65416 | 65270 | -146 | 0.997768 (65270/65416) | 0 | 154127 | 153979 | 0.999040 (153979/154127) |
| dealloc_free_only_16b_n32 | 67205 | 66913 | -292 | 0.995655 (66913/67205) | 0 | 156513 | 156145 | 0.997649 (156145/156513) |
| dealloc_prealloc_only_16b | 63472 | 63472 | 0 | 1.000000 (63472/63472) | 0 | 150447 | 150451 | 1.000027 (150451/150447) |
| dealloc_free_only_16b_n8 | 64043 | 64043 | 0 | 1.000000 (64043/64043) | 0 | 151567 | 151571 | 1.000026 (151571/151567) |
| dealloc_free_only_16b_n9 | 64113 | 64113 | 0 | 1.000000 (64113/64113) | 0 | 151658 | 151628 | 0.999802 (151628/151658) |
| dealloc_flush_class_only_16b_prefix | 51770 | 51770 | 0 | 1.000000 (51770/51770) | 12 | 135625 | 135629 | 1.000029 (135629/135625) |
| dealloc_flush_class_only_16b | 52276 | 52276 | 0 | 1.000000 (52276/52276) | 0 | 136892 | 136964 | 1.000526 (136964/136892) |
| alloc_clear_magazine_only_16b_prefix | 52393 | 52393 | 0 | 1.000000 (52393/52393) | 0 | 135869 | 135907 | 1.000280 (135907/135869) |
| alloc_clear_magazine_only_16b | 52576 | 52576 | 0 | 1.000000 (52576/52576) | 0 | 136459 | 136497 | 1.000278 (136497/136459) |
| alloc_magazine_hit_only_16b | 57207 | 57207 | 0 | 1.000000 (57207/57207) | 0 | 142034 | 142038 | 1.000028 (142038/142034) |
| small_churn_16b | 57518 | 57518 | 0 | 1.000000 (57518/57518) | 12 | 143411 | 143325 | 0.999400 (143325/143411) |
| small_churn_16b_2n | 63662 | 63662 | 0 | 1.000000 (63662/63662) | 12 | 151249 | 151197 | 0.999656 (151197/151249) |
| dealloc_flush_all_tcache_16b_prefix | 52391 | 52391 | 0 | 1.000000 (52391/52391) | 12 | 135871 | 135875 | 1.000029 (135875/135871) |
| dealloc_flush_all_tcache_16b | 54209 | 53949 | -260 | 0.995204 (53949/54209) | 0 | 138943 | 138556 | 0.997215 (138556/138943) |

Repeat equality: A1=A2 and B1=B2 for Ir/L1/L2/RAM/Total read+write/Estimated Cycles. C0 max absolute Ir delta=12; T=max(10; 12)=12 Ir.
Raw savings: n17=146 Ir; n32=292 Ir; flush-all=260 Ir. Flush-all paired saving=260 Ir; required >24 Ir.
Dose residual=0 Ir; tolerance=max(10; (5/100)*2*146)=14.600 Ir. Dose residual fraction=0.000000 (0/292).
Computed gate: GO. Failures: none.

Cache-model signal: max absolute C0 Estimated Cycles delta=114; adverse B control deltas: dealloc_free_only_16b_n16 +4; dealloc_prealloc_only_16b +4; dealloc_free_only_16b_n8 +4; dealloc_flush_class_only_16b_prefix +4; dealloc_flush_class_only_16b +72; alloc_clear_magazine_only_16b_prefix +38; alloc_clear_magazine_only_16b +38; alloc_magazine_hit_only_16b +4; dealloc_flush_all_tcache_16b_prefix +4. Secondary model only; no wall-clock/RSS claim.
<!-- r16_perf84:tables:end -->

The new sixteen-block flush-all/prefix pair asserts non-null pointers, empty magazine after
allocation and count16 after free; only its partner calls flush-all and asserts count0.
Existing overflow fixtures remain unchanged. Independent activation receipt reproduces their
64-allocation setup and counts 0/1/2 overflow transitions at 16/17/32 frees in A/C0/B.
Churn is Ir-neutral: this workload does not expose the overflow benefit; no general speedup claim.

## 5. Correctness and counterfactual receipts

Worker native debug `production internals bench-internals alloc-stats`: seven targets20 passed: heap_core_bulk_bypass(4), heap_core_tcache_decommit(2), r16_perf84_activation(1), r31_10_trim_current_thread_api(6), regression_flush_class_unsafe_boundary(2), regression_magazine_oracles(4), regression_r4_3_teardown_trim(1).
Frozen Linux B activation passed1; restored native activation passed1.
Parent independently reread diff/reran focused20 and broad34 targets: all executed binaries passed. Three targets ran0 feature-gated tests (r14_4_promotion_free_correctness/r25_4_dealloc_batch_multi_flush_oracle/r26_5_dealloc_batch_per_block_partition); no coverage claimed for them.
Parent exact-object-proto narrow passed exit0, no_panic_doc_accuracy4 passed, production-lib clippy/rustfmt/diffcheck exit0. Initial exact-object-proto inclusion refused Cargo101 before execution; a subsequent harness=false invocation with --test-threads=1 failed usage exit2, not an assertion; corrected narrow command passed. Worker's interrupted extra command is not counted.
Exact parent commands (same root-local target, no new execution in finalization):
```text
RUSTC_WRAPPER= CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/target" cargo test --locked --features "production internals alloc-stats bench-internals" --test r16_perf84_activation --test regression_magazine_oracles --test regression_flush_class_unsafe_boundary --test heap_core_tcache_decommit --test heap_core_bulk_bypass --test regression_r4_3_teardown_trim --test r31_10_trim_current_thread_api --test stats_foreign_or_unroutable_frees --test regression_realloc_heap_oob --test regression_free_path_chunk_oom_graceful --test r8_terminal_global --test r7_cold_small_trim_directory --test r7_cold_small_trim_current --test r6_terminal_owner_drain --test r17_8_deterministic_trim_releases_cached_large_span --test r14_4_promotion_free_correctness --test r12_01_foreign_free_of_unissued_granule --test r11_ph5c_trim_idempotent_after_join --test r11_ph4c_ingress_consume_exactly_once_oracle --test r11_ph4b_no_worker_pending_stays --test r11_ph4b_late_publication_across_recycle_exactly_once --test oxx_r2_01_orphaned_cursor_finalized --test stress_concurrent_boundaries --test stress_boundary_sweep --test r11_ph3b_kind_aware_entries --test r26_5_dealloc_batch_per_block_partition --test r25_4_dealloc_batch_multi_flush_oracle --test r6_class_issue --test r6_route_lifecycle_decommit --test r6_route_lifecycle_pool --test r12_02_magazine_hit_mask_equals_canonical_root --test regression_r1_01_magazine_free_budget --test regression_magazine_scan_bounds --test regression_magazine_bump_guard -- --test-threads=1
RUSTC_WRAPPER= CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/target" cargo test --locked --features "production internals alloc-stats bench-internals exact-object-proto" --test r11_exact_object_proto -- narrow
RUSTC_WRAPPER= CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/target" cargo test --locked --features "production internals alloc-stats bench-internals batch-api" --test no_panic_doc_accuracy
RUSTC_WRAPPER= CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/target" cargo clippy --locked --features production --lib -- -D warnings
rustfmt --edition 2021 --check src/registry/heap_core/free/dealloc_own_base.rs src/registry/heap_core/state/tcache_flush.rs benches/perf_gate_iai.rs tests/r16_perf84_activation.rs
git diff --check
```

Separate native debug mutants: overflow mask+16 and flush-all mask+16 each rejected at their own debug_assert; no-op flush-all rejected count16 versus0.
Actual Windows status0xc0000409 / shell exit9, not normal Cargo101; assertion diagnostics preceded termination. cp backups/restoration used; no git restoration/.bak residue; restored source matches frozen B. Debug mutants prove assertion/activation nonvacuity, not release safety.
Initial crate::os spelling failed E0433; corrected to existing alloc_core::os before final B freeze.
Miri/Loom/Kani, full suite, release/virgin-specific correctness and dedicated new-loop multi-segment debug scenarios were not run; no optional coverage or production gate change claimed.

## 6. Reproduce and artifacts

`scripts/r16_perf84_iai.mjs --prepare` freezes A/C0 under ignored root target; `--prepare-b`
freezes A inputs plus the two candidate files. Existing snapshots are never regenerated/overwritten.
Only generated `scripts/iai.mjs` changes: target="$PWD/target", jobs3, --locked, no runner install.
Driver invokes `node scripts/iai.mjs <fifteen table bench names>` FROM each nested snapshot;
original CLI filters reported rows only, so full benchmark group executes. No shared external target.

```text
node scripts/r16_perf84_iai.mjs --verify
node scripts/r16_perf84_iai.mjs --run A1
node scripts/r16_perf84_iai.mjs --run A2
node scripts/r16_perf84_iai.mjs --activation C0
node scripts/r16_perf84_iai.mjs --run C0
node scripts/r16_perf84_iai.mjs --activation B
node scripts/r16_perf84_iai.mjs --run B1
node scripts/r16_perf84_iai.mjs --run B2
node scripts/r16_perf84_gate_table.mjs --update-report
node scripts/r16_perf84_gate_table.mjs --check
```

Five `_raw_r16_perf84_{A1,A2,C0,B1,B2}.log` excerpts retain current metrics/table verbatim, explicit truncation/reproduction markers and full-output hashes; each <200KiB, paths sanitized.
Mutant logs: `_raw_r16_perf84_mutant_{overflow,flush-mask,flush-noop}.log`.
Summary CSV and identity JSON share this report basename. Judge validates raw/table arithmetic, all repeats, guarded ratios and raw hashes. Default prints tables/writes CSV; --update-report embeds; --check reads ONLY docs artifacts and checks both table/CSV without writes or target dependency.
Baseline CSV is superseded by final summary (all baseline counts/cache columns retained).
No commits, git mutations, manifests/indexes/README/CHANGELOG edits or dependency installs.
