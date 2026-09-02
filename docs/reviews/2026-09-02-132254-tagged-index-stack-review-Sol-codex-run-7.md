# `tagged-index-stack`: предрелизное ревью — Sol-codex, прогон 7

- Время начала исследования: 2026-09-02 13:22:54 CEST (`Europe/Berlin`)
- Метка запроса: 13:21
- Проверенный `HEAD`: `426ac2255b531a61a70645a4411d975a69fc61e8`
- Последний коммит на момент завершения чтения: `426ac22` — `checkpoint: 2026-09-02-1323`
- Режим: статическое чтение, самостоятельно, без агентов и под-агентов
- Не запускались: `cargo`, тесты, doctest, clippy, rustdoc, loom, benchmark, examples и scripts
- Единственное изменение репозитория в рамках ревью: этот Markdown-отчёт и его коммит

## Область проверки

Крейт просмотрен заново как самостоятельный предрелизный продукт: production-код и весь публичный API,
atomic/ABA/unsafe-контракты, manifest и package surface, README/CHANGELOG, тестовые и compile-fail
оракулы, loom-модели, real-thread test, benchmark, latency example, measurement scripts, релевантные CI
строки и интеграция с `sefer-alloc::Registry`. Отдельно проверены изменения после Sol-codex run 6:
замена `Hook` на `unsafe fn` hooks, feature-gating диагностических API, обновление durability/CI,
исправление timing window и защита JSON label.

Это bounded single-context review. Production-код прочитан полностью; сгенерированные fixture lockfiles
проверялись как package/dependency surface, а не построчно. Нерелевантные для этого крейта классы
(`async`, FFI, crypto, serde, файловый/сетевой I/O и RAII внешних ресурсов) после статического census
не углублялись: соответствующих production-механизмов здесь нет.

## Вердикт

**NO-GO к публикации в текущем виде.**

Основной Treiber/CAS-алгоритм на просмотренном `HEAD` выглядит согласованным, а прошлые блокеры прогона 6
в основном закрыты правильно. Однако новая граница `unsafe fn` содержит другое, более фундаментальное
несоответствие: safe `StackOps::push_index` вызывает unsafe `StackStorage::store_next`, не имея способа
доказать одно из явно записанных safety-preconditions — что storage действительно владеет ячейкой для
данного индекса. Для внешней реализации с более узкой ёмкостью это позволяет полностью safe-вызову
нарушить контракт unsafe-функции. До первой публикации ответственность должна быть перенесена на ту
сторону API, которая реально способна её выполнить.

После устранения P1-1 нужен ещё один прицельный статический аудит изменённой границы. P2/P3 не показывают
новой ошибки самого CAS-цикла, но их разумно закрыть до релиза, пока API и опубликованная документация
ещё свободно меняются.

## Блокирующая находка

### P1-1. Safe `push_index` не может выполнить safety-precondition unsafe `store_next`

Места:

- `crates/tagged-index-stack/src/imp.rs:488-517` — нормативные обязанности unsafe trait;
- `crates/tagged-index-stack/src/imp.rs:1083-1093` — safety-контракт `store_next`;
- `crates/tagged-index-stack/src/imp.rs:1103-1174` — safe `push_index` и два независимых диапазона;
- `crates/tagged-index-stack/src/imp.rs:1325-1332` — crate-private unsafe bridge и его доказательство;
- `crates/tagged-index-stack/src/imp.rs:1792-1818` — независимая ёмкость `ArrayLinks<N>`;
- `crates/tagged-index-stack/tests/stack_unit.rs:340-369` — закреплённый panic при индексе, допустимом
  по `INDEX_BITS`, но отсутствующем в backing;
- `crates/tagged-index-stack/tests/custom_storage_impl.rs:82-157` — внешний `VecStorage` с собственной
  runtime-ёмкостью.

`StackStorage::store_next` разрешено вызывать только когда `self` «owns the slot». При этом
`StackOps::push_index` — safe `fn`; он проверяет лишь `index < TaggedIndex::INDEX_MASK` и прямо
документирует, что конкретный storage может иметь более узкую ёмкость. Внутренний bridge доказывает
numeric range, no-double-push, форму `next` и CAS-фазу, но не доказывает существование backing cell —
оно из типа и trait API не наблюдаемо. Пять верхнеуровневых safety clauses также не назначают
реализации обязанность быть memory-safe для **каждого** численно допустимого индекса, переданного
safe `push_index`.

Текущие `ArrayLinks` и тестовый `VecStorage` используют checked indexing и поэтому паникуют. Но внешний
implementor вправе оптимизировать доступ, опираясь на записанный caller-side precondition, например
вызвать `get_unchecked(index as usize)` внутри `unsafe fn store_next`. Тогда `storage.push_index(i)`, где
`i < INDEX_MASK`, но `i >= storage_capacity`, остаётся полностью safe downstream-кодом и приводит к
нарушению safety-контракта внутри библиотечного bridge, потенциально к UB. Наличие `unsafe impl` не
переносит на implementor ответственность за условие, которое документация явно назначила caller'у.

Конкретный `Registry` этого сейчас не эксплуатирует: `src/registry/heap_registry.rs:611-684` использует
проверяемый `slot(index)` и ограничивает реальные индексы `MAX_HEAPS`. Это не исправляет публичный
контракт для других реализаций.

Лучшее исправление без оглядки на совместимость — выбрать одну непротиворечивую модель:

1. **Сохранить safe `push_index`:** trait contract должен требовать, чтобы `store_next` оставался
   memory-safe для любого индекса, прошедшего публичный numeric guard; более узкая реализация обязана
   checked-проверить индекс и panic/return error до unchecked access. Из method `# Safety` следует убрать
   недоказуемое caller-условие «owns the slot» и явно перенести доменную обязанность на implementor.
   Это минимальный вариант без дополнительного вызова/ветвления в generic hot path сверх того bounds
   check, который всё равно нужен более узкому storage.
2. **Сделать домен явным:** добавить `contains_index`/`capacity` и проверять его до unsafe hook. Это
   наиболее механически проверяемая граница, но добавляет операцию в push и требует определить модель
   для не-contiguous storage; associated const ломает `dyn StackStorage`, runtime method сохраняет его.
3. **Самая честная низкоуровневая модель:** выделить `unsafe push_index_unchecked` с условиями «ячейка
   существует» и «индекс не live», а safe wrapper строить только там, где эти условия можно реально
   проверить или обеспечить типом/ownership token. Сейчас no-double-push тоже является лишь prose-
   контрактом safe функции, хотя документация говорит, что allocator consumers строят на эксклюзивной
   выдаче memory safety (`src/imp.rs:490-493`). Если цель — совершенная allocator primitive API, эту
   обязанность стоит поместить в настоящую unsafe-границу, а не оставлять безопасно нарушаемым правилом.

После выбора модели нужны два оракула: внешний storage с ёмкостью меньше `INDEX_MASK` и реализация,
использующая unchecked-доступ только под новым, действительно выполнимым контрактом. Сам тест здесь не
запускался и не добавлялся из-за режима ревью.

## Существенные улучшения до публикации

### P2-1. Safe liveness-прекондиция не соответствует заявленной allocator-soundness роли API

Места: `src/imp.rs:490-493`, `1117-1152`, `1634-1647`; `src/lib.rs:281-295`.

Safe `StackOps::push_index` и safe `ArrayIndexStack::push` позволяют повторно положить уже reachable
индекс. Код честно объясняет последствия: цикл, бесконечная выдача и два владельца одного allocator
slot. Runtime guard ловит только self-loop текущей вершины; более глубокий цикл остаётся тихим. Для
локального контейнера `u32` это логическая порча, а не UB внутри крейта, но она конфликтует с формулировкой
«SOUNDNESS commitment», на которую предлагается опираться внешнему unsafe allocator-коду.

До релиза следует принять явную позицию:

- либо push становится unsafe и получает полное safety-условие домена + уникальной liveness;
- либо safe API официально считается fallible/corruptible при логическом misuse, а каждый allocator
  обязан закрывать storage type и самостоятельно доказывать дисциплину выдачи — тогда trait docs не
  должны обещать, что одной реализации `unsafe trait` достаточно для эксклюзивности.

Текущая промежуточная модель слишком легко заставляет автора внешнего allocator считать, что
`unsafe impl` уже замкнул soundness proof, хотя safe downstream call способен разрушить его состояние.

### P2-2. `CHANGELOG` одновременно описывает три несовместимые unsafe-архитектуры

Места: `crates/tagged-index-stack/CHANGELOG.md:202-248`.

Первый bullet утверждает, что hooks остаются safe `fn` и в production есть ровно один unsafe token.
Следующий подробно описывает уже удалённый `&Hook`; лишь третий описывает текущие `unsafe fn` и два
аудируемых site. `Hook`-bullet помечен superseded, но исходный bullet про safe methods — нет. Для ещё не
выпущенного 0.1.0 это не полезная история изменений, а ложное описание публикуемого состояния.

Следует свернуть эту последовательность в один итоговый пункт про фактическую архитектуру. Историю
экспериментов, даты, task/review IDs и доказательства выбора хранить в ADR/review, не в user-facing
changelog первого релиза.

### P2-3. `CHANGELOG` переобещает покрытие pop guard

Места: `CHANGELOG.md:54-70`; фактический guard — `src/imp.rs:1450-1466`; точное описание ограничения —
`src/imp.rs:907-943,956-982`.

Changelog говорит, что guard паникует при любом ответе кроме `TAIL` или «currently-valid index» и что
payload-aliased backing делает это на каждой обычной benign race. Реальный guard ловит только
out-of-range и `next == index`. Любое чужое, но численно допустимое и не-self значение проходит; тесты
намеренно демонстрируют acyclic silent corruption. Нужно заменить абсолютное утверждение точной
формой: guard ловит численно недопустимый link и self-loop, но не валидирует принадлежность/достижимость
и не делает payload aliasing безопасным.

## Пахнущий код и нейрослоп

### P3-1. Нормативный API утонул в истории ревью и повторяющихся объяснениях

`src/imp.rs` вырос до 1971 строки; документация одного `StackStorage` занимает примерно строки
`474-1033`. В public rustdoc находятся даты решений, номера задач, имена прошлых review-файлов,
несколько повторов hazard inventory и история `Hook`, которого в API уже нет. `CHANGELOG.md` для
первого unreleased релиза — 299 строк и сохраняет superseded implementation narrative.

Это уже не только эстетика: повторы породили две фактические несогласованности P2-2/P2-3 сразу после
doc-sync коммитов. Рекомендуемая структура:

- в rustdoc оставить короткие нормативные `# Safety`, ordering, panic и usage contracts;
- один canonical hazard table, на который ссылаются остальные разделы;
- rationale, эксперименты, даты и эволюцию решений оставить в ADR;
- changelog описывает только итоговый пользовательский diff опубликованной версии.

Так unsafe surface станет легче проверять, а риск очередного stale claim заметно уменьшится.

### P3-2. Latency example неверно характеризует overhead измерения

Места: `examples/backoff_per_call_latency.rs:41-52,223-235`.

Документ говорит, что два `Instant::now()` находятся вне timed `pop`, а абсолютная latency завышена
примерно на два clock reads. Фактически интервал задаётся `t0 = Instant::now()` и `t0.elapsed()` вокруг
`pop`: части timestamp overhead попадают в измеряемый интервал, но это не «два чтения снаружи» и не
обоснованная поправка ровно в два clock reads. Для cap-to-cap A/B форма симметрична и относительное
сравнение остаётся полезным; абсолютные fast-path наносекунды без baseline calibration нельзя так
интерпретировать.

Исправление: измерить пустую bracket baseline тем же способом, публиковать её рядом (не обязательно
слепо вычитать из хвостовых percentiles) и описать `wall_ms` как coordinator-to-last-join envelope:
второй barrier не допускает работу до start, но denominator включает release/join и deadline overshoot.

## Производительность

Обязательной оптимизации production hot path по статическому чтению не найдено.

- Push: relaxed head load, release CAS; pop: acquire head/failure/success. При инварианте «все записи
  head — RMW» release-sequence аргумент согласован; менять это без нового proof нельзя.
- `ArrayLinks` Acquire/Release сильнее минимально необходимого для самого head-publication proof. На
  AArch64 это реальный `ldar/stlr` delta, на x86-64 варианты эквивалентны по codegen. До native AArch64
  wall-clock данных ослабление остаётся спекуляцией; текущая отсрочка решения разумна.
- Strong/weak CAS уже описаны как codegen-identical на исследованных lowering; основания менять strong
  CAS сейчас нет.
- Saturating backoff ограничен и активируется только после проигранного CAS; default build не содержит
  test counters. Здесь лишней production instrumentation больше не видно.
- Dense `ArrayLinks` может давать false sharing при конкурентной работе по соседним индексам; benchmark
  это честно отмечает. Универсально padding добавлять не стоит: внешняя `StackStorage` реализация уже
  может выбрать padded/sharded layout под конкретный allocator workload.
- Generic storage hooks обычно доступны для monomorphization/inlining; `dyn StackStorage` осознанно
  платит за dispatch. Убирать object-safe путь без измерения нет причины.

Сначала нужно исправить P1-1. Любая оптимизация bounds/domain проверки должна следовать из выбранного
safety-контракта, а не удалять единственную защиту ради нескольких инструкций.

## Что в последних правках сделано хорошо

- Бывший `Hook` действительно заменён на compiler-enforced `unsafe fn` seam; прямой safe hook call
  теперь запрещается языком, а три вызова сосредоточены в одном crate-private bridge.
- Test-only raw head/link probes и retry counters gated через `test-internals`/`loom`; default build
  больше не несёт публичную диагностику и hot-path increments.
- Unsafe inventory production source теперь соответствует коду: два allow-site, три unsafe fn
  declarations, три unsafe blocks.
- `Registry` обновлён под новый API и использует одну стабильную head↔link binding; его checked slot
  path не создаёт обнаруженную в P1-1 UB-дыру в текущем in-workspace consumer.
- Public `TaggedIndex::pack` проверяет обе половины; truncating pack остаётся crate-private. Диапазон
  `INDEX_BITS=1..=16`, sentinel и H-2 переход в empty согласованы.
- Standalone `ArrayIndexStack` не реализует публичный `StackStorage`, его head не извлекается наружу;
  прежний competing-binding путь закрыт структурно.
- Compile-fail suite статически содержит оракулы unsafe impl, запрета прямого hook call, диапазонов
  const generic и закрытого `ArrayIndexStack` head; loom-тесты содержат activation counters и
  should-panic counterfactuals, то есть не выглядят вакуозными.
- CI статически содержит обычные/release/feature, clippy, rustdoc, MSRV, no_std, package и loom gates.
  Их фактическое прохождение в этом ревью не утверждается.
- Latency probe получил второй barrier и больше не начинает работу до coordinator timestamp; label
  ограничен JSON-safe алфавитом.

## Unsafe/системный census

В production `crates/tagged-index-stack/src` обнаружены:

- один `unsafe trait`;
- три `unsafe fn` declarations;
- три unsafe call blocks в одном bridge impl;
- нет raw-pointer arithmetic/dereference, FFI и manual `Send`/`Sync` impl;
- unsafe impl внешнего consumer находится в root `Registry`, проверен отдельно.

Главный риск не в количестве unsafe syntax, а в полноте доказательства на каждом из трёх вызовов. Для
`store_next` оно сейчас неполно по capacity/ownership domain — P1-1.

## Краткий путь к GO

1. Выбрать и реализовать непротиворечивую ownership/capacity модель из P1-1; bridge должен уметь
   доказать все preconditions каждого unsafe hook локально.
2. Решить статус no-double-push: настоящая unsafe-precondition либо честно ограниченная safe semantic
   contract с обязанностью allocator wrapper не экспортировать этот surface.
3. Добавить статические/исполняемые оракулы для более узкого custom storage и выбранной границы. В этом
   read-only прогоне они не запускались.
4. Свести CHANGELOG к текущей unsafe архитектуре и исправить обещание pop guard.
5. Сжать public rustdoc до одной нормативной версии контракта; убрать pre-release archaeology из API.
6. Уточнить методологию latency probe; ordering/backoff не менять без целевых измерений.

До закрытия пункта 1 публикацию не рекомендую.
