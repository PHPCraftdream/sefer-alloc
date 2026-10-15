# Ревью раунда 6: качество кода и «запахи» в `src/`

Дата: 2026-10-15
Объект: `src/` в коммите `132ab85bd2fa443cfa5c6e0fbdea46231c3beaaf`
Контекст: `CLAUDE.md`, `docs/agent_reviews_round5/code_quality_review.md`, `scripts/verify-dbg-hook-safety.mjs`, `docs/structure_limits_baseline.csv`

## Метод и ограничения

Ревью сделано только статическим чтением исходников `src/`, `Cargo.toml` и перечисленных документов. Я не запускал `cargo build/test/clippy`, бенчмарки и проектные скрипты (`npm run check`, `verify-*.mjs`). Поэтому ни реальный набор предупреждений компилятора, ни поведение во время выполнения не проверены. Счётчики (строки, вызовы, вхождения) получены поиском по тексту (grep/awk). Длина функций измерена эвристически: от строки `fn` до закрывающей скобки на том же отступе, комментарии включены. Утверждение «вызовов не найдено» означает только, что текстовый поиск их не нашёл. Это не доказательство через компилятор.

Пересечение идентификаторов. В `src/` уже есть метки `R6-CQ-2`, `R6-CQ-7a`, `R6-CQ-7b`, `R6-CQ-7c` от июльской remediation-волны (например, `src/alloc_core/segment/segment_header/segment_header_views.rs:2`, `src/registry/heap_core/alloc/hot.rs:2`). Идентификаторы этого отчёта (`R6-CQ-1`…`R6-CQ-33`) — новое пространство имён, оно с ними не связано. См. R6-CQ-33.

## Итог

Механические конвенции соблюдены:
- нет inline `#[cfg(test)] mod tests`;
- нет runnable doctest (все 20 doc-блоков кода имеют пометку ` ```text `);
- нет TODO/FIXME/`todo!`/`unimplemented!`;
- все файлы короче 1000 строк;
- каталоги сверх 7 записей ровно совпадают с ratchet-строками `docs/structure_limits_baseline.csv`;
- ни один `mod.rs` не определяет `fn/struct/enum/impl/const/static`;
- список tier-1 модулей с `#![allow(unsafe_code)]` (22 файла) совпадает с инвентарём в шапке `src/lib.rs`.

Главные замечания round5 закрыты:
- `AllocCore/HeapCore::{dealloc,realloc}` стали `unsafe fn`;
- `dbg_stamp_*` стали `unsafe` и внесены в `UNSAFE_HOOKS`;
- паникующий `debug_assert!` конфликта конфигурации удалён (`src/registry/heap_registry/claim.rs:186-196`);
- `alloc-runfreelist` и future-only заготовки удалены;
- описание в `Cargo.toml` исправлено.

Главный долг сейчас — не soundness, а сопровождаемость:
1. Документация в коде массово описывает удалённые механизмы (ring, TFS, `HeapOverflow`, `AbandonGuard`) и ссылается на ~135 несуществующих «плоских» файлов. Сюда же относятся устаревший SAFETY-аргумент у `unsafe impl GlobalAlloc` и неверное описание протокола bootstrap.
2. В конфигурации `production` без `internals` весь аллокатор (~44 тыс. строк) собирается под blanket `#[allow(dead_code, unused_imports)]`, поэтому компилятор не может ловить мёртвый код.
3. Обработка ошибок разнородна:
   - 108 голых `std::process::abort()` без диагностики, хотя есть помощник `fatal()`;
   - на путях, достижимых из `GlobalAlloc`, остаются конструкции, способные паниковать.
4. Логика дублируется. Пример: три копии цикла слива sidecar. Предикат достижимости промоушена вручную скопирован 18 раз в 9 файлах.

Итого 33 находки: High — 0, Medium — 9, Low — 20, Info — 4.

## Сводная таблица

| ID | Сер. | Категория | Место | Кратко |
|---|---|---|---|---|
| R6-CQ-1 | Low | Конвенции | `src/alloc_core/config/profile.rs:111,160,360` | Три стабильных публичных типа в одном файле |
| R6-CQ-2 | Low | Конвенции | `src/registry/heap_registry/claim.rs:35,462,558`; `src/global/tls_heap.rs:239-390`; `src/global/fallback.rs:62-64,142-405` | Внутренние production-API объявлены `pub` в doc-hidden модулях; несколько экспортов в файле |
| R6-CQ-3 | Low | Конвенции | `src/alloc_core/alloc_core/mod.rs:45-106`; `src/alloc_core/segment/mod.rs:24-45`; `src/alloc_core/mod.rs:60-142` | Лишние и межкаталожные `#[path]`, glob-реэкспорты `*_impl::*`, 9 алиасов «для совместимости путей» |
| R6-CQ-4 | Info | Конвенции | `.../alloc_core_small_impl.rs` (997), `.../segment_table_impl.rs` (984) | Файлы в 3–16 строках от лимита 1000 |
| R6-CQ-5 | Medium | Дублирование | `.../find_segment.rs:136-191`; `src/alloc_core/alloc_core/sidecar_drain.rs:88-184,194-292` | Три копии цикла слива Small sidecar |
| R6-CQ-6 | Low | Дублирование | `find_segment.rs:547-571`; `global_alloc.rs:41-49,102-110,147-177`; `tls_heap.rs:273,332,362,398`; `exact_fatal.rs:3-7` | Повторяющиеся фрагменты и дубль `fatal()` |
| R6-CQ-7 | Medium | Длинные функции | `find_segment.rs:205` (424 стр.), `bootstrap.rs:35` (362), `alloc_core_large.rs:170` (353), … | Функции по 180–424 строки с плотной cfg-сеткой |
| R6-CQ-8 | Medium | Мёртвый код | `src/lib.rs:457-459,479-481,491-493` | Без `internals` (то есть в `production`) dead-code lint выключен на весь аллокатор |
| R6-CQ-9 | Low | Мёртвый код | `os.rs:512`; `segment_header_views.rs:9`; `segment_table_impl.rs:696`; `claim.rs:572`; `segment/mod.rs:47` | Устаревшие или необоснованные `#[allow(dead_code)]` |
| R6-CQ-10 | Low | Мёртвый код | `src/registry/bootstrap/loom_shim.rs:329-463` | Зеркало tagged-stack «без текущего вызывающего» внутри tier-1 unsafe-файла |
| R6-CQ-11 | Low | Мёртвый код | `src/global/fallback.rs:338,520` и др. | `cfg(not(feature = "std"))` недостижим: `alloc-core` включает `std` |
| R6-CQ-12 | Medium | Устаревшие комментарии | `free/dealloc.rs:188`; `state/tcache.rs:23`; `free/dealloc_batch.rs:60,229-231,276`; … | Удалённые ring/TFS/HeapOverflow описаны в настоящем времени, в том числе в контракте `unsafe fn` |
| R6-CQ-13 | Medium | Устаревшие комментарии | `src/registry/bootstrap/mod.rs:53-97,138` | Модульная документация описывает несуществующий протокол bootstrap |
| R6-CQ-14 | Medium | Устаревшие комментарии | `src/global/sefer_alloc/global_alloc.rs:1-5,21-32` | Устаревший SAFETY-аргумент у `unsafe impl GlobalAlloc` |
| R6-CQ-15 | Low | Имена/комментарии | `tls_heap.rs:8-9,143,157,160-195`; `lib.rs:225`; `registry/mod.rs:9` | `AbandonGuard` назван по удалённому протоколу |
| R6-CQ-16 | Low | Устаревшие комментарии | ~135 мест, в том числе `alloc_core/alloc_core/mod.rs:23`, `realloc_fastpath.rs:364` | Ссылки на несуществующие файлы и `file.rs:NNN` |
| R6-CQ-17 | Low | Устаревшие комментарии | `alloc_core/alloc_core/mod.rs:35-42`; `registry/mod.rs:14-21`; `segment_header_layout.rs:26,31-37`; … | Устаревшие утверждения о фазах, статусах, разделах, количествах |
| R6-CQ-18 | Info | Комментарии | весь `src/` | 57% непустых строк — комментарии, 2159 меток задач; история в публичном rustdoc |
| R6-CQ-19 | Low | Магические числа | `sidecar_drain.rs:114,221`; `segment_directory_impl.rs:442-592`; `bitmap_scan.rs:30,59-62`; `claim.rs:55-67`; … | `64`, `MIN_BLOCK * 64`, «сырые» индексы счётчиков |
| R6-CQ-20 | Low | Имена | `src/global/fallback.rs:62-64`; `src/registry/heap_slot.rs:89-99` | `STATE_INITIALIZING` = 1 в одном модуле и = 2 в другом; `STATE_LIVE` — алиас `STATE_OWNED` |
| R6-CQ-21 | Info | Имена | `alloc_core::alloc_core::alloc_core_core_diag`; `claim.rs:470,574` | Повторы в именах модулей; разный тип `slot_index()` |
| R6-CQ-22 | Medium | cfg-матрицы | 18 копий в 9 файлах (`alloc_core/mod.rs:202-209`, `dealloc_own_base.rs:215-226,260-269,324-336`, …) | Предикат `medium_promotion_reachable` размножен вручную |
| R6-CQ-23 | Low | cfg-матрицы | `find_segment.rs:216,638-647`; `fallback.rs:157-159,223-234`; `sefer_alloc/diag.rs:155-162` | Параметры функций под cfg, лестницы из 4 cfg-ветвей |
| R6-CQ-24 | Low | cfg-матрицы | `remote_bitmap/bitmap_scan.rs:3-65`; `find_segment.rs:153-178`; `prf01_sidecar_oracle.rs:33-34` | Экспериментальный алгоритм R18 и оракул R19 в горячем пути; дублирующий счётчик |
| R6-CQ-25 | Low | dbg-поверхность | `src/global/tls_heap.rs:95-101,651-655` | tier-1 seam на весь файл ради одного bench-хука |
| R6-CQ-26 | Low | dbg-поверхность | `src/alloc_core/alloc_core/state.rs:184-188`; `totals.rs:26-47`; `alloc_core_small_pool_impl.rs:504` | Внутренний примитив виден в `production`; production-аксессоры названы `dbg_*` |
| R6-CQ-27 | Low | dbg-поверхность | `maintenance_service.rs:151-161`; `fallback.rs:203-208` | `internals` меняет production-путь, который потом и тестируется |
| R6-CQ-28 | Medium | Обработка ошибок | 108 мест, `fatal()` используется 1 раз (`alloc_core_small_impl.rs:403`) | Голый `abort()` без диагностики |
| R6-CQ-29 | Medium | Обработка ошибок | `alloc_core_large_cache.rs:146-170,364`; `terminal_words.rs:49`; `alloc_core_small_magazine.rs:169` | Паникующие конструкции на путях из `GlobalAlloc`, хотя политика для порчи — abort |
| R6-CQ-30 | Low | Обработка ошибок | `find_segment.rs:491-498` против `:585-592`; `sidecar_drain.rs:177,288` | Одно и то же условие то abort, то `None` |
| R6-CQ-31 | Low | Публичный API | `alloc_stats.rs:160-173`; `sefer_alloc/core.rs:368-374`; `maintenance.rs:36,43` | Конфликты конфигурации разнесены по двум API; непоследовательная форма API |
| R6-CQ-32 | Low | Документация | `Cargo.toml` `[workspace.lints]`; `terminal_words.rs:32-85`; `segment_route/*` | Нет `missing_docs`; недокументированные элементы `internals` |
| R6-CQ-33 | Info | Процесс | `segment_header_views.rs:2` и др. | Пространство ID `R6-CQ-*` уже занято метками в коде |

## Подробности по категориям

### A. Конвенции `CLAUDE.md` (one file — one export, `mod.rs`, лимиты)

**R6-CQ-1 — Low — `src/alloc_core/config/profile.rs:111,160,360`**
- Суть: файл определяет три стабильных публичных типа — `SmallPoolPolicy` (111), `LargeCachePolicy` (160) и `Profile` (360). Все три реэкспортированы из корня крейта (`src/lib.rs:514-518`).
- Почему плохо: это не doc-hidden test-forwarder, не кластер протокольных констант и не seam-крейт. Ни одно санкционированное исключение «one file — one export» сюда не подходит. Из-за этого правило соблюдается выборочно.
- Рекомендация: вынести `small_pool_policy.rs` и `large_cache_policy.rs`. В `config/` сейчас 5 записей, станет 7 — в пределах лимита. Реэкспорты в `src/alloc_core/mod.rs:237` и `src/lib.rs` сохранить.

**R6-CQ-2 — Low — внутренние production-API объявлены `pub` (а не `pub(crate)`) в doc-hidden модулях**
- Места:
  - `src/registry/heap_registry/claim.rs:35,462,558` — три `pub struct`: `HeapRegistry`, `MaintenanceLease`, `HeapLease`;
  - `src/global/tls_heap.rs:239,260,290,325,356,390` — `CurrentHeap`, `current_for_*`;
  - `src/global/fallback.rs:62-64,142,148,352` — `STATE_*`, `heap_ptr`, `with_heap`;
  - `src/registry/heap_registry/counters.rs:67,80,161,233,289`;
  - `src/alloc_core/segment/segment_header/terminal_words.rs:32-79` — `pub` элементы в модуле, который реэкспортируется только как `pub(crate) use terminal_words::*` (`segment_header/mod.rs:60`).
- Суть: эти элементы — настоящий production-код, а не тестовые forwarder'ы. `pub` им нужен только потому, что под `internals` родительский модуль становится `pub`.
- Почему плохо: размывается граница `internals`. Вся поверхность этих модулей становится доступна интеграционным тестам и downstream-коду при `--features internals`. Санкционированное исключение 1 из `CLAUDE.md` распространяется на внутренние функции, хотя рассчитано на тестовые хуки. Из-за этого в файлах по 3–14 «экспортов». Для `terminal_words.rs` ключевое слово `pub` вообще ничего не открывает и только вводит в заблуждение.
- Рекомендация: production-элементы перевести в `pub(crate)`. Наружу оставлять только явные `#[doc(hidden)]` forwarder'ы с префиксом `dbg_` или суффиксом `_for_test`, которые уже учтены в `scripts/verify-dbg-hook-safety.mjs`. Типы-аренды (`HeapLease`/`MaintenanceLease`) развести по своим файлам.

**R6-CQ-3 — Low — способ связывания модулей в `mod.rs`**
- Места:
  - `src/alloc_core/alloc_core/mod.rs:45-104` — восемь `#[path = "x.rs"]` указывают на путь, который Rust и так выбирает по умолчанию. Та же картина в `src/alloc_core/segment/segment_header/mod.rs:43-58`, `src/alloc_core/segment/segment_table/mod.rs:52-76`, `src/alloc_core/small/alloc_core_small/mod.rs:18-34`;
  - межкаталожные `#[path]`: файлы из `segment_header/` и `segment_directory/` подключены как дети модуля `segment` (`src/alloc_core/segment/mod.rs:24-29,44`);
  - 7 glob-реэкспортов `pub(crate)? use *_impl::*` (например, `alloc_core/alloc_core/mod.rs:106`, `segment_header/mod.rs:51`, `segment_table/mod.rs:78`);
  - 9 алиасов `#[allow(unused_imports)] use small::alloc_core_small;` и подобных — только «для совместимости путей модулей» (`src/alloc_core/mod.rs:60-142`, `segment_header_impl.rs:8-14`).
- Почему плохо: дерево модулей перестаёт совпадать с деревом каталогов. Glob-реэкспорт скрывает, что именно экспортируется. Алиасы-заглушки нужны только для того, чтобы старые пути продолжали резолвиться, и подавляют lint.
- Рекомендация: убрать лишние `#[path]`. Межкаталожные модули либо перенести физически, либо объявить внутри их каталога. Glob заменить явными списками. Внутренние пути обновить один раз и удалить алиасы вместе с их `allow`.

**R6-CQ-4 — Info — файлы у лимита**
- Места: `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs` (997 строк), `src/alloc_core/segment/segment_table/segment_table_impl.rs` (984), `src/registry/segment_route/directory.rs` (957, девять типов), `src/alloc_core/platform/os.rs` (898).
- Суть: следующая же содержательная правка уронит `verify-structure-limits`.
- Рекомендация: заранее спланировать разделение, чистым переносом кода. Для `directory.rs` естественная граница — `Block`/`BlockIndex`/`SpareBlock` против `Shard`/`RouteDirectory`.

### B. Дублирование логики

**R6-CQ-5 — Medium — три копии цикла слива Small sidecar**
- Места: `src/alloc_core/small/alloc_core_small/find_segment.rs:136-191` (`drain_segment_sidecar`), `src/alloc_core/alloc_core/sidecar_drain.rs:88-184` (`drain_sidecar_ingress_bounded`), `src/alloc_core/alloc_core/sidecar_drain.rs:194-292` (`drain_sidecar_ingress`).
- Суть: каждая копия заново:
  - считает `end_word = high_water.div_ceil(MIN_BLOCK * 64)` и `start_word`;
  - вызывает `scan_small_route_from(..)` с `else { abort() }`;
  - выполняет `cut.pop()` и `reclaim_sidecar_record`, накапливая `changed_classes |= 1 << class`;
  - вызывает `sync_directory_for_segment_classes` и `dec_live_and_maybe_decommit` / `release_or_pool_empty_segment`.

  Large-ветка (`claim_large_route` → `reclaim_large_segment` → `LARGE_REMOTE_RETIREMENTS`) дословно повторена в двух копиях.
- Почему плохо: здесь проходит граница времени жизни резервации. Комментарии прямо требуют, чтобы «все cut и заимствования route закончились до release». Любое изменение этого порядка придётся синхронно вносить в три места. Копии уже расходятся: по-разному клампят `start_word`, а bench-счётчики стоят только в двух.
- Рекомендация: выделить `fn consume_small_cuts(&mut self, base, index, scan) -> (u64 /*changed*/, usize)` и `fn finish_small_drain(&mut self, base, index, changed) -> DrainEnd { Live, Pooled, Released }`, а также общий `fn retire_large_route(..) -> bool`. Bench-наблюдатели передавать параметром-замыканием или ZST-наблюдателем.

**R6-CQ-6 — Low — повторяющиеся мелкие фрагменты**
- Места:
  - `find_segment.rs:547-556` и `:562-571` — идентичные вызовы `finalize_hit(..)` в numa- и не-numa ветках; различается только окружающий cfg;
  - `src/global/sefer_alloc/global_alloc.rs:41-49`, `:102-110`, `:147-177` — три варианта «fallback вернул `None` → во время выхода процесса уходим в `System`»;
  - `src/global/tls_heap.rs:273,332,362,398` — одно и то же условие классификации `p.addr().wrapping_sub(1) < usize::MAX - 1`; `finish_bind` (`:495`) проверяет иначе (`!is_null() && != TORN`);
  - `src/global/exact_object/exact_fatal.rs:3-7` побайтно повторяет `src/alloc_core/platform/fatal.rs:4-8`.
- Рекомендация: `#[inline(always)] fn classify(p) -> Local { Real(p), Null, Torn }` в `tls_heap.rs`; один `fn exit_escape_alloc(layout, zeroed)` в `global_alloc.rs`; удалить `exact_fatal` в пользу `platform::fatal`; поднять `finalize_hit` за пределы numa-ветвления.

### C. Длинные функции и глубокая вложенность

**R6-CQ-7 — Medium — функции по 180–424 строки**
- Самые длинные (строки включают комментарии):
  - `find_segment_with_free_impl` — `src/alloc_core/small/alloc_core_small/find_segment.rs:205`, 424 строки; в файле 69 `#[cfg]`, до 6 уровней вложенности (`if` → `for` → `for` → `while` → `if let`);
  - `primordial` — `src/alloc_core/alloc_core/bootstrap.rs:35`, 362;
  - `alloc_large` — `src/alloc_core/large/alloc_core_large.rs:170`, 353;
  - `HeapCore::realloc` — `src/registry/heap_core/free/realloc.rs:137`, 303;
  - `dealloc_at_base` — `src/alloc_core/alloc_core/mem/mem_impl.rs:263`, 290, `#[inline(always)]`;
  - `dealloc_own_thread_with_base` — `src/registry/heap_core/free/dealloc_own_base.rs:347`, 205, `#[inline(always)]`;
  - `alloc_with_class` — `src/registry/heap_core/alloc/hot.rs:183`, 194, `#[inline(always)]`;
  - `heap_ptr_impl` — `src/global/fallback.rs:157`, 186;
  - `realloc_inplace_fast_path_known_base` — `.../mem/realloc_fastpath.rs:225`, 185;
  - `refill_class_bump_impl` — `src/alloc_core/small/alloc_core_small_magazine.rs:180`, 180;
  - `claim_impl` — `src/registry/heap_registry/claim.rs:241`, 178.
- Почему плохо: ветки под cfg перемешаны с горячей логикой, поэтому трудно проверить, какой код реально собирается в данной конфигурации. Три функции с `#[inline(always)]` по 200–290 строк вклеивают холодные ветки (promotion, hardened, Large-маршрутизация) в каждый вызов.
- Рекомендация: в `find_segment_with_free_impl` выделить `scan_directory_candidates`, `linear_scan` и `numa_fallback_pick`. Холодные ветки `dealloc_*`/`alloc_with_class` вынести в `#[cold] #[inline(never)]` помощники. Не снимая `inline(always)` с горячего ядра, проверить эффект через `npm run iai`, по правилам perf-гейтов.

### D. Мёртвый код и `#[allow(...)]`

**R6-CQ-8 — Medium — `src/lib.rs:457-459,479-481,491-493`**
- Суть: при `not(feature = "internals")` модули `alloc_core`, `global` и `registry` объявлены с `#[allow(dead_code, unused_imports)]`. `production` не включает `internals` (`Cargo.toml`: `production = [...]`). Значит, в поставляемой конфигурации компилятор не проверяет мёртвый код примерно в 44 тыс. строк аллокатора.
- Почему плохо: настоящий мёртвый код маскируется так же, как тестовая поверхность. Примеры:
  - `Segment::len` (`src/alloc_core/platform/os.rs:512-515`) — вызовов не найдено;
  - зеркало стека в `loom_shim` (R6-CQ-10).

  CI-строка clippy для `production` из-за этого `allow` не может поймать ни одного случая `dead_code`.
- Рекомендация: снять blanket-allow. Тестовую поверхность уже почти везде гейтят по `internals` поэлементно (`alloc_core_core_diag/mod.rs:46-57`, impl-блоки `#[cfg(feature = "internals")]`). Для остатка использовать поэлементный `#[cfg_attr(not(feature = "internals"), allow(dead_code))]` с причиной.

**R6-CQ-9 — Low — устаревшие или необоснованные `#[allow(dead_code)]`**
- Места:
  - `src/alloc_core/platform/os.rs:512` — «Phase 9+ heaps read it», вызовов не найдено;
  - `src/alloc_core/segment/segment_header/segment_header_views.rs:9` — «Used by Phase 9+ cross-thread routing», хотя у `kind_at` десятки вызовов и allow устарел;
  - `src/alloc_core/segment/segment_table/segment_table_impl.rs:696` — «tests / Phase 9 use it», хотя `table.count()` вызывается около 20 раз;
  - `src/registry/heap_registry/claim.rs:572,579` — «Ph4c migrates tests onto it», но Ph4c уже выполнен (см. `:587`);
  - `src/alloc_core/segment/mod.rs:47` — `#[allow(dead_code)]` на весь production-модуль `remote_bitmap` без объяснения;
  - `src/alloc_core/segment/remote_bitmap/mod.rs:4-10` — комментарий «Source-include tests compile this module without the drain» не объясняет `allow` у `Prf01SidecarOracle`;
  - `src/alloc_core/platform/sidecar_stats.rs:48` — `#![allow(dead_code)]` на файл;
  - `#[allow(unused_variables)]` на всю функцию: `find_segment.rs:638`, `dealloc_own_base.rs:188`.
- Почему плохо: подавления остаются после того, как исчезла их причина, и прячут новые предупреждения.
- Рекомендация: снять `allow` с живых элементов. Для мёртвых удалить элемент или поставить точный `cfg_attr(not(feature = …))`. `unused_variables` подавлять точечно через `let _ = x;` в соответствующей cfg-ветке.

**R6-CQ-10 — Low — `src/registry/bootstrap/loom_shim.rs:329-463`**
- Суть: `StackHead`/`StackStorage`/`StackOps` — локальное зеркало tagged-index-stack. Комментарий на `:329-331` прямо говорит «with no current source caller», и это же повторено в `src/lib.rs:242-245` и `src/registry/bootstrap/mod.rs:163-165`. Около 130 строк кода и почти столько же комментариев о расхождениях (`:290-326`) лежат в файле с tier-1 `#![allow(unsafe_code)]`.
- Почему плохо: мёртвый код внутри unsafe-seam расширяет поверхность аудита. Комментарий о «намеренных расхождениях» придётся поддерживать вечно.
- Рекомендация: удалить зеркало и связанные с ним абзацы. Корневые loom-модели уже импортируют сам крейт `tagged-index-stack`.

**R6-CQ-11 — Low — недостижимые cfg-ветви `std`**
- Суть: `alloc-core = ["std", …]` (`Cargo.toml:168`), а `global` требует `alloc-global`. Поэтому ветки `#[cfg(not(feature = "std"))]` в `src/global/fallback.rs:338,520` никогда не компилируются. Условия `feature = "std"` в `fallback.rs:211,332,518,627,731,764,768` и `src/global/mod.rs:52,74` всегда истинны.
- Почему плохо: создаётся видимость поддержки `no_std` и раздувается cfg-сетка.
- Рекомендация: удалить недостижимые ветки и лишние конъюнкты.

### E. Устаревшие комментарии

**R6-CQ-12 — Medium — удалённые механизмы описаны как действующие**
- Места:
  - `src/registry/heap_core/free/dealloc.rs:188` — документация `unsafe fn HeapCore::dealloc`: «route cross-thread via the TFS». TFS удалён (`src/global/tls_heap.rs:442-447`);
  - `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:158-161` — «Cross-thread frees arrive via the inline TFS and are drained by `HeapCore::alloc` BEFORE this runs»; сейчас слив идёт при промахе (`find_segment.rs:136`);
  - `src/registry/heap_core/state/tcache.rs:23` — «they go to the per-segment ring»;
  - `src/registry/heap_core/free/dealloc_batch.rs:60,229-231,276` — «cross-thread ring push», «ring entry», ссылка на несуществующий `dealloc_foreign_routing`;
  - `src/registry/heap_core/alloc/batch.rs:101` — «cross-thread double-free ring entry»;
  - `src/registry/heap_core/alloc/hot.rs:24` («drain the TFS»), `:577` («drained its `HeapOverflow`»);
  - `src/alloc_core/small/alloc_core_small/directory.rs:223,229-230` — «ring-drain closure via `entry_class_idx(off)`»; такой функции нет;
  - `src/registry/heap_core/diag/queries.rs:294` (`drain_heap_overflow`, такой функции нет), `:330` («overflow-ring reclaim»);
  - `src/registry/heap_core/diag/mod.rs:2` и `diag_probes.rs:3` — «ring-simulation / ring-push hooks»;
  - `src/alloc_core/segment/segment_header/segment_header_impl.rs:82` — `heap_core_xthread::ring::owner_slot_is_live`; ни модуля, ни функции нет;
  - `src/registry/heap_core/free/dealloc_own_base.rs:183` — X7 упомянут честно, как «historical».
- Почему плохо: у сопровождающего складывается неверная модель маршрутизации cross-thread free. Особенно опасно, что ложное утверждение стоит в контракте `unsafe fn`. Тест `tests/no_stale_doc_references.rs` (2923 строки) эти токены не ловит.
- Рекомендация: один проход по `src/`, замена текста на текущий протокол (route directory → pinned sidecar → terminal publication). Добавить в `tests/no_stale_doc_references.rs` запрещённые токены: `TFS`, `ring push`, `ring entry`, `HeapOverflow` (вне исторического контекста), `drain_heap_overflow`, `owner_slot_is_live`, `entry_class_idx`, `dealloc_foreign_routing`.

**R6-CQ-13 — Medium — `src/registry/bootstrap/mod.rs:53-97,138`**
- Суть: модульная документация описывает протокол, которого уже нет.
  - `:68-73`: «Each `AtomicPtr<RegistryChunk>` in `Registry::chunks`». На деле `chunks: [OncePtrCell<RegistryChunk>; NUM_CHUNKS]` (`src/registry/bootstrap/registry.rs:72`, и сам этот файл так пишет в `:29`).
  - `:83-90`: «Calls `aligned_vmem::reserve_aligned(CHUNK_SIZE, CHUNK_ALIGN)` … `mem::forget(reservation)`». На деле `aligned_vmem::leak_zeroed_pages(CHUNK_SIZE)` (`src/registry/bootstrap/ensure.rs:107`), и сам `mod.rs:138` пишет `leak_zeroed_pages` — файл противоречит сам себе.
  - `:41-66` (RAD-1): `next_free`, `NEXT_FREE_TAIL`, `push_free_slot`/`pop_free_slot` — в `src/registry` этих имён больше нет.
  - `:53`: «now just 64 pointers + 2 atomics» — жёстко прописанное количество, которое уже противоречит `:29-30` («plus claim/scan/hint control words»).
- Почему плохо: это главная документация протокола bootstrap, на котором держится M5 (отсутствие реентерабельности) и OOM-поведение. Неверное описание ведёт к неверным правкам.
- Рекомендация: переписать как описание текущего состояния (`OncePtrCell` + `leak_zeroed_pages` + per-chunk OOM). История RAD-1 и чанкинга — в ADR или `CHANGELOG.md`.

**R6-CQ-14 — Medium — `src/global/sefer_alloc/global_alloc.rs:1-5,21-32`**
- Суть: SAFETY-аргумент у `unsafe impl GlobalAlloc` устарел.
  - «`HeapCore` returns null on OOM (never panics — the substrate panic sites were hardened in Phase 11)». Это противоречит перечню «reviewed release panic sites» в `src/global/sefer_alloc/mod.rs:98-125`.
  - «`dealloc` on the fallback is sound under the fallback's spinlock» через `current_heap()`. Но `dealloc` идёт через `current_for_dealloc()` и `HeapCore::publish_foreign` (`:70-81`), а не через `current_heap`/fallback.
  - Шапка файла (`:1-5`) утверждает, что «The ONLY unsafe here is the `unsafe impl GlobalAlloc` plus the pointer handoff». На деле в файле ещё `System.alloc`/`alloc_zeroed` и `copy_nonoverlapping` (`:46,107,132-137,165-171`) и `unsafe`-хук `dbg_dealloc_while_fallback_lock_held` (`:196`).
- Почему плохо: SAFETY-комментарий у `unsafe impl` — основной документ аудита этой seam. Сейчас он описывает другую реализацию.
- Рекомендация: переписать по текущим путям: four-way dispatch, `publish_foreign`, побег в System при выходе процесса, сослаться на failure policy в `sefer_alloc/mod.rs:178-197`. Шапку привести к фактическому набору `unsafe`.

**R6-CQ-15 — Low — имя `AbandonGuard`**
- Места: `src/global/tls_heap.rs:8-9,109,131,143,157,160-195,378,438,544`; `src/lib.rs:225`; `src/global/mod.rs:8`; `src/registry/mod.rs:9`; `src/registry/heap_slot.rs:20`; `src/registry/heap_core/state/ownership.rs:91`; `src/global/sefer_alloc/diag.rs:218`.
- Суть: guard отпускает аренду (`HeapLease`), то есть переводит слот `LIVE → FREE`. «Abandon» — название удалённого протокола abandon/adopt. Документации приходится отрицать собственное имя: «`AbandonGuard::drop` does NOT abandon segments» (`tls_heap.rs:8-9`, `registry/mod.rs:9`).
- Рекомендация: переименовать в `LeaseReleaseGuard` (или `ThreadExitGuard`) и одновременно поправить инвентарь seam в `src/lib.rs:225`.

**R6-CQ-16 — Low — ссылки на несуществующие файлы и номера строк**
- Суть: вне явного контекста «former/pre-reorg» есть около 135 упоминаний несуществующих «плоских» файлов. Всего: `alloc_core.rs` — 37, `alloc_core_small_pool.rs` — 32, `alloc_core_small.rs` — 28, `alloc_core_core_diag.rs` — 24, `heap_core.rs` — 16, `segment_header.rs` — 10, `heap_core_free.rs` — 7, `heap_core_diag.rs` — 6, `sefer_alloc.rs` — 4. Самые вредные:
  - `src/alloc_core/alloc_core/mod.rs:23` — самопроверочный рецепт `grep -n "pub unsafe fn dealloc…" src/alloc_core/alloc_core.rs` на отсутствующий файл; grep молча вернёт пусто;
  - `src/alloc_core/alloc_core/mem/realloc_fastpath.rs:364` — `alloc_core_small.rs:1508-1533`: файла нет, а номера строк больше лимита 1000;
  - `src/alloc_core/small/reserved_small_segment.rs:78,212` — `alloc_core_small_pool.rs:1095/1117`;
  - `src/alloc_core/alloc_core/sidecar_drain.rs:188,220` — `sidecar_bitmap.rs:116-121`, хотя отказ class-0 находится в `publish`, `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs:106-115`;
  - `src/registry/heap_core/diag/queries.rs:299,312,336`, `src/global/tls_heap.rs:649`, `src/alloc_core/mod.rs:184`, `src/alloc_core/alloc_core/counters.rs:246`, `src/alloc_core/alloc_core/alloc_core_core_diag/mod.rs:29`.
- Рекомендация: ссылаться на элементы по пути (`AllocCore::dealloc`, `SidecarBitmap::publish`), а не на файлы и строки. Запретить шаблон `\.rs:\d+` в комментариях `src/` через `tests/no_stale_doc_references.rs`.

**R6-CQ-17 — Low — прочие устаревшие утверждения**
- Места:
  - `src/alloc_core/alloc_core/mod.rs:35-36` — «Phase 8 is single-threaded… Per-thread heaps + lock-free cross-thread free are Phase 9/10»; `:41-42` — грамматически сломанная фраза «its owning thread changes when the slot is lease handoffs»;
  - `src/registry/mod.rs:14-21` — «`pub` only so integration tests … before 12.3 wires it into `SeferAlloc` … or moves behind a `registry-test` dev-feature»; сейчас это `internals`;
  - `src/alloc_core/segment/segment_header/segment_header_layout.rs:31-37` — «RAD-5 GO/NO-GO EXPERIMENT … before assuming this offset is load-bearing», хотя `src/alloc_core/segment/bitmap/magazine_bitmap.rs:1-6` говорит: verdict GO, путь production; `:26` — «ring / registry offsets»;
  - `src/alloc_core/alloc_core/bootstrap.rs:47-49` — «remote ring» в составе первичной разметки (ingress — внешний sidecar, см. `segment_header_layout.rs:51-53`) и «const-assert in `segment_header.rs`»;
  - `src/alloc_core/segment/segment_header/layout_asserts.rs:54` — «M9 abandoned-stack layout»;
  - `src/global/tls_heap.rs:118,278` и `src/registry/heap_slot.rs:25` — ссылки на раздел модульной документации «TLS destructor ordering», которого нет; заголовок называется «TLS teardown and the TORN sentinel» (`tls_heap.rs:39`);
  - `tls_heap.rs:111` — «the three resolvers», а их четыре;
  - `tls_heap.rs:122` — «reverse-declaration teardown order» противоречит `:51-55` («do NOT rely on any … ordering»);
  - `src/global/fallback.rs:220-222` — «`u32::MAX` remains reserved for "not yet bound"», хотя сторож — `OWNER_ID_NONE = 0x7FFF_FFFF` (`segment_header_impl.rs:65-84`);
  - `src/registry/heap_registry/claim.rs:214` — «this function returns `null`», а возвращается `Option<HeapLease>`;
  - `src/alloc_core/alloc_core/alloc_core_core_diag/mod.rs:35` — жёстко прописанное «~106 `tests/` files» (против принятой в репозитории политики «никаких жёстко прописанных количеств»).
- Рекомендация: исправить в том же проходе, что и R6-CQ-12.

**R6-CQ-18 — Info — плотность комментариев**
- Суть: 23 911 строк комментариев против 17 971 строки кода (57% непустых строк) и 2159 меток задач/раундов (`task #…`, `R…-…`, `Ph…`, `RAD-…`, `UBFIX-…`). Публичный rustdoc содержит changelog:
  - `src/global/sefer_alloc/diag.rs:211-225` — «Fixed post-R31-10 (task #492): an earlier version…»;
  - `src/alloc_core/small/alloc_core_small/find_segment.rs:267-284` — «as this comment used to claim»;
  - `src/alloc_core/large/alloc_core_large_cache_eviction.rs:297-320` — 24 строки обоснования процедуры гейтинга у getter'а из 3 строк.

  Ещё 42 кириллические метки `Э2…Э11` и русские вставки в англоязычных комментариях (`src/alloc_core/alloc_core/alloc_core_impl.rs:444`, `src/alloc_core/alloc_core/lifecycle.rs:177`, `src/alloc_core/config/large_cache_config.rs:70-72`).
- Почему плохо: действующие инварианты тонут в истории. Именно так R6-CQ-12…17 и пережили несколько раундов.
- Рекомендация: в исходнике держать текущий контракт, порядок операций и SAFETY. Историю, отклонённые варианты и измерения — в ADR, `CHANGELOG.md` и perf-документы. Это продолжение рекомендации round5 №7.

### F. TODO/FIXME/заглушки

Находок нет: `TODO`, `FIXME`, `XXX`, `HACK`, `todo!`, `unimplemented!` в `src/` отсутствуют.

### G. Магические числа

**R6-CQ-19 — Low**
- Места:
  - литерал `64` как «бит в слове» и `MIN_BLOCK * 64` как «байт на слово sidecar» встречается 7 раз: `src/alloc_core/alloc_core/sidecar_drain.rs:114,221`, `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs:131`, `find_segment.rs:128,145`, `src/alloc_core/segment/segment_layout.rs:28,34`;
  - разложение `slot / 64`, `slot % 64` повторено около 15 раз: `src/alloc_core/segment/segment_directory/segment_directory_impl.rs:442-592`, `src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs:59-60,149-150`, `src/alloc_core/segment/segment_table/active_kind_index.rs:71-143` — при том что общий `SegmentBitmap` уже есть;
  - «сырые» индексы счётчиков: `SCAN_COUNTS[0..4]` (`src/alloc_core/segment/remote_bitmap/bitmap_scan.rs:30,59-62`), `CLAIM_SOURCES[0..4]` с семантикой «hint, bump, cap scan, recovery» плюс `[4]` в отдельной функции (`src/registry/heap_registry/claim.rs:55-67`, `counters.rs:465`), `FAIL_WORKER` = 1/2 (`src/global/maintenance_service.rs:139-141,233`);
  - `OWNER_ID_FALLBACK as usize >= 4096` вместо `MAX_HEAPS` (`segment_header_impl.rs:98-103`; зависимость по слоям понятна, но число повторено).
- Рекомендация: `const SIDECAR_WORD_BYTES: usize = MIN_BLOCK * u64::BITS as usize;`, общий `fn word_bit(idx) -> (usize, u64)`, `enum ClaimSource`/`#[repr(usize)]` для индексов, `enum WorkerFault { Return, Unwind }`. Для `MAX_HEAPS` — `const` в общем модуле нижнего слоя.

### H. Неудачные имена

**R6-CQ-20 — Low — коллизии имён констант состояний**
- Суть: `src/global/fallback.rs:62-64` определяет `STATE_UNINIT = 0`, `STATE_INITIALIZING = 1`, `STATE_READY = 2`. `src/registry/heap_slot.rs:89-99` определяет `STATE_EMPTY = 0`, `STATE_INITIALIZING = 2`, `STATE_OWNED = 1`, `STATE_FREE = 3`, `STATE_MAINTENANCE = 4` и `STATE_LIVE = STATE_OWNED` («Compatibility name»). Оба набора `pub` в поверхности `internals`; fallback-набор реэкспортирован (`src/global/mod.rs:67-70`).
- Почему плохо: одно имя означает разные числа в соседних модулях. У одного состояния два имени. Порядок значений не монотонен.
- Рекомендация: `#[repr(u8)] enum FallbackInitState` и `enum SlotState` с `TryFrom<u8>`, без алиаса `STATE_LIVE`.

**R6-CQ-21 — Info**
- Суть: повторы в именах модулей — `alloc_core::alloc_core::alloc_core_core_diag`, `small::alloc_core_small::alloc_core_small_impl`, `small::alloc_core_small_pool::alloc_core_small_pool_impl`. Тип индекса не согласован: `MaintenanceLease::slot_index() -> usize` (`claim.rs:470`), а `HeapLease::slot_index() -> u32` (`claim.rs:574`).
- Рекомендация: при следующем переносе — `alloc_core/core/diag/`, `small/path/`; привести `slot_index` к одному типу.

### I. Запутанные `#[cfg]`-матрицы

**R6-CQ-22 — Medium — предикат достижимости промоушена размножен вручную**
- Суть: выражение `all(feature = "medium-classes", any(not(feature = "exact-span-large"), all(feature = "large-reserved-capacity", not(feature = "numa-aware"))))`, иногда с дополнительными `alloc-global`/`fastbin`/`bench-internals`/`alloc-stats`, встречается 18 раз в 9 файлах:
  - `src/alloc_core/mod.rs:202-209`;
  - `src/alloc_core/segment/segment_header/segment_header_views.rs:49-57,65-73`;
  - `src/registry/heap_core/free/dealloc_own_base.rs:215-226,260-269,324-336` — в том числе ручные отрицания;
  - `src/registry/heap_core/free/dealloc.rs:69-81` — сам макрос;
  - `src/alloc_core/alloc_core/counters.rs`, `src/alloc_core/large/alloc_core_large.rs`, `src/alloc_core/platform/os.rs`, `src/alloc_core/alloc_core/alloc_core_impl.rs`, `src/registry/heap_core/free/realloc.rs`.

  Макрос `medium_promotion_reachable!` покрывает только элементы, а `dealloc_own_base.rs:212-214` фиксирует, что отрицание «must stay hand-written».
- Почему плохо: изменение условия требует 18 синхронных правок. Ошибка в одной копии не ломает сборку, а молча выключает код. Именно этот класс уже ловил clippy `--all-features` (см. `src/alloc_core/mod.rs:189-197`).
- Рекомендация:
  - Вариант (а): `build.rs` выдаёт `cargo:rustc-cfg=sefer_medium_promotion` и регистрирует его в `check-cfg`. Минус — у корневого крейта появляется build-скрипт.
  - Вариант (б): build-зависимость `cfg_aliases`. Минус — новая зависимость, нужно явное решение.
  - Вариант (в), минимальный: `const MEDIUM_PROMOTION_REACHABLE: bool = cfg!(…)` в одном месте для ветвлений внутри функций; `#[cfg]` оставить только на элементах через существующий макрос.

**R6-CQ-23 — Low — параметры под cfg и cfg-лестницы**
- Места:
  - `find_segment.rs:216` — параметр `rescue` под cfg;
  - `find_segment.rs:638-647` — у `finalize_hit` три параметра под cfg плюс `#[allow(unused_variables)]`;
  - `src/global/fallback.rs:157-159` — параметр `config` под cfg; `:223-234` — тройной предикат в прямой и отрицательной форме ради одного `bool`;
  - `src/global/sefer_alloc/diag.rs:155-162` — четыре взаимоисключающие cfg-ветки ради двух счётчиков.
- Рекомендация:
  - контекстная структура скана, у которой поле при выключенной фиче — ZST/`()`;
  - `fn dbg_forced_oom() -> bool` с cfg внутри;
  - в `registry::*_total()` при выключенной фиче возвращать 0, чтобы `stats()` обходился одной строкой.

**R6-CQ-24 — Low — экспериментальные cfg в горячем пути**
- Суть:
  - `cfg(r18_sidecar_scan_bench)` заменяет алгоритм `BitmapScan::next_cut` на prefilter `load`-before-`swap` (`src/alloc_core/segment/remote_bitmap/bitmap_scan.rs:43-56`) и добавляет TLS-счётчики. По `docs/perf/OPEN_ITEMS_ARCHIVE.md` R18 «did not promote a production change» и порог GO не регистрировался;
  - `cfg(r19_sidecar_summary_oracle)` добавляет 6 вставок в `drain_segment_sidecar` (`find_segment.rs:153-178`);
  - в `src/alloc_core/segment/remote_bitmap/prf01_sidecar_oracle.rs:33-34` `PENDING_WORDS_EXCHANGED.fetch_add(visited)` всегда равен `PENDING_WORDS_VISITED`, то есть это две метрики с одним значением.

  Всего 25 вхождений.
- Почему плохо: исследовательские варианты постоянно присутствуют в production-коде и в cfg-матрице. Это прямо противоречит правилу 3 из `CLAUDE.md` о пересмотре хуков после NO-GO.
- Рекомендация: принять решение по R18 — удалить альтернативную ветку или перенести её в bench-копию. Убрать или переименовать дублирующий счётчик. Если оракул R19 нужен, оформить его как ZST-наблюдатель с одной точкой вставки.

### J. Поверхность `dbg_*` и тестовых хуков

Статически `dbg_*`-хуки на публичных типах (`AllocCore`, `SeferAlloc`) гейтятся на уровне impl-блоков или модулей по `internals` / `internals + bench-internals`, кроме задокументированных исключений, которые читают `stats()` (`totals.rs`, `dbg_decommit_count`). Несоответствий с таблицами `PURE_OBSERVERS`/`SAFE_MUTATORS`/`UNSAFE_HOOKS` при чтении не найдено. Сам скрипт я не запускал.

**R6-CQ-25 — Low — `src/global/tls_heap.rs:95-101,651-655`**
- Суть: файл остаётся tier-1 seam с `#![allow(unsafe_code)]` целиком ради единственного `pub unsafe fn dbg_restore_local_for_test`, который есть только с `bench-internals`. В `production` это «unsafe seam» без единого `unsafe`; при этом он числится в инвентаре `src/lib.rs:225`.
- Рекомендация: `#![cfg_attr(feature = "bench-internals", allow(unsafe_code))]` или поэлементный tier-2 `#[allow(unsafe_code)]` на хуке (позитивный шаблон — `src/registry/heap_core/diag/diag_probes.rs`). Инвентарь обновить.

**R6-CQ-26 — Low — внутренние примитивы на публичном `AllocCore`**
- Суть:
  - `AllocCore::segment_bases` (`src/alloc_core/alloc_core/state.rs:184-188`) — `#[doc(hidden)] pub`, гейт только `any(alloc-global, alloc-xthread)`. Значит, метод доступен как `sefer_alloc::AllocCore::segment_bases` в обычном `production`. Документация сама признаёт, что понижение видимости «left as a separate … follow-up». Сканер `verify-dbg-hook-safety.mjs` инвентаризирует только `dbg_*`/`*_for_test` и такой метод не видит;
  - `dbg_foreign_or_unroutable_frees`, `dbg_segments_reserved_total`, `dbg_segments_released_total` (`src/alloc_core/alloc_core/alloc_core_core_diag/totals.rs:26-47`) и `dbg_decommit_count` (`src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:504`) — production-аксессоры стабильного `stats()` с префиксом `dbg_` и без гейта.
- Рекомендация: `segment_bases` → `pub(crate)`. Production-аксессоры → `pub(crate)` без префикса `dbg_`; при необходимости оставить `dbg_`-алиасы под `internals`.

**R6-CQ-27 — Low — `internals` меняет production-путь**
- Суть:
  - под `internals` рабочий поток обслуживания на каждом проходе берёт `CONTROL`-мьютекс и вызывает `notify_all` (`src/global/maintenance_service.rs:151-161`);
  - в инициализацию fallback-кучи под `GlobalAlloc` встроено чтение флага panic-инъекции (`src/global/fallback.rs:203-208`).

  Основная тестовая конфигурация `production internals` поэтому исполняет иную синхронизацию, чем поставляемый `production`.
- Рекомендация: вынести подтверждения в отдельный наблюдатель, не меняющий блокировок, или под `bench-internals`. Минимум — явно указать в `docs/CI_TIERS.md`, какая строка CI проверяет поведение без `internals`.

### K. Обработка ошибок (abort / null / panic)

**R6-CQ-28 — Medium — 108 голых `std::process::abort()`**
- Суть: на 108 вызовов `std::process::abort()` (больше всего — `src/alloc_core/segment/segment_table/route_slots.rs`: 19, `src/registry/segment_route/directory.rs`: 15, `src/registry/heap_registry/claim.rs`: 7, `segment_table_impl.rs`: 7, `sidecar_drain.rs`: 7) помощник `platform::fatal::fatal(msg)` (`src/alloc_core/platform/fatal.rs:4-8`) используется один раз (`alloc_core_small_impl.rs:403`), плюс есть его копия `exact_fatal` (R6-CQ-6). `src/global/sefer_alloc/mod.rs:185-186` прямо отказывается от ретрофита.
- Почему плохо: при нарушении инварианта в production нет ни одного сигнала о том, какой из 108 сайтов сработал. `fatal()` уже не аллоцирует и не раскручивает стек, то есть даёт те же гарантии.
- Рекомендация: механически заменить на `fatal(b"<модуль>: <инвариант>\n")`, начиная с `route_slots.rs`/`directory.rs`. Затем запретить голый `process::abort` вне `fatal.rs` lexical-проверкой в стиле существующих `verify-*.mjs`.

**R6-CQ-29 — Medium — паникующие конструкции на путях из `GlobalAlloc`**
- Места:
  - `src/alloc_core/large/alloc_core_large_cache.rs:146-148,162-164` (`expect`), `:170`, `:364` (`unreachable!`);
  - `src/alloc_core/segment/segment_header/terminal_words.rs:49` — `assert!` в `pack_large_state`, который вызывается с runtime-поколением из `src/alloc_core/large/reservation_state.rs:45-161`;
  - `src/alloc_core/small/alloc_core_small_magazine.rs:169` (`assert!`);
  - опциональные `expect` в `numa.rs`/`exact_shard.rs` (перечислены в `sefer_alloc/mod.rs:119-125`).
- Почему плохо: собственная документация (`src/global/sefer_alloc/mod.rs:98-101,133-153`) говорит, что раскрутка из `GlobalAlloc` запрещена, а паника не гарантирует abort и может повторно войти в аллокатор при упаковке payload. Политика для порчи состояния — abort (`:178-186`). Две категории реакции на один класс событий — лишний риск.
- Рекомендация: заменить на `unwrap_or_else(|| fatal(b"…"))` и `if !cond { fatal(…) }`. Перечень в `sefer_alloc/mod.rs:98-125` сократить до пустого. Lexical-guard перевести с «разрешённого списка паник» на «паник нет».

**R6-CQ-30 — Low — разная реакция на одно условие**
- Суть:
  - `find_segment.rs:491-498`: при линейном скане `base.is_null() || kind ∉ {Small, Primordial}` ведёт к `abort()`, а в numa-fallback той же функции (`:585-592`) то же условие даёт `return None`;
  - `sidecar_drain.rs:177,288` вызывают `abort()` на `SegmentKind::Unknown`, хотя контракт `kind_at` (`segment_header_views.rs:33-39`) обещает, что вызывающие отправят `Unknown` в ветку «не этот вид / no-op».
- Рекомендация: зафиксировать в комментарии, почему эти состояния различны (для fallback-кандидата recycle возможен, для активного слота — нет), либо унифицировать. Контракт `kind_at` привести к фактическому поведению.

### L. Публичный API и документация

**R6-CQ-31 — Low — согласованность публичного API**
- Суть:
  - документация `AllocStats::config_conflicts` (`src/global/alloc_stats.rs:160-173`) называет его «the field to monitor when running multiple `SeferAlloc` instances», но поле считает только конфликты реестра (`src/global/sefer_alloc/diag.rs:186-189`). Конфликты fallback-кучи доступны отдельным методом `SeferAlloc::fallback_config_conflicts(&self)` (`src/global/sefer_alloc/core.rs:368-374`), который глобален для процесса, но принимает `&self`;
  - `start_maintenance()`/`maintenance_running()` — ассоциированные функции (`src/global/sefer_alloc/maintenance.rs:36,43`), а столь же глобальные `stats()`/`trim_current_thread()` принимают `&self`. У `maintenance_running()` нет `#[must_use]`.
- Рекомендация: добавить в `AllocStats` (он `#[non_exhaustive]`) поле `fallback_config_conflicts` или суммировать оба счётчика и поправить документацию. Выбрать одну форму для глобальных операций.

**R6-CQ-32 — Low — нет `missing_docs`**
- Суть: в `[workspace.lints]` и `src/lib.rs` нет `missing_docs`. Публичная стабильная поверхность при чтении документирована, но это держится на дисциплине. В поверхности `internals` недокументированы:
  - `terminal_words.rs:32-85` — `MAX_LARGE_GENERATION`, `LargePhase` и его варианты, `pack_large_state`, `large_generation`, `next_large_generation*`;
  - `src/registry/segment_route/pin.rs:10-16`, `registration.rs:20-55`, `large_state.rs:29-51`, `directory.rs:708-715`;
  - `src/registry/bootstrap/free_slot_hint.rs:26-70`, `saturation.rs:20-36`.
- Рекомендация: `#![warn(missing_docs)]` в `src/lib.rs`, чтобы закрепить стабильную поверхность. Для doc-hidden модулей — `#[allow(missing_docs)]` на уровне модуля либо минимальные однострочные doc-комментарии к элементам, которые становятся `pub` под `internals`, после выполнения R6-CQ-2.

**R6-CQ-33 — Info — пространство идентификаторов**
- Суть: в комментариях `src/` 12 меток `R6-CQ-2/7a/7b/7c` (например, `src/alloc_core/alloc_core/alloc_core_core_diag/mod.rs:2`, `src/registry/heap_core/free/dealloc.rs:2`, `src/registry/heap_core/free/realloc.rs:2`, `segment_header_views.rs:2`) от июльской remediation. Они совпадают по форме с ID этого отчёта.
- Рекомендация: при ссылках на этот отчёт указывать путь `docs/agent_reviews_round6/code_quality_review.md` или дату 2026-10-15. Исторические метки в коде не переименовывать.

## Подтверждено как исправное

1. Inline-тестов `#[cfg(test)] mod tests` в `src/` нет.
2. Runnable doctest нет: все 20 открывающих doc-блоков кода имеют пометку ` ```text `.
3. TODO/FIXME/`todo!`/`unimplemented!` нет.
4. Все файлы `src/` ≤ 1000 строк. Каталоги сверх 7 записей ровно совпадают с baseline: `src/alloc_core/alloc_core` 10, `platform` 8, `segment_header` 9, `segment_table` 9, `src/global` 9, `src/registry/segment_route` 13.
5. Ни один из 33 `mod.rs` не определяет `fn/struct/enum/impl/const/static/trait`.
6. 22 tier-1 модуля с `#![allow(unsafe_code)]` совпадают со списком в шапке `src/lib.rs:190-287`.
7. Закрыты замечания round5 №1 (сырые `dealloc/realloc` стали `unsafe fn`), №2 (`dbg_stamp_*` стали `unsafe`), №3 (паникующий `debug_assert!` удалён, `claim.rs:186-196`), №4 в части `alloc-runfreelist` и №5 (future-only заготовки удалены). Устаревшие формулировки №6 остаются частично (R6-CQ-12, 13, 15, 17). Устаревшие `legacy`-типы `experimental` (`#[deprecated(since = "0.1.0")]` при версии крейта 0.3.0, `src/concurrent/sharded/sharded_region.rs:354-357`) по-прежнему без срока удаления — это хвост round5 №4, отдельно не нумеруется.
