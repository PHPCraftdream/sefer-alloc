# B4-MT финал: X-notown (NO-GO) и X-pin (NO-GO) — отчёт о чужих free и heap-route pin-кэше

**Дата:** 2026-10-20. **Ветка:** `b4-mt` (base A = `origin/main` @ `2730d02a5824128804f847d006206bda75a9fc84`). **Коммиты нет:** все замеры выполнены на некоммитнутом worktree-diff, идентичность фиксируется хэшем `git diff | sha256sum` до и после каждого замера (см. таблицу идентичности ниже — вывод скрипта). `src/`-состояние финала = X-pin (a)+(б); X-notown полностью откачен (`git checkout -- .`, тест-файл удалён), остался только сырой лог.

Все таблицы этого файла — дословная вставка вывода `node scripts/b4_mt_pin_notown_table.mjs` (парсит сырые логи, включая `_raw_b4_mt_xp4_tls.log`, ничего руками не транскрибировано; внутри скрипта — assert'ы арифметики). Каждое число в прозе помечено **ИЗМЕРЕНО** или **ОЦЕНКА**.

> **Статус: X-pin GO §5 отменён — см. §11.**

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

### Исправления по ревью (mt-xp3, финальное состояние) vs base A и xp2

| судья | base A | xp2 (f0c0c623) | mt-xp3 | Δ (к base A) | порог | вердикт |
|---|---|---|---|---|---|---|
| foreign free (locality), Ir/free | 136 | 122 | 120 | −16 (−11.8%) | — | PASS |
| foreign capacity, Ir/free | 952 | 122 | 883 | −69 (−7.2%) | — | PASS |
| own free (контроль), Ir/free | 166 | 167 | 167 | +1 (+0.6%) | ≤ +1 Ir | PASS |
| larson T=1, Ir/op | 300 | 300 | 300 | +0 (+0.0%) | ≤ +0.5% | PASS |
| mstress T=1, Ir/op | 415 | 415 | 415 | +0 (+0.0%) | ≤ +0.5% | PASS |
| thrash64 w_alloc, Ir/op | 26 | 26 | 26 | +0 (+0.0%) | ≤ +2 Ir | PASS |
| thrash64 w_free, Ir/op | 103 | 103 | 103 | +0 (+0.0%) | ≤ +2 Ir | PASS |
| RSS-проба (блокер 2), Δ idle RSS | — | — | -20 KiB (-0.02 MiB) | A=14424 KiB / B=14404 KiB | ≤ +1024 KiB | PASS |
| foreign locality pin hit_frac | 0.9999 | — | 0.9999 | — | ≥ 0.95 | PASS |

### mt-xp4 (base A snapshot, production∧internals∧bench-internals): TLS route-pin-cache hits/misses, потолок выгоды X-pin и цена промахов

Идентичность источника (ИЗМЕРЕНО скриптом из первой строки лога): snapshot commit 2730d02a5824128804f847d006206bda75a9fc84; tar sha256 711008843e114e276a5713859a789c0603aea756fa5b2cd41534ac1cbd81e74d; probe sha256 0c7c1ea7aa52de448e9728df62067765b91a6a3eddb83aafe881a3b4d14bf081.
steps=100000/поток и T — ИЗ ЛОГА (конфигурационная строка обёртки «steps=… workload=… T=…»; проба захардкоживает working_set 768 / mstress_blocks 512 — записаны в той же строке как конфигурация запуска); экономия на чужой free — ИЗМЕРЕНО из судейских логов: 14 Ir (136−122, mt-xp2) и 16 Ir (136−120, mt-xp3); цена промаха 830 Ir — ИЗМЕРЕНО (952−122, capacity, base/xp2-логи).
 larson/mstress T=1 «flat» ничего не доказывают — при T=1 чужих free в трассе нет; все строки ниже — T≥2.

| трасса | T | hits | misses | hit_frac | чужие free/step = (hits+misses)/(steps·T) | потолок выгоды [ОЦЕНКА], Ir/step = foreign/step·14 (xp2) | % от Ir/op (база) | потолок [ОЦЕНКА], Ir/step = foreign/step·16 (xp3) | % от Ir/op (база) | потери на промахах [ОЦЕНКА], Ir/step = misses·830/(steps·T) | % от Ir/op (база) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| larson | 2 | 12497 | 3 | 0.9998 | 0.06250 = (12497+3)/(100000·2) | 0.8750 = 0.06250·14 — ОЦЕНКА | 0.2917% от 300 | 1.0000 = 0.06250·16 — ОЦЕНКА | 0.3333% от 300 | 0.01245 = 3·830/(100000·2) — ОЦЕНКА | 0.0042% от 300 |
| larson | 4 | 24985 | 15 | 0.9994 | 0.06250 = (24985+15)/(100000·4) | 0.8750 = 0.06250·14 — ОЦЕНКА | 0.2917% от 300 | 1.0000 = 0.06250·16 — ОЦЕНКА | 0.3333% от 300 | 0.03112 = 15·830/(100000·4) — ОЦЕНКА | 0.0104% от 300 |
| mstress | 2 | 9860 | 3 | 0.9997 | 0.04931 = (9860+3)/(100000·2) | 0.6904 = 0.04931·14 — ОЦЕНКА | 0.1664% от 415 | 0.7890 = 0.04931·16 — ОЦЕНКА | 0.1901% от 415 | 0.01245 = 3·830/(100000·2) — ОЦЕНКА | 0.0030% от 415 |
| mstress | 4 | 19513 | 15 | 0.9992 | 0.04882 = (19513+15)/(100000·4) | 0.6835 = 0.04882·14 — ОЦЕНКА | 0.1647% от 415 | 0.7811 = 0.04882·16 — ОЦЕНКА | 0.1882% от 415 | 0.03112 = 15·830/(100000·4) — ОЦЕНКА | 0.0075% от 415 |

Вердикт: ОЦЕНКА выгоды X-pin по измеренным частотам чужих free и измеренной цене попадания — доли процента Ir/step (максимум 0.3333% Ir/op, все точки ≪ порога принятия mt-bench +2%); потери на промахах — максимум 0.01038% Ir/op (ОЦЕНКА). callgrind не отражает когерентность кэшей и wall-clock. **X-pin NO-GO** против порога +2% (см. §11 отчёта).

<!-- /AUTO -->
```

## 4. X-notown — вердикт NO-GO

**Механизм (итерация mt-xn, затем mt-xn2):** 256-битный фильтр `notown_filter: [u64; 4]` в `SegmentTable`, бит = `(base >> 22) & 255`; установка в `hash_insert_identity` (единственная точка вставки ключа), полная перестройка в `hash_remove`, сборка из слотов в `from_primordial`; гейт в хвосте Tier-1 промаха `canonical_base_of_mut` ПЕРЕД `hash_find`. Счётчик `NOTOWN_FILTER_REJECTS` (Relaxed, internals ∧ bench-internals). Дизайн: sol_ox.md §7.

**Вердикт NO-GO (ИЗМЕРЕНО):** единственный драйвер — thrash64 w_free **108 Ir/op против базовых 103, +5 Ir > +2 Ir допуска** (ИЗМЕРЕНО); mt-xn2 (гейт-микрооптимизация: word/bit из независимых битовых диапазонов адреса, diff `003391ac…`) не изменил число (ИЗМЕРЕНО, оба прогона 108). При этом выигранная ось foreign −7 Ir (−5.1%, ИЗМЕРЕНО) и собственный/ларсон/мстресс — ровно flat (ИЗМЕРЕНО). Оракул нот-владельца: 100000 отклонений на 100000 foreign free = 1.0 (ИЗМЕРЕНО, ≥95% PASS) — фильтр работает, он просто дорог на OWN-хвосте. Стоимость чисто инструкционная: D1m/op = 0.88 без изменений (ИЗМЕРЕНО); верхняя строка профиля own-free — сам гейт в `hash.rs`, 4.7 (ИЗМЕРЕНО), own_cache hit-rate на 64-сегментном трейсе ~25% (ОЦЕНКА из лога).

**Дизайн-остаток (триггер для следующей попытки):** гейтить ТОЛЬКО не-владельца региона адресного пространства (per-heap foreign-bias flag / «gating only threads that are not the owner of the address space region») — убрать per-miss работу гейта с OWN-пути; это дизайн-изменение за пределами одной итерации, поэтому честный NO-GO. Mutant-контроль: инвертированный гейт → тест красный (MUTANT_EXIT=101, ИЗМЕРЕНО), откат назад, diff вернулся к `43c72c4f…` (ИЗМЕРЕНО).

## 5. X-pin — вердикт GO (ОТМЕНЁН, см. §11)

**Механизм `heap-route-pin-cache`:** кэш pin'ов на уровне HeapCore для foreign free — 16 direct-mapped слотов (`HEAP_PIN_SLOTS = 16`, `src/registry/heap_core_xthread/heap_route_pin_cache.rs:66` — ИЗМЕРЕНО строкой файла) с жёстким лимитом живых pin'ов `MAX_LIVE_PINS = 4` (там же :70; sweep на cold-пути + round-robin live eviction, :194–205) и `release_all` в `trim_for_recycle`. RSS-удержание ≈ 4 × 0.29 МиБ ≈ **1.16 МиБ** (ОЦЕНКА из оценки retention в route_pin_cache-комментарии/логе: 1 pin ≈ 0.29 МиБ при до 1024 class-leaves × 256 AtomicU8) — в пределах бюджета +1 МиБ (16 слотов без кэпа дали бы ≈ 4.6 МиБ worst-case — ОЦЕНКА). Вся foreign-нога вынесена в `#[cold] fn publish_foreign_heap` (`src/registry/heap_core_xthread/routing.rs:41–43`, вызов на :23–28) — это и есть итерация mt-xp2, исправившая проваленные mstress/thrash64 итерации 1 (см. таблицу сверхсённой итерации: mstress 419 (+0.96% > +0.5% FAIL), thrash64 w_free 106 (+3 > +2 FAIL) — ИЗМЕРЕНО). Компонент (б): `Entry` = `#[repr(C, align(64))]` (`src/registry/segment_route/directory.rs:26–27`), refs строго во второй кэш-линии @64 (layout-тест `entry_layout`, `src/registry/segment_route/pin.rs:160–186`; size_of Entry = 128). Модуль подключён в `src/registry/heap_core_xthread/mod.rs:7`; поля кэша в `HeapCore` (`src/registry/heap_core/core.rs`, ханки @172/@259/@307/@345); own-путь-гвардии в `src/registry/heap_core/state/ownership.rs` (@24) и `src/registry/heap_slot.rs` (@353/@365). Бюджет стека: `size_of::<HeapCore>()` = 8392 Б при бюджете 10240 (headroom 1848 Б) — ИЗМЕРЕНО (cargo test `r34_18_heap_core_stack_pressure_pin`, бюджет поднят 9216→10240 в `tests/r34_18_heap_core_stack_pressure_pin.rs` и core.rs).

**Оракулы:** pin hit_frac 0.9999 на foreign locality (ИЗМЕРЕНО); capacity-трейс — все 100000 free опубликованы, hit_frac 0.9999 при 16-слотовом direct-mapped кэше (ИЗМЕРЕНО; честная заметка лога: frees группируются по базе лучше, чем ожидалось; base A был 0.0000). Итоговые вердикты: foreign −14 Ir (−10.3%), fcap −830 Ir (952→122), own +1 (ровно на гейте ≤ +1, без запаса — честно), larson/mstress/thrash64 — все flat (ИЗМЕРЕНО) → **GO**.

**Верификация (финальное состояние):** loom `loom_b4_r1_pin_cache` 4 passed, `loom_route_pin_unlink` 2 passed (ИЗМЕРЕНО локально, CARGO_TARGET_DIR=.rush/cargo-target-M-loom, `--cfg loom`); cargo test: `b4_r1_pin_cache` 7, **`b4_xp_heap_pin_cache` 3 (новый, `tests/b4_xp_heap_pin_cache.rs`)**, `dealloc_only_no_bind` 2, `dealloc_only_no_bind_torn` 2, r34_18 1, `entry_layout` 1 — все passed, 0 failed (ИЗМЕРЕНО). Clippy -D warnings (production; production,internals,bench-internals; --lib --tests --benches), verify-structure-limits OK (directory.rs ровно 1000 строк), rustfmt по всем изменённым — ИЗМЕРЕНО.

## 6. Сырьё и сырые логи

- `docs/perf/_raw_b4_mt_base.log` — base A (85 строк).
- `docs/perf/_raw_b4_mt_xn.log` — X-notown + mt-xn2 + NOTOWN-оракул + revert (106 строк).
- `docs/perf/_raw_b4_mt_xp.log` — X-pin, итерации 1 и mt-xp2, оракулы, верификация (84 строки).
- `docs/perf/_raw_b4_mt_xp3_f.log`, `_raw_b4_mt_xp3_fcap.log`, `_raw_b4_mt_xp3_own.log`, `_raw_b4_mt_xp3_larson.log`, `_raw_b4_mt_xp3_mstress.log`, `_raw_b4_mt_xp3_thrash64.log` — судейский набор mt-xp3 (diff `751f1ad2…`).
- `docs/perf/_raw_b4_mt_xp3_rss.log` — RSS-проба (блокер 2), Δ −20 KiB PASS.
- `docs/perf/_raw_b4_mt_xp3_b1.log`, `docs/perf/_raw_b4_mt_xp3_b1b.log` — повторные контрольные пробы mt-xp3.
- `docs/perf/_raw_b4_mt_xp4_tls.log` — mt-xp4v3 (R30-8): подлинный сырой лог решающего замера на base A snapshot; первая часть лога — идентичность источника, вычисленная скриптом-обёрткой `mt_xp4_tls_v2.sh` (sha256 tar-файла `mt-xp3-rss-a.tar` и файла пробы `mt_xp4_tls_probe.rs`; все /mnt-пути санитизированы до `$REPO_WSL` САМИМ скриптом), вывод tee-ится ВНУТРИ WSL (`~/ab/mt-xp4v3-tls.log`), затем копируется в репозиторий как есть. Перед каждым запуском пробы обёртка печатает конфигурационную строку `steps=<N> workload=<w> T=<t>` (working_set 768 / mstress_blocks 512 захардкожены ВНУТРИ пробы — записаны в той же строке комментарием как конфигурация запуска); `set -euo pipefail` — exit-код обёртки есть exit-код пробы.
- Исходники проб и обёрток, породившие цитируемые логи: `docs/perf/b4_mt_probes/` (README перечисляет, какими командами снят каждый лог).
- Сводный CSV сгенерирован скриптом: `docs/perf/B4_MT_PIN_NOTOWN_summary.csv` (74 строки данных; статусы ИЗМЕРЕНО / ОЦЕНКА (вычислено) — счётчики hits/misses/hit_frac ИЗМЕРЕНО, потолок выгоды и потери на промахах ОЦЕНКА).
- Таблицы: `node scripts/b4_mt_pin_notown_table.mjs` (assert'ы арифметики внутри; `--csv` пишет файл).

Честные ограничения (ИЗМЕРЕНО как отсутствие): все Ir — callgrind T=1 детерминированные; wall-clock и MT-когерентность не мерились; бинарные SHA256 per-binary не снимались; own=167 без запаса до гейта.

## 7. CI-команды для оркестратора (НЕ запускались локально)

1. **iai A/B** (callgrind-оси base A vs X-pin diff): iai-callgrind bench-набор, base = `2730d02a`, head = worktree-diff; сверить foreign/own/larson/mstress/thrash64 marginal Ir с таблицами §3.
2. **mt-bench wall-clock T=1/2/4, два прогона** на Linux и на Windows (MT-когерентность — единственная не измеренная ось).
3. **Miri:** ячейка `b4_r1_pin_cache_miri` (`tests/b4_r1_pin_cache_miri.rs`).
4. **loom** уже зелёный локально (§5); CI-прогон для фиксации не требуется повторно, но допускается.

## 8. Риски

- **Loom-gap (закрыт: бывший item 248, повествование в `docs/correctness-open-items/RESOLVED.md`; lookup в `docs/CORRECTNESS_OPEN_ITEMS.md`):** модель `loom_b4_r1_pin_cache` покрывает TLS-путь pin-кэша; heap-ветка (`heap_route_pin_cache.rs`) loom-модели НЕ имела и была удалена вместе с X-pin-откатом.
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

Перф-исход: оба пункта (X-notown NO-GO с дизайн-триггером «гейтить только не-владельца региона»; X-pin NO-GO — потолок выгоды, §11) закрыты карточками 105/106 в `docs/perf/OPEN_ITEMS.md` (полная история закрытия 106 — в `docs/perf/OPEN_ITEMS_ARCHIVE.md`, секция A106). Verification-gap heap-ветки — карточка 248 закрыта, повествование в `docs/correctness-open-items/RESOLVED.md`.


## 11. Исправления по ревью: СТОП-находка второго ревью и NO-GO X-pin (mt-xp4, 2026-10-20)

**Основание NO-GO — главное: ПОТОЛОК ВЫГОДЫ.** Доля чужих free на step при T≥2 = (hits+misses)/(steps·T) по mt-xp4: 0.0488–0.0625 (ИЗМЕРЕНО). Умноженная на измеренную экономию X-pin на чужой free (14 Ir — mt-xp2, 16 Ir — mt-xp3; ИЗМЕРЕНО) это даёт потолок выгоды **0.68–1.00 Ir/step = 0.16–0.33% Ir/op** larson (300) / mstress (415) — на порядок ниже порога принятия mt-bench (+2%). Это оценка выгоды по измеренным частотам чужих free и измеренной цене попадания микропробы; callgrind не отражает когерентность/wall-clock, поэтому это оценка, а не доказанный предел — её достаточно для NO-GO против порога +2%. Полная таблица с числителями/знаменателями у каждого числа — дословная вставка вывода скрипта ниже и в §3.

**Безопасность:** `Box` на пути чужого free ЗАПРЕЩЁН (реентрантная аллокация внутри free + детлок fallback-LockGuard — см. ниже); TLS-вариант даёт меньше выгоды (потолок тот же, а экономия достигается уже существующим R1-кэшем); HeapSlot-хранилище требует кодовых доказательств (никогда by-value на стеке; нет пересекающихся `&mut`; Send-аудит) при нулевой измеренной пользе. **Бюджет 9216:** headroom всего 8 Б под `--all-features` (r34_18: 9208/9216), любое поле в `HeapCore` бюджет ломает; подъём бюджета запрещён.

**Роль hit-rate R1 ≥ 0.999 — честная формулировка.** Высокий hit-rate доказывает, что ПОТЕРИ НА ПРОМАХАХ существующего кэша пренебрежимы (≤ 0.0104% Ir/op, см. таблицу) — он НЕ доказывает, что «кэш не нужен»: X-pin экономил цену ПОПАДАНИЯ в существующий R1-кэш (_publish_-путь), а не цену промаха. Правильная структура аргумента: (1) потолок выгоды (выше) показывает, что выигрывать почти нечего; (2) hit-rate/цена промаха показывает, что и терять нечего — текущее состояние не платит заметно ни за попадания в существующем масштабе, ни за промахи.

**Чего замер НЕ доказывает:** larson/mstress T=1 «flat» ничего не доказывают — при T=1 чужих free в трассе НЕТ, эти судьи в принципе не чувствуют X-pin ни в плюс, ни в минус. Решающая ось — только T≥2 (mt-xp4).

**Ход раунда:** итерация mt-xp3 (бюджет 9216 восстановлен выносом слотов в ленивый `Box<HeapPinSlots>`, судейский набор на diff `751f1ad2…` — все гейты PASS, собственный raw-лог сохранён) была ОТКЛОНЕНА второй ревью-находкой (сбой доставки сообщения): `Box::new` в `state_or_init` на пути чужого free ЗАПРЕЩЁН НАВСЕГДА — (a) аллокация через установленный SeferAlloc внутри аллокатора при живом `&mut HeapCore` (реентрантность/алиасинг, abort-on-OOM внутри free); (b) самоблокировка fallback: `with_fallback_heap → h.realloc → self.dealloc` (`src/registry/heap_core/free/realloc.rs:512`) → `Box::new` → `alloc_via_fallback` → `with_heap_config` → `LockGuard::acquire()` на уже взятой этим же потоком блокировке (`src/global/fallback.rs:539`, `src/global/sefer_alloc/core.rs:354-366`) — детлок. Любой `Box`/`Vec`/`alloc::*` через глобальный аллокатор на пути alloc/free запрещён; подъём бюджета 9216 запрещён.

### 11.1 Решающий замер mt-xp4 (R30-8) — сырой лог и таблица

Проба `mt_xp4_tls_probe.rs` (исходник в `docs/perf/b4_mt_probes/`; обёртка `mt_xp4_tls.sh`, перезапуск mt-xp4v3 — `mt_xp4_tls_v2.sh`: `set -euo pipefail`, tee ВНУТРИ WSL в `~/ab/mt-xp4v3-tls.log`, конфигурационные строки `steps=… workload=… T=…` и идентичность источника пишутся скриптом в первую часть лога; сборка production∧internals∧bench-internals — counters-only, Ir из неё НЕ берётся) на snapshot base A `2730d02a` (git archive, без X-pin): hit/miss СУЩЕСТВУЮЩЕГО R1 TLS route pin cache (`RoutePin::dbg_route_pin_cache_totals`) на реальных larson/mstress при T=2 и T=4 (steps=100000/поток — из лога). Подлинный сырой лог: `docs/perf/_raw_b4_mt_xp4_tls.log` (mt-xp4v3; hit_frac 0.9992–0.9998). Таблица ниже — дословная вставка вывода `node scripts/b4_mt_pin_notown_table.mjs` (таблица mt-xp4; steps/T парсятся скриптом ИЗ ЛОГА):

### mt-xp4 (base A snapshot, production∧internals∧bench-internals): TLS route-pin-cache hits/misses, потолок выгоды X-pin и цена промахов

Идентичность источника (ИЗМЕРЕНО скриптом из первой строки лога): snapshot commit 2730d02a5824128804f847d006206bda75a9fc84; tar sha256 711008843e114e276a5713859a789c0603aea756fa5b2cd41534ac1cbd81e74d; probe sha256 0c7c1ea7aa52de448e9728df62067765b91a6a3eddb83aafe881a3b4d14bf081.
steps=100000/поток и T — ИЗ ЛОГА (конфигурационная строка обёртки «steps=… workload=… T=…»; проба захардкоживает working_set 768 / mstress_blocks 512 — записаны в той же строке как конфигурация запуска); экономия на чужой free — ИЗМЕРЕНО из судейских логов: 14 Ir (136−122, mt-xp2) и 16 Ir (136−120, mt-xp3); цена промаха 830 Ir — ИЗМЕРЕНО (952−122, capacity, base/xp2-логи).
 larson/mstress T=1 «flat» ничего не доказывают — при T=1 чужих free в трассе нет; все строки ниже — T≥2.

| трасса | T | hits | misses | hit_frac | чужие free/step = (hits+misses)/(steps·T) | потолок выгоды [ОЦЕНКА], Ir/step = foreign/step·14 (xp2) | % от Ir/op (база) | потолок [ОЦЕНКА], Ir/step = foreign/step·16 (xp3) | % от Ir/op (база) | потери на промахах [ОЦЕНКА], Ir/step = misses·830/(steps·T) | % от Ir/op (база) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| larson | 2 | 12497 | 3 | 0.9998 | 0.06250 = (12497+3)/(100000·2) | 0.8750 = 0.06250·14 — ОЦЕНКА | 0.2917% от 300 | 1.0000 = 0.06250·16 — ОЦЕНКА | 0.3333% от 300 | 0.01245 = 3·830/(100000·2) — ОЦЕНКА | 0.0042% от 300 |
| larson | 4 | 24985 | 15 | 0.9994 | 0.06250 = (24985+15)/(100000·4) | 0.8750 = 0.06250·14 — ОЦЕНКА | 0.2917% от 300 | 1.0000 = 0.06250·16 — ОЦЕНКА | 0.3333% от 300 | 0.03112 = 15·830/(100000·4) — ОЦЕНКА | 0.0104% от 300 |
| mstress | 2 | 9860 | 3 | 0.9997 | 0.04931 = (9860+3)/(100000·2) | 0.6904 = 0.04931·14 — ОЦЕНКА | 0.1664% от 415 | 0.7890 = 0.04931·16 — ОЦЕНКА | 0.1901% от 415 | 0.01245 = 3·830/(100000·2) — ОЦЕНКА | 0.0030% от 415 |
| mstress | 4 | 19513 | 15 | 0.9992 | 0.04882 = (19513+15)/(100000·4) | 0.6835 = 0.04882·14 — ОЦЕНКА | 0.1647% от 415 | 0.7811 = 0.04882·16 — ОЦЕНКА | 0.1882% от 415 | 0.03112 = 15·830/(100000·4) — ОЦЕНКА | 0.0075% от 415 |

Вердикт: ОЦЕНКА выгоды X-pin по измеренным частотам чужих free и измеренной цене попадания — доли процента Ir/step (максимум 0.3333% Ir/op, все точки ≪ порога принятия mt-bench +2%); потери на промахах — максимум 0.01038% Ir/op (ОЦЕНКА). callgrind не отражает когерентность кэшей и wall-clock. **X-pin NO-GO** против порога +2% (см. §11 отчёта).

Вывод: hit-rate ≥ 0.95 на всех четырёх точках — довод о том, что потери на промахах пренебрежимы (0.0030–0.0104% Ir/op), а НЕ о том, что кэш не нужен; вместе с потолком выгоды (вступление §11) это и есть NO-GO. **Вердикт: NO-GO — X-pin (heap-route pin cache в любой форме) отменён.**

### 11.2 Сравнение вариантов хранения (цифрами)

| вариант | HeapCore Δ | foreign free Ir/free (ИЗМЕРЕНО) | структурные аргументы |
|---|---|---|---|
| lazy Box (mt-xp3, ОТКЛОНЁН ревью) | +8 Б (9216/9216 под all-features, headroom 0) | 120 (−16 к base) | ЗАПРЕЩЁН: реентрантная аллокация внутри free + детлок fallback-LockGuard; abort-on-OOM в free |
| TLS direct-mapped 16–32 (расширение route_pin_cache.rs) | **0 Б** | ~136 (база; при 0.999+ hit-rate улучшение ≤ доли Ir) | RefCell допустим, без нового unsafe, ForeignNoBind-ветка не меняется; НО решающий замер показывает: даже существующий 4-слотовый R1 даёт 0.9992–0.9998 — расширение не нужно |
| HeapSlot-хранилище (вне тела HeapCore) | 0 Б | ~120–124 (по mt-xp3 heap-кэшу) | требует кодовых доказательств (HeapSlot никогда by-value на стеке; нет пересекающихся &mut; Send-аудит) — цена доказательств при нулевой измеримой пользе |

Выбор: откат, ни один вариант не продолжается (NO-GO по п.3 протокола).

### 11.3 Откат (ручной, git write запрещён)

Все затронутые X-pin файлы восстановлены к `2730d02a` побайтово (`git show 2730d02a:path > path`): `core.rs`, `ownership.rs`, `heap_core_xthread/mod.rs`, `route_pin_cache.rs`, `routing.rs`, `heap_slot.rs`, `directory.rs`, `pin.rs`, `tests/r34_18_heap_core_stack_pressure_pin.rs`; удалены `heap_route_pin_cache.rs`, `tests/b4_xp_heap_pin_cache.rs`. Проверка: `git diff 2730d02a --stat -- src tests` ПУСТО (ИЗМЕРЕНО). Diff-хэш после отката: `61bc39ed…` (docs/raw-логи остаются, как в прецеденте X-notown). Верификация отката: `cargo check --all-features` OK; r34_18: 7872 Б (production-internals) / **9208 Б** (`--all-features`) — оба ≤ 9216; Loom оба теста зелёные; clippy 8 строк PER_PR_ROWS зелёные; verify-*.mjs зелёные (кроме pre-existing evidence-registry). Честный механизм evidence-registry RED: верификатор сравнивает src-scope строк с committed HEAD = `f0c0c623`, который ещё СОДЕРЖИТ X-pin src (M/D относительно `2730d02a` — это расхождение HEAD↔рабочей копии, а не «тот же RED на 2730d02a»); поэтому 9 evidence-src-stale RED-строк (R4a-01/02/05, R4c-02/03/05/06/07/09) объясняются содержимым HEAD, а проверка «совпадает ли с базой» будет валидной только ПОСЛЕ коммита оркестратором (тогда HEAD сравнится с чистым src).
