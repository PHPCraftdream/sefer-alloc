# Read-only ревью безопасности памяти и soundness `unsafe` — round 6

Дата: 2026-10-15
Объект: каталог `src/` текущего дерева `sefer-alloc` (HEAD `132ab85b`)
Контекст: `docs/agent_reviews_round5/memory_safety_review.md`, `docs/agent_reviews_round4/memory_safety_review.md`, индексы `docs/CORRECTNESS_OPEN_ITEMS.md` / `docs/correctness-open-items/`

> Об идентификаторах: находки этого отчёта нумеруются `R6-MS-N` по заданию раунда. В коде и `CHANGELOG.md` уже встречаются исторические теги задач `R6-MS-1/2/3/4/5` (это ремедиация находок round5). Это разные пространства имён: ссылки вида «R6-MS-1/2: `unsafe fn` boundary» в комментариях `src/` относятся к старым задачам, а не к находкам ниже.

## Метод и ограничения

- Только статическое чтение исходников `src/**` (173 файла, ~44 тыс. строк); `crates/` и `tests/` — только как контекст. Сборка, тесты, Miri/Loom/Kani, бенчмарки и проектные скрипты не запускались. Файлы проекта не изменялись, кроме этого отчёта.
- Полностью прочитаны: все tier-1 seam-модули с `#![allow(unsafe_code)]` в production-пути (`global/sefer_alloc/global_alloc.rs`, `global/tls_heap.rs`, `global/fallback.rs`, `global/process_exit.rs`, `global/sefer_alloc/batch.rs`, `registry/heap_slot.rs`, `registry/heap_registry/claim.rs`, `registry/bootstrap/{registry,ensure}.rs`, `registry/segment_route/{directory,shard_lock,small_sidecar}.rs`, `alloc_core/segment/segment_table/route_slots.rs`, `alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs`, `alloc_core/platform/sidecar.rs`), а также `mem_impl.rs`, `realloc_fastpath.rs`, `sidecar_drain.rs`, `alloc_core_small_reclaim.rs`, `alloc_core_small_magazine.rs` (refill/flush), `heap_core/free/{dealloc,dealloc_own_base,dealloc_batch,realloc}.rs`, `heap_core_xthread/*`, `segment_route/{pin,registration,large_state,route_*}.rs`, большая часть `alloc_core_large.rs`, `alloc_core_large_cache.rs`, `lifecycle.rs` (Drop), `os.rs` (reserve_biased), диагностические файлы `alloc_core_core_diag/*`, `alloc_core_small_diag.rs`, `heap_core/diag/*`, `sefer_alloc/diag.rs`, `maintenance_service.rs`. Выборочно: `platform/node.rs`, `segment_table_impl.rs`, `alloc_core_small_pool_impl.rs`, `find_segment.rs`, `segment_directory_impl.rs`, `exact_object/*` (прототип вне `production`), `concurrent/epoch/hand.rs` (research-tier).
- Инвентарь сверен командой `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/`: 97 совпадений — 22 tier-1 (`#![...]`) и 75 tier-2 (`#[...]`). Список 22 tier-1 модулей совпадает с инвентарём в шапке `src/lib.rs` (строки 202–286); незадокументированных seam-модулей не найдено.
- Проверены категории: контракты `unsafe fn`/`// SAFETY:` на всех путях, safe `pub fn` с raw-указателями (правило R25-1), UAF/double free/aliasing, `Layout`/выравнивание, переполнение арифметики размеров, провенанс, uninit/`MaybeUninit`, `UnsafeCell`, порядок Drop, reentrancy аллокатора, TLS-teardown, fork, process-exit, panic/unwind из `GlobalAlloc`.
- Ограничение: отсутствие находки в непрочитанных полностью файлах не доказывает их корректность; динамические проверки (Miri/Loom/fuzz) этот отчёт не заменяет.

## Итог

Находки round5 уровня CRITICAL/HIGH закрыты. Публичные `AllocCore::{dealloc,realloc}` и `HeapCore::{dealloc,realloc}` теперь `unsafe fn` с полным `# Safety`. `flush_class` стал `unsafe fn` под `internals`. Кольцевые хуки удалены вместе с `RemoteFreeRing`. Межпоточное освобождение переписано: foreign-продюсер по адресу находит маршрут в процесс-глобальном `RouteDirectory`, получает refcount-пин на System-аллоцированный дескриптор и делает терминальную публикацию в независимый sidecar. Байты резервации и заголовки сегментов чужого heap продюсер больше не читает и не пишет. Этим сняты прежние гонки atomic/plain на `magic` и риск UAF метаданных при `Drop`.

Новых находок уровня Critical, High или Medium не обнаружено. Найдено 2 Low и 4 Info. Все они касаются либо путей при уже нарушенном контракте вызывающего, либо panic/unwind на `GlobalAlloc`-путях при нарушении внутренних инвариантов, либо неполной документации границы soundness. Нового safe `pub fn`, который принимает raw-указатель и пишет метаданные аллокатора (класс R25-1), не найдено: все такие хуки либо `unsafe fn`, либо только читают через канонический корень таблицы, либо закрыты `internals`/`bench-internals`.

| ID | Серьёзность | Статус | Кратко |
|---|---|---|---|
| R6-MS-1 | Low | подтверждено чтением | `canonical_block_of` делает `ptr::add` по смещению из чужого адреса до какой-либо проверки. Для `exact-span-large` и biased Large «защитный» reject foreign/interior-указателя сам по себе формально UB |
| R6-MS-2 | Low | подтверждено чтением | На путях `GlobalAlloc` остались паникующие конструкции: `expect`/`unreachable!`/`assert!` в release, `debug_assert!` (включая `debug_assert!(false)`) в debug. Unwind из глобального аллокатора — UB |
| R6-MS-3 | Info | подтверждено чтением | Проверка ёмкости в foreign-ветке `realloc` слабее own-ветки: `end` берётся по зарезервированной, а не закоммиченной VA; у Small начало payload — база сегмента |
| R6-MS-4 | Info | подтверждено чтением (residual) | Stale/double free после переиздания адреса освобождает текущего владельца (Small и Large). Это новое воплощение U-R5-5 |
| R6-MS-5 | Info | подтверждено чтением | `unsafe impl Sync for HeapSlot` переносит `!Send` `HeapCore` между потоками без доказательства его семантической `Send`-ности |
| R6-MS-6 | Info | подтверждено чтением | Граница soundness в `lib.rs` неполна, tier-1 allow в `tls_heap.rs` шире нужного, часть SAFETY-комментариев устарела |

## Подробности

### R6-MS-1 — Low — pointer arithmetic до валидации в `canonical_block_of`

**Файлы/строки**

- `src/alloc_core/alloc_core/mem/mem_impl.rs:36-44`: `canonical_block_of` находит канонический корень по ключу `segment_base_of_ptr(ptr)` и сразу вызывает `Node::deref(base, ptr.addr().wrapping_sub(base.addr()))`.
- `src/alloc_core/alloc_core/mem/realloc_fastpath.rs:232-240`: та же схема в `realloc_inplace_fast_path_known_base`.
- `src/alloc_core/platform/node.rs:141-153`: `Node::deref` — это `segment_base.add(offset)`. Контракт в документации требует `offset < segment_len`, проверки в коде нет.
- Вызывающие: `AllocCore::dealloc` (`mem_impl.rs:239`), `AllocCore::realloc` (`:656`), `HeapCore::dealloc_routing` → `owned_block_of` (`src/registry/heap_core_xthread/routing.rs:11`), `dealloc_batch_small` (`src/registry/heap_core/free/dealloc_batch.rs:264`).
- Reject срабатывает позже: `reject_large_interior_free` (`mem_impl.rs:271`, `src/alloc_core/large/alloc_core_large.rs:127-135`) и `large_payload_mismatch` (`mem_impl.rs:666`).

**Механизм.** Ключ таблицы — окно размером `SEGMENT` (4 MiB). При `exact-span-large` резервация Large выровнена на `SEGMENT`, но может быть короче его (например, ~264 KiB). Для biased-корней (`align >= SEGMENT`) окно ключа начинается с payload и не совпадает с границами окна корня. Указатель из того же окна, но за пределами резервации, даёт `offset` больше размера объекта. `ptr::add` с таким смещением — UB по правилам Rust ещё до разыменования. В документации контракта (`mem_impl.rs:212-230`) foreign/interior-защита названа best-effort defence-in-depth. На практике reject срабатывает корректно, но сам путь к reject формально не определён.

**Сценарий.** Сборка с `exact-span-large`. Large-аллокация на 260 KiB занимает `[B, B+264K)`. ОС размещает чужое отображение (например, heap `System`) по адресу `B+1 MiB` — это то же 4-MiB окно. Код ошибочно отдаёт указатель из этого отображения в `SeferAlloc::dealloc` (нарушение контракта). `canonical_block_of` находит корень `B` и вычисляет `B.add(1 MiB)` за пределами объекта: UB, Miri это отметит. Только после этого `reject_large_interior_free` возвращает no-op. Затронута только сборка с opt-in фичей и только при нарушении контракта, поэтому Low.

**Рекомендация.** Для свидетеля-адреса строить указатель через `base.wrapping_add(off)` или `base.with_addr(ptr.addr())` и только после проверки `off < span` (по `span_usable`/`reservation_len` для Large, `SEGMENT` для Small) получать указатель через `add`. Альтернатива — явно сказать в `# Safety`, что foreign-адрес в окне чужой Large-резервации — это немедленный UB, а не «benign at runtime».

**Уверенность:** высокая по механизму, по ущербу — только при нарушенном контракте.

### R6-MS-2 — Low — паникующие конструкции на путях `GlobalAlloc`

**Файлы/строки (release-сборки)**

- `src/alloc_core/large/alloc_core_large_cache.rs:146-148`, `:162-164` — `.expect("large_cache_slot_take: empty ... slot")`; `:170`, `:364` — `unreachable!`. Вызываются из `alloc_large` (cache hit) и из eviction/`dealloc`.
- `src/alloc_core/small/alloc_core_small_magazine.rs:169-173` — `assert!(out.len() <= 16)` в `refill_class_bump_virgin_internal`, это production-refill под `virgin-zero-skip`.
- `src/registry/bootstrap/registry.rs:134-136`, `:149-151` — `self.chunks[chunk_idx]` паникует при `idx >= MAX_HEAPS`, в документации прямо стоит «# Panics».

**Файлы/строки (debug-сборки)**

- `src/alloc_core/segment/segment_table/segment_table_impl.rs:617-622`, `:657-661` — `debug_assert!(false, ...)` в `recycle`. Соседний комментарий обещает «Leak rather than panic on the free path» и называет причину: «caller corruption or a double-recycle».
- `src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:199-202`, `src/global/tls_heap.rs:522`, `src/registry/heap_core/alloc/hot.rs:37-41`, `src/registry/heap_core/free/dealloc_own_base.rs:349`, `:482-486`, `src/registry/heap_core/state/tcache_flush.rs:89-93`, `src/registry/heap_core/free/realloc.rs:214-218`.

**Механизм.** В документации `GlobalAlloc` сказано, что unwind из глобального аллокатора — неопределённое поведение. Проект сам признаёт этот класс: `src/registry/heap_registry/claim.rs:186-195` (R2-08) объясняет, почему `debug_assert!` на cold-bind пути был заменён счётчиком. Перечисленные места остались. При корректных инвариантах они недостижимы. Но при внутреннем рассогласовании (битовая маска занятости и массив слотов large-cache) или при ошибке вызывающего, которую ловит assert (anomaly-ветки `recycle`), аллокатор вместо задуманного abort или leak делает unwind через `GlobalAlloc::{alloc,dealloc}`.

**Сценарий.** Debug-сборка приложения с `#[global_allocator] static A: SeferAlloc`. Внутренняя ошибка или порча метаданных приводит `SegmentTable::recycle` в ветку «hash member with no slots[] entry». Срабатывает `debug_assert!(false)`, panic раскручивается из `GlobalAlloc::dealloc` — это UB, вместо обещанного комментарием leak. В release то же произойдёт через `expect` в `large_cache_slot_take`, если `large_cache_occupied` разойдётся с `large_cache[]`.

**Рекомендация.** Завести единый макрос «invariant violated → `std::process::abort()`» (как в `directory.rs`, `route_slots.rs`, `claim.rs`) и заменить им `expect`/`unreachable!`/`assert!` на путях, достижимых из `GlobalAlloc`. `debug_assert!` на этих путях либо перевести на abort в debug, либо вынести в тесты. Отдельно привести `debug_assert!(false)` в `recycle` в соответствие с обещанием «leak, not panic».

**Уверенность:** высокая по наличию конструкций, низкая по достижимости при корректной работе.

### R6-MS-3 — Info — foreign-ветка `realloc` проверяет ёмкость по резервации, а не по закоммиченному диапазону

**Файлы/строки**

- `src/registry/heap_core/free/realloc.rs:417-438` — foreign-ветка: `lookup` → `contains_payload(ptr, old_layout.size())` → `self.alloc` → `copy_nonoverlapping(ptr, new_ptr, min)`.
- `src/registry/segment_route/directory.rs:174-184` (`contains_payload`), `:813-835` (`end = root + len`).
- `src/alloc_core/large/alloc_core_large.rs:456-461`, `:692-697` — для Large `len = reserved_capacity`. При `large-reserved-capacity` это зарезервированная, но не закоммиченная VA.
- `src/alloc_core/segment/segment_table/segment_table_impl.rs:372` — у Small/Primordial `payload = base`, то есть метаданные сегмента попадают в диапазон.
- Для сравнения own-ветка: `realloc.rs:340` + `realloc_fastpath.rs:92-100` (`safe_payload_read_span` по `span_usable`/`committed_payload_end`).

**Механизм.** Для своих блоков завышенный `old_layout.size()` отсекается фронтиром коммита и даёт null. Для блоков чужого heap та же ошибка вызывающего проходит проверку дескриптора, и `copy_nonoverlapping` читает зарезервированные, но не закоммиченные страницы: access violation или SIGSEGV вместо null. Документация (`realloc.rs:94-108`) честно предупреждает, что это проверка ёмкости, а не идентичности блока, и что всё держится на контракте `GlobalAlloc`. Поэтому это не нарушение soundness, а асимметрия defence-in-depth.

**Сценарий.** Сборка `production` с `large-reserved-capacity`. Поток A аллоцирует Large-блок на 1 MiB (закоммичено около 1 MiB, зарезервировано до 4×). Поток B вызывает `realloc(p, Layout(3 MiB), 8 MiB)` с ложным `old_layout`. Проверка `contains_payload` проходит, копирование уходит в незакоммиченный хвост, процесс падает.

**Рекомендация.** Хранить в дескрипторе неизменяемую верхнюю границу закоммиченного начального span (или текущий `span_usable` в атомарном поле sidecar) и ограничивать чтение ею. Для Small начинать payload с `small_meta_end()`/`primordial_meta_end()`.

### R6-MS-4 — Info (residual) — stale/double free после переиздания освобождает текущего владельца

**Файлы/строки**

- Small: `src/alloc_core/small/alloc_core_small_reclaim.rs:38-67`. Запись из sidecar отклоняется только если блок уже free в bitmap или лежит в магазине. Поколения или идентичности экземпляра нет.
- Large: `src/registry/segment_route/large_state.rs:15-27` — каждый новый маршрут создаётся с `LargeState::new()` (gen = 1). `publish_pending` (`:25-28`) публикует для текущего поколения слова, а не для поколения, наблюдённого при выдаче блока.
- Маршрутизация: `src/registry/segment_route/directory.rs:902-931` (lookup по адресу), `src/registry/heap_core_xthread/routing.rs:47-63`.

**Механизм.** Новый дизайн надёжно закрывает гонки и UAF на метаданных при добросовестном вызывающем. При двойном free после переиздания адреса (Small — блок снова выдан из freelist, Large — повторное использование резервации из large-cache с новым маршрутом на том же ключе) stale-продюсер находит текущий маршрут и публикует освобождение текущего экземпляра. Владелец его принимает, и адрес выдаётся повторно, пока новый владелец ещё жив. Это та же «информационная неразличимость», что в R5-MS-2 п.4 и U-R5-5. Теперь она явно описана как UB вызывающего в `# Safety` (`mem_impl.rs:220-222`) и в комментарии `alloc_core_small_pool_impl.rs:50-52`.

**Рекомендация.** Для `hardened` рассмотреть тег поколения выдачи в sidecar: свободные биты в `ClassLeaves` либо отдельный байт поколения на гранулу, который продюсер копирует до публикации. Без такого тега оставлять пункт в индексе residuals.

### R6-MS-5 — Info — `HeapSlot: Sync` переносит `!Send` `HeapCore` между потоками

**Файлы/строки**

- `src/registry/heap_slot.rs:204` (`heap: UnsafeCell<MaybeUninit<HeapCore>>`), `:261-283` (`unsafe impl Sync for HeapSlot`), `:285-304` (HeapSlot намеренно `!Send`).
- `src/registry/heap_registry/claim.rs:495-501` (`MaintenanceLease::with_core` — `&mut HeapCore` в потоке воркера), `:612-617` (`HeapLease::core`), `:381-396` (ручная инвалидация NUMA-кэша при reclaim).

**Механизм.** Стандартное обоснование `Sync` для обёртки над `UnsafeCell<T>` с выдачей `&mut T` другим потокам требует `T: Send`. `HeapCore` (через `AllocCore`) автоматически `!Send` из-за raw-указателей. Комментарий SAFETY доказывает single-writer и happens-before (Release/Acquire CAS), но не доказывает, что значение `HeapCore` безопасно переносить между потоками: оно не держит потокоаффинных ресурсов. Сейчас это верно по чтению: raw-указатели на собственные резервации, `Option<&'static AtomicU64>`, `RouteSlots` (System-массив `RouteRegistration` с `PhantomData<Cell<()>>`, который `Send`). Но компилятор этого больше не проверяет: любое будущее потокоаффинное поле (`Rc`, кэш TID/CPU, ссылка на `thread_local`) тихо станет межпоточным. Риск уже реализовывался: кэш NUMA-узла пришлось инвалидировать вручную при каждом claim (R11-5).

**Рекомендация.** Ввести обёртку `struct SlotCore(HeapCore)` с `unsafe impl Send` и отдельным SAFETY-обоснованием «семантически `Send`: нет потокоаффинных полей». Добавить compile-time проверку полей (например, `assert_send` для всех полей, кроме перечисленных raw-указателей) или хотя бы чек-лист в doc-комментарии `HeapCore`.

### R6-MS-6 — Info — граница soundness и SAFETY-документация устарели или неполны

**Файлы/строки**

- `src/lib.rs:291-311` — список «Concrete membranes to audit» не включает safe `pub(crate)`-функции, которые выполняют free или запись метаданных по указателю вызывающего: `HeapCore::dealloc_routing` (`src/registry/heap_core_xthread/routing.rs:8-20`; safe fn, внутри `unsafe { publish_foreign }` с обоснованием «reached only from …»), `dealloc_own_thread` (`src/registry/heap_core/free/dealloc.rs:247`), `dealloc_own_thread_with_base` (`src/registry/heap_core/free/dealloc_own_base.rs:347`), `AllocCore::reclaim_sidecar_record` (`src/alloc_core/small/alloc_core_small_reclaim.rs:21`; пишет freelist по `base` без проверки членства), `flush_run`, `canonical_block_of`.
- `src/global/tls_heap.rs:95-101` — tier-1 `#![allow(unsafe_code)]` действует безусловно, хотя единственное `unsafe` в файле — хук `dbg_restore_local_for_test` под `bench-internals` (`:651-655`). В `production` модуль разрешает `unsafe`, но не использует его. Описание в `src/lib.rs:225` («raw-pointer TLS binding + AbandonGuard seam») устарело после Ph4b.
- `src/registry/heap_registry/claim.rs:5-9` утверждает, что `get_unchecked` удалён. На деле `src/registry/bootstrap/registry.rs:138`, `:153`, `:212` по-прежнему используют `get_unchecked` (корректно: `idx % CHUNK_SLOTS`).
- `src/alloc_core/alloc_core/mem/mem_impl.rs:414-446`, `src/alloc_core/large/alloc_core_large.rs:310-348`, `:405-422`, `:821-838` обосновывают атомарный store `magic` и порядок записи заголовка защитой от удалённых читателей `kind_at`/`large_size_at`/`span_usable_at`. По текущему коду foreign-путь заголовки не читает вовсе, и эти инварианты больше ничего не несут.

**Механизм.** Ни одно из мест не является UB само по себе. Но аудит по инвентарю из `lib.rs` пропустит функции, на которых держится soundness. Неиспользуемый tier-1 allow позволяет добавить `unsafe` в `tls_heap.rs` без нового allow, который заметили бы на ревью. Устаревшие SAFETY-аргументы искажают модель угроз для следующих правок.

**Рекомендация.** Дополнить список мембран в `lib.rs`. Либо сделать `dealloc_routing` и `reclaim_sidecar_record` `unsafe fn` с `# Safety` (tier-2), что соответствует правилу «каждый caller-pointer контракт — `unsafe fn`». В `tls_heap.rs` заменить tier-1 на `#![cfg_attr(feature = "bench-internals", allow(unsafe_code))]` или на item-level allow. Обновить комментарии про `get_unchecked` и удалённых читателей заголовка.

## Гипотезы (не подтверждены)

- **Г-1. TLS без нативной поддержки (`!target_thread_local`).** На таких целях std хранит `thread_local!` через OS-ключ и при первом обращении размещает значение через глобальный аллокатор. Первый `LOCAL.try_with` в `current_for_alloc` (`src/global/tls_heap.rs:260-285`) мог бы тогда рекурсивно войти в `SeferAlloc::alloc` и уйти в бесконечную рекурсию. Для проверки нужно сверить реализацию `std::sys::thread_local` на MSRV и список поддерживаемых целей в README «Target support». Основные цели (Linux, Windows, macOS) используют нативный TLS, поэтому риск оценён как низкий.
- **Г-2. fork многопоточного процесса.** Удержанные `ShardLock`, fallback-`LOCK`, `INITIALIZING`-состояния и незабранные публикации наследуются ребёнком. По чтению кода это приводит к зависанию (liveness), а не к UB. Ограничение задокументировано в `src/global/sefer_alloc/core.rs:97-120`. Механизм UB не найден.
- **Г-3. P1-box (`docs/CORRECTNESS_OPEN_ITEMS.md` item 164).** Нарушение протектора Stacked/Tree Borrows в `Node::write_next` (`src/alloc_core/platform/node.rs:85-112`) при освобождении через by-value `Box` по-прежнему присутствует. Это принятый известный дефект модели алиасинга. Новых данных этот отчёт не добавляет, статус пункта не меняется.

## Проверенные области без новой подтверждённой находки

- `GlobalAlloc` adapter: `null`-ветки, fallback при process-exit (System-аллокация с утечкой, копирование `min(old,new)` из живого блока), диспетчеризация dealloc без bind (`current_for_dealloc`).
- TLS: порядок `LOCAL` → `GUARD` в `finish_bind`, откат при отказе `try_with`, сентинел `TORN`, отсутствие повторного bind после teardown, `AbandonGuard::drop` (trim до `drop(lease)`).
- Fallback heap: состояние инициализации, `InitStateGuard`, `LockGuard`, `try_with_heap` для повторного входа, отказ от ожидания при process-exit. Окно «panic после `write(hc)`» уже учтено как item 23.
- Реестр: CAS `EMPTY→INITIALIZING→LIVE`, `FREE→LIVE`/`FREE→MAINTENANCE`, публикация `initialised`, `HeapLease`/`MaintenanceLease` (`!Send`, abort при проигранном CAS и при panicking), `bind_slot_counters`, ленивые чанки (`OncePtrCell`, OS-обнулённые страницы как валидное состояние).
- `RouteDirectory`: двухуровневые блоки под `ShardLock`, refcount записи (`fetch_update` с abort при переполнении, освобождение по последнему пину), удаление из индекса до release резервации (`recycle`/`unregister` → `routes.remove` → `os::release_segment`), отсутствие разыменования адреса lookup, освобождение System-памяти вне блокировки (кроме `Block::release` — System не входит повторно в аллокатор).
- Sidecar-протокол: `prepare`/`issue` до выдачи, публикация с AcqRel, отбрасывание невыданных гранул, `BitmapCut::pop` с abort на некорректный класс, `reclaim_sidecar_record` с безусловными проверками payload, кратности и `bump`, порядок «все cut и заимствования маршрута сброшены до pool/release».
- Large: claim и переходы сгенерированного состояния в заголовке и в sidecar маршрута разведены корректно (у кэшированной резервации новый маршрут); biased-резервация с проверенной арифметикой и RAII-освобождением при отказе commit; `checked_add` в `hdr_aligned + size`; при cache hit заголовок пишется до `register_payload`.
- Realloc: OPT-F (только `==` класса), OPT-G (точное совпадение payload, `checked_add`, рост в `reserved_capacity` с выравниванием по 64 KiB), ограничение чтения `safe_payload_read_span` в own-ветке, неизменность `base` до `dealloc_own_thread_with_base` (блок жив).
- Магазин и batch: `small_free_guard` (Large-kind маршрут и interior-reject для Large без `hardened`), переполнение магазина (сегмент текущего блока не может опустеть во время flush), `flush_class_internal` + `canonical_base_of` перед `flush_run` (повторный run по освобождённой базе отсекается проверкой членства, даже когда `RECYCLED_CAP` исчерпан).
- `AllocCore::drop`: маршруты закрываются первыми, large-cache и расширение освобождаются, первичный сегмент — последним, `RouteSlots` живут в System-памяти и переживают освобождение первичного сегмента.
- Sidecar-примитив (`sidecar::{reserve, reserve_zeroed_with, deref, deref_mut}`): время жизни ссылок привязано к владельцу, const-assert выравнивания, обнуление под Miri.
- `process_exit.rs`: FFI-сигнатуры `GetModuleHandleW`/`GetProcAddress`, `transmute` к `extern "system" fn() -> u8`.
- Диагностические хуки `dbg_*`, `*_for_test`: все читающие закрыты `internals`/`bench-internals` и разрешают адрес через канонический корень таблицы. Все пишущие по raw-указателю — `unsafe fn` с `# Safety`. Нарушений правила R25-1 не найдено.

## Статус находок round5

| Round5 ID | Статус в round6 | Доказательство |
|---|---|---|
| R5-MS-1 (safe `realloc`) | **закрыт** | `AllocCore::realloc` и `HeapCore::realloc` стали `unsafe fn` (`src/alloc_core/alloc_core/mem/mem_impl.rs:643-644`, `src/registry/heap_core/free/realloc.rs:136-137`). Добавлены отказ при `large_payload_mismatch` (`mem_impl.rs:666`, `realloc.rs:150`) и ограничение чтения в move-ветке. Остаток живёт только при нарушенном контракте (R6-MS-3) |
| R5-MS-2 (safe `dealloc`) | **закрыт** | `unsafe fn dealloc` (`mem_impl.rs:232-233`, `src/registry/heap_core/free/dealloc.rs:218-219`). Interior free для Large отклоняется безусловно (`mem_impl.rs:271`). Foreign-путь не читает заголовки (`src/registry/heap_core_xthread/routing.rs`). Stale-after-reuse сохраняется как документированный UB вызывающего — R6-MS-4 |
| R5-MS-3 (safe `flush_class`, class-bounds) | **закрыт** | `pub unsafe fn flush_class` под `internals` (`src/alloc_core/small/alloc_core_small_magazine.rs:453-459`). Проверка границ в release в `BinTable::head/set_head` (`src/alloc_core/segment/segment_header/descriptors.rs:185-200`) и в `dbg_freelist_head_for` (`src/alloc_core/small/alloc_core_small_diag.rs:82-84`). Весь `impl` диагностики закрыт `internals` |
| R5-MS-4 (safe ring hooks) | **закрыт структурно** | `RemoteFreeRing`, `dbg_push_to_ring`, `dbg_drain_all_rings` в `src/` отсутствуют, межпоточное освобождение идёт через пин и sidecar |
| U-R5-1 (atomic/plain `magic`) | **закрыт** | `magic_at` удалён, foreign-читателей заголовка не осталось. Атомарный store `magic` теперь однопоточный и избыточный (см. R6-MS-6) |
| U-R5-2 (ABA 48-bit `TaggedPtr`) | **неактуален** | `TaggedPtr` в `src/` отсутствует, выбор свободного слота основан на CAS состояния слота и подсказке-битмапе |
| U-R5-3 (wrap 8-bit hardened generation) | **неактуален** | Таблица поколений (`gen_at`/`bump_gen`) удалена вместе с кольцом |
| U-R5-4 (`AllocCore::drop` без quiescence) | **закрыт** | Продюсеры трогают только System-sidecar под refcount-пином. `Drop` сначала закрывает маршруты (`src/alloc_core/alloc_core/lifecycle.rs:442`) |
| U-R5-5 (deferred-large stale free) | **остаётся в новой форме** | Механизм deferred-large удалён, но stale free после повторного использования резервации по-прежнему неотличим — R6-MS-4 |
