# Ревью исправлений раунда 6 — баги, внесённые самими исправлениями

Дата: 2026-10-16. Ревизия: `ac23d06e` (src не менялся до `20ce975b`). Диапазон: `132ab85b..HEAD`, merge-коммиты PR #34–#63.
Режим: только чтение. Ничего не собиралось и не запускалось. Каждая находка выведена из чтения кода по `git show -m --first-parent <merge>` и текущему HEAD.

## 1. Резюме

Нашёл 2 дефекта уровня P2 и 9 уровня P3. Critical (UB на корректном входе) нет.

- **P2-1 (R6-T29, PR #39).** Сжатие Large на месте при OOM создаёт блок, который лежит в Large-сегменте, но имеет small-layout. В сборках `hardened` без promotion (например, `production hardened`) ветка F7 (B) превращает его законный free в счётный no-op. В результате утекает вся Large-резервация и слот `SegmentTable`.
- **P2-2 (R6-T10(b), PR #49).** Перед Large alloc полный sweep заменили пробой из 4 слотов, а полный sweep при неудаче не восстановили. Когда `alloc_large` возвращает null (таблица заполнена или ОС отказала), очередь remote-freed Large не разбирается. На пределе 4095 Large это `handle_alloc_error` → abort там, где старый код освобождал слоты. Помимо этого, промах кэша резервирует свежий сегмент у ОС, хотя pending-сегмент того же размера можно было переиспользовать.
- P3: неизмеренный рост sidecar-памяти после T28; ложное утверждение T50 о жадном коммите при `primordial-lazy-commit` (входит в `production`); обход defence-in-depth в exit-escape `realloc` (T04); потерянное пробуждение в saturation-gate (T07); тесты, которые не покрывают заявленное поведение (T29, T03, loom-модель T53); неточный текст `fatal` (T06); глобальные инъекционные хуки под `internals` на production-пути; гипотеза о задержке до 8192 `yield` в T24.

## 2. Находки

### P2-1 — T29: после OOM-shrink законный free превращается в no-op и утечку в `hardened` без promotion

**Подтверждено чтением кода.** Уверенность высокая.

- Место: `src/registry/heap_core/free/realloc.rs:345-366` (ветка R6-CR-3), `src/registry/heap_core/free/dealloc_own_base.rs:254-268` (F7 branch (B)).
- Механизм:
  1. `HeapCore::realloc` (own-ветка), Large-блок, `new_size < old_size`. In-place shrink отклонён (`realloc_large_inplace_known_base` берёт только `new_eff >= old_eff`), управление уходит в move leg.
  2. Move leg вызывает `self.alloc(new_layout)` и получает null. Если `new_size <= SMALL_MAX`, это Small-аллокация, и null бывает при отказе резервирования Small-сегмента, заполненной таблице (`MAX_SEGMENTS`) или OOM спилла sidecar.
  3. T29: `SegmentHeader::set_large_size_at(base, new_eff); return ptr;`. Блок остаётся в Large-сегменте, но вызывающий теперь законно держит `Layout(new_size)`, а этот layout классифицируется как Small.
  4. Затем идёт `dealloc(ptr, Layout(new_size))`, путь `dealloc_own_thread_with_base`: `class_for` даёт `Some(c)`, вызывается `small_free_guard`, `BlockKind::of` даёт `Large`.
  5. При `hardened` и `not(medium_promotion_reachable)` срабатывает ветка (B): `if matches!(kind, BlockKind::Large) { HARDENED_LARGE_NOOP_COUNT += 1; return RejectNoOp; }`. Её документированная посылка звучит так: «when promotion is compiled OUT, a legitimate Large-with-small-layout is structurally unreachable» (`dealloc_own_base.rs:141-146`). T29 эту посылку нарушил.
  6. Тот же путь проходит последующий `realloc` такого блока: move leg освобождает старый блок через `dealloc_own_thread_with_base(owned, old_layout=small)`, получает no-op, а `realloc` при этом возвращает новый указатель. Через `dealloc_batch_small` тот же guard даёт тот же no-op.
- Минимальный сценарий: фичи `production hardened` (в `production` нет `medium-classes`, поэтому promotion недостижим). Шаги: `p = alloc(Layout(300 KiB))` (Large, так как больше `SMALL_MAX` ≈ 253 KiB) → `realloc(p, 300 KiB, 200 KiB)` при недоступном Small-сегменте (например, все 4095 слотов таблицы заняты Large, R6-T08) → вернулся `p` → `dealloc(p, Layout(200 KiB))`.
- Ожидается: сегмент освобождён или ушёл в large-cache. Фактически: no-op, утекают 4 MiB резервации и слот таблицы, растёт `HARDENED_LARGE_NOOP_COUNT`. До T29 `realloc` возвращал null, и старый блок оставался с Large-layout, который освобождается корректно.
- Затронутые сборки: любой `hardened` с недостижимым promotion. Это `hardened` без `medium-classes`, а также `medium-classes + exact-span-large` без `large-reserved-capacity` или с `numa-aware`. В ветке (A) (promotion достижим) проверка `large_size_at == layout.size().max(MIN_BLOCK)` проходит, там всё корректно. Без `hardened` блок ловит Ph3b pre-route, там тоже корректно.
- Тест не ловит: `tests/r6_t29_large_shrink_move_oom_keeps_ptr.rs` сжимает 8 MiB до 4 MiB (новый layout тоже Large) и собирается без `hardened`.
- Исправление, на выбор:
  - (а) в T29 оставлять блок на месте только при `SizeClasses::class_for(new_eff, align).is_none()`, иначе возвращать null, как раньше;
  - (б) в ветке (B) принимать Large-with-small-layout при совпадении `large_size_at/large_align_at` с layout, как в ветке (A). Для этого придётся снять promotion-гейт с `large_size_at`/`large_align_at`.
  - Тест: `hardened` без `medium-classes`, shrink Large до small-размера под `dbg_set_inject_large_alloc_oom` (или инъекцией OOM Small), затем `dealloc` с новым layout; проверить, что `dbg_segments_released_total` или large-cache вырос, а `HARDENED_LARGE_NOOP_COUNT` не изменился.

### P2-2 — T10(b): Large alloc больше не разбирает pending remote-free даже перед отказом

**Подтверждено чтением кода.** Уверенность высокая в механизме и средняя в частоте на практике.

- Место:
  - `src/registry/heap_core/alloc/hot.rs:186-191` → `drain_large_before_alloc` (`:368-373`, только `drain_large_sidecar_ingress_hot_bounded`);
  - `:359` (`alloc_large_without_magazine` → `AllocCore::alloc` → `alloc_large`);
  - `alloc_zeroed` Large: `:614-624`;
  - `src/alloc_core/large/alloc_core_large.rs:674-686` (`register_payload` вернул None → null, без rescue).
- Механизм: до PR #49 каждый Large alloc вызывал полный `drain_large_sidecar_ingress()`. Он ретайрил все Large, опубликованные чужими потоками (unregister слота + депозит в large-cache), до поиска в кэше и до OS-резерва. Теперь пробуются не больше 4 активных Large-слотов от курсора. План в `SUMMARY.md` (R6-T10(б)) требовал оставить полный sweep «на промахе кэша перед OS-резервом (с повторным поиском в кэше)». Gate-отчёт (`docs/perf/R6_T10_LARGE_SWEEP_PROBE_GATE.md`, абзац 2) фиксирует, что этот шаг удалили, потому что он ломал тесты bounded-probe. Замены на пути отказа нет: rescue (`refill_with_large_rescue`) есть только у Small refill. Полный sweep у живого владельца остаётся только в `realloc` на Large-размер и в trim/maintenance; последние для LIVE-слота не работают.
- Сценарий A (abort): фичи `production`. Поток A держит около 4090 живых Large и ещё 5 Large, освобождённых потоком B (pending, слоты таблицы заняты). Таблица полна. Следующий `alloc(Layout(1 MiB+))` в A пробует 4 слота от курсора. Если все 4 живые, ничего не ретайрится. Кэш промахивается, `register_payload` возвращает None, `alloc_large` → null, `GlobalAlloc::alloc` → null, `handle_alloc_error` → abort. Старый код освободил бы 5 слотов, и аллокация прошла бы.
- Сценарий B (RSS): у A K ≥ 5 живых Large других размеров. B освобождает Large X размера S. Если X не попал в окно из 4 слотов, `alloc(S)` в A промахивается мимо кэша и резервирует новый сегмент у ОС. X вернётся в кэш позже. Получаем лишнюю OS-резервацию и рост RSS на S до ретайра X. Ось цены RSS, которую требовал план («удержание RSS при cross-thread Large free в том же режиме»), гейт не измерял.
- Исправление: на null из `alloc_large` (в `alloc_large_without_magazine` и в Large-ветке `alloc_zeroed`) выполнять `drain_large_sidecar_ingress_rescue()` и повторять попытку один раз. Это холодный путь, контракт bounded-probe на успешных аллокациях не нарушается. Тест: модель полной таблицы, как в `dbg_large_rescue_refill_model`, для Large; плюс сценарий B с oracle «свежих резерваций = 0 при pending того же размера».

### P3-1 — T28: каждый 4 KiB-лист класса теперь требует System-спилл 256 B; рост памяти не измерен

**Механизм подтверждён, величина выведена расчётом, не измерена.**

- Место: `src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs:61-112` (`prepared`/`prepare`).
- До PR #42 `prepared()` возвращал true для uniform-листа с `old == 0 || old == class+1`, поэтому одноклассовый лист не аллоцировал ничего. Теперь `prepared` истинен, только если `mixed[leaf]` ненулевой, и первая выдача в каждом листе делает `System.alloc(256)`.
- Следствия:
  - на полностью нарезанный Small-сегмент приходится до 1024 × 256 B = 256 KiB System-метаданных плюс накладные malloc (около 6–7 % от 4 MiB) против примерно 40 KiB раньше;
  - на carve-пути появился новый источник отказа: OOM спилла даёт null, хотя в сегменте есть место.
- Гейт `R6_T28_START_GRANULE_CLASS_LEAVES_GATE.md` прямо пишет: «Sidecar memory is not measured». Это противоречит правилу «цена и выгода в одном режиме».
- Исправление: RSS/commit-ячейка (N Small-сегментов одного класса, RSS до и после). Если цена неприемлема, вернуть дешёвый uniform-режим для листов, где все выданные гранулы стартовые (например, хранить в uniform-листе пару «класс + фаза/шаг»), или документировать цену.

### P3-2 — T50: утверждение «Small/Primordial коммитятся целиком» ложно в `production`

**Подтверждено.** Уверенность высокая.

- Место: `src/registry/heap_core/free/realloc.rs:473-496`.
- Foreign-ветка `realloc` для Small/Primordial проверяет только `ptr >= root + *_meta_end()` с обоснованием «Small and Primordial segments commit the whole span eagerly». Но `production` включает `primordial-lazy-commit` (`Cargo.toml:423`), а `small-segment-lazy-commit` существует как фича. Own-ветка учитывает это через `small_committed_bound` (`realloc_fastpath.rs:107-126`, `committed_payload_end_of`).
- Сценарий (нарушение контракта, defence-in-depth): фичи `production`. Поток B делает `realloc` блока из primordial-сегмента потока A с подделанным `old_layout.size()`, который выходит за commit-frontier, но не за `entry.end`. `contains_payload` и проверка T50 проходят, `copy_nonoverlapping` читает некоммитнутые страницы и получает access violation.
- Исправление: либо атомарно публиковать frontier для чужих читателей, либо честно сузить текст и комментарий до eager-конфигураций. Тест T50 primordial-lazy-commit не покрывает.

### P3-3 — T04: exit-escape `realloc` обходит ограничения чтения, которые возвращают null

**Подтверждено.** Сценарий требует нарушения контракта.

- Место: `src/global/sefer_alloc/global_alloc.rs:40-59`, `:202-206`.
- Любой null от `HeapCore::realloc` во время выхода превращается в `System.alloc` плюс `copy_nonoverlapping(ptr, new, min(old,new))`. Но null возвращают и намеренные отказы defence-in-depth: `old_layout.size() > safe_payload_read_span` (`realloc.rs:337`), `!contains_payload` (`:441`), frontier T50 (`:497`), а также отсутствие маршрута для чужого указателя (`:437`). При `process_exit_in_progress()` то же нарушение контракта приводит уже к неограниченному чтению.
- Исправление: делать escape только на «маршрут недоступен из-за мёртвого лока» (сигнал из `lookup`/`with_fallback_heap`), а не на любой null; либо ограничивать копию теми же проверками.

### P3-4 — T07: версия saturation снимается после неудачного claim, что даёт потерянное пробуждение

**Подтверждено.** Влияние ограничено.

- Место: `src/global/tls_heap.rs:163-171` (`record_saturation_verdict`), вызов из `finish_bind(None)`.
- `snapshot()` читается после того, как `claim_lease` вернул None. Если слот освободился (`publish_claimable` → версия +2) между отрицательным сканом и снимком, кэш запомнит уже новую версию. Тогда поток до 64 аллокаций идёт на глобальный fallback-лок, хотя свободная куча есть. Та же блокировка на 64 аллокации срабатывает и после constructor-OOM (T03): `push_back_after_oom` поднимает версию до снимка.
- Исправление: брать снимок версии до `claim_lease` и передавать его в `record_saturation_verdict`.

### P3-5 — Тесты не покрывают заявленное поведение

**Подтверждено.**

- `tests/r6_t29_large_shrink_move_oom_keeps_ptr.rs`: новый layout 4 MiB тоже Large, `hardened` нет, поэтому опасный случай из P2-1 не исполняется.
- `tests/registry_claim_ctor_oom_recovery.rs`: проверяет только hint/bitmap и успех следующего claim. Не проверяет пункт (b) R6-T03: «после OOM конструктора текущий claim находит готовую инициализированную FREE-кучу». В тесте такой кучи нет (slot 0 LIVE).
- `tests/loom_r6_selector_model.rs:232-235`: hint-ветка модели делает `let _ = self.take(candidate); return true;`, то есть возвращает «claimed», даже если CAS не прошёл. Production при неудачном CAS делает `continue`. Сейчас liveness-ассерты эту ветку не задевают (hint = `NO_HINT`), но любое новое liveness-утверждение с ненулевым hint окажется вакуумным. Исправление: `return self.take(candidate)` или продолжение отбора.

### P3-6 — T06: текст `fatal` не соответствует условию

**Подтверждено.**

- Место: `src/alloc_core/config/continuation_bounds.rs:23-27`.
- Условие `bump < meta_end` означает порчу `bump` в заголовке сегмента, а сообщение говорит `node: free-list continuation out of range or misaligned` и ссылается на модуль `node`. По диагностике невозможно отличить порчу free-list от порчи заголовка.
- Исправление: `b"continuation_bounds: segment bump below payload start\n"`.

### P3-7 — Инъекционные хуки T03/T29/T34 под `internals` на production-путях

**Подтверждено.**

- Места:
  - `src/alloc_core/large/alloc_core_large.rs:198-203` (загрузка `DBG_INJECT_LARGE_ALLOC_OOM` на каждом `alloc_large`), `:618-629`;
  - `src/registry/heap_registry/claim.rs:392-399`.
- Это safe `pub fn`-мутаторы с процессно-глобальным состоянием, загейченные на `internals`, а не на `bench-internals` (правило 2 о `dbg_*`-хуках в CLAUDE.md: по умолчанию `bench-internals`). `production internals` — строка `npm run check`/CI, поэтому тестируемый бинарь отличается от production лишней атомарной загрузкой на Large-пути (тема R6-T47). Флаг процессно-глобальный: если когда-нибудь в одном тестовом бинаре окажутся два теста, один из которых включает OOM, параллельный тест получит ложные отказы.
- Исправление: перевести гейт на `bench-internals` или убедиться, что все три хука перечислены в `scripts/verify-dbg-hook-safety.mjs` с обоснованием (не проверял).

### P3-8 — T24: до 8192 `yield_now` внутри первой аллокации потока (гипотеза)

**Гипотеза, не подтверждена прогоном.**

- Место: `src/registry/heap_registry/claim.rs:371-387`, `MAINTENANCE_RECLAIM_BUDGET = 8192` (`:42`).
- Claimant, попавший на `MAINTENANCE`, публикует кандидата обратно и уступает CPU до 8192 раз. Всё это происходит внутри `GlobalAlloc::alloc` нового потока. Если проход maintenance долгий (trim с munmap), получается скачок латентности. Если владелец `MaintenanceLease` сам аллоцирует на непривязанной TLS (тесты с `dbg_try_maintenance` на том же потоке; worker без своей кучи), он ждёт сам себя все 8192 итерации, после чего всё равно делает bump.
- Исправление: ограничить ожидание по времени или числу короче; либо пропускать кандидата в MAINTENANCE в пользу scan.

### P3-9 — Латентное: `medium_promotion_unreachable!` гейтит только первый элемент

**Наблюдение по коду, сейчас не срабатывает.**

- Место: `src/alloc_core/config/medium_promotion.rs:182-194`.
- Макрос вешает `#[cfg(not(pred))]` перед `$($rest)*`. Если в вызове окажутся два и более item/statement, `cfg` получит только первый, остальные скомпилируются безусловно. Позитивные arms `const/fn/use` с `$($rest:tt)*` устроены так же. Все текущие вызовы содержат ровно один элемент (проверил 18 мест), но документация макроса этого ограничения не называет.
- Исправление: отдельный arm на один item или явная оговорка в doc.

## 3. Что проверено и признано корректным

- **T09 (batch → ExactNarrow):** narrow-развилка в `alloc_batch` и `dealloc_batch`; незарегистрированный блок идёт в heap-routed путь поблочно.
- **T04/T23/T25:**
  - escape в `alloc`/`alloc_zeroed` срабатывает только на холодном null;
  - `acquire_unless_exiting` обрабатывает spurious-fail;
  - `Retired` — не больше двух вытесненных блоков (разобраны все ветки `Shard::remove`), освобождаются после `drop(guard)`;
  - `rehash` возвращает старый массив и освобождает его после `release()`;
  - незарегистрированный System-блок на выходе при free становится безопасным no-op (за маршрутом не найден).
- **T11:** ключ `usize::MAX` не выровнен по SEGMENT, все производители ключа маскируют адрес; `own_cache_clear` по корню корректен и для biased Large.
- **T22:** граница `off < span_usable` (Large) / `SEGMENT` (Small/Primordial) стоит до `Node::deref`; ключ для Small совпадает с корнем.
- **T26:** TTAS-циклы shard/fallback-локов: spurious CAS, отсутствие ложного захвата, проба выхода раз в 64 yield-раунда; ограниченность при мёртвом держателе сохранена.
- **T30:** checked-округления в обеих ветках; валидация `Layout` до `try_promote_to_large`.
- **T34:** повтор biased-резерва после `drain_small_pool`.
- **T31:** вызов `maybe_decay_small_pool` с Large-путей. Проверил отсутствие UAF: все циклы слива перечитывают `base_at(index)`, держимые базы содержат живой блок и в пул не попадают, `retire_large_route` не держит указатель на пуловый сегмент.
- **T32:** ленивый `maybe_materialize_directory` в `linear_scan`; при уже материализованной директории это no-op.
- **T18:** все три пути слива (discovery, bounded, strict) после выделения `consume_small_cuts`/`finish_small_drain`/`retire_large_route` эквивалентны старым копиям по порядку, условиям sync/decommit и счётчикам.
- **T19:** вынос `scan_directory_candidates`/`linear_scan`/`dealloc_large_at_base`/`large_payload_offset` — чистый перенос; `return None` авторитетного промаха остался в вызывающей функции.
- **T05/T06/T33:**
  - reciprocal-делимость (обратный элемент через Ньютона, 6 итераций = 64 бита; `rotr ≤ ⌊2^64−1⌋/d`);
  - пробный проход free-list валидирует каждое звено, которое затем коммитится (`probed ≥ k`), поэтому обход без проверки в commit-цикле корректен;
  - `ContinuationBounds` = `[meta_end, min(bump, SEGMENT))`;
  - OPT-F допускает только тот же класс, поэтому divisibility-проверка не даёт ложных отказов;
  - `Unknown` → `RejectNoOp`.
- **T28:** коды классов не очищаются, но для живого маршрута это безопасно: пуловый сегмент не перенарезается, а release/recycle даёт свежий sidecar.
- **T01:** `owner_state` больше нигде не читается.
- **T03:** `push_back_after_oom` публикует слот в bitmap; повтор ограничен одной попыткой.
- **T13/PF-7/PF-10:** маска — тождество при `cnt < cap ≤ TCACHE_CAP = 16`; `FREE_PARK_CAP[c]` совпадает с `refill_n_for_class`; `dealloc_at_base(block, layout, root)` эквивалентен прежнему `dealloc`.
- **T35:** `wait_timeout_while` отпускает мьютекс; `timed_out()` истинен только при всё ещё ложном условии.
- **T37:** `unsafe` в `tls_heap.rs` осталось только в tier-2 хуке.
- **T38:** gates `foreign_or_unroutable_frees`/`segments_*` согласованы с `stats()`.
- **T40:** `initialize_at` пишет все поля `#[repr(C)] SmallSidecar` (pending 32 KiB + 1024 указателя, без padding), поэтому `System.alloc` вместо `alloc_zeroed` корректен.
- **T46–T49:** `cache_hit_totals` даёт те же пары, что прежние четыре ветки cfg.
- **T14/T15:** выборочно сверил тексты `fatal` с условиями. Расхождение одно (P3-6).

## 4. Ограничения

- Только статическое чтение. Ничего не собиралось и не запускалось, ни одна находка не воспроизведена. Компилируемость всех комбинаций фич не проверял (опирался на зелёный CI, указанный в PR).
- Комментарии T17 (−558 строк) и docs-PR (#62, T21, T36) читал только на предмет изменённых строк кода; в src их нет.
- Loom-модели (`loom_r6_selector_model.rs`, `loom_route_pin_unlink.rs`) читал частично. Большинство новых тестов оценивал по заголовку и ключевым ассертам, без полного анализа counterfactual.
- `scripts/verify-dbg-hook-safety.mjs` не открывал: не проверял, внесены ли туда хуки из P3-7.
- Частота P2-2 и P3-1 на реальных нагрузках выведена рассуждением, не измерена.
