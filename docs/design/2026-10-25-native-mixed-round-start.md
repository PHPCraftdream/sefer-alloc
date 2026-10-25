# Native mixed round — вход и disposition

Дата: 2026-10-25. Полный end-to-end read предыдущего раунда удостоверен `native_owner_codegen/round_start.json`. В новом раунде не заявляется повторное чтение неизменённых ~1.1 MiB correctness tiers: SHA-256 проверены целиком; все десять ACTIVE/thematic files byte-identical. Для двух top indexes весь остаток после удаления только уже прочитанных round-note/item118-closure побайтно совпадает с прежним полностью прочитанным snapshot; неизвестных изменений нет. Изменённые блоки прочитаны в diff, perf prefix1–1150 дополнительно перечитан. Это явная оптимизация повторного чтения в рамках той же сессии, не новый независимый full-read receipt.

Каждый прежний record наследует LEAVE status/trigger, кроме явно указанных ниже. Нет закрытия correctness/CI debt по benchmark результатам. Box164, process-exit185, TLS196, production/instrumented226 и unsafe benchmark-hook obligations сохраняются. Старые scalar tasks12/13 не являются live executors.

| Perf item | Disposition |
|---|---|
| 39 | LEAVE TLS native lowering receipt; no portability conclusion. |
| 67 | LEAVE old frame verdicts; no attribute sweep. |
| 81 | CONSIDER judge sensitivity only; old acceptance verdict unchanged. |
| 90 | DEFER U31: requires publication/skip Loom proof and separate explicit protocol scope. |
| 101 | DEFER MT discovery redesign until measured cause. |
| 102 | LEAVE frame experiments; leaf static gain not accepted speedup. |
| 103 | CONSIDER matched mixed-workload attribution; old hot/refill proportions not transferred. |
| 104 | CONSIDER attribution only: S1 already accepted, R3 already NO-GO; no resurrection from stale estimates. |
| 117 | B CLOSED in own-free-context index/archive; not baseline. |
| 118 | CLOSED / INCONCLUSIVE / DO NOT PROMOTE; not baseline. |

Per-record snapshot: `../perf/native_mixed_round_start.json`. Следующие задачи: judge calibration/attribution, кандидат только при evidence, gates и synthesis; no tests/agents/commit/push/merge.
