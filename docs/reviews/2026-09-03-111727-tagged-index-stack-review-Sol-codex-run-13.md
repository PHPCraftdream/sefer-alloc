# tagged-index-stack — предрелизное статическое ревью

**Ревьюер:** Sol-codex

**Раунд:** Codex run 13

**Метка пользователя:** 2026-09-03 11:16

**Время фиксации ревью:** 2026-09-03 11:17:27 +02:00

**Проверенная ревизия:** `49b437b54a1b9d9c9adc8916006dc1c8645fcdc7`

**Диапазон новых правок:** `0dbcc9d5986faf7aa7aa3b750cd513e03a773c8a..49b437b54a1b9d9c9adc8916006dc1c8645fcdc7`

**Охват:** полный повторный статический обзор текущего крейта и подробный обзор всех изменений после раунда 12

## Вердикт

**GO к публикации по результатам статического ревью.** P1/P2-дефектов,
soundness-дыр, ошибок ABA/seal-протокола, недостаточных atomic ordering или
доказанных регрессий производительности не найдено. Все три исправления по
результатам раунда 12 реализуют требуемое поведение; горячий push/pop путь не
изменился.

Остаются три P3. Cleanup терминологии unsafe-инвентаря прошёл не по всем живым
копиям. Perf-index ошибочно говорит, что для одного кандидата измерительная
инфраструктура уже готова, хотя соответствующего A/B-варианта в driver нет.
Кроме того, reviewer-facing проза снова заметно разрастается и остаётся
смешанной с небольшим алгоритмическим ядром в монолитном `imp.rs`. Это
качество и сопровождаемость, а не блокеры корректности production API.

Ревью выполнено без агентов как ограниченный single-context pass. Глубоко
проверены unsafe/публичные контракты, concurrency и состояние, арифметика и
представление, lifetimes/API, dependencies/features, testing/CI,
семантическое соответствие документации и performance-path. FFI, async,
криптография, внешнее I/O и ресурсный Drop/RAII-протокол в крейте отсутствуют,
поэтому соответствующие механизмы проверены только сканированием на наличие,
а не отдельным глубоким аудитом.

Ничего не исполнялось: не запускались `cargo`, `rustc`, Node-скрипты, тесты,
clippy, rustdoc, loom, Miri, benchmarks, examples, package/publish-команды и
сгенерированные бинарники. Вердикт является статическим и не заменяет
обычный release pipeline.

## Найденные проблемы

### P3-1 — исправленная терминология unsafe-инвентаря осталась несогласованной

**Места:**

- `crates/tagged-index-stack/CHANGELOG.md:13-15`;
- `README.md:603`;
- `crates/tagged-index-stack/src/imp.rs:912-916,930-941,1269-1274,1296-1302,1556-1559,1857-1861`.

`7306b86` правильно заменил в crate README «exactly EIGHT unsafe sites» на
точное «eight item-scoped `#[allow(unsafe_code)]` lint-exception regions» и
явно отделил границы lint-разрешений от содержащихся внутри unsafe
деклараций/блоков/операций. Но текущий unreleased CHANGELOG, корневой README и
комментарии у allow-регионов всё ещё называют эти восемь объектов unsafe
«sites».

В workspace README термин `site` частично определён локальной таблицей как
item-scoped allow, поэтому это не ошибка safety-модели. Однако без такого
уточнения — особенно в package CHANGELOG — фраза конфликтует с авторитетным
инвентарём `src/lib.rs:256-355`: восемь означает число lint-exception
регионов, тогда как внутри них находятся один `unsafe trait`, десять
`unsafe fn` declarations и шесть `unsafe {}` blocks. Читатель снова может
принять boundary count за operation count — ровно та неоднозначность, которую
исправлял `7306b86`.

**Рекомендация:** провести один узкий grep-driven sweep живых поверхностей и
везде, где число восемь относится к атрибутам, использовать единый термин
«item-scoped allow/lint-exception regions». `site` оставлять только там, где
речь действительно идёт об отдельной unsafe-декларации, блоке или операции.
Исторические review/ADR не переписывать.

### P3-2 — perf item 61 заявляет готовый gate, которого для его варианта нет

**Места:**

- `docs/perf/OPEN_ITEMS.md:2824-2856`, особенно `:2844-2853`;
- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:59,74-100`;
- `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md:58-66`.

Open item 61 предлагает пропускать повторный `store_next` после неудачного
push CAS, когда новый head требует того же `next_link`. Его status card
утверждает, что shared harness уже существует и кандидат «blocked only by»
ожидающим ARM64 wall-clock run.

Фактический driver материализует только три варианта:
`base`, `links_relaxed`, `cas_weak`. В `VARIANT_ANCHORS` нет варианта,
изменяющего retry-loop или устраняющего повторный link store. Поэтому запуск
существующего ARM64 job измерит items 62 и weak-CAS часть, но ничего не скажет
про item 61. Это не дефект runtime-кода, однако current-state index даёт
неверный следующий шаг и может привести к закрытию кандидата измерением,
которое его не содержало.

**Рекомендация:** до ARM64 dispatch добавить отдельный `store_elision`
вариант с exact-anchor/tripwire и явным activation oracle либо честно вернуть
item 61 в состояние «measurement variant not implemented». Вариант должен
различать CAS failures, на которых `next_link` сохранился, от failures с новым
head index; иначе он либо не активирует оптимизацию, либо пропустит нужную
перезапись. Только после этого wall-clock сравнение способно ответить на
вопрос об ускорении.

### P3-3 — документация снова растёт быстрее алгоритма и нарушает локальную модульность

**Места:**

- `crates/tagged-index-stack/src/imp.rs` — 2108 строк, из них 1536 comment/doc lines;
- `crates/tagged-index-stack/src/lib.rs` — 453 строки, из них 425 doc/comment lines;
- `crates/tagged-index-stack/tests/stack_unit.rs:670-700`;
- `crates/tagged-index-stack/src/imp.rs:509-538,1830-1848`;
- проектное правило `CLAUDE.md`, раздел `File and module structure`.

`49b437b` меняет одну семантическую операцию — truncating pack на checked pack
с `expect` — но добавляет 49 строк и удаляет 5: объяснение повторено в
`StackHead`, тонком `ArrayIndexStack` forwarder и длинной исторической
преамбуле теста. Сам тест полезен и невакуозен; проблема не в наличии boundary
coverage, а в повторении одной причины несколькими почти нормативными
абзацами с review/date provenance в коде.

Это продолжает уже измеренный drift: ADR консолидации фиксировал сокращение
`imp.rs` до 1887 строк и CHANGELOG до 250 строк; сейчас они снова выросли до
2108 и 316. Параллельно `imp.rs` содержит как минимум восемь top-level
публичных концепций (`TAIL`, `TaggedIndex`, `TagExhausted`, `StackHead`,
`StackStorage`, `StackOps`, `ArrayIndexStack`, `ArrayLinks`) и test hooks, хотя
локальная конвенция требует one file — one export/responsibility. Результат —
дорогие grep-инвентари, хрупкие line/path/count копии и повторяющийся
documentation drift; P3-1 является свежим примером.

**Рекомендация:** не урезать normative `# Safety`, ordering proofs, panic/error
контракты и counterfactual rationale. Удалять только review archaeology,
повторы и объяснения, которые уже доступны по ближайшей ссылке. Историю
оставлять в CHANGELOG/ADR/review. Отдельным механическим refactor разнести
представление, head, storage contracts, owned stack и links по приватным
модулям под тем же общим cfg-gate; заранее учесть, что A/B driver сейчас
копирует `imp.rs` и потребует синхронного обновления. Это P3 и не повод делать
рискованный структурный rewrite непосредственно перед публикацией.

## Обзор новых коммитов

Диапазон содержит три коммита, четыре затронутых файла, 63 добавления и 15
удалений. Статический `git diff --check` диапазона чист. Изменений обычного
production push/pop алгоритма, atomics и layout нет.

### `7306b86` — точное название восьми unsafe-регионов

Crate README теперь правильно говорит о восьми item-scoped
`#[allow(unsafe_code)]` lint-exception regions и объясняет отличие от
operation count. Это корректно закрывает указанное в раунде 12 место. Остаток
на других живых поверхностях вынесен в P3-1.

### `11e2e68` — durability row соответствует seal-протоколу

`docs/DURABILITY.md:34` теперь правильно фиксирует:

- tag расходуется только успешным push;
- pop сохраняет tag;
- production tag не оборачивается;
- при `TAG_MAX` push навсегда запечатан, а pop продолжает drain;
- оценка времени является lifetime budget до seal, не вероятностью ABA.

Формулировка согласуется с кодом и опубликованными crate docs. Замена имени
операции с pops на pushes и wrapping на permanent seal закрывает P3-3 раунда
12 полностью.

### `49b437b` — out-of-range тестовый tag больше не усекается

`StackHead::with_tag_for_test` теперь вызывает checked
`TaggedIndex::pack(...).expect(...)`; `ArrayIndexStack` корректно делегирует.
Для legal `INDEX_BITS` empty sentinel является допустимым index half, поэтому
единственная runtime failure здесь именно `tag > TAG_MAX`, как обещано в
`# Panics`.

Два новых теста образуют хороший boundary pair: `TAG_MAX` round-trips точно,
а `TAG_MAX + 1` обязан паниковать с целевым сообщением. Если вернуть прежний
`pack_truncating`, второй тест перестанет паниковать; oracle не вакуозен.
Условие находится под `test-internals`, и CI имеет whole-crate release row с
этой feature. Исправление закрывает P3-1 раунда 12; остаточный вопрос только в
объёме сопровождающей прозы (P3-3 этого отчёта).

## Полный обзор корректности и soundness

### Packed representation и арифметика

- `INDEX_BITS` принудительно ограничен `1..=16`; index half остаётся в `u32`,
  tag half имеет 48–63 бита, reserved empty index не совпадает с `TAIL`.
- `TAG_MAX = 2^TAG_BITS - 1` вычисляется без недопустимого shift count на всех
  разрешённых ширинах.
- Публичный `pack` отвергает обе переполненные половины. Приватный
  `pack_truncating` в production вызывается только после доказанных границ;
  test-only конструктор теперь также не пропускает непроверенный tag.
- Seal check расположен перед `store_next`; отказ первой попытки не оставляет
  side effect и возвращает ownership индекса вызывающему.

### Lock-free протокол и ordering

- `push_index_impl` валидирует index, читает head Relaxed только как пару
  значений, пишет link до публикации и делает strong CAS с
  `Release/Relaxed`.
- `pop_index_impl` начинает с Acquire head load, получает Acquire actual на
  CAS failure, загружает и проверяет link и публикует новый head strong CAS с
  `Acquire/Acquire`.
- Все модификации head остаются RMW. Поэтому Release CAS успешного push и
  последующие RMW образуют непрерывную release sequence; Acquire-only pop CAS
  не разрывает публикацию link для следующих наблюдателей.
- H-2 переход последнего элемента в empty сохраняет текущий tag. Повторяемого
  `(index, tag)` до seal нет, поэтому stale CAS не может успешно воскресить
  уже выданный индекс.
- Release-active guards отвергают out-of-range next и self-loop до публикации
  повреждённой головы. Глубокий acyclic corruption остаётся обязанностью
  unsafe implementor/caller, что документация сообщает честно.
- Backoff локален одному вызову, ограничен cap 6 и не меняет lock-free
  свойство; pop пропускает бесполезный spin, если failure уже показал empty.

### Unsafe/API boundary

- `StackStorage` — открытый `unsafe trait` с явными binding, backing,
  reachability, domain, liveness и atomic-access obligations.
- Все storage hooks и push boundary являются `unsafe fn`; их единственный
  crate-owned bridge содержит локальные `unsafe {}` и SAFETY-доказательства.
- Blanket `StackOps` удерживает алгоритм в крейте и не даёт implementor
  заменить протокол.
- `ArrayIndexStack` структурно связывает head и links, не реализует публичный
  `StackStorage` и не выдаёт свою приватную голову наружу.
- Обычная `test-internals` поверхность не даёт записывать link; mutating
  `store_next_for_test` остаётся только под `cfg(loom)`.
- В production source нет raw pointers, `transmute`, FFI, ручных `Send`/`Sync`
  и unmanaged resource lifecycle. Восемь allow-регионов остаются закрыты
  crate-level `deny(unsafe_code)` и `deny(unsafe_op_in_unsafe_fn)`.

## Тесты, features, package и CI — статическая оценка

Нормальная сборка не имеет внешних зависимостей. Loom optional и требует
одновременно feature и `cfg(loom)`; ошибочные cfg/target комбинации закрыты
именованными compile errors и общим cfg-gate implementation module.
`test-internals` выключена по умолчанию и не включает write-side hook.

По исходникам workflow имеются default/release tests, whole-crate
`test-internals` release row, default и feature-specific clippy, rustdoc,
MSRV, no-atomics error-shape, loom, compile-fail, packaged artifact и
отдельный codegen-wrapper build-check. Новый `with_tag_for_test` oracle
попадает в feature row. Эти пути не запускались в рамках ревью; подтверждена
только их статическая связанность.

## Производительность

Горячий код после раунда 12 не менялся, поэтому новых оснований заявлять
ускорение нет. Текущая структура разумна: один packed head atomic, один link
access, CAS-loop и bounded per-call backoff. Dense `ArrayLinks` способен
false-share, но универсальный padding увеличил бы footprint в 16 раз, а
custom slot-resident storage оставляет потребителю выбор layout.

Три прежних направления остаются содержательными:

1. Устранение повторного `store_next` на retry, если head index не изменился.
   Сначала нужен отдельный variant/oracle — P3-2.
2. Relaxed link-cell accesses имеют реальную AArch64 ISA-дельту, но native
   wall-clock результат ещё отсутствует.
3. Relaxed success ordering pop CAS выглядит допустимо по release-sequence
   доказательству, но отсутствует в driver и должно измеряться отдельным
   вариантом вместе с concurrency oracle.

Strong→weak CAS на текущем измеренном toolchain является codegen-null. Не
следует менять ordering, backoff или layout на основании статической
эстетики: сначала добавить недостающие варианты, выполнить target-native A/B
и менять runtime только при воспроизводимом выигрыше без ослабления proof.

## Итог публикационной готовности

Три новых исправления корректны, production-протокол остаётся согласованным,
а обнаруженные проблемы ограничены терминологией, measurement bookkeeping и
сопровождаемостью документации/структуры. Крейт **GO** по результатам этого
статического ревью. P3-1—P3-3 полезно закрыть, но они не требуют задерживать
публикацию.
