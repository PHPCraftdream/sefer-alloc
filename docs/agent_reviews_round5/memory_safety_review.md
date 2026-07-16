# Повторное read-only ревью безопасности памяти — round 5

Дата: 2026-07-14  
Объект: текущее дерево `sefer-alloc`  
Контекст: `docs/agent_reviews_round4/memory_safety_review.md` и текущие исходники после последних исправлений

## Метод и ограничения

Ревью выполнено статическим чтением Rust-исходников, `Cargo.toml`, конфигурации и предыдущего отчёта. Git, сборка, тесты, Miri/Loom/Kani, бенчмарки, fuzzing и проектные скрипты не запускались. Существующие файлы не изменялись; создан только этот отчёт.

Повторно проверены публичные safe/raw-pointer границы, small/large alloc/dealloc/realloc, freelist/bitmap/magazine, remote-free ring и heap overflow, deferred-large, registry/TLS ownership, slot generations/ABA, segment table lifecycle, decommit/pool/cache/release, `Layout`/size/alignment, confined-unsafe seams, `vmem`, NUMA и experimental concurrent tiers.

## Итог

Последние исправления полностью закрыли R4-MS-4 (внешнее вмешательство в registry state machine) и большую часть конкретных raw-memory hooks из R4-MS-3. Однако R4-MS-1 и R4-MS-2 не закрыты: публичные safe `realloc`/`dealloc` всё ещё принимают недоказанные raw pointers и caller-supplied `Layout`. Кроме того, остались публичные doc-hidden safe batch/ring hooks, через которые safe downstream-код может непосредственно вызвать invalid raw access либо детерминированно освободить уже переизданный блок.

| ID | Severity | Статус | Кратко |
|---|---:|---|---|
| R5-MS-1 | **CRITICAL** | подтверждено, R4-MS-1 не закрыт | Safe `realloc` не проверяет начало, liveness и фактический class/extent блока; возможны resurrection свободного блока, двойная выдача и UB в `copy_nonoverlapping` |
| R5-MS-2 | **CRITICAL** | подтверждено, R4-MS-2 не закрыт | Safe `dealloc` принимает interior/mismatched/stale pointers; возможны release живого Large segment, freelist/tcache corruption и освобождение текущего владельца stale-указателем |
| R5-MS-3 | **HIGH** | подтверждено, остаток R4-MS-3 | Safe doc-hidden `flush_class` и некоторые диагностические class/raw-pointer hooks выполняют unchecked metadata/block access |
| R5-MS-4 | **HIGH** | подтверждено | Safe ring test hooks позволяют в production-feature конфигурации оставить stale note, освободить переизданный живой блок и выдать тот же адрес повторно |

## Подтверждённые находки

### R5-MS-1 — CRITICAL — safe `realloc` не устанавливает identity/liveness/extent исходного allocation

**Файлы/строки**

- `src/alloc_core/alloc_core.rs:1377-1432` — публичный safe `AllocCore::realloc`; membership проверяется только по segment base, затем выполняются in-place либо alloc/copy/dealloc.
- `src/alloc_core/alloc_core.rs:1465-1475` — `safe_payload_read_span` ограничивает чтение физическим остатком segment span, а не размером фактически выданного блока.
- `src/alloc_core/alloc_core.rs:1576-1618` — in-place Large/same-small-class ветви доверяют `ptr` и `old_layout`, не проверяя block start, bitmap liveness и реальный class.
- `src/registry/heap_core.rs:1364-1465,1489-1523` — те же move/in-place проблемы в `HeapCore::realloc`.
- `src/alloc_core/node.rs:146-154` — unsafe sink `core::ptr::copy_nonoverlapping` требует валидных непересекающихся диапазонов.

**Доказательство**

`contains_base(base)` доказывает лишь, что вычисленный base сейчас зарегистрирован в этом `AllocCore`. Для Small/Primordial `safe_payload_read_span` возвращает остаток до конца всего 4 MiB segment. Ни одна из этих проверок не доказывает, что `ptr` — начало живого allocation, что bitmap отмечает его allocated, что `old_layout` соответствует реальному class и что destination не совпадёт с source.

Два полностью safe контрпримера:

1. Выделить Small `p`, вызвать safe `dealloc(p, layout)`, затем safe `realloc(p, layout, layout.size())`. Same-class in-place ветвь возвращает `p`, не удаляя его из freelist и не меняя bitmap. Следующий `alloc(layout)` также выдаёт `p`: два логически живых allocation имеют один адрес.
2. Выделить 16-byte `p`, освободить его, затем вызвать `realloc(p, old_layout=32, new_size=16)`, удерживая другой живой блок в segment, чтобы segment не был освобождён. Классы различаются, fresh 16-byte allocation может LIFO-путём снова вернуть `p`, после чего выполняется `copy_nonoverlapping(p, p, 16)`. Для ненулевой длины source и destination пересекаются полностью — это непосредственное UB внутри safe функции.

Дополнительно interior pointer в Small/Large segment может пройти membership и in-place predicate; функция способна вернуть interior address как будто это исходное allocation либо изменить `large_size` текущего Large header по недоказанному `old_layout`.

**Сценарий воздействия**

- Двойная выдача одного блока и одновременные mutable owners.
- Overlapping `copy_nonoverlapping`, OOB/stale read и последующий ошибочный `dealloc`.
- Повреждение соседнего allocation через возвращённый interior pointer.

**Уверенность:** **HIGH**. Все переходы видны в текущем коде; первый контрпример не требует даже move-copy и напрямую показывает отсутствие liveness bookkeeping в in-place ветви.

### R5-MS-2 — CRITICAL — safe `dealloc` не валидирует allocation identity, block start и Layout

**Файлы/строки**

- `src/alloc_core/alloc_core.rs:744-809` — API сознательно оставлен safe и описывает defensive-free contract.
- `src/alloc_core/alloc_core.rs:813-1027` — после segment membership маршрут выбирается по header kind; Large освобождает/cache-ит весь segment, Small class берётся из caller-supplied `Layout`.
- `src/alloc_core/alloc_core_small.rs:1022-1100` — interior-pointer modulo guard существует только под `hardened` (`:1042-1045`); production путь пишет intrusive `next`, head и bitmap.
- `src/registry/heap_core.rs:985-995,1037-1107,1216-1307` — magazine path классифицирует по переданному Layout; Large-kind и block-start guards включены только под `hardened` (`:1075-1107`).
- `Cargo.toml:164-165,188-205` — `production` включает `fastbin`/`alloc-xthread`/`alloc-decommit`, но не `hardened`.
- `src/registry/heap_core_xthread.rs:222-350` — foreign slow path отвергает только null base перед `magic_at`; произвольный ненулевой unmapped base всё ещё читается raw.

**Доказательство**

1. **Large interior free.** `alloc_large` возвращает payload по `base + hdr_aligned` (`src/alloc_core/alloc_core_large.rs:227,306-326`). Safe `dealloc(p.wrapping_add(1), layout)` вычисляет тот же base, видит `SegmentKind::Large`, читает header и unregister/release/cache-ит весь reservation. Проверки `ptr == base + hdr_aligned` и соответствия layout/header нет. Исходный `p` остаётся логически живым, но становится dangling.
2. **Small interior injection.** В non-`hardened` сборке `dealloc(p.wrapping_add(16), Layout(16,16))` для большего живого блока может пройти payload/bump/bitmap guards: bitmap имеет гранулярность 16 bytes и interior cell выглядит allocated. `Node::write_next` пишет allocator metadata в середину живого payload, а interior address попадает во freelist/magazine и позже выдаётся как отдельный allocation.
3. **Layout mismatch.** Для Small class выбирается из переданного Layout, а не из allocation identity. Освобождение меньшего реального блока как большего class может позднее выдать диапазон, заходящий в соседний живой блок; обратное несоответствие помещает адрес не в тот freelist и ломает placement/class invariants.
4. **Stale-after-reuse.** `p=alloc(L); dealloc(p,L); q=alloc(L)` может дать `q == p`. Повторный safe `dealloc(p,L)` неотличим для bitmap от легитимного free текущего `q` и освобождает его. Следующий alloc повторно выдаёт адрес, пока `q` остаётся живым. Информационная невозможность обнаружить stale alias не делает safe API sound: предусловие обязано быть выражено `unsafe`-границей или типом владения.
5. **Ненулевой foreign/unmapped pointer в `HeapCore`.** Исправление `base.is_null()` закрывает только `1 as *mut u8`. Для, например, ненулевого SEGMENT-aligned unmapped адреса `dealloc_foreign_slow` и foreign realloc leg всё ещё вызывают plain raw `SegmentHeader::magic_at(base)` до process-global membership/liveness proof.

**Сценарий воздействия**

- Release/cache живого Large reservation, UAF и double release при последующем штатном free.
- Запись freelist metadata в живой payload, перекрывающиеся allocations и двойная выдача.
- Освобождение текущего владельца stale raw pointer после address reuse.
- Invalid read/fault/UB на произвольном foreign base в cross-heap маршруте.

**Уверенность:** **HIGH** для standalone `AllocCore` Large/Small/stale/Layout сценариев; **MEDIUM-HIGH** для произвольного foreign `HeapCore` адреса из-за необходимости сначала законно получить receiver/heap handle, но после этого safe метод не несёт дополнительных unsafe-предусловий на `ptr`.

### R5-MS-3 — HIGH — публичные safe batch/debug hooks всё ещё имеют unchecked raw contracts

**Файлы/строки**

- `src/lib.rs:244-252` — `alloc_core` реально публичный doc-hidden module; `AllocCore` публично re-exported.
- `src/alloc_core/alloc_core_small_magazine.rs:264-338` — публичный safe `flush_class(class_idx, blocks)` принимает caller-controlled raw pointers.
- `src/alloc_core/alloc_core_small_magazine.rs:379-417,433-550` — base выводится арифметически; до `SegmentMeta`, BinTable, bitmap, bump и block-body reads/writes нет membership/lifetime проверки.
- `src/alloc_core/segment_header.rs:925-939` — `BinTable::head/set_head` имеют лишь `debug_assert!(c < SMALL_CLASS_COUNT)` и затем raw pointer arithmetic.
- `src/alloc_core/alloc_core_small_diag.rs:38-61` — safe `dbg_freelist_head_for` проверяет base membership, но не release-проверяет `class_idx` перед `BinTable::head`.

**Доказательство**

`AllocCore::flush_class(0, &[1usize as *mut u8])` доступен safe downstream-коду. `segment_base_of_ptr` даёт null base, после чего `flush_run` немедленно создаёт metadata views и читает BinTable/bump/kind по invalid address. В отличие от исправленных ring/run/gen helpers, метод не `unsafe fn` и не делает `contains_base` до raw access.

Даже с валидным owned base caller может передать interior pointer: payload/bump/bitmap guards не доказывают кратность offset реальному `block_size(class_idx)`, после чего `Node::write_next` пишет в середину allocation. Произвольный `class_idx` дополнительно обходит release bounds: `BinTable::head` защищён только исчезающим `debug_assert!`, а `heads + c * 4` становится OOB raw read/write.

**Сценарий воздействия**

- Immediate invalid/OOB metadata read/write из safe вызова.
- Freelist corruption и запись в живой allocation через interior block.
- Позднейшая выдача forged/interior адреса и перекрытие allocations.

**Уверенность:** **HIGH**. Минимальный invalid-base контрпример прямой; `#[doc(hidden)]` и имя batch/test surface не меняют safe API contract.

### R5-MS-4 — HIGH — safe ring hooks позволяют освободить переизданный живой блок

**Файлы/строки**

- `src/alloc_core/alloc_core_small_reclaim.rs:324-362` — safe `dbg_push_to_ring` публикует `(off,class)` для любого pointer, чей segment принадлежит core; liveness/единственность free не проверяются.
- `src/alloc_core/alloc_core_small_reclaim.rs:365-430` — safe `dbg_drain_all_rings` принудительно дренирует notes с always-false magazine predicate.
- `src/alloc_core/alloc_core_small.rs:43-49` — alloc сначала pop-ит current freelist и может переиздать адрес до ring drain.
- `src/alloc_core/alloc_core_small_reclaim.rs:136-189` — в non-`hardened` ветви current generation не сравнивается; bitmap `allocated` достаточен для повторного free.
- `Cargo.toml:164-165,188-205` — `production` имеет `alloc-xthread`, но не `hardened`; generational guard отсутствует.

**Доказательство**

В `production` feature set возможна полностью safe последовательность на standalone `AllocCore`:

1. `p = alloc(L)`; получить class через safe diagnostic/class helper.
2. `dbg_push_to_ring(p, class)` оставляет отложенный note.
3. `dealloc(p, L)` помещает `p` в current BinTable.
4. `q = alloc(L)` сначала pop-ит current freelist и может вернуть `q == p`, не дренируя note.
5. `dbg_drain_all_rings()` обрабатывает старый note. Bitmap текущего `q` имеет состояние allocated, magazine predicate всегда false, а generation check скомпилирован только под `hardened`; drain выполняет `write_next/mark_free` для живого `q`.
6. Следующий `r = alloc(L)` снова возвращает тот же адрес, пока `q` жив.

Это детерминированная stale-note → double-issue цепочка без потоков, unsafe блоков или нарушения типов downstream-кодом. Она также материализует прямо описанный в комментариях residual reissue-before-drain, но здесь источник stale note — публичный safe test hook.

**Сценарий воздействия**

Два живых владельца одного диапазона, запись allocator metadata в payload текущего владельца, последующие use-after-free/double-free и произвольное повреждение данных.

**Уверенность:** **HIGH**. Порядок `pop_free` до ring scan и отсутствие generation check в `production` видны непосредственно в текущих cfg-ветвях.

## Исправленные/закрытые пункты

### R4-MS-4 закрыт: registry control state больше не мутируется safe downstream-кодом

`Registry::{slots,count,free_slots}` теперь `pub(crate)` (`src/registry/bootstrap.rs:188-209`), а `HeapSlot::{state,generation,heap,next_free,initialised}` — `pub(crate)` (`src/registry/heap_slot.rs:238-324`). Внешние test accessors дают только чтение; preset generation и `HeapRegistry::recycle` стали `unsafe fn` (`bootstrap.rs:258-274`, `heap_registry.rs:281-307`). Прежний safe takeover LIVE slot через forged `free_slots` head больше не строится.

### Большая часть R4-MS-3 raw hooks закрыта unsafe-границами

- `RemoteFreeRing::over_test_buffer/init_test_buffer` — `unsafe fn` (`src/alloc_core/remote_free_ring.rs:511-540`).
- Все raw-base операции `RunStack` — `unsafe fn` (`src/alloc_core/run_stack.rs:214-394`).
- `gen_at`, `bump_gen`, `init_gen_table_in_place` — `unsafe fn` (`src/alloc_core/segment_header.rs:1576-1674`).
- `numa::bind_segment` — `unsafe fn` (`src/alloc_core/numa.rs:58-70`).
- Наиболее опасные raw bitmap/freelist diagnostics переведены в `unsafe fn` (`src/alloc_core/alloc_core_small_diag.rs:68-218`), `dbg_unregister/dbg_recycle` также unsafe (`src/alloc_core/alloc_core.rs:1264-1310`).

R4-MS-3 закрыт лишь частично: R5-MS-3/R5-MS-4 перечисляют оставшиеся safe поверхности.

### Закрыт точный null-base foreign-header контрпример

`HeapCore` foreign dealloc/realloc теперь проверяют `base.is_null()` до `magic_at` (`src/registry/heap_core_xthread.rs:253-265`, `src/registry/heap_core.rs:1491-1507`). Это закрывает прежний пример `1usize as *mut u8`, но не любой иной ненулевой unmapped base; остаток учтён в R5-MS-2.

### Закрыты ранее подтверждённые registry/layout/lifecycle дефекты

- Slot generation расширен до `AtomicU64`, а initialisation publication отделён `initialised` Release/Acquire gate (`src/registry/heap_slot.rs:247-324`).
- Empty transition free-slot stack сохраняет running tag; public control atomics скрыты.
- `HeapOverflow::drain` возвращает реальный stop cursor; unpublished slot не становится навсегда невидимым (`src/registry/heap_overflow.rs:339-397`).
- `flush_class` помнит recycled bases внутри production-size batch и не повторно касается уже released segment (`src/alloc_core/alloc_core_small_magazine.rs:339-415`). Это не защищает arbitrary initial base и over-cap test slice, поэтому R5-MS-3 остаётся.
- Generation/ring/run raw indices получили release guards и unsafe contracts; reclaim offset имеет unconditional class/payload/bump/alignment checks.
- Windows/Unix aligned reservation использует checked address fit (`crates/vmem/src/lib.rs:361-436,550-628`); прежний NUMA/alignment arithmetic риск по просмотренному коду закрыт.

### Не закрыты R4-MS-1 и R4-MS-2

Добавленная `safe_payload_read_span` предотвращает только выход copy за физический segment span. Она не восстанавливает allocation identity, liveness или extent, поэтому прежние findings не считаются исправленными.

## Неподтверждённые риски

### U-R5-1 — atomic write/plain read одного `magic`

Large cache/reclaim зануляет `magic` через `AtomicU32::store` (`src/alloc_core/alloc_core.rs:964-966`, `src/alloc_core/alloc_core_large.rs:389-406`), а `SegmentHeader::magic_at` читает те же bytes plain `Node::read_u32` (`src/alloc_core/segment_header.rs:675-685`). Реальная одновременность создаёт atomic/non-atomic data race. В текущем штатном протоколе она требует stale/duplicate remote free, то есть отдельного нарушения allocation ownership; well-formed внутренний interleaving не доказан. **Риск: MEDIUM; уверенность: HIGH в конфликте access kinds, MEDIUM в достижимости без уже существующей misuse.**

### U-R5-2 — конечный wrap 48-bit ABA tag free-slot stack

`TaggedPtr` оставляет 48 bits для tag (`src/registry/tagged_ptr.rs:65-117`). Теоретически остановленный CAS может пережить полный цикл и принять ABA после `2^48` transitions. Практически достижимый process-lifetime сценарий не найден. **Риск: LOW; уверенность: HIGH математически, LOW по эксплуатации.**

### U-R5-3 — wrap 8-bit hardened generation

Hardened remote-free table использует один `AtomicU8` на 16-byte cell; после 256 переизданий stale note может снова совпасть с current generation (`src/alloc_core/segment_header.rs:1593-1631`, `src/alloc_core/alloc_core_small_reclaim.rs:148-175`). Нужен источник note, переживший все циклы; при корректном caller такой источник не доказан. **Риск: MEDIUM как defence-in-depth residual; уверенность: HIGH в wrap, LOW-MEDIUM в достижимости.**

### U-R5-4 — `AllocCore::drop` без quiescence remote producers

Drop освобождает cache/table reservations без remote-producer handshake (`src/alloc_core/alloc_core.rs:1725-1805`). Сейчас standalone `AllocCore` не `Sync`, а registry heaps process-lifetime, поэтому корректный concurrent drop не строится. Будущая Send/Sync или реальное уничтожение registry heap реактивирует metadata UAF. **Риск: LOW, latent; уверенность: MEDIUM.**

### U-R5-5 — deferred-large post-reuse stale free с тем же layout

Double-push CAS закрывает duplicate queueing до drain, а `large_layout_consistent` отбрасывает большинство post-reuse stale frees. Если новый Large allocation повторно получил тот же base и тот же logical size, stale free неотличим и может queue/reclaim текущий segment (`src/registry/heap_core_xthread.rs:282-307`, `src/alloc_core/deferred_large/layout_consistent.rs`). Для штатного `GlobalAlloc` это требует double/stale free caller-а; независимый корректный источник не найден. **Риск: MEDIUM hardening residual; уверенность: MEDIUM.**

## Проверенные области без новой подтверждённой находки

- `SegmentTable` register/unregister/recycle/hash/backshift и cache ownership transfer.
- Small live-count, pool admission/eviction, reset/decommit/recommit/release order.
- RemoteFreeRing cursor publication, HeapOverflow two-word publication и drain stop handling.
- Deferred-large claim-before-head-CAS и single-consumer pop protocol.
- Registry claim/materialise/recycle, initialised publication, TLS teardown sentinels и fallback lock recovery.
- Large cache size/budget/span ownership, reservation transfer и `vmem` exact-layout release.
- NUMA reservation/binding FFI contracts по текущим platform cfg.
- `Region`/`SyncRegion` safe slotmap layer и experimental `AtomicSlot` epoch reclamation: нового доказанного UAF/double-free/aliasing interleaving не найдено статически.

Отсутствие подтверждённой находки в этих областях не заменяет не запускавшиеся Miri/Loom/Kani/fuzz/stress проверки.

## Приоритет исправлений

1. Сделать raw-pointer `dealloc`/`realloc` unsafe на публичных substrate/heap поверхностях либо заменить их capability/ownership API; внутренние `GlobalAlloc` adapters могут вызывать unsafe core после своего contract boundary.
2. До смены API добавить unconditional allocation-start/kind/class/bitmap checks: Large payload equality, Small offset modulo фактическому class, liveness и layout-vs-metadata verification. Это уменьшит impact, но stale-after-address-reuse полностью не решит.
3. Перевести `flush_class`, `dbg_push_to_ring` и другие mutating raw-pointer test hooks в `unsafe fn` либо сделать их `pub(crate)`/отдельной test-only crate surface; release-проверять class bounds и segment membership до любого metadata access.
4. Сделать `magic_at` атомарным load, если zeroing остаётся atomic store, чтобы stale defensive route не превращал caller misuse в Rust data-race UB.
