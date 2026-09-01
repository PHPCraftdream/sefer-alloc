# `tagged-index-stack`: предрелизное ревью — Sol-codex, прогон 4

- Время исследования: 2026-09-01 12:42:16 CEST (`Europe/Berlin`)
- Метка запроса: 12:37
- Проверенный `HEAD`: `90a01ed4fd556b96c1f026e9b07a66ff27f321cc`
- Режим: статическое чтение, самостоятельно, без под-агентов
- Не запускались: `cargo`, тесты, doctest, clippy, rustdoc, loom, benchmark, examples и scripts
- Область: весь публичный API и production-код крейта, тестовые оракулы, compile-fail fixtures, документация, manifest/package/CI, benchmark/example harness; отдельно просмотрены последние изменения после Sol-codex run 3

## Итог

**NO-GO к публикации.**

Редизайн удалил старый синтаксис, в котором `StackHead` принимал backing на каждом вызове, но не сделал главный инвариант структурным. Через полностью безопасный публичный API по-прежнему можно получить два конкурирующих представления одного head/backing или связать независимые головы с пересекающимися индексами в общих link cells. Результат — тихая повторная выдача одного индекса двум владельцам. Сам крейт содержит несколько компилируемых тестов, которые это демонстрируют.

Для standalone-типа `ArrayIndexStack` это особенно серьёзно: тип выглядит как безопасная цельная абстракция, однако blanket-реализация публичного `StackStorage` раскрывает `head()` и storage hooks любому вызывающему. В аллокаторе повторная выдача индекса обычно превращается в aliasing/use-after-free уже на следующем уровне.

Документация честно описывает большую часть ограничений, но контракт, который безопасная типовая система не обеспечивает, нельзя считать закрытым только потому, что он подробно задокументирован.

## Блокирующие находки

### P1-1. Safe API допускает тихий double-issue

`StackStorage` — открытый безопасный trait. Его методы `head`, `load_next` и `store_next` публичны и вызываемы пользователем. Blanket `StackOps` затем доверяет согласованности возвращаемой головы и link storage. Никакой типовой связи «одна голова ↔ один backing ↔ одна непересекающаяся популяция индексов» нет.

Это не гипотетическая атака. `tests/custom_storage_impl.rs` содержит работающие safe-Rust сценарии:

- `array_index_stack_head_still_double_issue` извлекает head даже из цельного `ArrayIndexStack` и строит конкурирующий storage view;
- `hand_crafted_acyclic_forgery_still_double_issues` создаёт ациклическую подделку, которую runtime guard не видит;
- `two_stacks_sharing_link_storage_still_double_issue` выдаёт одинаковые индексы из двух независимых голов над общими cells;
- `head_moved_into_fresh_links_leaks_and_then_panics` демонстрирует temporal rebinding: сначала тихая потеря элемента, лишь затем panic;
- `one_value_two_bindings_shared_backing_still_double_issue` получает две логические привязки даже из одного значения;
- `internally_disagreeing_storage_still_double_issue` показывает рассогласованный implementor; self-loop detector обнаруживает только один из последующих симптомов.

В `src/imp.rs:471-500` прямо сказано, что hooks — «не caller-facing API», но они остаются plain safe `pub` methods, позволяют замкнуть цикл и раскрывают `&StackHead` из `ArrayIndexStack`. Это API-дефект, а не только misuse.

Что исправить без оглядки на совместимость:

1. Разделить безопасный standalone API и expert custom-storage seam.
2. Не реализовывать публичный trait с извлекаемым head/hooks непосредственно для `ArrayIndexStack`; безопасный тип должен предоставлять только `push`/`pop` и не позволять собрать второй binding.
3. Сделать storage hooks недоступными обычному вызывающему. Практичный вариант — публичный, но неконструируемый снаружи access token, который передаётся в `head/load/store` и создаётся только внутренним алгоритмом. Это оставляет возможность внешней реализации trait, но блокирует прямой вызов hooks и извлечение головы из shipped type.
4. Для пользовательских реализаций либо владеть storage внутри stack wrapper по значению, либо обозначить неизбежные value-level обязательства как `unsafe trait StackStorage` с чётким `# Safety`. Обычный safe trait не должен обещать целостность, которую компилятор не проверяет.
5. Добавить compile-fail/compile-pass оракулы именно для нового дизайна: извлечь head shipped stack и построить competing binding должно быть невозможно; корректный внешний storage должен оставаться реализуемым.

### P1-2. Compile-fail test объявляет блокер закрытым, проверяя только удаление старых имён методов

`tests/compile_fail_two_backings.rs` утверждает, что прежний double-issue стал `UNEXPRESSIBLE`, а ошибка компиляции «IS the structural fix itself». Fixture лишь создаёт `StackHead` и вызывает удалённые методы `stack.push(&links, ...)` / `stack.pop(&links)`, после чего driver ищет `E0599`.

Это доказывает только отсутствие старого method spelling. В том же дереве `tests/custom_storage_impl.rs` находятся несколько компилируемых выражений того же класса ошибки через актуальные `StackStorage`/`StackOps`. Таким образом, тест — ложный зелёный оракул и может вводить release review в заблуждение.

Исправление: удалить утверждение о structural closure и заменить fixture проверкой реального нового свойства после исправления P1-1. Пока свойство не обеспечено, этот test должен считаться регрессионным тестом удаления API, но не доказательством safety-инварианта.

## Существенные ошибки и неточности

### P2-1. README противоречит каноническому контракту о shared link cells

`README.md:58` запрещает sharing самих link cells между независимыми stacks. Но `src/imp.rs:628-644` и crate-level docs в `src/lib.rs:67-71` правильно уточняют: sharing cells само по себе безвредно при непересекающихся индексных популяциях; проблема возникает при пересечении достижимых индексов.

README содержит устаревшее, более сильное правило и ссылается на trait rule 3, который говорит обратное. Нужно выбрать одну точную формулировку и синхронизировать все поверхности.

### P2-2. README переобещает эффективность corruption guard

`README.md:79` говорит, что payload aliasing «defeats the guard, which panics unconditionally on a backing that violates it». Реальный guard в `src/imp.rs:1208-1224` распознаёт только:

- значение вне диапазона;
- self-loop `next == index`.

Любое испорченное, но допустимое по диапазону ациклическое значение проходит тихо. Это подтверждает собственный `hand_crafted_acyclic_forgery_still_double_issues`. Следует писать «guard detects two value shapes», а не создавать впечатление гарантированного fail-fast для нарушенного backing.

### P2-3. Заявленный MSRV не соответствует описанному «full target set»

`Cargo.toml` объявляет `rust-version = "1.81"` и называет 1.81 реальным floor всего target set из-за test-only `PanicHookInfo`. Но CI-комментарии около `.github/workflows/ci.yml:2323-2333` признают, что Cargo 1.81 не может разрешить текущий dev-dependency graph из-за edition-2024 manifests, поэтому на 1.81 проверяется лишь урезанный library `cargo check`, а не тестовый/clippy target set, которым обосновано значение 1.81.

Нужно выбрать непротиворечивую политику:

- либо MSRV относится к публикуемой library surface — тогда определить минимальную версию production-кода и не поднимать её из-за dev-only API;
- либо обещается весь локальный target set — тогда pin dev dependencies, совместимые с Cargo 1.81, и действительно компилировать соответствующие targets на MSRV.

Текущая формулировка одновременно опирается на тестовый код и признаёт, что этот код с полным dependency graph на заявленном toolchain не проверяется.

## Производительность и измерения

### P3-1. Link cells используют доказанно ненужные Acquire/Release

`StackStorage` требует `Acquire` для `load_next` и `Release` для `store_next`, хотя `src/imp.rs:438-462` само выводит, что publication обеспечивается head CAS и link accesses могли бы быть `Relaxed`. `ArrayLinks` повторяет эту пару в `src/imp.rs:1496-1529`.

На x86 это обычно бесплатно, на слабоупорядоченных архитектурах — потенциально нет. Для низкоуровневого free-list primitive «defence-in-depth» не должна бессрочно оставаться неизмеренной платой на hot path. Сделать A/B на AArch64/другом weak-memory target и, если результат подтверждается, закрепить `Relaxed` вместе с loom/counterfactual proof. Не менять ordering вслепую.

### P3-2. Strong CAS оставляет вероятную LL/SC-оптимизацию неизмеренной

Push loop сознательно использует `compare_exchange`, хотя комментарий `src/imp.rs:1141-1147` признаёт, что `compare_exchange_weak` может лучше взаимодействовать с собственным backoff на LL/SC. Аналогично проверить pop path. Это не release blocker, но явная незакрытая возможность ускорения; решение должно опираться на multi-target contention A/B и retry counters.

### P3-3. Contention benchmark предполагает, что все workers успеют за 300 ms

Benchmark назначает `timed_start = now + 100 ms + 200 ms`, затем создаёт workers и barrier. Комментарий объявляет это «far above worst-case», но после barrier нет проверки, что `timed_start` ещё в будущем. На перегруженном CI/VM запуск может задержаться более чем на 300 ms: часть или всё окно будет потеряно, а результат останется правдоподобным числом.

Надёжнее после rendezvous проверять lateness и повторять/отбрасывать sample либо выдавать стартовый instant координатором только после готовности workers. Общий deadline и единый знаменатель — хорошее улучшение, но предположение о lead time остаётся хрупким.

### P3-4. Latency example принимает бессмысленные/падающие shapes

`examples/backoff_per_call_latency.rs` не валидирует:

- `threads == 0` — workers отсутствуют, набор samples пуст;
- `threads > LINKS_SIZE` — больше одновременно удерживаемых индексов, чем prefill, поэтому `pop().expect(...)` может законно упасть;
- `iters == 0` или `reps == 0` — пустые выборки/отсутствующий результат.

Проверять `1 <= threads <= LINKS_SIZE`, `iters > 0`, `reps > 0` на границе парсинга с диагностикой конфигурации.

## Качество кода и «нейрослоп»

### P3-5. Объём объяснений маскирует отсутствие структурной гарантии

`src/imp.rs` разросся примерно до 1.7k строк; значительная часть — повторяющаяся археология ревью, CAPITALIZED emphasis, ссылки на номера раундов и длинные объяснения известных дыр. Документация полезна и необычно честна, но сейчас она:

- повторяет один hazard в crate docs, README, trait docs, changelog и тестах;
- уже разошлась между README и каноническим trait contract;
- превращает известный safe-API дефект в «caller discipline»;
- усложняет проверку реального executable invariant.

После исправления модели владения сократить историю решений до ADR/checkpoint, а rustdoc оставить как точный текущий контракт: safety/preconditions, ordering proof, panic conditions и короткие примеры. Номера внутренних ревью не должны быть частью долгоживущего API-текста.

### P4-1. «Exhaustive loom model-check» требует ограничения области

README говорит об exhaustive loom model-check «against the real type». Loom исчерпывающе перебирает расписания только внутри конкретных малых моделей и заданных bounds; он не покрывает все значения, внешние `StackStorage` implementations или показанные storage-binding hazards. Рядом есть уточнения, но headline легко прочитать как более широкую гарантию. Лучше: «exhaustive exploration of the bounded scenarios listed below against the real implementation».

### P4-2. Public blanket trait surface заранее фиксирует semver и coherence

Публичный `StackOps` с blanket impl для каждого `StackStorage` удобен, но занимает downstream coherence slot и делает эволюцию обоих открытых traits трудной. Поскольку совместимость сейчас не ограничение, проще иметь inherent operations на безопасном типе и минимальный sealed/private algorithm adapter; expert extension seam сделать маленьким и явно unsafe/contract-bearing.

## Что стало лучше после прошлого прогона

- Удалён прежний per-call backing API; цельное владение в `ArrayIndexStack` — правильное направление.
- Реализация вынесена в cfg-gated `imp.rs`; invalid cfg/feature combinations теперь дают целевые diagnostics без каскада вторичных ошибок.
- Публичный `TaggedIndex::pack` стал checked; truncating helper закрыт внутри алгоритма.
- Начальный head load в push ослаблен до `Relaxed` с понятным ordering proof.
- Retry/backoff instrumentation лучше отделена feature/cfg gates.
- Contention benchmark теперь использует общий временной интервал, warm-up и общий denominator.
- CI/package gates заметно усилены: dry-run/package extraction, packaged build/test paths, feature rows и compile-fail fixtures.
- Panic-hook тестовая инфраструктура стала аккуратнее благодаря сериализации и RAII-восстановлению hook.
- Документация не скрывает известные failure modes. Это полезно для диагностики, хотя не заменяет structural fix.

## Рекомендуемый путь к GO

1. Закрыть P1-1 на уровне типов/API: из shipped safe stack нельзя получить head/hooks или создать competing binding.
2. Отделить safe owned stack от expert custom-storage seam; оставшиеся невыразимые в типах обязательства сделать явно `unsafe` и минимальными.
3. Заменить ложный compile-fail оракул тестом реального нового свойства.
4. Синхронизировать README, crate docs и trait docs; убрать обещание unconditional detection.
5. Привести MSRV claim и фактический CI gate к одной политике.
6. Валидировать benchmark/example inputs и lateness.
7. После correctness closure измерить `Relaxed` links и weak CAS на weak-memory/LL-SC targets.
8. Сократить повторяющуюся review-археологию в публичной документации.

После пунктов 1–4 нужен новый полный статический аудит: предлагаемое изменение затрагивает саму границу safe/unsafe API и invalidate значительную часть нынешних тестовых оракулов. На проверенном `HEAD` публикацию не рекомендую.
