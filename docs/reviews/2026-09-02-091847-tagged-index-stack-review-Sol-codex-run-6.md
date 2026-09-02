# `tagged-index-stack`: предрелизное ревью — Sol-codex, прогон 6

- Время начала исследования: 2026-09-02 09:18:47 CEST (`Europe/Berlin`)
- Метка запроса: 09:16
- Проверенный `HEAD`: `4bbcebd178fd61a8dff6b3ab2c1c47a5b118e694`
- Последний коммит на момент начала: `4bbcebd` — `checkpoint: 2026-09-02-0912`
- Режим: только статическое чтение, самостоятельно, без под-агентов
- Не запускались: `cargo`, тесты, doctest, clippy, rustdoc, loom, benchmark, examples и scripts
- Изменение репозитория в рамках ревью: только этот отчёт и его коммит

## Область проверки

Заново просмотрены production-код и весь публичный API крейта, unsafe/atomic
контракты, тестовые и compile-fail оракулы, manifest/package surface, README и
CHANGELOG, benchmark/example harness, релевантные CI-строки, perf-решения и
интеграция с `sefer-alloc::Registry`. Отдельно просмотрены новые коммиты после
Sol-codex run 4: sealing `ArrayIndexStack`, перевод `StackStorage` в `unsafe
trait`, внедрение `Hook`, сужение индекса до `u32`, консолидация compile-fail
driver и исправление contention benchmark.

Это bounded single-context review: production-код прочитан полностью, весь
остальной surface проиндексирован, а рискованные тесты и документы прочитаны
прицельно. Выполнение кода было прямо запрещено, поэтому утверждения о текущем
фактическом прохождении CI/loom/package gates в этом отчёте не делаются.

## Вердикт

**NO-GO к публикации в текущем виде.**

Сам Treiber/CAS алгоритм на просмотренном `HEAD` выглядит согласованным, и
прежний блокер вокруг safe standalone `ArrayIndexStack` действительно закрыт.
Но новое решение с `Hook` переобещает более сильную capability-границу, чем
реально задаёт Rust API, а нормативный `unsafe trait` contract не распределяет
ответственность за изготовление, удержание и повторную экспозицию свидетеля.
Для низкоуровневого примитива, на эксклюзивность выдачи которого затем опирается
unsafe allocator code, это нужно исправить до первой публикации.

После устранения P1-1 и синхронизации связанных safety-утверждений новый
статический контроль границы будет оправдан. Остальные пункты не показывают
ошибки в основном CAS-алгоритме, но их выгодно закрыть сейчас, пока публичный
surface ещё можно ломать свободно.

## Блокирующая находка

### P1-1. `Hook` закрывает safe-синтаксис, но документация и unsafe contract объявляют его абсолютной capability-границей

Места:

- `crates/tagged-index-stack/src/imp.rs:464-486`, `570-609`, `1023-1047`;
- `crates/tagged-index-stack/src/lib.rs:232-265`;
- `crates/tagged-index-stack/README.md:64-68`;
- `crates/tagged-index-stack/tests/compile_fail.rs:223-293`;
- `crates/tagged-index-stack/tests/compile_fail/hook_token_unconstructible/src/main.rs:1-65`;
- `docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md:82-100`.

Что сделано правильно: `pub struct Hook(())` с private field нельзя создать
обычным safe Rust синтаксисом, а `&Hook` нельзя безопасно сохранить за пределами
полученного borrow. Это закрывает прежний полностью safe вызов
`pool.store_next(...)` против корректного внешнего implementor.

Что заявлено сильнее этого:

- «no code outside this crate can construct a `Hook` value by any spelling»;
- hooks «unreachable from outside this crate regardless of what is in scope»;
- ссылочная форма якобы делает любое stashing невозможным;
- compile-fail fixture назван доказательством всей этой границы.

Публичный `Hook` — inhabited ZST с единственным representationally valid
значением. Downstream unsafe code может изготовить такое значение, например
через `MaybeUninit::<Hook>::zeroed().assume_init()`/`transmute`, либо внешний
`unsafe impl StackStorage` может сохранить полученный настоящий `&Hook` как raw
pointer и позднее обернуть его использование в собственный safe method.
Приватность поля блокирует safe-конструктор; она не делает значение физически
неподделываемым и не мешает raw-pointer stashing.

Это не утверждение, что чистый safe downstream сегодня ломает
`ArrayIndexStack`: не ломает. Проблема в распределении unsafe-ответственности.
Текущий `StackStorage::# Safety` перечисляет пять storage invariants, но не
говорит, что implementor обязан не удерживать, не копировать через unsafe, не
передавать callback-у и не реэкспортировать witness, а внешний unsafe-код не
должен его изготавливать. Одновременно сами `head/load_next/store_next` остаются
safe `fn`, хотя прямой `store_next` способен разрушить chain, после чего safe
`StackOps` может повторно выдать индекс. В `Registry` это уже граница allocator
soundness, а не только локальная порча контейнера.

Compile-fail fixture проверяет ровно две safe-синтаксические формы: отсутствие
аргумента и `Hook(())` с private field. Он не может доказывать более сильное
утверждение о любом внешнем коде или о поведении внешнего `unsafe impl`.
Следовательно, оракул корректен только после сужения формулировки до
«неполучаемо безопасным downstream-кодом».

Лучшее решение без оглядки на совместимость:

1. Удалить `Hook` как псевдо-capability.
2. Оставить `StackStorage` unsafe trait для implementor-side обязательств.
3. Сделать `head`, `load_next`, `store_next` `unsafe fn` с отдельными
   caller-side `# Safety` условиями.
4. В crate-private bridge оставить три минимальных `unsafe` вызова с
   локальными доказательствами: genuine binding обеспечен `unsafe impl`, а
   вызовы выполняются только CAS-алгоритмом в допустимой фазе.
5. Compile-fail оракул должен проверять, что прямой вызов hook без `unsafe`
   невозможен; compile-pass — что корректный внешний storage по-прежнему
   работает через safe `StackOps`.

Это честная граница: trait declaration назначает ответственность implementor,
а unsafe methods — прямому вызывающему. Если `Hook` всё же сохранять, минимально
необходимо сузить все claims до safe code и добавить в нормативный `# Safety`
контракт запрет fabrication/retention/re-export/callback leakage. Для заявленной
цели «совершенство без компромиссов» unsafe-method seam проще и надёжнее.

## Существенные улучшения до публикации

### P2-1. Test-only `pub` API уже становится реальным published surface

Места: `src/imp.rs:417-429`, `1595-1600`, `1621-1635`; README `200-210`.

В default build публичны `StackHead::raw_head`, `ArrayIndexStack::raw_head` и
`ArrayIndexStack::load_next_for_test`. Документация одновременно говорит, что
они существуют только ради собственных integration tests и «carry no semver
stability guarantee». `#[doc(hidden)]` скрывает навигацию rustdoc, но не
публичность, nameability или downstream use; декларация об отсутствии гарантии
не уменьшает уже опубликованный API.

До первой публикации следует выбрать одно из двух:

- признать эти операции поддерживаемой diagnostics/introspection API,
  переименовать без `_for_test` и дать стабильный контракт;
- убрать их из default published surface: перенести white-box проверки в unit
  tests либо feature/cfg-гейтировать тестовые probes и явно запускать нужный
  тестовый target с этим feature.

`test-internals` counters и loom-only CAS уже следуют второй модели; default
raw/link probes сейчас выбиваются из неё.

### P2-2. Unsafe inventory сформулирован и «самопроверяется» неверно

Места: `src/lib.rs:18-20`, `232-248`; README `24-27`.

Production `src/` действительно содержит один синтаксический unsafe seam —
`pub unsafe trait StackStorage` — и ни одного unsafe block/function. Но текст
говорит «exactly ONE unsafe token anywhere in the crate». Integration tests
содержат многочисленные `unsafe impl StackStorage`; они являются отдельными
crate targets и не наследуют `#![deny(unsafe_code)]` из library root. Более
того, предложенная self-verification команда ищет только
`allow(unsafe_code)`, а не unsafe tokens, поэтому заявленное количество она не
проверяет.

Нужно писать «production library source (`src/`)» и разделить два инвентаря:
unsafe syntax production target и намеренно broken/correct unsafe impl fixtures
в repository tests. Команда проверки должна соответствовать заявлению, а не
только считать lint exceptions.

### P2-3. Интеграционная durability-документация осталась на удалённой архитектуре

Места:

- `docs/DURABILITY.md:16-18,33`;
- `src/registry/heap_registry.rs:22-32`;
- `tests/regression_counter_wrap.rs:15-20`.

Документ, называющий себя authoritative inventory, всё ещё ссылается на
удалённый `src/registry/tagged_ptr.rs`, удалённый
`crates/tagged-index-stack/tests/regression_counter_wrap.rs`, старое имя
`TaggedPtr` и старый const assert. Root test повторяет ссылку на уже отсутствующий
crate test. `heap_registry.rs` называет budget «~89-year», тогда как актуальный
crate rustdoc честно даёт разные режимы: примерно 89 лет при 100k операций/с,
но только 3.3–16 дней при заявленном hardware-saturation ceiling.

Это не меняет арифметику текущего кода, но делает внешнюю аудит-трассу ложной.
После extraction нужно ссылаться на `TaggedIndex`/`StackStorage`, существующие
oracle names и явно указывать workload rate рядом с 89-летним числом.

## Производительность и измерения

### P3-1. У `Relaxed` links есть реальный AArch64 codegen delta; wall-clock решение ещё не измерено

Места: `src/imp.rs:537-561`, `1320-1330`;
`docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md`.

Perf-исследование уже установило:

- strong и weak CAS дают идентичный codegen на проверенных x86-64/AArch64
  lowering — менять CAS kind сейчас оснований нет;
- `Acquire`/`Release` link cells на AArch64 дают реальные `ldar/stlr`, а
  `Relaxed` их удаляет;
- native AArch64 wall-clock результата пока нет.

Поэтому тексты «real but unmeasured cost» и «pending a multi-target A/B
measurement» в `imp.rs` устарели: static multi-target A/B уже выполнен, не
измерен именно native weak-memory wall-clock эффект. Код правильно оставлен без
изменения; следующая полезная работа — только нативный AArch64 timing gate, а не
ещё одна x86 выборка и не спекулятивная замена ordering.

Заодно ordering contract лучше формулировать как «Acquire/Release or stronger»:
сейчас буквальное MUST запрещает `SeqCst`, хотя более сильное ordering не
нарушает требуемую синхронизацию.

### P3-2. Latency example начинает wall timer после release barrier

Место: `examples/backoff_per_call_latency.rs:170-205`.

Workers и coordinator выходят из одного `Barrier::wait()`, после чего workers
могут уже выполнять первые pop/push, а coordinator лишь затем вызывает
`Instant::now()`. Поэтому `wall_ms` способен не включать начало фактически
выполненной работы. Per-call samples от этого не теряются, но wall-throughput
denominator систематически коротковат и зависит от scheduling.

Использовать уже исправленную схему benchmark: ready barrier → coordinator
публикует start → второй barrier/window release. Либо измерять wall interval до
release, честно включая release skew. Первая схема точнее.

### P4-1. `TIS_CAP_LABEL` может сломать JSONL

Место: `examples/backoff_per_call_latency.rs:125-127,222-228`.

`cap_label` вставляется в JSON вручную без escaping. Значение с `"`, `\\` или
переводом строки делает output невалидным JSONL и может сломать derivation
script. Для zero-dependency probe достаточно ограничить label безопасным
алфавитом и падать на другом вводе либо реализовать минимальное JSON string
escaping.

## Качество кода и документации

### P4-2. Остались механические следы консолидации

- `src/imp.rs:305-307`: сломанная фраза `Owned by  /// [StackStorage]`.
- `Cargo.toml:22-25`: комментарий всё ещё описывает несколько
  `compile_fail_*.rs` drivers, хотя теперь есть один `tests/compile_fail.rs`.
- `.github/workflows/ci.yml:765-768`: комментарий фиксирует старые «four
  fixtures / three driver files» вместо нынешней консолидированной схемы.
- README `:32` говорит об «exhaustive loom model-check run against the real
  type» без слова bounded; точная формулировка ниже есть и должна быть поднята
  в headline: exhaustive только внутри перечисленных малых моделей.

Это не runtime-дефекты, но именно такие stale claims снова и снова создавали
ложные оракулы в предыдущих раундах. Их лучше удалить одним doc-sync проходом.

## Что в новых правках сделано хорошо

- `ArrayIndexStack` больше не реализует публичный `StackStorage`; head закрыт
  private sealed bridge, а competing binding для shipped safe type действительно
  стал невыразим обычным downstream API.
- `StackStorage` стал `unsafe trait` с явными storage-binding обязанностями;
  корректный и намеренно нарушающие impl sites разделены в тестовой
  документации.
- Индексная половина API последовательно сужена до `u32`; опасное truncating
  packing осталось crate-private, а public `pack` checked.
- Диапазоны `INDEX_BITS=1..=16`, sentinel, H-2 empty transition и tag wrap
  согласованы между кодом и основной crate-документацией.
- Push initial load `Relaxed`, push CAS `Release/Relaxed`, pop head/failure
  `Acquire`, pop success `Acquire`; при текущем инварианте «все head writes —
  RMW» release-sequence proof выглядит цельным.
- Pop проверяет invalid link и self-loop release-active; panic paths вынесены
  cold/noinline и сохраняют caller location.
- Backoff saturating, instrumentation отсутствует в default production path.
- Contention benchmark теперь публикует одно окно после ready rendezvous,
  проверяет entry lateness и не смешивает старые элементы со следующим phase.
- Compile-fail boilerplate консолидирован, packaged-test отсутствие fixture
  directories обработано явно.
- MSRV заявлен как library-surface floor и имеет отдельные CI check rows;
  нормальная сборка не имеет обязательных runtime dependencies.
- Loom suite статически содержит positive models, retry-activation oracles и
  `#[should_panic(expected = ...)]` counterfactuals; формулировка exhaustive
  внутри малых моделей в самом test module точная.

## Краткий путь к GO

1. Закрыть P1-1 честной caller-side unsafe boundary; предпочтительно убрать
   `Hook` и сделать hooks `unsafe fn` с локальными bridge proofs.
2. Переписать Hook/unsafe-inventory/compile-fail claims в соответствии с
   фактически проверяемой гарантией.
3. Убрать или официально поддержать default-public test probes.
4. Синхронизировать durability, manifest и CI комментарии после extraction и
   test consolidation.
5. Исправить latency wall start и JSON escaping.
6. Не менять link ordering без native AArch64 wall-clock результата; weak CAS
   по уже собранным данным оставить strong.

После пункта 1 нужен новый статический аудит именно изменённой safe/unsafe
границы. На текущем `HEAD` публикацию не рекомендую.
