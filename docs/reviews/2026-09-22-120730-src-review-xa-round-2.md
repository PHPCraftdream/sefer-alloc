# Независимое статическое ревью src — round 2 / xa

Дата: 2026-09-22. Метка отчёта: 120730.

Проверенный baseline: `c913ecd2ae45f6da241c0bc942231d7e15f164a4`.

Изолированный checkout: `worktrees/src-review-xa-round-2-20260922`, detached HEAD. Основной checkout не изменялся. Исходники, конфигурация и версии не исправлялись. Единственный добавленный отслеживаемый артефакт этого ревью — этот отчёт.

## Итог

**NO-GO для выпуска проверенного дерева как готового к production.** Найдены нарушения soundness публичных safe-поверхностей, в том числе доступной без `internals` поверхности `AllocCore`. Экспериментальные и диагностические features дополнительно открывают независимые пути к UB. Наличие `#[doc(hidden)]`, `experimental` или `internals` не отменяет обязательств safe Rust API.

| Приоритет | Количество |
|---|---:|
| P0 | 0 |
| P1 | 8 |
| P2 | 11 |
| P3 | 4 |
| P4 | 1 |
| Всего | 24 |

Это количество отдельных карточек ниже; родственные проявления одной проблемы внутри карточки не посчитаны повторно. P0 не заявлен: статическое чтение не установило немедленной универсальной катастрофы на каждом штатном вызове. P1 — блокирующие soundness/контрактные дефекты; P2 — существенные correctness, resource-lifecycle, portability и verification defects; P3 — локальные издержки и вводящая в заблуждение документация; P4 — сопровождаемость.

## Метод и границы достоверности

- С нуля просмотрены реализации и связи всех **128 Rust-файлов src, 42 967 строк**, включая wiring, диагностические поверхности, `cfg(miri)`, `cfg(loom)` и Kani harnesses. Для чтения реализаций использовались также представления без исторических комментариев; значимые публичные контракты, safety-обоснования и спорные комментарии перечитывались отдельно. Инвентарь файлов приведён в конце.
- Проверены манифест и lockfile, экспорты crate root, актуальные feature-зависимости, документация инвариантов и относящиеся к контрактам разделы README/архитектуры. Предыдущие отчёты ревью и индексы прежних замечаний **не использовались** как источник истины. Упоминание старой задачи или успешного теста в комментарии не принималось за доказательство.
- Использован `rust-intel` как контрольный список unsafe/FFI, provenance, ownership и error-path obligations; работа выполнена одним агентом, без делегирования. Это не многoагентная сертификация всех модулей skill и не формальное доказательство отсутствия остальных ошибок.
- Режим полностью статический: **не запускались cargo test/check/build/clippy/rustdoc, Miri, Kani, Loom, бенчмарки, примеры, стресс-прогоны и исполняемые воспроизведения дефектов**. Выводы основаны на коде и контрактах. Нативная воспроизводимость, результаты компилятора и величины ускорения не выдумывались.
- Вне src читались только необходимые для связей метаданные/контракты и конкретные seam-реализации `aligned-vmem`; полное ревью `crates/`, `tests/`, CI и зависимостей в этот результат не входит. Для неоднозначных языковых правил и reclamation-контракта прочитана официальная документация Rust и исходники Crossbeam соответствующего тега; это также статическое чтение, не выполнение кода.
- Версии: root `0.3.0`, edition `2021`, MSRV `1.88`; lockfile: `crossbeam-epoch 0.9.20`, `arc-swap 1.9.1`, `core_affinity 0.8.3`; workspace seams: `aligned-vmem/numa-shim/sefer-region 0.2.0`, `once-ptr-cell/size-classes/tagged-index-stack 0.1.0`. Установленный toolchain не запускался и не аттестован.
- Все номера строк относятся к baseline выше. Отчёт не содержит runnable exploit/PoC; рекомендации по проверкам — последующий acceptance plan, не заявление об уже выполненных проверках.

## Карта контрактов и feature-конфигураций

| Слой | Главные проверенные обязанности | Результат |
|---|---|---|
| `lib.rs`, default / no-default | Реэкспорты Region, no_std, границы видимости | `Region` находится в другом crate; его полная soundness здесь не аттестована |
| `AllocCore` | Время жизни таблицы, alloc/free/realloc, layout и reservation ownership | Блокер R2-01; standalone lifetime также важен для R2-12 |
| `HeapCore`, TLS, registry | Один владелец, release/acquire handoff, teardown, fallback | Основной протокол разобран; R2-08, R2-09, R2-11 требуют исправления |
| Small / fastbin / batch | live_count, bitmap/magazine, смена сегмента, staged free | Продублированную guard-chain не принимал за доказательство; R2-15 на публичном refill |
| Large / cache / reserved-capacity | Размер payload, сохранение старого allocation на failure, commit frontier | R2-12, R2-14, R2-18; дополнительные рекомендации по arithmetic ниже |
| Remote ring / overflow / deferred Large | Publication, single consumer, reservation lifetime, retry/drop | R2-06, R2-07, R2-09, R2-10, R2-13 |
| `experimental`, `pinning` | Идентичность handles, EBR lifetime/Send, runner/shard contract | R2-02–04 и R2-16 |
| OS / NUMA / lazy commit | Выравнивание, реальные страницы, ownership передачи в vmem | R2-17, R2-19; нативные ABI/backend-проверки ещё нужны |
| `internals`, `bench-internals`, diagnostics | Safe API остаётся sound; измерения отражают реальность | R2-05–07, R2-11, R2-14, R2-21 |
| Проверочная модель в src | Что именно доказано, отличие harness от production | Kani arithmetic не доказывает конкурентный протокол R2-10; `loom_shim` не равен исполнению реального global allocator под Loom |

`production` реально включает `alloc-global`, `alloc-xthread`, `alloc-decommit`, `fastbin`, `alloc-segment-directory`, `primordial-lazy-commit`, `class-aware-dirty` (`Cargo.toml:424`). `batch-api` дополнительно включает `experimental`; следовательно, небезопасная экспериментальная поверхность появляется и у пользователя batch API. `hardened` тянет `fastbin` и `alloc-xthread`, но сам по себе не является синонимом полного `production`. `numa-aware` выключает некоторые lazy/exact-capacity ветки через `not(feature = "numa-aware")`; поэтому `--all-features` не покрывает поведение обычного production или exact-span без NUMA.

## P1 — блокеры

### R2-01 — P1: safe-итератор сегментов не удерживает владельца живым

**Место:** `src/alloc_core/alloc_core/state.rs:173`, `AllocCore::segment_bases`; источник — `src/alloc_core/segment/segment_table/mod.rs:678`, `SegmentTable::bases`; транзитивный экспорт — `src/registry/heap_core/core.rs:716`.

**Механизм.** Возвращается `impl Iterator<Item = *mut u8>` без lifetime bound. В edition 2021 эта сигнатура inherent-метода не захватывает lifetime `&self`. Hidden type действительно хранит только скопированный raw `slots` и число элементов. Чтение памяти отложено до `next()`, который вызывает `Node::read_struct` по этому raw pointer. Правило захвата проверено по [Rust Edition Guide](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html).

**Последствие.** Safe-клиент может сохранить iterator независимо от `AllocCore`; после уничтожения core его primordial reservation уже освобождён, но safe `next()` продолжает читать таблицу. Это UAF внутри библиотеки, а не обязанность пользователя правильно разыменовать возвращённый raw pointer. `#[doc(hidden)]` не закрывает вызов; метод доступен через корневой `AllocCore` при `alloc-xthread`/`alloc-global`, без `internals`.

**Исправление.** Привязать lifetime на всех трёх уровнях: `impl Iterator<...> + '_` либо точный `use<'_>`. Предпочтительно также сделать внутренний registry walk `pub(crate)`: оба модуля находятся в одном crate, вопреки обоснованию публичности в `state.rs:167`. Не возвращать detached iterator, который сам читает borrowed storage.

**Приёмка.** Compile-fail контракт на использование итератора после drop/mutable reborrow владельца; отдельная проверка чтения живого iterator под Miri. Проверки здесь не запускались.

### R2-02 — P1: EBR удаляет произвольный T без Send и lifetime-ограничения

**Место:** `src/concurrent/epoch/hand.rs:318`, `AtomicSlot::try_evict_at`, особенно `hand.rs:392`; вход — `src/concurrent/epoch/epoch_region.rs:153`, `impl<T> EpochRegion<T>`, методы `remove:360` и `remote_evict:413`.

**Механизм.** `guard.defer_destroy(old)` передаёт destructor глобальному epoch collector. Safe `EpochRegion<T>` позволяет вставлять и удалять любой `T`, включая `!Send` и значения с короткими заимствованиями. Ограниченные manual `Send/Sync` impls самого slot не исправляют это: удаление доступно и на единственном потоке. Collector может выполнить destructor позднее и на другом потоке; это явно следует из [Crossbeam 0.9.20, Guard](https://raw.githubusercontent.com/crossbeam-rs/crossbeam/crossbeam-epoch-0.9.20/crossbeam-epoch/src/guard.rs).

**Последствие.** Деструктор thread-affine/`Rc`-содержащего значения может выполниться на чужом потоке; destructor значения с borrowed data — после конца жизни referent. `Drop for EpochRegion` очищает лишь ещё установленные значения и не ждёт уже deferred work. Возможны data race/UAF из safe API. `ShardedRegion` наследует дефект.

**Исправление.** Для текущей архитектуры глобального deferred reclamation потребовать `T: Send + 'static` на реально владеющем EBR-контейнере/его безопасных destructive операциях и протащить ограничение в sharded/pinning API. `Sync` дополнительно требуется там, где допускаются одновременные shared reads. Альтернатива — другая, локальная/scoped reclamation с доказанным drain до завершения borrow, а не только изменение комментария.

**Приёмка.** Compile-fail для non-Send и borrowed-drop payload; positive cases для корректного owned payload; проверка collector lifecycle с Miri/контролируемой межпоточной синхронизацией.

### R2-03 — P1: concurrent handles не идентифицируют контейнер, а eviction доверяет именно этому

**Место:** `src/concurrent/epoch/epoch_handle.rs:25`, `EpochHandle`; `epoch_region.rs:203,360`; `hand.rs:338,357,392`. Родственные поверхности: `src/concurrent/sharded/sharded_handle.rs:35`, `sharded_region.rs:393,428`; `src/concurrent/lock_free/lock_free_handle.rs:24`, `lock_free_region.rs:269,337`.

**Механизм.** Handle содержит index/generation, но не region identity. Разные новые regions имеют пересекающееся пространство ключей. Safe lookup/remove принимает handle другого контейнера, если integer fields совпали. Для epoch это ломает не только логическую изоляцию: generation CAS может успешно выполниться на vacant slot. Затем без null-check вызывается `defer_destroy(old)`, уменьшается `len`, slot повторно попадает в free list.

**Последствие.** В epoch/sharded возможны null reclamation, underflow учёта и нарушение уникальности free-list. В LockFreeRegion как минимум читается/удаляется чужое значение. Комментарий `hand.rs:381` о том, что `defer_destroy(null)` — no-op, неверен: Crossbeam преобразует pointer в `Owned`; nullable преобразование — другой API. См. [Crossbeam 0.9.20, Shared::into_owned](https://raw.githubusercontent.com/crossbeam-rs/crossbeam/crossbeam-epoch-0.9.20/crossbeam-epoch/src/atomic.rs).

**Исправление.** Добавить непереиспользуемый instance id в каждый concurrent handle, проверять его до slot lookup/CAS, включить в equality/hash. Отдельно сделать `AtomicSlot` state machine различающей vacant/occupied; не считать успешную смену generation доказательством наличия `T`, не defer-destroy null, не уменьшать len/не выдавать free index без фактического удаления. Один null-check без identity/state исправит лишь часть проблемы.

**Приёмка.** Матрица cross-instance операций для всех трёх семейств, включая vacant target, occupied target, saturated target; состояние и len не меняются при отказе.

### R2-04 — P1: публичный safe generation setter разрушает eviction-протокол

**Место:** `src/concurrent/epoch/epoch_region.rs:476`, `_set_slot_generation_for_tests`; `src/concurrent/epoch/hand.rs:156`, `set_generation_for_tests`.

**Механизм.** Setter доступен при обычном `experimental`, не требует `internals` и принимает `&self`. Проза требует vacant slot, но код этого не проверяет и не обеспечивает quiescence. Произвольный store generation может вернуть старому handle право на CAS, в том числе пока другая eviction уже выиграла CAS и ещё не завершила pointer swap.

**Последствие.** Даже после исправления межконтейнерной identity из R2-03 остаётся safe способ обойти одноразовость eviction внутри одного region: повторная обработка slot, null deferred destruction, неверный len/free-list. Это отдельный вход к нарушению soundness, не «тест вызывает unsafe неправильно».

**Исправление.** Удалить setter из пользовательского API; оставить private harness seam с `unsafe`-контрактом либо создать validated конструктор тестового состояния, который требует эксклюзивный доступ и доказанную vacancy. Feature gate сам по себе не заменяет safety boundary.

**Приёмка.** Проверка отсутствия setter на обычном experimental API и проверка quiescence/vacancy на выбранном тестовом интерфейсе.

### R2-05 — P1: safe metadata queries проверяют адрес, но разыменовывают provenance вызывающего

**Место:** `src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs:27` (`dbg_kind_byte_of`, чтение на строке 34), аналогичные методы на строках `116,135,152,170`; `table_diag.rs:28,52,186`; `src/alloc_core/small/alloc_core_small_diag.rs:52,76`; делегаты `src/registry/heap_core/diag/queries.rs:30,318`.

**Механизм.** `base` вычисляется из caller pointer через `map_addr`; membership проверяется сравнением адресов в таблице. Затем для raw reads используется всё тот же caller-derived `base`, а не canonical pointer, сохранённый владельцем. Равенство адреса с live allocation не добавляет provenance. Safe Rust может создать pointer с адресом существующего allocation, но без provenance; [контракт without_provenance_mut](https://doc.rust-lang.org/std/ptr/fn.without_provenance_mut.html) запрещает ненулевой memory access через него.

**Последствие.** Safe диагностическая функция может выполнить UB несмотря на успешный ownership guard. Native crash не обязателен: проблема в допустимости memory access, а не только в том, отображена ли страница. Область — `alloc-core + internals` и её superset-конфигурации.

**Исправление.** Превратить table lookup в возврат **сохранённого owner pointer**, а caller address использовать только как ключ/offset. Чтение делать от canonical pointer; для small-only metadata дополнительно проверять `SegmentKind`. Если интерфейс намеренно доверяет pointer provenance — объявить его `unsafe` с полным контрактом, не называть membership универсальной валидацией.

**Приёмка.** Miri-проверки диагностических методов на обычном, interior, stale-address и provenance-less input; значения вне области отклоняются без memory access.

### R2-06 — P1: полная копия SegmentHeader гоняется с атомарным deferred_next

**Место:** `src/alloc_core/segment/segment_header/mod.rs:925`, `SegmentHeader::read_at`; reachable calls — `src/alloc_core/small/alloc_core_small_reclaim.rs:453`, `dbg_drain_all_rings_impl`, и `src/alloc_core/small/alloc_core_small_pool/mod.rs:907`, `dbg_segment_state_reconciliation`.

**Механизм.** `Node::read_struct::<SegmentHeader>` делает обычное aggregate-чтение, включая plain `u64 deferred_next`. Другой поток законно пишет эти байты атомарным CAS/store в `src/alloc_core/large/deferred_large/push.rs:110,132`. Диагностический ring walk читает **весь** header до проверки kind, поэтому затрагивает также Large-сегменты. Exclusive access к owner `HeapCore` не запрещает foreign producer писать в segment metadata. Смешанный atomic write/non-atomic read без happens-before — [data race по Rust memory model](https://doc.rust-lang.org/std/sync/atomic/index.html#memory-model-for-atomic-accesses).

**Последствие.** Safe `SeferAlloc::dbg_drain_current_thread_rings` (`src/global/sefer_alloc/diag.rs:312`) может открыть гонку с корректным cross-thread Large free; reconciliation имеет ту же проблему. Не требуется испортить пользовательский pointer или дважды его освободить. Оптимизация aggregate read компилятором не является safety proof.

**Исправление.** На concurrent-capable путях читать только нужные immutable/owner-only поля field-specific accessor-ами; deferred/owner atomics — только atomic loads. Ограничить full-header snapshot доказанно quiescent фазами или заменить его snapshot-функцией с корректной классификацией каждого поля.

**Приёмка.** Контролируемая гонка diagnostic walk/reconciliation с remote Large enqueue; Miri/TSan. Проверить все оставшиеся вызовы `read_at/header`, а не только два перечисленных.

### R2-07 — P1: safe overflow test hooks обходят materialisation и quiescence

**Место:** `src/registry/heap_overflow.rs:806`, `dbg_reserve_unpublished_for_test`; `heap_overflow.rs:473`, `dbg_rollback_sidecar_sentinel_for_test`; `src/registry/bootstrap/overflow_sidecar.rs:232`, особенно unconditional store на строке 265.

**Механизм.** Первый hook меняет tail без обеспечения backing для sidecar range; допустимость inline cursor проверяется только `debug_assert!`. Consumer `slot` при sidecar index доверяет опубликованному tail и разыменовывает sidecar (`heap_overflow.rs:498–510`). Второй hook принимает `&self` на Sync-типе, временно освобождает sentinel и в конце безусловно пишет null; конкурентный настоящий initializer может уже опубликовать готовый sidecar в это окно.

**Последствие.** Safe diagnostic API способен оставить reachable cursor с null/unpublished sidecar и довести последующее чтение до invalid dereference. Текст «только standalone test, quiescent» не обеспечен типами. Конфигурация: `alloc-global + alloc-xthread + internals`; `bench-internals` не требуется.

**Исправление.** Требовать `&mut self` для state-corrupting test hooks, проверять cursor/capacity во всех профилях; создавать sidecar либо отклонять переход. Rollback должен восстанавливать только действительно принадлежащее probe состояние через CAS, не уничтожать чужую publication. Непроверяемую quiescence вынести в `unsafe` seam/закрытый harness.

**Приёмка.** Release-проверки границы inline/sidecar и concurrency-модель probe против настоящего initializer; отсутствие invalid dereference при safe вызовах.

### R2-08 — P1: GlobalAlloc допускает unwind, а документация ошибочно полагается на shims

**Место:** `src/registry/heap_registry/claim.rs:226`, config-conflict `debug_assert!`; вход — `src/global/sefer_alloc/global_alloc.rs:31`; неверное обобщение — `src/global/sefer_alloc/mod.rs:132`.

**Механизм.** Конфликт конфигураций recycled slot вызывает panic в debug. Публичный `GlobalAlloc::alloc` прямо делегирует cold binding; локальной unwind barrier или явного abort нет. При прямом trait call `__rust_alloc` shim вообще не участвует. Поэтому утверждение, что **любой** panic из **любого** метода GlobalAlloc обязательно перехватывается nounwind shim и «не UB», неприменимо к публичному интерфейсу. [GlobalAlloc::Safety](https://doc.rust-lang.org/std/alloc/trait.GlobalAlloc.html#safety) по-прежнему запрещает unwind.

**Последствие.** Для воспроизведения достижимости достаточно разрешённой конфигурационной коллизии между экземплярами/перепривязками, не некорректного Layout. Документация `with_config` сама описывает эту коллизию и debug panic. «Effectively unsupported» независимость конфигураций не даёт unsafe trait impl права unwind. Release-only предположение также не защищает debug consumers.

**Исправление.** На allocator path сигнализировать ожидаемый config conflict без panic — например, счётчиком и заранее выбранной first-wins/error policy. Если нужен fail-stop, применять доказанно non-unwinding abort, а не надеяться на внешний shim. Отдельно проверить все достижимые panic/hook/reentrancy пути; исправить blanket-обещание в module docs.

**Приёмка.** Обе поверхности — прямые trait calls и `#[global_allocator]` — в debug/release и с обеими panic strategies; cold binding/config conflict проверяется отдельно от steady-state.

## P2 — существенные correctness, readiness и resource defects

### R2-09 — P2: законные cross-thread frees теряются; ёмкость очередей не ограничивает накопленные потери

**Место:** `src/registry/heap_core_xthread/overflow.rs:655`, `push_with_overflow_retry`, terminal branch `:923`; ограничения ёмкости — `src/alloc_core/segment/remote_free_ring/mod.rs:423` и `src/registry/heap_overflow.rs:279`.

**Механизм.** После заполнения per-segment ring и heap overflow retries конечны. Последняя ветка только увеличивает `DBG_RING_PUSH_RETRY_EXHAUSTED` и возвращает управление, не сохраняя free note ни в одной структуре. Штатный сценарий owner, ожидающего завершения освобождающих worker-ов, не обеспечивает owner-side drain. Для одного сегмента доступны 256 ring slots плюс общая heap ёмкость 2048; discarded frees не будут восстановлены, когда owner снова начнёт работу.

**Последствие.** Блок остаётся логически live и неиспользуемым, удерживает сегмент. Следующие bursts могут добавлять потери: ёмкость очередей ограничивает pending notes, **не накопленные потери размером этих очередей**. Рост в конечном счёте упирается в VM/address-space и общие лимиты segment tables, то есть в отказ новых allocations; буквально бесконечная память не утверждается. Это не автоматически UB, но существенный production-lifecycle дефект и расхождение с eventual reclaim/M6–M7. Exit/recycle сохраняет heap, а не восстанавливает потерянные записи.

**Исправление.** Нужен lossless overflow protocol, не зависящий от активности owner: например, growable M5-clean OS-backed spill либо доказанный producer-side helping с отдельным ownership/lifetime протоколом. Увеличение числа spins/slots лишь сдвигает границу. До исправления явно ограничить поддерживаемый workload и экспортировать точный drop counter, не обещать общий bound.

**Приёмка.** Детерминированный paused/exited-owner сценарий с последующим возобновлением; ledger подтверждает, что каждое корректное free в итоге учтено и outstanding live count возвращается к baseline. Не стресс-прогон вместо проверки механизма.

### R2-10 — P2: u32 tail допускает ABA между capacity check и CAS после полного оборота

**Место:** `src/alloc_core/segment/remote_free_ring/ops.rs:315–342`, `push`, и `:375–392`, `try_push_uncounted`; `src/kani_proofs.rs:266`, ограниченность текущего arithmetic oracle.

**Механизм.** Свободное место проверяется по snapshot `t`; затем отдельным CAS меняется только u32 tail. Нет epoch/tag, исключающего повтор того же `t`, если producer задержался до CAS, а другие producers и consumer успели совершить полный оборот счётчика. Устаревший capacity proof тогда снова проходит сравнение tail, хотя относится к другой инкарнации очереди. `RING_CAP` как степень двойки сохраняет арифметику modulo, но не идентичность reservation.

**Последствие.** В длинноживущем allocator возможны сверхёмкостная reservation, перезапись непрочитанной записи и потеря прогресса/reclaim. Это редкий wrap/interleaving случай, а не утверждение о сбое при каждом обычном переходе через u32::MAX. Имеющиеся Kani функции предполагают occupancy ≤ cap и доказывают вычитание, а не сохранение этого предположения конкурентными операциями.

**Исправление.** Добавить реально ABA-защищённую reservation identity/sequence protocol и политику exhaustion; более широкий cursor полезен практически, но сам по себе не доказывает отсутствие wrap. Обновить и shared cached-head reasoning. Модель должна включать паузу **до** CAS и повтор incarnation, не только переход соседних значений через MAX.

**Приёмка.** Reduced-width model реального протокола с полным циклом счётчика и stalled producer; никакие миллиарды реальных операций для этого не нужны.

### R2-11 — P2: stats может материализовать registry chunk и abort-нуть процесс

**Место:** `src/registry/heap_registry/counters.rs:283`, `walk_initialised_slots`, особенно `:294`; publisher — `src/registry/heap_registry/stack.rs:314`; allocator side — `src/registry/bootstrap/registry.rs:223–255`.

**Механизм.** Сначала `bump_count` публикует увеличенный count, позже `claim` вызывает `reg.slot` и материализует chunk. `stats()` с `alloc-stats` может увидеть count в этом промежутке и сам вызвать `slot()`. Комментарий `counters.rs:287` о том, что оба пути уже вызвали slot до наблюдения count, опровергается порядком операций.

**Последствие.** Метрика неожиданно делает OS reservation/инициализацию, может spin-wait на initializer, а на chunk OOM вызывает `process::abort`. Это не promised read-only metadata walk. Без `alloc-stats` данный walk действительно скомпилирован из пути — базовый O(1) случай не обвиняется.

**Исправление.** Добавить non-materialising lookup/iterator по уже READY chunks и пропускать null/INITIALIZING chunks в relaxed snapshot. Не заменять `slot` на `slot_or_none`: последний тоже может запускать materialisation.

**Приёмка.** Синхронизировать mint между count increment и chunk publication; вызов stats не должен увеличивать VM call counters, ожидать publication или попадать в fatal OOM branch.

### R2-12 — P2: owner-only sidecars текут при churn standalone AllocCore

**Место:** `src/alloc_core/platform/sidecar.rs:144,240`; `src/alloc_core/alloc_core/lifecycle.rs:394–399`; `src/alloc_core/small/alloc_core_small/directory.rs:69`; `src/alloc_core/large/alloc_core_large_cache.rs:295`.

**Механизм.** Owner-only directory/large-cache-extension резервируются через `leak_zeroed_pages`; `Drop` освобождает segments/cache entries, но не reservations самих sidecars. Обоснование «one-time-per-heap, not a growing leak» верно только при ограниченном числе process-lifetime registry heaps, а не для публичного `AllocCore::new`/drop.

**Последствие.** Последовательное создание/уничтожение standalone cores, материализующих sidecar, накапливает невозвратные VM spans. Суммарная утечка не ограничена `MAX_HEAPS`, поскольку standalone cores не проходят registry. NUMA directory умножает стоимость своим `NODE_BITMAPS`. Это не та же утечка, что R2-09: здесь даже все пользовательские frees корректно учтены.

**Исправление.** Owner-only sidecar должен хранить свой Reservation/parts и освобождаться вместе с core; returned borrows связывать с владельцем, не с `'static`. Process-lifetime leaking оставить только действительно process-global shared metadata, где это явно обосновано. Если standalone core намеренно leaking — это необходимо честно включить в публичный lifecycle contract, но это слабее исправления.

**Приёмка.** Счётчики резервирования/release sidecars на repeated core lifecycle, отдельно directory/extended-cache/NUMA; после drop остаются только явно допустимые process-global spans.

### R2-13 — P2: HeapOverflow::drain теряет уже совершённый прогресс при panic callback

**Место:** `src/registry/heap_overflow.rs:752`, `drain`, callback `:769`, очистка `:773`, публикация head только `:776`.

**Механизм.** Успешно обработанные элементы очищаются по одному, но shared head изменяется лишь после всего цикла. Если более поздний `reclaim` паникует, уже очищенные элементы остаются перед старым head. Последующий drain видит EMPTY в старом head и прекращается; producer считает ту же область занятой. В sibling `RemoteFreeRing` уже есть `DrainHeadPublish`, а здесь аналог отсутствует.

**Последствие.** После catch_unwind очередь может навсегда заклинить и потерять оставшиеся reclaim notes. Public safe callback не имеет enforceable no-panic contract. Дополнительно `&self` позволяет reentrant/multiple consumers, тогда как реализация требует одного consumer; этот предел нужно закрепить в API, а не предполагать из нынешних внутренних call sites.

**Исправление.** RAII-публикация последнего полностью обработанного cursor, как у sibling; отдельно определить retry/at-most-once семантику элемента, на котором callback упал. Ввести consumer token/эксклюзивный drain guard либо закрыть generic drain внутри single-owner интерфейса. RAII alone не даёт exactly-once текущему паниковавшему элементу.

**Приёмка.** Panic после нескольких успешных callbacks, затем продолжение drain и новые push; отдельный reentrancy/consumer-ownership тест.

### R2-14 — P2: reconciliation oracle не учитывает cache и неверно считает committed/total

**Место:** `src/alloc_core/small/alloc_core_small_pool/mod.rs:801–810` (заявленная identity), `:833`, `dbg_segment_state_reconciliation`, ветки `:847–921`.

**Механизм.** Walk читает только registered table slots. Large cache deposits предварительно unregister-ят сегмент (`src/alloc_core/alloc_core/mem/mod.rs:343`; `src/alloc_core/large/alloc_core_large.rs:685`), поэтому поиск cached Large через `hdr.magic == 0` внутри этого walk не видит реальные cached entries. NULL slots пропускаются, хотя `table.count()` — high-water mark; заявленная identity с count не выполняется после recycle. Для primordial/active Small всегда прибавляется целый SEGMENT committed bytes, игнорируя Windows lazy frontier.

**Последствие.** Диагностические totals и сравнения режимов могут подтверждать неверную картину: `large_cached` пуст при непустом cache, count не сходится, lazy commit экономия исчезает из oracle. Это ошибка самого измерителя, не доказанный runtime leak по его показаниям. Отдельная concurrency-опасность aggregate read учтена в R2-06.

**Исправление.** Раздельно перечислять live table entries и occupied cache slots; различать high-water/live/cached counts; committed считать по применимому frontier/backend contract, reserved — по реальным reservation descriptors. Не выдавать accessible/committed bytes за measured RSS.

**Приёмка.** Малые известные состояния: recycled hole, cached Large, extension slot, lazy primordial, partial Small; totals сверяются с независимым ledger, а не с самим этим getter.

### R2-15 — P2: публичный virgin refill оставляет stale bits и не ограничивает размер маски

**Место:** `src/alloc_core/small/alloc_core_small_magazine.rs:166`, `refill_class_bump_virgin_checked`; обновления `:308–309` и `:329–330`.

**Механизм.** Документированная выходная маска только дополняется через `|=`, перед вызовом не обнуляется. Биты recycled/non-virgin slots не очищаются. `out` — произвольный slice, но маска u16 и расчёт промежуточного run — u32; API не ограничивает число выходов 16.

**Последствие.** При повторном использовании output mask грязный reused block может быть помечен virgin. Длинный output теряет биты и достигает некорректных shift counts, с различиями debug/release. Внутренний magazine consumer сейчас передаёт нулевую маску и ≤16 slots, поэтому это **не** утверждение, что текущий `HeapCore::alloc_zeroed` всегда неправильно работает. Дефект — публичный safe contract `AllocCore`, доступный без `internals` при нужных features.

**Исправление.** Обнулять маску до любой early-return ветки; enforce `out.len() <= 16` во всех профилях или выдавать маску переменной длины. Ещё лучше — сделать этот внутренний plumbing `pub(crate)` и явно типизировать вместимость magazine.

**Приёмка.** Ненулевая входная mask, recycled/fresh mixed refill, partial/OOM refill, границы 0/16/17/32 output slots; проверять обещанные биты, не только число pointers.

### R2-16 — P2: PinnedRunner не проверяет совместимость region при run

**Место:** `src/concurrent/pinning.rs:120,132` (constructor), `:197`, `run`; проигнорированный bind result — `:224`.

**Механизм.** Число workers ограничено shard count только того region, который передали constructor. Сам runner хранит лишь cores, а `run` принимает произвольный другой `ShardedRegion<T>`. Если его shards меньше, часть `bind_current_thread_to_shard` возвращает false; результат игнорируется, callback всё равно получает out-of-range `shard_id`.

**Последствие.** Обещание «worker i привязан к shard i» нарушается для допустимой последовательности API calls. Callback получает несуществующий shard, а actual inserts могут уйти в другой shard или неожиданно получить local-capacity exhaustion. Это не отказ affinity OS: documented best-effort affinity не оправдывает invalid shard routing.

**Исправление.** На run валидировать `worker_count <= region.shard_count()` и возвращать явную ошибку, либо обрезать workers с точным документированным контрактом. Bind failure не игнорировать. Если runner привязан к конкретному region, выразить это ownership/identity в типе.

**Приёмка.** Runner, созданный для большего region, запускается с меньшим; каждый реально вызванный callback имеет действительный, действительно связанный shard id.

### R2-17 — P2: 64-битные layout pins неявно запрещают 32-битный allocator

**Место:** `src/alloc_core/segment/segment_header/layout_asserts.rs:83`; `src/registry/heap_core/state/tcache.rs:257–259`.

**Механизм.** Без target gate утверждается `size_of::<SegmentHeader>() == 144` и offset pointer array в `PerClass == 8`. В repr(C) на обычном 32-битном ABI указатели/usize имеют другой размер, а `slots` начинается с offset 4. Это не универсальные layout relationships и не feature-neutral invariants.

**Последствие.** `alloc-core` и `production` не собираются на таких targets уже по const assertions. Это статически выводимый compilation defect/необъявленное ограничение, **не результат запущенного cargo check**. Region-only no_std поверхность отделена и этим утверждением не дисквалифицируется.

**Исправление.** Либо официально ограничить allocator 64-bit targets и выдавать понятный `compile_error!`/документированный support matrix, либо сделать pins target-aware и проверить alignment всех AtomicU64 views на 32-bit. Простое удаление assertions недостаточно: arithmetic и atomic support также требуют проверки.

**Приёмка.** Явные target rows для x86_64/aarch64 и выбранного 32-bit target либо проверка осмысленного unsupported-target отказа. Решение о поддержке должно предшествовать release.

### R2-18 — P2: decay_interval_ms(0) не означает обещанный tick на каждом событии

**Место:** `src/alloc_core/config/large_cache_config.rs:299`; реализация — `src/alloc_core/large/alloc_core_large_cache.rs:517–530`, `maybe_decay_large_cache`, zero-interval branch `:568`.

**Механизм.** Страйд 64 проверяется раньше значения interval. После инициализации `last_decay_tick` большинство large alloc/free возвращаются до zero-interval branch даже при cache выше headroom. Нулевой interval отменяет ожидание времени только на тех вызовах, которые уже прошли stride.

**Последствие.** Публичная конфигурация для immediate decay имеет другое поведение; sparse operations могут удерживать память дольше обещанного. Это проверяется по порядку условий, wall-clock измерение для подтверждения несоответствия не нужно.

**Исправление.** Для zero interval обходить stride (и определить первую tick semantics), либо изменить публичный контракт на throttled eligibility, явно указав частоту 64. Аналогично уточнить фразу в `mode()` о следующем событии после истечения интервала.

**Приёмка.** Счётчик фактических decay steps при interval 0 и превышенном headroom, включая первую операцию и редкие одиночные операции; без sleep-based oracle.

### R2-19 — P2: production наследует test-only глобальный mutex в commit seam

**Место:** `src/alloc_core/platform/os.rs:700`, `commit_pages`, forward `:708`; feature-связь — `Cargo.toml:424,810–814`; подтверждающий callee — `crates/aligned-vmem/src/api/commit_range.rs`, `try_commit_range`, и `crates/aligned-vmem/src/fault_injection.rs:173`, `should_fail_commit`.

**Механизм.** `production -> primordial-lazy-commit -> aligned-vmem/fault-injection`. В реальной non-mock ветке `try_commit_range` вызывает test hook. Даже при ничего не armed hook берёт process-global `FAULT_STATE: Mutex<_>` прежде, чем проверяет `target == 0`. Закрытие sefer-уровневых `dbg_arm_*` через `internals` не выключает эту dependency feature.

**Последствие.** Независимые heap commit slow paths сериализуются на тестовом mutex; production включает активную fault-injection инфраструктуру, которую callee обосновывает как test-only. Это конкретная transitive synchronization cost, не измеренный процент замедления и не самостоятельное доказательство deadlock на каждом target.

**Исправление.** Убрать fault-injection из обычных lazy-commit feature edges; включать отдельно только в dedicated test feature/fixture package и соответствующих проверках. Проверять resolved release graph отдельно от dev/all-features graph. Изменение требует последующего запроса на реализацию; здесь ничего не менялось.

**Приёмка.** Обычный production graph не содержит fault-injection; тестовые конфигурации продолжают достигать intended fault hooks; instrumentation подтверждает отсутствие FAULT_STATE lock на release commit path.

## P3 — издержки и документация

### R2-20 — P3: неуспешный LockFreeRegion::remove платит за весь COW snapshot

**Место:** `src/concurrent/lock_free/lock_free_region.rs:337–357`; `Snapshot::clone:94`.

**Механизм.** До проверки generation/occupancy клонируется полный `Vec<Arc<Page>>`, затем целевая page. Даже заведомо stale/removed handle создаёт allocations и refcount traffic; out-of-range handle уже оплатил snapshot clone. При P страницах rejected operation стоит O(P), вместо O(1) проверки ключа.

**Последствие.** Лишние allocations и atomic refcount updates под writer mutex; повторные stale removals блокируют другие writers без изменения состояния. Page-granularity COW сам по себе не устраняет O(P) copy списка страниц.

**Исправление.** Под тем же writer guard сначала проверить исходный snapshot, page, generation и occupied state, затем создавать next snapshot только для успешной мутации. `contains` аналогично может проверять slot без создания и немедленного drop `Arc<T>` через `get` (`:286`).

**Приёмка.** Allocation/refcount oracle для stale/out-of-range remove и contains; положительный write сохраняет snapshot visibility/retirement semantics. Численная экономия времени не измерялась.

### R2-21 — P3: remote-free drain EpochRegion выбрасывает capacity очереди

**Место:** `src/concurrent/epoch/epoch_region.rs:282`, `core::mem::take(&mut *q)`; producers — `:436–440`.

**Механизм.** Каждый непустой drain заменяет queue Vec на новый пустой Vec с нулевой capacity. Старый буфер уничтожается после переноса индексов. Следующий remote-free burst вновь расширяет очередь, хотя максимальное число distinct pending slots ограничено capacity region.

**Последствие.** Повторные allocate/free и копирования на steady-state пути, где достаточно повторно использовать уже оплаченный буфер; возможная аллокация происходит под remote queue mutex.

**Исправление.** Сохранять queue capacity: validated drain в заранее зарезервированный owner free list либо обмен двух reusable buffers. Выбор делать с учётом длительности критической секции, не заменять короткий lock неконтролируемо длинной работой.

**Приёмка.** После warm-up фиксированный remote churn не создаёт новых queue allocations; len/reusable-vs-retired accounting остаётся корректным.

### R2-22 — P3: AllocStats описывает overflow как потерю, хотя второй уровень обычно спасает free

**Место:** `src/global/alloc_stats.rs:95–105`, `ring_overflows`; чтение counter — `src/global/sefer_alloc/diag.rs:99`; спасённая ветка — `src/registry/heap_core_xthread/overflow.rs:687`.

**Механизм.** Field читает `DBG_RING_OVERFLOW`, который увеличивается при неуспешной **первой** попытке segment ring. После этого overflow ring или retries могут успешно сохранить free. Rustdoc же утверждает, что block discarded, а высокий counter означает actual leaked blocks. Точный terminal-drop counter другой: `DBG_RING_PUSH_RETRY_EXHAUSTED`.

**Последствие.** Ложная диагностика leak/ложные alerts и невозможность через обычный AllocStats отличить handled pressure от R2-09. Также `decommit_calls` (`alloc_stats.rs:71`) считает заходы в helper, включая `release_follows` early return без реального decommit syscall (`small/alloc_core_small_pool/decommit.rs:146–154`).

**Исправление.** Описать `ring_overflows` как first-tier misses, вывести отдельную метрику terminal discarded frees и явно разделить logical release/decommit events от OS syscall counts. `AllocStats` уже non-exhaustive; совместимое добавление счётчика предпочтительнее молчаливой смены смысла существующего поля.

**Приёмка.** Раздельные deterministic cases: ring miss + overflow success, retry success, terminal drop; одна и та же операция не должна интерпретироваться как утечка во всех трёх случаях.

### R2-23 — P3: исходники содержат противоречащие коду safety/feature/config обещания

**Места и механизмы:**

- `src/concurrent/epoch/hand.rs:3–6,103–107,433–437`: утверждается единственный unsafe module, `forbid` в остальных и unconditional Send/Sync. Crate root использует conditional forbid + deny с несколькими seams; фактические impls `hand.rs:461,471` ограничены `T: Send + Sync`. Исправить описания на текущие условия, не удалять правильные bounds ради согласования с устаревшим текстом.
- `src/concurrent/epoch/epoch_region.rs:3–5,23–27` и `src/concurrent/sharded/sharded_region.rs:48`: полная remote removal называется lock-free, но `remote_evict` берёт `Mutex<Vec<u32>>`. Lock-free лишь eviction CAS; после него ещё есть блокирующая bookkeeping часть. Так и разделить контракт.
- `src/concurrent/lock_free/lock_free_region.rs:151–154`: фраза, что ни один tier не выдаёт live handle при generation MAX, противоречит предшествующему тексту и `remove:362–369`/`insert_reusing:401`: LockFreeRegion делает именно одну такую последнюю выдачу. Исправить текст; сама описанная retirement policy не объявляется дефектом.
- `src/alloc_core/platform/dirty_by_class.rs:94–97`, `src/lib.rs:242`: `class-aware-dirty` описан как experimental/not production, хотя manifest включает его в production. Обновить feature status и unsafe inventory без исторического «раньше» в роли текущего контракта.
- `src/lib.rs:45–46`: `stats()` безусловно назван несколькими relaxed loads/no allocation. На самом деле при alloc-stats есть O(minted slots) walk, а R2-11 ещё сильнее нарушает формулировку. Краткое crate-root описание должно совпадать с уточнённым method-level контрактом.
- `src/alloc_core/config/large_cache_config.rs:265–276`: default budget описан как unbounded; `resolved_budget_bytes:403–407` при `large-cache-extended` возвращает finite default. `:285` обещает не опускаться ниже headroom, хотя whole-span eviction в `alloc_core_large_cache.rs:624–632` может пересечь этот уровень. Описать feature-dependent default и headroom как trigger/target с granularity overshoot, либо реализовать настоящий floor.
- `src/alloc_core/platform/sidecar.rs:1,62–71`: module intro ссылается на `OwnedSidecar`, которого в реализации нет; далее сам же объясняет отсутствие такого типа. Убрать несуществующий intra-doc target и назвать фактический набор функций. Это статически видимая ошибка ссылки, не утверждение о запущенном rustdoc.

**Последствие.** Ошибочный safety/performance mental model, неправильная feature матрица ревью и ложная уверенность от многостраничных «доказательств». Определить авторство текста как AI по коду нельзя; объективный дефект — противоречия, а не стиль автора.

**Исправление и приёмка.** Свести каждый текущий контракт к короткому проверяемому утверждению, историю вынести в design/changelog. Проверить docs в default, no-default, production, experimental и opt-in конфигурациях отдельно; не считать комментарий со ссылкой на старый успешный тест доказательством текущей истины.

## P4 — сопровождаемость

### R2-24 — P4: mod.rs используется как файл реализации вопреки собственному правилу проекта

**Место:** `src/alloc_core/alloc_core/mod.rs:298` (`AllocCore`), `src/alloc_core/segment/segment_header/mod.rs:374,771`, `src/alloc_core/segment/segment_table/mod.rs:210,255`, `src/alloc_core/segment/segment_directory/mod.rs:322,361`, `src/alloc_core/small/alloc_core_small/mod.rs:128`.

**Механизм.** CLAUDE.md устанавливает reexports-only для mod.rs, но перечисленные файлы содержат состояния и substantial executable implementation; часть достигает 700–900 строк. Новые split-модули существуют одновременно с такими смешанными roots, поэтому правило уже не объясняет, где искать владельца инварианта.

**Последствие.** Более дорогая навигация/review, повторение исторических контрактов и лёгкий drift между местом определения, реализацией и façade. Само имя файла не вызывает UB.

**Исправление.** Либо действительно вынести owning types/impls в именованные файлы и оставить mod.rs wiring-only, либо явно принять точечные исключения в conventions. Не делать автоматическую массовую перестановку кода одновременно с safety fixes: структурную работу проводить отдельно после закрытия P1.

## Дополнительные возможности оптимизации и проверки рисков

Ниже — направления, не включённые в 24 подтверждённые карточки и не выданные за измеренные ускорения:

1. `AllocCore::drop`, `src/alloc_core/alloc_core/lifecycle.rs:431`: временный массив на `MAX_SEGMENTS` занимает 64 KiB при 64-bit pointers независимо от числа live spans. Таблица живёт в primordial; можно рассмотреть освобождение остальных reservations с сохранением primordial до последнего шага, чтобы избежать фиксированного stack scratch. Потребуется доказать отсутствие дальнейших обращений к освобождённым headers и сохранить таблицу живой. Недостаточная stack-size сама по себе здесь не воспроизводилась.
2. `src/alloc_core/platform/os.rs:582`, `read_directory_class_words`: на каждый выбранный bucket копируется 64 u64. Это сознательно избегает overlapping reference, но можно читать отдельные words raw field-specific accessors, не создавая долгого borrow всего sidecar. Нужна проверка aliasing и измерение; обещать ускорение без codegen/bench нельзя.
3. `src/alloc_core/segment/segment_table/hash.rs:27`: hash использует низкие биты segment number без mixing. Регулярные VM spacing могут давать clusters. Bounds table защищают capacity, но стоимость probe/remove зависит от распределения адресов; сначала измерить реальные probes, не менять hash вслепую.
4. `src/alloc_core/small/alloc_core_small/find_segment.rs:75–81`: scalar refill выдаёт один block, а до 31 следующих проводит через carve/free bookkeeping; batch path избегает этой части. Возможен общий batch primitive для scalar slow path, но важно сохранить prefer-free, live_count, virginity и all-features behavior.
5. `src/alloc_core/large/alloc_core_large_cache.rs:602`: saturating multiplication до деления и наивное сравнение seq требуют отдельного numeric-domain анализа. На нынешнем 64-bit адресном пространстве обычные cache sizes не достигают overflow; 32-bit allocator уже остановлен R2-17. Поэтому это **не** дополнительный подтверждённый runtime bug. При поддержке 32-bit использовать widened arithmetic для процентов и явно определить seq wrap/exhaustion.
6. Бounded retry содержит nested CAS retry loops и scheduler sleep, поэтому число outer rounds — не строгая wall-clock deadline. Формулировка `overflow.rs:547–548` о hard wall-clock bound нуждается в ослаблении до algorithmic retry budget, если никакая реальная deadline не используется.
7. `small_cur` и process-lifetime recycled heaps сохраняют часть памяти по дизайну; обычный trim не является full global scavenger и не дренирует все remote queues. Нужны отдельно сформулированные retention bounds для idle owner, exited owner и standalone owner. Не смешивать этот штатный reserve floor с безвозвратно потерянными notes из R2-09.

## Что не было ошибочно объявлено уязвимостью

- Передача interior/stale/foreign pointer в **unsafe** dealloc/realloc в нарушение их Safety contract сама по себе не считается library soundness finding. Defence-in-depth hardening не превращает любой invalid free в разрешённый вход.
- `SegmentKind::kind_at` читает raw byte и сопоставляет неизвестные значения с `Unknown`; это правильная validate-before-enum граница. Такой же гарантии нет у full aggregate `read_at`.
- Возврат raw pointer из alloc безопасным методом не позволяет safe-клиенту автоматически разыменовать его. R2-01 отличается тем, что разыменование происходит внутри safe `Iterator::next` библиотеки.
- `AtomicSlot<T>` manual Send/Sync фактически имеют bounds; замечание R2-02 касается deferred destruction через доступный single-thread safe API, не несуществующего unconditional impl.
- Reclaim release выполняется после возврата ring drain и публикации его head guard; resolved dirty target не требует повторно читать освобождённый segment после успешного enqueue. Эти связи просмотрены, но не выдаются за полное доказательство каждого interleaving.
- `sidecar::reserve_zeroed_with` делает raw fixup до создания typed reference; изменение NUMA sentinel само по себе не является mint-before-validate дефектом. Owner-only leaking, однако, имеет другую проблему R2-12.
- В src нет исполняемых прямых C/Windows FFI declarations, `transmute` или `Vec::from_raw_parts` cleanup-пары, которые можно было бы ошибочно обвинить по одному ключевому слову: OS FFI вынесен в dependency seams. Полная нативная FFI аттестация этих crates этим review не заменяется.

## Release/readiness verdict по поверхностям

| Поверхность | Вердикт |
|---|---|
| Текущее дерево целиком, все документированные opt-ins | **NO-GO**: независимые P1 |
| Обычный production, internals off | **NO-GO для crate как безопасной публичной библиотеки**: R2-01 доступен через AllocCore; дополнительно R2-09/10/11/19 требуют release-решения в соответствующих конфигурациях |
| Только установленный SeferAlloc в штатном single-config 64-bit release, без diagnostic calls | Не найден универсальный immediate alloc/zero/realloc corruption path в этой узкой подповерхности; это **не GO**, не тестовое подтверждение и не снимает retention/long-run рисков |
| experimental / batch-api / pinning | **NO-GO** из-за R2-02–04; feature status legacy не оправдывает safe UB |
| internals / bench-internals | **NO-GO** как safe-callable tooling до R2-05–07; результаты defective oracle R2-14 нельзя использовать как release evidence |
| Default Region-only / no_std glue | В просмотренном root glue нового soundness-дефекта не подтверждено; независимый `sefer-region` и dependency closure полностью не аттестованы |

Минимальная последовательность: сначала закрыть P1 и unsafe boundaries; затем lossless reclaim/retention и диагностические oracles; далее feature/target contract и production dependency graph; только потом переоценивать performance и выпуск. Даже после статических исправлений требуется явная новая авторизация на перечисленные ниже исполняемые проверки.

## Необходимые проверки после отдельного разрешения

Все пункты этого раздела **не выполнялись** в данном раунде.

1. **API lifetime/type contract.** Compile-fail tests на lifetime segment iterator, EBR payload bounds и невозможность вызвать unsafe test seams из safe клиента. Проверить видимость как с `internals`, так и без него; `#[doc(hidden)]` не считать compile boundary.
2. **EBR state machine.** Cross-region handles, occupancy/generation coherence, retirement, destructor thread/lifetime и slot uniqueness. Контролируемые scheduling points и негативная калибровка oracles; не проверять только happy-path insert/get/remove.
3. **Provenance и data races.** Miri для canonical-pointer diagnostics/iterator lifetime; TSan либо подходящая actual-type модель для whole-header reads против remote Large push. Строгий provenance режим применять с учётом намеренно exposed-provenance owner-head seam; не объявлять весь путь strict-provenance-proven, если прогон использует permissive fallback.
4. **Queues.** Проверки panic/reentrancy consumer contract, late publication, sidecar materialisation/probe race, wrap с reduced-width actual protocol, full queue при paused/exited owner. Учитывать single-consumer ownership отдельно от атомарности элементов. Kani арифметика и зелёная модель другой структуры не заменяют эту проверку.
5. **Allocator semantic matrix.** alloc/alloc_zeroed/realloc/free для Small/medium/Large, boundary size/alignment, growth/shrink, batch/scalar, own/cross-thread, cache hit/miss, OOM до/после частичного refill. Сравнивать contents, alignment, non-overlap, ownership старого блока при failure и zeroed контракт.
6. **Feature matrix.** Default, no-default, alloc-core, alloc-core+alloc-xthread, alloc-global без xthread, production, production+alloc-stats, hardened, batch-api, virgin-zero-skip, medium-classes(+wide), exact-span-large, large-reserved-capacity, large-cache-extended, NUMA и обе lazy-commit axes. Особенно проверять exact/reserved-capacity **без** numa-aware: all-features выбирает другую ветку.
7. **Платформы/MSRV.** MSRV 1.88 и выбранный release toolchain; x86_64 Linux/Windows, aarch64 Linux/macOS, реальные страницы >4 KiB; явное решение для 32-bit. VM failure behavior и zero guarantee native backends проверять отдельно от Miri/mocks. QEMU smoke не равен полному weak-memory доказательству.
8. **OOM/error paths.** Publication count/chunk race и отсутствие allocation/abort в stats; commit failure не двигает frontier до успеха; ownership descriptors release ровно один раз. Fault injection должна работать в отдельной тестовой конфигурации, но отсутствовать в обычном production graph.
9. **Паника и teardown.** Прямые GlobalAlloc calls и global-allocator shims, panic=unwind/abort, config collision, TLS teardown, fallback initialization/lock и user panic hook. Проверки должны предотвращать unwind из allocator, а не лишь ловить его снаружи и считать это успехом.
10. **Метрики и bounded retention.** Независимые ledgers для VM reservations, occupied cache slots, actual discarded notes, live blocks и release; standalone-core churn и idle/recycled heap retention. Исправленный reconciliation сначала калибруется известными состояниями; он не может сам быть единственным доказательством собственной правильности.
11. **Документация и packaging.** Проверить rustdoc именно docs.rs `production` set, плюс default/no-default/experimental, а не только all-features. Проверить целостность публичных ссылок, Support/unsafe inventory, exported test APIs, package graph и актуальные dependency advisories. В этом раунде advisory audit/packaging не выполнялись.
12. **Performance после correctness.** Сначала deterministic allocation/VM/probe/lock counters для R2-19–21 и предложенных оптимизаций, затем scoped benchmarks с одинаковыми features/entry point/workload. Никакой benchmark результат прошлого отчёта не подменяет повторную проверку изменённого пути.

## Инвентарь src

Инвентарь baseline, не список «доказанно безопасных» файлов. Число строк получено статическим чтением. Wiring/proof/diagnostic files также включены, а не отброшены по названию.

| Файл | Строк |
|---|---:|
| `src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs` | 526 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs` | 263 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/mod.rs` | 57 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs` | 249 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs` | 319 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/totals.rs` | 51 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/vmem.rs` | 99 |
| `src/alloc_core/alloc_core/bootstrap.rs` | 424 |
| `src/alloc_core/alloc_core/counters.rs` | 424 |
| `src/alloc_core/alloc_core/lifecycle.rs` | 467 |
| `src/alloc_core/alloc_core/mem/mod.rs` | 585 |
| `src/alloc_core/alloc_core/mem/realloc_fastpath.rs` | 450 |
| `src/alloc_core/alloc_core/mod.rs` | 723 |
| `src/alloc_core/alloc_core/state.rs` | 244 |
| `src/alloc_core/config/large_cache_config.rs` | 471 |
| `src/alloc_core/config/large_cache_mode.rs` | 39 |
| `src/alloc_core/config/mod.rs` | 25 |
| `src/alloc_core/config/profile.rs` | 449 |
| `src/alloc_core/config/small_segment_pool_config.rs` | 187 |
| `src/alloc_core/large/alloc_core_large.rs` | 772 |
| `src/alloc_core/large/alloc_core_large_cache.rs` | 959 |
| `src/alloc_core/large/deferred_large/drain.rs` | 78 |
| `src/alloc_core/large/deferred_large/layout_consistent.rs` | 87 |
| `src/alloc_core/large/deferred_large/mod.rs` | 47 |
| `src/alloc_core/large/deferred_large/push.rs` | 144 |
| `src/alloc_core/large/deferred_large/tail.rs` | 22 |
| `src/alloc_core/large/large_cache_extended.rs` | 212 |
| `src/alloc_core/large/mod.rs` | 34 |
| `src/alloc_core/mod.rs` | 225 |
| `src/alloc_core/platform/dirty_by_class.rs` | 215 |
| `src/alloc_core/platform/mod.rs` | 37 |
| `src/alloc_core/platform/node.rs` | 597 |
| `src/alloc_core/platform/numa.rs` | 144 |
| `src/alloc_core/platform/os.rs` | 763 |
| `src/alloc_core/platform/sidecar.rs` | 344 |
| `src/alloc_core/platform/size_classes.rs` | 349 |
| `src/alloc_core/segment/bitmap/alloc_bitmap.rs` | 124 |
| `src/alloc_core/segment/bitmap/magazine_bitmap.rs` | 139 |
| `src/alloc_core/segment/bitmap/mod.rs` | 19 |
| `src/alloc_core/segment/bitmap/segment_bitmap.rs` | 118 |
| `src/alloc_core/segment/mod.rs` | 75 |
| `src/alloc_core/segment/remote_free_ring/mod.rs` | 910 |
| `src/alloc_core/segment/remote_free_ring/ops.rs` | 580 |
| `src/alloc_core/segment/segment_directory/directory_stats.rs` | 98 |
| `src/alloc_core/segment/segment_directory/mod.rs` | 783 |
| `src/alloc_core/segment/segment_header/descriptors.rs` | 387 |
| `src/alloc_core/segment/segment_header/layout_asserts.rs` | 177 |
| `src/alloc_core/segment/segment_header/mod.rs` | 941 |
| `src/alloc_core/segment/segment_header/segment_header_gen_table.rs` | 158 |
| `src/alloc_core/segment/segment_header/segment_header_layout.rs` | 311 |
| `src/alloc_core/segment/segment_header/segment_header_meta_fields.rs` | 298 |
| `src/alloc_core/segment/segment_header/segment_header_views.rs` | 372 |
| `src/alloc_core/segment/segment_layout.rs` | 226 |
| `src/alloc_core/segment/segment_table/harness.rs` | 108 |
| `src/alloc_core/segment/segment_table/hash.rs` | 248 |
| `src/alloc_core/segment/segment_table/mod.rs` | 781 |
| `src/alloc_core/small/alloc_core_small/dealloc.rs` | 142 |
| `src/alloc_core/small/alloc_core_small/directory.rs` | 493 |
| `src/alloc_core/small/alloc_core_small/find_segment.rs` | 891 |
| `src/alloc_core/small/alloc_core_small/mod.rs` | 884 |
| `src/alloc_core/small/alloc_core_small/reserve.rs` | 469 |
| `src/alloc_core/small/alloc_core_small_diag.rs` | 346 |
| `src/alloc_core/small/alloc_core_small_magazine.rs` | 696 |
| `src/alloc_core/small/alloc_core_small_pool/decommit.rs` | 279 |
| `src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs` | 432 |
| `src/alloc_core/small/alloc_core_small_pool/mod.rs` | 933 |
| `src/alloc_core/small/alloc_core_small_reclaim.rs` | 501 |
| `src/alloc_core/small/mod.rs` | 40 |
| `src/alloc_core/small/reserved_small_segment.rs` | 244 |
| `src/concurrent/epoch/epoch_handle.rs` | 76 |
| `src/concurrent/epoch/epoch_region.rs` | 500 |
| `src/concurrent/epoch/hand.rs` | 480 |
| `src/concurrent/epoch/mod.rs` | 10 |
| `src/concurrent/lock_free/lock_free_handle.rs` | 75 |
| `src/concurrent/lock_free/lock_free_region.rs` | 462 |
| `src/concurrent/lock_free/mod.rs` | 8 |
| `src/concurrent/mod.rs` | 38 |
| `src/concurrent/pinning.rs` | 244 |
| `src/concurrent/sharded/mod.rs` | 8 |
| `src/concurrent/sharded/sharded_handle.rs` | 94 |
| `src/concurrent/sharded/sharded_region.rs` | 556 |
| `src/global/alloc_stats.rs` | 196 |
| `src/global/fallback.rs` | 586 |
| `src/global/mod.rs` | 57 |
| `src/global/sefer_alloc/batch.rs` | 118 |
| `src/global/sefer_alloc/core.rs` | 290 |
| `src/global/sefer_alloc/diag.rs` | 500 |
| `src/global/sefer_alloc/global_alloc.rs` | 202 |
| `src/global/sefer_alloc/mod.rs` | 148 |
| `src/global/tls_heap.rs` | 736 |
| `src/kani_proofs.rs` | 393 |
| `src/lib.rs` | 443 |
| `src/registry/bootstrap/chunk.rs` | 103 |
| `src/registry/bootstrap/ensure.rs` | 284 |
| `src/registry/bootstrap/loom_shim.rs` | 453 |
| `src/registry/bootstrap/mod.rs` | 236 |
| `src/registry/bootstrap/overflow_sidecar.rs` | 296 |
| `src/registry/bootstrap/registry.rs` | 365 |
| `src/registry/heap_core/alloc/batch.rs` | 389 |
| `src/registry/heap_core/alloc/hot.rs` | 907 |
| `src/registry/heap_core/alloc/mod.rs` | 5 |
| `src/registry/heap_core/core.rs` | 783 |
| `src/registry/heap_core/diag/diag_probes.rs` | 667 |
| `src/registry/heap_core/diag/mod.rs` | 9 |
| `src/registry/heap_core/diag/queries.rs` | 814 |
| `src/registry/heap_core/free/dealloc.rs` | 261 |
| `src/registry/heap_core/free/dealloc_batch.rs` | 362 |
| `src/registry/heap_core/free/dealloc_own_base.rs` | 488 |
| `src/registry/heap_core/free/mod.rs` | 18 |
| `src/registry/heap_core/free/realloc.rs` | 657 |
| `src/registry/heap_core/mod.rs` | 39 |
| `src/registry/heap_core/state/mod.rs` | 8 |
| `src/registry/heap_core/state/ownership.rs` | 269 |
| `src/registry/heap_core/state/tcache.rs` | 296 |
| `src/registry/heap_core/state/tcache_flush.rs` | 123 |
| `src/registry/heap_core_xthread/drain.rs` | 325 |
| `src/registry/heap_core_xthread/mod.rs` | 13 |
| `src/registry/heap_core_xthread/overflow.rs` | 925 |
| `src/registry/heap_core_xthread/ring.rs` | 158 |
| `src/registry/heap_core_xthread/routing.rs` | 285 |
| `src/registry/heap_core_xthread/stall.rs` | 124 |
| `src/registry/heap_overflow.rs` | 846 |
| `src/registry/heap_registry/claim.rs` | 502 |
| `src/registry/heap_registry/counters.rs` | 381 |
| `src/registry/heap_registry/mod.rs` | 83 |
| `src/registry/heap_registry/stack.rs` | 323 |
| `src/registry/heap_slot.rs` | 565 |
| `src/registry/mod.rs` | 101 |

## Статус задач раунда

- Изолированный baseline и инвентарь — выполнено.
- Независимый статический разбор src и cross-module контрактов — выполнено в оговорённых выше пределах.
- Проверка механизмов, приоритетов и подробный отчёт — выполнено.
- Исправления исходников и исполняемые проверки — не выполнялись, не были разрешены.
- Отчёт предназначен для отдельного documentation-only коммита; его SHA сообщается при передаче результата, а не в содержимом самого коммита.
