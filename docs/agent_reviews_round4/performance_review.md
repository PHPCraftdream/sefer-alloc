# Повторное read-only ревью производительности — round 4

Дата: 2026-07-13  
Контекст: `docs/agent_reviews_round3/performance_review.md`

## Методика и границы

Проверены текущие Rust-исходники, конфигурация Cargo и предыдущий отчёт. Git, сборки, тесты, бенчмарки, скрипты и fuzzing не запускались; выводы ниже являются статическим анализом. Никакие существующие файлы и исходники не изменялись. Предложенные способы проверки перечислены только как план и **не выполнялись**.

Термин «новый» ниже означает «не был отражён в отчёте round3 и обнаружен в текущем коде». Без Git-истории он не означает доказанную привязку к конкретному последнему коммиту.

## Краткий итог

Главный новый риск — удержание кэшей в неактивных/recycled heap-слотах. По умолчанию каждый материализованный heap может оставить четыре small-сегмента (до 16 MiB при размере сегмента 4 MiB), до восьми large-span и tcache-объекты, мешающие decommit. Ни thread teardown, ни перевод слота `LIVE -> FREE` эти уровни не очищают, а decay происходит лишь при последующих операциях того же heap. После всплеска числа короткоживущих потоков RSS/commit может остаться высоким на неопределённое время; глобального бюджета нет.

Три прежних наиболее сильных CPU-узких места остаются: линейный поиск small-сегмента, до 262 145 попыток записи в переполненный remote ring и запись `MagazineBitmap` на каждом tcache-hit. Исправление статистики для production-конфигурации подтверждено: без `alloc-stats` два обхода heap-слотов устранены. Исправление tombstone-насыщения хеш-таблицы также полезно, но синхронный rebuild добавил локальный tail-latency spike.

Приоритет действий:

1. Ввести процессный бюджет и явный trim неактивных heap-кэшей; определить политику thread teardown/recycle.
2. Заменить полный обход small-сегментов индексом доступности по size class.
3. Резко ограничить retry remote ring и одновременно подготовить более компактный/менее contended overflow.
4. Убрать обязательную запись `MagazineBitmap` из каждого tcache-hit без ослабления exact double-free oracle.
5. После этого разбирать layout tcache, aligned lookup и исследовательские region-реализации.

## Статус пунктов round3

| Round3 | Статус сейчас | Текущие файлы/строки | Механизм и ожидаемый эффект | Риск | Как проверить (не выполнялось) |
|---|---|---|---|---|---|
| I1, zero-copy refill | **Закрыт, исправление сохранено** | `src/registry/heap_core.rs:2027-2096`; `src/alloc_core/alloc_core_small.rs:721-805,1821-2030` | Заполнение tcache идёт напрямую в целевой массив, без промежуточного magazine и второго копирования. Сохраняется снижение трафика памяти на refill. | Возможны off-by-one/частично заполненный batch при редких ошибках refill. | Сравнить циклы, retired loads/stores и корректность batch для глубин 0/1/16; отдельно исчерпание сегмента и смену active segment. |
| I2, дешёвый empty-guard remote ring | **Закрыт, исправление сохранено** | `src/alloc_core/alloc_core_small.rs:1554-1630`; `src/alloc_core/remote_free_ring.rs:738-785` | До дорогого drain проверяется `tail == head`; пустой ring не вызывает полный протокол. Сохраняется выигрыш на обычном owner miss. | Устаревшее наблюдение не должно приводить к потере wakeup/элемента. | Проверить гонки producer/owner вокруг guard и p50 owner allocation при всегда пустых ring. |
| I3, разделение горячих полей по cache line | **Закрыт, исправление сохранено** | `src/registry/heap_slot.rs:130-232`; `src/alloc_core/remote_free_ring.rs:404-451`; `src/alloc_core/segment_header.rs:297-396` | Owner/remote-счётчики и head/tail основных структур разнесены, уменьшая false sharing при fan-in. | Увеличенный layout ухудшает footprint; у `HeapOverflow` аналогичного разделения нет. | Измерить `HITM`, LLC misses, throughput и p99 при 1/2/8/32 producer на один owner. |
| I4, пропуск zero свежего bitmap | **Закрыт, исправление сохранено** | `src/bootstrap/bootstrap_config.rs:74-102`; `src/alloc_core/alloc_core_small.rs:2478-2520` | Для гарантированно свежей памяти исключено повторное обнуление bitmap. Сохраняется выигрыш при первом вводе сегмента. | Ошибочная классификация reused memory как fresh нарушит корректность allocator metadata. | Проверить fresh/reused/decommitted/recommitted пути с poison-паттерном и sanitizers/Miri-профилем. |
| I5, скачки в aligned classification | **Частично закрыт** | `src/alloc_core/size_classes.rs:131-203` | Переходы теперь пропускают заведомо несовместимые классы; среднее число итераций ниже. Строгого O(1) всё ещё нет. | Большая таблица замены может увеличить `.rodata` и I-cache pressure. | Exhaustive-сверка всех `(size, align)` и распределение числа итераций/циклов. |
| I6, реальный cursor в HeapOverflow drain | **Закрыт, исправление сохранено** | `src/registry/heap_overflow.rs:366-397` | Drain продолжает с фактической позиции, избегая повторного прохода с начала. | Ошибка cursor/head при wraparound способна пропустить либо повторить entry. | Проверить wraparound, budgeted drain и конкурентные push с модельным тестом. |
| P0-1, полный scan small-сегментов | **Остаётся** | `src/alloc_core/alloc_core_small.rs:1452-1687`; `src/alloc_core/segment_table.rs:129-176` | На miss обходится весь high-water range таблицы, включая holes. | См. пункт R1. | См. пункт R1. |
| P0-2, retry storm remote free | **Остаётся** | `src/registry/heap_core.rs:181-218,1841-1892`; `src/alloc_core/remote_free_ring.rs:621-655` | Live-owner gate сохранён, но живой медленный owner всё ещё допускает 262 144 повторные попытки после первого fail. | См. пункт R2. | См. пункт R2. |
| P0/P1-3, `MagazineBitmap` на tcache hit | **Остаётся** | `src/registry/heap_core.rs:935-1013`; `src/alloc_core/magazine_bitmap.rs:64-145` | Каждый hit вычисляет segment base и делает byte RMW. | См. пункт R3. | См. пункт R3. |
| P1-4, два обхода heaps ради stats | **Закрыт для production; остаток при feature-on** | `Cargo.toml:163-185`; `src/registry/heap_registry.rs:1010-1130`; `src/global/sefer_alloc.rs:280-367` | Без `alloc-stats` функции возвращают константу и обходов нет. С feature два независимых O(slots) scan сохраняются. | См. пункт R7. | См. пункт R7. |
| P1-5, inline `HeapOverflow` | **Остаётся** | `src/registry/heap_overflow.rs:109-216`; `src/registry/heap_slot.rs:334-350` | Фиксированные массивы остаются встроенными во все heap slots. | См. пункт R4. | См. пункт R4. |
| P1-6, experimental region copy/contention | **Остаётся; найден дополнительный дефект** | `src/concurrent/lock_free_region.rs:73-101,290-386`; `src/concurrent/epoch_region.rs:127-140,208-252`; `src/concurrent/sharded_region.rs:275-330,458-487` | Полное copy-on-write snapshot, mutex queue и глобальная TLS-привязка остаются. В `ShardedRegion` возможна утечка claimed token. | См. R10-R12. | См. R10-R12. |
| P2-8, layout `Tcache::PerClass` | **Остаётся** | `src/registry/tcache.rs:100-152`; `src/registry/heap_core.rs:935-985` | `count` и фактический top после refill лежат на разных cache line. | См. R6. | См. R6. |
| P2-9, coarse `SyncRegion` | **Остаётся** | `crates/region/src/sync_region.rs:31-123` | Все записи сериализуются одним `RwLock`. | См. R13. | См. R13. |
| R3, split ring cursor | **Не подтверждено измерениями; риск остаётся** | `src/alloc_core/remote_free_ring.rs:404-451,621-655` | Fan-in выигрывает от разделения, но single-producer push читает две линии. | См. R9. | См. R9. |
| R4, Miri cap | **Тестовый cap сохранён; native footprint остаётся** | `src/registry/heap_overflow.rs:109-175` | Miri использует 64 entry, native — 2048; тестовая экономия не влияет на production. | См. R4. | См. R4. |

## Новые регрессии и новые для round4 риски

### N1 — P0: неактивные heap-слоты бессрочно удерживают small pool, large cache и tcache

- **Файлы/строки:** `src/alloc_core/small_segment_pool_config.rs:31-56,111-133`; `src/alloc_core/large_cache_config.rs:19-20,46-54,86-118,151-159`; `src/alloc_core/alloc_core_small_pool.rs:161-253,393-449,521-534`; `src/alloc_core/alloc_core_small.rs:2350-2370`; `src/alloc_core/alloc_core_large_cache.rs:20-61`; `src/alloc_core/alloc_core.rs:65-78,1723-1730`; `src/global/tls_heap.rs:200-272`; `src/registry/heap_registry.rs:212-259`; `src/registry/heap_core.rs:359-370,2383-2427`.
- **Механизм:** production default включает small pool ёмкостью четыре сегмента. Pool сохраняет сегменты committed и registered; decay вызывается только на последующем cold reserve этого heap. Large cache имеет восемь слотов, default budget отсутствует, а decay до превышения 256 MiB headroom не начинается и тоже требует последующей large-операции. Thread teardown очищает deferred-large список и переводит slot в `FREE`, но не flush-ит tcache, small pool или large cache. `HeapCore` при recycle не уничтожается. Tcache-held blocks одновременно не дают опустевшим сегментам перейти к decommit.
- **Ожидаемый эффект:** после волны `K` короткоживущих потоков RSS/commit и page-table footprint могут остаться пропорциональны числу материализованных slots, а не текущей конкуренции. Только базовый default small pool допускает до `K * 16 MiB`; восемь cached large-span добавляют как минимум до `K * 32 MiB` при 4 MiB spans, но фактически могут быть намного крупнее, потому что admission budget по умолчанию не ограничен. Это прежде всего memory/latency regression: pressure, page faults, reclaim и NUMA-locality могут затмить выигрыш от повторного использования.
- **Рекомендация:** добавить глобальный/процессный cache budget и счётчик retained committed bytes; на TLS teardown либо при переходе slot в неактивный epoch выполнять ограниченный trim: flush tcache, drain small pool, evict large cache, оставляя только явно заданный малый warm reserve. Для долгоживущих простаивающих owners нужен timeout/epoch trim, не зависящий от следующего allocation данного heap.
- **Риски изменения:** агрессивный trim увеличит syscall/recommit/page-fault cost следующей волны, может ухудшить short-burst throughput и требует аккуратной синхронизации с remote free/deferred large. Глобальный точный счётчик сам может стать contended; предпочтительны batched/per-heap deltas.
- **Как проверить (не выполнялось):** волны из 1/16/256/4096 короткоживущих потоков с последующим idle; снимать RSS, committed/resident pages, page tables, minor faults и latency следующей волны. Отдельно прогнать small-only, large-only, mixed и custom pool/cache configs; сравнить policies «retain all / warm cap / teardown trim / global budget».

### N2 — P1: cache policy «прилипает» к recycled heap slot и может игнорировать конфигурацию нового allocator

- **Файлы/строки:** `src/registry/heap_registry.rs:146-151,161-200,212-259`; `src/registry/heap_core.rs:2383-2393`; `src/alloc_core/large_cache_config.rs:86-118,151-159`.
- **Механизм:** `claim_with_config` применяет конфигурацию только при первой материализации `HeapCore`. После `LIVE -> FREE` тот же slot выдаётся повторно с прежним core и прежней cache policy. Даже тестовый комментарий у small-pool drain отмечает, что pool нельзя надёжно отключить для уже использованного slot. Поэтому второй `SeferAlloc` с меньшим budget/выключенным pool может фактически наследовать более агрессивное удержание, а обратный порядок — потерять ожидаемое кэширование.
- **Ожидаемый эффект:** заданные пользователем memory bounds и latency/throughput trade-off становятся зависимыми от истории выдачи slots. Для обычного единственного static global allocator частота мала, но для нескольких allocator instances, тестовых harness и embedding-сценариев результат может быть крупным и недетерминированным.
- **Рекомендация:** либо сделать policy процессно-глобальной и явно запретить несовместимые configs, либо безопасно reconfigure recycled core с немедленным trim до нового cap. Версию policy стоит хранить рядом с состоянием slot.
- **Риски изменения:** reconfigure не должен выполняться до полной quiescence старого owner; уменьшение cap требует корректно освободить retained segments/spans, а увеличение не должно сбрасывать уже полезный cache.
- **Как проверить (не выполнялось):** последовательно получить/recycle один slot двумя allocator instances с pool cap `4 -> 0` и large budget `None -> small`, затем в обратном порядке; проверить фактический cap, retained bytes и отсутствие доступа старого поколения.

### N3 — P1/P2: исправление tombstone saturation добавило синхронный O(capacity + slots) spike

- **Файлы/строки:** `src/alloc_core/segment_table.rs:172-196,333-340,422-428,737-881,898-925`.
- **Механизм:** новый tombstone counter и rebuild закрывают прежний сценарий полного probe по 2048 hash slots после длительного churn — это важное улучшение амортизированной сложности. Но на 513-м tombstone `maybe_rebuild_hash_index` синхронно очищает все 2048 entries, затем обходит до 1024 table slots и повторно вставляет живые bases. Rebuild выполняется внутри unregister/recycle path.
- **Ожидаемый эффект:** средняя производительность и долговременная стабильность лучше, но один operation получает холодный проход по нескольким массивам и tail-latency spike. При churn large cache/segments этот периодический p99/p999 может быть заметен.
- **Рекомендация:** рассмотреть incremental rebuild с фиксированным budget на операцию, backshift deletion либо dual-table migration; как минимум переносить rebuild в owner cold path с ограничением работы.
- **Риски изменения:** нарушение open-addressing probe invariant даст false negative; dual table увеличит footprint и усложнит удаление/ABA при recycle.
- **Как проверить (не выполнялось):** создать контролируемые последовательности до 511/512/513 tombstones, измерять latency каждого unregister/recycle, probe length, L1/LLC misses и корректность lookup во время wraparound/churn.

### N4 — P2, experimental: `ShardedRegion` может навсегда потерять shard token при нескольких instances

- **Файлы/строки:** `src/concurrent/sharded_region.rs:29-40,146-155,275-330,405-417,458-487`.
- **Механизм:** TLS хранит один process-wide shard id/guard без identity конкретного region. `claim` принимает любой in-range cached id как owner этого instance. Если cached guard уже относится к другому region, новый token всё равно CAS-ится в claimed state, но новый guard не устанавливается и token не будет освобождён. `bind_current_thread` имеет тот же порядок. Повторение способно исчерпать owner shards и перевести операции в modulo/shared routing с lock contention.
- **Ожидаемый эффект:** деградация с owner-local shards к общим locks, растущая со временем и числом region instances; некорректная owner classification дополнительно искажает locality. Компонент помечен deprecated/experimental, поэтому production-приоритет ниже.
- **Рекомендация:** TLS binding должен содержать `(region identity/generation, shard id, guard)` либо API должен возвращать explicit binding handle. Claim нового token допустим только после успешной установки соответствующего guard.
- **Риски изменения:** lifetime/ABA identity, TLS allocation и несовместимость API; освобождение guard во время destruction region требует строгого порядка.
- **Как проверить (не выполнялось):** чередовать claim/drop множества `ShardedRegion` на одном потоке и между потоками; отслеживать число claimed tokens, долю shared routing, lock wait и корректность owner removal.

## Оставшиеся основные проблемы

### R1 — P0: поиск доступного small-сегмента остаётся O(S)

- **Файлы/строки:** `src/alloc_core/alloc_core_small.rs:1452-1687`; `src/alloc_core/segment_table.rs:129-176`.
- **Механизм:** `find_available_segment_for_class` берёт high-water `count()` и проходит `0..n`, читая slot/kind, ring state и `BinTable`. Null holes не сокращают диапазон. При NUMA policy проход может продолжиться после найденного foreign candidate. Стоимость miss растёт с максимальным когда-либо использованным slot index, а не только с числом подходящих сегментов.
- **Ожидаемый эффект:** при сотнях/тысячах сегментов cold/refill path получает тысячи pointer/metadata loads, branch misses и cache misses. Это наиболее радикальная оставшаяся алгоритмическая возможность: owner-private per-class availability index может приблизить выбор к O(1) или O(number of non-empty words).
- **Рекомендация:** поддерживать exact owner-private bitset/list сегментов с локально доступными blocks для каждого class и отдельный pending-remote bitmap/queue. Удалять membership при исчерпании, добавлять при refill/drain/free; NUMA candidates хранить раздельно.
- **Риски изменения:** lost membership/wakeup при recycle, pool/decommit, remote drain и смене поколения; дубликаты и stale segment ids; рост metadata и write amplification на переходах состояния.
- **Как проверить (не выполнялось):** 1/16/256/1024 сегмента, sparse holes, local/foreign NUMA и remote-only availability; сравнить cycles per miss, scanned slots, LLC misses и p99. Добавить модельные проверки membership на всех state transitions.

### R2 — P0: одна remote free способна создать retry/atomic storm

- **Файлы/строки:** `src/registry/heap_core.rs:181-218,1841-1892`; `src/alloc_core/remote_free_ring.rs:621-655`.
- **Механизм:** после первой неудачи live owner допускает ещё 262 144 push attempts. Каждая full-попытка читает tail/head и выполняет два `fetch_add` для статистики overflow. Итого одна логическая free может сделать 262 145 попыток и до 524 290 atomic RMW только в счётчиках, прежде чем попасть в `HeapOverflow`. Gate мёртвого owner полезен, но не помогает для paused/preempted живого owner; единый MPSC tail остаётся точкой fan-in contention.
- **Ожидаемый эффект:** непропорциональный CPU burn, coherence traffic и p99/p999 при заполненном ring; producer может тратить миллисекунды на allocator free. Радикальное сокращение retry даёт orders-of-magnitude выигрыш именно в патологическом, но эксплуатационно важном режиме.
- **Рекомендация:** 8-32 adaptive attempts с backoff, затем немедленный overflow; считать логический overflow один раз, а не каждую пробу. Для устойчивого fan-in рассмотреть producer lanes или batch publication. Изменять вместе с R4, потому что overflow станет более горячим.
- **Риски изменения:** слишком ранний fallback увеличит давление/ёмкость overflow; lanes повышают footprint и усложняют ordering/drain fairness.
- **Как проверить (не выполнялось):** остановленный/редко планируемый owner и 1/2/8/32 producers; sweep retry limit, ring capacity и overflow capacity. Снимать CPU/free, atomic ops, HITM, overflow occupancy, dropped/fallback paths и p99.

### R3 — P0/P1: обязательный `MagazineBitmap` RMW на каждом tcache-hit

- **Файлы/строки:** `src/registry/heap_core.rs:935-1013,1305-1405,2050-2105`; `src/alloc_core/magazine_bitmap.rs:64-145`.
- **Механизм:** hit извлекает pointer, вычисляет segment base и выполняет byte read-modify-write `clear_magazine`. Bitmap занимает 32 KiB на small segment. Own free снова обращается к тому же metadata для exact membership/double-free проверки. Таким образом самый горячий fast path несёт pointer arithmetic, дополнительный metadata load/store и потенциально случайную cache-line запись.
- **Ожидаемый эффект:** заметная доля циклов и cache write traffic при высоком tcache hit rate; при множестве активных сегментов bitmap вытесняет полезные данные. Устранение операции из каждого hit потенциально даёт крупный single-thread throughput gain.
- **Рекомендация:** сохранить точный oracle, но перенести membership/tag ближе к tcache entry или использовать heap-private exact table/compact generation tags; обновлять metadata пакетно при refill/flush. Вероятностный фильтр без точного подтверждения недостаточен для double-free correctness.
- **Риски изменения:** ошибочная композиция tcache/ring state пропустит double free или даст false positive; дополнительный tag увеличит tcache footprint и может нивелировать выигрыш.
- **Как проверить (не выполнялось):** hit-only workload по 1/4/64 сегментам и разным class/depth; cycles, stores, L1D/LLC misses и bitmap line residency. Exhaustive state-machine test `refill -> alloc -> free -> flush -> remote` с generation reuse.

### R4 — P1: `HeapOverflow` имеет большой inline footprint, разнесённые payload arrays и false sharing head/tail

- **Файлы/строки:** `src/registry/heap_overflow.rs:109-216,280-309,366-397`; `src/registry/heap_slot.rs:334-350`.
- **Механизм:** native capacity 2048; два SoA массива (`AtomicUsize` bases и `AtomicU32` packed) дают около 24 KiB на heap slot, то есть около 96 MiB virtual metadata при 4096 slots. Payload одной entry читается из двух далёких массивов. В отличие от основного ring, `tail` и `head` расположены рядом: producer CAS tail и owner store head могут делить cache line именно под overflow contention.
- **Ожидаемый эффект:** большой virtual/page-table/cache footprint, до двух payload cache streams на drain и HITM на control line. После сокращения R2 этот путь станет чаще, поэтому его стоимость нельзя рассматривать изолированно.
- **Рекомендация:** компактная 64-bit entry с segment id/generation + offset/class, меньшая inline queue и lazy/sharded sidecar для редкого пика; независимо разнести head/tail по cache lines. Если pointer/base нельзя упаковать безопасно, хотя бы перейти к AoS entry.
- **Риски изменения:** ABA при segment id reuse, потеря capacity, невозможность аллокации sidecar в reentrant allocator path, contention общего slab и wraparound ordering.
- **Как проверить (не выполнялось):** footprint до/после materialization 1..4096 heaps; saturated overflow с 1..32 producers, drain bandwidth, HITM и p99. Обязательны generation/wraparound/model checks.

### R5 — P2: direct own-segment cache из четырёх entries легко конфликтует

- **Файлы/строки:** `src/alloc_core/segment_table.rs:98,492-538,898-925`; `src/registry/heap_core.rs:1637-1667`.
- **Механизм:** каждая own free вызывает `contains_base`; direct cache имеет только четыре позиции и прямое индексирование. При interleaving более четырёх сегментов либо совпадении индексов почти каждая free падает в primordial hash probe, добавляя pointer chasing на горячем пути.
- **Ожидаемый эффект:** умеренная, но систематическая потеря throughput при producer working set >4 segments. 2-way associative cache или 8-16 recent bases может резко поднять hit rate при малом footprint.
- **Риски изменения:** рост `HeapCore`, cache pollution и сложнее invalidation при unregister/generation reuse.
- **Как проверить (не выполнялось):** interleaved own frees по 1/4/5/16/64 сегментам, включая adversarial direct-map aliases; hit rate, probes, cycles и L1D misses.

### R6 — P2: `Tcache::PerClass` отделяет `count` от реального top entry

- **Файлы/строки:** `src/registry/tcache.rs:100-152`; `src/registry/heap_core.rs:935-985`.
- **Механизм:** `PerClass` содержит `u8 count` и 16 pointers, фактический stride на 64-bit около 136 bytes. После обычного refill top находится у высокого индекса, то есть далеко от линии с count; комментарий о близости top применим только к малой глубине. Class churn затрагивает две-три линии и создаёт плохую spatial locality.
- **Ожидаемый эффект:** лишний L1D miss/load latency на fast hit, особенно при переключении size classes. Descending stack либо отдельное hot-window из 2-4 pointers может держать count и наиболее частый top в одной линии.
- **Риски изменения:** off-by-one, дополнительная ветка, padding/рост всего tcache и более дорогой refill/flush.
- **Как проверить (не выполнялось):** hit/refill при depth 1/4/8/16 и uniform/Zipf class mix; layout assertions, L1D misses, cycles/op и total heap footprint.

### R7 — P2: при `alloc-stats` snapshot всё ещё дважды сканирует registry

- **Файлы/строки:** `src/registry/heap_registry.rs:1010-1130`; `src/global/sefer_alloc.rs:280-367`; `src/registry/heap_core.rs:971-984`; `src/alloc_core/alloc_core_large.rs:109-130`; `Cargo.toml:163-185`.
- **Механизм:** feature-off path теперь O(1), поэтому прежняя production-регрессия закрыта. С включённой статистикой `tcache_hits_total` и `large_cache_hits_total` независимо проходят heap slots и делают atomic loads.
- **Ожидаемый эффект:** до двух O(slots) проходов на snapshot; важно для telemetry-heavy/debug deployments, но не для стандартной production feature set.
- **Рекомендация:** единый `registry_stats()` проход, агрегирующий оба счётчика и будущие heap-local metrics.
- **Риски изменения:** расширение snapshot API, consistency между счётчиками и условная компиляция.
- **Как проверить (не выполнялось):** snapshot latency при 1/256/4096 materialized slots с feature on/off; число loads и equivalence агрегатов.

### R8 — P2: aligned size-class lookup не является строгим O(1)

- **Файлы/строки:** `src/alloc_core/size_classes.rs:131-203`.
- **Механизм:** для alignment >16 после бинарного выбора стартового класса выполняется цикл и `is_multiple_of`; скачки уменьшают средний путь, но число проверок зависит от таблицы классов/alignment.
- **Ожидаемый эффект:** умеренный выигрыш на часто используемых over-aligned allocations от const lookup `next_compatible[class][align_log2]`; это ниже по приоритету, чем R1-R4.
- **Риски изменения:** рост `.rodata`, генератор таблицы/ручные ошибки и несогласованность с изменениями size classes.
- **Как проверить (не выполнялось):** exhaustive equivalence по всем supported size/alignment, code size/I-cache и histogram циклов lookup на реальном распределении.

### R9 — P3/watchlist: split head/tail основного ring может ухудшать single-producer locality

- **Файлы/строки:** `src/alloc_core/remote_free_ring.rs:404-451,621-655`.
- **Механизм:** cache-line separation уменьшает producer-owner false sharing при fan-in, но успешный push читает собственный tail и owner head с двух линий. При одном producer и редко активном owner прежняя совместная линия теоретически могла быть дешевле.
- **Ожидаемый эффект:** возможная небольшая регрессия low-contention latency в обмен на большой contended gain; статически знак определить нельзя.
- **Рекомендация:** не объединять поля без измерений; сравнить текущую схему с sequence-number ring либо producer-local cached head.
- **Риски изменения:** stale cached head увеличит false-full, а sequence numbers добавят атомик на каждую entry.
- **Как проверить (не выполнялось):** 1 producer + idle/slow owner и 2/8/32 active producers; cycles, L1 misses, HITM, false-full и p50/p99.

## Experimental/concurrent tier

### R10 — P2, experimental/deprecated: `LockFreeRegion` копирует snapshot/page table на каждую запись

- **Файлы/строки:** `src/concurrent/lock_free_region.rs:73-101,152-155,290-386`.
- **Механизм:** writer clone-ит весь `Vec<Arc<Page>>`/snapshot, а изменяемая page — до 64 `Slot`/`Arc<T>`. Формально lock-free reads оплачиваются O(number of pages) Arc refcount operations на write и большим pointer chasing.
- **Ожидаемый эффект:** write throughput падает с ростом region и создаёт allocator/refcount traffic. Radix/COW tree или sharded immutable pages дают O(log P)/O(1) touched nodes.
- **Риски изменения:** reclamation, snapshot consistency, ABA и более сложный unsafe proof. Тип уже deprecated.
- **Как проверить (не выполнялось):** фиксированная write rate при 1/16/256/4096 pages; Arc inc/dec, allocated bytes, cycles/write и reader latency.

### R11 — P2, experimental: `EpochRegion` блокирует даже проверку пустой remote queue и теряет capacity

- **Файлы/строки:** `src/concurrent/epoch_region.rs:127-140,208-252,275-280,338-341,393-398`.
- **Механизм:** вопреки комментарию об empty fast path, drain всегда берёт `remote_free.lock()` и только затем проверяет `is_empty`. `mem::take` оставляет исходный `Vec` с capacity 0, поэтому следующий burst может снова аллоцировать. Insert уже берёт `state` mutex, затем drain берёт второй mutex.
- **Ожидаемый эффект:** lock/unlock на owner operations при пустой очереди, повторные аллокации на bursts и convoy между remote removers.
- **Рекомендация:** atomic nonempty hint перед mutex и swap с reusable scratch Vec/capacity; для высокой нагрузки — bounded MPSC batches.
- **Риски изменения:** hint допускает false positive, но не false negative; порядок публикации должен исключать lost drain. Два reusable buffers требуют защиты от reentrancy.
- **Как проверить (не выполнялось):** empty-owner fast path, bursty remote frees и steady fan-in; lock acquisitions, allocations, wait time и p99.

### R12 — P2, experimental: `ShardedRegion` имеет глобальную TLS-привязку

- **Файлы/строки:** `src/concurrent/sharded_region.rs:29-40,146-155,275-330,405-417,458-487`.
- **Механизм:** кроме утечки N4, один TLS id используется между instances; cached in-range id ошибочно считается owner id нового region. Это коррелирует routing и делает owner fast path семантически зависимым от другого объекта.
- **Ожидаемый эффект:** дисбаланс shards, лишний lock contention и неверная оценка locality в multi-region workload.
- **Рекомендация:** использовать instance-keyed binding и не принимать cached id без совпадения identity/generation region.
- **Риски изменения:** lifetime/ABA identity, дополнительный TLS footprint и необходимость безопасно освобождать binding при destruction region.
- **Как проверить (не выполнялось):** чередовать несколько regions на одном наборе потоков; измерять распределение операций по shards, false-owner decisions, lock wait и устойчивость после многократного create/drop.

### R13 — P2, если тип нужен в production: `SyncRegion` сериализует все writes одним `RwLock`

- **Файлы/строки:** `crates/region/src/sync_region.rs:31-123`.
- **Механизм:** insert/remove/clear получают общий write lock; `get_cloned` клонирует `T` под read lock. Независимые slots не масштабируются по writers.
- **Ожидаемый эффект:** throughput ограничен одной critical section, p99 растёт с threads; cloning больших `T` добавляет скрытые копии/аллокации.
- **Рекомендация:** sharded locks по slot/page либо guard-based read API, если lifetime позволяет.
- **Риски изменения:** memory overhead locks, deadlock ordering, API/lifetime complexity и ухудшение single-thread path.
- **Как проверить (не выполнялось):** disjoint-slot и same-slot workloads для 1/2/8/32 threads, lock wait, clones/allocations и p99.

## Положительные изменения, которые важно не потерять

- Small segment pool (`src/alloc_core/alloc_core_small_pool.rs:161-253`) способен заметно сократить OS reserve/commit/decommit churn при колебаниях рабочей нагрузки. Исправлять N1 следует бюджетированием и lifecycle trim, а не безусловным удалением reuse.
- Tombstone rebuild (`src/alloc_core/segment_table.rs:807-881`) устраняет неограниченное накопление tombstones и худший полный probe; N3 касается только способа распределить стоимость rebuild.
- Dead-owner gate перед длительными remote-ring retries (`src/registry/heap_core.rs:1849-1877`) предотвращает бессмысленный spin для явно неактивного owner; сокращение R2 должно сохранить этот быстрый переход.
- Feature gating статистики (`src/registry/heap_registry.rs:1010-1130`) действительно убирает прежние registry scans из production feature set.

## Рекомендуемый порядок реализации и проверки

1. **Memory envelope:** определить формальную гарантию retained bytes, добавить observability по tcache/small pool/large cache и реализовать teardown/idle/global-budget policy (N1, N2).
2. **Miss asymptotics:** ввести per-class availability index и доказать переходы membership модельными тестами (R1).
3. **Remote pressure:** ограничить retry, затем уплотнить и развести control fields overflow; оценивать оба изменения совместно (R2, R4).
4. **Fast hit:** прототип exact membership без per-hit bitmap write (R3), затем улучшить tcache layout и own-cache associativity (R5, R6).
5. **Tail cleanup:** incremental hash rebuild, объединённая stats aggregation, O(1) aligned lookup (N3, R7, R8).
6. **Research tier:** исправлять N4/R10-R13 только если deprecated/experimental API действительно должен стать production-путём.

Ни один из перечисленных способов проверки в рамках этого read-only ревью не выполнялся.
