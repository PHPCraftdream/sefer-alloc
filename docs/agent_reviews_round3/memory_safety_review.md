# Повторное read-only ревью безопасности памяти — раунд 3

Дата: 2026-07-12  
Объект: текущее рабочее дерево `D:\dev\rust\sefer-alloc`  
Контекст: `docs/agent_reports_round2/memory_safety_audit.md`

## Метод и ограничения

Проведён только статический read-only анализ текущих Rust-исходников, конфигурации и предыдущих отчётов. Git, сборки, тесты, Miri, Loom, fuzzing, бенчмарки и скрипты не запускались. Исходники и существующие файлы не изменялись; создан только этот отчёт. Поэтому выводы о достижимости основаны на сигнатурах, visibility, feature-графе и прослеживании вызовов, а не на динамическом воспроизведении.

`#[doc(hidden)]` ниже не считается границей безопасности: такие `pub` items доступны downstream-коду. `AllocCore` публично реэкспортирован (`src/lib.rs:279-280`), а модули `alloc_core`, `global` и `registry` публичны под соответствующими features (`src/lib.rs:240-257`). Кроме того, `global::fallback::with_heap` безопасно выдаёт closure ссылку `&mut HeapCore` (`src/global/fallback.rs:231-258`).

## Итог

Подтверждены три оставшихся класса проблем:

| ID | Severity | Статус | Кратко |
|---|---|---|---|
| R3-MS-1 | **CRITICAL** | подтверждено | Safe `AllocCore::realloc` и `HeapCore::realloc` всё ещё не валидируют allocation identity/lifetime/extent; возможны self-copy/overlap в `copy_nonoverlapping`, чтение decommitted/stale памяти и ошибочный free |
| R3-MS-2 | **CRITICAL** | подтверждено | Safe `dealloc` принимает interior/неверно описанные указатели; Large-путь может освободить живой segment, Small-путь — внедрить interior pointer во free list; foreign-путь `HeapCore` сначала разыменовывает непроверенный segment base |
| R3-MS-3 | **HIGH** | подтверждено | Публичные safe raw-memory/test hooks по-прежнему выполняют unchecked чтение/запись по caller-controlled base; добавленные asserts закрывают лишь отдельные index/alignment случаи |

Из четырёх пунктов раунда 2 полностью закрыты R2-2 и R2-4. R2-1 и R2-3 исправлены лишь частично и остаются подтверждёнными в уточнённой форме.

## Подтверждённые находки

### R3-MS-1 — CRITICAL — safe `realloc` не доказывает, что source является живой allocation нужного размера

**Файлы и строки**

- `src/alloc_core/alloc_core.rs:1313-1368`: публичный safe `AllocCore::realloc`; membership проверяется по segment base, move-leg вызывает `Node::copy_nonoverlapping`.
- `src/alloc_core/alloc_core.rs:1354-1355`, `1371-1411`: новый `safe_payload_read_span` ограничивает размер только остатком физического segment span.
- `src/alloc_core/alloc_core.rs:1512-1554`: in-place решение выводится из caller-controlled `old_layout` и segment kind, без проверки начала/жизни фактического блока.
- `src/registry/heap_core.rs:1468-1612`: тот же дефект в own и cross-heap move legs `HeapCore::realloc`.
- `src/registry/heap_core.rs:1593-1600`: foreign leg считает `SegmentHeader::magic_at(base)` membership-барьером, хотя сам этот вызов уже читает по непроверенному адресу.
- `src/alloc_core/node.rs:146-154`: фактический unsafe sink — `core::ptr::copy_nonoverlapping` с обязательными validity и non-overlap предусловиями.

**Доказательство**

`contains_base(base)` доказывает только, что segment зарегистрирован в конкретном `AllocCore`. `safe_payload_read_span` вычисляет `segment_end - ptr`; он не доказывает, что `ptr` — начало текущей allocation, что allocation жива, что `old_layout` ей соответствует и что новый блок не пересекает заявленный source range. Для Small/Primordial функция безусловно использует весь `os::SEGMENT` (`alloc_core.rs:1402-1411`), несмотря на то что фактический блок может иметь 16 байт, а при `alloc-decommit` payload пустого pooled segment может быть decommitted. Комментарий о «fully committed» на строках 1405-1408 поэтому также не является достаточным инвариантом во всех feature-конфигурациях.

Особенно сильный контрпример — stale self-copy. Safe вызовы могут выделить `p` класса 16, освободить его, затем вызвать `realloc(p, Layout(32, align), 16)`. Старый и новый классы различаются, поэтому in-place путь не срабатывает; alloc-leg класса 16 может LIFO-переиздать тот же `p`. После этого строка 1366 (или `heap_core.rs:1567`) вызывает `copy_nonoverlapping(p, p, 16)`. Для ненулевого размера равенство source и destination нарушает non-overlap контракт и является UB внутри safe функции. Проверка segment span проходит.

Независимо от self-copy, завышенный, но помещающийся в segment `old_layout` может охватить соседний свежевыделенный destination, давая overlapping copy, либо читать чужие блоки/decommitted pages. После копии `dealloc(ptr, old_layout)` использует тот же недоказанный pointer/layout и может повредить другой класс или освободить весь Large segment.

В cross-heap leg безопасный вызов с `ptr = 1usize as *mut u8` вычисляет base 0 и до любого надёжного membership-доказательства выполняет `magic_at(0)` (`heap_core.rs:1595-1597`), который доходит до raw `u32` read (`segment_header.rs:694-696`). Это invalid read/UB, а не безопасная проверка адреса.

**Сценарий**

1. Создать `AllocCore` или получить `&mut HeapCore` через safe `global::fallback::with_heap`.
2. Safe выделить `p` с `Layout::from_size_align(16, 16)` и safe освободить его с правильным layout.
3. Safe вызвать `realloc(p, Layout::from_size_align(32, 16), 16)`.
4. При обычном LIFO reuse alloc-leg возвращает `p`, после чего allocator сам выполняет UB через `copy_nonoverlapping(p, p, 16)`.

Отдельный сценарий: при `alloc-xthread` вызвать safe `HeapCore::realloc(1usize as *mut u8, valid_layout, new_size)`; foreign leg читает header по base 0 до отклонения указателя.

**Возможные последствия:** немедленное UB, process fault, OOB/UAF/decommitted read, копирование чужих данных, повреждение free lists/segment state, последующая двойная выдача или double-free.

**Уверенность:** **HIGH**. Safe-to-unsafe поток прямой; новая проверка по границе segment математически не устанавливает обязательные предусловия `copy_nonoverlapping`.

### R3-MS-2 — CRITICAL — safe `dealloc` может освободить живой Large segment или поставить interior pointer во free list

**Файлы и строки**

- `src/alloc_core/alloc_core.rs:692-742`: API явно объявляет `dealloc` безопасным.
- `src/alloc_core/alloc_core.rs:746-767`: проверяется только membership вычисленного segment base, затем routing идёт по segment kind.
- `src/alloc_core/alloc_core.rs:768-940`: для любого указателя внутри Large segment освобождается/cache-ится reservation; равенство `ptr` фактическому payload start не проверяется.
- `src/alloc_core/alloc_core.rs:942-960`: Small class берётся из caller-controlled `layout`.
- `src/alloc_core/alloc_core_small.rs:2244-2322`: interior-pointer `% block_size` guard существует только под `hardened` (`2264-2266`); default/`production` без `hardened` допускает запись intrusive next и публикацию offset в BinTable (`2314-2322`).
- `src/registry/heap_core.rs:1094-1105`, `1637-1674`: публичный safe `HeapCore::dealloc` маршрутизирует own/foreign pointer.
- `src/registry/heap_core.rs:1697-1728`: для не-своего base cold path читает `SegmentHeader::magic_at(base)` без process-global membership/liveness доказательства.
- `Cargo.toml:162-203`: `production` включает `fastbin`, но не включает `hardened`; interior guards не являются production-инвариантом.

**Доказательство**

Safe raw-pointer arithmetic `wrapping_add` позволяет caller без unsafe получить interior pointer из значения, возвращённого safe `alloc`. Для Large allocation `p.wrapping_add(1)` имеет тот же registered base. `AllocCore::dealloc` видит `SegmentKind::Large` и освобождает/cache-ит весь reservation, хотя исходный `p` остаётся логически живым. Проверки фактического payload address, allocation identity или layout/header соответствия на own Large пути нет.

Для Small allocation размером не менее 32 байт caller может передать `p.wrapping_add(16)` вместе с layout класса 16. В non-hardened build pointer проходит payload lower bound и `off < bump`; bitmap на 16-байтовом interior offset обычно не помечен free. Код пишет free-list `next` внутрь пользовательского живого блока, отмечает interior offset свободным и ставит его в `BinTable`. Следующая allocation класса 16 может вернуть `p + 16`, перекрывая всё ещё живой блок `p`: двойная выдача/aliasing и последующее повреждение памяти.

`HeapCore::dealloc` добавляет независимую проблему для произвольного foreign pointer: `contains_base` относится только к текущей heap. При false код не обращается к глобальному реестру live segments, а сразу читает magic по вычисленному адресу. Комментарий `heap_core.rs:1698-1710` сам признаёт невозможность отличить live чужой segment от released/unmapped base; это допустимое ограничение unsafe `GlobalAlloc::dealloc`, но не безопасного публичного `HeapCore::dealloc`.

**Сценарий**

1. Safe выделить Large block `p` через `AllocCore::alloc`.
2. Safe вычислить `q = p.wrapping_add(1)`.
3. Safe вызвать `core.dealloc(q, layout)`; reservation `p` освобождается, хотя caller никогда не освобождал начало allocation.

Small-вариант: выделить блок `p >= 32`, вызвать `dealloc(p.wrapping_add(16), Layout(16, 16))`, затем выделить 16 байт и получить interior address живого `p`.

Foreign-вариант: при `alloc-xthread` safe `HeapCore::dealloc(1usize as *mut u8, layout)` доходит до `magic_at(0)` и выполняет invalid raw read.

**Возможные последствия:** use-after-free, unmap живой allocation, allocator metadata/free-list corruption, overlapping allocations, двойная выдача одного участка, invalid header read/process fault.

**Уверенность:** **HIGH**. Все проверки и raw sinks видны непосредственно; защита interior pointer feature-gated и отсутствует в `production`.

### R3-MS-3 — HIGH — публичные safe raw-memory/test hooks всё ещё имеют prose-only memory contracts

**Файлы и строки**

- `src/lib.rs:234-257`, `src/alloc_core/mod.rs:50-90`: doc-hidden модули публичны downstream-коду.
- `src/alloc_core/remote_free_ring.rs:509-532`, `570-588`: `over_test_buffer`/`init_test_buffer` проверяют лишь non-null и 4-byte alignment, затем инициализация пишет ring metadata/slots.
- `src/alloc_core/segment_header.rs:1575-1587`, `1611-1623`, `1656-1662`: `gen_at`, `bump_gen`, `init_gen_table_in_place` остаются safe; index assert не проверяет base/lifetime/writable extent.
- `src/alloc_core/run_stack.rs:206-218`, `232-255`, `279-345`: safe `RunStack` accessors; class asserts добавлены, но base/lifetime/extent не проверяются.
- `src/alloc_core/alloc_core_small.rs:896-975`: safe `flush_class` принимает caller-controlled pointers, не проверяет class range и membership перед `flush_run` metadata access.
- `src/alloc_core/alloc_core_small.rs:487-503`, `511-518`, `538-565`, `579-585`: дополнительные safe `dbg_*` raw accessors либо не имеют membership guard, либо используют только debug-only length checks.
- `src/alloc_core/alloc_core.rs:1105-1245`: часть `dbg_*` получила release-surviving membership assertions; это закрывает конкретные прежние варианты, но не перечисленные выше швы.

**Доказательство**

Release `assert!` на null/alignment/index предотвращает только некоторые простые bogus значения. Он не может доказать, что caller-provided address указывает на live writable buffer нужной длины. Например, `RemoteFreeRing::init_test_buffer(4usize as *mut u8)` проходит обе проверки и затем пишет по адресу 4. `RunStack::init_in_place(8usize as *mut u8)` и `init_gen_table_in_place(1usize as *mut u8)` аналогично доходят до `Node` raw writes. `flush_class` можно вызвать на slice произвольных non-null raw pointers; вычисленный base передаётся в `flush_run`, который материализует `SegmentMeta` до надёжного membership-доказательства.

Пометка test-only и `#[doc(hidden)]` снижает практическую вероятность, но не меняет safe API contract опубликованного crate. Запрет `unsafe_code` внутри модуля не делает композицию безопасной: unsafe разыменование перенесено в safe `Node` membrane, поэтому внешний safe caller всё равно может привести библиотеку к UB.

**Сценарий**

Downstream crate включает соответствующий opt-in feature и без unsafe вызывает один из публичных хуков с выровненным, но недействительным адресом либо слишком коротким buffer. Библиотека выполняет invalid/OOB/misaligned typed read/write или создаёт `&'static Atomic*` над памятью без требуемой lifetime.

**Возможные последствия:** invalid/OOB write/read, metadata corruption, invalid atomic reference, process fault; при `flush_class` — повреждение allocator state или UAF metadata.

**Уверенность:** **HIGH**. Safe signatures, public reachability и raw sinks подтверждены по текущему коду.

## Исправленные/закрытые пункты

### R2-2 — закрыто: safe abandoned-stack API

`HeapRegistry::push_abandoned_segment` и `pop_abandoned_segment` теперь `pub unsafe fn` с явными mapped/lifetime контрактами (`src/registry/heap_registry.rs:373-432`). Safe chain «опубликовать base standalone `AllocCore` → drop/unmap → safe pop» больше не компилируется без unsafe у caller. Внутренние вызовы остаются обязаны обеспечивать lifetime, но исходная safe-boundary unsoundness закрыта.

### R2-4 — закрыто: HeapOverflow cache после partial drain

`HeapOverflow::drain` теперь возвращает фактически опубликованный `h`, а не entry-time `tail` (`src/registry/heap_overflow.rs:339-397`). `HeapCore` кэширует этот результат (`src/registry/heap_core.rs:770-797`), поэтому reserved-but-unpublished slot оставляет cache отличным от tail и последующий drain не подавляется.

### R2-1 — только частично исправлено, не закрыто

Добавлены own-segment membership и segment-span bounds (`alloc_core.rs:1325-1355`, `heap_core.rs:1548-1556`), а non-xthread foreign leg возвращает null. Это закрывает часть чтений за физическим концом reservation. Однако segment span не равен allocation extent/lifetime и не доказывает non-overlap; foreign `magic_at` сам разыменовывает непроверенный base. Поэтому итоговый статус — R3-MS-1, подтверждено.

### R2-3 — только частично исправлено, не закрыто

Некоторые `debug_assert!` заменены на release `assert!`: class bounds в `RunStack`, index bounds generation table, alignment/null для ring test buffer, membership для ряда `AllocCore::dbg_*`. Эти изменения закрывают конкретные out-of-range/null варианты, но prose-only base/lifetime/extent contracts остались у нескольких public safe функций. Итоговый статус — R3-MS-3, подтверждено.

### Дополнительно перепроверенные закрытые hardening-пункты

- ABA tag больше не сбрасывается при переходе `free_slots` в empty: runtime empty сохраняет текущий tag (`src/registry/heap_registry.rs:698-746`, `src/registry/tagged_ptr.rs:124-154`).
- Аналогичный empty-transition fix присутствует для abandoned stack (`src/registry/heap_registry.rs:446-475`).
- `SegmentHeader::kind_at` строго декодирует неизвестный discriminant в `Unknown`, а не в `Small` (`src/alloc_core/segment_header.rs:650-683`).
- Payload lower bound и `off >= bump` guards присутствуют в small reclaim/dealloc путях; stale post-decommit offset отклоняется (`src/alloc_core/alloc_core_small.rs:2268-2303` и парные reclaim paths).
- `flush_class` отслеживает уже recycled bases внутри одного вызова и не трогает повторно потенциально unmapped metadata (`src/alloc_core/alloc_core_small.rs:896-974`).
- HeapOverflow partial-publication cache bug закрыт, как описано выше.

Эти пункты оценены статически; регрессионные тесты в рамках данного read-only ревью не запускались.

## Неподтверждённые риски и принятые остатки

Ниже нет достаточного статического доказательства UB при соблюдении заявленных unsafe/ownership контрактов, но области остаются чувствительными.

### U-R3-1 — ABA после полного wrap tag

`free_slots` использует 48-bit tag, abandoned stack — только 22 low bits (`src/registry/bootstrap.rs:199-203`). Empty-reset исправлен, но любой конечный tag теоретически допускает ABA, если CAS-поток приостановлен на полный цикл счётчика. Для abandoned stack это около 4,2 млн push transitions. Production-достижимость abandon/adopt пути ограничена, API теперь unsafe; реальный interleaving статически не подтверждён. **Риск: LOW–MEDIUM, уверенность: MEDIUM как теоретический residual.**

### U-R3-2 — generation wrap в hardened cross-thread ring

Generation table использует `u8`; совпадение stale entry после 256 жизней блока является документированным остатком. Для эксплуатации требуется stale/duplicate free и сохранение ring entry через полный цикл reuse. При корректном `GlobalAlloc` caller сценарий не подтверждён. **Риск: LOW, уверенность: HIGH в существовании wrap / LOW в достижимости без нарушения контракта.**

### U-R3-3 — stale/unmapped cross-thread headers под unsafe `GlobalAlloc`

`HeapCore::dealloc_foreign_slow` не имеет process-global live-segment lookup и читает header после проверки только «не в моей table» (`heap_core.rs:1697-1728`). Для public safe `HeapCore` это включено в подтверждённый R3-MS-2. Для штатного вызова из `unsafe impl GlobalAlloc` invalid/stale pointer уже нарушает caller contract; дополнительного well-formed production UAF не доказано. **Риск: MEDIUM как hardening residual, уверенность: MEDIUM.**

### U-R3-4 — `AllocCore::drop` против in-flight remote operations

`Drop` освобождает зарегистрированные reservations (`src/alloc_core/alloc_core.rs:1750-1803`) без quiescence handshake. Standalone `AllocCore` не `Sync`, registry heaps не дропаются обычным путём, а unsafe abandonment APIs теперь несут lifetime contract. Well-formed concurrent drop не найден, но будущая смена Send/Sync/ownership модели может реактивировать UAF. **Риск: LOW (latent), уверенность: MEDIUM.**

### U-R3-5 — lock-free intrusive stacks и общий `next_abandoned`

Deferred-Large stack использует untagged `AtomicPtr` и intrusive `next_abandoned`; soundness опирается на single consumer, exactly-once publication и отсутствие одновременного участия segment в abandoned stack (`src/alloc_core/deferred_large/push.rs:19-74`, `drain.rs:26-76`). Double-push guard присутствует, а abandoned APIs unsafe. Нарушение при корректном текущем call graph не подтверждено, но reactivation abandonment или новый consumer требует отдельного Loom/Miri-аудита. **Риск: LOW–MEDIUM (latent), уверенность: MEDIUM.**

## Проверенные области без новых подтверждённых дефектов

- Исправленные tagged free-slot/abandoned-stack empty transitions и generation-saturation handling в experimental `EpochRegion`/`AtomicSlot`.
- Large-cache publish ordering и field-wise atomic invalidation magic; fresh header пишется до регистрации cache-hit segment.
- `vmem::Reservation` ownership/`into_parts`/`release`, checked address arithmetic и NUMA RAII handoff.
- Small reclaim guards: kind/class/bounds/payload lower bound/bump, bitmap double-free oracle, recycled-base containment.
- RemoteFreeRing/HeapOverflow publish ordering и stop-at-unpublished discipline.

Это не доказательство общей корректности: особенно конкурентные отрицательные выводы ограничены статическим чтением без Loom/TSan/Miri.

## Приоритет исправлений

1. Сделать `AllocCore::realloc`, `HeapCore::realloc`, `AllocCore::dealloc` и `HeapCore::dealloc` unsafe либо ввести allocation identity/extent/liveness validation, достаточную для safe сигнатуры. Проверка только segment membership/span недостаточна.
2. До любой копии гарантировать оба диапазона и non-overlap; stale/self-reuse должен отклоняться до alloc/copy либо копирование должно происходить по доказанному фактическому extent с корректной overlap-семантикой. Простая замена на `ptr::copy` не исправит stale lifetime/ошибочный dealloc.
3. Для cross-heap маршрутизации добавить безопасный process-global membership/liveness барьер, который не разыменовывает candidate base, либо убрать safe публичный raw-pointer вход.
4. Закрыть public safe test hooks за непубличной/test-only границей или сделать их `unsafe fn` с формальным контрактом. Одни asserts на alignment/index не обеспечивают validity/lifetime/extent.
