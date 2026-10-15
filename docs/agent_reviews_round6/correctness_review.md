# Раунд 6 — ревью `src/`: логические ошибки и корректность поведения

Дата: 2026-10-15
Ревизия: `132ab85bd2fa443cfa5c6e0fbdea46231c3beaaf` (дерево чистое)
Тема: логическая корректность: размеры и выравнивания, realloc, alloc_zeroed и virgin, пул и decommit, large-cache, `SegmentTable`, учёт, статистика, OOM, конфиг, feature-комбинации, расхождения doc и кода. Unsafe-soundness и гонки в тему не входят.

## 1. Метод и ограничения

- Только статическое чтение `src/`. Сборок, тестов, бенчмарков, miri и проектных скриптов не было. Ни одна находка не подтверждена запуском.
- Целиком прочитаны пути, которые ведут от `GlobalAlloc` вниз:
  - `global/sefer_alloc/*`, `global/tls_heap.rs`, `global/fallback.rs`, `global/maintenance_service.rs`, `global/exact_object/{narrow,exact_shard}.rs`;
  - `registry/heap_core/**`, `registry/heap_core_xthread/*`, `registry/heap_registry/{claim,stack,maintenance,counters}.rs`, `registry/segment_route/{directory (выборочно),pin,registration,shard_lock,small_sidecar}.rs`;
  - `alloc_core/alloc_core/{alloc_core_impl,lifecycle,bootstrap,sidecar_drain}.rs`, `alloc_core/alloc_core/mem/*`;
  - `alloc_core/large/*`, `alloc_core/config/*`;
  - `alloc_core/small/alloc_core_small/*`, `alloc_core_small_magazine.rs`, `alloc_core_small_reclaim.rs`, `alloc_core_small_pool/{alloc_core_small_pool_impl,decommit}.rs`;
  - `alloc_core/segment/segment_table/{segment_table_impl,hash,active_kind_index,active_kind_ops,issue_transaction,route_slots}.rs`;
  - `alloc_core/segment/remote_bitmap/{sidecar_bitmap,leaf_classes,bitmap_cut,bitmap_scan}.rs`;
  - `alloc_core/segment/segment_header/{block_kind,segment_header_layout}.rs`, `segment_directory_impl.rs` (выборочно);
  - `alloc_core/platform/{size_classes,os,numa,process_exit}.rs`.
- Выборочно прочитан legacy-tier `concurrent/`: `lock_free_region.rs`.
- Перед ревью сверено с `docs/INVARIANTS.md`, `docs/CORRECTNESS_OPEN_ITEMS.md` (ACTIVE, `TRACKED_correctness_residuals.md`), `docs/perf/OPEN_ITEMS.md`, `docs/agent_reviews_round5/` и ревью раунда 17 по логике (`docs/reviews/2026-10-08-src-review-oxx-round-17-logic-invariants.md`). Уже известное повторно не заводилось. Где новая находка пересекается с открытым пунктом, это указано.
- Классы статусов:
  - **ПОДТВЕРЖДЕНО** — трасса по коду полная, вход и результат выведены из исходника;
  - **ГИПОТЕЗА** — механизм виден в коде, но достижимость зависит от условий, которые статически не доказаны.

## 2. Итог

Находок 12: Critical 0, High 0, Medium 1, Low 7, Info 4.

На горячем пути `production` (Small/Large alloc/dealloc/realloc, magazine, virgin-mask, flush, `SegmentTable`, large-cache, учёт `live_count`) ошибок, дающих неверный результат на корректном входе, не найдено.

Найденное делится на четыре группы:
1. Взаимодействие двух opt-in фич: R6-CR-1.
2. Пробелы в hardening при входе, нарушающем контракт: R6-CR-2, R6-CR-7.
3. Поведение на граничных и нештатных режимах: R6-CR-3, R6-CR-4, R6-CR-5, R6-CR-6, R6-CR-8, R6-CR-9.
4. Неточности документации и статистики: R6-CR-10, R6-CR-11, R6-CR-12.

Результаты раунда 17 перепроверены:
- R17-LOG-02 исправлен: счётчик попаданий в кэш теперь увеличивается после `register_payload` (`alloc_core_large.rs:456–517`).
- R17-LOG-03 исправлен: M2/M4/M6/M7 в `INVARIANTS.md` переписаны.
- Наблюдение раунда 17 про H-1 в `small_free_guard` закрыто проверкой `off < payload_start` (`dealloc_own_base.rs:294–301`).

## 3. Таблица находок

| ID | Серьёзность | Статус | Место | Суть |
|---|---|---|---|---|
| R6-CR-1 | Medium | ПОДТВЕРЖДЕНО | `src/global/sefer_alloc/batch.rs:115–136` | Под `exact-object-proto` + `batch-api` `dealloc_batch` обходит `ExactNarrow::try_dealloc`. Узкие объекты из `System` утекают, их дескрипторы остаются в `ExactTable` |
| R6-CR-2 | Low | ПОДТВЕРЖДЕНО (механизм); вход — UB вызывающего | `src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs:108–133`, `src/alloc_core/small/alloc_core_small_reclaim.rs:38–66` | Class-leaf присваивает внутренним гранулам чужой класс. Внешнее освобождение внутреннего указателя проходит геометрическую проверку владельца: блок молча линкуется во free-list, `live_count` занижается, abort не происходит. Уточняет item 166 |
| R6-CR-3 | Low | ПОДТВЕРЖДЕНО | `src/alloc_core/alloc_core/mem/realloc_fastpath.rs:243–284`, `src/registry/heap_core/free/realloc.rs:340–351` | Сжимающий `realloc` Large всегда аллоцирует и копирует. Под давлением памяти он возвращает null, хотя сжатие на месте тривиально. Механизм известен (perf item 83), аспект отказа — нет |
| R6-CR-4 | Low | ПОДТВЕРЖДЕНО | `src/alloc_core/large/alloc_core_large.rs:188–241`, `src/registry/heap_core/free/realloc.rs:297–317, 593` | Округление `usable` после `checked_add` не проверяется. Promotion вызывает `alloc_large` до валидации `Layout`. В debug возможна паника переполнения из `GlobalAlloc::realloc` |
| R6-CR-5 | Low | ПОДТВЕРЖДЕНО | `src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:422–484`, `src/alloc_core/small/alloc_core_small/reserve.rs:122–126` | Decay пула малых сегментов срабатывает только в `reserve_small_segment_impl`. На простаивающей, но занятой куче пул не сливается. Doc обещает «drain to zero when truly idle» |
| R6-CR-6 | Low | ПОДТВЕРЖДЕНО | `src/registry/heap_core/state/ownership.rs:152–153`, `src/alloc_core/small/alloc_core_small/directory.rs:59–69, 103–107` | Любой cold trim сбрасывает segment directory, а пересобирается он только при резерве нового малого сегмента. Нерастущая куча остаётся на линейном скане навсегда |
| R6-CR-7 | Low | ПОДТВЕРЖДЕНО (механизм); вход — порча заголовка | `src/registry/heap_core/free/dealloc_own_base.rs:198, 215–233, 292–312` | `small_free_guard` обрабатывает `BlockKind::Unknown` как Small и кладёт блок в magazine. Substrate (`dealloc_at_base`) для того же случая делает no-op |
| R6-CR-8 | Low | ГИПОТЕЗА (достижимость) | `src/global/sefer_alloc/global_alloc.rs:40–56, 143–183`, `src/registry/segment_route/shard_lock.rs:49–74` | Во время loader shutdown обход через `System` есть только у ветки `Fallback`. Own-куча при занятом шард-локе получает null. Блок, ушедший в `System`, позже нельзя `realloc` — null и abort |
| R6-CR-9 | Low | ПОДТВЕРЖДЕНО | `src/alloc_core/large/alloc_core_large.rs:562–574` против `:645–673` | Без `numa-aware` biased Large (`align >= SEGMENT`) не повторяет резерв после слива пула. С `numa-aware` повторяет. OOM-поведение зависит от фичи |
| R6-CR-10 | Info | ПОДТВЕРЖДЕНО | `src/global/alloc_stats.rs:137–151`, `src/alloc_core/large/alloc_core_large.rs:127–135` | `foreign_or_unroutable_frees` считает отказ по внутреннему указателю для собственного Large, но не считает отказы Small-оракулов. Семантика метрики непоследовательна |
| R6-CR-11 | Info | ПОДТВЕРЖДЕНО | `src/alloc_core/segment/segment_table/segment_table_impl.rs:67–78, 386–389`; `global_alloc.rs:27–32` | Более ~4095 одновременно живых сегментов на кучу дают null при свободной памяти. SAFETY-комментарий `GlobalAlloc` утверждает «only null is true OOM». Предел известен (perf item 4), комментарий неверен |
| R6-CR-12 | Info | ПОДТВЕРЖДЕНО | разные, см. §4 | Устаревшие doc-комментарии: TFS, ring, `HeapOverflow`, несуществующий `large_layout_consistent`, обоснование #138, противоречивая семантика headroom. Новые примеры к item 154 |

## 4. Подробности

### R6-CR-1 — Medium — `dealloc_batch` не видит exact-object-объекты

**Место:**
- `src/global/sefer_alloc/batch.rs:115–136` — `SeferAlloc::dealloc_batch`;
- для сравнения скалярный путь, `src/global/sefer_alloc/global_alloc.rs:64–69`;
- `src/global/exact_object/narrow.rs:38–57, 63–91`.

**Механизм.** Под `exact-object-proto` скалярный `GlobalAlloc::alloc`/`alloc_zeroed` обслуживает каждый «узкий» `Layout` (`size <= 16`, `align <= 8`) напрямую из `System` и регистрирует адрес в `ExactTable` (`narrow.rs:38–57`). Скалярный `dealloc` поэтому сначала вызывает `ExactNarrow::try_dealloc` (`global_alloc.rs:66–69`). `SeferAlloc::dealloc_batch` этого шага не делает. Он сразу идёт в `HeapCore::dealloc_batch` (ветка `Own`) или в `publish_foreign` (ветка `ForeignNoBind`).

**Вход:**
1. Сборка с `exact-object-proto` + `batch-api`.
2. `p = GLOBAL.alloc(Layout::from_size_align(8, 8))` — объект из `System`, дескриптор в `ExactTable`.
3. `GLOBAL.dealloc_batch(layout, &[p])`.

**Неверный результат.** `dealloc_batch_small` вызывает `canonical_block_of(p)` и получает `None`, потому что адрес не принадлежит сегменту кучи. Дальше путь такой: `HeapCore::dealloc`, `dealloc_routing`, `publish_foreign`, `RouteDirectory::lookup` — тоже `None`. В итоге:
- объект из `System` не освобождается (утечка);
- дескриптор в `ExactTable` живёт вечно, таблица растёт;
- `foreign_or_unroutable_frees` увеличивается, то есть корректная программа выглядит в метрике как «сбрасывающая free».

Симметричная мелочь: `alloc_batch` (`batch.rs:66–76`) для узкого `Layout` выдаёт блоки кучи, а не exact-объекты. Вреда в этом нет, потому что скалярный `dealloc` откатывается в кучу, но инвариант прототипа «узкий layout всегда exact» в batch-API не выполняется.

**Рекомендация:**
- под `cfg(feature = "exact-object-proto")` в `dealloc_batch` для каждого ненулевого блока сначала вызывать `ExactNarrow::try_dealloc(p, layout)`, кучу использовать только при `false`;
- либо запретить комбинацию фич через `compile_error!`;
- оракул: подпроцессный тест с этой комбинацией фич — `alloc` узкого `Layout`, затем `dealloc_batch`. Проверить, что `ExactNarrow::dbg_counts().1` вырос, а `foreign_or_unroutable_frees` не изменился.

### R6-CR-2 — Low — class-leaf ошибочно метит внутренние гранулы, освобождение внутреннего указателя молча портит free-list

**Место:**
- `src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs:108–113` — spill копирует `old` во все 256 гранул;
- `leaf_classes.rs:129–133` — uniform-лист метит весь лист классом первого выданного в нём блока;
- `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs:106–115` — `publish` отбрасывает только гранулы класса 0;
- `src/alloc_core/small/alloc_core_small_reclaim.rs:38–66` — геометрия владельца проверяется относительно класса из записи.

**Механизм.** Класс в листе 4 KiB хранится либо как одно значение на весь лист (uniform), либо, после spill, как байт на каждую гранулу. Значение для гранулы-начала блока при корректной выдаче всегда верное. Внутренние гранулы блока несут то, что лежит в их листе. Если блок B класса b заходит хвостом в соседний лист L1, а затем в L1 выдаётся первый блок класса 0 (16 B), `issue` записывает `uniform[L1] = 1`. После этого хвостовые гранулы B сообщают класс 0. Spill тоже копирует старое uniform-значение во все гранулы, включая внутренние и ещё не нарезанные.

**Вход** (нарушение контракта: free внутреннего указателя, с другого потока):
- B — блок 6144 B по смещению 61440. Он занимает лист 15 и половину листа 16.
- Следующий bump-carve выдаёт 16-байтный блок по смещению 67584 (лист 16).
- Поток-чужак вызывает `dealloc(B + 4096 + 16·k)`.

**Неверный результат.** Публикация проходит: класс 0 ≠ 0-маркера. Владелец в `reclaim_sidecar_record` проверяет кратность 16 (верно), `end <= bump` (верно), бит `is_free` для внутренней гранулы (0) и magazine-бит (0). Все проверки пройдены. Дальше:
- `write_next` пишет в середину живого B;
- гранула кладётся во free-list класса 0;
- выполняется `dec_live`.

Итог: следующая 16-байтная аллокация пересекается с живым B (нарушение M3). Заниженный `live_count` может довести сегмент до пула или освобождения при живом B. Item 166 описывает этот класс как «отвергается только владельцем через `abort()`»; для кросс-классовой разметки это неверно. Под `hardened` собственное освобождение внутреннего указателя отвергается H1 (`dealloc_own_base.rs:278–286`), а кросс-поточное — нет. Это асимметрия гарантий `hardened`.

**Рекомендация:**
- владельцу проверять, что запись указывает на начало выданного блока, а не только на кратность классу. Например, owner-only бит «issued-start», который ставится в `IssueTransaction::commit` и снимается при free/reclaim; запись без бита отбрасывать или abort;
- либо хранить класс только в грануле-начале даже для uniform-листа;
- обновить карточку item 166.

### R6-CR-3 — Low — сжатие Large в `realloc` может вернуть null

**Место:**
- `src/alloc_core/alloc_core/mem/realloc_fastpath.rs:243–284` — OPT-G работает только при `new_eff >= old_eff`, иначе `return None`;
- move-ветка: `src/registry/heap_core/free/realloc.rs:340–351` и `src/alloc_core/alloc_core/mem/mem_impl.rs:692–709`.

**Механизм.** Любое сжатие Large-блока идёт в move-ветку: новая аллокация, копирование `new_size` байт, освобождение старого блока.

**Вход.** `realloc(p /* 8 MiB */, 8 MiB, 7.9 MiB)` при исчерпанном commit-лимите. Новому блоку нужны те же 2 сегмента (8 MiB).

**Неверный результат.**
- Возвращается null, хотя оставить блок на месте можно всегда. Вызывающий (`Vec::shrink_to_fit`) уходит в `handle_alloc_error`.
- Без OOM: временно удваивается footprint, копируется до ~8 MiB, RSS не выигрывается (тот же `usable`).
- Doc OPT-G («shrink … reclaims RSS by moving») неверен, когда сжатие не уменьшает `usable`.

Механизм перемещения известен: perf item 83, NO-GO по Ir-контролю. Аспект «сжатие может упасть по OOM» в карточке не отмечен.

**Рекомендация:** минимум — при отказе аллокации в move-ветке сжатия вернуть исходный `ptr` и записать `large_size = new_eff`. Это корректно по контракту `GlobalAlloc`: блок остаётся достаточного размера. Сделать это решением владельца в item 83.

### R6-CR-4 — Low — непроверенное округление в `alloc_large` и promotion до валидации `Layout`

**Место:**
- `src/alloc_core/large/alloc_core_large.rs:188–196` — `checked_add` с комментарием «no longer RELIES on the caller's Layout being well-formed»;
- `:226–230` — `needed.div_ceil(SEGMENT) * SEGMENT` без проверки;
- `:241` — `align_up(needed, page_size())` без проверки;
- `src/registry/heap_core/free/realloc.rs:297–317` — promotion перед `Layout::from_size_align` на `:343`, вызов `alloc_large(new_size, …)` на `:593`.

**Вход.** Сборка с `medium-classes` (promotion) и debug overflow-checks. `GlobalAlloc::realloc(p, old /* ≥ 1 B small */, usize::MAX - 5000)` — нарушение контракта (`new_size` > `isize::MAX`). По порядку:
- OPT-F/OPT-G отказывают;
- promotion-условие выполняется, `try_promote_to_large` вызывает `alloc_large(usize::MAX - 5000, align)`;
- `needed = 4096 + size = usize::MAX - 904` проходит `checked_add`, но `div_ceil` даёт 2⁴², а `2⁴² · 2²²` переполняет `usize`.

**Неверный результат.**
- Debug: паника арифметического переполнения внутри `GlobalAlloc::realloc` — unwind из метода `GlobalAlloc`, который проект запрещает (R2-08).
- Release: `usable` заворачивается в 0, `Segment::reserve(0)` возвращает `None`, результат null. Безопасно, но случайно.
- Doc у `checked_add` обещает больше, чем делает код.

**Рекомендация:**
- `n_segments.checked_mul(SEGMENT)` и проверяемый `align_up`, при переполнении — null;
- в `HeapCore::realloc` валидировать `Layout::from_size_align(new_size, align)` до promotion;
- оракул: unit-тест `alloc_large(usize::MAX - 4096, 8)` под debug — null без паники.

### R6-CR-5 — Low — пул малых сегментов не «дренируется в ноль при простое»

**Место:**
- doc поля `last_pool_decay_tick`: `src/alloc_core/alloc_core/alloc_core_impl.rs:572–580` («makes retention TEMPORARY, not merely bounded»);
- doc `maybe_decay_small_pool`: `src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:422–441` («Repeated ticks drain the pool to zero when the workload goes quiet»);
- единственный вызов: `src/alloc_core/small/alloc_core_small/reserve.rs:126`.

**Механизм.** Decay выполняется только в начале `reserve_small_segment_impl`, то есть при промахе carve, когда нужен свежий сегмент. В пуле лежат сегменты со свободными блоками; `find_segment_with_free` находит их раньше резерва. Чем лучше пул работает, тем реже вызывается decay. Задача обслуживания (`registry/heap_registry/maintenance.rs:29`) берёт только FREE-слоты. Занятая, но простаивающая куча не тримится, пока поток сам не вызовет `trim_current_thread` или не завершится.

**Вход.** Поток делает всплеск малых аллокаций, освобождает всё и простаивает часами. Поток жив, его слот LIVE.

**Неверный результат.** До `pool_cap` сегментов (по умолчанию 16 MiB, `SmallPoolPolicy::Throughput` — 32 MiB) остаются закоммиченными бессрочно. Это противоречит doc о «временном» удержании. Для large-cache аналогичное ограничение честно описано в doc `SeferAlloc::new` («Idle time alone does not reclaim any of it»); для пула — нет.

**Рекомендация:** исправить doc (удержание ограничено, но не временно для владельца без новых промахов) либо добавить тик пула в существующие холодные события (Large alloc/free рядом с `maybe_decay_large_cache`, промах magazine).

### R6-CR-6 — Low — segment directory теряется после cold trim и не восстанавливается без роста кучи

**Место:**
- `src/registry/heap_core/state/ownership.rs:152–153` — каждый `trim_cold_retention` вызывает `release_directory_for_cold_trim`;
- `src/alloc_core/small/alloc_core_small/directory.rs:103–107` — указатель обнуляется;
- `directory.rs:59–69` — материализация;
- `src/alloc_core/small/alloc_core_small/reserve.rs:443–444` — единственный вызов `maybe_materialize_directory`.

**Механизм.** Cold trim выполняется при `trim_current_thread`, при каждом повторном claim слота (`claim.rs:378`), при каждом проходе воркера по FREE-куче и при выходе потока. Каждый раз он сбрасывает directory. Doc обещает, что «a later small-segment registration lazily rebuilds». Это верно только если куча зарезервирует ещё один малый сегмент.

**Вход.** Куча с ≥ 32 малыми сегментами (порог материализации) в установившемся режиме, без роста. Сценарии: поток периодически вызывает `trim_current_thread`; или слот переходит к новому потоку.

**Неверный результат.** После первого trim каждый промах free-list проходит линейный скан `find_segment_with_free_impl` (`find_segment.rs:452–575`) по всем Small-сегментам со сливом sidecar каждого (до 4096 слов на сегмент), вместо попадания по биту directory. Корректность не страдает; это бессрочная деградация производительности, не описанная в doc. Измерения нет.

**Рекомендация:** пересобирать directory лениво при промахе, если `table.count() >= DIRECTORY_MATERIALIZE_THRESHOLD`; либо не сбрасывать его на повторном claim. Судья: perf-гейт с оракулом `DIRECTORY_HITS` / `FULL_SCAN_SLOTS_EXAMINED` до и после trim.

### R6-CR-7 — Low — `small_free_guard` не отвергает `BlockKind::Unknown`

**Место:**
- `src/registry/heap_core/free/dealloc_own_base.rs:198` — `BlockKind::of`;
- `:215–233` — production pre-route проверяет только `Large`;
- `:292–312` — M2-оракулы и `Accept`;
- для сравнения `src/alloc_core/alloc_core/mem/mem_impl.rs:536, 539–551`: `Unknown` там — no-op (L-5/UBFIX-11).

**Механизм.** Если байт `kind` в заголовке зарегистрированного сегмента не является одним из трёх допустимых значений, magazine-путь всё равно читает bitmap и `bump` как у Small-сегмента, кладёт блок в magazine и потом выдаёт его. Substrate-путь в той же ситуации ничего не трогает.

**Вход.** Порча байта `kind`. Это нарушение инварианта, а не контракта вызывающего.

**Неверный результат.** Политика «нельзя доверять сегменту с неизвестным kind» в двух фасадах исполняется по-разному.

**Рекомендация:** в `small_free_guard` возвращать `RejectNoOp` для `BlockKind::Unknown` до M2-оракулов. Это одно сравнение на холодной ветке `match`.

### R6-CR-8 — Low (ГИПОТЕЗА о достижимости) — асимметрия поведения при завершении процесса

**Место:**
- `src/global/sefer_alloc/global_alloc.rs:40–56` (`alloc`) и `:143–183` (`realloc`): обход через `System` только в ветке `CurrentHeap::Fallback`;
- `src/registry/segment_route/directory.rs:836–843` (`register`) и `:902–908` (`lookup`);
- `src/registry/segment_route/shard_lock.rs:49–74` — сдаётся, только если лок занят и идёт выход;
- `src/registry/heap_core/free/realloc.rs:418–421`.

**Механизм.**
1. Own-куча, которой при Windows loader shutdown нужен новый сегмент, вызывает `RouteDirectory::register`. Если шард-лок держит завершённый поток, регистрация получает `ProcessExiting`, `register_payload` — `None`, аллокация — null. `alloc` возвращает null без запасного пути через `System`, вызывающий уходит в `handle_alloc_error`.
2. Блок, выданный из `System` в exit-ветке `Fallback`, при последующем `realloc` (лок fallback уже свободен) проходит через `HeapCore::realloc`, внешнюю ветку и `lookup`, получает `None` и null.

**Вход.** Освобождение памяти или рост `Vec` в TLS-деструкторе или DLL detach после того, как поток, державший шард-лок, был завершён загрузчиком.

**Неверный результат.** Abort при завершении процесса там, где для `Fallback` есть мягкий путь. Достижимость зависит от того, что лок держит завершённый поток. Статически не доказано.

**Рекомендация:**
- распространить обход через `System` на null-результат Own-ветки при `process_exit_in_progress()`;
- в `realloc`, если маршрут не найден и идёт выход, использовать `System.alloc` + копирование `old_layout.size()` по тому же правилу, что и fallback-ветка.

### R6-CR-9 — Low — biased Large без повтора после слива пула (не-NUMA)

**Место:** `src/alloc_core/large/alloc_core_large.rs`:
- `:562–574` — NUMA-ветка повторяет и `reserve_biased_on_node`, и `reserve_aligned_on_node`;
- `:645–663` — не-NUMA повторяет только при `align < SEGMENT`;
- `:664–673` — biased-резерв без повтора.

**Вход.** Сборка `production` (без `numa-aware`), `alloc(Layout(1 MiB, 8 MiB))` при отказе ОС в резерве и непустом пуле малых сегментов.

**Неверный результат.** Возвращается null, хотя слив пула (soft reserve, Mechanism 2) мог дать память. Под `numa-aware` тот же запрос получает повторную попытку.

**Рекомендация:** выполнить `drain_small_pool` и повторить `reserve_biased`, как в NUMA-ветке.

### R6-CR-10 — Info — непоследовательная семантика `foreign_or_unroutable_frees`

**Место:**
- `src/global/alloc_stats.rs:137–151` — по doc это free, сброшенные из-за отсутствующего маршрута или отказа публикации;
- `src/alloc_core/large/alloc_core_large.rs:127–135` — `reject_large_interior_free` увеличивает тот же счётчик для собственного Large;
- `src/registry/heap_core/free/dealloc_own_base.rs:299–310` — Small `RejectNoOp` (метаданные, повторное освобождение, magazine) не считается.

**Результат.** Метрика смешивает внешние отказы маршрута с одним видом локального отказа и пропускает остальные локальные. «0 означает здоровую программу» неверно в обе стороны: локальный double free Small не виден, а локальное освобождение внутреннего указателя Large видно.

**Рекомендация:** выделить отдельный счётчик для локальных отказов либо уточнить doc поля.

### R6-CR-11 — Info — предел `MAX_SEGMENTS` и неверный SAFETY-комментарий «only null is true OOM»

**Место:**
- `src/alloc_core/segment/segment_table/segment_table_impl.rs:67–78, 386–389`;
- `src/alloc_core/large/alloc_core_large.rs:689–704`;
- `src/global/sefer_alloc/global_alloc.rs:27–32`.

**Вход.** Один поток держит 5000 одновременно живых Large-блоков. Под `exact-span-large` блоки по ~300 KiB — это ~1,5 GiB, реалистично.

**Результат.** После ~4094 живых сегментов `register_payload` возвращает `None`, `alloc` — null при свободной памяти. Предел известен (perf item 4, R14-7, триггер «> `MAX_SEGMENTS`−1 живых Large»). Но SAFETY-комментарий impl `GlobalAlloc` («M10 … is upheld: the only null return is true OOM») неверен. Null дают также предел таблицы, отказ регистрации маршрута и OOM `System` при spill class-leaf.

**Рекомендация:** привести комментарий в соответствие с M4 в `INVARIANTS.md` («capacity, or reservation failure may return null»).

### R6-CR-12 — Info — устаревшая и противоречивая документация (новые примеры к item 154)

- `src/registry/heap_core/state/tcache.rs:23` — «Cross-thread frees … go to the per-segment ring»: ring удалён.
- `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:157–161` — «Cross-thread frees arrive via the inline TFS and are drained by `HeapCore::alloc` BEFORE this runs». На деле discovery через sidecar происходит на промахе внутри `find_segment_with_free`.
- `src/registry/heap_core/free/dealloc.rs:186–190` — «route cross-thread via the TFS»; `src/registry/heap_core/alloc/hot.rs:24` и `:576–578` — TFS и `HeapOverflow`.
- `src/registry/heap_core/free/dealloc_own_base.rs:126–127` — «`large_layout_consistent`, the same primitive `heap_core_xthread::dealloc_routing` uses». Такой функции в `src/` нет, `dealloc_routing` подобных проверок не делает.
- `src/alloc_core/alloc_core/mem/realloc_fastpath.rs:156–162` — clamp до `MIN_BLOCK` обоснован «#138 cross-thread consistency check … silently drop the free». Этой проверки больше нет: внешний путь публикует через маршрут.
- `src/registry/heap_core/core.rs:362–363` — «Read by `recycle`»: `recycle` удалён (item 165).
- `src/alloc_core/config/large_cache_config.rs:46–48` («The cache does not decay below this level») и `:187–188` («the decay step does **not** release bytes below this level») противоречат setter-doc `:293–300` («NOT a hard floor … can overshoot») и коду `evict_at_least` (`alloc_core_large_cache_eviction.rs:34–56`, вытесняет целые span'ы).

**Рекомендация:** свести к одной формулировке; внести в документационный проход item 154.

## 5. Проверено — находок нет

- **Классы размеров и границы.** `size = 0` во всех точках входа поднимается до `MIN_BLOCK` (`hot.rs:137–139`, `mem_impl.rs:64–66`, `realloc_fastpath.rs:244–247, 287–291`). Переход Small/Large на `SMALL_MAX`, slow path по выравниванию и `align > SMALL_MAX` ведут в Large. Блоки выровнены: база кратна `SEGMENT`, `align_up(bump, block_size)` в координатах сегмента.
- **Magazine и virgin-mask.** Инвариант «биты `>= count` равны 0» поддерживается во всех местах мутации:
  - hit в `alloc` (`hot.rs:300`), zeroed-hit (`:423–425`), refill virgin (`:516–524`);
  - push и overflow со сдвигом `>>= FLUSH_N` (`dealloc_own_base.rs:433, 517, 537`);
  - batch-путь (`batch.rs:122`, `dealloc_batch.rs:354`), `flush_all_tcache` (`tcache_flush.rs:113`).
  - Бит virgin ставится только для блоков `carve_batch` на сегменте с `payload_virgin` (`alloc_core_small_magazine.rs:278–317`). Блок из free-list никогда не считается virgin. Retain-decommit сбрасывает `payload_virgin`.
- **Ёмкость magazine.** `refill_n_for_class` и `FREE_PARK_CAP` симметричны, `count` не превышает `cap`, ветка `cap < TCACHE_CAP` уходит в одноблочный flush.
- **`flush_class_internal`.** Группировка по сегментам и L-4 `recycled_bases`; в одну серию повторно не заходит.
- **Учёт `live_count`.** `add_live`/`sub_live` с аварийной остановкой при переполнении и опустошении. Magazine-блоки считаются живыми; сегмент с такими блоками и с неразобранными публикациями не может опустеть.
- **`SegmentTable`.** Идентичность `key | (id+1)`. Условие backward-shift, `hash_find` с устаревшей идентичностью даёт `None`. Слот 0 не recycle-ится (`ActiveKindIndex::clear` прерывает выполнение). Free-list индексов без дублей. Предел `MAX_SEGMENTS` соблюдается во всех ветках `register_payload`. Корень в own-cache сбрасывается вместе с `hash_remove`.
- **Large.** Геометрия `hdr_aligned`/`usable`; biased-резерв (`root_offset < align`, `root_offset + useful <= raw_len`). Попадание в кэш перерегистрирует сегмент, пишет заголовок до регистрации, увеличивает счётчик после регистрации. Откаты `begin_reuse`/`register_payload`. Бюджет и цикл вытеснения сходятся. `large_cache_occupied` поддерживается в `slot_set`/`slot_take`. Biased-span не кэшируется.
- **`alloc_zeroed` для Large.** Обнуление пропускается только для свежего `alloc_large_slow` (`cfg!(not(miri))`); попадание в кэш всегда обнуляется.
- **realloc.** OPT-F только при совпадении класса, OPT-G только при росте от канонического начала payload. Move-ветки копируют `min(old, new)` и ограничивают чтение `safe_payload_read_span`. Promotion освобождает старый блок через собственный путь.
- **Конфиг.** Разрешение `LargeCacheConfig`/`SmallSegmentPoolConfig`/`Profile` совпадает с doc. `live_config_matches` сравнивает разрешённые значения, конфликты считаются без паники (R2-08). Fallback-куча инициализируется конфигом запрашивающего экземпляра.
- **Claim и слоты.** Повторная попытка при потерянном CAS, `push_back_after_oom`, восстановление при OOM chunk'а через `scan_claim_recovery_free`, `HeapLease`/`MaintenanceLease` Drop с аварийной остановкой при проигранном CAS.
- **Legacy `LockFreeRegion`.** Насыщение поколения описано в doc и реализовано одинаково (`lock_free_region.rs:475–487`).

## 6. Связь с индексами открытых пунктов

- Уточняют существующие: R6-CR-2 → item 166 (TRACKED_correctness_residuals); R6-CR-3 → perf item 83; R6-CR-11 → perf item 4; R6-CR-12 → item 154 (TRACKED_misc).
- Новые: R6-CR-1, R6-CR-4, R6-CR-5, R6-CR-6, R6-CR-7, R6-CR-8, R6-CR-9, R6-CR-10.

Индексы этим отчётом не правились: по условию задачи изменяется только этот файл.
