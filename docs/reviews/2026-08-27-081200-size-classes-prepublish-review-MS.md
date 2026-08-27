# size-classes: независимый статический аудит перед первой публикацией

Дата: 2026-08-27T08:12:00+02:00 (Europe/Berlin)  
Проверенный HEAD: `9c8e876c037df304a9fd88bbeefe0b1d291d3387`  
Итог: **NO-GO**

## Режим и ограничения

- Только чтение исходного дерева; единственная запись — этот отчёт.
- Без под-агентов, по прямому требованию заказчика.
- Не запускались тесты, сборка, `cargo check`, clippy, rustdoc, benchmarks, package/publish и вообще никакие Cargo-команды.
- Выводы о компиляции, тестах, package и CI сделаны только статически по текущим исходникам и конфигурации. Зелёный статус текущего HEAD не проверялся.
- История Git не использовалась. Содержимое `docs/reviews` и checkpoints не читалось; ссылки на старые ревью, встречающиеся в комментариях текущего кода, не использовались как доказательства.
- Рабочее дерево до аудита уже содержало посторонние untracked-файлы; они не читались как источники, не изменялись и не добавлялись в staging.

## Охват

Полностью прочитаны текущие файлы крейта:

- `crates/size-classes/src/lib.rs`;
- `Cargo.toml`, `README.md`, `CHANGELOG.md`, обе лицензии;
- `tests/builder.rs`, `tests/proptest_builder.rs`, `tests/common/mod.rs`;
- `benches/size_classes_bench.rs`.

Дополнительно статически проверены:

- workspace manifest/lockfile и версия реальных dev-зависимостей;
- релевантные участки `.github/workflows/ci.yml` и полный `.github/workflows/release.yml`;
- реальный consumer: root dependency/feature wiring, `src/alloc_core/size_classes.rs`, публичный forwarder `SegmentLayout`, путь carve/alignment и интеграционные size-class tests;
- публичная доступность имени: запрос crates.io API для `size-classes` вернул HTTP 404 на момент аудита;
- официальная Rust Reference для спорного утверждения о const-eval overflow.

Неприменимые классы риска: в production-коде крейта нет `unsafe`, FFI, потоков/атомиков, async, I/O, криптографии, аллокаций или runtime-зависимостей. `#![forbid(unsafe_code)]`, `#![no_std]` и пустой `[dependencies]` соответствуют коду.

## Findings

### P0 — блокирует сам release workflow

#### P0-1. CHANGELOG всё ещё помечает 0.1.0 как Unreleased

`crates/size-classes/CHANGELOG.md:7` содержит `## 0.1.0 - Unreleased`. Реальный release workflow требует ровно одну секцию версии и отклоняет её, если заголовок содержит `unreleased` (`.github/workflows/release.yml:215-309`). Следовательно, текущий HEAD нельзя опубликовать штатным non-dry-run workflow независимо от качества кода.

Исправление перед публикацией: заменить `Unreleased` фактической датой релиза и только после этого создавать `size-classes-v0.1.0` на том же проверенном commit.

### P1 — исправить до фиксации публичного контракта 0.1.0

#### P1-1. Safe public `class_for` тихо нарушает собственный fit-предикат в release для неверного `align`

`SizeClasses::class_for(size, align)` принимает два `usize`, но проверяет ключевой контракт `align.is_power_of_two()` только через `debug_assert!` (`crates/size-classes/src/lib.rs:705-726,751-756`). В release эта проверка исчезает. Сам rustdoc честно перечисляет последствия: fast path может вернуть неделимый block, slow path может пропустить подходящий класс, вернуть ложный `None` или принять неподходящий класс.

Это не UB внутри данного чисто арифметического крейта, но заявленное назначение — основа allocator classifier. Ошибочный `Some` способен привести consumer allocator к возврату адреса, не удовлетворяющего `Layout`; downstream-последствие safety-critical. Root consumer безопасен, потому что передаёт alignment из `Layout`, но опубликованный общий API этого типом не выражает.

До первой публикации следует выбрать и зафиксировать один контракт:

- предпочтительно сделать проверку release-active (`assert!`) и добавить явный `# Panics`;
- либо добавить проверяемый `try_class_for`/тип alignment и оставить unchecked-вариант явно low-level;
- либо возвращать `None`/ошибку для `align == 0 || !align.is_power_of_two()`.

Оставлять заведомо неверный release-ответ как поведение safe public API перед 0.1.0 не рекомендуется.

#### P1-2. Публичная численная документация неверно описывает overflow в const context

`crates/size-classes/src/lib.rs:151-154` утверждает, что unchecked `+ 1` мог бы wrap-нуться в release-profile const evaluation, потому что const-eval якобы следует profile `overflow-checks`. То же утверждение повторяется в production-комментарии на `lib.rs:303-306` и в тестовых комментариях `tests/builder.rs:620-630,728-739`.

Это расходится с Rust Reference: выражения в const context всегда вычисляются при компиляции, а overflow при обязательном compile-time evaluation является compiler error. Runtime-вызов `const fn` действительно следует обычной runtime overflow policy; вызов той же функции из `const`/`static` — нет.

Сами checked-операции корректны и полезны: они нужны для стабильного именованного runtime-диагноза и для независимости runtime correctness от profile. Ошибка находится в объяснении и тестовом оракуле, который приписывает фиксу несуществующий release-const эффект. Перед публикацией надо разделить два контекста во всём тексте: const context всегда rejects overflow; runtime release без checked arithmetic мог бы wrap.

Reference: <https://doc.rust-lang.org/reference/const_eval.html#constant-expressions>.

### P2 — контракт/API и тестовые оракулы

#### P2-1. `build_size2class` допускает hand-built таблицы с заведомо недостижимыми классами

`build_size2class` проверяет непустоту, power-of-two `min_block`, `N <= 256`, строгую монотонность и точный `L`, но не требует, чтобы элементы `table` были кратны `min_block` (`crates/size-classes/src/lib.rs:397-419,421-467`). Rustdoc даже приводит `[16, 24, 32]` и признаёт, что класс 24 навсегда недостижим через документированный bucket lookup.

Это не скрытый implementation bug — ограничение раскрыто, а `SizeClasses::build` получает только выровненную таблицу. Но для самостоятельного публичного builder это пахнущий контракт: функция успешно строит «size→class lookup», который не может вернуть часть переданной class table. До 0.1.0 разумнее либо reject-ить `table[i] % min_block != 0`, либо сузить название/документацию до low-level bucket map и явно не обещать достижимость каждого class index. Первый вариант согласован с основным `build_table` и упрощает mental model.

#### P2-2. Главный reference builder не полностью независим и не вполне faithful

`tests/builder.rs:19-60` повторяет ту же формулу rounding/merge, что production, с unchecked `usize` arithmetic. Кроме кругового оракула, он вычисляет следующий geometric value и после последнего добавленного класса, тогда как production делает advance только если впереди есть следующий geometric class (`lib.rs:291-339`). На обычной SEFER fixture расхождения нет, но как общий «faithful» oracle helper он может panic-нуть там, где последний production class ещё валиден, а уже ненужный следующий — нет.

Golden test на восемь вручную вычисленных значений частично закрывает круговость, а extreme tests используют `u128`; поэтому это не release blocker. Улучшение: единый независимый mathematical oracle на widened integers, который не повторяет control flow production и проверяет только реально требуемые `geo_count` значений.

#### P2-3. Property tests варьируют запросы, но не пространство схем

`tests/proptest_builder.rs` генерирует только `(size, align)` для трёх фиксированных const-схем. `min_block`, `(num, den)`, `geo_count` и `extras` не property-generated. Комментарии это честно признают, а hand-written boundary tests сильные, но остаются плохо покрыты комбинации: ratio 0/<=1, extras у границ geometric values, различные допустимые `N/L` и near-overflow схемы в одном независимом oracle.

Улучшение не требует менять production API: можно property-test-ить `build_table::<N>` набором нескольких N через macro-generated cases и runtime `Params`, либо вынести математический dynamic reference и сравнивать семейство заранее инстанцированных N.

### P3 — hardening, производительность и поддерживаемость

#### P3-1. Реальный consumer хранит лишнюю полную копию LUT

Root integration одновременно создаёт `SIZE2CLASS` и `SC`, а `SC` содержит собственный идентичный LUT (`src/alloc_core/size_classes.rs:197-222`). Комментарий признаёт около 16 KiB лишнего `.rodata` в default и около 64 KiB с medium classes. Это не ошибка данного крейта, но реальная стоимость текущей consumer-интеграции.

Можно убрать отдельный build и экспортировать slice из `SC.size2class()`, если const/static lifetime форма допускается на MSRV, либо добавить в крейт constructor/view API, позволяющий consumer хранить table/LUT ровно один раз. Измерить итоговый binary section size до/после.

#### P3-2. Benchmarks не покрывают верхнюю стоимость slow path и build-time cost

Bench корректно разделяет fast path, два one-jump slow-path случая, границу `small_max` и `is_huge`. Path-activation tests не дают строкам превратиться в no-op. Однако нет:

- slow path с несколькими jumps;
- slow-path `None` после поиска, а не раннего `need > small_max`;
- разных table densities/N;
- compile-time/build-table/LUT cost или binary-size counter;
- детерминированного perf regression gate.

Это не мешает correctness release, но заявленные 24–45% в комментарии `lib.rs:775-778` не подкреплены локальной воспроизводимой baseline/результатами в самом крейте.

#### P3-3. CI сильный, но первая публикация остаётся точкой смены режима

Статически присутствуют: debug/release tests, реальный i686 test run, thumbv7em no_std build, clippy all-targets, rustdoc `-D warnings`, MSRV 1.88 library/tests/bench compile, publish dry-run и release-time CI guard. Это хороший охват.

Ожидаемая граница: semver-check отсутствует до первой публикации (`ci.yml:721-724`). После успешной 0.1.0 его нужно добавить немедленно. Package gate использует moving stable, а MSRV gate отдельно компилирует workspace source; после первого publish полезен также минимальный внешний consumer crate против registry artifact, проверяющий README-shaped const construction и no_std import без ancestor workspace.

## Production code и численная корректность

Положительные результаты статической проверки:

- Production-файл один, safe-only, без аллокаций и зависимостей; no_std claim реален.
- `build_table` проверяет power-of-two/nonzero `min_block`, `geo_count`, denominator, точное `N`, shape extras, merged strict monotonicity и overflow обоих advance paths.
- Widening в `u128` корректно сохраняет случаи, где промежуточный `usize` product не помещается, но итоговый quotient/class помещается, для реально поддерживаемых 16/32/64-bit `usize` targets.
- `size2class_len` и per-bucket multiply checked; `N == 256` корректно допускает индексы `0..=255`, `N == 257` отклоняется.
- LUT monotone-pointer algorithm имеет `O(L + N)` build cost; fast classification — один lookup. Slow jump строго увеличивает index при выполненных preconditions и завершится не более чем за N probes.
- Off-by-one на `(size - 1) >> shift`, top sentinel и overflow следующего multiple обработаны последовательно.
- `class_for` для валидного power-of-two alignment возвращает минимальный class, удовлетворяющий size и stride divisibility; root consumer дополнительно выравнивает carve offset по `block_size`, а segment base имеет достаточное выравнивание.

Замечание о сложности: весь `class_for` не является O(1); O(1) обещан только fast path. Текущий публичный текст в основном формулирует это правильно.

## Публичный API и SemVer

- API мал: `Params`, три builder/query функции и `SizeClasses` с read-only accessors.
- `Params` предусмотрительно `#[non_exhaustive]` и имеет `const fn new`; до первой публикации это правильный момент закрепить форму.
- `SizeClasses` не `Copy`, что предотвращает неявное копирование больших массивов; ручной `Debug` не печатает LUT.
- Private fields не позволяют downstream собирать несогласованный `SizeClasses`; `build` является chokepoint.
- Возврат raw LUT через `size2class()` — сознательно low-level и подробно документирован, но увеличивает поверхность misuse. Предпочтительный обычный API — `class_for`.
- Главный незакрытый контракт — P1-1: runtime validation alignment надо решить до semver freeze.

## Документация и metadata

- Обязательные publish metadata присутствуют: version, edition/MSRV, license, description, README, repository/homepage/documentation, keywords/categories.
- Обе заявленные лицензии находятся в crate directory.
- Описание «zero deps», `no_std`, `forbid(unsafe_code)` подтверждается production manifest/code.
- Отдельная docs.rs feature metadata не нужна: у крейта нет features.
- README example зеркалируется executable test, хотя в этом аудите он не запускался.
- Crate name на момент read-only запроса не зарегистрирован (HTTP 404), но фактическая возможность first publish всё равно зависит от credentials/verified owner state и может измениться до upload.
- Публикацию блокируют P0-1 и документная корректность P1-2.

## CI, package и release

Статически workflow охватывает нужные конфигурации, но этот аудит не подтверждает, что jobs запускались и были зелёными именно на проверенном HEAD. Release workflow дополнительно проверяет tag/version, crate-local CHANGELOG, успешный CI run на том же SHA, default tests и packaged standalone verification. Non-dry-run сейчас гарантированно остановится на `Unreleased`.

Security hardening release workflow разумный: read-only GitHub permissions, SHA-pinned checkout в token-bearing workflow, crates.io token отдельно. Остаточный supply-chain риск moving `dtolnay/rust-toolchain@stable` принят явно; runtime artifact крейта зависимостей не имеет.

## Итоговый gate

**NO-GO для публикации текущего HEAD.**

Минимум для перехода к GO:

1. Принять и реализовать release-поведение `class_for` для invalid alignment (P1-1), затем обновить contracts/tests.
2. Исправить ложные утверждения о release const-eval overflow (P1-2).
3. Датировать `0.1.0` в crate CHANGELOG (P0-1).
4. После изменений выполнить фактические обязательные gates вне этого статического режима: fmt, MSRV/stable builds, debug/release/i686 tests, no_std target, clippy, rustdoc, package contents + publish dry-run и внешний consumer smoke.

P2/P3 можно формально отложить, но P2-1 особенно желательно решить до первой публикации: ужесточить контракт hand-built table после 0.1.0 может оказаться breaking change.
