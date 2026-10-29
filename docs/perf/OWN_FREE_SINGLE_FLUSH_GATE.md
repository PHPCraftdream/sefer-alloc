# Single-element magazine flush — runtime-кандидат и измеренный эффект

Дата отчёта: 2026-10-25. **EARLY GATES GO; FULL ACCEPTANCE PENDING**. Это эксперимент, не принятый runtime change.

## Что проверялось

База f80c681b799ef4ef7fa08d79c06896fdf1104fe9. Production diff — два файла: crate-private unsafe one-element flush в AllocCore переиспользует прежний flush_run; HeapCore меняет только cap-eviction call site. Frame/half-flush/guard/counter/policy и public API не менялись. Новый вход не cold, inline-never; общий batch-loop/recycled-run bookkeeping не нужен для единственного элемента. D1′ и scalar issue не включены. Пререгистрация: docs/design/2026-10-24-single-flush-prereg.md. Все source identities записаны до соответствующих сборок; hashes и точные patches находятся в own_free_single/.

## Детерминированный free: относительно mimalloc до/после

Production-only GlobalAlloc facade, WSL Linux, не installed allocator и не native wall clock. Один бинарник каждого source arm содержит обоих allocators. N/2N=50000/100000, два независимых прохода. Ir/free — marginal wrapper-inclusive instruction count, не ns и не чистая стоимость тела allocator. Контроль mimalloc и повторяемость сверены с raw; backend- и source-различия не скрыты.

| Mode | Sefer A Ir/free | Sefer B | mimalloc A | mimalloc B | mi/A | mi/B | B/A |
|---|---:|---:|---:|---:|---:|---:|---:|
| rt | 87.3590 | 86.7255 | 31.0021 | 31.0021 | 0.3549 | 0.3575 | 0.9927 |
| 16 | 52.0000 | 52.0000 | 28.0000 | 28.0000 | 0.5385 | 0.5385 | 1.0000 |
| 64 | 52.0000 | 52.0000 | 28.0000 | 28.0000 | 0.5385 | 0.5385 | 1.0000 |

## Проверка цены overflow

Bulk round: alloc64, затем free64. Sizes64B/16KiB; 100→200 rounds, normalizer6400 frees; два прохода каждого source/allocator arm. Это намеренно overflow-heavy control, а не такой же regime, как const churn. Callgraph activation oracle: generic/one-element call counts сверены по raw: A generic6/60 per round; B generic6/0 и one-element0/60 на bulk64/bulk16k. activation.json.

| Mode | Sefer A Ir/free | Sefer B | mimalloc A | mimalloc B | mi/A | mi/B | B/A |
|---|---:|---:|---:|---:|---:|---:|---:|
| bulk64 | 124.8125 | 121.3438 | 31.8906 | 31.8906 | 0.2555 | 0.2628 | 0.9722 |
| bulk16k | 235.6250 | 188.7188 | 73.1875 | 73.1875 | 0.3106 | 0.3878 | 0.8009 |

## Native Windows: явно относительно mimalloc до/после

CPU 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz; win32/10.0.19045/x64; features production. 20 fresh-process balanced blocks/mode: A/miA/B/miB/miB/B/miA/A. На block усредняются два одноимённых процесса, далее арифметическое среднее по20 blocks. Win=mean(mimalloc)/mean(Sefer); >1 — Sefer быстрее, <1 — медленнее. B/A<1 — сокращение времени Sefer. CI — Student-t paired mean B−A, df19. Это composite workload с setup/churn/teardown, НЕ pure free latency. Последовательные bulk allocations намеренно активируют другие paths, чем churn.

### Timed workload (ns/pair composite)

| Mode | Sefer A | Sefer B | mimalloc A | mimalloc B | Win A | Win B | B/A | Paired delta 95% CI ns/pair | Result |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| 16 | 6.0400 | 5.9523 | 6.6171 | 7.1954 | 1.0955 | 1.2088 | 0.9855 | -1.0265 … 0.8512 | INCONCLUSIVE |
| 64 | 6.0585 | 6.2815 | 6.9885 | 6.9124 | 1.1535 | 1.1004 | 1.0368 | -0.4548 … 0.9008 | INCONCLUSIVE |
| rt | 13.5042 | 14.1409 | 10.8923 | 10.7739 | 0.8066 | 0.7619 | 1.0472 | -0.5265 … 1.8000 | INCONCLUSIVE |
| bulk64 | 18.9818 | 19.4353 | 11.4386 | 11.2511 | 0.6026 | 0.5789 | 1.0239 | -1.8011 … 2.7081 | INCONCLUSIVE |
| bulk16k | 36.6740 | 34.2670 | 50.2547 | 51.1272 | 1.3703 | 1.4920 | 0.9344 | -8.1966 … 3.3825 | INCONCLUSIVE |

### Full process (ms/process)

| Mode | A | B | mi A | mi B | B/A | Paired delta 95% CI ms |
|---|---:|---:|---:|---:|---:|---|
| 16 | 13.7337 | 13.6077 | 14.2715 | 14.8374 | 0.9908 | -1.2452 … 0.9932 |
| 64 | 12.9867 | 13.3646 | 14.1239 | 14.1966 | 1.0291 | -0.5028 … 1.2584 |
| rt | 21.6306 | 22.0384 | 18.9514 | 19.1242 | 1.0189 | -1.2823 … 2.0978 |
| bulk64 | 6.8641 | 6.7323 | 6.5939 | 6.5169 | 0.9808 | -0.4356 … 0.1721 |
| bulk16k | 6.9164 | 6.9740 | 7.1311 | 7.2645 | 1.0083 | -0.3287 … 0.4439 |

### A/A и неизменённый mimalloc

- A/A workload_ns: paired delta -435115.0000 ns, 95% CI -2686626.1013 … 1816396.1013 ns.
- A/A process_ns: paired delta 173445.0000 ns, 95% CI -2839552.8096 … 3186442.8096 ns.
- mimalloc 16: mean B/A=1.0874; paired 95% CI -295014.0693 … 1451744.0693 ns/workload.
- mimalloc 64: mean B/A=0.9891; paired 95% CI -680047.2399 … 527797.2399 ns/workload.
- mimalloc rt: mean B/A=0.9891; paired 95% CI -1104412.7047 … 867472.7047 ns/workload.
- mimalloc bulk64: mean B/A=0.9836; paired 95% CI -17791.6764 … 11646.6764 ns/workload.
- mimalloc bulk16k: mean B/A=1.0174; paired 95% CI -49818.9271 … 78408.9271 ns/workload.

Если A/A или mimalloc control показывает направленный сдвиг, native attribution ограничена; красивое отношение само по себе не доказывает causal speedup. Полная IAI/canonical/Larson/mstress/MT/retention приёмка не заявляется; её выполнение и отсутствие execution/CI receipts отмечаются отдельно, ранний GO не является shipping acceptance.

## Безопасность и состояние проверки

Guard-before-mutation сохранён. One-element entry получает прежний evicted block и тот же class; null/missing-root paths совпадают с generic slice длины1. Stored root разрешается через ту же owner table, затем вызывается неизменённый flush_run; guard logic не копируется. Eviction возвращает в substrate старый magazine block, не текущий Box-frame block. Порядок flush/compact/state/mask/counters сохранён. Новый crate-private unsafe entry с #Safety контрактом находится в существующем magazine seam, caller unsafe block прежний. Новых public hooks или safe raw-pointer metadata API нет.

Новая tests/own_free_single_flush.rs проверяет full16 и smaller byte cap, exact residency/state/live credits и последующий reuse. Она прошла Clippy/compile-check вместе с production internals bench-internals; negative core-only internals check прошёл. Test execution/Miri/Loom/CI не получены, локально не разрешены. Поэтому никаких acceptance/merge claims; commit/push/merge не выполнялись.

Raw artifacts и reproducible scripts: own_free_single/. Проверка отчёта: node scripts/own_free_single_report.mjs --check. Source paths в опубликованных profiles нормализованы, числовые строки сохранены. Free baseline reference переиспользована из предыдущего раунда без изменения original identity; source/toolchain/harness фиксированы. Baseline native/overflow arms свежие. Новые snapshots каждого этапа имеют свои identities.
