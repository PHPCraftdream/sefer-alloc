# size-classes: независимый pre-publish review, round-3 (MS)

Дата: 2026-08-27 20:18:21 Europe/Berlin  
Проверенный HEAD: `017401db1da95d494db4541cf6a269f16e7ae279`  
Вердикт: **NO-GO**

## Режим и ограничения

Проверка выполнена с нуля, только чтением текущего дерева. Исходники, manifest, lockfile, staging и существующие изменения не изменялись; единственная запись — этот отчёт. Под-агенты не использовались. Предыдущие отчёты, checkpoints и содержимое `docs/reviews` не читались. История Git не просматривалась.

По прямому запрету не запускались тесты, сборка, `cargo check`, clippy, rustdoc, benchmarks, package/publish и вообще никакие Cargo-команды. Поэтому это статический аудит, а не подтверждение зелёного toolchain gate. Статус рабочего дерева перед записью отчёта был чистым; проверка статуса была read-only.

`rust-intel` применён вручную в одном контексте, поскольку задача прямо запрещает его штатный fan-out по под-агентам. Полнота относится ко всему коду и конфигурации самого небольшого синхронного safe-Rust крейта; глубокие async/concurrency/FFI/crypto-проходы неприменимы: в production-коде нет async, потоков, атомиков, FFI, криптографии, raw pointers или `unsafe`. Ограничение single-context всё же означает, что это не эквивалент рекомендуемого skill-модулем multi-agent аудита.

## Охват

Прочитаны полностью или по релевантным диапазонам:

- `crates/size-classes/src/lib.rs` — весь production-код и весь публичный API;
- `Cargo.toml`, `Cargo.lock`, `crates/size-classes/Cargo.toml`, обе лицензии, README и CHANGELOG;
- все три test source-файла (`builder.rs`, `proptest_builder.rs`, `tests/common/mod.rs`) и benchmark;
- workspace lint/MSRV/dependency wiring, `src/alloc_core/size_classes.rs`, реальные вызовы из allocator consumer;
- относящиеся к крейту части `.github/workflows/ci.yml` и весь publish path в `.github/workflows/release.yml`;
- package file inventory через `git ls-files`, а также статические упоминания/consumer call sites по репозиторию.

Не проверялись динамически: фактическое содержимое `.crate`, нормализованный опубликованный manifest, компиляция на Rust 1.88/32-bit/bare metal, результаты CI, docs.rs rendering, фактические benchmark numbers и доступность имени на crates.io.

## Краткая оценка

Ядро алгоритма выглядит численно корректным на поддерживаемых сегодня 32/64-bit `usize`: рост считается в `u128`, конечный класс проверяется на вместимость в `usize`, fallback использует `checked_add`, длина LUT — `checked_add`, bucket product и slow-path round-up защищены от переполнения. Merge сохраняет порядок и отдельно ловит collision extra/geometric. `class_for` для валидного power-of-two `align` действительно возвращает минимальный класс, удовлетворяющий size и stride-divisibility, либо `None`; jump монотонно продвигается и завершается. Production-код полностью safe, `no_std`, без runtime dependencies и без аллокаций.

Тестовая база существенно сильнее обычной для такого крейта: есть независимый линейный classifier oracle, отдельный reference builder, boundary/golden tests, debug/release-aware panic test, 32/64-bit overflow fixtures, предел 256 индексов, экстремальные bucket/round-up случаи, три дополнительные схемы proptest и path-activation oracles для benchmark. Root `sefer-alloc` является реальным path+version consumer и использует API на allocator paths с `Layout`-валидным alignment.

Тем не менее публикация в текущем состоянии механически заблокирована CHANGELOG, а до фиксации публичной документации и явного решения по опасному unchecked-by-contract API поверхность 0.1.0 не следует замораживать.

## Findings

### P0 — критические

Не найдено. В крейте нет `unsafe`, FFI, shared mutable state, crypto или runtime resource lifecycle.

### P1 — блокирующие публикацию

#### P1-1. Release workflow гарантированно отвергнет текущий CHANGELOG

- `crates/size-classes/CHANGELOG.md:7` содержит `## 0.1.0 - Unreleased`.
- `.github/workflows/release.yml:215-309` для любого реального publish требует ровно одну секцию текущей версии и явно падает, если она содержит `unreleased`.

То есть текущее дерево не только процессно, но и механически не готово к публикации. Перед тегом/не-dry-run dispatch нужно заменить `Unreleased` на реальную дату и повторно проверить release gate.

### P2 — важные до заморозки 0.1.0

#### P2-1. Основной safe API допускает release-silent неправильную классификацию

`SizeClasses::class_for(size, align)` (`src/lib.rs:804-853`) требует power-of-two `align`, но проверяет это только `debug_assert!`. В release валидный Rust-вызов с ошибочным alignment может:

- принять неделимый block на fast path;
- перескочить подходящий класс либо вернуть ложный `None` на jump path;
- принять класс, не удовлетворяющий заявленному `% align == 0`;
- при `(size, align) == (0, 0)` паниковать на LUT index.

Контракт подробно документирован, и `try_class_for` закрывает безопасный общий случай, поэтому это не memory-safety дефект внутри крейта. Но название обычного safe метода выглядит как основной classifier API, а неправильный результат в allocator consumer способен стать входом в последующий unsafe код и нарушить внешний `Layout` contract. До первой публикации стоит явно выбрать один из вариантов:

1. сделать `class_for` checked API, а доверенный hot path назвать так, чтобы assumption был виден в имени;
2. оставить дизайн, но зафиксировать его как осознанный 0.1.0 contract и добавить benchmark `class_for` vs `try_class_for`, подтверждающий, что release-active `is_power_of_two` действительно стоит отдельной поверхности.

Сейчас benchmark не измеряет стоимость проверки, поэтому ключевой аргумент «zero cost on hot path» не имеет собственного статического/измерительного gate.

#### P2-2. Публичный rustdoc неверно описывает overflow в const context

`src/lib.rs:171-174` утверждает, что unchecked `+ 1` мог бы завернуться в release-profile const evaluation, потому что const-eval якобы следует profile `overflow-checks`. Это неверно: обязательное вычисление overflow в const context является compile error; profile-dependent wrap относится к runtime-вызову `const fn` вне const context. Текущий код всё равно правильно использует `checked_add`, и runtime correctness от этого finding не страдает, но численный контракт документации должен быть точным. Аналогичную формулировку следует убрать и из внутренних комментариев около `src/lib.rs:322-326`.

Источник семантики: Rust Reference, `const_eval.const-expr.error` — <https://doc.rust-lang.org/reference/const_eval.html#constant-expressions>.

#### P2-3. CHANGELOG противоречит текущему контракту `class_for`

`CHANGELOG.md:73-75` говорит, что нарушение alignment-precondition даёт «suboptimal/wrong class choice, not memory unsafety». Текущий rustdoc `src/lib.rs:758-766` отдельно признаёт возможную панику, а `src/lib.rs:795-802` — safety-critical downstream consequence для allocator. Для release notes формулировка слишком абсолютна и пропускает конкретный `(0, 0)` panic. Нужно синхронизировать CHANGELOG с README/rustdoc: внутри крейта UB нет, но возможны wrong answer или panic, а downstream allocator обязан не превращать такой ответ в нарушение `Layout`.

#### P2-4. Benchmark подтверждает формы путей, но не закрепляет performance contract

`benches/size_classes_bench.rs` хорошо разделяет fast hit, jump, multi-jump, exhausting `None`, boundaries и `is_huge`; test oracles проверяют, что fixtures реально активируют нужные ветви. Однако:

- jump-vs-linear сравнение есть только для одного `JUMP_A` input;
- benchmark нигде не запускается в CI — `.github/workflows/ci.yml:2093` только type-checks его через `--no-run`;
- нет committed result/baseline с допустимым regression policy;
- `src/lib.rs:827-831` утверждает, что bitmask вариант «measured faster», но package-local воспроизводимое доказательство результата отсутствует; `bench-iters.txt` хранит iteration counts, а не итоговые performance measurements.

Это не correctness blocker, но до публикации следует либо ослабить performance wording до структурного «avoids division/fewer iterations», либо сохранить репрезентативные результаты и расширить comparator на разные density/depth/`None` cases. Wall-clock не следует делать hard CI gate; для gate лучше deterministic instruction count.

### P3 — улучшения и residual risks

#### P3-1. Нет registry/tarball consumer, который исполняет публичный API

Интеграция лучше, чем у изолированного leaf crate: root manifest использует `size-classes = { path = ..., version = "0.1" }`, а `src/alloc_core/size_classes.rs` реально строит схему и allocator вызывает classifier. Windows/macOS production CI косвенно компилирует и исполняет этот consumer. `cargo publish --dry-run` также проверяет packaged build.

Но dry-run не запускает tests из собранного `.crate`, и нет отдельного consumer fixture, зависящего от упакованного/registry-shaped artefact и вызывающего README API. Остаточный риск — package normalization/content отличается от workspace path build. Полезный post-package smoke: создать временный внешний crate, подключить именно полученный archive/path package без workspace inheritance, собрать и исполнить README scenario. По условиям этого раунда это не запускалось.

#### P3-2. MSRV проверяется в workspace, но не на нормализованном package artefact

CI имеет хорошие отдельные Rust 1.88 rows (`ci.yml:2073-2093`) для library, tests и bench, а stable package dry-run — отдельно (`ci.yml:711-734`). Их пересечение отсутствует: archive/normalized manifest не проверяется именно toolchain 1.88. Это малый риск, но для publish gate сильнее выполнить package verification и внешний smoke также на MSRV.

#### P3-3. Test oracle силён для classifier, но Params-space остаётся hand-picked

Proptest случайно варьирует `(size, align)`, но сами схемы A/B/C фиксированы compile-time constants. Reference builder близко повторяет production control flow; golden/extreme tests хорошо уменьшают circular-oracle риск, но не генерируются сочетания `growth`, `geo_count`, interleaving extras и malformed Params. Для следующего усиления полезны:

- property generator маленьких валидных схем с independent big-integer/reference arithmetic;
- negative generated Params cases;
- compile-fail/UI fixtures для обещания одинаково громких const-context diagnostics (сейчас главным образом проверяются runtime `#[should_panic]`).

#### P3-4. Consumer хранит потенциально вторую копию LUT

Сам крейт хранит одну таблицу и один LUT. Однако root shim одновременно содержит LUT внутри `SC` и отдельный `static SIZE2CLASS = *SC.size2class()` (`src/alloc_core/size_classes.rs:197-235`). Комментарий честно признаёт, что это отдельное storage и linker dedup не гарантирован. Это около 16 KiB в default scheme, около 64 KiB при 1 MiB medium ceiling и ещё больше в wide configuration. Поскольку реальные call sites используют `SegmentLayout::SIZE2CLASS` как slice, стоит измерить и попробовать экспортировать ссылку на LUT внутри `SC`, а не копию массива. Это consumer integration optimization, не дефект standalone crate.

#### P3-5. Public docs перегружены историей внутренних ревью

Production-файл на 902 строки содержит сравнительно небольшой алгоритм, но rustdoc и комментарии повторяют расследования, task IDs и прежние формы дефектов. Test/bench source ещё сильнее привязан к audit archaeology. Инварианты и counterexamples полезны, но публичной документации перед crates.io лучше оставить contract, panic domain, complexity и короткие rationale; хронологию переместить в CHANGELOG/ADR. Это уменьшит риск будущего doc drift — уже проявившийся в P2-2/P2-3.

#### P3-6. Не все CI actions SHA-pinned

Именно release workflow разумно SHA-pins `actions/checkout`, но обычный package gate и большинство CI rows используют mutable `actions/checkout@v5`, `dtolnay/rust-toolchain@stable`, а cargo-deny install action — tag. Release затем доверяет зелёному CI этого SHA. Это cross-workspace supply-chain hardening item, а не специфический дефект алгоритма; как минимум checkout/install actions в security/package gates стоит SHA-pin, toolchain version — записывать в job output.

## API/contract и semver

- `Params` корректно `#[non_exhaustive]` и имеет const constructor; это оставляет пространство для полей.
- `SizeClasses` имеет private fields, не `Copy`, но `Clone`; решение разумно для объекта порядка десятков KiB.
- `InvalidAlign(pub usize)` намеренно исчерпывающий tuple struct. Для единственной причины ошибки это допустимо, но после публикации расширение checked classifier до новых error kinds потребует нового типа/варианта или breaking API; решение следует считать сознательно замороженным.
- Raw `size2class()` accessor честно документирует underflow/OOB/false-sentinel domain. Это low-level API с высоким misuse potential, но документация ясная, а `class_for` предпочтителен.
- Runtime builders панически валидируют configuration. Для const-first API это приемлемо, но runtime consumer не получает typed configuration errors; менять после 0.1 будет сложнее.

## CI, metadata и публикация

Положительно:

- manifest полон: version, edition, MSRV 1.88, dual license, README, repository/homepage/docs, 5 keywords, валидные categories;
- локальные MIT/Apache тексты присутствуют и tracked;
- runtime dependencies отсутствуют; `#![no_std]` и `#![forbid(unsafe_code)]` соответствуют коду;
- CI статически содержит debug/release tests, real 32-bit tests, bare-metal no_std build, clippy all-targets, rustdoc `-D warnings`, MSRV library/test/bench compile и publish dry-run;
- release workflow проверяет дату CHANGELOG, CI status, tests и package verification.

Ограничение этого отчёта: ни один из этих gate фактически не запускался в round-3, поэтому их наличие не является доказательством текущего зелёного результата.

## Итоговый gate

**NO-GO для публикации текущего HEAD.** Минимальный обязательный блокер — P1-1: release workflow отвергнет `0.1.0 - Unreleased`. До снятия NO-GO также рекомендую исправить обе фактические ошибки документации (P2-2/P2-3) и принять явное maintainer-решение по форме `class_for`/`try_class_for` до первой semver-фиксации API. После изменений нужен полный предусмотренный CI/package gate, которого этот read-only/no-Cargo раунд намеренно не выполнял.

Условная оценка самого алгоритмического ядра: **GO after fixes/gates** — статически не найдено production correctness или memory-safety дефекта на валидном documented domain.
