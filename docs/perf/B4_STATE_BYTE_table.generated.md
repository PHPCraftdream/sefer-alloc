## Таблица судей (сгенерировано scripts/b4_state_byte_table.mjs)

Источник: docs/perf/B4_STATE_BYTE_summary.csv, сверено с сырыми логами
docs/perf/_raw_b4_state_byte_*.log (база A = снапшот 897463e6, дерево B = байт состояния).
Примечание: base rt-alloc отсутствует в sb-base-aoc, использовано значение ao (55).

| Судья | Метрика | A (base) | B (байт) | Δ | Δ% | Порог | Вердикт | Сверка с логом |
|---|---|---|---|---|---|---|---|---|
| ao | alloc Ir/op | 55 | 50 | -5 | -9.09% | <= -3 | GO | OK |
| ao | free Ir/op | 105 | 93 | -12 | -11.43% | <= -5 | GO | OK |
| ao | EstCycles alloc | 62.0 | 61.4 | -0.6 | -0.97% | — | INFO | OK |
| ao | EstCycles free | 110.2 | 100.4 | -9.8 | -8.89% | — | INFO | OK |
| const | alloc rt Ir/op (A=ao fallback) | 55 | 50 | -5 | -9.09% | не хуже +1 | GO | OK |
| const | free rt Ir/op | 105 | 93 | -12 | -11.43% | не хуже +1 | GO | OK |
| const | alloc c16 Ir/op | 26 | 22 | -4 | -15.38% | не хуже +1 | GO | OK |
| const | free c16 Ir/op | 66 | 54 | -12 | -18.18% | не хуже +1 | GO | OK |
| const | alloc c64 Ir/op | 26 | 22 | -4 | -15.38% | не хуже +1 | GO | OK |
| const | free c64 Ir/op | 66 | 54 | -12 | -18.18% | не хуже +1 | GO | OK |
| batch | alloc Ir/call (256/round) | 126 | 114 | -12 | -9.52% | <= +3 | GO | OK |
| batch | free Ir/call (256/round) | 159 | 141 | -18 | -11.32% | <= +3 | GO | OK |
| batch_const | alloc16 Ir/call (256/round) | 106 | 94 | -12 | -11.32% | <= +3 | GO | OK |
| batch_const | free16 Ir/call (256/round) | 128 | 110 | -18 | -14.06% | <= +3 | GO | OK |
| batchl | alloc Ir/call (258/round) | 131 | 120 | -11 | -8.40% | <= +3 | GO | OK |
| batchl | free Ir/call (258/round) | 172 | 155 | -17 | -9.88% | <= +3 | GO | OK |
| larson | Ir/op (T=1) | 302 | 283 | -19 | -6.30% | <= -1% | GO | OK |
| mstress | Ir/op (T=1) | 422 | 384 | -38 | -9.00% | <= 0 | GO | OK |
| thrash64 | alloc Ir/op | 26 | 22 | -4 | -15.38% | <= +3 | GO | OK |
| thrash64 | free Ir/op | 103 | 91 | -12 | -11.65% | <= +3 | GO | OK |
| thrash64 | EstCycles alloc | 26.0 | 22.0 | -4.0 | -15.38% | <= +3% | GO | OK |
| thrash64 | EstCycles free | 112.6 | 100.2 | -12.4 | -11.01% | <= +3% | GO | OK |
| rss | post_total_committed KiB | 32768 | 36864 | 4096 | +12.50% | <= +5% | NO-GO (accepted by owner 2026-10-20) | OK |
| rss | small_pooled segments | 6 | 7 | 1 | +16.67% | — | INFO | OK |
| rss | small_pooled committed KiB | 24576 | 28672 | 4096 | +16.67% | — | INFO | OK |
| oracle | state_byte mismatches /N (denominator = loop N) | — | 0/100000 | 0 mismatches | 100.0000% matching | >= 95% matching | GO | OK |
| capacity | 16B blocks/segment base→pre-F6→post-F6 | 257536 | 257536→241152→245248 | exact aligned strides | — | — | INFO | OK (exact capacity assertions) |

Согласованность CSV ↔ сырые логи: OK (27 строк, 0 расхождений).
