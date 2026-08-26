# `size-classes`: предрелизный аудит, прогон 7

**Автор:** Сол-кодекс

**Время:** 2026-08-26 22:01:31 Europe/Berlin

**Проверенный HEAD:** `cdebcfd5cf10d40da8cc225e278c5f9019c78af4`

**База сравнения:** `61a5b62` (отчёт прогона 6)

**Режим:** новое статическое исследование в режиме только чтения; без под-агентов. Тесты, сборка, `cargo check`, Clippy, rustdoc, Miri, benchmarks, packaging и publish-команды не запускались. Единственные создаваемые артефакты — этот отчёт и его отдельный commit.

## Вердикт

**NO-GO для публикации в текущем виде из-за одной новой фактической ошибки в публичном rustdoc. Production-код и hot path готовы.**

Blocker прогона 6 закрыт правильно: raw-LUT contract больше не предлагает вычислять переполняющееся `L * min_block`, описывает domain через `checked_sub`, `small_max` и индекс, а extreme64 test закрепляет именно overflow-free derivation. Остальные новые правки усиливают документацию, negative tests, fixture consistency и MSRV CI без изменения runtime-алгоритма.

Но самый свежий commit `cdebcfd` подробно документирует `class_for(0, 0)` и утверждает, что этот вызов «unchecked even in debug» и в debug падает на `need - 1`. Реальный порядок обратный: первая инструкция функции — `debug_assert!(align.is_power_of_two())`; для `align == 0` она срабатывает раньше вычисления `need`. Более того, текст сводит поведение к debug/release, хотя `debug_assertions` и `overflow-checks` — независимые profile knobs.

Это не алгоритмический дефект для контрактных входов: `(0,0)` нарушает preconditions. Но если публичная документация добровольно обещает точное поведение invalid-input edge, обещание должно совпадать с кодом. Самое чистое исправление — удалить избыточный параграф и оставить контракт; альтернатива — точно описать порядок guards и обе profile axes.

## Охват и ограничения

Просмотрены заново:

- весь `crates/size-classes/src/lib.rs`;
- все изменения после прогона 6: `d1eb74b`, `0cd60c3`, `d00788e`, `9800297`, `2c9625d`, `a71b12b`, `eaa3310`, `a692c47`, `bf64ce7`, `ff5a2ea`, `b85249a`, `cdebcfd`;
- `Cargo.toml`, `README.md`, `CHANGELOG.md`, licenses и package file set;
- весь shape `tests/builder.rs`, `tests/proptest_builder.rs` и новый `tests/common/mod.rs`;
- benchmark после перевода на shared fixture;
- size-classes package/debug/release/no_std/Clippy/rustdoc/MSRV CI rows;
- реальный потребитель `src/alloc_core/size_classes.rs` и прямые raw-LUT references вне standalone crate.

По `rust-intel` применены релевантные части numerics/data, public API, tests/oracles, dependencies/features и semantic conformance. Требование пользователя «без под-агентов» имеет приоритет над skill fan-out, поэтому это bounded single-context review. Async, unsafe/FFI, concurrency, security и Drop/RAII не проходились отдельными тематическими аудитами: production-код — `#![forbid(unsafe_code)]`, pure const arithmetic без I/O, ресурсов, потоков, atomics, locks, crypto, async и production dependencies.

Незакоммиченные rush-review файлы и прочие существовавшие untracked artifacts не использовались как изменения крейта и не модифицировались.

## Обзор последних правок

| Коммит | Изменение | Оценка |
|---|---|---|
| `d1eb74b` | overflow-free raw-LUT contract и точное объяснение `class_for` | **Закрывает P2/P3 прогона 6 корректно.** `size <= small_max`, `idx == L-1`, `idx >= L` разделены без overflow-prone byte bound. |
| `0cd60c3` | extreme64 raw-index regression oracle | **Корректно.** `small_max+1` и `usize::MAX` проходят через `checked_sub + shift`, не вычисляя `L * min_block`. |
| `d00788e` | удалён hardcoded test count из CI comment | **Корректно.** Команда и историческая task attribution не изменены. |
| `9800297` | stale `geo_count=177` заменён на widened boundary 183/84 | **Согласовано с текущим `u128` builder и width caveat.** Runtime-код не менялся. |
| `2c9625d` | математический `L * min_block` в builder docs получил non-representability caveat | **Корректно.** Текст явно связывает идеальную математику с фактическим `checked_mul`. |
| `a71b12b` | исправлены signature и смысл `u8` pin в CHANGELOG | **Корректно.** Pin относится к индексам `0..=255`, поэтому 256 классов валидны. |
| `eaa3310` | уточнено crates.io description | **Корректно и информативнее.** Никаких API/behavior изменений. |
| `a692c47` | MSRV job компилирует `harness=false` benchmark | **Корректно.** `cargo bench --no-run` закрывает target gap и не исполняет benchmark. |
| `bf64ce7` | negative tests для документированных panic conditions | **Корректно.** Каждый test достигает нужного chokepoint, `should_panic` имеет достаточно точный substring. |
| `ff5a2ea` | pin accessors и align-only range rejection | **Корректно.** Дополняет прежний size-driven early rejection независимой align-driven активацией. |
| `b85249a` | общий SEFER/JUMP fixture для test и bench | **Корректно.** Inputs и static placement сохранены; comment-only synchronization заменена единым source. |
| `cdebcfd` | описание `class_for(0,0)` | **Фактически неверно.** `debug_assert` срабатывает до underflow; profile semantics сведены к неверной единственной оси — P2-1. |

## Находки

### P2-1 — BLOCKER: `class_for(0,0)` rustdoc описывает не тот debug panic и смешивает две profile axes

**Где:** `crates/size-classes/src/lib.rs:686-699`, особенно `:689-691`; реальный порядок — `:737-746`.

Документация утверждает:

> this is unchecked even in debug (the underflow itself panics there, loudly)

Но функция начинает выполнение так:

```text
debug_assert!(align.is_power_of_two(), ...);
let need = max(size, align);
...
size2class[(need - 1) >> shift]
```

Для `(size, align) == (0, 0)` `0.is_power_of_two()` ложно. В обычной debug-сборке panic происходит в `debug_assert!` с сообщением `align must be a power of two`; `need` и subtraction ещё не вычислялись. Это дополнительно противоречит следующему абзацу rustdoc (`:701`), который правильно говорит, что нарушение align-precondition trips a `debug_assert!`, и существующему test `class_for_non_pow2_align_violates_debug_assert` (`tests/builder.rs:827-842`), чей комментарий явно закрепляет «before either path arithmetic».

Есть и более глубокая неточность: поведение задаётся не одним переключателем debug/release.

- `debug_assertions = true`: первым срабатывает `debug_assert`, независимо от overflow semantics.
- `debug_assertions = false`, `overflow-checks = true`: assert удалён, затем `need - 1` паникует на underflow.
- `debug_assertions = false`, `overflow-checks = false`: subtraction wraps; затем индекс может либо паниковать OOB, либо для extreme-схем попасть в top bucket и вернуть sentinel.

Rust-профили позволяют настраивать `debug-assertions` и `overflow-checks` независимо, поэтому «debug underflows / release wraps» не является общим контрактом.

Фраза «the one input that violates BOTH preconditions» тоже слишком буквальна: `(0,3)` и другие `size=0` с non-power-of-two align также нарушают обе заявленные обязанности, хотя только `(0,0)` даёт `need == 0`. Если параграф сохранять, нужно говорить «the only input producing `need == 0`», а не «the one input violating both preconditions».

**Предпочтительное исправление:** удалить весь специальный параграф `class_for(0,0)`. Поведение входа, нарушающего сразу две preconditions, не нужно превращать в дополнительное публичное обещание; основной контракт, debug assertion и объяснение `need >= 1` уже достаточны.

Если edge всё же документировать, необходимо перечислить фактический execution order и независимые `debug_assertions`/`overflow-checks`, не обещая конкретный release result для всех схем.

### P4-1 — новый shared fixture использует unrestricted `pub` внутри private helper module

**Где:** `crates/size-classes/tests/common/mod.rs:13-36`.

Это не публичный API крейта: `common` остаётся private child module каждого test/bench binary, поэтому semver surface не расширен. Тем не менее все helper items объявлены `pub`, хотя потребителям достаточно `pub(crate)` (или точечной видимости). Сужение улучшит локальную инкапсуляцию и явно покажет, что файл не предназначен для внешнего reuse.

Пункт опциональный, behavior/size/performance не меняет и публикацию отдельно не блокирует.

## Общий обзор production-кода

### Builder и числовые границы

- `size2class_len` проверяет power-of-two `min_block` и checked `+1`.
- `build_table` валидирует `geo_count`, denominator, exact `N`, extras shape и merged strict monotonicity.
- Geometric advance выполняется в checked `u128`, затем проверяется representability результата в `usize`; min-step fallback также checked.
- Sorted merge принимает interleaving extras и отклоняет geometric collisions в builder chokepoint.
- `growth.0 == 0` намеренно деградирует в linear min-block sequence и не создаёт zero/duplicate classes.
- Ширина `usize <= 64` и hypothetical 128-bit limitation описаны честно.

### LUT и classifier

- Monotone-pointer builder имеет `O(buckets + classes)` и точный limit 256 классов для `u8` indices.
- Top-bucket multiplication защищено `checked_mul`; hand-built table path имеет самостоятельную monotonicity defense.
- Raw accessor теперь правильно отделяет classifier domain от array bounds и не предлагает overflow-prone bound.
- `class_for` rejects `need > small_max` до lookup.
- Fast path — одна LUT access; slow path прыгает через next viable alignment multiple и защищён `checked_add`.
- Stride divisibility и caller-owned base-address alignment больше нигде не смешиваются.
- `SizeClasses` immutable, поля private, deliberate non-`Copy`; большая схема рекомендуется как `static`.

Алгоритмических ошибок, UB, production arithmetic wrap, in-contract panic regressions и hot-path slowdown не найдено.

## Производительность

Новых обязательных ускорений нет:

- fast path уже сводится к `max`, boundary check, shift/index и return;
- power-of-two bitmask избегает division;
- jump path избегает линейного class walk;
- checked widened arithmetic находится только в construction path;
- benchmark сохраняет девять раздельных rows для fast, genuine jump, small-max и huge-policy paths;
- shared fixture предотвращает drift между activation oracle и benchmark inputs.

Рефакторинг fixture не попадает в production artifact. Добавленный MSRV `cargo bench --no-run` увеличивает только CI compile coverage, не runtime cost библиотеки.

## Тестовые оракулы и CI — статическая оценка

- Full SEFER sweep сравнивает classifier с независимым scan predicate.
- Hand-derived golden sequence компенсирует частичную circularity runtime reference builder.
- Три proptest schemes сравнивают jump, walk и scan.
- Normal и extreme raw-index domains закреплены отдельно.
- Negative tests теперь покрывают named public panic conditions и `block_size` OOB.
- 256/257 class boundary, arithmetic overflow, next-multiple wrap и extras interleaving имеют отдельные tests.
- Benchmark jump rows механически разделяют fixture с activation oracle.
- CI содержит package dry-run, debug/release tests, bare-metal `no_std`, all-targets Clippy `-D warnings`, rustdoc `-D warnings`, MSRV library/tests/dev-deps и теперь MSRV benchmark compile.

Safe-only deterministic arithmetic crate не нуждается в Miri/Loom/sanitizer gate. Фактическую зелёность этих gates данный аудит не подтверждает: по требованию пользователя ничего не запускалось.

## Что исправить перед публикацией

1. **Обязательно:** удалить либо исправить параграф `class_for(0,0)` с реальным guard order и независимыми profile knobs.
2. Опционально сузить visibility shared test/bench fixture до `pub(crate)`.
3. После правки выполнить обычные project gates; этот аудит намеренно их не запускал.

## Итог

`size-classes` технически зрел: safe-only, `no_std`, zero production dependencies, checked construction arithmetic, компактный classifier и сильные независимые оракулы. Все substantive замечания прогона 6 исправлены правильно; новая волна также полезно закрыла несколько старых test/docs/CI gaps.

Текущий вердикт — **NO-GO только из-за P2-1**, локальной фактической ошибки в новом необязательном описании invalid-input edge. После удаления или точного исправления этого абзаца ожидаемый вердикт — **GO**; иных release blockers или требующих вмешательства performance issues в просмотренном состоянии не найдено.
