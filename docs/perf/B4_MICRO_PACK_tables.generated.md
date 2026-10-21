### Self-test PREREG (встроенный, всегда): пройден

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

### Сводка общих вердиктов шагов (вычислено из PREREG)

| шаг | цель | вердикт |
|---|---|---|
| S1 | mstress op ≤ −0.3 % базы И larson op ≤ −0 Ir | GO |
| R3 | mstress op ≤ −0.3 % базы | NO-GO |
| X-null | ao free ≤ −1 Ir ИЛИ aoc rt alloc ≤ −1 Ir | GO |
| X-span | ao free ≤ −1 Ir ИЛИ aoc rt alloc ≤ −1 Ir | GO |
| X-c1 | ao free ≤ −1 Ir ИЛИ aoc rt alloc ≤ −1 Ir | NO-GO |
| X-c1b | ao free ≤ −1 Ir ИЛИ aoc rt alloc ≤ −1 Ir | NO-GO |
| N9 v3 | thrash64 free ≤ −2 Ir | NO-GO |
| N9 v1 | thrash64 free ≤ −2 Ir | rejected |
| N9 v2 | thrash64 free ≤ −2 Ir | rejected |

### §9 Мутанты (команда = первая строка лога)

| имя | лог | команда | ожидание RED | полученный exit | process exit | RED |
|---|---|---|---|---|---|---|
| s1a | `_raw_b4_micro_mutant_s1a.log` | `CARGO_TARGET_DIR=$REPO/.rush/cargo-target-C timeout 600 cargo test --locked --features "production batch-api internals bench-internals" --test b4_micro_s1_refill_mag_bytes` | RED — debug_assert in finish_magazine_refill fires ("refill-issued block must arrive MAG..."), test process aborts abnormally. | 127 | 0xc0000409 | RED ✓ |
| s1b | `_raw_b4_micro_mutant_s1b.log` | `CARGO_TARGET_DIR=$REPO/.rush/cargo-target-C timeout 600 cargo test --locked --features "production batch-api internals bench-internals" --test b4_micro_s1_refill_mag_bytes` | RED — full mutant (drain issues LIVE + HEAD finish_magazine_refill tail): counter assert (dbg_s1_mag_byte_writes delta == refill_n) fails. OBSERVED: RED via abnormal test-process abort (0xc0000409) — debug-path panic in the mutated refill tail, counter assert never reached. | 127 (счётчикный assert не достигнут (abort раньше)) | 0xc0000409 | RED ✓ |
| xnull | `_raw_b4_micro_mutant_xnull.log` | `CARGO_TARGET_DIR=$REPO/.rush/cargo-target-C timeout 600 cargo test --locked --features "production batch-api internals bench-internals" --test b4_micro_xnull_null_free_noop   # cwd: worktrees/b4-micro (mutant: null-guard body removed in src/registry/heap_core_xthread/routing.rs publish_foreign; RC=101 RED)` | (mutant: null-guard body removed in src/registry/heap_core_xthread/routing.rs publish_foreign | 101 | — | RED ✓ |
| span | `_raw_b4_micro_mutant_span.log` | `CARGO_TARGET_DIR=$REPO/.rush/cargo-target-C timeout 600 cargo test --locked --features "production batch-api internals bench-internals exact-span-large" --test b4_micro_xspan_small_primordial_mask` | RED — LOCAL_REJECT 1!=0 in xspan_large_span_bound_untouched (exact Large free rejected: 1 vs 0). | 101 | — | RED ✓ |
| mask | `_raw_b4_micro_mutant_mask.log` | `CARGO_TARGET_DIR=$REPO/.rush/cargo-target-C timeout 600 cargo test --locked --features "production batch-api internals bench-internals" --test b4_micro_xspan_small_primordial_mask` | RED — xspan_small_and_primordial_deeps_free_exactly FAILED (block at off 0x3ff000 must read FREE after dealloc: 0 vs 2). | 101 | — | RED ✓ |
| n9 | `_raw_b4_micro_mutant_n9.log` | `CARGO_TARGET_DIR=$REPO/.rush/cargo-target-C timeout 600 cargo test --locked --features "production batch-api internals bench-internals" --test b4_micro_n9_memo_fill_policy   # cwd: worktrees/b4-micro (mutant: won-probe memo fill restored in segment_table_impl.rs canonical_base_of_mut; RC=101 RED)` | (mutant: won-probe memo fill restored in segment_table_impl.rs canonical_base_of_mut | 101 | — | RED ✓ |

### §10 Оракулы активации (рендер `scripts/b4_micro_pack_table.mjs`, лог `_raw_b4_micro_counters.log`)

| приём | числитель | знаменатель | доля |
|---|---|---|---|
| S1 | 45 (MAG-записи refill-блоков минус события) | 45 (retained = сумма n_i−1) | 100.00% |
| X-null | tail guard в `publish_foreign` = 64 | 64 | 100.00% |
| X-span | маска-ветка taken = 512 | calls = 516 (small 512 + Large 4) | 99.22% (4 Large-вызова не в маске-ветке — по построению) |
| N9 | fill при own_cache-хите = 1 | 2 потенциальных fill-события (won-probe exact 1 + own_cache-hit fill 1) | 50.00% — это доля отличий политики заполнения, а не активация ветки; раскладка попаданий memo:own_cache = 10:1 (memo-share 90.91%) |

Оба прогона счётчиков в логе совпадают побитово (determinism check пройден ассертом скрипта).

### CSV — `docs/perf/B4_MICRO_PACK_summary.csv` (перезаписан скриптом: 160 строк данных, из них 19 базовых; Δ = base − variant; 141 из 141 шаг-строк перечитаны и пересчитаны из сырья — сверка пройдена (базовые 19 — источник, не дельта); вердикты шагов из CSV совпадают с отчётом)

