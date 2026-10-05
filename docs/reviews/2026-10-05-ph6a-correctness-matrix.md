# Ph6a correctness matrix

Дата: 2026-10-05. Worktree `worktrees/ph6a`, ветка `ph6a` (база = main).

## Source identity

- Git SHA: `ba557ca4d4719a09da70ebe7c7debe0e51bce4ee` (HEAD, ph6a).
- Во время матрицы внесены **2 правки только в tests/**: `tests/r11_ph5c_registry_saturation_fallback.rs`
  (cfg-гейт: исключение Windows без primordial-lazy-commit, дефект D1) и
  `tests/r11_ph4c_ingress_consume_exactly_once_oracle.rs` (bounded pre-drain от флака, дефект D2).
  **src/ не менялся**: `git diff --stat -- src` пуст (проверено на HEAD).
- Toolchain:
  - Windows x86_64: rustc 1.97.0 (2d8144b78 2026-07-07);
  - WSL Linux x86_64: rustc 1.98.1 (48a229cea 2026-09-01);
  - nightly (Miri): rustc 1.99.0-nightly (3659db0d3 2026-07-05), LLVM 22.1.8 (.logs/tc_nightly.log).

## Методология (правила вердикта)

- Каждый PASS подтверждён positive-marker'ом из лога: строка `<test> ... ok`, `test result: ok. ...`
  или `[EXIT=0]`/верди́кт скрипта. Имена тестов не выдумываются — только grep из .logs/.
- `cfg-empty` (тесты не собрались из-за feature-гейта) ≠ PASS — фиксируется как NOT_RUN(cfg-broken).
- EXIT≠0 разбирается до причины: предписанное KNOWN-RED исключение параллельного агента, окружение
  (env/infra) или реальный дефект. KNOWN-RED НЕ чинился по предписанию.
- Miri EXPECTED RED (P1-box) — известный дефект, не переводится в PASS; зелёный narrow-witness
  подтверждает, что красный только на paused-пути.
- Loom: root `loom_*` модели — shadow-model (протокол под `--cfg loom`), real-type — только crate-сюиты;
  PASS для root-моделей не повышается до shipping PASS без этой оговорки.
- Мутационная часть: мутант считается CAUGHT только при красном прогоне ловца и зелёном re-run после
  отката (для M1 — пара M1b).

## Таблица клеток

### A. Native Windows (cargo test)

| cell | features / команда | verdict | positive-marker (из лога) | receipt |
|---|---|---|---|---|
| A1 | production internals, `cargo test` | PASS | `test registry_saturation_routes_to_fallback_and_back ... ok` | native_win_production_internals.log |
| A2 | production, `cargo test --lib --tests` | PASS | `test result: ok. 10 passed; 0 failed` (+EXIT=0). Примечание: полный `cargo test` без `--lib --tests` красный на сборке by-design compile-fail bait `examples/sol_f1_dbg_carve_batch_negative_probe.rs` (граница internals, Sol-F1; маркер красного в f_prod.log:9) — это НЕ дефект | f_prod2.log, f_prod.log |
| A3 | alloc-global internals bench-internals | PASS | EXIT=0, `test result: ok. 2 passed` | f_allocglob2.log (первый прогон f_allocglob.log EXIT=9, crash saturation-теста — дефект D1, после фикса зелёный) |
| A4 | production alloc-stats bench-internals internals | PASS | 346 × `test result: ok`, EXIT=0; `test fallback_oom_injection_nulls_alloc_and_recovers_when_disarmed ... ok`; `test registry_saturation_routes_to_fallback_and_back ... ok` | f_allocstats.log |
| A5 | batch-api internals | PASS | 345 × `test result: ok`, EXIT=0 | f_batch.log |
| A6 | pinning | PASS | 345 × `test result: ok`, EXIT=0 | f_pinning.log |
| A7 | hardened medium-classes internals | PASS | EXIT=0 (изначальный f_hardened.log EXIT=9 по D1) | f_hardened2.log |
| A8 | CI-строка: production virgin-zero-skip alloc-stats bench-internals internals, `--no-fail-fast` | PASS | EXIT=0 | f_vzs2.log. Голый virgin-zero-skip — NOT_RUN(cfg-broken): `error[E0432]: unresolved import sefer_alloc::SeferAlloc` (f_vzs.log, EXIT=101); поддерживаемая линия — только CI-строка |
| A9 | `--all-features` | PASS | EXIT=0, 0 failed | f_allfeat.log |
| A10 | RUSTFLAGS=`--cfg numa_shim_mock` `--all-features` | PASS + 2 KNOWN-RED | EXIT=101, 347 × `test result: ok`; красные ТОЛЬКО `r6_fix_p3_numa_unknown_bucket` (паника `unknown_bucket_bit_is_cleared_after_node_gets_dedicated_bucket`, numa-mock семья) и `segment_directory_numa_high_node_ids` (`ninth_distinct_high_node_overflows_to_unknown_bucket_without_corruption`) — оба KNOWN-RED, домен параллельного агента | f_numamock2.log (первый прогон f_numamock.log NOT_RUN(infra): sccache os error 10054, D3) |
| A11 | production internals bench-internals lazy-commit-fault-injection, `--no-fail-fast` | PASS | EXIT=0. Первый прогон f_faultinj.log EXIT=101 — флак exactly_once 17≠16 (D2); изолированный прогон до фикса 3× зелёный: repro_ingress.log | f_faultinj3.log |
| A12 | `node scripts/run-check-matrix.mjs --kind check` | PASS | `[check-matrix] ALL GREEN`, EXIT=0 | f_checkmatrix.log |

### B. Fault injection (named)

PASS. Covered A11 (lazy-commit-fault-injection fixture; aligned-vmem/fault-injection через неё) +
fallback-OOM / registry-chunk-OOM хуки под bench-internals в A4. Маркеры из f_allocstats.log:
`test fallback_oom_injection_nulls_alloc_and_recovers_when_disarmed ... ok`,
`test registry_saturation_routes_to_fallback_and_back ... ok`.

### C. Miri (nightly, SB strict, miri.mjs, 16 entries) — f_miri.log

| cell | entry | verdict | positive-marker / маркер UB | receipt |
|---|---|---|---|---|
| C1 | 15/16 entries | PASS | `[miri:region_invariants] PASS`, `[miri:regression_r2_05_diag_provenance_miri] PASS`, `[miri:decommit_miri_cycle] PASS`, `[miri:regression_large_align_no_segment_exhaustion] PASS`, `[miri:regression_page_aligned_no_segment_exhaustion] PASS`, `[miri:regression_realloc_cross_class_shrink] PASS`, `[miri:regression_realloc_oob_old_layout] PASS`, `[miri:regression_magazine_oracles] PASS`, `[miri:regression_bump_direct_refill] PASS`, `[miri:stress_boundary_sweep] PASS`, `[miri:regression_virgin_bitmap_skip] PASS`, `[miri:regression_w3_stats_aliasing_miri] PASS`, `[miri:segment_directory_a5_miri] PASS`, `[miri:narrow_domain_unchecked_storage] PASS`, `[miri:miri_global_box_acceptance:narrow] PASS` | f_miri.log |
| C2 | miri_global_box_acceptance:paused | FAIL = KNOWN-DEFECT P1-box (EXPECTED RED) | `error: Undefined Behavior: not granting access to tag <1484> ... which is weakly protected` → `src\alloc_core\platform\node.rs:90:18` (Node::write_next) ← reclaim_sidecar_record ← drain_sidecar_ingress ← trim_for_recycle; место совпадает с ci.yml EXPECTED RED - P1. НЕ переводить в PASS | f_miri.log |
| C3 | production SB paused | KNOWN-DEFECT (EXPECTED RED) | стек: `Node::write_next` ← `reclaim_sidecar_record` (m_prod_sb_paused.log:25–27) | m_prod_sb_paused.log |
| C4 | production TB paused | KNOWN-DEFECT (EXPECTED RED) | `write access through <1221> ... is forbidden`, `Node::write_next` ← `reclaim_sidecar_record` | m_prod_tb_paused.log |
| C5 | alloc-global TB paused | KNOWN-DEFECT (EXPECTED RED) | `Node::write_next` ← `reclaim_sidecar_record` | m_glob_tb_paused.log |
| C6 | production narrow witness SB/TB | PASS | `[miri_global_box_acceptance] COMPLETE narrow_two_rounds`, EXIT=0 (оба лога) — красный только на paused-пути | m_prod_sb_narrow.log, m_prod_tb_narrow.log |

### D. Loom — f_loom.log, `node scripts/loom.mjs`

PASS (EXIT=0), 6 групп: `[loom -p once-ptr-cell] PASS` (crate, real-type),
`[loom -p tagged-index-stack] PASS` (crate, real-type ABA),
`[loom --features alloc-core,alloc-xthread] PASS` (4 модели),
`[loom --features alloc-global,alloc-xthread,tagged-index-stack/loom] PASS` (registry_free_slots),
`[loom --features alloc-global,alloc-xthread,internals,tagged-index-stack/loom] PASS` (5 моделей internals-группы),
`[loom --features experimental] PASS` (3 модели).
**MODEL-LIMIT:** root `loom_*` модели — shadow-model (протокол под `--cfg loom`, не production-код
аллокатора); real-type только у crate-сюит.

### E. WSL Linux x86_64

| cell | features | verdict | маркеры | receipt |
|---|---|---|---|---|
| E1 | production internals | PASS(с KNOWN-RED) | EXIT=101, 187 × `test result: ok`; красный ТОЛЬКО `r6_route_directory_model`: паника `thread 'terminal_publication_signatures_consume_the_pin' panicked at tests/r6_route_directory_model.rs:30:5` (KNOWN-RED offline-smallvec, параллельный агент) | wsl_prod_int.log |
| E2 | --all-features | PASS(с KNOWN-RED + NOT_RUN(env)) | EXIT=101, 347 × `test result: ok`; красные: `r6_route_directory_model` (KNOWN-RED) + `regression_r2_01_segment_bases_lifetime` — compile-fail fixture не смог резолвнуть `cc ^1.0` из crates.io в WSL (окружение, не код; на Windows A9 та же клетка зелёная) | wsl_allfeat.log |
| E3 | numa_shim_mock --all-features | PASS(с KNOWN-RED + NOT_RUN(env)) | EXIT=101, 345 × `test result: ok`; красные: `r1_04_alloc_core_drop_stack_pressure` (`drop_on_64kib_stack_bare`/`..._with_many_segments`, KNOWN-RED debug-стек), `r6_route_directory_model` (KNOWN-RED), `r6_fix_p3_numa_unknown_bucket` (`unknown_bucket_bit_is_cleared_after_node_gets_dedicated_bucket`, KNOWN-RED), `regression_r2_01_segment_bases_lifetime` (NOT_RUN(env)) | wsl_numamock.log |

### F. TSan — f_tsan.log, `scripts/tsan.mjs`

PASS. features `alloc-global alloc-xthread alloc-decommit internals bench-internals`;
маркер: `test teardown_ordering_stress_no_crash ... ok`, `[tsan:production internals bench-internals] PASS`, EXIT=0.

### G. CI_PENDING

macOS arm64 native (все ключевые feature-строки), Linux aarch64 native — локального тулчейна нет.
Вердикт: CI_PENDING (будет по landing SHA).

## Таблица мутантов (8/8 CAUGHT, src/ чист после откатов)

| mutant | сайт (src/) | ловец | маркер красного | откат | маркер зелёного |
|---|---|---|---|---|---|
| M1 installed-Box/free-list | alloc_core/platform/node.rs:90 `write_next` → `next.wrapping_add(1)` | `free_list_reuses_freed_blocks` (alloc_core_invariants.rs) [alloc-core internals] | mut_M1b.log: `test free_list_reuses_freed_blocks ... FAILED` | sha256 совпал, `git diff --stat -- src` пуст | mut_M1b_green.log: `test result: ok. 1 passed`. Примечание: первая попытка M1 с ловцом `freelist_reuse` — NOT_CAUGHT (mut_M1.log `test result: ok. 1 passed`), инертный slotmap-тест; ловец заменён |
| M2 own free | registry/heap_core/free/dealloc_own_base.rs:399 RouteToLargeFree пропущен core.dealloc | r14_4_promotion_free_correctness [production medium-classes internals bench-internals] | `test canary_survives_promotion_and_free_leaves_no_leak_per_base ... FAILED` | ок | mut_M2_green.log: `test result: ok. 3 passed` |
| M3 foreign publication | registry/heap_core_xthread/routing.rs:85 publish_foreign small → false | regression_r5_01_alloc_global_cross_thread [production internals] | `test cross_thread_small_free_does_not_hit_foreign_or_unroutable_frees ... FAILED` | ок | mut_M3_green.log: `test result: ok. 5 passed` |
| M4 fallback | global/tls_heap.rs null-слот → Own(null) | r11_ph5c_registry_saturation_fallback [production internals] | mut_M4.log: `thread caused non-unwinding panic. aborting.` + `error: test failed` (null deref abort) | ок | mut_M4_green.log: `test result: ok. 1 passed` |
| M5 batch | registry/heap_core/alloc/batch.rs:188 `filled += n` → `saturating_sub(1)` | batch_tcache [production batch-api] | 6 FAILED, вкл. `alloc_batch_freed_via_scalar_dealloc`, `batch_mixed_size_classes_back_to_back` | ок | mut_M5_green.log: `test result: ok. 7 passed` |
| M6 realloc OOM | registry/heap_core/free/realloc.rs:333 при OOM Node::zero старого блока | r11_ph5b_c6_realloc_oom_old_alive [production internals] | `small_realloc_oom_keeps_old_allocation_alive ... FAILED` + `large_realloc_oom_keeps_old_allocation_alive ... FAILED` (2 failed) | ок | mut_M6_green.log: `test result: ok. 2 passed`. Замечание: вариант «освободить старый» не ловится — double-free глушится M2-оракулом; пойман вариант «испортить» |
| M7 zeroed | alloc_core/alloc_core/mem/mem_impl.rs:118 `!is_virgin` → `is_virgin` | alloc_zeroed_virgin_small_skip [internals alloc-core alloc-decommit virgin-zero-skip] | 4 FAILED, вкл. `interleaved_virgin_and_reuse_always_zero`, `pooled_segment_alloc_zeroed_never_claims_virgin` | ок | mut_M7_green.log: `test result: ok. 5 passed` |
| M8 worker failure rollback | global/maintenance_service.rs:57 StartingGuard::drop без rollback | r11_ph4b_spawn_failure_retry_releases_leases [production internals bench-internals] | `test spawn_failure_leaves_no_half_activation_and_retry_releases_leases ... FAILED` | ок | mut_M8_green.log: `test result: ok. 1 passed` |

Итог: **8/8 CAUGHT**, после всех откатов `git diff --stat -- src` пуст.

## Найденные дефекты

- **D1 (tests/, не src/).** `tests/r11_ph5c_registry_saturation_fallback.rs` собирался на конфигах с
  eager primordial (Windows без `primordial-lazy-commit`); drain MAX_HEAPS=4096 × 4 MiB eager-commit
  ≈ 16 GiB commit charge; измерено: drain дошёл до 1924 lease, затем fallback `HeapCore::new` → OS
  refusal → alloc null (M10 true-OOM контракт); 0xc0000409 при unwind. Красный подтверждён бисектом:
  primordial-lazy-commit ON → PASS, OFF → red. Фикс: cfg-гейт исключает `windows && !primordial-lazy-commit`
  (по образцу существующего numa-aware исключения). Продуктового дефекта НЕТ: shipping-линия
  production (lazy ON) PASS.
- **D2 (tests/, не src/).** Флак `tests/r11_ph4c_ingress_consume_exactly_once_oracle.rs`: ORACLE_LOCK
  сериализует тела, но остаточные pending-публикации sibling-теста на hinted-слоте попадали в окно
  оракула (17 вместо 16). Фикс: bounded pre-drain до окна. Полный прогон после фикса зелёный (A11).
- **D3 (инфра).** sccache os error 10054 при полной перекомпиляции (f_numamock.log:99) —
  NOT_RUN(infra); перегон без wrapper зелёный по коду (A10, 347 ok).

## Известные исключения (KNOWN-RED, подтверждены красным, НЕ чинились по предписанию — параллельный агент)

- `r1_04_alloc_core_drop_stack_pressure` (debug-стек) — wsl_numamock.log:1563,1576;
- `r6_route_directory_model` (offline-smallvec) — wsl_prod_int.log:1524, wsl_allfeat.log:1717;
- `r6_fix_p3_numa_unknown_bucket` + `segment_directory_numa_high_node_ids` (numa-mock семья) —
  f_numamock2.log:1602,2773; wsl_numamock.log:1736.

## NOT_RUN / CI_PENDING

- NOT_RUN(cfg-broken): голый virgin-zero-skip (f_vzs.log, E0432 SeferAlloc без alloc-global);
  поддерживаемая линия — CI-строка (A8, f_vzs2.log PASS).
- NOT_RUN(infra): f_numamock.log (sccache 10054, D3) — перегон A10 зелёный.
- NOT_RUN(env): `regression_r2_01_segment_bases_lifetime` в WSL (cc ^1.0 из crates.io) — на Windows зелёный.
- CI_PENDING: macOS arm64 native (все ключевые feature-строки), Linux aarch64 native.

## Вывод

Поставляемая композиция (production*) зелёная на native Windows, Miri (кроме EXPECTED-RED P1-box),
Loom, TSan и WSL Linux. P1-box — KNOWN-DEFECT с точным местом UB (Node::write_next,
src/alloc_core/platform/node.rs:90, по addendum path-b: не MODEL-LIMIT). Дефектов src/ не найдено;
правки во время матрицы — только tests/ (D1, D2). Мутационное покрытие 8/8 CAUGHT.
