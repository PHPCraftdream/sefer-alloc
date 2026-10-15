# Раунд 6 — сводный план исправлений по ревью `src/`

Дата: 2026-10-15. Ревизия, на которой сделаны все шесть отчётов: `132ab85b`. Номера строк ниже даны по ней и могли сдвинуться. Перед правкой найди место по имени функции.

Источники (в каждом — полный механизм, сценарий отказа и доказательная база):

| Код | Отчёт | Находок |
|---|---|---|
| MS | [memory_safety_review.md](memory_safety_review.md) | 6 (Low 2, Info 4) |
| CC | [concurrency_review.md](concurrency_review.md) | 9 (Medium 2, Low 5, Info 2) |
| CR | [correctness_review.md](correctness_review.md) | 12 (Medium 1, Low 7, Info 4) |
| PF | [performance_review.md](performance_review.md) | 10 (High 2, Medium 4, Low 4) |
| CQ | [code_quality_review.md](code_quality_review.md) | 33 (Medium 9, Low 20, Info 4) |
| SEC | [api_security_review.md](api_security_review.md) | 16 (Medium 4, Low 8, Info 4) |

Ни один отчёт не нашёл Critical. Ни один не нашёл способа вызвать UB из safe-кода в `production` без `internals`. Все отчёты — статическое чтение: ничего не собиралось и не запускалось, ни одна находка не воспроизведена.

**Об идентификаторах.** В коде и `CHANGELOG.md` уже есть исторические метки `R6-MS-1..5` и `R6-CQ-2/7a/7b/7c` от июльской ремедиации. Это другое пространство имён. Ссылаясь на находку этого раунда, указывай путь отчёта (`docs/agent_reviews_round6/<file>.md`, ID) или задачу `R6-Txx` из этого файла.

## Приоритеты

- **P0** — Critical: UB или порча памяти на корректном входе. В этом раунде нет.
- **P1** — High: наибольший выигрыш или риск, брать первым.
- **P2** — Medium: реальный дефект деградации или отказа, внешне достижимый DoS, крупный долг сопровождения.
- **P3** — Low: hardening при нарушенном контракте, узкие режимы, мелкая оптимизация, чистка.
- **P4** — Info: документация, покрытие верификации, гипотезы.

Если одна задача объединяет находки разной серьёзности, ей дан приоритет самой серьёзной.

## Общие правила для исполнителя

Полностью правила в `CLAUDE.md`. Ниже то, что касается этих задач.

- **Тяжёлые прогоны только в GitHub CI.** Гейты (iai, Ir-судьи) запускаются только как PR-проверки: метка `gate-run` и файл `.github/gate-run.json`.
- **Перед пушем** — `npm run check`. После слияния проверить зелёный CI на landing SHA.
- **Префикс коммита (R30-12)** проверяет `scripts/verify-commit-prefixes.mjs`:
  - `perf(runtime)` — изменение пути, который уходит в `production`;
  - `fix(perf)` — исправление без заявленного ускорения;
  - `fix` / `test` — корректность;
  - `docs:` — коммит, который НЕ трогает `src/` и `Cargo.toml`, иначе CI красный;
  - `build:` — правки `Cargo.toml`.
- **Perf-изменение требует** преррегистрации или гейта, path-activation oracle, указания слоя (`AllocCore` / `HeapCore` / `#[global_allocator]`) и одного режима для цены и выгоды. Таблицы отчёта генерируются скриптом.
- **Флаги не добавлять**, если изменение может быть эффективным дефолтом (решение владельца).
- **Каждое исправление — с тестом**, который падает без исправления (counterfactual). Протоколы — с loom-моделью.
- **Новые открытые пункты** вносить в `docs/perf/OPEN_ITEMS.md` или `docs/CORRECTNESS_OPEN_ITEMS.md` в том же коммите (задача R6-T21).
- **Отвергнутое повторно не предлагать:** C′/C''/C‴ (сводка у producer), `TCACHE_CAP`↑, `FLUSH_N`, DualBitmap, hint-ы `find_segment_with_free` и другие. Полный список — `performance_review.md` §6.

## Сводная таблица

| ID | P | Источник | Кратко |
|---|---|---|---|
| R6-T01 | P1 | PF-1 | Убрать мёртвую запись владельца в `owner_state` на refill и Large alloc |
| R6-T02 | P1 | PF-2 (+R17-OPT-01, OPT-03) | Пропуск drain сегмента по эпохе pin-ов в `Entry.refs` — **нужно решение владельца** |
| R6-T03 | P2 | CC-1 | OOM конструктора `HeapCore` монополизирует `reuse_hint` |
| R6-T04 | P2 | CC-2, CR-8, SEC-12 | На выходе процесса (Windows) null из Own/Fallback/`realloc` → abort вместо escape в `System` |
| R6-T05 | P2 | SEC-1 | Own-thread small free берёт класс из `Layout`, interior/mismatched free → перекрывающиеся выдачи |
| R6-T06 | P2 | SEC-3 | Free-list-ссылки сырые и непроверяемые вне `hardened` (poisoning, утечка адресов) |
| R6-T07 | P2 | SEC-4 | Больше 4096 живых потоков → сериализация на fallback-спинлоке и claim на каждой аллокации |
| R6-T08 | P2 | SEC-5 | Large чуть больше `SMALL_MAX` резервирует и коммитит кратно 4 MiB; предел 4095 → внешний abort |
| R6-T09 | P2 | CR-1, SEC-11(а) | `dealloc_batch`/`alloc_batch` обходят `ExactNarrow` (`exact-object-proto` + `batch-api`) |
| R6-T10 | P2 | PF-3 | Полный Large-sweep с чтением заголовков на каждом Large alloc |
| R6-T11 | P2 | PF-4 | Раскладка Tier-1 `own_cache`: два массива + проверка null на каждом free |
| R6-T12 | P2 | PF-5 (+R17-OPT-08) | Инварианты сегмента пересчитываются на каждой записи owner-drain |
| R6-T13 | P2 | PF-6 | Вероятный bounds-check индекса magazine-слота на hit/push (проверить asm) |
| R6-T14 | P2 | CQ-29, MS-2, SEC-14 | Паникующие конструкции на путях `GlobalAlloc` → `fatal` |
| R6-T15 | P2 | CQ-28 | 108 голых `process::abort()` без диагностики → `fatal(msg)` + lexical-guard |
| R6-T16 | P2 | CQ-8, CQ-9 | Blanket `#[allow(dead_code, unused_imports)]` в `lib.rs` глушит lint во всём `production` |
| R6-T17 | P2 | CQ-12/13/14/16/17, CR-11, CR-12, MS-6(SAFETY) | Устаревшие комментарии и контракты `unsafe` (ring/TFS/HeapOverflow, bootstrap, `GlobalAlloc` SAFETY) |
| R6-T18 | P2 | CQ-5 | Три копии цикла слива Small sidecar |
| R6-T19 | P2 | CQ-7 | Функции на 180–424 строки с плотной cfg-сеткой |
| R6-T20 | P2 | CQ-22 | Предикат `medium_promotion_reachable` скопирован вручную 18 раз |
| R6-T21 | P2 | процесс | Внести находки раунда 6 в индексы открытых пунктов |
| R6-T22 | P3 | MS-1 | `ptr::add` по смещению из входного адреса до валидации |
| R6-T23 | P3 | CC-3, SEC-11(б) | `ExactShard` спинит без `yield` и без отказа при выходе процесса |
| R6-T24 | P3 | CC-4 | Claimant, попавший на `MAINTENANCE`, теряет кандидата и чеканит новую кучу |
| R6-T25 | P3 | CC-5 | `System.dealloc` под shard-локом в `Shard::remove` |
| R6-T26 | P3 | CC-6 | CAS-спины без TTAS; FFI-проба выхода на каждой итерации |
| R6-T27 | P3 | CC-7 | Воркер полностью trim-ит активную fallback-кучу под глобальным локом |
| R6-T28 | P3 | CR-2, SEC-2 | Class-leaf метит внутренние гранулы чужим классом; cross-thread interior free портит free-list |
| R6-T29 | P3 | CR-3 | Сжатие Large в `realloc` может вернуть null |
| R6-T30 | P3 | CR-4 | Непроверенное округление в `alloc_large`; promotion до валидации `Layout` |
| R6-T31 | P3 | CR-5 | Пул малых сегментов не дренируется у простаивающего владельца |
| R6-T32 | P3 | CR-6 | Segment directory сбрасывается cold trim-ом и не восстанавливается без роста |
| R6-T33 | P3 | CR-7 | `small_free_guard` не отвергает `BlockKind::Unknown` |
| R6-T34 | P3 | CR-9 | Biased Large без повтора резерва после слива пула (не-NUMA) |
| R6-T35 | P3 | SEC-6 | `start_maintenance()`: процессный worker из любой зависимости, ожидание без таймаута |
| R6-T36 | P3 | SEC-7, SEC-8 | `SECURITY.md` и doc `hardened` в `Cargo.toml` расходятся с кодом |
| R6-T37 | P3 | SEC-9, CQ-25, MS-6(tls) | Tier-1 `#![allow(unsafe_code)]` в `tls_heap.rs` без `unsafe` в `production` |
| R6-T38 | P3 | SEC-10, SEC-15, CQ-26 | Safe-мутаторы под `internals` вне сканера хуков; публичный хвост `AllocCore` |
| R6-T39 | P3 | PF-7, PF-8, PF-9, PF-10 | Мелкие оптимизации: `div` на refill, `memset`, TTAS в воркере, повторный lookup |
| R6-T40 | P3 | R17-OPT-05, -04, -07, -09, -10, -11 | Невзятые идеи R17, подтверждённые актуальными |
| R6-T41 | P3 | CQ-1, CQ-2, CQ-3 | Конвенции: one-file-one-export, `pub` вместо `pub(crate)`, `#[path]`/glob/алиасы |
| R6-T42 | P3 | CQ-6 | Мелкие дубли (exit escape, классификация TLS, `exact_fatal`) |
| R6-T43 | P3 | CQ-10, CQ-11 | Мёртвое зеркало стека в `loom_shim`; недостижимые ветки `not(std)` |
| R6-T44 | P3 | CQ-15 | Переименовать `AbandonGuard` |
| R6-T45 | P3 | CQ-19, CQ-20, CQ-21 | Магические числа, коллизии `STATE_*`, имена модулей |
| R6-T46 | P3 | CQ-23, CQ-24 | Cfg-параметры и cfg-лестницы; экспериментальные cfg R18/R19 в горячем пути |
| R6-T47 | P3 | CQ-27 | `internals` меняет синхронизацию production-пути |
| R6-T48 | P3 | CQ-30 | Одно условие даёт то abort, то `None` |
| R6-T49 | P3 | CQ-31, CQ-32 | Согласованность публичного API; `missing_docs` |
| R6-T50 | P4 | MS-3 | Foreign `realloc` проверяет ёмкость по резервации, а не по коммиту |
| R6-T51 | P4 | MS-4 | Stale/double free после переиздания освобождает текущего владельца (U-R5-5) |
| R6-T52 | P4 | MS-5 | `unsafe impl Sync for HeapSlot` без доказательства `Send` для `HeapCore` |
| R6-T53 | P4 | CC-8 | Пробелы и расхождения loom-моделей |
| R6-T54 | P4 | CC-9 | Гипотезы: ленивое разрешение детектора выхода; abort из `HeapLease::drop` при panicking |
| R6-T55 | P4 | CR-10 | Непоследовательная семантика `foreign_or_unroutable_frees` |
| R6-T56 | P4 | SEC-13, SEC-16 | Аддитивные фичи меняют глобальный аллокатор; FFI-тип `RtlDllShutdownInProgress` |
| R6-T57 | P4 | CQ-4, CQ-18, CQ-33 | Файлы у лимита 1000 строк, плотность комментариев, пространство ID |

---

## P1

### R6-T01 — Убрать мёртвую запись владельца (`stamp_segment_owner`)
- **Источник:** `performance_review.md` R6-PF-1. Закрывает `docs/perf/OPEN_ITEMS.md` item 72.
- **Где:**
  - `src/registry/heap_core/state/ownership.rs:40-86` (`stamp_segment_owner`);
  - вызовы: `src/registry/heap_core/alloc/hot.rs:76-86` (`finish_magazine_refill`), `:373-375`, `:645-647`; `src/registry/heap_core/free/realloc.rs:597`; `src/registry/heap_core/alloc/batch.rs:158,234,270`;
  - поле `last_stamped_segment` — `src/registry/heap_core/core.rs:208-209`;
  - сброс `owner_state` на cache-hit — `src/alloc_core/large/alloc_core_large.rs:438-447`.
- **Механизм:** `owner_state` в production никто не читает. Чужой free находит владельца через `Entry.owner` каталога маршрутов (`src/registry/heap_core_xthread/routing.rs:35`). Единственный читатель — тестовый `dbg_owner_id_for` (`src/registry/heap_core/diag/queries.rs:92-98`).
- **Что сделать:**
  - убрать вызовы stamp из production-путей, поле `last_stamped_segment` и сброс на cache-hit;
  - поле `owner_state` в заголовке оставить инертным, чтобы не ломать 144-байтовый layout-пин;
  - в `finish_magazine_refill` оставить один проход (`mark_magazine`);
  - 8 тестов на `dbg_owner_id_for`/`dbg_last_stamped_segment` перевести: проверки принадлежности — на `canonical_root_for`, тесты самого stamp-кэша удалить. Среди них `tests/heap_core_tcache_stamp.rs`, `tests/stamp_cache.rs`, `tests/r11_ph3b_kind_aware_entries.rs`;
  - убрать оба наблюдателя из `scripts/verify-dbg-hook-safety.mjs`.
- **Проверка:** iai-гейт на слое `HeapCore`/`SeferAlloc`.
  - Выигрыш: `cold_alloc_only_256x16b{,_2n,_4n}` (наклон линейный), `recycle_alloc_only_256x16b`, `large_alloc_free_cycle`, `large_cache_hit_only_4mib − prefill`.
  - Контроли ±T: `small_churn_16b{,_2n}`, `alloc_magazine_hit_only_16b`, `dealloc_free_only_16b`; mimalloc-ячейки = 0.
  - Oracle: ΔIr ∝ числу refill-промахов и Large alloc.
- **Оценка (не измерено):** −(3·n + 15…25) Ir на refill-промах, −15…25 Ir на Large alloc.
- **Риск:** низкий. Префикс: `perf(runtime)`.

### R6-T02 — Пропуск owner-drain по эпохе pin-ов (без новых RMW у producer)
- **Источник:** `performance_review.md` R6-PF-2, с ним R17-OPT-01 (load перед `swap`) и R17-OPT-03.
- **Блокер:** меняет протокол терминальной публикации (`docs/REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md`). Нужно решение владельца о переоткрытии perf item 82 (`docs/perf/OPEN_ITEMS_ARCHIVE.md` § 82, «Reopen trigger»). До решения не начинать.
- **Где:**
  - `src/registry/segment_route/directory.rs:32` (`refs`), `:142-157` (`pin_from_array`, `fetch_update`), `:215-229` (unpin);
  - `src/alloc_core/small/alloc_core_small/find_segment.rs:136-191` (`drain_segment_sidecar`), `:410-440`, `:480-520`;
  - `src/alloc_core/segment/remote_bitmap/bitmap_scan.rs:55` (безусловный `swap(0, AcqRel)`).
- **Механизм:** routed-ядро не доверяет отрицательному ответу директории. Каждый refill-промах без своих free проходит все Small-сегменты и делает `swap` по каждому слову pending от payload до `bump`: до ~4 024 locked RMW на сегмент.
- **Что сделать:**
  - `Entry.refs` = младшие 24 бита (держатели) + старшие 40 бит (эпоха захватов);
  - pin: `fetch_add(1 + EPOCH_ONE)` с abort при переполнении младшей части. Это заменяет CAS-цикл `fetch_update`;
  - владелец хранит `last_epoch[slot]` в `RouteSlots`. Перед полным drain — RMW-снимок `refs` (`fetch_add(0, AcqRel)`): держатели = база (1) и эпоха = E → drain пропускается;
  - bounded/strict trim-пути не трогать;
  - периодический принудительный полный drain против ABA эпохи.
  - Набросок доказательства — в отчёте.
- **Проверка:**
  1. Loom на actual-source (расширить `tests/loom_sidecar_bitmap_actual.rs` на `Entry.refs`) + мутант «Acquire-load вместо RMW».
  2. Новая преррегистрация по каркасу `docs/perf/R20_PRF01_C3_*`.
     - Выигрыш: `multiseg_cold_256k`, `seg_cycle_decommit_256k`, наклон `growth16_*`.
     - Цена: `r19_prf01_joined_producer_same_word`, distinct-word, publication net, construction.
     - Oracle: счётчики «drain пропущен/выполнен» (≥ 95 % пропусков в growth, 0 в producer-ячейках).
- **Риск:** высокий. Ошибка проявится удержанием памяти, а не UB, но доказательство обязательно.

## P2

### R6-T03 — OOM конструктора `HeapCore` монополизирует `reuse_hint`
- **Источник:** `concurrency_review.md` R6-CC-1.
- **Где:**
  - `src/registry/heap_registry/claim.rs:328-339` (`claim_impl` после `materialise` → `None` сразу выходит), `:689-706` (`push_back_after_oom` делает `reuse_hint.swap(idx)`);
  - `src/registry/heap_registry/stack.rs:157-161` (`pick_with_hint_spill` берёт hint без проверки);
  - `src/global/tls_heap.rs:276`, `:486-490` (`finish_bind(None)` оставляет `LOCAL` = null).
- **Механизм:** неинициализированный FREE-слот возвращается в hint и вытесняет готовые FREE-кучи в bitmap. Пока держится OOM, каждый claimant берёт его, снова получает OOM и снова кладёт в hint. Готовые кучи недостижимы. Каждая аллокация непривязанного потока повторяет ОС-резервацию и идёт на fallback-лок. Пошаговое чередование — в отчёте.
- **Что сделать:**
  - (a) при OOM конструктора не трогать `reuse_hint`: публиковать в `FREE_SLOTS` или положиться на scan;
  - (b) продолжать цикл через `scan_claim_recovery_free(reg, idx)` с предпочтением `initialised`, симметрично chunk-OOM (`claim.rs:266-276`);
  - (c) ограниченный backoff повторной привязки в TLS.
- **Проверка:**
  - тест с инъекцией OOM конструктора: после OOM готовая FREE-куча берётся следующим claimant-ом; counterfactual — без фикса тест красный;
  - loom: добавить неинициализированный FREE-слот в `tests/loom_r19_registry_free_set.rs` и `tests/loom_r20_heap_reuse_spill.rs` с инвариантом «инициализированная FREE-куча достижима».
- **Префикс:** `fix`.

### R6-T04 — Escape в `System` на выходе процесса для Own, Fallback и `realloc`
- **Источник:** `concurrency_review.md` R6-CC-2, `correctness_review.md` R6-CR-8, `api_security_review.md` R6-SEC-12.
- **Где:**
  - `src/registry/segment_route/directory.rs:837-843`, `:870-875` (`register` → `Err(ProcessExiting)`), `:933-945` (`remove` → `false`);
  - `src/global/sefer_alloc/global_alloc.rs:41-55`, `:101-112`, `:143-182`;
  - `src/registry/heap_core/free/realloc.rs:418-423` (foreign-ветка без маршрута → null);
  - `README.md` «Process exit on Windows» (~1691-1700).
- **Механизм:** escape в `System` срабатывает только когда отказал fallback-лок (`with_fallback_heap` → `None`). Если отказал shard-лок в `register` (держатель убит `ExitProcess`), Own-ветка и `Some(null)` из Fallback возвращают null, и следует `handle_alloc_error` → abort. Блок, выданный `System` на выходе, при `realloc` маршрута не находит → null. Неудачный `remove` оставляет маршрут на уже освобождённом VA, повторная регистрация того же ключа получает `Duplicate`. Это расходится с решением владельца «мёртвый держатель → escape, не abort» (`docs/reviews/2026-10-11-owner-questions-decisions-ox.md`, вопрос 7).
- **Что сделать:**
  - в `alloc`/`alloc_zeroed`/`realloc`, в обеих ветках: если результат null и `process_exit_in_progress()`, выполнить тот же escape в `System`. Это только холодный путь null;
  - для `realloc`: `System.alloc` + копирование `min(old, new)`, старый блок утекает;
  - обновить README;
  - один helper `exit_escape_alloc` (см. R6-T42).
- **Проверка:** сценарий «shard» в подпроцессном оракуле CON-01 с ожидаемым кодом выхода 0; сценарий `realloc` System-блока на выходе.
- **Префикс:** `fix`.

### R6-T05 — Own-thread small free доверяет `Layout`
- **Источник:** `api_security_review.md` R6-SEC-1.
- **Где:** `src/registry/heap_core/free/dealloc_own_base.rs:278-312` (`small_free_guard`), `:366` (`class_for(size, align)` из `Layout`).
- **Механизм:** без `hardened` guard проверяет только `off >= payload_start`, отсутствие в magazine, `off < bump` и `!is_free(off)`. Interior-гранула живого блока проходит `Accept` и попадает в magazine класса из чужого `Layout`. Под `hardened` ловится только interior (`off % block_size`), идентичность класса не проверяется ни в одной сборке. Сценарий: `alloc(64)` → `dealloc(p+16, Layout(48))` → следующий `alloc(48) == p+16` внутри живого блока.
- **Что сделать:**
  - под `hardened` сверять класс из `Layout` с классом гранулы в owner class map маршрута (одна Acquire-загрузка; для выданных блоков это заменяет и модульную проверку);
  - в `production` минимум задокументировать асимметрию own/cross-thread в README «Honest limitations» и в описании `hardened`;
  - вариант без флага — проверка класса через reciprocal вместо `%` с iai-гейтом цены.
- **Проверка:** тест под `hardened`: mismatched-class free → no-op или abort, а не повторная выдача; counterfactual.

### R6-T06 — Сырые free-list-ссылки вне `hardened`
- **Источник:** `api_security_review.md` R6-SEC-3.
- **Где:** `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:425-448` (`pop_free`), `:603-610`, `:639` (`try_drain_freelist_batch`). Валидация `hardened` — `:372-423`.
- **Механизм:** `next` читается из первого слова свободного блока, head = `(next − segment) as u32` без проверки диапазона и кратности. UAF-запись 8 байт → выдача адреса атакующего в пределах `segment + 4 GiB`. Первое слово свежевыданного блока содержит старый `next` — утечка адреса кучи. В debug `next < segment` даёт панику вычитания внутри `GlobalAlloc` (см. R6-T14).
- **Что сделать:**
  - хранить смещение, а не указатель, и маскировать его XOR-ом с per-segment секретом (аналог glibc safe-linking);
  - при pop проверять `offset < bump` и кратность классу;
  - мерить цену на hot refill-пути iai;
  - минимум — пункт в «Honest limitations».
- **Проверка:** тест «poisoned next → abort, а не выдача»; iai-гейт цены (`small_churn_16b`, `cold_alloc_only_256x16b`).

### R6-T07 — Больше `MAX_HEAPS = 4096` живых потоков
- **Источник:** `api_security_review.md` R6-SEC-4. Пересекается с perf item 66.
- **Где:**
  - `src/registry/bootstrap/registry.rs:45`;
  - `src/global/tls_heap.rs:276`, `:486-490`;
  - `src/registry/heap_registry/stack.rs:12-35`, `:157-171`;
  - `src/registry/bootstrap/saturation.rs:41`.
- **Механизм:** когда все слоты LIVE, `LOCAL` остаётся null. Каждая аллокация избыточного потока делает `claim_lease` с RMW `reuse_hint.swap` и `fetch_or` saturation на общих линиях, затем берёт глобальный fallback-спинлок. Каждый free избыточного потока берёт shard-лок. Любой выход потока сбрасывает saturation, и следующая аллокация каждого избыточного потока сканирует до 4096 слотов. Внешне достижимо в серверах thread-per-connection.
- **Что сделать:**
  - кэшировать в TLS вердикт «registry saturated»: sentinel `FALLBACK` в `LOCAL` с повтором claim раз в N аллокаций или по версии `SaturationHint`;
  - рассмотреть шардирование fallback-кучи;
  - добавить пункт в «Honest limitations».
- **Проверка:** тест «4097-й поток: claim не повторяется на каждой аллокации» по счётчику `CLAIM_SOURCES`; нативный бенч только на CI.

### R6-T08 — Амплификация коммита Large и предел 4095
- **Источник:** `api_security_review.md` R6-SEC-5. Связано с perf item 4 (R14-7), perf item 28 (`exact-span-large`, CONDITIONAL-GO), R6-T17 (CR-11).
- **Где:**
  - `src/alloc_core/large/alloc_core_large.rs:197-230` (`usable = needed.div_ceil(SEGMENT) * SEGMENT`; комментарий `:199-202` прямо признаёт минимум 4 MiB), `:626-635`;
  - `src/alloc_core/platform/os.rs:224-236`;
  - на Windows — eager `MEM_RESERVE | MEM_COMMIT` (`crates/aligned-vmem/src/os/windows.rs:32`).
- **Механизм:** запрос в 260 KiB коммитит 4 MiB (~16×). 4095 живых Large на heap → null → abort. Пример: однопоточный async-сервер с буфером 260 KiB на соединение.
- **Что сделать:**
  1. документировать амплификацию в «Honest limitations» и «Memory policy»;
  2. оценить lazy-commit хвоста Large в `production` или продвижение `exact-span-large`. Это отдельный гейт с RSS/commit-осью в одном режиме (правило R31-1).
- **Проверка:** для doc — нет. Для кода — гейт commit-charge и RSS на ячейке «N × 260 KiB живых».

### R6-T09 — `dealloc_batch`/`alloc_batch` обходят `ExactNarrow`
- **Источник:** `correctness_review.md` R6-CR-1, `api_security_review.md` R6-SEC-11 (часть а).
- **Где:** `src/global/sefer_alloc/batch.rs:66-76` (`alloc_batch`), `:115-136` (`dealloc_batch`). Скалярный эталон — `src/global/sefer_alloc/global_alloc.rs:64-69`. Также `src/global/exact_object/narrow.rs:38-57`, `:63-91`.
- **Механизм:** под `exact-object-proto` узкий `Layout` (≤ 16 B, align ≤ 8) обслуживается `System` и регистрируется в `ExactTable`. `dealloc_batch` сразу идёт в `HeapCore::dealloc_batch`/`publish_foreign`, маршрута не находит. Объект утекает, дескриптор живёт вечно, `foreign_or_unroutable_frees` растёт на корректной программе.
- **Что сделать:** под `cfg(feature = "exact-object-proto")` в `dealloc_batch` сначала вызывать `ExactNarrow::try_dealloc(p, layout)` для каждого ненулевого блока. Аналогично направить `alloc_batch` через narrow-развилку. Либо `compile_error!` на комбинацию фич.
- **Проверка:** подпроцессный тест с `exact-object-proto batch-api`: `alloc` узкого → `dealloc_batch` → `ExactNarrow::dbg_counts().1` вырос, `foreign_or_unroutable_frees` не изменился.

### R6-T10 — Large-sweep на каждом Large alloc
- **Источник:** `performance_review.md` R6-PF-3. Связано с R17-OPT-11.
- **Где:** `src/registry/heap_core/alloc/hot.rs:195-200`, `:639-642`; `src/alloc_core/alloc_core/sidecar_drain.rs:297-322` (`kind_at` в `:312`), `:327-378` (`:361`).
- **Механизм:** каждый Large alloc (включая cache-hit) проходит все живые Large-слоты: `base_at` + чтение заголовка (`kind_at`) только ради abort + `claim_large_route` + Acquire `LargeState`. Это ~3–4 промаха на живой Large.
- **Что сделать:**
  - (а) `kind_at`-проверку в обоих sweep-ах заменить на `debug_assert!`; инвариант даёт `ActiveKindIndex` + `route.root() == base`;
  - (б) на Large alloc делать ограниченную пробу (`large_hot_cursor` / `LARGE_HOT_BUDGET = 4`), полный sweep — только на промахе кэша перед OS-резервом (с повторным поиском в кэше) и в A1-пути `realloc`.
- **Проверка:** новая iai-ячейка K = 0/4/16/64 живых Large × цикл cache-hit, наклон ≈ 0 после (б). Oracle — `LARGE_SIDECAR_SLOT_INSPECTIONS`. Ось цены — удержание RSS при cross-thread Large free в том же режиме.
- **Риск:** (а) очень низкий, (б) средний.

### R6-T11 — Раскладка Tier-1 `own_cache`
- **Источник:** `performance_review.md` R6-PF-4.
- **Где:** `src/alloc_core/segment/segment_table/segment_table_impl.rs:203-204`, `:258-259`, `:733-748`, `:791-798`, `:856-863`.
- **Механизм:** ключи и корни лежат в двух массивах по 128 B (разные строки L1). Hit дополнительно проверяет `!is_null()`.
- **Что сделать:** массив пар `[(usize, *mut u8); 16]`, пустой ключ = `usize::MAX` (не SEGMENT-выровнен). Hit = одно сравнение. `own_cache_clear` пишет `usize::MAX`.
- **Проверка:** iai `dealloc_contains_base_probe_only_16b`, `dealloc_free_only_16b{,_n8,_n16}`, `small_churn_16b{,_2n}` (ожидаемо −1…−2 Ir/оп); construction-ячейка с односторонним допуском; Tier-1 hit-rate (`CONTAINS_BASE_TIER1_HITS/_MISSES`) не меняется.

### R6-T12 — Инварианты сегмента в owner-drain на каждую запись
- **Источник:** `performance_review.md` R6-PF-5, включает R17-OPT-08 (делимость без `div`).
- **Где:** `src/alloc_core/small/alloc_core_small_reclaim.rs:21-68`; циклы `find_segment.rs:159-174`, `sidecar_drain.rs:137-142`, `:229-251`. Эталон — `flush_run` в `src/alloc_core/small/alloc_core_small_magazine.rs:567-669`.
- **Механизм:** на каждую запись заново: `kind_at`, `payload_start`, `block_size(class)`, `off % block_size` (div), `bump_of`, `dec_live`.
- **Что сделать:** разделить на «контекст сегмента» (один раз на скан) и «запись». Копить принятые записи и делать `sub_live(k)` в конце скана. Делимость — таблицей reciprocal на класс. Abort-проверки сохранить.
- **Проверка:** iai `r19_prf01_publication` (наклон по числу записей). Oracle: `SIDECAR_INGRESS_RECORDS_CONSUMED` == числу публикаций.
- **Порядок:** делать после R6-T18 (общий цикл).

### R6-T13 — Bounds-check индекса magazine-слота
- **Источник:** `performance_review.md` R6-PF-6. Это гипотеза.
- **Где:** `src/registry/heap_core/alloc/hot.rs:237`, `:286`; `src/registry/heap_core/free/dealloc_own_base.rs:404`, `:416`. `count: u8` в `src/registry/heap_core/state/tcache.rs:235`.
- **Что сделать:**
  1. снять `cargo asm` для `HeapCore::alloc` (hit) и `dealloc_own_thread_with_base` (push) под `--release --features production` — на CI или локально без нагрузки;
  2. если есть `cmp $0xf; jae panic_bounds_check` — индексировать через `idx & (TCACHE_CAP - 1)` + `const_assert!(TCACHE_CAP.is_power_of_two())`;
  3. `classes[c]` (49 классов) не трогать: стековый пин `HeapCore ≤ 9216 B`.
  4. Если check-а нет — закрыть как NULL.
- **Проверка:** asm-дифф; iai `alloc_magazine_hit_only_16b − prefill`, `dealloc_free_only_16b_n8/_n9/_n16`.

### R6-T14 — Паники на путях `GlobalAlloc` → `fatal`
- **Источник:** `code_quality_review.md` R6-CQ-29, `memory_safety_review.md` R6-MS-2, `api_security_review.md` R6-SEC-14.
- **Где (release):**
  - `src/alloc_core/large/alloc_core_large_cache.rs:146-148`, `:162-164` (`expect`), `:170`, `:364` (`unreachable!`);
  - `src/alloc_core/segment/segment_header/terminal_words.rs:49` (`assert!` в `pack_large_state` с runtime-поколением из `reservation_state.rs:45-161`);
  - `src/alloc_core/small/alloc_core_small_magazine.rs:169-173` (`assert!(out.len() <= 16)`, `virgin-zero-skip`);
  - `src/registry/bootstrap/registry.rs:134-136`, `:149-151` (индексация чанков, «# Panics»).
- **Где (debug):**
  - `src/alloc_core/segment/segment_table/segment_table_impl.rs:617-622`, `:657-661` — `debug_assert!(false)` вопреки комментарию «leak, not panic»;
  - `alloc_core_small_pool_impl.rs:199-202`, `tls_heap.rs:522`, `hot.rs:37-41`, `dealloc_own_base.rs:349`, `:482-486`, `tcache_flush.rs:89-93`, `realloc.rs:214-218`;
  - вычитание `next as usize - segment as usize` в `alloc_core_small_impl.rs:447`, `:608`, `:639` паникует при порче free-list.
- **Механизм:** unwind из `GlobalAlloc` — UB (документация std; в проекте R2-08, `claim.rs:186-195`). Политика проекта для порчи состояния — abort (`src/global/sefer_alloc/mod.rs:178-186`).
- **Что сделать:**
  - `expect`/`unreachable!`/`assert!` → `unwrap_or_else(|| fatal(b"…"))` / `if !cond { fatal(…) }`;
  - `debug_assert!(false)` в `recycle` → leak, как обещано;
  - вычитания → `wrapping_sub`/`checked_sub` + `fatal`;
  - перечень разрешённых паник в `src/global/sefer_alloc/mod.rs:98-125` сократить до пустого; lexical-guard перевести на «паник нет»;
  - опционально abort-on-unwind guard в `GlobalAlloc`.
- **Проверка:** lexical-guard в `scripts/` (по образцу `verify-*.mjs`), подключить в `check-all.mjs` и CI. iai: hot path без изменений.
- **Префикс:** `fix`.

### R6-T15 — 108 голых `process::abort()` → `fatal(msg)`
- **Источник:** `code_quality_review.md` R6-CQ-28.
- **Где:**
  - больше всего в `src/alloc_core/segment/segment_table/route_slots.rs` (19), `src/registry/segment_route/directory.rs` (15), `claim.rs` (7), `segment_table_impl.rs` (7), `sidecar_drain.rs` (7);
  - помощник — `src/alloc_core/platform/fatal.rs:4-8`, сейчас используется один раз (`alloc_core_small_impl.rs:403`);
  - дубль — `src/global/exact_object/exact_fatal.rs`.
- **Что сделать:**
  - механически заменить на `fatal(b"<модуль>: <инвариант>\n")` (не аллоцирует, не раскручивает);
  - удалить `exact_fatal`;
  - lexical-проверка «нет `process::abort` вне `fatal.rs`»;
  - снять отказ от ретрофита в `src/global/sefer_alloc/mod.rs:185-186`.
- **Проверка:** lexical-guard в CI. Поведение не меняется.
- **Порядок:** делать вместе с R6-T14.

### R6-T16 — Blanket `allow(dead_code)` в `production`
- **Источник:** `code_quality_review.md` R6-CQ-8, R6-CQ-9.
- **Где:**
  - `src/lib.rs:457-459`, `:479-481`, `:491-493` — `#[allow(dead_code, unused_imports)]` на модулях `alloc_core`/`global`/`registry` при `not(internals)`;
  - устаревшие allow: `src/alloc_core/platform/os.rs:512` (`Segment::len` без вызовов), `segment_header_views.rs:9`, `segment_table_impl.rs:696`, `claim.rs:572`, `:579`, `src/alloc_core/segment/mod.rs:47`, `remote_bitmap/mod.rs:4-10`, `platform/sidecar_stats.rs:48`;
  - `#[allow(unused_variables)]` на функции: `find_segment.rs:638`, `dealloc_own_base.rs:188`.
- **Что сделать:**
  - снять blanket-allow;
  - тестовую поверхность гейтить поэлементно `#[cfg(feature = "internals")]` или `#[cfg_attr(not(feature = "internals"), allow(dead_code))]` с причиной;
  - мёртвое удалить;
  - `unused_variables` — точечно `let _ = x;`.
- **Проверка:** `npm run check` (все 6 clippy-строк) зелёный без blanket-allow.

### R6-T17 — Устаревшие комментарии и контракты `unsafe`
- **Источник:** `code_quality_review.md` R6-CQ-12, -13, -14, -16, -17; `correctness_review.md` R6-CR-11, R6-CR-12 (item 154); `memory_safety_review.md` R6-MS-6 (SAFETY-часть).
- **Контракты `unsafe` — исправить в первую очередь:**
  - `src/registry/heap_core/free/dealloc.rs:186-190` — doc `unsafe fn HeapCore::dealloc` говорит «route cross-thread via the TFS»;
  - `src/global/sefer_alloc/global_alloc.rs:1-5`, `:21-32` — SAFETY у `unsafe impl GlobalAlloc`. Неверно: «never panics», «dealloc через current_heap/fallback», «only null is true OOM» (null дают ещё `MAX_SEGMENTS`, отказ регистрации, OOM `System` при spill), неполный список `unsafe` в шапке. Переписать по текущим путям: four-way dispatch, `publish_foreign`, escape на выходе, failure policy `sefer_alloc/mod.rs:178-197`; формулировку M10 привести к M4 из `docs/INVARIANTS.md`;
  - `src/registry/bootstrap/mod.rs:53-97`, `:138` — описание протокола bootstrap: `AtomicPtr` вместо `OncePtrCell`, `reserve_aligned` + `forget` вместо `leak_zeroed_pages`, `next_free`/`push_free_slot`;
  - `src/registry/heap_registry/claim.rs:5-9` — «`get_unchecked` удалён», хотя он есть в `registry.rs:138`, `:153`, `:212`;
  - `mem_impl.rs:414-446`, `alloc_core_large.rs:310-348`, `:405-422`, `:821-838` — обоснование атомарного `magic` удалёнными читателями заголовка, которых больше нет.
- **Удалённые механизмы в настоящем времени:**
  - `state/tcache.rs:23`;
  - `alloc_core_small_impl.rs:157-161`;
  - `dealloc_batch.rs:60`, `:229-231`, `:276`;
  - `alloc/batch.rs:101`;
  - `hot.rs:24`, `:576-578`;
  - `alloc_core_small/directory.rs:223`, `:229-230`;
  - `diag/queries.rs:294`, `:330`;
  - `diag/mod.rs:2`, `diag_probes.rs:3`;
  - `segment_header_impl.rs:82`;
  - `dealloc_own_base.rs:126-127` (несуществующий `large_layout_consistent`);
  - `realloc_fastpath.rs:156-162` (обоснование #138);
  - `heap_core/core.rs:362-363` («Read by `recycle`»);
  - `large_cache_config.rs:46-48` / `:187-188` против `:293-300` (headroom — не жёсткий floor).
- **Ссылки на несуществующие файлы:** ~135 мест (`alloc_core.rs`, `alloc_core_small_pool.rs`, `alloc_core_small.rs`, `heap_core.rs`, …). Особенно вредные:
  - `src/alloc_core/alloc_core/mod.rs:23` — grep-рецепт на несуществующий файл;
  - `realloc_fastpath.rs:364`;
  - `reserved_small_segment.rs:78`, `:212`;
  - `sidecar_drain.rs:188`, `:220`.
- **Прочие устаревшие утверждения:** перечень в R6-CQ-17 (фазы, «three resolvers», `u32::MAX` sentinel, «returns null», «~106 tests files»).
- **Что сделать:**
  - один проход по `src/`, текст — на текущий протокол (route directory → pinned sidecar → terminal publication);
  - ссылаться на элементы по пути (`SidecarBitmap::publish`), а не `file.rs:NNN`;
  - в `tests/no_stale_doc_references.rs` добавить запрещённые токены: `TFS`, `ring push`, `ring entry`, `HeapOverflow` вне исторического контекста, `drain_heap_overflow`, `owner_slot_is_live`, `entry_class_idx`, `dealloc_foreign_routing`, `large_layout_consistent`, и шаблон `\.rs:\d+` в комментариях `src/`.
- **Проверка:** расширенный `tests/no_stale_doc_references.rs` красный до прохода и зелёный после.
- **Префикс:** `docs:` недопустим, потому что меняется `src/`. Правки только в комментариях проходят direction-2 как comment-only, но надёжнее `fix(docs)` по taxonomy — уточнить по `scripts/verify-commit-prefixes.mjs`.

### R6-T18 — Три копии цикла слива Small sidecar
- **Источник:** `code_quality_review.md` R6-CQ-5.
- **Где:** `src/alloc_core/small/alloc_core_small/find_segment.rs:136-191` (`drain_segment_sidecar`), `src/alloc_core/alloc_core/sidecar_drain.rs:88-184` (`drain_sidecar_ingress_bounded`), `:194-292` (`drain_sidecar_ingress`). Large-ветка `claim_large_route` → `reclaim_large_segment` повторена дважды.
- **Механизм:** каждая копия заново считает `start_word`/`end_word`, делает `scan_small_route_from` с abort, `cut.pop` + `reclaim_sidecar_record`, `sync_directory_for_segment_classes`, `dec_live_and_maybe_decommit`/`release_or_pool_empty_segment`. Копии уже расходятся: разный клампинг `start_word`, bench-счётчики только в двух. Это граница времени жизни резервации: «все cut и заимствования route до release».
- **Что сделать:** выделить `consume_small_cuts(&mut self, base, index, scan) -> (u64, usize)`, `finish_small_drain(...) -> DrainEnd { Live, Pooled, Released }`, `retire_large_route(..) -> bool`. Bench-наблюдатели — параметром ZST или замыканием.
- **Проверка:** рефакторинг без изменения поведения. Все sidecar/loom-тесты, iai Ir-нейтрально (±T).
- **Префикс:** `fix(perf)` или refactor-эквивалент по taxonomy.

### R6-T19 — Длинные функции
- **Источник:** `code_quality_review.md` R6-CQ-7.
- **Где:**
  - `find_segment_with_free_impl` — `find_segment.rs:205`, 424 строки, 69 `#[cfg]`, вложенность до 6;
  - `primordial` — `bootstrap.rs:35`, 362;
  - `alloc_large` — `alloc_core_large.rs:170`, 353;
  - `HeapCore::realloc` — `realloc.rs:137`, 303;
  - `dealloc_at_base` — `mem_impl.rs:263`, 290, `inline(always)`;
  - `dealloc_own_thread_with_base` — `dealloc_own_base.rs:347`, 205, `inline(always)`;
  - `alloc_with_class` — `hot.rs:183`, 194, `inline(always)`;
  - `heap_ptr_impl` — `fallback.rs:157`;
  - `realloc_inplace_fast_path_known_base` — `realloc_fastpath.rs:225`;
  - `refill_class_bump_impl` — `alloc_core_small_magazine.rs:180`;
  - `claim_impl` — `claim.rs:241`.
- **Что сделать:**
  - в `find_segment_with_free_impl` выделить `scan_directory_candidates`, `linear_scan`, `numa_fallback_pick`;
  - холодные ветки `inline(always)`-функций (promotion, hardened, Large-маршрутизация) — в `#[cold] #[inline(never)]` помощники.
- **Проверка:** iai: hot path Ir-нейтрален или лучше.
- **Порядок:** после R6-T18.

### R6-T20 — Предикат `medium_promotion_reachable` × 18
- **Источник:** `code_quality_review.md` R6-CQ-22.
- **Где:** `src/alloc_core/mod.rs:202-209`; `segment_header_views.rs:49-57`, `:65-73`; `dealloc_own_base.rs:215-226`, `:260-269`, `:324-336` (ручные отрицания); макрос `dealloc.rs:69-81`; а также `counters.rs`, `alloc_core_large.rs`, `os.rs`, `alloc_core_impl.rs`, `realloc.rs`.
- **Механизм:** `all(medium-classes, any(not(exact-span-large), all(large-reserved-capacity, not(numa-aware))))`. Ошибка в одной копии молча выключает код.
- **Что сделать:** минимально — `const MEDIUM_PROMOTION_REACHABLE: bool = cfg!(…)` в одном месте для ветвлений внутри функций, на элементах — существующий макрос. Альтернативы (`build.rs` с `rustc-cfg` или `cfg_aliases`) требуют решения владельца: новая build-поверхность или зависимость.
- **Проверка:** feature-powerset (еженедельный CI-job, `workflow_dispatch`) зелёный.

### R6-T21 — Индексы открытых пунктов
- **Источник:** правило `CLAUDE.md` «Round start: check BOTH open-items indexes».
- **Что сделать:**
  - perf-находки (PF-*, SEC-5, CR-6) — в `docs/perf/OPEN_ITEMS.md`;
  - корректность, верификация и CI (CC-*, CR-*, MS-*, SEC-*, CQ-*) — в `docs/CORRECTNESS_OPEN_ITEMS.md` и тематические файлы `docs/correctness-open-items/TRACKED_*.md`, с обновлением таблицы item → файл.
- **Уточнения к существующим пунктам:**
  - CR-2 → item 166;
  - CR-3 → perf item 83;
  - CR-11 → perf item 4;
  - CR-12 → item 154;
  - CC-8 → item 186;
  - PF-1 → perf item 72;
  - PF-2 → perf item 82;
  - SEC-4 → perf item 66;
  - SEC-5 → perf items 4/28;
  - MS-4 → residual U-R5-5.
- Карточки — в формате current-state (Status / Current / Next trigger / Evidence).

## P3

### R6-T22 — `ptr::add` до валидации адреса
- **Источник:** `memory_safety_review.md` R6-MS-1.
- **Где:** `src/alloc_core/alloc_core/mem/mem_impl.rs:36-44` (`canonical_block_of` → `Node::deref(base, ptr.addr() − base.addr())`), `realloc_fastpath.rs:232-240`; `src/alloc_core/platform/node.rs:141-153` (`Node::deref` = `add` без проверки `offset < len`). Reject позже: `mem_impl.rs:271`, `:666`.
- **Механизм:** под `exact-span-large` или для biased Large окно ключа 4 MiB шире резервации. Foreign-указатель из того же окна даёт `add` за пределы объекта — UB до reject.
- **Что сделать:** свидетель-адрес строить через `base.wrapping_add(off)` / `base.with_addr(ptr.addr())`. `add` — только после `off < span` (`span_usable`/`reservation_len` для Large, `SEGMENT` для Small).
- **Проверка:** Miri-тест на `exact-span-large`: foreign-адрес в окне → без UB-диагностики. Miri гоняется в CI.

### R6-T23 — `ExactShard` без отказа при выходе
- **Источник:** `concurrency_review.md` R6-CC-3 (CON-01 W5), `api_security_review.md` R6-SEC-11 (часть б).
- **Где:** `src/global/exact_object/exact_shard.rs:184-196` (`acquire`), `:222-265`, `:45-77`, `:205-218` (`rehash` с `System` под локом); `global_alloc.rs:64-69`.
- **Что сделать:**
  - схема `ShardLock::lock_unless_exiting`: спин → `yield` → отказ при `process_exit_in_progress()`;
  - при отказе `try_dealloc` → `false` (unroutable, утечка безопасна);
  - `register` → освободить и вернуть null или escape;
  - `rehash` вынести из-под лока (паттерн `Surplus`).
- **Проверка:** подпроцессный оракул CON-01 со сценарием «exact shard».

### R6-T24 — Потеря кандидата при `MAINTENANCE`
- **Источник:** `concurrency_review.md` R6-CC-4.
- **Где:** `src/registry/heap_registry/claim.rs:278-326`; `stack.rs:157-178` (bump раньше scan); воркер — `claim.rs:126-151`.
- **Механизм:** кандидат в `MAINTENANCE` → `continue`, кандидат уже потреблён. `bump` чеканит новую кучу с 4 MiB, навсегда.
- **Что сделать:** при `MAINTENANCE` вернуть кандидата в bitmap или коротко повторить; либо один `scan_claimable_slot` перед `bump`; либо воркер пропускает текущий `reuse_hint`.
- **Проверка:** детерминированный тест с удержанием `MaintenanceLease` (`dbg_try_maintenance`): claimant берёт освободившийся слот, `count` не растёт. Loom-модель — в R6-T53.

### R6-T25 — `System.dealloc` под shard-локом
- **Источник:** `concurrency_review.md` R6-CC-5.
- **Где:** `src/registry/segment_route/directory.rs:610-657` (`Shard::remove` → `recycle` → `Block::release`); эталон `Surplus` — `:499-503`, `:876-878`.
- **Что сделать:** возвращать вытесненный `Block` наружу (`Option<SpareBlock>`) и освобождать после `drop(guard)`.
- **Проверка:** существующие route-тесты; поведение не меняется.

### R6-T26 — CAS-спины без TTAS
- **Источник:** `concurrency_review.md` R6-CC-6, R17-OPT-07 (item 70).
- **Где:** `src/registry/segment_route/shard_lock.rs:49-95`; `src/global/fallback.rs:487-526`; `src/global/process_exit.rs:55-75`.
- **Что сделать:** TTAS (спин на `load(Relaxed)`) + экспоненциальный backoff. FFI-пробу `process_exit_in_progress()` прореживать или кэшировать на итерацию ожидания.
- **Проверка:** loom-модели локов (если есть) и нативный contention-бенч на CI.

### R6-T27 — Trim fallback-кучи воркером
- **Источник:** `concurrency_review.md` R6-CC-7.
- **Где:** `src/global/maintenance_service.rs:144-150`; `src/registry/heap_core/state/ownership.rs:117-154` (`background_maintenance_step` → `trim_cold_retention`).
- **Механизм:** каждые 10 мс под глобальным `fallback::LOCK`: flush tcache, `drain_small_pool`, `evict_all`, munmap — у активной общей кучи.
- **Что сделать:** для fallback только ingress-шаг без cold-retention, либо trim после интервала простоя fallback.
- **Проверка:** тест «воркер не сбрасывает tcache fallback-кучи в течение активного использования» (по счётчику `FALLBACK_VISITS` и состоянию magazine).

### R6-T28 — Class-leaf и cross-thread interior free
- **Источник:** `correctness_review.md` R6-CR-2, `api_security_review.md` R6-SEC-2. Уточняет item 166.
- **Где:**
  - `src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs:108-135` (spill копирует `old` во все гранулы; uniform-лист метит весь лист классом первого блока; сброса при reset сегмента нет);
  - `sidecar_bitmap.rs:91-115` (`publish` отбрасывает только класс 0);
  - `src/alloc_core/small/alloc_core_small_reclaim.rs:38-66`.
- **Механизм:** внутренние гранулы блока несут чужой класс. Cross-thread free внутреннего указателя проходит все проверки владельца: `write_next` пишет в живой блок, `dec_live` занижает счётчик. После reset сегмента устаревшая class map даёт то же. Под `hardened` own-thread такое отвергается, cross-thread — нет.
- **Что сделать:** owner-only бит «issued-start» (ставится в `IssueTransaction::commit`, снимается при free/reclaim), запись без бита — отброс или abort. Либо класс хранить только в грануле-начале. Очистка class map при reset сегмента. Обновить карточку item 166 и README («cross-thread misuse может давать abort в потоке владельца»).
- **Проверка:** тест-сценарий из CR-2: 6144-байтный блок по смещению 61440 + 16-байтный в соседнем листе, foreign free внутреннего указателя → отброс или abort, а не порча.

### R6-T29 — Сжатие Large может вернуть null
- **Источник:** `correctness_review.md` R6-CR-3. Решение — в perf item 83.
- **Где:** `src/alloc_core/alloc_core/mem/realloc_fastpath.rs:243-284` (OPT-G только при росте); move-ветка `src/registry/heap_core/free/realloc.rs:340-351`, `mem_impl.rs:692-709`.
- **Что сделать:** минимум — при отказе аллокации в move-ветке сжатия вернуть исходный `ptr` и записать `large_size = new_eff` (корректно по контракту `GlobalAlloc`). Исправить doc OPT-G.
- **Проверка:** тест с инъекцией OOM: shrink возвращает исходный указатель.

### R6-T30 — Переполнение в `alloc_large`; promotion до валидации `Layout`
- **Источник:** `correctness_review.md` R6-CR-4.
- **Где:** `src/alloc_core/large/alloc_core_large.rs:188-196`, `:226-230` (`div_ceil * SEGMENT` без проверки), `:241` (`align_up`); `src/registry/heap_core/free/realloc.rs:297-317` (promotion до `Layout::from_size_align` на `:343`), `:593`.
- **Что сделать:** `n_segments.checked_mul(SEGMENT)` и проверяемый `align_up` → null. В `HeapCore::realloc` валидировать `Layout` до promotion. Исправить doc у `checked_add`.
- **Проверка:** unit-тест `alloc_large(usize::MAX - 4096, 8)` в debug → null без паники.

### R6-T31 — Пул малых сегментов у простаивающего владельца
- **Источник:** `correctness_review.md` R6-CR-5.
- **Где:** `src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:422-484` (`maybe_decay_small_pool`); единственный вызов — `src/alloc_core/small/alloc_core_small/reserve.rs:126`; doc — `alloc_core_impl.rs:572-580`.
- **Что сделать:** либо исправить doc («удержание ограничено, но не временно»), либо добавить тик пула в холодные события (рядом с `maybe_decay_large_cache`, промах magazine). Второе — гейт RSS-оси.
- **Проверка:** для кода — тест «всплеск → idle → тик → пул пуст»; RSS-гейт.

### R6-T32 — Segment directory после cold trim
- **Источник:** `correctness_review.md` R6-CR-6.
- **Где:** `src/registry/heap_core/state/ownership.rs:152-153`; `src/alloc_core/small/alloc_core_small/directory.rs:59-69`, `:103-107`; `reserve.rs:443-444` (единственный `maybe_materialize_directory`).
- **Механизм:** любой cold trim (`trim_current_thread`, повторный claim, проход воркера, выход потока) сбрасывает directory. Пересборка — только при резерве нового сегмента. Нерастущая куча остаётся на линейном скане навсегда.
- **Что сделать:** пересобирать лениво при промахе, если `table.count() >= DIRECTORY_MATERIALIZE_THRESHOLD`, или не сбрасывать на повторном claim.
- **Проверка:** perf-гейт с oracle `DIRECTORY_HITS` / `FULL_SCAN_SLOTS_EXAMINED` до и после trim.

### R6-T33 — `BlockKind::Unknown` в `small_free_guard`
- **Источник:** `correctness_review.md` R6-CR-7.
- **Где:** `src/registry/heap_core/free/dealloc_own_base.rs:198`, `:215-233`, `:292-312`. Эталон no-op — `mem_impl.rs:536`, `:539-551`.
- **Что сделать:** `RejectNoOp` для `Unknown` до M2-оракулов.
- **Проверка:** тест с испорченным `kind` → no-op, блок не в magazine.

### R6-T34 — Biased Large без повтора (не-NUMA)
- **Источник:** `correctness_review.md` R6-CR-9.
- **Где:** `src/alloc_core/large/alloc_core_large.rs:562-574` (NUMA повторяет), `:645-673` (не-NUMA не повторяет для `align >= SEGMENT`).
- **Что сделать:** `drain_small_pool` + повтор `reserve_biased`, как в NUMA-ветке.
- **Проверка:** тест с инъекцией отказа резерва: `alloc(Layout(1 MiB, 8 MiB))` получает память после слива пула.

### R6-T35 — `start_maintenance()`
- **Источник:** `api_security_review.md` R6-SEC-6.
- **Где:** `src/global/sefer_alloc/maintenance.rs:36-38`; `src/global/maintenance_service.rs:30`, `:64-69`, `:71-108` (ожидание `RUNNING` без таймаута на `:101-105`), `:129-169`.
- **Что сделать:**
  - документировать глобальный побочный эффект и запрет вызова под loader lock (DllMain, static-конструкторы DLL);
  - ожидание с таймаутом и отдельной ошибкой `MaintenanceStartError`;
  - opt-out для владельца бинарника — только по решению владельца (флаг).
- **Проверка:** тест таймаута через `START_PAUSE` (уже есть инъекция).

### R6-T36 — `SECURITY.md` и doc `hardened`
- **Источник:** `api_security_review.md` R6-SEC-7, R6-SEC-8.
- **Где:** `SECURITY.md:59-86`; `Cargo.toml:671-688` (`:682` — несуществующий `reclaim_offset`, `:687` — несуществующий раздел README «Hardened vs production»), `:254` (`RemoteFreeRing`).
- **Что сделать:**
  - double-free и неверный `Layout` через `unsafe` API — нарушение контракта, а не уязвимость, если поведение не хуже задокументированного;
  - panic или unwind из `GlobalAlloc` — в scope;
  - статус `internals`/`bench-internals`/`experimental`/`batch-api`/`exact-object-proto` указать явно;
  - doc `hardened` привести к sidecar-протоколу (cross-thread interior даёт abort у владельца, см. R6-T28) и сослаться на существующий раздел README.
- **Префиксы:** `SECURITY.md` — `docs:`; `Cargo.toml` — отдельный коммит `build:`, потому что `docs:` с `Cargo.toml` красный в CI.

### R6-T37 — Tier-1 allow в `tls_heap.rs`
- **Источник:** `api_security_review.md` R6-SEC-9, `code_quality_review.md` R6-CQ-25, `memory_safety_review.md` R6-MS-6.
- **Где:** `src/global/tls_heap.rs:95-101`; единственный `unsafe` — `dbg_restore_local_for_test` (`:651-655`, `bench-internals`). Инвентарь — `src/lib.rs:225` (описание устарело после Ph4b).
- **Что сделать:** `#![cfg_attr(feature = "bench-internals", allow(unsafe_code))]` или item-level `#[allow(unsafe_code)]` на хуке (tier 2, по образцу `src/registry/heap_core/diag/diag_probes.rs`). Обновить инвентарь в `src/lib.rs` и README «Where unsafe lives» (и счётчики tier-1/tier-2, если они там есть).
- **Проверка:** `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/` до и после; `npm run check`.

### R6-T38 — Сканер хуков и публичный хвост `AllocCore`
- **Источник:** `api_security_review.md` R6-SEC-10, R6-SEC-15; `code_quality_review.md` R6-CQ-26.
- **Где:**
  - `src/registry/segment_route/directory.rs:716-719`, `:794-880` (safe `RouteDirectory::global().register(...)` под `internals` — DoS через `mem::forget`);
  - `src/registry/heap_registry/claim.rs:85-87`, `:612` (`dbg_claim_lease` + `HeapLease::core`);
  - `scripts/verify-dbg-hook-safety.mjs:311` (инвентарь только по имени);
  - `src/alloc_core/alloc_core/state.rs:184-206` (`segment_bases` виден в `production`);
  - `alloc_core_core_diag/totals.rs:26-47`, `alloc_core_small_pool_impl.rs:503-504` (`dbg_*`-аксессоры `stats()` без гейта).
- **Что сделать:**
  - `RouteDirectory::register` → `pub(crate)` + `internals`-форвардер в инвентаре;
  - сканер: любая `pub fn` под `internals` с `*mut`/`*const` параметром попадает в инвентарь независимо от имени;
  - `segment_bases` → `pub(crate)`;
  - production-аксессоры без префикса `dbg_` → `pub(crate)`, `dbg_`-алиасы под `internals`.
- **Проверка:** обновлённый `verify-dbg-hook-safety.mjs` ловит отрицательный пример (counterfactual).

### R6-T39 — Мелкие оптимизации
- **Источник:** `performance_review.md` R6-PF-7, -8, -9, -10.
- **PF-7:** `refill_n_for_class(block_size(c))` (`hot.rs:494-496`, `:681-683` → `tcache.rs:127-139`) → `FREE_PARK_CAP[c] as usize` (`tcache.rs:176-186`). −1 `div` на refill-промах.
- **PF-8:** `Node::zero(ptr, size)` (`hot.rs:631-634`, `node.rs:159-169`) при `size ≤ 128` → фиксированный цикл 16-байтовых записей по `align_up(size, 16)`. Нужна iai/wall-ячейка с `black_box(size)` через реальный `#[global_allocator]`, существующая ячейка с константным layout этого не видит.
- **PF-9:** `try_maintenance_at` (`claim.rs:126-151`) — `load(Relaxed)` перед CAS, пропуск не-FREE. Опционально выровнять `heap` в `HeapSlot` (`heap_slot.rs:150-244`) на 64. Только нативная ось (`perf c2c`).
- **PF-10:** Large own-free (`dealloc_own_base.rs:384-396`, `:549-551` → `mem_impl.rs:233-250`) повторно резолвит корень. Передавать `owned.root()` в `dealloc_at_base`.
- **Проверка:** PF-7 и PF-10 — одним iai-гейтом вместе с R6-T10(а).

### R6-T40 — Невзятые идеи R17
- **Источник:** `performance_review.md` §5. Все подтверждены как актуальные по текущему коду, ни одна не измерялась.
- **R17-OPT-05 (Medium–High):** проверка маршрута раз на партию refill. `prepared` вычисляется ≥ 3 раз на блок (`segment_table_impl.rs:316-333`, `route_slots.rs:137-179`, `small_sidecar.rs:91-102`, `issue_transaction.rs:108-122`); `try_drain_freelist_batch` проходит цепочку дважды (`alloc_core_small_impl.rs:561-641`). Оценка R17: 20–50 Ir на блок.
- **R17-OPT-04 (Low):** двойная инициализация `SmallSidecar` (`directory.rs:56-65`).
- **R17-OPT-07:** см. R6-T26 и R6-T02 (`fetch_update` → `fetch_add`).
- **R17-OPT-09, -10, -11 (Low):** стек `PerClass` вниз (`tcache.rs:228-286`); бюджет фона по непустым cut (`sidecar_drain.rs:88-184`); второй Large-sweep в `realloc` (теряет смысл после R6-T10(б)).
- **Не трогать:** `TCACHE_CAP`, `FLUSH_N` — NO-GO.

### R6-T41 — Конвенции модулей
- **Источник:** `code_quality_review.md` R6-CQ-1, -2, -3.
- **CQ-1:** `src/alloc_core/config/profile.rs:111`, `:160`, `:360` — три стабильных типа в одном файле. Вынести `small_pool_policy.rs` и `large_cache_policy.rs` (в `config/` станет 7 записей).
- **CQ-2:** production-API `pub` вместо `pub(crate)` в doc-hidden модулях:
  - `claim.rs:35`, `:462`, `:558`;
  - `tls_heap.rs:239-390`;
  - `fallback.rs:62-64`, `:142-405`;
  - `counters.rs:67`, `:80`, `:161`, `:233`, `:289`;
  - `terminal_words.rs:32-79`.

  Наружу — только `dbg_`/`_for_test`-форвардеры. Аренды (`HeapLease`/`MaintenanceLease`) развести по файлам.
- **CQ-3:** лишние `#[path]` (`alloc_core/alloc_core/mod.rs:45-104` и др.), межкаталожные `#[path]` (`segment/mod.rs:24-29`, `:44`), 7 glob-реэкспортов `*_impl::*`, 9 алиасов `#[allow(unused_imports)]` (`src/alloc_core/mod.rs:60-142`).
- **Проверка:** чистый перенос, `npm run check` и `verify-structure-limits` зелёные.

### R6-T42 — Мелкие дубли
- **Источник:** `code_quality_review.md` R6-CQ-6.
- **Где и что:**
  - `finalize_hit` в numa- и не-numa ветках (`find_segment.rs:547-571`) — поднять за пределы numa-ветвления;
  - три escape-варианта (`global_alloc.rs:41-49`, `:102-110`, `:147-177`) — один `exit_escape_alloc` (вместе с R6-T04);
  - классификация TLS-указателя (`tls_heap.rs:273`, `:332`, `:362`, `:398`, отличается в `:495`) — `fn classify(p) -> Local { Real, Null, Torn }`;
  - `exact_fatal.rs` — удалить (вместе с R6-T15).

### R6-T43 — Мёртвый код
- **Источник:** `code_quality_review.md` R6-CQ-10, R6-CQ-11.
- **Где и что:**
  - `src/registry/bootstrap/loom_shim.rs:329-463` — зеркало tagged-stack «without a current source caller» внутри tier-1 unsafe-файла. Удалить вместе с абзацами в `src/lib.rs:242-245` и `bootstrap/mod.rs:163-165`;
  - недостижимые `cfg(not(feature = "std"))` в `fallback.rs:338`, `:520` (`alloc-core` включает `std`) и лишние `feature = "std"`-конъюнкты (`fallback.rs:211`, `:332`, `:518`, `:627`, `:731`, `:764`, `:768`, `global/mod.rs:52`, `:74`).

### R6-T44 — Переименовать `AbandonGuard`
- **Источник:** `code_quality_review.md` R6-CQ-15.
- **Где:** `src/global/tls_heap.rs:8-9`, `:109`, `:131`, `:143`, `:157`, `:160-195`, `:378`, `:438`, `:544`; `src/lib.rs:225`; `global/mod.rs:8`; `registry/mod.rs:9`; `heap_slot.rs:20`; `ownership.rs:91`; `sefer_alloc/diag.rs:218`.
- **Что сделать:** переименовать в `LeaseReleaseGuard` (или `ThreadExitGuard`) и одновременно обновить инвентарь seam. README упоминает `AbandonGuard` в таблице unsafe.

### R6-T45 — Магические числа и имена
- **Источник:** `code_quality_review.md` R6-CQ-19, -20, -21.
- **CQ-19:**
  - `64` / `MIN_BLOCK * 64` (`sidecar_drain.rs:114`, `:221`, `sidecar_bitmap.rs:131`, `find_segment.rs:128`, `:145`, `segment_layout.rs:28`, `:34`) → `const SIDECAR_WORD_BYTES`;
  - `slot / 64`, `% 64` × ~15 → `fn word_bit`;
  - `SCAN_COUNTS[0..4]`, `CLAIM_SOURCES[0..4]` (+`[4]`), `FAIL_WORKER` = 1/2 → `#[repr(usize)] enum`;
  - `4096` вместо `MAX_HEAPS` (`segment_header_impl.rs:98-103`).
- **CQ-20:** `STATE_INITIALIZING` = 1 в `fallback.rs:62-64` и = 2 в `heap_slot.rs:89-99`; `STATE_LIVE` — алиас `STATE_OWNED`. Заменить на `enum FallbackInitState` / `enum SlotState` с `TryFrom<u8>`.
- **CQ-21:** `MaintenanceLease::slot_index() -> usize` против `HeapLease::slot_index() -> u32` (`claim.rs:470`, `:574`) — привести к одному типу. Повторы имён модулей — при следующем переносе.

### R6-T46 — Cfg-сетка и экспериментальные cfg
- **Источник:** `code_quality_review.md` R6-CQ-23, -24.
- **CQ-23:** параметры под cfg (`find_segment.rs:216`, `:638-647`, `fallback.rs:157-159`), тройной предикат (`fallback.rs:223-234`), 4 cfg-ветки ради двух счётчиков (`sefer_alloc/diag.rs:155-162`). Решение: контекстная структура с ZST-полями, `fn dbg_forced_oom() -> bool`, `*_total()` = 0 при выключенной фиче.
- **CQ-24:**
  - `cfg(r18_sidecar_scan_bench)` подменяет алгоритм `BitmapScan::next_cut` (`bitmap_scan.rs:3-65`);
  - `cfg(r19_sidecar_summary_oracle)` — 6 вставок в `drain_segment_sidecar` (`find_segment.rs:153-178`);
  - `PENDING_WORDS_EXCHANGED` == `PENDING_WORDS_VISITED` (`prf01_sidecar_oracle.rs:33-34`).

  По правилу 3 `CLAUDE.md` (пересмотр хуков после NO-GO): решить судьбу R18 (удалить ветку или перенести в bench-копию; связано с R6-T02/OPT-01), R19-оракул сделать ZST-наблюдателем с одной точкой вставки, дублирующий счётчик убрать.

### R6-T47 — `internals` меняет production-путь
- **Источник:** `code_quality_review.md` R6-CQ-27.
- **Где:** `src/global/maintenance_service.rs:151-161` (под `internals` воркер берёт `CONTROL` и `notify_all` на каждом проходе); `src/global/fallback.rs:203-208` (флаг panic-инъекции в инициализации fallback).
- **Механизм:** основная тестовая конфигурация `production internals` исполняет иную синхронизацию, чем поставляемый `production`.
- **Что сделать:** вынести подтверждения в наблюдатель без изменения блокировок или под `bench-internals`. Минимум — в `docs/CI_TIERS.md` указать, какая строка CI проверяет поведение без `internals`.

### R6-T48 — Разная реакция на одно условие
- **Источник:** `code_quality_review.md` R6-CQ-30.
- **Где:** `find_segment.rs:491-498` (abort) против `:585-592` (`None`) для `base.is_null() || kind ∉ {Small, Primordial}`; `sidecar_drain.rs:177`, `:288` — abort на `SegmentKind::Unknown`, хотя контракт `kind_at` (`segment_header_views.rs:33-39`) обещает no-op.
- **Что сделать:** унифицировать либо задокументировать различие. Контракт `kind_at` привести к фактическому поведению.

### R6-T49 — Публичный API и `missing_docs`
- **Источник:** `code_quality_review.md` R6-CQ-31, -32.
- **CQ-31:** `AllocStats::config_conflicts` (`alloc_stats.rs:160-173`) не включает конфликты fallback, которые доступны отдельно через `SeferAlloc::fallback_config_conflicts(&self)` (`core.rs:368-374`). Добавить поле в `#[non_exhaustive]` `AllocStats` или суммировать. `start_maintenance`/`maintenance_running` — ассоциированные, а `stats`/`trim_current_thread` — `&self`; у `maintenance_running` нет `#[must_use]`.
- **CQ-32:** `#![warn(missing_docs)]` в `src/lib.rs`. Для doc-hidden модулей — `allow` на модуль или однострочные доки. Без документации сейчас: `terminal_words.rs:32-85`, `segment_route/pin.rs:10-16`, `registration.rs:20-55`, `large_state.rs:29-51`, `directory.rs:708-715`, `free_slot_hint.rs:26-70`, `saturation.rs:20-36`.
- **Внимание:** изменение публичного API — semver. Согласовать с владельцем; версии не поднимать без явной просьбы.

## P4

### R6-T50 — Foreign `realloc`: ёмкость по резервации
- **Источник:** `memory_safety_review.md` R6-MS-3.
- **Где:** `src/registry/heap_core/free/realloc.rs:417-438`; `directory.rs:174-184` (`contains_payload`), `:813-835`; `alloc_core_large.rs:456-461`, `:692-697` (`len = reserved_capacity`); `segment_table_impl.rs:372` (Small payload = база сегмента).
- **Механизм:** при ложном `old_layout` foreign-ветка копирует из незакоммиченного хвоста (`large-reserved-capacity`) → SIGSEGV вместо null. Own-ветка ограничена `safe_payload_read_span`.
- **Что сделать:** хранить в дескрипторе верхнюю границу закоммиченного span. Для Small начинать payload с `small_meta_end()`/`primordial_meta_end()`.

### R6-T51 — Stale free после переиздания (U-R5-5)
- **Источник:** `memory_safety_review.md` R6-MS-4.
- **Где:** `alloc_core_small_reclaim.rs:38-67` (нет поколения); `segment_route/large_state.rs:15-28` (новый маршрут всегда gen = 1).
- **Что сделать:** для `hardened` — тег поколения выдачи в sidecar (свободные биты `ClassLeaves` или байт на гранулу). Иначе держать как residual в индексе.

### R6-T52 — `HeapSlot: Sync` и `Send` для `HeapCore`
- **Источник:** `memory_safety_review.md` R6-MS-5.
- **Где:** `src/registry/heap_slot.rs:204`, `:261-283`, `:285-304`; `claim.rs:381-396`, `:495-501`, `:612-617`.
- **Что сделать:** обёртка `struct SlotCore(HeapCore)` с `unsafe impl Send` и SAFETY «нет потокоаффинных полей»; compile-time `assert_send` для полей кроме перечисленных raw-указателей; чек-лист в doc `HeapCore`.

### R6-T53 — Loom: пробелы и расхождения
- **Источник:** `concurrency_review.md` R6-CC-8. Связано с item 186.
- **Что сделать:**
  1. actual-source loom для `ClassLeaves`: атомики `leaf_classes.rs:9` перевести на `super::{AtomicU8, AtomicPtr}` по образцу `sidecar_bitmap.rs`. Сейчас `tests/loom_sidecar_bitmap_actual.rs` использует `ClassMap::Dense`;
  2. в модели селектора (`loom_r11_registry_claim`, `loom_r19_registry_free_set`, `loom_r20_heap_reuse_spill`) добавить «bump перед scan», `candidate < count` / `before == max`, OOM конструктора, chunk-OOM recovery (ловят R6-T03, R6-T24). В `loom_r11_registry_claim::recycle_one` заменить `hint.store` на production-`swap` со spill;
  3. pin-vs-unlink — item 186;
  4. `fallback::LOCK`/`INIT_STATE` и handshake `MaintenanceService::start` (низкий риск);
  5. `tests/loom_sidecar_summary.rs` моделирует отвергнутый C‴. Удалить или пометить как нерелевантный и не засчитывать в покрытие.

### R6-T54 — Гипотезы exit/panic
- **Источник:** `concurrency_review.md` R6-CC-9; гипотезы Г-1/Г-2 в `memory_safety_review.md`.
- **Что сделать:**
  - разрешать `RtlDllShutdownInProgress` (`process_exit.rs:30-52`) заранее, при первой привязке или инициализации fallback, вне локов аллокатора;
  - проверить достижимость abort из `HeapLease::drop` при `panicking()` (`claim.rs:620-624`) через rollback-ветки `finish_bind` (`tls_heap.rs:494-498`, `:530-535`);
  - Г-1: сверить TLS без нативной поддержки (`!target_thread_local`) с рекурсией `current_for_alloc` (`tls_heap.rs:260-285`) и списком целей README.

### R6-T55 — Семантика `foreign_or_unroutable_frees`
- **Источник:** `correctness_review.md` R6-CR-10.
- **Где:** `alloc_stats.rs:137-151`; `alloc_core_large.rs:127-135` (считает локальный interior-reject Large); `dealloc_own_base.rs:299-310` (Small `RejectNoOp` не считается).
- **Что сделать:** отдельный счётчик локальных отказов или уточнить doc поля. Изменение `AllocStats` — semver, см. R6-T49.

### R6-T56 — Аддитивные фичи и FFI-тип
- **Источник:** `api_security_review.md` R6-SEC-13, R6-SEC-16.
- **SEC-13:** любая зависимость может включить `exact-object-proto`, `virgin-zero-skip`, `hardened`, `bench-internals` и молча поменять глобальный аллокатор. Описать в README/`SECURITY.md`; для прототипов рассмотреть `--cfg`, как уже сделано для mock (`Cargo.toml:699-723`).
- **SEC-16:** `process_exit.rs:11-15` объявляет `fn() -> isize`, а `:66-71` вызывает как `fn() -> u8` через двойной `transmute`. Объявить точный тип.

### R6-T57 — Файлы у лимита, плотность комментариев, ID
- **Источник:** `code_quality_review.md` R6-CQ-4, -18, -33.
- **CQ-4:** у лимита 1000 строк: `alloc_core_small_impl.rs` (997), `segment_table_impl.rs` (984), `directory.rs` (957, девять типов — граница `Block`/`BlockIndex`/`SpareBlock` против `Shard`/`RouteDirectory`), `os.rs` (898). Разделение — чистым переносом до следующей содержательной правки.
- **CQ-18:** 57 % непустых строк — комментарии, 2159 меток задач, история в публичном rustdoc, кириллические `Э*`. В коде держать контракт, порядок и SAFETY; историю — в ADR/`CHANGELOG.md`/perf-доках.
- **CQ-33:** ID этого раунда пересекаются с метками в коде. Ссылаться через путь отчёта или `R6-Txx`.

---

## Рекомендуемый порядок

1. **Быстрые и детерминированные:** R6-T01; R6-T39 (PF-7, PF-10) + R6-T10(а) одним iai-гейтом; R6-T13 (сначала asm).
2. **Корректность прогресса и выхода:** R6-T03, R6-T04 (+R6-T42 escape-helper), R6-T09, R6-T24.
3. **Политика отказов:** R6-T14 + R6-T15 одним проходом с lexical-guard.
4. **Документация и контракты:** R6-T17, R6-T36, R6-T21 — снимают ложные модели для всех последующих правок.
5. **Hardening и DoS:** R6-T05, R6-T06, R6-T07, R6-T08, R6-T28 — каждый с решением «код или документация» и ценой по iai.
6. **Рефакторинг:** R6-T18 → R6-T12 → R6-T19, R6-T20, R6-T16; остальные P3 чистки.
7. **После решения владельца:** R6-T02 (+R17-OPT-01, R6-T46 по R18).
