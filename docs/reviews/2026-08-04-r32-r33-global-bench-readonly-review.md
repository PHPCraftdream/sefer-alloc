# Независимое read-only ревью новых волн R32/R33 и `global_alloc`

Дата: 2026-08-04  
Диапазон: `e124a48..40241b0810b42c672f3f7c507f21b2de762b782b`  
Масштаб: 61 коммит, 211 файлов, `+90 065 / -331`; из них `docs/perf` — 115 файлов и около 73 тысяч добавленных строк.

## Ограничения и метод

Это статическое read-only ревью. По прямому указанию владельца я не запускал сборку, тесты, Miri, Kani, loom, Criterion, iai, скрипты или исполняемые примеры. Разрешённые действия были ограничены чтением Git-истории и файлов. Единственная запись — этот отчёт.

Я принимал численные результаты отчётов R32/R33 как предоставленное авторами свидетельство, но отдельно проверял по коду:

- попала ли правка в production/default-путь;
- соответствует ли реализация заявленному механизму;
- сопоставимы ли A/B-руки;
- не подменяет ли локальный счётчик/iai реальный wall-clock;
- не изменён ли контракт памяти, RSS или correctness ради скорости;
- не противоречат ли текущему коду комментарии и benchmark-ярлыки.

## Короткий вердикт

**Да, код действительно ускорен, но волна не является новым радикальным ускорением всего аллокатора.** В ней есть один сильный локальный default-reachable выигрыш, несколько настоящих маленьких wins, одна перспективная, но пока недоказанная cross-thread правка и несколько изменений с нулевым измеренным wall-clock.

Главные выводы:

1. Самый сильный подтверждённый runtime-результат — `DECAY_CLOCK_CHECK_STRIDE=64`: выше headroom стоимость `maybe_decay_large_cache` в отчёте падает примерно со 116.4 до 45.4 нс на вызов, то есть приблизительно на 61%. Это реальное устранение дорогого `Instant::now()` с большинства событий.
2. Удаление повторной проверки базы в `realloc` и частичная запись Large-header — правильные, но маленькие улучшения: около 7.5 Ir на realloc и 32 Ir на Large-cache hit соответственно. Это не множители wall-clock.
3. Occupancy-bitmask Large-cache даёт около 5 Ir на admission, но production N=8 показал wall-clock NULL. Это полезное упрощение поиска, особенно для opt-in N=40, но не доказанное ускорение default.
4. Расширение `OWN_CACHE_SIZE` с 4 до 16 резко улучшило hit-rate на некоторых K, но последующий честный gate не обнаружил статистически значимого выигрыша latency ни на одном K. Ускорением это пока называть нельзя.
5. Shadow-head у `RemoteFreeRing` может быть сильным cross-thread win, но текущий before/after gate не является чистым A/B: руки собраны с разными feature-наборами и разными drain-путями. Заявленные 30–36% требуют повторного измерения.
6. `virgin-zero-skip`, `DiverseTurnover`, extended Large-cache и reserved-capacity не входят в `production`. Их результаты нельзя складывать с default-ускорением.
7. Огромная таблица `Run-over-run change` не доказывает регрессию текущего кода: почти одновременно замедлились SeferAlloc, mimalloc, System, Vec и все substrate arms. Это явный сдвиг машины/частоты/нагрузки либо несопоставимый сохранённый Criterion baseline.
8. Текущий steady-state churn уже очень силён: 20–33 нс за одну пару `free+alloc`, а на 1024 B предоставленная таблица показывает преимущество 9–10× над mimalloc. Универсальный дальнейший множитель здесь маловероятен. Следующие большие выигрыши будут workload-specific: cold/bulk 16–64 B, batch consumer, medium/page-run arena, realloc growth и cross-thread traffic.

## Находки ревью по приоритету

### P1 — `global_alloc` нельзя использовать для причинного run-over-run вердикта

В приложенной таблице почти все независимые реализации регрессировали одновременно:

- `Vec_push`: mimalloc `+90.01%`, System `+95.91%`, SeferAlloc `+49.43%`;
- большинство System/mimalloc alloc/churn строк — примерно `+50…+90%`;
- одновременно на `+50…+90%` замедлились и `batch_core_warm`, и `scalar_core_warm`, и `scalar_sefer`;
- даже diagnostic workloads и pool sweeps с независимой механикой сдвинулись в ту же сторону.

Изменение SeferAlloc не может причинно замедлить прямые вызовы System и mimalloc почти в одинаковой пропорции. Следовательно, сохранённый `target/criterion` baseline был снят в другой среде: другой частотный режим, background load, power plan, CPU affinity, build identity либо иной общий host-state.

**Следствие:** столбец Criterion `Performance has regressed/improved` для этого запуска надо считать невалидным для атрибуции к коммитам. Допустимы только отношения рук, снятых в одной сессии, и то после проверки симметрии состояния.

Что исправить:

- хранить с baseline точные `HEAD`, dirty state, rustc/LLVM, Cargo features, target triple, CPU model, governor/power plan, affinity и benchmark binary hash;
- не печатать verdict против baseline, если identity не совпадает;
- главный production gate делать парным A/B/B/A в свежих subprocesses;
- для сравнительной таблицы использовать одинаковый бинарник/feature set и нормировать на same-run controls;
- записывать фактическую перестановку arms, а не только случайно выбирать её.

### P1 — decay throttle ускоряет код, но sparse-rate RSS-контракт ещё не доказан

`src/alloc_core/alloc_core_large_cache.rs:30` задаёт stride 64. В `maybe_decay_large_cache` после headroom guard счётчик разрешает реальный `Instant::now()` только на каждом 64-м large-событии (`:412-470`). Механизм соответствует заявленному и устраняет дорогую работу.

Однако формулировки о цене задержки слишком оптимистичны. Код событийный: если процесс делает один Large alloc/free раз в секунду, часы могут не читаться примерно до 32 циклов, а не «на следующем временном интервале». R33-6 измерил один sleep, после которого сделал плотный кластер из N операций. Такой gate показывает `+36 MiB` после 1/8 ops и сходимость к 32/63, но не проверяет много последовательных редких интервалов.

Не доказан тезис «лишнее удержание ограничено одним segment на пропущенный interval». При редком трафике unthrottled-рука способна освобождать по span на каждом прошедшем интервале, тогда как throttled-рука не заглянет в часы десятки секунд. Разрыв может накопить несколько spans.

Следующий обязательный gate:

- 1, 2, 4 и 8 Large-событий на interval;
- 10–40 последовательных interval;
- временной ряд `cached_bytes`, release/decommit count и RSS после каждого interval;
- отдельные alloc-only, dealloc-only и alloc+free профили;
- сравнение фиксированного stride с adaptive policy.

Предпочтительный следующий дизайн — адаптивный, а не ещё больший фиксированный stride: чаще проверять часы при большом excess, реже около headroom; либо запускать clock/decay преимущественно на deposit, где cache действительно растёт, оставив дешёвый механизм idle trim. Убирать alloc-side вызов без измерения нельзя: после последнего deposit последующие alloc могут быть единственными событиями, способными запустить decay.

### P1 — эффект `RemoteFreeRing::cached_head` пока не изолирован

Код действительно переносит common-case full-check с consumer-owned `head` на producer-line `cached_head` (`src/alloc_core/remote_free_ring.rs:77-99`). Это хорошее направление: cross-core Acquire-read на каждом push может быть дорогим.

Но measurement identity в `docs/perf/r32_11_run.json` различается:

- before: `alloc-global alloc-xthread bench-internals`;
- after: `alloc-global alloc-xthread`;
- before использует публичный diagnostic drain-wrapper, after — прямое получение heap и другой drain-вызов.

Owner drain идёт одновременно с timed producers. Поэтому изменение wrapper/codegen/drain cadence может менять ring occupancy и producer timing. Даже правильное направление результата не превращает эти руки в чистый A/B.

Нужен повтор:

1. одинаковые features в обеих руках;
2. один и тот же source-level harness;
3. direct drain helper backport в before;
4. oracle-counters вне timed region;
5. favourable, near-full и overflow режимы отдельно;
6. желательно аппаратные counters cache-line transfers/LLC misses.

Correctness-документ теперь честно указывает предположение о bounded staleness. Но абсолютное «cached head всегда stale-low» зависит от того, что producer не зависнет между чтением `head` и публикацией `cached_head` на полный оборот `u32`. При приблизительно `2^32` consumer advances старое значение модульно может стать stale-high. Это астрономический сценарий и не практический P0, но доказательство является условным. В документации это следует продолжать называть допущением, а не безусловной теоремой.

### P2 — частичная запись Large-header требует более полного invariant pin

`eb2463a` заменяет построение и запись всего 144-byte `SegmentHeader` на четыре записи: `magic`, `large_size`, `large_align`, `bump`; затем отдельно патчится `segment_id` (`src/alloc_core/alloc_core_large.rs:315-350`). Изолированный gate сообщает около 32 Ir экономии на hit — реальный маленький default win.

Сейчас debug-проверки закрепляют лишь carried-forward span/reservation поля. Старый конструктор одновременно переинициализировал и другие поля: owner state/thread-free, deferred link, pool links, live/decommit state, committed frontier, node/ring/virgin metadata. Часть из них действительно должна быть инертна или перезаписана позже, но это должно быть явно перечислено и закреплено.

Особенно важно, что `table.register` публикует segment до последующего `HeapCore::stamp_segment_owner` (`src/registry/heap_core_alloc.rs:302-306`). Для корректного `GlobalAlloc` caller это не даёт наблюдаемой ошибки: pointer ещё не возвращён пользователю. Но защитные пути против stale/invalid free теоретически видят более широкий промежуток со старыми owner/deferred полями.

Рекомендация:

- либо явно сбросить owner/deferred state до register;
- либо добавить полный invariant-list всех полей, которые разрешено carry-forward;
- pin-test должен падать при добавлении нового mutable field в header без классификации;
- исправить устаревший комментарий в `src/alloc_core/segment_header_views.rs:120-153`, который всё ещё утверждает, что cache-hit переписывает весь header и никогда не меняет поля по одному.

Это correctness-hardening и proof maintenance, а не утверждение о найденной эксплуатации.

### P2 — `OWN_CACHE_SIZE=16` не показал latency win

Расширение direct-mapped cache 4→16 действительно изменило hit-rate: для K=4/8 отчёт показывает 0→99.99%. Но R33-5 на полном sweep K=4…64 не получил статистически значимого latency-эффекта; номинальные дельты шумят примерно от -9.1% до +16.2%.

Цена — ещё 12 pointers, около 96 B на `SegmentTable`/heap, плюс одноразовая инициализация. Это допустимо, но пока не является performance win.

Варианты:

- оставить как telemetry-driven capacity reserve, но не учитывать в итоговом ускорении;
- вернуть 4, если реальный workload не показывает Tier-2 victim;
- вместо дальнейшего роста проверить 2-way/4-way set-associative маленький cache: он может убрать pathological address collisions при меньшей площади.

Текущий комментарий `src/alloc_core/segment_table.rs:589-594` всё ещё жёстко говорит `OWN_CACHE_SIZE (4)` и маску `& 3`; после перехода на 16 он устарел.

### P2 — benchmark-ярлыки описывают не тот workload

Есть несколько существенных расхождений текста и реализации.

1. `Cold-direct, no reuse` не является truly cold. `bench_direct_alloc` выделяет 1024 блока и затем освобождает их (`benches/global_alloc.rs:190-205`), но следующая Criterion iteration использует только что освобождённые cache/freelist/pool blocks. Корректное имя: **warm repeated bulk burst, no reuse within allocation half**.
2. Module doc говорит, что SeferAlloc установлен как process `#[global_allocator]` (`:1-18`), но код `:391-394` прямо говорит обратное и вызывает все три `GlobalAlloc` напрямую.
3. `Vec_push` — не `Vec` и не `GlobalAlloc::realloc`. Код вручную делает `alloc(new) + copy_nonoverlapping + dealloc(old)` для каждой руки (`:409-535`). Он не измеряет in-place realloc и не видит R32-3 так, как реальный `Vec`.
4. Табличное «ns per operation» для alloc/churn — фактически ns за пару `alloc+free` или `free+alloc`, потому что время closure делится на 1024 rounds, а не на 2048 allocator calls.
5. Teardown делает 1024 churn pairs плюс дополнительный teardown, но всё равно делится на 1024. Это diagnostic composite unit, не обычный ns/op.
6. Объяснение write-arm через старый `TCACHE_KEY` больше не соответствует текущему tcache: exact oracles всё равно читаются. Оставить payload-touch workload полезно, но rationale надо переписать.

### P2 — состояние трёх allocator arms несимметрично

Перед каждой group очищается только SeferAlloc через `dbg_trim_current_thread` (`benches/global_alloc.rs:23-40`, `:371-376`, `:552-558`). Состояние mimalloc и System живёт весь процесс. Rotation меняет порядок регистрации, но не обеспечивает fresh allocator state и не интерливит arms во времени.

Особенно подозрительный symptom в приложенной таблице: mimalloc 1024 B равен 241.1 нс в churn, но 66.5 нс в churn+teardown, хотя второй workload включает тот же churn и дополнительную работу. Дополнительный teardown не может причинно сделать базовую операцию в 3.6 раза быстрее. Это признак group history, allocator state, order или host noise.

Канонический comparative bench должен запускать отдельный свежий процесс для `(allocator, workload, size, repetition)`. Тогда каждой руке можно действительно установить свой `#[global_allocator]`, исключить асимметрию setup allocations и получить real `Vec`/`Box` behavior.

### P3 — документация и current-state индексы продолжают дрейфовать

Конкретные примеры:

- `segment_header_views.rs:120-153` утверждает full-header rewrite после перехода на targeted writes;
- `segment_table.rs:589-594` всё ещё зафиксирован на cache size 4;
- `benches/global_alloc.rs:1-18` противоречит `:391-394` насчёт `#[global_allocator]`;
- `scripts/bench-table.mjs:48` называет повторяющийся warm burst cold/no-reuse;
- `OPEN_ITEMS.md` местами держит старую цифру в заголовке и новую в теле либо одновременно говорит, что macro-harness отсутствует и позже что он уже создан;
- prose для `PerClass` указывает `virgin_mask` offset 1, тогда как layout assert учитывает alignment и ставит его на offset 2.

Это не косметика: проект принимает решения по накопленному performance ledger, поэтому противоречивый current-state может направить следующую волну на уже закрытую или неверно понятую задачу.

## Что именно ускорила новая волна

| Изменение | Где действует | Измеренный результат | Вердикт ревью |
|---|---|---:|---|
| Decay clock stride | default-reachable при Large cache выше headroom | ~116.4→45.4 нс/call, около -61% | Реальный сильный локальный win; проверить sparse RSS |
| Realloc redundant base/contains removal | default realloc move/promotion | -120 Ir / 16 grows, ~-7.5 Ir/realloc | Реальный маленький win |
| Targeted Large-header writes | default Large-cache hit | -32 Ir/hit | Реальный маленький win; усилить invariants |
| Large-cache occupancy mask | default N=8, opt-in N=40 | -5 Ir/admission; default wall-clock NULL | Микро-win, не пользовательский множитель |
| `OWN_CACHE_SIZE` 4→16 | default ownership checks | hit-rate win, latency statistically NULL | Не считать ускорением |
| Remote ring cached head | cross-thread Small free | заявлено -30…-36% favorable | Перспективно, но A/B не доказателен |
| `PerClass #[repr(C)]` | default layout | isolated perf delta 0 | Correctness/docs fix, не speedup |
| `virgin-zero-skip` stamp removal | только opt-in | -12 Ir на feature-enabled magazine hit | Настоящий opt-in micro-win |
| `DiverseTurnover` | только opt-in policy | workload-specific hit-rate/RSS trade | Продуктовая политика, не default win |
| dual bitmap | не landed | +3.5…+8.2 Ir/op | Правильно отклонено |
| 64+ segment macro harness | bench only | smoke result, без A/B | Инфраструктура, не ускорение |
| Windows reserve/commit decomposition | measurement only | reserve 4.3–4.8%, commit/touch ~95% | Не оптимизировать reserve wrapper |

Production bundle в `Cargo.toml:399` не изменился:

```text
alloc-global + alloc-xthread + alloc-decommit + fastbin +
alloc-segment-directory + primordial-lazy-commit + class-aware-dirty
```

Поэтому opt-in результаты нельзя представлять как ускорение обычного `--features production`.

## Разбор предоставленных benchmark-таблиц

### Что можно считать полезным same-run сигналом

После оговорок о состоянии и коротком профиле таблица показывает форму текущего поведения:

- steady-state non-writing churn: SeferAlloc 23.3–32.8 нс на `free+alloc` pair и быстрее mimalloc на всех четырёх размерах;
- 1024 B churn: очень сильные 9.23×/10.17× относительно mimalloc в этой сессии;
- 64/256 B churn+write: преимущество 1.49×/1.28×;
- repeated bulk burst 16/64 B: главный оставшийся явный gap, 2.37×/2.28× медленнее mimalloc;
- 256 B bulk gap уже только 1.51×, 1024 B SeferAlloc слегка быстрее;
- segment decommit cycle: 3.50× быстрее mimalloc и около 24× быстрее System;
- manual geometric growth: лишь 4% медленнее mimalloc — на уровне, где без paired subprocess gate нельзя говорить о значимой разнице.

Это сравнение форм текущего run, а не доказательство изменения относительно прошлой версии.

### `working_set_cycle` выглядит медленным только из-за единицы измерения

Один batch содержит 64 working sets × 256 `free+alloc` pairs = 16 384 pairs. Нормализация даёт приблизительно:

| Size | ns/batch | ns на `free+alloc` pair |
|---|---:|---:|
| 16 B | 340 100 | 20.8 |
| 64 B | 326 650 | 19.9 |
| 256 B | 290 840 | 17.8 |
| 1024 B | 362 340 | 22.1 |

То есть этот judge показывает сильный steady-state, а не 300-микросекундный провал.

`segment_decommit_cycle` содержит 34 alloc + 34 free; 2310.9 нс/batch — приблизительно 34 нс на allocator call вместе с segment lifecycle. Это также хороший результат.

### Batch-механизм всё ещё является реальным ускорителем

Сравнение **внутри текущего запуска** `scalar_core_warm / batch_core_warm` даёт:

- 16 B: примерно 1.82–2.10×;
- 64 B: примерно 1.55–2.02×;
- 256 B: примерно 1.56–2.08×.

Значит, тёплый substrate batch действительно экономит работу. Исторический production-facing API gate давал меньшие 1.1–1.6×, потому что поверх механизма остаются routing/tcache/API costs. Большой практический выигрыш появится только при реальном потребителе `alloc_batch/dealloc_batch`: object pool, DB page/buffer batch, arena refill, message slab. Для `Box` и обычного `Vec` он равен нулю.

### Почему нельзя оптимизировать по отдельным странным клеткам

Например, write превращает mimalloc 16 B из 24.1 в 19.5 нс, хотя добавляет две volatile stores; churn+teardown делает mimalloc 1024 B намного быстрее простого churn. Эти инверсии сильнее ожидаемого allocator effect и указывают на state/order/noise. Пока benchmark isolation не исправлен, нельзя строить новый алгоритм специально под клетку 16 B write или 1024 B teardown.

## Где ещё возможны сильные ускорения

### 1. P0: сначала сделать сравнительный benchmark причинным

Это не «инфраструктура вместо оптимизации»: сейчас отсутствие чистого gate мешает отличать 5–20% wins от шума и уже породило неверные выводы.

Новый canonical runner:

1. отдельные binaries с настоящим `#[global_allocator]` для SeferAlloc/mimalloc/System;
2. отдельный свежий subprocess для каждого `(allocator, workload, size, repetition)`;
3. парный порядок A/B/B/A либо Latin square;
4. реальные `Vec`, `Box`, `HashMap`, realloc и multi-thread producer/consumer workloads;
5. минимум два профиля: short smoke и long decision gate;
6. identity-bound baseline и same-run controls;
7. отчёт разделяет pair, allocator call, whole transaction и batch units.

Без этого следующая «волна ускорения» с большой вероятностью оптимизирует шум.

### 2. P1: убрать значимую часть Small magazine-hit bookkeeping

Главный наблюдаемый gap — repeated bulk 16/64 B. На production magazine hit сейчас остаются `segment_base_of_ptr` и обновление `MagazineBitmap`; предыдущий изолирующий gate оценивал этот компонент примерно в 12.2 Ir/hit, то есть около 54.5% от изолированного 22.4-Ir hit.

Уже отклонены плохие формы решения: delayed clear, bloom/dual bitmap и прежний run-encoded freelist добавляли scan/sort/state и регрессировали. Их повторять не надо.

Новый архитектурный кандидат — сохранить provenance refill-run без удвоения pointer storage:

- маленькая per-class palette segment bases + compact slot index;
- descriptor для однородного freshly-carved run и fallback на loose pointers для recycled blocks;
- refill-local sidecar, позволяющий очистить bitmap пачкой, когда несколько slots принадлежат одному segment.

Hard gates: 16/64 bulk burst, steady churn, dealloc correctness, cross-thread free, cache footprint и Miri/loom-modelы. Потенциал существенный, но обещать 2× до прототипа нельзя: часть gap находится в TLS/routing и в разнице allocator policies.

### 3. P1: page-run layer для 256 KiB–2 MiB

Прямая promotion `medium-classes` ранее дала огромный alloc/free win, но проиграла realloc примерно в 2111× из-за move leg и поэтому справедливо получила NO-GO. Это показывает, что проблема не в отсутствии size classes как таковых, а в архитектуре их carve/grow.

R10-4 уже описал более сильную альтернативу: 16-MiB page-run layer даёт ориентировочно 11/9/8 объектов там, где wide classes дают 3/2/2, то есть 3–6× плотнее, и может проектироваться с in-place grow/coalescing.

Это самый вероятный оставшийся **архитектурный множитель** для medium/large workload, но только если у проекта есть реальный потребитель диапазона 256 KiB–2 MiB. Нужны:

- trace размеров и realloc cadence реального приложения;
- buddy/run bitmap либо extent tree;
- in-place adjacent-run grow как обязательный P0 design property;
- RSS, fragmentation, alloc/free/realloc и cross-thread gates;
- никакой default promotion до выигрыша по realloc.

### 4. P1: adaptive decay вместо фиксированного stride

Текущий stride доказал, что clock-read был реальной потерей. Следующий шаг — сохранить ~61% local win, но уменьшить sparse retention:

- stride как функция `cached_bytes - headroom`;
- немедленная проверка при крупном deposit/excess;
- экспоненциальное разрежение около headroom;
- coarse monotonic clock, если платформа даёт действительно более дешёвый источник;
- явный idle/budgeted trim.

Это может одновременно ускорить active workload и улучшить RSS semantics.

### 5. P1/P2: реальный realloc и growth-reserve gate

Текущий `Vec_push` никогда не вызывает `realloc`, поэтому он не судит ни R32-3, ни in-place Large grow. Нужно добавить:

- direct `GlobalAlloc::realloc` geometric chain;
- настоящий `Vec` в отдельном allocator-specific binary;
- resize patterns ×1.25, ×1.5, ×2 и shrink/grow oscillation;
- копируемые и untouched payload варианты;
- committed bytes и RSS вместе с latency.

`large-reserved-capacity` остаётся CONDITIONAL-GO и не входит в production. R20-2 показал NULL для medium→Large promotion по механизму: headroom появляется уже после первой copy и не может удешевить её. Возвращаться к feature стоит лишь на workload, где следующий grow действительно укладывается в уже reserved span. Более перспективный вопрос — адаптивный growth factor и сохранение/компаунд headroom через relocations, а не blanket promotion существующей формы.

### 6. P2: batch API только вместе с потребителем

Механизм даёт 1.5–2.1× на тёплом AllocCore и исторически 1.1–1.6× через production surface. Следующий этап должен быть не ещё одним synthetic ceiling, а один реальный consumer pilot:

- интеграция в arena/slab/object-pool;
- batch 8–64, mixed lifetimes и partial failure;
- end-to-end latency/throughput и RSS;
- API остаётся hidden до доказанного потребителя.

### 7. P2: частичный trim вместо all-or-nothing cliff

R32-1 полезно исправил passive no-bind semantics, но cost gate показывает важную цену: полный `trim_current_thread` около 24.2 ms для 4×32 MiB и следующий burst около 65.2 ms против ~0.8 ms без trim — примерно 83× cold penalty в обмен на 128 MiB idle RSS.

Нужна более практичная политика:

- `trim_current_thread_to_headroom(bytes)`;
- time/segment budget на один trim;
- oldest/largest-first release;
- background cooperative trim вне latency-critical request;
- явные telemetry counters «released bytes / pause / refill penalty».

Это не ускорит hot path, но уберёт большой latency cliff проекта.

### 8. P2: macro judge должен создавать нужное состояние, а не только высокий count

Новый 64+ segment harness — полезная инфраструктура, но 80 dedicated Large segments плюс один Small churn class не воспроизводят fragmented/holey multi-class Small directory state. Он ещё не перепроверил X5/T10/R1/R15-1.

Перед новым bitmap/hint проектом нужен вариант с:

- 64+ живыми Small segments;
- несколькими классами и holes;
- controlled directory misses, remote frees и pool transitions;
- production features;
- profile сначала, A/B затем.

Production directory уже убрала главный O(S) scan, поэтому новый глобальный индекс без актуального victim почти наверняка добавит state дороже сохранённой работы.

## Что улучшить в коде и проекте

### Код

- Закрепить полный state contract partial Large-header reuse; обновить stale comments.
- Добавить sparse multi-interval decay test/gate и отделить «ops late» от «seconds/RSS late».
- Повторно измерить remote-ring shadow с идентичным build shape.
- Рассмотреть set-associative own-cache вместо механического увеличения direct-map.
- Не добавлять новый hot-path metadata без counterfactual red-before/green-after и footprint gate.
- Держать rejected designs в deny-list следующего round, чтобы не повторять delayed clear, dual bitmap и старый run-freelist под новым названием.

### Benchmarks

- Исправить `global_alloc` module docs и названия таблиц.
- Разделить workloads: true cold first heap; warm bulk burst; steady churn; teardown diagnostic; real realloc; real container.
- Сделать единицы измерения явными: `ns/pair`, `ns/call`, `ns/transaction`, `ns/batch`.
- Не использовать старый Criterion baseline при несовпадении identity.
- Сохранять rotation/order и проводить несколько независимых permutations.
- Вынести каждую allocator arm в subprocess и настоящий `#[global_allocator]`.
- Pool-cap timing не публиковать как throughput; там signal — counter deltas, как уже отмечено в комментарии.

### Документация и процесс

- За одну волну добавлено около 73 тысяч строк `docs/perf`. Raw logs лучше хранить как сжатые CI artifacts/object-store с hash, а в Git — summary CSV, manifest, команды, identity и минимальный воспроизводимый excerpt.
- Добавить round manifest: production source commits, opt-in source commits, bench-only/docs-only, default feature impact, measured wall/Ir/RSS delta и конечный verdict.
- `OPEN_ITEMS` должен быть current-state index, а история — archive. Закрытые/null/rejected пункты не должны выглядеть активными из-за старого заголовка.
- После runtime commit автоматически проверять stale numeric literals/phrases в docs (`OWN_CACHE_SIZE (4)`, whole-header rewrite, old feature composition).
- Продолжить хороший R33 procedural контроль: после push подтверждать CI именно на landing SHA, а не на локальном предшественнике.

## Предлагаемый Round 34

| Приоритет | Задача | Exit criterion |
|---|---|---|
| P0 | Новый subprocess comparative harness | Same-host A/B/B/A, real global allocators, identity-bound baseline; controls стабильны |
| P0 | Sparse multi-interval decay gate | Временной ряд RSS/cache на 1/2/4/8 ops per interval; доказан bound или исправлена политика |
| P1 | Remote-ring clean re-gate | Идентичные features/harness/drain path; подтверждён producer win без overflow regression |
| P1 | Real realloc/Vec suite | Настоящий `realloc`, geometric factors, payload touch/copy, RSS |
| P1 | Small magazine provenance design/prototype | Уменьшены base+bitmap Ir без cache-footprint/churn/correctness regression |
| P1 | Page-run design gate | Real medium workload victim; in-place grow является обязательным свойством |
| P2 | Partial trim API prototype | Существенно меньше 24 ms pause/65 ms refill cliff при контролируемом RSS |
| P2 | Один end-to-end batch consumer | Пользовательский throughput win, не только substrate ceiling |
| P2 | Docs/current-state cleanup | Ноль известных противоречий из списка этого ревью |

## Итоговый ответ

Проект уже прошёл фазу, где каждое новое микроизменение даёт большой универсальный прирост. Steady-state small churn находится около локального максимума и в предоставленном run уже конкурентнее mimalloc. Новая волна **реально ускорила** expensive decay path и слегка подчистила realloc/Large-cache hit, но `OWN_CACHE_SIZE`, occupancy mask и layout fix не дали заметного default wall-clock, а shadow-head ещё нуждается в чистом измерении.

Радикальные оставшиеся возможности есть, но они не универсальны:

- 1.5–2.1× для batch-aware потребителя;
- потенциально крупный выигрыш page-run arena на 256 KiB–2 MiB;
- существенное сокращение cold/bulk gap 16/64 B через новый provenance-oriented magazine design;
- сильный cross-thread выигрыш, если clean gate подтвердит remote shadow-head;
- adaptive decay, сохраняющий clock-read win без длинного sparse RSS хвоста.

Первое действие следующей волны — не ещё один hot-path patch, а причинный benchmark harness. Без него большая часть приложенного run-over-run списка измеряет состояние машины, а не качество аллокатора.
