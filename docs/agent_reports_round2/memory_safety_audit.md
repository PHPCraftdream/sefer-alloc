# Углублённый аудит безопасности памяти — раунд 2

Дата: 2026-07-12  
Объект: текущее рабочее дерево `D:\dev\rust\sefer-alloc`  
Метод: исключительно статическое read-only чтение исходников и конфигурации. Git, сборка, тесты, Miri, Loom, fuzzing, бенчмарки и скрипты не запускались. Исходники и существующие файлы не изменялись.

## Итог

Подтверждены четыре группы проблем:

| ID | Severity | Кратко |
|---|---|---|
| R2-1 | **CRITICAL** | Публичные safe `realloc` всё ещё могут выполнить OOB/UAF-read через `copy_nonoverlapping` |
| R2-2 | **CRITICAL** | Safe abandoned-stack API позволяет оставить глобальный dangling segment pointer и затем разыменовать его после `AllocCore::drop` |
| R2-3 | **HIGH** | Публичные `#[doc(hidden)]` safe raw-memory hooks выполняют unchecked чтение/запись по caller-controlled адресам |
| R2-4 | **MEDIUM** | Heap-overflow cache запоминает `tail`, а не фактически продвинутый `head`, и может навсегда скрыть опубликованное освобождение |

`#[doc(hidden)]` не ограничивает доступность API для downstream crate и потому не является границей безопасности.

## Подтверждённые находки

### R2-1 — CRITICAL — safe `realloc` разыменовывает неверно описанный, interior, stale или foreign pointer

**Файлы и строки**

- `src/alloc_core/alloc_core.rs:1287-1327`: публичный safe `AllocCore::realloc`; проверяется только принадлежность вычисленного segment base, затем размер чтения берётся непосредственно из caller-controlled `old_layout`.
- `src/alloc_core/alloc_core.rs:1324-1326`: `copy = min(old_layout.size(), new_size)`, затем копирование и `dealloc` с тем же непроверенным layout.
- `src/registry/heap_core.rs:1463-1572`: публичный safe `HeapCore::realloc`; собственный путь имеет ту же проблему, а foreign-путь вообще безусловно выделяет новый блок и копирует из `ptr`.
- `src/registry/heap_core.rs:1550-1552`, `1569-1571`: фактические copy/dealloc legs.
- `src/alloc_core/node.rs:146-154`: мембрана вызывает настоящий unsafe `core::ptr::copy_nonoverlapping`.
- `src/global/fallback.rs:238-258`: публичный safe `with_heap` передаёт closure безопасную `&mut HeapCore`, поэтому дефект `HeapCore::realloc` достижим без внешнего unsafe.
- `src/lib.rs:280`: `AllocCore` публично реэкспортирован.

**Доказательство**

Исправление предыдущего foreign-pointer дефекта в `AllocCore::realloc` (`contains_base` на строках 1299-1308) доказывает только, что адрес находится внутри зарегистрированного OS-сегмента. Оно не доказывает, что `ptr` является началом текущей аллокации, что аллокация ещё жива и что `old_layout.size()` не превышает её фактический размер. Move leg всё равно доверяет `old_layout` и передаёт этот размер в unsafe copy.

Для `HeapCore::realloc` foreign leg на строках 1556-1572 не имеет даже segment-membership барьера. Комментарий считает foreign pointer допустимым межheap-сценарием, но функция не может отличить такой pointer от `1usize as *mut u8`, dangling pointer или указателя другого allocator-а до чтения из него.

**Сценарий**

1. Создать `AllocCore` и безопасно получить небольшой блок `p = alloc(Layout(16, 16))`.
2. Вызвать safe `realloc(p, Layout(8 MiB, 16), 8 MiB)`. Layout сам по себе валиден, segment base зарегистрирован, но source range длиной 8 MiB выходит за 4 MiB small segment; `copy_nonoverlapping` читает за пределами reservation.
3. Независимый полностью safe foreign-сценарий доступен через `global::fallback::with_heap(|h| h.realloc(1usize as *mut u8, old, new))`: при успешной новой аллокации foreign leg читает из адреса `0x1`.

Возможные последствия: invalid/OOB/UAF read, process fault, утечка соседних данных в новый блок и последующая порча allocator state при неверном `dealloc`.

**Уверенность:** **HIGH**. Safe-to-unsafe data flow прямой и не содержит проверки фактической allocation extent/lifetime.

### R2-2 — CRITICAL — safe abandoned-stack API создаёт глобальный dangling pointer и UAF

**Файлы и строки**

- `src/alloc_core/alloc_core.rs:1491-1506`: safe публичный `segment_bases()` выдаёт raw bases живых сегментов.
- `src/registry/heap_registry.rs:368-375`: safe публичный `HeapRegistry::push_abandoned_segment(base)` принимает base по prose-only контракту.
- `src/registry/heap_registry.rs:787-829`: push записывает intrusive `next_abandoned` и публикует base в process-global stack.
- `src/registry/heap_registry.rs:389-443`: safe `pop_abandoned_segment` разыменовывает опубликованный base через `SegmentMeta::new(base).next_abandoned_atomic().load(...)` на строках 401-402.
- `src/alloc_core/alloc_core.rs:1665-1714`: `AllocCore::drop` освобождает все зарегистрированные OS reservations, не удаляя их из глобального abandoned stack.
- `src/lib.rs:246-255`, `src/registry/mod.rs:40-93`: `alloc_core` и `registry` доступны как публичные `#[doc(hidden)]` модули.

**Доказательство**

`push_abandoned_segment` не помечен `unsafe`, не принимает ownership token и не связывает lifetime записи в глобальном стеке с владельцем reservation. Поэтому safe caller может опубликовать base сегмента standalone `AllocCore`, после чего обычный safe `drop(core)` освобождает reservation. Глобальная atomic head продолжает содержать этот адрес. Следующий safe pop строит `&AtomicU64` поверх освобождённой памяти и выполняет load — use-after-free/invalid atomic reference внутри библиотеки.

Это не требует произвольного bogus address или нарушения unsafe-контракта: base получен самой библиотекой из safe `segment_bases()`.

**Сценарий**

1. С feature `alloc-global` создать `let core = AllocCore::new().unwrap()`.
2. Safe получить `base = core.segment_bases().next().unwrap()`.
3. Safe вызвать `HeapRegistry::push_abandoned_segment(base)`.
4. Safe уничтожить `core`; `Drop` unmap-ит segment.
5. Safe вызвать `HeapRegistry::pop_abandoned_segment()`; строки 401-402 читают atomic link из unmapped/freed reservation.

Повторный push того же valid base также создаёт self-link и допускает многократную выдачу одного segment base, но UAF-сценарий выше уже достаточен для подтверждения unsoundness.

**Уверенность:** **HIGH**. Ownership разорван на явном safe call chain; cleanup глобального stack при `AllocCore::drop` отсутствует.

### R2-3 — HIGH — safe публичные raw-memory test hooks имеют unsafe prose-контракты

**Файлы и строки**

- `src/alloc_core/remote_free_ring.rs:487-510`: safe `over_test_buffer`/`init_test_buffer` принимают произвольный `*mut u8`; init доходит до raw writes на строках 555-565.
- `src/alloc_core/segment_header.rs:1555-1573`, `1588-1604`, `1632-1643`: safe публичные `gen_at`, `bump_gen`, `init_gen_table_in_place` материализуют atomic views и пишут по caller-provided base/off.
- `src/alloc_core/run_stack.rs:190-213`, `215-248`, `251-340`: safe публичные `RunStack::{init_in_place,push,pop,peek,is_empty,clear_all}` читают/пишут `RunDesc` по caller-controlled base/class; class защищён лишь `debug_assert!`.
- `src/alloc_core/alloc_core_small.rs:894-975`: safe публичный `flush_class` принимает caller-controlled raw pointers и передаёт вычисленный base в `flush_run`; membership segment table перед metadata reads не проверяется.
- `src/alloc_core/alloc_core.rs:1083-1185`: несколько safe `dbg_*` accessors полагаются только на `debug_assert!(contains_base)`; release build после assert непосредственно читает/пишет header.

**Доказательство**

Все перечисленные функции публичны для downstream code при соответствующих features и вызываются без `unsafe`. Их обязательства — «base указывает на live segment/buffer нужного размера/alignment», «class в диапазоне», «off валиден» — существуют только в комментариях или `debug_assert!`. Реализация переносит эти значения в `Node::offset`, `Node::read_struct`, `Node::write_struct` либо создаёт `&Atomic*`; эти внутренние primitives содержат unsafe dereference/write и требуют доказанных bounds/lifetime/alignment.

Простейший safe вызов `RunStack::init_in_place(1usize as *mut u8)` при `alloc-runfreelist`, либо `RemoteFreeRing::init_test_buffer(1usize as *mut u8)` при `alloc-xthread`, приводит к invalid writes внутри библиотеки. `#[doc(hidden)]` только исключает item из документации и не меняет visibility.

**Сценарий**

Downstream crate включает соответствующий feature, импортирует публичный скрытый модуль и передаёт ненулевой bogus pointer или слишком короткий buffer. В debug часть вариантов panic-нет на base; в release `debug_assert!` для class исчезает. Библиотека выполняет OOB/misaligned/dangling read/write либо создаёт недопустимую atomic reference.

**Уверенность:** **HIGH**. Прямые safe entry points и raw-memory sinks видны в исходниках. Практическая severity ниже R2-1/R2-2 лишь потому, что API обозначены test-only и часть доступна только под opt-in features.

### R2-4 — MEDIUM — `HeapOverflow::drain` возвращает snapshot `tail`, а cache должен хранить фактический `head`

**Файлы и строки**

- `src/registry/heap_overflow.rs:339-380`: drain загружает `t` на строке 356, может остановиться на reserved-but-not-published slot (365-369), публикует фактический `h` в `head` (379), но возвращает `t` (380).
- `src/registry/heap_core.rs:755-800`: owner пропускает drain при `tail == overflow_tail_cache` (772-774) и присваивает cache возвращённое из `drain` значение (785 или 797).
- Для сравнения корректный per-segment протокол: `src/alloc_core/remote_free_ring.rs:645-702` возвращает фактический final `h`; `src/alloc_core/alloc_core_small.rs:1622-1630` именно его сохраняет как cached head.

**Доказательство**

Рассмотрим producer P0: он выигрывает CAS `tail: 0 -> 1` в `HeapOverflow::push` (строки 290-297), но до Release-store `base` (303-304) приостанавливается. Owner вызывает drain, видит `t = 1`, `h = 0`, обнаруживает пустой unpublished slot и выходит из цикла с `h = 0`. Несмотря на это, drain возвращает `t = 1`, и `HeapCore` записывает `overflow_tail_cache = 1`. После этого P0 публикует slot, но `tail` остаётся 1. На каждом следующем owner call `is_likely_empty(1)` видит `tail == cache` и пропускает drain. Запись останется невостребованной до появления ещё одного push, который изменит tail; если его нет — навсегда.

Комментарии `HeapOverflow::drain` обещают «later drain picks it up», но cache guard исключает этот later drain. Соседний `RemoteFreeRing` специально возвращает final head и не содержит дефекта.

**Сценарий**

Последний cross-thread free в burst попадает в heap overflow; producer прерывается между reserve и publish, owner успевает сделать drain, затем producer завершает publish и больше pushes нет. Блок не возвращается в BinTable, `live_count` не уменьшается, empty segment не decommit/recycle-ится. При повторении на разных heaps/segments это даёт удержание памяти и SegmentTable exhaustion; это не прямой double-free, но опасная потеря освобождений.

**Уверенность:** **HIGH** для логической ошибки и permanent-until-next-push retention; **MEDIUM** для практической severity, поскольку нужен конкретный interleaving и последняя запись без последующего push.

## Проверенные области без дополнительных подтверждённых проблем

- `crates/vmem`: ownership `Reservation`, `into_parts`/`Drop`/`release`, Windows reserve-then-commit, Unix mmap/trim, decommit/recommit ranges, checked address alignment. Новых подтверждённых double-release, wrong-layout release или alignment нарушений на корректных unsafe-call contracts не найдено.
- `crates/numa`: Linux/Windows FFI, ownership результата `reserve_on_node`, передача raw reservation в `aligned-vmem`. Новых подтверждённых UAF/double-free не найдено.
- `crates/region`: `Region<T>`, `Handle<T>`, `SyncRegion<T>`; собственный unsafe отсутствует, stale handles делегированы generational slotmap semantics. Подтверждённых проблем не найдено.
- `src/concurrent/hand.rs` и `EpochRegion`: epoch pinning, generation double-check, CAS-before-swap eviction, exactly-once `defer_destroy`, retirement на `u32::MAX`, remote-free queue. Дополнительных подтверждённых UAF/double-free/ABA не найдено.
- `LockFreeRegion`, `ShardedRegion`, pinning: snapshot/locking/shard routing и handle generation paths. Новых memory-safety дефектов не подтверждено.
- Small allocation geometry: size-class lookup, divisibility по alignment, bump carve/batch bounds, metadata lower bound, unconditional `off >= bump`, allocation/magazine bitmaps, live-count/decommit reset. На корректных Layout и allocation lifetimes новых size/alignment/OOB дефектов не найдено.
- Freelist и run-freelist production paths: bitmap transitions, batch drain, run reconstruction, hardened next-pointer validation. Помимо публичных unsafe-by-contract hooks из R2-3 новых подтверждённых дефектов не найдено.
- RemoteFreeRing основной MPSC protocol: cursor wrap, Release/Acquire publication, stop-at-unpublished и cache actual-head path. Он не имеет ошибки R2-4, найденной в новом `HeapOverflow` аналоге.
- Registry free-slot stack: running ABA tag теперь сохраняется через empty transition; generation widened to `u64`; initialisation publish gate и OOM push-back просмотрены. Дополнительного slot double-claim не подтверждено.
- Large allocation/cache: Layout classification, `align >= SEGMENT` reject, span sizing, cache ownership/accounting, unregister-before-release, deferred-large double-push claim. Нового double-release на корректном `GlobalAlloc` usage не подтверждено.
- TLS/fallback/registry lifetime: torn sentinel, slot reuse, stable slot-resident atomics, fallback lock panic recovery. Кроме safe-достижимости `HeapCore::realloc` в R2-1 новых подтверждённых проблем не найдено.

## Ограничения аудита

Это статический аудит. Interleavings R2-4 и platform-specific OS paths не воспроизводились динамически из-за прямого запрета на сборки/тесты/Miri/Loom. Отсутствие находки в перечисленных проверенных областях не является формальным доказательством отсутствия UB.
