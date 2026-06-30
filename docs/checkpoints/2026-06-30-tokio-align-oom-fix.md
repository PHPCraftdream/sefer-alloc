# Checkpoint — 2026-06-30 tokio-align-oom-fix

## Session summary

Пользователь привёл bug-report `D:\dev\rust\shamir-db\docs\perf\sefer-alloc-0.2.0-oom-bug-2026-06-30.md`:
после переключения с `mimalloc` на `sefer-alloc 0.2.0` (features = `production`)
bench `duplex_throughput/duplex_cap32/32` в shamir-db детерминированно падал с
`memory allocation of 640 bytes failed` после ~55×32 ≈ 1760 task-spawn'ов на
машине с 32 GiB RAM (явно не реальный OOM). Также — 22-31% perf-регрессия на
single-thread `db_handler/*` бенчах vs mimalloc (пользователь сказал её пока
не трогать, фокус только на OOM).

**Воспроизвели** bug 1-в-1: `cargo bench -p shamir-server --bench
duplex_throughput -- 'duplex_cap32/32'` падает в фазе measurement (Windows exit
0xc0000409 = abort). Backtrace показал: stack идёт от уже-bind'нутого tokio
multi-thread worker через `request_loop → JoinSet::spawn → tokio task
Cell::new → box_new_uninit → handle_alloc_error → rust_oom`.

**Локализация** — добавили временное инструментирование (atomic counters +
no-alloc raw-write на stderr через guarded thread-local в `SeferAlloc::alloc`
и `alloc_large_slow`). Получили:
```
[sefer-alloc] OOM branch=Large.SegmentTable::register size=640 align=128 usable=4194304
```

**Root cause (архитектурный):** `SizeClasses::class_for(size, align)` в
`src/alloc_core/size_classes.rs` имел гард `if align > SMALL_ALIGN_MAX (=16) →
None`. tokio `runtime::task::core::Cell<T,S>` — `#[repr(align(128))]` против
false sharing → каждый spawn = `(640, align=128)` → Large path → выделение
1 целого 4 MiB segment + 1 SegmentTable slot **на каждую task cell**. 32
одновременных task × 55 итераций превышает `MAX_SEGMENTS=1024` → `register=None`
→ null → abort. Параллельно объясняет и perf-регрессию (32 × 4 MiB = 128 MiB
RSS на 32 task cells суммарным полезным объёмом ~20 KiB — TLB pressure +
heap blow-up).

**Fix:** в `class_for` снят `align > SMALL_ALIGN_MAX` гард; вместо него — поиск
первого small-класса с `block_size >= max(size, align)` AND
`block_size % align == 0`. M4 (alignment fidelity) сохранён: `base`
SEGMENT-aligned, offset кратен `block_size`, который кратен `align` → ptr
aligned. Fast path для `align ≤ 16` (типичный) — O(1), без изменений. Slow
path — walk ≤ 40 классов, 0–3 итерации для async-runtime align'ов (32/64/128/256).
Для `(640, 128)` находит class 13 (block_size=768, 768 % 128 == 0).

**Verified:**
- Полный sefer-alloc test suite под features=production — 0 failed (включая
  loom_bootstrap_cas, loom_xthread_protocol, loom_thread_free).
- Новый `tests/regression_large_align_no_segment_exhaustion.rs` (2 теста)
  — passed. **Counterfactual проверен**: revert фикса валит на iteration 1023
  (= MAX_SEGMENTS-1, primordial занимает 1 slot) — тест не vacuous.
- `tests/size_classes_lookup.rs` обновлён: reference impl приведён к
  divisibility-aware контракту; старый boundary test переписан под новое
  поведение; добавлен `tokio_task_cell_shape_resolves_to_small_class_not_large`.
- `shamir-db/duplex_throughput` все 4 варианта pass; `duplex_cap32/32`
  показывает `Performance has improved` (-5% vs prior run).
- Инструментирование полностью удалено (DBG-counters + emit_oom_diag +
  format helpers — все revert).

**Текущее состояние shamir-db**: `crates/shamir-server/Cargo.toml` всё ещё
с path override `sefer-alloc = { path = "D:/dev/rust/sefer-alloc", … }`.
Lock-file тоже изменён. Это даёт shamir-db живой фикс ДО релиза 0.2.1.
Откатывать path override — после релиза 0.2.1, или сразу если пользователь
скажет (нужен явный сигнал).

**НЕ сделано (отложено / не просили):**
- Коммит / релиз 0.2.1 (пользователь не просил).
- Snятие path override в shamir-db (ждём релиза).
- Perf-регрессия 22-31% на single-thread (пользователь сказал не трогать
  на opt-0 бенч-профиле; вероятно частично уйдёт сама т.к. align>16
  объекты теперь не платят 4 MiB segment-tax).

## Active goal

none

## TaskList

(пусто — все 6 таск этой сессии (#113–#118) закрыты status=completed или
deleted; runtime TaskList агенту вернул empty после закрытия)

### recently completed (из этой сессии)
- #113 Воспроизвести OOM на duplex_cap32/32 (shamir-db) ✓
- #114 Локализовать root cause OOM (sefer-alloc 0.2.0) ✓
- #116 Фикс root cause бага в sefer-alloc ✓
- #117 Регрессионный тест на OOM в sefer-alloc ✓
- #118 Верификация фикса на shamir-db ✓

### deleted
- #115 (Извлечь минимальный repro в sefer-alloc — пользователь решил
  работать прямо в shamir-db как testbed; #117 закрывает суть)

## Decisions

- **Работать на shamir-db напрямую как testbed**, а не извлекать
  минимальный repro в sefer-alloc. Пользователь сказал "для начала наверно
  можно прямо на той репе наладить". Отвергнуто: standalone repro
  (#115 → deleted) — итерация быстрее на shamir-db с path override.
- **path override `sefer-alloc` в shamir-server/Cargo.toml**,
  а не временный feature toggle. Toggle сломал бы компиляцию `main.rs`
  (использует `LargeCacheConfig::with_config` под `alloc-decommit`).
  Path override позволил итерировать sefer-alloc локально + сразу видеть
  эффект в shamir-db bench.
- **Lock-free raw write через `std::io::stderr().lock().write_all`**
  + `thread_local` guard от reentry, NOT `eprintln!`, для diagnostic emit
  внутри alloc-face. Под Windows write на STD_ERROR_HANDLE через WriteFile
  — без alloc. eprintln мог бы recurse в global allocator.
- **Сделать `class_for` divisibility-aware вместо подъёма `SMALL_ALIGN_MAX`**.
  Подъём SMALL_ALIGN_MAX до 128 сменил бы инвариант "every block
  MIN_BLOCK-aligned" и потребовал бы пересмотра carve / dealloc /
  bitmap / page-map. Divisibility-проверка — surgical: M4 сохранён через
  `block_size % align == 0`, hot path для align≤16 байт-идентичен.
- **Не трогать perf-регрессию 22-31% в этой сессии**. Пользователь явно
  сказал "на opt 0 не имеет смысл сравнивать бенчи — пока не обращаем
  внимания. Весь фокус на баг". OOM-фикс может частично её закрыть (align>16
  объекты больше не платят 4 MiB segment-tax), но это нужно отдельно мерить
  на opt-3.

## Open questions

- **Когда откатывать path override в shamir-db?** Зависит от сроков
  релиза sefer-alloc 0.2.1. Пользователь не сказал. Если сразу
  фикс-релиз — оставить override до 0.2.1, потом вернуть `version = "0.2.0"`
  → `version = "0.2.1"`. Если откладываем релиз — override остаётся
  в shamir-db до выпуска.
- **Перепрогон perf-регрессии на opt-3 после фикса**. shamir-db
  `[profile.bench]` сейчас `opt-level=0` (быстрый цикл). Финальный замер
  perf vs mimalloc нужно делать с `cargo bench --profile=release`. Не
  начато.
- **Соседние большие бенчи в shamir-db** (`db_handler_rps`,
  `wire_pipelining`, `wire_latencies`, `subscription_*`) — нужно ли тоже
  прогонять для подтверждения отсутствия OOM-регрессий? `db_handler_rps`
  уже прогнан, прошёл (но числа absolute — opt-0). Остальные — нет.
- **commit message format / sefer-alloc releaserate** — pending явной
  команды пользователя.

## Repo state

### sefer-alloc

```
 M src/alloc_core/size_classes.rs
 M tests/size_classes_lookup.rs
?? docs/checkpoints/2026-06-29-v020-released.md
?? run_tsan.sh
?? tests/regression_large_align_no_segment_exhaustion.rs
```

```
b14f83c release: 0.2.0 — SeferMalloc → SeferAlloc rename
f22c034 docs(readme): restructure top — Install / Basic usage / Configuration upfront
e4d1e32 ci: add release workflow — tag-based + manual crates.io publishing
d95ea7f fix(vmem): MAP_ANON differs across BSD (0x1000) vs Linux (0x20)
6c96ec8 style: cargo fmt --all (CI rustfmt gate)
```

### shamir-db

```
 M Cargo.lock
 M crates/shamir-server/Cargo.toml
?? .cargo-target-bench/
?? docs/perf/sefer-alloc-0.2.0-oom-bug-2026-06-30.md
```

```
29a58c37 deps(server): mimalloc → sefer-alloc 0.2.0 (native-Rust global allocator)
2989117a perf(index): N3-N4 SortedIndexManager ArcSwap → NodeReplicated (NUMA Фаза 2)
5a2ac0e4 perf(index): N3 IndexInfo ArcSwap → NodeReplicated (NUMA Фаза 2)
```

(shamir-db Cargo.toml патч локальный path override; вернуть на
`version = "0.2.1"` после релиза fix'а.)
