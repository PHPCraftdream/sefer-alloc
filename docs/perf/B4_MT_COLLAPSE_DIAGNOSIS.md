# B4: диагностика бимодального коллапса MT larson (scan storm на refill-miss) и два минимальных исправления

Дата: 2026-10-18. Машина: Windows 10 владельца, i7-11800H 8C/16T; фоновая нагрузка — сборки других агентов в моменты замеров (синтетическая нагрузка НЕ добавлялась). Бинар примера: `examples/b4_collapse_probe` (larson, семантика `malloc-bench-rs`, WORK-time измерение PASS), `CARGO_TARGET_DIR=D:/dev/rust/sefer-alloc/.rush/cargo-target-A`. База: `6b166a7bb2e763f4db7ce0325746024eadaa9ddd` + рабочее дерево (см. §Идентификация). Порог коллапса: ops/s < 10M при T=4 (наблюдаемая бимодальность: 0.54M против 26M+; промежуточных значений нет).

## 0. Резюме

- Бимодальный 100× коллапс MT larson — это ШТОРМ sidecar-сканов на пути обнаружения при refill-miss: в коллапсированных прогонах `find_segment_with_free` вызывается 119,875 раз за проход (норма 550–674) и каждый вызов обходит sidecar ВСЕХ активных small-сегментов.
- Сгоревшее время — побайтовый проход `pending`-слов: `b4_scan_words` = 517,956,323 за коллапсированный проход против 1.05–1.37M в норме (≈380×); при старом безусловном `pending.swap(0, AcqRel)` это 518M RMW ≈ всё время прохода (2.5–3.1s).
- Гипотеза «конвой на shard-lock» ОПРОВЕРГНУТА: `b4_shard_lock_spins` в коллапсе 297–1,493 — тот же порядок, что в норме (292–364); согласуется с опровержением b4-r2-shardlock.
- Гипотезы «fallback-шторм», «шторм резервирований сегментов», «page-fault шторм», «потерянные/необработанные foreign-free» ОПРОВЕРГНУТЫ нулевыми/порядково-равными счётчиками (§2).
- Два минимальных исправления — (1) load-guarded swap в `BitmapScan::next_cut` (RMW только для ненулевого слова), (2) «сначала бин-голова, drain только при пустом бине» в `linear_scan` — убрали коллапс в той же серии: 0/48 коллапсов (было 10/48), минимум warm-серии поднялся с 0.65M до 35.85M.
- ЧАСТОТА триггера (массовый refill-miss → discovery) НЕ устранена: в counters-сборке `b4_find_segment_calls` до сих пор всплескивает до 368,604/проход — но после исправлений это стоит только чтений bin-головы, без проходов. Открытый пункт (§7a).
- Все новые инструменты (7 счётчиков, `dbg_b4_collapse_counters`, пример-проба) — под `internals`/`bench-internals`, продакшн-поверхность не тронута.

## 1. Воспроизведение

Команды калибровки (харнесс владельца):
```
cargo run --release --example malloc_macro --features "alloc-global alloc-xthread"
```
→ larson T=1 19.13M / T=2 31.61M / T=4 53.77M ops/s (нормальная полоса того же дня).

Проба (серии ниже):
```
cargo build --release --example b4_collapse_probe --features "alloc-global alloc-xthread"            # plain
cargo build --release --example b4_collapse_probe --features "alloc-global alloc-xthread internals bench-internals"  # counters
b4_collapse_probe.exe --sweep 4 --steps 400000 --workload larson --window-ms 100        # «cold» (T=4 первым)
b4_collapse_probe.exe --sweep 1,2,4 --steps 400000 --workload larson --window-ms 100    # «warm» (T=1,2,4)
```
Счётчик коллапса: PASS t=4 ops_per_sec < 10M.

| серия | сборка | прогонов | min / median / max (M ops/s) | коллапсов (<10M) | деградированных (10–20M) |
|---|---|---|---|---|---|
| s2 (ДО исправлений) | pre-fix plain | 24 cold | 3.12 / 46.39 / 59.70 | 2 (9.21; 3.12) | 0 |
| s2 (ДО исправлений) | pre-fix plain | 24 warm | 0.65 / 44.81 / 63.76 | 3 (3.19; 0.65; 3.01) | 5 (13.48; 17.73; 13.55; 12.01; 10.35) |
| s3 (ДО исправлений) | pre-fix counters | 60 (warm+cold) | 0.54 / — / 63+ | 20 (0.54–6.74M) | 5 (10–20M) |
| s5 (ПОСЛЕ фикса 1) | fix1 plain | 24 cold | 29.11 / 49.52 / 53.26 | 0 | 0 |
| s5 (ПОСЛЕ фикса 1) | fix1 plain | 24 warm | 25.36 / 47.38 / 60.03 | 0 | 0 |
| s6 (ПОСЛЕ фикса 1) | fix1 counters | 10 warm | 4.82 / — / — | шторм частоты остался: 133,352 find_segment_calls, 584,007,469 scan_words за проход @4.82M ops/s | — |
| s7 (ПОСЛЕ фиксов 1+2) | fix1+2 plain | 24 cold | 46.47 / 51.29 / 61.03 | 0 | 0 |
| s7 (ПОСЛЕ фиксов 1+2) | fix1+2 plain | 24 warm | 35.85 / 46.12 / 65.23 | 0 | 0 |
| s7 (ПОСЛЕ фиксов 1+2) | fix1+2 counters | 12 warm | 23.03 / 27.14 / 30.05 | 0; b4_scan_words ≤ 4,071,583 (−143×), b4_scan_calls ≤ 1,093; b4_find_segment_calls до 368,604 | — |

Медианы в таблице — интерполированные по 24 значениям `ops_per_sec_x100` из `RESULT_JSON` каждого лога; машинный срез — `docs/perf/B4_MT_COLLAPSE_DIAGNOSIS_summary.csv` (156 строк данных), сгенерирован скриптом `D:/dev/rust/sefer-alloc/.rush/b4-collapse/make_b4_csv.mjs` (`node D:/dev/rust/sefer-alloc/.rush/b4-collapse/make_b4_csv.mjs` — парсит RESULT_JSON всех s2/s5/s7 логов, без ручного ввода значений).

Сырые копии логов (каждая ≤20KB, в `docs/perf/`, вырезаны из `.rush/b4-collapse/`): `_raw_b4_prefix_series_warm.log`, `_raw_b4_prefix_series_cold.log`, `_raw_b4_collapsed_counters.log` (= s4_warm_06), `_raw_b4_normal_counters.log` (= s4_warm_14), `_raw_b4_fix1_storm_counters.log` (= s6_warm_01), `_raw_b4_final_series_warm.log`, `_raw_b4_final_series_cold.log`, `_raw_b4_final_series_counters.log`.

## 2. Счётчики

DELTAS за один T=4 проход (1.67–1.70M ops) из CNT-строк пробы. Коллапсированные: s4_warm_06 (0.67M) / s4_warm_02 (3.21M) / s4_warm_15 (4.96M); нормальные: s4_warm_11 (31.97M) / s4_warm_14 (31.50M) / s4_warm_09 (28.10M).

| счётчик (delta/проход) | норма (27–32M) | коллапс (0.54–6.7M) | источник |
|---|---|---|---|
| b4_find_segment_calls | 550–674 | 17,264–119,875 (в самом глубоком 119,875; 30–240×) | s4_warm_11/14/09 vs s4_warm_06/02/15 |
| b4_scan_calls | 552–674 | 17,307–132,546 | те же |
| b4_scan_words | 1,050,348–1,374,861 | 65,980,112–517,956,323 (в глубоком ≈380×; ≈2.3–2.6s из 2.5s прохода) | те же |
| b4_shard_lock_spins | 292–364 | 297–1,493 (тот же порядок — НЕ механизм) | те же |
| b4_directory_lookups | ровно 100,000 | ровно 100,000 (= steps×4/16, фиксировано workload'ом) | те же |
| b4_foreign_publish_ok | ровно 100,000 | ровно 100,000 | те же |
| proc_cycles | 0.485–0.516G | 1.66–6.11G | те же |
| proc_user_ms | 187–250 | 719–2,641 (≈100% CPU; wall ≈ user+kernel) | те же |
| proc_page_faults | 1,032–1,385 | 2,184–2,463 (тот же порядок — не fault-шторм) | те же |
| segments_reserved_total | +2 | +3..+4; segments_released delta = 0 | те же |
| sidecar_ingress_drain_calls | 6 | 6 (records_consumed 3,926–11,408 vs 4,415–9,048 — тот же порядок) | те же |
| fallback_lock_acquisitions | 0 | 0 | все логи s4 |
| foreign_or_unroutable_frees | 0 | 0 | все логи s4 |
| claims_* (hint/bump/free_set/cap_scan/recovery) | идентично | идентично | все логи s4 |

## 3. Механизм (текущие номера строк проверены чтением)

Цепочка при refill-miss (владелец не нашёл свободный блок класса):

1. `AllocCore::find_segment_with_free` — `src/alloc_core/small/alloc_core_small/find_segment.rs:113` (счётчик `B4_FIND_SEGMENT_CALLS` там же, :116–118).
2. → `linear_scan` — `find_segment.rs:491`. ДО фикса: для КАЖДОГО активного small/primordial сегмента-кандидата безусловно вызывался `drain_segment_sidecar(base)` (старый код ~:572, безусловно перед проверкой бин-головы).
3. → `drain_segment_sidecar` — `find_segment.rs:168`: проходит `ceil(high_water / SIDECAR_WORD_BYTES)` pending-слов (до 4,096 слов на сегмент).
4. → `BitmapScan::next_cut` — `src/alloc_core/segment/remote_bitmap/bitmap_scan.rs:71` (до фикса): безусловный `pending.swap(0, AcqRel)` — RMW на КАЖДОЕ слово, включая нулевые.
5. Усилитель: larson с `HANDOFF_EVERY=16` даёт ровно 100,000 кросс-поточных публикаций за проход (= steps×4/16 — подтверждено счётчиками `b4_foreign_publish_ok = b4_directory_lookups = 100,000` в КАЖДОМ прогоне). Их pending-биты извлекаются ТОЛЬКО этими сканами.
6. Спираль: пер-(segment,class) free-списки высыхают → discovery на ~каждый 14-й op → стоимость скана растёт с high_water накопленных сегментов → в орбите шторма ≈4.2µs/op чистого CPU (`proc_user_ms` ≈ wall, 100% CPU).
7. Бимодальность: детерминированный поток запросов, но состояние-зависимая орбита — «высохшие» списки против «сытых» зависят от ранней истории (свежие vs переработанные registry-слоты, дрейф reclaim/carve). Попал в сухую орбиту — коллапс до конца прохода. Сам ТРИГГЕР бифуркации в этой работе не атрибутирован — открытый пункт §7a; счётчик наблюдения — `b4_find_segment_calls`.

## 4. Кандидат-исправление

### 4.1 Фикс 1 — load-guarded swap (`bitmap_scan.rs:70–81`)
Продакшн-ветка `next_cut` (r18-бенч-ветки не тронуты):
```rust
#[cfg(not(all(feature = "bench-internals", r18_sidecar_scan_bench)))]
let bits = if pending.load(Ordering::Acquire) == 0 {
    0
} else {
    pending.swap(0, Ordering::AcqRel)
};
```
Убирает RMW на нулевых словах (их подавляющее большинство). Зеркалит порядок бенч-ветки r18 (load Acquire → swap AcqRel).

### 4.2 Фикс 2 — «бин-голова до drain» (`find_segment.rs`, `linear_scan`, ~:565–601)
Одно пред-drain чтение бин-головы класса, дренаж только при пустом бине; ветки drain (обе alloc-decommit руки + Skipped/Drained) сохранены дословно; хит-блок после дренажа не тронут (повторное чтение `SegmentMeta::new(base)` после дренажа — обязательно для случая drain-may-release):
```rust
let pre_nonempty = SegmentMeta::new(base).bin_table().head(class_idx) != FREE_LIST_NULL;
#[cfg(feature = "alloc-stats")]
let was_nonempty = routed_miss_scan && pre_nonempty;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
if !pre_nonempty {
    match self.drain_segment_sidecar(base) { /* руки дословно как раньше */ }
}
```
Одно чтение обслуживает и решение о дренаже, и alloc-stats классификацию `was_nonempty` (семантика классификации не изменилась — то же пред-drain значение). Неизбежный сдвиг событий задокументирован в комментарии: не-пустой сегмент больше не дренируется вовсе, drain-outcome счётчики видят меньше Drained событий.

### 4.3 Диагностическая инфраструктура (та же работа)
- 7 Relaxed-счётчиков `B4_*` (`src/alloc_core/alloc_core/counters.rs:11–41`, зеркала в `alloc_core_impl.rs`, ре-экспорт в `alloc_core/mod.rs`), все — только на медленных/холодных путях, `#[cfg(feature = "bench-internals")]`.
- Один `#[doc(hidden)]` акцессор `SeferAlloc::dbg_b4_collapse_counters() -> [u64; 7]` (`src/global/sefer_alloc/diag.rs:29–52`).
- Пример `examples/b4_collapse_probe.rs` (проба с per-window throughput + снапшотами счётчиков).

### 4.4 Анализ безопасности
- One-pass скан: окно гонки НЕ изменилось. Бит, опубликованный после прохода слова сканом, был вне досягаемости ЭТОГО скана и при безусловном swap; биты сбрасываются только владельческим swap, поэтому пропущенное (прочитанное как 0) слово остаётся pending и находится следующим discovery-сканом, strict trim'ом или maintenance ingress.
- Release credit-gated: `finish_small_drain` (`src/alloc_core/alloc_core/sidecar_drain.rs:218`) освобождает/декоммитит только через `dec_live_and_maybe_decommit` (`sidecar_drain.rs:231`) — unreclaimed публикация держит ledger-credit, сегмент не может быть release'нут с потерянным битом.
- В bench-сборке (без `alloc-decommit`) дренажи не освобождают ничего вовсе.
- Strict trim и maintenance-пути не менялись; публичный API не менялся; нового `unsafe` нет; r18-бенч-ветки `next_cut` не тронуты.

## 5. Проверка

- Серия s7 (см. §1): 48/48 plain прогонов без единого коллапса/деградации; counters-сборка 12/12 чисто, `b4_scan_words` ≤ 4,071,583/проход (−143×), `b4_scan_calls` ≤ 1,093; `b4_find_segment_calls` до 368,604/проход — частотный триггер остался (§7a).
- Юнит/интеграционные тесты (release, `alloc-global alloc-xthread internals bench-internals`): `r6_sidecar_bitmap` (включая `r18_concurrent_publications_survive_empty_cuts_and_resume`), `r3_1_fallback_remote_free`, `r11_ph3b_kind_aware_entries`, `b4_scan_zero_word_skip_keeps_bits` (новый) — все ok; плюс все 33 тестовых файла, затрагивающих `find_segment`/drain/scan-счётчики (полный список в задаче B4-fix-2) — все ok.
- `tests/b4_scan_zero_word_skip_keeps_bits.rs`: пинует, что guard НЕ перескакивает ненулевые слова (word0=0 → пустой cut; word1 → полный паттерн и pop записи). ЧЕСТНО: тест зелёный И с фиксом, И без него — это пин регрессии, а не контрфактический красный тест; контрфактическое доказательство самого фикса — измерительная серия (ДО: 10/48 коллапсов и 518M RMW-слов; ПОСЛЕ: 0/48 и ≤4.07M слов), а не юнит-тест.
- `tests/loom_b4_scan_skip_empty_publish.rs`: loom-модель протокола scan∥publish (load-then-conditional-swap против fetch_or), неисчезновение поздней публикации; исполняется на CI с `--cfg loom` (локально, по правилам проекта, не запускался; компиляция как обычного теста проверена).
- `node scripts/verify-dbg-hook-safety.mjs` → 0; `node scripts/verify-structure-limits.mjs` → 0 (directory.rs удержан ≤1000 строк).
- `cargo clippy --lib --release -- -D warnings` → 0 для bench-набора фич и для `production`; `cargo check --release` → 0 для bench-набора, `production`, `alloc-decommit`, `numa-aware`.
- CSV-кросс-чек: `docs/perf/B4_MT_COLLAPSE_DIAGNOSIS_summary.csv` (157 строк = заголовок + 156 прогонов) сгенерирован механически скриптом `make_b4_csv.mjs` из тех же логов, что и таблицы §1/§2.

## 6. Что нужно на CI

- **Loom**: новый `tests/loom_b4_scan_skip_empty_publish.rs` (модель scan∥publish, неисчезновение) + существующий `tests/loom_sidecar_bitmap.rs`.
- **Miri**: инварианты sidecar (`r11_p3_small_sidecar_*`) — паттерн доступа к памяти не изменился (guard лишь добавил чтение перед существующим swap).
- **iai**: `cold_alloc_free_256x*`, `recycle_alloc_free_256x*`, `small_churn_16b` — ожидаем absence регрессии, возможен небольшой выигрыш на refill-miss.
- **mt-bench workflow** (`malloc_macro`, windows-2022, label `mt-bench`) — основное приёмочное подтверждение на чистой машине.
- Повтор серии владельца (`malloc_macro` T=1/2/4) под фоновой нагрузкой сборок — воспроизвести отсутствие коллапсов в «шумных» условиях.

## 7. Открытые пункты

- (a) **Триггер бифуркации не атрибутирован**: почему в одних прогонах пер-(segment,class) списки высыхают (орбита массового discovery), а в других нет — не определено. Счётчик наблюдения: `b4_find_segment_calls` (штатно ~550–700/проход; шторм >100k).
- (b) Discovery-скан на каждый miss в многосегментных кучах остаётся дизайнерской стоимостью (теперь дешёвой, но O(сегменты) на miss) — направление B1 §6.3 / R7 (безблокировочный каталог/эпохи).
- (c) Проба и 7 счётчиков — diagnostics под `internals`/`bench-internals`; продакшн-поверхность (публичный API, поведение без фич) не изменена.

## 8. Предлагаемые коммиты

- `perf(runtime): guard sidecar scans against empty words and drain-only-when-empty on discovery (B4 MT collapse)` — body: «Load-guarded swap in BitmapScan::next_cut and single pre-drain bin-head check in linear_scan; 48-run series: 10/48 collapses → 0/48, scan_words −143×.»
- `bench: b4_collapse_probe example + B4 route/scan/shard diagnostic counters` — body: «bench-internals-gated Relaxed observers (7 counters, one dbg accessor) and a per-window larson/mstress probe for collapse attribution.»
- `docs: B4 MT collapse diagnosis (scan-storm mechanism, 48-run series)` — body: «Mechanism, counter deltas, safety analysis and CI plan for the sidecar scan-storm collapse; summary CSV derived mechanically from raw logs.»

## Идентификация (R29-6, форма 4 — незафиксированное дерево)

- База: `6b166a7bb2e763f4db7ce0325746024eadaa9ddd` + рабочее дерево: изменённые `src/alloc_core/alloc_core/alloc_core_impl.rs`, `src/alloc_core/alloc_core/counters.rs`, `src/alloc_core/mod.rs`, `src/alloc_core/segment/remote_bitmap/bitmap_scan.rs`, `src/alloc_core/small/alloc_core_small/find_segment.rs`, `src/global/sefer_alloc/diag.rs`, `src/registry/heap_core_xthread/routing.rs`, `src/registry/segment_route/directory.rs`, `src/registry/segment_route/shard_lock.rs`; новые `examples/b4_collapse_probe.rs`, `tests/b4_scan_zero_word_skip_keeps_bits.rs`, `tests/loom_b4_scan_skip_empty_publish.rs`, `docs/perf/B4_MT_COLLAPSE_DIAGNOSIS.md`, `docs/perf/B4_MT_COLLAPSE_DIAGNOSIS_summary.csv`, `docs/perf/_raw_b4_*.log` (8 шт.).
- Toolchain: rustc 1.97.0, target `x86_64-pc-windows-msvc`.
- Бинарники `b4_collapse_probe.exe` (sha256, пересобраны подряд на финальном дереве):
  - counters (`alloc-global alloc-xthread internals bench-internals`): `3886ac8ef994449dce16247b3da422b002ee4e49cde8811a97944b7b3e2d788b`
  - plain (`alloc-global alloc-xthread`, соответствует последней серии s7): `ce0c02a9979d85e73c78d2697c6b688cc79b74e983b1e80132a5cdf591911971`
