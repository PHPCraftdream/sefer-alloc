# Read-only аудит производительности: возможности радикального ускорения

Дата аудита: 2026-07-12. Область: Rust-исходники и `Cargo.toml`. Аудит статический: сборка, тесты, бенчмарки, профилировщики, fuzzing и скрипты не запускались. Исходники и конфигурация не изменялись.

## Краткий вывод

Самый большой потенциал сосредоточен не в мелких инструкционных правках, а в четырёх архитектурных местах:

1. miss малого аллокатора выполняет `O(number_of_segments)` обход всех сегментов;
2. fan-in cross-thread free сериализуется на одном CAS-курсоре, а полный ring способен породить до 262 144 повторов с глобальными atomic RMW;
3. magazine hit, то есть главный small-allocation fast path, обязательно обращается к отдельному 32-КиБ bitmap сегмента;
4. experimental `LockFreeRegion` копирует таблицу страниц и целую страницу слотов на каждую запись.

Ниже предложения отсортированы по ожидаемому потенциалу. Оценки эффекта качественные и являются гипотезами до измерения.

## P0 — максимальный потенциал

### 1. Заменить полный обход SegmentTable на индекс непустых сегментов по size class

**Файлы и строки:** `src/alloc_core/alloc_core_small.rs:709-790`, `src/alloc_core/alloc_core_small.rs:1264-1344`, `src/alloc_core/alloc_core_small.rs:1476-1673`; связанные поля `src/alloc_core/alloc_core.rs:297-305`.

**Наблюдение.** После промаха текущего сегмента `find_segment_with_free_impl` проходит `slots[0..count]`, пропускает null/large entries, проверяет remote ring каждого small-сегмента и затем `BinTable[class]`. Асимптотика одного miss — `O(S)`, где `S` — число когда-либо заведённых слотов до high-water mark; разреженные recycled slots тоже читаются. В NUMA-конфигурации тот же проход дополнительно читает node metadata. Batch-refill устраняет повторный scan внутри одного refill, но не следующий miss.

**Механизм ускорения.** Завести owner-private intrusive списки/битовые множества сегментов с доступными блоками отдельно для каждого класса, плюс отдельный список сегментов с потенциально непустым remote ring. Переход `BinTable[class]: empty -> non-empty` добавляет сегмент в класс, `non-empty -> empty` удаляет; remote producer выставляет один atomic pending-bit сегмента/heap, а owner при drain раскладывает reclaimed blocks по class lists. Для NUMA можно иметь head на `(node, class)` и fallback head. Тогда обычный miss становится `O(1)` вместо полного pointer-chasing scan; полный аудит сегментов остаётся редким maintenance/scavenge проходом.

**Ожидаемый эффект.** Радикальный на фрагментированных/high-thread-count heaps: latency miss перестаёт расти с количеством сегментов, уменьшаются чтения чужих cache lines и TLB misses. На steady-state hit-path эффект нулевой; на refill-heavy и remote-free workloads возможен многократный выигрыш.

**Риски.** Нужно строго поддерживать membership при flush, ring drain, decommit, pool/unpool, recycle и adoption; дубликат intrusive entry может привести к повторной выдаче. Atomic pending-bit должен исключать lost wakeup между producer и owner. Дополнительные heads увеличат metadata (49 классов × NUMA nodes).

**Как проверить (не выполнялось).** Criterion/iai сценарии с фиксированным hit rate и `S={1,16,128,1024}`, большим числом recycled holes и remote frees; считать cycles, instructions, LLC/TLB misses и p50/p99 refill latency. Property/loom тестами проверить переходы empty/non-empty, concurrent set/clear pending-bit и отсутствие двойного членства. Сравнить число прочитанных segment headers на miss.

### 2. Убрать CAS-сериализацию и retry storm из cross-thread free

**Файлы и строки:** `src/alloc_core/remote_free_ring.rs:592-633`, `src/alloc_core/remote_free_ring.rs:636-702`, `src/registry/heap_core.rs:1794-1845` (лимит задан в `src/registry/heap_core.rs:216-218`), `src/registry/heap_overflow.rs:260-309`.

**Наблюдение.** Все producers одного сегмента конкурируют за `tail.compare_exchange_weak`. При full ring каждый `RemoteFreeRing::push` ещё выполняет два диагностических `fetch_add` (`overflow` сегмента и глобальный `DBG_RING_OVERFLOW`, строки 607-613). `push_with_overflow_retry` для live owner повторяет такой полный push до 262 144 раз; значит один неудачный free может создать сотни тысяч CAS/load/locked-RMW, забить interconnect и ухудшить работу owner, который должен освободить ring. Только после этого используется heap-level overflow ring, снова с общим CAS tail.

**Механизм ускорения.** Немедленно уходить в overflow после одного/малого адаптивного числа неудач, не инкрементируя diagnostics на каждую внутреннюю попытку. Архитектурно сильнее — заменить один MPSC cursor на sharded producer lanes: например, несколько SPSC/MPSC sub-rings по hash(thread-id), либо per-thread remote batches, публикуемые одним exchange в heap inbox. Owner последовательно дренирует lanes; счётчики агрегируются локально и публикуются редко. Backoff должен быть bounded в десятках итераций и учитывать прогресс `head`, а не только факт live owner.

**Ожидаемый эффект.** Очень высокий при fan-in и saturated ring: устранение потенциально шести порядков лишней работы на одном `dealloc`, снижение p99/p999 и cache-line bouncing. Даже до saturation sharding уменьшит CAS contention почти пропорционально числу lanes до их заполнения.

**Риски.** Больше metadata; порядок между lanes перестанет быть глобальным (он, вероятно, не нужен для независимых frees, но это надо доказать). Per-thread batching усложняет teardown и late frees. Нельзя потерять текущие ABA/generation и publish-before-consume гарантии; bounded leak semantics должны остаться явными.

**Как проверить (не выполнялось).** Fan-in benchmark 2/4/8/16/32 producers на один segment при active, slow и paused owner; измерить throughput, CAS failures, locked instructions, cache-to-cache transfers и tail latency каждого `dealloc`. Отдельно заполнить ring заранее и сравнить стоимость одной неудачи. Loom-модель reserve/publish/drain и тесты wraparound cursors, producer teardown, generation guard.

### 3. Убрать 32-КиБ MagazineBitmap из каждого magazine hit/push

**Файлы и строки:** `src/registry/heap_core.rs:937-1015`, `src/registry/heap_core.rs:1307-1365`, `src/registry/heap_core.rs:2033-2048`; реализация `src/alloc_core/magazine_bitmap.rs:62-141`.

**Наблюдение.** На каждом magazine hit после чтения hot owner-private `Tcache` вычисляется segment base и offset, затем делается read-modify-write байта в отдельном bitmap (`clear_magazine`). На каждом own-thread free аналогично читаются и обновляются segment metadata. Bitmap занимает 32 КиБ на сегмент и индексируется по адресу блока: при churn по многим сегментам это добавляет TLB/cache working set и лишает magazine hit свойства «только hot TLS/Tcache». Сам `Tcache` ограничен 16 указателями на класс, поэтому заменённая bitmap-ом проверка была строго bounded.

**Механизм ускорения.** Провести дизайн без per-hit segment write. Практичные варианты: (a) вернуть owner-private scan `slots[0..count]` для own-thread double-free и сканировать все непустые magazine только на редком ring drain; (b) небольшой owner-private open-addressed set только для реально закэшированных pointers, размером порядка общего числа magazine entries, обновляемый на push/pop без обращения к segment; (c) хранить компактный tag/state рядом с tcache pointer и проверять remote duplicates пакетно при drain. Критерий выбора — magazine alloc hit не должен касаться segment metadata вообще.

**Ожидаемый эффект.** Высокий для главного small-object churn path: минус вычисление base/offset и случайный RMW в 32-КиБ области; особенно заметно при множестве активных сегментов и классов. Дополнительно освобождается 32 КиБ metadata на каждый small segment, улучшается payload density и TLB locality. Для `count` 1-3 линейный hot-L1 scan может быть дешевле одной удалённой bitmap операции.

**Риски.** Bitmap закрывает cross-thread duplicate-free окна; простое удаление без эквивалентного точного membership oracle недопустимо. Полный scan всех 49×16 slots на каждом drained entry тоже может стать хуже fan-in сценария. Owner-private hash set добавляет собственные collision/tombstone случаи и должен быть allocation-free/reentrancy-safe.

**Как проверить (не выполнялось).** A/B для вариантов bitmap, bounded scan и fixed hash set: 16/64/256/4096-byte churn, 1-64 active segments, magazine depths 1/3/8/16; cycles, instructions, L1/LLC/TLB misses. Отдельно remote-drain-heavy сценарий. Все regression cases вокруг magazine/ring duplicate free и refill window прогнать под loom/miri после реализации.

## P1 — крупные архитектурные ускорения

### 4. Сделать записи LockFreeRegion сублинейными по числу страниц и без 64 Arc bumps

**Файлы и строки:** `src/concurrent/lock_free_region.rs:19-24`, `src/concurrent/lock_free_region.rs:26-97`, `src/concurrent/lock_free_region.rs:285-300`, `src/concurrent/lock_free_region.rs:321-362`, `src/concurrent/lock_free_region.rs:373-398`.

**Наблюдение.** Каждая insert/remove сначала клонирует `Snapshot.pages`, то есть выполняет `O(P)` копирований и atomic refcount increments для всех страниц. Затем touched page клонируется целиком: до 64 `SlotState`, а для occupied slots каждый clone делает `Arc::clone`, то есть до 64 дополнительных atomic RMW. Запись также делает минимум две новые heap allocations (page Vec и Snapshot/Vec storage). Поэтому writer cost и contention allocator/refcounts растут с capacity, хотя меняется один slot.

**Механизм ускорения.** Заменить плоский `Vec<Arc<Page>>` на persistent radix tree/chunked two-level page table: публикация записи копирует только путь `O(log_B P)` из узлов большой ветвистости. Более радикально — использовать уже имеющуюся epoch-схему: stable boxed page table, per-slot atomic pointer+generation и epoch reclamation, оставив mutex только для free-list/growth. Это даёт `O(1)` изменение slot после нахождения страницы и не клонирует соседние `Arc<T>`.

**Ожидаемый эффект.** Многократный выигрыш writes для больших regions и резкое уменьшение allocation/refcount traffic; reads сохраняют lock-free lookup, radix добавит 1-2 зависимых чтения. На маленьких read-mostly regions текущая простота может быть быстрее.

**Риски.** Persistent tree усложняет reclamation и ухудшает read locality; epoch-вариант требует аккуратного ABA/generation proof и меняет возвращаемый lifetime/Arc contract. Этот модуль deprecated/research-tier, поэтому инвестицию стоит привязать к реальному пользователю API.

**Как проверить (не выполнялось).** Write/read mix 0.1/1/10/50% при `P={1,16,1024,65536}`, считать allocations/op и Arc atomic operations, p99 writer latency, read regression. Loom/miri для publish/reclaim, stale handles и concurrent remove/get.

### 5. Заменить Mutex<Vec> remote-free очередь EpochRegion и сохранять ёмкость при drain

**Файлы и строки:** `src/concurrent/epoch_region.rs:122-135`, `src/concurrent/epoch_region.rs:217-247`, `src/concurrent/epoch_region.rs:365-395`.

**Наблюдение.** Каждый успешный remote remove после slot CAS и глобального `len.fetch_sub` берёт один `remote_free: Mutex<Vec<u32>>`; все producers shard-а сериализуются. Owner на drain делает `mem::take`, оставляя очередь с capacity 0, поэтому следующий burst снова reallocates Vec. Owner затем повторно делает Acquire generation load каждого уже проверенного reusable index.

**Механизм ускорения.** Минимальный вариант — swap с owner-owned scratch Vec и возвращать очищенный buffer под mutex, сохраняя capacity. Основной вариант — bounded MPSC index ring или intrusive atomic batch-list: producer делает один reserve/publish без mutex/allocator, owner batch-drain. `len` можно шардировать/накапливать локально, если exact instantaneous `len()` не является обязательным контрактом.

**Ожидаемый эффект.** Высокий при many-to-one remove: исчезают mutex convoy и повторные allocations; owner drain становится последовательным проходом contiguous indices. Снижается interference с insert/remove owner-а.

**Риски.** Full-queue policy должна быть lossless либо иметь fallback: потеря index навсегда уменьшает capacity. Нужны wraparound и publish-order гарантии. Удаление defensive generation check допустимо только после доказательства всех producers.

**Как проверить (не выполнялось).** 1 owner + 1/2/4/8/16 remote removers, burst/steady/owner-paused; throughput, mutex waits, allocations, CAS failures, p99. Loom для full/wrap/reserved-not-published; проверка `len`, free-list uniqueness и generation saturation.

### 6. Сделать TLS binding ShardedRegion привязанным к экземпляру, а не процессу

**Файлы и строки:** `src/concurrent/sharded_region.rs:62-71`, `src/concurrent/sharded_region.rs:91-137`, `src/concurrent/sharded_region.rs:250-325`, `src/concurrent/sharded_region.rs:400-412`.

**Наблюдение.** `MY_SHARD`/`ERASED_GUARD` едины на thread для всех `ShardedRegion` instances. Если cached id находится в диапазоне нового region, `claim_or_get_shard` возвращает его без claim токена этого instance. Это концентрирует одинаковые threads на одинаковых shard ids во всех regions и не даёт второму instance независимо балансировать/закреплять ownership; `remove` также определяет owner только сравнением общего TLS id. При нескольких regions это способно вернуть mutex contention, ради устранения которого существует sharding.

**Механизм ускорения.** TLS small-map из `(region_instance_id, shard_id, guard)` с inline capacity для типичного 1-2 instances; instance id — стабильный уникальный token, не адрес без generation. Альтернатива — явный `ShardBinding` handle, передаваемый caller-ом, что убирает TLS lookup и делает thread-per-core topology явной.

**Ожидаемый эффект.** Радикальный для приложений с несколькими активно изменяемыми ShardedRegion: независимое распределение и восстановление shard-local writer path вместо случайного shared/remote path. Для одного instance — небольшая цена дополнительной проверки id или нулевая при explicit binding.

**Риски.** TLS map не должна аллоцировать рекурсивно в чувствительном контексте; lifecycle id/ABA при drop/recreate; больше guards на thread. Изменение owner detection должно сохранить корректность remote eviction.

**Как проверить (не выполнялось).** 2-16 regions одинакового и разного shard_count, thread-per-core writes/removes; распределение операций по shards, mutex wait, remote queue rate. Lifecycle тесты drop/recreate и thread exit; loom для token release/adoption.

### 7. Убрать глобальный atomic diagnostic storm и линейный scrape stats

**Файлы и строки:** `src/alloc_core/remote_free_ring.rs:607-613`, `src/registry/heap_core.rs:1823-1845`, `src/registry/heap_registry.rs:972-999`, `src/registry/heap_registry.rs:1027-1049`, вызовы `src/global/sefer_alloc.rs:291-310`.

**Наблюдение.** Глобальные overflow/retry counters обновляются atomic RMW именно во время максимального contention; retry loop умножает обновления. `stats()` для hit counters проходит все slots до process high-water mark и делает по Acquire gate + Relaxed counter load, даже если live heaps мало. Частый metrics scrape создаёт `O(MAX_HEAPS)` cache pollution.

**Механизм ускорения.** Per-heap/per-CPU saturating plain counters с редкой публикацией; инкрементировать один раз на логическое событие после завершения retry, не на попытку. Для scrape поддерживать intrusive list/bitmap initialised slots либо hierarchical aggregates, обновляемые при claim/recycle и периодически owner-ом. Разделить exact slow snapshot и cheap approximate telemetry API.

**Ожидаемый эффект.** Большой только при overflow storms или частом scrape, но именно тогда убирается общий cache-line hotspot. Обычный small hit при `alloc-stats` off не меняется.

**Риски.** Approximate counters меняют семантику; per-CPU aggregation сложна при migration. Live-list lifecycle должен быть ABA-safe. Exact aggregation при одновременной публикации требует явно документированной консистентности.

**Как проверить (не выполнялось).** Saturated fan-in вместе с 1-10 kHz stats scraper; cache-to-cache transfers, locked ops, allocator throughput. Проверить монотонность/допустимую погрешность и claim/recycle races loom-тестом.

## P2 — адресные, но потенциально заметные улучшения

### 8. Уменьшить inline footprint HeapSlot overflow ring и улучшить locality записи

**Файлы и строки:** `src/registry/heap_slot.rs:217-340`, `src/registry/heap_overflow.rs:172-215`, `src/registry/heap_overflow.rs:280-305`, `src/registry/heap_overflow.rs:355-379`.

**Наблюдение.** Native `HeapOverflow` содержит 2048 `AtomicUsize` bases и 2048 `AtomicU32` packed inline в каждом из 4096 registry slots — около 24 КиБ на slot, около 96 МиБ виртуального metadata до прочих полей. SoA layout заставляет consumer читать две далеко расположенные arrays; claim/use конкретного slot может fault-in значительный metadata диапазон. Ring фактически нужен только после saturation per-segment ring.

**Механизм ускорения.** Ленивая sidecar reservation через прямой OS aperture при первом overflow либо общий slab overflow rings для только claimed/active heaps. Для locality рассмотреть AoS entry с одним 128-bit publish word там, где lock-free atomic128 доступен, или компактный 64-bit encoding: segment id вместо полного base + packed offset/class, с generation для recycle safety. Это сокращает entry с 12/16 до 8 bytes и делает drain одним последовательным load.

**Ожидаемый эффект.** Существенное снижение virtual/page-table/committed metadata и лучшее drain locality; compact encoding до ~2× уменьшает ring traffic. На workload без overflow throughput почти не изменится, но memory footprint процесса и cost большого числа heaps уменьшатся.

**Риски.** Lazy sidecar нельзя выделять через global allocator; OS allocation на первом overflow увеличит tail latency. Segment id recycling требует generation, иначе ABA. Atomic128 не lock-free на всех targets; нужен portable fallback.

**Как проверить (не выполнялось).** RSS/page-table size и minor faults для 1/64/1024/4096 claimed heaps; overflow drain bandwidth и cache misses. Stress recycle/generation/wrap и target matrix atomic capabilities.

### 9. Сделать aligned size-class lookup строго O(1)

**Файлы и строки:** `src/alloc_core/size_classes.rs:161-181`; вызовы hot paths `src/registry/heap_core.rs:848-860`, `src/registry/heap_core.rs:1152-1161`, `src/registry/heap_core.rs:1731-1739`.

**Наблюдение.** Для align > 16 `class_for` начинает с `SIZE2CLASS`, затем выполняет modulo/divisibility loop до 49 классов. Обычно это 0-3 шага, но функция вызывается и на alloc, и на dealloc, включая foreign free. Modulo переменным table value дороже обычного lookup.

**Механизм ускорения.** Поскольку `Layout` alignment — степень двойки, сгенерировать const table `CLASS_BY_ALIGN_LOG2_AND_SIZE_BUCKET` или для каждого класса хранить next-compatible class по alignment log2. Lookup: `trailing_zeros(align)` + один/два table reads, без division/loop. Ограничить таблицу supported small alignments; остальные сразу Large/None.

**Ожидаемый эффект.** Умеренный, но стабильный для align 32-16384 и async/SIMD workloads; worst-case становится O(1). Для align <=16 fast path не меняется.

**Риски.** Увеличение read-only table/I-cache; генератор должен точно отражать divisibility и `SMALL_MAX`. Ошибка даёт misalignment, то есть критическую корректность.

**Как проверить (не выполнялось).** Исчерпывающе сравнить новую table со старым reference для всех size 1..SMALL_MAX и всех допустимых power-of-two align; iai instruction counts и Criterion по alignment distribution.

### 10. Избежать coarse RwLock для независимых SyncRegion операций

**Файлы и строки:** `crates/region/src/sync_region.rs:31-65`, `crates/region/src/sync_region.rs:68-122`.

**Наблюдение.** Один `RwLock<Region<T>>` сериализует все writes и блокирует весь store на insert/remove/clear. Даже reads одного handle участвуют в общей lock cache line. `get_cloned` дополнительно клонирует `T`, что может доминировать для больших значений.

**Механизм ускорения.** Для concurrent-default API предоставить sharded slotmap: handle несёт shard id, каждый shard имеет свой lock; `len` — sharded counters, `clear` — ordered lock all. Для read-heavy больших `T` дать closure/guard API на shard, чтобы не клонировать значение. Уже существующий experimental `ShardedRegion` показывает маршрутизационную модель, но production-safe вариант может остаться на обычных locks без epoch/unsafe сложности.

**Ожидаемый эффект.** Почти линейное масштабирование независимых writes до числа shards и меньше reader/writer contention. Для одного thread/малого region появится routing overhead.

**Риски.** Меняется layout handle/API совместимость; `clear`, iteration и exact len становятся многолоковыми операциями. Нужен фиксированный порядок захвата locks против deadlock.

**Как проверить (не выполнялось).** Read/write mixes и uniform/hotspot keys на 1-64 threads, разные shard counts; throughput, fairness и p99. Проверить stale handles, clear/iteration consistency и poison recovery.

## Рекомендуемый порядок экспериментов

1. Сначала ограничить retry storm (предложение 2): минимальная правка политики может убрать катастрофический tail без смены структуры.
2. Затем A/B membership без MagazineBitmap (3), потому что это непосредственно главный allocation hit path.
3. После этого реализовать per-class segment availability index (1): самый большой асимптотический выигрыш, но самый высокий correctness risk.
4. Experimental region tier измерять отдельно от production allocator; начинать с дешёвого buffer reuse для `EpochRegion` (5), затем решать судьбу CoW архитектуры (4).

Любое GO/NO-GO решение следует принимать отдельно для throughput, p99 latency, RSS/page tables и correctness tooling: оптимизация одного измерения здесь часто переносит стоимость между owner fast path, remote producer и cold reclamation.
