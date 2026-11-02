# WG-M0 — сколько стоит release сегмента внутри `free` на росте (Windows)

**Тип работы: `bench` — только измерение. Код sefer-alloc (shipping и opt-in) не менялся**: в судью `examples/g1_wild_judge.rs` добавлено плечо `sefer_pool` (тот же `SeferAlloc` с `SmallSegmentPoolConfig::new().pool_segments(1024).pool_byte_cap(4 << 30)`), поле `config_conflicts` в вывод, запуск и отчёт — отдельными скриптами. Задача #2489, карточка `docs/perf/OPEN_ITEMS.md` 146 (G1-WJ1), консультация @ox: `.rush/stdin/g1-wg1-review2.out`.

## 0. Вывод

1. **Release опустевших сегментов — главная часть остатка разрыва с mimalloc на росте, как только AO-1 убрал «пилу» выдачи.** Парная медиана `sefer/mi` → `sefer_pool/mi`: grow64 1.285 → 1.034, grow1k 1.182 → 0.800, regrow 1.464 → 1.147 (сборка с AO-1 v2b); доля разрыва, снимаемая отказом от release, — 0.882, 2.094 (пул обгоняет mi), 0.682 (определение — в заголовке последнего столбца таблицы).
2. **На базе (без AO-1) доля release меньше**: grow64 0.292, grow1k 0.710, regrow 0.624 — основная часть grow64 там порядок выдачи (AO-1, `docs/perf/AO1_ASCENDING_ISSUE.md`).
3. **growmix от release не зависит**: сегменты не освобождаются (`seg_released` 0..0 у обоих плеч sefer), `sefer_pool/mi` не лучше `sefer/mi` (база 1.309 против 1.299; AO-1 1.256 против 1.294). Разрыв growmix — локальность state byte при перемешанном free (#2491 WG-MLP), не release.
4. **Цена — RSS после полного free**: конец прогона sefer 29 МиБ (grow64/grow1k), `sefer_pool` 1175 МиБ, mi 1031 МиБ; regrow 72 / 669 / 572 МиБ. Пул уже настраивается пользователем (`SeferAlloc::with_config`), варианта `Profile` с такой удержкой нет (`SmallPoolPolicy` — `Default` и `Throughput` = 8 сегментов / 32 МиБ). **Включать ли удержку по умолчанию или вводить opt-in `Retain` — решение владельца по политике RSS**; этот отчёт даёт потолок выигрыша, не рекомендацию по умолчанию.

Kill-критерий @ox («отвергнуть гипотезу доминирования release, если измеренный release не объясняет разрыв»): гипотеза выдерживает для grow64 (после AO-1), grow1k, regrow; отвергнута для growmix и для базового grow64 (0.292).

## 1. Что измерено

- **Точка входа:** `GlobalAlloc`-слой `SeferAlloc` через `#[inline(never)]`-прокладки судьи (глобальный аллокатор процесса остаётся `System`, как в WJ-1); не `AllocCore`/`HeapCore`. Решение о политике удержки относится к тому же слою, на котором конфигурация задаётся пользователем.
- **Плечи (по одному процессу на запуск):** `sefer` (production-умолчание: пул 4 сегмента / 16 МиБ), `sefer_pool` (1024 сегмента / 4 ГиБ, потолок: release внутри `free` не происходит), `mi` (mimalloc).
- **Доказательства конфигурации (R26-4):** (1) запрошено 1024 / 4 ГиБ; (2) прочитать разрешённое значение через `dbg_pool_cap()` судья не может (нужен указатель на кучу), вместо этого — оракул активации механизма: **во всех запусках `sefer_pool` `seg_released` = 0, во всех запусках `sefer` = 268 (grow64/grow1k) и 140 (regrow)**; (3) `config_conflicts` = 0 во всех 288 запусках; (4) каждый запуск — отдельный процесс (подпроцессная изоляция, реестр пуст).
- **Сборки:** `--release --features "production internals"`, rustc 1.97.0. Две сборки: `base` — коммит `4cb465bc` (main `8462e42c` + судья); `ao1` — то же + `alloc_core_small_impl.rs` из ветки `g1-ao1` (`6967bf89`, AO-1 v2b), патч sha256 и sha256 обоих exe — `docs/perf/wgm0/ident.txt`.
- **План:** сценарии grow64, grow1k, regrow, growmix (масштаб 1, ≤1.4 ГиБ RSS на процесс); на сценарий 2 блока × все 6 порядков плеч = 12 запусков на плечо (позиционно сбалансировано), сценарии подряд. Статистики: «min» — минимум общего времени `ns_per_op × ops` по 12 запускам ячейки; «парная медиана» — медиана `плечо / mi` по 12 парам (тот же блок и порядок). Кампании `base` и `ao1` проведены в разное время на общей машине: абсолютные миллисекунды между ними не сравнивать (дрейф `mi` до 14 %: grow1k min 310 мс в `base` против 352 мс в `ao1`), сравнивать отношения внутри кампании.
- **Не измерено:** разложение времени на фазы grow/free внутри одного запуска (счётчики времени release лежали бы в `src/`; в черновой сборке с таймерами release занимал 112–135 мс на grow64 и 59–71 мс на regrow; в отчёт не входит — нет закоммиченного кода), хвосты задержек, MT.

## 2. Результаты

Таблицы получены `node scripts/wgm0_report.mjs` из `docs/perf/wgm0/raw_{base,ao1}.jsonl`, CSV — `docs/perf/WG_M0_RELEASE_ATTRIBUTION_summary.csv`. Значения «(sefer − sefer_pool) / (sefer − mi)» — доля разрыва sefer→mi в миллисекундах (min), снимаемая отказом от release; больше 1 — пул быстрее mi.

### base: main 8462e42c

| scenario | sefer min ms | sefer_pool min ms | mi min ms | sefer/mi | sefer_pool/mi | (sefer − sefer_pool) / (sefer − mi) | seg_released sefer | seg_released sefer_pool | end RSS MiB sefer / pool / mi |
|---|---|---|---|---|---|---|---|---|---|
| grow64 | 1206 | 1064 | 618 | 1.951 | 1.721 | 0.241 | 268..268 | 0..0 | 29 / 1175 / 1031 |
| grow1k | 513 | 357 | 310 | 1.654 | 1.152 | 0.767 | 268..268 | 0..0 | 29 / 1175 / 1030 |
| regrow | 345 | 266 | 242 | 1.424 | 1.097 | 0.772 | 140..140 | 0..0 | 72 / 669 / 572 |
| growmix | 531 | 496 | 401 | 1.323 | 1.237 | 0.266 | 0..0 | 0..0 | 1332 / 1332 / 1137 |

Paired median of arm/mi (same block and order, n = 12 pairs per cell), base:

| scenario | sefer/mi | sefer_pool/mi | (sefer/mi − sefer_pool/mi) / (sefer/mi − 1) |
|---|---|---|---|
| grow64 | 1.833 | 1.590 | 0.292 |
| grow1k | 1.667 | 1.193 | 0.710 |
| regrow | 1.425 | 1.160 | 0.624 |
| growmix | 1.299 | 1.309 | -0.033 |

### ao1: main 8462e42c + AO-1 v2b carve

| scenario | sefer min ms | sefer_pool min ms | mi min ms | sefer/mi | sefer_pool/mi | (sefer − sefer_pool) / (sefer − mi) | seg_released sefer | seg_released sefer_pool | end RSS MiB sefer / pool / mi |
|---|---|---|---|---|---|---|---|---|---|
| grow64 | 761 | 650 | 632 | 1.204 | 1.028 | 0.861 | 268..268 | 0..0 | 29 / 1175 / 1031 |
| grow1k | 393 | 264 | 352 | 1.118 | 0.750 | 3.121 | 268..268 | 0..0 | 29 / 1175 / 1030 |
| regrow | 416 | 294 | 278 | 1.497 | 1.061 | 0.878 | 140..140 | 0..0 | 72 / 669 / 572 |
| growmix | 570 | 550 | 462 | 1.234 | 1.191 | 0.182 | 0..0 | 0..0 | 1332 / 1332 / 1137 |

Paired median of arm/mi (same block and order, n = 12 pairs per cell), ao1:

| scenario | sefer/mi | sefer_pool/mi | (sefer/mi − sefer_pool/mi) / (sefer/mi − 1) |
|---|---|---|---|
| grow64 | 1.285 | 1.034 | 0.882 |
| grow1k | 1.182 | 0.800 | 2.094 |
| regrow | 1.464 | 1.147 | 0.682 |
| growmix | 1.294 | 1.256 | 0.130 |

Path-activation oracle: in every run of the `sefer_pool` arm seg_released == 0, and config_conflicts == 0 in all 288 runs.

## 3. Что дальше

- **Решение владельца:** политика удержки (opt-in `Retain` в `Profile` или документирование `with_config` для растущих куч). Выигрыш — потолок из таблиц выше; цена — RSS после полного free: (1175 − 1031) / 1031 = 14 % к mi на grow64/grow1k и 1175 МиБ вместо 29 МиБ у текущего sefer.
- **AO-1 (#153, HOLD)** остаётся предпосылкой: на базе release объясняет только 0.292 разрыва grow64.
- **Остаток после AO-1 + пула:** regrow 1.147 (повторное использование дырявой кучи), growmix 1.256–1.309 (локальность state byte; #2491 WG-MLP — предтест MLP без аллокатора), grow64 1.034.
- **Отдельно от release:** листья `ClassLeaves` (#2490 WG-LEAF).

## 4. Воспроизведение

```text
cargo build --release --example g1_wild_judge --features "production internals"   # коммит из ident.txt
node docs/perf/wgm0/run.mjs --exe <judge.exe> --label base --out docs/perf/wgm0/raw_base.jsonl
node scripts/wgm0_report.mjs
```
