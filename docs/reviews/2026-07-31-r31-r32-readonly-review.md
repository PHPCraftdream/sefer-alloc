# Read-only review новых волн R31/R32

**Дата:** 2026-07-31  
**Диапазон:** `14a9ef3..e124a48`  
**Объём:** 31 коммит  
**Режим:** только чтение истории Git и файлов. Сборка, тесты, clippy, Miri,
Kani, benchmarks, examples и проектные scripts в ходе этого ревью не
запускались. Единственная запись — этот отчёт.

## Короткий вердикт

Код действительно стал лучше, но ответ на вопрос «ускорили ли мы allocator»
зависит от того, что именно считать ускорением.

- **Default production hot path автоматически быстрее не стал.** Состав
  feature `production` в `Cargo.toml` не изменился. `virgin-zero-skip` и
  `large-cache-extended`, где измерены большие выигрыши, по-прежнему opt-in.
- **Появилась реальная production-возможность резко снизить удерживаемую
  память:** публичный `SeferAlloc::trim_current_thread()`. В измеренном
  burst/idle сценарии он снизил idle RSS примерно с 131.2 MiB до 3.2 MiB и
  commit примерно с 145.9 MiB до 1.6 MiB. Это сильное улучшение управления
  памятью, но не ускорение alloc/free: вызов trim выполняет дополнительную
  работу и намеренно делает следующий burst холодным.
- **`virgin-zero-skip` показал настоящий большой выигрыш** на production-layer
  пути для virgin `alloc_zeroed` без последующего чтения всей области:
  примерно 89–98.6%. Но feature не promoted, поэтому default-пользователи
  выигрыша пока не получили.
- **`large-cache-extended` снова показал сильный turnover-win:** hit rate
  33.3% -> 100%, около 386 us среднего выигрыша в опубликованном paired gate.
  Это полезный opt-in механизм для разнообразных Large-размеров, но новый
  narrow-working-set gate содержит серьёзные ошибки и не разрешает promotion.
- Profile API стал существенно честнее: независимые
  `SmallPoolPolicy`/`LargeCachePolicy` устранили старое скрытое связывание
  двух разных retention-политик. Это качество API/configuration, не
  автоматическое ускорение.

Главная новая находка ревью: **P0 soundness-дефект в safe diagnostic API
`ReservedSmallSegment`**. Тип защищает от подделки handle и повторного
потребления одного значения, но не связывает handle с создавшим его
`AllocCore`. Safe-код с `bench-internals` может передать reservation от
`core_a` в `core_b.dbg_decomp_release(...)`, повредить pool/table metadata и
в конечном счёте получить use-after-release/unmapped-memory access. Два
внутренних ревью R31/R32 этот сценарий пропустили.

## 1. Что изменилось в диапазоне

Runtime/API изменения:

1. `ReservedSmallSegment` заменил bare pointer в decomposition hook.
2. Старый bundled `Profile` разделён на две независимые policy-оси.
3. Добавлены Windows recommit hooks для корректного decomposition example.
4. `dbg_trim_current_thread` promoted в публичный
   `SeferAlloc::trim_current_thread()`.
5. После внутреннего R32-review исправлены три P1 вокруг trim:
   документация о TLS binding/fallback, потерянный fastbin-only flush в
   diagnostic alias и устаревшие пользовательские документы.

Measurement/project изменения:

1. Production-layer re-gate `virgin-zero-skip`.
2. Headroom crossing-regime gate.
3. Pool-cap sweep 4/8/16/32.
4. Reverification `large-cache-extended`, narrow timing и multi-heap RSS.
5. Structural/semantic gate-report verifier, commit-prefix lint и CI-clippy
   matrix consistency test.
6. Review-response волны, которые исправили ряд data/doc/process дефектов.

Текущий production bundle остался:

```toml
production = [
  "alloc-global",
  "alloc-xthread",
  "alloc-decommit",
  "fastbin",
  "alloc-segment-directory",
  "primordial-lazy-commit",
  "class-aware-dirty",
]
```

В нём нет ни `virgin-zero-skip`, ни `large-cache-extended`. Следовательно,
заявлять ускорение default allocation throughput по этой волне нельзя.

## 2. P0: `ReservedSmallSegment` не привязан к владельцу

### Наблюдение

`src/alloc_core/reserved_small_segment.rs` хранит только:

```rust
pub struct ReservedSmallSegment {
    base: *mut u8,
}
```

Конструктор закрыт от внешнего кода, handle не `Copy`/`Clone`, а
`into_base(self)` потребляет его. Это действительно закрывает два класса
ошибок:

- внешний код не может создать handle из произвольного pointer;
- один и тот же handle нельзя штатно передать в release дважды.

Но публичная safe-функция
`AllocCore::dbg_decomp_release(&mut self, handle: ReservedSmallSegment)`
принимает handle, созданный **любым** `AllocCore`. В типе нет owner identity,
lifetime, token или membership proof, связывающего reservation с конкретным
`self`.

Допустимый safe Rust-код под feature `bench-internals`:

```rust
let mut core_a = AllocCore::new().unwrap();
let mut core_b = AllocCore::new().unwrap();

let h = core_a.dbg_decomp_reserve_and_keep().unwrap();
core_b.dbg_decomp_release(h); // safe API, но чужой owner
```

### Почему это опасно

`dbg_decomp_release` передаёт base в
`self.release_or_pool_empty_segment(base)`, то есть использует pool,
directory и `SegmentTable` **принимающего** `core_b`, а не владельца
reservation.

Если в pool `core_b` есть место, foreign base записывается в intrusive pool
`core_b`, при этом остаётся зарегистрированным в table `core_a`. `core_a`
может продолжить обращаться к segment, а `core_b` позже может извлечь и
освободить его.

Если segment сразу release-ится, `core_b.table.recycle(base)` читает header
foreign segment. Defensive tail `SegmentTable::recycle` освобождает OS
reservation даже когда stamped slot не принадлежит таблице `core_b`.
Регистрация в `core_a.table` при этом остаётся stale и указывает на уже
освобождённое отображение. Последующий lookup/drop/reuse у `core_a` может
читать unmapped/reused address.

Это не default-production surface, потому что API gated на
`bench-internals`, но это всё равно soundness-дефект safe public API и
потенциальное повреждение allocator metadata.

### Исправление

Минимально безопасный hotfix:

1. Снова сделать `dbg_decomp_release` `unsafe fn`.
2. Вернуть явный контракт: handle должен происходить от paired reserve на
   том же `AllocCore`, segment должен быть live и не освобождён.
3. Добавить two-core counterfactual test, который показывает, что safe
   cross-core release больше невозможен.

Предпочтительный окончательный дизайн:

- scoped closure/RAII API, например
  `with_reserved_small_segment(&mut self, |reservation| ...)`, где владелец
  удерживается на всё время reservation и cleanup выполняется тем же core;
  либо
- stable owner token внутри handle плюс release-build проверка принадлежности
  **до** pool/decommit/table операций, с отказом при mismatch.

Привязывать owner к адресу `AllocCore` недостаточно надёжно: сам `AllocCore`
может перемещаться. Нужна стабильная identity или структурная lifetime-связь.

Следует переоткрыть запись в `docs/CORRECTNESS_OPEN_ITEMS.md`, которая сейчас
считает typed handle завершённым исправлением. Внутренний
`docs/reviews/2026-07-31-r31-full-review.md` проверил unforgeability и
single-consumption, но ошибочно экстраполировал их на весь старый unsafe
контракт.

## 3. Проверка performance-утверждений

### 3.1 `virgin-zero-skip`: сильный реальный кандидат, пока opt-in

R31-0 исправил главный недостаток предыдущего gate: измерение теперь идёт
через production-layer `HeapCore::alloc_zeroed`, включая magazine path, а
fresh heap берётся из registry на каждую repetition.

Опубликованный notouch результат:

| Размер | OFF | ON | Выигрыш |
|---:|---:|---:|---:|
| 4 KiB | 2342 ns | 257 ns | около 89% |
| 16 KiB | 11230 ns | 565 ns | около 95% |
| 64 KiB | 52117 ns | 754 ns | около 98.6% |
| 128 KiB | 97198 ns | 1355 ns | около 98.6% |

Механизм выглядит настоящим: ON показывает полную активацию virgin path, а
retention mask соответствует ожидаемому magazine поведению. Повтор сохраняет
направление и порядок выигрыша.

Ограничения:

- one-byte/full-touch результаты меняют знак между запусками;
- recycled controls также нестабильны по знаку;
- gate использует `HeapCore`, а не полный `SeferAlloc`/`GlobalAlloc` entry
  point, хотя сама feature-логика действительно живёт на HeapCore-слое;
- fresh heaps остаются заняты до завершения процесса, поэтому OFF-рука,
  физически обнуляющая значительно больше страниц, может сильнее менять
  последующие memory-pressure условия;
- утверждение отчёта, что touch-heavy сценарии составляют «большинство», не
  подтверждено workload telemetry.

Вывод: **не NO-GO, а незавершённое promotion decision.** Механизм уже даёт
радикальный выигрыш на подходящем пути. Следующий gate должен проверять не
«угадает ли caller, будет ли память потом прочитана», а цену feature на
проигрываемом фронте:

1. regular `alloc`, recycled `alloc_zeroed` и `realloc` A/B;
2. deterministic instruction-count/branch gate;
3. subprocess-per-cell application/calloc workloads;
4. только затем explicit promotion в `production`, если worst-case overhead
   практически нулевой.

Это самый близкий к shipping сильный speedup: реализация уже существует, а
выигрыш в выигрышном режиме на порядок больше обычного micro-tuning.

### 3.2 `large-cache-extended`: turnover-win реален, narrow gate невалиден

Обновлённый turnover A/B поддерживает прежний вывод:

- hit rate 33.3% -> 100%;
- paired sign 20/20 в пользу ON;
- `t = 127.776`;
- средний `OFF - ON` около 385.7 us.

Это сильное свидетельство для workload с более чем восемью повторно
используемыми Large-размерами. Но R31-3 одновременно пытается закрыть вопрос
цены O(40) scan на узком working set и здесь содержит несколько дефектов.

#### Ошибка 1: workload подменяет 4 MiB segment константой 2 MiB

В
`examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs`:

```rust
let segment = 2 * 1024 * 1024usize;
// комментарий утверждает: SegmentLayout::SEGMENT
```

Фактический allocator invariant в `src/alloc_core/os.rs`:

```rust
pub(crate) const SEGMENT: usize = 1 << 22; // 4 MiB
```

Соседний correctness test использует настоящий
`SegmentLayout::SEGMENT` и перед materialization явно устанавливает
unbounded budget. Новый timing example не делает ни того, ни другого.

#### Ошибка 2: 256 MiB budget ломает заявленную materialization схему

ON arm использует default extended budget 256 MiB. Девять размеров растут
примерно геометрически. Budget eviction/rejection начинает вмешиваться до
того, как можно честно утверждать «девять resident distinct spans переполнили
base 8 и materialized sidecar». Для span, который сам больше budget,
production path специально выполняет reject **до** materialization.

В workload нет собственного assertion или read-back oracle
`extension_materialised == true`. Он ссылается на другой test с другой
конфигурацией (`budget=None`) и другим определением segment size. Поэтому
нельзя заключить, что timed ON arm вообще сканирует 40 slots.

#### Ошибка 3: счётчики противоречат механистическому объяснению

Summary CSV публикует для N=4:

- OFF: `segments_reserved_total = 10`;
- ON: `segments_reserved_total = 14`.

Но текст объясняет ON-faster тем, что **OFF** якобы выполнил дополнительный
OS reserve после FIFO eviction, а ON якобы ничего не evict-ил. Если причина
действительно в extra reserve OFF, направление счётчика должно это
поддерживать; опубликованные totals показывают обратное. Оба timed paths дают
100% cache hits, поэтому измеренный persistent-state delta остаётся
необъяснённым и не изолирует O(8) против O(40).

#### Следствие

Фразы «gap closed», «no narrow-working-set regression exists» и
рекомендация promotion в текущем R31-3 отчёте не поддержаны этим gate.
Turnover-win не опровергнут, но scan-cost вопрос остаётся открытым.

Правильный rerun:

1. Использовать `SegmentLayout::SEGMENT`, не локальную magic constant.
2. Явно форсировать одинаковую конфигурацию и доказать ON materialization
   собственным oracle внутри каждого child run.
3. Перед timed region привести обе руки к одинаковым:
   - числу resident entries;
   - requested/usable sizes;
   - cache hits;
   - `segments_reserved/released`;
   - large-cache used bytes;
   - pool/table occupancy.
4. Изолировать только scan bound. Ещё лучше — добавить deterministic IAI
   microjudge lookup по 8 и 40 slots с одинаковой позицией hit.
5. После этого измерить real process workload отдельно; не выводить scan
   cost из остаточного различия setup state.

### 3.3 Multi-heap RSS: bound доказан, «no blow-up» сформулировано неверно

R31-3 убедительно показывает, что 256 MiB budget исполняется примерно как
248 MiB retained на heap, а OFF arm удерживает примерно 432 MiB/heap в этом
workload. Также показана почти идеальная линейность на 1/8/32 heaps.

Именно поэтому формулировка «no multi-heap RSS blow-up» слишком мягкая:
32 x 248 MiB — это примерно 7.75 GiB retention. Gate доказал
**предсказуемый per-heap ceiling**, а не безопасный process-wide потолок.

До default promotion нужны:

- process-wide shared budget или явная документация линейного worst case;
- отдельная политика/профиль для diverse Large turnover;
- измерение contention/coordination цены, если вводится shared budget.

### 3.4 Pool-cap sweep: честный NO-EFFECT для этого workload, слабый oracle

В 320 запусках caps 4/8/16/32 не изменили reservation count, decommit count,
wall clock или RSS. Вывод «этот workload не является victim» разумен.

Но `decommit_calls_total` считается через thread teardown: worker
`join`-ится до остановки timer, а `AbandonGuard::drop -> trim_for_recycle`
в конце всё равно дренирует pool. Разные mid-run retention истории могут
сойтись к одинаковому final decommit total.

Повторять cap sweep без новых observability нет смысла. Сначала нужны
per-heap:

- `pooled_count` high-water;
- pool admissions/reuses/evictions;
- OS reserve count в измеряемом окне;
- snapshot через barrier **до** thread teardown.

Только если механизм реально меняется между caps, нужен новый wall-clock
gate.

### 3.5 `trim_current_thread()`: реальное RSS-улучшение, не speedup

Публичный метод делает именно то, что обещает:

- flush всех tcache magazines;
- drain small-segment pool;
- eviction всего large cache;
- registry slot/TLS binding остаются живыми.

В опубликованном scenario память действительно возвращается ОС, тогда как
pure idle не возвращает ничего. Это полезная production capability и
первая runtime-функция этой волны, доступная пользователю без experimental
feature.

Но gate не измеряет:

- latency самого trim;
- latency второго burst после уничтожения warm state;
- throughput/CPU cost;
- cost при mixed Small/Large workload;
- координацию на реальном multi-thread worker pool.

Поэтому формулировку «first measured runtime improvement» следует читать как
**первое измеренное улучшение memory residency**, не как ускорение времени.

#### Code/API improvement

Сейчас `trim_current_thread()` вызывает `current_heap()`. На свежем потоке
один speculative trim сам захватывает registry slot и создаёт/binds empty
heap, хотя освобождать нечего. Документация это честно раскрывает после R32,
но поведение всё равно нежелательно.

Лучше добавить passive TLS resolver вроде `try_current_bound_heap()`:

- вернуть current owned heap, если он уже bound;
- ничего не создавать и не claim-ить;
- fallback/fresh thread -> no-op.

Также полезнее одного all-or-nothing API будут tiered варианты:

- flush только tcache;
- drain small pool до target;
- trim large cache до budget/headroom;
- `TrimOptions`/`TrimReport` с количеством released bytes/spans.

Для каждого варианта нужен парный cost/benefit gate:
idle RSS **и** trim latency **и** post-trim burst latency в одном workload.

## 4. Качество кода и проекта

### Что стало лучше

1. Profile оси теперь отражают реальные независимые механизмы. Старый
   `Profile::Throughput` больше не может незаметно одновременно менять small
   pool и large-cache headroom.
2. R32-review действительно нашёл три живых дефекта вокруг единственного
   runtime API, а response commit их исправил.
3. Gate reports получили structural/semantic verifier.
4. CI clippy feature matrix теперь проверяется на drift.
5. Исправлены несколько ragged/stale/wrong-unit артефактов и устаревшие
   ссылки на старый Profile API.
6. Trim report после response правильно объясняет 144 MiB commit через
   whole-4-MiB-segment rounding, а не через выдуманные 16 MiB metadata.

### Что нужно улучшить

#### P1: измерительные harnesses должны импортировать production constants

Ошибка 2 MiB против 4 MiB — классическая причина не дублировать allocator
invariants в examples. `SegmentLayout::SEGMENT` уже публично доступен и
используется соседним test. Добавить lint/test, запрещающий локальные
`2 * 1024 * 1024`/`4 * 1024 * 1024` под именем segment в allocator gates,
либо общий test-support helper.

#### P1: каждый gate должен доказывать собственную feature activation

Нельзя переносить oracle из test с `budget=None` в example с default finite
budget. Gate, чья гипотеза зависит от sidecar/materialization/directory/
virgin status, обязан записать и assert-нуть этот status в той же child
process и в том же timed setup.

#### P1: отделять скорость от RSS и цену от выгоды

Проект уже требует counterfactuals, но R31-10 снова публикует только сторону
выгоды. Для policy/API оптимизаций обязательная минимальная таблица:

| Механизм | Выгода | Цена |
|---|---|---|
| cache/retention | hit rate, alloc latency | RSS/commit |
| trim/decommit | RSS/commit returned | trim latency, next-burst latency |
| zero-skip | virgin zero latency | regular/recycled/touch-heavy overhead |
| directory/index | miss latency | metadata, update-path overhead |

#### P2: verifier не должен завершаться «ALL GREEN» с сотнями WARN

Внутренний R32-review сообщает около 353 WARN при exit 0. Тогда зелёный
headline почти не несёт информации. Нужны:

- machine-readable allowlist с owner/expiry/reason;
- budget на количество WARN;
- promotion новой warning class в failure после migration window;
- отдельный итог `PASS WITH N WARNINGS`, не `ALL GREEN`.

#### P2: provenance должен быть одним атомарным snapshot

Открыты дефекты helper: invalid recovery command, tree SHA и patch hash из
разных состояний, hand-typed provenance в CSV. Генератор должен один раз
создать immutable tree/commit identity, затем сам вставить один и тот же
machine-readable block в raw log, CSV и Markdown.

#### P2: привести open-item ledgers к единому состоянию

`docs/perf/OPEN_ITEMS.md` одновременно содержит утверждение, что narrow
timing вопрос `large-cache-extended` закрыт, и старую low-priority запись,
что он deferred. После этого ревью вопрос следует явно переоткрыть из-за
ошибки segment constant и отсутствующего materialization oracle.

`docs/CORRECTNESS_OPEN_ITEMS.md` после `e124a48` также требует housekeeping:
changelog coverage P2 уже исправлен, а typed-handle item нужно переоткрыть с
новым cross-core P0.

#### P2: concurrency test не должен опираться на общий mutable counter

Внутренний R32-review отметил test `trim_current_thread` с равенством на
process-wide counter в окне, которое соседние tests могут изменить. Нужна
subprocess isolation, serial fixture или per-heap counter.

## 5. Что ещё можно сильно ускорить

### Приоритет 0 — сначала закрыть soundness

Нельзя расширять/promote diagnostic или reservation API, пока cross-core
handle можно безопасно передать не тому владельцу. Это первый следующий
коммит, независимо от performance roadmap.

### Приоритет 1 — довести `virgin-zero-skip` до решения о production

Потенциал: **примерно 9x–72x** по опубликованным notouch значениям
(`OFF / ON`), или 89–98.6% экономии времени на конкретном virgin-zero пути.

Почему первым:

- код уже реализован;
- production-layer механизм подтверждён;
- выигрыш огромный;
- не нужен новый API или пользовательская adoption.

Осталось доказать, что выигрыш не покупается заметной ценой в обычном
alloc/recycled/realloc пути. Если instruction-count и application A/B
покажут практически нулевой проигрыш, это наиболее рациональный следующий
production promotion.

### Приоритет 2 — `large-cache-extended` как named opt-in policy

Потенциал: убрать OS reserve/release miss на diverse Large turnover,
33.3% -> 100% hits в измеренном victim workload.

Не включать blanket-default сейчас. Сначала:

1. исправить narrow gate;
2. измерить O(40) lookup в matched state;
3. ввести process-level retention story;
4. затем добавить явно названную policy, например
   `LargeCachePolicy::DiverseTurnover`, а не скрыто менять default.

Если O(40) scan окажется значимым, не обязательно отказываться от 40 slots:
можно заменить полный линейный поиск на компактный occupancy bitmap плюс
size-ordered/bucketed candidate index. Но при всего 40 entries сначала
нужно число из корректного deterministic gate — структура может оказаться
дороже scan.

### Приоритет 3 — batch API только вместе с реальным потребителем

Предыдущие волны уже показывали около 1.1–1.6x относительно production
scalar path на подходящих batch. Без caller adoption Box/Vec не ускорятся.
Следующая работа должна начинаться не с очередного allocator microbench, а с
конкретного consumer integration: object pool, packet/buffer batch, arena
refill или runtime slab. Публичную поверхность имеет смысл стабилизировать
только вместе с таким end-to-end benchmark.

### Приоритет 4 — tiered trim для memory-pressure workloads

Это не чистый throughput speedup, но может сильно улучшить latency системы
под реальным memory pressure, исключив paging/working-set contention.
Нужен partial trim, passive no-bind lookup и application gate, измеряющий
tail latency до/после, а не только RSS.

### Условные направления, не брать без victim workload

- Page-run layer для диапазона примерно 1.25–2 MiB: потенциально повышает
  density, но нужен профиль с реальной долей этих размеров.
- Macro directory/hints при >=64 live segments: полезно только если
  production telemetry показывает такой S.
- Более агрессивные medium classes: прежние волны уже нашли разрушительную
  realloc-регрессию; без redesign in-place grow возвращаться к blanket
  promotion нельзя.
- Новые pool-cap sweeps: бессмысленны без mid-run pool telemetry.

## 6. Рекомендуемый следующий этап работ

### Этап A — correctness и достоверность

1. Закрыть cross-core `ReservedSmallSegment` P0.
2. Добавить two-core negative test и обновить correctness ledger.
3. Исправить R31-3 segment constant.
4. Добавить собственный materialization/config oracle в narrow gate.
5. Переоткрыть неверно закрытый scan-cost item.

### Этап B — два коротких decision gates

1. `virgin-zero-skip`: won-front + lost-front gate на одном production entry
   point, subprocess isolation, deterministic counters/Ir.
2. `large-cache-extended`: matched-state O(8)/O(40) gate и process-wide
   retention decision.

На этом этапе не менять `production`; сначала получить честный verdict.

### Этап C — shipping

1. Если цена `virgin-zero-skip` около нуля — explicit promotion и полный
   README/IAI/cross-version refresh.
2. Если extended-cache scan приемлем — named opt-in Large policy с явным
   budget и документацией per-heap/process worst case.
3. `trim_current_thread` перевести на passive bound-heap lookup; добавить
   cost/next-burst gate и partial trim design.

### Этап D — project hygiene

1. Сделать gate verifier signal-bearing: warning budget/allowlist/expiry.
2. Починить atomic provenance generation.
3. Свести performance/correctness ledgers к одному актуальному состоянию.
4. Вынести размерные derivations gates в общий helper или всегда брать
   public allocator constants.

## 7. Итоговая оценка волн

**Да, проект стал лучше:** честнее конфигурация, сильнее measurement tooling,
появился полезный production trim API, исправлены реальные review findings,
а два opt-in механизма подтвердили крупный потенциал.

**Нет, default allocator throughput этой волной заметно не ускорен:** hot
feature composition не менялся, а единственное новое shipping runtime
действие вызывается пользователем вручную и оптимизирует retained RSS, не
скорость alloc/free.

**До предела радикального ускорения ещё не дошли.** Самый близкий большой
выигрыш — `virgin-zero-skip`; следующий workload-specific — расширенный
Large cache. Но сначала обязателен P0 fix typed handle и ремонт неверного
narrow gate. После этого решения можно принимать на данных, а не на
оптимистичных headline-формулировках.
