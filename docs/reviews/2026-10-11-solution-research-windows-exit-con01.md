# Исследование решения: зависание выхода процесса на Windows (R17-CON-01 / R17-LIF-01)

База: `31db7f17698459e9ce26c3f8701c314304b9c00b` (проверено `git rev-parse HEAD`).
Режим: только чтение. Компиляция, тесты, скрипты, бенчмарки не запускались.
Владелец в индексе: `docs/correctness-open-items/TRACKED_platform_contracts.md:320–325`, item **185**
(«Windows shutdown may strand allocator locks after a holder can no longer run»). Item 152 — POSIX
`fork`, item 162 — terminal/service acceptance; оба смежные, не владельцы.

## §0 Чек-лист хода

- [x] Шаг 0: база проверена (HEAD = 31db7f17)
- [x] Прочитаны CLAUDE.md, индекс (item 185, 152), R17-CON-01, R17-LIF-01, синтез P4
- [x] (1) Инвентарь неограниченных ожиданий — §2.1
- [x] (2) Семантика завершения процесса Rust на Windows — §2.2
- [x] (3) Достижимость зависания по сценариям — §2.3
- [x] (4) Варианты решения — §3
- [x] (5) Дизайн теста — §5.1–§5.2
- [x] Рекомендация, план, вопросы — §4–§7

## §1 Постановка

Гипотеза R17-CON-01/R17-LIF-01 (условный P2, синтез P4 понизил до «P4 исследования», диспозиция
NOT VERIFIABLE FROM STATIC SOURCE): на Windows `ExitProcess` убивает остальные потоки без
выполнения их кода; если убитый поток держал спин-лок крейта, то код, исполняемый ПОСЛЕ этого на
выходящем потоке (его TLS-деструкторы: `AbandonGuard::drop` → trim → снятие маршрутов; поздние
`free`/`alloc` после `TORN`), ждёт этот лок вечно. Последствие — процесс не завершается, одно ядро
занято. Не memory-safety.

Что нужно: (1) инвентарь мест ожидания; (2) реальная семантика выхода; (3) достижимость;
(4) варианты решения; (5) тест-оракул. Итог — одно рекомендованное решение.

## §2 Анализ механизма

### §2.1 Инвентарь ожиданий без ограничения (НАБЛЮДЕНО по коду на 31db7f17)

| # | Ожидание | Место | Форма | Кто снимает | Восстановление после смерти владельца |
|---|---|---|---|---|---|
| W1 | `ShardLock::lock` (64 шарда `RouteDirectory`) | `src/registry/segment_route/shard_lock.rs:48–66` | CAS `false→true`, 64× `spin_loop`, далее бесконечный `std::thread::yield_now()` | только `ShardGuard::drop` владельца (`:85–88`) | нет; владелец не записывается |
| W2 | fallback `LockGuard::acquire` | `src/global/fallback.rs:488–504` | CAS `LOCK`, 64× `spin_loop` (`LOCK_TIGHT_SPINS`, `:442`), далее `yield_now` | `LockGuard::drop` (`:507–511`) | нет |
| W3 | fallback `INIT_STATE == INITIALIZING` (проигравший) | `src/global/fallback.rs:310–325` | load-цикл, spin → `yield_now` | победитель (READY/UNINIT) или `InitStateGuard::drop` при unwind (`:578–589`) | только unwind; смерть потока не покрыта |
| W4 | `OncePtrCell` sentinel (материализация чанков реестра/sidecar) | `crates/once-ptr-cell/src/lib.rs` (док `:59–74` прямо: «A cell whose INITIALIZING owner has stopped running … is waited on forever») | чистый `spin_loop`, без yield | победитель | нет |
| W5 | `ExactShard` спин (только opt-in `exact-object-proto`) | `src/global/exact_object/exact_shard.rs:190` | чистый спин | владелец | нет; вне `production` |

Неблокирующие (НАБЛЮДЕНО): `fallback::try_with_heap` (`fallback.rs:388–402`, `try_acquire` без ожидания) —
используется maintenance-воркером и `publish_foreign` для fallback-блоков (`src/registry/heap_core_xthread/routing.rs:35–44`).
Пины `RoutePin` не ждут (refcount, освобождение последним держателем, `directory.rs:142–223`).
`terminal_publication_gate.rs` — `std::sync::Mutex`, только `cfg(miri, internals, bench-internals)`.

Критические секции держателей (НАБЛЮДЕНО, длительность — оценка по коду, не измерение):

| Лок | Кто берёт | Что внутри | Длительность |
|---|---|---|---|
| W1 `lookup` | каждый межпоточный `free` (`publish_foreign`, `routing.rs:27`), поздний `free` после `TORN` | `find` (бинпоиск в индексе блоков) + `pin_from_array` (атомарный инкремент) + повторный `find` (`directory.rs:868–889`) | десятки–сотни нс |
| W1 `register` | каждый новый Small/Primordial сегмент, каждая Large-выдача (и повтор из кэша) | `Shard::insert` со сдвигом элементов блока; аллокации `System` — ВНЕ лока (`directory.rs:832–864`) | сотни нс |
| W1 `remove` | `RouteRegistration::drop` (`registration.rs:78–82`) из `SegmentTable::recycle` (`segment_table_impl.rs:677`) / `unregister` (`:516`) | `Shard::remove` со сдвигом, слиянием блоков и **`Block::release` → `System.dealloc` (HeapFree) ПОД локом** (`directory.rs:605–652`) | сотни нс; с HeapFree — до мкс |
| W2 | `with_heap(_config)` (alloc/realloc на TORN-потоке, насыщение реестра); `try_with_heap` (воркер, `publish_foreign` fallback-блока) | произвольная операция fallback-`HeapCore`, включая trim с `VirtualFree` и снятие маршрутов (вложенно берёт W1) | от сотен нс до десятков мкс (воркерский `background_maintenance_step` → `trim_cold_retention`, `ownership.rs:117–154`) |
| W3 | первая инициализация fallback | `HeapCore::new` (резерв ОС) | однократно, мкс |

Порядок вложенности (НАБЛЮДЕНО): W2 → W1 (fallback-операция снимает/регистрирует маршруты). Обратного
нет: `lookup` отпускает W1 до `try_with_heap` (`routing.rs:32`). Циклов нет — на Linux взаимная
блокировка невозможна.

Пути выходящего потока, которые входят в W1/W2 (НАБЛЮДЕНО; совпадает с R17-CON-01, перепроверено):

- **P-trim:** `AbandonGuard::drop` (`src/global/tls_heap.rs:195–224`) → `trim_for_recycle`
  (`src/registry/heap_core/state/ownership.rs:106–113`) → `trim_cold_retention` (`:138–154`) →
  `drain_small_pool` / `release_empty_current_small_for_trim` / `evict_all` → `SegmentTable::recycle`/`unregister`
  → `RouteRegistration::drop` → `RouteDirectory::remove` (`directory.rs:891–897`) → W1.
- **P-free:** любой `free` чужого блока до GUARD (`Own` → `dealloc_routing` → `publish_foreign`,
  `routing.rs:10–20`) или любой `free` после GUARD (`TORN` → `ForeignNoBind`, `tls_heap.rs:319–334`;
  `global_alloc.rs:75–79`) → `RouteDirectory::lookup` → W1.
- **P-alloc:** `alloc`/`alloc_zeroed`/`realloc` после `TORN` → `CurrentHeap::Fallback`
  (`tls_heap.rs:275`, `:358`) → `with_fallback_heap` → `with_heap(_config)` → W3 (если не READY) и W2
  (`fallback.rs:350`, `:369`).
- **P-bind:** первое выделение НЕпривязанного потока в его TLS-деструкторе → `claim` → W4 (редко).

Порядок TLS-деструкторов (НАБЛЮДЕНО косвенно): `tests/r14_shard_late_tls_teardown.rs:4–9` опирается на
«Thread destructors run in reverse initialization order» (LIFO). `GUARD` армируется при первом
выделении потока (`finish_bind`, `tls_heap.rs:507–514`), т.е. обычно одним из первых, значит его
деструктор обычно идёт ПОСЛЕДНИМ, а все более поздние TLS-деструкторы (пользовательские, std)
исполняются ДО него с ещё живой привязкой (`Own`) — их `free` чужих блоков идут через P-free/W1.
Модульный док `tls_heap.rs:51–55` предупреждает, что порядок не специфицирован; вывод «GUARD обычно
последний» — ВЫВЕДЕНО, не гарантия.

Следствие для решения (ВЫВЕДЕНО): закрыть только `AbandonGuard`-trim недостаточно — поздние `free`
из ДРУГИХ TLS-деструкторов того же потока идут через W1 ещё до `GUARD` (по пути `Own`, не `TORN`).
Решение должно действовать на уровне самого ожидания W1/W2, а не только «контекста TORN».

### §2.2 Семантика завершения процесса Rust на Windows

Источники в этом режиме: только репозиторий. Исходники std/ОС и документация Microsoft/Rust вне
worktree НЕ читались. Ниже три уровня достоверности.

**Д — документированное поведение (по памяти; формулировки стабильны много лет, но сверка с текстом
документации в этой сессии не делалась):**

- Д1. Win32 `ExitProcess`, Remarks: (1) все потоки процесса, кроме вызывающего, завершаются без
  `DLL_THREAD_DETACH`; (2) их объекты становятся signaled; (3) точки входа DLL вызываются с
  `DLL_PROCESS_DETACH` в контексте вызывающего потока; (4) процесс завершается. В тех же Remarks
  прямо сказано, что код detach может заблокироваться, если убитый поток держал нужную ему
  синхронизацию. TLS-callbacks образа (секция `.CRT$XL*`) вызываются тем же механизмом, что и
  `DllMain`, с тем же `reason`.
- Д2. `std::thread::LocalKey`, раздел «Platform-specific behavior»: на Windows при выходе процесса
  TLS-деструкторы могут выполниться только на потоке, вызвавшем выход, т.к. остальные могут быть
  принудительно завершены. (Цитирует и R17-LIF-01 `…panic-lifecycle.md:88`; R17-CON-01 `:86` —
  «по памяти ревьюера».)
- Д3. Завершённый поток Windows никогда больше не исполняет код: AtomicBool-лок, который он держал,
  не будет отпущен. Нативные объекты ядра/ntdll (мьютексы, critical sections) имеют собственную
  обработку брошенного владельца; самописный `AtomicBool` её не получает (Д1/Д3 вместе — суть
  находки).

**П — реализация std по памяти (НЕ проверено; проверяется только исполнением или чтением
исходников std соответствующей версии; MSRV проекта 1.93, `Cargo.toml:8`, CI — stable):**

- П1. Возврат из Rust-`main` (MSVC): сгенерированный `main` → `lang_start` → `rt::cleanup()` →
  возврат кода в CRT → CRT `exit()` → atexit/статические деструкторы C/C++ → `ExitProcess`. Значит
  atexit-обработчики исполняются ДО убийства остальных потоков.
- П2. `std::process::exit` на Windows: `rt::cleanup()` → прямой `ExitProcess` (без CRT `exit`). На
  каком потоке вызван — тот и становится «выходящим».
- П3. std на Windows исполняет деструкторы `thread_local!` из своего TLS-callback (секция
  `.CRT$XLB`) для причин `DLL_THREAD_DETACH` И `DLL_PROCESS_DETACH`. Следствие: на выходе процесса
  деструкторы выходящего потока (включая `GUARD`) исполняются в шаге (3) Д1 — ПОСЛЕ шага (1).
- П4. `JoinHandle::join` на Windows ждёт объект потока (`WaitForSingleObject`), который становится
  signaled только после завершения потока, т.е. после его `DLL_THREAD_DETACH`-callback и всех его
  TLS-деструкторов.
- П5. `std::thread::scope` возвращает управление, когда счётчик работающих потоков scope падает до
  нуля; счётчик уменьшается при drop пакета результата в конце тела потока — ДО того, как ОС-поток
  исполнит свои TLS-деструкторы. Если это так, scoped-потоки после возврата `scope` ещё могут
  выполнять `AbandonGuard` (trim с W1). Касается и собственного `PinnedRunner::run` крейта
  (feature `pinning`, R17-LIF-04 говорит, что он на `std::thread::Scope`).
- П6. `std::thread::yield_now` на Windows — `SwitchToThread()`. В процессе, где остался один поток,
  он возвращается сразу: ожидание W1/W2 превращается в чистый CPU-спин.
- П7. libtest присоединяет (join) потоки тестов. Если так, зелёные Windows-прогоны CI почти ничего
  не говорят о сценарии «поток ещё в TLS-teardown в момент выхода».

**Н — неизвестно, решается только исполнением:**

- Н1. Берёт ли `ExitProcess` (ntdll `RtlExitUserProcess`) loader lock ДО завершения остальных
  потоков. Если да, то поток, находящийся внутри своего `DLL_THREAD_DETACH` (а значит внутри
  `AbandonGuard`-trim), будет дождан и успеет отпустить W1 — сценарий S4 ниже исчезает. Если нет —
  S4 реален.
- Н2. Когда относительно шага (1) выставляется признак «идёт завершение процесса» (ntdll
  `RtlDllShutdownInProgress()` / `PEB->Ldr->ShutdownInProgress`), и документирован ли
  `RtlDllShutdownInProgress` (R17-LIF-01 `:98` тоже оставил это непроверенным). По памяти:
  экспортируется ntdll, используется библиотеками Microsoft (WIL) для того же вопроса «идёт ли
  shutdown»; статус документированности не установлен.
- Н3. Порядок TLS-callback нашего образа относительно callback std, если ставить свой
  (`.CRT$XL?`-секция): лексикографическая сортировка суффиксов линкером — по памяти, для
  `link.exe`/`lld-link`/GNU ld не проверено.

### §2.3 Достижимость по сценариям (ВЫВЕДЕНО из Д/П; не исполнялось)

Условие зависания: (a) в момент шага (1) Д1 какой-то другой поток держит W1/W2 или стоит в W3/W4;
(b) после шага (1) выходящий поток входит в тот же лок (P-trim, P-free, P-alloc, P-bind);
(c) П3 верно (код выходящего потока вообще исполняется после убийства).

| # | Сценарий | (a) возможно? | (b) возможно? | Вердикт |
|---|---|---|---|---|
| S1 | все рабочие потоки `join`-нуты до возврата из `main` | нет (П4: их деструкторы завершены до join) | — | зависания нет |
| S2 | `thread::scope` (и `PinnedRunner::run`), затем сразу выход | да, если П5 верно и Н1 = «не ждёт loader lock» | да | ГИПОТЕЗА, окно = длительность trim scoped-потоков |
| S3 | detached-потоки активно делают межпоточные `free`/`alloc` в момент выхода | да: убийство в произвольной инструкции; доля времени внутри W1 при плотном межпоточном free — существенная (оценка по коду: критическая секция `lookup` сопоставима по стоимости с остальной работой `publish_foreign`) | да; в producer→consumer шарды КОРРЕЛИРОВАНЫ: потребитель ищет маршруты сегментов производителя (главного), а главный в trim снимает маршруты тех же сегментов | ГИПОТЕЗА, наиболее вероятный реальный случай |
| S4 | detached-потоки закончили тело и находятся в своём `AbandonGuard`-trim (main получил «готово» по каналу и вернулся без join) | да, если Н1 = «не ждёт» | да | ГИПОТЕЗА |
| S5 | `std::process::exit` из НЕ-главного потока X | как S3/S4 для всех прочих, включая main | да, на X (П2+П3) | как S3/S4 |
| S6 | запущен maintenance-воркер (`start_maintenance`, opt-in) | да: каждые 10 мс `try_with_heap` держит W2 на `background_maintenance_step` (`maintenance_service.rs:147–150`), внутри trim — вложенный W1 | да (P-alloc после TORN, P-free/P-trim на том же шарде) | ГИПОТЕЗА, малая доля времени (оценка: мкс из 10 мс) |
| S7 | `abort`/`panic=abort` | — | TLS-деструкторы не исполняются | зависания нет |
| S8 | на выходе остальные потоки простаивают (парк/ожидание I/O) | нет: простаивающий поток вне критических секций | — | зависания нет |
| S9 | Linux/glibc: `exit()` → `__call_tls_dtors` выходящего потока при ЖИВЫХ остальных, затем `exit_group` | держатель жив и отпустит | да | зависания нет (только ожидание); по памяти, не исполнялось |

Итог §2.3: при верных П3 и Д1 зависание достижимо из безопасного кода в штатной программе на
Windows, если она выходит при работающих detached-потоках, использующих аллокатор (S3, S5), и,
возможно, в S2/S4/S6. Вероятность на один выход — малая и зависит от нагрузки; без исполнения не
оценивается. Последствие — вечный CPU-спин под loader lock, не UB. Это согласуется с условной оценкой
P2; подтверждение требует §6.

## §3 Варианты решения

Общие требования к любому исправлению (ВЫВЕДЕНО из §2.1): (R1) «чужой» лок нельзя «украсть» —
убитый держатель мог оставить `Shard` посреди `remove_at`/`absorb` (`directory.rs:605–641`), захват
дал бы неконсистентный индекс и `abort()`/UB на следующем `find`; допустим только ОТКАЗ от входа с
утечкой. (R2) Нельзя отказываться от входа в обычной работе (живой держатель): отказ в `lookup`
= утечка блока навсегда, в fallback = null → `handle_alloc_error` → abort. (R3) Горячий путь
(`lookup` на каждом межпоточном free, `register` на каждом сегменте/Large) не должен измениться в
неконкурентном случае.

| ID | Механизм | Аргумент корректности | Цена | Риск | Что нужно доказать |
|---|---|---|---|---|---|
| V0 | Ничего не менять в коде; README «Process exit on Windows» рядом с «Fork safety» + rustdoc `SeferAlloc` + doc-tripwire по образцу `tests/r1_02_fork_safety_contract_docs.rs` | контракт: «перед выходом останавливайте/join-ите потоки, работающие с аллокатором» | минимальная | зависание остаётся; S2 (`thread::scope`, свой `PinnedRunner`) пользователь не может исправить, т.к. он уже «присоединил» потоки | ничего; но П5 делает совет «join» неполным |
| V1 | Флаг `EXITING`, выставляемый CRT-`atexit` (FFI к ucrt), + пропуск trim/lookup/fallback-ожидания при флаге | atexit (П1) исполняется до убийства, флаг виден всем | малая, FFI | не покрывает `std::process::exit` (П2: прямой `ExitProcess`, без CRT `exit`) и cdylib-сценарии; флаг выставлен, пока остальные ЖИВЫ → отказ при живом держателе нарушает R2 (утечки на финише, абортов на fallback) | П1, П2 исполнением |
| V2 | **Отказ от ожидания только при доказанном завершении процесса.** Детектор `process_exit_in_progress()` (Windows: ntdll `RtlDllShutdownInProgress`; иначе `false`), опрашиваемый ТОЛЬКО на медленном пути W1/W2/W3 после исчерпания 64 спинов, перед каждым `yield_now`. При `true` ожидание возвращает «отказ», call-site выбирает безопасный исход: `lookup` → `None` (уже существующий путь «unroutable», `routing.rs:27–31`); `remove` → не снимать маршрут и УТЕЧЬ дескриптор (не ронять owner-ref); `register` → `Err` (уже обрабатывается как OOM, `route_slots.rs:89–91`); fallback W2/W3 → `None` (уже путь «true OOM»). Плюс `AbandonGuard::drop` при `true` пропускает trim целиком | после выставления признака (Н2) все прочие потоки уже завершены (Д1 шаг 1) → любой занятый лок занят мёртвым → ждать бессмысленно, а отказ не трогает защищённые данные (R1). До выставления — поведение байт-в-байт прежнее (R2). Неконкурентный путь не меняется (R3) | ~60–120 строк + одна FFI-декларация (первая `cfg(windows)` в `src/`, сейчас 0 совпадений) + `ManuallyDrop` у `RouteRegistration::entry` | детектор недокументирован/меняет семантику (Н2); на fallback-отказе выход превращается в abort (код 0xC0000409 по памяти) вместо 0; `OncePtrCell` (W4, отдельный крейт) не покрыт | Н2 исполнением: признак `false` при обычном выходе потока и `true` в TLS-деструкторе при `ExitProcess`, и выставлен ПОСЛЕ убийства потоков |
| V3 | «Лок с владельцем»: в слово лока пишется TID (`GetCurrentThreadId`), ожидающий после бюджета проверяет жив ли владелец (`OpenThread`+`GetExitCodeThread`) и при смерти отказывается | точное определение смерти держателя без недокументированных API | меняется ГОРЯЧИЙ путь: TID-вызов на каждом `lookup`/`register`/`remove` (нарушает R3); `AtomicBool`→`AtomicU32`; несколько FFI | повторное использование TID другим процессом/потоком → ложное «жив» → зависание остаётся; сложнее модель для loom | стоимость TID на горячем пути (iai), семантика `OpenThread` в shutdown |
| V4 | Ограниченное ожидание (таймаут ≥1 с) только в «контексте teardown» (`TORN`, `AbandonGuard`) с отказом-утечкой; без детектора ОС | при выходе держатель мёртв → по таймауту отказ; в обычной работе таймаут почти не срабатывает | малая, без FFI | не покрывает поздние `free` ДО `GUARD` по пути `Own` (§2.1, LIFO) — главный сценарий S3; ложные утечки блоков при долгой вытесненности держателя (нарушает R2); выход задерживается на таймаут | распределение длительности удержания под нагрузкой |
| V5 | `try_lock` без ожидания в деструкторах (trim пропускается при конкуренции, поздние `free` утекают при конкуренции) | простой | малая | утечки блоков в ОБЫЧНЫХ выходах потоков под нагрузкой (нарушает R2); то же покрытие, что V4 | — |
| V6 | Lock-free директория маршрутов (RCU/эпохи вместо 64 спин-локов) | нет ожиданий W1 | перепроектирование (`directory.rs` ~900 строк, loom-модели) | огромный; fallback-`HeapCore` по природе однописательный (W2 не устранить) | всё заново |

## §4 Рекомендация

**Выбор: «сначала доказать тестом», затем V2, плюс V0-документация в том же раунде.**

1. Фаза A (без изменения продукта): детерминированный подпроцессный оракул §5.1 на текущем дереве,
   запуск на Windows. Он одновременно (а) подтверждает/опровергает Д1+П3 (единственное внешнее
   допущение) и (б) служит красным контрфактуалом для фазы B. Если на Windows ребёнок завершается
   кодом 0 с маркером «деструктор исполнился», гипотеза ОПРОВЕРГНУТА — item 185 закрывается как
   NULL, код не меняется.
2. Фаза B (только при красном A и подтверждённом Н2 пробой §5.2): V2.
3. V0 (документация контракта) — в любом случае: даже V2 не покрывает W4 и меняет исход на abort
   при мёртвом держателе fallback-лока.

Почему отвергнуты остальные: V1 не покрывает `std::process::exit` и срабатывает при живых
держателях (R2). V3 платит на горячем пути за то, что нужно только на выходе, и всё равно не точен
(TID reuse). V4/V5 не покрывают поздние `free` до `GUARD` (а это основной путь S3) и дают ложные
утечки в обычной работе. V6 несоразмерен. V0 в одиночку не лечит S2, где пользователь всё сделал
правильно.

Что в V2 НЕ доказано (до исполнения):
- Н2: семантика и момент выставления `RtlDllShutdownInProgress`; если он окажется непригоден,
  запасной детектор — собственный TLS-callback в секции, сортирующейся раньше `.CRT$XLB` std (Н3),
  ставящий флаг на `DLL_PROCESS_DETACH` (по Д1 это уже после убийства потоков). Оба варианта
  требуют проверки §5.2.
- Что после выставления признака никакой другой поток процесса больше не исполняется (Д1, шаг 1).
- Исход для W2/W3 (abort вместо hang) приемлем владельцу — вопрос §6.
- W4 (`OncePtrCell`) остаётся: закрывается только частично — пропуском привязки (`bind_slow_tagged*`)
  при `true`; материализация чанков/sidecar убитым победителем на ДРУГИХ путях не покрыта (оценка:
  однократные события на процесс, крайне редки).

## §5 План реализации (эскиз, не исполнялся)

### §5.0 Код (фаза B)

Новый файл `src/global/process_exit.rs` (один экспорт; tier-1 seam `#![allow(unsafe_code)]` с
единственной причиной — FFI; внести в README «Where unsafe lives» и шапку `src/lib.rs`; FFI-стиль
как `tests/support/r8_native_os_mapping.rs:40–41` и `crates/aligned-vmem/src/os/windows.rs:474`):

```text
//! Process-exit detector for lock waits: true only after the OS has
//! terminated every other thread of this process.
#![allow(unsafe_code)]

#[cfg(windows)]
#[link(name = "ntdll")]
extern "system" { fn RtlDllShutdownInProgress() -> u8; } // BOOLEAN

#[inline]
pub(crate) fn process_exit_in_progress() -> bool {
    #[cfg(windows)]
    // SAFETY: argument-less ntdll query; reads loader state only.
    { unsafe { RtlDllShutdownInProgress() != 0 } }
    #[cfg(not(windows))]
    { false }
}
```

Регистрация: `src/global/mod.rs` — `mod process_exit;` + `pub(crate) use` (mod.rs — только
реэкспорт). Модуль `global` и `registry` компилируются вместе (`src/lib.rs:473–492`, оба под
`alloc-global`), так что `segment_route` может звать `crate::global::process_exit_in_progress`.

`src/registry/segment_route/shard_lock.rs` — добавить, НЕ меняя `lock()`:

```text
/// Like `lock`, but gives up once the process is exiting: the holder is
/// then a terminated thread and will never release (R17-CON-01).
pub(super) fn lock_unless_exiting(&self) -> Option<ShardGuard<'_, T>> {
    let mut spins = 0u32;
    while self.locked.compare_exchange_weak(false, true, Acquire, Relaxed).is_err() {
        if spins < SPINS_BEFORE_YIELD { spins += 1; core::hint::spin_loop(); }
        else if crate::global::process_exit_in_progress() { return None; }
        else { std::thread::yield_now(); }
    }
    Some(ShardGuard { lock: self, _unique: PhantomData })
}
```

`src/registry/segment_route/directory.rs`:
- `lookup` (`:871`): `let guard = self.shards[..].lock_unless_exiting()?;` — исход уже
  обрабатывается всеми вызывающими (`routing.rs:27`, `realloc.rs:414` → null).
- `register` (`:833`, `:860`): при `None` → `Err(RouteError::…)`. Либо новый вариант
  `OwnerTerminated` в `error.rs` (честнее для диагностики), либо `OutOfMemory`; вызывающий
  `route_slots.rs:89–91` делает `.ok()`, т.е. разница только диагностическая. Важно: в `:860` уже
  выделенные `block`/`enlarged` нужно уронить (они `System`, drop безопасен).
- `remove` (`:891–897`) → `pub(super) fn remove(&self, entry) -> bool`.

`src/registry/segment_route/registration.rs`: поле `entry: ManuallyDrop<EntryHandle>`; `Drop`:
`if self.directory.remove(&self.entry) { unsafe { ManuallyDrop::drop(&mut self.entry) } }` — иначе
дескриптор намеренно утекает (шард заморожен навсегда, никто его не увидит; VA сегмента
освобождается дальше как обычно — `lookup` не читает байты резервации, `directory.rs:867`).
Альтернатива без `unsafe`: `Option<EntryHandle>` + `take()`/`mem::forget` — дороже на каждом
обращении к `entry`; выбрать при реализации.

`src/global/fallback.rs`: `LockGuard::acquire` → `Option<Self>` с той же веткой детектора;
`with_heap`/`with_heap_config` возвращают `None` при отказе (существующий «true OOM»);
цикл W3 (`:311`) — тот же выход с `return core::ptr::null_mut()` при `true`.

`src/global/tls_heap.rs`:
- `AbandonGuard::drop`: после `mark_local_torn()` — `if process_exit_in_progress() { drop(lease);
  return; }` (trim на выходе бесполезен: адресное пространство освобождает ОС; R17-LIF-01 H4 —
  ещё и ускоряет выход).
- `bind_slow_tagged*`: при `true` вернуть `CurrentHeap::Fallback` (не входить в W4 `claim`).

Диагностика: один счётчик `EXIT_LOCK_GIVEUPS: AtomicU64` (Relaxed, холодный путь), доступ через
`#[doc(hidden)] dbg_exit_lock_giveups()` под `bench-internals` (правило benchmark-hook п.2; чистый
наблюдатель → строка в `PURE_OBSERVERS` `scripts/verify-dbg-hook-safety.mjs`).

Влияние на горячий путь: неконкурентный CAS и первые 64 спина не меняются; на не-Windows детектор
— константа `false` и ветка исчезает. Заявлений о скорости нет; `npm run check` (включая iai) —
как обычно.

### §5.1 Тест-оракул A: брошенный лок на выходе процесса (детерминированный)

Файл `tests/con01_exit_stranded_lock.rs`, `#![cfg(all(feature = "alloc-global",
feature = "alloc-xthread", feature = "internals", feature = "bench-internals"))]`, НЕ гейтится на
Windows: на Linux он — контроль (зелёный до и после).

Новый хук (только для shard-сценария; fallback-сценарию хватает существующего
`HeapCore::dbg_with_fallback_for_test`, `fallback.rs:414–416`):
`RouteDirectory::dbg_with_shard_of_locked_for_test(ptr: *mut u8, f: impl FnOnce())` — берёт
`lock()` шарда ключа `ptr` и исполняет `f` под ним. Не разыменовывает `ptr` (только адрес-ключ, как
`lookup`), поэтому safe `fn`; `bench-internals`-гейт; в `SAFE_MUTATORS` с обоснованием «держит лок
— риск живости, не памяти».

Ребёнок (режим по env `SEFER_CON01_CHILD`, паттерн `tests/r8_autonomous_maintenance.rs:202–218`):

- `shard`: тестовый поток аллоцирует `p` (64 байта), передаёт адрес. Поток H:
  `dbg_with_shard_of_locked_for_test(p, || { HELD=1; while !RELEASE { park_timeout(1 ms) } })`.
  Поток E: (1) ждёт `HELD`; (2) инициализирует TLS `Late` (Drop); (3) `std::process::exit(0)`.
  E НЕ должен аллоцировать до `exit` (иначе армируется `GUARD`; если его trim по LIFO пойдёт ДО
  `Late` и снимет маршрут в шарде `p`, на Linux получится ложное зависание: H ждёт `RELEASE`, который
  ставит ещё не исполненный `Late`). Проверка: `Late::drop` печатает, был ли E привязан, через
  пассивный `sefer_alloc::global::tls_heap::current_for_trim()` (`tls_heap.rs:384`, не привязывает).
- `init` (W3, существующие хуки): H вызывает `dbg_set_hold_fallback_init(true)`
  (`src/global/mod.rs:68–70`, `fallback.rs:198–201`) и затем первую fallback-операцию — победитель
  застревает в `INITIALIZING`; `Late::drop` на E снимает удержание (`dbg_set_hold_fallback_init(false)`
  = `RELEASE`) и аллоцирует после `TORN`. Требует, чтобы fallback ещё не был READY (отдельный ребёнок).
  `Late::drop`: `eprintln!("CON01_DTOR_ENTERED")`; снимок `dbg_exit_lock_giveups()` (после фазы B)
  и `AllocCore::dbg_foreign_or_unroutable_frees()` (`totals.rs:26`); `RELEASE=1`;
  `dealloc(p)` (чужой для E → `publish_foreign` → `lookup` → шард `p`); печать дельт и
  `CON01_DTOR_DONE`.
- `fallback`: H — `HeapCore::dbg_with_fallback_for_test(|_| { HELD=1; wait RELEASE })`. E: сначала
  `Late` (до первой аллокации E, чтобы по LIFO `Late` разрушался ПОСЛЕ `GUARD`), затем любая
  аллокация (привязка), затем `exit(0)`. `Late::drop`: маркер, `RELEASE=1`, `Box::new(0u64)` (после
  `TORN` → fallback → W2). Path-activation оракул (правило R30-8): дельта
  `dbg_fallback_lock_acquisitions()` (`fallback.rs:635`) == 1 печатается и проверяется — иначе
  аллокация не пошла в fallback и прогон невалиден (например, если `Late` разрушился до `GUARD`).

Почему `RELEASE` ставит сам деструктор: если ОС НЕ убила H (Linux-порядок S9), H увидит `RELEASE` и
отпустит лок → ребёнок выходит с 0. Если убила (Windows Д1+П3) — отпускать некому → вечное
ожидание. Так тест различает ПОРЯДОК, а не просто «залипший лок» (держи H лок вечно — Linux тоже
зависал бы, и тест ничего не доказывал бы).

Родитель: `Command::spawn` с `Stdio::piped()` для stdout/stderr, потоки-читатели (не допускать
заполнения пайпа), опрос `child.try_wait()` каждые 50 мс до дедлайна 20 с; по дедлайну
`child.kill()` (только свой ребёнок, по его хэндлу) + `wait()`. Вердикты:

| Платформа/дерево | Ожидание |
|---|---|
| Windows, текущее дерево | ТАЙМАУТ + `CON01_DTOR_ENTERED` есть, `CON01_DTOR_DONE` нет → гипотеза ПОДТВЕРЖДЕНА (красный) |
| Windows, текущее дерево, выход 0, `ENTERED` есть | гипотеза ОПРОВЕРГНУТА (держатель не был убит до деструктора) |
| Windows, текущее дерево, выход 0, `ENTERED` нет | П3 неверно (деструкторы выходящего потока не исполняются) — ветки P-trim/P-free/P-alloc на выходе нет |
| Windows, после V2 | выход 0 ≤ 20 с; `shard`: дельта unroutable = 1, giveups = 1; `fallback`: ребёнок завершается abort'ом `handle_alloc_error` (если владелец выберет abort) — тест проверяет именно это и НЕ код 0 |
| Linux/macOS, любое дерево | выход 0; `shard`: unroutable = 0, giveups = 0 (контроль, что тест сам не зависает) |

Нагрузка/флейки: один ребёнок на сценарий, нормальный путь — миллисекунды; 20 с — запас на
холодный CI. В красном состоянии ребёнок крутит одно ядро до убийства (≤ 20 с). Стресс-циклов нет.
CI: строка в job `test windows (production)` (`.github/workflows/ci.yml:2075–2098`) по образцу
r8_os_release_oracle: `--features "production internals bench-internals" --test
con01_exit_stranded_lock -- --test-threads=1` + `grep -F` сентинел «... ok».

Дополнительно (желательно, не обязательно): сценарий `main_return` в отдельном `[[test]]
harness = false` бинаре, чтобы покрыть П1 (выход возвратом из `main`, а не `process::exit`); и
сценарий `scope` для П5 (`thread::scope` + немедленный выход, без хуков — вероятностный).

### §5.2 Пробa детектора (фаза B, до опоры на V2)

В том же файле сценарий `probe`: печатать `process_exit_in_progress()` (через `bench-internals`
наблюдатель) (а) в TLS-деструкторе обычного выхода потока с join → должно быть `false`;
(б) в TLS-деструкторе выходящего потока при `process::exit` → `true`; (в) в atexit-фазе
(необязательно). Плюс в (б) проверить, что поток H к этому моменту мёртв
(`GetExitCodeThread` его хэндла или просто по факту: H пишет маркер раз в 1 мс — после `true`
новых маркеров нет). Если (а) даёт `true` — V2 с этим детектором недопустим (нарушение R2).

### §5.3 Порядок коммитов (префиксы R30-12)

1. `test(global): add Windows exit stranded-lock subprocess oracle (CON-01)` — тест + хук
   `dbg_with_shard_of_locked_for_test` + строка в `verify-dbg-hook-safety.mjs`. Чтобы не коммитить
   красный CI: в этом коммите ожидание для Windows кодируется как характеристика текущего поведения
   (таймаут = ожидаемо, записывается в лог), либо коммит 1 объединяется с коммитом 2. Сырые логи
   фазы A — в `docs/` с отчётом по правилу «верхний вывод опирается на исполненное число».
2. `fix(registry): give up shard-lock waits once the process is exiting` — `process_exit.rs`,
   `shard_lock.rs`, `directory.rs`, `registration.rs`, `error.rs` (если новый вариант).
3. `fix(global): skip teardown trim and fallback waits during process exit` — `fallback.rs`,
   `tls_heap.rs`; переключение ожиданий теста на зелёные.
4. `docs: document the Windows process-exit contract` — README рядом с «Fork safety», rustdoc
   `SeferAlloc`, item 185 → CLOSED/частично, новый остаток W4 отдельной карточкой.
5. `build(ci): run the CON-01 exit oracle on windows-latest`.

Если фаза A опровергнет гипотезу: только коммит 4 в форме «NULL: item 185 закрыт, тест оставлен как
регрессионный» + коммит 1 (тест зелёный на всех ОС) + 5.

## §6 Что нельзя решить без исполнения; вопросы владельцу

Эксперимент (Windows, локально или CI): §5.1 сценарии `shard` и `fallback` на `31db7f17`, по 1
прогону, таймаут 20 с; затем §5.2. Записать сырые stdout/stderr, код выхода, флаги фич, `rustc -V`,
версию Windows; сводный CSV по правилу CLAUDE.md о сырых логах.

Отдельно (опционально, оценка реальной вероятности): пример в `examples/` — N подпроцессов,
detached-потоки в плотном цикле межпоточных `free` блоков главного потока, main возвращается через
случайную задержку; считать таймауты. Это уже измерение, требует сырых логов + CSV.

Вопросы владельцу:
1. Исход для мёртвого держателя fallback-лока на выходе: abort (`handle_alloc_error`, ненулевой код)
   или «escape» в `System` (блок не маршрутизируется; `free` → no-op; `realloc` → копия + утечка
   старого)? Рекомендация исследования — сначала abort (меньше поверхности), escape — отдельной
   задачей, если ненулевой код выхода окажется проблемой.
2. Допустима ли зависимость от ntdll `RtlDllShutdownInProgress` (статус документированности не
   установлен), или требуется документированный механизм (собственный TLS-callback, Н3)?
3. Нужен ли сценарий `main_return` (`[[test]] harness = false`, новая запись в `Cargo.toml`) или
   достаточно `process::exit` из потока?
4. Считать ли П5 (`thread::scope` возвращается до TLS-деструкторов) отдельной находкой для
   собственного `PinnedRunner::run` (feature `pinning`)?

## §7 Проверено и НЕ дало вывода

- CI-история: `gh run list --workflow ci.yml --limit 40` и `gh run view` для run
  `37770561319`, `37681398463`, `37315754670`, `36917358888` — job `test windows (production)` во
  всех просмотренных прогонах `success` (≈8–9 мин), зависаний/отмен не найдено. Вывода нет: если П7
  верно (libtest join-ит потоки тестов), обычный прогон тестов на Windows не попадает в S2–S6.
- Поиск `cfg(windows)`/`target_os = "windows"` в `src/` — 0 совпадений: в корневом крейте нет ни
  одного Windows-специфичного пути, детектора завершения или упоминания `ExitProcess`
  (подтверждает R17-CON-01 «Наблюдено» п.2); вывода о поведении ОС это не даёт.
- `tests/r14_shard_late_tls_teardown.rs:4–9` подтверждает, что проект опирается на LIFO-порядок
  TLS-деструкторов в обычном выходе потока; о порядке на выходе ПРОЦЕССА он ничего не говорит.
- Исходники std/ntdll и документация Microsoft/Rust в этом режиме не читались (вне worktree), поэтому
  все пункты П1–П7 и Н1–Н3 остаются непроверенными.

Новые находки этого исследования (ложноположительное хуже пропуска, поэтому только ГИПОТЕЗЫ):
- **ГИПОТЕЗА, P4 (контракт):** если П5 верно, совет «join-ите потоки перед выходом» (V0) неполон для
  `std::thread::scope` и собственного `PinnedRunner::run`: scoped-потоки могут ещё исполнять
  `AbandonGuard`-trim после возврата `scope`. Опровергается исполнением: сценарий `scope` §5.1 или
  чтение `std::thread::scope` используемой версии.
- **Уточнение к R17-CON-01 (НАБЛЮДЕНО):** критическая секция `remove` включает `System.dealloc`
  (`Block::release`, `directory.rs:650`), т.е. HeapFree под спин-локом; удерживается дольше, чем
  `lookup`. На вероятность S3/S4 влияет в сторону увеличения; величина не измерялась.
