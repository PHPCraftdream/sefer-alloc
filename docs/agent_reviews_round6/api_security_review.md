# Раунд 6 — ревью публичной поверхности API и безопасности `src/`

Дата: 2026-10-15
Объект: дерево `sefer-alloc` на коммите `132ab85b` (только `src/`, плюс сверка с `README.md`, `SECURITY.md`, `Cargo.toml`, `scripts/verify-dbg-hook-safety.mjs`)
Тема: публичная поверхность API, доступная из safe downstream-кода, и злоупотребления со стороны атакующего или небрежного пользователя.

## Метод и ограничения

- Только статическое чтение исходников. `cargo build/test`, Miri/Loom/Kani, бенчмарки и проектные скрипты не запускались, ни одно утверждение не проверено исполнением. Если сценарий помечен «подтверждено», это значит: путь управления целиком прослежен по коду, но не воспроизведён.
- Прочитаны: `src/lib.rs`, весь `src/global/**`, публичные `impl` крейт-корневых типов (`AllocCore`, `SeferAlloc`, `SegmentLayout`, конфиги), путь освобождения own-thread/foreign/sidecar, `registry/**` (claim/lease/maintenance/saturation/route directory), `platform/{os,numa,fatal,node}`, `exact_object/**`, конфигурационные резолверы и decay-арифметика.
- Feature-наборы, по которым определялась достижимость: default (`std`), `production`, `production internals`, `production hardened`, `production bench-internals`, `experimental`/`batch-api`/`exact-object-proto`. Достижимость определялась по видимости модулей (`src/lib.rs:443-493`) и `#[cfg]` на `impl`/методах.
- Не проверялись глубоко: `crates/*` (в том числе `aligned-vmem`, `numa-shim` и FFI-объявления в них), `sefer-region`/`slotmap`, экспериментальные `concurrent::*` (кроме поверхностного взгляда на test-хуки), `kani_proofs.rs`.
- Номера строк соответствуют `132ab85b`.

## Итог

Подтверждённой дыры в soundness, достижимой из **safe**-кода в `production` без `internals`, не найдено. Стабильная поверхность узкая. Это `SeferAlloc::{new,with_config,with_profile,stats,trim_current_thread,start_maintenance,maintenance_running,fallback_config_conflicts}`, `AllocCore::{new,new_with_config,alloc,alloc_zeroed}` плюс `unsafe` `dealloc`/`realloc`, `SegmentLayout` и конфиг-типы. Все пути с сырыми указателями — `unsafe fn`. Находки R5-MS-1..4 закрыты: `dealloc`/`realloc`/`flush_class` теперь `unsafe`, ring-хуки удалены.

Основной риск лежит в двух других плоскостях.

1. **Поведение при нарушении контракта `GlobalAlloc`.** Это defence-in-depth, а не soundness. Own-thread free в `production` доверяет `Layout` вызывающего и не проверяет, что указатель — начало блока. Interior-указатель или free с неверным `Layout` тихо превращаются в перекрывающиеся выдачи. Тот же misuse из другого потока даёт отложенный `abort` у владельца либо, при устаревшей class map, тоже порчу кучи. Free-list-ссылки хранятся в освобождённых блоках сырыми и без валидации: UAF-запись даёт классический примитив «выдать произвольный адрес». `hardened` закрывает часть этого, но не идентичность класса.
2. **DoS-векторы, достижимые извне.** Порог `MAX_HEAPS = 4096` живых потоков: дальше все потоки сериализуются на одном fallback-спинлоке, и этот предел не описан в «Honest limitations». Для Large минимум 4 МиБ reserve+commit на аллокацию чуть больше `SMALL_MAX`, плюс жёсткий предел 4095 живых Large на heap, после которого — `handle_alloc_error`.

Документация местами обещает больше, чем делает код. `SECURITY.md` относит double-free к «well-defined error», хотя код прямо отказывается от такой гарантии. Doc-комментарий `hardened` в `Cargo.toml` ссылается на несуществующий `reclaim_offset` и несуществующий раздел README. Name-based инвентарь хуков не видит safe-мутаторы под `internals` с «обычными» именами.

## Сводная таблица

| ID | Серьёзность | Feature-набор | Место | Кратко |
|---|---|---|---|---|
| R6-SEC-1 | Medium | `production` (частично и `hardened`) | `src/registry/heap_core/free/dealloc_own_base.rs:278-312,366` | Own-thread small free берёт класс из `Layout` вызывающего и не проверяет начало блока; interior/mismatched free → перекрывающиеся выдачи |
| R6-SEC-2 | Low | `production` | `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs:91-110`, `.../sidecar_bitmap/leaf_classes.rs:117-135`, `src/alloc_core/small/alloc_core_small_reclaim.rs:38-66` | Cross-thread interior free: отложенный `abort` у владельца, а после reset сегмента устаревшая class map даёт тихую порчу |
| R6-SEC-3 | Medium | `production` (без `hardened`) | `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:425-448,603-610,639` | Сырые, не проверяемые free-list-ссылки в теле блока: UAF-запись → выдача произвольного адреса; утечка адресов кучи |
| R6-SEC-4 | Medium | `production` | `src/global/tls_heap.rs:276,486-490`; `src/registry/heap_registry/stack.rs:157-171`; `src/registry/bootstrap/registry.rs:45` | Больше 4096 живых потоков: весь избыток сериализуется на fallback-спинлоке и на каждой аллокации заново пытается claim; не описано в «Honest limitations» |
| R6-SEC-5 | Medium | `production` | `src/alloc_core/large/alloc_core_large.rs:197-230,626-635`; `src/alloc_core/platform/os.rs:224-236` | Large ≥ `SMALL_MAX` всегда reserve+commit кратно 4 МиБ (до ~16× амплификации) плюс жёсткий предел 4095 живых Large на heap → `abort`, вызываемый извне |
| R6-SEC-6 | Low | `production` | `src/global/sefer_alloc/maintenance.rs:36-38`; `src/global/maintenance_service.rs:30,64-69,71-108,129-169` | Любая зависимость может безвозвратно запустить процессный worker (100 пробуждений/с навсегда); `start()` ждёт без таймаута |
| R6-SEC-7 | Low | все | `SECURITY.md:59-86` | Scope политики противоречит коду: double-free как «well-defined error», panic как out-of-scope, нет статуса `internals`/prototype-фич |
| R6-SEC-8 | Low | `hardened` | `Cargo.toml:671-688` (`:682`, `:687`) | Doc-комментарий `hardened` устарел: несуществующий `reclaim_offset`, несуществующий раздел README, неверное «identical check» для cross-thread |
| R6-SEC-9 | Low | `alloc-global` без `bench-internals` | `src/global/tls_heap.rs:101,653` | Модульный tier-1 `#![allow(unsafe_code)]` в файле, где без `bench-internals` нет ни одного `unsafe` |
| R6-SEC-10 | Low | `production internals` | `src/registry/segment_route/directory.rs:716-719,794-880`; `src/registry/heap_registry/claim.rs:85-87,612`; `scripts/verify-dbg-hook-safety.mjs:311` | Safe-мутаторы глобального состояния с «не-dbg» именами (`RouteDirectory::register`, `HeapLease::core`) вне инвентаря сканера; DoS через `mem::forget` |
| R6-SEC-11 | Low | `exact-object-proto` (+`batch-api`) | `src/global/sefer_alloc/batch.rs:115-136`; `src/global/exact_object/exact_shard.rs:184-192` | `dealloc_batch` обходит ExactNarrow (утечка System-объектов); спинлок шарда не сдаётся при выходе процесса |
| R6-SEC-12 | Low | `production`, Windows | `src/global/sefer_alloc/global_alloc.rs:41-48,143-183`; `src/registry/heap_core/free/realloc.rs:418-423`; `README.md:1696-1699` | System-блок, выданный при выходе процесса, при `realloc` → null → `handle_alloc_error`; README описывает только free |
| R6-SEC-13 | Info | граф зависимостей | `Cargo.toml` `[features]` | Аддитивные фичи (`exact-object-proto`, `virgin-zero-skip`, `hardened`, `bench-internals`) включаются любой зависимостью и меняют глобальный аллокатор |
| R6-SEC-14 | Info | `production` | `src/global/sefer_alloc/global_alloc.rs:33-185`; `src/alloc_core/platform/numa.rs:94`; `alloc_core_small_impl.rs:447,608,639` | Нет abort-on-unwind предохранителя в `GlobalAlloc`; debug-overflow panic при порче free-list |
| R6-SEC-15 | Info | `production` | `src/alloc_core/alloc_core/state.rs:184-206`; `.../alloc_core_core_diag/totals.rs`; `.../alloc_core_small_pool_impl.rs:503-504` | Недокументированный публичный хвост крейт-корневого `AllocCore` (`segment_bases`, 4 статических `dbg_*`) |
| R6-SEC-16 | Info | Windows | `src/global/process_exit.rs:9-15,66-71` | Тип FFI объявлен как `fn() -> isize`, вызывается как `fn() -> u8`; безвредно, но рассогласовано |

Итого: Critical 0, High 0, Medium 4, Low 8, Info 4.

## Подробности

### R6-SEC-1 — Medium — own-thread free доверяет `Layout` и не проверяет начало блока

**Достижимо:** `production` через `GlobalAlloc::dealloc`/`realloc`, то есть `unsafe` вызывающего с нарушенным контрактом. В `hardened` закрыта только interior-часть.

**Код.** `HeapCore::dealloc_own_thread_with_base` вычисляет класс из `Layout` вызывающего (`SizeClasses::class_for(size, align)`, `dealloc_own_base.rs:366`) и вызывает `small_free_guard`. Без `hardened` цепочка guard'ов (`:292-310`) проверяет только `off >= payload_start`, отсутствие в magazine, `off < bump` и `!is_free(off)`. Interior-гранула живого блока в AllocBitmap читается как «allocated» (бит снят), поэтому interior-указатель проходит `Accept` и попадает в magazine класса `c`. Проверка `off % block_size(c)` есть только под `hardened` (`:278-286`). Идентичность класса (класс блока против класса из `Layout`) не проверяется ни в одной сборке, хотя owner class map в route-sidecar существует и производитель cross-thread free использует именно её.

**Сценарий** (обычный баг FFI или ручного `unsafe` в зависимости):
```text
let p = alloc(Layout::from_size_align(64, 16));   // class A
dealloc(p.add(16), Layout::from_size_align(48,16)); // interior + чужой класс
let q = alloc(Layout::from_size_align(48, 16));   // q == p+16 — внутри живого p
```
Итог: две живые выдачи перекрываются, запись через `q` портит `p`. Тот же misuse с другого потока приводит к `abort` (см. R6-SEC-2), то есть строгость зависит от того, какой поток освобождает.

**Рекомендация.** Под `hardened` сверять класс из `Layout` с классом гранулы в owner class map (одна Acquire-загрузка; для выданных блоков это заменяет и модульную проверку). В `production` хотя бы задокументировать асимметрию own-thread и cross-thread в «Honest limitations» и в описании `hardened`. Деление на не-степень двойки заменяется умножением на обратное (reciprocal), если цена `%` — единственный аргумент против.

### R6-SEC-2 — Low — cross-thread interior free: отложенный `abort` или порча через устаревшую class map

**Достижимо:** `production` через `GlobalAlloc::dealloc` из чужого потока с нарушенным контрактом.

**Код.** Производитель (`SidecarBitmap::publish`, `sidecar_bitmap.rs:91-110`) отклоняет только гранулы с нулевой кодировкой класса. По его же комментарию, «interior pointer into an issued block is NOT rejected» для uniform-leaf. Владелец в `reclaim_sidecar_record` (`alloc_core_small_reclaim.rs:38-45`) при `off % block_size != 0` вызывает `std::process::abort()`. Это происходит позже, в другом потоке, во время drain/trim/maintenance-шага. Кроме того, class map монотонна на всё время жизни маршрута: `LeafClasses::issue` (`leaf_classes.rs:117-135`) только перезаписывает начала новых блоков, сброса нет. После reset сегмента (pool-reuse или decommit-retain, когда bump обнуляется) старое начало блока класса `c1` может оказаться внутри живого блока класса `c2`. Cross-thread free такого адреса пройдёт и кратность `block_size(c1)`, и `!is_free`, и тогда `Node::write_next` (`:63`) пишет free-list-узел в середину живого блока.

**Статус.** Путь прослежен статически, требует нарушения контракта. Не воспроизведено.

**Рекомендация.** Признать в документации, что cross-thread misuse может приводить к `abort` в произвольном потоке владельца, а не к no-op (сейчас README говорит только «not legal»). Для `hardened` рассмотреть очистку class map при reset сегмента или проверку «гранула — начало блока» по bump-эпохе.

### R6-SEC-3 — Medium — free-list-ссылки хранятся сырыми и не проверяются вне `hardened`

**Достижимо:** `production`; срабатывает при любой UAF-записи в освобождённый small-блок (баг в `unsafe`-коде приложения или в C-зависимости).

**Код.** `pop_free` (`alloc_core_small_impl.rs:425-448`) читает `next` из первого слова свободного блока. Без `hardened` новый head вычисляется как `(next as usize - segment as usize) as u32` (`:447`) без проверки диапазона, кратности, `is_free` или принадлежности сегменту. Аналогично в `try_drain_freelist_batch` (`:603-610`) и на `:639`. Следующий `pop_free` выполняет `Node::deref(segment, head_off)` для произвольного 32-битного смещения, то есть до 4 ГиБ за базой сегмента. Указатели не маскируются (нет аналога glibc safe-linking).

**Сценарий.** Типичная эксплуатация tcache poisoning: UAF-запись 8 байт в освобождённый блок → `alloc` возвращает блок, затем «следующий» `alloc` возвращает адрес, выбранный атакующим (в пределах `segment + u32`) → запись в чужие данные или метаданные. Побочно: первое слово свежевыданного (не обнулённого) блока содержит старый `next`, то есть адрес кучи. Это утечка адресов, полезная для обхода ASLR. В debug-сборке вычитание при `next < segment` паникует внутри `GlobalAlloc` (см. R6-SEC-14).

**Рекомендация.** Валидация `hardened` (`valid_free_node`/`free_continuation`, `:372-423`) уже есть. Для `production` дешёвый вариант — хранить смещение, а не указатель, и маскировать его XOR'ом с per-segment секретом или битами адреса, проверяя только `offset < bump` и кратность классу при pop. Как минимум нужен пункт в «Honest limitations», что `production` не защищает от free-list poisoning.

### R6-SEC-4 — Medium — обрыв при `MAX_HEAPS = 4096` живых потоков

**Достижимо:** `production`; удалённо — в thread-per-connection-серверах (атакующий открывает больше 4096 соединений).

**Код.** `MAX_HEAPS = 4096` (`registry.rs:45`). Слот освобождается только при выходе потока. Когда все слоты LIVE, `finish_bind(None)` возвращает `Fallback`, не меняя `LOCAL` (`tls_heap.rs:486-490`). Поэтому **каждая** аллокация такого потока снова идёт в `bind_slow_tagged` → `claim_lease` → `pick_with_hint_spill`. Там выполняется RMW `reuse_hint.swap` (`stack.rs:157`) и RMW `saturation.is_saturated()` (`fetch_or`, `saturation.rs:41`) на общих кэш-линиях, после чего захватывается глобальный fallback-спинлок (`global_alloc.rs:41`). Освобождения таких потоков (LOCAL null) идут через `publish_foreign` с захватом шард-лока route directory на каждый free. Любой `publish_claimable` (выход любого потока) сбрасывает saturation, и следующая аллокация каждого избыточного потока заново сканирует до 4096 слотов (`stack.rs:12-35`).

**Документация.** В README `MAX_HEAPS` упоминается только в таблице seam'ов (`README.md:693`). В «Honest limitations» этого обрыва нет. `docs/perf/OPEN_ITEMS.md` п. 66 описывает лишь некалиброванный порог backoff как «cold path».

**Рекомендация.** Добавить пункт в «Honest limitations» (порог, деградация, удалённая достижимость). Кэшировать в TLS вердикт «registry saturated», например sentinel `FALLBACK` в `LOCAL` с повтором claim раз в N аллокаций или по версии saturation, чтобы избыточный поток не выполнял два RMW на каждую аллокацию. Рассмотреть шардирование fallback-heap.

### R6-SEC-5 — Medium — минимальная цена Large 4 МиБ и жёсткий предел 4095 живых Large

**Достижимо:** `production` (`exact-span-large`/`large-reserved-capacity` в него не входят). Удалённо — в сервере, где на запрос выделяется буфер больше `SMALL_MAX` (≈253 КиБ по документации `Cargo.toml`).

**Код.** Без `exact-span-large` `usable = needed.div_ceil(SEGMENT) * SEGMENT` (`alloc_core_large.rs:226-230`), затем `Segment::reserve(usable)` (`:631-635`) → `vmem::reserve_aligned` (`os.rs:224-236`). На Windows это eager `MEM_RESERVE | MEM_COMMIT` (`crates/aligned-vmem/src/os/windows.rs:32`). Комментарий в коде прямо говорит: «Every Large request today pays a MINIMUM of 4 MiB reserved+committed address space (a 260 KiB request costs the same as a 4 MiB one)» (`alloc_core_large.rs:199-202`). Каждый живой Large занимает слот `SegmentTable` своего heap. После 4095 слотов `alloc` возвращает null, что для `GlobalAlloc` означает `handle_alloc_error` — `abort`.

**Сценарий.** Однопоточный async-рантайм (`current_thread`), где каждое соединение держит буфер 260 КиБ. Атакующий открывает 4095 соединений → ~16 ГиБ commit charge на Windows (~16× от запрошенного ~1 ГиБ), следующее соединение → `abort` процесса. Предел 4095 описан в README (`README.md:1662-1687`), commit-амплификация — только в комментарии в коде.

**Рекомендация.** Описать амплификацию в «Honest limitations» и в «Memory policy». Рассмотреть продвижение `exact-span-large` (CONDITIONAL-GO, `docs/perf/OPEN_ITEMS.md` п. 28) или lazy-commit для Large в `production`. Явно рекомендовать multi-thread-рантаймы или `medium-classes` для серверов с ≥ 256 КиБ буферами на соединение.

### R6-SEC-6 — Low — `start_maintenance()`: безвозвратный процессный worker, вызываемый откуда угодно

**Достижимо:** `production`; любой safe-код любой зависимости. Работает, даже если `SeferAlloc` не установлен как `#[global_allocator]`.

**Код.** `SeferAlloc::start_maintenance()` — ассоциированная функция без `&self` (`maintenance.rs:36-38`). Она запускает detached-поток, который живёт до конца процесса и выполняет `park_timeout(10 ms)` в бесконечном цикле (`maintenance_service.rs:30,167`), то есть ~100 пробуждений в секунду и на простаивающем процессе. API остановки нет. Любой выход или unwind worker'а — `abort` (`:64-69`). Возможности запретить запуск у владельца бинарника нет. `start()` после spawn ждёт `RUNNING` на condvar **без таймаута** (`:101-105`). Если поток не может быть запланирован, вызов зависает навсегда: на Windows это вызов из DllMain или static-конструктора DLL под loader lock. Документация перечисляет «allocator callbacks, heap leases, fallback-lock closures and TLS teardown», но не loader lock и не конструкторы.

**Рекомендация.** Задокументировать, что вызов — глобальный побочный эффект, доступный любой зависимости, и добавить предупреждение про loader lock. Рассмотреть ожидание с таймаутом и отдельной ошибкой. Возможно, нужен opt-out для владельца бинарника (feature или cfg, запрещающий worker).

### R6-SEC-7 — Low — `SECURITY.md` расходится с реальными гарантиями

**Где:** `SECURITY.md:59-86`.

- `:64-65` относит к уязвимостям «Double-free — calling `remove` (or `dealloc` in the byte-allocator tier) … twice without triggering a well-defined error». Но `GlobalAlloc::dealloc` и `AllocCore::dealloc` — `unsafe fn`, и код явно не гарантирует обнаружения повторного free: «They do not guarantee detection of own-thread duplicate free/reissue» (`src/registry/heap_core/free/dealloc_own_base.rs:179-185`); README про `hardened`: «does not make duplicate or invalid foreign frees legal». Термин «byte-allocator tier» устарел. Внешний исследователь сочтёт штатный double-issue уязвимостью по политике проекта.
- `:80-81` выводит из scope «Panics reachable through documented panic conditions», хотя unwind из `GlobalAlloc` — UB по документации std, и проект сам так считает (`src/registry/heap_registry/claim.rs:186-195`, R2-08).
- Не определён статус `internals`, `bench-internals`, `experimental`, `batch-api`, `exact-object-proto`: относятся ли soundness-дыры в них к уязвимостям.

**Рекомендация.** Переформулировать так: double-free и неверный `Layout` через `unsafe` API — нарушение контракта вызывающего, а не уязвимость, если только поведение не хуже задокументированного. Добавить в scope: panic или unwind из любого метода `GlobalAlloc`. Явно перечислить test-only и prototype-фичи как out of scope (или как scope с пониженной серьёзностью).

### R6-SEC-8 — Low — устаревшая документация `hardened` в `Cargo.toml`

**Где:** `Cargo.toml:671-688`.

- `:682`: «The cross-thread leg is already covered UNCONDITIONALLY by `reclaim_offset`'s identical check». Функции `reclaim_offset` в `src/` нет. Нынешний аналог `reclaim_sidecar_record` не выполняет «identical check» с исходом no-op: он делает `abort` (R6-SEC-2), а производитель interior для uniform leaf не отклоняет.
- `:687`: «See README "Hardened vs production"» — такого раздела в README нет. Описание `hardened` живёт только строкой в Features matrix (`README.md:1439`).
- `:254` упоминает `RemoteFreeRing`, которого тоже больше нет.

**Рекомендация.** Привести комментарий к текущему протоколу sidecar и ссылаться на существующий раздел README. Это та самая документация, по которой пользователь решает, включать ли `hardened`.

### R6-SEC-9 — Low — tier-1 `#![allow(unsafe_code)]` в `tls_heap.rs` при отсутствии `unsafe`

**Где:** `src/global/tls_heap.rs:101`; единственный `unsafe` — `dbg_restore_local_for_test`, `:653`, под `bench-internals`.

Во всех сборках `alloc-global` без `bench-internals`, включая `production`, файл сохраняет модульное разрешение `unsafe`, но не содержит ни одного `unsafe`. Комментарий `:97-100` это подтверждает. Компиляторная граница «unsafe только в именованных seam'ах» здесь шире необходимого: будущий `unsafe` в самом горячем файле TLS-привязки (single-writer-инвариант) скомпилируется без сигнала.

**Рекомендация.** `#![cfg_attr(feature = "bench-internals", allow(unsafe_code))]` или item-level `#[allow(unsafe_code)]` на хуке (tier 2). Синхронизировать инвентарь в README и `src/lib.rs`.

### R6-SEC-10 — Low — safe-мутаторы глобального состояния под `internals` вне инвентаря сканера

**Достижимо:** `production internals`. `internals` — обычная аддитивная фича, её может включить любая зависимость.

**Код.**
- `RouteDirectory::global()` (`directory.rs:716-719`) и `pub fn register(&self, root: *mut u8, len, route_ptr: *mut u8, owner: usize, kind)` (`:794-880`) — safe, публичны через `sefer_alloc::registry::segment_route`. Они регистрируют в **процессном** route directory маршрут с произвольным ключом и произвольным `owner`, в том числе `OWNER_ID_FALLBACK`.
- `HeapRegistry::dbg_claim_lease()` (`claim.rs:85-87`) с `HeapLease::core()` (`:612`) выдаёт `&mut HeapCore` реального слота.

**Сценарий (DoS, не UB).** `mem::forget(RouteDirectory::global().register(addr, SEGMENT, addr, 0, RouteKind::Small)?)` для свободного SEGMENT-выровненного VA навсегда занимает ключ. Когда ОС позже выдаст этот VA реальному сегменту, `RouteSlots::prepare` получит `Duplicate` (`route_slots.rs:120-122`), `reserve_small_segment` освободит резерв и вернёт None (`reserve.rs:249-255`), аллокация вернёт null → `handle_alloc_error`. 4096 вызовов `mem::forget(HeapRegistry::dbg_claim_lease())` навсегда исчерпывают `MAX_HEAPS` (R6-SEC-4). Подделанный маршрут не может затенить существующий ключ (проверка `Duplicate`), и пути к записи по `root` не найдено, поэтому UB не подтверждено.

**Пробел инструмента.** `scripts/verify-dbg-hook-safety.mjs:311` инвентаризует только имена `^dbg_|_for_tests?$|inject` и восемь `ROOT_FORWARDERS`. `register`, `lookup`, `RouteRegistration::issue_small`, `HeapLease::core`, `SmallSidecar::scan` и прочие safe-функции с сырыми указателями или глобальными мутациями под `internals` механически не проверяются. Их soundness держится только на ревью.

**Рекомендация.** Сделать `RouteDirectory::register` `pub(crate)` с отдельным `internals`-форвардером, проходящим инвентарь, либо явно внести `segment_route` в `REVIEWED_SURFACE`. Расширить сканер: любая `pub fn` под `internals` с параметром `*mut`/`*const` должна попадать в инвентарь независимо от имени.

### R6-SEC-11 — Low — `exact-object-proto`: обход в `dealloc_batch` и спинлок без сдачи при выходе

**Достижимо:** `exact-object-proto`; первая часть — вместе с `batch-api`. Прототипы вне `production`.

- `GlobalAlloc::dealloc` сначала ищет адрес в ExactTable (`global_alloc.rs:64-69`), а `SeferAlloc::dealloc_batch` (`batch.rs:115-136`) — нет. Narrow-объект (≤16 Б, выданный `System`), освобождённый через `dealloc_batch`, уходит в `HeapCore::dealloc_batch`/`publish_foreign`, маршрута не находит и учитывается как `foreign_or_unroutable_frees`. Итог — тихая утечка блока и вечный дескриптор в таблице. Аналогично `alloc_batch` для narrow-`Layout` обслуживается heap'ом, а не ExactNarrow, то есть политика несимметрична.
- `ExactShard::acquire` (`exact_shard.rs:184-192`) крутится безусловно. В отличие от fallback-лока и шардов route directory, он не сдаётся при `process_exit_in_progress()`: на Windows поток, убитый `ExitProcess` под этим локом, вешает выход процесса. После `fork` из многопоточного процесса — то же самое.

**Рекомендация.** Направить `dealloc_batch` и `alloc_batch` через ту же развилку narrow, что и скалярный путь. Дать шард-локу ту же логику «give up at exit».

### R6-SEC-12 — Low — `realloc` блока, выданного System при выходе процесса, приводит к `abort`

**Достижимо:** `production`, Windows, окно выхода процесса.

При `process_exit_in_progress()` и недоступном fallback `alloc` отдаёт блок `System` (`global_alloc.rs:43-47`). README обещает: «A later free of such a block is a counted no-op» (`README.md:1698-1699`). Но `realloc` такого блока через `CurrentHeap::Own` или fallback попадает в foreign-ветку `HeapCore::realloc`: маршрута нет → `return core::ptr::null_mut()` (`realloc.rs:418-423`). Для `Vec`, растущего в TLS-деструкторе на выходе, это null → `handle_alloc_error` → `abort` вместо штатного завершения. Escape-ветка в `SeferAlloc::realloc` (`global_alloc.rs:151-175`) срабатывает только когда `with_fallback_heap` вернул `None`, а не когда heap вернул null.

**Рекомендация.** В `realloc` при null от foreign-ветки и `process_exit_in_progress()` выделять новый блок через `System` и копировать `min(old,new)` (старый блок допустимо оставить, процесс завершается). Отразить это в README.

### R6-SEC-13 — Info — аддитивные фичи меняют глобальный аллокатор для всего бинарника

По Cargo feature unification любая зависимость может включить `exact-object-proto` (все запросы ≤16 Б идут в `System`, при несовпадении `Layout` — `abort`), `virgin-zero-skip`, `hardened` (новые пути `abort`) или `bench-internals` (дополнительные счётчики на горячих путях и `unsafe`-хуки). Владелец бинарника получает их молча. Для mock-бэкендов проект это уже решил через `--cfg` (`Cargo.toml:699-723`), для перечисленных фич — нет. Отдельно: при `virgin-zero-skip` корректность `alloc_zeroed` становится условием soundness safe-кода. `vec![None::<&T>; n]` через специализацию `IsZero` полагается на `alloc_zeroed`, и ненулевая память даёт невалидные ссылки. Это общее свойство любого аллокатора, но здесь оно держится на более сложном инварианте virgin-бита. Дефекта не найдено.

**Рекомендация.** Описать в README и `SECURITY.md`, какие фичи меняют поведение глобального аллокатора. Для прототипов рассмотреть перевод на `--cfg`, как уже сделано для mock.

### R6-SEC-14 — Info — нет предохранителя от unwind в `GlobalAlloc`

В `unsafe impl GlobalAlloc for SeferAlloc` (`global_alloc.rs:33-185`) нет guard'а «abort on unwind». В release-сборке достижимых panic не найдено: `numa.rs:94` `.expect` недостижим по построению, `assert!` в `refill_class_bump_virgin_internal` — внутренний инвариант. Но в debug-сборке при порче free-list вычитание `next as usize - segment as usize` (`alloc_core_small_impl.rs:447,608,639`) паникует внутри `GlobalAlloc`, а это UB по документации std. Проект сам признаёт этот класс проблем (R2-08).

**Рекомендация.** Добавить локальный guard, который вызывает `abort` в `Drop`, если не был `forget` (в нормальном пути нулевая стоимость). Либо использовать `wrapping_sub` или `checked_sub` с `fatal` в этих трёх местах.

### R6-SEC-15 — Info — недокументированный публичный хвост крейт-корневого `AllocCore`

В `production` без `internals` у `sefer_alloc::AllocCore` доступны `segment_bases()` (`state.rs:184-206`; возвращает адреса сегментов, имя не `dbg_*`, поэтому сканер его не видит) и статические `dbg_foreign_or_unroutable_frees`, `dbg_segments_reserved_total`, `dbg_segments_released_total` (`totals.rs`), `dbg_decommit_count` (`alloc_core_small_pool_impl.rs:503-504`). Они безопасны (только чтение или адреса собственного standalone-ядра), но входят в semver-поверхность и не значатся в списке стабильных типов в `Cargo.toml`/README.

**Рекомендация.** Перевести их в `pub(crate)` с форвардерами для `stats()` либо явно задокументировать как стабильные.

### R6-SEC-16 — Info — рассогласование типа FFI в `process_exit.rs`

`GetProcAddress` объявлен как возвращающий `Option<unsafe extern "system" fn() -> isize>` (`process_exit.rs:11-15`), а результат вызывается через `transmute` в `fn() -> u8` (`:66-71`). На x86-64/AArch64 Windows это безвредно: BOOLEAN читается из младшего байта. Для FFI-seam'а лучше объявить точный тип (`*const c_void` или `fn() -> u8`) и обойтись без двойного преобразования через `usize`.

## Проверено, новых находок нет

- **R5-MS-1..4 закрыты.** `AllocCore::dealloc`/`realloc`, `HeapCore::realloc` и `flush_class` — `unsafe fn`; `flush_class` дополнительно под `internals`. `dbg_push_to_ring`/`dbg_drain_all_rings` удалены. `BinTable::head/set_head` в release проверяют границы класса (`descriptors.rs:185-204`).
- **Граница `internals`.** Модули `alloc_core`/`global`/`registry` без `internals` имеют видимость `pub(crate)` (`src/lib.rs:443-493`). `dbg_*` на re-export'нутом `AllocCore` закрыты `#[cfg(feature = "internals")]` на `impl`. Обхода не найдено, кроме хвоста из R6-SEC-15.
- **Конфигурация.** Переменные окружения не читаются, все значения задаются через `const`-конфиг. `decay_rate` ограничен диапазоном [1,100] %. `pool_cap` насыщается через `u32::try_from`. Decay-арифметика использует `saturating_mul`, catch-up ограничен 8 шагами. Входа, вызывающего panic в пути `GlobalAlloc`, не найдено.
- **Арифметика Large.** `checked_add` для `hdr + size`. Сверхсегментное выравнивание идёт через `reserve_biased` с проверками `checked_*`. Interior-free Large отклоняется во всех сборках (`reject_large_interior_free`, `alloc_core_large.rs:127-135`).
- **Foreign free.** Поиск в route directory только числовой, под шард-локом. Публикация — одна RMW в независимый sidecar, байты пользователя не трогаются. Неизвестный адрес — учитываемый no-op.
- **TLS и teardown.** Sentinel `TORN` выставляется до `LIVE → FREE`. Пассивные резолверы `current_for_dealloc`/`current_for_trim` не привязывают слот. `trim_current_thread` без привязанного heap — no-op.
- **Registry.** OOM при materialisation чанка обрабатывается штатно. Индексы, выделенные через `bump_count`, но не materialised, восстанавливаются сканированием (`stack.rs:12-35`), поэтому `dbg_bump_count_without_materialising` вечного исчерпания не вызывает.
- **Supply chain рантайма `production`.** In-repo крейты (`aligned-vmem`, `once-ptr-cell`, `size-classes`, `sefer-region`) плюс `slotmap` через `sefer-region`. Сторонних `unsafe`-крейтов на пути аллокатора нет; `aligned-vmem` объявляет FFI сам, без `libc`/`windows-sys` (сами объявления не ревьюились).
- **Инвентарь `unsafe`.** `grep -rnE '^\s*#!\[allow\(unsafe_code\)\]' src/` даёт 22 файла. Это совпадает с перечнем в `src/lib.rs` и с таблицей README «Where unsafe lives».
- **Fork и выход процесса.** Ограничения задокументированы честно (README «Fork safety», «Process exit on Windows»); новых путей сверх R6-SEC-11/12 не найдено.
- **Утечка информации.** Сообщения `abort`/`fatal` статические, адресов не печатают. `AllocStats` содержит только агрегированные счётчики.
