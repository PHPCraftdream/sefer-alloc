# Own-thread free half-flush split — runtime-кандидат и измеренный эффект

Дата: 2026-10-24. **NO-GO: FREE MICRO-GATE**. Это эксперимент, не принятый runtime change.

## Что проверялось

База f80c681b799ef4ef7fa08d79c06896fdf1104fe9. Production diff — один файл dealloc_own_base.rs: только full-magazine half-flush/compaction вынесен в private cold/inline-never helper. Cap-eviction, normal push и вся guard-цепь остаются byte-identical inline; policy/counters/orderings/public API не менялись. D1′ и scalar issue не включены. Пререгистрация: docs/design/2026-10-24-own-free-half-flush-split.md. Все source identities записаны до соответствующих сборок; hashes и точные patches находятся в own_free_split/.

## Детерминированный free: относительно mimalloc до/после

Production-only GlobalAlloc facade, WSL Linux, не installed allocator и не native wall clock. Один бинарник каждого source arm содержит обоих allocators. N/2N=50000/100000, два независимых прохода. Ir/free — marginal wrapper-inclusive instruction count, не ns и не чистая стоимость тела allocator. Контроль mimalloc и повторяемость сверены с raw; backend- и source-различия не скрыты.

| Mode | Sefer A Ir/free | Sefer B | mimalloc A | mimalloc B | mi/A | mi/B | B/A |
|---|---:|---:|---:|---:|---:|---:|---:|
| rt | 87.3590 | 89.3932 | 31.0021 | 31.0021 | 0.3549 | 0.3468 | 1.0233 |
| 16 | 52.0000 | 43.0000 | 28.0000 | 28.0000 | 0.5385 | 0.6512 | 0.8269 |
| 64 | 52.0000 | 43.0000 | 28.0000 | 28.0000 | 0.5385 | 0.6512 | 0.8269 |

## Проверка цены overflow

Bulk round: alloc64, затем free64. Sizes64B/16KiB; 100→200 rounds, normalizer6400 frees; два прохода каждого source/allocator arm. Это намеренно overflow-heavy control, а не такой же regime, как const churn. Callgraph activation oracle: flush call counts сверены с ожидаемыми 6/60 на round; half-flush helper активен только на bulk64 и отсутствует на bulk16k. Raw callgraph oracle — activation.json.

| Mode | Sefer A Ir/free | Sefer B | mimalloc A | mimalloc B | mi/A | mi/B | B/A |
|---|---:|---:|---:|---:|---:|---:|---:|
| bulk64 | 124.8125 | 127.0000 | 31.8906 | 31.8906 | 0.2555 | 0.2511 | 1.0175 |
| bulk16k | 235.6250 | 141.9844 | 73.1875 | 73.1875 | 0.3106 | 0.5155 | 0.6026 |

## Неожиданный single-block flush codegen: проверка без toggle

Cap-eviction одинаков на уровне исходника, но вызываемый flush скомпилирован иначе. Поэтому source-equivalence не означает одинаковый машинный код. Для исключения toggle-артефакта выполнены четыре дополнительных full-program Callgrind процесса (те же control binaries, 100/200 rounds, без collect/toggle фильтра). Binary hashes совпали с control snapshots. Это диагностическая проверка, не новая promotion-матрица; mimalloc в этом дополнительном sanity не измерялся.

| Phase | Whole-program Ir100 | Ir200 | Marginal Ir/pair | Active flush bytes |
|---|---:|---:|---:|---:|
| before | 3332566 | 6028766 | 421.2813 | 1205 |
| after | 2733098 | 4829998 | 327.6406 | 537 |

Whole-program delta -93.6406 Ir/pair совпал с collected-free delta -93.6406 Ir/free. Снижение не создано toggle-фильтром. Caller disassembly связывает изменившийся callee с фактическим cap flush call site; второй одноимённый generic экземпляр нельзя принять за активный. В A — slice/length, общий batch-loop и recycled-array bookkeeping; в B — прямой block argument и single-element specialization. Raw asm/symbols и codegen_summary.json сохранены. Это WSL codegen-наблюдение, не обещание такого же native wall-clock эффекта.

Отдельная гипотеза для будущей работы: изолировать эту одноэлементную специализацию, не меняя frame обычного runtime free и не ослабляя canonical-root/state/span/credit guards. В текущем кандидате она связана с placement/inlining half-flush; самостоятельный source-level вариант ещё не реализован и не измерен. Текущий NO-GO остаётся в силе.

## Native Windows: явно относительно mimalloc до/после

CPU 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz; win32/10.0.19045/x64; features production. 20 fresh-process balanced blocks/mode: A/miA/B/miB/miB/B/miA/A. На block усредняются два одноимённых процесса, далее арифметическое среднее по20 blocks. Win=mean(mimalloc)/mean(Sefer); >1 — Sefer быстрее, <1 — медленнее. B/A<1 — сокращение времени Sefer. CI — Student-t paired mean B−A, df19. Это composite workload с setup/churn/teardown, НЕ pure free latency. Последовательные bulk allocations намеренно активируют другие paths, чем churn.

### Timed workload (ns/pair composite)

| Mode | Sefer A | Sefer B | mimalloc A | mimalloc B | Win A | Win B | B/A | Paired delta 95% CI ns/pair | Result |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| 16 | 7.1187 | 6.6864 | 7.2080 | 7.3684 | 1.0125 | 1.1020 | 0.9393 | -0.9209 … 0.0564 | INCONCLUSIVE |
| 64 | 7.0270 | 7.0359 | 7.8220 | 7.5918 | 1.1131 | 1.0790 | 1.0013 | -0.6400 … 0.6579 | INCONCLUSIVE |
| rt | 15.3756 | 14.9610 | 11.3581 | 11.5859 | 0.7387 | 0.7744 | 0.9730 | -1.4026 … 0.5734 | INCONCLUSIVE |
| bulk64 | 21.8280 | 21.5810 | 13.1734 | 13.2553 | 0.6035 | 0.6142 | 0.9887 | -2.9371 … 2.4430 | INCONCLUSIVE |
| bulk16k | 41.4742 | 39.4664 | 61.0565 | 61.5950 | 1.4722 | 1.5607 | 0.9516 | -5.9738 … 1.9583 | INCONCLUSIVE |

### Full process (ms/process)

| Mode | A | B | mi A | mi B | B/A | Paired delta 95% CI ms |
|---|---:|---:|---:|---:|---:|---|
| 16 | 15.6955 | 15.0362 | 15.3323 | 15.5534 | 0.9580 | -1.3960 … 0.0774 |
| 64 | 15.5301 | 15.2609 | 16.0227 | 15.5342 | 0.9827 | -1.1069 … 0.5686 |
| rt | 24.0758 | 23.6792 | 20.2642 | 20.0424 | 0.9835 | -1.7545 … 0.9612 |
| bulk64 | 7.8486 | 7.7105 | 7.4125 | 7.1949 | 0.9824 | -0.7779 … 0.5018 |
| bulk16k | 8.1872 | 8.1767 | 8.5764 | 8.5404 | 0.9987 | -0.4494 … 0.4282 |

### A/A и неизменённый mimalloc

- A/A workload_ns: paired delta 125770.0000 ns, 95% CI -1676540.9734 … 1928080.9734 ns.
- A/A process_ns: paired delta 74580.0000 ns, 95% CI -1858151.0026 … 2007311.0026 ns.
- mimalloc 16: mean B/A=1.0223; paired 95% CI -535843.9102 … 856838.9102 ns/workload.
- mimalloc 64: mean B/A=0.9706; paired 95% CI -948969.9951 … 488639.9951 ns/workload.
- mimalloc rt: mean B/A=1.0201; paired 95% CI -537253.5563 … 993223.5563 ns/workload.
- mimalloc bulk64: mean B/A=1.0062; paired 95% CI -23085.7795 … 25770.7795 ns/workload.
- mimalloc bulk16k: mean B/A=1.0088; paired 95% CI -99999.9875 … 117644.9875 ns/workload.

Если A/A или mimalloc control показывает направленный сдвиг, native attribution ограничена; красивое отношение само по себе не доказывает causal speedup. Полная IAI/canonical/Larson/mstress/MT/retention приёмка не заявляется; при провале любого раннего gate её не запускали ради спасения кандидата.

## Безопасность и состояние проверки

Guard-before-mutation сохранён. Helper получает table-derived OwnedBlock и готовый guard offset после успешного small_free_guard и full-cap split; count/cap/offset повторно не разрешаются. Cap-eviction оставлен inline byte-for-byte. Eviction возвращает в substrate старый magazine block, не текущий Box-frame block. Порядок flush/compact/state/mask/counters сохранён. Существующие unsafe flush sites перемещены в тот же seam; новых public hooks или safe raw-pointer metadata API нет.

Новая tests/own_free_half_flush_split.rs проверяет full16 и smaller byte cap, exact residency/state/live credits и последующий reuse. Она прошла Clippy/compile-check вместе с production internals bench-internals; negative core-only internals check прошёл. Test execution/Miri/Loom/CI не получены, локально не разрешены. Поэтому никаких acceptance/merge claims; commit/push/merge не выполнялись.

Raw artifacts и reproducible scripts: own_free_split/. Проверка отчёта: node scripts/own_free_split_report.mjs --check. Source paths в опубликованных profiles нормализованы, числовые строки сохранены. Attribution/codegen относятся к сохранённым измеренным binaries и заранее зафиксированным source snapshots; новые benchmark declaration/harness snapshots каждого этапа имеют свои identities. Все native CI номинальные per-row, без family-wise correction.
