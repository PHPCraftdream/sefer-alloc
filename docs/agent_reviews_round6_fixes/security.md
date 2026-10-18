# Ревью исправлений раунда 6 — безопасность (soundness и устойчивость к злоупотреблениям)

- Ревизия: `main` @ `ac23d06ebd1d4dd033df11a099f97b5fe8ed4ff7`. Номера строк даны по ней.
- Диапазон: `132ab85b..HEAD` (merge-коммиты PR #37–#63), в основном `git diff 132ab85b HEAD -- src`.
- Режим: только статическое чтение. Ничего не собиралось и не запускалось, ни одна находка не воспроизведена.
- Вне ревью, потому что не в `main`: R6-T16 (PR #64), R6-T41/43/44/45, R6-T55–T57, R6-T02.

## 1. Резюме

Ни одна правка раунда 6 не даёт UB из safe-кода в `production` без `internals`/`bench-internals`. Хорошо сделаны:
- R6-T06 — проверка ссылок free-list по диапазону и выравниванию с `fatal`;
- R6-T22 — граница смещения до `add`;
- R6-T28 — класс хранится только в стартовой грануле;
- R6-T33, R6-T30, R6-T29, R6-T04, R6-T09, R6-T23, R6-T25, R6-T14.

Главный остаточный риск в default-сборке — own-thread free с неверным `Layout`, и он шире, чем написано в README.
- Арифметическая проверка R6-T05 (`off % block_size(class из Layout) == 0`) не мешает выдать перекрывающиеся живые блоки.
- Для самого массового класса (16 B) проверка сводится к выравниванию по 16 и перестаёт быть проверкой.
- Пакетный путь `dealloc_batch` не делает и её.

Это сознательное решение владельца (`docs/perf/R6_T05_T06_T33_FREE_VALIDATION_GATE.md`, item 247). Но формулировки README и описание item 247 не называют последствие (перекрытие выдач) и неполны по охвату.

Остальные находки — P3: сужение гарантий в узких режимах (гонка, выход процесса, `internals`), гигиена `unsafe` и доступность.

| ID | P | Кратко | Статус |
|---|---|---|---|
| SECF-1 | P2 | Default own-thread free с неверным `Layout` даёт перекрывающиеся выдачи; для класса 16 B проверка пуста; README занижает последствие | подтверждено чтением |
| SECF-2 | P3 | `dealloc_batch_small` обходит обе проверки R6-T05 (default и `hardened`); README утверждает обратное для default | подтверждено чтением |
| SECF-3 | P3 | Двойное чтение `next` в `try_drain_freelist_batch`: проход фиксации не перепроверяет ссылку, а голова без `hardened` не валидируется | код подтверждён, эксплуатация — гипотеза |
| SECF-4 | P3 | R6-T50 добавил неатомарное чтение заголовка чужого сегмента; обоснование хука `RouteDirectory::register` («указатели не разыменовываются») стало ложным | подтверждено чтением |
| SECF-5 | P3 | `exit_escape_realloc` на выходе копирует `old_layout.size()` в обход отказов foreign-ветки (маршрут, ёмкость, фронтир R6-T50) | подтверждено чтением |
| SECF-6 | P3 | `fatal()` пишет через `std::io::stderr()` (reentrant-лок), теперь в том числе под shard-локом и на выходе: abort может стать зависанием | гипотеза |
| SECF-7 | P3 | `route_to_large_free` — safe `fn` с `unsafe`-предусловием | подтверждено чтением |
| SECF-8 | P3 | `start_maintenance()` (safe API) прерывает процесс через 30 с без подтверждения: при SIGSTOP, отладчике, паузе VM, CPU-throttling | подтверждено чтением, сценарии — гипотеза |
| SECF-9 | P3 | Аудит `Send` в R6-T52 исключает `core: AllocCore`, т.е. почти всё содержимое `HeapCore` | подтверждено чтением |
| SECF-10 | P3/Info | Пустой ключ `own_cache` (`usize::MAX`) не отделён от невыровненного ключа: `canonical_base_of` может вернуть `Some(null)`; production-вызовы маскируют ключ | подтверждено чтением |

## 2. Находки

### SECF-1 (P2) — default own-thread free с неверным `Layout` даёт перекрывающиеся выдачи

- **Где:**
  - `src/registry/heap_core/free/dealloc_own_base.rs:405-421` — после `Accept` без `hardened` проверяется только `SizeClasses::divides(c, off)`, где `c` берётся из `Layout` вызывающего;
  - `src/registry/heap_core/free/dealloc_own_base.rs:280-315` — `small_free_guard`: отказ для `Unknown`, `off >= payload_start`, `!is_in_magazine(off)`, `off < bump`, `!is_free(off)`;
  - `README.md:1680-1692`.
- **Механизм (подтверждён чтением):**
  - Блоки класса лежат по смещениям, кратным `block_size` от базы сегмента. На этом инварианте стоит H1 под `hardened` (`dealloc_own_base.rs:283-289`).
  - `alloc_bitmap` и `magazine_bitmap` ставят бит только в стартовой грануле блока. Поэтому для внутренней гранулы живого блока `is_free == false` и `is_in_magazine == false`, и все M2-оракулы проходят.
  - Default-проверка сравнивает смещение с шагом класса из `Layout`, а не с реальным классом гранулы.
- **Сценарий 1 (внутренний указатель, самый массовый класс):**
  1. `p = alloc(Layout(4096, 8))`.
  2. Тот же поток: `dealloc(p + 16, Layout::new::<u64>())`. Класс 0 (16 B), `off + 16` делится на 16, проверка проходит.
  3. `p + 16` попадает в магазин класса 16 B.
  4. Следующий `alloc(Layout::new::<u64>())` возвращает `p + 16` внутри живого 4-KiB блока.
  - Для класса 16 B `divides` эквивалентен `off % 16 == 0`, а это выполняется для любой выровненной гранулы любого блока. Значит, проверка R6-T05 ничего не отсекает для `Box<u64>`, мелких `String`/`Vec` и т.п.
- **Сценарий 2 (больший класс):**
  1. Блок класса 64 B по смещению, кратному 128.
  2. `dealloc(p, Layout(128, 8))` проходит `divides(128)`.
  3. Следующий `alloc(128)` выдаёт `[p, p+128)`, а это перекрывает соседний живой 64-байтовый блок `p + 64`.
- **Последствие:** две живые выдачи над одними байтами. Дальше — порча пользовательских данных. Free-list затем строится из перекрывающихся блоков, и `next` пишется в живые объекты. Нужен нарушенный контракт `GlobalAlloc::dealloc` — это `unsafe`, по `SECURITY.md` нарушение контракта, а не уязвимость. Но это ровно та эскалация «ошибка в чужом `unsafe` → контролируемое перекрытие», от которой защищают hardened-аллокаторы.
- **Почему P2, а не P3:**
  - README (`README.md:1689-1692`) пишет «a coincident-divisible wrong-class free is rejected only by `hardened`». Читатель не узнает ни о перекрытии выдач, ни о том, что для класса 16 B default-проверки фактически нет.
  - Соседний пункт (`README.md:1680`) про cross-thread interior free создаёт впечатление, что внутренние указатели отбрасываются вообще. Для own-thread это не так.
  - Карточка item 247 (`docs/correctness-open-items/TRACKED_correctness_residuals.md:177-220`) называет остаток «aligned wrong-class pointers can pass» и тоже не называет последствие.
- **Уверенность:** высокая по коду. Сценарий не запускался. Тест `tests/r6_t33_t05_own_free_kind_and_class.rs` проверяет недели́мое смещение, а не этот случай.
- **Исправление:**
  1. Документация (без кода): в README «Honest limitations» и в item 247 явно написать, что в default own-thread free внутреннего указателя или с неверным `Layout` может привести к повторной выдаче живой памяти; для класса 16 B проверка сводится к выравниванию; `hardened` закрывает это (H1 + class map). Добавить тест-свидетель для default-сборки (сценарий 1, ожидание — перекрытие), чтобы остаток был зафиксирован, а не только описан.
  2. Если владелец захочет закрыть дешевле, чем 33 Ir: признак «начало выданного блока» в битовой карте, которую этот путь уже читает (`magazine_bitmap`/`alloc_bitmap`, та же строка кэша). Ставится на issue, снимается на free. Цена — запись бита на каждом issue. Нужен iai-гейт на `cold_alloc_only_256x16b`, `recycle_alloc_only_256x16b`, `dealloc_free_only_16b`. Новая проверка — только с согласия владельца, по правилу «без флагов».

### SECF-2 (P3) — `dealloc_batch_small` обходит проверки R6-T05

- **Где:**
  - `src/registry/heap_core/free/dealloc_batch.rs:311-341`: `Accept { off }` сразу ведёт к `mark_magazine` и записи в слот;
  - сравни `dealloc_own_base.rs:405-421`.
- **Механизм:** в пакетном пути нет ни default-проверки `SizeClasses::divides`, ни hardened-проверки `small_class_matches`. H1 под `hardened` (модуль по шагу) выполняется: он внутри `small_free_guard`.
- **Сценарий:**
  - default + `batch-api`: `SeferAlloc::dealloc_batch(Layout(8,8), &[p + 8])` для живого блока по `p`. Указатель, не делящийся на 16, проходит в магазин класса 16, и следующий `alloc(8)` выдаёт невыровненный по классу адрес внутри живого блока. В скалярном пути R6-T05 такое отвергается.
  - `hardened` + `batch-api`: обходится точная проверка класса, т.е. сценарий 2 из SECF-1 снова открыт.
- **Последствие:** то же, что в SECF-1, через пакетный API.
- **Что задокументировано:** item 247 (b) перечисляет `dealloc_batch_small` как непокрытый только для hardened class map. README (`README.md:1689-1690`) утверждает «The default path rejects offsets not divisible by the supplied class stride», а для пакетного пути это неверно.
- **Уверенность:** высокая.
- **Исправление:** вынести обе проверки из `dealloc_own_thread_with_base` в общий помощник (или в `small_free_guard` после M2-оракулов) и вызвать его в `dealloc_batch_small`. Цена — тот же iai-допуск, что в PR #57. Плюс тест `dealloc_batch` с неделимым смещением: no-op. Минимум — поправить README и item 247.

### SECF-3 (P3) — двойное чтение `next` в `try_drain_freelist_batch` (без `hardened`)

- **Где:** `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:567-641`.
  - Проход-проба (`:583`, `:605-611`) валидирует каждую ссылку через `Node::continuation_offset_or_fatal`.
  - Проход фиксации (`:628`, `:636-640`) читает `next` из тех же блоков заново и вычисляет голову без проверки: `next.addr().wrapping_sub(segment.addr()) as u32`.
  - Голова списка без `hardened` нигде не валидируется: `pop_free` (`:423-438`) и проба (`:563`) доверяют `bt.head`.
- **Сценарий (гипотеза):**
  1. Владелец доливает магазин; другой поток с UAF-записью меняет первое слово уже проверенного свободного блока между двумя проходами. Окно — наносекунды, но без синхронизации оно не нулевое.
  2. Голова получает непроверенное смещение: внутрь метаданных, за `bump`, усечённое `as u32` или невыровненное.
  3. Следующий `pop_free` разыменовывает его через `Node::deref` (`add` без границы) и выдаёт пользователю адрес метаданных сегмента или чужого блока.
  - Под `hardened` следующая операция ловит это: `valid_free_node(head)` в `pop_free` и в начале пробы.
- **Последствие:** R6-T06 обещает «каждая ссылка проверена, порча — `fatal`». При гонке это не выполняется. Результат — выдача метаданных пользователю, то есть полная порча аллокатора, вместо `fatal`. Нужен UAF плюс гонка. Это нарушенный контракт, но именно от него защищает R6-T06.
- **Уверенность:** код — высокая; практическая эксплуатация — низкая.
- **Исправление (без лишнего Ir):** на пробе записывать указатели блоков прямо в `out[probed]`, а на фиксации брать их из `out` и не перечитывать `next`. Итоговую голову задать валидированным `probe`, ведь `k == probed`. Это заодно убирает второе зависимое чтение.

### SECF-4 (P3) — R6-T50 читает заголовок чужого сегмента; обоснование `RouteDirectory::register` устарело

- **Где:**
  - `src/registry/heap_core/free/realloc.rs:444-499` — foreign-ветка: `SegmentHeader::span_usable_at(route.root())`;
  - `src/alloc_core/segment/segment_header/segment_header_views.rs:108-111` — неатомарный `Node::read_usize`;
  - `src/registry/segment_route/pin.rs:20-25` — новый `RoutePin::root`;
  - `src/registry/segment_route/directory.rs:816-829` — safe `pub fn register` под `internals`;
  - `scripts/verify-dbg-hook-safety.mjs:424` — обоснование «pointer args are numeric segment keys never dereferenced».
- **Механизм (подтверждён):**
  - До R6-T50 foreign-путь, по собственному правилу кода, не трогал заголовки чужих резерваций (`mem_impl.rs:427-428`: «Foreign frees resolve route descriptors and sidecars, not reservation headers»). Теперь foreign `realloc` читает `span_usable` по `root` из дескриптора. Чтение неатомарное, а у владельца `span_usable` пишет `try_grow_large_reserved_capacity`.
  - (а) Злоупотребление со stale-указателем: поток B делает `realloc` уже освобождённого им Large-блока. Сегмент тем временем переиспользован владельцем A из large-cache, и A растит новый блок на месте (`large-reserved-capacity`). Получается гонка неатомарного чтения с неатомарной записью — data race UB в модели Rust. Нужен UAF; копирование и без того читало бы устаревшую память, так что приращение риска мало.
  - (б) `internals`: safe-код вызывает `RouteDirectory::global().register(fake_root, len, route_ptr, owner, RouteKind::Large)` и `mem::forget` для регистрации. Ключ остаётся связан с произвольным `fake_root`. Любой последующий foreign `realloc` указателя из сегмента этого ключа читает `*(fake_root + offset_of!(span_usable))`. Указатели, легально выданные sefer, в этот ключ не попадут: регистрация того же ключа даст `Duplicate`, а это DoS из R6-SEC-10. Остаются System-блоки escape-пути на выходе, так что сценарий надуманный. Но обоснование в инвентаре хуков стало фактически неверным.
- **Последствие:** (а) теоретический data race при уже нарушенном контракте; (б) safe API под `internals` получает путь к чтению по произвольному адресу; сканер этого не видит.
- **Уверенность:** механизм — высокая; практическая значимость — низкая.
- **Исправление:**
  - Хранить верхнюю границу закоммиченного span в дескрипторе маршрута (`AtomicUsize` рядом с `end`; владелец обновляет её при `try_grow_large_reserved_capacity`). Так и предлагал R6-T50 в `SUMMARY.md`: «хранить в дескрипторе верхнюю границу закоммиченного span». Тогда foreign-путь снова не читает заголовок.
  - Либо читать `span_usable` атомарно (`AtomicUsize` view, как `magic`).
  - В любом случае исправить обоснование в `verify-dbg-hook-safety.mjs:424` и комментарий в `mem_impl.rs:427-428`, или сделать `register` `unsafe fn` с `# Safety` «`root` — живая резервация».

### SECF-5 (P3) — `exit_escape_realloc` обходит foreign-проверки на выходе процесса

- **Где:** `src/global/sefer_alloc/global_alloc.rs:40-58` и `:202-206`.
- **Механизм:**
  - Любой null из `HeapCore::realloc` при `process_exit_in_progress()` теперь ведёт к `System.alloc` и `copy_nonoverlapping(ptr, new, min(old, new))`.
  - Foreign-ветка `HeapCore::realloc` возвращает null намеренно, как defence-in-depth (`realloc.rs:436-499`): нет маршрута, `contains_payload` отверг ёмкость, фронтир R6-T50 отверг чтение за коммитом. На выходе все три отказа превращаются в копирование ровно того диапазона, от чтения которого они защищали.
  - Это по замыслу нужно для System-блоков, выданных escape-путём, у которых маршрута нет. Но код не отличает «нет маршрута» от «маршрут отверг ёмкость или фронтир».
- **Сценарий:** Windows, выход процесса. `realloc(p, Layout(forged_size), n)`, где `forged_size` выходит за закоммиченный хвост (`large-reserved-capacity`). Без выхода R6-T50 даёт null. На выходе следует копирование из незакоммиченной VA, т.е. access violation во время `ExitProcess`.
- **Последствие:** на выходе гарантия R6-T50 не действует. Нужен нарушенный контракт.
- **Уверенность:** высокая по коду; узкий режим.
- **Исправление:** вызывать escape только когда `RouteDirectory::lookup(ptr)` вернул `None` или lookup был отклонён из-за выхода. Для этого `HeapCore::realloc` должен различать причину null: внутренний enum или отдельный холодный helper. Отказы ёмкости и фронтира оставить null.

### SECF-6 (P3, гипотеза) — `fatal()` может зависнуть вместо abort

- **Где:**
  - `src/alloc_core/platform/fatal.rs:4-8`: `std::io::stderr().write_all(msg)`, затем `abort()`;
  - R6-T15 заменил около 108 голых `process::abort()` на `fatal(...)`, в том числе внутри shard-лока: `src/registry/segment_route/directory.rs:271-460` (`Block`/`BlockIndex`/`Shard::remove`, «All access is under the shard lock»).
- **Механизм:** запись в `Stderr` берёт `ReentrantLock` std. Удалённый `exact_fatal.rs` прямо оговаривал «Never called under a shard lock»; общий `fatal` такой оговорки не имеет.
  - (1) Поток B держит лок stderr (`eprintln!`), а его `Display` аллоцирует или освобождает. Free идёт в shard-лок, который держит поток A. A обнаруживает порчу и вызывает `fatal`, который ждёт stderr. Взаимная блокировка: процесс висит вместо abort.
  - (2) Windows, выход: поток, держащий лок stderr, убит `ExitProcess`. Любой `fatal` на выходе висит навсегда.
- **Последствие:** при обнаруженной порче вместо немедленного abort возможен бесконечный hang. Детерминированность fail-stop теряется.
- **Уверенность:** низкая–средняя (зависит от конкретного чередования; не воспроизводилось).
- **Исправление:** писать сообщение в обход лока std: `libc::write(2, …)` на Unix, `WriteFile(GetStdHandle(STD_ERROR_HANDLE))` на Windows. Либо писать только при успешной неблокирующей попытке. `abort()` в любом случае безусловен.

### SECF-7 (P3) — safe `fn route_to_large_free` с `unsafe`-предусловием

- **Где:** `src/registry/heap_core/free/dealloc_own_base.rs:340-360`.
- **Механизм:** приватная safe-функция принимает произвольные `block: *mut u8` и `root: *mut u8` и внутри `#[allow(unsafe_code)] unsafe { core.core.dealloc_at_base(block, layout, root) }`. Предусловие (`root` — канонический живой корень таблицы для `block`) обеспечивает только вызывающий. Сейчас вызывающий единственный и корректный, через свидетель `OwnedBlock`. Но сигнатура позволяет будущему вызову из safe-кода модуля вызвать UB без `unsafe`. Это нарушает собственное правило проекта: «unsafe-предусловие ⇒ `unsafe fn` + `# Safety`».
- **Уверенность:** высокая; эксплуатации нет.
- **Исправление:** `unsafe fn route_to_large_free` с `# Safety`, на месте вызова — `// SAFETY:`. Либо принимать `OwnedBlock` вместо двух сырых указателей.

### SECF-8 (P3) — `start_maintenance()` прерывает процесс по таймауту 30 с

- **Где:** `src/global/maintenance_service.rs:43` (`STARTUP_CONFIRM_BOUND = 30 s`), `:96-117`, `:127-145` — `fatal` при `timed_out`.
- **Механизм:**
  - Safe публичный API (`SeferAlloc::start_maintenance`) завершает весь процесс, если воркер не дошёл до `RUNNING` за 30 с по монотонным часам. Документация (`README.md:1671-1676`, `sefer_alloc/maintenance.rs`) объясняет это loader lock-ом.
  - Тот же исход дают и другие, вполне легальные состояния:
    - SIGSTOP или остановка в отладчике между `spawn` и первым квантом воркера: после SIGCONT ожидание видит истёкший таймаут, и если воркер ещё не опубликовал `RUNNING`, следует `fatal`;
    - пауза VM;
    - жёсткий CPU-throttling cgroup;
    - `SCHED_FIFO`-голодание.
- **Последствие:** DoS, достижимый из safe-кода любой зависимости, которая вызывает `start_maintenance()`. Abort, а не ошибка.
- **Уверенность:** механизм — высокая; частота — низкая.
- **Исправление:** без смены API — увеличить границу или ждать без таймаута, но с проверкой loader lock там, где её можно сделать. С согласия владельца (semver) — `Result`/`MaintenanceStartError`, как предлагал R6-T35. Как минимум — перечислить эти триггеры в README.

### SECF-9 (P3) — аудит `Send` (R6-T52) не покрывает `AllocCore`

- **Где:** `src/registry/heap_slot.rs:303-333` (`core: _`, `tcache: _`).
- **Механизм:** compile-time деструктуризация проверяет `Send` только для `id`, курсоров и `tcache_hits`. `core: AllocCore` исключён с обоснованием «raw pointers into this core's own reservations». Но там же лежат таблица сегментов, large-cache с extension-sidecar, кэш NUMA-узла, `RouteSlots` с `RouteRegistration` (`PhantomData<Cell<()>>` — `!Sync`; это нормально, но не проверено). Появление в `AllocCore` любого потокоаффинного поля (`Rc`, `Cell`-хэндл TLS) аудит не поймает. Ровно этот риск и закрывал R6-T52 (`memory_safety_review.md` R6-MS-5).
- **Уверенность:** высокая.
- **Исправление:** такой же exhaustive-деструктор для `AllocCore` (и `Tcache`) с явным списком исключённых сырых указателей. Либо обёртка `SlotCore(HeapCore)` с одним `unsafe impl Send` и полным SAFETY, как предлагал `SUMMARY.md`.

### SECF-10 (P3/Info) — пустой ключ `own_cache` не отделён от невыровненного ключа

- **Где:**
  - `src/alloc_core/segment/segment_table/segment_table_impl.rs:156-160` (`OWN_CACHE_EMPTY_KEY = usize::MAX`), `:746-760`, `:804-811`;
  - `src/registry/heap_core/diag/diag_probes.rs:125-126` (`dbg_contains_base` передаёт ключ без маскирования).
- **Механизм:**
  - После R6-T11 попадание — одно сравнение ключа, без `!is_null()`. Это корректно, только если ключ выровнен по `SEGMENT`; инвариант держат вызывающие, но не функция.
  - `canonical_base_of(usize::MAX as *mut u8)` даёт `cache_index = 15`. Пустой слот совпадает, и функция возвращает `Some(null)`, то есть «живой корень», равный null.
  - Все production-вызовы маскируют ключ через `os::segment_base_of_ptr`. Немаскированный ключ передают только `HeapCore::dbg_contains_base` (ложный `true`) и тестовая обвязка `harness.rs:201-208`.
- **Последствие:** сейчас — только ложный ответ тестового хука. Это латентная ловушка: будущий вызов с немаскированным ключом получит null-корень и разыменует его.
- **Исправление:** `debug_assert!(key.addr() & (SEGMENT - 1) == 0)` в обоих методах (cfg-гейт, в release без цены) и маскирование в `dbg_contains_base`.

### Информационные заметки (без приоритета)

- **R6-T13** (`hot.rs:275`, `:422`; `dealloc_own_base.rs:447`): маска `& (TCACHE_CAP - 1)` превращает возможный выход за границу (раньше — паника-bounds-check, т.е. UB-unwind из `GlobalAlloc`) в молчаливый перенос индекса. При порче `count` следствием была бы повторная выдача слота 0, а не abort. При текущих инвариантах это тождество: `FREE_PARK_CAP ≤ TCACHE_CAP` по конструкции (`tcache.rs:123-143`), `TCACHE_CAP` — степень двойки (`tcache.rs:78-83`). Регрессией не считаю.
- **R6-T28, ресурсы:** класс теперь хранится только в «mixed»-листах. Любой лист, где выдаётся хоть один блок, требует `System`-аллокации на 256 B: до 1024 листов × 256 B ≈ 256 KiB на полностью нарезанный 4-MiB Small-сегмент, против почти нуля для uniform-листов раньше. Гейт (`docs/perf/R6_T28_START_GRANULE_CLASS_LEAVES_GATE.md:54-56`) это признаёт и память не меряет. Не уязвимость, но при `System`-OOM растёт число отказов Small-аллокаций.
- **R6-T28, расхождение в документах:** item 247 (b) говорит, что «stale class bytes after an in-route segment reset remain». `sidecar_bitmap.rs:93-98` утверждает, что повторная регистрация даёт обнулённый sidecar. В production сброса внутри маршрута нет: retain-ветка `decommit_empty_segment_impl(.., false)` вызывается только из `bench-internals`-хука `dbg_force_decommit_retain_for` (`alloc_core_small_pool/decommit.rs:106-121`). Только после этого хука устаревшие стартовые коды позволят cross-thread interior free пройти фильтр producer-а. Это тестовая поверхность, но текст item 247 стоит уточнить.

## 3. Что проверено и признано корректным

- **R6-T06** (`node.rs:72-108`, `config/continuation_bounds.rs`, `alloc_core_small_impl.rs:438-448`, `:605-611`):
  - диапазон `[meta_end, min(bump, SEGMENT))` и выравнивание `MIN_BLOCK` проверяются одним `wrapping_sub` + сравнением;
  - `next < base` даёт огромное смещение и отвергается;
  - прежнее усечение `as u32` для смещений ≥ 4 GiB закрыто;
  - паника вычитания в debug устранена;
  - `hi < lo` даёт `fatal`.
  - Корректно для `pop_free` и прохода-пробы; оговорка — SECF-3.
- **`SizeClasses::divides`** (`size_classes.rs:210-233`, `:319-336`): обращение нечётной части методом Ньютона (6 итераций от `x = 1`: 1→2→4→…→64 верных бита) и тест `rotr(off·inv, k) ≤ ⌊(2⁶⁴−1)/d⌋` — стандартный точный критерий делимости. Таблица вычисляется в `const`.
- **R6-T33:** `BlockKind::Unknown` отвергается до M2-оракулов (`dealloc_own_base.rs:270-279`), симметрично substrate (`mem_impl.rs:572-585`).
- **R6-T05 под `hardened`:** `small_class_matches` (`route_slots.rs:185-210`, `small_sidecar.rs:105-117`, `:113`) сравнивает `encoded == class + 1` в стартовой грануле. Fail-open срабатывает только на инфраструктурных состояниях, которые путь не строил. H1 (модуль) сохранён.
- **R6-T22:** `canonical_block_of` (`mem_impl.rs:41-57`) и `realloc_inplace_fast_path_known_base` (`realloc_fastpath.rs:235-249`) ограничивают смещение `span_usable` (Large) или `SEGMENT` (Small) до `Node::deref`; doc у `Node::deref` (`node.rs:181-194`) фиксирует обязанность вызывающего.
- **R6-T28:**
  - producer отбрасывает гранулы с кодом 0, т.е. внутренние и невыданные (`sidecar_bitmap.rs:90-115`);
  - `issue` без предварительного `prepare` возвращает `false`, а фиксация делает `fatal` (`route_slots.rs:141-157`);
  - `ClassLeaves::initialize_at` инициализирует единственное поле;
  - `SmallSidecar` из необнулённой `System.alloc` полностью инициализируется в `initialize_at` (`small_sidecar.rs:52-65`; `#[repr(C)]` из одних атомиков, без padding);
  - владелец в `reclaim_sidecar_record` (`alloc_core_small_reclaim.rs:23-70`) сохраняет геометрические abort-проверки;
  - в production нет пути, меняющего геометрию сегмента внутри живого маршрута (`set_bump` только в carve и decommit-перед-release).
- **R6-T30:** `checked_mul` на SEGMENT-округлении и на страничном округлении `exact-span-large` (`alloc_core_large.rs:225-285`); `Layout::from_size_align` до promotion (`realloc.rs:303-309`).
- **R6-T29:** OOM в move-ветке сжатия Large записывает `large_size = new_eff` только при `ptr == base + payload_offset` (`realloc.rs:345-366`). Исходный блок не освобождён, контракт `GlobalAlloc` соблюдён.
- **R6-T04:** escape-помощники на холодном null-пути (`global_alloc.rs:21-58`). Старый блок никогда не передаётся в `System.dealloc`/`realloc`; копируется `min(old, new)`; повторная валидация `Layout`; `new_size == 0` даёт null. Оговорка — SECF-5.
- **R6-T09:** narrow-развилка в `alloc_batch`/`dealloc_batch` (`batch.rs:68-84`, `:131-155`) зеркалит скалярный путь; незарегистрированный объект уходит в heap-routing и утекает как unroutable, как и раньше.
- **R6-T23:** `ExactShard::acquire_unless_exiting` (`exact_shard.rs:204-224`) отказывает только на выходе; `try_dealloc` при отказе возвращает «не наш», и блок безопасно утекает. Освобождение массива `rehash` вынесено за лок (`:258-263`). На выходе `ExactNarrow::alloc` при отказе таблицы отдаёт незарегистрированный System-блок; последующий free — учтённая утечка, не abort.
- **R6-T25:** `Shard::remove` возвращает `Retired`, который отбрасывается после `drop(guard)` (`directory.rs:632-672`, `:973-988`). Все ветки возвращают собранные блоки.
- **R6-T26 (TTAS):** захват — только успешный CAS с `Acquire`, спин — `load(Relaxed)` без публикации; освобождение — `Release` (`shard_lock.rs:63-125`, `fallback.rs` lock loop). Порядок памяти не ослаблен.
- **R6-T14/R6-T15:** перечисленные паники в путях `GlobalAlloc` заменены на `fatal` (`alloc_core_large_cache.rs`, `terminal_words.rs:48-53`, `alloc_core_small_magazine.rs:175-179`); индексация чанков реестра идёт через `get` + `fatal` (`registry.rs:199-275`); удалён `debug_assert!(false)` в `recycle`; есть lexical-guard `scripts/verify-no-bare-abort.mjs`. Оговорка — SECF-6.
- **R6-T37:** `tls_heap.rs` больше не tier-1-модуль: единственный `unsafe` — item-level `#[allow(unsafe_code)]` на `bench-internals`-хуке.
- **R6-T38:** `segment_bases` без `internals` — `pub(crate)` (`state.rs`); `dbg_`-алиасы счётчиков — под `internals` (`totals.rs`); `RouteDirectory::register_route` — `pub(crate)` с инвентаризованным форвардером. Оговорка про обоснование — SECF-4.
- **Новые хуки этого раунда:**
  - `AllocCore`/`HeapCore::dbg_stamp_kind_byte` — `pub unsafe fn` с `# Safety`, разыменование только после проверки членства в таблице (`header_diag.rs:157-164`);
  - `dbg_kind_byte_of` и `dbg_segment_registered_for` — только чтение после `canonical_base_of`/`canonical_root_for`;
  - `dbg_set_inject_*`, `dbg_reuse_hint`, `dbg_saturation_retry_decision`, `dbg_size_class_*` не принимают указателей.
  - Новых safe `pub fn`, которые выводят базу из сырого указателя и пишут метаданные, не найдено.
- **R6-T11:** пустой ключ `usize::MAX` не равен ни одному `SEGMENT`-выровненному ключу; `own_cache_clear` сравнивает корни, а не ключи. Оговорка — SECF-10.
- **R6-T13:** `const_assert` степени двойки есть; `FREE_PARK_CAP ≤ TCACHE_CAP` выполняется по конструкции.

## 4. Ограничения

- Только статическое чтение по `ac23d06e`. Ничего не собиралось и не запускалось (ни cargo, ни miri, ни loom). Ни одна находка не воспроизведена. Сценарии в SECF-1/2 выведены из кода и не проверены тестом.
- Из 92 изменённых файлов `src/` построчно прочитаны те, что касаются валидации free/realloc, free-list, sidecar, маршрутов, выхода процесса, `unsafe`-сайтов и новых хуков. Рефакторинги без смены поведения (R6-T18/T19/T20, перенос cfg-предикатов, правки комментариев R6-T17) просмотрены по диффу, но не перепроверены на эквивалентность во всех feature-комбинациях.
- Concurrency-аргументы (SECF-3, SECF-6, гонки R6-T50) не проверялись loom/TSan. Это гипотезы о чередованиях.
- Вне области: R6-T16 (PR #64), R6-T41/43/44/45, R6-T55–T57, R6-T02, а также крейты `crates/*`, кроме упомянутых мест.
