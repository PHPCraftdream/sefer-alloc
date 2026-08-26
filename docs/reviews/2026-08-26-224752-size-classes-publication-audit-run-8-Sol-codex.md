# `size-classes`: предрелизный аудит, прогон 8

**Автор:** Сол-кодекс

**Время:** 2026-08-26 22:47:52 Europe/Berlin

**Проверенный HEAD:** `313504c4cf82f1af22883b2003fe9c35c14ced5d`

**База сравнения:** `e69d977` (commit отчёта прогона 7)

**Режим:** новое статическое исследование в режиме только чтения, без под-агентов. Тесты, сборка, `cargo check`, Clippy, rustdoc, Miri, benchmarks, packaging и publish-команды не запускались. Единственные создаваемые артефакты — этот отчёт и его отдельный commit.

## Вердикт

**NO-GO для публикации в текущем виде из-за одной новой фактической ошибки в публичном rustdoc. Production-код и hot path готовы.**

Blocker прогона 7 закрыт правильно: ошибочный специальный параграф о `class_for(0, 0)` удалён целиком. Опциональная рекомендация о `pub(crate)` для общей test/bench fixture также выполнена корректно. Последующие изменения удачно консолидировали alignment contract, уточнили происхождение крейта и derivation `L`, а ручной `Debug` разумно перестал печатать содержимое больших массивов.

Однако новый rustdoc `SizeClasses` утверждает, что derived `Debug` печатал две таблицы размером `~16 KiB + ~16 KiB`; test-comment усиливает это до `~16 KiB each`. Для названной в тексте realistic production-like схемы это неверно: `size2class: [u8; L]` занимает около 16 KiB, но `table: [usize; 49]` — только 392 байта на 64-bit (196 байт на 32-bit). Весь `SizeClasses` уже правильно описан соседним абзацем как объект примерно на 16 KiB, а consumer-comment называет дополнительную LUT-копию table-sized `~16 KiB`. Новый текст одновременно противоречит обоим этим корректным описаниям и завышает объём примерно вдвое.

Ошибка не влияет на runtime semantics и не отменяет полезность ручного `Debug`, но это проверяемое числовое утверждение в публичной API-документации, появившееся прямо перед публикацией. Исправление локальное: заменить `~16 KiB + ~16 KiB` / `~16 KiB each` на «около 16 KiB суммарно, почти полностью LUT» либо вообще убрать числа. После этого ожидаемый вердикт — **GO**.

## Охват и ограничения

Просмотрены заново:

- весь production-код `crates/size-classes/src/lib.rs`;
- все изменения после прогона 7: `aafa09a`, `d22679e`, `565da79`, `65e9d3a`, `727fd39`, `4b5bf37`, `8fe10dc`, `c6fa927`, `ae7c66b`, `313504c`;
- `Cargo.toml`, `README.md`, `CHANGELOG.md`, обе лицензии и tracked package file set;
- `tests/builder.rs`, `tests/proptest_builder.rs`, `tests/common/mod.rs` и benchmark;
- package/debug/release/no_std/Clippy/rustdoc/MSRV CI rows;
- реальные consumer shims `src/alloc_core/size_classes.rs` и `src/alloc_core/segment_layout.rs`, а также список остальных вызовов classifier/accessors.

По `rust-intel` применены релевантные проверки numerics/data, public API, semantic conformance, tests/oracles, dependencies/features и performance-at-scale. Async, unsafe/FFI, concurrency, Drop/RAII и resource lifecycle здесь неприменимы: production-код — `#![forbid(unsafe_code)]`, pure const arithmetic без I/O, ресурсов, потоков, atomics, locks, crypto, async и production dependencies.

Существовавшие untracked logs, checkpoints и чужие review-файлы не изменялись и в commit не включаются.

## Обзор последних правок

| Коммит | Изменение | Оценка |
|---|---|---|
| `aafa09a` | удалён неверный `class_for(0,0)` rustdoc | **Полностью закрывает P2-1 прогона 7.** Публичный контракт остаётся достаточным и больше не обещает ошибочное invalid-input/profile поведение. |
| `d22679e` | fixture items сужены до `pub(crate)` | **Корректно закрывает опциональный P4-1.** Test/bench reuse сохранён, production surface не затронут. |
| `565da79` | исправлен consumer alignment proof | **Корректно.** Документированы обе необходимые части: segment-aligned base и absolute block-size alignment carve offset; отношение делимости исправлено. |
| `65e9d3a` | consumer contract больше не требует `size >= MIN_BLOCK` от самого classifier | **Корректно.** Совпадает с реальным доменом `need = max(size, align) >= 1` и существующим вызовом с `size == 1`. |
| `727fd39` | удалены случайные кириллические фрагменты в test comments | **Корректно, behavior-neutral.** |
| `4b5bf37` | мелкие README/test/bench/CI уточнения | **Корректно.** В частности, bench exclusion теперь привязан к target kind, а не ошибочно к `harness = false`. |
| `8fe10dc` | расшифровано происхождение `SEFER`, документирован `growth num <= den` | **Корректно.** Для любого отношения `<= 1` geometric result не превосходит `cur`, поэтому срабатывает minimum-step fallback. |
| `c6fa927` | ручной summary-only `Debug` | **Реализация корректна и полезна; сопровождающая оценка размеров неверна — P2-1.** Output намеренно non-exhaustive и не раскрывает raw arrays. |
| `ae7c66b` | base-address caveat собран в одном canonical `# Preconditions` | **Корректно.** Повторы сокращены без потери safety-critical caller obligation. |
| `313504c` | точное объяснение derivation `N`/`L` и rustdoc example | **Корректно.** `L` действительно требует предварительно построить table, а `SizeClasses::build` детерминированно строит её повторно из тех же params. |

## Находки

### P2-1 — BLOCKER: новый `Debug` rustdoc завышает размер raw output примерно вдвое

**Где:** `crates/size-classes/src/lib.rs:520-524`; та же ошибка в непубличном test-comment `crates/size-classes/tests/builder.rs:191-194`.

Публичный текст говорит:

```text
both raw tables (~16 KiB + ~16 KiB of numbers for a realistic scheme)
```

Test-comment говорит ещё определённее:

```text
both raw tables (~16 KiB each for a realistic scheme)
```

Но production-like fixture прямо задаёт:

- `N = 40 + 9 = 49`, поэтому `[usize; N]` занимает 392 байта на 64-bit;
- production consumer документирует `SMALL_MAX ≈ 253 KiB` и `min_block = 16`, поэтому `L = SMALL_MAX / 16 + 1 ≈ 16 Ki` элементов;
- `[u8; L]` поэтому занимает около 16 KiB;
- остальные scalar fields добавляют лишь несколько машинных слов.

Именно поэтому соседний rustdoc `SizeClasses` корректно говорит, что **весь instance** имеет размер `~16 KiB`, CHANGELOG повторяет это, а consumer отдельно называет одну дополнительную `SIZE2CLASS` copy `~16 KiB`. Таблица классов не может одновременно быть ещё одной таблицей на `~16 KiB`.

Ручной `Debug` всё равно оправдан: даже ~16 KiB чисел — неприемлемо шумный diagnostic output. Исправлять реализацию не нужно.

**Предпочтительное исправление:** написать «a derive would print the raw arrays (about 16 KiB total for a realistic scheme, almost all of it the LUT)» и синхронно исправить test-comment. Ещё устойчивее — удалить конкретную оценку и сказать «potentially thousands of LUT entries».

### P4-1 — consumer-comment всё ещё говорит, что `SizeClassesImpl` «derives Debug, Clone»

**Где:** `src/alloc_core/size_classes.rs:205-210`.

После `c6fa927` тип больше не derives `Debug`: он derives только `Clone`, а `Debug` реализует вручную. Комментарий непубличный и его основной вывод — immutable plain data безопасно хранить в `static` — остаётся верным, поэтому публикацию standalone crate это не блокирует.

**Исправление:** заменить `derives only Debug, Clone` на `implements Debug and Clone` или просто удалить нерелевантную ссылку на traits.

## Общий обзор production-кода

### Builder и числовые границы

- `size2class_len` валидирует power-of-two `min_block` и использует checked `+1`.
- `build_table` проверяет `geo_count`, denominator, exact `N`, shape `extras` и strict monotonicity итогового merge.
- Arithmetic progression шагает через widened checked `u128`, затем проверяет representability в `usize`; minimum-step fallback использует checked addition.
- Interleaving extras разрешены, geometric collisions обнаруживаются в builder chokepoint.
- `growth.0 <= growth.1`, включая zero numerator, намеренно и корректно даёт linear min-block sequence.
- Ограничение widened arithmetic для гипотетического 128-bit `usize` явно документировано; для поддерживаемых 32/64-bit targets product двух `usize` помещается в `u128`.

### LUT и classifier

- Monotone-pointer construction остаётся `O(buckets + classes)` и корректно допускает 256 классов с индексами `0..=255`.
- Hand-built tables получают независимую strict-monotonicity validation.
- Top bucket и потенциально переполняющийся `(k + 1) * min_block` корректно сведены к sentinel через `checked_mul`.
- Raw LUT accessor ясно отделяет array domain от classifier contract и не предлагает переполняющееся вычисление `L * min_block`.
- `class_for` отклоняет `need > small_max` до индексирования.
- Fast path остаётся одной LUT access после max/range/shift; release-active дополнительных checks в него не добавлено.
- Slow path использует power-of-two bitmask и jump через следующий возможный multiple вместо линейного прохода; `checked_add` закрывает wrap у `usize::MAX`.
- Stride divisibility и caller-owned base-address alignment теперь последовательно разделены во всех публичных текстах.
- `SizeClasses` immutable, private-field, deliberate non-`Copy`; ручной `Debug` не меняет classifier cost или object layout.

Алгоритмических ошибок, in-contract panic regressions, unchecked production wrap, UB или ухудшения hot path в просмотренном HEAD не найдено.

## API, metadata и публикационный контур

- Публичная поверхность компактна: `Params`, три builder/classifier helpers и immutable `SizeClasses` accessors.
- `Params` остаётся `#[non_exhaustive]` с `const fn new`, что оставляет путь для additive evolution.
- `SizeClasses` сознательно `Clone`, но не `Copy`; ручная смена `Debug` сделана до первой публикации.
- Manifest содержит license, repository/homepage/docs/readme/keywords/categories; production dependencies отсутствуют.
- Tracked crate set содержит исходник, README, CHANGELOG, manifests, tests, benchmark и обе license files.
- CI статически содержит package dry-run, bare-metal no_std build, debug/release tests, all-targets Clippy `-D warnings`, rustdoc `-D warnings`, MSRV library/test compile и MSRV benchmark compile.

Фактическую зелёность этих gates этот аудит не подтверждает: по прямому требованию пользователя команды Cargo и тесты не запускались.

## Тестовые оракулы и benchmark — статическая оценка

- Hand-derived golden table и independent reference builder/classifier уменьшают circularity.
- Full size/alignment sweep проверяет smallest-fit + divisibility.
- Три разные proptest schemes сравнивают jump, linear walk и independent scan.
- Normal и extreme raw-index domains, 256/257 class boundary, arithmetic overflows, duplicate/interleaving extras и next-multiple wrap закреплены отдельными tests.
- Документированные panic conditions имеют targeted negative tests.
- Shared fixture механически связывает production-like test и benchmark inputs.
- Benchmark различает fast path, genuine jump activation, small-max rejection и `is_huge` policy boundary.
- Новый Debug test проверяет именно отсутствие raw field dumps; числовая ошибка находится только в комментарии, не в oracle.

Safe-only deterministic arithmetic crate не требует Loom/Miri/sanitizer gate как обязательного publication criterion.

## Производительность и возможные улучшения

Обязательных ускорений не найдено:

- classifier fast path уже минимален и не содержит division;
- slow path перескакивает невозможные классы и использует bitmask для power-of-two divisibility;
- checked/widened тяжёлая арифметика выполняется только при construction/const-eval;
- runtime state contiguous и immutable;
- ручной `Debug` улучшает диагностическое поведение, не затрагивая layout/hot path.

Возможная будущая архитектурная оптимизация — не хранить отдельную публично доступную LUT рядом с classifier либо позволить consumer передать уже построенные arrays, чтобы не строить table дважды в const expressions и не дублировать LUT у конкретного root-consumer. Но это существенно усложнит API/const generics и не ускорит классификацию; текущая цена явно документирована и для standalone crate выглядит разумной. Перед первым релизом такой redesign не требуется.

## Что исправить перед публикацией

1. **Обязательно:** исправить оценку размера в `SizeClasses` rustdoc и синхронном test-comment: около 16 KiB суммарно, не `16 + 16 KiB` и не `16 KiB each`.
2. Опционально обновить consumer-comment после перехода с derived на ручной `Debug`.
3. После исправления выполнить обычные project gates; этот аудит намеренно их не запускал.

## Итог

`size-classes` остаётся технически зрелым: safe-only, `no_std`, без production dependencies, с checked construction arithmetic, быстрым classifier и сильными независимыми оракулами. Все замечания прогона 7 закрыты правильно. Новая волна не внесла runtime-регрессий и в целом улучшила контракт и standalone usability.

Текущий **NO-GO** вызван только локальной фактической ошибкой в новом публичном описании `Debug`, а не дефектом алгоритма или производительности. После исправления двух строк ожидаемый вердикт — **GO**; иных release blockers в полном повторном просмотре не найдено.
