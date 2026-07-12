# Повторное read-only ревью производительности и регрессий

Дата: 2026-07-12. Область: текущие Rust-исходники, `Cargo.toml` и предыдущие отчёты, прежде всего `docs/agent_reports_round2/performance_opportunities.md`. Ревью статическое: Git, сборка, тесты, бенчмарки, профилировщики, fuzzing и скрипты не запускались. Исходники и существующие файлы не изменялись.

## Итог

После последних исправлений видны реальные улучшения горячих и около-горячих путей: refill пишет прямо в magazine, пустой remote-ring drain теперь отсекается одним чтением, поля с удалённым доступом отделены от owner-hot данных, fresh bitmap больше не обнуляется повторно, а aligned size-class lookup перескакивает заведомо неподходящие классы. Исправлен и возврат cursor из `HeapOverflow::drain`, из-за которого освобождения могли задерживаться.

Однако три главные возможности радикального ускорения из round 2 остаются:

1. small-allocation miss всё ещё сканирует весь high-water диапазон `SegmentTable` — `O(S)`;
2. saturated cross-thread free всё ещё способен выполнить до 262 144 полных попыток push с глобальными atomic RMW;
3. magazine hit по-прежнему делает запись в удалённый 32-КиБ `MagazineBitmap` сегмента.

Новая существенная регрессия/несогласованность после оптимизации diagnostics: `production` не включает `alloc-stats`, поэтому hit increments скомпилированы из горячего пути, но `SeferAlloc::stats()` всё равно дважды линейно обходит все когда-либо инициализированные heap slots ради заведомо нулевых `tcache_hits` и `large_cache_hits`. Документация при этом обещает отсутствие heap walk.

Оценки эффекта ниже качественные и должны считаться гипотезами до измерений.

## Приоритеты

| Приоритет | Пункт | Статус относительно round 2 |
|---|---|---|
| P0 | Индекс доступных small-сегментов вместо полного scan | осталось без архитектурного исправления |
| P0 | Ограничить retry storm и убрать единый MPSC hotspot | частично исправлен только dead-owner случай |
| P0/P1 | Убрать per-hit запись в `MagazineBitmap` | осталось; прежний GO был узким instruction-count решением |
| P1 | Не сканировать registry в `stats()` при выключенных counters | новая найденная регрессия/несогласованность |
| P1 | Компактный/lazy `HeapOverflow` | осталось |
| P1 research | Перестроить `LockFreeRegion`, `EpochRegion`, instance binding `ShardedRegion` | осталось, но tier deprecated |
| P2 | Строго O(1) aligned classification | частично улучшено |
| P2 | Реально локальный layout `Tcache::PerClass` | новая оставшаяся возможность |
| P2 | Sharded production-safe замена coarse `SyncRegion` | осталось |
| P3 | Проверить low-contention цену разнесения cursor cache lines | потенциальная регрессия, нужна A/B проверка |

## Что улучшено

### I1. Zero-copy refill и batched freelist drain

- **Файлы/строки:** `src/registry/heap_core.rs:2027-2052`; `src/alloc_core/alloc_core_small.rs:721-805`, `src/alloc_core/alloc_core_small.rs:1821-2030`.
- **Механизм:** refill пишет непосредственно в `tcache.classes[c].slots`; freelist head и live-count обновляются пакетно, а fresh carve заполняет остаток batch без промежуточного `BinTable` round-trip.
- **Ожидаемый эффект:** меньше копирований, вызовов и pointer chasing на magazine miss; выигрыш ограничен miss/refill-путём и не меняет magazine hit.
- **Риски:** сложнее инвариант refill-window; `issued_so_far.contains` остаётся bounded линейной проверкой при редком сочетании непустого ring и уже частично заполненного output.
- **Как проверить:** отдельно измерить miss/refill instructions, loads и allocations/op при refill depth 1/4/8/16; property/loom-проверка уникальности выдаваемых блоков. Не выполнялось.

### I2. Cheap guard пустого remote drain

- **Файлы/строки:** `src/alloc_core/alloc_core_small.rs:1554-1576`, `src/alloc_core/alloc_core_small.rs:1622-1630`; `src/alloc_core/remote_free_ring.rs:667-724`, `src/alloc_core/remote_free_ring.rs:738-785`; `src/alloc_core/segment_header.rs:359-396`.
- **Механизм:** owner хранит последний фактический `head` в header сегмента и сравнивает его с одним Relaxed-чтением `tail`; пустой ring больше не выполняет полный drain и Release-store `head` на каждом scan.
- **Ожидаемый эффект:** заметно меньше atomic traffic при refill scan множества сегментов без remote frees.
- **Риски:** cache обязан обновляться фактическим `head`, а не snapshot `tail`; текущий код это соблюдает. Дополнительное поле находится в hot cache line header.
- **Как проверить:** workload с `S=1..1024`, пустыми ring и частыми refill misses; считать atomic stores, cache-to-cache и LLC misses; loom для reserved-but-not-published slot. Не выполнялось.

### I3. Layout и false sharing частично улучшены

- **Файлы/строки:** `src/registry/heap_slot.rs:93-130`, `src/registry/heap_slot.rs:217-233`, `src/registry/heap_slot.rs:319-350`; `src/alloc_core/segment_header.rs:234-300`; `src/alloc_core/remote_free_ring.rs:58-66`, `src/alloc_core/remote_free_ring.rs:404-451`.
- **Механизм:** remote-access fields `HeapSlot` отделены 64-byte alignment, stride slot стал кратен cache line; hot-поля `SegmentHeader` собраны в первые 41 байт; consumer `head`, producer `tail` и ring data разнесены.
- **Ожидаемый эффект:** меньше ложного sharing между owner, remote producers и соседними slots; лучше locality header fast paths.
- **Риски:** увеличение metadata и рабочей выборки в low-contention сценарии; подробная потенциальная регрессия — пункт R3.
- **Как проверить:** cache-to-cache transfers и L1/LLC misses при 1 и 2/8/32 producers, отдельно owner-active/paused; `-Zprint-type-sizes` для supported targets. Не выполнялось.

### I4. Убрано лишнее обнуление fresh bitmap

- **Файлы/строки:** `src/alloc_core/bootstrap.rs:74-102`; `src/alloc_core/alloc_core_small.rs:2439-2484`; `src/alloc_core/alloc_bitmap.rs:75-95`.
- **Механизм:** native fresh OS reservation уже demand-zero, поэтому два 32-КиБ bitmap не записываются повторно; miri-path сохраняет явную инициализацию.
- **Ожидаемый эффект:** на cold segment reserve не fault-in/dirty до 16 дополнительных metadata pages; ниже startup/RSS и reserve latency.
- **Риски:** корректно только для действительно fresh OS memory; recycled/decommitted paths обязаны сохранять explicit reset.
- **Как проверить:** first-segment и repeated-reserve minor faults/RSS, отдельные recycle/decommit correctness cases на всех OS. Не выполнялось.

### I5. Aligned classification стала короче, но не O(1)

- **Файлы/строки:** `src/alloc_core/size_classes.rs:162-202`; вызовы `src/registry/heap_core.rs:832-858`, `src/registry/heap_core.rs:1146-1159`, `src/registry/heap_core.rs:1778-1785`.
- **Механизм:** вместо шага по каждому классу выполняется переход к классу, покрывающему следующую кратную `align` величину.
- **Ожидаемый эффект:** меньше modulo/table iterations для align 32..16384; обычный `align <= 16` fast path не изменён.
- **Риски:** цикл и variable divisibility test остались; точность критична для alignment correctness.
- **Как проверить:** исчерпывающее сравнение с reference walk по всем sizes/alignments и instruction-count distribution по aligned workload. Не выполнялось.

### I6. Исправлен фактический cursor `HeapOverflow::drain`

- **Файлы/строки:** `src/registry/heap_overflow.rs:350-399`; потребитель `src/registry/heap_core.rs:770-797`.
- **Механизм:** drain возвращает опубликованный фактический `head`, включая остановку на reserved-but-unpublished entry; cache больше не считает такой tail уже обработанным.
- **Ожидаемый эффект:** устраняется задержка reclaim до следующего push и последующее ложное пропускание drain.
- **Риски:** изменения throughput минимальны; критичен wraparound и ordering publish pair.
- **Как проверить:** loom interleaving reserve/pause/drain/publish и p99 reclaim delay. Не выполнялось.

## Оставшиеся возможности и регрессии

### P0-1. Small miss остаётся `O(S)` по high-water `SegmentTable`

- **Файлы/строки:** `src/alloc_core/alloc_core_small.rs:1476-1527`, `src/alloc_core/alloc_core_small.rs:1540-1635`, `src/alloc_core/alloc_core_small.rs:1636-1673`; структура `src/alloc_core/segment_table.rs:129-176`.
- **Статус:** подтверждено повторно, round2 #1 не закрыт. Убрана прежняя stack-copy bases, но асимптотика scan не изменилась.
- **Механизм проблемы:** каждый miss идёт по `0..table.count()`, читает slot, kind и для каждого small segment — remote tail/cache и `BinTable[class]`. Null holes после recycle также читаются. В NUMA build возможен проход до конца даже после нахождения foreign-node candidate.
- **Предложение:** owner-private availability index по `(class, node)` и отдельный dirty/pending index для ring-bearing segments. Практичный промежуточный вариант — per-class bitset segment ids: `mark_free`/drain ставит bit, исчерпание freelist очищает; owner выбирает `trailing_zeros`. Remote producer ставит heap-level pending bit/word, чтобы ring discovery не требовал полного scan.
- **Ожидаемый эффект:** miss/refill latency из `O(S)` в `O(1)`/`O(number of nonempty words)`; многократный выигрыш при фрагментации, большом high-water mark и NUMA.
- **Риски:** lost wakeup и stale membership на flush, decommit, pool/unpool, recycle, adoption; дублированная выдача при неверном transition. Metadata растёт на число классов/NUMA nodes.
- **Как проверить:** `S={1,16,128,1024}`, dense и 90% recycled holes, fixed miss rate, remote-free и NUMA variants; cycles, segment headers read, LLC/TLB misses, p50/p99. Loom/property для empty↔nonempty и pending clear/set. Не выполнялось.

### P0-2. Live-owner retry storm и единый producer cursor остаются

- **Файлы/строки:** лимит `src/registry/heap_core.rs:181-218`; retry `src/registry/heap_core.rs:1841-1892`; push `src/alloc_core/remote_free_ring.rs:621-655`; overflow push `src/registry/heap_overflow.rs:280-309`.
- **Статус:** частично улучшено: `owner_slot_is_live` обходит spin при dead/free owner (`src/registry/heap_core.rs:1849-1877`). Для LIVE, но paused/slow owner round2 #2 полностью сохраняется.
- **Механизм проблемы:** один logical free после первого full может выполнить до 262 144 вызовов `ring.push`. Каждый full-attempt читает `tail/head` и делает два `fetch_add` (`src/alloc_core/remote_free_ring.rs:629-635`); при открывшемся slot producers снова конкурируют за один `tail.compare_exchange_weak`. Это создаёт counter storm и мешает owner освободить ring.
- **Предложение:** немедленно/после 8–32 adaptive attempts уходить в heap overflow; retries учитывать изменение `head`, а diagnostics инкрементировать один раз на логическое событие. Архитектурный вариант — 4–16 producer lanes или per-thread batches с одним publish/exchange, owner drain round-robin.
- **Ожидаемый эффект:** до нескольких порядков снижения p99/p999 dealloc при saturated fan-in; меньше locked RMW и cache-line bouncing. В нормальном незаполненном ring почти нейтрально.
- **Риски:** больше metadata; отсутствие глобального FIFO между lanes нужно доказать ненужным; batching усложняет thread teardown и generation protocol; overflow остаётся bounded.
- **Как проверить:** 1/2/4/8/16/32 producers на один segment при active/slow/paused LIVE owner, отдельно prefilled ring; throughput, CAS failures, locked instructions, HITM/cache-to-cache, p99/p999. Loom reserve/publish/wrap/teardown. Не выполнялось.

### P0/P1-3. `MagazineBitmap` всё ещё делает segment write на каждом magazine hit

- **Файлы/строки:** alloc hit `src/registry/heap_core.rs:935-1000`; own free `src/registry/heap_core.rs:1305-1363`; refill marking `src/registry/heap_core.rs:2080-2094`; bitmap `src/alloc_core/magazine_bitmap.rs:69-141`; layout `src/alloc_core/segment_header.rs:975-1004`.
- **Статус:** round2 #3 остаётся. Предыдущий GO подтвердил допустимую instruction-count цену в конкретном churn gate, но не опровергает cache/TLB механизм на many-segment working set.
- **Механизм проблемы:** magazine pop уже знает pointer из hot `Tcache`, но дополнительно вычисляет base/off и делает byte RMW в отдельной 32-КиБ области сегмента. Own free выполняет probe и mark. При множестве активных сегментов magazine перестаёт быть TLS-only fast path.
- **Предложение:** A/B exact membership oracle без per-hit segment write: компактный owner-private set/tag array размером до `SMALL_CLASS_COUNT*TCACHE_CAP`, либо bounded scan для малой depth плюс batch membership check только на ring drain. Сильный вариант — хранить segment id+offset рядом с tcache entry и поддерживать компактный per-heap fingerprint/table.
- **Ожидаемый эффект:** убрать удалённую запись из самого частого small-allocation hit, снизить DTLB/LLC working set и освободить 32 КиБ на small segment; особенно важно для churn по десяткам/сотням segments.
- **Риски:** bitmap закрывает cross-thread duplicate-free окна; probabilistic filter без exact fallback недопустим. Полный 49×16 scan на каждый drained entry может быть хуже текущего решения.
- **Как проверить:** A/B depths 1/3/8/16, active segments 1/16/64/256, sizes 16..4096, local churn и remote-drain-heavy; cycles, stores, L1/LLC/DTLB. Весь набор double-free/refill-window cases + loom/miri после реализации. Не выполнялось.

### P1-4. `stats()` делает два registry scan даже когда counters скомпилированы из production

- **Файлы/строки:** feature contract `Cargo.toml:163-185`; aggregation `src/registry/heap_registry.rs:1015-1041`, `src/registry/heap_registry.rs:1070-1092`; вызовы и обещание API `src/global/sefer_alloc.rs:280-287`, `src/global/sefer_alloc.rs:313-340`.
- **Статус:** новая найденная регрессия/несогласованность. Round2 #7 отмечал линейный scrape; текущая документация теперь прямо утверждает «no segment or heap walk», но реализация вызывает два независимых `0..count` прохода.
- **Механизм проблемы:** `production` не включает `alloc-stats`; hit writes отсутствуют и все slot counters остаются нулевыми, однако cfg aggregator привязан к `fastbin`/`alloc-decommit`, а не к `alloc-stats`. Один `stats()` читает `count`, затем дважды трогает `initialised` и remote counter cache lines до high-water mark, включая recycled heaps.
- **Предложение:** при `not(feature="alloc-stats")` возвращать compile-time zero без `ensure()` и scan. При feature on объединить оба totals в один pass либо вести hierarchical/per-CPU aggregates; для exact snapshot оставить явно slow API.
- **Ожидаемый эффект:** `stats()` в обычной production-конфигурации становится действительно набором нескольких atomic loads; при 4096 slots устраняется до 8192 gate loads и 8192 counter loads на scrape. Снижает cache pollution при частом metrics polling.
- **Риски:** после включения feature семантика totals должна остаться монотонной; global aggregate write может вернуть contention, поэтому обновлять его на каждом hit нельзя.
- **Как проверить:** stats latency при high-water 1/64/1024/4096 и 1 Hz–10 kHz scrape параллельно allocator load; проверить нулевые поля feature-off и totals feature-on. Не выполнялось.

### P1-5. Inline `HeapOverflow` сохраняет 96 МиБ virtual metadata и SoA drain

- **Файлы/строки:** capacity/layout `src/registry/heap_overflow.rs:110-175`, `src/registry/heap_overflow.rs:184-215`; push/drain `src/registry/heap_overflow.rs:280-399`; embedding `src/registry/heap_slot.rs:334-350`; registry array `src/registry/bootstrap.rs:281-301`.
- **Статус:** round2 #8 остаётся. Miri-specific cap=64 устранил CI RSS-регрессию только под `cfg(miri)`; native cap=2048 × 4096 slots остаётся.
- **Механизм проблемы:** каждый slot содержит 2048 bases и 2048 packed values (24 КиБ), хотя очередь нужна только после saturation segment ring и retry. SoA заставляет drain читать две области на расстоянии примерно 16 КиБ. Lazy demand-zero спасает RSS пустых slots, но не address space/page tables и не working set реально overflowing heaps.
- **Предложение:** компактная 64-bit entry `(segment_id,generation,offset,class)` либо sidecar/slab, зарезервированный напрямую через OS aperture только для heaps, впервые достигших overflow. Альтернатива — резко меньшая inline queue + shared sharded overflow slab.
- **Ожидаемый эффект:** до ~2× меньше traffic на entry и десятки МиБ меньше virtual metadata; лучше sequential drain locality. Почти нулевой throughput effect без overflow.
- **Риски:** segment-id ABA требует generation; lazy OS reservation повышает first-overflow latency и должна быть reentrancy-free; shared slab добавляет contention и full policy.
- **Как проверить:** virtual size, page tables, RSS/minor faults при 1/64/1024/4096 claimed и overflowing heaps; drain bandwidth/LLC; recycle/generation/wrap stress. Не выполнялось.

### P1-6. Research-tier concurrent regions сохраняют копирования и contention

#### LockFreeRegion

- **Файлы/строки:** `src/concurrent/lock_free_region.rs:81-100`, `src/concurrent/lock_free_region.rs:290-305`, `src/concurrent/lock_free_region.rs:326-367`, `src/concurrent/lock_free_region.rs:383-403`.
- **Механизм:** каждая запись клонирует `Snapshot.pages` (`O(P)` Arc bumps), затем до 64 `Slot` touched page, включая `Arc<T>` increments, и публикует новые allocations.
- **Предложение/эффект:** persistent radix page table (`O(log_B P)` copy) либо stable epoch page table с per-slot atomics (`O(1)` mutation); многократный выигрыш write latency больших regions.
- **Риски:** read pointer chasing, reclamation/ABA complexity; API deprecated, поэтому высокий engineering cost оправдан только реальным пользователем.
- **Как проверить:** allocations/refcount RMW/op и p99 write при pages 1/16/1024/65536, read regression и loom/miri. Не выполнялось.

#### EpochRegion

- **Файлы/строки:** `src/concurrent/epoch_region.rs:127-140`, `src/concurrent/epoch_region.rs:208-250`, `src/concurrent/epoch_region.rs:365-390`.
- **Механизм:** remote removers сериализуются на `Mutex<Vec<u32>>`; `mem::take` оставляет queue с capacity 0, следующий burst reallocates; drain повторно Acquire-load generation.
- **Предложение/эффект:** owner scratch Vec со swap-back как минимальная правка; bounded lossless MPSC ring/batches как основная — меньше mutex convoy и allocations.
- **Риски:** потерянный index навсегда уменьшает capacity; publish/wrap и retirement требуют модели.
- **Как проверить:** 1 owner + 1..16 removers, bursts/paused owner; waits, allocations, p99; loom full/wrap/reserved-not-published. Не выполнялось.

#### ShardedRegion

- **Файлы/строки:** `src/concurrent/sharded_region.rs:254-331`, `src/concurrent/sharded_region.rs:405-417`, `src/concurrent/sharded_region.rs:458-487`.
- **Механизм:** `MY_SHARD` и единственный `ERASED_GUARD` process-global на thread, не на region instance. In-range id другого instance принимается без claim его token; owner detection также сравнивает только общий TLS id. Multiple regions получают коррелированное распределение и ложный owner-path.
- **Предложение/эффект:** explicit `ShardBinding` или TLS inline map `(instance generation, shard, guard)`; независимое распределение и меньше mutex/remote queue traffic при нескольких regions.
- **Риски:** instance ABA, lifecycle guards, TLS allocation; explicit handle меняет API.
- **Как проверить:** 2..16 regions и thread-per-core writes/removes; shard distribution, mutex waits, remote queue rate, drop/recreate. Не выполнялось.

### P2-7. Aligned lookup всё ещё цикл, хотя число шагов уменьшено

- **Файлы/строки:** `src/alloc_core/size_classes.rs:162-202`.
- **Механизм проблемы:** для `align>16` остаются loop, table reads и `is_multiple_of`; classification выполняется на alloc и dealloc, включая foreign free.
- **Предложение:** const table `[align_log2][size_bucket]` или `next_compatible[class][align_log2]`, один `trailing_zeros` и lookup.
- **Ожидаемый эффект:** умеренный, стабильный для SIMD/page-aligned workloads; worst case становится строго O(1).
- **Риски:** rodata footprint; ошибка классификации означает misalignment.
- **Как проверить:** exhaustive equivalence и aligned iai/criterion distribution. Не выполнялось.

### P2-8. `PerClass` grouping не гарантирует один cache line для фактического top

- **Файлы/строки:** layout `src/registry/tcache.rs:100-152`; hit `src/registry/heap_core.rs:935-985`; refill `src/registry/heap_core.rs:2027-2096`.
- **Статус:** новая оставшаяся возможность после locality fix.
- **Механизм проблемы:** `PerClass { count: u8, slots: [ptr;16] }` имеет примерно 136-byte stride на 64-bit. После refill малых классов `count≈15`, а pop читает `slots[14]`, то есть более чем через cache line от `count`; границы соседних `PerClass` также дрейфуют относительно 64-byte lines. Заявленная co-location работает только для некоторых depth/alignment.
- **Предложение:** descending stack layout, где top растёт к `count`, разделение на hot top window (например 4 pointers) и cold remainder, либо небольшой per-class top cache. Выбирать только по type-layout + cache benchmark.
- **Ожидаемый эффект:** убрать один dependent cache miss на hit при холодном/широком set классов; для одного hot class данные и так останутся в L1.
- **Риски:** больше index arithmetic/branches, padding раздует `HeapCore`; изменение refill/flush порядка повышает риск off-by-one.
- **Как проверить:** type-size/offset audit и random-class churn с depths 1/4/8/15; L1 misses и instructions. Не выполнялось.

### P2-9. `SyncRegion` остаётся coarse-grained и клонирует `T`

- **Файлы/строки:** `crates/region/src/sync_region.rs:31-65`, `crates/region/src/sync_region.rs:68-123`.
- **Статус:** round2 #10 без изменений.
- **Механизм проблемы:** один `RwLock<Region<T>>` сериализует все writes и связывает readers общей lock line; `get_cloned` копирует значение.
- **Предложение:** production-safe sharded locked region с shard id в handle; closure/guard read API без clone; ordered all-shard locking для clear.
- **Ожидаемый эффект:** масштабирование независимых writes до shard count и меньше clone cost больших `T`.
- **Риски:** handle/API compatibility, exact len, iteration и deadlock order; single-thread получает routing overhead.
- **Как проверить:** 1..64 threads, uniform/hotspot keys и read/write mixes; throughput/fairness/p99, stale handles и poison recovery. Не выполнялось.

## Потенциальные регрессии последних layout/diagnostic исправлений

### R1. Feature-off counters всё ещё делают feature-on scrape work

Это подтверждённая статическая регрессия, подробно описанная в P1-4: writes были убраны из production hot path, но read-side scan не был скомпилирован из feature-off конфигурации. При частом scrape стоимость переместилась с alloc hit на metrics thread и cache hierarchy всего процесса.

### R2. Dead-owner spin исправлен, slow-live-owner spin не исправлен

`owner_slot_is_live` устранил многоминутный dead-owner случай, но boolean liveness не означает прогресс. Paused LIVE owner всё ещё заставляет producers платить полный budget, а `DBG_RING_OVERFLOW` растёт на каждую попытку. Это остаточная регрессия RAD-4 retry policy, а не новый correctness defect.

### R3. Разнесение `head` и `tail` может ухудшить low-contention remote free

- **Файлы/строки:** `src/alloc_core/remote_free_ring.rs:404-451`, push `src/alloc_core/remote_free_ring.rs:623-650`.
- **Механизм:** push всегда читает и `tail`, и `head`. Раньше оба cursor были в одной cache line; теперь producer должен затронуть две. При fan-in/active owner это устраняет ping-pong и должно выигрывать, но при одном producer и редко пишущем owner потенциально увеличивает working set одного успешного push.
- **Ожидаемый эффект:** вероятная небольшая регрессия только low-contention; под contention ожидается крупный выигрыш. Статически знак определить нельзя.
- **Риски изменения:** возвращать cursors на одну line нельзя без A/B — это восстановит доказанное false sharing. Возможен компромисс: producer не читать `head` до локально вероятного full, но protocol должен сохранить boundedness.
- **Как проверить:** A/B 1 producer + idle/slow owner и 2..32 producers + active owner; cycles, L1 misses, HITM и p99. Не выполнялось.

### R4. Miri footprint исправлен cfg-способом, native архитектурная стоимость осталась

`HEAP_OVERFLOW_CAP=64` под miri устранил eager interpreter-memory blow-up, но не является production optimization: native по-прежнему резервирует 96 МиБ inline arrays. Это корректное targeted исправление CI-регрессии, однако оно не закрывает round2 #8 и создаёт различие ёмкости protocol между miri и native, которое следует учитывать при coverage full-queue сценариев.

## Рекомендуемый порядок экспериментов

1. Сначала дешёво исправить P1-4: compile-time zero при `!alloc-stats`, затем отдельный feature-on combined scan. Это минимальный риск и убирает явное противоречие API.
2. Ограничить P0-2 retry до десятков попыток и считать overflow один раз на logical free; только затем сравнивать sharded lanes/batching.
3. Построить инструментированный prototype P0-1 с per-class bitset и отдельным pending-ring index; не смешивать сразу с NUMA policy.
4. A/B P0/P1-3 на many-segment working set: прежний узкий GO недостаточен для решения о cache/TLB locality.
5. P1-5 compact overflow encoding исследовать вместе с P0-2, потому что короткий retry повысит частоту обращения к heap overflow.
6. Research-tier P1-6 и `SyncRegion` развивать отдельно от production allocator; сначала установить реальный API/user demand.

Для каждого GO/NO-GO нужны отдельные бюджеты throughput, p99/p999, RSS/page tables и correctness. Ни одна из перечисленных проверок в рамках этого read-only ревью не выполнялась.
