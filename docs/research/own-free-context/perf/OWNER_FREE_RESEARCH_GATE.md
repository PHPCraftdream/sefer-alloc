# Owner-free research: измерения трёх runtime-кандидатов

**Вердикт: NO-GO FOR PROMOTION; FREE-INSTRUCTION GAIN; NATIVE SPEEDUP NOT ESTABLISHED.** Ни одна экспериментальная композиция не влита в принятую базу. Нет права на shipping/merge по этому отчёту.

## Механизм и границы

База всех вариантов: f80c681b799ef4ef7fa08d79c06896fdf1104fe9. A (`scalar`) — отдельный one-element magazine flush с прежним flush_run. B (`context`) — A плюс передача проверенного metadata byte-slot/count в overflow helper вместо удержания root/offset в горячем frame. C (`mir`) — B с inline(always) scalar-entry; tradeoff не доминирует B. Для широкой проверки B выбран до broad/native данных. Это не сумма исторических выигрышей; каждый кандидат измерен целиком против f80. D1′, scalar-issue и два отвергнутых cold split не входят в композицию. Production feature composition, политика magazine и producer protocol не менялись.

Guard-before-mutation, current LIVE credit, canonical stored-root provenance и retirement сохраняются. Slot не покидает синхронный helper; старые resident blocks flush, текущий dealloc/Box payload не трогается. Новые unsafe sites: scalar flush entry в существующем seam; private dealloc_with_state_slot и вызов после guard. Slot getter сам память не разыменовывает. Это code-review reasoning, не Miri/CI receipt.

## Предыдущие гипотезы этой исследовательской цепочки

| Попытка | Результат | Урок |
|---|---|---|
| Scalar prepared issue | INCONCLUSIVE / DO NOT PROMOTE | Повторяемая source-работа найдена, production-выигрыш не доказан. |
| Общий cold overflow helper | NO-GO: OVERFLOW CONTROL | Hot frame дешевле, но cap-eviction — частый путь своего режима, его цена выросла. |
| Только full-magazine half-flush split | NO-GO: FREE MICRO-GATE | Scalar/free режим подорожал; source-byte equivalence cap-ветки не фиксирует callee codegen. |
Полные прежние отчёты и собственные provenance/raw/CSV сохраняются в соседних worktrees: scalar-issue-prepared/docs/perf/SCALAR_ISSUE_PREPARED_GATE.md, own-free-cost/docs/perf/OWN_FREE_OVERFLOW_OUTLINE_GATE.md и own-free-split/docs/perf/OWN_FREE_HALF_FLUSH_SPLIT_GATE.md. Новые A/B/C не суммируются с этими отвергнутыми кандидатами. D1′ — отдельный предшествующий NO-GO (PR #96), не база этой цепочки.

## Детерминированный frontier, Ir/free

GlobalAlloc facade, production-only, WSL Linux; wrapper-inclusive marginal N/2N, два прохода. Не ns/free и не installed global allocator. Overflow: alloc64 затем free64, 100/200 rounds. Числа каждого варианта и controls сохранены в owner_research/{scalar,context,mir}; shared baseline хранится один раз.

| Mode | f80 | A scalar | B context | C MIR | mimalloc | mi/f80 | mi/B | B/f80 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| rt | 87.3590 | 86.7255 | 80.0337 | 80.0850 | 31.0021 | 0.3549 | 0.3874 | 0.9161 |
| 16 | 52.0000 | 52.0000 | 43.0000 | 43.0000 | 28.0000 | 0.5385 | 0.6512 | 0.8269 |
| 64 | 52.0000 | 52.0000 | 43.0000 | 43.0000 | 28.0000 | 0.5385 | 0.6512 | 0.8269 |
| bulk64 | 124.8125 | 121.3438 | 116.0313 | 116.3125 | 31.8906 | 0.2555 | 0.2748 | 0.9296 |
| bulk16k | 235.6250 | 188.7188 | 194.8750 | 191.1250 | 73.1875 | 0.3106 | 0.3756 | 0.8271 |

B не доминирует A на bulk16k; выбран за hot/const/bulk64. Удалён generic run-discovery/bookkeeping при single-element eviction, а validated slot уменьшает live context обычного free. Это измеренное сокращение инструкций, не доказанный native прорыв. Activation oracle — callgraph generic/scalar/helper в сохранённых control profiles; 6 full flush events/round на bulk64, 60 cap-evictions/round на bulk16k, не одна универсальная «холодная» частота.

## Широкая IAI-матрица B

106 cells: 46 ниже, 50 равны, 10 выше. Все строки: own_free_context/wide/summary.csv. Exact built ELF запущен напрямую; before/after SHA проверены. Первая попытка через cargo пересобрала binary и отвергнута, её log сохранён отдельно; это не числовое evidence. Версия toolchain/runner не менялась ради результата.

| Raw-positive cell | A Ir | B Ir | B−A Ir |
|---|---:|---:|---:|
| alloc_clear_magazine_only_16b | 48937 | 48961 | 24 |
| alloc_magazine_hit_only_16b | 52010 | 52022 | 12 |
| alloc_magazine_prefill_only_16b | 51732 | 51748 | 16 |
| realloc_grow | 569496 | 569529 | 33 |
| realloc_large_equal_8mib | 16826080 | 16826109 | 29 |
| realloc_large_equal_8mib_prefix | 8437229 | 8437253 | 24 |
| realloc_large_grow_6_to_8mib | 15423390 | 15423417 | 27 |
| realloc_large_shrink_8_to_3mib | 11931669 | 11931696 | 27 |
| realloc_large_shrink_8_to_4p5mib | 13677549 | 13677552 | 3 |
| realloc_large_shrink_8_to_6mib | 15423381 | 15423408 | 27 |

Clear paired increment 107→155 (+48) НЕ доказывает +3 Ir/clear: все 16 развёрнутых clear-блоков имеют одинаковую нормализованную assembly, 128 instructions плюс один shared address lea в обоих binaries. Разница находится вне этих блоков; source-prefix не является machine-prefix. Assembly и проверяющий скрипт сохранены в owner_research/sanity.
- Actual production alloc rt: raw totals 3056628/5832332 в обоих binaries; marginal 55.5141 Ir/alloc в обоих.
- Actual production alloc 16: raw totals 1148611/2248611 в обоих binaries; marginal 22.0000 Ir/alloc в обоих.
- Actual production alloc 64: raw totals 1148611/2248611 в обоих binaries; marginal 22.0000 Ir/alloc в обоих.

Остальные raw-positive cells не объявлены автоматически fixed/bootstrap cost: полной per-component атрибуции realloc здесь нет. Строгая broad zero-regression приёмка не заявлена. Отдельные actual hit increments и raw totals не смешиваются.

## Native: относительно mimalloc до/после

CPU 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz; win32/10.0.19045/x64. Features production; native toolchain в comparison_identity.json (отличается от WSL). 20 balanced blocks/mode: A/miA/B/miB/miB/B/miA/A, fresh process для каждого sample. Среднее двух процессов/arm внутри block, затем арифметическое среднее 20 blocks. Win=mean(mi)/mean(Sefer); >1 — Sefer быстрее. CI — nominal Student-t paired B−A, df19, без multiple-comparison correction и без удаления samples. Setup/churn/teardown входят в workload_ns; process_ns включает также startup/output/exit.

| Mode | A ns/pair | B ns/pair | mi A | mi B | Win A | Win B | B/A | Paired 95% CI ns/pair | Result |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| 16 | 6.2603 | 6.0652 | 6.8849 | 6.8162 | 1.0998 | 1.1238 | 0.9688 | -1.2643 … 0.8742 | INCONCLUSIVE |
| 64 | 5.9529 | 5.8146 | 7.2382 | 7.3102 | 1.2159 | 1.2572 | 0.9768 | -0.9195 … 0.6428 | INCONCLUSIVE |
| rt | 13.3145 | 13.4098 | 10.4806 | 10.2106 | 0.7872 | 0.7614 | 1.0072 | -1.4853 … 1.6758 | INCONCLUSIVE |
| bulk64 | 17.4303 | 16.8138 | 10.5612 | 10.6293 | 0.6059 | 0.6322 | 0.9646 | -2.4657 … 1.2328 | INCONCLUSIVE |
| bulk16k | 31.9711 | 31.8759 | 48.0327 | 48.8820 | 1.5024 | 1.5335 | 0.9970 | -3.1393 … 2.9489 | INCONCLUSIVE |

### Full process, ms/process

| Mode | A | B | mi A | mi B | Paired 95% CI ms |
|---|---:|---:|---:|---:|---|
| 16 | 13.9351 | 14.2398 | 14.2556 | 13.9647 | -1.3422 … 1.9516 |
| 64 | 13.4550 | 13.2393 | 14.6680 | 14.8628 | -1.2538 … 0.8225 |
| rt | 21.1858 | 20.8360 | 18.8524 | 18.0912 | -2.3889 … 1.6892 |
| bulk64 | 6.4240 | 6.3934 | 6.2275 | 6.1938 | -0.5647 … 0.5035 |
| bulk16k | 6.6404 | 6.5830 | 6.9563 | 7.1356 | -0.3095 … 0.1946 |

- A/A workload_ns: inner−outer mean -19885.0000 ns; nominal 95% CI df9 -2014596.7211 … 1974826.7211 ns.
- A/A process_ns: inner−outer mean 108660.0000 ns; nominal 95% CI df9 -2269033.6393 … 2486353.6393 ns.
- mimalloc 16: B/A 0.9900; paired 95% CI B−A -979184.9849 … 841794.9849 ns/workload.
- mimalloc 64: B/A 1.0099; paired 95% CI B−A -934917.3148 … 1078847.3148 ns/workload.
- mimalloc rt: B/A 0.9742; paired 95% CI B−A -1431982.2172 … 891552.2172 ns/workload.
- mimalloc bulk64: B/A 1.0064; paired 95% CI B−A -17173.1353 … 19403.1353 ns/workload.
- mimalloc bulk16k: B/A 1.0177; paired 95% CI B−A -72082.0180 … 99912.0180 ns/workload.

## Canonical bench:table ABBA

Четыре sweep: A/B/B/A, два process-level sweep на arm. Каждая ячейка — среднее arithmetic per-iteration mean из Criterion sample.json двух sweep, не pooled independent samples и не статистическая significance по двум sweep. 1024 pairs/iteration; manual_realloc_sim — ns/полная growth chain. Сохраняются все Criterion samples/estimates и полный bench:table log, включая System. Это прямой GlobalAlloc facade, НЕ installed global allocator.

| Cell | Unit | Sefer A | Sefer B | mi A | mi B | Win A | Win B | B/A |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| global_alloc/16B | ns/pair | 18.9245 | 19.9572 | 6.4706 | 5.7488 | 0.3419 | 0.2881 | 1.0546 |
| global_alloc/64B | ns/pair | 22.0119 | 19.8609 | 8.2476 | 8.3647 | 0.3747 | 0.4212 | 0.9023 |
| global_alloc/256B | ns/pair | 20.2612 | 20.1068 | 10.0523 | 9.1672 | 0.4961 | 0.4559 | 0.9924 |
| global_alloc/1024B | ns/pair | 23.6145 | 21.4294 | 21.1603 | 20.5949 | 0.8961 | 0.9611 | 0.9075 |
| global_alloc_churn/16B | ns/pair | 10.1192 | 12.0207 | 8.2509 | 8.1573 | 0.8154 | 0.6786 | 1.1879 |
| global_alloc_churn/64B | ns/pair | 9.5110 | 9.4543 | 10.6277 | 10.5688 | 1.1174 | 1.1179 | 0.9940 |
| global_alloc_churn/256B | ns/pair | 10.6674 | 12.2037 | 14.8834 | 14.5446 | 1.3952 | 1.1918 | 1.1440 |
| global_alloc_churn/1024B | ns/pair | 10.6157 | 11.5863 | 88.7960 | 81.4209 | 8.3646 | 7.0274 | 1.0914 |
| global_alloc_churn_write/16B | ns/pair | 10.7538 | 10.1111 | 9.1289 | 8.5340 | 0.8489 | 0.8440 | 0.9402 |
| global_alloc_churn_write/64B | ns/pair | 11.5102 | 10.4442 | 10.1445 | 11.0225 | 0.8813 | 1.0554 | 0.9074 |
| global_alloc_churn_write/256B | ns/pair | 13.0369 | 14.6328 | 16.3364 | 15.4955 | 1.2531 | 1.0590 | 1.1224 |
| global_alloc_churn_write/1024B | ns/pair | 10.8647 | 13.7470 | 86.3541 | 93.8777 | 7.9481 | 6.8289 | 1.2653 |
| global_alloc_churn_with_teardown/16B | ns/pair composite | 12.1353 | 11.7192 | 9.1848 | 8.9298 | 0.7569 | 0.7620 | 0.9657 |
| global_alloc_churn_with_teardown/64B | ns/pair composite | 12.8807 | 13.8451 | 9.0119 | 8.8002 | 0.6996 | 0.6356 | 1.0749 |
| global_alloc_churn_with_teardown/256B | ns/pair composite | 15.8187 | 16.0070 | 12.2671 | 11.2742 | 0.7755 | 0.7043 | 1.0119 |
| global_alloc_churn_with_teardown/1024B | ns/pair composite | 19.2257 | 24.6020 | 21.1075 | 20.0629 | 1.0979 | 0.8155 | 1.2796 |
| manual_realloc_sim | ns/chain | 486.0245 | 569.0949 | 455.1717 | 439.5197 | 0.9365 | 0.7723 | 1.1709 |

Canonical содержит сигналы ухудшения: меньший Ir/free НЕ закрывает эти ячейки. Все sweep, включая System и остальные benchmark groups, сохранены в owner_research/canonical_sweeps.csv; raw samples не отфильтрованы. Ни общий geometric mean, ни случайно выгодные ячейки не заменяют проверки ухудшений.

### Отдельная проверка churn16B

После обнаружения canonical-сигнала выполнены 10 коротких ABBA blocks точных уже построенных canonical binaries, без Cargo и без изменения source. Каждый из 40 процессов измеряет SeferAlloc и mimalloc в одном и том же bench filter; два процесса каждого source arm на block. Это те же production internals bench-internals и canonical harness, а не прежний production-only micro probe. Mean — arithmetic Criterion per-iteration time/1024; paired nominal 95% CI df9 по block means.

| Arm | A ns/pair | B ns/pair | B/A | Paired 95% CI ns/pair | Result |
|---|---:|---:|---:|---|---|
| SeferAlloc | 10.2874 | 10.9008 | 1.0596 | -0.3765 … 1.6032 | INCONCLUSIVE |
| mimalloc | 8.9604 | 8.8394 | 0.9865 | -0.5148 … 0.2727 | INCONCLUSIVE |

Focused win mimalloc/Sefer: 0.8710 → 0.8109. Это отдельный follow-up, не подмена первоначального ABBA: оба набора сохранены. Другие canonical ухудшения этим одним filter не закрываются.

## Вывод и остатки

Удаление реально исполняемой работы найдено, но «прорыв относительно mimalloc» не доказан. Уменьшение free Ir не гарантирует ускорение полного workload: alloc, адресация, TLS/route, frontend/backend и шум остаются. Случайные attribute permutations и новые caches без trace не продолжаются. Следующий обоснованный масштаб — representative application trace и activation на нужном layer; U31/epoch/protocol redesign требует отдельного доказательства, а не экстраполяции этих чисел.

Test execution, Miri/Loom, CI, MT/Larson/mstress и retention acceptance для B не получены. Написанные regression tests и compile/clippy receipts не заменяют execution. Никаких commit/push/merge, dependency/version bumps или изменений README baseline по эксперименту. Perf item 117 закрыт как NO-GO FOR PROMOTION; история и reopen trigger — OPEN_ITEMS_ARCHIVE.md § L117.

Provenance: owner_research/{scalar,context,mir}/*identity.json и patches; own_free_context/{comparison_identity.json,native_binaries.json,wide/identity.json,wide/attempt2-identity.json,canonical/identity.json}. Source identity зафиксирована до measurement; binary drift guards сохранены. Reproduction scripts — owner_research/reproduction/. Проверка артефакта: node docs/perf/owner_research/generate.mjs --check.
