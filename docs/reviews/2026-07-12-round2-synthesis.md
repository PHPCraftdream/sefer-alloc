# Round2-аудит: сверка трёх отчётов с текущим состоянием кода

**Дата:** 2026-07-12. Репозиторий `D:\dev\rust\sefer-alloc`, ветка `main`, HEAD `c26ee57`.

Источники (агентские отчёты, статические, без запуска тестов/сборки):
- `docs/agent_reports_round2/memory_safety_audit.md`
- `docs/agent_reports_round2/code_cleanup_review.md`
- `docs/agent_reports_round2/performance_opportunities.md`

Сверка выполнена агентом `oh` (Opus, high) — каждая находка перепроверена по актуальному коду (файл:строка), не по дате отчёта.

## Ключевой контекст

Отчёты писались статически 2026-07-12, уже после большого пула фиксов текущей сессии
(`4afce9c` F1/F2, `c4b2cf4` H-2/M-6, `3238061` H-1/M-1, `68f323b` L-9b, `aa4ccf9` gate
RAD-4 spin, `8604d4a` RAD-5 GO и др.). Часть находок частично или полностью
неактуальна из-за этих фиксов — ниже это отмечено по каждой находке отдельно.

---

## Отчёт 1 — memory_safety_audit.md (4 находки)

### R2-1 — CRITICAL → HIGH — safe `realloc` разыменовывает interior/stale/foreign pointer
**Статус: частично неактуальна, ядро актуально.**

- `AllocCore::realloc` (`src/alloc_core/alloc_core.rs:1287-1328`): foreign-pointer leg
  **исправлен** — `contains_base(base)` guard, `null` для чужих указателей (`4afce9c`).
- **Остаток:** копирование по caller-controlled `old_layout.size()` (:1324) в
  `Node::copy_nonoverlapping` — OOB read/write при некорректном layout, даже для
  зарегистрированного base.
- `HeapCore::realloc` (`src/registry/heap_core.rs:1556-1572`): foreign leg **без**
  membership-барьера (by design, межхиповый сценарий под `alloc-xthread`) — безусловно
  копирует `old_layout.size()` байт из произвольного `ptr`.

### R2-2 — CRITICAL — safe abandoned-stack API → global dangling pointer / UAF
**Статус: актуальна. Приоритет HIGH (потенциально CRITICAL).**

- `HeapRegistry::push_abandoned_segment`/`pop_abandoned_segment` — safe `pub fn`
  (`src/registry/heap_registry.rs:372`, `:389`), реэкспортированы, достижимы из
  downstream safe-кода без единого `unsafe`.
- `AllocCore::drop` (`alloc_core.rs:1665-1715`) не чистит global abandoned stack.
- `pop_abandoned_segment` (`:401-402`) читает atomic из уже освобождённой памяти → UAF.
- «Правильный» путь `abandon_segments` — `pub unsafe fn` (:328); `push_abandoned_segment`
  — отдельная safe-дверь, минующая эту дисциплину.

### R2-3 — HIGH → MEDIUM — safe публичные raw-memory test hooks
**Статус: актуальна, частично смягчена.**

- `RemoteFreeRing::over_test_buffer`/`init_test_buffer` (`remote_free_ring.rs:498-511`)
  — safe, `#[doc(hidden)] pub`, доходят до raw-writes.
- `dbg_*` accessors (`alloc_core.rs:1075-1186`) — теперь под `debug_assert!` guard
  (`68f323b` L-9b), но в release assert исчезает, raw-доступ остаётся.

### R2-4 — MEDIUM — `HeapOverflow::drain` возвращает `tail`, а не фактический `head`
**Статус: актуальна.**

- `heap_overflow.rs:355-381`: при остановке на неопубликованном слоте (:369) публикует
  `h` в `self.head` (:379), но возвращает `t` (:380).
- `heap_core.rs:772-785`: owner кэширует возврат как `overflow_tail_cache`;
  `is_likely_empty` сравнивает `tail == cache` (`heap_overflow.rs:336`) — освобождение
  «застревает» до следующего push. Тихая потеря освобождений, не double-free.
- Сосед `RemoteFreeRing::drain` возвращает фактический `h` — подтверждает, что это
  регрессия нового «двойника».

---

## Отчёт 2 — code_cleanup_review.md (15 находок)

| # | Суть | Статус | Приоритет |
|---|------|--------|-----------|
| 1 | `dealloc`/`realloc` — safe API с unsafe-предусловиями | Актуальна (дубль R2-1) | HIGH |
| 2 | Test-only raw-pointer швы публичны и safe | Актуальна (дубль R2-2/R2-3) | MEDIUM |
| 3 | Config `SeferAlloc` = «first-bind-wins» на потоке | Актуальна | MEDIUM |
| 4 | Rejected `alloc-runfreelist` эксперимент оставлен | Актуальна | LOW-MEDIUM |
| 5 | Legacy concurrent tier тянет `experimental`/`pinning` | Актуальна | LOW-MEDIUM |
| 6 | `LargeCacheMode::{Background,Both}` не меняют поведение | Актуальна | MEDIUM |
| 7 | `AllocBitmap`/`MagazineBitmap` дублируют реализацию | Актуальна | LOW |
| 8 | Docs `PageMap` противоречат mixed-class модели | Вероятно актуальна (не верифицировано построчно) | LOW |
| 9 | Устаревшие module docs (run_stack, magazine_bitmap) | Частично: `magazine_bitmap.rs` всё ещё «GO/NO-GO EXPERIMENT» при уже принятом GO | LOW |
| 10 | Future-only функции + неиспользуемый `is_huge` | Частично актуальна (`set_small_current` под `allow(dead_code)`, :1548) | LOW |
| 11 | Крупные модули + исторические комментарии | Актуальна (файлы 1700-2500+ строк) | LOW |
| 12 | Docs `Region` обещают dense/always-compact над `SlotMap` | Актуальна (`region.rs:7-8` vs фактический тип) | LOW |
| 13 | `sefer-region` категория `no-std::no-alloc` неверна | Актуальна (`Cargo.toml:13`) | LOW |
| 14 | `Reservation::is_empty` всегда `false` | Актуальна (`vmem/lib.rs:116-119`) | LOW |
| 15 | Root `Cargo.toml` = changelog/ledger | Актуальна | LOW |

---

## Отчёт 3 — performance_opportunities.md (10 находок)

| # | Суть | Статус | Потенциал |
|---|------|--------|--------|
| 1 | Miss малого аллокатора = O(S) обход SegmentTable | Актуальна (`alloc_core_small.rs:1520`, `for i in 0..n`) | HIGH |
| 2 | CAS-сериализация + retry storm в cross-thread free | Частично неактуальна (dead-owner случай закрыт `aa4ccf9`) | MEDIUM-HIGH |
| 3 | 32-КиБ MagazineBitmap RMW на каждый magazine hit/push | Актуальна, но предпосылка спорна — это принятое GO-решение RAD-5 | HIGH (спорно) |
| 4 | LockFreeRegion копирует таблицу страниц + 64 Arc bumps/запись | Актуальна (research tier, deprecated) | HIGH |
| 5 | EpochRegion `Mutex<Vec>` remote-free, теряет capacity при drain | Актуальна (research tier) | MEDIUM |
| 6 | ShardedRegion TLS process-global, не per-instance | Актуальна, есть нюанс ревалидации | MEDIUM |
| 7 | Глобальный atomic diagnostic storm + линейный `stats()` scrape | Актуальна (связана с #2) | MEDIUM |
| 8 | Inline footprint HeapOverflow ring (~24 КиБ/slot) | Актуальна | MEDIUM |
| 9 | `class_for` для align>16 — modulo/divisibility loop | Актуальна (`size_classes.rs:161-181`) | LOW-MEDIUM |
| 10 | Coarse `RwLock<Region>` в SyncRegion | Актуальна | LOW-MEDIUM |

---

## Дубли и противоречия

- **Дубль (усиливающий):** `cleanup#1` ≡ `memory_safety R2-1` (safe `realloc`/`dealloc`).
- **Дубль (усиливающий):** `cleanup#2` ≡ `memory_safety R2-2`+`R2-3` (публичные test-only
  raw-pointer швы через `#[doc(hidden)] pub`).
- **Расхождение с состоянием репо (не между отчётами):** `performance#3` предлагает
  убрать/переработать MagazineBitmap, тогда как репозиторий только что влил его как
  GO-улучшение (RAD-5, `docs/perf/IAI_BASELINE.md`).
- **Частичное расхождение:** `performance#2` описывает retry-storm как открытый
  катастрофический дефект, но `aa4ccf9` уже закрыл dead-owner сценарий.
- Прямых противоречий между тремя отчётами нет — покрывают разные оси, в пересечениях
  согласованы.

---

## Итоговая сводка

- **Всего находок:** 29 (memory_safety: 4, cleanup: 15, performance: 10).
- **Уникальных** (с учётом 3 дублей cleanup↔memory_safety): ~26.
- **Актуальных** (полностью или в остаточной части): ~26 из 29.
- **Существенно смягчены/частично неактуальны:** 3 — R2-1 (foreign-leg `AllocCore`
  закрыт), performance#2 (dead-owner storm закрыт), performance#3 (предпосылка
  отменена принятым RAD-5).

### Топ-5 приоритетных актуальных находок

1. **R2-2 / cleanup#2 — safe abandoned-stack API → UAF**
   (`src/registry/heap_registry.rs:372,401-402`, `src/alloc_core/alloc_core.rs:1665-1715`).
   Safe `push_abandoned_segment`/`pop_abandoned_segment` через `#[doc(hidden)] pub`
   `registry`, публикуют base сегмента в process-global stack; `AllocCore::drop` его не
   вычищает → следующий `pop` читает atomic из unmapped памяти. Достижимо без единого
   `unsafe` у вызывающего.

2. **R2-1 / cleanup#1 — остаток небезопасного `realloc`**
   (`src/registry/heap_core.rs:1556-1572`, `src/alloc_core/alloc_core.rs:1324`).
   `HeapCore::realloc` foreign-leg без membership-проверки; оба пути доверяют
   caller-controlled `old_layout.size()` при копировании.

3. **performance#1 — O(S) обход SegmentTable на miss малого аллокатора**
   (`src/alloc_core/alloc_core_small.rs:1476-1673`, `for i in 0..n`). Каждый промах
   линейно сканирует все когда-либо заведённые слоты + drain каждого ring + BinTable.
   Крупнейший асимптотический потенциал, но требует строгого membership-учёта.

4. **R2-4 — `HeapOverflow::drain` возвращает `tail`, а не `head`**
   (`src/registry/heap_overflow.rs:380`, `src/registry/heap_core.rs:785`). Тихая потеря
   освобождений при конкретном interleaving; сосед `RemoteFreeRing` этого дефекта не
   имеет.

5. **cleanup#6 — `LargeCacheMode::{Background,Both}` — молчаливый no-op**
   (`src/alloc_core/large_cache_mode.rs:21-32`). Публичный API принимает режимы,
   документированные как «future», но идентичные `Lazy`. Дешёвое исправление, высокий
   приоритет по цене/эффекту.

### Релевантные файлы (абсолютные пути)

- `D:\dev\rust\sefer-alloc\src\registry\heap_registry.rs` (R2-2)
- `D:\dev\rust\sefer-alloc\src\alloc_core\alloc_core.rs` (R2-1, R2-2 drop, R2-3 dbg_*)
- `D:\dev\rust\sefer-alloc\src\registry\heap_core.rs` (R2-1 HeapCore::realloc, R2-4 cache, perf#2/#3)
- `D:\dev\rust\sefer-alloc\src\registry\heap_overflow.rs` (R2-4)
- `D:\dev\rust\sefer-alloc\src\alloc_core\alloc_core_small.rs` (perf#1)
- `D:\dev\rust\sefer-alloc\src\alloc_core\remote_free_ring.rs` (R2-3)
- `D:\dev\rust\sefer-alloc\src\alloc_core\large_cache_mode.rs` (cleanup#6)
- `D:\dev\rust\sefer-alloc\src\global\sefer_alloc.rs` (cleanup#3)
- `D:\dev\rust\sefer-alloc\src\concurrent\lock_free_region.rs`, `epoch_region.rs`, `sharded_region.rs` (perf#4/#5/#6)
- `D:\dev\rust\sefer-alloc\crates\region\src\region.rs`, `crates\region\Cargo.toml`, `crates\vmem\src\lib.rs` (cleanup#12/#13/#14)

**Оговорка:** сверка статическая (grep + чтение), тесты/сборка не запускались.
Достижимость unsafe-путей R2-1..R2-3 подтверждена по сигнатурам и visibility, не всегда
по фактической эксплуатации end-to-end.
