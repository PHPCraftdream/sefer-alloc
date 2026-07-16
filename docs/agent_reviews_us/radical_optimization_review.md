# Радикальное performance-review `sefer-alloc`

Дата: 2026-07-14  
Режим: автономный read-only аудит текущего дерева; сборки, тесты, бенчмарки,
профилировщики и fuzzing не запускались. Единственная запись — этот отчёт.

## 1. Итоговый вердикт

Текущий allocator уже хорошо оптимизирован для узкого headline-сценария:
однопоточный warm churn небольшого фиксированного working set. Последняя
сохранённая таблица показывает примерно 29.0/28.4/31.4/32.1 ns на пару
alloc+free для 16/64/256/1024 B; при 64 B и выше это уже быстрее mimalloc,
особенно на 1024 B (`docs/ALLOC_BENCH.md:43-57`). Поэтому следующая заметная
ступень не получится из ещё одного локального `inline`, увеличения magazine или
сокращения пары арифметических инструкций. Большие оставшиеся рычаги находятся
на границах текущего дизайна:

1. Поток, который только освобождает чужой блок, сегодня всё равно bind'ит
   полноценный per-thread heap и при первом materialization резервирует/commit'ит
   4 MiB primordial segment. Это можно полностью убрать с `GlobalAlloc::dealloc`.
2. На Windows «lazy, untouched» registry не является дешёвым по commit charge:
   `aligned_vmem` commit'ит весь `Registry`, включая примерно 96 MiB inline
   `HeapOverflow`, одним вызовом. Текущий probe измеряет Working Set, но не
   `PagefileUsage`/Private Commit, поэтому объявленный успех RAD-1 видит лишь RSS.
3. Между 258,752 B и 262,144 B есть резкий архитектурный cliff: запрос чуть
   больше `SMALL_MAX` получает отдельный 4 MiB span. Для 256 KiB это до ~16x
   лишнего commit/VA и один `SegmentTable` slot на объект.
4. При заполненном remote ring один логический free с живым, но не успевающим
   owner'ом может сделать 8,193 попытки, причём каждая неудача выполняет два
   contended atomic RMW только ради диагностики. Уже существующий heap-level
   overflow используется лишь после этой бури, хотя его надо пробовать сразу.
5. `find_segment_with_free` остаётся O(S) по high-water числу segment slots и
   одновременно опрашивает remote rings. Текущий judge имеет всего три сегмента;
   ещё один hint закономерно ничего не дал. Для S=64..1024 нужен полный
   per-class directory плюс dirty-segment publication, а не четвёртый hint.
6. Внутренние batch-примитивы уже есть, но scalar `GlobalAlloc` повторяет TLS
   resolve, classification, routing и M2 bookkeeping на каждом объекте. Явный
   batch/scoped-local API имеет самый высокий потолок для DBMS/ECS/arena
   потребителей, хотя ничего не даст коду, который его не использует.

Главный практический вывод: сначала надо исправить измерительные слепые зоны и
архитектурные cliff'ы. Дальнейшая настройка текущего 29 ns churn-пути без этого с
высокой вероятностью снова даст «Ir лучше, Windows wall-clock хуже/шумнее».

## 2. Семь наиболее перспективных направлений

| Ранг | Направление | Где проявится | Ожидаемый эффект для затронутого сценария | Уверенность |
|---:|---|---|---|---|
| 1 | Не bind'ить heap на dealloc-only потоке | producer/consumer, fan-in, ephemeral workers | 10–1000x ниже first-free latency при mint нового slot; минус 4 MiB Windows commit на каждый предотвращённый materialized heap | Высокая по механизму, средняя по частоте workload |
| 2 | Chunked registry + lazy `HeapOverflow` sidecar | startup, много/мало heaps, Windows | примерно >95% меньше fixed first-allocation commit; ~125 MiB сейчас против порядка 0.5–2 MiB первого chunk | Высокая |
| 3 | Medium allocator для ~256 KiB–2 MiB | medium objects, buffers, caches | ориентир 3–10x throughput после amortization и 4–16x меньше commit/VA около cliff | Высокая по cliff, средняя по выбранному дизайну |
| 4 | Overflow-first remote free, без counted retry storm | fan-in, paused/starved owner, p99 | >99.9% меньше бесполезных failed probes в worst case; от 2–20x до порядков величины на saturated path | Высокая |
| 5 | Reserve-only + chunked commit small segments на Windows | sparse heaps, thread fan-out, empty→reuse | 8–32x меньше commit на слабо используемый heap; возможны десятки процентов first-alloc latency, но dense stream может не ускориться | Средне-высокая |
| 6 | Полный per-class segment directory + dirty queue | 64–1024 segments/heap | 5–50x для refill-miss компонента; общий workload выигрыш зависит от доли misses | Средняя |
| 7 | Явный batch API / scoped local heap handle | пользователи, способные batch'ить | ориентир 1.5–3x на alloc/free storm 16–256 B; ноль для обычного scalar API | Высокая по механизму, средняя по adoption |

Диапазоны выше — цели для A/B, а не обещания. Они относятся только к workload,
в котором соответствующий механизм действительно доминирует.

## 3. Ранжированный backlog

| Приоритет | Идея | Эффект | Уверенность | Основной риск |
|---|---|---:|---:|---|
| P0 | Dealloc resolver без TLS bind для unbound/TORN thread | Очень высокий, workload-specific | Высокая | корректно обработать fallback и teardown |
| P0 | Chunked `Registry`; вынести `HeapOverflow` из каждого slot | Очень высокий по commit/startup | Высокая | стабильность адресов и M5/reentrancy |
| P0 | Medium size classes/page-run allocator | Очень высокий на 256 KiB–2 MiB | Средне-высокая | fragmentation и lifecycle |
| P0 | Сначала heap overflow, потом retry; uncounted probes | Очень высокий на saturation/p99 | Высокая | liveness и семантика counters |
| P1 | Частичный commit payload на Windows | Высокий по commit, средний по latency | Средне-высокая | дополнительные syscalls на dense growth |
| P1 | Per-class nonempty directory + remote dirty bitmap/queue | Высокий при большом S | Средняя | lost-wakeup/table-id reuse |
| P1 | Batch API или thread-bound scoped handle | Высокий для adopters | Средне-высокая | новый публичный контракт |
| P1/P2 | Opt-in `trusted-fast` для unsafe `GlobalAlloc` boundary | Средний/высокий | Средняя | ослабление документированной M2 defence |
| P2 | Разнести `HeapOverflow` head/tail, AoS payload | Средний только на overflow | Средняя | ordering/wraparound |
| P2 | PGO/code layout и native Windows bisection | 3–15% потенциально | Средняя | platform/profile dependence |
| P3 | Relaxed reservation CAS на weak-memory targets | 0% на x86, возможно 5–15% remote push на ARM | Низко-средняя | memory-model ошибка |

## 4. Детальные находки

### P0-1. `dealloc` не должен создавать heap на потоке, который только освобождает

**Точные места.** `SeferAlloc::dealloc` всегда вызывает `current_heap`
(`src/global/sefer_alloc.rs:463-490`); `current_heap` делегирует alloc-oriented
resolver'у (`src/global/sefer_alloc.rs:268-288`). При `LOCAL == null`
`current_for_alloc_with_config` вызывает bind slow path
(`src/global/tls_heap.rs:421-445`), а тот делает `HeapRegistry::claim_with_config`
(`src/global/tls_heap.rs:477-486,543-569`). Первый claim slot'а materialize'ит
`HeapCore` (`src/registry/heap_registry.rs:119-155,196-224`). `AllocCore::new`
сразу создаёт primordial segment (`src/alloc_core/alloc_core.rs:509-528,632-646`),
а bootstrap резервирует ровно один 4 MiB `SEGMENT`
(`src/alloc_core/bootstrap.rs:33-46`). При recycle heap остаётся целиком в slot и
переиспользуется (`src/registry/heap_registry.rs:268-271`).

**Почему это дорого.** Поток, получивший один pointer через channel и вызвавший
только `drop`, не имеет собственных allocations от SeferAlloc и потому не
нуждается в `AllocCore`, tcache, segment table или primordial payload. Но первый
`dealloc` mint'ит всё это лишь затем, чтобы `dealloc_routing` прочитал owner
чужого segment и сделал ring/TFS push. На Windows `Segment::reserve` вызывает
`vmem::reserve_aligned` для всех 4 MiB (`src/alloc_core/os.rs:120-134`), а
Windows path commit'ит весь usable span (`crates/vmem/src/lib.rs:361-420`). При
всплеске N ранее не materialized concurrent workers это до `N * 4 MiB` process
commit, который остаётся привязан к high-water числу materialized slots.

**Предлагаемое изменение.** Ввести отдельный dealloc resolver:

- `LOCAL` содержит реальный heap: оставить текущий owner-private fast path.
- `LOCAL == null`: не claim'ить slot. Поскольку этот поток ещё ничего не
  alloc'ировал из собственного heap, валидный pointer по контракту unsafe
  `GlobalAlloc::dealloc` является foreign/fallback-owned. Прочитать stamped
  owner из live segment header и сразу выполнить small-ring либо large-TFS push.
- `TORN`/TLS unavailable: тот же route-only path; не брать fallback lock только
  ради маршрутизации чужого pointer.
- Fallback не требует особой схемы данных: его blocks уже stamped стабильным
  `FALLBACK_TFS`, и текущая документация прямо говорит, что они являются
  обычными segment blocks с нормальным cross-thread routing
  (`src/global/fallback.rs:31-37,81-101`). Нужен лишь внутренний helper, который
  не раскрывает приватный static наружу.
- Сохранить нынешний safe `HeapCore::dealloc` без изменений. Новый shortcut
  должен быть внутренним unsafe entry point исключительно для валидного
  `GlobalAlloc` pointer.

**Ожидаемый выигрыш.** Когда без изменения mint'ился новый slot: убрать 4 MiB
reserve/commit, metadata initialization, registry CAS/TLS guard wiring и
teardown; first-free может стать на один–три порядка дешевле. На уже bound
потоке выигрыш нулевой. На recycled, уже materialized slot экономится bind/TLS
lifecycle, но не новый OS span, поэтому эффект меньше.

**Почему выигрыш может не проявиться.** Обычный thread-pool сначала alloc'ирует
служебные объекты и уже имеет LOCAL; headline single-thread churn тоже всегда
bound. Нужны реальные dealloc-only workers, а не искусственный same-thread loop.

**Риски корректности.** Нельзя применять shortcut к произвольному safe API:
чтение header у dangling/garbage pointer может fault. Надо обработать Small,
Large, fallback, owner-exited state и TORN одинаково с нынешним foreign slow
path; не допустить локального direct free без `&mut HeapCore`; не ослабить
double-free semantics safe `HeapCore` surface.

**План измерения.** Отдельный process-per-sample benchmark с реально
установленным allocator'ом: owner alloc'ирует B={1,64,400,4096} blocks, затем
T={1,8,64,512,4096} ранее unbound persistent или ephemeral workers только
освобождают. Измерять first-dealloc и steady dealloc p50/p99, process
PrivateUsage/PagefileUsage, Working Set, `heaps_claimed_high_water`, число
segment reservations и retained commit после join. Контроль: те же workers
сначала делают одну собственную allocation, чтобы принудительно оставить
старый bound path.

### P0-2. Registry является ~125 MiB Windows commit, а не только дешёвым VA

**Точные места.** `Registry` содержит inline `[HeapSlot; 4096]`
(`src/registry/bootstrap.rs:172-209`). Размер округляется до страниц и целиком
передаётся `reserve_aligned` (`src/registry/bootstrap.rs:302-315,399-412`). Каждый
slot inline содержит `HeapOverflow` (`src/registry/heap_slot.rs:341-357`), а
native `HeapOverflow` имеет capacity 2048 и два atomic массива общей payload-
стоимостью 24 KiB/slot, то есть 96 MiB на 4096 slots
(`src/registry/heap_overflow.rs:109-175,193-215`). До его добавления комментарии
фиксировали примерно 7,488 B/slot и ~29 MiB остального registry
(`src/registry/bootstrap.rs:295-299,514-520`; `examples/first_alloc_process.rs:68-75`).

Windows implementation сначала reserve'ит over-region, но затем вызывает
`winapi_virtual_commit(base, size)` для **всего точного size**
(`crates/vmem/src/lib.rs:361-420,516-526`). Untouched pages могут не войти в
Working Set, однако process commit charge уже зарезервирован. Поэтому нижняя
оценка текущего first-allocation commit — примерно `29 MiB + 96 MiB`, плюс
alignment/control padding, то есть порядка 125 MiB.

**Почему прошлое измерение этого не увидело.** Windows probe возвращает только
`working_set_size`, хотя его FFI struct уже содержит `pagefile_usage`
(`examples/first_alloc_process.rs:121-160`). Печатаются RSS, peak RSS и latency,
но не commit/private bytes (`examples/first_alloc_process.rs:299-318`). Runner
также агрегирует только RSS (`scripts/first-alloc-bench.mjs:138-190`). Поэтому
сохранённый вывод, что нетронутый overflow «ничего не стоит», верен для RSS, но
не для Windows commit limit/pagefile pressure.

**Предлагаемое изменение.** Два взаимодополняющих слоя:

1. Заменить monolithic array на, например, 64 `AtomicPtr<RegistryChunk>` по 64
   slots. Chunk публиковать CAS'ом, получать напрямую по `id >> 6`, никогда не
   освобождать; адрес каждого `HeapSlot` остаётся process-stable.
2. Убрать `[AtomicUsize; 2048] + [AtomicU32; 2048]` из каждого slot. Хранить
   nullable OS-allocated sidecar, создаваемый M5-clean direct VM path только при
   первом реальном overflow. Допустим маленький inline emergency tier на
   32–64 entries, затем sidecar. Remote producer получает sidecar через stable
   atomic pointer; allocation sidecar не должна рекурсивно входить в allocator.

При минимально инвазивном варианте даже chunking без sidecar снижает стартовый
commit до порядка 64 * 32 KiB ~= 2 MiB. С sidecar первый chunk ближе к старому
64 * 7.5 KiB ~= 0.5 MiB.

**Ожидаемый выигрыш.** >95% снижения fixed Windows commit на first allocation;
меньше miri metadata explosion; меньший TLB/page-table и dump footprint. Steady
small-object churn почти наверняка не изменится. First alloc latency может
улучшиться, если commit-accounting большого range заметен, но это надо мерить.

**Риски корректности.** Адрес slot'а и его `thread_free` должен быть неизменным
через recycle; chunk никогда нельзя перемещать/освобождать. Нужны корректные
Acquire/Release publication и rollback при VM OOM. Stats/high-water walks должны
переживать отсутствующие chunks. Lazy overflow publication должна закрыть гонку
двух первых producers и не потерять entry при OOM. Miri path нельзя снова
раздуть eager allocation.

**План измерения.** Fresh process с 0/1/8/64/4096 simultaneously live heaps;
на Windows снимать WorkingSet, PeakWorkingSet, `PagefileUsage`/PrivateUsage и
system Commit до/после первого alloc. Отдельно first overflow materialization,
T={1,8,32} producers, overflow occupancy и latency. Контроль: Unix RSS/VA и
miri host RSS. Сравнивать не только `rss_after_1_heap`, но и commit delta.

### P0-3. Medium cliff: 258,752 B — Small, 262,144 B — отдельные 4 MiB

**Точные места.** Есть 49 small classes до ~253 KiB
(`src/alloc_core/size_classes.rs:11-42,84-110`); `class_for` возвращает `None`
сразу выше `SMALL_MAX` (`src/alloc_core/size_classes.rs:162-200`). Large path
вычисляет минимум один 4 MiB span независимо от того, насколько мало превышен
порог (`src/alloc_core/alloc_core_large.rs:22-75`). В одном heap максимум 1024
registered segment slots (`src/alloc_core/segment_table.rs:60-77`).

Текущий IAI judge намеренно использует 258,752 B и прямо предупреждает, что
literal 256 KiB уже уйдёт в dedicated Large (`benches/perf_gate_iai.rs:340-375`).
Large wall-clock bench проверяет только 4/16/64 MiB
(`benches/large_realloc.rs:174-208`). Neighbour-pressure realloc начинается с
512 KiB, но для Sefer это уже один dedicated span, внутри которого дальнейший
рост выполняется in-place; сам cliff и множество независимых medium objects
там не измеряются (`benches/large_realloc.rs:246-268`).

**Почему это дорого.** 262,144-byte object получает 4 MiB committed/reserved
span и отдельную table registration. Для batch из 1024 live objects это до
~4 GiB usable spans и исчерпание table, хотя payload лишь 256 MiB. Large cache
из восьми entries помогает повторному reuse небольшого working set, но не
первой волне множества одновременно live objects.

**Предлагаемое изменение.** Предпочтительный вариант — отдельный medium
page-run layer (например 64 KiB/страничные runs) внутри 4 MiB segments с
per-class/size-segregated partial lists. Более дешёвый первый эксперимент —
добавить точные classes 256/320/384/512/768 KiB/1 MiB и использовать нынешний
small substrate. Для 256 KiB один segment вместит около 15 объектов вместо
одного. `SIZE2CLASS` вырастет (до ~64 KiB при `SMALL_MAX=1 MiB`), что приемлемо
для эксперимента, но arbitrary medium sizes потребуют контроля fragmentation;
page-run design масштабируется лучше.

**Ожидаемый выигрыш.** Около cliff — 4–16x меньше commit/VA и table slots;
3–10x throughput после amortization OS reservation/registration; ниже allocator
p99. На существующих 16–1024 B таблицах эффект нулевой. Если workload всегда
имеет один medium object и large cache hit, новый слой может не выиграть.

**Риски корректности.** Alignment до MiB, mixed-size fragmentation, exact
dealloc classification, page map, decommit/recommit, cross-thread free,
realloc grow/shrink и segment-id recycling. Простое поднятие `SMALL_MAX` без
новых free-segment indexes может усилить O(S) scan, поэтому P0-3 желательно
сочетать с P1-6.

**План измерения.** Независимые alloc/free, а не realloc одного span: sweep
240/252/253/255/256/257/320/384/512/768 KiB/1/1.5/2/4 MiB; live cardinality
1/8/64/1024; cold, repeated reuse, random lifetime, same- и cross-thread.
Снимать ns/op, OS reservations, committed/private bytes, RSS после touch,
internal fragmentation, table occupancy и p99. Обязательная точка контроля —
текущие 258,752 B и 262,144 B по обе стороны cliff.

### P0-4. Remote-free retry storm надо инвертировать: overflow прежде spin

**Точные места.** Native budget равен 8,192
(`src/registry/heap_core.rs:204-248`). После первой неудачи при live owner код
8,192 раза делает `spin_loop` + `ring.push`, и только затем пробует heap-level
overflow (`src/registry/heap_core_xthread.rs:388-439`). Каждый `push` на полном
ring выполняет два `fetch_add(Relaxed)` — per-segment и global diagnostic — и
возвращает ошибку (`src/alloc_core/remote_free_ring.rs:634-668`). Значит один
логический free может сделать 8,193 full checks и 16,386 locked RMW только на
counters, помимо cursor traffic. Production bench сам фиксирует, что при N=400
144 overflow blocks и 8,192 retries делают samples CPU-bound, а N=2,000 занимал
секунды (`benches/heap_fanin_production.rs:124-140`).

При этом уже есть heap-level queue на 2,048 entries, но она вызывается лишь
после spin (`src/registry/heap_overflow.rs:109-175,280-305`).

**Предлагаемое изменение.** Политика:

1. Одна обычная `RemoteFreeRing::push`.
2. При full немедленно одна попытка `HeapOverflow::push`.
3. Только если обе capacity заполнены — bounded adaptive retry/park: повторно
   проверить segment ring, затем overflow; live-owner probe определяет, есть ли
   смысл ждать.
4. Разделить `try_push_uncounted` и логический overflow counter. Failed polling
   attempts не должны делать два global/segment RMW. Production без diagnostic
   feature должен либо считать только один event, либо компилировать counters
   из saturation loop.

Это использует уже оплаченную capacity 2048 как настоящий второй tier, а не как
послесловие к 8,192 попыткам.

**Ожидаемый выигрыш.** Worst-case failed attempts сокращаются с 8,193 до 1–2,
то есть более чем на 99.9%; saturated/starved dealloc latency — от нескольких
раз до порядков величины. При никогда не полном ring добавочного overhead нет.
При очень активном owner новый порядок может поместить entry в heap overflow,
который дренируется реже, хотя краткий spin успел бы попасть в освободившийся
segment ring; это основной trade-off.

**Риски корректности.** Нельзя терять entry при одновременном заполнении обоих
tiers; owner обязан регулярно drain'ить overflow; counters поменяют смысл и
потребуют обновить judges; порядок reclaim может измениться. Нужны модели гонок
owner drain vs producer reservation, exited/reclaimed owner и wraparound.

**План измерения.** Persistent threads и barriers, allocation/setup вне timer.
T={1,2,8,32,64}, burst={256,400,1k,100k,1M}, owner active/slow/paused/exited;
p50/p99/max dealloc, CPU time/free, failed probes/logical free, occupancy обоих
tiers, lost/exhausted entries и time-to-reclaim после wake. Существующий
`heap_fanin_production` надо сохранить как correctness/stress контроль, но не
как чистый throughput judge: он создаёт, claim'ит, join'ит и recycle'ит threads
внутри каждого timed iteration (`benches/heap_fanin_production.rs:178-233,244-293`).

**Вторая фаза layout.** `HeapOverflow { tail, head, bases[], packed[] }` держит
producer-CAS tail и consumer-store head рядом и читает payload двумя далёкими
streams (`src/registry/heap_overflow.rs:193-215,280-305,366-390`). Основной
`RemoteFreeRing` уже разнёс head/tail по разным 64-byte lines
(`src/alloc_core/remote_free_ring.rs:404-451`). Повторить это для overflow и
проверить AoS entry `{base, packed}`. Ожидаемо 10–40% на реально занятом
overflow path, ноль на обычном churn. Риски — layout footprint, publication
ordering и wraparound.

### P1-5. Windows small segments надо reserve'ить отдельно от commit

**Точные места.** Любой `Segment::reserve` округляет до 4 MiB и вызывает общий
`vmem::reserve_aligned` (`src/alloc_core/os.rs:120-134`). Windows path commit'ит
весь exact usable range (`crates/vmem/src/lib.rs:361-420`). Каждый свежий
per-thread `AllocCore` создаёт primordial 4 MiB segment ещё до первого 16-byte
block (`src/alloc_core/alloc_core.rs:509-528,632-646`;
`src/alloc_core/bootstrap.rs:41-46`). После decommit reuse current carve снова
recommit'ит **весь** `[small_meta_end, 4 MiB)` payload
(`src/alloc_core/alloc_core_small.rs:847-883,948-976`).

**Почему это дорого.** На Windows sparse heap с одним маленьким object платит
4 MiB commit charge. 64 одновременно впервые materialized heaps — около 256 MiB
только primordial spans, сверх monolithic registry. RSS probe этого почти не
показывает, пока payload не touched. Empty→reuse также recommit'ит почти 4 MiB,
даже если затем нужны только 16 magazine blocks.

**Предлагаемое изменение.** Добавить в `aligned-vmem` Windows-specific
reserve-only handle и explicit commit API. Для small segment сначала commit'ить
metadata и стартовый payload chunk (например 128–512 KiB), хранить owner-private
`committed_end`, а `carve_batch` расширяет commit до chunk boundary. После
decommit reuse commit'ить только требуемый run/chunk. Возможен geometric policy:
малый первый chunk для sparse heap, затем 256 KiB → 1 MiB → остаток, чтобы dense
stream не получил 16 syscalls на segment. Unix можно оставить на anonymous mmap
с demand paging.

**Ожидаемый выигрыш.** 8–32x меньше commit на sparse heap и empty→small reuse;
меньше first-allocation latency и commit pressure при thread fan-out. Dense
sequential allocation может остаться без выигрыша или регрессировать из-за
дополнительных `VirtualAlloc(MEM_COMMIT)` calls — поэтому chunk policy является
частью performance-контракта, а не деталью.

**Риски корректности.** Любая metadata/payload write обязана быть внутри
committed range; cross-thread free не должен писать `next` в uncommitted block
(сейчас он пишет только ring entry, owner recommit/reclaim должен быть
упорядочен); decommit reset, pool reuse, large path и NUMA reservation требуют
разных policies. `committed_end` owner-private, но release/decommit boundaries
должны быть page-aligned и OOM должен оставлять segment в повторяемом состоянии.

**План измерения.** Windows fresh process: 1/8/64/512 heaps, по
1/16/1k/full-segment live objects, без touch и с touch. Commit, RSS, first-alloc,
number/time VirtualAlloc calls. Отдельно 10k empty→reuse cycles с последующим
использованием 1/16/full blocks. Контроль dense throughput обязан не ухудшиться
более заранее заданного порога; Unix должен быть нейтрален.

### P1-6. O(S) scan надо заменять полным directory, а не ещё одним hint

**Точные места.** `find_segment_with_free_impl` берёт high-water `count` и
проходит все `[0,count)` slots, включая NULL holes, читает kind, опрашивает
remote ring и BinTable (`src/alloc_core/alloc_core_small.rs:239-450`). `count`
является числом когда-либо записанных slots, а не live count
(`src/alloc_core/segment_table.rs:421-427`); `base_at` — отдельный pointer read
(`src/alloc_core/segment_table.rs:560-577`). Максимум — 1024 segments
(`src/alloc_core/segment_table.rs:60-73`). Refill правильно latch'ит
`free_exhausted`, поэтому scan выполняется один раз на refill, не на каждый
carved block (`src/alloc_core/alloc_core_small_magazine.rs:151-185,230-249`).

Текущий multisegment judge создаёт лишь три segments
(`benches/perf_gate_iai.rs:340-375`). Неудивительно, что ещё один per-segment
hint дал только -4.3/-6.6 Ir/op на двух multi-segment tests и практически ноль
на остальных (`docs/perf/IAI_BASELINE.md:1293-1314`). Это не опровергает
асимптотическую проблему; это опровергает маленький hint при S<=3.

**Предлагаемое изменение.** Полный exact owner-private directory:

- `SMALL_CLASS_COUNT * ceil(MAX_SEGMENTS/64)` nonempty bits: примерно
  `49 * 16 * 8 = 6.1 KiB` на heap.
- Ставить bit на transition BinTable head empty→nonempty; очищать при
  nonempty→empty; очищать все class bits при recycle конкретного table slot.
- Для remote entries отдельный shared dirty-segment bitmap/queue. Producer
  после успешного ring push Release-публикует table slot/generation как dirty;
  owner exchange'ит dirty words, drain'ит только эти segments и обновляет
  per-class directory. Хранить stable segment_id/generation в header.
- Ниже S=32/64 можно сохранить простой scan, чтобы не платить directory updates
  на типичном маленьком heap.

**Ожидаемый выигрыш.** При S=64–1024 refill miss вместо десятков/тысяч kind,
ring-tail и bin-head loads получает несколько word scans и один candidate.
Компонент может ускориться 5–50x; общий workload — от нуля при S<=3 до
многократного выигрыша при frequent misses. Directory updates способны
регрессировать current churn, поэтому threshold обязателен.

**Риски корректности.** Lost dirty wakeup, stale bit после table slot reuse,
generation ABA, segment release одновременно с поздним remote publication,
NUMA fallback order, decommit/pool transitions. Producer не должен писать
owner-private directory; только atomic dirty publication. Exact nonempty bits
нельзя превращать в best-effort hint, если scan fallback удаляется.

**План измерения.** Новый deterministic S={1,3,16,64,256,1024} harness с holes,
49 classes и controlled free distribution; отдельно remote dirty density
0/1/10/100%. Снимать refill cycles, loads/L1/LLC, directory update cost,
segment scan count, p99. Kill gate — текущие 16/64/256 B churn и S<=3 должны
остаться нейтральны. Correctness plan: producer publication до/после owner
exchange, release/reuse generation, full dirty word, wraparound.

### P1-7. Scalar `GlobalAlloc` не может заменить явный batch/scoped API

**Точные места.** Каждый scalar call снова разрешает TLS
(`src/global/sefer_alloc.rs:443-490`; `src/global/tls_heap.rs:394-445`), а alloc
классифицирует layout (`src/registry/heap_core.rs:707-744`). В то же время
`refill_class_bump` уже делает free-drain once, batch freelist drain и
`carve_batch` (`src/alloc_core/alloc_core_small_magazine.rs:138-262`), а
`refill_magazine_slow` пишет напрямую в tcache без промежуточной копии
(`src/registry/heap_core.rs:1567-1651`). Предыдущий план тоже признал internals
готовыми, но остановился на product gate
(`docs/perf/PERF_PLAN_2026-07-10-radical-audit-implementation-plan.md:271-293,508-518`).

**Почему это дорого.** Последний cold/bulk result остаётся 1.62–2.49x хуже
mimalloc на 16–256 B (`docs/ALLOC_BENCH.md:59-67`). Увеличение `TCACHE_CAP`
прямо измерено как регрессия; проблема не в глубине magazine, а в повторении
scalar orchestration и defensive metadata transitions.

**Предлагаемое изменение.** Experimental API одного из двух уровней:

- `unsafe alloc_batch_same_layout(layout, &mut [*mut u8]) -> usize` и
  `unsafe dealloc_batch_same_layout`; TLS и class один раз, alloc напрямую
  выдаёт runs/refills, dealloc группирует по segment без sorting для обычных
  contiguous runs.
- Safe `Pool<T>` либо thread-bound, `!Send` scoped `LocalHeap` handle, который
  hoist'ит TLS resolve/class и даёт scalar-looking методы внутри scope.

Не возвращать heuristic bulk mode в обычный `GlobalAlloc`: прежний streak-based
bypass был удалён, потому что менял hot path и не имел надёжного сигнала
(`src/registry/heap_core.rs:1262-1271`).

**Ожидаемый выигрыш.** 1.5–3x для batch 16–256 B является реалистичной целью;
возможен больший выигрыш при выдаче длинного virgin run. Обычные `Box`/`Vec` и
third-party code без adoption не ускорятся. Scoped handle без batch даст более
скромные 5–20%, если TLS resolve заметен на целевой платформе.

**Риски корректности.** Partial OOM, uniform-layout contract, duplicate pointer
в dealloc batch, cross-segment/foreign entries, thread affinity, panic/drop
cleanup и стабильность публичного unsafe API. Batch нельзя считать M2-free без
отдельного policy решения.

**План измерения.** Scalar vs batch N={1,8,16,64,256,4096}, sizes
16/64/256/1024 B и medium, alloc-only/free-only/round-trip, write/no-write,
same/cross-thread. Setup/output buffer вне timer. Scalar control должен точно
воспроизводить текущую direct storm; существующий scalar API и IAI должны быть
byte/codegen-neutral при выключенной feature.

### P1/P2-8. `trusted-fast` — большой потолок, но это policy, а не бесплатный fix

**Точные места.** `GlobalAlloc::dealloc` уже unsafe и его caller обязан передать
живой pointer с правильным layout (`src/global/sefer_alloc.rs:463-490`). Но он
вызывает safe `HeapCore::dealloc`, который дополнительно обещает foreign pointer
no-op и live double-free defence (`src/registry/heap_core.rs:976-997`). На каждом
own free сначала выполняется SegmentTable membership
(`src/registry/heap_core_xthread.rs:168-205`;
`src/alloc_core/segment_table.rs:429-468`), затем exact MagazineBitmap и
AllocBitmap checks/RMW (`src/registry/heap_core.rs:1201-1259`). На каждом
magazine alloc hit очищается MagazineBitmap
(`src/registry/heap_core.rs:821-885`).

RAD-5 нельзя просто откатить: замена variable O(count) scans на exact bitmap
улучшила marginal churn примерно с 125 до 73 Ir/op и cold/recycle на ~20 Ir/op
(`docs/perf/IAI_BASELINE.md:784-845`). Также нельзя batch'ить clear: stale bit
проглатывает легитимный own или remote free и течёт память
(`docs/perf/IAI_BASELINE.md:1206-1241`). Единственная осмысленная быстрая
альтернатива — не возвращать scan, а целиком отказаться от defence для строго
валидного unsafe boundary. Предыдущий план уже называл это `trusted-fast` и
правильно оставил policy-gated (`docs/perf/PERF_PLAN_2026-07-10-radical-audit-implementation-plan.md:266-269`).

**Предлагаемое изменение.** Отдельная opt-in feature/profile:

- safe `HeapCore::dealloc` и default `production` сохраняют нынешние гарантии;
- внутренний `unsafe dealloc_trusted` используется только `GlobalAlloc` и может
  маршрутизировать по stamped owner header без own-table defensive proof;
- более агрессивный tier убирает M2 MagazineBitmap/AllocBitmap oracle с
  valid-free path и, если полный аудит подтверждает отсутствие внутренней
  зависимости, сам второй bitmap/32 KiB metadata. Это отдельный tier от простой
  routing optimization.

**Ожидаемый выигрыш.** Routing-only может дать 0–10%: нынешний four-entry
`own_cache` обычно L1-hot, поэтому header read способен оказаться не дешевле.
Полный M2-free tier имеет цель 10–30% на small alloc/free и 15–35% на cold free
storm, но сохранённые данные не содержат честного no-defence baseline. Нулевой
результат возможен, если TLS и tcache line уже доминируют.

**Риски корректности.** Ослабляется документированная defence-in-depth; unsafe
caller misuse снова может corrupt allocator. Кроме caller double-free надо
доказать, что bitmap не маскирует внутренние duplicate remote entries или
reuse races. Нельзя молча включать tier в `production`.

**План измерения.** Три A/B: current; trusted routing с M2; trusted routing без
M2/second bitmap. Isolated alloc-only/free-only/pair, 1/4/64 segments,
same/cross-thread, write/no-write; native Windows/Linux instructions, branches,
L1/LLC и wall-clock. Отдельный adversarial suite должен демонстрировать
ожидаемое различие semantics, а internal-protocol duplicate tests оставаться
green даже в trusted tier.

### P2-9. Atomic ordering можно ослаблять только после архитектурных fixes

`RemoteFreeRing::push` резервирует tail через `compare_exchange_weak(...,
AcqRel, Relaxed)`, затем публикует slot Release; consumer читает slot Acquire
(`src/alloc_core/remote_free_ring.rs:634-668`). `HeapOverflow` имеет аналогичный
AcqRel tail CAS и отдельный Release publish base
(`src/registry/heap_overflow.rs:280-305`). Вероятно, reservation CAS не обязан
сам публиковать payload: синхронизация идёт через slot/base Release→Acquire, а
head Acquire/Release управляет reuse. Поэтому Relaxed success ordering является
валидным кандидатом для модели.

Однако на x86/x64 locked CAS останется той же инструкцией, то есть текущую
Windows проблему это не исправит. Потенциальный выигрыш 5–15% относится лишь к
contended remote push на ARM/weak-memory targets. Риск — тонкая ошибка
reservation/reuse ordering. План: сначала формальная happens-before таблица,
Loom для reserve-before-publish, consumer stop at unpublished slot, wrap и slot
reuse; затем ARM hardware counters. До P0-4 менять ordering нецелесообразно:
8,192 лишних CAS/counter RMW на порядки важнее barrier strength одного CAS.

### P2-10. Native code layout/PGO — вероятный источник оставшегося Windows gap

Production feature set сейчас корректно исключает per-hit stats, `hardened` и
rejected runfreelist: `production = alloc-global + alloc-xthread +
alloc-decommit + fastbin` (`Cargo.toml:143-187,188-237`). Bench/release profiles
используют ThinLTO и один codegen unit (`Cargo.toml:520-537`). В конечном
consumer binary root crate всё равно контролирует profile; настройки library
не гарантируют, что downstream application собирается так же.

Сохранённый paired native Windows A/B действительно обнаружил регрессию:
медиана +13.69..19.13%, 17–19 из 20 пар в старую сторону, t=3.94..5.27
(`docs/perf/R5_R2_CHURN_REGRESSION_PAIRED_AB.md:145-173`). Она уже присутствует
к `7b4acb3` и локализована между `e6b9b3a` и этим commit
(`docs/perf/R5_R2_CHURN_REGRESSION_PAIRED_AB.md:228-282`). Но на том же окне
Callgrind Ir улучшился на 20.6%, а EstCycles/RAM hits тоже улучшились
(`docs/perf/IAI_BASELINE.md:1354-1378`). Это несовместимо с простой историей
«hot path добавил работу» и указывает на native Windows code placement,
TLS lowering, branch alignment, frequency/host effects либо различие
Callgrind-модели и реального CPU.

**Предлагаемое изменение.** Не делать source-level revert наугад. Сначала
закончить single-commit bisection оставшихся production diffs и снять native
disassembly/counters. Затем A/B:

- PGO на настоящем application workload;
- `target-cpu`/target-feature profile для контролируемого deployment;
- function ordering/outlined cold blocks только по sampled profile;
- ThinLTO vs fat LTO и CGU=1 на конечном binary.

Ожидаемый диапазон PGO/layout — 3–15%, но может быть ноль и platform-specific.
Риски: benchmark overfit, непереносимый binary, рост build time, изменение
exception/unwind policy. `panic=abort` нельзя оценивать по bench profile:
Cargo там его игнорирует (`Cargo.toml:538-542`).

## 5. Почему прошлые волны часто не росли в benchmark'ах

### 5.1 Оптимизировался не тот путь, который меряет headline

Многие последние изменения закрывали teardown, OOM, foreign pointer,
cross-thread Large reclaim, owner-starved overflow, registry lifecycle и RSS.
Warm single-thread churn после первого refill не входит ни в один из этих
сценариев. Там остаются TLS resolve, tcache pop/push и exact bitmap transitions.
Отсутствие роста headline не опровергает lifecycle fix; оно означает, что
контрольный benchmark не содержит механизма.

Обратный пример — RAD-5. Предварительная модель ожидала цену нового bitmap RMW,
но он удалил variable-trip scans и фактически снизил churn с 125 до 73 marginal
Ir/op (`docs/perf/IAI_BASELINE.md:818-845`). Следовательно, считать отдельные
loads/stores без полного generated path недостаточно.

### 5.2 Текущая frontier уже разделилась: warm churn хорош, cold tiny плох

Последняя таблица:

- warm churn 16/64/256/1024 B: 29.0/28.4/31.4/32.1 ns;
- cold/direct storm: 36.4/45.7/59.1/56.9 ns;
- mimalloc cold: 14.6/28.2/35.7/70.3 ns
  (`docs/ALLOC_BENCH.md:43-67`).

Поэтому изменение, которое улучшает refill/cold path, легко теряется в churn,
а изменение hit path не закрывает 16 B storm gap. Увеличение magazine смешивает
эти цели и уже показало худший результат на обеих.

### 5.3 Предыдущие experiments атаковали константу, а не асимптотику

- TCACHE_CAP 32/64/128 регрессировал все 11 IAI benches; CAP=128 дал до +153%
  Ir и +120..224% wall-clock (`docs/perf/PERF2_TCACHE_CAP_SWEEP_EXPERIMENT.md:1-13,90-175`).
- Дополнительный segment hint при S<=3 дал лишь -4.3/-6.6 Ir/op на специальных
  tests и ноль на обычных (`docs/perf/IAI_BASELINE.md:1293-1314`). Нужен exact
  directory при S>=64.
- Run-encoded freelist проиграл pointer chain: cold/recycle +23..31% Ir, потому
  что sorting/descriptor traffic оказался дороже prefetcher-covered chain
  (`Cargo.toml:206-237`).
- `REFILL_BATCH > 31`, CLZ classifier и alloc_zeroed virgin skip уже имели
  отрицательные verdicts; их повтор без нового механизма не создаст рост.

### 5.4 Native wall-clock и Callgrind отвечают на разные вопросы

Paired A/B доказал реальный Windows signal, но deterministic Linux/WSL
Callgrind в том же commit window показывает -20.6% Ir. Callgrind не моделирует
Windows TLS/VirtualAlloc/scheduler и реальный atomic contention; Windows
wall-clock с sample_size=10 чувствителен к code placement и frequency. Нельзя
объявлять один из сигналов «ложным». Нужен platform-native counter/disassembly
этап, а не ещё один source tweak по одному числу.

### 5.5 Harness'ы всё ещё имеют существенные слепые зоны

1. `benches/global_alloc.rs` в module doc утверждает, что SeferAlloc установлен
   как `#[global_allocator]` (`:1-18`), но в файле нет соответствующего static;
   сами hot tests вызывают generic `A: GlobalAlloc` напрямую
   (`:190-203,391-405`), а Vec pattern реализован вручную (`:409-529`). Это
   apples-to-apples direct-call comparison, но не codegen shape реального
   `__rust_alloc`/глобального allocator binary.
2. Все группы используют `sample_size(10)` и 150/600 ms окна
   (`benches/global_alloc.rs:361-365,544-546,701-703`). Group-start trim и
   rotation исправили прошлые confounds (`benches/global_alloc.rs:21-55`), но
   arms не чередуются process-by-process и весь suite остаётся одним warm
   process/TLS heap.
3. Direct «cold/no reuse» делает 1024 alloc, затем 1024 free
   (`benches/global_alloc.rs:190-204`). Внутри batch reuse нет, но следующая
   Criterion iteration получает уже прогретый heap/free lists. Это recycle
   storm, не process-cold first touch.
4. Churn prefill и teardown теперь корректно исключены из timer через
   `iter_batched`/drop guard (`benches/global_alloc.rs:207-282,566-620`); старые
   результаты до этого исправления нельзя сравнивать как pure allocator delta.
5. `heap_fanin_production` проходит настоящий protocol, но spawn/Vec/claim/join/
   recycle находятся внутри timed iteration (`:178-233,244-293`). Он доказал
   retry catastrophe, но не изолирует ns/free и не проверяет SeferAlloc's
   dealloc-only TLS bind, поскольку напрямую claim'ит `HeapCore` producer'ам.
6. `heap_xthread` сейчас исправил старые ошибки: class выводится тем же mapping,
   pointers fresh на sample (`benches/heap_xthread.rs:43-76,80-142`). Но это
   по-прежнему test seam без настоящих producers/contention.
7. `multiseg_cold_256k` — три segments, не 64–1024
   (`benches/perf_gate_iai.rs:340-375`).
8. Large bench пропускает диапазон 253 KiB–4 MiB
   (`benches/large_realloc.rs:174-208`).
9. First-allocation probe измеряет RSS, но не Windows commit
   (`examples/first_alloc_process.rs:121-160,299-318`).

### 5.6 Host drift был больше многих ожидаемых wins

В последней таблице выросли не только Sefer, но и mimalloc/System; документ сам
правильно считает это host drift, а не source regression
(`docs/ALLOC_BENCH.md:70-93`). Оптимизация на 2–5% неразрешима 10 samples одним
запуском. Paired alternating processes, как в R5-R2, должны стать минимальным
protocol для таких изменений.

## 6. Рекомендуемая measurement-программа — не выполнялась

### Stage A. Исправить judges до source changes

1. Три отдельных executable на allocator, каждый действительно имеет свой
   `#[global_allocator]`; один workload source, process-level A/B/B/A запуск.
2. Сохранять raw Criterion samples, commit hash, rustc/LLVM, CPU affinity,
   power plan, temperature/frequency и feature set. Минимум 20 paired processes
   для заявлений <20%.
3. Добавить Windows `PrivateUsage`/`PagefileUsage`, не заменяя Working Set.
4. Persistent-thread fan-in harness: allocation и thread creation вне timer,
   barriers вокруг только dealloc burst.
5. Medium sweep вокруг `SMALL_MAX`; multi-segment S=1..1024; dealloc-only bind
   benchmark.
6. IAI оставлять deterministic instruction judge, но отдельно считать
   bootstrap и marginal per-op; не использовать его как модель contention/OS.

### Stage B. Порядок безопасных экспериментов

1. P0-4 overflow-first + uncounted probe: локальная protocol change с уже
   существующим secondary queue и сильным worst-case доказательством.
2. P0-1 unbound dealloc route: большой resource/latency win без изменения
   owner-bound steady path.
3. P0-2 chunked registry/overflow sidecar: сначала commit metric, затем latency.
4. P0-3 medium allocator prototype под feature и размерный sweep.
5. P1-5 partial Windows commit только после отдельного dense-stream kill gate.
6. P1-6 directory только при S>=64 и с churn-neutral threshold.
7. Batch/trusted-fast — после явного решения публичного API и safety policy.

### Stage C. Метрики принятия

Для каждой идеи одновременно нужны:

- target benchmark с ожидаемым механизмом;
- current headline churn как kill gate;
- memory: VA, process commit, RSS after touch, retained high-water;
- p50/p99/max, а не только median;
- OS calls, CAS attempts/logical free, table/queue occupancy;
- native Windows и Linux wall-clock; IAI/Callgrind отдельно;
- correctness plan: Loom/Miri/property tests перечислить до implementation, но
  performance verdict не подменять их результатом.

## 7. Анти-паттерны оптимизации — чего не делать

1. **Не увеличивать `TCACHE_CAP`.** 32/64/128 уже дали монотонную и
   super-linear регрессию; CAP=128 выталкивает metadata из L1 и увеличивает
   wall-clock до +224% (`docs/perf/PERF2_TCACHE_CAP_SWEEP_EXPERIMENT.md:90-186`).
2. **Не откатывать MagazineBitmap к scan и не batch'ить clear.** Текущий bitmap
   измеренно дешевле scan; delayed clear ломает exact membership и проглатывает
   легитимный free (`docs/perf/IAI_BASELINE.md:784-845,1206-1241`).
3. **Не добавлять ещё один маленький segment hint.** При трёх segments он не
   может показать асимптотический выигрыш; нужен полный directory и judge >=64.
4. **Не возвращать runfreelist/sorting в текущей форме.** Pointer chase оказался
   prefetcher-covered, а representation overhead дал +23..31% Ir
   (`Cargo.toml:206-237`).
5. **Не лечить retry storm только меньшей константой, backoff или infinite spin.**
   Backoff уже пропускал drain windows; infinite spin нарушает bounded dealloc.
   Сначала использовать существующий secondary capacity и убрать per-attempt
   counters.
6. **Не ослаблять atomics ради x86 headline до изменения алгоритма.** AcqRel→
   Relaxed CAS не уберёт locked instruction на x86; потенциальный ARM win не
   сопоставим с 8,192 лишними retries и требует модели.
7. **Не принимать `repr(Rust)` source order за реальный cache layout.** У
   `PerClass { count, slots }` нет `repr(C)` (`src/registry/tcache.rs:100-140`),
   поэтому перестановка полей по исходнику без `offset_of!/size_of!` artifact и
   generated-code A/B может закрепить худший layout. Сначала измерить фактические
   offsets и occupancy distribution.
8. **Не включать `alloc-stats`, `hardened` или `alloc-runfreelist` в production
   benchmark случайно.** Current `production` специально исключает их
   (`Cargo.toml:165-237`); feature mismatch делает сравнение бессмысленным.
9. **Не выдавать RSS за Windows commit и Callgrind за native contention.** Это
   две разные оси; именно их смешение уже скрыло registry commit и запутало
   интерпретацию R5-R2.
10. **Не оптимизировать старые абсолютные таблицы без повторного paired baseline.**
    Harness semantics и host state менялись; сохранённые цифры полезны как форма
    workload и counterfactual, но не как вечная абсолютная скорость.

## 8. Краткая карта ожидаемого результата

- Если цель — **обычный small-object steady churn**, текущий path уже близок к
  сильной локальной точке. Реальный следующий шаг — scoped/batch или явно
  opt-in trusted policy; микротюнинг вероятно даст единицы процентов.
- Если цель — **fan-in и tail latency**, P0-4 и P0-1 имеют наибольшую
  доказательную базу и не требуют менять small allocation policy.
- Если цель — **Windows startup/commit и много threads**, P0-2 + P1-5 важнее
  любой ns/op оптимизации: текущий RSS judge не видит сотни MiB commit.
- Если цель — **medium buffers**, сначала устранить 253 KiB cliff; нынешние
  headline benchmarks этот класс полностью обходят.
- Если цель — **heaps с сотнями segments**, строить exact directory/dirty
  publication и измерять при S>=64; ещё один hint при S=3 — заведомо слабый
  эксперимент.

Самый рациональный первый implementation sequence: overflow-first remote free,
dealloc-without-bind, commit-aware registry probe/chunking, затем medium layer.
Эти четыре направления атакуют реальные multiplicative costs и high-percentile
path, не пытаясь выжать ещё одну инструкцию из уже сильного magazine hit.
