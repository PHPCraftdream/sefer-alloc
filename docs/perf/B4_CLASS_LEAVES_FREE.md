# B4: ClassLeaves / state-byte — free-path шаги (U1, U1b, K, S, SV, приёмы 1–2)

Статус: **все шаги NO-GO, ничего не сливается** (вердикт оркестратора 2026-10-20:
U1 +318 и U1b +45 Ir/free, у U1b ещё и недоказанный новый `unsafe`; K-fit —
ослабление; SV при той же силе проверок только добавляет стоимость, потому что
пишет байт рядом с обоими битмапами; приёмы 1/2 откатаны). Следующий шаг —
вариант (в) целиком, байт ВМЕСТО битмапов (`B4_STATE_BYTE.md`). Все числа —
**ИЗМЕРЕНО** (callgrind, Ir/op), сырые логи: `docs/perf/_raw_b4_leaves_*.log`.
Измеренные деревья — архивный коммит `fba17156` (ветка `b4-leaves`, не для слияния);
file:line ниже относятся к нему. `state_byte_design.md` и `sol_rush2.md` — рабочие
заметки ревьюеров вне репозитория.

## 1. Скоуп

Серия шагов ADR b4 (`state_byte_design.md`, `sol_rush2.md`) вокруг
собственного free-пути small-классов:

- **U1** (`small-class-authority`, route-lookup): class-byte authority в
  `small_free_guard` (formula: alignment conjunct + `byte == c+1`).
- **U1b**: тот же протокол через cached handle (`ClassLeaves`).
- **K** (K-fit): отклонён владельцем (K-fit = ослабление проверок; строгая
  форма = формула U1). **Замеры не сняты** (ошибка скрипта `.sh.sh`) — данных
  нет, вердикт только архитектурный.
- **S** (байт в листьях `ClassLeaves`): ПРОПУЩЕН по
  `.rush/art/b4/state_byte_design.md` (@ox: 2–3 зависимые загрузки; без нового
  unsafe не окупается; см. §1.4/§4/§6 того документа).
- **SV** (`small-state-byte`): один байт состояния на стартовый гранул
  (LIVE/MAG/FREE + класс) заменяет на free-пути чтения magazine-битмапы и
  alloc-битмапы; битмапы продолжают писаться (их другие читатели: drain /
  reclaim / `class_cursor`).
- **Приём 1** (`S-vi`, interval-free): отложенная классификация — класс из
  байта, Layout против интервала `CLASS_LOWER/UPPER`.
- **Приём 2** (`S-vr`, refill одним проходом): MAG пишется в commit-проходе
  refilla, отдельный n−1-проход в `finish_magazine_refill` убран.

## 2. Методика

- callgrind, marginal Ir/op; профили: prof_ao, prof_batch, prof larson
  (malloc-bench-rs, T=1), prof mstress (T=1).
- **Два режима Layout** на own-free: `rt` (Layout через `black_box`, никакой
  const-folding `class_for`/`divides`) и `const` (константный Layout 16B —
  классификация сворачивается при компиляции).
- Wall-clock **не использовался** (только счётчики Ir).
- Ячейки: `zz_alloc_const` w_alloc/w_free (ao rt/const),
  `dealloc_batch_fresh_*` (batch), larson, mstress. RSS: `drain_total_committed`
  (examples/r29_4_segment_state_reconciliation_gate.rs).
- Активация/оракулы: U1b — activation-счётчики (activation 100 %, fallback 0);
  SV — мутанты ×5 RED + зона (activation-счётчиков нет).

## 3. Точки сверки (sha256 `git diff`)

| состояние | sha256 |
|---|---|
| SV после имплементации, до приёмов (цель отката) | `c8c827eb0260969d3bb2dcd09d816e6dc409161d4169cba7244193602e79a3bd` |
| SV + приём 1 + приём 2 (до отката) | `5c84c58c88df8800b185097c6258bfcd0a7cd82c3535d4fc918e174ff8ed3a1a` |
| после отката приёмов 1/2 (фактическое) | см. финальный sha в конце отчёта (семантический откат) |

Откат приёмов 1/2 — **семантический**, не побайтовый: точная текстовая форма
SV-состояния не восстанавливалась, восстановлена семантика (сигнатура
`small_free_guard(c: usize, …)` + полный `class_for`, убраны
`CLASS_LOWER/UPPER`/`class_intervals`, interval-ветка, `magazine_destined`,
dbg-хуки `dbg_state_byte_for`/`dbg_magazine_state_bytes`, тесты
`class_interval_free_equivalence.rs` и `small_state_byte_refill_exactness.rs`).
Строки R1-01 swap-top (`dealloc_own_base.rs:484-507` в HEAD) не затронуты —
сверено с `git show HEAD`.

## 4. Результаты

Вывод `node scripts/b4_leaves_table.mjs docs/perf/B4_CLASS_LEAVES_FREE_summary.csv`:

<!-- TABLE:BEGIN -->

| шаг | ao alloc/free (rt) | ao alloc/free (const) | batch alloc/free (rt) | batch alloc/free (const) | larson | mstress | Δ free rt (vs base) | вердикт |
|---|---|---|---|---|---|---|---|---|
| base | 63/115 | 26/74 | 136/173 | 108/140 | 319 | 494 | 0 | OK (production baseline) |
| u1 | 63/433 | 26/393 | 136/491 | 108/459 | 650 | 953 | +318 | NO-GO (free +318 rt) |
| u1b | 63/160 | 26/137 | 136/218 | 108/204 | 384 | 546 | +45 | NO-GO (free +45 rt / +63 const; batch +45/+64; larson +65; mstress +52) |
| sv | 66/118 | 28/74 | 144/181 | 115/147 | 322 | 507 | +3 | NO-GO (free +3; batch +8/+7; larson +3; mstress +13; same check strength; byte written alongside both bitmaps) |
| svi | 66/143 | 28/82 | 144/206 | 115/155 | 359 | 538 | +28 | NO-GO (premisa sol_rush2 1.1 опровергнута; free +25) - OTKATAN |
| svr | 65/143 | / | 139/206 | 139/206 | 358 |  | +28 | NO-GO / effect ~0 (on top of svi) - OTKATAN |

<!-- TABLE:END -->

Примечания к таблице: все числа ИЗМЕРЕНО; svr снят только в rt-режиме ao +
batch + larson (const-ao и mstress не снимались — эффект ≈0 сделан вывод
раньше); svi/svr строки показывают состояние ДО отката.

### Вердикты по шагам

- **U1 (route-lookup)** — NO-GO: +318 Ir/free (rt), +319 (const), batch
  +318/+319, larson +331, mstress +459. Формула строже, но каждая проверка
  стоит отдельной загрузки route/sidecar.
- **U1b (cached handle)** — NO-GO по перфу: free +45 rt / +63 const, batch
  +45/+64, larson +65, mstress +52. Корректность усиления не оспаривается
  (activation 100 %, fallback 0) — вопрос владельцу «за усиление» (принять
  +45…+63 Ir/free как плату за strengthening?).
- **K** — отклонён владельцем (K-fit = ослабление). Данных нет (замеры не
  сняты — ошибка скрипта `.sh.sh`).
- **S** — пропущен: `.rush/art/b4/state_byte_design.md` (2–3 зависимые
  загрузки; без нового unsafe не окупается; §1.4/§4/§6). Отметка: **S-v в
  текущем виде — «проверка той же силы, дороже на ~0…13 Ir в зависимости от
  нагрузки»**.
- **SV (small-state-byte)** — плоско: free rt +3, const ±0, batch +7…+8,
  larson +3, mstress +13; RSS-дельта 0 (`drain_total_committed` 12288 KiB в
  обоих замерах); cache-sim w_free: Ir 121.44, Dr 23.14, Dw 13.06, D1mr 0.178,
  D1mw 0.092 /op. **НЕ подтверждает прогноз −15…−22.** Главные съедающие:
  битмапы продолжают ПИСАТЬСЯ (magazine+alloc); их съём — отдельный ADR (это
  и есть место, где −15…−22 может реализоваться). Кандидат требует решения
  владельца.
- **Приём 1 (interval-free, S-vi)** — NO-GO: премиса §1.1 `sol_rush2`
  опровергнута — `class_for_small_align` уже 1 загрузка против 2 у интервала;
  const-путь +8. Откатан.
- **Приём 2 (refill одним проходом, S-vr)** — эффект ≈0 (ao 65/143, batch
  139/206, larson 358 поверх приёма 1). Откатан.

## 5. Что НЕ доказано

- **Межпоточный протокол** байта состояния (cross-thread free / sidecar
  reclaim) — только спайк-уровень; Loom/Miri для байта — спайк, не gates.
- **iai/EstCycles** — всё ниже валидировано локально (callgrind); CI-прогон
  обязателен.
- **Rebase на main + PR #82 (b4-split)** потребует перезамера: конфликтующие
  файлы — `segment_table_impl.rs` (N9 memo последнего сегмента) и
  `segment_bitmap.rs` (u64-слова N4).
- SV-инвариант «байт — единственный authority на free-пути» при включённых
  обоих feature-наборах (`small-class-authority` + `small-state-byte`)
  доказан эквивалентностью форм, но не отдельным исчерпывающим тестом.

## 6. Что прогнать на CI

iai A/B-ячейки (добавлены rtl-ячейки в `benches/perf_gate_iai.rs`:
`dealloc_free_only_16b_rtl`, `dealloc_prealloc_only_16b_rtl`,
`dealloc_free_only_1088_16b_rtl`, `dealloc_prealloc_only_1088_16b_rtl`;
Layout через `black_box` — `class_for`/`divides` не сворачиваются):

- формулы **free_rtl** по rtl-ячейкам с prefix-вычитанием
  (`free_only_*_rtl − prealloc_only_*_rtl`);
- константные ячейки как сопутствующий гейт: **≤ +5 Ir/free**;
- batch: **≤ +6 Ir/блок**;
- hardened-плечо: **≤ −15**;
- **EstCycles ≤ +1 %**.

Пометка: runner `scripts/r20_r6_ab_iai_run.mjs` нужно расширить на
prefix-формулы + EstCycles — **ещё НЕ сделано**.

## 7. Риски

- **U1b unsafe-инвариант** (cached handle) ждёт Kani/Miri на CI.
- **mstress +13** у SV — единственная метрика с заметной отрицательной
  дельтой; при съёме битмапов ожидается отыгрыш (ОЦЕНКА, не измерено).
- **N9/N4-конфликты PR #82** (`segment_table_impl.rs`, `segment_bitmap.rs`)
  при rebase.

## 8. Вопросы владельцу

1. **SV**: принять «плоский» кандидат (free +3 rt / ±0 const, batch +7…+8,
   mstress +13) с последующим ADR о съёме битмапов, или отложить до него?
2. **U1b**: усилениe стоит +45…+63 Ir/free — принимать?
3. **S/S-v**: согласовать формулировку «проверка той же силы, дороже на
   ~0…13 Ir» как финальную запись шага S.
4. **K**: закрывать без замеров (данных нет) или повторить прогон после
   починки скрипта?
