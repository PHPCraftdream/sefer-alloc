# Повторное read-only ревью безопасности памяти — round 4

Дата: 2026-07-13  
Объект: текущее дерево `sefer-alloc`  
Контекст: `docs/agent_reviews_round3/memory_safety_review.md`, `docs/reviews/2026-07-12-round3-synthesis.md`, `docs/reviews/2026-07-12-round3-remediation-plan.md`, `docs/checkpoints/2026-07-13-0030.md`

## Метод и ограничения

Ревью выполнено только статическим чтением текущих Rust-исходников, `Cargo.toml`, конфигурации и предыдущих отчётов. Git, сборка, тесты, Miri/Loom/Kani, бенчмарки, fuzzing и проектные скрипты не запускались. Существующие файлы не изменялись.

Перепроверены публичные safe raw-pointer входы, все фактические unsafe-seams, lifecycle сегментов и registry slots, small/large alloc/dealloc/realloc, tcache/bitmap/freelist, remote-free ring и heap overflow, deferred-large и abandoned stacks, TLS ownership, ABA/generation, decommit/recommit/release, `Layout`/size/alignment, `vmem`, `numa` и experimental epoch/atomic-slot пути.

## Итог

Текущий код всё ещё имеет четыре подтверждённые soundness/invariant-проблемы: три соответствуют R3-MS-1/2/3 и не были исправлены, четвёртая обнаружена при повторной проверке публичной registry-поверхности. Наиболее сильные контрпримеры требуют только safe вызовов downstream-кода и приводят либо непосредственно к UB внутри библиотеки, либо к повторной выдаче/освобождению живой памяти и конкурентному доступу к одному `HeapCore`.

| ID | Severity | Статус | Кратко |
|---|---:|---|---|
| R4-MS-1 | **CRITICAL** | подтверждено | Safe `realloc` не доказывает identity/liveness/extent исходного allocation; достижим `copy_nonoverlapping(p, p, n)` |
| R4-MS-2 | **CRITICAL** | подтверждено | Safe `dealloc` принимает interior/mismatched/stale pointers; возможны release живого Large segment, freelist/tcache corruption и invalid foreign-header read |
| R4-MS-3 | **HIGH** | подтверждено | Публичные safe test/debug raw-memory hooks имеют только prose-contract и выполняют unchecked raw access |
| R4-MS-4 | **CRITICAL** | подтверждено, новое | Публичные атомики registry позволяют safe-коду повторно выдать LIVE `HeapCore` другому потоку, разрушив single-writer инвариант `unsafe impl Sync` |

## Подтверждённые находки

### R4-MS-1 — CRITICAL — safe `realloc` не устанавливает предусловия source-copy

**Файлы/строки**

- `src/alloc_core/alloc_core.rs:1341-1396` — публичный safe `AllocCore::realloc`; проверяется только принадлежность segment base, затем выполняются alloc/copy/dealloc.
- `src/alloc_core/alloc_core.rs:1399-1439` — `safe_payload_read_span` ограничивает чтение остатком физического segment span, а не границей фактического блока.
- `src/alloc_core/alloc_core.rs:1540-1582` — in-place ветви доверяют caller-supplied `old_layout`; не проверяются payload start, текущая жизнь блока или реально выданный class/size.
- `src/registry/heap_core.rs:1468-1613` — те же own/cross-heap move legs в safe `HeapCore::realloc`.
- `src/registry/heap_core.rs:1593-1600`, `src/alloc_core/segment_header.rs:694-696` — foreign leg читает `magic` по вычисленному candidate base до process-global membership/liveness barrier.
- `src/alloc_core/node.rs:146-154` — unsafe sink: `core::ptr::copy_nonoverlapping` требует валидных непересекающихся диапазонов.

**Доказательство**

`contains_base(base)` доказывает только существование зарегистрированного и отображённого segment. Для Small/Primordial `safe_payload_read_span` возвращает остаток до конца всего 4 MiB segment (`alloc_core.rs:1429-1439`), хотя allocation может иметь 16 байт. Функция не доказывает, что `ptr` является началом текущего allocation, что блок ещё жив, что `old_layout` соответствует реально выданному class/extent и что fresh destination не пересекается с заявленным source.

Round3-синтез отклонил self-copy, разобрав другую последовательность: `realloc(p_класс16, old=16, new=32)` (`docs/reviews/2026-07-12-round3-synthesis.md:30-32`). Исходный контрпример round3 был обратным: после освобождения 16-byte блока вызывается `realloc(p, old=32, new=16)` (`docs/agent_reviews_round3/memory_safety_review.md:42,50-53`). В текущем коде:

1. `old=32` и `new=16` дают разные small classes, поэтому same-class in-place ветвь `alloc_core.rs:1569-1579` возвращает `None`.
2. LIFO free list может немедленно переиздать освобождённый 16-byte адрес `p` в `self.alloc(new_layout)` на `alloc_core.rs:1389`.
3. `copy = min(32,16) = 16`, и `Node::copy_nonoverlapping(p, p, 16)` вызывается на `alloc_core.rs:1393-1394`.
4. Для ненулевой длины равенство source/destination нарушает обязательное non-overlap предусловие и является UB внутри safe функции.

Этот сценарий достижим, например, в standalone `alloc-core` конфигурации без release пустого segment; альтернативно достаточно оставить ещё один живой блок в segment, чтобы освобождение `p` не уничтожало сам segment. Проверка segment span проходит.

Отдельно safe `HeapCore::realloc(1usize as *mut u8, valid_layout, new_size)` под `alloc-xthread` вычисляет base 0 (`src/alloc_core/os.rs:90-102`) и выполняет plain raw read в `SegmentHeader::magic_at(0)` до отклонения указателя. Это самостоятельный immediate-UB путь, не зависящий от LIFO reuse.

**Сценарий воздействия**

- Непосредственный UB через overlapping `copy_nonoverlapping`.
- OOB/stale/decommitted read при завышенном, но помещающемся в segment `old_layout`.
- После copy вызывается safe `dealloc(ptr, old_layout)` с тем же недоказанным identity/layout, что может повредить другой class или освободить весь Large segment.

**Уверенность:** **HIGH**. Safe-to-unsafe поток и точная последовательность классов видны непосредственно в текущем коде; вывод round3-синтеза опровергает не тот контрпример.

### R4-MS-2 — CRITICAL — safe `dealloc` не валидирует allocation identity, начало блока и Layout

**Файлы/строки**

- `src/alloc_core/alloc_core.rs:705-770` — API явно оставлен safe; prose требует «well-behaved caller», но signature не выражает memory-safety preconditions.
- `src/alloc_core/alloc_core.rs:774-818` — membership проверяется только по segment base; Large path выбирается по header kind и читает/освобождает весь segment.
- `src/alloc_core/alloc_core.rs:970-988` — Small class выводится из caller-supplied `Layout`.
- `src/alloc_core/alloc_core_small.rs:2244-2322` — interior-pointer guard существует только под `hardened`; production-путь пишет intrusive `next`, head и bitmap.
- `src/registry/heap_core.rs:1094-1104`, `1146-1211`, `1320-1405` — safe `HeapCore::dealloc`; Large-kind и block-start проверки tcache включены только под `hardened`, затем pointer помещается в magazine.
- `Cargo.toml:162-163,186-203` — `production` включает `fastbin`, но не `hardened`.
- `src/registry/heap_core.rs:1637-1674`, `1697-1728` — foreign slow path после проверки только «не в моей table» читает candidate header; комментарий сам допускает released/unmapped base.

**Доказательство**

1. **Large interior free.** Из safe `alloc` получен Large pointer `p`. Safe raw-pointer операция `p.wrapping_add(1)` создаёт `q` с тем же segment base. `AllocCore::dealloc(q, layout)` проходит `contains_base`, видит `SegmentKind::Large` и cache/release-ит reservation целиком, хотя `q` не является payload start и исходный `p` остаётся логически живым. Проверки фактического Large payload address и соответствия `layout`/header отсутствуют.

2. **Small interior injection.** Выделяется Small блок `p` не меньше 32 байт; safe вызывается `dealloc(p.wrapping_add(16), Layout(16,16))`. В non-`hardened` сборке offset находится в payload и ниже bump, а 16-byte bitmap cell для interior address выглядит «allocated». Код `alloc_core_small.rs:2314-2322` записывает freelist link в середину живого блока и ставит interior address во freelist. Следующий 16-byte alloc может вернуть этот адрес, создав две пересекающиеся живые allocation.

3. **Production tcache: Large как Small.** В `production` safe `HeapCore::dealloc(p_large, Layout(16,16))` классифицируется по переданному Layout. Guard `SegmentKind::Large` находится только в `#[cfg(feature = "hardened")]` (`heap_core.rs:1179-1184`). Нулевые Large payload/metadata bytes проходят magazine/alloc bitmap probes, после чего `p_large` записывается в small-class tcache (`heap_core.rs:1351-1364`). Следующий small alloc переиздаёт тот же адрес, пересекающийся с исходным Large allocation. Комментарии `heap_core.rs:1161-1170` прямо описывают этот failure mode, но production его сознательно не блокирует.

4. **Произвольный foreign pointer.** `HeapCore::dealloc(1usize as *mut u8, valid_layout)` под `alloc-xthread` доходит до `SegmentHeader::magic_at(0)` (`heap_core.rs:1727`) и выполняет invalid raw read из safe функции.

Bitmap M2 обнаруживает только некоторые уже известные resting states; он не восстанавливает allocation identity и не превращает произвольный segment-relative address в начало выданного блока. Сравнение с C `free` неприменимо к soundness safe Rust API: нарушение невыразимого в типах prose-contract не может разрешать библиотеке UB или allocator-state corruption.

**Сценарий воздействия**

- Release/cache живого Large segment и dangling pointers.
- Запись allocator metadata в тело живого Small/Large allocation.
- Двойная выдача одного адресного диапазона, пересекающиеся allocations, последующие double-free/UAF.
- Immediate UB/fault на чтении header произвольного или уже unmapped base.

**Уверенность:** **HIGH**. Все четыре пути прямые; два strongest cases дополнительно описаны комментариями самого кода как опасность, отключённая в production ради стоимости проверки.

### R4-MS-3 — HIGH — публичные safe raw-memory/test hooks остаются unsound

**Файлы/строки**

- `src/lib.rs:237-260`, `src/alloc_core/mod.rs`, `src/registry/mod.rs:40-68` — doc-hidden modules реально публичны downstream crate.
- `src/alloc_core/remote_free_ring.rs:509-532,570-588` — safe `over_test_buffer`/`init_test_buffer`; проверяются только null и 4-byte alignment, затем выполняются raw writes по `FOOTPRINT`.
- `src/alloc_core/run_stack.rs:206-218,232-255,279-350` — public safe raw reads/writes по caller-controlled `base`; class assert не проверяет validity/lifetime/extent.
- `src/alloc_core/segment_header.rs:1575-1587,1611-1623,1656-1662` — public safe generation-table hooks создают atomic view или пишут по произвольному live-base contract.
- `src/alloc_core/alloc_core_small.rs:487-518,538-585,896-974` — safe corruption/debug/batch hooks не устанавливают membership/lifetime; два length bounds остаются `debug_assert!`.
- `src/alloc_core/node.rs:170-226,374-423` — фактические raw dereference/write seams и создание `&'static Atomic*`; безопасность полностью делегирована caller-инварианту safe wrapper-а.

**Доказательство**

Например, `RemoteFreeRing::init_test_buffer(4usize as *mut u8)` проходит обе release-проверки (`non-null`, `4`-aligned), после чего `init_in_place` пишет cursors и все slots начиная с адреса 4. Никакого unsafe блока downstream caller не требуется; invalid write выполняет библиотека. Аналогично `RunStack::init_in_place`, `gen_at`/`bump_gen`/`init_gen_table_in_place` и `dbg_*` принимают адреса/буферы, для которых доступность, размер, alignment typed access, эксклюзивность и lifetime проверяются только текстом.

`#[doc(hidden)]`, имя `dbg_*`, opt-in feature и `#![forbid(unsafe_code)]` внутри wrapper-модуля не меняют safe API contract. Последнее лишь переносит фактический unsafe в `Node`.

**Сценарий воздействия**

Downstream crate включает соответствующий feature и safe-вызовом передаёт выровненный invalid/dangling base либо слишком короткий buffer. Библиотека делает invalid/OOB read/write, создаёт ссылку с ложной `'static` lifetime или портит allocator metadata.

**Уверенность:** **HIGH**. Минимальный invalid-address контрпример детерминирован и проходит текущие release guards.

### R4-MS-4 — CRITICAL — публичные registry atomics позволяют повторно claim-ить LIVE `HeapCore`

**Файлы/строки**

- `src/lib.rs:254-260`, `src/registry/mod.rs:40-68` — `registry`, `bootstrap`, `heap_slot`, `heap_registry` публичны.
- `src/registry/bootstrap.rs:281-300,359-380` — safe `ensure()` возвращает `&'static Registry`; `slots`, `count`, `free_slots`, `abandoned_segs` публичны.
- `src/registry/heap_slot.rs:233-270` — `HeapSlot::state` и `generation` публичны именно ради integration tests.
- `src/registry/heap_slot.rs:398-411` — `unsafe impl Sync` обоснован эксклюзивностью единственного CAS-победителя.
- `src/registry/tagged_ptr.rs:181-214` — публичные `dbg_pack`/constants позволяют safe-коду сформировать корректный `free_slots` head.
- `src/registry/heap_registry.rs:95-142,698-746` — safe `claim()` pop-ит указанный slot, CAS-ит `FREE -> LIVE` и возвращает его `HeapCore` без знания о старом TLS-owner.
- `src/global/tls_heap.rs:367-388` — горячий TLS path принимает любой cached non-sentinel pointer как `CurrentHeap::Own`, не перепроверяя slot state/generation.
- `src/global/sefer_alloc.rs:383-456` — `GlobalAlloc` создаёт `&mut HeapCore` через `(*heap).alloc/dealloc/realloc`, полагаясь на single-writer invariant.

**Доказательство**

Публичность атомиков разрешает safe downstream-коду выполнять сами transition-ы, на эксклюзивность которых опирается unsafe-код:

1. Поток A делает обычную safe allocation через установленный `SeferAlloc`, получает и кеширует LIVE slot.
2. Safe-код вызывает `bootstrap::ensure()`, находит `slots[idx].state == STATE_LIVE`, затем делает `state.store(STATE_FREE, ...)` и `free_slots.store(tagged_ptr::dbg_pack(idx as u64, 0), ...)`.
3. Поток B при первом allocator use вызывает штатный safe `HeapRegistry::claim`; `pop_free_slot` возвращает `idx`, CAS `FREE -> LIVE` успешен, и B кеширует тот же `*mut HeapCore`.
4. Поток A не проверяет state/generation своего TLS pointer на горячем пути. Одновременные обычные allocations A и B материализуют два конкурентных `&mut HeapCore` к одному `UnsafeCell`-объекту.

Следствие — нарушение Rust aliasing rules и data races по неатомарным `AllocCore`, tcache, bin/freelist/segment-table полям; практический результат включает повреждение списков и bitmap, двойную выдачу блока, UAF/double-free. `Ordering` атакующих stores не устраняет проблему: API вообще не имеет capability/visibility barrier, запрещающего внешнему safe-коду участвовать в протоколе.

Публичный `generation` дополнительно позволяет подделывать owner epochs, но для приведённого takeover он не нужен.

**Сценарий воздействия**

Любой safe dependency в процессе с `SeferAlloc` может нарушить registry state machine без `unsafe`, после чего безопасные стандартные allocations в двух потоках приводят к UB внутри `unsafe impl GlobalAlloc`.

**Уверенность:** **HIGH**. Публичность полей, pack helper, отсутствие TLS revalidation и конечный unsafe dereference подтверждены текущими строками; invariant в комментарии `unsafe impl Sync` прямо противоречит доступной safe mutation surface.

## Исправленные/закрытые пункты

### Закрыто: abandoned-stack raw APIs больше не safe

`HeapRegistry::push_abandoned_segment`, `pop_abandoned_segment`, `try_adopt`, `abandon_segments` и `recycle` имеют `unsafe fn` boundary и документированные mapped/lifetime/ownership contracts (`src/registry/heap_registry.rs:229,328,396,432,518`). Round2-сценарий safe publish → drop/unmap → pop закрыт. Это не закрывает R4-MS-4: сами `Registry`/`HeapSlot` control atomics всё ещё публично мутируемы из safe-кода.

### Закрыто: `HeapOverflow::drain` возвращает реальный stop cursor

`src/registry/heap_overflow.rs:339-397` возвращает итоговый `h`, а не входной snapshot `t`. Reserved-but-not-yet-published entry поэтому не становится навсегда невидимой из-за ошибочного cached tail. R2-4 закрыт.

### Закрыто: membership/cache invalidation и recycle slot reuse

`src/alloc_core/segment_table.rs:318-353,381-428` удаляет hash membership, очищает own-cache до OS release, NULL-ит table slot и переиспользует его через bounded free list. Старый stale-cache hit на released base и исчерпание table slot при обычном recycle закрыты в проверенном пути.

### Закрыто: повторный same-base run внутри одного `flush_class`

`src/alloc_core/alloc_core_small.rs:896-974` запоминает уже recycled bases и не касается их metadata повторно; `flush_run` также имеет unconditional payload/bump bounds (`:1011-1076`). Ранее возможный metadata UAF после release первого run закрыт для production-size batches. Публичный произвольный `flush_class` остаётся частью R4-MS-3, поскольку initial base membership не проверяется.

### Закрыто: конкретные ABA empty-transition и kind-decode ошибки

- `free_slots` сохраняет текущий tag при переходе в empty (`src/registry/heap_registry.rs:721-746`).
- `abandoned_segs` сохраняет tag на empty transition (`src/registry/heap_registry.rs:450-485`).
- `SegmentHeader::kind_at` строго отображает неизвестный byte в `Unknown`, а не в `Small` (`src/alloc_core/segment_header.rs:650-683`).

Это закрывает конкретные reset/amplification дефекты, но не математический wrap конечных tags и не публичное вмешательство R4-MS-4.

### Частично исправлено, но не закрыто: raw test-hook guards

Release `assert!` для class/index/null/alignment действительно добавлены в `RunStack`, generation table и ring test buffer. Они закрывают соответствующие узкие invalid-index/misalignment варианты, но не validity/lifetime/writable-extent contracts; поэтому R4-MS-3 остаётся подтверждённым.

### Не закрыто: round3 R3-MS-1/2/3

После round3 изменены документация и формулировка design decision, но сигнатуры и достаточная runtime validation не появились. Текущие источники воспроизводят все три safe-boundary проблемы. Утверждение remediation plan, что они закрыты как «design-affirmed», не является memory-safety исправлением.

## Неподтверждённые риски

Ниже недостаточно статического доказательства UB при соблюдении текущих unsafe/ownership contracts; пункты не считаются подтверждёнными находками.

### U-R4-1 — конечный wrap ABA tags

`free_slots` имеет 48-bit tag (`src/registry/tagged_ptr.rs:73-121`), `abandoned_segs` — 22-bit tag из alignment bits (`src/registry/bootstrap.rs:184-203`). Empty-reset исправлен, но поток с остановленным CAS теоретически может пережить полный цикл tag и принять ABA. Для abandoned stack это `2^22` push transitions. Production abandonment сейчас неактивен, public abandoned operations unsafe, а корректный достижимый interleaving не доказан. **Риск: LOW–MEDIUM, уверенность: MEDIUM как теоретический residual.**

### U-R4-2 — wrap 8-bit generation remote-free guard

Hardened generation table использует `AtomicU8`; код сам принимает совпадение stale note после 256 reissues (`src/alloc_core/segment_header.rs:1590-1600`). Для эксплуатации требуется stale/duplicate remote-free note, переживший необходимые циклы; при корректном обычном caller такой источник не доказан. **Риск: MEDIUM как defence-in-depth residual, уверенность: HIGH в существовании wrap, LOW–MEDIUM в production-достижимости.**

### U-R4-3 — atomic write/plain read одного `magic`

Large cache/reclaim зануляет `magic` через `AtomicU32::store` (`src/alloc_core/alloc_core.rs:906-927`, `src/alloc_core/alloc_core_large.rs:390-406`), но `SegmentHeader::magic_at` читает те же bytes plain `u32` load (`src/alloc_core/segment_header.rs:686-696`, `src/alloc_core/node.rs:270-273`). Реальная одновременность этих access-ов была бы atomic/non-atomic data race. Текущие комментарии допускают её только при stale/duplicate remote free, уже нарушающем `GlobalAlloc` caller contract; well-formed internal race не найден. **Риск: MEDIUM hardening residual, уверенность: MEDIUM.**

### U-R4-4 — `AllocCore::drop` без quiescence remote producers

Drop освобождает cache и все table reservations без handshake (`src/alloc_core/alloc_core.rs:1755-1805`). Сейчас standalone `AllocCore` не `Sync`, а registry heaps живут до конца процесса, поэтому корректный concurrent drop не строится. Изменение Send/Sync или heap teardown немедленно реактивирует UAF-риск. **Риск: LOW, latent; уверенность: MEDIUM.**

### U-R4-5 — untagged deferred-large stack и общий `next_abandoned`

Deferred-large использует untagged `AtomicPtr`, опираясь на single consumer/exactly-once publication (`src/alloc_core/deferred_large/push.rs:19-74`, `drain.rs:26-76`). Global abandoned stack повторно использует тот же intrusive field; код документирует reactivation hazard (`src/registry/heap_registry.rs:290-320`). Production abandonment сейчас неактивен и double-push guard присутствует, поэтому текущий valid-call-graph UAF не подтверждён. **Риск: LOW–MEDIUM, latent; уверенность: MEDIUM.**

### U-R4-6 — Windows NUMA alignment arithmetic не имеет явной checked-fit проверки

`crates/numa/src/lib.rs:667-703` проверяет `size + align`, но округляет `raw_u + align - 1` обычным сложением и передаёт результат в unsafe `Reservation::from_raw_parts`, тогда как Windows `vmem` path явно использует checked align-up и range-fit (`crates/vmem/src/lib.rs:348-423`). Валидный `VirtualAllocExNuma` region по OS contract не должен пересекать конец адресного пространства, поэтому реальный overflow не доказан; это разрыв локального статического доказательства, а не подтверждённый UB. **Риск: LOW, уверенность: HIGH в code gap, LOW в достижимости.**

## Проверенные области без новой подтверждённой находки

- `crates/vmem`: RAII ownership reservation, `into_parts`/`from_raw_parts`, release/decommit/recommit и Unix/Windows alignment/fit paths — новых double-release/Layout дефектов не найдено.
- `crates/numa`: Linux bind path и публичный `unsafe bind_range` корректно оставляют validity caller-у; кроме U-R4-6 новой подтверждённой ошибки нет.
- `RemoteFreeRing` и `HeapOverflow`: cursor wrap, reserve/publish ordering, single-consumer drain и stop-at-unpublished логика статически согласованы; исправление возврата `h` присутствует.
- Experimental `AtomicSlot`/epoch region: pin/double-check/defer-destroy и Send/Sync bounds не дали нового подтверждённого UAF при заявленных contracts.
- Segment table/hash/cache/recycled-slot lifecycle: текущий основной register/unregister/recycle порядок не показывает новой double-release или stale-cache выдачи.

## Приоритет исправлений

1. Убрать safe raw-pointer preconditions: сделать `AllocCore`/`HeapCore::{dealloc,realloc}` unsafe либо ввести неподлежащее подделке allocation handle/identity + exact block start/class/extent/liveness validation. Segment membership/span недостаточны.
2. Сделать registry control plane непубличным: `Registry` fields и `HeapSlot::{state,generation}` как минимум `pub(crate)`; integration tests должны идти через test-only cfg/API, который не экспортируется downstream. Перепроверить все public `dbg_pack`/state-machine helpers.
3. Закрыть safe raw-memory hooks (`cfg(test)`/непубличный module) или пометить их `unsafe fn` с формальным validity/alignment/extent/lifetime/exclusivity contract.
4. Для cross-heap raw-pointer routing добавить process-global liveness membership barrier, не разыменовывающий candidate base, либо убрать safe внешний вход.

