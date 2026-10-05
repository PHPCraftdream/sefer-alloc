# Приёмка Ph4c — сужение legacy registry-поверхности (#2107)

Дата: 2026-10-05 · worktree `ph4c` · база B_4c = main HEAD `a0dc988d`

## 1. Безопасная замена под internals (шаг 1)

- `HeapLease::core(&mut self) -> &mut HeapCore` → `#[doc(hidden)] pub` (sound: lease держит эксклюзивную власть S1–S4, borrow привязан к `&mut self`, `!Send`).
- `MaintenanceLease::with_core` стал SAFE (`pub fn`, был `pub unsafe fn`): доказательство в doc-комментарии — терминальные публикаторы (`publish_foreign`, `RouteDirectory::lookup` → `pin.publish_small/publish_large`) пишут только в независимые sidecar-структуры (`SmallSidecar`/`LargeState`), глобальные атомики и fallback heap, никогда в `HeapCore` MAINTENANCE-слота; `HeapSlotRemote` на своей выровненной линии вне borrow-области; эксклюзивность от выигранного FREE→MAINTENANCE CAS (S2-пара). `maintenance.rs` больше не содержит `unsafe` (tier-1 `#![allow(unsafe_code)]` снят).
- Новый `dbg_claim_lease_with_config` (internals + alloc-decommit), тонкая обёртка над `claim_lease_with_config`.
- В `scripts/verify-dbg-hook-safety.mjs` SAFE_MUTATORS добавлены: `dbg_claim_lease_with_config`, `dbg_try_maintenance`, `dbg_with_core` (обоснования в скрипте). Итог сканера: 137 safe / 30 unsafe / 76 bench-gated.

## 2. Миграция тестов/бенчей/примеров (шаг 2)

| Семейство | Объём | Батч | Примечания |
|---|---|---|---|
| `tests/r6_*` + `r7` + `r8_*` + `r9_*` | 13 файлов | B1 | |
| `tests` r10/r11/oxx/r13/r25/r26/r30/r34 | 14 правлено, 6 без легаси | B2 | Особый случай: `r11_4_dealloc_batch_mixed_ownership` — remote-поток держит свой lease, парковка на mpsc; смысл (маршрутизация → дренаж владельцем) сохранён |
| `tests/regression_*` | 30 мигрировано, 2 без легаси | B3 | `mem::forget(lease)` для намеренных утечек слотов (`regression_claim_oom_initialised_gate`) |
| `tests` misc (`heap_core_*`, `numa_*`, `registry_basic` и др.) | 13 мигрировано | B4 | `registry_basic`: эквиваленты через `slot_index()`/`generation()`/drop; легаси-only тесты (`recycle(null)` no-op, double-recycle no-op) ПЕРЕНЕСЕНЫ в `#[cfg(all(test, feature="alloc-global"))] mod legacy_semantics` внутри `src/registry/heap_registry/claim.rs` (pub(crate) легален, внешняя поверхность не растёт; тесты сохранены, не удалены) |
| `benches` | 12 файлов | B5 + дочистка | см. ниже |
| `examples` | 19 файлов | B6 | 18 мигрировано (много `dbg_claim_lease_with_config`), 1 (`dealloc_only_unbound_thread`) — только doc-упоминание, кода нет |

Детали benches (B5 + дочистка):

- `perf_gate_iai` — helper `claim_leaked_heap()` (`dbg_claim_lease` + `mem::forget`, слоты умышленно never-recycled, измеряемое тело без Drop-Ir);
- `heap_fanin_persistent::run_cell` и `heap_fanin_production::run_active` — owner-поток сам держит lease (семантически точнее легаси), пре-аллокация до барьеров через mpsc, drain после join через свежий `dbg_claim_lease` с assert `slot_index` (LIFO), `mem::forget` для «never recycle in process»;
- `segment_directory_sweep` — lease паркуется в `Vec` (слоты LIVE до конца процесса = эквивалент утечённого указателя), продюсеры получают raw, выведенный из спаркованного лиза (`HeapCore` `!Send` — scoped-вариант невозможен), измеряемый путь (`dealloc_foreign_slow` → `RemoteFreeRing::push`) 1:1;
- `r29_16` — unsafe-обёртки dealloc.

Ложные совпадения (не тронуты): `loom_registry_free_slots`, `no_stale_doc_references`, `medium_size_sweep`, `r30_3_virgin_zero_skip_native_gate`, `heap_lifecycle_teardown`, `oxx_r2_03`, `r11_ph3b` (мигрирован дочисткой), `r30_1` (`table.recycle` — другой API) и др. — полный список в git diff.

Итого изменено ~106 файлов (git diff --stat: 106 files, +2310/−2100 на момент промежуточной фиксации; финальные счётчики в git).

## 3. Сужение (шаг 3)

`HeapRegistry::{claim, claim_with_config, recycle, try_maintenance}` и `MaintenanceLease::with_core` → `pub(crate)` (`recycle` остался `pub(crate) unsafe fn`; с `#[allow(dead_code)]` — вызывается только unit-тестами `legacy_semantics`). Новые internals-алиасы для тестов: `dbg_try_maintenance`, `MaintenanceLease::dbg_with_core`.

Внешние вызовы легаси после сужения: 0 (grep по tests/benches/examples/crates — только doc-комментарии).

Doc-комментарии обновлены (ADR addendum §2.6/Ph4c). README: строка tier-1 `claim.rs` и `maintenance.rs` обновлены; счётчики tier-1/tier-2 не изменились (28 tier-1 — минус seam `maintenance.rs` после снятия allow); токены числа тест-файлов 339→340 (новый оракул-тест). `src/lib.rs`: шапка без фазового нарратива — изменений нет; удалена устаревшая prose-запись seam `registry::heap_registry::maintenance` (по требованию `no_stale_doc_references`).

## 4. Долг ADR addendum §3 (шаг 4)

Карточка item 165 (`TRACKED_correctness_residuals.md`, filed and CLOSED 2026-10-05): `pub unsafe recycle/with_core` — долг с триггером Ph4c; `with_core` стал safe, `recycle` — остаточный `pub(crate) unsafe fn` до удаления легаси-поверхности. `RESOLVED.md` запись + lookup-таблица (140 vs 138 карточек — предсуществующий зазор +2 сохранён).

## 5. M-C оракул (шаг 5, из receipt Ph4b)

Закрыт. `drain_sidecar_ingress` (cold trim/re-claim) инструментирован чистыми числовыми наблюдателями `SIDECAR_INGRESS_DRAIN_CALLS`/`SIDECAR_INGRESS_RECORDS_CONSUMED` (cfg alloc-global+alloc-xthread+bench-internals), accessor `SeferAlloc::dbg_sidecar_ingress_stats()`; плюс `BACKGROUND_INGRESS_STEP_CALLS` + `SeferAlloc::dbg_background_ingress_step_calls()` для ingress-шага `maintenance_pass` (bounded drain cursor-идемпотентен — потребление записей не различимо, считаются шаги).

Новый тест `tests/r11_ph4c_ingress_consume_exactly_once_oracle.rs` (2 теста, сериализованы Mutex): trim при re-claim — Δdrain_calls==1, Δrecords==16; `maintenance_pass` — Δstep_calls==maintained. Горячий путь не тронут.

## 6. Мутанты

Прогоны 1:1 с прогонами; фичи `production internals bench-internals`, `registry_basic` — `alloc-global internals`.

| № | Мутант | Ловец | Результат |
|---|---|---|---|
| 1 | Release→Relaxed в `HeapLease::drop` CAS | `loom_r11_ph4a_heap_lease` (флип DROP_ORDERING в shadow-модели) | КРАСНЫЙ (data race); нативные однопоточные — не ловят принципиально |
| 2 | убран `reuse_hint.store` в Drop | `registry_basic::latest_hint_then_fresh_defers_older_free` | КРАСНЫЙ |
| 3 | `generation = const 0` | `registry_basic::recycle_then_claim_reuses_slot_and_bumps_generation` | КРАСНЫЙ |
| 4 | убран `trim_for_recycle` на re-claim | `r11_ph4c_ingress_consume_exactly_once_oracle` | КРАСНЫЙ (r6_route_lifecycle_pool/regression_registry_initialised_gate/r8_owner_sidecar_miss — зелёные) |
| 5 | не публиковать initialised | `r11_p3_registry_claim_uninit_oom` | КРАСНЫЙ (gate-тест ловит только сторону читателя) |
| 6 | убран CONFIG_CONFLICTS инкремент | `regression_r4_3_config_conflict` | КРАСНЫЙ |
| 7 | `dbg_claim_lease_with_config` без конфига | `regression_r4_3_config_conflict` | КРАСНЫЙ |
| 8 | `try_maintenance_at` без initialised-гейта | `r8_maintenance_registry` | КРАСНЫЙ (STATUS_ACCESS_VIOLATION) |
| 9 | maintenance игнорирует budget | `r8_maintenance_registry` (`maintenance_pass_for_test`) | КРАСНЫЙ |
| 10 | `push_back_after_oom` → LIVE | `r11_p3_registry_claim_uninit_oom` | КРАСНЫЙ |
| 11 | `MaintenanceLease::drop` Release→Relaxed | `loom_r8_maintenance_lease` | КРАСНЫЙ (shadow) |
| 12 | `with_core` дважды в `maintenance_pass` | `r11_ph4c_...oracle::maintenance_pass_ingress_step_runs_exactly_once` | КРАСНЫЙ (после добавления шагового оракула; изначально был потерянный ловец — закрыт) |
| 13 | `drain_sidecar_ingress` ранний return | `r11_ph4c_...oracle` | КРАСНЫЙ |
| 14 | легаси `recycle` игнорирует CAS | unit `legacy_semantics` и др. | ЗЕЛЁНЫЙ — доброкачественный: post-CAS тело `recycle` публикует только advisory `reuse_hint` (одиночный store, идемпотентный); доступ к ядру в любом случае защищён Acquire-CAS у claimant; наблюдаемого нарушения нет |

Итог: 14 мутантов, 13 красных (2 — только через loom-shadow модели), 1 доброкачественный с обоснованием. Все мутанты внесены/откатаны руками, бейзлайн src чист.

## 7. Чек-лист (все GREEN)

- `rustfmt --check`: все изменённые .rs (фиксы: импорт-сортировка оракул-теста, форматирование после clippy-фиксов).
- Clippy `-D warnings`: `--all-features`; `--features production`; `"production internals"`; `--all-targets "production internals bench-internals"`; `--all-targets --all-features` — все зелёные (фиксы: unused_mut/unused_unsafe/unused_parens по ~25 файлам — механические последствия lease-миграции: alloc/alloc_batch lease-методов safe, лишние mut/unsafe сняты).
- `node scripts/run-check-matrix.mjs --kind check` — GREEN (2 rows).
- `cargo check --benches --examples`: `"production internals bench-internals"` и `--all-features` — GREEN.
- `verify-dbg-hook-safety` PASS; `verify-alloc-core-dbg-internals-exhaustive` (88 файлов, 135 `dbg_*`, 0 violations); `verify-ci-sentinels` (113); `verify-internals-negative-boundary` — ALL GREEN.
- `no_stale_doc_references` GREEN (фиксы токенов 339→340, tier-1 29→28, lib.rs seam-строка); `ci_clippy_matrix_consistency` GREEN.
- Полные тесты: `--features "production internals"` — 0 failed (после фикса `registry_basic`: миграционные зависимости порядков — forget вместо drop в `bootstrap_is_idempotent` и дренаж FREE-пула в хелперах; 30/30 повторов стабильности); `--features "production alloc-stats bench-internals internals"` — 0 failed. Исключения r1_04/r6_route_directory_model: красными НЕ подтвердились (зелёные/не в наборе).
- Loom 5/5 (loom_r8_maintenance_lease, loom_r11_ph4a_heap_lease, loom_r11_ph4b_publish_recycle_drain, loom_r11_registry_claim, loom_registry_free_slots).
- Miri: r11_ph4a_lease_miri SB PASS; r11_ph4b_lease_miri SB PASS; regression_r2_06_header_race_miri и regression_xthread_small_ring_miri — SB-краснота pre-existing (scaffold `addr as *mut u8` int→ptr cast), в штатной PLAIN-модели PASS; regression_xthread_thread_free_alias_miri PLAIN PASS; decommit_miri_cycle SB PASS. P1-box `Node::write_next` не встретился.

## 8. Открытые вопросы / ограничения

1. `recycle` остаётся `pub(crate) unsafe fn` (легаси, только unit-тесты `legacy_semantics`) — удаляется вместе с легаси-поверхностью (будущая фаза).
2. Мутант №14 — доброкачественный (обоснование в таблице); мутанты №1/№11 ловятся только loom-shadow моделями (наследие Ph4a/Ph4b, открытый вопрос L1/L2 оттуда).
3. L1/L2 (Ph4b) остаются открытыми: полный HB-ловец на уровне модели — вне скоупа Ph4c.
4. Miri не покрывает реальный TLS teardown (наследие Ph4a/Ph4b) — не закрыто Ph4c.

## Идентичность дерева

```
git diff | sha256sum
b94095fb7cccbc6c21ec4dd8baf8dd2fd4fa411db9272dea2a010dc288eb9bd5
```

Untracked-файлы (в `git diff` не входят):

- `tests/r11_ph4c_ingress_consume_exactly_once_oracle.rs`
- `docs/reviews/2026-10-05-ph4c-narrow-legacy-registry-receipt.md` (этот файл)
