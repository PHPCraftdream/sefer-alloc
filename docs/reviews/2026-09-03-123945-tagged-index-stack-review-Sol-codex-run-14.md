# tagged-index-stack — предрелизное статическое ревью

**Ревьюер:** Sol-codex

**Раунд:** Codex run 14

**Метка пользователя:** 2026-09-03 12:29

**Время фиксации ревью:** 2026-09-03 12:39:45 +02:00

**Проверенная ревизия:** `6f9b04fb56e3a8fe9e2a20767300d5a7d1e87861`

**Диапазон новых правок:** `d329757ce9a2d837118307f52b6536a338dd08d7..6f9b04fb56e3a8fe9e2a20767300d5a7d1e87861`

## Вердикт

**NO-GO к публикации в текущем виде.** В production-алгоритме push/pop, packed
representation, seal-протоколе и atomic ordering новой ошибки не найдено, однако
обнаружен **P1-дефект публичного unsafe-контракта**: `StackOps::push_index` не требует
эксклюзивного права на повторную публикацию индекса и не запрещает два одновременно
выполняющихся push одного отсутствующего индекса. Оба вызова могут удовлетворять
буквально записанному предусловию на входе, но проигравший CAS затем способен записать
`next[index] = index` и успешно опубликовать self-loop. Для unsafe API контракт — часть
границы soundness, поэтому подразумеваемого требования недостаточно.

Также найдены две существенные проверяемые неточности:

- заявленная self-verifying команда инвентаризации unsafe возвращает 12 совпадений,
  а не обещанные 8;
- A/B wall-clock harness начинает общее измерительное окно **до** заявленного
  неучитываемого warm-up, из-за чего его протокол не соответствует собственному
  описанию и эталонному benchmark.

До публикации следует закрыть P1 и исправить обе ложные проверяемые декларации.
Оставшиеся P3 относятся к точности layout-документации, хрупким line-ссылкам и
сопровождаемости, а не к найденной ошибке production hot path.

## Методика и границы

Ревью выполнено лично, без под-агентов, как ограниченный single-context pass.
Проведён полный статический проход по `src/lib.rs` и `src/imp.rs`, manifest, README,
CHANGELOG, всем test targets и compile-fail fixtures, benchmark/example, A/B runner и
его шаблонам, релевантным CI workflow и perf-документам. Для проверки реального
caller-side инварианта прочитан workspace-потребитель в `heap_registry.rs`.

Глубоко проверялись unsafe/public API contracts, concurrency и atomic ordering,
состояния и переходы, арифметика packed word, panic/error paths, feature/cfg-матрица,
зависимости, тестовые оракулы, документационное соответствие и performance path.
В production-коде не обнаружены async, FFI, криптография, raw pointers, ручные
`Send`/`Sync` или ресурсный Drop/RAII-протокол; эти категории проверены сканированием
на наличие, но не образуют отдельного механизма для глубокого аудита.

По требованию пользователя ничего не исполнялось: не запускались `cargo`, `rustc`,
clippy, rustdoc, тесты, loom, Miri, benchmarks, examples, Node-скрипты, package/publish
и сгенерированные бинарники. Поэтому не подтверждены runtime-поведение, фактическая
матрица платформ, содержимое package artifact и воспроизводимость прежних измерений.
`git diff --check` проверенного диапазона статически чист.

## Найденные проблемы

### P1 — unsafe-контракт push не запрещает конкурентную публикацию одного индекса

**Места:**

- `crates/tagged-index-stack/src/imp.rs:1027-1086` — канонический `# Safety` для
  `StackOps::push_index`;
- `crates/tagged-index-stack/src/imp.rs:1047-1066` — текущая liveness-оговорка;
- `crates/tagged-index-stack/tests/loom_aba.rs:857-918` — concurrent push проверяется
  только для **разных** индексов;
- `src/registry/heap_registry.rs:806-821` — реальный потребитель доказывает более
  сильный инвариант единственного владельца.

Контракт сейчас состоит из link-domain и liveness-условия: индекс «не должен сейчас
быть достижим», то есть либо никогда не публиковался, либо был возвращён успешным pop
и после этого не публиковался снова. Это покрывает последовательный double-push, но
не формулирует временное владение индексом на всём протяжении вызова.

Контрпример для свежего пустого стека:

1. Потоки A и B одновременно вызывают `unsafe push(X)` для одного `X`.
2. В момент входа `X` ещё не достижим; каждый вызов удовлетворяет буквальному
   текущему тексту обеих safety-клауз.
3. A записывает link и выигрывает CAS, публикуя `X`.
4. B проигрывает CAS, повторяет цикл, видит `X` в head и записывает `next[X] = X`.
5. CAS B может успешно опубликовать новый tag с тем же head index; стек содержит
   self-loop, а последующий pop обнаруживает повреждение и паникует.

При промежуточном pop возможна родственная проблема: один участник уже считает индекс
своим после pop, пока другой вызов всё ещё имеет право повторить его push. Для
allocator-потребителя это угрожает exclusive issuance. Код `heap_registry` фактически
предотвращает сценарий отдельной slot-state machine и прямо доказывает «at most one
owner», но публичный крейт не требует этого от остальных implementor/caller пар.

Это не доказательство ошибки CAS-алгоритма при предполагаемом уникальном владении.
Это дефект нормативной границы: безопасная абстракция над публичным `unsafe fn` может
быть написана по опубликованным условиям и всё же допустить повреждение.

**Минимальное исправление до публикации:** явно потребовать, что вызывающий обладает
эксклюзивным recycle/publish authority для `index`; ни один другой push того же индекса
не выполняется и не может начаться до возврата вызова; свежий индекс создан уникально
либо право владения получено от последнего успешного pop. Зафиксировать переходы:
при `Ok` владение передано стеку, при `Err(TagExhausted)` остаётся у вызывающего.
Тот же текст или однозначную ссылку на него дать forwarding API и README.

Добавить целевой concurrency oracle: либо Loom-контрпример, намеренно демонстрирующий
self-loop при нарушении новой клаузы, либо положительную модель ownership-обёртки,
которая структурно исключает два push одного индекса. Лучшее долгосрочное API —
не-`Copy` capability/token, возвращаемый pop и потребляемый push, с отдельным резко
unsafe путём первоначального mint. Тогда основная часть временного обязательства
будет выражена типами, а не только прозой.

### P2-1 — команда self-verifying unsafe inventory выдаёт неверный результат

**Места:**

- `crates/tagged-index-stack/src/lib.rs:321-340`;
- `crates/tagged-index-stack/CHANGELOG.md:224-227`;
- дополнительные совпадения:
  `scripts/tis_p3_ab/harness_bin.rs:104,153,178` и
  `scripts/tis_p3_ab/codegen_wrapper.rs.tmpl:44`.

Документация предлагает выполнить из корня workspace:

```text
grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' crates/tagged-index-stack/
```

и обещает ровно восемь совпадений, все в `src/imp.rs`. Статический эквивалентный поиск
по текущему дереву возвращает **12**: восемь production-регионов в `src/imp.rs` и
четыре атрибута в tracked perf harness/template. Поэтому команда не самопроверяет
заявленный инвариант и немедленно противоречит следующей строке документации.

**Исправление:** ограничить команду `crates/tagged-index-stack/src/` и точно назвать
результат «восемь lint-exception regions в production library source». Если нужен
whole-crate inventory, отдельно перечислить четыре script/template-региона и не
смешивать их с поверхностью библиотечного target. Синхронно исправить CHANGELOG.

### P2-2 — A/B harness учитывает время, объявленное неучитываемым warm-up

**Места:**

- `crates/tagged-index-stack/scripts/tis_p3_ab/harness_bin.rs:116-166,198-207`;
- `crates/tagged-index-stack/benches/tagged_index_stack_bench.rs:122-220` — корректный
  эталон;
- `crates/tagged-index-stack/CHANGELOG.md:202-203` — декларация протокола.

Координатор harness записывает `timed_start = Instant::now()` и затем отпускает общий
barrier. Рабочий поток только после barrier читает `timed_start`, выполняет порцию
якобы неучитываемого warm-up и прекращает её при `now >= timed_start`. Это условие
неизбежно истинно уже при первой проверке: измерительное окно открыто до выхода
worker из barrier. Получается примерно одна неучитываемая порция **внутри уже идущего
окна**, после чего каждый worker начинает счёт со своей задержкой, но заканчивает по
общему deadline. Обещанное полное общее `[timed_start, deadline)` окно для каждого
worker не реализовано.

Benchmark делает требуемое правильно: публикует будущий
`timed_start = Instant::now() + WARMUP`, прогревается до него и имеет oracle на
опоздание входа. Harness не повторяет ни будущий anchor, ни entry-lateness guard.

**Исправление:** зеркально перенести benchmark protocol: будущий `timed_start`, второй
barrier, warm-up до anchor, entry-lateness oracle и явный вывод warm-up параметра.
Альтернатива — отдельная фаза warm-up, затем barrier и только после неё публикация
начала counted window. До исправления не использовать wall-clock результаты этого
harness как gate для ослабления atomics; особенно это важно для ожидаемого ARM64 run.

### P3-1 — ссылки OPEN_ITEMS item 63 уже указывают не на заявленные строки

**Места:** `docs/perf/OPEN_ITEMS.md:2922,2937,2944`.

Item 63 ссылается на список вариантов как `tis_p3_ab_runner.mjs:39`, тогда как
`VARIANTS = ['base', 'links_relaxed', 'cas_weak']` сейчас находится на строке 59.
Ссылки на CHANGELOG `:181-187` также захватывают конец обсуждения weak CAS и обрезают
текущий блок pop-success-ordering, расположенный примерно на `:186-192`.

**Исправление:** обновить диапазоны либо предпочесть стабильные имена символов и
заголовков. Исправленный в новых коммитах item 61 уже использует строку 59; соседний
item 63 показывает, насколько быстро расходятся ручные exact-line ссылки.

### P3-2 — точное layout-утверждение StackHead не обеспечено repr

**Место:** `crates/tagged-index-stack/src/imp.rs:383-394,412`.

Документация утверждает, что `StackHead` — bare `AtomicU64` «with no padding or
alignment of its own». Но это одно-field структура с обычным `repr(Rust)`, для
которого язык не закрепляет публичное точное layout-равенство полю. Практический
layout ожидаемо совпадает, и зависимости FFI/внешнего layout в ревью не найдены,
однако обещание сильнее формального API.

**Исправление:** если точное равенство layout является намеренным контрактом — добавить
`#[repr(transparent)]`; иначе ослабить текст до «contains only an AtomicU64 and adds no
explicit padding/alignment». Это точность документации, не доказанное ускорение.

### P3-3 — нормативный код остаётся перегружен review archaeology

`src/imp.rs` содержит 2107 строк, из них около 1535 comment/doc lines; `src/lib.rs` —
453 строки, из них около 425 comment/doc lines. Один `imp.rs` объединяет packed value,
errors, head, storage contract, algorithm facade, owned stack, links и test hooks.
Коммит `15778e9` полезно убрал часть повтора, но системную стоимость не меняет.

Нельзя сокращать normative `# Safety`, ordering proofs, error semantics и причины
release-active guards. Следует вынести исторические сведения о ревью/измерениях в ADR,
сократить дубли и разнести внутренности по приватным модулям под тем же общим cfg-gate.
Это снизит drift вроде P2-1/P3-1 и облегчит повторный аудит без рискованного rewrite
самого алгоритма перед публикацией.

## Обзор новых коммитов

После отчёта раунда 13 проверены четыре коммита:

- `f6abce6` — термин `sites` заменён на точное «lint-exception regions» в оставшихся
  живых местах. Правка корректна и закрывает предыдущий терминологический P3.
- `15778e9` — сокращено повторное объяснение `with_tag_for_test`. Поведение не меняется,
  локальная читаемость улучшена.
- `f83c90e` — OPEN_ITEMS item 61 теперь честно говорит, что вариант store-elision в
  runner отсутствует. Это соответствует текущему списку трёх вариантов и закрывает
  прежнее ложное состояние готовности.
- `6f9b04f` — версии GitHub Actions обновлены с `setup-node@v4` до `v5` и с
  `upload-artifact@v4` до `v6`; статический diff не меняет команды или логику jobs.
  Workflow не исполнялся, поэтому здесь подтверждена только локальная связность diff.

Production `src` hot path в этом диапазоне семантически не изменён. Новые проблемы
P1 и P2-2 обнаружены полным просмотром текущего состояния, а не внесены этими четырьмя
коммитами. `git diff --check` диапазона замечаний не дал.

## Полный обзор production-кода

### Представление и арифметика

- `INDEX_BITS` compile-time ограничен диапазоном `1..=16`; tag получает 48–63 бита.
- `TAIL`, `INDEX_MASK`, `TAG_MAX`, сдвиги и checked pack согласованы на всех разрешённых
  ширинах; недопустимого shift count не найдено.
- Публичный `pack` отвергает переполнение обеих половин. Production-вызовы
  truncating pack предшествуют доказанным границам; test-only tag constructor теперь
  также использует checked path.
- Push проверяет seal до `store_next`: `TagExhausted` не оставляет частичный link-side
  effect и сохраняет ownership у вызывающего.

### Lock-free протокол

- Push: Relaxed head load, запись link до публикации, strong CAS с
  `Release/Relaxed`, bounded per-call backoff с cap 6.
- Pop: Acquire head load, Acquire link load, guards на out-of-domain/self-loop,
  strong CAS с `Acquire/Acquire` и повтор с фактическим head после failure.
- Все записи head являются RMW; release sequence от успешного push сохраняется через
  pop RMW. Empty transition сохраняет текущий tag, а tag никогда не оборачивается:
  повторная установка старой `(index, tag)` до seal не найдена.
- При `TAG_MAX` push навсегда закрыт, но pop может дренировать существующий список.
- Release-active проверки не дают опубликовать непосредственно наблюдаемый invalid
  next/self-loop. Более глубокая acyclic corruption остаётся обязанностью unsafe
  storage/caller, что документация в целом сообщает честно.

При условии **усиленного exclusive-ownership предусловия из P1** сам алгоритм выглядит
корректным. Новых data race, ABA window, lost-node или ordering дефектов не найдено.

### Unsafe/API boundary

- `StackStorage` имеет подробные backing, binding, reachability, domain и atomic-access
  обязательства; blanket `StackOps` не позволяет implementor подменить алгоритм.
- `ArrayIndexStack` связывает head и links структурно и не реализует наружный
  `StackStorage`, поэтому его private head нельзя случайно rebinding-нуть.
- `test-internals` не открывает production write hook; `store_next_for_test` остаётся
  только под `cfg(loom)`.
- Восемь production `#[allow(unsafe_code)]` регионов локализованы в `src/imp.rs` и
  находятся под crate-wide `deny(unsafe_code)`/`deny(unsafe_op_in_unsafe_fn)`.
- Критический остаток — не количество регионов, а неполная временная клауза `push`
  из P1.

## Тесты, features, зависимости и CI — статическая оценка

У обычной сборки нет normal dependencies. Loom optional и требует одновременно
feature и `cfg(loom)`; ошибочные комбинации имеют явные compile errors. `test-internals`
выключен по умолчанию. Dev-зависимости ограничены proptest и bench tooling.

Статически присутствует покрытие packing boundaries, seal, custom storage,
shared-storage hazards, threaded conservation, compile-fail unsafe boundaries,
track-caller и Loom ABA/interleavings. Полезные оракулы обычно проверяют активацию
целевой ветви, а не только итог. Пробел, связанный с новым P1: concurrent push модели
используют разные индексы и потому не фиксируют требуемое эксклюзивное право на один
индекс.

Workflow описывает default/release tests, feature rows, clippy, rustdoc, MSRV,
no-atomic target error shape, loom, compile-fail, package и codegen-wrapper checks.
Ни одна строка не запускалась в этом ревью. Изменения action major versions оценены
только по diff; фактическая совместимость должна подтверждаться обычным release CI.

## Возможности ускорения

Нового безопасного ускорения, которое можно рекомендовать без измерения, не найдено.
Текущий hot path уже компактен: один packed head atomic, один link access, CAS-loop и
ограниченный backoff. Dense `ArrayLinks` может false-share, но безусловный cache-line
padding увеличил бы footprint примерно в 16 раз; custom slot-resident storage оставляет
layout решением потребителя.

Исследовательские кандидаты остаются разумными, но должны измеряться раздельно:

1. пропуск повторного `store_next`, если после CAS failure head index не изменился —
   нужен отдельный вариант и activation oracle;
2. Relaxed link-cell operations — есть статическая ISA-мотивация для AArch64, но нет
   принятого native wall-clock результата;
3. Relaxed success ordering pop CAS — требуется отдельный вариант и concurrency proof;
4. strong→weak CAS по текущим записанным данным является codegen-null.

До любого решения по этим кандидатам необходимо исправить P2-2: существующий A/B
wall-clock harness не реализует заявленное окно и не является надёжным gate. Изменять
ordering, backoff или layout только по эстетическому рассуждению не следует.

## Критерии перехода к GO

1. Дополнить канонический `push_index # Safety` эксклюзивным временным ownership
   обязательством и синхронизировать публичные forwarding docs/README.
2. Зафиксировать same-index concurrent-push counterexample или структурный ownership
   oracle, чтобы следующая редакция контракта не регрессировала.
3. Исправить область self-verifying unsafe grep и CHANGELOG: команда должна выдавать
   ровно тот результат, который обещает текст.
4. Исправить warm-up/window протокол A/B harness либо удалить утверждение о протоколе
   и не использовать текущие wall-clock данные как основание для atomic change.
5. Обычный release pipeline выполнить вне рамок этого read-only ревью.

После пунктов 1–4 статический блокер будет снят. P3-1—P3-3 желательно закрыть, но сами
по себе они не требуют удерживать публикацию.
