# Ревью раунда 6: конкурентность и атомарные протоколы `src/`

Дата: 2026-10-15
Объект: дерево `src/` на коммите `132ab85b`
Тема: memory ordering, ABA, cross-thread free, registry claim/recycle, maintenance, fallback, process exit, TLS teardown, route directory, прогресс и deadlock-порядок, соответствие loom-моделей коду.

## Метод и ограничения

- Только статическое чтение `src/**`. Как контекст прочитаны `tests/loom_*.rs`, `docs/CROSS_THREAD_STATE_MACHINES.md`, `docs/agent_reviews_round4/`, `docs/agent_reviews_round5/`, а также `docs/reviews/2026-10-11-solution-research-windows-exit-con01.md`, `docs/reviews/2026-10-11-owner-questions-decisions-ox.md` и `docs/correctness-open-items/` (для отделения известного от нового).
- Сборка, тесты, loom, Miri, бенчмарки и проектные скрипты не запускались. Ни одно чередование не проверено исполнением. «Подтверждено» означает: механизм виден в коде целиком, от входа до наблюдаемого исхода. «Гипотеза» означает: исход зависит от внешнего поведения (ОС, std), которое по коду не проверить.
- Файлы вне `src/` (`crates/*`, в частности `once-ptr-cell`) не ревьюировались. Ссылки на них только контекстные.
- Номера строк соответствуют `132ab85b`.

## Итог

Ошибок memory ordering в production-протоколах я не нашёл. Пары publish/consume согласованы:
- терминальная публикация `fetch_or(AcqRel)` против `swap(0, AcqRel)` на cut;
- CAS `LIVE→PENDING` на дескрипторе `LargeState` против `PENDING→CONSUMING`;
- `LIVE/MAINTENANCE→FREE` (Release) против выигрышного `FREE→*` (AcqRel);
- `initialised` (Release/Acquire);
- публикация mixed-leaf (Release/Acquire);
- refcount `EntryHandle` (AcqRel, освобождение на последнем pin);
- `FREE_SLOTS` summary/word с re-check;
- `SaturationHint` (версионный CAS).

ABA на дескрипторе Large исключён: дескриптор свежий на каждую регистрацию, обратного перехода в `LIVE` нет. ABA на pending-битах Small возможен только при double/stale free, а это нарушение контракта, и оно задокументировано.

Найденные дефекты касаются прогресса, деградации и выхода процесса, а не гонок данных:
1. **Medium.** После OOM конструктора `HeapCore` неинициализированный FREE-слот монополизирует `reuse_hint`. Готовые FREE-кучи из spill-bitmap становятся недостижимы, а каждая аллокация непривязанного потока заново пытается зарезервировать ОС-память и уходит на глобальный fallback-лок.
2. **Medium.** На Windows при выходе отказ от shard-лока в `register` превращается в null из `Own`- и `Fallback`-веток `GlobalAlloc`. Escape в `System` не срабатывает, и процесс падает через `handle_alloc_error` во время выхода. Это расходится с решением владельца «escape, а не abort».
3. **Low/Info.** Пять мелких находок (прототипный спин-лок без выхода, рост числа куч из-за аренды maintenance, `System.dealloc` под shard-локом, нечестные CAS-спины, trim fallback-кучи воркером) и два Info: пробелы и расхождения loom-моделей, а также неподтверждённые гипотезы.

| Серьёзность | Кол-во |
|---|---:|
| Critical | 0 |
| High | 0 |
| Medium | 2 |
| Low | 5 |
| Info | 2 |
| **Всего** | **9** |

## Таблица находок

| ID | Серьёзность | Статус | Место | Кратко |
|---|---|---|---|---|
| R6-CC-1 | Medium | подтверждено, новое | `src/registry/heap_registry/claim.rs:328-339`, `:689-706`; `src/registry/heap_registry/stack.rs:157-161`; `src/global/tls_heap.rs:276`, `:486-490` | После OOM конструктора слот возвращается в `reuse_hint` и отнимает его у готовых FREE-куч. Повтор ОС-резервации на каждой аллокации, сериализация на fallback-локе |
| R6-CC-2 | Medium | подтверждено; механизм частично известен (CON-01), исход не закрыт | `src/registry/segment_route/directory.rs:837-843`, `:870-875`, `:933-945`; `src/global/sefer_alloc/global_alloc.rs:41-55`, `:101-112`, `:143-182` | Отказ shard-лока при выходе на Windows: `register` даёт `Err`, аллокация возвращает null, следует abort. Escape в `System` покрывает только отказ fallback-лока |
| R6-CC-3 | Low | подтверждено; известно как CON-01 W5, не исправлено | `src/global/exact_object/exact_shard.rs:184-196`, `:222-265`; `src/global/sefer_alloc/global_alloc.rs:64-69` | `exact-object-proto`: чистый спин без `yield` и без отказа при выходе; лок берётся на каждом `dealloc`. Вечное зависание выхода, если убитый поток держал шард |
| R6-CC-4 | Low | подтверждено, новое | `src/registry/heap_registry/claim.rs:278-326`; `src/registry/heap_registry/stack.rs:157-178` | Claimant, попавший на слот в `MAINTENANCE`, теряет кандидата и чеканит новую кучу через `bump` (bump идёт раньше scan). Перманентный рост числа куч |
| R6-CC-5 | Low | подтверждено; упомянуто в CON-01 §1, не исправлено | `src/registry/segment_route/directory.rs:610-657` | `Shard::remove` освобождает `Block` через `System.dealloc` под shard-локом, в обход паттерна `Surplus` |
| R6-CC-6 | Low | подтверждено (производительность и справедливость) | `src/registry/segment_route/shard_lock.rs:49-95`; `src/global/fallback.rs:487-526`; `src/global/process_exit.rs:55-75` | CAS-в-цикле без read-only спина (TTAS), без FIFO. Горячий шард при producer→consumer; FFI-вызов ntdll на каждой итерации после 64 спинов |
| R6-CC-7 | Low | подтверждено | `src/global/maintenance_service.rs:144-150`; `src/registry/heap_core/state/ownership.rs:117-154` | Воркер каждые 10 мс делает полный cold-trim fallback-кучи (flush tcache, drain pool, `evict_all`) под глобальным fallback-локом |
| R6-CC-8 | Info | подтверждено (покрытие) | `tests/loom_*.rs` относительно `src/registry/heap_registry/*`, `src/alloc_core/segment/remote_bitmap/*`, `src/registry/segment_route/*` | Loom-модели не покрывают production-путь `ClassLeaves`, порядок «bump перед scan», OOM-ветки claim, pin-vs-unlink и fallback/maintenance |
| R6-CC-9 | Info | гипотезы | `src/global/process_exit.rs:30-52`; `src/registry/heap_registry/claim.rs:620-644`; `src/global/tls_heap.rs:494-536` | Ленивое разрешение `RtlDllShutdownInProgress` внутри ожидания лока; abort из `HeapLease::drop` при `panicking()` в rollback-ветках `finish_bind` |

## Подробности

### R6-CC-1 — Medium — OOM конструктора `HeapCore` монополизирует `reuse_hint`

**Механизм (подтверждено).**

1. В `claim_impl` (`src/registry/heap_registry/claim.rs:328-339`) первая материализация вызывает `materialise(idx)`, то есть `HeapCore::new`. Если она вернула `None`, вызывается `push_back_after_oom` и функция **сразу возвращает `None`**. Других кандидатов она не пробует.
2. `push_back_after_oom` (`claim.rs:689-706`) переводит слот `INITIALIZING→FREE` с `initialised == false`, а затем выполняет `reg.reuse_hint.swap(idx)`. Неисправный слот кладётся именно в hint, вытесняя предыдущий hint в bitmap.
3. Селектор `pick_with_hint_spill` (`src/registry/heap_registry/stack.rs:157-161`) всегда начинает с hint и возвращает его без проверки состояния.
4. `finish_bind(None)` (`src/global/tls_heap.rs:486-490`) возвращает `Fallback`, а `LOCAL` остаётся `null`. Поэтому **каждая** следующая аллокация этого потока снова входит в `bind_slow_tagged` (`tls_heap.rs:276`, `:417-422`).
5. Для сравнения: путь chunk-OOM (`claim.rs:266-276`) делает правильно. Он продолжает цикл через `scan_claim_recovery_free`, предпочитая уже инициализированные FREE-кучи. Для OOM конструктора аналога нет.

**Чередование, ведущее к отказу.** Условие: устойчивая нехватка памяти или commit (например, 32-битный VA или исчерпанный commit charge на Windows), при которой новый 4 MiB primordial и `System`-аллокации маршрута не проходят, а существующие кучи живы.

1. Все кучи заняты. T1 через `bump` чеканит индекс X, получает OOM в `HeapCore::new`, X оказывается в hint. Hint = X, bitmap = {}.
2. Поток-владелец кучи D завершается. `HeapLease::drop`: hint ← D, X вытесняется в bitmap. Затем владелец E: hint ← E, D вытесняется в bitmap. Hint = E, bitmap = {X, D}.
3. T2 берёт E из hint.
4. T3: hint пуст, `take_nonempty` берёт младший бит (`free_slot_hint.rs:54`). Если X < D (D был отчеканен после X), берётся X. OOM, X снова в hint. Hint = X, bitmap = {D}.
5. T4, T5, …: каждый берёт X из hint, получает OOM и возвращает X в hint. D, полностью готовая FREE-куча, недостижима, пока OOM сохраняется и никакой новый release не вытеснит X.
6. Каждая аллокация на T3…Tn повторяет шаги 4–5: CAS, попытку ОС-резервации и, после неудачи, работу на fallback-куче под глобальным spin-локом.

**Последствия.** Все новые потоки сериализуются на `fallback::LOCK`. На каждый `malloc` приходится неудачный системный вызов резервации. Готовые кучи и их кэши простаивают. Корректность памяти не нарушена, но под нагрузкой при давлении памяти пропускная способность обрушивается.

**Рекомендация.**
- (a) При OOM конструктора не класть индекс в `reuse_hint`. Достаточно `FREE_SLOTS.publish(idx)` или полагаться на scan, а текущий hint не вытеснять.
- (b) После OOM конструктора продолжать цикл с `scan_claim_recovery_free(reg, idx)`, симметрично chunk-OOM, с предпочтением `initialised`.
- (c) Ввести ограниченный backoff привязки на потоке, например отдельный sentinel или счётчик в TLS, чтобы не повторять резервацию на каждой аллокации.
- (d) Добавить в loom-модель селектора (`tests/loom_r19_registry_free_set.rs` и `tests/loom_r20_heap_reuse_spill.rs`) неинициализированный FREE-слот и инвариант «инициализированная FREE-куча достижима, пока есть claimant».

### R6-CC-2 — Medium — отказ shard-лока при выходе на Windows превращается в null и abort, а не в escape

**Механизм (подтверждено).**
- `RouteDirectory::register` (`src/registry/segment_route/directory.rs:837-843`, `:870-875`) при `lock_unless_exiting` → `None` возвращает `Err(RouteError::ProcessExiting)`.
- Эта ошибка проходит как OOM:
  - `RouteSlots::prepare` → `.ok()` (`src/alloc_core/segment/segment_table/route_slots.rs:120-122`);
  - `SegmentTable::register_payload` → `?` (`src/alloc_core/segment/segment_table/segment_table_impl.rs:393-396`);
  - Small: `src/alloc_core/small/alloc_core_small/reserve.rs:249-255` → `None`;
  - Large: `src/alloc_core/large/alloc_core_large.rs:692-703` → null, а при попадании в кэш `:456-471` → `alloc_large_slow` и снова `register`.
- В `GlobalAlloc` (`src/global/sefer_alloc/global_alloc.rs`):
  - ветка `CurrentHeap::Own` (`:55`, `:112`, `:182`) возвращает null как есть;
  - ветка `Fallback` уходит в `System` только при `None` от `with_fallback_heap` (`:41-49`, `:101-109`, `:147-176`), то есть только при отказе **fallback-лока**. Если лок взят, но внутренняя аллокация вернула null (`Some(null)`), escape не выполняется.

Дополнение: `RouteDirectory::remove` при отказе возвращает `false` (`directory.rs:933-945`). `RouteRegistration::drop` (`registration.rs:84-92`) тогда намеренно оставляет запись связанной, но вызывающий код (`segment_table_impl.rs:665-684`, `:504-507`) всё равно освобождает VA. Если ОС выдаст тот же VA выходящему потоку под новый сегмент, `register` получит `Duplicate` по тому же ключу и вернёт null.

**Чередование.**
1. Рабочий поток W выполняет `publish_foreign` → `lookup` (`directory.rs:902-931`) и держит shard-лок S.
2. Главный поток возвращается из `main` → `ExitProcess`. ОС убивает W внутри критической секции S. `RtlDllShutdownInProgress` = true.
3. Главный поток ещё в `Own`-привязке: его `AbandonGuard` срабатывает позже, а до этого идут atexit, деструкторы статиков и `DLL_PROCESS_DETACH` других DLL. Он выделяет Large-буфер или блок, для которого нужен новый Small-сегмент, и ключ попадает в S.
4. После 64 спинов `register` → `ProcessExiting` → null. Вызывающий (`Vec`, `String`, `format!`) → `handle_alloc_error` → abort, код `0xC0000409`/fastfail вместо нормального завершения.

**Оценка достижимости.** При плотном межпоточном free критические секции `lookup` занимают заметную долю времени (CON-01 research §2.3, сценарий S3). Каждая Large-выдача и каждый новый сегмент делают `register`, так что на выходе нагруженного сервера исход реален, а не теоретичен.

**Контекст.** CON-01 research (§3, V2) считала `register → Err` приемлемым «уже обработанным как OOM» исходом. Решение владельца (`docs/reviews/2026-10-11-owner-questions-decisions-ox.md`, вопрос 7: «мёртвый держатель → escape в `System`, не abort») было реализовано только для fallback-лока. Shard-лок на путях аллокации этим решением не покрыт.

**Рекомендация.** В `GlobalAlloc::alloc`, `alloc_zeroed` и `realloc` в обеих ветках, `Own` и `Fallback`, если результат null и `process_exit_in_progress()`, выполнять тот же escape в `System`. Проверка только на холодном пути null, на горячий путь не влияет. Для `realloc` подходит существующая схема копирования `min(old,new)` с утечкой старого. Добавить сценарий «shard» в подпроцессный оракул CON-01 с ожидаемым кодом 0.

### R6-CC-3 — Low — `exact-object-proto`: спин без отказа и без `yield` на каждом `dealloc`

**Механизм (подтверждено).**
- `ExactShard::acquire` (`src/global/exact_object/exact_shard.rs:184-192`) крутит `compare_exchange_weak` + `spin_loop` бесконечно, без `yield_now` и без `process_exit_in_progress`.
- При включённой фиче каждый `dealloc` сначала вызывает `ExactNarrow::try_dealloc` (`global_alloc.rs:64-69`) → `ExactTable::take` → `acquire`, если `live_hint != 0` (`exact_shard.rs:240-265`).
- Под этим локом выполняется ещё и `rehash` с `System.alloc_zeroed` и `dealloc` (`:45-77`, `:205-218`).

**Чередование.** Поток W держит шард K (`insert` или `take`). `ExitProcess` убивает W. Выходящий поток освобождает любой указатель, хеш которого попадает в K, при `live_hint(K) != 0`, и крутится вечно. Процесс не завершается. Вне выхода чистый спин без `yield` при вытеснении держателя (oversubscription, инверсия приоритетов на Windows) сжигает квант CPU.

**Статус.** Перечислено как W5 в CON-01 research («вне production»), но `lock_unless_exiting` сюда не перенесён. Фича opt-in, поэтому Low.

**Рекомендация.** Применить схему `ShardLock::lock_unless_exiting`: спин, затем `yield`, затем отказ при выходе. При отказе в `try_dealloc` возвращать `false`: указатель из `System` не найдётся в route directory и будет посчитан как unroutable и утечёт, что безопасно. При отказе в `register` освободить объект и вернуть null или выполнить escape. `rehash` вынести из-под лока по образцу `Surplus`.

### R6-CC-4 — Low — аренда maintenance заставляет claimant чеканить новую кучу

**Механизм (подтверждено).**
- В `claim_impl` (`src/registry/heap_registry/claim.rs:278-326`) кандидат из hint или bitmap в любом состоянии, кроме `EMPTY`/`FREE` (в том числе `MAINTENANCE`), ведёт к `continue`, а кандидат уже потреблён (`hint.swap(max)` или снятый бит).
- На следующей итерации `pick_with_hint_spill` (`stack.rs:157-178`) идёт hint → bitmap → `bump` → scan. `bump` идёт **раньше** scan, поэтому ниже `MAX_HEAPS` чеканится новый индекс, а заодно материализуется новый `HeapCore` с 4 MiB резервацией.

**Чередование.**
1. Единственная FREE-куча X находится в hint.
2. Воркер выполняет `try_maintenance_at(X)` (`claim.rs:126-151`), X → `MAINTENANCE`. Внутри шага выполняется `trim_cold_retention` с `evict_all` и munmap, так что окно не нулевое.
3. Новый поток N: hint ← max, X в `MAINTENANCE`, `continue`. Hint и bitmap пусты, `bump` → Y, материализация новой кучи.
4. Воркер отпускает X (hint ← X). Куч стало на одну больше, навсегда: кучи не уничтожаются.

Рост самоограничен: при двух и более FREE-кучах проигравший claimant берёт следующую из bitmap. Порядок величины — пик конкурентных потоков плюс O(1) на воркер.

**Рекомендация.** При наблюдении `MAINTENANCE` у кандидата не терять его: вернуть в bitmap или коротко повторить, либо перед `bump` выполнить один проход `scan_claimable_slot`. Альтернатива: воркер пропускает слот, совпадающий с текущим `reuse_hint`.

### R6-CC-5 — Low — `System.dealloc` под shard-локом в `Shard::remove`

**Механизм (подтверждено).**
- `RouteDirectory::remove` держит guard (`directory.rs:936-944`) и вызывает `Shard::remove` (`:610-646`).
- При опустевшем блоке или слиянии вызывается `recycle` (`:623`, `:633`, `:642`), который при уже занятом `spare` выполняет `Block::release` → `System.dealloc` **под локом** (`:649-656`).
- Путь `insert` для той же цели использует `Surplus`: освобождение после снятия лока (`:499-503`, `:876-878`).

**Последствия.** На каждом unregister или recycle сегмента критическая секция удлиняется до `HeapFree`/`free`. Это повышает конкуренцию с `lookup` межпоточных free и вероятность того, что держатель будет убит на выходе (см. R6-CC-2). Корректность не нарушена: `System` не возвращается в аллокатор.

**Рекомендация.** Передавать вытесненный `Block` наружу (`Option<SpareBlock>`) и освобождать после `drop(guard)`, как в `install`.

### R6-CC-6 — Low — нечестные CAS-спины без TTAS; горячий шард на межпоточном free

**Механизм (подтверждено).**
- `ShardLock::lock` и `lock_unless_exiting` (`src/registry/segment_route/shard_lock.rs:49-95`), а также `fallback::LockGuard::acquire` (`src/global/fallback.rs:505-525`) крутят `compare_exchange(_weak)` напрямую, без предварительного `load(Relaxed)`. Каждая попытка — RMW, кеш-линия пинг-понгует между ожидающими. FIFO и справедливости нет, голодание отдельного ожидающего не ограничено.
- Каждый межпоточный free берёт shard-лок по ключу сегмента владельца (`directory.rs:902-931`). В схеме producer→consumer все free одного сегмента бьют в один из 64 шардов.
- На Windows после 64 спинов каждая итерация вызывает `process_exit_in_progress()` (`src/global/process_exit.rs:55-75`) с FFI-вызовом в ntdll, затем `yield_now`.

**Рекомендация.** Перейти на TTAS (спин на `load(Relaxed)` до `false`) с экспоненциальным backoff. Пробу выхода прореживать (каждые N итераций) или кешировать на итерацию ожидания. Корректность не меняется.

### R6-CC-7 — Low — воркер полностью trim-ит fallback-кучу под глобальным локом

**Механизм (подтверждено).**
- Каждый проход воркера (`src/global/maintenance_service.rs:144-150`) вызывает `fallback::try_with_heap(|core| core.background_maintenance_step(..))`.
- `background_maintenance_step` (`src/registry/heap_core/state/ownership.rs:117-136`) после ограниченного ingress-шага вызывает `trim_cold_retention` (`:138-154`): `flush_all_tcache`, `drain_small_pool`, `release_empty_current_small_for_trim`, `evict_all`, `release_directory_for_cold_trim`.
- Для registry-куч это применяется только к FREE-слотам. Fallback же является общей **активной** кучей для потоков, у которых исчерпан registry, не удалась привязка (см. R6-CC-1) или идёт TLS teardown.

**Последствия.** Раз в 10 мс глобальный `fallback::LOCK` удерживается на время munmap/VirtualFree и сброса кэшей, а кэши активной кучи уничтожаются. Пользователи fallback (`with_heap`) ждут, растёт латентность. Порча памяти не происходит.

**Рекомендация.** Для fallback ограничиться ingress-шагом без cold-retention либо выполнять trim только после интервала простоя fallback-кучи.

### R6-CC-8 — Info — покрытие и расхождения loom-моделей с кодом

1. `tests/loom_sidecar_bitmap_actual.rs` компилирует настоящие `sidecar_bitmap.rs`, `bitmap_scan.rs` и `bitmap_cut.rs`, но с `ClassMap::Dense`. Production использует `ClassLeaves` (`src/registry/segment_route/small_sidecar.rs:68-71`), атомики которого берутся из `core`, а не из loom (`leaf_classes.rs:9`). Шапка теста это признаёт. Публикация mixed-leaf покрыта только shadow-моделью `tests/loom_r11_small_sidecar.rs`.
2. Модели селектора registry (`loom_r11_registry_claim`, `loom_r19_registry_free_set`, `loom_r20_heap_reuse_spill`) не содержат:
   - порядка «bump перед scan»;
   - ограничения `candidate < count` и `before == max`;
   - ветки OOM конструктора (`push_back_after_oom` в hint);
   - chunk-OOM recovery.

   Поэтому R6-CC-1 и R6-CC-4 этими моделями не ловятся. Кроме того, `loom_r11_registry_claim::recycle_one` использует `hint.store` вместо production-`swap` со spill, а claim делает CAS прямо в scan, а не «scan, затем повторное чтение состояния и CAS».
3. Захват pin под shard-локом против unlink и последнего pin не смоделирован. Это уже отслеживается как item 186 (`docs/correctness-open-items/TRACKED_verification_coverage.md`).
4. Нет модели для `fallback::LOCK`/`INIT_STATE` с откатом и для handshake `MaintenanceService::start` (std `Mutex`/`Condvar`). Риск низкий, код простой.
5. `tests/loom_sidecar_summary.rs` моделирует отклонённый протокол C‴ (summary-hint). В production summary отсутствует, и этот тест нельзя засчитывать в покрытие текущего пути.

**Рекомендация.** Расширить shadow-модель селектора по пункту 2 и завести actual-source loom-харнесс для `ClassLeaves`: перевести его атомики на `super::{AtomicU8, AtomicPtr}` по образцу `sidecar_bitmap.rs`.

### R6-CC-9 — Info — неподтверждённые гипотезы

1. **Ленивое разрешение детектора выхода внутри ожидания лока.** `process_exit_in_progress` при первом вызове выполняет `GetModuleHandleW` и `GetProcAddress` (`src/global/process_exit.rs:30-52`). Первый вызов может случиться в `lock_unless_exiting` или `LockGuard::acquire`, когда вызывающий **уже держит** `fallback::LOCK` (`with_heap` → alloc → `register` → конкурирующий шард). Если эти loader-API когда-либо ждут лок, удерживаемый потоком, который сам ждёт `fallback::LOCK`, возможен deadlock. На современных Windows `GetModuleHandleW` берёт SRW таблицы модулей, а не loader lock, и по коду это не проверяемо. Дёшево закрыть: разрешать адрес заранее (при первой привязке или инициализации fallback), вне любых локов аллокатора.
2. **Abort из `HeapLease::drop` при `std::thread::panicking()`** (`src/registry/heap_registry/claim.rs:620-624`). Rollback-ветки `finish_bind` (`src/global/tls_heap.rs:494-498`, `:530-535`) роняют lease. Если они будут достигнуты из аллокации в panic hook на свежем потоке, процесс абортится. Сейчас ветки достижимы только при вложенной аллокации во время claim или при отказе `GUARD.try_with`. Первое исключено M5, второе возможно лишь в поздней фазе teardown. Подтверждённого пути нет.

## Проверено без подтверждённых дефектов

- **Терминальная публикация Small.**
  - `SidecarBitmap::publish`: чтение класса (Acquire), затем `fetch_or(AcqRel)`.
  - Cut `swap(0, AcqRel)` по каждому слову; post-cut публикации остаются для следующего прохода, кредит удерживает сегмент. Live-счётчик уменьшается только при reclaim записи, поэтому reservation не освобождается при опубликованном, но не собранном бите.
  - Spill uniform→mixed: копия старого кода во все гранулы, затем публикация указателя Release.
  - Освобождение листьев только на последнем pin.
- **Large.**
  - Дескриптор: `LIVE(g)→PENDING(g)` у producer, `PENDING→CONSUMING` у owner, после чего unlink. Физическое слово меняет только owner (`claim_live`, abort при несоответствии).
  - Порядок «route unlink, затем release/cache VA» соблюдён в `unregister`, `recycle` и `reclaim_large_segment`.
- **Route directory.**
  - Pin берётся под shard-локом с self-check; refcount AcqRel; освобождение на последнем pin.
  - Отказ `remove` при выходе сохраняет owner-count, поэтому UAF записи нет.
  - `System`-аллокации в `register` выполняются вне лока.
- **Registry.**
  - Только CAS состояния даёт владение.
  - `initialised` проверяется после CAS `FREE→LIVE`; maintenance требует `initialised` до CAS.
  - Все переходы в `FREE` публикуют hint и вытесняют предыдущий в bitmap; потерянный кандидат всегда переопубликуется при следующем переходе в `FREE`.
  - `FREE_SLOTS` (summary clear + re-check) и `SaturationHint` (версия + `fetch_or(0)`) корректны.
- **Аренды.**
  - `HeapLease` и `MaintenanceLease`: Release-публикация при Drop, abort при проигранном CAS.
  - Воркер держит не более одной аренды и не берёт `CONTROL` во время аренды.
  - `start()` ждёт `RUNNING` под `CONTROL`, потерянного пробуждения нет.
- **TLS teardown.**
  - `TORN` выставляется до `LIVE→FREE`.
  - Пассивные резолверы (`current_for_dealloc`, `current_for_trim`) не привязывают слот.
  - Порядок «сначала `LOCAL`, затем `GUARD`» в `finish_bind`; на выходе (Windows) lease отпускается без trim.
- **Fallback.**
  - `INIT_STATE` с откатом при панике и OOM; `LockGuard` на RAII.
  - `try_with_heap` для межпоточного free fallback-блоков не ждёт и не рекурсирует.
  - Escape в `System` при отказе fallback-лока на выходе.
- **Порядок локов.** Возможна только вложенность «fallback-лок → shard-лок» (через аллокацию в fallback-куче). Обратной вложенности нет: под shard-локом не берутся ни fallback-лок, ни другие шарды. `ExactShard` вкладывает только `System`. Цикла нет.
- **Legacy `experimental`.**
  - `AtomicSlot` (seqlock g1/value/g2 на Acquire, CAS на поколении с отбраковкой null `old`), `EpochRegion` (флаг pending под мьютексом очереди), `LockFreeRegion` (`ArcSwap` + мьютекс писателя) — новых гонок не найдено.
  - Накопление claim-ов `ShardedRegion` после FIFO-вытеснения задокументировано как потеря пропускной способности.

Отсутствие подтверждённого дефекта в перечисленных областях не заменяет loom, Miri, TSan и стресс-прогонов, которые в этом ревью не запускались.
