# Own-thread free overflow outlining — runtime-кандидат и измеренный эффект

Дата: 2026-10-24. **NO-GO: OVERFLOW CONTROL**. Это эксперимент, не принятый runtime change.

## Что проверялось

База f80c681b799ef4ef7fa08d79c06896fdf1104fe9. Production diff — один файл dealloc_own_base.rs: только заполненный magazine вынесен в private cold/inline-never helper. Normal push и вся guard-цепь остаются inline; policy/counters/orderings/public API не менялись. D1′ и scalar issue не включены. Пререгистрация: docs/design/2026-10-24-own-free-frame-candidate.md. Все source identities записаны до соответствующих сборок; hashes и точные patches находятся в own_free_cost/.

## Детерминированный free: относительно mimalloc до/после

Production-only GlobalAlloc facade, WSL Linux, не installed allocator и не native wall clock. Один бинарник каждого source arm содержит обоих allocators. N/2N=50000/100000, два независимых прохода. Ir/free — marginal wrapper-inclusive instruction count, не ns и не чистая стоимость тела allocator. Контроль mimalloc и повторяемость сверены с raw; backend- и source-различия не скрыты.

| Mode | Sefer A Ir/free | Sefer B | mimalloc A | mimalloc B | mi/A | mi/B | B/A |
|---|---:|---:|---:|---:|---:|---:|---:|
| rt | 87.3590 | 80.4103 | 31.0021 | 31.0021 | 0.3549 | 0.3855 | 0.9205 |
| 16 | 52.0000 | 43.0000 | 28.0000 | 28.0000 | 0.5385 | 0.6512 | 0.8269 |
| 64 | 52.0000 | 43.0000 | 28.0000 | 28.0000 | 0.5385 | 0.6512 | 0.8269 |

## Проверка цены overflow

Bulk round: alloc64, затем free64. Sizes64B/16KiB; 100→200 rounds, normalizer6400 frees; два прохода каждого source/allocator arm. Это намеренно overflow-heavy control, а не такой же regime, как const churn. Callgraph activation oracle: helper и flush call counts сверены с ожидаемым числом реальных событий; activation.json.

| Mode | Sefer A Ir/free | Sefer B | mimalloc A | mimalloc B | mi/A | mi/B | B/A |
|---|---:|---:|---:|---:|---:|---:|---:|
| bulk64 | 124.8125 | 118.0938 | 31.8906 | 31.8906 | 0.2555 | 0.2700 | 0.9462 |
| bulk16k | 235.6250 | 253.0000 | 73.1875 | 73.1875 | 0.3106 | 0.2893 | 1.0737 |

## Native Windows: явно относительно mimalloc до/после

CPU 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz; win32/10.0.19045/x64; features production. 20 fresh-process balanced blocks/mode: A/miA/B/miB/miB/B/miA/A. На block усредняются два одноимённых процесса, далее арифметическое среднее по20 blocks. Win=mean(mimalloc)/mean(Sefer); >1 — Sefer быстрее, <1 — медленнее. B/A<1 — сокращение времени Sefer. CI — Student-t paired mean B−A, df19. Это composite workload с setup/churn/teardown, НЕ pure free latency. Последовательные bulk allocations намеренно активируют другие paths, чем churn.

### Timed workload (ns/pair composite)

| Mode | Sefer A | Sefer B | mimalloc A | mimalloc B | Win A | Win B | B/A | Paired delta 95% CI ns/pair | Result |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| 16 | 6.4012 | 6.2068 | 6.6749 | 6.5288 | 1.0428 | 1.0519 | 0.9696 | -0.9816 … 0.5928 | INCONCLUSIVE |
| 64 | 6.0907 | 5.5842 | 6.7023 | 7.1712 | 1.1004 | 1.2842 | 0.9168 | -0.9873 … -0.0257 | LOWER |
| rt | 13.3690 | 13.9307 | 11.1918 | 11.9358 | 0.8371 | 0.8568 | 1.0420 | -0.9587 … 2.0821 | INCONCLUSIVE |
| bulk64 | 18.9954 | 19.1130 | 11.6515 | 11.5034 | 0.6134 | 0.6019 | 1.0062 | -1.3908 … 1.6261 | INCONCLUSIVE |
| bulk16k | 34.6442 | 39.3553 | 54.8073 | 52.5899 | 1.5820 | 1.3363 | 1.1360 | 0.7118 … 8.7105 | HIGHER |

### Full process (ms/process)

| Mode | A | B | mi A | mi B | B/A | Paired delta 95% CI ms |
|---|---:|---:|---:|---:|---:|---|
| 16 | 14.5752 | 14.2901 | 14.6718 | 14.7089 | 0.9804 | -1.5444 … 0.9741 |
| 64 | 13.5153 | 13.1630 | 14.1991 | 14.8220 | 0.9739 | -1.1558 … 0.4513 |
| rt | 21.2506 | 22.6088 | 19.1765 | 20.8619 | 1.0639 | -0.8862 … 3.6027 |
| bulk64 | 7.2131 | 7.1501 | 7.0746 | 7.1726 | 0.9913 | -0.5881 … 0.4621 |
| bulk16k | 7.5310 | 7.6697 | 7.9873 | 8.0456 | 1.0184 | -0.2712 … 0.5486 |

### A/A и неизменённый mimalloc

- A/A workload_ns: paired delta -264435.0000 ns, 95% CI -2136506.9847 … 1607636.9847 ns.
- A/A process_ns: paired delta -493050.0000 ns, 95% CI -2747427.5758 … 1761327.5758 ns.
- mimalloc 16: mean B/A=0.9781; paired 95% CI -943810.7809 … 651630.7809 ns/workload.
- mimalloc 64: mean B/A=1.0700; paired 95% CI -168282.9505 … 1106132.9505 ns/workload.
- mimalloc rt: mean B/A=1.0665; paired 95% CI -800367.4233 … 2289552.4233 ns/workload.
- mimalloc bulk64: mean B/A=0.9873; paired 95% CI -18783.2047 … 13933.2047 ns/workload.
- mimalloc bulk16k: mean B/A=0.9595; paired 95% CI -126044.9820 … 53384.9820 ns/workload.

Если A/A или mimalloc control показывает направленный сдвиг, native attribution ограничена; красивое отношение само по себе не доказывает causal speedup. Полная IAI/canonical/Larson/mstress/MT/retention приёмка не заявляется; при провале overflow control её не запускали ради спасения кандидата.

## Безопасность и состояние проверки

Guard-before-mutation сохранён. Helper получает table-derived OwnedBlock после успешного small_free_guard и cnt>=cap; owner-exclusive reads count/cap/root/offset повторены до mutation. Eviction возвращает в substrate старый magazine block, не текущий Box-frame block. Порядок flush/compact/state/mask/counters сохранён. Существующие unsafe flush sites перемещены в тот же seam; новых public hooks или safe raw-pointer metadata API нет.

Новая tests/own_free_overflow_outline.rs проверяет full16 и smaller byte cap, exact residency/state/live credits и последующий reuse. Она прошла Clippy/compile-check вместе с production internals bench-internals; negative core-only internals check прошёл. Test execution/Miri/Loom/CI не получены, локально не разрешены. Поэтому никаких acceptance/merge claims; commit/push/merge не выполнялись.

Raw artifacts и reproducible scripts: own_free_cost/. Проверка отчёта: node scripts/own_free_report.mjs --check. Source paths в опубликованных profiles нормализованы, числовые строки сохранены. Baseline attribution/codegen получены до product edits; новые benchmark declaration/harness snapshots каждого этапа имеют свои identities.
