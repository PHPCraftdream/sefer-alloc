# B4 FRAME TAX — разбор налога кадра аллокатора (чанк-отчёт)

Ветка `b4-split` (от main `f93c1b52`). Сырые данные: `docs/perf/_raw_b4_frame_*.log`
(100 логов) + `docs/perf/_raw_b4_frame_*.state` (39 state-файлов с diff-sha256
состояния дерева на момент каждого замера). Таблицы ниже сгенерированы скриптом
`node scripts/b4_frame_table.mjs` (вывод: `docs/perf/B4_FRAME_TAX_tables.md`,
включён в §3–§4 и §8 дословно). Сводный CSV: `docs/perf/B4_FRAME_TAX_summary.csv`.

## Содержание

1. [Резюме](#1-резюме)
2. [Методика](#2-методика)
3. [Принятая цепочка](#3-принятая-цепочка)
4. [NO-GO-ветки и counterfactual-выводы](#4-no-go-ветки-и-counterfactual-выводы)
5. [Изменения по файлам; тесты](#5-изменения-по-файлам-тесты)
6. [Ограничения и риски](#6-ограничения-и-риски)
7. [Что прогнать на CI](#7-что-прогнать-на-ci)
8. [Приложение: сырые артефакты](#8-приложение-сырые-артефакты)

## 1. Резюме

Задача: уменьшить «налог кадра» (frame tax) горячего пути аллокатора — стоимость,
вносимую прологом/эпилогом и живыми значениями в кадрах `w_alloc`/`w_free`
(пример-обёртки `examples/zz_alloc_only.rs`), а также телом вызываемых хвостов.

Принято (все числа ИЗМЕРЕНО, цепочка diff-sha зафиксирована в state-файлах):

| Шаг | diff-sha (полный в §3/state) | Суть | Эффект |
|---|---|---|---|
| N1-alloc (split-a1) | `2992e8c8…` | fallback-рукав → `#[inline(never)] alloc_via_fallback`, exit-escape в хвост | ao alloc 63→55 (−8, −12.7 %); larson 319.4→311.4; mstress 494.1→449.6 |
| N8 n8b+n8c (split-n8-fin) | `d061cdbc…` | hoisted prev_ptr, span-форма сравнения | batch free 173→171; magazine-тело дешевле |
| N9 (split-n9c/n9d) | `639e44c3…` | last-сегмент кэш в segment_table | ao free 115→108; batch free 171→162; const free 74→67; thrash 74→67; larson 311.4→302.8; mstress 447.5→431.5 |
| N4 (split-n4) | `a0daba8c…` (итог) | u64-пословный битмап | ao free 108→105 (итого −10, −8.7 %); batch free 159 (−8.6 %); const/thrash free 66; larson 302.5; mstress 422.2 |

Итоговое Δ (база → N4-финал): ao free 115→105 (−8.7 %), EstCycles free
120.6→110.2; batch free 174→159 (−8.6 %; в сыром логе базы 173 — см. аномалии §6);
larson 319.4→302 (−5.4 %); mstress 494.1→422 (−14.6 %); const free 74→66;
flush-событие 509.1→421.0 Ir (−88; значения из записи владельца, сырого лога с
этим числом нет — см. §6).

Отклонено (NO-GO, §4): N1-free (4 варианта), N2, N3, N5/N5b, N6, N7/N7b, N8a, N8d.
Открытый пункт: n4b «бит из указателя» на alloc-hit — не измерен (бюджет замеров),
оценка −2…−4 Ir.

## 2. Методика

- Судьи: `prof_ao` (ao: `w_alloc`/`w_free`, Ir/op), `prof_batch` (batch:
  `zz_batch_probe`, Ir/call, 256/раунд), `prof_const` (const-режим, 16/64-байт
  варианты, Ir/op + EstCycles), `prof_batchl` (`zz_batch_large_probe`, Ir/call,
  258/раунд), `prof_thrash` (`zz_thrash`, Ir/op). larson/mstress — контроль
  регресса, допуск ≤ +1 %.
- EstCycles = Ir + 10·D1m + 100·LLm (модель iai-callgrind; строки `cache:
  D1m/op=… LLm/op=… EstCycles/op=…` в логах).
- **iai НЕ видит налог кадра Windows**: счётчики — эмуляция на Linux (WSL), где
  `process_exit ≡ false` и Win64-часть пролога/эпилога (shadow-space, homes for
  register args) не генерируется/не считается. Поэтому (а) IR-дельты здесь —
  нижняя оценка реального выигрыша кадра; (б) обязательна asm-проверка на
  x86_64-pc-windows-msvc (см. §7).
- sha-дисциплина: перед каждым замером фиксировался `git diff | sha256sum` +
  список изменённых/untracked файлов — сохранён в `docs/perf/_raw_b4_frame_*.state`.
  Финальная идентичность подтверждена: текущий `git diff | sha256sum` =
  `a0daba8c4ddcbd09830c9cc9314f04ce9d96faac286b6da426ccc13a490b4448`
  (совпадает с `_raw_b4_frame_split-n4.state`); untracked
  `tests/bitmap_word_boundaries.rs` sha256 =
  `3afe06470fc2b5653bacecbb30485324071ff414dfa937cea6ac5a4d8cf2bae4`
  (в diff не входит — упомянут отдельно).
- Ограничение среды: замеры iai выполнены под WSL/Linux; process_exit-ветка
  отключена (`process_exit ≡ false`), Win64-часть кадра не видна — asm-проверка
  на CI обязательна.

## 3. Принятая цепочка

<!-- Сгенерировано scripts/b4_frame_table.mjs из docs/perf/_raw_b4_frame_*.log; не редактировать вручную -->

### 3.0 Итоговые Δ (база main f93c1b52 → N4-финал), ИЗМЕРЕНО

| Метрика | база | финал | Δ |
|---|---|---|---|
| ao alloc Ir/op | 63 | 55 | -8 (-12.7 %) |
| ao free Ir/op | 115 | 105 | -10 (-8.7 %) |
| ao alloc EstCycles/op | — | 62 | — |
| ao free EstCycles/op | — | 110.2 | — |
| batch alloc Ir/call | 136 | 126 | -10 (-7.4 %) |
| batch free Ir/call | 173 | 159 | -14 (-8.1 %) |
| const alloc Ir/op | 26 | 26 | 0 (0.0 %) |
| const free Ir/op | 74 | 66 | -8 (-10.8 %) |
| batchl alloc Ir/call | 133 | 131 | -2 (-1.5 %) |
| batchl free Ir/call | 175 | 172 | -3 (-1.7 %) |
| thrash alloc Ir/op | 26 | 26 | 0 (0.0 %) |
| thrash free Ir/op | 74 | 66 | -8 (-10.8 %) |
| larson Ir/op | 319.4 | 302.5 | -16.9 (-5.3 %) |
| mstress Ir/op | 494.1 | 422.2 | -71.9 (-14.6 %) |

### 3.1 Принятая цепочка (все числа ИЗМЕРЕНО — извлечены скриптом из `docs/perf/_raw_b4_frame_*.log`)

| Шаг | diff-sha | ao a/f Ir | ao a/f EstCycles | batch a/f Ir/call | const a/f | batchl a/f | thrash a/f | larson | mstress |
|---|---|---|---|---|---|---|---|---|---|
| база main f93c1b52 | (пустой diff) | 63/115 | —/— | 136/173 | 26/74 | 133/175 | 26/74 | 319.4 | 494.1 |
| N1-alloc (split-a1) | 2992e8c8… | 55/115 | —/— | 127/173 | —/— | — | — | 311.4 | 449.6 |
| N8 n8b+n8c (split-n8-fin) | d061cdbc… | 55/115 | —/— | 127/171 | —/— | — | — | 311.1 | 447.5 |
| N9 (split-n9c/n9d) | 639e44c3… | 55/108 | 62.3/113.2 | 127/162 | 26/67 | 133/175 | 26/67 | 302.8 | 431.5 |
| N4 (split-n4, итог) | a0daba8c… | 55/105 | 62/110.2 | 126/159 | 26/66 | 131/172 | 26/66 | 302.5 | 422.2 |

### 3.2 Per-шаг детали (логи)

- **база main f93c1b52** (пустой diff): ao `_raw_b4_frame_split-base.log`
  (63/115), batch `_raw_b4_frame_split-base-b2.log` (136/173), batchl
  `_raw_b4_frame_split-base-bl.log` (133/175), const
  `_raw_b4_frame_split-base-const.log` (26/74), thrash
  `_raw_b4_frame_split-base-th.log` (26/74), larson
  `_raw_b4_frame_split-base-l.log` (319.4), mstress `_raw_b4_frame_split-base-m.log`
  (494.1), EstCycles free 120.6 — `_raw_b4_frame_split-base-cs.log`.
- **N1-alloc (split-a1)**, sha `2992e8c8…`: `_raw_b4_frame_split-a1.log` (55/115),
  `-batch` (127/173), `-larson` (311.4), `-mstress` (449.6). Ре-прогон
  `_raw_b4_frame_split-a1b.log` (55/115) — воспроизводимость.
- **N8 (split-n8-fin)**, sha `d061cdbc…`: промежуточные `_raw_b4_frame_split-n8b*`
  / `-n8c*` (ao 55/115, batch 127/171), судьи `_raw_b4_frame_split-base-n8fin-l.log`
  (larson 311.4) и `_raw_b4_frame_split-base-n8fin-m.log` (mstress 447.5).
- **N9 (split-n9c/n9d)**, sha `639e44c3…` (n9d) / `a8d03a45…` (n9c): ao
  `_raw_b4_frame_split-n9c.log` (55/108, EstCycles 62.3/113.2), batch `-n9c.batch`
  (127/162), batchl `-n9c.batchl` (133/175), const `-n9c.const` (26/67), thrash
  `-n9c-th` (26/67), larson `-n9c.larson` (302.8), mstress `-n9c.mstress` (431.5);
  вариант n9d: `-n9d.log` (55/108), `-n9d.batchl` (133/175).
- **N4 (split-n4, итог)**, sha `a0daba8c…`: ao `_raw_b4_frame_split-n4.log`
  (55/105, EstCycles 62.0/110.2), batch `-n4-b` (126/159), batchl `-n4-bl`
  (131/172), const `-n4-c` (26/66), thrash `-n4-th` (26/66), larson `-n4-l`
  (302.5), mstress `-n4-m` (422.2).

## 4. NO-GO-ветки и counterfactual-выводы

### 4.1 NO-GO-сводка (все числа ИЗМЕРЕНО — извлечены скриптом из тех же сырых логов)

| Шаг | ao a/f Ir | batch a/f | const a/f | batchl a/f | larson | mstress | причина NO-GO |
|---|---|---|---|---|---|---|---|
| N1a (cnt-перенос) | 63/115 | —/— | —/— | — | 316.4 | — | larson 316 — регресс против a1 311.4; перенос счётчика не окупается |
| N1b (хвосты) | 63/110 | —/— | —/— | — | 322.1 | — | w_free 110 (−5), fn w_free 10.9, но larson 322 — регресс |
| N1c (контекст хвостам) | 63/111 | —/— | —/— | — | 320.9 | — | w_free 111; larson 320 — регресс |
| N1d (counterfactual) | 63/110 | —/— | —/— | — | 320.1 | — | w_free 110 ≈ n1c 111 → контекст не при чём (c≈d); larson 320 |
| N2 (inline free-хвоста) | 55/115 | 127/176 | —/— | — | 308 | — | fn w_free 15.0 не сдвинулся — кадр держат живые значения guard-цепи (~8-9 значений) + TLS-диспетчеризация, не вызовы; batch free 176 |
| N3 (root-only Tier-1) | 55/116 | 127/170 | 26/75 | — | — | — | +1 Ir/free стабильно (116; n3b 116); const free 75; batch −1 не компенсирует |
| N6 (enum ClassIdx) | 61/113 | 134/171 | —/— | — | 315.4 | — | −2/−2 Ir (61/113), но пороги не достигнуты: w_free 113 > 112, cnt-строки 5.0/4.0 > 3; range-метаданные сворачиваются лишь частично |
| N7 (refill до середины) | 50/101 | 141/159 | 26/66 | 146/172 | 293.3 | 408 | активация −52 %/−38 %, ao 50/101 (−5/−4), larson 293 (−9), НО batch alloc 141 (+15), batchl alloc 146 (+15) → NO-GO; пин r6_t39 не тронут |
| N7b (refill, вариант b) | 52/102 | 131/159 | 26/66 | 137/172 | 297.7 | 422.2 | ao 52/102, larson 297 (−5), mstress 422 (−72), НО batch alloc 131 (+5), batchl alloc 137 (+11) → NO-GO |
| N8a (слияние поиска runs) | 55/116 | 127/175 | —/— | — | — | — | +90 Ir/событие (codegen): ao free 116, batch free 175 |
| N8d (маска) | —/— | 127/171 | —/— | — | — | — | −6.6 Ir/событие < порога 8 |
| N5 (LUT класс+divides) | 55/113 | 126/166 | 26/66 | — | — | — | +8/+9 Ir ao (free 113, EstCycles 118.2); const-режим сворачивает полностью (расхождения режимов нет) |
| N5b (counterfactual) | 55/114 | 126/167 | 26/66 | — | — | — | free 114 ≈ n5 113 → counterfactual n5≡n5b: совместная загрузка divisibility не подтвердилась |


Сводная таблица (числа — вывод скрипта, §4.1 выше). Ключевые научные результаты:

1. **Кадр free держат живые значения guard-цепи, а не вызовы** (N2): инlining
   free-хвоста не сдвинул fn w_free (15.0 Ir), batch free даже вырос до 176.
   Кадр w_free держат ~8–9 живых значений guard-цепи + TLS-диспетчеризация.
2. **Передача контекста хвостам бесплатна** (N1c vs N1d): w_free 111 vs 110
   (c≈d) — контекст не при чём; регресс larson у N1b..d (322/320.9/320.1 против
   311.4 у a1) — это следствие переноса счётчика (N1a), а не контекста.
3. **Механизм N3 отрицательный**: root-only Tier-1 стабильно стоит +1 Ir/free
   (116; n3b 116; const 75) — экономия batch −1 не компенсирует.
4. **N5-LUT дороже существующих таблиц**: +8/+9 Ir ao (free 113/114,
   EstCycles 118.2); counterfactual n5≡n5b — совместная загрузка divisibility не
   подтвердилась; const-режим сворачивает LUT полностью (расхождений режимов нет).
5. N6 (enum ClassIdx): −2/−2 Ir (61/113), но пороги не достигнуты (w_free 113 >
   112; cnt-строки 5.0/4.0 > 3) — кандидат при пересмотре порогов.
6. N7/N7b: активация −52 %/−38 %, ao −5/−4, larson −9/−5, mstress −87/−72 —
   НО batch alloc +15/+5, batchl alloc +15/+11 → NO-GO (см. §6, решение владельца).
7. N8a (слияние поиска runs): +90 Ir/событие (codegen-регресс). N8d (маска):
   −6.6 < порога 8.

Прочие NO-GO строки: n1a (cnt-перенос, larson 316), n1b/n1c/n1d (см. выше), n2, n3.

### 4.3 Исправление базы N7/N7b (append-only, 2026-10-21; ИЗМЕРЕНО, те же сырые логи)

База N7/N7b — финал N4 (`_raw_b4_frame_split-n7-base.log` = ao 55/105, как `-n4.log`). Δ ниже — к N4; столбец «к main» — к базе `f93c1b52`, от которой были взяты mstress-цифры §4.1/п. 6.

| Шаг | метрика | N4 | шаг | Δ к N4 | Δ к main |
|---|---|---|---|---|---|
| N7 | ao alloc Ir/op | 55 | 50 | −5 | −13 |
| N7 | ao free Ir/op | 105 | 101 | −4 | −14 |
| N7 | batch alloc Ir/call | 126 | 141 | +15 | +5 |
| N7 | batchl alloc Ir/call | 131 | 146 | +15 | +13 |
| N7 | larson Ir/op | 302.5 | 293.3 | −9.2 | −26.1 |
| N7 | mstress Ir/op | 422.2 | 408 | −14.2 | −86.1 |
| N7b | ao alloc Ir/op | 55 | 52 | −3 | −11 |
| N7b | ao free Ir/op | 105 | 102 | −3 | −13 |
| N7b | batch alloc Ir/call | 126 | 131 | +5 | −5 |
| N7b | batchl alloc Ir/call | 131 | 137 | +6 | +4 |
| N7b | larson Ir/op | 302.5 | 297.7 | −4.8 | −21.7 |
| N7b | mstress Ir/op | 422.2 | 422.2 | 0 | −71.9 |

Следствие: п. 6 выше и строки N7/N7b в §4.1 смешивали базы — ao/batch/larson
считались к N4, а mstress «−87/−72» — к main `f93c1b52`. К фактической базе N4
mstress N7 −14.2, N7b 0; batchl alloc N7b +6 (не +11); ao N7b −3/−3. Вердикт
NO-GO не меняется: batch/batchl alloc — скалярный `GlobalAlloc`-путь, регресс
+5…+15 реальный. Адаптивный вариант — N7a (`docs/perf/OPEN_ITEMS.md` item 102(б)).
Проверено по сырью: `-n7-base.log`, `-n4-m.log` (422.2), `-n7-mstress.log`
(408.0), `-n7b-mstress.log` (422.2), `-base-m.log` (494.1); ревью
rush reviewer + @oxx (2026-10-20).

## 5. Изменения по файлам; тесты

- `src/global/sefer_alloc/global_alloc.rs:35-54,104` — N1: fallback-рукав
  вынесен в `#[inline(never)] alloc_via_fallback`; exit-escape внутри хвоста.
- ~~`src/global/tls_heap.rs:418,489,496` — N1: `bind_slow_tagged_with_config(&LargeCacheConfig)`.~~ Снято при ревью (см. §9): замер без этой правки даёт те же 55/105 Ir и 62.0/110.2 EstCycles, а файл входит в область десяти строк реестра доказательств.
- `src/registry/heap_core/alloc/hot.rs:186-199,364-381` — N1: drain слит в
  `alloc_large_without_magazine`, Large-рукава терминальны.
- `src/alloc_core/small/alloc_core_small_magazine.rs:616-669` — N8: hoisted
  prev_ptr (n8b), span-форма `(off−ps) ≥ bump.saturating_sub(ps)` (n8c), комментарии.
- `src/alloc_core/segment/segment_table/segment_table_impl.rs:175,236-247,298-300,813-835,869-873,920-924` —
  N9: last-сегмент кэш (EMPTY=`usize::MAX`), fill при `root.addr()==key.addr()`,
  инвалидация в lockstep с own_cache.
- `src/alloc_core/segment/bitmap/segment_bitmap.rs:83-160` — N4: u64-пословные
  test/set/clear через `Node::read_struct`/`write_struct`, from_le/to_le +
  BE-гейты, const-assert.
- `tests/bitmap_word_boundaries.rs` (новый, N4; sha256 `3afe0647…`, untracked —
  в diff `a0daba8c…` не входит): 4 теста границ слов (bit 7/8; 63/64; последний
  footprint; эквивалентность байтовой форме); мутант (word+1) краснил все 4.
  Строка 172: `#[allow(clippy::manual_is_multiple_of)]` (фикс-чанк).

Прогоны тестов: адресные тесты N8/N9/N4 — зелёные; финальный гейт — 10 таргетов
зелёные; `clippy` production/internals ± `--tests` — чисто; hardened-clippy
падает на БАЗЕ (pre-existing): unused import `NonNull` в
`src/alloc_core/small/alloc_core_small_diag.rs:22` — вне скоупа чанка.

## 6. Ограничения и риски

- **n4b «бит из указателя» на alloc-hit не измерен** (бюджет замеров); оценка
  ox −2…−4 Ir. Перед включением — обязательный замер по методике §2.
- **Windows-asm не проверен**: iai-числа получены под WSL/Linux; Win64-часть
  кадра (shadow-space) не видна. Трипвайр-проверка asm — §7.
- **batch-policy N7 — «решение владельца»**: batchl alloc +11 при larson −5
  (n7b) — если владелец хочет larson-режим (низкая латентность одиночных
  операций), это отдельное сознательное решение; текущий вердикт NO-GO
  защищает batch-пропускную способность.
  **Исправление 2026-10-21 (append-only, §4.3):** у N7b к базе N4 — larson
  −4.8, mstress 0, batchl alloc +6; выигрыша mstress у N7b нет. Решение
  владельца по консультации: N7/N7b в этой форме — NO-GO, далее адаптивный
  N7a поверх S1/D1′.
- **N6** (−2 Ir) — кандидат при пересмотре порогов (w_free 113 > 112,
  cnt-строки 5.0/4.0 > 3).
- Аномалии данных: (а) в сыром логе базы batch = 136/173
  (`_raw_b4_frame_split-base-b2.log`), в записи владельца 137/174 — расхождение
  1 Ir, вероятно повторный прогон; скрипт выводит значение из лога. (б)
  flush-событие 509.1→421.0 Ir не представлено ни одним `_raw_b4_frame_*`-логом
  — значения из записи владельца, помечены ОЦЕНКА (не подтверждено сырыми
  данными). (в) `_raw_b4_frame_split-base-cs.log` (источник EstCycles free
  120.6) показывает ao-заголовки 55/115 — контрольный прогон сделан уже поверх
  N1-состояния; Ir-значения из него не используются. (г) diff-sha N9-шага:
  n9d `639e44c3…`, n9c `a8d03a45…` (n9c — промежуточное состояние).

## 7. Что прогнать на CI

- iai A/B ячейки: `dealloc_free_only_16b_n16`, `dealloc_free_only_16b_n17`,
  `dealloc_free_only_16b_n32`, `cold_alloc_free_256x16b`,
  `recycle_alloc_free_256x16b`, `dealloc_free_only_1088_16b_n1024`,
  `small_churn_16b`, `churn_256b`, `aligned_churn_640b_a128`, `growth16_*`,
  `oscillating_live_set_16b`.
- asm-трипвайр `w_alloc`/`w_free` для `x86_64-pc-windows-msvc`: первая
  инструкция не `push`/`sub rsp`; суммарно ≤ 2 push/pop в кадре.
- hardened-ряд (clippy) — после выноса pre-existing падения
  (`alloc_core_small_diag.rs:22`, unused import `NonNull`) в отдельный фикс.
- mimalloc-контроль (`_raw_b4_frame_split-base-mi.log` — прогон не дал
  MARGINAL-строки; перезапустить).

## 8. Приложение: сырые артефакты

Логи (100):

- `docs/perf/_raw_b4_frame_split-a1-batch.log`
- `docs/perf/_raw_b4_frame_split-a1-larson.log`
- `docs/perf/_raw_b4_frame_split-a1-mstress.log`
- `docs/perf/_raw_b4_frame_split-a1.log`
- `docs/perf/_raw_b4_frame_split-a1b.log`
- `docs/perf/_raw_b4_frame_split-base-b.log`
- `docs/perf/_raw_b4_frame_split-base-b2.log`
- `docs/perf/_raw_b4_frame_split-base-bl.log`
- `docs/perf/_raw_b4_frame_split-base-const.log`
- `docs/perf/_raw_b4_frame_split-base-cs.log`
- `docs/perf/_raw_b4_frame_split-base-l.log`
- `docs/perf/_raw_b4_frame_split-base-m.log`
- `docs/perf/_raw_b4_frame_split-base-mi.log`
- `docs/perf/_raw_b4_frame_split-base-n8fin-l.log`
- `docs/perf/_raw_b4_frame_split-base-n8fin-m.log`
- `docs/perf/_raw_b4_frame_split-base-th.log`
- `docs/perf/_raw_b4_frame_split-base.log`
- `docs/perf/_raw_b4_frame_split-n1a-batch.log`
- `docs/perf/_raw_b4_frame_split-n1a-larson.log`
- `docs/perf/_raw_b4_frame_split-n1a.log`
- `docs/perf/_raw_b4_frame_split-n1b-batch.log`
- `docs/perf/_raw_b4_frame_split-n1b-larson.log`
- `docs/perf/_raw_b4_frame_split-n1b.log`
- `docs/perf/_raw_b4_frame_split-n1c-batch.log`
- `docs/perf/_raw_b4_frame_split-n1c-larson.log`
- `docs/perf/_raw_b4_frame_split-n1c.log`
- `docs/perf/_raw_b4_frame_split-n1d-batch.log`
- `docs/perf/_raw_b4_frame_split-n1d-larson.log`
- `docs/perf/_raw_b4_frame_split-n1d.log`
- `docs/perf/_raw_b4_frame_split-n2-batch.log`
- `docs/perf/_raw_b4_frame_split-n2-larson.log`
- `docs/perf/_raw_b4_frame_split-n2.log`
- `docs/perf/_raw_b4_frame_split-n3-bl.log`
- `docs/perf/_raw_b4_frame_split-n3.batch.log`
- `docs/perf/_raw_b4_frame_split-n3.const.log`
- `docs/perf/_raw_b4_frame_split-n3.log`
- `docs/perf/_raw_b4_frame_split-n3b.log`
- `docs/perf/_raw_b4_frame_split-n4-b.log`
- `docs/perf/_raw_b4_frame_split-n4-bl.log`
- `docs/perf/_raw_b4_frame_split-n4-c.log`
- `docs/perf/_raw_b4_frame_split-n4-l.log`
- `docs/perf/_raw_b4_frame_split-n4-m.log`
- `docs/perf/_raw_b4_frame_split-n4-th.log`
- `docs/perf/_raw_b4_frame_split-n4.log`
- `docs/perf/_raw_b4_frame_split-n5-b.log`
- `docs/perf/_raw_b4_frame_split-n5-c.log`
- `docs/perf/_raw_b4_frame_split-n5.log`
- `docs/perf/_raw_b4_frame_split-n5b-b.log`
- `docs/perf/_raw_b4_frame_split-n5b-c.log`
- `docs/perf/_raw_b4_frame_split-n5b.log`
- `docs/perf/_raw_b4_frame_split-n6-batch.log`
- `docs/perf/_raw_b4_frame_split-n6-larson.log`
- `docs/perf/_raw_b4_frame_split-n6.log`
- `docs/perf/_raw_b4_frame_split-n6pre-batch.log`
- `docs/perf/_raw_b4_frame_split-n7-ao.log`
- `docs/perf/_raw_b4_frame_split-n7-base-b.log`
- `docs/perf/_raw_b4_frame_split-n7-base.log`
- `docs/perf/_raw_b4_frame_split-n7-batch.log`
- `docs/perf/_raw_b4_frame_split-n7-batchl.log`
- `docs/perf/_raw_b4_frame_split-n7-const.log`
- `docs/perf/_raw_b4_frame_split-n7-larson.log`
- `docs/perf/_raw_b4_frame_split-n7-mstress.log`
- `docs/perf/_raw_b4_frame_split-n7-thrash.log`
- `docs/perf/_raw_b4_frame_split-n7b-ao.log`
- `docs/perf/_raw_b4_frame_split-n7b-batch.log`
- `docs/perf/_raw_b4_frame_split-n7b-batchl.log`
- `docs/perf/_raw_b4_frame_split-n7b-const.log`
- `docs/perf/_raw_b4_frame_split-n7b-larson.log`
- `docs/perf/_raw_b4_frame_split-n7b-mstress.log`
- `docs/perf/_raw_b4_frame_split-n7b-thrash.log`
- `docs/perf/_raw_b4_frame_split-n8-base-ao.log`
- `docs/perf/_raw_b4_frame_split-n8-base.log`
- `docs/perf/_raw_b4_frame_split-n8-fin-larson.log`
- `docs/perf/_raw_b4_frame_split-n8-fin-mstress.log`
- `docs/perf/_raw_b4_frame_split-n8a-ao.log`
- `docs/perf/_raw_b4_frame_split-n8a.log`
- `docs/perf/_raw_b4_frame_split-n8b-ao.log`
- `docs/perf/_raw_b4_frame_split-n8b.log`
- `docs/perf/_raw_b4_frame_split-n8c-ao.log`
- `docs/perf/_raw_b4_frame_split-n8c.log`
- `docs/perf/_raw_b4_frame_split-n8c2-ao.log`
- `docs/perf/_raw_b4_frame_split-n8c2.log`
- `docs/perf/_raw_b4_frame_split-n8d.log`
- `docs/perf/_raw_b4_frame_split-n9-th.log`
- `docs/perf/_raw_b4_frame_split-n9.batch.log`
- `docs/perf/_raw_b4_frame_split-n9.batchl.log`
- `docs/perf/_raw_b4_frame_split-n9.const.log`
- `docs/perf/_raw_b4_frame_split-n9.larson.log`
- `docs/perf/_raw_b4_frame_split-n9.log`
- `docs/perf/_raw_b4_frame_split-n9.mstress.log`
- `docs/perf/_raw_b4_frame_split-n9b.batchl.log`
- `docs/perf/_raw_b4_frame_split-n9c-th.log`
- `docs/perf/_raw_b4_frame_split-n9c.batch.log`
- `docs/perf/_raw_b4_frame_split-n9c.batchl.log`
- `docs/perf/_raw_b4_frame_split-n9c.const.log`
- `docs/perf/_raw_b4_frame_split-n9c.larson.log`
- `docs/perf/_raw_b4_frame_split-n9c.log`
- `docs/perf/_raw_b4_frame_split-n9c.mstress.log`
- `docs/perf/_raw_b4_frame_split-n9d.batchl.log`
- `docs/perf/_raw_b4_frame_split-n9d.log`

State-файлы (39; фиксируют diff-sha256 состояния дерева на момент каждого замера):

- `docs/perf/_raw_b4_frame_fix-clippy.state`
- `docs/perf/_raw_b4_frame_split-a1.state`
- `docs/perf/_raw_b4_frame_split-a1b.state`
- `docs/perf/_raw_b4_frame_split-base-b2.state`
- `docs/perf/_raw_b4_frame_split-base-bl.state`
- `docs/perf/_raw_b4_frame_split-base-const.state`
- `docs/perf/_raw_b4_frame_split-base-cs.state`
- `docs/perf/_raw_b4_frame_split-base-n8fin-l.state`
- `docs/perf/_raw_b4_frame_split-base-n8fin-m.state`
- `docs/perf/_raw_b4_frame_split-base-th.state`
- `docs/perf/_raw_b4_frame_split-n1.state`
- `docs/perf/_raw_b4_frame_split-n2.state`
- `docs/perf/_raw_b4_frame_split-n3-bl.state`
- `docs/perf/_raw_b4_frame_split-n3.state`
- `docs/perf/_raw_b4_frame_split-n3b.state`
- `docs/perf/_raw_b4_frame_split-n4-b.state`
- `docs/perf/_raw_b4_frame_split-n4-bl.state`
- `docs/perf/_raw_b4_frame_split-n4-c.state`
- `docs/perf/_raw_b4_frame_split-n4-l.state`
- `docs/perf/_raw_b4_frame_split-n4-m.state`
- `docs/perf/_raw_b4_frame_split-n4-th.state`
- `docs/perf/_raw_b4_frame_split-n4.state`
- `docs/perf/_raw_b4_frame_split-n5.state`
- `docs/perf/_raw_b4_frame_split-n5b.state`
- `docs/perf/_raw_b4_frame_split-n6.state`
- `docs/perf/_raw_b4_frame_split-n7-base.state`
- `docs/perf/_raw_b4_frame_split-n7.state`
- `docs/perf/_raw_b4_frame_split-n7b.state`
- `docs/perf/_raw_b4_frame_split-n8-base.state`
- `docs/perf/_raw_b4_frame_split-n8-fin.state`
- `docs/perf/_raw_b4_frame_split-n8a.state`
- `docs/perf/_raw_b4_frame_split-n8b.state`
- `docs/perf/_raw_b4_frame_split-n8c.state`
- `docs/perf/_raw_b4_frame_split-n8d.state`
- `docs/perf/_raw_b4_frame_split-n9-th.state`
- `docs/perf/_raw_b4_frame_split-n9.state`
- `docs/perf/_raw_b4_frame_split-n9b.state`
- `docs/perf/_raw_b4_frame_split-n9c.state`
- `docs/perf/_raw_b4_frame_split-n9d.state`

## 9. Дополнение оркестратора: многосегментный thrash64 (цена промаха N9)

Независимое ревью (`sol_ox`, §2.4) показало, что проба `prof_thrash` из §2 вакуумна
для N9: 256 окон × 16 блоков × 16 Б = 64 КиБ — один сегмент, поэтому её числа
совпадают с константной пробой (26/74), а промах кэша последнего сегмента и
`own_cache` не измерялся. Добавлен судья `thrash64`: 64 окна по 16 блоков × 16 Б,
после каждого окна живой наполнитель ≈4 МиБ из блоков по 8 КиБ, чтобы соседние
окна легли в разные сегменты; цикл — free случайного блока случайного окна, затем
alloc (LIFO-magazine возвращает тот же блок). Оракул активации: проба считает
различные сегменты блоков окон и для sefer требует ≥ 32 (иначе `assert!` роняет
прогон); в обоих плечах sefer — 64 из 64 (ИЗМЕРЕНО). Для mimalloc оракул не
применим (его «сегмент» по маске 4 МиБ не совпадает с нашим), число 1 в таблице —
не активация.

Воспроизведение: исходник пробы — ниже (в репозитории его нет, как и у остальных
проб серии); `bash .rush/wsl/prof_thrash64_dir.sh <каталог-дерева> <метка> [sefer|mimalloc]`
(callgrind, маргинальные Ir/op по `w_alloc`/`w_free` между 50 000 и 100 000 шагов,
второй проход `--cache-sim=yes`). Таблица выведена скриптом
`node scripts/b4_frame_thrash64.mjs` из сырых логов (он же пишет
`docs/perf/B4_FRAME_TAX_thrash64_summary.csv` и проверяет оракул):

| Плечо | Источник | Сегментов (оракул) | w_alloc Ir/op | w_free Ir/op | w_alloc EstCycles | w_free EstCycles | Лог |
|---|---|---:|---:|---:|---:|---:|---|
| main f93c1b52 | f93c1b52 (src clean) | 64 | 25 | 99 | 25 | 108.6 | `docs/perf/_raw_b4_frame_t64-base.log` |
| b4-split final | f93c1b52 + diff sha256 1638450c744e6adf86c5763d6be9be00c8fcfcc3e2c0229c240fd2a8b1922a97 | 64 | 26 | 103 | 26 | 112.6 | `docs/perf/_raw_b4_frame_t64-split.log` |
| mimalloc | f93c1b52 harness | 1 | 212 | 23 | 212.4 | 23 | `docs/perf/_raw_b4_frame_t64-mi2.log` |
| Δ b4-split − main | | | +1 | +4 | | | |

Вывод (ИЗМЕРЕНО): на промахо-тяжёлой нагрузке цепочка b4-split дороже базы на
+4 Ir/free и +1 Ir/alloc — это цена N9 (сравнение memo перед `own_cache` и
перезапись memo на каждом выигрыше `hash_find`; построчно видна строка
`if self.last_key == key.addr()` = 2.0 Ir/op). На нагрузках с локальностью N9 даёт
−7 Ir/free (§3), larson −8.3 Ir/step, mstress −16 Ir/step. Принято как осознанный
компромисс; уточнение политики заполнения memo (чтобы промах не платил запись)
вынесено в следующий раунд. mimalloc на этой пробе: alloc 212 Ir/op (уходит в
`_mi_malloc_generic` — свободный блок лежит на другой странице), free 23; сумма
alloc+free: sefer 129 против mimalloc 235.

Исходник пробы `thrash64_probe.rs` (подключается скриптом как `examples/zz_thrash64.rs`; плечи sefer мерились версией, где `assert!` активации был безусловным — флаг `check` добавлен позже только для плеча mimalloc, путь sefer не изменился):

```text
// Multi-segment thrash probe (N9 miss-path judge): 64 windows x 16 blocks x 16 B,
// each window followed by a ~4 MiB live filler of 8 KiB blocks, so consecutive
// windows land in different segments. Churn: free a random block of a random
// window, then alloc (LIFO magazine hands the same block back). The free side
// therefore resolves a random one of >= 32 segments per op.
use core::alloc::{GlobalAlloc, Layout};
#[inline(never)]
fn w_alloc<A: GlobalAlloc>(a: &A, l: Layout) -> *mut u8 { unsafe { a.alloc(l) } }
#[inline(never)]
fn w_free<A: GlobalAlloc>(a: &A, p: *mut u8, l: Layout) { unsafe { a.dealloc(p, l) } }
const WINDOWS: usize = 64;
const PER_WIN: usize = 16;
const SZ: usize = 16;
const FILL_SZ: usize = 8192;
const FILL_PER_WIN: usize = 512; // 4 MiB
fn run<A: GlobalAlloc>(a: &A, steps: usize, check: bool) {
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    let mut rnd = || { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x };
    let l = Layout::from_size_align(SZ, 8).unwrap();
    let lf = Layout::from_size_align(FILL_SZ, 8).unwrap();
    let mut slots: Vec<Vec<*mut u8>> = Vec::with_capacity(WINDOWS);
    let mut fill: Vec<*mut u8> = Vec::with_capacity(WINDOWS * FILL_PER_WIN);
    for _ in 0..WINDOWS {
        let mut w = Vec::with_capacity(PER_WIN);
        for _ in 0..PER_WIN { w.push(w_alloc(a, l)); }
        slots.push(w);
        for _ in 0..FILL_PER_WIN { fill.push(unsafe { a.alloc(lf) }); }
    }
    let mut segs: Vec<usize> = slots.iter().flatten().map(|&p| (p as usize) >> 22).collect();
    segs.sort_unstable();
    segs.dedup();
    assert!(!check || segs.len() >= 32, "activation: only {} distinct segments", segs.len());
    eprintln!("[thrash64] distinct 16B-window segments: {}", segs.len());
    for _ in 0..steps {
        let wi = (rnd() as usize) % WINDOWS;
        let si = (rnd() as usize) % PER_WIN;
        let p = slots[wi][si];
        w_free(a, p, l);
        slots[wi][si] = w_alloc(a, l);
    }
    for w in slots { for p in w { w_free(a, p, l); } }
    for p in fill { unsafe { a.dealloc(p, lf) } }
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let steps: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    match args.get(1).map(String::as_str).unwrap_or("sefer") {
        "mimalloc" => run(&mimalloc::MiMalloc, steps, false),
        _ => { let s = sefer_alloc::SeferAlloc::new(); run(&s, steps, true) }
    }
}
```

Правка оркестратора при ревью: `src/alloc_core/segment/segment_table/segment_table_impl.rs`
после N9 вырос до 1046 строк (лимит 1000); free-list-хелперы перенесены без
изменений в `src/alloc_core/segment/segment_table/hash.rs` (видимость полей
`free_list`/`free_top` и трёх функций — `pub(super)`), каталог не растёт.

Ещё две правки оркестратора после первого CI PR #82:

- `src/global/tls_heap.rs` возвращён к базе (передача `LargeCacheConfig` по
  ссылке в холодный `bind_slow_tagged_with_config`). Замер без неё
  (`docs/perf/_raw_b4_frame_split-tlsrev.log`, `prof_ao`, production):
  w_alloc 55, w_free 105, EstCycles 62.0 / 110.2 — ровно как у финала §3, то
  есть правка ничего не давала, а её файл входит в исходную область десяти
  строк реестра доказательств (R4b-03…R4c-04), которые иначе пришлось бы
  перепроверять.
- Попадание в memo N9 теперь считается `CONTAINS_BASE_TIER1_HITS`
  (`bench-internals`) — memo является частью Tier-1. Без этого
  `tests/segment_table_contains_base_tier1_counters.rs` падал: освобождения в одном
  сегменте проходили через memo и не попадали ни в hits, ни в misses.
  Production-сборка не меняется (счётчик под `bench-internals`).
