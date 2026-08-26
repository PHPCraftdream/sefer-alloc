# `size-classes`: предрелизный аудит, прогон 6

**Автор:** Сол-кодекс

**Время:** 2026-08-26 21:23:18 Europe/Berlin

**Проверенный HEAD:** `1a908c023fd4618bef187fb23961d3774cde641e`

**База сравнения:** `8930d7a` (отчёт прогона 5)

**Режим:** новое статическое исследование в режиме только чтения; без под-агентов. Тесты, сборка, `cargo check`, Clippy, rustdoc, Miri, benchmarks, packaging и publish-команды не запускались. Единственные создаваемые артефакты — этот отчёт и его отдельный commit.

## Вердикт

**NO-GO для публикации в текущем виде из-за одной regression-in-fix в публичном raw-LUT контракте. Production-алгоритмы по-прежнему выглядят готовыми.**

Два последних коммита правильно закрывают обычный пример из прогона 5: документация теперь различает valid domain, false-sentinel window и настоящий out-of-bounds, а новые тесты закрепляют схему `min_block=16`, `small_max=64`, `L=5`. Код `class_for`, builders и LUT не менялся и новых runtime-регрессий в нём не найдено.

Но граница sentinel window теперь выражена как `L * min_block()`. Для валидной `SizeClasses`, уже существующей в собственных extreme-тестах крейта, это произведение переполняет `usize`. Если raw caller буквально реализует опубликованный guard, debug-сборка паникует, а release-сборка получает wrapped bound и другое решение. Это тот же класс profile-dependent числовой ошибки, от которого production builder уже защищён `checked_mul`.

До публикации следует переписать low-level инструкцию через сам overflow-free индекс (`idx < L` / `idx == L - 1`) либо checked arithmetic, не предлагая вычислять непредставимую байтовую границу.

## Охват и ограничения

Заново просмотрены:

- весь `crates/size-classes/src/lib.rs`, включая builders, LUT и оба classifier path;
- весь diff после прогона 5: `657deab`, `1a908c0`;
- `Cargo.toml`, `README.md`, `CHANGELOG.md`, package contents и лицензии;
- `tests/builder.rs` и `tests/proptest_builder.rs` как статические оракулы, включая extreme-width cases;
- `benches/size_classes_bench.rs` и activation oracle для jump path;
- package, debug/release, `no_std`, Clippy, rustdoc и MSRV строки CI;
- реальный потребитель `src/alloc_core/size_classes.rs` и все прямые использования `build_size2class`/raw LUT вне standalone crate.

По `rust-intel` применены релевантные модули numerics/data, public API, tests/oracles, dependencies/features и semantic conformance. Требование пользователя «без под-агентов» имеет приоритет над skill fan-out, поэтому это bounded single-context review. Async, unsafe/FFI, concurrency, security и Drop/RAII не проходились как самостоятельные тематические аудиты: структурный просмотр подтверждает, что production-код — `#![forbid(unsafe_code)]`, pure const arithmetic без I/O, ресурсов, потоков, atomics, locks, crypto, async и production-зависимостей.

## Обзор последних исправлений

| Коммит | Что сделано | Оценка |
|---|---|---|
| `657deab` | raw accessor получил нижнюю границу, upper guard и разделение sentinel/OOB | **Основная семантика исправлена, но закрытие неполное.** Обычный пример точен; выражение `L * min_block()` переполняется на валидной extreme-схеме — P2-1. Фраза о том, что `class_for` «applies both guards», также шире реализации — P3-1. |
| `1a908c0` | добавлены два raw-domain contract tests для `[16, 32, 64]` | **Корректные и невакуозные тесты для выбранной схемы.** Valid domain сравнивается с `class_for`, false sentinel наблюдается напрямую, panic привязан к точному OOB site. Но схема не активирует overflow нового документального bound — P4-1. |

## Находки

### P2-1 — BLOCKER: опубликованная граница `L * min_block()` переполняется для валидной `SizeClasses`

**Где:** `crates/size-classes/src/lib.rs:559-564`, прежде всего `:560`.

Новая документация описывает in-bounds false-sentinel window так:

> `small_max() < size <= L * min_block()`

Как математический интервал это понятная запись, но здесь она находится в low-level инструкции рядом с копируемой Rust-формулой индексирования. В `usize` произведение не обязано быть представимо даже для полностью валидной схемы.

Контрпример уже есть в `tests/builder.rs:517-612`:

- 64-bit target;
- `min_block = 1 << 62`;
- `table = [1 << 62, 2 << 62, 3 << 62]`;
- `small_max = 3 << 62`;
- `L = 4`;
- следовательно, `L * min_block = 4 * 2^62 = 2^64`, что не помещается в `usize`.

Собственный production builder обрабатывает именно этот случай через `(k + 1).checked_mul(min_block)` и clamp; комментарий существующего regression test прямо фиксирует overflow top bucket. Raw caller, скопировавший новый bound как `size <= sc.size2class().len() * sc.min_block()`, получает:

- panic при включённых overflow checks;
- wrap до `0` при release-default semantics;
- неверный результат guard-а вместо документированного определения sentinel zone.

Для этой extreme-схемы все представимые `size > small_max` до `usize::MAX` всё ещё дают `idx == L - 1`; представимого размера с `idx >= L` вообще нет. Реальная array-index семантика остаётся корректной — неверно только предлагать вычислять overflow-prone byte bound для её описания.

**Исправление:** описать и рекомендовать overflow-free последовательность:

1. `zero_based = size.checked_sub(1)`; `None` означает invalid zero-size raw input;
2. `idx = zero_based >> min_block_shift()`;
3. `size <= small_max()` — обязательный classifier-domain guard;
4. если guard намеренно пропущен, `idx < L` означает in-bounds raw access, `idx == L - 1` — top bucket, `idx >= L` — OOB;
5. не вычислять `L * min_block` в `usize`; если байтовая верхняя граница всё же нужна, использовать `checked_mul` и трактовать `None` как математическую границу выше `usize::MAX`.

После этого контракт будет одинаково точен для обычной и extreme-схем.

### P3-1 — `class_for` не «применяет оба raw guards» буквально

**Где:** `src/lib.rs:566-569` в сравнении с `:702-711`.

Документация утверждает, что `class_for` «applies both guards». Верхнюю границу он действительно проверяет через `need > small_max`. Но `size >= 1` он не валидирует: при `size == 0` и валидном `align >= 1` функция вычисляет `need = max(size, align) >= 1` и безопасно классифицирует запрос.

Это не runtime-баг: `Layout`-совместимый `align` делает индексируемый `need` ненулевым, поэтому underflow невозможен. Неточна причинная формулировка. Лучше написать, что `class_for` индексирует по ненулевому `need = max(size, align)` и отдельно rejects `need > small_max`; raw `size >= 1` precondition к его входу буквально не применяется.

Можно закрыть вместе с P2-1 одной правкой rustdoc.

### P4-1 — новые domain tests не используют уже имеющийся overflow-activating fixture

Новые тесты качественно закрепляют normal-width example, но ни один не связывает исправленный accessor contract с `extreme64_overflow`, где `L * min_block` реально переполняется. Именно поэтому новый документальный bound прошёл мимо тестовой правки, хотя нужный fixture уже находится в том же файле.

После исправления документации полезно расширить `build_size2class_bucket_need_overflow_clamps_to_last_class`: для `size = small_max + 1` и `usize::MAX` вычислить raw `idx` через shift, доказать `idx == L - 1`, получить sentinel без вычисления `L * min_block`. Это закрепит overflow-free формулировку поведением существующей схемы. Пункт не является самостоятельным блокером.

### P4-2 — CI-комментарий снова содержит протухший hardcoded test count

**Где:** `.github/workflows/ci.yml:1838`.

Комментарий всё ещё говорит «All 14 of the crate's tests», хотя набор давно вырос и последние два теста увеличили его ещё раз. Команды CI корректны и запускают suite без фильтра; проблема только в обслуживаемости комментария. Удалить число и оставить «all crate tests», следуя уже принятому в соседних CI-комментариях правилу не фиксировать счётчик вручную.

## Общий обзор production-кода

### Builder и арифметика

- `size2class_len` валидирует power-of-two `min_block` и checked `+1`.
- `build_table` проверяет `geo_count`, denominator, `N`, форму extras и strict monotonicity merged table.
- Geometric advance выполняет widened checked multiply/add, проверяет downcast в `usize`; min-step fallback также checked.
- Sorted merge корректно принимает extras между geometric entries и громко отклоняет duplicate collision.
- `growth.0 == 0` имеет осмысленную linear-step семантику и не создаёт скрытый zero class.
- На текущих `usize <= 64` targets representable quotient не отклоняется из-за промежуточного `usize` overflow; ограничение будущей 128-bit цели честно документировано.

### LUT и classifier

- LUT строится monotone-pointer алгоритмом `O(buckets + classes)`; class-index pin точно допускает 256 классов и отклоняет 257.
- Hand-built tables имеют отдельную monotonicity defense; их отличия от Params-built схем документированы.
- `class_for` делает range rejection до lookup; in-contract path не наблюдает top sentinel как answer.
- Fast path остаётся одной LUT access после boundary branch.
- Slow path через bitmask/next-multiple/re-seed эквивалентен scan predicate и защищён от wrap через `checked_add`.
- Alignment contract теперь последовательно разделяет stride divisibility и caller-owned base address alignment.
- `SizeClasses` immutable, private-field, не `Copy`; большая готовая схема правильно рекомендуется как `static`.

Новых алгоритмических ошибок, unsafe/UB, silently wrapped production arithmetic, in-contract panic paths или semantic drift fit predicate не найдено.

## Производительность

Требующих вмешательства возможностей ускорения не обнаружено:

- per-request fast path уже минимален;
- bitmask дешевле division для контрактного power-of-two alignment;
- jump path перескакивает runs неподходящих классов;
- checked `u128` arithmetic находится только в builder path;
- benchmark отдельно измеряет fast hit, два реально активированных jump case, `small_max` boundaries и `huge_threshold` policy;
- activation oracle не позволяет jump benchmarks незаметно деградировать в single-check rows.

Без новых измерений усложнять representation или classifier не следует. Отдельная копия `SIZE2CLASS` в in-tree compatibility shim рядом со встроенной копией в `SC` явно документирована как consumer trade-off и не является дефектом публикуемого крейта.

## Тестовые оракулы и CI — статическая оценка

Сильные стороны корпуса сохранены:

- hand-derived geometric golden values дополняют runtime reference builder;
- полный SEFER size×alignment sweep сравнивается с независимым scan predicate;
- три property schemes сравнивают jump, walk и scan;
- отдельно покрыты arithmetic overflow, top-bucket clamp, next-multiple wrap, 256/257 boundary, extras interleaving и debug-only precondition;
- новые raw-domain tests невакуозны и используют точный panic substring;
- README example зеркалирован integration test;
- CI содержит package dry-run, debug/release tests, bare-metal `no_std`, all-targets Clippy `-D warnings`, rustdoc `-D warnings` и MSRV compile тестовых targets/dev-dependencies.

Safe-only deterministic arithmetic crate не нуждается в Miri/Loom/sanitizer gate. Фактическую зелёность существующих gates этот аудит не подтверждает: по требованию пользователя ничего не запускалось.

## Что исправить перед публикацией

1. **Обязательно:** заменить overflow-prone `L * min_block()` в raw-LUT contract на index/checked-arithmetic формулировку.
2. Вместе с этим точно описать, почему `class_for` не имеет zero-underflow, не заявляя буквальную проверку `size >= 1`.
3. Желательно закрепить extreme raw-index case существующим 64-bit fixture.
4. Опционально убрать hardcoded test count из CI-комментария.
5. После правок выполнить обычные project gates; этот аудит намеренно их не запускал.

## Итог

Реализация `size-classes` остаётся зрелой и быстрой: safe-only, `no_std`, zero production dependencies, checked numeric construction, компактный query path и сильные оракулы. Последние изменения не затронули runtime-код и правильно исправили normal-width случай из прогона 5.

Текущий вердикт — **NO-GO только из-за P2-1**: low-level документация, призванная защитить raw caller от overflow/OOB, сама предлагает overflow-prone boundary expression на валидной схеме, которую крейт уже тестирует. После перехода к overflow-free index contract ожидаемый вердикт — **GO**; иных release blockers и performance problems в просмотренном состоянии не найдено.
