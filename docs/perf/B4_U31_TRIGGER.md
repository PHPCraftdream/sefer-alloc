# B4-29 / U31 — триггеры возврата owner-drain skip по pin-epoch (item 90)

**Измерение, без изменения runtime.** Все наблюдатели U31
(`B4_U31_SCAN_SEGMENTS`, `B4_U31_SCAN_HOLDERS_BASE`, `B4_U31_SCAN_NO_PENDING`)
— чистые `Relaxed`-инкременты под `bench-internals`; runtime production-сборки
не меняется (`node scripts/verify-dbg-hook-safety.mjs` — PASS 133 hooks;
индивидуальная запись хука `dbg_entry_refs_relaxed` внесена в
`PURE_OBSERVERS` — `scripts/verify-dbg-hook-safety.mjs:63`).

- База: `origin/main` после PR #87 (байт состояния) и PR #88 = `1e20e9aa`
  (константа базы фичевого дерева в скрипте; рядом печатается HEAD).
  T1/T3-сырьё (ut-u31run4/lar2/ms2) снято на дереве `512508e8`, MT-сырьё
  `ut-u31mt3-*` — на текущем дереве (`git_diff_sha256` в каждом логе
  проверяется скриптом).
- Сырьё (`docs/perf/b4_u31_raw/`, пробы и обёртки — `docs/perf/b4_u31_probes/`): `ut-u31run4*` (growth16 callgrind), `ut-u31lar2*` /
  `ut-u31ms2*` (larson/mstress T=1), `ut-u31mt3-*` (4 ячейки × 3 повтора = 12
  MT-прогонов; larson-t2-r1 = файл `ut-u31mt3-larson-t2-r1b.log` — первый r1
  упал на компиляции, перезапущен), `ut-u31mutant-nopending.log` (мутант).
- Скрипт: `scripts/b4_u31_trigger_table.mjs` (все доли/вердикты — одним
  скриптом из сырья, с assert'ами арифметики); вывод — раздел «Таблицы, вывод скрипта дословно» ниже;
  CSV: `docs/perf/B4_U31_TRIGGER_summary.csv`.

## Методика: нижняя и верхняя границы T1

**Нижняя граница (по ней вердикт)** — self-Ir: построчная агрегация
`callgrind_annotate --auto=yes` по файлам `scan.rs` (bin-head discovery-скан) и
`bitmap_scan.rs` (pending sidecar scan). Строки `=>` (сводные по функции)
исключены — перекрытий и двойного счёта нет. `ATTR_TOTALS RESIDUAL`:
`uncovered = program − annotated` — это Ir кода **без исходников**
(glibc malloc/startup, rustc std без локальных исходников), атрибуция на Sefer
не раскладывается; на `self-долю` он не влияет (числитель только по нашим
файлам, знаменатель — полный program total).

**Верхняя граница (ОЦЕНКА границы)** — inclusive-Ir
`find_segment_with_free_impl`: сумма записей функции в отсортированном списке
`.annot` (по файлам inline-expansion, `=>`-строки исключены). Включает всё,
что инлайнится в discovery-путь, поэтому систематически выше self-суммы.

**Вердикт T1 — только по нижней границе и только на n=131072 (4n).**
Остальные размеры — справочные строки.

**Потолок выгоды U31 (ОЦЕНКА, НЕ вердикт)** — `self(bitmap_scan.rs)/total`:
U31 устраняет только pending-скан (sidecar), bin-head discovery-скан (`scan.rs`)
остаётся; поэтому весь discovery-скан — недостижимая для U31 экономия.

### T1 — growth16 (callgrind Ir)

*(таблица — в разделе «Таблицы, вывод скрипта дословно» ниже; числа печатает `scripts/b4_u31_trigger_table.mjs`)*

**Вердикт T1: FIRED** — n=131072 (4n): 8.60× ≥ 3.0× и self-доля
48.56 % (50 991 106/105 000 655) ≥ 30 %. Потолок U31 на 4n — 47.97 %
(50 368 515/105 000 655), ОЦЕНКА.

### T1-справка: larson/mstress T=1

*(таблица — в разделе «Таблицы, вывод скрипта дословно» ниже; числа печатает `scripts/b4_u31_trigger_table.mjs`)*

## T2-семантика: holders==base — формально FIRED-строка с НЕВАЛИДНОЙ интерпретацией

Исходное определение наблюдателя `B4_U31_SCAN_HOLDERS_BASE` — «текущие
держатели entry == база» (`refs == 1` в момент скана). Это **не** означает
«с прошлого скана не было публикаций»:

- `RoutePin::publish_small` (`src/registry/segment_route/pin.rs:63-80`)
  публикует и **сразу освобождает** pin: refs возвращается к базе, хотя
  pending-слово стало ненулевым;
- `EntryHandle::drop` (`src/registry/segment_route/directory.rs:218-225`) при
  непустом pending возвращает refs к базе (контрпример:
  `tests/r6_route_directory.rs:109-129`).

Поэтому предрегистрированная строка T2 (SCAN_WORDS/op ≥ 1.0 **по ops_actual** И
медиана holders==base ≥ 50 %) — вердикт **по формальному определению**;
интерпретировать её как «U31 можно активировать» нельзя.

**Корректный верхний предел активации U31** — доля
`u31_scan_no_pending / u31_discovery_scans`: сканы discovery-сайта
(owner-drain через `drain_segment_sidecar`, `scan.rs`; strict-trim/bounded
сканы не входят), не нашедшие ни одного ненулевого pending-слова (именно такие
U31-skip пропустил бы). Числитель и знаменатель — ОДНО множество
(`u31_discovery_scans`); holders==base — другое измерение со знаменателем
`u31_scan_segments`.

*(таблица — в разделе «Таблицы, вывод скрипта дословно» ниже; числа печатает `scripts/b4_u31_trigger_table.mjs`)*

Пометка «близость к порогу» вычисляется скриптом (`holdersMedian >=
0.5×0.8`, т.е. ≥ 40 %): на данных mt3 ни одна ячейка не в пределах 20 % от
порога. holders==base — формальное измерение с невалидной интерпретацией для
U31; пометка — только о близости ЧИСЛА к порогу, не о валидности измерения.

**Вердикт T2: NOT FIRED** по формальному определению (медианы holders==base
< 50 % во всех 4 ячейках); верхний предел no_pending-доли при этом 40–55 %
(медианы), т.е. потенциальный skip есть — но это предел, не активация: он
подсчитывает сканы, которые **уже** прошли и не нашли pending. Числитель и
знаменатель каждой доли по повторам — в выводе скрипта
(раздел «Таблицы…» ниже, например mstress T=2 r2: 198/380 = 52.11 %).

## Нормировка MT: ops_actual

SCAN_WORDS/op нормируется на `ops_actual` — **фактическое** число операций по
определению workload'а, а не `steps×threads`: larson — `steps×threads` +
mailbox-frees; mstress — `rounds = steps/512 + 1`, полраунда free + локальный
хвост + mailbox + финальные дрейны. Определения — в вендоренных worker-циклах
проб (`docs/perf/b4_u31_probes/ut_larson_probe2.rs`, `ut_u31mt_probe.rs`);
`ops_actual` печатается пробой в каждый лог. Сходимость
`scan_words_per_op_scaled_1e3 = trunc(scan_words/ops_actual×1000)` проверяется
assert'ом скрипта.

## Оракул активации

Доля `scan_calls / find_segment_calls` (числитель/знаменатель из `COUNTERS`),
порог ≥ 95 % для каждого заявленного discovery-пути — вычисляется скриптом из
сырья (без строковых констант с результатами): growth16 — 100.00 %
(2048/2048, 4096/4096, 8192/8192), larson T=1 — 100.00 % (106/106), mstress
T=1 — 100.00 % (82/82), все 12 MT-прогонов ut-u31mt3-* — 100.00 % (например,
larson T=2: 498/498, mstress T=4: 999/999, медианы по повторам).

Семантика `(u31_scan_segments − scan_calls) > 0` (например, 944 против 942):
`u31_scan_segments` срабатывает на входе `scan_small_from` на **всех**
owner-drain scan sites, `scan_calls` — только вызовы `find_segment`; разница —
сканы из drain/free-путей вне `find_segment`. Assert'ы скрипта:
`u31_scan_segments ≥ scan_calls`, `no_pending ≤ segments`,
`holders_base ≤ segments`, `num ≤ den`.

**Мутационное доказательство счётчика no_pending:** мутант «всегда пусто»
(`finish` форсирует NO_PENDING) краснеет — `tests/b4_u31_trigger_growth16_oracle.rs`,
тест `u31_no_pending_counts_empty_scans_but_not_published_pending_scans`,
leg `published`; лог `ut-u31mutant-nopending.log`, exit=101.

## Число прогонов

12 MT-прогонов = 4 ячейки (larson/mstress × T=2/4) × 3 повтора (не «12×3»;
larson-t2-r1 = файл `ut-u31mt3-larson-t2-r1b.log` — первый r1 упал на
компиляции и был перезапущен с новым суффиксом);
плюс growth16 3×2, larson/mstress T=1 по одному callgrind-прогону, 1 мутант.

## Почему наблюдатель holders==base остаётся

`B4_U31_SCAN_HOLDERS_BASE` (читается через `dbg_entry_refs_relaxed`,
`src/registry/segment_route/registration.rs`) — это **отдельное измерение**
«доля сканов с базовым числом текущих держателей pin» (не условие допустимого пропуска) (предрегистрированная строка
T2). Читатель `dbg_entry_refs_relaxed` читает refs **только той Entry, на
которую уже есть владеющая ссылка RouteRegistration в owner-слоте** — чтений
по произвольному адресу нет. Хук под `bench-internals`, индивидуальная запись
внесена в `PURE_OBSERVERS` (`scripts/verify-dbg-hook-safety.mjs:63`; PASS
133 hooks); удаления не требуется.

## Ограничения

- База без D1′/S1/X-null/X-span (PR #89): T1 может сместиться после вливания;
  пересчёт после PR #89 обязателен.
- RESIDUAL-атрибуция: `uncovered` (glibc/startup без исходников) не
  раскладывается на аллокаторы; self-доля — нижняя граница, inclusive —
  верхняя (ОЦЕНКА).
- Рукав multiseg в этих прогонах не снимался (T3-ветка multiseg — нет данных).
- T1-вердикт предрегистрирован только на n=131072 (4n); остальные размеры —
  справочные.
- Замеры — callgrind Ir и счётчики (детерминированные); wall-clock на шумной
  машине не использовался.

## Таблицы, вывод скрипта дословно

Команда: `B4_U31_RAW_DIR=<каталог сырья> B4_U31_DIFF_SHA256=bcaba550066895a065d416952059b30fe4c1baf146cad295838dcaf088d92a3d B4_U31_OS_INFO="Linux 6.18.33.2-microsoft-standard-WSL2 x86_64 (WSL2)" B4_U31_CPU_INFO="11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz, 16 CPU(s)" node scripts/b4_u31_trigger_table.mjs`; сырьё — `docs/perf/b4_u31_raw/` (`B4_U31_RAW_DIR=docs/perf/b4_u31_raw`).

## B4-29/U31 — триггеры возврата T1/T2/T3 (измерение, без изменения runtime)

Identity: base фичевого дерева = `1e20e9aa` (константа базы измеренного дерева = origin/main после PR #87/#88; текущий HEAD = `1e20e9aa`); git_diff_sha256: T1/T3-сырьё (ut-u31run4/lar2/ms2, старое дерево) = `512508e8`, MT-сырьё ut-u31mt3-* = `bcaba550`; rustc 1.97.0 (2d8144b78 2026-07-07); CPU: 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz, 16 CPU(s); OS: Linux 6.18.33.2-microsoft-standard-WSL2 x86_64 (WSL2).

Прогоны: chunk A = ut-u31run4 (growth16 × {32768,65536,131072} × {sefer,mimalloc}, callgrind), ut-u31lar2/ut-u31ms2 (larson/mstress T=1), ut-u31mt3-* (4 ячейки × 3 повтора = 12 MT-прогонов, счётчики bench-internals, T=2/4; larson-t2-r1 = файл ut-u31mt3-larson-t2-r1b.log — первый r1 упал на компиляции, перезапущен); мутант «всегда пусто» — ut-u31mutant-nopending.log (exit=101).

### T1 — growth16 (callgrind Ir): нижняя граница self-Ir скана, inclusive — верхняя

Числитель (нижняя граница) — self-Ir scan.rs (bin-head discovery-скан) + bitmap_scan.rs (pending sidecar scan), построчная агрегация --auto=yes, без перекрытий. Верхняя граница — inclusive-Ir find_segment_with_free_impl из .annot (ОЦЕНКА границы, включает всё inline- expansion). **Вердикт — только по нижней границе и только на n=131072 (4n).**

| n | Ir Sefer | Ir mimalloc | Sefer/mimalloc | self: scan.rs | self: bitmap_scan.rs | self сумма (нижняя) | доля self | inclusive (верхняя, ОЦЕНКА) | доля inclusive | потолок U31 = bitmap/total (ОЦЕНКА) |
|---|---|---|---|---|---|---|---|---|---|---|
| 32768 | 12 499 110 | 3 430 226 | 3.64× | 155 647 | 3 154 947 | 3 310 594 | 26.49 % (3 310 594/12 499 110) | 5 196 275 | 41.57 % (5 196 275/12 499 110) | 25.24 % (3 154 947/12 499 110) |
| 65536 | 33 895 720 | 6 358 244 | 5.33× | 311 295 | 12 601 347 | 12 912 642 | 38.10 % (12 912 642/33 895 720) | 19 829 747 | 58.50 % (19 829 747/33 895 720) | 37.18 % (12 601 347/33 895 720) |
| 131072 | 105 000 655 | 12 214 979 | 8.60× | 622 591 | 50 368 515 | 50 991 106 | 48.56 % (50 991 106/105 000 655) | 77 408 243 | 73.72 % (77 408 243/105 000 655) | 47.97 % (50 368 515/105 000 655) |

**Вердикт T1: FIRED** — по n=131072 (4n): Sefer/mimalloc = 8.60× (порог ≥ 3.0×), self-доля нижней границы = 48.56 % (50 991 106/105 000 655) (порог ≥ 30 %). Остальные размеры — справочные строки, в вердикт не входят.

Потолок выгоды U31 (ОЦЕНКА, НЕ вердикт) — доля self-Ir bitmap_scan.rs (pending-скан), который U31 мог бы устранить: n=32768: 25.24 % (3 154 947/12 499 110); n=65536: 37.18 % (12 601 347/33 895 720); n=131072: 47.97 % (50 368 515/105 000 655). Bin-head discovery-скан (scan.rs) U31 не устраняет.

### T1-справка: larson/mstress T=1 (self-нижняя / inclusive-верхняя)

| Рукав | Ir всего | self сумма (нижняя) | доля self | inclusive (верхняя, ОЦЕНКА) | доля inclusive | потолок U31 (ОЦЕНКА) |
|---|---|---|---|---|---|---|
| larson T=1 | 30 352 132 | 408 748 | 1.35 % (408 748/30 352 132) | 617 389 | 2.03 % (617 389/30 352 132) | 1.32 % (400 692/30 352 132) |
| mstress T=1 | 40 081 140 | 272 946 | 0.68 % (272 946/40 081 140) | 412 480 | 1.03 % (412 480/40 081 140) | 0.67 % (266 714/40 081 140) |

### T2 — MT (T=2/4): SCAN_WORDS/op по ops_actual и holders==base (формально) + верхний предел no_pending

Нормировка — ops_actual: ФАКТИЧЕСКОЕ число операций по определению workload'а (larson: steps×threads + mailbox-frees; mstress: полраунда free + локальный хвост + mailbox + финальные дрейны), включено в логи пробой. min–max — по 3 повторам.

**Предрегистрированная строка (формальное определение holders==base):** SCAN_WORDS/op ≥ 1.0 И медиана holders==base ≥ 50 %. ВНИМАНИЕ: интерпретация «holders==base ⇒ не было публикаций с прошлого скана» НЕВАЛИДНА для U31 (публикация и освобождение pin возвращают refs к базе — см. раздел T2-семантики в B4_U31_TRIGGER.md).

| Ячейка | SCAN_WORDS/op (медиана) | SCAN_WORDS/op (min–max) | holders==base медиана | holders==base min–max | Вердикт (формальный) | Близость к порогу 50 % |
|---|---|---|---|---|---|---|
| larson T=2 | 2.19 | 2.18–2.35 | 28.40 % (142/500) | 26.71 % (133/498) – 30.55 % (161/527) | НЕТ | не в пределах 20 % от порога |
| larson T=4 | 2.22 | 2.20–2.27 | 27.50 % (278/1011) | 27.43 % (274/999) – 28.14 % (289/1027) | НЕТ | не в пределах 20 % от порога |
| mstress T=2 | 1.07 | 1.07–1.08 | 26.70 % (102/382) | 26.68 % (103/386) – 27.08 % (104/384) | НЕТ | не в пределах 20 % от порога |
| mstress T=4 | 1.56 | 1.23–1.58 | 29.67 % (254/856) | 28.46 % (290/1019) – 34.80 % (349/1003) | НЕТ | не в пределах 20 % от порога |

Пометка «близко к порогу» — вычислено скриптом: медиана holders==base ≥ 50 % × 0.8 = 40 %. holders==base — формальное измерение с невалидной интерпретацией для U31; пометка только о близости ЧИСЛА к порогу, не о валидности измерения.

**Корректный верхний предел активации U31: доля u31_scan_no_pending / u31_discovery_scans — сканы discovery-сайта (owner-drain через drain_segment_sidecar в scan.rs; strict-trim/bounded сканы не входят) без единого ненулевого pending-слова** (именно такие сканы U31-skip смог бы пропустить). ОДНО множество числителя и знаменателя — u31_discovery_scans; holders==base ниже — другое измерение со знаменателем u31_scan_segments:

| Ячейка | no_pending/discovery_scans медиана | min–max |
|---|---|---|
| larson T=2 | 40.36 % (201/498) | 40.12 % (199/496) – 43.62 % (229/525) |
| larson T=4 | 43.12 % (429/995) | 42.20 % (425/1007) – 43.50 % (445/1023) |
| mstress T=2 | 52.11 % (198/380) | 50.79 % (194/382) – 52.60 % (202/384) |
| mstress T=4 | 54.98 % (558/1015) | 51.41 % (438/852) – 55.46 % (554/999) |

Детали по повторам (числитель/знаменатель в каждой доле):

- larson T=2: r1 holders 30.55 % (161/527) (u31_holders_base/u31_scan_segments), no_pending 43.62 % (229/525) (u31_scan_no_pending/u31_discovery_scans), discovery_scans 525, SCAN_WORDS/op 2.35 (ops_actual 212 500); r2 holders 26.71 % (133/498) (u31_holders_base/u31_scan_segments), no_pending 40.12 % (199/496) (u31_scan_no_pending/u31_discovery_scans), discovery_scans 496, SCAN_WORDS/op 2.18 (ops_actual 212 500); r3 holders 28.40 % (142/500) (u31_holders_base/u31_scan_segments), no_pending 40.36 % (201/498) (u31_scan_no_pending/u31_discovery_scans), discovery_scans 498, SCAN_WORDS/op 2.19 (ops_actual 212 500)
- larson T=4: r1 holders 27.43 % (274/999) (u31_holders_base/u31_scan_segments), no_pending 43.12 % (429/995) (u31_scan_no_pending/u31_discovery_scans), discovery_scans 995, SCAN_WORDS/op 2.20 (ops_actual 425 000); r2 holders 28.14 % (289/1 027) (u31_holders_base/u31_scan_segments), no_pending 43.50 % (445/1 023) (u31_scan_no_pending/u31_discovery_scans), discovery_scans 1 023, SCAN_WORDS/op 2.27 (ops_actual 425 000); r3 holders 27.50 % (278/1 011) (u31_holders_base/u31_scan_segments), no_pending 42.20 % (425/1 007) (u31_scan_no_pending/u31_discovery_scans), discovery_scans 1 007, SCAN_WORDS/op 2.22 (ops_actual 425 000)
- mstress T=2: r1 holders 27.08 % (104/384) (u31_holders_base/u31_scan_segments), no_pending 50.79 % (194/382) (u31_scan_no_pending/u31_discovery_scans), discovery_scans 382, SCAN_WORDS/op 1.07 (ops_actual 289 708); r2 holders 26.70 % (102/382) (u31_holders_base/u31_scan_segments), no_pending 52.11 % (198/380) (u31_scan_no_pending/u31_discovery_scans), discovery_scans 380, SCAN_WORDS/op 1.07 (ops_actual 289 708); r3 holders 26.68 % (103/386) (u31_holders_base/u31_scan_segments), no_pending 52.60 % (202/384) (u31_scan_no_pending/u31_discovery_scans), discovery_scans 384, SCAN_WORDS/op 1.08 (ops_actual 289 708)
- mstress T=4: r1 holders 28.46 % (290/1 019) (u31_holders_base/u31_scan_segments), no_pending 54.98 % (558/1 015) (u31_scan_no_pending/u31_discovery_scans), discovery_scans 1 015, SCAN_WORDS/op 1.58 (ops_actual 579 305); r2 holders 34.80 % (349/1 003) (u31_holders_base/u31_scan_segments), no_pending 55.46 % (554/999) (u31_scan_no_pending/u31_discovery_scans), discovery_scans 999, SCAN_WORDS/op 1.56 (ops_actual 579 305); r3 holders 29.67 % (254/856) (u31_holders_base/u31_scan_segments), no_pending 51.41 % (438/852) (u31_scan_no_pending/u31_discovery_scans), discovery_scans 852, SCAN_WORDS/op 1.23 (ops_actual 579 305)

**Вердикт T2: NOT FIRED** — по формальному определению (SCAN_WORDS/op ≥ 1.0 по ops_actual во всех ячейках: да; медиана holders==base ≥ 50 %: нет). Формальная интерпретация holders==base для U31 невалидна — решение принимать по no_pending-доле (см. выше).

### T3 — доля discovery-скана (self, нижняя граница): larson/mstress ≥ 5 % ИЛИ growth16 ≥ 10 %

| Рукав | Ir всего | self сумма (нижняя) | доля self | ≥ порога |
|---|---|---|---|---|
| larson T=1 | 30 352 132 | 408 748 | 1.35 % (408 748/30 352 132) | нет |
| mstress T=1 | 40 081 140 | 272 946 | 0.68 % (272 946/40 081 140) | нет |
| growth16 n=32768 | 12 499 110 | 3 310 594 | 26.49 % (3 310 594/12 499 110) | да (10 %) |
| growth16 n=65536 | 33 895 720 | 12 912 642 | 38.10 % (12 912 642/33 895 720) | да (10 %) |
| growth16 n=131072 | 105 000 655 | 50 991 106 | 48.56 % (50 991 106/105 000 655) | да (10 %) |
| multiseg | — | — | не снимался в этих прогонах | — |

**Вердикт T3: FIRED** (growth16: 3/3 рукавов ≥ 10 %; larson/mstress T=1: 0/2 ≥ 5 %; ветка OR — достаточно одной).

### Оракул активации (вычисляется из COUNTERS, порог ≥ 95 %)

Доля = scan_calls / find_segment_calls — каждый find_segment уходит в discovery-скан. Для MT-прогонов семантика (u31_scan_segments − scan_calls) > 0: u31_scan_segments срабатывает на входе scan_small_from на ВСЕХ owner-drain scan sites, scan_calls — только вызовы find_segment; разница = сканы из drain/free-путей вне find_segment; u31_discovery_scans — только discovery-сайт (drain_segment_sidecar в scan.rs). Assert-ы: u31_scan_segments ≥ scan_calls, discovery_scans ≤ segments, no_pending ≤ discovery_scans, holders_base ≤ segments, num ≤ den.

| Прогон | scan_calls | find_segment_calls | доля | u31_scan_segments | no_pending | holders_base |
|---|---|---|---|---|---|---|
| growth16 n=32768 | 2 048 | 2 048 | 100.00 % | 2 048 | 2 048 | 2 048 |
| growth16 n=65536 | 4 096 | 4 096 | 100.00 % | 4 096 | 4 096 | 4 096 |
| growth16 n=131072 | 8 192 | 8 192 | 100.00 % | 8 192 | 8 192 | 8 192 |
| larson T=1 | 106 | 106 | 100.00 % | 107 | 106 | 107 |
| mstress T=1 | 82 | 82 | 100.00 % | 83 | 82 | 83 |

MT-ячейки (медианы по 3 повторам; доли вычислены из СВОИХ счётчиков, без констант; raw-сырьё ut-u31mt3-*):

| Ячейка | scan_calls | find_segment_calls | доля scan/find | u31_scan_segments | u31_discovery_scans |
|---|---|---|---|---|---|
| larson T=2 | 498 | 498 | 100.00 % | 500 | 498 |
| larson T=4 | 1 007 | 1 007 | 100.00 % | 1 011 | 1 007 |
| mstress T=2 | 382 | 382 | 100.00 % | 384 | 382 |
| mstress T=4 | 999 | 999 | 100.00 % | 1 003 | 999 |

Мутационное доказательство счётчика no_pending: мутант «всегда пусто» краснеет — лог ut-u31mutant-nopending.log, exit=101 (тест u31_no_pending_counts_empty_scans_but_not_published_pending_scans, leg published).

CSV: docs/perf/B4_U31_TRIGGER_summary.csv
