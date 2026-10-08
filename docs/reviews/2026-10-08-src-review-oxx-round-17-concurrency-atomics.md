# Ревью src/ — раунд 17 (oxx) — конкурентность, атомики, порядок памяти, lock-free протоколы

## 1. Охват, инвентарь, ограничения, вердикт

**База:** `453439c123f71a98890408dd0170bd909ad9ecdf` (`docs: close correctness item 175 after green CI and record the red CI of the first push`), HEAD изолированного worktree; `git status` на старте чистый. Это семнадцатый раунд обзоров `src/` (предыдущий — `docs/reviews/2026-10-08-src-review-oxx-round-16.md`, манифест `docs/perf/round-manifests/SRC_REVIEW_R16_MANIFEST.md`). В раунде восемь независимых ревьюеров с разными темами. Тема этого отчёта — конкурентность: все `Ordering::*`, пары acquire/release, CAS и их failure-ordering, ABA, протокол удалённого освобождения (sidecar/bitmap, drain/reclaim, терминальная публикация), гонки владения сегментами, `HeapRegistry` claim/recycle/pick_slot, TLS-heap и `AbandonGuard`, epoch/lock-free/sharded регионы, pinning, ожидания и повторы (lost wakeup, livelock, starvation), счётчики, `Send`/`Sync`, покрытие Loom/TSan.

**Режим: ревью проведено без компиляции и исполнения кода** (приказ владельца «только чтение»). Не запускались `cargo` в любой форме (build/check/test/clippy/doc), `rustc`, Miri, Loom, Kani, TSan и бенчмарки; witness-тесты и мутанты не создавались. Использовались только чтение файлов, grep/rg/awk/wc и git в режиме чтения (`rev-parse`, `status`, `log`, `show`, `diff --stat`). Поэтому каждый вывод помечен как **SOURCE-CONFIRMED** (установлен чтением исходника и истории) или **ГИПОТЕЗА**; класса «исполненный witness» в отчёте нет.

**Ревьюер:** oxx (Claude Opus 5.5, effort=max), один контекст, без суб-агентов. Исходники не правились; единственный созданный файл — этот отчёт.

**Инвентарь (пересчитан скриптом, приложение A):** 168 Rust-файлов в `src/`, 43 325 физических строк; 17 059 непустых строк, не начинающихся с `//`; 24 303 строк-комментариев; 1 963 пустых. По группам: `src/alloc_core/` 88 файлов / 24 802 строки; `src/registry/` 46 / 10 413; `src/global/` 18 / 3 809; `src/concurrent/` 14 / 3 554; `src/lib.rs` + `src/kani_proofs.rs` 2 / 747. Разница с R16 (43 317 строк) — шесть коммитов после базы R16 (`git log 6a0d47f6..453439c1 -- src/`: `486f5ace`, `32cacc97`, `222e913d`, `9df9f6b8`, `c8b9344a`, `b707d196`; 22 файла, +130/−122). Unsafe-инвентарь командой из CLAUDE.md: в `src/` 21 модульный (tier 1) и 83 item-уровня (tier 2) в 30 файлах; в `crates/` 6 + 20 — как в R16.

**Атомарная перепись `src/`** (вхождения токенов, включая упоминания в комментариях; приложение A): `Ordering::Relaxed` 279, `Acquire` 109, `Release` 47, `AcqRel` 24, `SeqCst` 5 (все пять — test-only инъекции: `src/global/fallback.rs:650, 704, 712, 739`, `src/registry/bootstrap/ensure.rs:246`); `compare_exchange(` 24, `compare_exchange_weak(` 3, `fetch_update(` 3, `.swap(` 9, `fetch_or(` 2, `fetch_add(` 118, `fetch_sub(` 5, `fetch_max(` 2, `fetch_min(` 1, `fetch_and`/`fetch_xor` 0; `fence`/`compiler_fence` 0. Атомарные типы упоминаются в 55 файлах. Production-циклы ожидания (`spin_loop`/`yield_now`) есть в четырёх местах: `src/registry/segment_route/shard_lock.rs:50–61`, `src/global/fallback.rs:310–325` и `:471–484`, `src/global/exact_object/exact_shard.rs:185–191`; кроме них — test-only `fallback.rs:199–201` и loom-only `src/registry/bootstrap/loom_shim.rs:178`.

**Метод.**

1. Прочитаны правила проекта (`CLAUDE.md`) и оба индекса открытых пунктов: `docs/perf/OPEN_ITEMS.md`, а также `docs/CORRECTNESS_OPEN_ITEMS.md` с `docs/correctness-open-items/{ACTIVE,TRACKED_*}.md` (current-state карточки; полностью — пункты по теме: correctness 17, 23, 152, 154, 160, 162, 164, 166, 167, 171; perf 70, 78, 82 и архивная запись 80). Кроме того прочитаны протокольные документы `docs/CROSS_THREAD_STATE_MACHINES.md` §9 (текущий якорь), `docs/REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md` §1–3, заголовок `docs/RACE_DRAIN_RECLAIM.md` (RESOLVED, исторический) и отчёт R16.
2. **Полное чтение** (код и комментарии): `registry/segment_route/{directory,shard_lock,registration,pin,large_state,small_sidecar,terminal_publication_gate}.rs`; `alloc_core/segment/remote_bitmap/{sidecar_bitmap,bitmap_scan,bitmap_cut}.rs` и `.../sidecar_bitmap/leaf_classes.rs`; `alloc_core/large/reservation_state.rs`; `alloc_core/segment/segment_header/{terminal_words,segment_header_gen_table}.rs`; `alloc_core/alloc_core/sidecar_drain.rs`; `alloc_core/small/alloc_core_small_reclaim.rs`; `registry/heap_core_xthread/routing.rs`; `registry/heap_registry/{claim,stack,maintenance}.rs`; `registry/bootstrap/saturation.rs`; `registry/heap_core/state/ownership.rs`; `global/{fallback,maintenance_service}.rs`; `global/sefer_alloc/{global_alloc,maintenance}.rs`; `global/exact_object/{exact_shard,narrow,exact_table}.rs`; `crates/once-ptr-cell/src/imp.rs` (как зависимость реестра).
3. **Выборочно, по диапазонам:** `global/tls_heap.rs` (110–550 и модульный doc по ключевым словам), `registry/heap_slot.rs` (80–300), `concurrent/epoch/epoch_region.rs` (300–450, 545–605, `Drop`), `concurrent/epoch/hand.rs` (`read_with` 232–300, `install` 339–352, `try_evict_at` 395–517, `Send`/`Sync` 556–582), `concurrent/sharded/sharded_region.rs` (160–290, 474–526, 690–700), `concurrent/lock_free/lock_free_region.rs` (220–310, 380–440), `alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs` (255–305, 630–677), `alloc_core/segment/segment_table/segment_table_impl.rs` (330–360, 385–537, 605–701), `route_slots.rs` (150–230), `registry/heap_core/alloc/hot.rs` (25–160, 314–340), `registry/heap_core/alloc/batch.rs` (вызовы `bump_gen_on_issue`), `alloc_core/small/alloc_core_small/{alloc_core_small_impl.rs (435–465), reserve.rs (420–463), find_segment.rs (130–172)}`, `alloc_core/alloc_core/bootstrap.rs` (1–30, 160–215), `segment_header_impl.rs` (136–165, 296–360, 748–764), `descriptors.rs` (315–339), `platform/node.rs` (405–440), `registry/heap_core/free/dealloc_own_base.rs` (130–330), `registry/bootstrap/registry.rs` (140–175), `global/sefer_alloc/core.rs` (330–375), `alloc_core/large/alloc_core_large.rs` (712–747), `lib.rs` (120–265).
4. **Скрининг всех 168 файлов:** перепись выше. Все сайты `compare_exchange*`, `fetch_update`, `swap`, `fetch_or`, `fetch_sub`, `fetch_max/min` и все циклы ожидания прочитаны в контексте. `fetch_add` (118) разделены grep-ом на протокольные и диагностические/статистические, протокольные прочитаны. Проверены все `unsafe impl Send/Sync` и все упоминания удалённых механизмов (ring/overflow, `dirty_by_class`, `reclaim_offset`, X7).
5. **Сверка покрытия:** 13 корневых `tests/loom_*.rs` и два крейтовых loom-набора сопоставлены с CI-джобами (`.github/workflows/ci.yml:3093–3216`), а состав TSan-джобы (`:3218–3293`) — с тестами, которые гоняют те же протоколы.

**Не прочитаны вглубь (только скрининг):** `alloc_core/platform/{os,numa,sidecar_stats,size_classes}.rs`; `alloc_core/large/*` вне указанных строк; `alloc_core/small/alloc_core_small/{directory,dealloc}.rs`; `alloc_core_small_magazine.rs`; `segment_directory/*`; `config/*`; все `*_diag*.rs`; `registry/heap_core/{core,free/dealloc,free/realloc}.rs`; `registry/heap_registry/counters.rs`; `registry/bootstrap/{ensure,chunk,mod}.rs`; `concurrent/pinning.rs`; `lock_free_page_table.rs`; `kani_proofs.rs`. Утверждения «не найдено» относятся только к прочитанному.

**Не запускалось — режим только чтение:** любые сборки и тесты, Miri, Loom, Kani, TSan/ASan, MSRV, бенчмарки/iai, witness и мутанты, CI (история GitHub Actions не просматривалась).

**Вердикт.** Одна **ГИПОТЕЗА** уровня P2 при подтверждении: R17-CON-01, зависание выхода процесса на Windows в неограниченном spin-ожидании блокировки, которую держал поток, уже завершённый `ExitProcess`. Пять находок **P4, SOURCE-CONFIRMED** (R17-CON-02…06):

- write-only таблица поколений под `hardened`;
- устаревшая карточка correctness 160 при уже существующей loom-модели;
- три пробела покрытия Loom/TSan;
- глобальная spin-сериализация всех `dealloc` под `exact-object-proto`;
- устаревшие межпоточные контракты в doc-комментариях.

Новых P0/P1 и подтверждённых P2/P3 в прочитанной области не найдено. При чтении согласованы ordering-протоколы терминальной публикации, route-директории, lease-автомата слотов, TLS-teardown, fallback и epoch-регионов (§5). Это не доказательство отсутствия ошибок, не release GO и не perf GO: ничего не исполнялось.

## 2. Находки

Шкала как в R16:

- **P2** — реальный дефект корректности, утечка или deadlock в нормальной работе;
- **P3** — ограниченный дефект корректности, ресурса или диагностики без нового production memory-safety эксплойта;
- **P4** — вводящий в заблуждение контракт, пробел покрытия или проблема поддерживаемости без установленного runtime-сбоя.

Классы доказательства:

- **SOURCE-CONFIRMED** — путь или противоречие установлены по исходнику, без исполнения;
- **ГИПОТЕЗА** — механизм выведен, но зависит от непроверенного внешнего допущения и не исполнялся.

| ID | Severity | Класс доказательства | Достижимость | Суть |
|---|---|---|---|---|
| R17-CON-01 | P2 при подтверждении | ГИПОТЕЗА (путь в `src/` — SOURCE-CONFIRMED; поведение ОС/std при выходе — внешнее допущение) | Windows; `production` и любой `alloc-global`; безопасный код: возврат из `main` или `std::process::exit`, пока другие потоки работают с аллокатором | Пути TLS-деструктора и поздних free/alloc выходящего потока ждут без ограничения в `ShardLock::lock`, `LockGuard::acquire` и на fallback-`INIT_STATE`. Владелец блокировки может быть уже завершён `ExitProcess`, и тогда процесс не завершается |
| R17-CON-02 | P4 | SOURCE-CONFIRMED | только opt-in `hardened` (не в `production`) | После удаления ring таблица поколений стала write-only. Атомарный RMW на каждую выдачу и 256 KiB побайтного обнуления на каждый новый Small/Primordial сегмент — без единого production-читателя; документация описывает удалённого потребителя |
| R17-CON-03 | P4 | SOURCE-CONFIRMED | индекс открытых пунктов | Карточка correctness 160 утверждает «no loom model exists». Модель push∥drain по Relaxed-hint и очереди есть (`tests/loom_r11_epoch_false_full.rs`, `692b9a93`) и с негативным контролем стоит в CI |
| R17-CON-04 | P4 | SOURCE-CONFIRMED | тестовая инфраструктура | Пробелы покрытия: (a) нет модели lookup∥unlink∥last-pin для `RouteDirectory`; (b) TSan не гоняет тесты maintenance-воркера и конкуренции за fallback; (c) комментарий CI приписывает fallback-протоколу покрытие real-type `OncePtrCell` |
| R17-CON-05 | P4 | SOURCE-CONFIRMED | только opt-in прототип `exact-object-proto` | Как только в шарде есть живой exact-объект, каждый `GlobalAlloc::dealloc` любого layout'а с адресом в этом шарде берёт один из 64 глобальных spin-lock'ов без уступки |
| R17-CON-06 | P4 | SOURCE-CONFIRMED | документация `src/` (новые доказательства для correctness 154) | Doc-комментарии описывают несуществующих межпоточных читателей и публикации (`PerClassDirty`, F-3 free-path `slot_or_none`, межпоточное чтение `owner_state`/`kind`); `Release`-store назван «MFENCE-эквивалентом на x86» |

### R17-CON-01 — (ГИПОТЕЗА) Windows: выход процесса может зависнуть на блокировке, владелец которой уже завершён `ExitProcess`

**Места.**

- Ожидания без ограничения и без обнаружения гибели владельца:
  - `src/registry/segment_route/shard_lock.rs:48–66` — `ShardLock::lock`: цикл `compare_exchange_weak(false, true, Acquire, Relaxed)`, 64 итерации `spin_loop`, затем бесконечный `yield_now`;
  - `src/global/fallback.rs:469–486` — `LockGuard::acquire` той же формы, `LOCK_TIGHT_SPINS = 64` на :443;
  - `src/global/fallback.rs:310–325` — проигравший ждёт, пока `INIT_STATE == STATE_INITIALIZING`.
- Пути выходящего потока, которые берут эти блокировки:
  1. **TLS-деструктор.** `src/global/tls_heap.rs:197–226` (`AbandonGuard::drop`: `mark_local_torn` :214, `lease.core().trim_for_recycle()` :221, `drop(lease)` :224) → `src/registry/heap_core/state/ownership.rs:108–115` (`drain_sidecar_ingress` :113, `trim_cold_retention` :114) → `:140–156` (`drain_small_pool` :150, `release_empty_current_small_for_trim` :151) и `src/alloc_core/alloc_core/sidecar_drain.rs:241–246, 249–250` (`release_or_pool_empty_segment`, `reclaim_large_segment`). Дальше — либо `SegmentTable::recycle` (`src/alloc_core/segment/segment_table/segment_table_impl.rs:614–701`, `routes.remove(slot_id)` :677; вызовы `alloc_core_small_pool_impl.rs:303, 646, 676`), либо `SegmentTable::unregister` (`:489–537`, `routes.remove` :516; вызов из `src/alloc_core/large/alloc_core_large.rs:740`). Оба ведут в `RouteSlots::remove` (`src/alloc_core/segment/segment_table/route_slots.rs:203–212`; `slot.take()` роняет `RouteRegistration`) → `Drop for RouteRegistration` (`src/registry/segment_route/registration.rs:78–82`) → `RouteDirectory::remove` (`src/registry/segment_route/directory.rs:891–897`, `shard.lock()` :894).
  2. **Позднее освобождение после `TORN`.** `src/global/sefer_alloc/global_alloc.rs:68–79` → `current_for_dealloc` (`tls_heap.rs:321–336`; `TORN` → `ForeignNoBind` :332) → `HeapCore::publish_foreign` (`src/registry/heap_core_xthread/routing.rs:26–74`) → `RouteDirectory::lookup` (`directory.rs:868–889`, захват :871).
  3. **Позднее выделение после `TORN`.** `global_alloc.rs:40–47` → `current_for_alloc(_with_config)` (`tls_heap.rs:256–281`, `:352–363`; `TORN` → `Fallback`) → `SeferAlloc::with_fallback_heap` (`src/global/sefer_alloc/core.rs:354–366`) → `fallback::with_heap(_config)` (`fallback.rs:336–357`, `:362–378`) → `heap_ptr_impl` (цикл :310–325) и `LockGuard::acquire()` (:351, :370).
- Кто может держать эти блокировки:
  - любой поток во время `lookup`, то есть при каждом межпоточном освобождении;
  - любой поток при `register`/`remove` маршрута: новые и освобождаемые Small-сегменты, каждое Large-выделение (при повторном использовании из кэша тоже регистрируется свежий дескриптор, `docs/CROSS_THREAD_STATE_MACHINES.md` §9.1);
  - пользователи fallback (TLS-teardown, насыщенный реестр);
  - maintenance-воркер: каждые 10 мс он держит fallback `LOCK` на весь `background_maintenance_step` fallback-кучи (`src/global/maintenance_service.rs:147–150` → `ownership.rs:119–138`: ограниченный drain плюс полный `trim_cold_retention` с освобождениями ОС и снятием маршрутов). Модульный doc воркера: «OS process exit stops the worker» (`maintenance_service.rs:10–13`).

**Наблюдено (SOURCE-CONFIRMED).**

1. Все три ожидания бесконечны: нет таймаута, отказа после N попыток, флага завершения процесса или проверки, жив ли владелец. Состояние `true`/`INITIALIZING` снимает только сам владелец.
2. TLS-деструктор выходящего потока по замыслу делает полный холодный trim со снятием маршрутов. Поздние free/alloc после `TORN` идут через shard-lock и fallback-`LOCK`. Ни в `src/`, ни в README нет упоминаний `ExitProcess`/`DLL_PROCESS_DETACH` (grep).
3. Тот же механизм для POSIX `fork()` в проекте уже признан и документирован: README «Fork safety» (`README.md:1698–1708`: «The fallback lock, primordial initialization and registry/directory locks can be inherited while held by a thread that does not survive the fork. A child allocation/free can then deadlock»), correctness item 152. Там это следствие неподдерживаемого действия пользователя. На Windows то же состояние возникает при обычном выходе процесса.

**Внешние допущения.** Не проверены: исходники std и ОС вне репозитория в этом режиме не читались.

- **A.** `ExitProcess` (в него приходят и возврат из `main` через CRT `exit`, и `std::process::exit`) сначала принудительно завершает все остальные потоки процесса, не выполняя их код. Только затем на выходящем потоке выполняются `DLL_PROCESS_DETACH` и TLS-callbacks.
- **B.** std на Windows выполняет TLS-деструкторы выходящего потока из этого TLS-callback. По памяти ревьюера, документация `std::thread::LocalKey` (раздел о платформенном поведении) говорит: при выходе процесса на Windows TLS-деструкторы могут выполняться только на потоке, вызвавшем выход, потому что остальные потоки могут быть принудительно завершены.

**Трасса (выведено; Windows, `production`).**

| Шаг | Поток W (рабочий, межпоточные `dealloc`) | Поток M (main) |
|---|---|---|
| 1 | `dealloc` блока чужой кучи → `publish_foreign` → `lookup` → CAS `locked: false → true` шарда S успешен | — |
| 2 | внутри критической секции (`find`, `pin_from_array`) | `main` возвращается → CRT `exit` → `ExitProcess` |
| 3 | завершён ОС; `locked` шарда S навсегда `true` | — |
| 4 | — | `DLL_PROCESS_DETACH` → TLS-деструкторы → `AbandonGuard::drop` → `trim_for_recycle` → `drain_small_pool` → `recycle` сегмента, чей ключ попадает в S → `RouteDirectory::remove` → `ShardLock::lock(S)` |
| 5 | — | `compare_exchange_weak` всегда неуспешен → 64 × `spin_loop`, далее `yield_now` в бесконечном цикле; процесс не завершается, одно ядро занято |

Другие варианты:

- **B.** W — maintenance-воркер внутри `fallback::try_with_heap` (держит fallback `LOCK`). После его завершения любой поздний `alloc` на M (например, другой TLS-деструктор выделяет память после `TORN`) уходит в `with_heap` → `LockGuard::acquire` → бесконечный цикл.
- **C.** W завершён во время однократной инициализации fallback (`INITIALIZING`) — маловероятно.
- **D.** Поздний `dealloc` на M после `TORN` → `lookup` того же шарда S.

Вероятность на один выход мала: убитый поток должен находиться внутри короткой критической секции. Но каждое попадание — необратимое зависание. Наиболее подвержены программы, которые завершаются при работающих потоках (detached-пулы, `process::exit` из обработчика ошибки), и процессы с запущенным maintenance-воркером и активным fallback. Windows — основная платформа разработки проекта (`ci.yml:2069`).

**Почему не Linux/macOS (выведено).** Там `exit()` выполняет TLS-деструкторы вызывающего потока, пока остальные потоки продолжают работать, и занятую блокировку отпустит её владелец. Опасен только `fork` (item 152).

**Когда гипотеза ложна.**

- Неверно A или B: например, std не выполняет TLS-деструкторы выходящего потока при `DLL_PROCESS_DETACH` или выполняет их до завершения остальных потоков. Тогда ветка 1 исчезает. Ветки 2–3 тоже требуют, чтобы код Rust исполнялся на выходящем потоке после гибели остальных.
- Выходящий поток не привязал кучу реестра и не делает free/alloc при teardown.

**Рекомендация.** Решение — за владельцем. Возможные направления:

1. Не делать уборку при завершении процесса: если процесс завершается, `AbandonGuard::drop` пропускает `trim_for_recycle` (память всё равно возвращает ОС).
2. На путях teardown и поздних free/alloc заменить ожидание ограниченной попыткой с явным исходом «утечь/отложить».
3. Дописать контракт рядом с README «Fork safety».

Как надёжно обнаружить завершение процесса на Windows — отдельный вопрос, требующий проверки; здесь способ не предписывается.

**Oracle исправления.** Windows-only тест с дочерним процессом по образцу `tests/r8_autonomous_maintenance.rs`: сценарий в подпроцессе, родитель ждёт с таймаутом и по таймауту убивает только своего ребёнка по PID.

- **Детерминированный вариант.** Вспомогательный поток берёт shard-lock известного ключа через новый `bench-internals`-хук и паркуется. У main есть пустой Small-сегмент в пуле с маршрутом в этом шарде (или удерживаемый указатель для освобождения после `TORN`). Затем main возвращается. До исправления — таймаут, после — код 0. Если на неизменённом дереве ребёнок завершается нормально, гипотеза опровергнута (ложно A или B).
- **Вероятностный вариант, без нового хука.** N подпроцессов, в которых рабочие потоки крутят межпоточные `dealloc`, пока main возвращается; считается число таймаутов.

### R17-CON-02 — `hardened`: после удаления ring таблица поколений стала write-only

**Места.**

- `src/alloc_core/segment/segment_header/segment_header_gen_table.rs:50–67` — `gen_at`; в `src/` нет вызовов, кроме re-export в `segment_header_impl.rs:763`.
- `:93–110` — `bump_gen`, `fetch_add(1, Relaxed)` на :109.
- `:148–158` — `init_gen_table_in_place`: побайтная запись нуля по `GEN_TABLE_FOOTPRINT = SEGMENT / MIN_BLOCK` = 262 144 байт (`segment_header_impl.rs:161`).
- Писатели: `src/registry/heap_core/alloc/hot.rs:74–80` (`bump_gen_on_issue`; вызовы `hot.rs:154, 382, 509`), `src/registry/heap_core/alloc/batch.rs:141, 184`, `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:455–463` (`pop_free`).
- При создании сегмента таблица под `hardened` обнуляется безусловно (`src/alloc_core/small/alloc_core_small/reserve.rs:446–454`, `src/alloc_core/alloc_core/bootstrap.rs:203–211`). `AllocBitmap`/`MagazineBitmap` обнуляются только под `#[cfg(miri)]`, потому что свежие страницы ОС и так нулевые (`reserve.rs:425–438`, `bootstrap.rs:170–194`).

**Наблюдено.**

- Читателя таблицы в `src/` нет: `gen_at` используют только тесты `tests/regression_gen_table_layout.rs`, `tests/regression_gen_table_lifecycle_seams.rs` и `tests/regression_r2_3_gen_table_index_guard.rs`. Файла `tests/regression_gen_wrap_boundary.rs`, указанного в `segment_header_impl.rs:756–761`, нет.
- Документация описывает удалённого потребителя:
  - «remote reads Relaxed (also Ф3)» — `segment_header_gen_table.rs:11–16`, `src/alloc_core/platform/node.rs:419–426`;
  - «the remote-free drain compares this against the generation stamped in the ring note» — `segment_header_gen_table.rs:26–29`, также `:69–80`;
  - «the hardened remote-free staleness guard» — `segment_header_impl.rs:142–143`;
  - «Cross-thread (ring-drain) small-path reclaim — `reclaim_offset` and its `hardened` generation-checked variant» — `src/alloc_core/small/mod.rs:31–32`. В модуле есть только `reclaim_sidecar_record`, без проверки поколения (`alloc_core_small_reclaim.rs:21–68`);
  - `src/registry/heap_core/free/dealloc_own_base.rs:179–191` ссылается на `reclaim_offset_checked` (в `src/` нет), на тест `residual_xthread_double_free_no_corruption` (в `tests/` нет) и на «task X7 (hardened, generational ring entry)» как на будущее исправление;
  - модульный doc `tests/regression_gen_table_lifecycle_seams.rs:7–30` описывает stamp-at-remote-free/compare-at-drain.
- README уже фиксирует обратное: «the old X7 ring generation path was removed with the ring» (`README.md:1426`; также `:1363–1364`).

**Выведено.** Под `hardened` каждая выдача из magazine (и `pop_free` у прямых пользователей `AllocCore`) выполняет locked RMW (`lock xadd` на x86-64) над байтом, который никто не читает. Каждый новый Small/Primordial сегмент платит 256 KiB побайтной записи (64 страницы резидентной памяти), даже если в нём нарезано всего несколько блоков. Обоснование обнуления — «иначе Relaxed-load по никогда не записанной ячейке — UB (miri-confirmed)» (`bootstrap.rs:198–199`, `reserve.rs:442–444`) — описывает ту же miri-ситуацию, ради которой битмапы обнуляются только под `#[cfg(miri)]`. Ошибкой корректности это не является: писатель один (владелец), межпоточной опасности нет. Накладные расходы не измерены.

**Рекомендация.** Либо удалить таблицу поколений из `hardened` (layout `segment_header_layout.rs`, `layout_asserts.rs`, тесты `regression_gen_*`), либо, если потребитель ещё планируется, сделать обнуление только для `cfg(miri)`, заменить `fetch_add` на load+store (писатель один) и исправить документацию.

**Oracle:**

1. Статический: в `src/` нет записи в таблицу без читателя.
2. Если таблица остаётся — RSS свежего Small-сегмента под `hardened` с одним нарезанным блоком (при ленивом обнулении ожидается −252 KiB) и iai-Ir выдачи из magazine под `hardened`. Судья — на `HeapCore::alloc` (правило entry-point из CLAUDE.md).

### R17-CON-03 — карточка correctness 160 устарела: loom-модель существует и стоит в CI

**Места.**

- Карточка: `docs/correctness-open-items/TRACKED_misc.md:85–90` (Status OPEN; «no loom model exists for this hint»; Next trigger — «a directed loom model of push versus drain over the hint and the queue»).
- Модель: `tests/loom_r11_epoch_false_full.rs:1–123` (добавлена коммитом `692b9a93` от 2026-10-01, `fix: inspect remote capacity before epoch insert reports full`).
- CI: `.github/workflows/ci.yml:3175–3183` — джоба `loom-experimental` с sentinel'ами `completed_enqueue_cannot_be_hidden_by_negative_hint` и `#[should_panic]`-контроля `old_hint_only_drain_has_a_negative_counterexample`.
- Real-type тест: `tests/r11_epoch_false_full.rs:1–58`.
- Код: `src/concurrent/epoch/epoch_region.rs:312–396` (`drain_remote_free`: Relaxed-hint :338–344, `store(false)` под мьютексом очереди :366–367), `:423–447` (`insert`, принудительный drain при пустом free-list :433–437), `:583–595` (push под мьютексом, затем `store(true, Relaxed)`).

**Наблюдено.** Модель повторяет протокол строка в строку:

| Модель | Код |
|---|---|
| `enqueue_remote` (:32–36) | `remote_evict` (:583–595) |
| `drain` (:38–54) | `drain_remote_free` (:338–376) |
| `insert` (:56–63) | `insert` (:427–438) |

Четыре теста: завершённый enqueue не скрывается отрицательным hint; негативный контроль показывает, что старый drain только по hint ошибается; перекрывающийся enqueue не теряется; ложный или лишний hint безвреден.

**Выведено.** Триггер карточки по свойству безопасности выполнен: нет ни потерянного индекса, ни ложного «full» после завершённого enqueue. Остаются граница задержки для перекрывающегося enqueue, которую код прямо не обещает (`epoch_region.rs:335–337`), и обычный остаток shadow-модели (модель — не реальный `EpochRegion`). Дефект ложного «full», найденный R11 уже после заведения карточки, исправлен тем же коммитом. В нынешнем виде карточка нарушает правило CLAUDE.md, по которому индексы отражают текущее состояние.

**Рекомендация.** Обновить карточку 160: Status → CLOSED (или сузить до «shadow-модель плюс real-type тест `tests/r11_epoch_false_full.rs`; latency не обещается») и перенести нарратив в `RESOLVED.md`. **Oracle:** карточка ссылается на файл модели и CI-sentinel, lookup-таблица `docs/CORRECTNESS_OPEN_ITEMS.md` согласована с ней.

### R17-CON-04 — пробелы покрытия Loom/TSan (не ошибки)

**(a) Нет модели lookup∥unlink∥last-pin для `RouteDirectory`.** `tests/loom_r11_small_sidecar.rs:16–62` начинает с `refs = 2` («registration plus producer pin», :21) и не моделирует гонку трёх операций: получение pin под shard-lock (`directory.rs:868–889`, `pin_from_array` :141–157), снятие маршрута (`:891–897`) и последний `EntryHandle::drop` владельца (`:210–225`). Real-type тесты (`tests/r6_route_directory.rs:57–106`) упорядочивают шаги каналами. Конкурентно этот путь исполняется только вероятностно, в TSan-шаге `production`. По чтению протокол корректен (§5); это пробел модели, а не дефект.

**(b) TSan не гоняет maintenance-воркер и конкуренцию за fallback.** Шаги `ci.yml:3242–3250` и `:3280–3293` запускают `race_repro`, `race_norecycle`, `global_alloc_mt`, `tls_heap_teardown_ordering_stress`, `regression_percounter_perheap_aggregation`, `regression_realloc_xthread_stamp` и `stress_concurrent_boundaries`. В них не входят `tests/r8_autonomous_maintenance.rs`, `tests/r9_bounded_background_maintenance.rs`, `tests/r3_1_fallback_remote_free.rs`, `tests/r11_ph5c_registry_saturation_fallback.rs` и `tests/r8_terminal_global.rs`, хотя набор TSan-шага (`production internals bench-internals`) удовлетворяет их `#![cfg]`. Значит, lease `FREE → MAINTENANCE` воркера, его fallback try-lock и одновременные продюсеры ни разу не исполнялись под детектором гонок. Автомат lease покрыт только моделями (`tests/loom_r8_maintenance_lease.rs`, `tests/loom_r11_ph4a_heap_lease.rs`).

**(c) Комментарий CI приписывает fallback-протоколу real-type покрытие.** По `ci.yml:3101–3108`, четыре shadow-модели, включая `loom_fallback_init`, «were collapsed into ONE suite that model-checks the REAL `once_ptr_cell::OncePtrCell` type». Но `src/global/fallback.rs` не использует `OncePtrCell` (grep), а сообщение коммита `63991cc3` прямо говорит, что `global/fallback.rs::heap_ptr` «did NOT map cleanly onto the cell and were deliberately left inline». Добавленный позже откат `InitStateGuard` (`fallback.rs:541–572`, R34-17) не моделировался никогда. Correctness item 17 (`TRACKED_verification_coverage.md:115–118`) уже принимает отсутствие miri/loom/kani для `global::fallback` как остаточный риск. Новое только то, что комментарий CI противоречит этому статусу; пункт 17 не переоткрывается.

**Рекомендация.**

- (a) Loom-модель на три потока: lookup+pin под моделируемым lock'ом, remove плюс drop владельца, drop pin. Утверждения: запись освобождается ровно один раз и никогда — при живом pin. Негативный контроль — инкремент вне lock'а.
- (b) Добавить пять тестов в TSan-шаг `production`, ограничив число потоков, как у `stress_concurrent_boundaries`.
- (c) Исправить комментарий `ci.yml:3101–3108`.

**Oracle:** для (a) негативный контроль красный, модель зелёная; для (b) лог TSan-джобы содержит имена пяти бинарников.

### R17-CON-05 — `exact-object-proto`: каждый `dealloc` проходит через один из 64 глобальных spin-lock'ов

**Места.**

- `src/global/sefer_alloc/global_alloc.rs:62–67` — `ExactNarrow::try_dealloc` вызывается до маршрутизации для каждого ненулевого указателя любого layout'а.
- `src/global/exact_object/narrow.rs:63–91` — `try_dealloc` → `ExactTable::take` (:79).
- `src/global/exact_object/exact_table.rs:6–8, 16–18, 37–39` — 64 шарда по младшим битам хэша адреса.
- `src/global/exact_object/exact_shard.rs:240–265` — `take`: проверка `live_hint` (:241), затем `acquire()` (:244) и пробирование таблицы.
- `:184–192` — `acquire`: только `compare_exchange_weak` + `spin_loop`, без `yield`.
- `:45–77` — `rehash` вызывает `System.alloc_zeroed`/`dealloc` под захваченным shard-lock'ом (вызов из `insert`, :99).

**Наблюдено.** Быстрый путь `live_hint` пропускает lock, только когда шард пуст. Как только в шарде есть хотя бы один живой exact-объект, любой `GlobalAlloc::dealloc`, чей адрес хэшируется в этот шард, берёт spin-lock шарда и пробирует таблицу — неважно, узкий это объект или нет. Уже при нескольких сотнях живых узких объектов непустыми оказываются почти все 64 шарда, и практически каждое освобождение в процессе проходит через одну из 64 глобальных блокировок.

**Выведено.** При конкуренции и переподписке ядер вытесненный держатель (особенно внутри системных вызовов `rehash`) оставляет ожидающих крутиться целый квант без уступки: CPU сжигается впустую, возможна инверсия приоритетов. В обычном пути освобождения появляется глобальная точка сериализации. Протокол упорядочения памяти при этом корректен (§5).

**Связь с ранее известным.** R13 (`docs/reviews/2026-10-06-src-review-sol-round-13.md:158`) уже отметил spin без уступки и O(cap)-rehash под lock'ом как кандидатов на CPU/tail-latency. Новое: под эту конкуренцию попадают не только узкие освобождения, а **все**, потому что поиск в таблице идёт до маршрутизации для каждого указателя.

**Рекомендация.** Если прототип будет развиваться:

- уступка/backoff, как в `ShardLock`;
- фильтр `ExactNarrow::is_narrow(layout)` перед `try_dealloc`. Для корректного использования это эквивалентно: exact-объект выделяется только с узким layout'ом, а контракт `dealloc` требует тот же layout. При нарушении контракта исход меняется с `fatal` на утечку через `publish_foreign`, поэтому нужно решение владельца.

**Oracle:** `bench-internals`-счётчик захватов shard-lock'а при неузких освобождениях на смешанной нагрузке. До исправления он равен числу неузких освобождений в непустые шарды, после фильтра — 0.

### R17-CON-06 — устаревшие межпоточные контракты в doc-комментариях `src/` (новые доказательства для correctness 154)

**Места и наблюдено** (каждая строка перечитана на базе):

1. `src/alloc_core/platform/sidecar.rs:64–93` и `src/alloc_core/platform/mod.rs:16–17` описывают `PerClassDirty` (`alloc_core::dirty_by_class`) как структуру, которая «is published CROSS-THREAD (any thread's remote free can be the first to materialise it)» через `OncePtrCell`. В `src/` нет ни модуля, ни типа: grep находит только упоминания в комментариях. То же в `src/alloc_core/large/large_cache_extended.rs:44, 50–51`, `src/alloc_core/alloc_core/alloc_core_impl.rs:349–350`, `src/alloc_core/small/mod.rs:28`, `src/alloc_core/small/alloc_core_small/mod.rs:17` и `src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs:283–286` (там же — несуществующий `registry::heap_slot::DIRTY_BITMAP_WORDS`). Tripwire `tests/no_stale_doc_references.rs:2884–2892` проверяет эти имена только в `alloc_core/mod.rs` и `lifecycle.rs`.
2. `src/registry/bootstrap/registry.rs:144–168` (`slot_or_none`): «Fallible variant … for claim and free paths» и «F-3 context: the two production callers — `resolve_dirty_bit_target` and `resolve_heap_overflow` in `heap_core_xthread` — read `owner_id` from *foreign* segment memory». Обеих функций нет. Единственный production-вызов — `claim_impl` (`src/registry/heap_registry/claim.rs:213`), а межпоточное освобождение вообще не читает память сегмента (`routing.rs:26–74`).
3. `src/alloc_core/segment/segment_header/segment_header_impl.rs:306–314` говорит, что `owner_state` «read by cross-thread free routing… Cross-thread readers access it through the dedicated `owner_state_atomic` view… because a plain struct-field read would race a concurrent owner-stamp store»; `:349–350` — что `kind` «Read on every cross-thread dealloc-routing dispatch». Это противоречит doc'у `magic` в той же структуре (`:318–321`: «the cross-thread foreign-free path also reads no header bytes») и `src/alloc_core/segment/segment_header/descriptors.rs:316–320` («Foreign free routing resolves the owning heap from the route descriptor, not from this field»), хотя `descriptors.rs:327–328` всё ещё называет вызывающим «cross-thread free routing». Фактически `owner_state` пишет и читает только владелец (`src/registry/heap_core/state/ownership.rs:59–60, 75–82`) и диагностика через `&HeapCore` владельца (`src/registry/heap_core/diag/queries.rs:35–36`).
4. `ownership.rs:29–30`: fast path стампа возвращается «with NO Release-store (the expensive part on x86 — an `MFENCE`-equivalent)». На x86-64 `store(Release)` компилируется в обычный `MOV` (TSO); барьер или `XCHG` нужен только для `SeqCst`-store. Реально экономится запись в строку заголовка, а не барьер. При настройке ordering'ов такая формулировка вводит в заблуждение.
5. `segment_header_impl.rs:327, 333` (doc `live_count` называет `reclaim_offset`) и `src/alloc_core/small/mod.rs:31–32` (ring-drain и generation-checked variant): сегодня это `reclaim_sidecar_record` без проверки поколения (пересекается с CON-02).
6. `src/alloc_core/segment/segment_header/terminal_words.rs:36`: `#[allow(dead_code)] // Pending/Consuming belong to the next ingress stage.` Но `Pending`/`Consuming` уже production-состояния дескриптора (`src/alloc_core/large/reservation_state.rs:42–73` через `src/registry/segment_route/large_state.rs:25–32`).
7. `src/lib.rs:246`: «`registry::heap_slot` — `Sync`/`Send` impls». Есть только `unsafe impl Sync` (`src/registry/heap_slot.rs:283`); `Send` намеренно отсутствует (`:285–294`).

**Выведено.** Это примеры уже открытого долга 154, но именно они искажают картину «кто что трогает между потоками». Аудит ordering'ов по этим комментариям будет искать несуществующих межпоточных читателей `owner_state`/`kind`, несуществующую CAS-публикацию `PerClassDirty` и несуществующий вход в реестр с free-пути. Runtime-эффекта нет.

**Рекомендация.** Исправить в рамках docs-раунда 154. Для `owner_state` — после исправления doc'ов решить, нужны ли Acquire/Release в owner-only пути (гипотеза H7).

**Oracle:** расширить `tests/no_stale_doc_references.rs` на весь `src/` для удалённых идентификаторов (`PerClassDirty`, `dirty_by_class`, `resolve_dirty_bit_target`, `resolve_heap_overflow`, `reclaim_offset`, `DIRTY_BITMAP_WORDS`, `residual_xthread_double_free_no_corruption`) с явно перечисленными историческими исключениями. Сейчас такой тест красный, после исправления — зелёный.

## 3. Гипотезы оптимизации и поддерживаемости

**Все пункты не измерены.** Это не speedup, не RSS/latency-результат и не promotion GO. Для каждой гипотезы указаны слой, жертва и oracle. Судья обязан соблюдать правила R26-4/R30-8/entry-point/regime из CLAUDE.md.

- **H1 — test-and-test-and-set и backoff для трёх spin-lock'ов.**
  - `ShardLock::lock` (`shard_lock.rs:50–61`), `LockGuard::acquire` (`fallback.rs:471–484`) и `ExactShard::acquire` (`exact_shard.rs:185–191`) на каждой итерации ожидания выполняют `compare_exchange_weak`, то есть RMW, который забирает строку кэша эксклюзивно даже при занятом lock'е. Ожидание на `load(Relaxed)` до `false` с последующим CAS убирает пинг-понг строки.
  - Жертва — fan-in межпоточных освобождений в один шард (64 шарда, `directory.rs:899–902`). Это часть стоимости, которую должен измерить perf item 70 (fan-in latency, HITM).
  - Oracle: `bench-internals`-счётчик неуспешных CAS в `ShardLock` (активация пути) плюс latency `GlobalAlloc::dealloc` на реальном `#[global_allocator]`.
  - Родственное место вне `src/`: проигравшие в `OncePtrCell` (`crates/once-ptr-cell/src/imp.rs:525–531`) крутятся чистым `spin_loop`. Это документированный компромисс крейта (`:345–348`); используется при материализации чанков реестра.
- **H2 — инкремент pin в стиле `Arc`.**
  - `EntryHandle::pin_from_array` под shard-lock'ом использует CAS-цикл `fetch_update(AcqRel, Acquire, checked_add)` (`directory.rs:146–153`). Достаточно `fetch_add(1, Relaxed)` с abort при приближении к переполнению: порядок инкремента относительно снятия маршрута даёт сам lock, а release/acquire для освобождения несут декременты `AcqRel` (`:216`).
  - Выигрыш — один RMW без цикла на каждое межпоточное освобождение; тоже часть perf 70.
  - Предусловие: сначала нужна модель CON-04(a).
- **H3 — load-фильтр перед `swap(0, AcqRel)` в `BitmapScan::next_cut`** (`bitmap_scan.rs:21–22`): `if pending.load(Relaxed) == 0 { bits = 0 } else { swap(0, AcqRel) }`.
  - Контракт strict trim (`REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md` §1: включить публикации, завершённые до входа в trim) сохраняется по read-coherence. Load после happens-before-предшествующего `fetch_or` видит его бит или более позднее значение, а ноль означает, что все такие биты уже забраны предыдущими срезами.
  - Жертвы: strict trim при выходе потока (`ownership.rs:113`) и при каждом повторном захвате слота (`claim.rs:317`), drain на промахе refill (`find_segment.rs:136–160`), ограниченные срезы maintenance (`sidecar_drain.rs:128–137`).
  - Структурный факт (SOURCE-CONFIRMED): strict-срез идёт через `scan_small_route` → `RouteSlots::scan_small` → `SmallSidecar::scan` = `scan_from(…, 0)` (`sidecar_drain.rs:213`, `route_slots.rs:150–166`, `small_sidecar.rs:154–158`, `sidecar_bitmap.rs:126–128`). Он по-прежнему начинает со слова 0, включая слова метаданных, которые нулевые по построению: патч S коснулся только ограниченного курсора и `drain_segment_sidecar`.
  - Архивная запись perf 80 даёт до 4096 атомарных swap (≈49 тыс. Ir) на полностью нарезанный сегмент. Пропуск пустых payload-слов оставлен там «новой карточкой только по измерению». Load-фильтр — другой, более дешёвый механизм, чем summary-bitmap, но правило то же: без измерения не заводится.
  - Oracle: iai на thread-churn с K нарезанными сегментами плюс `bench-internals`-счётчик слов с `load == 0`.
- **H4 — не делать уборку при завершении процесса.** Пропуск `trim_for_recycle` выходящего потока при shutdown снижает задержку выхода и одновременно закрывает экспозицию CON-01. Слой — `AbandonGuard::drop`. Oracle — время выхода процесса с K сегментами в пуле и тест из CON-01.
- **H5 — `is_narrow(layout)` до `try_dealloc`** (CON-05). Для корректного использования эквивалентно, но меняет исход нарушения контракта; требует решения владельца.
- **H6 — fallback-шаг maintenance держит `LOCK` на весь холодный trim.**
  - `maintenance_service.rs:147–150` захватывает fallback `LOCK` и выполняет `background_maintenance_step`, включая полный `trim_cold_retention` (`ownership.rs:136`). Пока идут освобождения ОС, одновременные пользователи fallback ждут в `LockGuard::acquire`.
  - Кроме того, при активном использовании fallback (насыщенный реестр, TLS-teardown) воркер каждые 10 мс, застав lock свободным, сбрасывает его tcache, пул и Large-кэш.
  - Это новый угол к perf 78(a): там мерится стоимость визита (освобождения ОС, задержка), но не время удержания lock'а и не конфликт с пользователями.
  - Oracle: гистограмма времени удержания fallback `LOCK` (`bench-internals`) и latency fallback-выделений с воркером и без него.
- **H7 — поддерживаемость атомиков.**
  - `remote_head: AtomicU32` (`terminal_words.rs:17`; инициализация :99, :109; сброс :158–159) не имеет ни производителя, ни потребителя — только диагностический снимок (`:135–140`, `alloc_core_core_diag/header_diag.rs:64, 76`). Это мёртвое состояние интрузивного дизайна фазы 0.
  - `try_maintenance_at` делает CAS `FREE → MAINTENANCE` и на LIVE-слотах (`claim.rs:92–102`). Неуспешный `lock cmpxchg` тоже забирает строку эксклюзивно, а `HeapSlot.state` делит 64-байтную строку с `generation` и началом `heap` (`heap_slot.rs:150–204`, `repr(C)`; у `HeapCore` нет явного `align(64)`). Эффект ожидаемо пренебрежим (не более 32 слотов за 10 мс), но `load` перед CAS ничего не стоит.
  - Owner-only ordering'и: Acquire/Release в медленном пути `stamp_segment_owner` (`ownership.rs:76, 82`) и Release-обнуление `magic` при депозите Large-кэша, у которого больше нет удалённого Acquire-читателя (`segment_header_views.rs:186–190`). На x86 они безвредны, на AArch64 немного дороже. Упростить вместе с doc'ами CON-06.

## 4. Наблюдения вне темы

- `src/alloc_core/large/large_cache_extended.rs:41–42, 85`, `src/alloc_core/large/mod.rs:23`, `src/alloc_core/large/alloc_core_large_cache.rs:193, 254`: сайдкар расширения Large-кэша назван «`leak_zeroed_pages`-reserved». После R2-12 это владеющий `AccountedSidecar` через `sidecar::reserve` (`large_cache_extended.rs:137–169`; история — `platform/sidecar.rs:34–40`).
- `Cargo.toml:650–657` (`medium-classes-wide`) ссылается на упакованную запись hardened `RemoteFreeRing`, на `src/alloc_core/remote_free_ring.rs` и на тест `entry_never_collides_with_ring_slot_empty`. Ничего из этого нет.
- `Cargo.toml:680–681` (`hardened`): «The cross-thread leg is already covered UNCONDITIONALLY by `reclaim_offset`'s identical check». Функция теперь называется `reclaim_sidecar_record`, и на смещении не по границе блока она делает `abort` (`alloc_core_small_reclaim.rs:38–45`), а не no-op.
- `docs/REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md:3, 11, 33–35`: датированный контракт фазы 0 (он сам себя называет «не описанием работающей реализации») описывает интрузивный CAS по `remote_head` и ссылается на удалённые `src/registry/heap_overflow/spill.rs` и `src/alloc_core/large/deferred_large/`. Текущий якорь — `docs/CROSS_THREAD_STATE_MACHINES.md` §9. Ссылка на него в шапке контракта уберегла бы аудит от сверки с чужим протоколом.
- `docs/perf/OPEN_ITEMS.md:3039` (perf 78(a)) цитирует `ownership.rs:118-142`; сейчас `background_maintenance_step` — это `:119–138`.

## 5. Проверено без находок

**Терминальная публикация Small/Primordial.**

- Продюсер читает код класса (Acquire, `sidecar_bitmap.rs:114`) и публикует `fetch_or(bit, AcqRel)` (`:117`). Владелец забирает слово через `swap(0, AcqRel)` (`bitmap_scan.rs:22`), читает класс собственной записи (`bitmap_cut.rs:20–35`) и делает reclaim (`alloc_core_small_reclaim.rs:21–68`).
- Каждый бит достаётся ровно одному срезу. Последний пользовательский доступ продюсера happens-before повторного использования блока владельцем: Release-часть `fetch_or` синхронизируется с Acquire-частью `swap`.
- `ClassLeaves`: uniform переходит `0 → c+1` один раз (`leaf_classes.rs:129–133`); mixed-лист полностью инициализируется копией `old` до `store(Release)` (`:98–113`); после spill uniform больше не пишется; читатель `encoded` использует Acquire на обоих уровнях (`:137–147`). Видимость публикации покрывает `loom_r11_small_sidecar::model_visibility` с негативным контролем.

**Large-дескриптор.** Продюсер делает CAS `LIVE(g) → PENDING(g)` с `AcqRel/Acquire` (`reservation_state.rs:42–51`), владелец — `PENDING → CONSUMING` (`:58–73`); физическое слово отдельное (`:86–101`). Дубликат отсекается CAS. Дескриптор свежий на каждую регистрацию (поколение 1, `large_state.rs:14–17`). Всё согласуется с `CROSS_THREAD_STATE_MACHINES.md` §9.1.

**Route-директория.**

- `lookup` берёт pin под shard-lock'ом и проверяет себя abort'ом (`directory.rs:868–889`); `remove` идёт под тем же lock'ом (`:891–897`), а ссылку владельца роняют после освобождения lock'а (`registration.rs:78–82`). `EntryHandle::drop` — `fetch_sub(AcqRel)`, последний освобождает сайдкар и запись (`:210–225`).
- Инкремент под lock'ом happens-before снятия маршрута, поэтому по когерентности предшествует декременту владельца в modification order. Lookup после снятия запись уже не находит.
- Порядок блокировок: единственное вложение — fallback `LOCK` → shard-lock (операции fallback-кучи регистрируют и снимают маршруты). Обратного нет: `lookup` отпускает shard-lock до `try_with_heap` (`routing.rs:27–44`), `register` выделяет память вне lock'а (`:849–863`). `Shard::recycle` вызывает `System.dealloc` под lock'ом (`:644–652`), но это не выбранный глобальный аллокатор, реентрантности нет.
- Типы: `ShardLock<T>: Send + Sync` при `T: Send` (`shard_lock.rs:22–24`), guard — `PhantomData<&'a mut T>` (`:33–38`); `RouteRegistration` — `!Sync` (`PhantomData<Cell<()>>`, `registration.rs:12`); `RoutePin` содержит только `EntryHandle`, а его `&self`-методы лишь читают неизменяемые поля.

**Lease-автомат слотов реестра.**

- Захват — CAS из конкретных состояний (`claim.rs:220–265`). При первой публикации `initialised.store(Release)` выполняется до `INITIALIZING → LIVE` (`:295–306`). `HeapLease::drop` делает `LIVE → FREE` с Release (`:529–535`) и образует пару с AcqRel-CAS следующего захвата или maintenance.
- Maintenance берёт только `FREE → MAINTENANCE` и требует Acquire-наблюдения `initialised` (`:89–102`). Неинициализированный FREE-слот уходит `LIVE → INITIALIZING` (`:250–262`) и для maintenance недоступен.
- Цикл захвата lock-free: hint забирается через `swap` (`stack.rs:116`), каждый неуспешный CAS означает прогресс другого потока.
- Saturation-hint версионирован монотонно и отключается до переполнения (`saturation.rs:47–72`); `is_saturated` читает через RMW ради свежести (`:37–43`).
- Повторный захват выполняет `trim_for_recycle` (`claim.rs:317`) и тем подбирает публикации, адресованные куче при прежнем владельце (§9.1 `CROSS_THREAD_STATE_MACHINES.md`).
- ABA на tagged index stack в production неприменим: реестр больше не использует интрузивный список (`stack.rs:1`); остаток покрытия ведёт correctness 167.
- У счётчиков `alloc-stats` (load+store, `hot.rs:333–339`) один писатель — на lease или под fallback `LOCK`.

**TLS-teardown.** `TORN` ставится до снятия lease (`tls_heap.rs:214–224`). Резолверы отображают `TORN` в `Fallback`/`ForeignNoBind`/`None` (`:256–281`, `:321–336`, `:386–401`). `finish_bind` публикует `LOCAL` до установки `GUARD` и откатывается при сбое (`:476–533`). Abort `HeapLease::drop` при `panicking()` на этих путях в production недостижим: внутри claim нет выделений через глобальный аллокатор.

**Fallback.** `INIT_STATE`: CAS с Acquire, `READY` публикуется с Release (`fallback.rs:149–167, 278`), есть откат при OOM (:295) и при раскрутке (`InitStateGuard`, :541–572; item 23 без изменений). `LOCK` — Acquire/Release (:453, :471–491). `try_with_heap` проверяет `READY` через Acquire до try-lock (:393–399).

**MaintenanceService.** `IDLE → STARTING` — CAS AcqRel (`maintenance_service.rs:72`) с откатом через `StartingGuard` (:52–59, :77, :96). `start` ждёт `RUNNING` под `CONTROL` и перепроверяет предикат в цикле (:100–105), а воркер пишет `RUNNING` под `CONTROL` и будит всех (:131–135), поэтому потерянного пробуждения нет. Неожиданный выход воркера — abort (:61–68).

**Epoch, lock-free и sharded регионы.**

- Протокол hint `EpochRegion` — см. CON-03.
- Seqlock `AtomicSlot::read_with` (Acquire на g1, значении и g2, `hand.rs:232–300`) обнаруживает вытеснение: если читатель видит null, записанный `swap` вытеснителя, он синхронизируется с вытеснителем и видит CAS поколения. Поколения не переполняются (насыщение на `u32::MAX`).
- `try_evict_at` уникален благодаря CAS поколения (`:395–437`). `Send`/`Sync` для `AtomicSlot<T>` требуют `T: Send + Sync` (`:572, :582`).
- `EpochRegion::drop` после `486f5ace` изолирует панику деструктора под `&mut self` — это однопоточный код.
- `LockFreeRegion`: писатели под мьютексом, читатели через `ArcSwap` (`lock_free_region.rs:227–229`).
- `ShardedRegion`: `TOKEN_BACKING_DEATHS` — Release/Acquire (`sharded_region.rs:211, 278, 492`), захват шарда — CAS Acquire (`:499`).
- `pinning.rs` использует `std::thread::scope` без собственных атомиков.

**Общее.**

- Fence'ов в `src/` нет; `SeqCst` встречается только в test-only инъекциях.
- Гонок данных на неатомарных разделяемых полях в прочитанном не найдено.
- Глобальный `LARGE_REMOTE_RETIREMENTS` (`Relaxed`, `sidecar_drain.rs:169–170, 251–252, 292, 341`) инкрементируется на холодном пути, не на hot path.
- Все 13 корневых loom-файлов входят в CI-джобы (`ci.yml:3134–3216`), осиротевших моделей нет.

**Связь с открытыми пунктами.** Для остальных пунктов обоих индексов новых доказательств нет (LEAVE).

| Пункт | Связь |
|---|---|
| correctness 160 | CON-03: рекомендуется закрыть или сузить |
| correctness 154 | CON-06: новые примеры, касающиеся именно межпоточных контрактов |
| correctness 152 | тот же механизм, что в CON-01, но для POSIX `fork`; не переоткрывается |
| correctness 17 | CON-04(c): комментарий CI противоречит принятому остатку; не переоткрывается |
| correctness 162 | CON-01 и CON-04(b) — входы для его приёмки (OS-матрица, fallback, воркер); статус не меняется |
| correctness 23, 164, 166, 167, 171 | не затронуты |
| perf 70 | H1 и H2 — составляющие стоимости foreign-publish |
| perf 78(a) | H6 — новый угол (удержание fallback `LOCK`) |
| perf 80 (архив) | H3 — другой механизм; правило «только по измерению» соблюдено |

## Приложение A. Инструменты и команды (только чтение)

- База и чистота дерева: `git rev-parse HEAD`, `git status --porcelain`.
- Перепись строк: `git ls-files 'src/*.rs' | xargs cat | awk '...'` (пустые строки — `^[[:space:]]*$`, комментарии — `^[[:space:]]*//`, остальное — «код») → `total 43325 code 17059 comment 24303 blank 1963`. По каталогам — `find <dir> -name '*.rs' | wc -l` и `find <dir> -name '*.rs' -exec cat {} + | wc -l`.
- Unsafe-инвентарь: `grep -rlE '^\s*#!\[allow\(unsafe_code\)\]' src | wc -l` (21) и `grep -rnE '^\s*#\[allow\(unsafe_code\)\]' src | wc -l` (83, в 30 файлах); то же для `crates` (6 / 20).
- Атомарная перепись: `grep -rho -- '<токен>' src | wc -l` по каждому токену; файлы с атомарными типами — `grep -rlE 'Atomic(U8|U16|U32|U64|Usize|Bool|Ptr|I64|Isize)' src | wc -l`.
- Циклы ожидания: `grep -rn "spin_loop()\|yield_now()" src crates/once-ptr-cell/src crates/tagged-index-stack/src`.
- История: `git log --format=... -- tests/loom_r11_epoch_false_full.rs`, `git show -s --format=%B 63991cc3`, `git show 486f5ace -- src/concurrent/epoch/epoch_region.rs`, `git diff --stat 6a0d47f6 453439c1 -- src/`, `git log 6a0d47f6..453439c1 -- src/`.
- Остальное — чтение файлов и поиск (ripgrep).

## Приложение B. Witness, мутанты, исполненная верификация

Не запускалось: режим только чтение. Oracle'ы исправлений в §2 — проекты тестов, а не исполненные проверки.
