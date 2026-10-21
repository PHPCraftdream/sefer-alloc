# B4-M — пакет микроприёмов: S1 + X-null + X-span приняты (perf(runtime)); R3, X-c1, N9 v3 — NO-GO

Ветка `b4-micro`. Хэш финального diff кода правок (включая bench-internals
счётчики; сами счётчики под `cfg` и на замеряемые production-сборки не
влияют):
`daf979894bfdd0f0d4cc13cb5a557d5f65c9b933603200d3a97a679c6f1b003c`
(`git diff | sha256sum` на старте документационного шага; после правок только
документации/скрипта — `8f26c8bf724c1332676a69b0316f870c66b665501386ec2f53d907e271c3cb1c`).
База пакета A: `bbc3c7b4d376971b9ea7627ad3b4ff43ac39084d`
(diff от неё пустой).

**Состав PR.** Измеренное дерево целиком закоммичено как коммит-идентичность
`14ce71eb` (`wip(b4-micro): measured micro-pack tree …`, родитель `bbc3c7b4`;
закреплён веткой `b4-micro-measured`): `git diff bbc3c7b4 14ce71eb
--diff-filter=M | sha256sum` воспроизводит `8f26c8bf…` выше. В PR тот же коммит
перенесён rebase на текущий `main` (новый SHA, содержимое пакета то же). Следующий коммит
убирает N9 v3 (локальный NO-GO по PREREG, §5): код в `segment_table_impl.rs`,
счётчики `N9_*`, `dbg_segment_memo_holds` и `tests/b4_micro_n9_memo_fill_policy.rs`;
сырые логи N9 остаются как доказательство вердикта. В production уходят только
S1, X-null и X-span; окончательная приёмка — точный iai A/B и mt-bench на CI.

**Наблюдатели после замера.** Счётчики `XSPAN_MASK_BRANCH_{CALLS,TAKEN}` (на каждом
`canonical_block_of`) и `XNULL_NULL_DEALLOC_TOTAL` (вход `SeferAlloc::dealloc`)
нужны были только оракулу §10; в PR они убраны, т. к. стандартный iai-судья
собирается с `bench-internals`, и они навсегда добавляли бы B-only инструкции на
горячий free (CI iai A/B на `bed33681`: +1 Ir на чужой free в
`r19_prf01_joined_producer_same_word`, 8 192 free → +8 192 Ir — артефакт
входного счётчика). `S1_MAG_BYTE_WRITES` теперь прибавляется один раз на
drain/carve, а не на блок (та же сумма). Оракул §10 воспроизводится с `14ce71eb`.

## 0. Методика

- Все Ir-числа — **callgrind**, маржинальная стоимость на op/блок; wall-clock
  в этом пакете не использовался и не приводится.
- Перед каждым замером фиксировался хэш рабочего diff:
  `git diff | sha256sum`; первая строка каждого сырого лога — этот хэш.
- Каждый шаг пакета сравнивается со **своей** базой (стек шагов):
  S1-база = mc-base; R3/X-null-база = mc-s1; X-span-база = mc-xn;
  X-c1-база = mc-xs; N9-база = mc-xs.
- Судьи: mstress, larson (Ir/op), ao, aoc (rt/const16/const64, alloc/free
  Ir/op), batch/batchc/batchl (alloc/free Ir на блок, 256/258 на round),
  thrash64 (alloc/free Ir/op + EstCycles/op на free).
- Все числа ниже **ИЗМЕРЕНО**, если не помечено «ОЦЕНКА». Числа, для которых
  замер не проводился, помечены «—».
- Пометка о дефекте сырых логов: см. §7 (дефект протокола S1-хэшей) и §8
  (пост-замерные фиксы); сырые логи X-null были заглушками неудавшихся
  первых прогонов (EXIT=127) — mstress/larson/thrash64 перегенерированы из
  callgrind out-files в `~/ab`, см. Приложение А.1; ao/aoc/batch-судьи X-null
  и N9 подкреплены новыми сырыми логами `mc_xn2_*` / `mc_n92_*` / `mc_n92b_batch`
  (Приложение А.2).

### 0.1 Пересчёт общих вердиктов по PREREG (без захардкоженных ожиданий)

Вердикты шагов вычисляются скриптом из единственной константы `PREREG` в
`scripts/b4_micro_pack_table.mjs` (других источников порогов в скрипте нет).
Правило: GO ⇔ целевое условие шага выполнено (альтернативы цели объединяются
OR — достаточно одной; проигравшая альтернатива сама по себе не FAIL) И ни
одна нерегресс-строка по всем судьям шага, присутствующим в сырье, не FAIL.
Нерегресс-допуски по судьям: ao/aoc — `variant ≤ base + 0 Ir`; batch* —
`≤ +1 Ir/блок`; larson/mstress — `≤ base×1.002` (+0.2 % от базы); thrash64
(alloc/free и cyc) — `≤ +1`. Целевые судьи шага в нерегресс-проверку не
входят (их порог — целевой). `int (±1)` — НЕ допуск: порог применяется к
значению как есть; приёмка int-целевых строк — по точному iai на CI.

Фактический результат пересчёта:

- **X-null: GO.** Цель: ao free 92 → 90 (улучшение 2 ≥ 1 Ir). Нерегрессы:
  mstress 372.66 ≤ 372×1.002 = 372.744 ok; larson 281.54 ≤ 281×1.002 =
  281.562 ok; прочие судьи — чисты.
- **N9 v3: NO-GO.** Цель выполнена: thrash64 free 84 → 81.43 (улучшение 2.57 ≥ 2),
  но нерегрессы FAIL: mstress 368.86 > 368×1.002 = 368.736; larson 279.58 >
  279×1.002 = 279.558; aoc rt alloc 49.51 > 49 + 0. thrash64 cyc 90.3 ≤
  93.3 + 1 = 94.3 ok.
- Остальное — вычислением: S1 GO (mstress 380 → 372, улучшение 8 ≥ 1.14 (= 0.3 % × 380) И larson 282 → 281, улучшение 1 ≥ 0; нерегрессы чисты); R3 NO-GO (mstress 374 против базы 372 —
  улучшение −2 < 1.14, цель не выполнена); X-span GO (ao free 90 → 87);
  X-c1/X-c1b NO-GO (mstress 369 > 368.736 FAIL; целевые ao free/aoc rt
  alloc улучшений не дали); N9 v1/v2 rejected (цель thrash64 free: улучшение 0.57 /
  0.36 < 2 не выполнена).

Целевые улучшения (ao free: улучшение 2, thrash64 free: улучшение 2.57) сами несут int-
неопределённость (база — int-ячейка): приёмка по точному iai A/B на CI (§12).

## 1. Базовые судьи (mc-base, база пакета A)

<!-- BEGIN GENERATED: b4_micro_pack_table.mjs §0 -->
### §0 Базовые судьи (mc-base, база пакета A) — рендер скрипта

| судья | метрика | base | точность | пометка |
|---|---|---|---|---|
| mstress | Ir/op | 380 int (±1) | int (±1) | ИЗМЕРЕНО |
| larson | Ir/op | 282 int (±1) | int (±1) | ИЗМЕРЕНО |
| ao | alloc Ir/op | 50 int (±1) | int (±1) | ИЗМЕРЕНО |
| ao | free Ir/op | 92 int (±1) | int (±1) | ИЗМЕРЕНО |
| aoc rt | alloc Ir/op | 50 int (±1) | int (±1) | ИЗМЕРЕНО |
| aoc rt | free Ir/op | 92 int (±1) | int (±1) | ИЗМЕРЕНО |
| aoc const16 | alloc Ir/op | 22 int (±1) | int (±1) | ИЗМЕРЕНО |
| aoc const16 | free Ir/op | 54 int (±1) | int (±1) | ИЗМЕРЕНО |
| aoc const64 | alloc Ir/op | 22 int (±1) | int (±1) | ИЗМЕРЕНО |
| aoc const64 | free Ir/op | 54 int (±1) | int (±1) | ИЗМЕРЕНО |
| batch | alloc Ir/block | 114 int (±1) | int (±1) | ИЗМЕРЕНО |
| batch | free Ir/block | 140 int (±1) | int (±1) | ИЗМЕРЕНО |
| batchc | alloc Ir/block | 94 int (±1) | int (±1) | ИЗМЕРЕНО |
| batchc | free Ir/block | 110 int (±1) | int (±1) | ИЗМЕРЕНО |
| batchl | alloc Ir/block | 120 int (±1) | int (±1) | ИЗМЕРЕНО |
| batchl | free Ir/block | 155 int (±1) | int (±1) | ИЗМЕРЕНО |
| thrash64 | alloc Ir/op | 22 int (±1) | int (±1) | ИЗМЕРЕНО |
| thrash64 | free Ir/op | 91 int (±1) | int (±1) | ИЗМЕРЕНО |
| thrash64 cyc | EstCycles/op free | 100.2 | exact | ИЗМЕРЕНО |
<!-- END GENERATED: b4_micro_pack_table.mjs §0 -->

Сводная таблица шаг × судья — рендер скрипта `scripts/b4_micro_pack_table.mjs`
(полный вывод с audit-trail: `docs/perf/B4_MICRO_PACK_tables.generated.md`).
Дробные значения — пересчёт по точной паре TOTALS из сырого лога
(`[t1=… t2=…]` либо секция «regenerated alloc-rt», маргинал = (t2−t1)/50000,
2 знака); суффикс `int (±1)` — в логе только целое из summary-строки (старые
prof-логи без точных пар). Строки X-null и N9 v3 — merge mc-xn+mc-xn2 и
mc-n9+mc-n92(+n92b) по правилу из заголовка скрипта:

<!-- BEGIN GENERATED: b4_micro_pack_table.mjs §1 -->
### §1 Сводная: шаг × судья (рендер `scripts/b4_micro_pack_table.mjs`)

Дробные значения — пересчёт по точной паре TOTALS из сырого лога
(`[t1=… t2=…]` либо секция «regenerated alloc-rt», маргинал =
(t2−t1)/50000, 2 знака). Суффикс `int (±1)` — в логе только целое из
summary-строки (старые prof-логи без точных пар).

| шаг | mstress | larson | ao a/f | aoc rt a/f | aoc c16 a/f | aoc c64 a/f | batch a/f | batchc a/f | batchl a/f | thrash64 a/f | thrash64 cyc(free) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| base (mc-base) | 380 int (±1) | 282 int (±1) | 50 int (±1)/92 int (±1) | 50 int (±1)/92 int (±1) | 22 int (±1)/54 int (±1) | 22 int (±1)/54 int (±1) | 114 int (±1)/140 int (±1) | 94 int (±1)/110 int (±1) | 120 int (±1)/155 int (±1) | 22 int (±1)/91 int (±1) | 100.2 |
| S1 (mc-s1) | 372 int (±1) | 281 int (±1) | 49 int (±1)/92 int (±1) | —/92 int (±1) | 22 int (±1)/54 int (±1) | 22 int (±1)/54 int (±1) | 108 int (±1)/140 int (±1) | 88 int (±1)/110 int (±1) | 114 int (±1)/155 int (±1) | 22 int (±1)/91 int (±1) | 100.2 |
| R3 (mc-r3) | 374 int (±1) | 281 int (±1) | 49 int (±1)/92 int (±1) | 49 int (±1)/92 int (±1) | 22 int (±1)/54 int (±1) | 22 int (±1)/54 int (±1) | 108 int (±1)/141 int (±1) | 88 int (±1)/110 int (±1) | 114 int (±1)/155 int (±1) | 22 int (±1)/91 int (±1) | 100.2 |
| R3b (mc-r3b) | 375 int (±1) | — | — | — | — | — | — | — | — | — | — |
| X-null | 372.66 | 281.54 | 49 int (±1)/90 int (±1) | 49.51/90 int (±1) | 22 int (±1)/52 int (±1) | 22 int (±1)/52 int (±1) | 108 int (±1)/138 int (±1) | 88 int (±1)/108 int (±1) | 114 int (±1)/153 int (±1) | 22/89.33 | 98.2 |
| X-span (mc-xs) | 368 int (±1) | 279 int (±1) | 49 int (±1)/87 int (±1) | 49 int (±1)/87 int (±1) | 22 int (±1)/49 int (±1) | 22 int (±1)/49 int (±1) | 108 int (±1)/135 int (±1) | 88 int (±1)/105 int (±1) | 114 int (±1)/150 int (±1) | 22 int (±1)/84 int (±1) | 93.3 |
| X-c1 (mc-xc1) | 369 int (±1) | 275 int (±1) | 49 int (±1)/87 int (±1) | 49 int (±1)/87 int (±1) | 22 int (±1)/49 int (±1) | 22 int (±1)/49 int (±1) | 108 int (±1)/135 int (±1) | 88 int (±1)/105 int (±1) | 114 int (±1)/149 int (±1) | 22 int (±1)/84 int (±1) | 93.3 |
| X-c1b (mc-xc1b) | 369 int (±1) | 275 int (±1) | 49 int (±1)/87 int (±1) | 49 int (±1)/87 int (±1) | 21 int (±1)/49 int (±1) | 21 int (±1)/49 int (±1) | 108 int (±1)/135 int (±1) | 88 int (±1)/105 int (±1) | 114 int (±1)/149 int (±1) | 22 int (±1)/84 int (±1) | 93.3 |
| N9 v3 (mc-n9) | 368.86 | 279.58 | 49 int (±1)/87 int (±1) | 49.51/87 int (±1) | 22 int (±1)/49 int (±1) | 22 int (±1)/49 int (±1) | 108 int (±1)/135 int (±1) | 88 int (±1)/105 int (±1) | 114 int (±1)/140 int (±1) | 22/81.43 | 90.3 |
| N9 v1 (mc-n9v1) отклонён | 368.86 | 279.58 | — | — | — | — | — | — | — | 22/84.57 | 93.5 |
| N9 v2 (mc-n9v2) отклонён | 368.86 | 279.58 | — | — | — | — | — | — | — | 22/84.36 | 93.3 |
<!-- END GENERATED: b4_micro_pack_table.mjs §1 -->

Примечание: X-span thrash64 EstCycles/op = 93.3 (в ранней редакции отчёта
стояло 94.4 — это значение ao-кэша, а не thrash64; исправлено по сырым
логам `mc_xs_thrash64`).

Все числа в таблице — ИЗМЕРЕНО: значения без суффикса `int (±1)` пересчитаны
по точным парам TOTALS из сырых логов (дробные); суффикс `int (±1)` — целое
из summary-строки старого prof-лога (точной пары в логе нет). «—» = замер
для этой ячейки не проводился.

## 2. S1 — MAG-маркировка на refill — **GO**

**Механика.** `try_drain_freelist_batch` / `try_carve_batch` получили
параметр `issue_state` (LIVE для batch-API, MAG для refill);
`finish_magazine_refill` (`src/registry/heap_core/alloc/hot.rs`): вместо
цикла MAG-маркировки n−1 блоков с `segment_base_of_ptr` на каждом — одна
LIVE-запись на выдаваемый блок. Побочный эффект: batch-alloc −6 Ir/вызов.

| судья | метрика | A (mc-base) | B (mc-s1) | Δ | порог | вердикт |
|---|---|---|---|---|---|---|

<!-- BEGIN GENERATED: b4_micro_pack_table.mjs §2 (Δ = base−variant, положительный = улучшение) -->
### §2 A/B: S1 (A = base, B = S1) — порог: S1: mstress ≤ −0.3 % И larson ≤ 0; остальные — нерегресс

| судья | метрика | A | B | Δ (base−variant) | порог | точность | вердикт |
|---|---|---|---|---|---|---|---|
| mstress | Ir/op | 380 int (±1) | 372 int (±1) | 8 | mstress op ≤ −0.3 % базы (порог проверяется точным iai на CI) | int (±1) | GO (target; int (±1): локально; приёмка по точному iai на CI) |
| larson | Ir/op | 282 int (±1) | 281 int (±1) | 1 | larson op ≤ −0 Ir (порог проверяется точным iai на CI) | int (±1) | GO (target; int (±1): локально; приёмка по точному iai на CI) |
| ao | alloc Ir/op | 50 int (±1) | 49 int (±1) | 1 | ≤ +0 Ir | int (±1) | ok |
| ao | free Ir/op | 92 int (±1) | 92 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc rt | free Ir/op | 92 int (±1) | 92 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const16 | alloc Ir/op | 22 int (±1) | 22 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const16 | free Ir/op | 54 int (±1) | 54 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const64 | alloc Ir/op | 22 int (±1) | 22 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const64 | free Ir/op | 54 int (±1) | 54 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| batch | alloc Ir/block | 114 int (±1) | 108 int (±1) | 6 | ≤ +1 Ir/блок | int (±1) | ok |
| batch | free Ir/block | 140 int (±1) | 140 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchc | alloc Ir/block | 94 int (±1) | 88 int (±1) | 6 | ≤ +1 Ir/блок | int (±1) | ok |
| batchc | free Ir/block | 110 int (±1) | 110 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchl | alloc Ir/block | 120 int (±1) | 114 int (±1) | 6 | ≤ +1 Ir/блок | int (±1) | ok |
| batchl | free Ir/block | 155 int (±1) | 155 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| thrash64 | alloc Ir/op | 22 int (±1) | 22 int (±1) | 0 | ≤ +1 Ir | int (±1) | ok |
| thrash64 | free Ir/op | 91 int (±1) | 91 int (±1) | 0 | ≤ +1 Ir | int (±1) | ok |
| thrash64 cyc | EstCycles/op free | 100.2 | 100.2 | 0 | ≤ +1 Ir | exact | ok |

**Вердикт S1: GO.**
<!-- END GENERATED: b4_micro_pack_table.mjs §2 -->

Остальные судьи — в пределах базы (см. §1 таблицу). Безопасность: семантика
состояний не меняется — refill-блоки не наблюдаются потребителем до выдачи;
параметр `issue_state` делает выбор состояния явным на уровне типов.
Оракул активации (R30-8, счётчики, лог `_raw_b4_micro_counters.log`):
`S1_MAG_BYTE_WRITES` = 48 = retained (45, суммы n_i−1) + events (3 refill) —
доля refill-блоков с MAG-записью в сабстрейте = 100% по построению счётчика.
Мутант M-h (MAG→LIVE в refill) — красный (debug_assert срабатывает).
Тест `tests/b4_micro_s1_refill_mag_bytes.rs` — зелёный.

## 3. X-null — снятие is_null с горячего free — **GO (пересчёт PREREG, см. §0.1)**

**Механика.** Проверка `is_null` убрана из горячего free
(`src/global/sefer_alloc/global_alloc.rs`, `src/registry/heap_core/free/dealloc.rs`,
`mem_impl.rs::canonical_block_of`); компенсирующий хвостовой guard добавлен в
`publish_foreign` (`src/registry/heap_core_xthread/routing.rs`).

| судья | метрика | A (mc-s1) | B (mc-xn) | Δ | порог | вердикт |
|---|---|---|---|---|---|---|

<!-- BEGIN GENERATED: b4_micro_pack_table.mjs §3 (Δ = base−variant) -->
### §3 A/B: X-null (A = S1, B = X-null) — порог: ao free ≤ −1 Ir ИЛИ aoc rt alloc ≤ −1 Ir

| судья | метрика | A | B | Δ (base−variant) | порог | точность | вердикт |
|---|---|---|---|---|---|---|---|
| mstress | Ir/op | 372 int (±1) | 372.66 | -0.66 | ≤ +0.2 % базы | int (±1) | ok |
| larson | Ir/op | 281 int (±1) | 281.54 | -0.54 | ≤ +0.2 % базы | int (±1) | ok |
| ao | alloc Ir/op | 49 int (±1) | 49 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| ao | free Ir/op | 92 int (±1) | 90 int (±1) | 2 | ao free ≤ −1 Ir (порог проверяется точным iai на CI) | int (±1) | GO (target; int (±1): локально; приёмка по точному iai на CI) |
| aoc rt | alloc Ir/op | — | 49.51 | — | aoc rt alloc ≤ −1 Ir | — | нет данных (target) |
| aoc rt | free Ir/op | 92 int (±1) | 90 int (±1) | 2 | ≤ +0 Ir | int (±1) | ok |
| aoc const16 | alloc Ir/op | 22 int (±1) | 22 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const16 | free Ir/op | 54 int (±1) | 52 int (±1) | 2 | ≤ +0 Ir | int (±1) | ok |
| aoc const64 | alloc Ir/op | 22 int (±1) | 22 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const64 | free Ir/op | 54 int (±1) | 52 int (±1) | 2 | ≤ +0 Ir | int (±1) | ok |
| batch | alloc Ir/block | 108 int (±1) | 108 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batch | free Ir/block | 140 int (±1) | 138 int (±1) | 2 | ≤ +1 Ir/блок | int (±1) | ok |
| batchc | alloc Ir/block | 88 int (±1) | 88 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchc | free Ir/block | 110 int (±1) | 108 int (±1) | 2 | ≤ +1 Ir/блок | int (±1) | ok |
| batchl | alloc Ir/block | 114 int (±1) | 114 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchl | free Ir/block | 155 int (±1) | 153 int (±1) | 2 | ≤ +1 Ir/блок | int (±1) | ok |
| thrash64 | alloc Ir/op | 22 int (±1) | 22 | 0 | ≤ +1 Ir | int (±1) | ok |
| thrash64 | free Ir/op | 91 int (±1) | 89.33 | 1.67 | ≤ +1 Ir | int (±1) | ok |
| thrash64 cyc | EstCycles/op free | 100.2 | 98.2 | 2 | ≤ +1 Ir | exact | ok |

**Вердикт X-null: GO.**
<!-- END GENERATED: b4_micro_pack_table.mjs §3 -->

Безопасность: null-free остаётся no-op контрактом `GlobalAlloc::dealloc`;
единственный путь, куда может попасть чужой/нулевой указатель без раннего
`is_null`, завершается guard'ом в `publish_foreign`, т.е. отбраковка
сохранилась, просто перенесена из всеобщего горячего префикса в холодную
ветвь. Тест `tests/b4_micro_xnull_null_free_noop.rs` — зелёный; мутант
(удаление guard'а из `publish_foreign`) — красный (лог
`_raw_b4_micro_mutant_xnull.log`, RC=101). Примечание: логи
`_raw_b4_micro_mc_xn_{mstress,larson,thrash64}.log` перегенерированы из
callgrind out-files (точные пары TOTALS в логах); ao/aoc/batch-судьи X-null
подкреплены логами `mc_xn2_*`.

## 4. X-span — маска сегмента в `canonical_block_of` — **GO**

**Механика.** `canonical_block_of` (`src/alloc_core/alloc_core/mem/mem_impl.rs`):
для не-Large offset вычисляется как `addr & (SEGMENT − 1)`; ветка
`off >= span` осталась только в Large-пути (T22-семантика сохранена).

| судья | метрика | A (mc-xn) | B (mc-xs) | Δ | порог | вердикт |
|---|---|---|---|---|---|---|

<!-- BEGIN GENERATED: b4_micro_pack_table.mjs §4 (Δ = base−variant) -->
### §4 A/B: X-span (A = X-null, B = X-span) — порог: ao free ≤ −1 Ir ИЛИ aoc rt alloc ≤ −1 Ir

| судья | метрика | A | B | Δ (base−variant) | порог | точность | вердикт |
|---|---|---|---|---|---|---|---|
| mstress | Ir/op | 372.66 | 368 int (±1) | 4.66 | ≤ +0.2 % базы | int (±1) | ok |
| larson | Ir/op | 281.54 | 279 int (±1) | 2.54 | ≤ +0.2 % базы | int (±1) | ok |
| ao | alloc Ir/op | 49 int (±1) | 49 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| ao | free Ir/op | 90 int (±1) | 87 int (±1) | 3 | ao free ≤ −1 Ir (порог проверяется точным iai на CI) | int (±1) | GO (target; int (±1): локально; приёмка по точному iai на CI) |
| aoc rt | alloc Ir/op | 49.51 | 49 int (±1) | 0.51 | aoc rt alloc ≤ −1 Ir (порог проверяется точным iai на CI) | int (±1) | не выполнена (target) |
| aoc rt | free Ir/op | 90 int (±1) | 87 int (±1) | 3 | ≤ +0 Ir | int (±1) | ok |
| aoc const16 | alloc Ir/op | 22 int (±1) | 22 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const16 | free Ir/op | 52 int (±1) | 49 int (±1) | 3 | ≤ +0 Ir | int (±1) | ok |
| aoc const64 | alloc Ir/op | 22 int (±1) | 22 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const64 | free Ir/op | 52 int (±1) | 49 int (±1) | 3 | ≤ +0 Ir | int (±1) | ok |
| batch | alloc Ir/block | 108 int (±1) | 108 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batch | free Ir/block | 138 int (±1) | 135 int (±1) | 3 | ≤ +1 Ir/блок | int (±1) | ok |
| batchc | alloc Ir/block | 88 int (±1) | 88 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchc | free Ir/block | 108 int (±1) | 105 int (±1) | 3 | ≤ +1 Ir/блок | int (±1) | ok |
| batchl | alloc Ir/block | 114 int (±1) | 114 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchl | free Ir/block | 153 int (±1) | 150 int (±1) | 3 | ≤ +1 Ir/блок | int (±1) | ok |
| thrash64 | alloc Ir/op | 22 | 22 int (±1) | 0 | ≤ +1 Ir | int (±1) | ok |
| thrash64 | free Ir/op | 89.33 | 84 int (±1) | 5.33 | ≤ +1 Ir | int (±1) | ok |
| thrash64 cyc | EstCycles/op free | 98.2 | 93.3 | 4.9 | ≤ +1 Ir | exact | ok |

**Вердикт X-span: GO.**
<!-- END GENERATED: b4_micro_pack_table.mjs §4 -->

Безопасность: для Small/Medium сегменты SEGMENT-выровнены, маска эквивалентна
бывшему вычитанию базы; Large-путь не тронут (off >= span остался). Тест
`tests/b4_micro_xspan_small_primordial_mask.rs` — зелёный; 2 мутанта
(маска без учёта span / Large-путь с маской) — красные.
Оракул активации (R30-8, счётчики): маска-ветка сработала 512 из 516 вызовов
(99.22%); остальные 4 — Large-вызовы вне маски-ветки (по построению).
Примечание: в `mem_impl.rs` по этому поводу оставлен NOTE-комментарий;
`debug_assert_eq!(base.addr(), segment_base_of_ptr(ptr))` в X-span не входит —
такой assert добавлялся в ходе итерации и был снят, когда оказался ложным
для Large-вызовов (после чего Large-путь выведен из-под маски явно).

## 5. N9 — memo последнего сегмента, политика заполнения — **NO-GO (v3, пересчёт PREREG, см. §0.1)**

**Механика.** `segment_table_impl.rs::canonical_base_of_mut`: one-entry memo
последнего сегмента заполняется ТОЛЬКО при own_cache-хите; заполнение на
won-probe убрано (на full-miss нагрузках memo постоянно перезаписывался
впустую — 2 dead store + compare на каждый промах).

| судья | метрика | A (mc-xs) | B (mc-n9 v3) | Δ | порог | вердикт |
|---|---|---|---|---|---|---|

<!-- BEGIN GENERATED: b4_micro_pack_table.mjs §5 (Δ = base−variant) -->
### §5 A/B: N9 v3 (A = X-span, B = N9 v3) — порог: thrash64 free ≤ −2 Ir

| судья | метрика | A | B | Δ (base−variant) | порог | точность | вердикт |
|---|---|---|---|---|---|---|---|
| mstress | Ir/op | 368 int (±1) | 368.86 | -0.86 | ≤ +0.2 % базы | int (±1) | FAIL |
| larson | Ir/op | 279 int (±1) | 279.58 | -0.58 | ≤ +0.2 % базы | int (±1) | FAIL |
| ao | alloc Ir/op | 49 int (±1) | 49 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| ao | free Ir/op | 87 int (±1) | 87 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc rt | alloc Ir/op | 49 int (±1) | 49.51 | -0.51 | ≤ +0 Ir | int (±1) | FAIL |
| aoc rt | free Ir/op | 87 int (±1) | 87 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const16 | alloc Ir/op | 22 int (±1) | 22 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const16 | free Ir/op | 49 int (±1) | 49 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const64 | alloc Ir/op | 22 int (±1) | 22 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const64 | free Ir/op | 49 int (±1) | 49 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| batch | alloc Ir/block | 108 int (±1) | 108 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batch | free Ir/block | 135 int (±1) | 135 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchc | alloc Ir/block | 88 int (±1) | 88 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchc | free Ir/block | 105 int (±1) | 105 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchl | alloc Ir/block | 114 int (±1) | 114 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchl | free Ir/block | 150 int (±1) | 140 int (±1) | 10 | ≤ +1 Ir/блок | int (±1) | ok |
| thrash64 | alloc Ir/op | 22 int (±1) | 22 | 0 | ≤ +1 Ir | int (±1) | ok |
| thrash64 | free Ir/op | 84 int (±1) | 81.43 | 2.57 | thrash64 free ≤ −2 Ir (порог проверяется точным iai на CI) | int (±1) | GO (target; int (±1): локально; приёмка по точному iai на CI) |
| thrash64 cyc | EstCycles/op free | 93.3 | 90.3 | 3 | ≤ +1 Ir | exact | ok |

**Вердикт N9 v3: NO-GO.**
<!-- END GENERATED: b4_micro_pack_table.mjs §5 -->

Итерации: v1 (fill на обоих сайтах) и v2 (memo после own_cache) — без
улучшений (thrash64 free остался 84.57/84.36 — точные дробные значения по
парам TOTALS), отклонены; v3 достигает цели (thrash64 free −2.57 ≤ −2 Ir),
но по пересчёту PREREG шаг NO-GO (mstress/larson/aoc rt alloc > допусков, см. §0.1).
Безопасность: memo — чистый кэш; корректность обеспечивается проверкой
`last_key` при чтении, политика заполнения влияет только на hit-rate.
Оракул активации: счётчики `CONTAINS_BASE_TIER1_*` в bench-internals
(`tests/segment_table_contains_base_tier1_counters.rs` и новый
`tests/b4_micro_n9_memo_fill_policy.rs`): fill при own_cache-хите — 1 из 2
потенциальных fill-событий (won-probe exact 1 + own_cache-hit fill 1,
знаменатель = 2) = 50.00% — это доля отличий политики заполнения, а не
активация ветки (активация доказывается thrash64 free −2.57 Ir и
batchl free −10); раскладка попаданий memo:own_cache = 10:1 (memo-share
90.91%). Примечание: на fresh primordial AllocCore все 4 probing-класса
разрешаются в один сегмент, поэтому ротации own_cache нет — см. NOTE в
`_raw_b4_micro_counters.log`.
Тест `tests/b4_micro_n9_memo_fill_policy.rs` — зелёный; мутант (откат к
won-probe fill) — красный.

## 6. Отклонённые приёмы

### 6.1 R3 — соседство `q == prev + block_size` в run-детекции flush — **NO-GO**

| судья | метрика | A (mc-s1) | B (mc-r3) | Δ | вердикт |
|---|---|---|---|---|---|

<!-- BEGIN GENERATED: b4_micro_pack_table.mjs §6.1 (Δ = base−variant) -->
### §6.1 A/B: R3 (A = S1, B = R3) — порог: mstress ≤ −0.3 %

| судья | метрика | A | B | Δ (base−variant) | порог | точность | вердикт |
|---|---|---|---|---|---|---|---|
| mstress | Ir/op | 372 int (±1) | 374 int (±1) | -2 | mstress op ≤ −0.3 % базы (порог проверяется точным iai на CI) | int (±1) | не выполнена (target) |
| larson | Ir/op | 281 int (±1) | 281 int (±1) | 0 | ≤ +0.2 % базы | int (±1) | ok |
| ao | alloc Ir/op | 49 int (±1) | 49 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| ao | free Ir/op | 92 int (±1) | 92 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc rt | free Ir/op | 92 int (±1) | 92 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const16 | alloc Ir/op | 22 int (±1) | 22 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const16 | free Ir/op | 54 int (±1) | 54 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const64 | alloc Ir/op | 22 int (±1) | 22 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const64 | free Ir/op | 54 int (±1) | 54 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| batch | alloc Ir/block | 108 int (±1) | 108 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batch | free Ir/block | 140 int (±1) | 141 int (±1) | -1 | ≤ +1 Ir/блок | int (±1) | ok |
| batchc | alloc Ir/block | 88 int (±1) | 88 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchc | free Ir/block | 110 int (±1) | 110 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchl | alloc Ir/block | 114 int (±1) | 114 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchl | free Ir/block | 155 int (±1) | 155 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| thrash64 | alloc Ir/op | 22 int (±1) | 22 int (±1) | 0 | ≤ +1 Ir | int (±1) | ok |
| thrash64 | free Ir/op | 91 int (±1) | 91 int (±1) | 0 | ≤ +1 Ir | int (±1) | ok |
| thrash64 cyc | EstCycles/op free | 100.2 | 100.2 | 0 | ≤ +1 Ir | exact | ok |

**Вердикт R3: NO-GO.**
<!-- END GENERATED: b4_micro_pack_table.mjs §6.1 -->

Причина (**ОЦЕНКА**, механизм не доказан атрибуцией; подтверждено только
направление эффекта): flush-руны в mstress предположительно короткие, загрузка
`block_size` + лишний compare дороже экономии от соседства. Итерация с hoist `step` — хуже ещё на
1 (mstress 375, ИЗМЕРЕНО, mc-r3b). Оценка из `B4_EVENTS_COST`
(−1.8…−2.9 Ir/op mstress) не подтвердилась. Построчная атрибуция mc_s1 vs
mc_r3: статья `magazine.rs` 36.9 → 40.1 Ir/op. Полностью откачен; в diff
финального пакета R3 не входит.

### 6.2 X-c1 — таблица `SIZE2C1` (c+1) — **NO-GO**

| судья | метрика | A (mc-xs) | B (mc-xc1) | Δ | вердикт |
|---|---|---|---|---|---|

<!-- BEGIN GENERATED: b4_micro_pack_table.mjs §6.2 (Δ = base−variant) -->
### §6.2 A/B: X-c1 (A = X-span, B = X-c1) — порог: ao free ≤ −1 Ir ИЛИ aoc rt alloc ≤ −1 Ir

| судья | метрика | A | B | Δ (base−variant) | порог | точность | вердикт |
|---|---|---|---|---|---|---|---|
| mstress | Ir/op | 368 int (±1) | 369 int (±1) | -1 | ≤ +0.2 % базы | int (±1) | FAIL |
| larson | Ir/op | 279 int (±1) | 275 int (±1) | 4 | ≤ +0.2 % базы | int (±1) | ok |
| ao | alloc Ir/op | 49 int (±1) | 49 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| ao | free Ir/op | 87 int (±1) | 87 int (±1) | 0 | ao free ≤ −1 Ir (порог проверяется точным iai на CI) | int (±1) | не выполнена (target) |
| aoc rt | alloc Ir/op | 49 int (±1) | 49 int (±1) | 0 | aoc rt alloc ≤ −1 Ir (порог проверяется точным iai на CI) | int (±1) | не выполнена (target) |
| aoc rt | free Ir/op | 87 int (±1) | 87 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const16 | alloc Ir/op | 22 int (±1) | 22 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const16 | free Ir/op | 49 int (±1) | 49 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const64 | alloc Ir/op | 22 int (±1) | 22 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const64 | free Ir/op | 49 int (±1) | 49 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| batch | alloc Ir/block | 108 int (±1) | 108 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batch | free Ir/block | 135 int (±1) | 135 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchc | alloc Ir/block | 88 int (±1) | 88 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchc | free Ir/block | 105 int (±1) | 105 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchl | alloc Ir/block | 114 int (±1) | 114 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchl | free Ir/block | 150 int (±1) | 149 int (±1) | 1 | ≤ +1 Ir/блок | int (±1) | ok |
| thrash64 | alloc Ir/op | 22 int (±1) | 22 int (±1) | 0 | ≤ +1 Ir | int (±1) | ok |
| thrash64 | free Ir/op | 84 int (±1) | 84 int (±1) | 0 | ≤ +1 Ir | int (±1) | ok |
| thrash64 cyc | EstCycles/op free | 93.3 | 93.3 | 0 | ≤ +1 Ir | exact | ok |

**Вердикт X-c1: NO-GO.**
<!-- END GENERATED: b4_micro_pack_table.mjs §6.2 -->

<!-- BEGIN GENERATED: b4_micro_pack_table.mjs §6.2b (Δ = base−variant) -->
### §6.2b A/B: X-c1b (A = X-span, B = X-c1b) — порог: ao free ≤ −1 Ir ИЛИ aoc rt alloc ≤ −1 Ir

| судья | метрика | A | B | Δ (base−variant) | порог | точность | вердикт |
|---|---|---|---|---|---|---|---|
| mstress | Ir/op | 368 int (±1) | 369 int (±1) | -1 | ≤ +0.2 % базы | int (±1) | FAIL |
| larson | Ir/op | 279 int (±1) | 275 int (±1) | 4 | ≤ +0.2 % базы | int (±1) | ok |
| ao | alloc Ir/op | 49 int (±1) | 49 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| ao | free Ir/op | 87 int (±1) | 87 int (±1) | 0 | ao free ≤ −1 Ir (порог проверяется точным iai на CI) | int (±1) | не выполнена (target) |
| aoc rt | alloc Ir/op | 49 int (±1) | 49 int (±1) | 0 | aoc rt alloc ≤ −1 Ir (порог проверяется точным iai на CI) | int (±1) | не выполнена (target) |
| aoc rt | free Ir/op | 87 int (±1) | 87 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const16 | alloc Ir/op | 22 int (±1) | 21 int (±1) | 1 | ≤ +0 Ir | int (±1) | ok |
| aoc const16 | free Ir/op | 49 int (±1) | 49 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| aoc const64 | alloc Ir/op | 22 int (±1) | 21 int (±1) | 1 | ≤ +0 Ir | int (±1) | ok |
| aoc const64 | free Ir/op | 49 int (±1) | 49 int (±1) | 0 | ≤ +0 Ir | int (±1) | ok |
| batch | alloc Ir/block | 108 int (±1) | 108 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batch | free Ir/block | 135 int (±1) | 135 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchc | alloc Ir/block | 88 int (±1) | 88 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchc | free Ir/block | 105 int (±1) | 105 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchl | alloc Ir/block | 114 int (±1) | 114 int (±1) | 0 | ≤ +1 Ir/блок | int (±1) | ok |
| batchl | free Ir/block | 150 int (±1) | 149 int (±1) | 1 | ≤ +1 Ir/блок | int (±1) | ok |
| thrash64 | alloc Ir/op | 22 int (±1) | 22 int (±1) | 0 | ≤ +1 Ir | int (±1) | ok |
| thrash64 | free Ir/op | 84 int (±1) | 84 int (±1) | 0 | ≤ +1 Ir | int (±1) | ok |
| thrash64 cyc | EstCycles/op free | 93.3 | 93.3 | 0 | ≤ +1 Ir | exact | ok |

**Вердикт X-c1b: NO-GO.**
<!-- END GENERATED: b4_micro_pack_table.mjs §6.2b -->

Порог был «сумма ao alloc+free ≤ 135»; фактически 136 без изменений —
**ОЦЕНКА**: предполагается, что LLVM и так сворачивал `+1` в encode.
Итерация b (mc-xc1b) дала только const16/64
21 вместо 22, порог не закрыла. Полностью откачен.

### 6.3 N9 v1 / v2 — отклонённые варианты политики заполнения

v1 (fill на обоих сайтах) и v2 (memo-заполнение после own_cache): thrash64
free остался 84.57 (v1) / 84.36 (v2) — точные дробные значения по парам
TOTALS из перегенерированных логов, против 81.43 у v3; mstress/larson
368.86/279.58 — без улучшений. Оба варианта отклонены, в финальный diff не входят.

## 7. Дефект протокола S1-хэшей

Для 8 S1-замеров хэш diff снят из корня репозитория, а не из worktree — в
логах стоит пустой diff `e3b0c442…b855`. Фактический diff S1-шага:
`4e9a152db4f587adb82cca8f017dda05aef7c3f9d191ca3608ebf8eae40525f1`
(зафиксирован здесь ретроспективно; последующие шаги считали хэш корректно
внутри worktree). На сами Ir-числа дефект не влияет — код S1 в замере был
тем же, но протокольная цепочка «хэш перед замером» для S1 разорвана.

Хэши diff по шагам (**таблица справочная, не измерение**; пометка: сняты
`git diff | sha256sum` перед каждым
замером; S1 — ретроспективно восстановленный, см. выше):

| шаг | хэш diff | пометка |
|---|---|---|
| base (mc-base) | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` | пустой diff — ИЗМЕРЕНО |
| S1 | `4e9a152db4f587adb82cca8f017dda05aef7c3f9d191ca3608ebf8eae40525f1` | ретроспективный (дефект протокола) |
| R3 | `bd83c84468ff457b1a76122a13e3bae62f54b648d0d6870f61862a8233a6263` | ИЗМЕРЕНО |
| R3b | `2b56982de2fca1a3239c56171c9bd9349d6da11a684522392161e4bf97d29b66` | ИЗМЕРЕНО |
| X-null | `455ce0aef775e7433ad88a6ed0847f2a957ce0b46f0c740fe7b5296adbfce329` | ИЗМЕРЕНО |
| X-span | `90d3ffb8518625e447c91e7062b0732ffd54cc28b98e9c704745f3a0129c0f51` | ИЗМЕРЕНО |
| X-c1 | `8dd106ac90cc55e55946d9c0f9fa613b0ec73d1173a3a7ff43f6eb968385fb7c` | ИЗМЕРЕНО |
| N9 v1 | `e2d9e7f4feb861ae77d8b4ef984979d1c6f4374c6a0daa4559cbeded6fd29d1b` | ИЗМЕРЕНО |
| N9 v2 | `edf0e12a3e302ac11100e27a007f22876b096ca8f8f35e54dd8d39523a86d55a` | ИЗМЕРЕНО |
| N9 v3 (до фиксов) | `df6af7238720f96c0d6968ab6a8e31c610701fa8317e2d71e58d17448449db05` | ИЗМЕРЕНО |
| mc-xn2 (перезамер ao/aoc/batch-судей X-null) | `b58dbae93cd6fdfbd4e1e2491d2694e090d4fb210fa6d6105e9bfa814558ed8d` | ИЗМЕРЕНО (tree-hash, логи `_raw_b4_micro_mc_xn2_*`) |
| mc-n92 / mc-n92b (перезамер ao/aoc/batch-судей N9) | `1af59799c67764e02f3406bd1102e508401acbc1e760b2fad99276a35f8e5c10` | ИЗМЕРЕНО (логи `_raw_b4_micro_mc_n92*`) |
| финальный (после фиксов) | `058985f68caf7d92b537eeff130c3a1f5bd2b8101391002dce009d912938679f` | см. §8 |
| финальный (после counter-правок; на старте документационного шага) | `daf979894bfdd0f0d4cc13cb5a557d5f65c9b933603200d3a97a679c6f1b003c` | bench-internals счётчики под cfg, на Ir не влияют |
| после док-правок этого шага (скрипт/отчёт/csv) | `8f26c8bf724c1332676a69b0316f870c66b665501386ec2f53d907e271c3cb1c` | только docs/ + scripts/, код не тронут |

## 8. Пост-замерные фиксы (на Ir не влияют)

После снятия всех замеров в финальный diff вошли правки, не влияющие на
горячие пути:

- `cfg_attr`-гейт dead_code на `StateByte::MAG` (`state_byte.rs`);
- clippy-фиксы в `tests/b4_micro_s1_refill_mag_bytes.rs`;
- feature-гейт `alloc-decommit` в `tests/state_byte_oracle.rs`;
- счётчики тестов 445→449 в README/ARCHITECTURE.

Финальный хэш diff после фиксов — `058985f6…` (см. §7); после последующих
counter-правок (счётчики под cfg) — `daf97989…` (см. §7). Все Ir-числа
отчёта соответствуют diff'ам из §7 (до пост-замерных фиксов).

## 9. Тесты и мутанты

<!-- BEGIN GENERATED: b4_micro_pack_table.mjs §9 (логи `_raw_b4_micro_mutant_*.log`) -->
### §9 Мутанты (команда = первая строка лога)

| имя | лог | команда | ожидание RED | полученный exit | process exit | RED |
|---|---|---|---|---|---|---|
| s1a | `_raw_b4_micro_mutant_s1a.log` | `CARGO_TARGET_DIR=$REPO/.rush/cargo-target-C timeout 600 cargo test --locked --features "production batch-api internals bench-internals" --test b4_micro_s1_refill_mag_bytes` | RED — debug_assert in finish_magazine_refill fires ("refill-issued block must arrive MAG..."), test process aborts abnormally. | 127 | 0xc0000409 | RED ✓ |
| s1b | `_raw_b4_micro_mutant_s1b.log` | `CARGO_TARGET_DIR=$REPO/.rush/cargo-target-C timeout 600 cargo test --locked --features "production batch-api internals bench-internals" --test b4_micro_s1_refill_mag_bytes` | RED — full mutant (drain issues LIVE + HEAD finish_magazine_refill tail): counter assert (dbg_s1_mag_byte_writes delta == refill_n) fails. OBSERVED: RED via abnormal test-process abort (0xc0000409) — debug-path panic in the mutated refill tail, counter assert never reached. | 127 (счётчикный assert не достигнут (abort раньше)) | 0xc0000409 | RED ✓ |
| xnull | `_raw_b4_micro_mutant_xnull.log` | `CARGO_TARGET_DIR=$REPO/.rush/cargo-target-C timeout 600 cargo test --locked --features "production batch-api internals bench-internals" --test b4_micro_xnull_null_free_noop   # cwd: worktrees/b4-micro (mutant: null-guard body removed in src/registry/heap_core_xthread/routing.rs publish_foreign; RC=101 RED)` | (mutant: null-guard body removed in src/registry/heap_core_xthread/routing.rs publish_foreign | 101 | — | RED ✓ |
| span | `_raw_b4_micro_mutant_span.log` | `CARGO_TARGET_DIR=$REPO/.rush/cargo-target-C timeout 600 cargo test --locked --features "production batch-api internals bench-internals exact-span-large" --test b4_micro_xspan_small_primordial_mask` | RED — LOCAL_REJECT 1!=0 in xspan_large_span_bound_untouched (exact Large free rejected: 1 vs 0). | 101 | — | RED ✓ |
| mask | `_raw_b4_micro_mutant_mask.log` | `CARGO_TARGET_DIR=$REPO/.rush/cargo-target-C timeout 600 cargo test --locked --features "production batch-api internals bench-internals" --test b4_micro_xspan_small_primordial_mask` | RED — xspan_small_and_primordial_deeps_free_exactly FAILED (block at off 0x3ff000 must read FREE after dealloc: 0 vs 2). | 101 | — | RED ✓ |
| n9 | `_raw_b4_micro_mutant_n9.log` | `CARGO_TARGET_DIR=$REPO/.rush/cargo-target-C timeout 600 cargo test --locked --features "production batch-api internals bench-internals" --test b4_micro_n9_memo_fill_policy   # cwd: worktrees/b4-micro (mutant: won-probe memo fill restored in segment_table_impl.rs canonical_base_of_mut; RC=101 RED)` | (mutant: won-probe memo fill restored in segment_table_impl.rs canonical_base_of_mut | 101 | — | RED ✓ |
<!-- END GENERATED: b4_micro_pack_table.mjs §9 -->

Примечание: команды восстановлены дословно — теперь это первые строки
логов мутантов; для s1a/s1b тестовый процесс завершился аварийно
(`0xc0000409`, STATUS_STACK_BUFFER_OVERRUN) до того, как cargo смог
сообщить RC=101 (в логе зафиксирован оберточный RC=127 от timeout).

Полноматричный прогон `node scripts/run-check-matrix.mjs`: RC=0, один полный запуск, 2026-10-20, дерево `8f26c8bf724c1332676a69b0316f870c66b665501386ec2f53d907e271c3cb1c` (лог `.rush/wsl/mc_final_matrix2.log`) — **справочная, не измерение**.

Исторические мутанты отклонённых шагов (эксперимент откачен, мутант-код не
сохранился в дереве):

- R3 — мутант теста adjacency: зелёный на приёме / красный RC=101 на откате;
- X-c1 — equivalence-мутант: 2 failed;
- N9 v1/v2 — мутанты не прогонялись отдельно (варианты отклонены по числам).

Тестовые логи сохранены как `docs/perf/_raw_b4_micro_mc_s1_t_*.log`
(S1-итерация: 12 тестов, включая oracle/transitions/property клетки и
`segment_table_contains_base_tier1_counters`).

## 10. Оракулы активации (R30-8)

Числовые оракулы (счётчики bench-internals, счётчики под `cfg`, production
сборки не затрагивают; сырой лог: `_raw_b4_micro_counters.log`):

| приём | числитель | знаменатель | доля |
|---|---|---|---|
<!-- BEGIN GENERATED: b4_micro_pack_table.mjs §10 (лог `_raw_b4_micro_counters.log`) -->
### §10 Оракулы активации (рендер `scripts/b4_micro_pack_table.mjs`, лог `_raw_b4_micro_counters.log`)

| приём | числитель | знаменатель | доля |
|---|---|---|---|
| S1 | 45 (MAG-записи refill-блоков минус события) | 45 (retained = сумма n_i−1) | 100.00% |
| X-null | tail guard в `publish_foreign` = 64 | 64 | 100.00% |
| X-span | маска-ветка taken = 512 | calls = 516 (small 512 + Large 4) | 99.22% (4 Large-вызова не в маске-ветке — по построению) |
| N9 | fill при own_cache-хите = 1 | 2 потенциальных fill-события (won-probe exact 1 + own_cache-hit fill 1) | 50.00% — это доля отличий политики заполнения, а не активация ветки; раскладка попаданий memo:own_cache = 10:1 (memo-share 90.91%) |

Оба прогона счётчиков в логе совпадают побитово (determinism check пройден ассертом скрипта).
<!-- END GENERATED: b4_micro_pack_table.mjs §10 -->

- **S1:** таблица выше заменяет прежний аналитический текст; `debug_assert`
  в `finish_magazine_refill` остаётся дополнительной страховкой.
- **R3:** аналитический/атрибуционный (приём NO-GO): доля соседств
  недостаточна — статья `magazine.rs` 36.9 → 40.1 Ir/op в построчной
  атрибуции mc_s1 vs mc_r3.
- **N9:** счётчики `CONTAINS_BASE_TIER1_*` в bench-internals
  (`tests/segment_table_contains_base_tier1_counters.rs`,
  `tests/b4_micro_n9_memo_fill_policy.rs`) — см. таблицу выше. Знаменатель
  N9-оракула = 2 (won-probe exact 1 + own_cache-hit fill 1) — честная доля
  отличий политики заполнения (50.00%), а не доля активации ветки;
  активация доказывается судьями (thrash64 free, batchl free). Оба прогона
  счётчиков в логе совпадают (determinism check ассертом скрипта).

## 11. Риски

- X-null переносит отбраковку null в `publish_foreign`: контракт
  «null-free — no-op» теперь держится на хвостовом guard'е; любые новые
  пути, попадающие в free мимо guard'а, — потенциальный UB. Miri-клетки
  обязательны.
- X-span опирается на SEGMENT-выравнивание Small/Medium-сегментов; изменение
  аллокатора сегментов сломает инвариант молча — держать тест
  `b4_micro_xspan_small_primordial_mask` в обязательном наборе.
- N9 memo — per-`AllocCore` состояние; поведение при teardown/re-init
  покрыто n9-тестом, при изменении `canonical_base_of_mut` перепроверять.

## 12. Что гнать на CI

- iai A/B судей: mstress, larson, ao, batch, thrash64 (все шаги пакета).
- Miri-клетки зоны: `state_byte_*` miri-клетки, `b4_r1_pin_cache_miri`.

## 13. CI-квитанции (iai A/B и mt-bench) — измерение, без изменения runtime

Вывод `node scripts/b4_micro_ci_table.mjs` (сырьё: `_raw_b4_micro_iai_ab.csv`,
`_raw_b4_micro_iai_ab_verdict.log` — r20 gate-run 39517950738 на `e26321de`;
`_raw_b4_micro_iai_ab_v1.csv` — r20 39511431025 на `bed33681` (ещё с
горячими счётчиками-наблюдателями); `_raw_b4_micro_mt_v1a.log`/`_v1b.log`/`_v2.log` —
mt-bench 39511430913 / 39511430538 / 39517950754, усечены до таблиц; сводка —
`B4_MICRO_PACK_ci_summary.csv`):

```text
iai A/B (r20 gate-run, A = `4a2071ed`, B = `e26321de`, features `production bench-internals internals`):
- cells with A > 100 Ir: 77; better 62, worse 8, equal 7; largest regression 0.44 %
- mimalloc control cells: 13, all Δ = 0: yes

| cell (largest gains) | A Ir | B Ir | Δ | Δ % |
|---|---:|---:|---:|---:|
| `oscillating_live_set_16b` | 68386 | 67068 | -1318 | -1.93 % |
| `recycle_alloc_free_256x16b` | 151581 | 148732 | -2849 | -1.88 % |
| `recycle_alloc_free_256x64b` | 153084 | 150232 | -2852 | -1.86 % |
| `growth16_n` | 13489142 | 13247350 | -241792 | -1.79 % |
| `recycle_alloc_only_256x16b` | 124257 | 122284 | -1973 | -1.59 % |
| `growth16_2n` | 36371064 | 35887480 | -483584 | -1.33 % |
| `dealloc_free_only_1088_16b_n1024` | 269515 | 266092 | -3423 | -1.27 % |
| `cold_alloc_free_256x16b_4n` | 271360 | 268026 | -3334 | -1.23 % |
| `cold_alloc_free_256x16b_2n` | 156484 | 154816 | -1668 | -1.07 % |
| `dealloc_free_only_1088_16b_n256` | 190411 | 188428 | -1983 | -1.04 % |

| cell (all regressions) | A Ir | B Ir | Δ | Δ % |
|---|---:|---:|---:|---:|
| `r19_prf01_publication` | 149610 | 150265 | 655 | 0.44 % |
| `carve_batch_only_16b` | 3453 | 3462 | 9 | 0.26 % |
| `seg_cycle_decommit_256k` | 1673547 | 1676348 | 2801 | 0.17 % |
| `carve_batch_only_16b_2n` | 9610 | 9619 | 9 | 0.09 % |
| `multiseg_cold_256k` | 1554997 | 1556070 | 1073 | 0.07 % |
| `large_cache_hit_only_4mib` | 57877 | 57885 | 8 | 0.01 % |
| `large_cache_free_slot_search_prefill_only` | 61280 | 61287 | 7 | 0.01 % |
| `large_cache_free_slot_search_cycle_only` | 73433 | 73440 | 7 | 0.01 % |

B side, first head (hot-path observer counters) → final head: 74 cells changed; largest:

| cell | B v1 | B v2 | v1 − v2 |
|---|---:|---:|---:|
| `growth16_4n` | 110094863 | 109479439 | 615424 |
| `growth16_2n` | 36195192 | 35887480 | 307712 |
| `growth16_n` | 13401206 | 13247350 | 153856 |
| `r19_prf01_joined_producer_same_word` | 488309 | 480117 | 8192 |
| `cold_alloc_free_256x16b_4n` | 269762 | 268026 | 1736 |
| `recycle_alloc_free_256x64b` | 151893 | 150232 | 1661 |
| `recycle_alloc_free_256x16b` | 150387 | 148732 | 1655 |
| `dealloc_free_only_1088_16b_n1024` | 267746 | 266092 | 1654 |

mt-bench, production (identical production code in all three PR #89 runs), SeferAlloc / mimalloc ops-per-sec ratio:

| OS | workload | T | PR #87 head (run1 / run2) | PR #89, 6 runs: min … max |
|---|---|---:|---|---|
| ubuntu-24.04 | larson | 1 | 0.91 / 0.93 | 0.76 … 0.92 |
| ubuntu-24.04 | larson | 2 | 1.30 / 1.02 | 0.64 … 1.25 |
| ubuntu-24.04 | larson | 4 | 1.27 / 1.26 | 1.12 … 1.52 |
| ubuntu-24.04 | mstress | 1 | 0.77 / 0.76 | 0.62 … 0.78 |
| ubuntu-24.04 | mstress | 2 | 0.75 / 0.75 | 0.62 … 1.13 |
| ubuntu-24.04 | mstress | 4 | 0.69 / 0.74 | 0.62 … 0.75 |
| windows-2022 | larson | 1 | 0.75 / 0.72 | 0.70 … 0.72 |
| windows-2022 | larson | 2 | 0.95 / 0.73 | 0.74 … 0.95 |
| windows-2022 | larson | 4 | 0.83 / 0.65 | 0.82 … 0.96 |
| windows-2022 | mstress | 1 | 0.54 / 0.54 | 0.41 … 0.55 |
| windows-2022 | mstress | 2 | 0.73 / 0.55 | 0.41 … 0.68 |
| windows-2022 | mstress | 4 | 0.66 / 0.56 | 0.59 … 0.63 |
```

**Итог iai (детерминированный судья).** Второй прогон v1 (r20 39511430638)
дал побитово те же Ir во всех ячейках, что и 39511431025. Регрессии финальной
головы — ≤ 0.44 % (8 ячеек из 77): `r19_prf01_publication` (+655 Ir) и
carve-ячейки (`carve_batch_only_16b` +9 Ir, `seg_cycle_decommit_256k` +0.17 %,
`multiseg_cold_256k` +0.07 %) — путь S1 (`issue_state`-параметр carve/drain и
`bench-internals`-счётчики S1 на refill); ни одна не входит в предрегистрированные
судьи §0.1, которые все прошли. Разница v1 → v2 на стороне B (`growth16_4n`
−615 424 Ir, producer −8 192 Ir) — это ровно удалённые горячие наблюдатели
(см. «Наблюдатели после замера»): в production-сборке их не было никогда.

**mt-bench — справочно, не судья.** Шесть прогонов PR #89 — один и тот же
production-код (наблюдатели только под `bench-internals`), а разброс отношения
к mimalloc внутри них доходит до 0.64 … 1.25 (ubuntu larson T=2); значения
головы PR #87 лежат внутри или на границе этого разброса во всех 12 ячейках.
Межпрогонное сравнение на общих VM не различает эффекты такого размера.

## Приложение А. Сверка с сырыми логами

Вывод `node scripts/b4_micro_pack_table.mjs` (парсит
`docs/perf/_raw_b4_micro_*.log`); расхождений с таблицей §1 нет, кроме
ячейки R3/aoc-rt (в логе 49/92, в сводке «—» — замер был, в таблицу не
включён как несущественный). Полный вывод —
`docs/perf/B4_MICRO_PACK_tables.generated.md`.

### Приложение А.1. Статус сырых логов X-null / N9 (перегенерация)

Первоначально сырые логи mc-xn на диске были заглушками неудавшихся первых
прогонов (EXIT=127), а логи mc_n9 отсутствовали. Сохранённые callgrind
выходы в WSL `~/ab` позволили перегенерировать часть логов заново
(пересчёт marginal Ir/op = (TOTALS@100k − TOTALS@50k)/50000 из
callgrind_annotate; для thrash64 — также EstCycles/op из cache-sim;
файлы `_raw_b4_micro_mc_xn_*.log` и `_raw_b4_micro_mc_n9*.log`,
источник указан в заголовке каждого файла: «regenerated from callgrind
out-files in ~/ab»).

Подкреплены перегенерированными сырыми логами (пересчитанные числа сходятся
со сводкой §3 и csv: mc-xn mstress=372.66, larson=281.54, thrash64 22/89.33,
EstCycles free=98.2; mc-n9 mstress=368.86, larson=279.58, thrash64 22/81.43,
EstCycles free=90.3; mc-n9v1/v2 thrash64 free=84.57/84.36):

- mc-xn: mstress, larson, thrash64;
- mc-n9 (= N9 v3): mstress, larson, thrash64;
- mc-n9v1, mc-n9v2 (отклонённые варианты): mstress, larson, thrash64.

### Приложение А.2. Полная матрица «шаг × судья → файл сырого лога»

Подкреплены перегенерированными сырыми логами
(пересчёт marginal Ir/op = (TOTALS@100k − TOTALS@50k)/50000 из
callgrind_annotate; для thrash64 — также EstCycles/op из cache-sim; для
alloc-rt — секция «regenerated alloc-rt» в `_raw_b4_micro_mc_{xn2,n92}_aoc.log`
с точной парой TOTALS(50k)=2 751 523 / TOTALS(100k)=5 226 799 → 49.51 Ir/op
— прежняя пометка «alloc ?» снята):

Дерево X-null (перезамер, tree-hash `b58dbae9…`) и финальное дерево N9
(`1af59799…`): ao/aoc-rt/c16/c64 и batch-судьи подкреплены сырыми логами
`_raw_b4_micro_mc_xn2_*` и `_raw_b4_micro_mc_n92_*` (N9 batch —
`_raw_b4_micro_mc_n92b_batch.log` после перезапуска; значения совпадают с
mc_n92_batch по free, alloc прочитан из n92b). Пробелов по ao/aoc/batch
больше нет.

Пометка: матрица ниже — справочная, не измерение.

| шаг | mstress | larson | ao | aoc rt | aoc c16 | aoc c64 | batch | batchc | batchl | thrash64 |
|---|---|---|---|---|---|---|---|---|---|---|
| mc-base | `mc_base_mstress` | `mc_base_larson` | `mc_base_ao` | `mc_base_aoc` | `mc_base_aoc` | `mc_base_aoc` | `mc_base_batch` | `mc_base_batchc` | `mc_base_batchl` | `mc_base_thrash64` |
| mc-s1 | `mc_s1_mstress` | `mc_s1_larson` | `mc_s1_ao` | `mc_s1_aoc` (alloc `?`) | `mc_s1_aoc` | `mc_s1_aoc` | `mc_s1_batch` | `mc_s1_batchc` | `mc_s1_batchl` | `mc_s1_thrash64` |
| mc-r3 | `mc_r3_mstress` | `mc_r3_larson` | `mc_r3_ao` | `mc_r3_aoc` | `mc_r3_aoc` | `mc_r3_aoc` | `mc_r3_batch` | `mc_r3_batchc` | `mc_r3_batchl` | `mc_r3_thrash64` |
| mc-r3b | `mc_r3b_mstress` | — | — | — | — | — | — | — | — | — |
| mc-xn | `mc_xn_mstress`* | `mc_xn_larson`* | `mc_xn2_ao` | `mc_xn2_aoc` | `mc_xn2_aoc` | `mc_xn2_aoc` | `mc_xn2_batch` | `mc_xn2_batchc` | `mc_xn2_batchl` | `mc_xn_thrash64`* |
| mc-xs | `mc_xs_mstress` | `mc_xs_larson` | `mc_xs_ao` | `mc_xs_aoc` | `mc_xs_aoc` | `mc_xs_aoc` | `mc_xs_batch` | `mc_xs_batchc` | `mc_xs_batchl` | `mc_xs_thrash64` |
| mc-xc1 | `mc_xc1_mstress` | `mc_xc1_larson` | `mc_xc1_ao` | `mc_xc1_aoc` | `mc_xc1_aoc` | `mc_xc1_aoc` | `mc_xc1_batch` | `mc_xc1_batchc` | `mc_xc1_batchl` | `mc_xc1_thrash64` |
| mc-xc1b | `mc_xc1b_mstress` | `mc_xc1b_larson` | `mc_xc1b_ao` | `mc_xc1b_aoc` | `mc_xc1b_aoc` | `mc_xc1b_aoc` | `mc_xc1b_batch` | `mc_xc1b_batchc` | `mc_xc1b_batchl` | `mc_xc1b_thrash64` |
| mc-n9 | `mc_n9_mstress`* | `mc_n9_larson`* | `mc_n92_ao` | `mc_n92_aoc` | `mc_n92_aoc` | `mc_n92_aoc` | `mc_n92b_batch` | `mc_n92_batchc` | `mc_n92_batchl` | `mc_n9_thrash64`* |
| mc-n9v1/v2 | `mc_n9v{1,2}_mstress` | `mc_n9v{1,2}_larson` | — | — | — | — | — | — | — | `mc_n9v{1,2}_thrash64` |

Имена в таблице — суффиксы файлов `_raw_b4_micro_<имя>.log`.
«*» — лог перегенерирован из callgrind out-files в `~/ab`
(см. выше); «—» — замер не проводился / сырой лог отсутствует;
«alloc ?» — в логе нет строки summary для alloc (число подтверждено
сводкой замеров и/или парным логом). Счётчик-оракул —
`_raw_b4_micro_counters.log`.

Старые prof-логи (base/s1/r3/xs/xc1/xc1b) содержат только целые числа без
точных пар TOTALS — такие ячейки помечены в §1 суффиксом `int (±1)`.
Raw outs для них отсутствуют — судьи остались подтверждёнными только
summary-строками логов:

- mc-xn: mstress/larson/thrash64 — перегенерированы (точные пары есть),
  ao/aoc/batch-судьи — логи `mc_xn2_*`;
- mc-n9 (= N9 v3): mstress/larson/thrash64 — перегенерированы, ao/aoc/batch
  — логи `mc_n92*`; alloc-rt — секция «regenerated alloc-rt» (49.51).
