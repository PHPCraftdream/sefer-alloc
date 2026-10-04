# Ph3c шаг 1′ — правка §3 аддендума step1prime (carve-курсоры): ветка ph3c-b3p, PR #2116

Дата: 2026-10-05. Правка по §3 аддендума
`docs/design/2026-10-02-adr-addendum-ph3c-step1prime.md`; проект и таблица
k/потерь — `docs/perf/PH3C_B3S_SPIKE_DESIGN.md` (§7, §7.4). Решающий замер
(шаг 1′c: iai + фрагментация) выполняется оркестратором отдельно на
зафиксированном ниже SHA; после этого отчёта правок больше нет.

## Идентичность

| Что | Значение |
|---|---|
| база | интегрированный спайк B3: коммит `093c9d23` (tree `f0504322`), ветка `ph3c-b3s` |
| дифф против базы | `git diff 093c9d23 \| sha256sum` = `e41f0da82370098a4cb117d3e17126ad90af7175b75d7a1b075e0727bd535d0d` (зафиксирован ДО коммита) |
| дерево рабочего состояния | write-tree через временный индекс выполняется оркестратором на коммите (git-запись из этого сеанса заблокирована): `GIT_INDEX_FILE=<tmp> git add -A && GIT_INDEX_FILE=<tmp> git write-tree`; SHA дописывается сюда при коммите |
| новые (untracked) файлы, sha256 (первые 16 hex) | `src/alloc_core/small/alloc_core_small/b3_carve.rs` = `c995960c70e099ea`; `tests/b3p_carve_waste_proptest.rs` = `b24647d42af1c497`; `tests/b3p_interleave_footprint.rs` = `3be643fde4b89c63`; `tests/b3p_leaf_boundary_no_overlap.rs` = `d0e0e88012b62292` |

Список изменённых файлов (`git diff 093c9d23 --stat`, 618 insertions / 300 deletions):

```
 README.md                                          |   4 +-
 docs/ARCHITECTURE.md                               |   2 +-
 docs/perf/PH3C_B3S_SPIKE_DESIGN.md                 | 119 ++++++
 src/alloc_core/alloc_core/lifecycle.rs             |   5 +-
 src/alloc_core/alloc_core/mod.rs                   |  19 +
 src/alloc_core/mod.rs                              |  19 +
 src/alloc_core/segment/segment_header/b3_free.rs   |  97 ++++-
 src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs | 429 +++++-----
 src/alloc_core/small/alloc_core_small/mod.rs       |   3 +
 src/lib.rs                                         |  24 ++
 tests/r11_ph3a_issue_transaction_batch.rs          |  66 +++-
 tests/r11_ph3a_issue_transaction_scalar.rs         |  49 ++-
 tests/r11_ph3a_magazine_refill_partial.rs          |  19 +-
 tests/r9_diagnostic_reader_kinds.rs                |   9 +-
 tests/regression_batch_freelist_drain.rs           |  22 +-
 tests/regression_flush_class_unsafe_boundary.rs    |  32 +-
 16 files changed, 618 insertions(+), 300 deletions(-)
```

Проверка на посторонние правки (пункт ревью): grep добавленных строк на
DBG/TODO/FIXME/приватные пути — чисто.

## Правка (§3 аддендума step1prime)

Per-class carve-курсоры в B3Free-регионе сегмента: `[u32; 49]`, кодировка
`(start<<2)|delta`. Пробеги k под-листов по таблице `RUN_LEAVES` —
детерминированное правило: минимальное k ∈ 1..=4 с потерей ≤6%, иначе минимум;
корректностный пол `⌈(2·bs−gcd)/4096⌉`, поднимает класс 3712 до k=2. Блок не
пересекает run_end; классы > 4 KiB — span как раньше. Файл-хелпер
`src/alloc_core/small/alloc_core_small/b3_carve.rs` выделен (капа 1000 строк).
Таблица k/потерь — `docs/perf/PH3C_B3S_SPIKE_DESIGN.md` §7.4.

## Тесты

- `cargo test --features "production internals"` (skip `drop_on_64kib`) →
  **EXIT=0**, 324 цели ok. Лог `docs/perf/_raw_ph3c_b3p_cargo_prod_internals.log`.
- `cargo test --features "production alloc-stats bench-internals internals"`
  (skips: `drop_on_64kib`, `persistent_spill_oom`,
  `four_issue_transactions_preserve_state`, + 6 тестов
  `r9_bounded_background_maintenance`) → **EXIT=0**, 325 целей ok. Лог
  `docs/perf/_raw_ph3c_b3p_cargo_prod_stats.log`.
- `r1_04_alloc_core_drop_stack_pressure` в `--release`: ok (2 passed,
  1 ignored). Лог `docs/perf/_raw_ph3c_b3p_cargo_r1_04_release.log`.
- Новые тесты `b3p_*` (proptest waste 64 кейса, leaf-boundary no-overlap,
  interleave 40k single-segment) — все зелёные.
- Контрфакт: `tests/b3p_interleave_footprint.rs` на базовом дереве `ph3c-b3s`
  (093c9d23, тест-файл добавлялся копированием и удалён после) — **КРАСНЫЙ**
  «reserved extra segments: 1 -> 2»; на `ph3c-b3p` — зелёный.

Исключения (все проверены красными и на базе 093c9d23 — пре-существующие):

1. `r1_04` в debug — известная Windows-host стек-маргинальность (release зелёный);
2. `r11_p3_small_sidecar_issue_oom::persistent_spill_oom...` и
   `r11_p3_small_sidecar_preflight::four_issue_transactions...` — offline-smallvec
   инфрафейл;
3. 6 тестов `r9_bounded_background_maintenance` — offline-harness;
4. `r6_route_directory_model` — предсуществующий, как у спайка (в этом сеансе
   отдельно не прогонялся — унаследованное исключение спайка).

Адаптации тестов под B3/шаг 1′ (все честные, ослаблений нет; пре-существующие
красные на 093c9d23, кроме оговорённого):

- `r9_diagnostic_reader_kinds` — head→bitmap-инвариант;
- `regression_batch_freelist_drain` — то же;
- `regression_flush_class_unsafe_boundary` — то же;
- `r11_ph3a_magazine_refill_partial` — head→bitmap-оракул;
- `r11_ph3a_issue_transaction_scalar` — LIFO→min-offset pop;
  carve-spill→prepare-refusal (root-cause: пробег берётся с virgin-листа,
  spill на carve-пути структурно недостижим при шаге 1′);
- `r11_ph3a_issue_transaction_batch` — оракул room пересчитан от листовой
  квантованности пробега.

## Miri (paused, 4 клетки)

Конфигурации: **A** = `production internals bench-internals`,
**B** = `alloc-global internals bench-internals`;
**SB** = `-Zmiri-strict-provenance -Zmiri-disable-isolation`,
**TB** = SB `+ -Zmiri-tree-borrows`; команды как в
`docs/reviews/2026-10-01-adr-pg2-offbody-spike-receipt.md` §3, `-j 1`.

| клетка | результат |
|---|---|
| paused A SB | зелёный, COMPLETE paused_terminal_owner_retirement (`docs/perf/_raw_ph3c_b3p_miri_paused_A_sb.log`) |
| paused A TB | зелёный, COMPLETE (`docs/perf/_raw_ph3c_b3p_miri_paused_A_tb.log`) |
| paused B SB | зелёный, COMPLETE (`docs/perf/_raw_ph3c_b3p_miri_paused_B_sb.log`) |
| paused B TB | зелёный, COMPLETE (`docs/perf/_raw_ph3c_b3p_miri_paused_B_tb.log`) |

Мутант (`Node::write_next` временно возвращён в `reclaim_sidecar_record`):
paused A SB **КРАСНЫЙ**,
`error: Undefined Behavior: not granting access to tag <1263> ... weakly
protected` в `src/alloc_core/platform/node.rs:94` (стек:
reclaim_sidecar_record ← drain_sidecar_ingress ← trim_for_recycle ←
paused_terminal_owner_retirement) — ✔ покраснел именно в node.rs. Лог
`docs/perf/_raw_ph3c_b3p_miri_mutant_A_sb.log`. Откат подтверждён sha256
`reclaim.rs` до/после =
`0d7ff7ce94756b3d8004e2bed97e6d3287ea755ccebc3c4c69738de0565fbe41`.

## Прочее

Пункты ревью:

1. `lifecycle.rs` мусор убран (`bootstrap::primordial()?`);
2. `B3Free::FOOTPRINT` 12 KiB паддинг **СОХРАНЁН** — причина:
   forced-page-инварианты `lazy_initial_commit_forced_page` /
   `decomp_hooks_forced_page` (`small_meta_end()+LAZY_FIRST_CHUNK` не кратен
   страницам >4 KiB); стоимость ≤ 3 страниц/сегмент;
3. дифф чист;
4. §7 в `docs/perf/PH3C_B3S_SPIKE_DESIGN.md` добавлен.

Решающий замер (шаг 1′c: iai + фрагментация) выполняется оркестратором
отдельно на этом SHA; правок больше нет.
