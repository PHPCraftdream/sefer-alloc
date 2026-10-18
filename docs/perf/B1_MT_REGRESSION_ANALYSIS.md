# B1: многопоточный регресс SeferAlloc и путь к опережению mimalloc

Дата: 2026-10-18. Режим: read-only исследование (без сборок/прогонов); все утверждения о коде проверены чтением на `main` = `dab9686f` (worktree `r6-b1-bisect1`), кроме случаев, где явно указан `git show <sha>:<path>`. Замеры — из задачи (GitHub CI windows-2022, `examples/malloc_macro`, larson/mstress, T=4; iai `perf_gate_iai` на Linux; criterion `global_alloc` на машине владельца); шум раннеров ±5–10% на ячейку.

## 0. Резюме

- MT-регресс относительно 0.2.1 создали пять коммитов 29.09–01.10 (960b35b9 → e8843374) и усугубил b50f04f4. Наибольшая ступенька — e8843374: он поставил на путь чужого free вторую разделяемую cache-линию (класс-лист) и удвоил учётную цепочку issue на пути владельца.
- На текущем main один межпоточный free стоит: 1 захват спин-блокировки шарда + ~2 бинарных поиска + 3 разделяемых RMW (`refs` +1, `pending` fetch_or, `refs` −1) + ~10 зависимых загрузок по 4–5 общим линиям (§3.1). В 0.2.1 это был 1 RMW и 0 блокировок (§3.3).
- Наихудший сценарий (машина владельца, фоновая нагрузка, 100×+) объясним конвоем на спин-блокировке шарда (`shard_lock.rs`, b50f04f4): ожидание = TTAS-кручение + `yield_now`, при перенасыщении CPU это деградирует нелинейно (§4, помечено как гипотеза).
- Однопоточный разрыв (burst 16B ×4.8 wall-clock; маргинально ≈385 Ir/пару против ≈102 у mimalloc) складывается из: per-block issue-цепочки (prepare/commit, e8843374), guard-цепочки и не-степенного деления на free, пер-блочных bitmap-операций журнала (mark_magazine/mark_alloc/publish_nonempty), дорогого attach и teardown (§5–6).
- Рекомендованный порядок (§7.4, таблица): R2 фьюксы/WaitOnAddress вместо кручения → R1 TLS-кэш пинов (≤1 RMW на foreign free) → R3 батчевый issue-touch → R6 дешёвый init/Drop sidecar → R4 штамп magazine-резиденции вместо второго bitmap → R5 убрать `divides()` → R7 безблокировочный каталог/эпохи (финал). Честные пределы: на cold burst паритет по Ir при текущих контрактах C6/OOM недостижим — только сокращение разрыва; на churn и MT паритет реалистичен, опережение — условно (§7.5).

## 1. Контекст намерения перестройки

- ADR `docs/design/2026-10-01-adr-physical-boundary-and-progress.md`: цель — корректность по Miri SB/TB ( dealloc не трогает тело, чужой free не пишет в память владельца, маршруты с incarnation), гарантии G1–G4, перф-гейты (в т.ч. larson/mstress geomean ≥ 0.95, каждая T ≥ 0.90 — ADR §Гейты).
- Аддендум path-b (`docs/design/2026-10-05-adr-addendum-ph3c-path-b.md`): off-body учёт для refill/reuse отменён (P1-box принят; интрузивный free-list в телах свободных блоков), но маршрутизация чужих free (sidecar-публикация) осталась — она решает другую задачу: producer не пишет в чужую reservation.
- Контракт `docs/REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md`: терминальная публикация = одна RMW-точка линеаризации; producer после неё не трогает reservation; owner-only ledger (`live_count`) держит credit за unpublished/published producer (§2–3). Именно ledger защищает от release сегмента при живом in-flight producer.
- Ревью round 7 (`docs/reviews/2026-09-29-213015-src-review-xs-sol-round-7.md`): P2-1 (remote frees после recycle без consumer'а) — мотив для каталога; P3-4 (sidecar живёт вечно в слоте) — мотив для incarnation/перестройки.

## 2. Разбор коммитов: что защищает и что добавил на горячие пути

### 2.1 `08d54ff4` «isolate terminal remote-free atomic words» (база волны)
Изолирует терминальные атомарные слова remote-free (deferred Large) в отдельные независимые от reservation слова. На путь MT тогда ещё не ставил каталог/публикацию. Бенч: 0.92/0.83.

### 2.2 `960b35b9` «add external sidecar bitmap remote-free primitive» (0.84/0.77)
- **Гарантия:** примитив `SidecarBitmap` — внешние (вне reservation) `pending`-биты + class-байты; публикация = один `fetch_or(AcqRel)`; никакой аллокации и доступа к телу (модульная док-строка `sidecar_bitmap.rs:1-8`). Это форма §3/§10 контракта (single-bank bitmap).
- **Горячие пути:** ничего не подключил — только примитив + тесты (`tests/loom_sidecar_bitmap.rs`, `tests/r6_sidecar_bitmap.rs`).
- Файлы сегодня: `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs:113-122` (`publish`: `pending[granule/64].fetch_or(1<<(granule%64), AcqRel)`).

### 2.3 `efec9d16` «add pinned dynamic sidecar route directory» (0.76/0.68)
- **Гарантия:** адресная диспетчеризация чужих free без разыменования пользовательского указателя: `RouteDirectory` (64 шарда, `directory.rs:18`), записи `Entry{key,end,payload,root,owner,kind,incarnation,refs,sidecar}` (`directory.rs:25-35`), pin-able sidecar, независимый от reservation lifetime (`routing.rs:32-34`: «The independent pin remains valid even if this free releases its root»). Закрывает P2-1 round-7 (нет consumer'а после recycle) и BLOCKER-P1/A1 контракта (producer не трогает reservation).
- **Горячие пути:** добавил сам путь `publish_foreign` → `lookup` → `publish_small/large` + `System`-аллокации Entry/SmallSidecar на регистрации. Large: `LargeState::publish_pending` — CAS LIVE(g)→PENDING(g) (`large_state.rs:24-30`).

### 2.4 `1d881429` «bind route lifetime to HeapCore-owned segments» (0.73/0.65)
- **Гарантия:** маршрут жив ровно столько, сколько жив сегмент владельца: `RouteSlots` привязан к `SegmentTable` — регистрация при резервировании сегмента, `remove` при recycle (`route_slots.rs:108-138, 246-255`; `segment_table_impl.rs:696-699`); монотонная incarnation (`directory.rs:869-874`, `fetch_update` на `next_incarnation`). Тесты `tests/r6_route_lifecycle_*` (basic/decommit/fallback/oom/pool/release).
- **Горячие пути:** регистрация/снятие маршрута на путях reserve/recycle; для MT — рост числа объектов под шард-блокировкой.

### 2.5 `be7a5ae2` «require unique ownership for terminal route publication» (0.65/0.61)
- **Гарантия:** ровно одна публикация на pin: `publish_small/large(self)` стали `unsafe` и потребляют pin по значению (`pin.rs:53-69`); компилируемый отрицательный тест `tests/route_publication_requires_unsafe/` доказывает, что безопасный код не может публиковать через общий `&RoutePin`; Miri-гейт `TerminalPublicationGate::pause_after_publication` (`routing.rs:66-73`) — «dealloc не трогает тело» после точки линеаризации.
- **Горячие пути:** нейтрален по инструкциям; но сделал pin обязательным артефактом каждого foreign free (с этим — refs-RMW).

### 2.6 `e8843374` «perf: use adaptive class leaves with fallible pre-issue preparation» (0.54/0.44 — наибольшая ступенька)
- **Гарантия:** OOM-безопасность выдачи (C6 ADR: инъекция OOM на границе prepare/commit) и компактность метаданных: прежний плотный класс-мап `[AtomicU8; 262144]` (≈256 KiB на маршрут) заменён на 4KiB-листья, промотируемые владельцем fallible `System.alloc` (SRC_REVIEW_R11_MANIFEST, `tests/loom_r11_small_sidecar.rs`, `tests/r11_p3_small_sidecar_{oom,preflight,routes,zeroed_oom}`).
- **Что добавил (важно для MT):**
  1. Продюсерская публикация получила предварительное чтение класс-кода: до RMW теперь 2 зависимые Acquire-загрузки (указатель листа → байт класса) — `sidecar_bitmap.rs:117-120` → `leaf_classes.rs:132-143`. До коммита publish был одним RMW без чтения класса (`git show 1d881429:src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs`, fn `publish`).
  2. Класс-лист стал **новой общей линией** «владелец↔продюсеры»: владелец пишет байт на каждом issue (`leaf_classes.rs:128`, store Release), каждый продюсер читает его на каждом publish.
  3. Путь владельца (carve/pop) получил per-block prepare/commit-цепочку: `prepare_small_issue` → `SmallSidecar::prepare` → `ClassLeaves::prepare` (+ на свежем листе `System.alloc(256B)` и 256 store'ов прямо в carve/pop-пути, `leaf_classes.rs:95-111`) и commit → `issue_small` с повторными re-валидациями (root/base/kind/sidecar/prepared) — `route_slots.rs:140-182`, `small_sidecar.rs:83-106`, `issue_transaction.rs:88-124`.
  4. Регистрация маршрута стала дороже: `System.alloc_zeroed(294,912)` заменён на `System.alloc(32 KiB)` + 4 096+1 024 явных store'ов инициализации (`directory.rs` diff e8843374; `small_sidecar.rs:58-70`, `leaf_classes.rs:48-59`) — attach-путь и teardown.
- См. §4.2 — почему это ударило именно MT.

### 2.7 `b50f04f4` «route directory shards use a non-allocating spin lock»
- **Гарантия:** не-аллоцирующая блокировка вместо `std::sync::Mutex` (на macOS боксирует pthread-mutex на первом lock → реентерабельность в `SeferAlloc::alloc` при регистрации во время конструирования HeapCore → переполнение стека; текст коммита, `shard_lock.rs:1-5`).
- **Цена:** на Windows/Linux это спин-блокировка: CAS → TTAS-кручение (64) → экспоненциальный backoff (cap 64 паузы) → `std::thread::yield_now`, с проверкой process-exit раз в 64 yield'а (`shard_lock.rs:13-20, 64-100`). Критическая секция `lookup` держит блокировку на весь поиск+pin (§3.1). На Windows нет futex/WaitOnAddress-ожидания — при перенасыщении CPU ожидание превращается в конвой (§4).

## 3. Текущие пути (main) против 0.2.1

### 3.1 Межпоточный (чужой) free на main — пошагово

Вход: `HeapCore::dealloc` → `dealloc_routing` (`routing.rs:10-20`).

1. **Own-проверка:** `owned_block_of` → `canonical_block_of` → tier-1 `own_cache` (загрузка+сравнение) → промах: `hash_find` линейным пробированием (`mem_impl.rs:37-65`; `segment_table_impl.rs:772-787, 834-836`; `hash.rs:28-31`). Для своего блока — быстрый путь (§3.2).
2. **`RouteDirectory::global().lookup(ptr)`** (`routing.rs:27`; `directory.rs:942-971`):
   - `key = addr & !(SEGMENT-1)`, шард = `(key/SEGMENT) ^ (>>13) ^ (>>26)) % 64` (`directory.rs:990-993`);
   - **захват спин-блокировки шарда**: CAS Acquire (`shard_lock.rs:74-77`); при конкуренции TTAS+backoff+yield (`shard_lock.rs:86-98`);
   - **два бинарных поиска**: по блокам шарда (по `key_at(0)`) и внутри блока (≤64 слота) — каждый шаг: `AtomicPtr` load + разыменование `Entry.key` (`directory.rs:285-299, 564-579`);
   - проверка диапазона `[payload, end)` (+ Large-проверка) (`directory.rs:953-960`);
   - **RMW #1**: `refs.fetch_update(AcqRel, Acquire, +1)` (`directory.rs:143-158` через `:965`);
   - повторная валидация: второй `find` + сверка incarnation, иначе `fatal` (`directory.rs:966-968`);
   - отпускание блокировки: store Release (`shard_lock.rs:147-151`).
3. **Fallback-владелец?** `pin.owner()`; для fallback — `try_with_heap` (try-lock) и синхронный `heap.dealloc` (`routing.rs:32-44`).
4. **Публикация Small/Primordial**: `pin.kind()` → `offset = addr − segment_base` (`routing.rs:49-59`) → `SmallSidecar::publish` → `SidecarBitmap::publish`:
   - чтение класс-кода: load указателя листа (Acquire) + load байта (Acquire) (`sidecar_bitmap.rs:117` → `leaf_classes.rs:132-143`); 0 → отклонить;
   - **RMW #2 (точка линеаризации)**: `pending[w].fetch_or(bit, AcqRel)` (`sidecar_bitmap.rs:120`).
5. **Drop pin** → **RMW #3**: `refs.fetch_sub(1, AcqRel)` (`directory.rs:216-231`, строка 222).

**Итого на один чужой free:** 1 CAS блокировки + 1 Release-store блокировки + **3 разделяемых RMW** + ~10+ зависимых загрузок по **4–5 общим cache-линиям** (слово блокировки шарда; линия Entry: refs/owner/kind/incarnation; линия leaf-ptr+байт класса; слово `pending`; опционально линии Block-слотов при поиске). Всё под непререкаемой сериализацией на шард: `lookup`, `register_route`, `remove` берут одну и ту же блокировку (`directory.rs:877-883, 945-948, 976-981`).

### 3.2 Путь владельца (reclaim) и refill на main

- **Обнаружение публикаций**: только на реальных промахах — `find_segment_with_free` → `drain_segment_sidecar` (`find_segment.rs:113-115, 165-200`), strict trim `drain_sidecar_ingress` (`alloc_core/sidecar_drain.rs:431-487`), бюджетный worker `drain_sidecar_ingress_bounded` (`:340-421`). Скан: на каждое слово до `high_water` — `pending.swap(0, AcqRel)` (RMW на каждое слово, включая пустые; `bitmap_scan.rs:37-59`), затем `BitmapCut::pop` → класс-байт → `reclaim_sidecar_record` (`alloc_core_small_reclaim.rs:23-72`): проверки геометрии, `is_free`, `is_in_magazine`, `Node::write_next` (тело свободного блока — класс P1-box), `set_head`, `mark_free`, `dec_live`.
- **Refill** (`refill_magazine_slow`, `hot.rs:652-674`): батч `FREE_PARK_CAP[c]` (≤16, `tcache.rs:64,117`) → `refill_with_large_rescue` (плюс до 4 Large-слотов прощупывания на каждый промах, `sidecar_drain.rs:525-579`, `LARGE_HOT_BUDGET=4`) → `refill_class_bump_impl` (`alloc_core_small_magazine.rs:192-365`): drain freelist батчем (`try_drain_freelist_batch`: prepare-цикл + per-block `mark_alloc`/`commit_prepared`, `alloc_core_small_impl.rs:538-662`) → иначе `try_carve_batch` (`:870-998`: prepare-цикл на блок, один `set_bump`, один `add_live`, per-block `commit_prepared`) → `finish_magazine_refill`: **per-block `mark_magazine`** — `segment_base_of_ptr` + bitmap-RMW на каждый блок, оставленный в журнале (`hot.rs:66-89`, строки 81-87).
- **Own-free** (`dealloc_own_thread_with_base`, `free/dealloc_own_base.rs:376-592`): `class_for` → `small_free_guard` (`:188-322`): `BlockKind::of` (header), `payload_start`, `is_in_magazine` (bitmap), `bump_of`, `is_free` (bitmap) → **`SizeClasses::divides(c, off)` — не-степенное деление на каждом small free** (`:420-425`) → push: `mark_magazine` (bitmap-RMW) + слот (`:451-454`). Переполнение журнала (16): clear-bit цикл по 8 + `flush_class` → `dealloc_small` на блок: `kind_at`, `bump`, `is_free`, `Node::write_next`, `set_head`, `mark_free`, `dec_live`, **`publish_nonempty`** — обновление directory-батмапы на каждый переход empty→non-empty (`alloc_core_small/dealloc.rs:34-144`, строки 119-130; фича `alloc-segment-directory` входит в `production`, `Cargo.toml:423`).
- **Hit-путь alloc** (`hot.rs:226-301`): pop из журнала + **`clear_magazine_on_issue`** — `segment_base_of_ptr` + bitmap-RMW на каждый hit (`hot.rs:35-46, 300`; отдельный замер стоимости — `docs/perf/R29_10_ALLOC_HIT_CLEAR_MAGAZINE_ISOLATION_GATE.md`, `tcache.rs:9-20`).

### 3.3 0.2.1 (`7ea2798d`) — тот же путь

Вход `HeapCore::dealloc` → `dealloc_routing` (`git show 7ea2798d:src/registry/heap_core.rs`, fn на строке 920):
1. `base = segment_base_of_ptr(ptr)` (маска);
2. чтение отдельных полей заголовка: `magic_at`, `owner_thread_free_at`, `kind_at` (поле-специфичные чтения, задача #33 — согласованы с параллельными записями владельца);
3. свой/чужой — сравнение штампа с собственным `thread_free_head`;
4. чужой Small: `class_for` → **`RemoteFreeRing::push`**: load tail (Relaxed) + load head (Acquire, проверка заполнения) + **CAS tail (AcqRel) — единственный RMW** + store слота (Release) (`git show 7ea2798d:src/alloc_core/remote_free_ring.rs`, строки 332-358);
5. чужой Large: push base в owner-стек (`push_large_deferred_free`).

**Итого 0.2.1: 1 RMW, 0 блокировок, 0 каталога, 0 pin-счётчиков; всё состояние — в заголовке самого сегмента.** Owner-переиспользование — дренирование кольца при своих операциях. Плата этой схемы — ровно то, что волна 29.09–01.10 чинит: UAF-окно маршрута при recycle сегмента (P2-1), отсутствие incarnation, блокирующий owner-drain. Таким образом регресс — цена корректности, но текущая реализация берёт этой корректности максимум: блокировка+pin+двойной поиск на каждый free — избыточны относительно контракта (§3 контракта разрешает схему «по одному RMW на слово и полный скан», а гарантии lifetime может нести epoch/ledger вместо per-free refcount).

## 4. Диагноз MT-регресса

### 4.1 Лестница бисекта → механизмы
| коммит | larson/mstress | главный механизм удорожания |
|---|---|---|
| 08d54ff4 | 0.92/0.83 | база: изоляция терминальных слов |
| 960b35b9 | 0.84/0.77 | примитив добавлен (не подключён) — эффект в пределах шума |
| efec9d16 | 0.76/0.68 | foreign free пошёл через каталог: блокировка шарда + поиск + System-аллокации на регистрации |
| 1d881429 | 0.73/0.65 | маршруты на каждый сегмент, register/remove под той же блокировкой; рост трафика на шардах |
| be7a5ae2 | 0.65/0.61 | pin обязателен → refs-RMW (+1/−1) на каждый foreign free; владелец↔продюсеры делят линию Entry |
| e8843374 | 0.54/0.44 | +2 зависимые Acquire-загрузки и вторая общая линия (класс-лист) на точке линеаризации; per-block prepare/commit на refill владельца; дорогая регистрация (5120 store) |
| b50f04f4 | (в составе main) | спин-блокировка вместо Mutex: без futex-ожидания — конвой при перенасыщении |

### 4.2 Почему «perf»-коммит e8843374 замедлил именно MT
1. **Точка линеаризации продюсера удлинилась и обзавелась второй разделяемой линией.** До: 1 RMW на `pending`. После: load листа → load байта → RMW `pending` — три шага зависимых Acquire-доступов (`sidecar_bitmap.rs:113-122`). Байт класса для гранул сегмента пишет владелец на каждом issue (`leaf_classes.rs:128`), читают все продюсеры на каждом publish → линия класс-листа постоянно пинг-понгуется между 4 потоками даже там, где `pending`-слово разное.
2. **Владельческий refill/issue подорожал per-block**: prepare (load листа) + prepared-перепроверка + store класса + повторные fatal-re-валидации root/base/kind/sidecar (`route_slots.rs:140-182`) — на свежем листе ещё и `System.alloc(256B)`+256 store внутри carve (`leaf_classes.rs:95-111`). В larson (постоянные refill-промахи при churn) владелец дольше «держит» рабочие линии и чаще встречает продюсерские публикации в своих листах.
3. **Attach/teardown подорожали**: инициализация sidecar 5120 store'ов (±страничные прерывания) на регистрацию маршрута — в MT-бенчах с постоянным созданием/гибелью потоков (larson) это на пути критической секции attach.
4. Совместно с refs-RMW из be7a5ae2 это дало наибольшую ступеньку (0.76/0.68 → 0.54/0.44). Оценка: вклад (1)–(2) — основной, (3) — второй; точную атрибуцию даст iai-плечо на foreign-free (сейчас его нет — см. §7.4, метрика).

### 4.3 Коллапс 100× под фоновой нагрузкой — гипотеза (не измерялось)
`lookup` держит шард-блокировку весь поиск; ожидание — кручение с `yield_now` (`shard_lock.rs:86-98`). При нехватке CPU (фоновая нагрузка) держатель блокировки вытесняется, ожидающие сжигают свои спины, переходят в yield-циклы; планировщик Windows под нагрузкой даёт полосы держателю редко → каскадный конвой на всех операциях, попадающих в тот же шард. Согласуется с избирательностью (падает MT под нагрузкой, однопоточно — нет). Проверка: заменить ожидание на `WaitOnAddress`/futex (R2) и замерить тот же сценарий; также считать время под блокировкой (`bench-internals` уже имеет `dbg_with_shard_of_locked_for_test`, `directory.rs:926-933`).

## 5. Разбор горячих путей против mimalloc (однопоточно)

Измеренные Ir (CI, `perf_gate_iai`, маргинально через N/2N): `cold_alloc_free_256x16b` ≈ 385 Ir/пару против ≈102 у mimalloc (×3.8); `small_churn_16b` ≈88 против ≈77 (×1.14); фиксированная часть ≈48.1k против ≈11.7k (×4.1). Wall-clock (Windows, criterion): burst 16B ×4.79, 64B ×3.56, 256B ×2.93, 1024B ×1.51; churn 16B ×1.44–1.69; 256B/1024B churn — Sefer уже быстрее.

mimalloc (механика, по общедоступному устройству; без ссылок на код): alloc-hit = чтение thread-local heap + pop из page-local free list; free = поиск страницы по маске адреса + push в local free list (0 атомиков) либо в thread-free (1 CAS); carve = bump по странице с ленивым extend; per-block учётных цепочек (битмапы/транзакции/класс-листы) нет.

Где Sefer платит лишнее (все проверки чтением):
1. **Per-block issue-цепочка** (carve/pop, e8843374): `prepare_small_issue`+`commit` = 2×(`segment_id_at` + `base_at` + root-сравнение + load листа) + prepared-проверка + store класса + bounds — `segment_table_impl.rs:324-377`, `route_slots.rs:140-182`, `small_sidecar.rs:83-106`, `leaf_classes.rs:114-143`. На `try_carve_batch`/`try_drain_freelist_batch` — prepare-цикл и `commit_prepared` на каждый блок (`alloc_core_small_impl.rs:893-902, 994, 568-615, 633`).
2. **Guard-цепочка и деление на free**: `small_free_guard` — 5 обращений к метаданным (`dealloc_own_base.rs:204-321`) + **`divides()` — не-степенное деление на каждом small free** (`:420-425`; обычные сборки, не `hardened`).
3. **Bitmap-журнал на пару**: `mark_magazine` на push (`:451`), `clear_magazine_on_issue` на hit (`hot.rs:300`), `is_in_magazine` на guard (`:311`), `mark_alloc`/`mark_free` freelist-битмапы, `inc_live/dec_live`, `publish_nonempty/publish_empty` directory-битмапы (`alloc_core_small/dealloc.rs:126-130`, `alloc_core_small_impl.rs:456-460, 648-652`).
4. **Refill-хвост**: per-block `mark_magazine` по n−1 блокам (`hot.rs:81-87`) + bounded Large-прощупывание на каждый промах (`hot.rs:654`, `sidecar_drain.rs:525-579`).
5. **Attach/teardown**: `SeferAlloc::new()` в iai-ячейке внутри замеряемого тела (`perf_gate_iai.rs:166-179`) — bootstrap, primordial, `RouteSlots::new` → `register_route` → 32KiB sidecar + 5120 store (`small_sidecar.rs:58-70`) + Entry-аллокации; teardown: flush журнала с per-block clear (`dealloc_own_base.rs:516-528`), strict drain всех слов (`sidecar_drain.rs:431-487`), `ClassLeaves::Drop` — цикл по 1024 слотам (`leaf_classes.rs:220-236`).

Фиксированная часть ×4 в `small_churn_16b` — это именно bootstrap+claim+регистрация маршрута+первый refill (iaj-арм вызывает `SeferAlloc::new()` внутри замера, `perf_gate_iai.rs:166-167`), у mimalloc — ленивый статический init. На прод через 2N-маргинальный метод это константу процесса, не throughput; в wall-clock burst-бенчах она вне таймера, поэтому там доминируют п.1–4 (и, вероятно, вклад first-touch страничных прерываний при carve — требует счётчиков pf, сейчас не измерено).

## 6. Что делать: кандидаты ускорения

### 6.1 Межпоточный free — дизайн «≤1 разделяемый RMW на блок» без ослабления гарантий
Контракт (§2–3 REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT) требует: тело не трогать; терминальная публикация = одна RMW-точка; producer после неё не пишет в reservation; ledger держит credit → сегмент не освободится при in-flight producer; incarnation против повторной регистрации. Ничего из этого не требует per-free блокировки, бинарного поиска и refcount-pair.

**R1. TLS-кэш пинов (рекомендуется первым из структурных).** Поток кэширует `(base → (ptr Entry, incarnation))` после первого `lookup`; пин удерживает `refs` постоянно (Entry+sidecar — независимые System-аллокации, безопасно переживают release root: `routing.rs:32-34`). Каждый последующий foreign free: 1 Acquire-load флага `linked` в Entry (новое атомарное поле, ставится false при unlink под блокировкой) + сверка incarnation + класс-проверка + **`pending.fetch_or` — единственный RMW**. Промах кэша/`linked=false` → старый `lookup`. Убирает с пути: блокировку, оба поиска, refs +1/−1 → ~3 RMW → 1. Почему не теряется free: unpublished producer держит сегментный credit (контракт §2), значит сегмент не может уйти, пока его блок не опубликован и не reclaim-нут; публикация в unlink-нутый, но живой sidecar невозможна (флаг проверен Acquire-до-RMW; unlink → drain → free sidecar последовательность исключает набор бита после освобождения, т.к. refs пина держат sidecar). Доказательства: Loom-модель «unlink ∥ publish» (обязана ловить публикацию в умирающий маршрут), Miri-клетки на узкий provenance (как существующие `TerminalPublicationGate`), mutation-тесты `r6_route_directory*`. Метрика: новый iai-плечо foreign-free; mt larson/mstress T=4.

**R2. Ожидание вместо кручения (низкий риск, сразу).** В `ShardLock::lock_unless_exiting` после исчерпания спин-фазы — `WaitOnAddress` (Windows) / futex (Linux), macOS — оставить спин (b50f04f4 сохраняет non-allocating гарантию: обе ОС-примитивы не аллоцируют). Убирает конвой §4.3. Доказательство: существующие тесты process-exit/exact-shard refusal (b50f04f4 README), плюс stress-тест «держатель вытеснен». Метрика: сценарий «фоновая нагрузка» владельца + larson/mstress T=4/8.

**R7 (финальный). Безблокировочный каталог/эпохи.** Чтение каталога без блокировки: либо seqlock/версионированные шарды (copy-on-write вставок), либо epoch-based reclamation (в дереве уже есть `src/concurrent/epoch/` и `src/concurrent/lock_free/lock_free_page_table.rs` — переиспользовать, не изобретать). Предельный вариант по духу §3 контракта — per-segment терминальное слово в заголовке сегмента + эпоха/ledger вместо каталога (0.2.1-геометрия с incarnation): producer: 1 Acquire-load слота + 1 RMW в заголовке/ sidecar сегмента. Требует полного набора: Loom «release ∥ in-flight publisher», Miri-клетки на чтение заголовка чужого сегмента при узком provenance (аналог задачи #33, field-specific reads), Kani/Loom ABA на incarnation, bounded reclamation. Это самый большой выигрыш и самый большой объём доказательств — делать после R1/R2, отдельным ADR.

### 6.2 Однопоточные cold/burst-кандидаты
**R3. Батчевый issue-touch (низкий-средний риск).** В `try_carve_batch`/`try_drain_freelist_batch`: hoist`segment_id_at`/`base_at`/root/kind/sidecar-валидации на один прогон (лист меняется раз в 256 гранул — prepared-проверка листа раз на прогон), per-block оставить только store класса и (в debug_assert) witness-проверки; OOM-семантику не менять (prepare-цикл до мутаций сохраняется). Экономия ~15–30 Ir/блок на carve и pop. Доказательства: `tests/r11_p3_*` (oom/preflight/routes/zeroed) обязаны остаться зелёными + мутант «prepare пропущен» краснеет; iai `cold_alloc_free_256x{16,64}b`, `recycle_alloc_free_256x{16,64}b`.

**R4. Штамп magazine-резиденции в слове блока вместо второго bitmap (средний риск).** P1-box уже легализует запись в тело свободного блока на retire/reuse-путях (path-b). Перенос oracle «в журнале?» с `magazine_bitmap` на word1-штамп: hit-path теряет `clear_magazine_on_issue` (1 RMW), free-guard теряет `is_in_magazine` (1 load), push теряет `mark_magazine` (1 RMW), refill-хвост (`hot.rs:81-87`) — n−1 RMW. Осторожно: (а) это расширяет класс P1-box на ещё одно место записи в тело — обновить сигнатуру ожидаемо-красных Miri-шагов (path-b CI: место+вызывающий), (б) M2-oracle (Э6) заменить эквивалентно (штамп ставится только на блок в журнале; flush снимает; оракул «word1==marker → медленная проверка bitmap»), (в) не трогать интрузивный free-list. Доказательства: мутирующие тесты двойного free (в журнале/после flush), `R24_2_FREE_BY_MAGAZINE_STATE`-плечи, Miri paused-witness с новой сигнатурой, iai `alloc_magazine_hit_only_16b`, `dealloc_free_only_16b`.

**R5. Убрать `divides()` с обычного пути (средний риск).** Замена: класс страницы из page map в заголовке (уже пишется карвером под `page-map-diag`; сделать безусловным и authoritative для free-guard: `class_of(off/PAGE)==c` — 1 load+сравнение) либо u16-таблица кратных. Требует поддержания page map на reset/decommit-путях. Доказательства: H1/M2-матрица (interior/wrong-class отклонения), iai free-плечи.

**R6. Дешёвые init/Drop sidecar (низкий риск).** (а) `pending` — через `System.alloc_zeroed` без 4096-цикла (нулевая страница = валидное состояние; так было до e8843374), `ClassLeaves::initialize_at` сжать: вместо 1024 store — leaf-присутствие кодировать одним bitmap'ом на 16 u64 и аллоцировать массив лениво; (б) `ClassLeaves::Drop` — обход по bitmap присутствия (16 слов) вместо 1024 pointer-load (`leaf_classes.rs:220-236`). Ускоряет attach и churn+teardown. Доказательства: `r6_route_directory*`, `r11_p3_small_sidecar_preflight` (leaf-promotion), iai teardown-плечи.

### 6.3 Что даёт MT сверх R1/R2
Владельческий reclaim при fan-in (mstress): скан `pending` по всем словам до high_water на refill-промахе (`sidecar_drain.rs:443-465`) — известный пункт OPEN_ITEMS (до 4096 swap ≈49k Ir на нарезанном сегменте; эскалация §6). R3 снижает частоту промахов-сканов косвенно (быстрее refill); прямое лечение — summary-бит (C‴ NO-GO 2026-10-14 по цене продюсера, контракт §13) остаётся закрытым; честная альтернатива — cursor-скан только в сегментах с недавними публикациями (числовой hint, обновляемый продюсерским RMW в то же слово `pending` — не добавляет продюсеру операций). Это设计与 доказательство уровня R7, в краткий список не включено.

## 7. Итоговая таблица целей и порядок внедрения

Ожидания: «паритет» = ≥0.95 от mimalloc (метрика ячейки), «опережение» = устойчиво >1.0, «разрыв» = сокращение без паритета. Порядок — по выигрыш/риск.

| # | Цель (разрыв) | Причина (file:line) | Изменение | Ожидание | Сохранение безопасности | Доказательства | Метрика приёмки |
|---|---|---|---|---|---|---|---|
| R2 | MT larson ×1.7, mstress ×2.0; коллапс 100× под фоном | Спин+yield в `shard_lock.rs:86-98`; секция `lookup` `directory.rs:945-968` | Futex/WaitOnAddress в `ShardLock` (macOS — спин) | Паритет MT-деградации под нагрузкой; MT +5–15% (гипотеза) | Без изменений протокола; non-allocating сохранён (ОС-примитивы не аллоцируют) | Существующие process-exit/exact-shard тесты + стресс «вытесненный держатель»; Loom не требуется (обёртка ожидания) | mt larson/mstress T=4,8; сценарий фон-нагрузки владельца |
| R1 | MT larson ×1.7, mstress ×2.0 | 3 RMW+блокировка+2 поиска на foreign free: `directory.rs:945-968`, `:147-158`, `:222`, `sidecar_bitmap.rs:117-120` | TLS-кэш пинов + флаг `linked`; путь = 1 read + 1 RMW | Паритет MT вероятен; опережение — вместе с R3 | Ledger-credit исключает release при in-flight producer (контракт §2); sidecar независим от reservation (`routing.rs:32-34`); unlink проверен Acquire до RMW | Loom «unlink ∥ publish» (новая), Miri-клетки narrow-provenance, mutation `r6_route_directory*` | iai foreign-free (новое плечо), mt larson/mstress T=4 |
| R3 | burst 16B ×4.8 / 64B ×3.6 / 256B ×2.9; MT (refill-хвост) | Per-block prepare/commit-цепочка: `route_slots.rs:140-182`, `alloc_core_small_impl.rs:893-902,994,568-633`, `leaf_classes.rs:114-143` | Hoist re-валидаций и prepared-проверки на батч; per-block — только store класса | 385 → ~300 Ir/пару (разрыв), MT +3–8% | OOM-семантика C6 не меняется (prepare-до-мутаций сохранён); fatal-гард'ы остаются на прогон | `tests/r11_p3_*` зелёные + мутант «skip prepare» красный; `cold_alloc_free_256x{16,64}b`, `recycle_alloc_free_256x{16,64}b` ≤ гейтов ADR | iai cold/recycle-ячейки; mt larson T=4 |
| R6 | attach (фикс. часть ×4 в churn-ячейке), churn+teardown ×1.7/1.6/1.5 | `small_sidecar.rs:58-70` (5120 store), `leaf_classes.rs:48-59, 220-236` (Drop 1024 load), `dealloc_own_base.rs:516-528` (clear-цикл) | alloc_zeroed для pending; bitmap присутствия листьев; сжатый Drop | Фикс. часть и teardown-константа ↓ в 2–4 раза; на throughput минимально | Состояние «нулевая страница» валидно (инвариант publish: encoded 0 → reject, `sidecar_bitmap.rs:117`); Drop по bitmap полон (инвариант store+set вместе) | `r6_route_directory*`, `r11_p3_small_sidecar_preflight`, quiescent-оракулы System-totals | iai `small_churn_16b` (B-часть через N/2N), teardown-плечи criterion |
| R4 | churn 16B ×1.4; churn+write ×1.6; burst-хвост | `hot.rs:300` (clear на hit), `dealloc_own_base.rs:311,451`, `hot.rs:81-87` (mark-цикл) | Штамп word1 вместо magazine_bitmap (P1-box-класс) | Churn-путь ≈ mimalloc: паритет; опережение не обещаю | P1-box расширяется осознанно: место записи — только свободный/журнальный блок; M2-оракул эквивалентен; интрузивный free-list не тронут | Мутанты двойного free (в журнале/после flush) красные; Miri paused-witness с новой сигнатурой (path-b CI); `R24_2/R24_3`-плечи | iai `alloc_magazine_hit_only_16b`, `dealloc_free_only_16b`, `small_churn_16b`; criterion churn |
| R5 | churn 16B ×1.4 (остаток), burst 64B/256B | `dealloc_own_base.rs:420-425` — не-степенной div на каждом small free | Класс страницы из page map / u16-таблица кратных | −10–20 Ir/free; вместе с R4 — паритет churn | page-map становится load-bearing: поддерживается carve/reset/decommit; расхождение = fatal (как `reject_large_interior_free`) | H1/M2-матрица (interior/wrong-class), reset/decommit-тесты page map | iai `dealloc_free_only_16b`, `small_churn_16b` маргинально |
| R7 | MT опережение; mstress fan-in | Каталог+блокировка+pin как класс: §3.1; скан владельца `sidecar_drain.rs:443-465` | Безблокировочный каталог (эпохи/seqlock) или per-segment терминальное слово (контракт §3) + epoch/ledger lifetime | Опережение MT возможно при дешёвом владельце; без R3-цикла — только паритет | Все гарантии контракта: инкарнация, ledger, no-body; эпохи исключают UAF маршрута | Полный набор: Loom (release∥publisher, ABA), Miri-клетки заголовка чужого сегмента, Kani incarnation; отдельный ADR | mt larson/mstress T=1..8; iai foreign-free; p99 remote-lag |

Честные пределы:
- **Cold burst (16–256B): паритет по Ir при текущих контрактах недостижим.** ~100 Ir/пару у mimalloc — это «pop+push без учётных цепочек»; у Sefer per-block цена несёт C6/OOM-безопасность (prepare/commit), exactly-once credit (`inc_live/dec_live`), M2-оракулы и SEC-1. После R3+R4+R5 ожидаемо ~230–300 Ir/пару (×1.3–1.6 wall-clock). Дальше — только перенос учёта с per-block на per-batch/пер-сегмент, что требует нового решения по C6 (не рекомендуется в рамках B1). Wall-clock-обгон возможен, если доминируют страничные прерывания carve — сначала измерить pf-счётчики, не обещать.
- **Churn 16B/256B/1024B: паритет реалистичен (R4+R5+R3); устойчивое опережение не обещаю** — hit-путь mimalloc уже минимален, у Sefer после R4/R5 останется лишняя ветка guard-цепочки и no-body-преимущество, которое не конвертируется в Ir.
- **MT: паритет (≥0.95 по ADR) реалистичен после R2+R1+R3; опережение — только после R7** и при дешёвом owner-reclaim; mstress может упираться в скан владельца при fan-in — это отдельный пункт (§6.3), не обещан.

## 8. Рекомендация

1. **Сразу (дни):** R2 (futex/WaitOnAddress) — снимает худший хвост (коллапс под фоном) при минимальном риске; R6 (инициализация/Drop) — без протокольных изменений.
2. **Затем (1–2 недели):** R3 (батчевый issue) и R1 (TLS-кэш пинов) с Loom+Miri-набором; ввести iai-плечо foreign-free и mt-гейт в CI, чтобы дальнейшее измеряемо.
3. **Потом (отдельный ADR):** R4 (штамп журнала — расширяет P1-box, требует обновления Miri-сигнатур), R5 (page-map authoritative).
4. **Финал (ADR + полный proof-набор):** R7 (безблокировочный каталог / per-segment слово + эпохи) — путь к опережению MT и выполнению предельного варианта контракта §3.
Каждый шаг — отдельный пред-registered гейт по образцу ADR §Гейты: iai-ячейки + mt-bench + существующие Loom/Miri-клетки, с negative-контролями (мутант обязателен красным).
