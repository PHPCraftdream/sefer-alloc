# B4-hot: горячий путь alloc/free своего потока — разметка и оптимизации

Задача: срезать Ir/op на own-thread alloc/free до уровня mimalloc без ослабления гарантий. Инструмент: детерминированный callgrind через `.rush/wsl/prof_win.sh` (маргинальный N/2N-метод, 50k→100k, `examples/zz_larson_probe`, production).

## 0. Резюме

- Baseline (b4hot-base): larson T=1 **323 Ir/op** (mimalloc 210.5, харнесс ~134).
- Шаг 1 (единственный реализованный — остальные кандидаты либо обслуживают гарантии, либо лежат в файлах соседних агентов): larson **317** (−6), mstress T=1 **499 → 495** (−4).
- Паритет с mimalloc на этом пути без протокольных изменений (R4/R5, §5) подтверждённо недостижим: оставшиеся ~180 Ir/op собственного кода почти целиком обслуживают перечисленные ниже гарантии (M2-оракулы, RAD-5, exactly-once credit, C6/OOM-цепочка issue).

## 1. Разметка горячих путей (b4hot-base, larson T=1, Ir/op на операцию, callgrind_annotate --auto=yes)

### 1.1 Alloc (magazine hit, `src/registry/heap_core/alloc/hot.rs` — 13.7 Ir/op собств.)

| Инструкции | Ir/op | Гарантия | Вердикт |
|---|---|---|---|
| `tcache.classes[c].count` load (строка 227 базовой версии; в текущем дереве — `hot.rs:227`, уже через helper `Tcache::class`) | 6 | инвариант журнала (владелец — единственный писатель) | bounds-check `c` не доказуем LLVM — устранён шагом 1 |
| hit-проверки + `count` store + `slots` load | ~3 | то же | частично устранены шагом 1 |
| `clear_magazine_on_issue` (segment_base + locate + RMW bitmap; `hot.rs:35`) | ~7 | RAD-5/M2 oracle 1 («блок в журнале?»): сброс резидентности при выдаче | НЕ избыточно — трогать нельзя (R4 — только проектом) |
| refill-хвост (`refill_magazine_slow`, `hot.rs:652`; `refill_magazine_slow_virgin`, `hot.rs:478` — inclusive) | ~14.7 (уже после R3) | C6/OOM-цепочка issue, drain sidecar, bump-carve | гарантирующий путь |

### 1.2 Free (`src/registry/heap_core/free/dealloc_own_base.rs` — 32.5 Ir/op собств.)

| Инструкции | Ir/op | Гарантия | Вердикт |
|---|---|---|---|
| `kind_at` + decode (segment_header_views, BlockKind::of) | ~3 | F7/Ph3b роутинг Large-vs-Small по физическому kind | избыточных чтений нет (один byte load) |
| guard: `payload_start` cmp (`dealloc_own_base.rs:309`) | 3 | M2: запрет free в область заголовка | load-bearing (биты в заголовке нулевые, без него header ушёл бы в журнал) |
| guard: `is_in_magazine` (`dealloc_own_base.rs:312`) | 5 | M2 oracle 1 (in-magazine double-free, Э6) | load-bearing |
| guard: `bump_of` (`dealloc_own_base.rs:315`) | 2 | M2 oracle 1.5 (uncarved/stale-free после reset) | load-bearing |
| guard: `alloc_bitmap.is_free` (`dealloc_own_base.rs:318`) | 2 | M2 oracle 2 (flushed-then-double-free) | load-bearing; порядок 4→6 зафиксирован докой |
| `divides()` (не-hardened, `platform/size_classes.rs`: DIVISIBILITY load 4 + mul/rot 2; сайт `dealloc_own_base.rs:422`) | 6 | слабый wrong-class фильтр («aligned wrong-class проходит», см. коммент в коде) | не удалять без решения владельца → R5 (§5) |
| `tcache.classes[c].count` load + push (`mark_magazine` + slot store + count store; вход `dealloc_own_thread_with_base`, `dealloc_own_base.rs:377`) | ~12 | R1-01 budget, RAD-5 mark, инварианты журнала | bounds-check'и устранены шагом 1 |
| overflow-flush (1756/100k операций) + `flush_class_internal` (`dealloc_own_base.rs:504`) | ~8.2 | exactly-once credit `dec_live`/`mark_free` при возврате в substrate | гарантирующий путь |

### 1.3 Классификация и разделяемая инфраструктура

- `class_for` (crates/size-classes): суммарно ~20 Ir/op (10 alloc + 10 free) — Э9 «classify ONCE» уже применён; дальше не сжимается без протокольных изменений.
- root-lookup free-пути (`segment_table_impl.rs` own_cache: ~10 Ir/op + `routing.rs` ~2): файлы соседних агентов (PR #76 — find_segment/*, R1 — routing.rs) — не тронуто.
- reclaim-скан `drain_segment_sidecar` ~5 Ir/op, `try_drain_freelist_batch` ~8.6 Ir/op — substrate refill; подготовка R3 уже слита.
- Атомиков на пути своего потока в production-сборке нет (8 Ir/op из сводки — это mpmc-харнесс; `tcache_hits` под alloc-stats выключен).

## 2. Реализованные изменения

### Шаг 1: одно bounds-элиминированное обращение к per-class журналу

- `src/registry/heap_core/state/tcache.rs`: новые `Tcache::class` / `Tcache::class_mut` (`tcache.rs:385` / `tcache.rs:400`) — в debug-профилях обычная (проверяемая) индексация, в production `get_unchecked`/`get_unchecked_mut`. SAFETY: контракт `SizeClasses::class_for` — `Some(c)` ⇒ `c < SMALL_CLASS_COUNT`; каждый прежний `self.classes[c]` делал собственный bounds-check, тот же инвариант уже предполагался.
- `src/registry/heap_core/alloc/hot.rs` (hit-плечо `alloc`, `hot.rs:227`–`hot.rs:300`): 3 индексации (`count` read/write, `slots` read) через helper.
- `src/registry/heap_core/free/dealloc_own_base.rs` (`dealloc_own_thread_with_base`, `dealloc_own_base.rs:377`): `count` read + Accept-push (`slots`/`count`), overflow-push, 2×virgin_mask — через helper; холодные циклы (компакция, перебор flush) не тронуты.
- README: tier-2 unsafe-инвентарь + строка `src/registry/heap_core/state/tcache.rs` (2 сайта), счётчики 97→99 sites / 31→32 files (проверяется `tests/no_stale_doc_references.rs`).

Замеры (marginal Ir/op, один и тот же инструмент):

| шаг | larson T=1 | mstress T=1 |
|---|---|---|
| b4hot-base | 323 | 499 |
| шаг 1 | **317** | **495** |

По файлам (larson): dealloc_own_base 32.5→27.5, hot.rs 13.7→9.7.

Безопасность: ни одна проверка не удалена — в debug-профилях каждый доступ по-прежнему паникует при выходе за границы; в production элиминируются только проверки одного и того же недостижимого условия при каждом доступе к одному и тому же массиву. Публичный API не изменён; unsafe добавлен внутри существующего deny-инвентаря (tier-2, README обновлён, `tests/unsafe_allow_placement.rs` зелёный).

### Почему дальше руками не делал (каждый кандидат — по разметке §1)

- M2-оракулы, RAD-5, payload_start, kind_at — load-bearing гарантии; их удешевление = протокольное изменение (§5).
- `divides()` (6 Ir/op) — замена описана в B1 §6.2 R5, требует page-map authoritative.
- root-lookup, refill/flush substrate — файлы PR #76 / R1.
- wall-clock-бенчи не запускались (запрещены); все цифры — callgrind.

## 3. Проекты (НЕ реализованы — требуют отдельного ADR)

### R4: штамп magazine-резиденции в слове блока вместо `magazine_bitmap`

Экономия по разметке: hit-path `clear_magazine_on_issue` ~7 Ir/op + free `is_in_magazine` 5 + push `mark_magazine` ~2 + refill-хвост (n−1 RMW ≈ 2 Ir/op в larson) ⇒ суммарно ~12–15 Ir/op. Требует: (а) расширение класса P1-box (path-b) на запись word1 свободного/журнального блока — обновление сигнатур ожидаемо-красных Miri-шагов; (б) эквивалентную замену M2-oracle 1 («word1==marker → медленная проверка bitmap»), мутирующие тесты двойного free (в журнале / после flush), `R24_2/R24_3`-плечи; (в) неподвижный интрузивный free-list. Loom не требуется (однопоточный оракул), Miri-клетки path-b — обязательны.

### R5: убрать `divides()` с обычного пути

Экономия: 6 Ir/free. Варианты: класс страницы из page map (авторитетный для free-guard, поддерживается carve/reset/decommit; расхождение = fatal) либо u16-таблица кратных. Доказательства: H1/M2-матрица (interior/wrong-class отклонения должны остаться красными как мутанты), reset/decommit-тесты page map.

## 4. Тесты и прогоны

Изменение структурное (ни одна проверка не убрана, семантика прежняя) — counterfactual: в debug-профилях индексация осталась проверяемой, контракт покрывают существующие тесты. Прогнано локально (CARGO_TARGET_DIR=.rush/cargo-target-C):

- release `production internals bench-internals`, 18 тест-файлов (double_free_guard, heap_core_tcache[_m2], regression_magazine_{oracles,bump_guard,free_bounds}, regression_r1_01_magazine_free_budget, regression_hardened_interior_ptr, r12_01/r12_02, r13_3, r17_4, r19_large_interior_free, regression_tcache_byte_budget, r11_ph3a_magazine_refill_partial, phase13_3_dealloc_layout_class, batch_tcache, stats_foreign_or_unroutable_frees) — все ok, exit 0.
- debug (default features) — тот же набор, ok.
- release+debug `production hardened internals bench-internals`: + regression_hardened_large_kind_own_free, r11_4_dealloc_batch_hardened_guards, regression_dealloc_metadata_region_guard — ok.
- release+debug `alloc-global alloc-xthread internals bench-internals`: r6_u21_foreign_free_interior_granule, r6_fix_p2_fallback_dealloc, dealloc_only_no_bind[_torn], r24_8/r25_4/r26_5_dealloc_batch_*, regression_dealloc_batch_promoted_large_free, r11_4_dealloc_batch_{mixed_ownership,same_segment_double_free}, r10_7_alloc_batch_xthread_double_free, dealloc_sublinear — ok.
- no_stale_doc_references / unsafe_allow_placement / src_file_size_cap — ok.
- clippy `-D warnings`, все 8 конфигураций ci.yml (default --all-targets; experimental; --all-features; hardened+medium-classes+internals; --lib production; production+internals --all-targets; --lib numa-aware+internals; --lib alloc-global+alloc-xthread) — все 0.
- CI-строки для обязательного прогона: Miri (path-b, широкая матрица), Loom-модели, iai (`perf_gate_iai`), mt-bench (larson/mstress T=1..8). Локальный callgrind — детерминированный, приведён выше.

## 5. Что осталось до mimalloc и почему

После шага 1 собственный код ≈ 180 Ir/op против ~77 у mimalloc. Разница почти полностью = цена гарантий (§1): per-block issue-цепочка C6 (prepare/commit), M2-оракулы, RAD-5, exactly-once credit, каталог маршрутов. Единственные крупные рычаги — R4 (~12–15 Ir/op) и R5 (6 Ir/op) из §3; дальше — перенос учёта с per-block на per-batch (новое решение по C6, вне скоупа волны B4-hot).
