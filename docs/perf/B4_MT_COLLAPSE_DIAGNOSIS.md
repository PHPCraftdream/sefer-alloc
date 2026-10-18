# B4: диагностика бимодального коллапса MT larson (scan storm на refill-miss) и два минимальных исправления

Дата: 2026-10-18. Машина: Windows 10 владельца, i7-11800H 8C/16T; фоновая нагрузка — сборки других агентов в моменты замеров (синтетическая нагрузка НЕ добавлялась). Бинар примера: `examples/b4_collapse_probe` (larson, семантика `malloc-bench-rs`, WORK-time измерение PASS), `CARGO_TARGET_DIR` — отдельный общий каталог вне репозитория. База: коммит `114eb41aa2a2c9bedeb1e5d362ac507eb315f676` + рабочее дерево данной задачи (см. §Идентификация). Порог коллапса: ops/s < 10M при T=4 (наблюдаемая бимодальность: 0.54M против 26M+; промежуточных значений нет).

## 0. Резюме

- Бимодальный 100× коллапс MT larson — это ШТОРМ sidecar-сканов на пути обнаружения при refill-miss: в коллапсированных прогонах `find_segment_with_free` вызывается 119,875 раз за проход (норма 552–674) и каждый вызов обходит sidecar ВСЕХ активных small-сегментов.
- Сгоревшее время — побайтовый проход `pending`-слов: `b4_scan_words` = 517,956,323 за коллапсированный проход (s4_warm_06) против 1,050,348–1,374,861 в норме (377–493× против нормальной полосы); при старом безусловном `pending.swap(0, AcqRel)` это 518M RMW ≈ всё время прохода (2.5–3.1s).
- Гипотеза «конвой на shard-lock» ОПРОВЕРГНУТА: `b4_shard_lock_spins` в коллапсе 297–1,493 — тот же порядок, что в норме (292–364); согласуется с опровержением b4-r2-shardlock.
- Гипотезы «fallback-шторм», «шторм резервирований сегментов», «page-fault шторм», «потерянные/необработанные foreign-free» ОПРОВЕРГНУТЫ нулевыми/порядково-равными счётчиками (§2).
- Два минимальных исправления — (1) load-guarded swap в `BitmapScan::next_cut` (RMW только для ненулевого слова), (2) «сначала бин-голова, drain только при пустом бине» в `linear_scan` — убрали коллапс в той же серии: 0/48 коллапсов (было 5/48: 2 в s2_cold + 3 в s2_warm), минимум warm-серии поднялся с 0.65M до 35.85M ops/s.
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

Таблицы ниже — дословный вывод `node scripts/b4_collapse_summary.mjs` (без ручной правки). Источник — `docs/perf/_raw_b4_collapse_series.log.gz`: все логи серий s2–s7, каждый после строки `=== FILE <имя>`, сжаты gzip (политика хранения логов 200 KiB–2 MiB). Скрипт парсит `RESULT_JSON` каждого лога (проход t=4) и пишет `docs/perf/B4_MT_COLLAPSE_DIAGNOSIS_summary.csv`. Медиана интерполированная (при чётном n — среднее двух средних). Коллапс: ops/s < 10M; деградация: 10M ≤ ops/s < 20M. Серии s2/s5/s7 — обычная сборка, s3/s4/s6/s7_cnt — сборка со счётчиками (`internals bench-internals`), она медленнее сама по себе.

| серия | сборка | прогонов | min | median | max (M ops/s) | коллапсов (<10M) | деградированных (10–20M) |
|---|---|---|---|---|---|---|---|
| s2_cold | pre-fix | 24 | 3.12 | 46.19 | 59.70 | 2 (s2_cold_01:9.21, s2_cold_15:3.12) | 0 |
| s2_warm | pre-fix | 24 | 0.65 | 43.68 | 63.76 | 3 (s2_warm_03:3.19, s2_warm_07:0.65, s2_warm_24:3.01) | 5 (13.48/17.73/13.55/12.01/10.35) |
| s3_cold | pre-fix counters | 20 | 2.44 | 29.49 | 32.12 | 6 (s3_cold_02:2.58, s3_cold_08:2.44, s3_cold_09:2.52, s3_cold_11:3.05, s3_cold_15:3.97, s3_cold_19:3.28) | 1 (14.13) |
| s3_warm | pre-fix counters | 40 | 0.54 | 24.80 | 32.17 | 14 (s3_warm_02:6.74, s3_warm_05:0.64, s3_warm_07:6.53, s3_warm_08:0.58, s3_warm_09:0.65, s3_warm_17:0.57, s3_warm_18:3.32, s3_warm_19:2.17, s3_warm_20:2.16, s3_warm_22:0.54, s3_warm_26:5.62, s3_warm_27:0.63, s3_warm_29:4.99, s3_warm_40:1.85) | 4 (10.17/11.98/14.87/10.04) |
| s4_warm | pre-fix counters | 16 | 0.67 | 12.82 | 31.97 | 7 (s4_warm_02:3.21, s4_warm_04:3.47, s4_warm_05:7.83, s4_warm_06:0.67, s4_warm_07:2.94, s4_warm_10:3.45, s4_warm_15:4.96) | 2 (15.37/10.26) |
| s5_cold | fix1 | 24 | 29.11 | 49.35 | 53.26 | 0 | 0 |
| s5_warm | fix1 | 24 | 25.36 | 47.28 | 60.03 | 0 | 0 |
| s6_warm | fix1 counters | 10 | 4.82 | 23.24 | 27.89 | 1 (s6_warm_01:4.82) | 2 (16.59/18.47) |
| s7_cold | fix1+2 | 24 | 46.47 | 51.15 | 61.03 | 0 | 0 |
| s7_warm | fix1+2 | 24 | 35.85 | 45.89 | 65.23 | 0 | 0 |
| s7_cnt_warm | fix1+2 counters-build | 12 | 23.03 | 26.80 | 30.05 | 0 | 0 |

counters maxima (counters_delta t=4):
| серия | max b4_find_segment_calls (прогон) | max b4_scan_calls (прогон) | max b4_scan_words (прогон) |
|---|---|---|---|
| s4_warm | 119,875 (s4_warm_06) | 132,546 (s4_warm_06) | 517,956,323 (s4_warm_06) |
| s6_warm | 133,352 (s6_warm_01) | 149,580 (s6_warm_01) | 584,007,469 (s6_warm_01) |
| s7_cnt | 368,604 (s7_cnt_12) | 1,706 (s7_cnt_09) | 4,071,583 (s7_cnt_09) |

Сырые копии логов (каждая < 6 KB, в `docs/perf/`, выдержки из тех же логов, что и в архиве серий): `_raw_b4_prefix_series_warm.log`, `_raw_b4_prefix_series_cold.log`, `_raw_b4_collapsed_counters.log` (= s4_warm_06), `_raw_b4_normal_counters.log` (= s4_warm_14), `_raw_b4_fix1_storm_counters.log` (= s6_warm_01: find 133,352 / words 584,007,469 — совпадает), `_raw_b4_final_series_warm.log`, `_raw_b4_final_series_cold.log`, `_raw_b4_final_series_counters.log`. Отдельно в том же каталоге лежит служебная серия `series_base_*` (24 лога) — другая конфигурация пробы: `--sweep 1,2,4` с `--window-ms 250` и постоянной полосой ~6.72–6.78M ops/s; это посторонняя/служебная серия, в таблицы отчёта она НЕ включена.

## 2. Счётчики

DELTAS за один T=4 проход (1.67–1.70M ops) из CNT-строк пробы. Коллапсированные: s4_warm_06 (0.67M) / s4_warm_02 (3.21M) / s4_warm_15 (4.96M); нормальные: s4_warm_11 (31.97M) / s4_warm_14 (31.50M) / s4_warm_09 (28.10M).

Дословный вывод того же скрипта: диапазон delta за проход t=4 по трём нормальным и трём коллапсированным прогонам; счётчик, которого нет в выводе пробы для прогона, равен 0 (проба печатает только ненулевые delta).

| счётчик (delta/проход t=4) | норма | коллапс |
|---|---|---|
| b4_directory_lookups | 100,000 | 100,000 |
| b4_find_segment_calls | 552–674 | 17,264–119,875 |
| b4_foreign_publish_ok | 100,000 | 100,000 |
| b4_scan_calls | 552–674 | 17,307–132,546 |
| b4_scan_words | 1,050,348–1,374,861 | 65,980,112–517,956,323 |
| b4_shard_lock_spins | 292–364 | 297–1,493 |
| claims_bump | 2 | 2 |
| claims_free_set | 1 | 1 |
| claims_hint | 1 | 1 |
| heaps_claimed_high_water | 2 | 2 |
| proc_cycles | 485,043,646–515,663,546 | 1,660,209,932–6,108,615,518 |
| proc_page_faults | 1,032–1,385 | 2,184–2,463 |
| proc_user_ms | 187–250 | 719–2,641 |
| proc_working_set_bytes | 4,124,672–5,435,392 | 8,441,856–9,359,360 |
| segments_reserved_total | 2 | 3–4 |
| sidecar_ingress_drain_calls | 6 | 6 |
| sidecar_ingress_records_consumed | 4,415–9,048 | 3,926–11,408 |
| fallback_lock_acquisitions (counters_end, max over all 16 s4 runs) | 0 | 0 |
| foreign_or_unroutable_frees (counters_end, max over all 16 s4 runs) | 0 | 0 |
| segments_released_total (counters_end, max over all 16 s4 runs) | 0 | 0 |

## 3. Механизм (текущие номера строк проверены чтением)

Цепочка при refill-miss (владелец не нашёл свободный блок класса):

1. `AllocCore::find_segment_with_free` — `src/alloc_core/small/alloc_core_small/find_segment.rs:113` (счётчик `B4_FIND_SEGMENT_CALLS` там же, :115–117).
2. → `linear_scan` — `find_segment.rs:491`. ДО фикса: для КАЖДОГО активного small/primordial сегмента-кандидата безусловно вызывался `drain_segment_sidecar(base)` (старый код ~:572, безусловно перед проверкой бин-головы).
3. → `drain_segment_sidecar` — `find_segment.rs:168`: проходит `ceil(high_water / SIDECAR_WORD_BYTES)` pending-слов (до 4,096 слов на сегмент).
4. → `BitmapScan::next_cut` — `src/alloc_core/segment/remote_bitmap/bitmap_scan.rs:76` (до фикса): безусловный `pending.swap(0, AcqRel)` — RMW на КАЖДОЕ слово, включая нулевые.
5. Усилитель: larson с `HANDOFF_EVERY=16` даёт ровно 100,000 кросс-поточных публикаций за проход (= steps×4/16 — подтверждено счётчиками `b4_foreign_publish_ok = b4_directory_lookups = 100,000` в КАЖДОМ прогоне). Их pending-биты извлекаются ТОЛЬКО этими сканами.
6. Спираль: пер-(segment,class) free-списки высыхают → discovery на ~каждый 14-й op → стоимость скана растёт с high_water накопленных сегментов → в орбите шторма ≈4.2µs/op чистого CPU (`proc_user_ms` ≈ wall, 100% CPU).
7. Бимодальность: детерминированный поток запросов, но состояние-зависимая орбита — «высохшие» списки против «сытых» зависят от ранней истории (свежие vs переработанные registry-слоты, дрейф reclaim/carve). Попал в сухую орбиту — коллапс до конца прохода. Сам ТРИГГЕР бифуркации в этой работе не атрибутирован — открытый пункт §7a; счётчик наблюдения — `b4_find_segment_calls`.

## 4. Кандидат-исправление

### 4.1 Фикс 1 — load-guarded swap (`bitmap_scan.rs:97–115`, production-гард)
Продакшн-ветка `next_cut` (счётчик RMW :108–113, guarded-ветка :105–115; r18-механизм `ScanCount`/`SCAN_COUNTS` :4–20 и bench-ветка не тронуты):
```rust
#[cfg(not(all(feature = "bench-internals", r18_sidecar_scan_bench)))]
let bits = if pending.load(Ordering::Acquire) == 0 {
    0
} else {
    pending.swap(0, Ordering::AcqRel)
};
```
Убирает RMW на нулевых словах (их подавляющее большинство). Зеркалит порядок бенч-ветки r18 (load Acquire → swap AcqRel).

### 4.2 Фикс 2 — «бин-голова до drain» (`find_segment.rs`, `linear_scan` :491, pre-check ~:579–601)
Одно пред-drain чтение бин-головы класса, дренаж только при пустом бине; ветки drain (обе alloc-decommit руки + Skipped/Drained) сохранены дословно; хит-блок после дренажа не тронут (повторное чтение `SegmentMeta::new(base)` после дренажа — обязательно для случая drain-may-release):
```rust
let pre_nonempty = SegmentMeta::new(base).bin_table().head(class_idx) != FREE_LIST_NULL;  // :583, cfg(any(alloc-stats, all(alloc-global, alloc-xthread))) :579–582
#[cfg(feature = "alloc-stats")]
let was_nonempty = routed_miss_scan && pre_nonempty;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
if !pre_nonempty {  // :588, условие ~:588–601
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

- Серия s7 (см. §1): 48/48 plain прогонов без единого коллапса/деградации; counters-сборка 12/12 чисто, `b4_scan_words` ≤ 4,071,583/проход (−143× к s6_warm_01; −127× к s4_warm_06), `b4_scan_calls` ≤ 1,706 (s7_cnt_09); `b4_find_segment_calls` до 368,604/проход (s7_cnt_12) — частотный триггер остался (§7a).
- Юнит/интеграционные тесты (release, `alloc-global alloc-xthread internals bench-internals`): `r6_sidecar_bitmap` (включая `r18_concurrent_publications_survive_empty_cuts_and_resume`), `r3_1_fallback_remote_free`, `r11_ph3b_kind_aware_entries`, `b4_scan_zero_word_skip_keeps_bits`, `b4_scan_rmw_zero_word_skips_swap` (новый), `b4_find_segment_bin_head_gates_drain` (новый; под этим набором фич компилируется в 0 тестов — его гейт требует `production` и `alloc-stats`; содержательный прогон — `production internals bench-internals alloc-stats`, на CI его покрывает существующий шаг `cargo test --features "production alloc-stats bench-internals internals"`) — все ok; плюс все 33 тестовых файла, затрагивающих `find_segment`/drain/scan-счётчики (полный список в задаче B4-fix-2) — все ok.
- `tests/b4_scan_zero_word_skip_keeps_bits.rs`: пинует, что guard НЕ перескакивает ненулевые слова (word0=0 → пустой cut; word1 → полный паттерн и pop записи). ЧЕСТНО: тест зелёный И с фиксом, И без него — это пин регрессии, а не контрфактический красный тест.
- `tests/b4_scan_rmw_zero_word_skips_swap.rs` (контрфактический тест фикса 1, thread-local счётчик `B4_PROD_SCAN_RMW` продакшн-ветки `bitmap_scan.rs`, `bitmap_scan.rs:108–113`): нулевой банк → 0 RMW-свопов; ненулевое слово → ровно 1 своп, потребление ровно один раз. КОНТРФАКТ: при временном откате фикса 1 (безусловный swap) тест краснеет («left: 4096, right: 0» на 4096-словном банке).
- `tests/b4_find_segment_bin_head_gates_drain.rs` (контрфактический тест фикса 2, наблюдатель — статический `B4_SCAN_CALLS`, инкремент на входе `drain_segment_sidecar`): пустой бин → дренаж выполняется, `B4_SCAN_CALLS` +1, блок из sidecar найден; непустой бин → дренаж НЕ вызывается, дельта 0. КОНТРФАКТ: откат фикса 2 → красный («left: 1, right: 0»).
- `tests/loom_b4_scan_skip_empty_publish.rs`: loom-модель протокола scan∥publish — теперь НЕ shadow-модель, а ПРОДАКШН `bitmap_scan.rs` под `cfg(loom)` через `#[path]`-инклюд (как `tests/loom_sidecar_bitmap_actual.rs`); подключён в CI job `loom-xthread` и в `scripts/loom.mjs`. Проверка проводки: `node scripts/verify-loom-target-wiring.mjs` → 0 (20 таргетов). Локально (по правилам проекта) loom не запускался; сентинел CI-лога: `test loom_b4_publish_racing_zero_word_skip_found_by_later_scan ... ok`.
- `node scripts/verify-dbg-hook-safety.mjs` → 0 (250 reviewed safe; регистрация `dbg_b4_collapse_counters` в списке); `node scripts/verify-structure-limits.mjs` → 0 (directory.rs удержан ≤1000 строк).
- Зелёная clippy/check-матрица (прогон оркестратора, все exit 0): `cargo clippy --all-targets` для hardened medium-classes internals; `--lib` production; `--all-targets` production internals; `--lib` numa-aware internals; `--lib` alloc-global alloc-xthread; `--all-targets` experimental; `--all-targets --all-features`; `--all-targets` default; плюс `cargo check --target x86_64-unknown-linux-gnu --lib --features "production internals"` → 0. ЧЕСТНО: кросс-компиляция `--all-targets` на Linux локально невозможна — нет Linux-кросс-тулчейна.
- Честный статус инфраструктуры пробы: пример `b4_collapse_probe` зарегистрирован `[[example]]` с `required-features`; файловый `#![cfg]` снят; не-Windows стаб `proc_metrics_snapshot` удалён (мёртвый код на Linux); `pre_nonempty` в `linear_scan` под точным `cfg(any(alloc-stats, all(alloc-global, alloc-xthread)))` (`find_segment.rs:579–583`).
- CSV-кросс-чек: `docs/perf/B4_MT_COLLAPSE_DIAGNOSIS_summary.csv` (243 строки = заголовок + 242 прогонов: s2/s3/s4/s5/s6/s7/s7_cnt) сгенерирован механически скриптом `scripts/b4_collapse_summary.mjs` из тех же логов, что и таблицы §1/§2.

## 6. Что нужно на CI

- **Loom**: `tests/loom_b4_scan_skip_empty_publish.rs` (продакшн-модель scan∥publish, неисчезновение) — теперь в CI job `loom-xthread` (сентинел `test loom_b4_publish_racing_zero_word_skip_found_by_later_scan ... ok`) + существующий `tests/loom_sidecar_bitmap.rs`.
- **Miri**: инварианты sidecar (`r11_p3_small_sidecar_*`) — паттерн доступа к памяти не изменился (guard лишь добавил чтение перед существующим swap).
- **iai**: `cold_alloc_free_256x*`, `recycle_alloc_free_256x*`, `small_churn_16b` — ожидаем absence регрессии, возможен небольшой выигрыш на refill-miss.
- **mt-bench workflow** (`malloc_macro`, windows-2022, label `mt-bench`) — основное приёмочное подтверждение на чистой машине.
- Повтор серии владельца (`malloc_macro` T=1/2/4) под фоновой нагрузкой сборок — воспроизвести отсутствие коллапсов в «шумных» условиях.

## 7. Открытые пункты

- (a) **Триггер бифуркации не атрибутирован**: почему в одних прогонах пер-(segment,class) списки высыхают (орбита массового discovery), а в других нет — не определено. Счётчик наблюдения: `b4_find_segment_calls` (штатно 552–674/проход в s4-норме; в коллапсе 17,264–119,875).
- (b) Discovery-скан на каждый miss в многосегментных кучах остаётся дизайнерской стоимостью (теперь дешёвой, но O(сегменты) на miss) — направление B1 §6.3 / R7 (безблокировочный каталог/эпохи).
- (c) Проба и 7 счётчиков — diagnostics под `internals`/`bench-internals`; продакшн-поверхность (публичный API, поведение без фич) не изменена.

## 8. Предлагаемые коммиты

- `perf(runtime): guard sidecar scans against empty words and drain-only-when-empty on discovery (B4 MT collapse)` — body: «Load-guarded swap in BitmapScan::next_cut and single pre-drain bin-head check in linear_scan; 48-run series: 5/48 collapses (2 cold + 3 warm) → 0/48, scan_words −143× к s6_warm_01.»
- `bench: b4_collapse_probe example + B4 route/scan/shard diagnostic counters` — body: «bench-internals-gated Relaxed observers (7 counters, one dbg accessor) and a per-window larson/mstress probe for collapse attribution.»
- `docs: B4 MT collapse diagnosis (scan-storm mechanism, 48-run series)` — body: «Mechanism, counter deltas, safety analysis and CI plan for the sidecar scan-storm collapse; summary CSV derived mechanically from raw logs.»

## Идентификация (R29-6, форма 4)

- База: коммит `114eb41aa2a2c9bedeb1e5d362ac507eb315f676` + рабочее дерево данной задачи. Изменённые файлы по `git diff --stat` относительно базы: `.github/workflows/ci.yml`, `Cargo.toml`, `examples/b4_collapse_probe.rs`, `scripts/loom.mjs`, `scripts/verify-dbg-hook-safety.mjs`, `src/alloc_core/segment/remote_bitmap/bitmap_scan.rs`, `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs`, `src/alloc_core/small/alloc_core_small/find_segment.rs`, `src/registry/segment_route/shard_lock.rs`, `tests/b4_scan_zero_word_skip_keeps_bits.rs`, `tests/loom_b4_scan_skip_empty_publish.rs`, `tests/loom_sidecar_bitmap_actual.rs`, `tests/r6_sidecar_bitmap.rs`; новые `tests/b4_find_segment_bin_head_gates_drain.rs`, `tests/b4_scan_rmw_zero_word_skips_swap.rs`, плюс документы/CSV/логи этой задачи (`docs/perf/B4_MT_COLLAPSE_DIAGNOSIS.md`, `docs/perf/B4_MT_COLLAPSE_DIAGNOSIS_summary.csv`, `docs/perf/_raw_b4_*.log`, `scripts/b4_collapse_summary.mjs`).
- ЧЕСТНО про бинарники: хеши sha256 ниже сняты с бинарей, ПЕРЕСОБРАННЫХ ПОСЛЕ серий замеров — они фиксируют финальное дерево, а НЕ артефакты, которыми измерялись серии.
- Toolchain: rustc 1.97.0, target `x86_64-pc-windows-msvc`.
- Бинарники `b4_collapse_probe.exe` (sha256, пересобраны после серий на финальном дереве — см. честную оговорку выше):
  - counters (`alloc-global alloc-xthread internals bench-internals`): `3886ac8ef994449dce16247b3da422b002ee4e49cde8811a97944b7b3e2d788b`
  - plain (`alloc-global alloc-xthread`, соответствует последней серии s7): `ce0c02a9979d85e73c78d2697c6b688cc79b74e983b1e80132a5cdf591911971`
