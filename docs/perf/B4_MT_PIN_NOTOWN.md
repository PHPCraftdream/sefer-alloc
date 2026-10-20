# B4-MT финал: X-notown (NO-GO) и X-pin (GO) — отчёт о чужих free и heap-route pin-кэше

**Дата:** 2026-10-20. **Ветка:** `b4-mt` (base A = `origin/main` @ `2730d02a5824128804f847d006206bda75a9fc84`). **Коммиты нет:** все замеры выполнены на некоммитнутом worktree-diff, идентичность фиксируется хэшем `git diff | sha256sum` до и после каждого замера (см. таблицу идентичности ниже — вывод скрипта). `src/`-состояние финала = X-pin (a)+(б); X-notown полностью откачен (`git checkout -- .`, тест-файл удалён), остался только сырой лог.

Все таблицы этого файла — дословная вставка вывода `node scripts/b4_mt_pin_notown_table.mjs` (парсит три сырых лога, ничего руками не транскрибировано; внутри скрипта — assert'ы арифметики). Каждое число в прозе помечено **ИЗМЕРЕНО** или **ОЦЕНКА**.

## 1. Схема судьи `mt_foreign_free_probe`

Один и тот же судья (не менялся ни в одной фазе — ИЗМЕРЕНО по cmd-строкам всех логов):

1. **Owner alloc** — поток-владелец аллоцирует N блоков. Режим *locality*: классы 16/64/256, ≤4 сегментных баз. Режим *capacity*: класс 256 Б, >4 сегментных баз, free перемежается между базами.
2. **Worker own-heap warmup pre-window** — рабочий поток прогревает СВОЮ кучу (128× alloc/free) до окна, чтобы `current_for_dealloc = Own`, и `dealloc_routing` реально проходит ветку `owned_block_of`/`hash_find` (промах).
3. **Окно — только foreign free** — в замеряемом окне worker вызывает только `#[inline(never)] w_free` (`GlobalAlloc::dealloc`) на блоках владельца. Контроль *own*: worker пре-аллоцирует свои блоки до окна, в окне освобождает только их.
4. **Owner drain** после join (`trim_current_thread`).

Инструмент: callgrind `--toggle-collect='*zz_mt_foreign_free::w_free*'`, totals 50k/100k, marginal Ir/free; T=1 детерминированно. larson/mstress — через нетронутый `prof_win.sh`, thrash64 — через нетронутый `prof_thrash64_dir.sh`. **Оракулы** (≥95% порог активации) — отдельная bench-internals сборка: `SeferAlloc::dbg_b4_collapse_counters()[3]=B4_FOREIGN_PUBLISH_OK`, `[4]=B4_DIRECTORY_LOOKUPS`, `RoutePin::dbg_route_pin_cache_totals()` hits/misses; X-notown — счётчик `NOTOWN_FILTER_REJECTS`. Reported Ir всегда из counter-free production-сборки (ИЗМЕРЕНО, честная заметка base-лога).

## 2. Base A — идентичность

HEAD = `2730d02a5824128804f847d006206bda75a9fc84` (ИЗМЕРЕНО), diff пустой: sha256 = `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` (ИЗМЕРЕНО, canonical empty-sha256), до и после каждого замера. Инструмент: rustc 1.97.0 x86_64-pc-windows-msvc, release, `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`, WSL-callgrind. Честное ограничение (в обоих логах): бинарные SHA256 per-binary не снимались — идентичность держится на пустом/фиксированном diff-хэше + фиксированных snapshot-tar'ах на лейбл.

## 3. Таблицы (вывод скрипта)

```
<!-- AUTO: node scripts/b4_mt_pin_notown_table.mjs (paste of stdout) -->
### Судьи — X-notown (mt-xn, src откачен) vs base A

| судья | base A | xn | Δ | порог | вердикт |
|---|---|---|---|---|---|
| foreign free, Ir/free | 136 | 129 | −7 (−5.1%) | — | PASS |
| own free (контроль), Ir/free | 166 | 166 | +0 (+0.0%) | ≤ +1 Ir | PASS |
| larson T=1, Ir/op | 300 | 300 | +0 (+0.0%) | ≤ +0.5% | PASS |
| mstress T=1, Ir/op | 415 | 415 | +0 (+0.0%) | ≤ +0.5% | PASS |
| thrash64 w_alloc, Ir/op | 26 | 26 | +0 (+0.0%) | ≤ +2 Ir | PASS |
| thrash64 w_free, Ir/op | 103 | 108 | +5 (+4.9%) | ≤ +2 Ir | FAIL |
| **Итог X-notown** | — | — | — | — | **NO-GO** (thrash64 w_free) |

### Судьи — X-pin (mt-xp2, финал (a)+(б)) vs base A

| судья | base A | xp2 | Δ | порог | вердикт |
|---|---|---|---|---|---|
| foreign free, Ir/free | 136 | 122 | −14 (−10.3%) | — | PASS |
| foreign capacity, Ir/free | 952 | 122 | −830 (−87.2%) | — | PASS |
| own free (контроль), Ir/free | 166 | 167 | +1 (+0.6%) | ≤ +1 Ir | PASS |
| larson T=1, Ir/op | 300 | 300 | +0 (+0.0%) | ≤ +0.5% | PASS |
| mstress T=1, Ir/op | 415 | 415 | +0 (+0.0%) | ≤ +0.5% | PASS |
| thrash64 w_alloc, Ir/op | 26 | 26 | +0 (+0.0%) | ≤ +2 Ir | PASS |
| thrash64 w_free, Ir/op | 103 | 103 | +0 (+0.0%) | ≤ +2 Ir | PASS |
| **Итог X-pin** | — | — | — | — | **GO** |

### Сверхсённая итерация 1 (mt-xp-a/ab, gates FAILED — только для записи)

| судья | base A | итерация 1 | Δ | порог | вердикт |
|---|---|---|---|---|---|
| foreign free, Ir/free | 136 | 127 | −9 (−6.6%) | — | PASS |
| own free, Ir/free | 166 | 167 | +1 (+0.6%) | ≤ +1 Ir | PASS |
| larson T=1, Ir/op | 300 | 299 | −1 (−0.3%) | ≤ +0.5% | PASS |
| mstress T=1, Ir/op | 415 | 419 | +4 (+1.0%) | ≤ +0.5% | FAIL |
| thrash64 w_alloc, Ir/op | 26 | 26 | +0 (+0.0%) | ≤ +2 Ir | PASS |
| thrash64 w_free, Ir/op | 103 | 106 | +3 (+2.9%) | ≤ +2 Ir | FAIL |

### Оракулы (bench-internals, порог ≥ 95%)

| оракул | base A | xn | xp2 | порог | вердикт |
|---|---|---|---|---|---|
| foreign locality pin hit_frac | 0.9999 | 0.9999 | 0.9999 | ≥ 0.95 | PASS |
| foreign capacity: base publish hit_frac (100% uncached) | 0 | — | 0.9999 | publish 100% | PASS |
| notown filter rejects | — | 100000/100000 = 1.0000 | — | ≥ 0.95 | PASS |

### Идентичность (git diff | sha256sum, до/после каждого замера)

| состояние | diff sha256 |
|---|---|
| base A (пустой diff) | e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 |
| mt-xn implementation (pre-mutant) | 43c72c4f1b80db6a9770e8e5f177697c4d385f03acbe839686769f215b44aa22 |
| mt-xn2 (gate micro-opt) | 003391ac5f4edbad2c2821a5a004e5072648a65e4203ac5f4cb4a0e65ee28c13 |
| mt-xp2 (a) only, PRE | d43222b0a9533184210fab73d0a117b602fb5ecd1571cb8ecc1129c4a8cce778 |
| mt-xp2 (a)+(b), PRE | 445ade2bf73002f514cb72d0800ab32bc549dad76e20ac457b035783ee3cb5cb |
| HEAD (base A) | 2730d02a5824128804f847d006206bda75a9fc84 |
<!-- /AUTO -->
```

## 4. X-notown — вердикт NO-GO

**Механизм (итерация mt-xn, затем mt-xn2):** 256-битный фильтр `notown_filter: [u64; 4]` в `SegmentTable`, бит = `(base >> 22) & 255`; установка в `hash_insert_identity` (единственная точка вставки ключа), полная перестройка в `hash_remove`, сборка из слотов в `from_primordial`; гейт в хвосте Tier-1 промаха `canonical_base_of_mut` ПЕРЕД `hash_find`. Счётчик `NOTOWN_FILTER_REJECTS` (Relaxed, internals ∧ bench-internals). Дизайн: sol_ox.md §7.

**Вердикт NO-GO (ИЗМЕРЕНО):** единственный драйвер — thrash64 w_free **108 Ir/op против базовых 103, +5 Ir > +2 Ir допуска** (ИЗМЕРЕНО); mt-xn2 (гейт-микрооптимизация: word/bit из независимых битовых диапазонов адреса, diff `003391ac…`) не изменил число (ИЗМЕРЕНО, оба прогона 108). При этом выигранная ось foreign −7 Ir (−5.1%, ИЗМЕРЕНО) и собственный/ларсон/мстресс — ровно flat (ИЗМЕРЕНО). Оракул нот-владельца: 100000 отклонений на 100000 foreign free = 1.0 (ИЗМЕРЕНО, ≥95% PASS) — фильтр работает, он просто дорог на OWN-хвосте. Стоимость чисто инструкционная: D1m/op = 0.88 без изменений (ИЗМЕРЕНО); верхняя строка профиля own-free — сам гейт в `hash.rs`, 4.7 (ИЗМЕРЕНО), own_cache hit-rate на 64-сегментном трейсе ~25% (ОЦЕНКА из лога).

**Дизайн-остаток (триггер для следующей попытки):** гейтить ТОЛЬКО не-владельца региона адресного пространства (per-heap foreign-bias flag / «gating only threads that are not the owner of the address space region») — убрать per-miss работу гейта с OWN-пути; это дизайн-изменение за пределами одной итерации, поэтому честный NO-GO. Mutant-контроль: инвертированный гейт → тест красный (MUTANT_EXIT=101, ИЗМЕРЕНО), откат назад, diff вернулся к `43c72c4f…` (ИЗМЕРЕНО).

## 5. X-pin — вердикт GO

**Механизм `heap-route-pin-cache`:** кэш pin'ов на уровне HeapCore для foreign free — 16 direct-mapped слотов (`HEAP_PIN_SLOTS = 16`, `src/registry/heap_core_xthread/heap_route_pin_cache.rs:66` — ИЗМЕРЕНО строкой файла) с жёстким лимитом живых pin'ов `MAX_LIVE_PINS = 4` (там же :70; sweep на cold-пути + round-robin live eviction, :194–205) и `release_all` в `trim_for_recycle`. RSS-удержание ≈ 4 × 0.29 МиБ ≈ **1.16 МиБ** (ОЦЕНКА из оценки retention в route_pin_cache-комментарии/логе: 1 pin ≈ 0.29 МиБ при до 1024 class-leaves × 256 AtomicU8) — в пределах бюджета +1 МиБ (16 слотов без кэпа дали бы ≈ 4.6 МиБ worst-case — ОЦЕНКА). Вся foreign-нога вынесена в `#[cold] fn publish_foreign_heap` (`src/registry/heap_core_xthread/routing.rs:41–43`, вызов на :23–28) — это и есть итерация mt-xp2, исправившая проваленные mstress/thrash64 итерации 1 (см. таблицу сверхсённой итерации: mstress 419 (+0.96% > +0.5% FAIL), thrash64 w_free 106 (+3 > +2 FAIL) — ИЗМЕРЕНО). Компонент (б): `Entry` = `#[repr(C, align(64))]` (`src/registry/segment_route/directory.rs:26–27`), refs строго во второй кэш-линии @64 (layout-тест `entry_layout`, `src/registry/segment_route/pin.rs:160–186`; size_of Entry = 128). Модуль подключён в `src/registry/heap_core_xthread/mod.rs:7`; поля кэша в `HeapCore` (`src/registry/heap_core/core.rs`, ханки @172/@259/@307/@345); own-путь-гвардии в `src/registry/heap_core/state/ownership.rs` (@24) и `src/registry/heap_slot.rs` (@353/@365). Бюджет стека: `size_of::<HeapCore>()` = 8392 Б при бюджете 10240 (headroom 1848 Б) — ИЗМЕРЕНО (cargo test `r34_18_heap_core_stack_pressure_pin`, бюджет поднят 9216→10240 в `tests/r34_18_heap_core_stack_pressure_pin.rs` и core.rs).

**Оракулы:** pin hit_frac 0.9999 на foreign locality (ИЗМЕРЕНО); capacity-трейс — все 100000 free опубликованы, hit_frac 0.9999 при 16-слотовом direct-mapped кэше (ИЗМЕРЕНО; честная заметка лога: frees группируются по базе лучше, чем ожидалось; base A был 0.0000). Итоговые вердикты: foreign −14 Ir (−10.3%), fcap −830 Ir (952→122), own +1 (ровно на гейте ≤ +1, без запаса — честно), larson/mstress/thrash64 — все flat (ИЗМЕРЕНО) → **GO**.

**Верификация (финальное состояние):** loom `loom_b4_r1_pin_cache` 4 passed, `loom_route_pin_unlink` 2 passed (ИЗМЕРЕНО локально, CARGO_TARGET_DIR=.rush/cargo-target-M-loom, `--cfg loom`); cargo test: `b4_r1_pin_cache` 7, **`b4_xp_heap_pin_cache` 3 (новый, `tests/b4_xp_heap_pin_cache.rs`)**, `dealloc_only_no_bind` 2, `dealloc_only_no_bind_torn` 2, r34_18 1, `entry_layout` 1 — все passed, 0 failed (ИЗМЕРЕНО). Clippy -D warnings (production; production,internals,bench-internals; --lib --tests --benches), verify-structure-limits OK (directory.rs ровно 1000 строк), rustfmt по всем изменённым — ИЗМЕРЕНО.

## 6. Сырьё и сырые логи

- `docs/perf/_raw_b4_mt_base.log` — base A (85 строк).
- `docs/perf/_raw_b4_mt_xn.log` — X-notown + mt-xn2 + NOTOWN-оракул + revert (106 строк).
- `docs/perf/_raw_b4_mt_xp.log` — X-pin, итерации 1 и mt-xp2, оракулы, верификация (84 строки).
- Сводный CSV сгенерирован скриптом: `docs/perf/B4_MT_PIN_NOTOWN_summary.csv` (30 строк данных; статусы ИЗМЕРЕНО / ИЗМЕРЕНО (вычислено)).
- Таблицы: `node scripts/b4_mt_pin_notown_table.mjs` (assert'ы арифметики внутри; `--csv` пишет файл).

Честные ограничения (ИЗМЕРЕНО как отсутствие): все Ir — callgrind T=1 детерминированные; wall-clock и MT-когерентность не мерились; бинарные SHA256 per-binary не снимались; own=167 без запаса до гейта.

## 7. CI-команды для оркестратора (НЕ запускались локально)

1. **iai A/B** (callgrind-оси base A vs X-pin diff): iai-callgrind bench-набор, base = `2730d02a`, head = worktree-diff; сверить foreign/own/larson/mstress/thrash64 marginal Ir с таблицами §3.
2. **mt-bench wall-clock T=1/2/4, два прогона** на Linux и на Windows (MT-когерентность — единственная не измеренная ось).
3. **Miri:** ячейка `b4_r1_pin_cache_miri` (`tests/b4_r1_pin_cache_miri.rs`).
4. **loom** уже зелёный локально (§5); CI-прогон для фиксации не требуется повторно, но допускается.

## 8. Риски

- **Loom-gap (занесено в `docs/CORRECTNESS_OPEN_ITEMS.md` item 248 → `docs/correctness-open-items/TRACKED_verification_coverage.md`):** модель `loom_b4_r1_pin_cache` покрывает TLS-путь pin-кэша; **heap-ветка (`heap_route_pin_cache.rs`) loom-модели НЕ имеет** — она покрыта только юнит-тестами `tests/b4_xp_heap_pin_cache.rs` (3 passed). Это verification-coverage gap, не демонстрированная гонка.
- **own = 167 ровно на гейте** (+1 Ir, допуск исчерпан): любой будущий own-путь-прирост в `dealloc_routing`-хвосте, смежном с publish-гвардией, формально валит гейт — перепроверять при каждом изменении routing.rs.
- **RSS-оценка 1.16 МиБ — ОЦЕНКА** (производная от оценки retention 0.29 МиБ/pin, не прямой RSS-замер); wall-clock/RSS-ось не измерена (п. 7).
- `directory.rs` стоит ровно на 1000 строк (лимит структуры) — любое добавление требует либо переноса, либо отступа.

## 9. Ссылки на изменения (file:line)

- `src/registry/heap_core_xthread/heap_route_pin_cache.rs` — НОВЫЙ файл (222 строки): HEAP_PIN_SLOTS=16 (:66), MAX_LIVE_PINS=4 (:70), sweep+eviction (:194–205).
- `src/registry/heap_core_xthread/mod.rs:7` — объявление модуля.
- `src/registry/heap_core_xthread/routing.rs:23–28, 41–43` — foreign-гвардия + `#[cold] publish_foreign_heap` (mt-xp2).
- `src/registry/heap_core_xthread/route_pin_cache.rs:75–90` — счётчик-макрос, общий подсчёт TLS+heap pin-операций.
- `src/registry/segment_route/directory.rs:26–27` — `#[repr(C, align(64))] Entry`.
- `src/registry/segment_route/pin.rs:160–186` — layout-тест `entry_layout` (refs @64).
- `src/registry/heap_core/core.rs` (@172/@259/@307/@345) — поля/методы кэша + compile-time бюджет 10240.
- `src/registry/heap_core/state/ownership.rs` (@24) — own-путь-гвардии.
- `src/registry/heap_slot.rs` (@353/@365) — статические assert'ы инвариантов.
- `tests/b4_xp_heap_pin_cache.rs` — НОВЫЙ (177 строк, 3 теста).
- `tests/r34_18_heap_core_stack_pressure_pin.rs` — бюджет 9216→10240.

## 10. Open items

Перф-исход: оба пункта (X-notown NO-GO с дизайн-триггером «гейтить только не-владельца региона»; X-pin GO) закрыты карточками 105/106 в `docs/perf/OPEN_ITEMS.md`. Verification-gap heap-ветки — карточка 248 в correctness-индексе.
