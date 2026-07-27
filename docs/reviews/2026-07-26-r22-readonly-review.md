# Read-only review Round 22 (`b6af12d..610f915`)

**Дата:** 2026-07-26  
**Режим:** только чтение истории Git, диффов и файлов. Сборка, тесты,
бенчмарки, примеры и скрипты не запускались. Числа ниже либо взяты из
закоммиченных артефактов, либо вычислены непосредственно из них; фактическое
исполнение не перепроверялось.

## 0. Краткий вердикт

### Ускорили ли этой волной production-код?

**Практически нет.**

Диапазон содержит 18 коммитов, 35 изменённых файлов и `+5924/-238` строк, но
production-состав feature-флагов не изменён, а изменения в `src/` сводятся
главным образом к:

- исправлению hardened-проверки layout для Large (теперь проверяются и size,
  и align);
- диагностическому счётчику rejected Large frees;
- measurement-only hook для `contains_base`;
- комментариям к OPT-H и небольшим тестовым/диагностическим поверхностям.

То есть Round 22 — это прежде всего **correctness, CI, измерения и
документация**, а не волна ускорения. Из неё нельзя честно вывести
before/after-ускорение production runtime: оптимизирующей реализации в
production-пути почти не было.

### Была ли волна полезной?

**Да.** Она:

- закрыла реальный hardened correctness gap: Large layout теперь сверяется
  полностью, включая alignment;
- добавила CI-комбинацию, в которой раньше важная ветка не исполнялась;
- сделала часть тестов хотя бы компилируемой во всех relevant feature-комбо;
- сформировала долговечный correctness/CI debt index;
- подтвердила, что дальнейший поиск ускорения не закончен: горячий free и
  cold carve/recycle всё ещё имеют заметный запас.

Но два главных численных вывода Round 22 пока нельзя использовать как точные
основания для архитектурных решений, а remap-направление закрыто
преждевременно.

## 1. Что просмотрено

- Git-диапазон: `b6af12d..610f915` (18 коммитов Round 22).
- Полный список и статистика изменённых файлов.
- Runtime-диффы в `src/alloc_core` и `src/registry`.
- Новые/изменённые CI-строки и тесты.
- `R22_15_MIMALLOC_IR_ARM_GATE.md` и raw/CSV companion artifacts.
- `R22_16_PROMOTION_REMAP_DESIGN.md`.
- `R22_17_CONTAINS_BASE_FREE_HOT_PATH_GATE.md` и raw/CSV artifacts.
- `R22_18_MEDIUM_CLASSES_FATE_DECISION.md`.
- `docs/perf/OPEN_ITEMS.md` и `docs/CORRECTNESS_OPEN_ITEMS.md`.
- Изменения `scripts/iai.mjs`, `benches/perf_gate_iai.rs`, `Cargo.toml`.

## 2. Findings

### P1 — «18.6% стоимости free приходится на `contains_base`» измерено не в изоляции

В `dealloc_contains_base_probe_only_16b` измеряется не один
`SegmentTable::contains_base`. Цикл выполняет:

1. `dbg_segment_base_of_ptr(ptr)`;
2. `dbg_contains_base(base)`;
3. два вызова measurement hooks;
4. `black_box`.

Это прямо видно в `benches/perf_gate_iai.rs:232-239`. При этом отчёт и
`OPEN_ITEMS.md` превращают `1101 / 5920 = 18.6%` в точную долю
`contains_base`.

Корректная интерпретация текущего числа:

> **18.6% — стоимость изолированного routing-prefix harness
> (`segment_base_of_ptr + contains_base + measurement-call/black_box
> overhead`) относительно отдельного free harness.**

Это не точная стоимость одного `contains_base`. Более того, утверждение, что
18.6% является «консервативной нижней границей», не доказано:

- много горячих сегментов действительно может увеличить число hash misses;
- но measurement-only вызовы и `black_box` могут завысить текущий Tier-1
  результат;
- компиляторский контекст отдельного публичного hook отличается от
  `#[inline(always)]` production-цепочки
  `HeapCore::dealloc -> dealloc_routing -> contains_base`.

**Что сделать:** добавить как минимум четвёртую руку
`segment_base_probe_only`, затем считать:

```text
contains_only =
    (base_plus_contains_arm - prealloc_arm)
  - (base_only_arm          - prealloc_arm)
```

Лучше — сделать counterfactual A/B внутри той же production-функции либо
использовать function-level Callgrind attribution, чтобы сохранить одинаковый
inline/codegen context. До этого заменить в `OPEN_ITEMS.md` «18.6% нижняя
граница contains_base» на «18.6% routing-prefix upper-envelope на
single-segment hit workload».

### P1 — сравнение с mimalloc 1.3–2.4× опирается на неэквивалентное вычитание

Round 22 вычитает из каждого Sefer-бенча весь
`large_alloc_free_cycle = 3308 Ir`, а из каждого mimalloc-бенча весь
`mimalloc_bootstrap_proxy = 13050 Ir`.

Оба proxy включают не только bootstrap, но и **настоящий 4 MiB alloc+free**.
Сам `scripts/iai.mjs:102-109` признаёт это over-estimate. Для
внутриаллокаторного regression signal это может быть приемлемой стабильной
аппроксимацией. Для cross-allocator ratio ошибка уже не общая: из двух сторон
вычитаются разные операции с разной реализацией и неизвестной разной
стоимостью.

Особенно чувствителен hot churn:

- raw Sefer: `8051 Ir`;
- raw mimalloc: `16629 Ir`;
- после вычитания proxy остаются соответственно `4743` и `3579 Ir`;
- заявленный ratio `1.326` строится на двух небольших остатках после большого,
  allocator-specific вычитания.

Это не означает, что mimalloc обязательно медленнее или что gap отсутствует.
Это означает, что точные коэффициенты **1.326–2.430 пока не идентифицированы
надёжно**. Cold/recycle-сигнал, где остаток больше и делитель 256/512,
правдоподобнее hot-сигнала, но также нуждается в корректном warm gate.

**Что сделать:**

- прогреть каждый allocator внутри его собственного процесса;
- измерить одинаковый второй/третий цикл;
- либо использовать `Ir(2N) - Ir(N)` для того же workload: общая
  инициализация сократится алгебраически без постороннего 4 MiB proxy;
- держать одинаковые N, layout, порядок alloc/free и состояние cache;
- публиковать raw ratio и warm marginal ratio рядом;
- только после этого заявлять точный cross-allocator множитель.

До повторного гейта безопасная формулировка: **«закоммиченный Callgrind arm
показывает вероятный существенный instruction gap, особенно на
cold/recycle; точный размер gap требует warm matched gate».**

### P1 — Linux sub-region remap ошибочно закрыт как NO-GO

`R22_16_PROMOTION_REMAP_DESIGN.md` содержит внутреннее противоречие:

- в headline (`:56`) говорится, что у medium-объекта обычно нет
  page-aligned границ;
- ниже (`:295-331`) документ правильно выводит, что все medium class sizes
  кратны 4 KiB, `align_up(bump, block_size)` делает начало page-aligned, а
  конец также page-aligned;
- далее документ вводит необходимость promotion-time проверки, «не занял ли
  sibling страницы старого объекта».

Последний переход логически неверен. Carve-диапазоны не перекрываются. Если
`[start, end)` объекта page-aligned в момент carve, последующие bump-carves
начинаются не раньше `end` и **не могут позднее занять страницы внутри
`[start, end)`**. Будущий объект после `end` может занять соседние страницы
или пространство, в которое первый объект хотел бы вырасти, но это не мешает
переместить уже существующие `old_size` страниц в новый отдельный mapping.

Также whole-segment base-stability не является блокером именно для
**sub-region** remap: документ сам признаёт это в §3.3. Старый Small segment
остаётся по прежнему base, siblings не перемещаются, а realloc возвращает
новый pointer, который может быть оформлен как новый Large/extent.

Реальные нерешённые проблемы остаются:

- Linux-only primitive и новая FFI-поверхность;
- корректное представление unmapped/reserved hole в старом Small segment;
- недопущение помещения старого блока в обычный BinTable/free-list;
- release/decommit сегмента с hole;
- регистрация destination как отдельного Large/extent;
- Windows требует существенно иной mapping model.

Это серьёзные задачи, но они не доказывают NO-GO. На Linux это всё ещё
кандидат на **асимптотическое устранение `O(old_size)` memcpy** для
256 KiB–1 MiB realloc — потенциально самое радикальное ускорение, оставшееся
в medium-профиле.

**Вердикт ревью:** переоткрыть Linux sub-region remap как
`CONDITIONAL-GO / design+prototype`; оставить whole-segment remap и текущий
Windows anonymous-`VirtualAlloc` путь как NO-GO.

### P2 — новая CI-строка сознательно включает известный ~1/3 flaky test

`00fb53c` добавляет полный:

```text
cargo test --features "hardened medium-classes" --no-fail-fast
```

При этом commit message и `docs/CORRECTNESS_OPEN_ITEMS.md:63+` фиксируют
известный flake `canary_survives_promotion_and_free_leaves_no_leak`,
воспроизводившийся примерно в одном из трёх прогонов.

Покрытие нужной branch-A стало лучше, но CI reliability стало хуже: известный
частый flake превращает полезную строку в источник случайных красных сборок и
приучает игнорировать CI.

**Что сделать немедленно:**

1. либо сначала изолировать/исправить flaky stats-test;
2. либо временно запускать в новой строке только
   `--test regression_hardened_large_kind_own_free`;
3. после исправления вернуть full-suite combo.

### P2 — `hardened medium-classes` всё ещё не clippy-clean

Новая test-строка закрывает execution coverage, но
`docs/CORRECTNESS_OPEN_ITEMS.md:121+` всё ещё перечисляет 11
`-D warnings` ошибок для этой комбинации. Следовательно, feature-combo
исполняется, но не имеет такого же compile-quality gate, как основные
профили.

Это не runtime-баг, однако для feature-heavy unsafe allocator cfg-долг опасен:
неиспользуемые imports/helpers часто указывают на разъехавшиеся predicates.

**Что сделать:** выровнять cfg predicates item-by-item, затем добавить
`cargo clippy --all-targets --features "hardened medium-classes" -- -D warnings`.

### P2 — буквальные doc-tripwires размножают источник истины

`272478c` добавляет ещё три литерала (`25_088`, `28_160`, `29_696`) в
`tests/no_stale_doc_references.rs:415+`, чтобы проверять такие же литералы в
другом тесте и prose.

Это ловит случайный drift, но теперь изменение legitimate constants требует
синхронно обновлять:

1. production constants;
2. sidecar sizing test;
3. новый prose snapshot test;
4. сам prose.

Тест проверяет наличие строк во всём файле, а не конкретную таблицу/секцию.
Неверный старый literal может остаться где-то ещё и дать ложный green.

**Лучше:** вычислять размеры из экспортированного test-only descriptor и
генерировать/проверять одну структурированную таблицу. Если prose должен быть
snapshot, парсить ограниченный маркированный блок, а не `text.contains` по
всему файлу.

## 3. Положительная оценка runtime/correctness правок

### `de5e0dc` — правильный и полезный fix

`large_layout_consistent(base, layout)` теперь сравнивает:

- `large_size_at(base)` с нормализованным `layout.size()`;
- `large_align_at(base)` с `layout.align()`.

Оба вызывающих пути переведены на полный `Layout`. Это корректно сужает
окно stale-reuse/contract-violation, не добавляя стоимости plain production
small hot path: проверка относится к hardened/foreign Large routing.

Остаточное ограничение честно сохраняется: повторное использование с теми же
size **и** align не обнаруживается; полноценной защитой от double free это не
является и не должно так называться.

### `91510ce` — разумное улучшение compile coverage

Перевод тестовых функций с duplicated `#[cfg]` на runtime
`dbg_promotion_compiled()`:

- заставляет тела компилироваться в большем числе feature-комбо;
- уменьшает риск тихого syntax/type drift;
- при истинном predicate тесты реально выполняются благодаря отдельной
  `hardened medium-classes` CI-строке.

Ранний `return` под false predicate, конечно, не даёт behavioral coverage, но
это честно и компенсируется специализированной true-predicate строкой.

### `252a06d` — приемлемая диагностика

Счётчик rejected hardened Large frees инкрементируется только под
`alloc-stats`, поэтому plain production cost не добавлен. Один общий счётчик
для двух cfg-взаимоисключающих веток соответствует внешнему смыслу события.

### Process/documentation improvements

`docs/CORRECTNESS_OPEN_ITEMS.md`, raw-log boundary rule, исправление
commit-vs-RSS терминологии, LCM closure OPT-H и явное решение по судьбе
`medium-classes` — полезные улучшения процесса. Они уменьшают повторное
исследование уже закрытых направлений.

## 4. Где ещё возможно сильное ускорение

### 4.1 P0 — сначала починить перф-атрибуцию

Сейчас проект знает, что gap, вероятно, есть, но плохо знает, **где именно**
он находится.

Нужны ортогональные matched arms:

- alloc-only hot magazine hit;
- free-only hot magazine push;
- free routing base-mask only;
- Tier-1 `contains_base` hit only;
- Tier-2 `contains_base` miss/hash only;
- free после routing: magazine bitmap oracle, alloc bitmap oracle, mark;
- cold refill/find/carve отдельно;
- recycle freelist-pop отдельно.

Для каждого — warm `N`/`2N` delta вместо вычитания постороннего bootstrap
proxy. Это дешёвая работа, которая не даст следующей волне оптимизировать
не тот механизм.

### 4.2 P1 — hot free routing: реалистичный потолок порядка 10–20% free

Каждый production free под `alloc-xthread` сначала вычисляет segment base и
проверяет membership (`heap_core_xthread.rs:750-771`). Уже есть хороший
4-entry cache (`segment_table.rs:98,455`), поэтому простое увеличение cache
4→8/16 не ускорит single-hot hit и не является радикальным решением.

Варианты для честного A/B:

- улучшить codegen самого routing prefix после правильного разложения
  base-mask/probe/call overhead;
- проверить 1-entry last-base fast path перед 4-entry direct map, но только
  если его invalidation остаётся структурно связанной с unregister/recycle;
- проверить, нет ли повторной membership/metadata работы ниже по цепочке, и
  протащить уже валидированный base/token до конечного действия;
- исследовать отдельный **thread-affine fast profile**, где magazine не
  требует `alloc-xthread`, если продукт готов явно запретить cross-thread
  free. Это стратегический opt-in контракт, не замена safety-first
  production.

Полное удаление текущего measured routing-prefix envelope даёт максимум
около 18.6% именно free-half в измеренном сценарии, а не 2× всего allocator
churn. Это сильная двузначная оптимизация, но не «1000×».

### 4.3 P1 — два bitmap oracle на каждом magazine free

После routing горячий small free читает:

- `magazine_bitmap().is_in_magazine(off)`;
- `alloc_bitmap().is_free(off)`;
- затем пишет magazine bitmap и tcache slot.

Это цена safety-first double-free защиты. На фоне mimalloc она может быть
существенной частью оставшегося instruction gap.

Нельзя просто удалить проверки из production без стратегического решения.
Но стоит измерить:

- стоимость каждого oracle отдельно;
- можно ли объединить два состояния в одно слово/одну cache line без потери
  различения «в magazine» и «уже flushed free»;
- можно ли обновлять оба бита одной RMW-операцией;
- действительно ли оба чтения нужны на всех non-hardened contract-valid
  путях или один из инвариантов уже логически следует из другого состояния.

Если объединение сохраняет M2-гарантии, это может закрыть заметную часть hot
gap. Если нет — документировать эту разницу с mimalloc как осознанную цену
за защиту.

### 4.4 P1 — cold carve/recycle: потенциально самый большой общий резерв

Даже с методологической оговоркой Round 22 последовательно показывает
наибольший gap на cold/recycle, а не на magazine hit. Следующий gate должен
разделить:

- allocation refill;
- directory lookup;
- `carve_batch`/bitmap initialization;
- virgin marking/zero policy;
- free routing;
- flush в BinTable;
- subsequent freelist drain.

Цель — найти один доминирующий component, а не продолжать scalar
микротюнинг. Если после корректного warm split `carve/refill` остаётся
примерно 2× от mimalloc, архитектурный кандидат — page-local metadata/run
layer для tiny classes, уменьшающий число bitmap/table touches на блок.
Это большой redesign, поэтому он оправдан только после attribution.

### 4.5 P1/P2 — переоткрыть Linux `mremap` sub-region prototype

Для opt-in `medium-classes` это единственный видимый путь убрать сам
`O(old_size)` copy, а не немного удешевить его окружение.

Минимальный design gate:

1. только Linux;
2. только page-aligned medium block;
3. remap ровно `[ptr, ptr + old_block_size)`;
4. destination оформляется как отдельный Large/MediumExtent;
5. old range заменяется безопасным недоступным reservation/hole marker;
6. обычный BinTable больше никогда не получает этот старый block;
7. segment teardown знает о holes;
8. при любой ошибке — старый memcpy move-leg.

Сначала нужен correctness prototype и syscall-vs-memcpy crossover по размерам
256/320/512/768/1024 KiB. Windows оставить на обычном move-leg.

Это может дать кратный выигрыш именно на realloc medium buffers, но не
ускорит default production, пока `medium-classes` остаётся opt-in.

### 4.6 P2 — Batch API: ускорение существует только при наличии потребителя

Batch API уже реализован и ранее показал выигрыш, но без реального caller его
эффект на Box/Vec равен нулю. Следующий шаг здесь не ещё один microbench, а:

- найти downstream consumer;
- измерить реальный batch-size distribution;
- стабилизировать минимальную API-поверхность только после этого.

## 5. Что улучшить в проекте

### Немедленно

1. Исправить или изолировать известный flaky test из новой CI-строки.
2. Сделать `hardened medium-classes` clippy-clean и поставить `-D warnings`.
3. Исправить формулировку `contains_base = 18.6%` в отчёте/open-items.
4. Пометить mimalloc ratios как provisional до matched warm gate.
5. Переоткрыть Linux sub-region remap; исправить противоречивый R22-16.

### Следующая перф-волна

1. Один round только на attribution, без production changes.
2. Затем не более двух A/B implementations против найденных dominant costs.
3. Обязательные judges:
   - Ir;
   - Estimated cycles/cache signal;
   - wall-clock paired A/B;
   - RSS/commit;
   - feature-combo correctness.
4. Не считать docs/bench/test commits «ускорением кода».

### Поддерживаемость

- Сократить многосотстрочные исторические комментарии в hot source files:
  держать локально только invariant и ссылку на design doc.
- Не размножать derived literals между production, тестами и prose.
- Отделять три статуса в CHANGELOG/round summary:
  `runtime change`, `measurement`, `correctness/process`.
- Для каждого headline ratio хранить формулу и проверку чувствительности к
  bootstrap/setup модели.
- CI-матрица должна описываться через проверяемые capability predicates,
  насколько это возможно, а не через всё новые вручную подобранные комбинации.

## 6. Рекомендуемый план Round 23

### P0 — measurement repair

1. R23-1: base-only arm и корректный `contains_base` decomposition.
2. R23-2: warm `N/2N` matched Sefer-vs-mimalloc gate.
3. R23-3: split hot alloc / hot free / cold alloc / cold free.

### P0 correctness/CI

4. R23-4: исправить stats-based flaky test либо изолировать его процессом.
5. R23-5: закрыть 11 clippy cfg-debt ошибок и включить gate.

### P1 implementation candidates — только после R23-1..3

6. R23-6: один safe hot-free prototype по реально доминирующему component.
7. R23-7: Linux sub-region `mremap` correctness design/prototype с fallback.
8. R23-8: A/B результата; production promotion только при двузначном
   wall-clock выигрыше без ослабления memory-safety invariants.

## 7. Итог

Round 22 **не был новой волной радикального runtime-ускорения**. Он был
качественной волной hardening, coverage и постановки новых вопросов.

Самые важные результаты ревью:

- correctness действительно улучшился;
- точные `1.3–2.4× vs mimalloc` и `18.6% contains_base` пока переоценены
  методологически;
- hot free имеет правдоподобный двузначный резерв;
- cold carve/recycle остаётся главным общим кандидатом, но требует
  attribution;
- Linux sub-region remap ошибочно закрыт и остаётся наиболее радикальным
  асимптотическим кандидатом для medium realloc;
- прежде следующей большой реализации нужно сделать короткую волну
  измерительных исправлений.

