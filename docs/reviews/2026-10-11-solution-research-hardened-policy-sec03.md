# Исследование решений: политика `hardened` при порче freelist (A) и own-thread Large free без сверки `payload_offset` (B, R17-SEC-03)

Базовый SHA: `31db7f17698459e9ce26c3f8701c314304b9c00b` (проверено `git rev-parse HEAD`).
Режим: только чтение; ничего не компилировалось и не запускалось.

## §0 Чек-лист хода

- [x] Шаг 0: база проверена (HEAD = 31db7f17)
- [x] Чтение CLAUDE.md / OPEN_ITEMS (164, 166 — TRACKED_correctness_residuals; 174/178/180 — RESOLVED)
- [x] Чтение R17 security (SEC-01/02/03)
- [x] (A) код `valid_free_node`/`free_continuation`, вызывающие, HeapCore-пути (заметки ниже)
- [x] (A) abort-сайты и контракт `src/global/sefer_alloc/mod.rs`
- [x] (A) README про hardened
- [x] (B) все сайты Large free (own/foreign/OPT-G)
- [x] (B) контракт `dealloc`, items 164/166, `small_free_guard`
- [x] §1–§7 оформлены

**Итог в одной строке.** (A) `hardened`: обнаруженная порча free-list → `std::process::abort()` (fail-closed), `Err`/null остаётся только для настоящего spill-OOM. (B) own-thread Large free и оба own-thread realloc move-leg сверяют адрес с `base + payload_offset` ДО `claim_live`, всегда (не только `hardened`); несовпадение → no-op + `FOREIGN_OR_UNROUTABLE_FREES` (free) / null (realloc). Общий принцип, который это фиксирует: **плохой вход, обнаруженный до мутации, — отклоняется (no-op/null + счётчик); испорченное состояние аллокатора — abort.**

## §1 Постановка (базовый SHA `31db7f17`)

### (A) Порча free-list под `hardened`

- `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:372-395` — `valid_free_node` (только `hardened`): узел должен лежать в `[payload_start, SEGMENT)`, быть кратным `block_size`, `end <= bump`, `is_free` в alloc-bitmap, не быть в магазине.
- `:397-413` — `free_continuation`: `next` → смещение через `Node::continuation_offset` (`src/alloc_core/platform/node.rs:64-70`), запрет self-link, затем `valid_free_node`.
- `:415-466` — `pop_free`: голова невалидна → `Err(())` (`:424-427`); продолжение невалидно → `Err(())` (`:433-434`); **и** `prepare_small_issue` → `None` (spill-OOM) → тот же `Err(())` (`:441-443`).
- `:517-525` — `drain_freelist_batch` = `try_drain_freelist_batch(...).unwrap_or(0)`; `:527-651` — `try_drain_freelist_batch`: hardened-проверки `:552-592` (включая Floyd-детектор цикла `:579-591`), spill-OOM `:564-570`; все → `Err(())`.
- Поведение до `54fc2a21`: при `next` вне сегмента цепочка **усекалась** (`next = null`) — см. `git show 54fc2a21 -- src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs`, удалённые строки с комментарием «mimalloc `MI_SECURE`-style … TRUNCATES the chain».
- Тест, фиксирующий текущий выбор: `tests/regression_freelist_next_validation.rs` (гейт `:48` — `hardened` + `internals`): `ac.alloc(layout).is_null()` (`:89-92`, `:223-226`), `drained == 0` (`:154`, `:216`), затем «ремонт» хуком и продолжение работы (`:97-116`, `:235-247`).
- README `:1437`: `hardened` — «Opt-in own-thread interior-pointer guard, rejecting non-block starts … Extra misuse detection». Про поведение при порче free-list README молчит.

### (B) R17-SEC-03

- `src/alloc_core/alloc_core/mem/mem_impl.rs:255-542` — `dealloc_at_base`; Large-ветка `:262-490`: `claim_live` `:269-271` (неудача → `abort()`), `read_at` `:293`, депозит в large-cache `:387-455` (`unregister`, `magic = 0`, `cache_consumed`), релиз `:471-475` / `:484-488`. Сверки `ptr` с `base + stale.payload_offset` нет.
- Контракт: `mem_impl.rs:203-209` — «MUST NOT be an interior pointer … the foreign/interior defences are best-effort … an interior Large free would release the whole reservation of a still-live neighbour». То же для `realloc`: `:620-622`.
- Сиблинги со сверкой: `src/registry/segment_route/directory.rs:876-881`; `src/alloc_core/alloc_core/mem/realloc_fastpath.rs:249-252`.

## §2 Анализ

### (A) — трасса «что происходит после обнаружения»

НАБЛЮДЕНО (по коду):
1. `Err(())` из `pop_free`/`try_drain_freelist_batch` несёт два разных смысла: «spill-OOM, состояние не тронуто, можно повторить» и «free-list испорчен». Вызывающие различить их не могут.
2. `alloc_small` (`alloc_core_small_impl.rs:163-262`) на `Err` с `small_cur` возвращает `null` **сразу** (`:169`), не пробуя другие сегменты, bump-carve или резерв. Так же в ветке `find_segment_with_free` (`:186`), после резерва (`:222`) и в rescue-скане (`:254`). `alloc_small_with_virgin` — зеркально (`:299`, `:307`, `:331`, `:352`).
3. `refill_class_bump_impl` (`src/alloc_core/small/alloc_core_small_magazine.rs:180-360`) на `Err` drain'а → `return filled` (`:235`, `:251`, `:347`; `:284`, `:307` — это `Err` carve-spill, к порче не относятся). Если порча на первом же шаге — `0`.
4. HeapCore: `refill_magazine_slow` (`src/registry/heap_core/alloc/hot.rs:675-698`) → `refill_with_large_rescue` (`:702-712`): один повтор после Large-drain, затем `0` → `null` (`:691-693`). Тот же исход у `alloc_zeroed` через `refill_class_bump_virgin_internal` (`hot.rs:500-507`).
5. Выбор сегмента детерминирован: `small_cur` первым, затем `find_segment_with_free_impl` (`src/alloc_core/small/alloc_core_small/find_segment.rs:188-614`) возвращает младший установленный бит директории (`:369-392`) или младший активный слот (`:465-560`). Испорченный сегмент будет выбираться снова и снова.
6. Свежий free в этот класс/сегмент не «лечит»: push делает новую голову, чей `next` указывает на испорченный узел; при следующем pop проверка продолжения (`:433-434`) отвергает уже эту, валидную, голову. Для batch-drain валидный префикс выдаётся, пока испорченный узел не окажется в пределах `out.len()` от головы (`:555-602`), после чего — снова `Err`.

ВЫВЕДЕНО:
- Класс в данном heap «залипает» в постоянный OOM: каждое обращение к substrate за этим классом → `null`, пока процесс жив. Магазин маскирует проблему, пока не опустеет. Восстановления нет.
- Под `#[global_allocator]` `null` для инфаллибельных std-API (`Box`, `Vec::push`, `String`) превращается в `handle_alloc_error` → по умолчанию abort с диагнозом «memory allocation of N bytes failed» (цепочку null → `handle_alloc_error` README фиксирует сам: `README.md:1667-1670`). То есть **для типичного потребителя текущий режим уже заканчивается abort'ом, только отложенным и с ложным диагнозом (OOM вместо порчи)**. Для fallible-API (`try_reserve`, `Box::try_new`) и для standalone `AllocCore` процесс продолжает работать с вечным OOM класса и с испорченной памятью — это и есть fail-open.
- Порча `next` требует записи в освобождённый блок (UAF) или переполнения — по определению уже состоявшегося UB/порчи памяти процесса. Owner-only free-list не имеет легальной гонки, которая могла бы дать невалидный узел; поэтому разделение «безопасно продолжить / нельзя» не имеет основания: **любое** срабатывание `valid_free_node` означает испорченное состояние. (ГИПОТЕЗА о нулевом false-positive у валидной программы — опровергается, если какой-либо валидный путь кладёт в free-list узел с `is_free == false`, в магазине, за `bump` или некратный `block_size`; по прочитанному коду таких путей нет: `dealloc_small`/`flush_run`/`reclaim_sidecar_record` (`src/alloc_core/small/alloc_core_small_reclaim.rs:46-65`) все ставят `mark_free` перед/вместе с линковкой, flush снимает магазинный бит до `flush_class` (`dealloc_own_base.rs:486-498`). Косвенное свидетельство: CI-job `test (hardened tier)` зелёный на `31db7f17` (run `38103112281`), а любой false-positive при текущем null-режиме уже ронял бы инфаллибельные аллокации в тестах.)
- Соседние нарушения инвариантов метаданных крейт уже обрабатывает `abort()`: `find_segment.rs:151`, `:482`; `alloc_core_small_reclaim.rs:25`, `:29`, `:44`, `:62`; `src/alloc_core/segment/segment_table/segment_table_impl.rs:301-305`, `:324-328`; Large-переходы `mem_impl.rs:270`, `:437`, `:473`, `:486`. Всего `grep -rn 'process::abort()' src | wc -l` = 110 (28 файлов). Контракт `src/global/sefer_alloc/mod.rs:165-173` прямо относит «Small sidecar/reclaim class/kind/offset geometry» к категории invariant aborts. Free-list — ровно та же категория (owner-метаданные малых сегментов), и только она одна отвечает null'ом.
- Внешний ориентир (из общих знаний, не проверено): glibc при провале проверок tcache/safe-linking вызывает `malloc_printerr` → abort; scudo при несовпадении checksum заголовка чанка — fatal report → abort; hardened_malloc — `fatal_error` → abort; mimalloc при `MI_SECURE` на «corrupted free list entry» сообщает `EFAULT`, и обработчик ошибок по умолчанию в secure-режиме делает abort. Ни один из известных мне hardening-аллокаторов не превращает обнаруженную порчу метаданных в OOM.

### (B) — трасса

НАБЛЮДЕНО (по коду):
1. `canonical_block_of` ключуется маской сегмента (`mem_impl.rs:34-42`), поэтому `ptr = payload + k` резолвится в тот же `base`, и `block.addr() == ptr.addr()` (`:40`).
2. Все own-thread Large free сходятся в `dealloc_at_base` (заметки (B) выше). Для Large-раскладки HeapCore идёт мимо `small_free_guard` (`dealloc_own_base.rs:372` → `None` → `:550-559`).
3. `claim_live` (`:269`) переводит LIVE → claimed до любой другой проверки; несовпадение, обнаруженное **после** него, оставило бы сегмент в claimed-состоянии, и валидный free того же блока позже упал бы в `abort()` (`:270`).
4. Foreign-путь для того же входа (interior Large от другого потока): `lookup` → `None` → `FOREIGN_OR_UNROUTABLE_FREES += 1`, free отброшен (`routing.rs:26-30`). Foreign realloc: `lookup` → `None` → `null` (`realloc.rs:414-417`).
5. Own-thread realloc: OPT-G-отказ (`realloc_fastpath.rs:249-252`) → move-leg → `dealloc_at_base(interior)` → весь Large освобождён/закэширован, возвращён `new_ptr`. Для shrink OPT-G вообще не применяется (`:248`), сразу move-leg. (Расширение SEC-03: R17 упомянул move-leg `mem_impl.rs:693`, но не то, что OPT-G-проверка его не перекрывает, и не HeapCore-move-leg `realloc.rs:403-406`.)
6. Последствие interior free под `production` (`alloc-decommit`): сегмент депонируется в large-cache (`:387-455`) без unmap; следующий `alloc_large` подходящего размера получает тот же адрес → две «живые» аллокации на одном адресе. Без `alloc-decommit` — `os::release_segment` (`:488`) → UAF на unmapped.

ВЫВЕДЕНО:
- Для валидной программы `ptr == base + payload_offset` всегда: `payload_offset` пишется только при выдаче (`segment_header_impl.rs:672`, перезапись при cache-hit — `alloc_core_large.rs:397`), OPT-G его не меняет, а `RouteDirectory` строит `entry.payload` из того же поля (`segment_table_impl.rs:512`) и уже отвергает несовпадение для каждого валидного cross-thread Large free. False-positive по построению исключён тем же аргументом, на котором стоит foreign-путь.
- Цена (оценка по коду, не измерение): одна 8-байтная загрузка поля заголовка (та же кэш-линия, что `kind`/`large_state`, уже затронутая), сложение и сравнение с предсказуемым ветвлением — на пути, который и так делает атомарный CAS (`claim_live`), а дальше либо скан/депозит large-cache, либо системный вызов release. На realloc move-leg — плюс `kind_at` (1 байт) перед `alloc`+`memcpy`. Горячий small-путь не затрагивается.
- Связь с 164: нет (Large-путь не пишет интрузивные ссылки). Связь с 166: **прямой нет** — 166 про foreign Small interior в выданный блок однородного листа; foreign Large interior уже отбрасывается на `lookup`. Утверждение R17-отчёта (`2026-10-08-src-review-oxx-round-17-security.md:189`) о том, что SEC-03 «даёт 166 счёт+отбрасывание на Large-ветке», считаю неточным; связь лишь прецедентная (тот же счётчик и тот же принцип «вход → no-op + счёт» — это вариант (2) из Next trigger карточки 166). Синтез P4 (`2026-10-08-src-review-xxs-round-17-P4.md:249`, `:365`) и синтез (`…-synthesis.md:97`) это разделение уже фиксируют.
- Связь с `small_free_guard` (после `36ba3758`/item 180): там тот же принцип — безусловные границы `off < payload_start` и `off >= bump` → `RejectNoOp` до мутации (`dealloc_own_base.rs:285-301`); R18 сделал их безусловными именно потому, что они дешёвые. SEC-03 — Large-аналог той же границы, и по той же логике должен быть безусловным.

## §3 Варианты

### (A) Политика при обнаруженной порче free-list (`hardened`)

| # | Механизм | Аргумент корректности | Цена hot / cold | Риск |
|---|---|---|---|---|
| A0 | Оставить как есть (`Err` → null, «залипший» класс) + задокументировать | Не мутирует состояние до отказа | hot 0 / cold 0 | Fail-open для fallible-API и standalone `AllocCore`; для std-коллекций — отложенный abort через `handle_alloc_error` с ложным диагнозом OOM; `Err` смешан со spill-OOM; класс в heap мёртв навсегда |
| A1 | **`std::process::abort()` при любом провале `valid_free_node`/`free_continuation`/детектора цикла; `Err` — только spill-OOM** | Любое срабатывание = испорченное owner-состояние (легальной гонки нет); совпадает с категорией invariant aborts (`sefer_alloc/mod.rs:165-173`) и с соседним `reclaim_sidecar_record` | hot 0 (hardened-проверки на refill/pop, не на magazine hit; и так `hardened`-only) / cold: замена `return Err` на вызов `abort` — без новых проверок | Убивает процесс при false-positive — но при текущем null-режиме false-positive уже фатален для инфаллибельных аллокаций; abort не unwind'ит и не зовёт panic hook (`mod.rs:172-173`), т.е. no-unwind-контракт `GlobalAlloc` соблюдён |
| A2 | null + счётчик (`AllocStats`/`dbg_*`) | Как A0, плюс наблюдаемость | hot 0 / cold 1 relaxed add | Все минусы A0; новая публичная поверхность счётчика ради fail-open режима |
| A3 | Карантин: обнулить голову класса в испорченном сегменте (`set_head(NULL)`, `publish_empty`), счётчик, продолжить поиск в других сегментах/bump | Блоки за головой утекают, но адресно не выдаются; decommit-reset позже всё обнулит | hot 0 / cold: +сброс головы, +директория, +повтор поиска | Fail-open: атакующий с UAF-примитивом получает неограниченное число попыток; утечка; новая логика восстановления на редко исполняемом пути (сложно протестировать все ветки: directory, rescue, virgin, batch) |
| A4 | Возврат к усечению цепочки (до `54fc2a21`) + счётчик | Как MI_SECURE-«truncate» | hot 0 / cold ~0 | Fail-open; усечение принимает испорченную *голову* (до `54fc2a21` проверялось только «тот же сегмент», не free/magazine/bump); утечка хвоста; откатывает закрытый item 178 |
| A5 | Различать «безопасно продолжить» (напр. `next` вне сегмента → усечь) и «нельзя» (узел занят/в магазине/цикл → abort) | — | как A1/A4 | Основания для разделения нет: все классы провала требуют записи в освобождённую память; разделение добавляет режим без критерия приёмки |

### (B) Interior-указатель в own-thread Large free / realloc

| # | Механизм | Аргумент корректности | Цена hot / cold | Риск |
|---|---|---|---|---|
| B0 | Оставить как есть + документировать (уже задокументировано `mem_impl.rs:203-209`) | Контракт `unsafe fn` запрещает interior | 0 / 0 | Асимметрия с foreign/OPT-G; interior free → aliasing через large-cache (production) или UAF (без decommit) |
| B1 | Проверка только в `dealloc_at_base` (как предлагал R17) | Закрывает free | hot 0 / cold 1 сравнение | Если поставить после `claim_live` — ломает последующий валидный free (abort); realloc move-leg превращается в «успешный realloc + утечка старого Large» |
| B2 | **Проверка в `dealloc_at_base` ДО `claim_live` + пред-проверка в обоих own-thread realloc move-leg; всегда (не только `hardened`); free → no-op + `FOREIGN_OR_UNROUTABLE_FREES`; realloc → null, `ptr` нетронут** | Для валидной программы равенство всегда истинно (тот же инвариант, что у `RouteDirectory`); отказ до любой мутации; поведение own == foreign для одного и того же входа | hot 0 / cold: 1 загрузка + 1 сравнение на Large free; +1 байт `kind_at` + сравнение на realloc move-leg | Изменение видимо только при нарушении контракта; счётчик теперь считает и own-thread случай — нужна правка doc счётчика |
| B3 | Как B2, но только под `hardened` | Как B2 | 0 в production | Асимметрия остаётся в production без ценового обоснования (цена ~0 на холодном пути); противоречит R18-решению сделать дешёвые границы безусловными (items 180/181) |
| B4 | Как B2, но при несовпадении `abort()` | Fail-closed на плохой вход | как B2 | Ломает принцип крейта «вход до мутации → no-op» (H1/F7-B `RejectNoOp`, foreign drop, `SegmentKind::Unknown` no-op `mem_impl.rs:528-540`); own и foreign снова расходятся. Внешние hardened-аллокаторы обычно abort'ят на invalid free (из общих знаний, не проверено) — но это решение уровня всего профиля, не одного сайта (см. §6) |

## §4 Рекомендация

### (A) → A1: abort

- **Почему.** (1) Обнаруженная порча free-list — испорченное owner-состояние, а не плохой вход; легальной гонки, дающей невалидный узел, нет. (2) Крейт уже abort'ит на все родственные нарушения геометрии/вида малых сегментов, а контракт `GlobalAlloc` в `sefer_alloc/mod.rs:165-173` определяет это как штатную категорию «invariant aborts», не нарушающую no-unwind. (3) Для главного потребителя (`#[global_allocator]` + std) A0 уже даёт abort, только позже и с ложным «OOM» — A1 делает отказ немедленным, детерминированным и честным. (4) `hardened` — единственный deploy-hardening профиль; fail-open (A0/A2/A3/A4) противоречит его смыслу. (5) Убирает смешение `Err` = порча / `Err` = spill-OOM: `Err` снова значит только «повторяемый OOM, ничего не тронуто».
- **Почему не остальные.** A0/A2 — fail-open и вечный OOM класса. A3 — fail-open + новая логика восстановления на редком пути; A4 — откат закрытого 178 и приём испорченной головы; A5 — нет критерия различения.
- **Что недоказано.** Отсутствие false-positive `valid_free_node` на всех валидных траекториях (есть только чтение кода + зелёный hardened-tier CI). Точный код завершения abort-подпроцесса на каждой ОС (план тестов ниже не зависит от конкретного кода).
- **Новая находка (серьёзность P4):** текущий `hardened`-режим превращает обнаруженную порчу в постоянный и неотличимый от OOM отказ класса; README не описывает поведение. Не memory-safety: порча уже произошла до детекции.

### (B) → B2

- **Почему.** Цена ~0 на холодном пути; инвариант для валидной программы тот же, на котором уже стоит foreign-путь; отказ до мутации; одинаковый наблюдаемый исход для own и foreign; закрывает и realloc-вариант, который R17 не учёл.
- **Поведение при несовпадении — no-op + счётчик (не abort)**, потому что это обнаруженный на входе плохой вход до любой мутации — та же категория, что `RejectNoOp` в `small_free_guard` и drop в `publish_foreign`. Для realloc — `null` с нетронутым `ptr` (как foreign realloc `realloc.rs:414-417` и `safe_payload_read_span`-отказ).
- **Всегда, не только `hardened`** — по прецеденту items 180/181 (дешёвые границы сделаны безусловными) и потому, что сравнение уже есть безусловно в двух сиблингах.
- **Контракт `dealloc` не меняется:** interior остаётся UB контракта вызывающего; меняется только формулировка best-effort-абзаца (`mem_impl.rs:203-209`): own-thread interior Large теперь отклоняется, а не «would release the whole reservation».
- **Что недоказано.** Что во всех feature-комбинациях (`large-reserved-capacity`, `numa-aware` biased-резерв при `align >= SEGMENT`, `exact-span-large`) `payload_offset` в заголовке совпадает с адресом, выданным вызывающему. Аргумент — общий с `RouteDirectory`, но фичи `numa-aware`/biased я по коду не трассировал.

## §5 План реализации

### Коммит 1 — (A)

Префикс R30-12: `fix(perf): abort on hardened free-list corruption` (прецедент `54fc2a21` и `36ba3758` — тот же класс правок назван `fix(perf)`; в теле: «no speed claim; behavior change only after detected allocator-state corruption under opt-in `hardened`»). Если владелец сочтёт, что `fix(perf)` подразумевает perf-чувствительность, допустим `fix(alloc)` — вопрос в §6.

Файлы/функции:
- `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs`:
  - `valid_free_node` оставить предикатом; добавить приватный `#[cfg(feature = "hardened")] #[cold] #[inline(never)] fn free_list_corrupted() -> !` с `std::process::abort()` (приватная функция, не экспорт — правило «один файл — один экспорт» не затронуто).
  - `free_continuation` → возвращает `u32`; вместо `Err(())`/`ok_or(())?` вызывает `Self::free_list_corrupted()`.
  - `pop_free` `:424-427`: `if !valid { Self::free_list_corrupted() }`; `:434` без `?`. `Err(())` остаётся только на `:441-443`; doc `:415` → «`Err` is an uncommitted class-spill OOM; detected corruption aborts under `hardened`».
  - `try_drain_freelist_batch` `:556-592`: все hardened `return Err(())` и `.ok_or(())?` → `free_list_corrupted()`; spill-OOM `:564-570` остаётся `Err(())`.

  Эскиз:
  ```text
  #[cfg(feature = "hardened")]
  #[cold]
  #[inline(never)]
  fn free_list_corrupted() -> ! {
      // Allocator-owned free-list state is corrupt (UAF/overflow); fail closed.
      std::process::abort()
  }
  ...
  #[cfg(feature = "hardened")]
  if !Self::valid_free_node(segment, head_off, class_idx) {
      Self::free_list_corrupted();
  }
  ```
- `src/global/sefer_alloc/mod.rs:165-171`: в перечень «Invariant aborts» добавить «`hardened` free-list head/continuation validation».
- `README.md:1437`: строка `hardened` — добавить «detected Small free-list corruption aborts the process».
- `CHANGELOG.md` — `[correctness fix]`, с явным «no speed claim».
- Индекс: этот отчёт вводит новую P4-находку (A) → в `docs/correctness-open-items/` завести карточку и в том же коммите закрыть её в `RESOLVED.md` (прецедент item 165 «filed and CLOSED» одним проходом), плюс строка в lookup-таблице `docs/CORRECTNESS_OPEN_ITEMS.md`.

Тесты (`tests/regression_freelist_next_validation.rs`, переписать в подпроцессный по образцу `tests/r12_01_foreign_free_of_unissued_granule.rs:40-101`; гейт без изменений: `hardened` + `internals`):
- Parent-тест на каждый случай запускает `current_exe()` с env-маркером случая; child: setup → `println!("ARMED")` → триггер → `println!("SURVIVED")` → `exit(0)`.
- Случаи «abort»: `pop_out_of_segment_next_aborts`, `drain_out_of_segment_next_aborts`, `pop_allocated_continuation_aborts`, `drain_allocated_continuation_aborts`, `pop_self_cycle_aborts`, `drain_self_cycle_aborts` (setup — дословно из текущих `:60-87`, `:122-150`, `:187-209`).
- Оракул parent: `!status.success()`, stdout содержит `ARMED` и **не** содержит `SURVIVED`, stderr **не** содержит `panicked` (abort, а не паника).
- Контрольные случаи (зелёные и сейчас, и после): тот же setup без порчи/с восстановленной ссылкой → `success()` + `SURVIVED` и проверки из текущих `:97-116`, `:235-247` (выдача ровно один раз, попарная различимость).
- Красный-до/зелёный-после: на `31db7f17` все шесть abort-случаев печатают `SURVIVED` (null) и выходят успешно → parent красный; после фикса — зелёные.
- Мутанты (каждый должен уронить ≥1 случай): (M1) вернуть `Err(())` вместо abort в `free_continuation`; (M2) снять `valid_free_node` с головы в drain; (M3) снять self-link `next_off == off`; (M4) снять Floyd-проверку `probe == slow` (должен падать batch-цикл длины >1 — добавить случай `drain_two_node_cycle_aborts`: a→b→a); (M5) сделать spill-OOM тоже abort'ом — ловит существующий `tests/r11_p3_small_sidecar_preflight.rs` (проверить, что он покрывает spill-`Err`; если нет — добавить случай с принудительным spill-OOM, это вопрос §6).
- Entry-point (правило CLAUDE.md «точка входа»): дополнительно один подпроцессный случай через `HeapRegistry::dbg_claim_lease()` (как `tests/regression_magazine_free_bounds.rs:9-10`) — free → `dbg_flush_all` → порча → `alloc` до опустения магазина → abort. Для этого нужен HeapCore-делегат порчи (у `HeapCore` нет доступа к внутреннему `AllocCore`, grep `dbg_core` пуст): `pub unsafe fn dbg_corrupt_freelist_head_next` в `src/registry/heap_core/diag/diag_probes.rs`, гейт как у оригинала (`hardened` + `internals`), `# Safety` дословно, запись в `UNSAFE_HOOKS` `scripts/verify-dbg-hook-safety.mjs:284-287`. Если владелец предпочтёт не расширять поверхность хуков — этот случай опустить и честно указать, что судья на HeapCore-слое отсутствует.
- Прогоны: `cargo test --test regression_freelist_next_validation --features "hardened internals"` и `--features "production hardened internals"`, debug и release.

### Коммит 2 — (B)

Префикс: `fix(perf): reject interior own-thread Large frees` (прецедент `36ba3758` для production-границы на free; тело: «production behavior changes only under a `dealloc`/`realloc` contract violation; no speed claim»).

Файлы/функции:
- `src/alloc_core/segment/segment_header/segment_header_views.rs`: `pub(crate) fn payload_offset_at(base) -> usize` по образцу `large_size_at` (`:59-62`), без cfg-гейта (используется в всегда-компилируемом `dealloc_at_base`). Метод существующего `impl` — не новый экспорт.
- `src/alloc_core/alloc_core/mem/mem_impl.rs`, `dealloc_at_base`, Large-ветка — **перед** `claim_live` (`:265`):
  ```text
  if ptr.addr() != base.addr().wrapping_add(SegmentHeader::payload_offset_at(base)) {
      // Interior/non-payload own-thread Large free: reject before claiming.
      #[cfg(any(feature = "alloc-global", feature = "alloc-stats"))]
      FOREIGN_OR_UNROUTABLE_FREES.fetch_add(1, Ordering::Relaxed);
      return;
  }
  ```
  (cfg инкремента — объединение: под `alloc-global` foreign-аналог считает безусловно, standalone `AllocCore::dealloc` — под `alloc-stats`, `:235-236`. Сам static объявлен под `#[cfg(feature = "alloc-core")]` (`counters.rs:377-379`) — при реализации проверить, что оба плеча cfg его подразумевают; по коду не проверял.)
- Хелпер-предикат `AllocCore::large_payload_mismatch(base, block) -> bool` (`kind_at(base) == Large && block != base + payload_offset_at(base)`) в `src/alloc_core/alloc_core/mem/realloc_fastpath.rs` рядом с `safe_payload_read_span` (`:92-100`); вызвать:
  - `AllocCore::realloc` — сразу после неудачного in-place (`mem_impl.rs:655-659`), до `safe_payload_read_span` (`:678`): `return null`;
  - `HeapCore::realloc` — после in-place/promotion, до `realloc.rs:336`: `return null`.
  Без этого фикс в `dealloc_at_base` превратит interior-realloc в возврат `new_ptr` при неосвобождённом старом блоке.
- Doc: `mem_impl.rs:203-209` и `:620-622` (best-effort-абзац); `src/alloc_core/alloc_core/counters.rs:366-376` и `src/global/alloc_stats.rs:137-151` — счётчик также считает own-thread Large free/realloc с адресом ≠ payload; `src/lib.rs:42` — проверить согласованность.
- Индекс: SEC-03 сейчас без карточки (решение `2026-10-08-src-review-xxs-round-17-P4.md:365`) → завести и закрыть в `RESOLVED.md` в том же коммите; CHANGELOG `[correctness fix]`.

Тесты — новый файл `tests/regression_own_large_interior_free.rs` (подпроцесс на случай: счётчики процесс-глобальные, а красный случай без `alloc-decommit` — unmap и SIGSEGV при чтении):
- `alloc_core_interior_large_free_is_counted_noop` (`alloc-core` + `internals`): `AllocCore::new()`, `p = alloc(2 MiB, align 16)`, запись сентинелов в `p[0]`, `p[len-1]`; снимки `AllocCore::dbg_segments_released_total()` (`alloc_core_core_diag/totals.rs:47`), `dbg_foreign_or_unroutable_frees()` (`:26`), при `alloc-decommit` — `ac.dbg_large_cache_occupied_bits()` (`alloc_core_large_cache_eviction.rs:268`). `dealloc(p + 64)` и, если `payload_offset >= 64` (вычислить через `p - segment_base`), `dealloc(p - 64)`. Оракул: released и occupancy не изменились; foreign-счётчик +N (под `any(alloc-global, alloc-stats)`); сентинелы целы; `q = alloc(same)` ≠ `p` (оракул aliasing через large-cache — именно он красный на `31db7f17` под `production`); затем валидный `dealloc(p)` — released +1 или occupancy изменилась (ловит мутант «после `claim_live`», который здесь abort'ит → child неуспешен).
- `heap_core_interior_large_free_is_counted_noop` (`internals` + `alloc-global`): то же через `HeapRegistry::dbg_claim_lease()`; вход — `HeapCore::dealloc` (слой `#[global_allocator]`).
- `own_realloc_interior_large_returns_null` — `AllocCore::realloc(p + 64, layout, 2×)` и shrink-вариант → `null`, сентинелы целы, released не изменился; то же для `HeapCore::realloc`.
- Контроль: точный `dealloc(p)` по-прежнему освобождает/кэширует (ловит мутант «no-op для всех Large»).
- Мутанты: (M1) убрать проверку → красные free-случаи; (M2) перенести после `claim_live` → финальный валидный free abort'ит; (M3) `<`/`>` вместо `!=` → красный один из `±64`; (M4) убрать инкремент → красный счётчик; (M5) убрать realloc-пред-проверку при наличии free-проверки → realloc возвращает не-null (красный).
- Строки фич: `alloc-core internals`, `fastbin internals` (без `alloc-decommit` — путь release), `production internals`, `production hardened internals`; debug и release.

Порядок: коммит 1 и 2 независимы; (B) раньше, т.к. затрагивает production. Перед каждым: `npm run check` (по правилам проекта), zero-trust ревью диффа, CI на landing SHA. Перф-гейт не требуется: в production добавляется одна проверка на холодном Large-пути; если владелец захочет квитанцию — `npm run iai` до/после с оракулом активации (число Large free), как в правилах R30-8.

## §6 Что нельзя решить без исполнения и вопросы владельцу

Без исполнения не проверено: отсутствие false-positive `valid_free_node` вне покрытых CI сценариев; что `payload_offset` == выданный адрес под `numa-aware`/biased-резервом (`align >= SEGMENT`) и `exact-span-large`; коды завершения abort-подпроцесса по ОС; покрывает ли `tests/r11_p3_small_sidecar_preflight.rs` именно spill-`Err` из `pop_free`/drain (нужен для мутанта M5 в (A)).

Вопросы владельцу:
1. Утвердить принцип «плохой вход до мутации → no-op/null + счётчик; испорченное состояние → abort» как явную политику (одна фраза в `sefer_alloc/mod.rs` «Caller boundary»/«Invariant aborts»).
2. Должен ли `hardened` в целом стать fail-closed и для **входов** (H1, F7-B, SEC-03 → abort вместо no-op), как это обычно у hardened-аллокаторов (из общих знаний)? Это решение уровня профиля; я рекомендую не менять его точечно в SEC-03.
3. Допустим ли новый HeapCore-делегат порчи free-list (hardened + internals, `UNSAFE_HOOKS`) ради entry-point-судьи в (A), или ограничиться `AllocCore`-слоем?
4. Префикс для (A): `fix(perf)` (прецедент `54fc2a21`) или `fix(alloc)`?
5. Item 166: при принятии принципа из п.1 — foreign Small interior в выданный блок однородного листа остаётся abort'ом владельца (вход распознан поздно, после публикации). Оставить как есть или выбрать вариант (2) из Next trigger карточки?
6. Production без `hardened` по-прежнему не проверяет `next` (item 178 это оговаривает; R17-SEC-02 рекомендовал хотя бы безусловный `is_free(head)`). Вне этой темы, но решение (A) делает разрыв hardened/production явнее — ставить ли отдельную задачу?

## §7 Проверено и НЕ дало вывода

- `scripts/verify-dbg-hook-safety.mjs` и hook-карточки 5/7/8/9 (`TRACKED_hook_safety.md`) — для плана важна только классификация `dbg_corrupt_freelist_head_next` (`UNSAFE_HOOKS`, гейт `hardened` + `internals`, R17 `§«Кандидаты»` счёл не нарушающим политику); карточки целиком не перечитывал, вывода для темы нет.
- Попытка найти в README/ARCHITECTURE/`src` явную формулировку «fail-closed» — нет ни одного вхождения; политика существует только неявно (через категорию invariant aborts в `sefer_alloc/mod.rs`).
- Поиск существующего теста interior-Large free в `tests/` — нет; `regression_hardened_interior_ptr.rs` покрывает только Small H1.
- `realloc_fastpath.rs` OPT-G читает `payload_offset` через полный `read_at` (`:249`) — можно было бы перевести на новый `payload_offset_at`, но это вне задачи и без выигрыша, кроме единообразия.

### Черновые заметки (B), записаны по ходу

- Единственная точка own-thread Large free: `AllocCore::dealloc_at_base` (`mem_impl.rs:255-542`), Large-ветка `:262-490`. Вызывающие: `AllocCore::dealloc` (`:241`), `dealloc_with_base` (`:566`, не-`fastbin`), move-leg `AllocCore::realloc` (`:694`). HeapCore: `dealloc_own_thread_with_base` → `self.core.dealloc` для Large-раскладки (`dealloc_own_base.rs:550-559`) и для `RouteToLargeFree` (`:387-404`); `dealloc_batch` → `self.core.dealloc` (`dealloc_batch.rs:319-328`); move-leg `HeapCore::realloc` (`realloc.rs:403-406`).
- `claim_live()` (`mem_impl.rs:269-271`) выполняется ДО `read_at` (`:293`): проверку надо ставить до `claim_live`, иначе несовпадение «съест» LIVE-кредит, и последующий корректный free упадёт в `abort()` на `:270`.
- Сиблинги со сверкой: foreign `RouteDirectory::lookup` (`directory.rs:876-881`) → `None` → `publish_foreign` считает `FOREIGN_OR_UNROUTABLE_FREES` (`routing.rs:26-30`); OPT-G (`realloc_fastpath.rs:249-252`) → `None`.
- НОВОЕ (уточнение SEC-03): OPT-G-отказ не защищает realloc — `None` ведёт в move-leg, который освобождает interior-указатель через `dealloc_at_base` (`mem_impl.rs:655-694`; `realloc.rs:201-216` → `:336-406`). `safe_payload_read_span` (`realloc_fastpath.rs:92-100`) для Large даёт `span_usable - off`, т.е. interior проходит. Значит фикс только в `dealloc_at_base` превратил бы interior-realloc в «успешный» realloc с утечкой старого Large (new_ptr возвращён, старый не освобождён) — realloc-пути нужно проверять ДО move-leg (как foreign realloc: `realloc.rs:414-420` → null).
- `small_free_guard` (после `36ba3758`) не участвует: Large-раскладка туда не попадает (`class_for` → `None`), Large-kind с small-раскладкой уходит в `RouteToLargeFree` → `core.dealloc` (`dealloc_own_base.rs:227-229`, `:236-242`). Hardened F7-A сверяет только `large_size/large_align` с раскладкой (`:238-239`), не адрес.
- Существующих тестов interior-Large free нет (grep по `tests/`).
- CI: run `38103112281` (CI, SHA `31db7f17`) — success; job `test (hardened tier)` — success (`gh run view`, только чтение). Этот job гоняет `cargo test --features "hardened internals"` и `"production hardened internals"` (`.github/workflows/ci.yml:1810-1821`).

### Черновые заметки (A), записаны по ходу

- `alloc_core_small_impl.rs:372-413` `valid_free_node`/`free_continuation` (только `hardened`); `pop_free` `:417-466` возвращает `Err(())` в ДВУХ разных случаях: (1) hardened-порча (`:425-427`, `:434`), (2) `prepare_small_issue` → `None` = легальный spill-OOM (`:441-443`; `segment_table_impl.rs:312-333` — нарушения инварианта там уже `abort()`, `None` — только OOM). Сигнал смешан.
- `try_drain_freelist_batch` `:527-651`: тот же смешанный `Err(())` (`:558`, `:569`, `:574-590`); `drain_freelist_batch` `:517-525` превращает `Err` в `0` (= «пусто»).
- Вызывающие: `alloc_small` `:163-262` — `Err` на шаге 1 (`small_cur`) сразу `null`, без перехода к другим сегментам/bump (`:169`, `:186`, `:222`, `:254`); то же `alloc_small_with_virgin` `:293-360`. `refill_class_bump_impl` (`alloc_core_small_magazine.rs:180-360`): `Err` → `return filled` (`:235`, `:251`, `:347`), т.е. 0 если первым же шагом.
- HeapCore: `refill_magazine_slow` (`hot.rs:675-698`) → `refill_with_large_rescue` (`:702-712`) → 0 → `null`. `find_segment_with_free_impl` (`find_segment.rs:188-614`) детерминированно возвращает первый сегмент с непустой головой (директория — младший бит; fallback — младший слот) → испорченный сегмент выбирается каждый раз.
- Вывод (по коду): класс «залипает»: каждое последующее обращение к substrate за этим классом → `null`, пока голова не станет валидной. Даже свежий free на этот сегмент не помогает: push делает новую голову, чей `next` = испорченный узел, и hardened-проверка continuation отвергает уже её (`:434`). Магазин продолжает обслуживать hit'ы, пока не опустеет.
- Под `#[global_allocator]` `null` для инфаллибельных std-API → `handle_alloc_error` → abort по умолчанию с ложным диагнозом «memory allocation failed» (README `:1667-1670` фиксирует эту цепочку для null). Для fallible API (`try_reserve`) программа продолжает работу с «вечным OOM» класса.
- Крейт уже делает `abort()` на нарушения инвариантов метаданных в соседних местах: `find_segment.rs:151`, `:482`; `alloc_core_small_reclaim.rs:25,29,44,62` (геометрия sidecar-записи → abort; дубликат/магазин → `false`); `segment_table_impl.rs:301-305,324-328`. Всего `grep -rn 'process::abort()' src | wc -l` = 110 в 28 файлах.
- README `:1437`: `hardened` = «Opt-in own-thread interior-pointer guard … Extra misuse detection». Слова «fail-closed» в README/ARCHITECTURE/src нет (grep пуст).
