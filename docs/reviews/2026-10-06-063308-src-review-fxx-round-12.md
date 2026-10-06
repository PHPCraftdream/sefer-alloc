# Независимый обзор `src/` — раунд 12 (fxx)

- **Дата:** 2026-10-06 06:33:08 +02:00
- **Автор:** независимый ревьюер (fxx), раунд 12 обзоров `src/`
- **База:** `main` @ `b797f21326c7d47eb2bbb699b289f18bc1e8df8d` (ветка
  `fxx-src-review-r12`, рабочее дерево чистое на момент начала и окончания
  обзора, кроме этого файла)
- **Предыдущий раунд:** `docs/reviews/2026-09-30-232702-src-review-xs-sol-round-11.md`
- **Область:** весь `src/` — 168 файлов `.rs`, 42 843 строки
  (`alloc_core/`, `registry/`, `global/`, `concurrent/`, `kani_proofs.rs`,
  `lib.rs`). Прочитаны полностью, а не по заголовкам.
- **Метод:** сплошное чтение исходников; трассировка каждой находки до
  конкретного сценария отказа; сверка кода с документацией в том же файле;
  сверка с индексами открытых пунктов (`docs/perf/OPEN_ITEMS.md`,
  `docs/CORRECTNESS_OPEN_ITEMS.md` + `docs/correctness-open-items/*.md`) —
  чтобы не переоткрывать уже закрытое и не предлагать заново отвергнутое;
  проверка на HEAD тех исправлений, которые раунды 1–11 и фазы Ph1–Ph7
  объявили закрытыми. Тяжёлые прогоны (`npm run check`, полный
  `cargo test --all-features`, бенчмарки) не запускались — область
  подтверждения указана в каждой карточке отдельно.
- **Тулчейны:** `rustc 1.97.0 (2d8144b78 2026-07-07)`,
  `cargo 1.97.0 (c980f4866 2026-06-30)`. Miri/loom/kani в этом раунде не
  запускались (точечно — только там, где карточка это прямо указывает как
  способ проверки).
- **Нумерация:** `fxx R12-NN`, сквозная по этому отчёту.
- **Контекст прочитан:** `CLAUDE.md`; обзоры `docs/reviews/*src-review*`
  раундов 1–11 плюс `docs/reviews/2026-10-01-src-r11-production-cost-advice-xxs-sol.md`
  и `docs/reviews/2026-10-01-r11-protected-box-advice-xxs-sol.md`; расписки
  фаз `docs/reviews/2026-10-05-ph4a-heap-lease-receipt.md`,
  `…ph4b-tls-lease-receipt.md`, `…ph4c-narrow-legacy-registry-receipt.md`,
  `docs/reviews/2026-10-05-ph6a-correctness-matrix.md`,
  `docs/perf/PH6B_COST_AB.md`, `docs/evidence/REGISTRY.md`.

---

## 0. Сводка

| ID | Severity | Категория | Одной строкой |
|---|---|---|---|
| fxx R12-01 | P2 | устойчивость / DoS | Кросс-потоковый `dealloc` 16-выровненного никогда-не-выданного смещения в чужом Small-сегменте приводит к `std::process::abort()` у владельца; own-thread ветка тот же вход гасит как no-op. |
| fxx R12-02 | P3 | паника в `GlobalAlloc` / горячий путь | `clear_magazine_on_issue` вызывает `.expect(...)` на каждом попадании в магазин и заменил маску на двухуровневый поиск в таблице сегментов. |
| fxx R12-03 | P3 | расхождение код↔док + производительность | Быстрый путь R8-2 («доверяй промаху директории, не делай O(S) скан») выключен во всём `production`; документация и счётчик продолжают описывать его как рабочее поведение. |
| fxx R12-04 | P3 | живучесть под переподпиской | Проигравшие гонку инициализации fallback-кучи крутятся в `spin_loop()` без `yield_now()`, в отличие от соседнего `LockGuard`/`ShardLock`. |
| fxx R12-05 | P3 | задержка (хвост) | `alloc_batch` и две точки `realloc` используют неограниченный O(L) свип Large-ингресса там, где скалярный путь уже перешёл на ограниченный курсор (R11 HS-A). |
| fxx R12-06 | P4 | конвенция `unsafe`-швов | `src/registry/heap_registry/counters.rs` — объявленный шов tier-1 с НОЛЬ `unsafe`-токенов. |
| fxx R12-07 | P4 | устаревшая документация | `src/global/tls_heap.rs` содержит два взаимно противоречащих комментария про свою `unsafe`-поверхность. |
| fxx R12-08 | P4 | мёртвая зависимость + устаревшая документация | `tagged-index-stack` — жёсткая зависимость `alloc-global`/`production`, но вне `cfg(loom)`/`cfg(kani)` её никто не использует; два видных места документации описывают снятую обвязку `free_slots`/`StackHead`. |
| fxx R12-09 | P4 | устаревшая документация | `bootstrap/chunk.rs` и `bootstrap/ensure.rs` описывают нулевое состояние слота через удалённые поля (`slot.remote.thread_free`, `slot.overflow`, `HeapOverflow`). |
| fxx R12-10 | P4 | нарушения конвенций проекта | Три объявления `#[cfg(test)] mod tests` внутри `src/`; модульный `#![allow(dead_code)]` без актуальной мёртвой кодовой базы; упоминание снятого модуля `deferred_large`. |

**Главный вывод.** Ядро терминального протокола (RouteDirectory → sidecar →
owner-reclaim), типизированные лизы (Ph4a/4b/4c) и машина состояний Large
читаются как согласованное целое: ни одного нового дефекта памяти (UB,
UAF, двойного освобождения, переполнения арифметики размеров) я не нашёл, и
ни одно исправление раундов 1–11, которое я проверял на HEAD, не
регрессировало. Единственная находка уровня P2 — не ошибка памяти, а
**асимметрия сдерживания**: на own-thread ветке освобождения внутренний/
мусорный указатель отбрасывается как no-op и учитывается счётчиком, а на
кросс-потоковой ветке тот же вход доводит владельца до
`std::process::abort()`. Остальное — одна паника на горячем пути аллокации,
один фактически выключенный в `production` оптимизационный путь, чья
документация этого не отражает, и накопившийся долг документации/конвенций
после терминального перехода (снятые поля, снятая обвязка `free_slots`,
«пустой» шов `unsafe`).

---

## 1. Проверка исправлений и регрессий (раунды 1–11, фазы Ph1–Ph7) на HEAD

Способ: для каждого пункта — чтение текущего кода в указанном месте.
«ПОДТВЕРЖДЕНО» = исправление присутствует и механизм на месте;
«ЧАСТИЧНО» = присутствует, но с оговоркой, выписанной в §2.

| Пункт | Источник | Состояние на HEAD | Где смотрел |
|---|---|---|---|
| fxx R2-01 — `finish_bind` публикует `LOCAL` до вооружения `GUARD` | обзор раунда 2 | ПОДТВЕРЖДЕНО | `src/global/tls_heap.rs:501-520` |
| R2-02 — границы `T: Send + 'static` / `T: Sync` у epoch-слоя | обзор раунда 2 | ПОДТВЕРЖДЕНО | `src/concurrent/epoch/hand.rs:232-241,339-342,395-398` |
| R2-03 — `region_id` в `EpochHandle`/`LockFreeHandle` + отказ до поиска слота | обзор раунда 2 | ПОДТВЕРЖДЕНО | `src/concurrent/epoch/epoch_region.rs:270-276,488-490,559-561`; `src/concurrent/lock_free/lock_free_region.rs:338-340,428-431` |
| R2-03 (вторая половина) — `try_evict_at` возвращает `Stale` на нулевом `old` вместо `defer_destroy(null)` | обзор раунда 2 | ПОДТВЕРЖДЕНО | `src/concurrent/epoch/hand.rs:437-473` |
| R2-04 — `set_generation_for_tests` требует `&mut` и проверяет незанятость | обзор раунда 2 | ПОДТВЕРЖДЕНО | `src/concurrent/epoch/hand.rs:183-203` |
| R2-05 — `dbg_*` читают через канонический сохранённый корень | обзор раунда 2 | ПОДТВЕРЖДЕНО | `src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs:99-107,191-203`; `table_diag.rs:227-242` |
| R2-16 — `PinnedRunner::run` усекает воркеров по `region.shard_count()` переданного региона | обзор раунда 2 | ПОДТВЕРЖДЕНО | `src/concurrent/pinning.rs:231` |
| R2-20 — отклонённый `remove` не платит за CoW | обзор раунда 2 | ПОДТВЕРЖДЕНО | `src/concurrent/lock_free/lock_free_region.rs:434-456` |
| R2-21 — `drain_remote_free` делает `swap` со scratch, а не `take` | обзор раунда 2 | ПОДТВЕРЖДЕНО | `src/concurrent/epoch/epoch_region.rs:355-396` |
| R9-01 / R9-03 — kind-гейт и пример в `stats()` | раунд 9 | ПОДТВЕРЖДЕНО | `src/alloc_core/small/alloc_core_small_diag.rs:41-47,85-90`; `src/global/sefer_alloc/diag.rs` |
| R10 P4 — контракт `Node::offset` (`off <= isize::MAX`) | раунд 10 | ПОДТВЕРЖДЕНО | `src/alloc_core/platform/node.rs` |
| R11 P2-1 — `remote_free_pending` (Relaxed) не может «потерять» слот | раунд 11 | ПОДТВЕРЖДЕНО (закрыто иначе, чем предлагалось): флаг остался Relaxed, но `insert` при пустом локальном списке делает принудительный заблокированный дренаж до возврата `Err` | `src/concurrent/epoch/epoch_region.rs:432-441` |
| R11 P3-1 — K×L скан Large на промахе Small-refill | раунд 11 | **ЧАСТИЧНО** — скалярный путь переведён на ограниченный курсор (`LARGE_HOT_BUDGET`), батч и realloc остались на полном свипе → fxx R12-05 | `src/registry/heap_core/alloc/hot.rs:575,759`; `src/alloc_core/alloc_core/sidecar_drain.rs:302-353`; против `src/registry/heap_core/alloc/batch.rs:160,276` |
| R11 P3-2 — 288 KiB на Small-маршрут | раунд 11 | ЧАСТИЧНО (по дизайну): плотные классы заменены на `ClassLeaves` (uniform-байт на лист + System-лист 256 B только при смешении) — остаток числится как `docs/perf/OPEN_ITEMS.md` пункт 78(b) | `src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs:35-133` |
| R11 P3-3 — треугольный скан при claim | раунд 11 | ПОДТВЕРЖДЕНО (`pick_with_saturation`: hint-swap → ограниченный `bump_count` → скан) | `src/registry/bootstrap/saturation.rs`, `src/registry/heap_registry/claim.rs` |
| R11 HS-A/HS-B/HS-C (советы по цене production) | `docs/reviews/2026-10-01-src-r11-production-cost-advice-xxs-sol.md` | ПОДТВЕРЖДЕНО: HS-A = `large_hot_cursor`+`LARGE_HOT_BUDGET`, HS-B = `ClassLeaves`, HS-C = hint-first селектор | см. выше |
| Ph4a/4b/4c — типизированные лизы, снятие legacy `claim`/`recycle`/`into_raw` | расписки Ph4a/4b/4c | ПОДТВЕРЖДЕНО: `HeapLease::drop` делает LIVE→FREE Release-CAS и `abort` при проигрыше CAS/`thread::panicking()`; `MaintenanceLease` даёт безопасный `with_core`; legacy-входов нет | `src/registry/heap_registry/claim.rs:521-542` |
| `docs/CORRECTNESS_OPEN_ITEMS.md` пункт 5 — висячий `small_cur` после измерительных хуков | индекс корректности | ПОДТВЕРЖДЕНО: оба хука идут через `reserve_small_segment_impl` (без публикации курсора) | `src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs:52-59,225-229`; `src/alloc_core/small/alloc_core_small/reserve.rs:99-120` |
| R25-1 (правило benchmark-хуков) | `CLAUDE.md` | ПОДТВЕРЖДЕНО: `dbg_force_decommit_retain_for`, `dbg_decomp_release`, `dbg_decomp_decommit_payload`, `dbg_restore_local_for_test` — `pub unsafe fn` + `bench-internals`; `ReservedSmallSegment::dbg_base` переименован под сканер трипвайра | `src/alloc_core/small/alloc_core_small_pool/decommit.rs:93-108`; `decomp_hooks.rs:270-321`; `src/alloc_core/small/reserved_small_segment.rs:196-201` |
| R31-15 — owner-binding `ReservedSmallSegment` | раунд 31 | ПОДТВЕРЖДЕНО, включая порядок «`into_base()` до `assert_eq!`», чтобы не получить panic-in-panic | `src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs:299-306` |
| R34-17 (F-8) — `InitStateGuard` откатывает `INIT_STATE` при разворачивании | раунд 34 | ПОДТВЕРЖДЕНО | `src/global/fallback.rs:181,274,293` |
| L-4 / UBFIX-11 — `flush_class` не трогает уже утилизированную базу дважды | раунд 4 | ПОДТВЕРЖДЕНО (`recycled_bases`, `RECYCLED_CAP = 16`) | `src/alloc_core/small/alloc_core_small_magazine.rs:489-532` |
| UBFIX-7 (M-3) — hardened-проверка `next` в `pop_free`/`drain_freelist_batch` | аудит UB | ПОДТВЕРЖДЕНО (обе точки) | `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:407-412,565-570,598-603` |
| H-1 / M-1 (UBFIX-3) — безусловные границы `off < payload_start` и `off >= bump` | аудит UB | ПОДТВЕРЖДЕНО в `dealloc_small` и `flush_run` | `src/alloc_core/small/alloc_core_small/dealloc.rs:73-99`; `alloc_core_small_magazine.rs:562-599` |
| R12-1/R12-2/R13-2 — директория читается по значению, плотная карта узлов, переиспользование бакетов | раунды 12/13 | ПОДТВЕРЖДЕНО | `src/alloc_core/small/alloc_core_small/find_segment.rs:265-361`; `src/alloc_core/segment/segment_directory/segment_directory_impl.rs:436-615` |
| oxx R2-01 — финализация опустевшего `small_cur` при смене курсора | обзор oxx раунда 2 | ПОДТВЕРЖДЕНО | `src/alloc_core/small/alloc_core_small/reserve.rs:40-57`; `…_pool/alloc_core_small_pool_impl.rs:337-355` |
| P1-box (пункт 164 индекса корректности) | индекс корректности | не регрессировал и не закрыт: `Node::write_next` в освобождённый Small-блок по-прежнему на пути `reclaim_sidecar_record`/`dealloc_small` — принятый известный дефект, вне области этого раунда | `src/alloc_core/small/alloc_core_small_reclaim.rs:54-67` |

**Регрессий среди перечисленного не обнаружено.** Два пункта помечены
ЧАСТИЧНО: R11 P3-1 (закрыт только на скалярном пути) — выписан как
fxx R12-05; R11 P3-2 — остаток уже числится в индексе перф-пунктов.

---

## 2. Находки по severity

### fxx R12-01 — кросс-потоковое освобождение невыданного смещения доводит владельца до `abort()`

- **Severity:** P2
- **Категория:** устойчивость / отказ в обслуживании; асимметрия сдерживания
  между own-thread и кросс-потоковой ветками освобождения
- **Место:**
  - `src/registry/heap_core_xthread/routing.rs:26-61` (`publish_foreign`)
  - `src/registry/segment_route/directory.rs:868-889` (`lookup`), `:174-179`
    (`contains_payload`)
  - `src/registry/segment_route/pin.rs:46-50` (`publish_small`)
  - `src/registry/segment_route/small_sidecar.rs:142-144` (`publish`)
  - `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs:103-109`
    (`publish`), `:137-140` (`granule`)
  - `src/alloc_core/segment/remote_bitmap/bitmap_cut.rs:27-29` (точка
    `std::process::abort()`)
  - `src/alloc_core/segment/segment_table/segment_table_impl.rs:381-383`
    (`register` → `payload == base`)
- **Механизм (пошагово):**
  1. `SeferAlloc::dealloc(ptr, layout)` → `HeapCore::dealloc_routing`.
     `self.core.canonical_block_of(ptr)` возвращает `None`, если сегмент не
     принадлежит таблице ЭТОГО ядра — т.е. для любого сегмента чужой кучи
     (`src/alloc_core/alloc_core/mem/mem_impl.rs:34-42`). Управление уходит в
     `publish_foreign`.
  2. `RouteDirectory::lookup(ptr)` ищет маршрут по ключу
     `addr & !(SEGMENT-1)` и принимает адрес, если
     `addr >= entry.payload && addr < entry.end`. Для Small/Primordial
     регистрация идёт через `SegmentTable::register(base, SEGMENT, kind)`,
     который передаёт `payload = base`
     (`segment_table_impl.rs:381-383`), т.е. **весь сегмент, включая
     метаданные**, попадает в допустимый диапазон. Проверка
     `addr == entry.payload` применяется только к `RouteKind::Large`.
  3. Ветка `RouteKind::Small | RouteKind::Primordial` считает
     `offset = addr - segment_base` и вызывает `pin.publish_small(offset)`.
  4. `SidecarBitmap::publish` проверяет ТОЛЬКО `offset < SEGMENT` и
     `offset % MIN_BLOCK == 0` (`granule`, `:137-140`) и делает
     `pending[granule/64].fetch_or(...)`. Класс гранулы не проверяется —
     соответствие «гранула была выдана» нигде на стороне производителя не
     устанавливается.
  5. Владелец на следующем дренаже (`drain_sidecar_ingress` /
     `drain_sidecar_ingress_bounded` / `drain_segment_sidecar`) доходит до
     `BitmapCut::pop`, читает `self.classes.encoded(granule)` и при `0`
     (гранула никогда не выдавалась) выполняет **`std::process::abort()`**
     (`bitmap_cut.rs:27-29`).
- **Сценарий отказа:** поток A владеет Small-сегментом; поток B вызывает
  `dealloc` с 16-выровненным указателем, который не является началом
  выданного блока этого сегмента. Достаточно любого из:
  внутреннего указателя в блок класса ≥ 32 B (`block + 16`, `+32`, `+48`
  при `MIN_BLOCK = 16`); 16-выровненного адреса в области метаданных
  сегмента (`offset < payload_start`); 16-выровненного адреса в ещё не
  вырезанном хвосте (`offset >= bump`). Процесс целиком падает по `abort()`
  — в потоке A, в произвольный более поздний момент, без связи с местом
  ошибки.
- **Почему это дефект, а не «ожидаемое наказание за UB»:** вход сам по себе
  — нарушение контракта `GlobalAlloc` (освобождать можно только начало
  выданной аллокации), и ни один корректный вызывающий код сюда не придёт.
  Но own-thread ветка для ТОГО ЖЕ входа ведёт себя ровно наоборот —
  гасит его как no-op и считает:
  `dealloc_small` отбрасывает `off < payload_start` безусловно
  (`dealloc.rs:79-81`), `off >= bump` безусловно (`:97-99`), уже свободный
  блок по битовой карте (`:102-104`), а под `hardened` ещё и
  `off % block_size != 0` (`:59-62`); `publish_foreign` сам считает
  неотмаршрутизированные освобождения в `FOREIGN_OR_UNROUTABLE_FREES`
  (`routing.rs:62-65`). То есть дизайн сдерживания в крате есть и
  последователен — кросс-потоковая ветка просто не закрывает последний шаг,
  и вместо локального отказа даёт глобальный `abort()`. Дополнительно:
  16-НЕвыровненный мусор уже отбрасывается корректно (`granule` вернёт
  `None` → `publish_small` вернёт `false` → счётчик), так что
  непоследовательность видна внутри одной функции.
- **Подтверждение:** прочитано (сквозная трассировка всех шести файлов
  выше). Поведение рантайма **НЕ ПОДТВЕРЖДЕНО** экспериментом. Как
  проверить дешёво: временный тест в `tests/` под
  `all(feature = "production", feature = "internals", feature = "bench-internals")`:
  поток A выделяет блок класса 64 B и держит его; поток B вызывает
  `HeapCore::dbg_publish_small_sidecar_free` (есть в
  `src/registry/heap_core_xthread/sidecar_drain.rs:56-59`) либо напрямую
  `SeferAlloc::dealloc` с `ptr.add(32)`; затем A вызывает
  `dbg_drain_sidecar_ingress`. Ожидание при наличии дефекта — падение
  процесса; тест нужно запускать как подпроцесс (`std::process::Command`
  на `current_exe()`, как уже сделано в `tests/r8_terminal_global.rs`),
  потому что `abort()` не перехватывается.
- **Предлагаемое исправление:** проверять «гранула была выдана» на стороне
  производителя, до AcqRel-RMW. В `SidecarBitmap::publish`
  (`sidecar_bitmap.rs:103-109`) добавить после вычисления `granule`:
  если `self.classes.encoded(granule) == 0` — вернуть `false` (вызывающий
  `publish_foreign` уже учтёт это как `FOREIGN_OR_UNROUTABLE_FREES`).
  Чтение корректно упорядочено: для легально выданного блока `issue`
  выполнил Release-запись класса до передачи блока пользователю, а передача
  владения между потоками несёт это ребро happens-before, поэтому для
  настоящего блока наблюдается `encoded != 0`. `abort()` в `BitmapCut::pop`
  при этом стоит оставить — после такой проверки он снова становится
  честным детектором нарушения инварианта, а не способом падения от
  мусорного указателя.
- **Риск исправления:** низкий. Одна дополнительная Acquire-загрузка
  (`uniform[leaf]`, та же строка кэша, что и дальше используется) на
  кросс-потоковое освобождение — холодная ветка относительно
  `fetch_or`. Семантика для корректных освобождений не меняется.
  Альтернатива (сделать `pop` пропускающим, а не аварийным) хуже: владелец
  не отличает «мусор от чужого потока» от «порча собственных метаданных»,
  и пропуск размывает единственный детектор инварианта.
- **Статус относительно индексов:** **новый**. В
  `docs/CORRECTNESS_OPEN_ITEMS.md` (и в разбивке
  `docs/correctness-open-items/*.md`) кросс-потоковая ветка публикации
  невыданной гранулы не упоминается; `docs/perf/OPEN_ITEMS.md` пункт 70
  (`oxx R2 H3 successor`) касается только contention/footprint того же
  sidecar, не валидации входа.

---

### fxx R12-02 — `clear_magazine_on_issue`: паника на пути попадания в магазин + поиск в таблице вместо маски

- **Severity:** P3
- **Категория:** паника в `GlobalAlloc` / стоимость самого горячего пути
- **Место:** `src/registry/heap_core/alloc/hot.rs:43-56`;
  `src/alloc_core/alloc_core/mem/mem_impl.rs:26-28` (`canonical_root_for`);
  вызовы — `hot.rs` (обе ветки попадания), `src/registry/heap_core/alloc/batch.rs:204-206`
- **Механизм:**
  1. `clear_magazine_on_issue` выполняется на КАЖДОМ попадании в магазин
     (и на каждый блок батча) и начинается с
     `self.core.canonical_root_for(issued).expect("issued magazine block belongs to a live segment")`.
  2. `canonical_root_for` = `table.canonical_base_of(os::segment_base_of_ptr(ptr))`,
     т.е. Tier-1 (`own_cache`, прямое отображение) + при промахе Tier-2
     (открытая адресация по хэшу)
     — `src/alloc_core/segment/segment_table/segment_table_impl.rs:801-808`.
  3. Соседняя функция `finish_magazine_refill` для той же цели (пометить
     блок резидентом магазина) использует ДЕШЁВУЮ маску
     `os::segment_base_of_ptr(p)` (`hot.rs:113`). Биты ставятся через маску,
     а снимаются через поиск в таблице.
- **Сценарий отказа (паника):** `expect` на пути `GlobalAlloc::alloc`.
  Разворачивание из `GlobalAlloc::alloc` недопустимо, и в крате это уже
  закреплено отдельным тестом
  (`tests/regression_r2_08_global_allocator_path_no_unwind.rs`), а
  `alloc_small` прямо документирует противоположное правило: «we return
  null (graceful OOM) rather than panicking — the GlobalAlloc face
  (Phase 11) must never abort»
  (`src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:225-231`).
  Достижимость: под инвариантом `live_count` блок, лежащий в магазине,
  удерживает кредит сегмента, поэтому сегмент не может быть утилизирован
  (`dec_live_and_maybe_decommit` требует `live == 0`), и в штатном
  production-пути `expect` не срабатывает — но это НЕлокальный инвариант,
  доказываемый в другом файле, и единственная точка паники на этом пути.
- **Сценарий деградации (стоимость):** `docs/perf/OPEN_ITEMS.md` пункт 17
  измерил именно этот блок в 12.19 Ir на попадание, из которых ~9.03 Ir —
  `os::segment_base_of_ptr` (маска). Текущая реализация добавляет сверху
  сравнение ключа `own_cache` и, при промахе Tier-1, полный зонд Tier-2
  (~12 Ir по измерению пункта 1 / R32-10). Там же (пункт 1, закрытая
  часть) зафиксировано, что при ≥16 одновременно «горячих» сегментах
  Tier-1 с прямым отображением промахивается полностью — то есть на
  таких профилях каждое попадание в магазин платит зонд хэша вместо маски.
  Это удорожание относительно измеренной базы, не отражённое ни в одной
  карточке.
- **Подтверждение:** прочитано; измеренные числа взяты из уже
  существующих отчётов (`R29_10_ALLOC_HIT_CLEAR_MAGAZINE_ISOLATION_GATE.md`
  через карточку пункта 17, `R32_10_OWN_CACHE_TIER1_THRASH_GATE.md` через
  карточку пункта 1), собственного замера в этом раунде нет.
- **Предлагаемое исправление:** вернуть на этом пути маску, а согласие
  таблицы закрепить проверкой только в отладочной сборке:
  `let base = os::segment_base_of_ptr(issued);` +
  `debug_assert_eq!(self.core.canonical_root_for(issued), Some(base), ...)`.
  Для Small-сегментов эти два значения тождественны по построению:
  `register` кладёт в хэш ключ `segment_base_of_ptr(payload)` при
  `payload == base`, а `base` у Small/Primordial всегда `SEGMENT`-выровнен
  (смещённый корень существует только у Large при `align >= SEGMENT`, а в
  магазине Large-блоков нет). Это одновременно снимает панику с горячего
  пути и возвращает симметрию с `finish_magazine_refill`.
- **Риск исправления:** низкий; изменение локально в одной функции,
  инвариант проверяется `debug_assert` и уже покрыт инвариантом
  `live_count`. Обязательно: `npm run iai` на `small_churn_*` до/после и
  счётчики `dbg_contains_base_tier1_hits`/`_misses` как оракул активации
  пути (правило R30-8) — иначе это снова незамеренное изменение горячего
  пути.
- **Статус относительно индексов:** **новый** (как дефект). Стоимость
  самого блока числится как `docs/perf/OPEN_ITEMS.md` пункт 17 (закрыт как
  «honest reject» по ОТЛОЖЕННОЙ очистке бита), но замена маски на
  двухуровневый поиск и `expect` в этой карточке не упомянуты —
  карточка измеряла вариант с маской.

---

### fxx R12-03 — быстрый путь «доверяй промаху директории» (R8-2) выключен в `production`, документация этого не отражает

- **Severity:** P3
- **Категория:** расхождение код↔док + производительность
- **Место:** `src/alloc_core/small/alloc_core_small/find_segment.rs:391-416`
  (решение `trust_negative`), `:418-530` (линейный скан),
  `:136-172` (`drain_segment_sidecar`), `:469` (вызов на каждого
  кандидата);
  `src/alloc_core/segment/segment_table/segment_table_impl.rs:285-287`
  (`is_routed`);
  `src/alloc_core/segment/segment_directory/directory_stats.rs:52-58`
  (документация счётчика)
- **Механизм:**
  ```rust
  #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
  let trust_negative = !self.table.is_routed();
  ```
  `is_routed()` = `self.routes.is_some()`, и в `production`
  (`alloc-global` + `alloc-xthread`) маршруты всегда материализованы —
  иначе не работал бы терминальный ингресс. Значит `trust_negative == false`
  всегда, ветка `else if !rescue { … return None; }` (авторитетный промах,
  пропуск O(S)-скана) недостижима, а вместо неё выполняется
  `periodic_revalidation_active = !rescue` и управление падает в полный
  линейный скан. Следствия:
  1. `DIRECTORY_AUTHORITATIVE_MISS` в `production` тождественно 0 —
     при этом его документация утверждает: «This is the primary
     observability counter for the fix: it directly measures how often the
     O(S) scan is now avoided» (`directory_stats.rs:55-58`). Та же
     неточность в `DIRECTORY_MISS_FULL_SCAN_PERIOD`
     (`segment_directory_impl.rs:16-57`), чья роль «периодической
     перепроверки» вырождается: перепроверка идёт на КАЖДОМ промахе.
  2. Счётчик `DIRECTORY_MISS_SELF_HEAL` документирован как канарейка
     («nonzero value … is a canary indicating a genuine directory-tracking
     bug»), но в `production` он инкрементируется на каждом попадании
     линейного скана после промаха директории (`finalize_hit` вызывается с
     `periodic_revalidation_active == true`) — то есть канарейка
     сигналит в штатном режиме.
  3. Цена: на каждый промах списка свободных блоков выполняется проход по
     всем активным Small-слотам, и для КАЖДОГО кандидата —
     `drain_segment_sidecar(base)`, который делает
     `scan_small_route_from(index, base, high_water, start_word)` по
     `bump / (MIN_BLOCK*64)` словам (до 4096 слов на полностью вырезанный
     сегмент). То есть O(S × W), а не O(S).
- **Сценарий отказа:** не аварийный — деградация задержки на холодной
  ветке Small-аллокации плюс ложные показания диагностики. Воспроизводится
  профилем «много сегментов, частые промахи класса»: чем больше
  зарегистрированных Small-сегментов, тем дороже каждый промах, при том
  что механизм, построенный ровно чтобы этого избежать, в production
  не задействован.
- **Подтверждение:** прочитано. Оценок стоимости не снимал — числа 442 ns
  при S=32 и порог материализации 32 взяты из уже существующей
  документации (`segment_directory_impl.rs:5-14`, ссылающейся на
  `docs/perf/R7_DIRECTORY_BASELINE.md`). **НЕ ПОДТВЕРЖДЕНО** замером. Как
  проверить: собрать с `--features "production internals alloc-stats"`,
  прогнать профиль с >32 сегментами и промахами класса и прочитать
  `AllocCore::dbg_directory_authoritative_miss()` (ожидание: 0) вместе с
  `dbg_full_scan_slots_examined()` и `dbg_directory_miss_self_heal()`
  (ожидание: растут вместе). Это и есть оракул активации пути для любого
  последующего перф-замера.
- **Предлагаемое исправление:** разделить два вопроса.
  (а) Документация — обязательна и дешева: привести доки
  `DIRECTORY_AUTHORITATIVE_MISS`, `DIRECTORY_MISS_SELF_HEAL` и
  `DIRECTORY_MISS_FULL_SCAN_PERIOD` в соответствие с тем, что при
  маршрутизированной таблице доверие к отрицательному ответу директории
  отключено, и что в `production` эти счётчики означают не то, что
  написано.
  (б) Поведение — отдельной задачей с замером: доверять отрицательному
  ответу директории можно, если доказать, что ни одна неотдренированная
  терминальная публикация не может скрывать свободный блок искомого класса
  (например, сверять с per-сегментным «есть необработанная публикация»
  признаком вместо полного свипа). Это требование корректности, а не
  микрооптимизация, поэтому в §3 оно идёт с явной оговоркой.
- **Риск исправления:** (а) нулевой (только комментарии); (б) высокий —
  затрагивает инвариант «владелец не пропускает опубликованное
  освобождение», трогать только с доказательством и оракулом активации.
- **Статус относительно индексов:** **новый**. Ближайшие соседи —
  `docs/perf/OPEN_ITEMS.md` пункт 80 (патч S: скан начинается с первого
  слова payload — ровно этот же свип, но вопрос о доверии промаху там не
  поднимался) и пункт 78(c) (подсказка для Large-свипа). Ни один из них
  не владеет этим.

---

### fxx R12-04 — проигравшие гонку инициализации fallback-кучи крутятся без `yield_now()`

- **Severity:** P3
- **Категория:** живучесть/латентность под переподпиской CPU
- **Место:** `src/global/fallback.rs:303-305`
- **Механизм:**
  ```rust
  while INIT_STATE.load(Ordering::Acquire) == STATE_INITIALIZING {
      core::hint::spin_loop();
  }
  ```
  Это чистый спин без уступки планировщику. Победитель гонки между CAS и
  публикацией `READY` выполняет `HeapCore::new(...)` — то есть резервирует
  primordial-сегмент у ОС и инициализирует метаданные (сотни микросекунд в
  худшем случае, с системными вызовами). Если победителя снимут с
  процессора в этом окне, каждый проигравший сжигает свой квант целиком.
  Оба соседних примитива в крате так не делают: `ShardLock` крутится
  `SPINS_BEFORE_YIELD = 64` раз и затем вызывает `std::thread::yield_now()`
  (`src/registry/segment_route/shard_lock.rs:12,44-49`), и `LockGuard` в
  этом же файле `fallback.rs` построен по тому же правилу
  (`LOCK_TIGHT_SPINS = 64`, затем уступка — см. карточку
  `docs/perf/OPEN_ITEMS.md` пункт 66).
- **Сценарий отказа:** контейнер с 1 vCPU (или сильная переподписка), N
  потоков одновременно впервые доходят до fallback-кучи (TLS уже разрушен
  либо реестр исчерпан). Победитель снят с процессора; N−1 потоков
  крутятся до конца своих квантов, суммарно задерживая и победителя. Не
  дедлок (прогресс гарантирован), но ровно та разница между «спин» и
  «спин+уступка», которую крат уже признал в двух других местах.
- **Подтверждение:** прочитано; сравнение с двумя соседними примитивами в
  том же дереве. Замера нет (**НЕ ПОДТВЕРЖДЕНО**). Как проверить:
  прогнать существующий `tests/regression_fallback_init_unwind_guard.rs`-
  подобный сценарий на машине с ограничением в 1 CPU
  (`taskset -c 0` / Job Object на Windows) с N потоками и измерить
  задержку первого успешного `heap_ptr`.
- **Предлагаемое исправление:** применить тот же шаблон, что в
  `ShardLock`: счётчик, `spin_loop()` до порога, затем
  `std::thread::yield_now()` на каждой последующей итерации (под `no_std`
  — остаётся чистый спин, как и в `LockGuard`). Это ~5 строк, без
  изменения протокола состояний.
- **Риск исправления:** низкий. Упорядочивание не меняется (уступка не
  добавляет и не убирает ребра happens-before), путь холодный по
  определению (только первая инициализация fallback-кучи процесса).
- **Статус относительно индексов:** **новый**. `docs/perf/OPEN_ITEMS.md`
  пункт 66 владеет КАЛИБРОВКОЙ порога `LOCK_TIGHT_SPINS` у `LockGuard`
  (`with_heap`) — это другая петля, на уровень ниже; петля
  инициализации `heap_ptr_impl` там не упомянута.

---

### fxx R12-05 — `alloc_batch`/`realloc` используют неограниченный Large-свип там, где скалярный путь уже ограничен

- **Severity:** P3
- **Категория:** задержка (хвост распределения)
- **Место:** `src/registry/heap_core/alloc/batch.rs:160,276`;
  `src/registry/heap_core/free/realloc.rs:178,576` — вызовы
  `self.drain_large_sidecar_ingress()`;
  против `src/registry/heap_core/alloc/hot.rs:575,759`
  (`drain_large_sidecar_ingress_hot_bounded`);
  реализации — `src/alloc_core/alloc_core/sidecar_drain.rs:272-297`
  (полный свип) и `:302-353` (ограниченный, `LARGE_HOT_BUDGET`)
- **Механизм:** `drain_large_sidecar_ingress` проходит `next_active(Large,
  …)` по всей таблице до `table.count()` и для каждого активного
  Large-слота пытается `claim_large_route`. Скалярный путь промаха магазина
  был переведён (ответ на R11 P3-1/HS-A) на ограниченный курсор с бюджетом
  `LARGE_HOT_BUDGET` и сохранением позиции между вызовами; батч-путь и две
  точки `realloc` остались на полном свипе. То есть на куче с L активными
  Large-сегментами каждый `alloc_batch` платит O(L), и ровно тот пик
  задержки, который закрывали для скалярного пути, воспроизводится на
  батч-пути.
- **Сценарий отказа:** куча с большим числом живых Large-аллокаций
  (например, пул буферов по 4–64 MiB) плюс регулярные `alloc_batch` под
  `batch-api`: каждый батч делает полный обход Large-слотов с
  `claim_large_route` на каждом. Для `realloc` то же на холодной ветке.
- **Подтверждение:** прочитано (сравнение вызовов и двух реализаций).
  Замера нет (**НЕ ПОДТВЕРЖДЕНО**).
- **Предлагаемое исправление:** в `alloc_batch` использовать
  `drain_large_sidecar_ingress_hot_bounded()` (он уже есть на `HeapCore`,
  `src/registry/heap_core_xthread/sidecar_drain.rs:18-21`), а полный свип
  оставить как rescue — точно та же структура, что на скалярном пути
  (`hot.rs:759` ограниченный, `:786` rescue). Для `realloc` — решить
  отдельно: там свип стоит на пути, который и так делает OS-работу, так что
  выигрыш может быть в пределах шума.
- **Риск исправления:** низкий по корректности (ограниченный вариант уже
  в production на скалярном пути и сохраняет курсор, т.е. ни одна
  публикация не теряется — просто обрабатывается позже), но это изменение
  поведения опционального `batch-api`, поэтому нужен замер p99 батча плюс
  оракул активации (`dbg_large_sidecar_slot_inspections`,
  `dbg_large_sidecar_full_rescues` — оба уже есть).
- **Статус относительно индексов:** **новый** (как остаток). Родительский
  пункт — R11 P3-1 из `docs/reviews/2026-09-30-232702-src-review-xs-sol-round-11.md`,
  закрытый только для скалярного пути; в `docs/perf/OPEN_ITEMS.md` отдельной
  карточки на батч-путь нет (пункт 71 про `dealloc_batch` — другая сторона
  API и другой вопрос).

---

### fxx R12-06 — объявленный `unsafe`-шов tier-1 без единого `unsafe`

- **Severity:** P4
- **Категория:** нарушение конвенции `unsafe`-швов
- **Место:** `src/registry/heap_registry/counters.rs:4`
  (`#![allow(unsafe_code)]`); инвентарь — `src/lib.rs:250-251`,
  README §«Where unsafe lives»
- **Механизм:** файл несёт модульный `#![allow(unsafe_code)]` (tier-1 по
  классификации `CLAUDE.md`), но не содержит ни одного токена `unsafe`
  (проверено: `grep -E '\bunsafe\b'` по файлу без строк комментариев даёт
  0 совпадений). `CLAUDE.md` требует для tier-1 «each with a single
  documented reason to hold `unsafe`»; здесь причины нет вовсе, а шов
  при этом перечислен в двух инвентарях как действующий. Практический
  эффект: расширенное разрешение, которое компилятор больше не удерживает
  от нового `unsafe` в этом файле, и инвентарь, завышающий размер
  доверенной базы.
- **Сценарий отказа:** нет отказа рантайма. Ослабление
  compiler-enforced-гарантии, на которой построено заявление крата
  «unsafe живёт только в названных файлах».
- **Подтверждение:** прочитано + перечисление
  `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/` (самопроверяющая
  команда из `CLAUDE.md`) и подсчёт токенов `unsafe` по каждому файлу
  tier-1: `counters.rs` — 0; для сравнения `bootstrap/ensure.rs` — 1,
  `heap_slot.rs` — 1 (`unsafe impl Sync`), `tls_heap.rs` — 1 (см. R12-07).
- **Предлагаемое исправление:** снять `#![allow(unsafe_code)]` из
  `counters.rs` и соответствующую строку из инвентарей (`src/lib.rs`,
  README). Если сборка остаётся зелёной во всех конфигурациях — шов
  действительно был пустым.
- **Риск исправления:** минимальный; проверяется компиляцией (в т.ч.
  конфигурациями, где файл участвует: `alloc-global`, `production`,
  `--all-features`).
- **Статус относительно индексов:** **новый**. В
  `docs/correctness-open-items/TRACKED_hook_safety.md` ведётся учёт
  `dbg_*`-хуков и их гейтов, но не «пустых» швов tier-1.

---

### fxx R12-07 — `tls_heap.rs`: два взаимно противоречащих утверждения о своей `unsafe`-поверхности

- **Severity:** P4
- **Категория:** устаревшая документация внутри `unsafe`-шва
- **Место:** `src/global/tls_heap.rs:94-102` против `:637-651`
- **Механизм:** заголовок файла утверждает: «Since Ph4b (#2092) the
  `unsafe` surface here is empty: the only core access goes through
  `HeapLease::core` … a SAFE function». Через 500 строк, в обоснование
  `pub unsafe fn dbg_restore_local_for_test` (`:649`), написано
  противоположное: «It is covered by this file's existing tier-1
  `#![allow(unsafe_code)]` … `tls_heap.rs` already holds `unsafe` for the
  lease-core handoff + `trim_for_recycle`». Оба утверждения не могут быть
  верны: `HeapLease::core` безопасна (это и есть содержание Ph4b), а
  `trim_for_recycle` вызывается без `unsafe`. Фактическое состояние: файл
  держит ровно один `unsafe` — сигнатуру измерительного хука под
  `bench-internals`.
- **Сценарий отказа:** нет отказа рантайма. Читатель, проводящий аудит
  доверенной базы, получает из одного файла два несовместимых ответа о том,
  зачем здесь шов.
- **Подтверждение:** прочитано; перечисление токенов `unsafe` по файлу
  (ровно один, строка 649).
- **Предлагаемое исправление:** оставить одно обоснование — «шов держится
  ради `unsafe fn dbg_restore_local_for_test`: установка
  непроверенного `*mut HeapCore` как live-binding текущего потока» — и
  убрать из комментария `:643-646` ссылки на lease-core handoff и
  `trim_for_recycle` как на источники `unsafe`. Заодно решить вопрос
  симметрии: это ровно та форма, которую `CLAUDE.md` в похожих случаях
  закрывал item-scoped `#[allow(unsafe_code)]` (tier 2) — если шов
  tier-1 оставлять только ради хука под `bench-internals`, это стоит
  сказать прямо.
- **Риск исправления:** нулевой (только комментарии).
- **Статус относительно индексов:** **новый**.

---

### fxx R12-08 — `tagged-index-stack` в зависимостях `production` без потребителя; два устаревших описания обвязки

- **Severity:** P4
- **Категория:** мёртвая зависимость + расхождение код↔док
- **Место:** `Cargo.toml:198` (`alloc-global = [… "dep:tagged-index-stack"]`),
  `Cargo.toml:931-938` (описание обвязки), `src/lib.rs:112,173-183`
  (инвентарь), `src/registry/bootstrap/loom_shim.rs:237-467` (зеркало
  протокола), `src/kani_proofs.rs:177-221` (единственный второй
  потребитель), против `src/registry/heap_registry/mod.rs:10-11` («FREE
  discovery scans materialised chunks. No intrusive free list remains»)
- **Механизм:**
  1. В `src/` нет ни одной конкретной реализации `StackStorage` —
     перечисление `grep -rn "StackStorage<" src/` даёт совпадения
     исключительно внутри `loom_shim.rs` (где объявлены ЗЕРКАЛА трейтов и
     blanket-`impl StackOps for S: StackStorage`, строка 467), то есть у
     зеркала нет имплементора даже под `--cfg loom`.
  2. `tagged_index_stack::*` используется только из
     `loom_shim.rs:326` (`cfg(loom)`) и `kani_proofs.rs:182`
     (`cfg(kani)`). В обычной сборке `production` ни один путь на крату не
     ссылается, при этом она остаётся в замыкании зависимостей
     `alloc-global` (время сборки, граф зависимостей публикуемого крата).
  3. Документация описывает снятую обвязку: `src/lib.rs:183` — «sefer's
     registry free_slots uses it»; `Cargo.toml:931-933` — «registry's
     `free_slots` is now a `StackHead<16>`; `Registry` implements
     `StackStorage<16>` once, binding that head to the slot-resident
     `next_free` links». Ни `free_slots`, ни такой `impl` в `src/` больше
     нет (перечисление `grep -rn 'free_slots|next_free' src/` даёт только
     комментарии и не связанный `SlotState::Vacant{next_free}` в
     `concurrent/lock_free`).
  4. Следствие для верификации: loom-работа, которая моделирует head-протокол
     этого стека, моделирует протокол без production-имплементора — см. §5.
- **Сценарий отказа:** нет отказа рантайма. Завышенный граф зависимостей
  у публикуемого крата и две точки документации, по которым читатель
  будет искать несуществующую обвязку.
- **Подтверждение:** перечисления выше (две команды, вывод приведён в §8);
  прочитано `heap_registry/mod.rs`, `loom_shim.rs`, `kani_proofs.rs`.
- **Предлагаемое исправление:** решение владельца из двух:
  (а) если `cfg(kani)`-доказательства упаковки и `cfg(loom)`-зеркало
  нужно сохранить — перевести `tagged-index-stack` из `dep:` в
  `[target.'cfg(any(loom, kani))'.dependencies]` (или в
  `dev-dependencies`, если доказательства запускаются только локально) и
  исправить оба описания; (б) если обвязка снята окончательно — удалить
  зависимость, зеркало `StackHead`/`StackStorage`/`StackOps` в
  `loom_shim.rs:237-467` и `pack_proofs` из `kani_proofs.rs`, оставив в
  документации одну строку о том, что упаковка переехала в собственный
  крат и проверяется там. **Файл `Cargo.toml` этим раундом не правится**
  (вне области обзора) — это предложение для оркестратора.
- **Риск исправления:** (а) низкий; (б) средний — удаление снимает
  kani-покрытие упаковки на стороне sefer (в самом крате
  `tagged-index-stack` оно есть независимо).
- **Статус относительно индексов:** **новый** в этой части.
  `docs/correctness-open-items/TRACKED_publish_readiness.md` ведёт
  публикационные задачи по самой крате `tagged-index-stack` (#660/#661),
  но не вопрос «нужна ли она в графе зависимостей `production`».

---

### fxx R12-09 — описание нулевого состояния слота ссылается на удалённые поля

- **Severity:** P4
- **Категория:** устаревшая документация
- **Место:** `src/registry/bootstrap/chunk.rs:41-44`
  («`&'static` references into slot fields (`&slot.remote.thread_free`,
  `&slot.overflow`)»), `src/registry/bootstrap/ensure.rs:84-88`
  («`overflow = all-zero HeapOverflow`»), плюс
  `src/registry/bootstrap/mod.rs:129` («the `alloc-xthread`
  overflow-sidecar path below»), `src/registry/bootstrap/registry.rs:156`
  (`resolve_heap_overflow`)
- **Механизм:** терминальный переход снял `RemoteFreeRing`/`HeapOverflow`
  с `HeapSlot`; кросс-потоковый ингресс идёт через `RouteDirectory` и
  `SmallSidecar`. Документация обоснования «почему нулевая страница — валидное
  начальное состояние слота» по-прежнему перечисляет снятые поля как
  обоснование. Это самая чувствительная документация в файле: именно на ней
  держится аргумент, что чанк можно материализовать из OS-нулевой страницы
  без типизированной инициализации.
- **Сценарий отказа:** нет отказа рантайма; риск в том, что следующий,
  кто будет добавлять поле в `HeapSlot`, сверится с перечнем, который уже
  неполон/неверен, и пропустит поле, для которого нули не валидны.
- **Подтверждение:** прочитано; перечисление упоминаний
  `thread_free|overflow` по `src/registry/bootstrap/` (4 совпадения, все —
  комментарии).
- **Предлагаемое исправление:** переписать перечень полей в
  `chunk.rs:41-44` и `ensure.rs:84-88` под текущий состав `HeapSlot`
  (включая `next_free`, если он сохранён, и счётчики), оставив тот же
  аргумент, но с актуальным списком; поправить две оставшиеся ссылки на
  «overflow-sidecar path».
- **Риск исправления:** нулевой (только комментарии).
- **Статус относительно индексов:** **новый**.
  `docs/correctness-open-items/TRACKED_misc.md` пункт 154 («prose debt»)
  — ближайший по духу, но эти конкретные места в нём не перечислены.

---

### fxx R12-10 — нарушения конвенций проекта: `#[cfg(test)] mod tests` в `src/`, устаревший `allow(dead_code)`, снятый модуль в документации

- **Severity:** P4
- **Категория:** конвенции `CLAUDE.md`
- **Место:**
  - `src/alloc_core/large/reservation_state.rs:137-143` — два
    объявления `#[cfg(test)] #[path = "../../../tests/support/…"] mod tests;`
  - `src/registry/segment_route/large_state.rs:68-70` — третье такое же
  - `src/alloc_core/large/reservation_state.rs:2` —
    `#![allow(dead_code)] // Stage 2B is wired to the active ingress by the integrator`
  - `src/alloc_core/large/mod.rs:5` — «the cross-thread deferred-free
    Treiber stack (`deferred_large`)»
- **Механизм:**
  1. `CLAUDE.md` («Tests»): «Do not leave tests inline in the module file
     (`#[cfg(test)] mod tests` inside `src/*.rs`)». Тела тестов здесь
     действительно лежат в `tests/support/`, но само объявление —
     в `src/`, то есть ровно та конструкция, которую правило называет; три
     таких места.
  2. Модульный `#![allow(dead_code)]` в `reservation_state.rs`
     обоснован тем, что «Stage 2B is wired … by the integrator». На HEAD
     все девять методов `LargeReservationState` имеют живых вызывающих
     (`publish_pending`, `claim_pending`, `begin_reuse`, `finish_reuse` —
     из `src/registry/segment_route/large_state.rs:25-45`;
     `claim_live`, `cache_consumed`, `release_consumed`, `release_cached`,
     `release_initializing` — из
     `src/alloc_core/large/alloc_core_large.rs`,
     `src/alloc_core/alloc_core/mem/mem_impl.rs:268`,
     `src/alloc_core/alloc_core/lifecycle.rs:516`,
     `src/alloc_core/large/alloc_core_large_cache_eviction.rs:50,241`).
     То есть разрешение больше ничего не прикрывает, но продолжает
     скрывать будущий мёртвый код в самом чувствительном файле
     Large-протокола.
  3. Модуль `deferred_large` отсутствует; упоминания остались только в
     документации (`src/alloc_core/large/mod.rs:5`,
     `src/registry/bootstrap/mod.rs:132`,
     `src/alloc_core/platform/node.rs:467,502,575`).
- **Сценарий отказа:** нет отказа рантайма.
- **Подтверждение:** прочитано; перечисление `#[cfg(test)]` по `src/`
  (3 совпадения, все три перечислены выше); перечисление вызывающих для
  каждого метода `LargeReservationState`; перечисление `deferred_large`
  по `src/` (5 совпадений, все — комментарии).
- **Предлагаемое исправление:** (1) перенести объявления `mod tests`
  в сами файлы `tests/support/*.rs` как обычные интеграционные тесты
  (или, если они требуют `pub(crate)`-внутренностей — вынести их по
  образцу `src/kani_proofs.rs`, который `CLAUDE.md` признаёт
  санкционированным исключением, и описать это как исключение); (2) снять
  `#![allow(dead_code)]` и убедиться, что сборка зелёная во всех
  конфигурациях; (3) заменить упоминания `deferred_large` на актуальный
  механизм (`LargeState`/терминальный ингресс).
- **Риск исправления:** (1) средний механически (перенос тестов), (2)–(3)
  нулевой/минимальный.
- **Статус относительно индексов:** **новый**.

---

## 3. Возможности ускорения

| Идея | Место | Ожидаемый эффект | Риск | Связь с индексом | Как мерить |
|---|---|---|---|---|---|
| O-1. Вернуть маску вместо двухуровневого поиска в `clear_magazine_on_issue` | `src/registry/heap_core/alloc/hot.rs:47-50` | снять сравнение ключа Tier-1 и зонд Tier-2 с КАЖДОГО попадания магазина; относительно измеренных 12.19 Ir/hit (пункт 17) это возврат к базе, на профилях с ≥16 горячими сегментами (пункт 1/R32-10) — существенно больше | низкий | `OPEN_ITEMS.md` пункты 17 и 1 — ни один не владеет именно заменой маски на поиск; это НЕ переоткрытие отвергнутой «отложенной очистки бита» (R3 NO-GO) | `npm run iai` `small_churn_*` до/после; оракул активации — `dbg_contains_base_tier1_hits`/`_misses` + `dbg_small_class_count` |
| O-2. Ограниченный Large-ингресс на батч-пути | `src/registry/heap_core/alloc/batch.rs:160,276` | убрать O(L) обход Large-слотов из каждого `alloc_batch`; хвост задержки на кучах с большим L | низкий | остаток R11 P3-1/HS-A (скалярный путь закрыт) | p99 `alloc_batch` под `batch-api`; оракул — `dbg_large_sidecar_slot_inspections`, `dbg_large_sidecar_full_rescues` |
| O-3. Проверка «гранула выдана» в `publish_small` | `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs:103-109` | нейтрально-положительно: мусорное освобождение отбрасывается ДО AcqRel-RMW по разделяемой строке кэша (и это же исправление R12-01) | низкий | новый | A/B кросс-потокового free-профиля; оракул — `FOREIGN_OR_UNROUTABLE_FREES` |
| O-4. Доверие отрицательному ответу директории при маршрутизированной таблице | `src/alloc_core/small/alloc_core_small/find_segment.rs:391-416` | снять O(S × W) с каждого промаха класса в `production` (в пределе — весь смысл директории) | **высокий** — требование корректности, не микрооптимизация: нужно доказать, что неотдренированная терминальная публикация не может скрывать свободный блок | см. R12-03; соседние пункты 80 и 78(c) этим не владеют | только после доказательства: `dbg_directory_authoritative_miss` > 0 как оракул активации + `dbg_full_scan_slots_examined` как метрика |
| O-5. Уступка планировщику в петле проигравших `heap_ptr_impl` | `src/global/fallback.rs:303-305` | устранить сжигание квантов на переподписанном CPU | низкий | сосед — пункт 66 (калибровка `LOCK_TIGHT_SPINS` у `LockGuard`), это другая петля | задержка первого `heap_ptr` при N потоках на 1 CPU |

**Уже занятые/отвергнутые идеи, которые я НЕ предлагаю повторно**
(сверено с `docs/perf/OPEN_ITEMS.md`): summary-слово для пропуска пустых
payload-слов при свипе (пункт 80 — «триггера нет», и пункт 9/R15-1 §7);
компактное представление Small-sidecar (пункт 78(b)); подсказка/очередь
вместо O(L)-скана Large (пункт 78(c) — с зафиксированным требованием
корректности); отложенная очистка бита магазина (пункт 17, R3 NO-GO);
слияние `AllocBitmap`/`MagazineBitmap` (пункт 45/F1b); один проход в
`finish_magazine_refill` (пункт 72); `#[inline(always)]` на холодных телах
(пункт 67); повторное использование пулированных сегментов как цели
carve (пункт 65/R1-03); свипы `pool_segments`/`headroom` (пункты 13, 27,
29, 30).

### Подробнее по O-1

Текущая реализация делает на попадании магазина: `segment_base_of_ptr`
(маска) → `cache_index` → сравнение `own_cache_keys[idx]` → проверка на
null → (при промахе) `hash_find`. Из этого для снятия бита резидентности
нужен только результат маски: биты СТАВЯТСЯ через маску
(`finish_magazine_refill`, `hot.rs:113`), значит для согласованности
достаточно её же. Тождественность маски и сохранённого корня для
Small-сегментов следует из `register` (`payload == base`, ключ хэша
`segment_base_of_ptr(base) == base`, так как Small/Primordial всегда
`SEGMENT`-выровнены) — смещённый корень бывает только у Large при
`align >= SEGMENT`, а Large в магазине не бывает. Поэтому замена
безопасна при `debug_assert_eq!` на согласие.

### Подробнее по O-4

Ограничение, которое нельзя потерять: владелец обязан увидеть каждую
опубликованную терминальную публикацию. Полный свип сейчас играет роль
«недостающего индикатора»: директория знает только о состоянии
`BinTable`, но не о битах `pending`, которые ещё не отдренированы.
Любой вариант доверия промаху должен иметь per-сегментный (или
per-маршрутный) признак «есть необработанная публикация», обновляемый
производителем в том же RMW, что и бит, и читаемый владельцем ДО решения
«скан не нужен». Это ровно то требование, которое уже записано в
`docs/perf/OPEN_ITEMS.md` пункте 78(c) для Large-подсказки, и его надо
переиспользовать, а не выводить заново.

---

## 4. Пахнущий код и нарушения конвенций

1. **Три `#[cfg(test)] mod tests` внутри `src/`** — fxx R12-10(1).
   Единственное санкционированное `CLAUDE.md` исключение такого вида —
   `#[cfg(kani)]`-гарнессы; эти три к нему не относятся.
2. **Модульный `#![allow(dead_code)]` в
   `src/alloc_core/large/reservation_state.rs:2`** — fxx R12-10(2):
   обоснование («Stage 2B is wired … by the integrator») устарело, все
   методы имеют вызывающих.
3. **Пустой шов `unsafe` tier-1** — fxx R12-06.
4. **Два противоречащих комментария о `unsafe`-поверхности в одном
   файле** — fxx R12-07.
5. **Устаревшие описания снятых полей/модулей** — fxx R12-08, R12-09,
   R12-10(3). Это не единичные опечатки, а системный долг после
   терминального перехода: ровно те места, где документация несёт
   обоснование безопасности (нулевая страница валидна; зачем шов; где
   живёт упаковка).
6. **`abort()` как механизм обработки входа.** В терминальном дереве
   `std::process::abort()` используется часто и в основном уместно (как
   детектор нарушенного инварианта владельцем). Но два места стоят на
   границе: `BitmapCut::pop` (`bitmap_cut.rs:27-29`) — достижим мусорным
   входом производителя (R12-01); `AllocCore::reclaim_sidecar_record`
   (`alloc_core_small_reclaim.rs:24-45`) — пять `abort()` на
   геометрических проверках, т.е. те же условия, которые own-thread
   `dealloc_small` трактует как no-op. Для owner-only входов это
   оправдано; как только вход может быть производным от чужого
   указателя — нет. Это та же асимметрия, что в R12-01, выписанная как
   стилевое наблюдение.
7. **`Profile::DiverseTurnover`** (`src/alloc_core/config/profile.rs:240-316`)
   разрешается в то же `headroom_bytes`, что и `Default`; её реальный
   эффект целиком зависит от compile-time-фичи `large-cache-extended`.
   Документация варианта это честно и подробно объясняет (и это
   осознанное решение задачи #491), но публичный `enum`-вариант, выбор
   которого при выключенной фиче не меняет ничего, остаётся формой,
   приглашающей к рантайм-ожиданию. Наблюдение, не находка.
8. **Размер функций на границе читаемости:**
   `find_segment_with_free_impl` (`find_segment.rs:186-569`, ~380 строк с
   пятью `#[cfg]`-измерениями: директория/NUMA/decommit/stats/xthread) и
   `alloc_large` (`alloc_core_large.rs:136-487`). Обе — именно те места,
   где в этом раунде нашлись расхождение код↔док (R12-03) и
   неочевидный поток откатов. Декомпозиция здесь была бы не косметикой:
   `trust_negative`-решение (6 строк) определяет поведение всего
   production и сейчас похоронено на 200-й строке функции.

---

## 5. Пробелы в тестах/верификации

1. **Кросс-потоковая публикация невыданной гранулы не покрыта.**
   Перечисление по `tests/`: файлы, упоминающие «interior», —
   `r11_4_dealloc_batch_hardened_guards.rs`,
   `regression_r4_2_null_base_foreign_guard.rs`, `size_classes_lookup.rs`;
   ни один не комбинирует внутренний указатель с кросс-потоковым
   маршрутом. Нужен подпроцессный тест (см. R12-01 «как проверить»).
   Контрфактуальность обеспечивается самим `abort()`: без исправления
   подпроцесс падает, с исправлением возвращает ненулевой
   `FOREIGN_OR_UNROUTABLE_FREES`.
2. **`clear_magazine_on_issue` не покрыт ничем по имени.** Перечисление
   по `tests/` по `clear_magazine_on_issue|canonical_root_for` — ноль
   совпадений. В частности нет теста, который удерживает ≥16 горячих
   сегментов и проверяет, что путь попадания магазина не уходит в Tier-2
   (это и оракул для O-1).
3. **Петля проигравших в `heap_ptr_impl` покрыта только по ветке
   разворачивания.** `tests/regression_fallback_init_unwind_guard.rs`
   проверяет откат `INIT_STATE`, но не поведение самой петли под
   переподпиской; теста с ограничением на 1 CPU нет.
4. **Диагностические счётчики директории не имеют теста на собственное
   значение в `production`.** Если бы существовал тест «в
   production-конфигурации `dbg_directory_authoritative_miss()` строго
   растёт на промахах», R12-03 был бы найден автоматически. Сейчас
   счётчик тождественно нулевой, и ничто этого не фиксирует.
5. **Неограниченный Large-свип на батч-пути не имеет оракула.**
   `dbg_large_sidecar_slot_inspections` существует
   (`alloc_core_core_diag/table_diag.rs:101-104`), но ни один тест не
   сравнивает число инспекций на скалярном и батч-путях — то есть
   расхождение R12-05 не зафиксировано ни тестом, ни замером.
6. **loom-покрытие head-протокола стека индексов вакуумно
   относительно поставляемого кода.** У зеркала
   `StackHead`/`StackStorage`/`StackOps` в
   `src/registry/bootstrap/loom_shim.rs:237-467` нет имплементора ни в
   production, ни под `--cfg loom` (перечисление в §8). Соответствующая
   loom-работа моделирует протокол, который крат больше не исполняет;
   это не ложно-зелёный тест, но потраченное покрытие (и аргумент в
   пользу варианта (б) из R12-08).
7. **Известный принятый остаток (не новый):** `global`/`fallback` не
   покрыты miri/loom —
   `docs/correctness-open-items/TRACKED_verification_coverage.md`
   пункт 17. Находки R12-04 и R12-01 обе лежат в этой зоне, что
   поднимает цену остатка: один путь — холодная петля инициализации,
   другой — кросс-потоковая публикация.

---

## 6. Что проверено и чисто

Перечислено только то, что я читал целиком и с трассировкой, а не по
заголовкам.

- **Фасад `GlobalAlloc` и TLS-резолверы.** `src/global/sefer_alloc/*`,
  `src/global/tls_heap.rs`: все три резолвера различают `null` (никогда не
  привязан) и `TORN` (идёт разрушение) одной ветвью; `finish_bind`
  публикует `LOCAL` до вооружения `GUARD` и корректно сворачивает обе
  ветви отказа `try_with` (при отказе `GUARD` — сначала `LOCAL = null`,
  потом `drop(lease)`); `AbandonGuard::drop` выдерживает порядок
  TORN → `trim_for_recycle` → `drop(lease)`; вложенная привязка
  (реентерабельность внутри claim) распознаётся и внешний лиз
  освобождается, а не публикуется поверх внутреннего.
- **Протокол лизов реестра.** `HeapLease::drop` — LIVE→FREE Release-CAS,
  `abort` при проигрыше CAS и при `thread::panicking()`;
  `MaintenanceLease` — FREE→MAINTENANCE и безопасный `with_core`;
  `claim_impl`/`pick_with_saturation` — машина состояний
  EMPTY/OWNED/INITIALIZING/FREE/MAINTENANCE с `initialised`-гейтом
  Release/Acquire, восстановление при OOM чанка
  (`scan_claim_recovery_free`), версионированная подсказка насыщения.
- **Терминальный маршрут (кроме входа, выписанного в R12-01).**
  `RouteDirectory`: 64 шарда, неаллоцирующий `ShardLock`, двухуровневый
  отсортированный индекс, split/merge, `EntryHandle` со счётчиком и
  проверкой инкарнации после пина (`directory.rs:882-886`);
  `publish_foreign` завершается без рекурсии (fallback-ветка проверяется
  через `try_with_heap`, занятый fallback не ожидается, а публикуется);
  `LargeState`/`LargeReservationState` — полная решётка фаз
  Unused/Initializing/Live/Pending/Consuming/Cached/Released с генерацией
  в одном слове, все переходы — CAS с ожидаемым значением (`transition`),
  монотонная генерация без обёртки (`next_large_generation`).
- **Large-кэш и его откаты.** Путь попадания
  (`alloc_core_large.rs:216-483`): `begin_large_reuse` → при `None`
  корректный `release_cached` + `release_segment` + падение в медленный
  путь; при отказе `register_payload` — `release_initializing` +
  `release_segment`; `span_usable`/`reserved_capacity` переносятся из
  слота, а не пересчитываются (баг #134 закрыт); F12-целевые записи
  выполняются ДО регистрации (UBFIX-6 сохранён), `segment_id`
  патчится одним словом после; `debug_assertions`-пин на перенос полей
  на месте. Вытеснение: `oldest_occupied_slot` по битовой маске
  занятости, `evict_at_least`/`evict_one_oldest` снимают фазу
  `Cached → Released` до `release_segment`.
- **Политика затухания.** `maybe_decay_large_cache`: быстрый выход по
  headroom, страйд чтения часов, исключение для нулевого интервала,
  catch-up с ограничением `DECAY_CATCHUP_MAX_STEPS = 8` и честным
  переносом остатка (`t + interval * due`, не `now`);
  `maybe_decay_small_pool` — ранний выход при пустом пуле, прайминг
  таймера, выселение хвоста списка (FIFO) за O(1).
- **Пул пустых Small-сегментов.** Интрузивный двусвязный список в
  заголовках; `release_or_pool_empty_segment` с защитой от двойного
  включения; `unpool_if_present` с O(1)-тестом членства
  (`pool_head == base || pool_prev != null`), исчерпывающим по
  построению; `finalize_old_cursor_if_orphaned` (oxx R2-01);
  `drain_small_pool` как реклеймируемый резерв для OS-отказов
  (и в numa-, и в не-numa-ветках `reserve_small_segment_impl`, и в
  `alloc_large_slow`).
- **Путь освобождения own-thread.** `dealloc_routing` →
  `canonical_block_of` (корень из таблицы, блок — производный от корня) →
  `dealloc_own_thread_with_base`; цепочка оракулов (битовая карта
  магазина, `off >= bump`, `is_free`) и безусловные границы H-1/M-1;
  `flush_class` с группировкой в однородные по сегменту пробеги,
  доказательством байт-идентичности сплайса и `recycled_bases` против
  повторного касания утилизированной базы.
- **Carve/refill.** `carve_block`/`carve_batch`: `prepare_small_issue` до
  любой мутации, корректный порядок «recommit → grow-commit → bump →
  live → page-map → выдача», честный OOM при отказе коммита без
  продвижения курсора; `drain_freelist_batch` с предварительным
  `prepare` всей цепочки (откат без частичных эффектов);
  `refill_class_bump_impl` с латчем `free_exhausted` и доказательством,
  почему повторный дренаж после латча тавтологичен.
- **Директория сегментов (структура).** `set_bit`/`clear_bit`/
  `clear_bit_all_nodes`/`clear_slot` синхронно ведут счётчик активных
  бит по бакетам (R13-2) и освобождают слот `node_ids` при нуле; чтение
  массива слов по значению без удержания `&SegmentDirectory` через
  вызовы, способные взять `&mut` (R12-1); `init_node_ids_raw` через
  `addr_of_mut!` без промежуточной ссылки на невалидные байты.
- **`SegmentTable`.** `register`/`register_payload`/`unregister`/
  `recycle`, backward-shift удаление из хэша, инвалидация `own_cache`,
  `ActiveKindIndex`, `IssueTransaction` (prepare/commit с проверкой
  свидетеля), `RouteSlots` как owner-only non-Copy хранилище.
- **Sidecar-битовая карта.** `issue`/`publish`/`scan`/`scan_from`,
  AcqRel-протокол, `ClassLeaves` с uniform-байтом на лист и System-листом
  при смешении (включая корректную инициализацию листа предыдущим
  uniform-значением и точное освобождение в `Drop`); `System` здесь —
  аллокатор std, а не `#[global_allocator]`, поэтому реентерабельности
  нет.
- **`MaintenanceService`.** Явный `start_maintenance()`,
  IDLE/STARTING/RUNNING, `WorkerGuard` c `abort` при выходе рабочего,
  `maintenance_pass` с бюджетом слотов и ветка `fallback::try_with_heap`.
- **Fallback-куча (кроме петли R12-04).** `INIT_STATE` с
  `InitStateGuard`, корректный откат на OOM в UNINIT (а не зависание в
  INITIALIZING), `LockGuard` со спином и уступкой,
  `with_heap_config`/`CONFIG_CONFLICTS`.
- **`concurrent/` (legacy-уровень).** `AtomicSlot::try_evict_at`
  (единственная точка линеаризации, доказательство единственности
  победителя CAS, отказ `Stale` на нулевом `old`), seqlock-валидация в
  `read_with`, политика насыщения генерации (и её документированное
  расхождение между epoch- и lock-free-уровнями),
  `LockFreeRegion` на персистентном 16-арном дереве страниц,
  `ShardedRegion` с per-регионной привязкой и `ErasedGuard`,
  освобождающим ВСЕ выигранные токены.
- **`kani_proofs.rs`.** Гарнессы соответствуют своим заявлениям;
  модульная документация честно перечисляет, что НЕ доказывается
  (контракты вызывающих, конкурентность).
- **Реентерабельность NUMA — проверено и чисто.** Гипотеза «инициализация
  топологии NUMA аллоцирует и реентерабельно входит в
  `OnceLock::get_or_init` того же cell» не подтвердилась:
  `crates/numa-shim/src/lib.rs` (строки ~1497-1557) документирует и
  реализует аллокационно-свободный обратный индекс именно по этой причине
  (задачи #723/#777), а журнал mock-вызовов переведён на фиксированное
  inline-хранилище ровно потому, что рост `Vec` реентерабельно входил в
  глобальный аллокатор. `src/alloc_core/platform/numa.rs:34-35` —
  безопасная делегация.
- **Арифметика размеров.** `alloc_large`: `checked_add` для
  `hdr_aligned + size`, `div_ceil`/`align_up` по рантайм-странице,
  `saturating_mul` для `reserved_capacity_target` с потолком
  `LARGE_RESERVED_CAP_BYTES`; `lock_free_capacity::checked_total_slots`
  через `u64` с проверкой на `u32`; константные `assert!` на
  `LAZY_FIRST_CHUNK`/`GROW_CHUNK`/`WORDS`/`GRANULES`.

---

## 7. Рекомендуемая очередность исправлений

1. **fxx R12-01** — проверка «гранула выдана» в `publish_small`
   (+ подпроцессный тест). Единственная находка, превращающая
   нарушение контракта вызывающего в падение процесса.
2. **fxx R12-02, часть «паника»** — убрать `expect` с пути попадания
   магазина (маска + `debug_assert_eq!`). Дешево, снимает единственную
   точку паники с горячего пути `GlobalAlloc`.
3. **fxx R12-03, часть (а)** — привести документацию трёх счётчиков
   директории в соответствие с фактическим поведением `production`.
   Только комментарии; устраняет ложные показания для всех будущих
   раундов.
4. **fxx R12-04** — уступка планировщику в петле проигравших
   `heap_ptr_impl` (по образцу `ShardLock`).
5. **fxx R12-06 + R12-07** — снять пустой шов tier-1 и привести
   обоснование `tls_heap.rs` к одной непротиворечивой версии (вместе:
   обе находки про один и тот же инвентарь `unsafe`).
6. **fxx R12-09** — актуализировать описание нулевого состояния слота
   (самая чувствительная из устаревших документаций: на ней держится
   аргумент безопасности материализации чанка).
7. **fxx R12-05** — ограниченный Large-ингресс на батч-пути (с замером и
   оракулом активации).
8. **fxx R12-08** — решение владельца по `tagged-index-stack`
   (перенос гейта зависимости либо удаление обвязки вместе с мёртвым
   зеркалом и `pack_proofs`); правка `Cargo.toml` — за оркестратором.
9. **fxx R12-10** — конвенции: три `mod tests` из `src/`, снятие
   устаревшего `allow(dead_code)`, упоминания `deferred_large`.
10. **fxx R12-02, часть «стоимость» + O-1/O-4** — перф-работа с
    замерами: сначала O-1 (локально, низкий риск), затем O-4 только
    после доказательства требования корректности из §3.

---

## 8. Приложение: использованные команды и ключевой вывод

Пути ниже — относительно корня репозитория; префикс рабочего дерева
заменён на `<repo>`.

**База и инвентарь**

```
git rev-parse HEAD
# b797f21326c7d47eb2bbb699b289f18bc1e8df8d
git status --porcelain
# (пусто на входе; на выходе — только этот файл)
git ls-files 'src/*.rs' 'src/**/*.rs' | wc -l
# 168
git ls-files 'src/*.rs' 'src/**/*.rs' | xargs wc -l | tail -1
#  42843 total
rustc --version && cargo --version
# rustc 1.97.0 (2d8144b78 2026-07-07)
# cargo 1.97.0 (c980f4866 2026-06-30)
```

**Перечисление `unsafe`-швов (самопроверяющая команда из `CLAUDE.md`)**

```
grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/
```

Tier-1 (модульные `#![...]`) в `src/` — 22 файла; число токенов `unsafe`
в каждом (строки комментариев исключены):

```
  4  src/alloc_core/large/large_cache_extended.rs
 26  src/alloc_core/platform/node.rs
 14  src/alloc_core/platform/os.rs
  7  src/alloc_core/platform/sidecar.rs
  9  src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs
 13  src/alloc_core/segment/segment_table/route_slots.rs
  6  src/concurrent/epoch/hand.rs
 15  src/global/exact_object/exact_shard.rs
  5  src/global/exact_object/narrow.rs
  5  src/global/fallback.rs
  5  src/global/sefer_alloc/batch.rs
 15  src/global/sefer_alloc/global_alloc.rs
  1  src/global/tls_heap.rs            <- fxx R12-07
  1  src/registry/bootstrap/ensure.rs
  5  src/registry/bootstrap/loom_shim.rs
  7  src/registry/bootstrap/registry.rs
  9  src/registry/heap_registry/claim.rs
  0  src/registry/heap_registry/counters.rs   <- fxx R12-06
  1  src/registry/heap_slot.rs
 38  src/registry/segment_route/directory.rs
  4  src/registry/segment_route/shard_lock.rs
  5  src/registry/segment_route/small_sidecar.rs
```

**fxx R12-08: отсутствие потребителя `tagged-index-stack` в `src/`**

```
grep -rn "StackStorage<" src/ | grep -v loom_shim
# (пусто)
grep -rn "tagged_index_stack" src/
# src/kani_proofs.rs:182   (cfg(kani))
# src/registry/bootstrap/loom_shim.rs:326  (cfg(loom))
grep -rn 'free_slots' src/
# только комментарии (src/lib.rs:183, bootstrap/mod.rs:30, kani_proofs.rs, …)
```

**fxx R12-09: устаревшие поля слота**

```
grep -rn 'thread_free|overflow' src/registry/bootstrap/
# registry.rs:156  (resolve_heap_overflow)
# mod.rs:129       (overflow-sidecar path)
# ensure.rs:88     (overflow = all-zero HeapOverflow)
# chunk.rs:42-43   (&slot.remote.thread_free, &slot.overflow)
```

**fxx R12-10: `#[cfg(test)]` внутри `src/`**

```
grep -rn --include='*.rs' -A2 '#\[cfg\(test\)\]' src/
# src/registry/segment_route/large_state.rs:68   mod tests  (path tests/support/r6_route_large_adapter.rs)
# src/alloc_core/large/reservation_state.rs:137  mod tests  (path tests/support/r6_terminal_large_state.rs)
# src/alloc_core/large/reservation_state.rs:141  mod credit_tests (path tests/support/r6_large_credit_state.rs)
```

**fxx R12-05: ограниченный против полного Large-ингресса**

```
grep -rn 'drain_large_sidecar_ingress' src/registry/
# hot.rs:250  (полный, холодный путь)
# hot.rs:575,759  (ограниченный, hot_bounded)
# hot.rs:786     (rescue)
# batch.rs:160,276      (полный)      <- fxx R12-05
# free/realloc.rs:178,576 (полный)    <- fxx R12-05
```

**Покрытие тестами (для §5)**

```
git ls-files 'tests/*.rs' | xargs grep -l 'clear_magazine_on_issue|canonical_root_for'
# (пусто)
git ls-files 'tests/*.rs' | xargs grep -l 'interior' | xargs grep -l 'foreign|remote|cross.thread|xthread|spawn'
# tests/r11_4_dealloc_batch_hardened_guards.rs
# tests/regression_r4_2_null_base_foreign_guard.rs
# tests/size_classes_lookup.rs
git ls-files 'tests/*.rs' | xargs grep -l 'INIT_STATE|STATE_INITIALIZING'
# tests/regression_fallback_init_unwind_guard.rs
```

**Чего я не запускал (и почему):** `npm run check`, полный
`cargo test --all-features`, бенчмарки, miri/loom/kani — этот раунд
основан на сплошном чтении исходников и на уже существующих измерениях
из `docs/perf/*`; каждая карточка §2 отдельно указывает, подтверждена она
чтением или остаётся **НЕ ПОДТВЕРЖДЕННОЙ**, и как её проверить дешевле
всего. Временных изменений в `src/` не вносилось, временных тестов в
`tests/` не создавалось; `git status` перед коммитом содержит только этот
файл.
