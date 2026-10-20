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

### 8. Приложение: сырые артефакты

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
