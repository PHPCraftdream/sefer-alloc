# Ревью исправлений раунда 6 — оптимизация

Ревизия: `main` @ `ac23d06e`. Диапазон исправлений: `132ab85b..HEAD`. Режим — только чтение: ничего не собиралось и не запускалось, asm не снимался. Все измеренные числа ниже взяты из закоммиченных CSV гейтов (`docs/perf/R6_*_summary.csv`). Если число не из CSV, оно помечено как **оценка**. Номера строк даны по HEAD, если не указано иное.

## 1. Резюме

1. **Атрибуция фиксированной цены OPT-05 (R6-T40).** Цена в run 1 не «≈ +113 Ir на refill». Это **≈ +7.06 Ir на каждый routed-вызов скалярного `prepare_small_issue`**, то есть на каждый блок, нарезанный bump-carve. В 16-блочных ячейках две модели дают одинаковые числа: 113 / 16 = 7.0625. Различают их ячейки класса 256 KiB, где refill = 1 блок. По модели «на refill» `multiseg_cold_256k` и `seg_cycle_decommit_256k` должны были вырасти минимум на +3.8k / +23k Ir. Они упали: −824 / −5196. Механизм виден в диффе кандидата `26843755`. OPT-05 переписал **общий скалярный** `RouteSlots::prepare_small_issue`, через который идёт carve. Новая версия делает лишний `small_sidecar()` на каждый блок. Кэшированный `sidecar` фазой `prepare` вообще не используется. Хелпер `small_issue_route` потерял `inline(always)`. Это подтверждено чтением; размер вклада по инструкциям — оценка.
2. **У OPT-05 есть исправимый вариант (OPT-05′), и он покрывает именно ту ячейку, которая его завалила.** Нужно: не трогать скалярный путь; в фазе `prepare` использовать кэшированный sidecar; слить `prepared` и `issue` в одну загрузку листа; применить тот же контекст к `try_carve_batch`. Сейчас carve повторно резолвит маршрут 3–4 раза на блок. Измеренная валовая экономия OPT-05 на drain — 45 Ir/блок (выведено из CSV). Ожидаемый эффект на carve — того же порядка (**оценка**).
3. **Систематическое повторное чтение неизменяемых атомиков маршрута.** `Entry.root`, `Entry.sidecar` и `EntryHandle.ptr` после конструирования не пишутся (grep: ни одного `store`/`swap`). Тем не менее каждый `root()`/`small_sidecar()` заново проходит цепочку атомарных загрузок. Owner-private кэш в `RouteSlots` удешевит prepare/issue/scan. Кроме того, он может сделать точную проверку класса на own-free (R6-SEC-1) достаточно дешёвой для дефолтной сборки (**гипотеза**).
4. **Дешёвые находки на пути refill-промаха, подтверждённые чтением:**
   - `drain_large_sidecar_ingress_hot_bounded` даже без единого живого Large делает два прохода `next_active`;
   - `maybe_materialize_directory` (R6-T32) стоит измеренные +23 Ir на refill (+368 Ir / 16 refill в `cold_alloc_only_256x16b`), хотя его быстрый путь — одна проверка на null;
   - `finish_magazine_refill` ставит бит магазина поблочно, хотя у нарезанного прогона смещения подряд.
5. **NO-GO R6-T12 и неотгейтованный OPT-01 связаны со слепотой оси Ir.** Callgrind считает `div r64` и `lock xchg` за 1 Ir. Поэтому reciprocal вместо `%` и `load` перед `swap` обязаны выглядеть в Ir нейтрально или хуже, даже если они экономят такты. R18 видел то же самое («skipped empty-word exchanges but retired 32 more Ir»). Для этих двух идей нужна нативная ось тактов, а не повторный iai.

## 2. Предложения по приоритету

Обозначения: **[П]** — подтверждено чтением кода или CSV; **[Г]** — гипотеза, требует asm или измерения. Все «ожидаемые выигрыши» — **оценки**, если не процитированы из CSV.

### O-1 (P1) — атрибуция цены OPT-05 и вариант OPT-05′

**Атрибуция [П по CSV + диффу; Г по разбивке на инструкции].** Источник — `docs/perf/R6_T40_OPT04_OPT05_GATE_run1_summary.csv` (A = `ef694302`, B = `26843755`).

| ячейка | routed-carve блоков (по коду ячейки и `FREE_PARK_CAP` = 16) | delta Ir | delta / блок |
|---|---:|---:|---:|
| `alloc_magazine_hit_only_16b` | 16 | +112 | 7.000 |
| `small_churn_16b` | 16 | +112 | 7.000 |
| `dealloc_free_only_16b` | 64 | +451 | 7.047 |
| `cold_alloc_only_256x16b` | 256 | +1807 | 7.059 |
| `cold_alloc_only_256x16b_2n` | 512 | +3615 | 7.061 |
| `cold_alloc_only_256x16b_4n` | 1024 | +7231 | 7.062 |
| `dealloc_prealloc_only_1088_16b` | 1088 | +7683 | 7.062 |
| `growth16_n` / `_2n` / `_4n` | 32768 / 65536 / 131072 | +231423 / +462847 / +925695 | 7.0625 / 7.0625 / 7.0625 |
| `carve_batch_only_16b` (standalone `AllocCore`, `routes = None`) | 256 | 0 | 0 |
| `multiseg_cold_256k` (класс 258 752 B, refill = 1 блок; 2 × 34 = 68 refill) | — | −824 | — |
| `seg_cycle_decommit_256k` (6 × 34 = 204 refill по 1 блоку) | — | −5196 | — |

Знаменатели — число блоков, нарезанных в ячейке. Для 16 B класса refill = 16 блоков (`refill_n_for_class`, `tcache.rs:135`), поэтому «на блок» и «на refill / 16» здесь неразличимы. Различают их две ячейки 256 KiB. По модели «+113 на refill» только раунд 1 `multiseg_cold_256k` (34 carve-refill) дал бы ≥ +3842 Ir. Чтобы получить −824, экономия на drain в раунде 2 должна была бы составить > 137 Ir на одноблочный drain. Это втрое больше валовой экономии, измеренной на длинных цепочках (45 Ir/блок, см. ниже). Значит, модель «на refill» опровергнута. Модель «7 Ir на routed-carve блок» согласуется со всеми строками. Полную декомпозицию 256K-ячеек я не делал: смесь carve и drain в раунде 2 не восстановлена.

Ячейка `carve_batch_only_16b` = 0 локализует цену в routed-ветке `SegmentTable::prepare_small_issue`. Standalone-таблица туда не заходит.

**Механизм (дифф `e966076b..26843755`, файлы `route_slots.rs` и `small_issue_route_impl.rs`) [П].** OPT-05 не только добавил контекст в `try_drain_freelist_batch`. Он **переписал общий скалярный** `RouteSlots::prepare_small_issue` (HEAD: `src/alloc_core/segment/segment_table/route_slots.rs:159-182`). Через него проходит каждый carve: `alloc_core_small_impl.rs:895` (`try_carve_batch`), `:692` (`carve_block`), `:449` (`pop_free`), `:367`. Новое тело делегировало в `small_issue_route(index, base)` и `SmallIssueRoute::prepare`. Расхождения с HEAD на каждый вызов:
- (a) в HEAD `route.small_sidecar()` вызывается только на ветке отказа (`route_slots.rs:178`). У кандидата он вызывался безусловно, с `unwrap_or_else(fatal)`;
- (b) `SmallIssueRoute::prepare` вызывал `self.route.prepare_small(offset, class)`. Тот снова делает `small_sidecar()` (`src/registry/segment_route/registration.rs:29-32`). Кэшированное поле `sidecar` в prepare-фазе не использовалось. Lookup на каждый блок не убран, а удвоен;
- (c) у `RouteSlots::small_issue_route` в кандидате нет атрибута inline, хотя заменённое тело было `#[cfg_attr(not(debug_assertions), inline(always))]`. При 5+ местах раскрытия возможен вызов вне строки с прологом и возвратом пары указателей **[Г, asm не снимался]**.

Один `small_sidecar()` — это загрузка `EntryHandle.ptr` (атомик), загрузка `Entry.kind`, сравнение и загрузка `Entry.sidecar` (атомик). Вместе с ветвлениями это ~4–5 инструкций, вызов из (c) — ещё ~2–4. В сумме это порядок 7 Ir, но разбивка — **оценка**.

**Почему валовая экономия на drain только ~45 Ir/блок [П по CSV + коду].** В `recycle_alloc_only_256x16b` раунд 1 побайтно совпадает с `cold_alloc_free_256x16b` (delta +1807). Значит, раунд 2 (drain 256 блоков) = −9728 − 1807 = −11535 Ir, или −45.06 Ir/блок. Это больше, чем «−38», указанные в отчёте гейта. Но и в кандидате commit-фаза оставалась избыточной. `SmallIssueRoute::commit` выполнял `sidecar.prepared()`, затем `route.issue_small()`, затем снова `small_sidecar()` (`registration.rs:35-38`). Дальше шёл `SmallSidecar::issue` (`small_sidecar.rs:92-95`, это `prepared() || prepare()`, повторная загрузка листа), затем `bitmap()` (`:69`), затем `SidecarBitmap::issue` (`sidecar_bitmap.rs:74`) и, наконец, `ClassLeaves::issue` (`leaf_classes.rs:113-130`, третья загрузка `mixed[leaf]` и повтор проверок диапазона). Атомарные загрузки LLVM не сливает **[Г, общеизвестное поведение, не проверено asm]**, так что все они остаются в коде.

**Предложение OPT-05′ [идея, П по коду]:**
1. Скалярный `RouteSlots::prepare_small_issue` не трогать: тело HEAD побайтно, `inline(always)` сохранить.
2. `SmallIssueRoute::prepare` вызывает `self.sidecar.prepare(offset, class)` напрямую. Sidecar гарантирован конструктором; fatal-байты отказа сохранить.
3. Добавить `SmallSidecar::issue_prepared(offset, class) -> bool`: одна Acquire-загрузка `mixed[leaf]` и Release-store класса. Это объединяет проверку `prepared` и `issue`, abort при null-листе сохраняется. Batch-commit использует его.
4. Распространить контекст на `try_carve_batch` (`alloc_core_small_impl.rs:891-900` — prepare-цикл, `:979-994` — commit-цикл). Нарезанные смещения идут подряд, поэтому prepare можно делать **по листу** (`ClassLeaves::prepare_range`: одна проверка promotion на 4 KiB лист, 256 гранул), а в commit загружать указатель листа один раз на лист.

**Ожидаемый выигрыш (оценка):** drain — не меньше измеренных валовых 45 Ir/блок, плюс снятая избыточность commit (−5…−10 Ir/блок). Carve — −30…−45 Ir/блок: цепочка резолва та же, что на drain. Для `cold_alloc_only_256x16b` (102 838 Ir) это порядка −8k…−11k Ir.

**Риск:** средний. Затрагивается протокол `prepare → commit` (`IssueTransaction`, `issue_transaction.rs:88-123`) и байты fatal. Откат при OOM должен остаться побайтным: output, head, live и bump не меняются. Тесты: `tests/r11_ph3a_issue_transaction_batch.rs`, инъекции `fail_prepare_after_for_test`/`fail_spill_after_for_test`. Loom не требуется: код owner-only.

**Гейт:** `perf_gate_iai` A/B на слое `HeapCore` / `SeferAlloc`.
- Выигрыш: `recycle_alloc_only_256x16b`, `recycle_alloc_free_256x16b`, `recycle_alloc_free_256x64b`; наклон `cold_alloc_only_256x16b{,_2n,_4n}` и `cold_alloc_free_256x16b{,_2n,_4n}`; `dealloc_prealloc_only_1088_16b`; наклон `growth16_{n,2n,4n}`.
- Контроли: `alloc_magazine_hit_only_16b − alloc_magazine_prefill_only_16b` (только hit), `small_churn_16b_2n − small_churn_16b`, `carve_batch_only_16b` (должна остаться 0), `mimalloc_*` = 0.
- **Обязательно:** ячейки 256 KiB (`multiseg_cold_256k`, `seg_cycle_decommit_256k`) как дискриминатор «на refill против на блок».
- Oracle: новый счётчик `bench-internals` «резолвов маршрута на выданный блок» (≤ 1 на партию). Отчёт U25 честно отмечает, что такого oracle нет. Без него `tests/r6_t40_batch_issue_route.rs` проходит и при поблочном резолве.

### O-2 (P1/P2) — owner-private кэш корня и sidecar в `RouteSlots`

**Где [П]:** `src/registry/segment_route/directory.rs:132-135` (`entry()` — Relaxed-загрузка `AtomicPtr<Entry>`), `:172-174` (`root()`), `:187-199` (`small_sidecar()`). Вызывающие:
- `route_slots.rs:140-157` (`issue_small`) — `root()` и два `small_sidecar()`;
- `:159-182` (`prepare_small_issue`) — `root()` и один-два `small_sidecar()`;
- `:191-210` (`small_class_matches`, hardened);
- `:212-230` (`scan_small_from`).

`grep` по `src/registry/segment_route/*.rs` не находит ни одного `store`/`swap` этих полей после `EntryHandle::new` (`directory.rs:44-106`): значения неизменяемы на всё время регистрации.

**Идея:** в слоте `RouteSlots` рядом с `Option<RouteRegistration>` хранить простые `root: *mut u8` и `sidecar: *const SmallSidecar`. Заполнять их в `put()`, обнулять в `remove()` и `close_all()`. Prepare и issue читают `slots[index].sidecar` — одна неатомарная загрузка вместо цепочки «слот → handle → entry → kind → sidecar».

**Ожидаемый выигрыш (оценка):** −10…−20 Ir на скалярный prepare или issue; суммируется с O-1, если O-1 не возьмут. Побочный эффект: дешевле становится hardened `small_class_matches`. По отчёту `R6_T05_T06_T33_FREE_VALIDATION_GATE.md` цепочка lookup класса стоит «~33 Ir per free». С кэшем это порядка 10–15 Ir (**оценка**). Тогда по правилу «без флага, если цена в допуске» точную проверку класса на own-free (полное закрытие R6-SEC-1) можно переоценить для дефолтной сборки. Это решение владельца.

**Риск:** средний. Кэш не должен пережить `RouteRegistration`: порядок `Drop`, повторное использование слота после `remove`. Нужен тест «remove → put в тот же индекс → кэш свежий» и miri-прогон на `route_slots`.

**Гейт:** как в O-1, плюс сборка `hardened` на `dealloc_free_only_16b{,_n8,_n16}` и `small_churn_16b` для варианта с проверкой класса. Oracle — счётчик загрузок дескриптора (`bench-internals`).

### O-3 (P2) — Large hot-probe на каждом Small refill-промахе без живых Large

**Где [П]:** вызов — `src/registry/heap_core/alloc/hot.rs:654` (`refill_magazine_slow`) и `:481` (virgin-сосед). Тело — `src/alloc_core/alloc_core/sidecar_drain.rs:525-575`. При пустом Large-индексе `next_active(Large, start)` возвращает `None`, затем выставляется `wrapped = true` и следует второй `next_active(Large, 0)`. Это два прохода `ActiveKindIndex::next` (`active_kind_index.rs:122-147`: загрузка листа, загрузка summary, маски), даже когда `start == 0`.

**Идея:** ранний выход по `large_summary == 0` (точный owner-only индекс, `active_kind_index.rs:16`) и пропуск wrap-прохода при `start == 0`.

**Ожидаемый выигрыш (оценка):** −15…−25 Ir на Small refill-промах.

**Риск:** очень низкий. Семантика не меняется: summary точен по построению (`set`/`clear`).

**Гейт:** iai — наклон `cold_alloc_only_256x16b{,_2n,_4n}` (16/32/64 refill), `small_churn_16b`, `growth16_*`. Контроли: все `large_*` и `realloc_large_*` без регресса, `LARGE_SIDECAR_SLOT_INSPECTIONS` на Large-ячейках не меняется.

### O-4 (P2) — выделить холодную часть `maybe_materialize_directory` (R6-T32)

**Где [П]:** вызов — `src/alloc_core/small/alloc_core_small/find_segment.rs:502`. Он идёт в `linear_scan` на каждом routed refill-промахе, потому что routed-ядро не доверяет отрицательному ответу директории (`:246-250`). Тело — `src/alloc_core/small/alloc_core_small/directory.rs:59-97`: функция без атрибута inline, быстрый путь (null-проверка и сравнение `count`) находится в одной функции с OS-резервом и `rebuild_from_table`.

**Измерено:** `R6_T31_T32_POOL_DECAY_DIRECTORY_REBUILD_GATE.md` — `cold_alloc_only_256x16b` +368 из 109 387, `recycle_alloc_only_256x16b` +368. В этих ячейках нет Large, а T31-decay стоит только на Large-рёбрах. Значит, это цена T32 на 16 refill, ~23 Ir на refill.

**Идея:** `#[inline(always)] fn maybe_materialize_directory()`, который проверяет `null && count >= THRESHOLD` и только тогда зовёт `#[cold] #[inline(never)] fn materialize_directory_slow()`.

**Ожидаемый выигрыш (оценка):** вернуть большую часть из 23 Ir/refill (−15…−20 Ir/refill) **[Г: что цена — это пролог и вызов, а не перераспределение регистров в `linear_scan`, требует asm]**.

**Риск:** низкий. **Гейт:** те же cold- и recycle-ячейки. Oracle: `DIRECTORY_HITS` / `FULL_SCAN_SLOTS_EXAMINED` до и после trim не меняются (тест R6-T32).

### O-5 (P2) — диапазонная отметка magazine-бита в `finish_magazine_refill`

**Где [П]:** `src/registry/heap_core/alloc/hot.rs:81-87`. Для каждого из `n − 1` блоков выполняется `segment_base_of_ptr`, вычитание и `SegmentMeta::new(...).magazine_bitmap().mark_magazine(off)`, то есть байтовый RMW (`segment_bitmap.rs:92-97`). После carve-refill эти `n − 1` блоков — непрерывный прогон с шагом `block_size` в одном сегменте (`try_carve_batch`, `alloc_core_small_impl.rs:979-994`).

**Идея:** refill возвращает границу «drained | carved»; для carved-хвоста — один `set_range(first_off, n, stride)`: 15 бит для 16 B класса укладываются в ≤ 3 байта.

**Ожидаемый выигрыш (оценка):** −5…−8 Ir на удержанный блок carve-refill, ~−70…−110 Ir на 16-блочный cold-refill.

**Риск:** низко-средний. Семантика RAD-5 не меняется, но меняется контракт `refill_class_bump_internal`. Тест: после refill `is_in_magazine` истинен ровно для `slots[0..n-1]`.

**Гейт:** наклон `cold_alloc_only_256x16b*`, `growth16_*`, `alloc_magazine_prefill_only_16b`. Контроль — `recycle_*`: там drained-блоки разбросаны, ожидается ≈ 0.

### O-6 (P2, методология) — R6-T12 и OPT-01: ось Ir слепа к `div` и `lock xchg`

**Где [П]:**
- `src/alloc_core/small/alloc_core_small_reclaim.rs:41` — `!off.is_multiple_of(block_size)`, делитель известен только во время исполнения, то есть одна инструкция `div` на запись owner-drain;
- `src/alloc_core/segment/remote_bitmap/bitmap_scan.rs:59` — безусловный `pending.swap(0, AcqRel)` на каждое слово скана, в том числе пустое.

**Наблюдение [П по отчётам]:**
- R6-T12 заменил `%` на reciprocal и поднял контекст: все drain-ячейки выросли на +0.03…+0.14 %.
- R18 (запись в `OPEN_ITEMS_ARCHIVE.md:1475`): «Candidate mode skipped empty-word exchanges but retired 32 more Ir per scenario».
- R6-T05: проверка `divides` на free стоит +2 Ir/free (`dealloc_free_only_16b` +128 / 64 free).

**Объяснение [Г, без asm]:** callgrind считает `div r64` и `lock xchg` за одну инструкцию. Замена `div` на mul+rotate+cmp+загрузку таблицы или `xchg` на load+branch неизбежно добавляет Ir. Такты при этом уменьшаются: по публичным таблицам латентности x86 это десятки тактов на `div` и на заблокированный RMW. «Estimated Cycles» в iai моделирует только промахи кэша, а не латентность инструкций. Поэтому NO-GO R6-T12 верен для Ir и ничего не говорит о тактах. OPT-01 (owner-only `load` перед `swap`, `OPEN_ITEMS.md` item 88: «never gated on its own … unowned») по Ir отклонить нельзя, его можно только не измерить.

**Предложение:**
1. Отгейтовать OPT-01 и отдельно reciprocal из R6-T12 (без переноса контекста сегмента — его NO-GO не переоткрывается) на **нативной оси**. Варианты: paired A/B через `scripts/paired-ab-runner.mjs` с `perf stat -e cycles,instructions` на закреплённом ядре или criterion-обвязка тех же сценариев. Детерминированная альтернатива — `llvm-mca` по ядру цикла скана и reclaim **[Г: применимость к CI не проверена]**.
2. В Ir-гейте допуск для этих двух правок — **рост** Ir, равный числу снятых `div`/`xchg`, а не «≤ 0».

**Ожидаемый выигрыш (оценка):** для OPT-01 — порядка десятка тактов на пустое слово скана. Число слов на refill-промах растёт с `bump` (до ~4 024 на сегмент, `SUMMARY.md` R6-T02), поэтому в `growth16_*` и `multiseg_cold_256k` эффект может быть заметен по wall-clock. Величина не измерена.

**Риск:** OPT-01 меняет owner-side чтение протокола терминальной публикации. Нужен loom actual-source (`tests/loom_sidecar_bitmap_actual.rs`) с мутантом «Relaxed-load вместо Acquire». Это **не** C′/C''/C‴ и не epoch-skip R6-T02: producer не меняется, drain не пропускается, пропускается только RMW по нулевому слову.

**Гейт:** нативные `growth16_*`, `multiseg_cold_256k`, `seg_cycle_decommit_256k`, `r19_prf01_publication`; контроль — `r19_prf01_joined_producer_same_word`, `r20_prf01_c3_producer_distinct_words` (producer не меняется, ожидается 0). Oracle — существующий `PENDING_WORDS_VISITED` и счётчик пропущенных обменов (`SCAN_COUNTS[2]` из R18). Обвязку R18 (`cfg(r18_sidecar_scan_bench)`, `bitmap_scan.rs:47-69`) можно переиспользовать, а не писать заново.

### O-7 (P3) — повторный `find` под тем же shard-локом в `RouteDirectory::lookup`

**Где [П]:** `src/registry/segment_route/directory.rs:942-971`. `guard.find(key)` вызывается на `:949` и ещё раз на `:966`, после `pin_from_array`, плюс повторное чтение неизменяемого `incarnation`. Комментарий на `:950-951` прямо говорит: «the shard lock excludes unlink and final release». Под удерживаемым локом второй `find` — тавтология. Это двоичный поиск по индексу и блоку, с двумя зависимыми загрузками на шаг (`key_at`).

**Идея:** заменить release-проверку `:966-968` на `debug_assert!`.

**Ожидаемый выигрыш (оценка):** −10…−30 Ir на cross-thread free, зависит от заполненности шарда.

**Риск:** низко-средний. Снимается release-abort, который ловит только нарушение инварианта самого лока. Это решение по hardening, а не чистая оптимизация.

**Гейт:** iai `r19_prf01_joined_producer_same_word`, `r20_prf01_c3_producer_distinct_words`, `r20_prf01_c3_remote_large_free`; контроли — owner-ячейки `perf_gate_iai`, они должны дать 0.

### O-8 (P3) — загрузка указателя листа один раз на cut в `BitmapCut::pop`

**Где [П]:** `src/alloc_core/segment/remote_bitmap/bitmap_cut.rs:21-36`. На каждую запись выполняется `classes.encoded(granule)`, то есть `leaf_classes.rs:132-145`: Acquire-загрузка `mixed[leaf]` и загрузка байта. Слово pending покрывает 64 гранулы, лист — 256 (`LEAF_GRANULES`, `leaf_classes.rs:18`). Поэтому все записи одного cut лежат в одном листе.

**Идея:** `BitmapCut` кэширует указатель листа при создании cut. Контекст сегмента из R6-T12 не возвращается: это свойство самого cut.

**Ожидаемый выигрыш (оценка):** −2…−3 Ir на запись.

**Риск:** низкий: лист живёт, пока держится pin маршрута.

**Гейт:** наклон `r19_prf01_publication`; oracle — `SIDECAR_INGRESS_RECORDS_CONSUMED`.

### O-9 (P3) — двойной проход цепочки в `try_drain_freelist_batch`

**Где [П]:** `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:567-614` (probe-проход: `read_next` и continuation-проверка) и `:617-641` (commit-проход: повторный `read_next` по той же цепочке).

**Идея:** в probe-проходе сохранять смещения в **локальный** `[u32; TCACHE_CAP]`, а commit-проход вести по нему. Писать в `out` в первом проходе нельзя: контракт отката при OOM требует, чтобы `out` не менялся, и тест U25 проверяет «output pattern survives». Для `out.len() > TCACHE_CAP` (batch-api) сохранить текущий путь.

**Ожидаемый выигрыш (оценка):** −3…−4 Ir на выданный блок.

**Риск:** низкий. **Гейт:** `recycle_alloc_*`. Удобно объединить с O-1 в одном A/B, если O-1 берут.

### O-10 (P4) — открытые пункты без изменений

- **R6-PF-8 (`OPEN_ITEMS.md` item 96).** `Node::zero` по-прежнему — `write_bytes` с длиной, известной только во время исполнения (`src/alloc_core/platform/node.rs:201-211`). Для этого нужна ячейка с `black_box(size)` через `#[global_allocator]`: в существующих ячейках layout константный, и они разницы не видят.
- **R6-T02 (epoch-skip drain)** остаётся главным рычагом на путях refill-промаха (полный скан sidecar при каждом промахе). Он заблокирован решением владельца. O-3, O-4 и O-6 от него не зависят.

## 3. Что проверено

- `git log --merges 132ab85b..HEAD`, `git diff --stat 132ab85b HEAD -- src`, неслияния по `src/` в диапазоне.
- `docs/agent_reviews_round6/SUMMARY.md` целиком. `R6_T40_U25_REPORT.md`. Гейт-отчёты `R6_T40`, `R6_T12`, `R6_T05_T06_T33`, `R6_T28`, `R6_T31_T32`, а также заголовки и вердикты `R6_T01`, `R6_T10`, `R6_T11`, `R6_T18`.
- CSV run 1 R6-T40 (`R6_T40_OPT04_OPT05_GATE_run1_summary.csv`) — все строки с ненулевой delta. Код ячеек `perf_gate_iai.rs`: `small_churn_16b`, `dealloc_free_only_16b`, `alloc_magazine_hit_only_16b`, `dealloc_*_1088_*`, `cold_alloc_only_*`, `recycle_*`, `multiseg_cold_256k`, `seg_cycle_decommit_256k`, `carve_batch_only_16b`, `growth16_*` (через `benches/support/r19_prf01.rs`), константы `CHURN_OPS`, `COLD_BATCH`, `PREFIX_OPS`, `MULTISEG_*`, `SEGCYCLE_*`.
- Дифф кандидата OPT-05: `git diff e966076b 26843755 -- src Cargo.toml`.
- Горячие пути HEAD:
  - `hot.rs` (целиком);
  - `dealloc_own_base.rs:180-587`;
  - `alloc_core_small_impl.rs:330-997`;
  - `alloc_core_small_magazine.rs:170-366`;
  - `sidecar_drain.rs` (целиком до `:595`);
  - `find_segment.rs:120-540`;
  - `route_slots.rs:1-260`;
  - `segment_table_impl.rs:290-343`;
  - `issue_transaction.rs`;
  - `registration.rs`;
  - `small_sidecar.rs:1-200`;
  - `sidecar_bitmap.rs`, `leaf_classes.rs`, `bitmap_cut.rs`, `bitmap_scan.rs`;
  - `directory.rs:1-240` и `:942-982`;
  - `shard_lock.rs`, `active_kind_index.rs`, `continuation_bounds.rs`, `node.rs` (continuation, zero);
  - `segment_bitmap.rs`, `magazine_bitmap.rs`;
  - `alloc_core_small_reclaim.rs`, `size_classes.rs` (divisibility);
  - `tcache.rs:100-200`;
  - `tls_heap.rs` (`current_for_*`), `global_alloc.rs:1-140`, `routing.rs` (`publish_foreign`).
- Неизменяемость `Entry.root` / `Entry.sidecar` / `EntryHandle.ptr`: grep по `\.(store|swap|compare)` в `src/registry/segment_route/*.rs` ничего не нашёл.
- Профили `Cargo.toml`: `lto = "thin"`, `codegen-units = 1` (release и bench).

## 4. Ограничения

- Ничего не собиралось, не запускалось и не дизассемблировалось. Все утверждения о кодогенерации — инлайнинг `small_issue_route`, отсутствие слияния атомарных загрузок, цена пролога `maybe_materialize_directory`, Ir-эквивалент `div`/`xchg` — **гипотезы**. Их проверка: `cargo asm` / `objdump` на CI или нагрузочно-свободной машине.
- Разбивка 7.06 Ir/блок на составляющие (a)/(b)/(c) — оценка. Подтверждены только линейность по числу routed-carve блоков (CSV) и наличие самих расхождений (дифф).
- Ячейки 256 KiB опровергают модель «на refill», но их отрицательная delta полностью не разложена: смесь carve и drain в раунде 2 по коду не восстановлена.
- Все «ожидаемые выигрыши» — оценки по чтению кода, а не измерения. Ни одна идея не прошла гейт. Каждой нужны своя преррегистрация, path-activation oracle и указание слоя (CLAUDE.md, R26-4/R30-8/R31-0).
- Нативная ось тактов (O-6) на GitHub-раннерах может быть недоступна: PMU в VM, `perf_event_paranoid`. Это не проверялось.
- Вне ревью: R6-T02, T16, T41/43/44/45, T55–T57 (не в `main`).
