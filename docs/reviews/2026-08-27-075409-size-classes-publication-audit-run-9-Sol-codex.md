# `size-classes`: предрелизный аудит, прогон 9

**Автор:** Сол-кодекс

**Время:** 2026-08-27 07:54:09 Europe/Berlin

**Проверенный HEAD:** `cd7ed7127d6f9c5775ac1715115e4d62bda7fddc`

**База сравнения:** `8b5cfe3` (commit отчёта прогона 8)

**Режим:** новое статическое исследование в режиме только чтения, без под-агентов. Тесты, сборка, `cargo check`, Clippy, rustdoc, Miri, benchmarks, packaging, publish и любые Cargo-команды не запускались. Единственные создаваемые артефакты — этот отчёт и его отдельный commit.

## Вердикт

**GO — крейт готов к первой публикации.**

Оба замечания прогона 8 закрыты корректно. Публичный rustdoc и test-comment теперь честно описывают realistic `SizeClasses` как примерно 16 KiB суммарно, почти полностью занятые LUT; consumer-comment больше не утверждает, что `Debug` derived. Последующая волна также точно документировала ceiling division, добавила реальные 32-bit boundary tests и CI execution, улучшила README и поставила compile-time drift guard в root-consumer.

Полный повторный просмотр не обнаружил алгоритмических ошибок, unchecked arithmetic на контрактных входах, UB, API-contract regressions, проблем package surface или требуемых ускорений hot path. Единственная новая находка — P4 wording в CI-комментарии, который историческую причину новой строки формулирует в настоящем времени и потому уже не буквально верен после добавления 32-bit tests. На команды, coverage и публикационную готовность это не влияет.

## Охват и ограничения

Просмотрены заново:

- весь `crates/size-classes/src/lib.rs`;
- все изменения после прогона 8: `33d647a`, `fa4ba69`, `efcd142`, `3e41d0b`, `e99dc92`, `df17fd9`, `cd7ed71`;
- `Cargo.toml`, `README.md`, `CHANGELOG.md`, обе лицензии и tracked package file set;
- `tests/builder.rs`, `tests/proptest_builder.rs`, общая fixture и benchmark;
- package/no_std/debug/release/Clippy/rustdoc/MSRV/i686 CI rows;
- root-consumer shims `src/alloc_core/size_classes.rs` и `src/alloc_core/segment_layout.rs`, включая новый const drift guard;
- реальные consumer call sites classifier/accessors по workspace.

Численные границы дополнительно проверены независимым exact-integer replay вне кода крейта: при `min_block = 16`, `growth = (5,4)` первый непредставимый класс действительно имеет `geo_count = 84` на 32-bit и `183` на 64-bit; предыдущие `83` и `182` помещаются.

По `rust-intel` выполнен bounded single-context review релевантных numerics/data, public API, dependencies/features, tests/oracles, semantic conformance и performance-at-scale аспектов. Требование пользователя «без под-агентов» имеет приоритет над рекомендуемым skill fan-out. Async, unsafe/FFI, concurrency, crypto, Drop/RAII и resource lifecycle неприменимы: production-код — pure safe const arithmetic с `#![forbid(unsafe_code)]`, без I/O, ресурсов, потоков и production dependencies.

Существовавшие untracked logs, checkpoints и чужие review-файлы не изменялись и в commit не включаются.

## Обзор последних правок

| Коммит | Изменение | Оценка |
|---|---|---|
| `33d647a` | исправлена оценка размера raw Debug output | **Полностью закрывает P2-1 прогона 8.** `table: [usize;49]` отделена от примерно 16 KiB LUT; соседние docs теперь согласованы. |
| `fa4ba69` | `derives Debug, Clone` заменено на `implements Debug, Clone` | **Корректно закрывает опциональный P4-1.** Runtime/API не затронуты. |
| `efcd142` | добавлен внешний publication audit | Documentation-only commit; production artifact не меняет. Его вывод не использовался как замена собственному повторному просмотру. |
| `3e41d0b` | формула роста явно получила `ceil` | **Корректно.** Совпадает с `u128::div_ceil` в реализации и устраняет floor-interpretation Rust-читателем. |
| `e99dc92` | README fence `text` → `rust` | **Корректно.** README не включён в rustdoc и сам по себе не doctest; mirror test остаётся compile oracle. |
| `df17fd9` | boundary tests 83/84 и 182/183, i686 CI execution | **Корректно и независимо подтверждено.** Success/panic пары закрепляют обе стороны границы; настоящий 32-bit target закрывает width gap. |
| `cd7ed71` | root compile-time guard для `SMALL_ALIGN_MAX`; fixture отмечена snapshot | **Корректно.** Guard compile-time-only, не попадает в runtime path; snapshot limitation больше не скрыта. |

## Находки

### P4-1 — CI rationale после собственной правки сформулирован как всё ещё действующий факт

**Где:** `.github/workflows/ci.yml:1852-1857`.

Комментарий новой i686 строки начинается утверждением:

```text
every extreme-value test in this crate is
#[cfg(target_pointer_width = "64")]-gated
```

Но этот же commit уже добавил две `#[cfg(target_pointer_width = "32")]` boundary tests. Причина изменения исторически верна — **до** `df17fd9` extreme coverage было только 64-bit, — однако настоящее время делает текущий комментарий буквально ложным.

**Исправление:** заменить `is` на `was` и, при желании, уточнить «before this row/change». Это чистая поддерживаемость CI-документации; команды, target installation и test execution корректны. Публикацию не блокирует.

## Общий обзор production-кода

### `Params` и построение table

- `Params` — plain `Copy` data, `#[non_exhaustive]`, с `const fn new`; добавление будущих policy fields не требует внешних struct literals.
- `size2class_len` проверяет power-of-two `min_block` и checked `+1`.
- `build_table` проверяет `min_block`, `geo_count`, denominator, exact `N`, alignment/range/order `extras` и strict monotonicity итогового merge.
- Геометрический шаг реализует документированное `round_up(ceil(cur * num / den), min_block)` в widened `u128`.
- Actual result проверяется на representability в `usize`; minimum-step fallback использует `checked_add`.
- `num <= den`, включая zero numerator, намеренно деградирует к linear min-block sequence без duplicates.
- Interleaving extras принимаются; collision с geometric class отклоняется в собственном chokepoint `build_table`.
- Для поддерживаемых 32/64-bit `usize` произведение двух widened `usize` помещается в `u128`; hypothetical 128-bit limitation явно документировано.

### Построение LUT

- `build_size2class` принимает только non-empty, strictly increasing table и power-of-two `min_block`.
- Bound `N <= 256` точен для `u8` indices `0..=255`.
- `L` сверяется через единый `size2class_len`, без дублированной overflow-prone формулы.
- Monotone pointer даёт `O(L + N)`, а не повторный scan каждого bucket.
- `(k + 1) * min_block` использует `checked_mul`; overflow и top bucket корректно сводятся к последнему class sentinel.
- Документация hand-built table честно описывает возможность monotone, но недостижимого через buckets промежуточного класса.

### `SizeClasses` и classifier

- Поля private и immutable; большой объект намеренно `Clone`, но не `Copy`, рекомендуемое размещение — `static`.
- Ручной `Debug` печатает summary и не раскрывает тысячи LUT entries; object layout и hot path от этого не меняются.
- Accessors `const`, без аллокаций; raw LUT accessor подробно документирует собственные domain obligations.
- `class_for` вычисляет `need = max(size, align)` и отклоняет `need > small_max` до indexing.
- Fast path для `align <= min_block` — max, range check, shift/LUT access и return.
- Slow path для больших power-of-two alignments использует mask-divisibility и перескакивает к следующему возможному multiple вместо линейного class walk.
- `(block | (align - 1)) + 1` защищён `checked_add`; wrap у `usize::MAX` возвращает `None`.
- Termination следует из strict table growth и `next_mult > block`.
- Power-of-two alignment — явно заявленная precondition с debug assertion; release check сознательно не добавлен в hot path.
- Stride divisibility не выдаётся за address-alignment guarantee: caller-owned base requirement подробно и корректно описан.

Алгоритмических дефектов, profile-dependent wrap на контрактных путях, in-contract panic regression или safety-проблем не найдено.

## API и публикационный контур

- Публичная поверхность мала и связна: `Params`, `size2class_len`, `build_table`, `build_size2class`, `SizeClasses` и query accessors.
- `no_std`, `forbid(unsafe_code)` и zero production dependencies подтверждаются исходником и manifest.
- Metadata содержит license, repository, homepage, documentation, README, keywords и categories.
- Tracked crate set включает исходник, manifest, README, CHANGELOG, обе license files, tests и benchmark.
- `README` construction example синхронизирован compile mirror test; rustdoc construction recipe использует `text` fence сознательно.
- CHANGELOG описывает фактическую ещё не выпущенную поверхность и числовые contracts.
- Root-consumer использует те же builders и classifier через тонкий shim; новый guard закрывает потенциальный future drift `SMALL_ALIGN_MAX`.

## Тестовые оракулы — статическая оценка

- Independent reference builder/classifier и hand-derived golden sequence уменьшают circularity.
- Полный SEFER size/alignment sweep проверяет smallest fit и divisibility.
- Три proptest schemes сравнивают jump, linear walk и independent scan.
- Normal/extreme raw LUT domains, sentinel behavior и OOB boundary закреплены отдельно.
- Negative tests имеют конкретные expected panic substrings и валидные preceding inputs.
- 256/257 class capacity, next-class overflow, intermediate-product widening, min-step overflow, length overflow и next-multiple wrap покрыты targeted cases.
- Новые success/panic pairs закрепляют точные 32/64-bit geometric boundaries.
- Benchmark jump inputs разделяют fixture с path-activation oracle, поэтому не деградируют незаметно в trivial seed hit.
- Debug summary test проверяет отсутствие raw field names, а не хрупкую полную строку formatting output.

## CI — статическая оценка

В workflow присутствуют:

- package publish dry-run;
- bare-metal `thumbv7em-none-eabi` no_std build;
- debug и release test rows;
- all-targets Clippy с `-D warnings`;
- rustdoc с `-D warnings`;
- MSRV library, tests/dev-dependencies и benchmark compile;
- настоящий i686 test execution с установленным Rust target и `gcc-multilib`.

Добавление i686 в stable toolchain target list и установка 32-bit linker/runtime перед execution согласованы. Отдельный release-i686 row не обязателен: проверяемые boundary branches используют explicit checked/assert behavior, а profile split уже покрывается основным 64-bit debug/release набором.

Фактическую зелёность CI этот аудит не подтверждает: по прямому требованию пользователя ничего не запускалось.

## Производительность

Обязательных улучшений не найдено:

- runtime fast path уже минимален и не содержит division;
- slow path использует bitmask и jump lookup;
- checked `u128` arithmetic находится только на construction/const-eval path;
- classifier state contiguous и immutable;
- root drift guard повторно выполняет build только при compile-time evaluation и не добавляет runtime/binary cost;
- benchmark разделяет действительно разные ветви classifier и policy check.

Публичное хранение одновременно class table и LUT — осознанная цена O(1) classifier. Альтернативы с borrowed/prebuilt arrays усложнили бы const API и ownership ради экономии, которая для realistic scheme в основном равна одной LUT и уже явно документирована. Без профиля и consumer demand redesign не оправдан.

## Перед публикацией

Обязательных исправлений нет.

Опционально привести CI rationale к историческому времени (`was ... gated`) для точности будущих ревью.

После принятого GO следует выполнить штатные project gates; этот аудит намеренно их не запускал и не подтверждает их фактический результат.

## Итог

`size-classes` готов к публикации: safe-only, `no_std`, без production dependencies, с checked const construction, точным LUT builder, быстрым alignment-aware classifier, сильными boundary/property оракулами и полным статически видимым release-контуром CI.

Все замечания прогона 8 исправлены без регрессий. Новые коммиты дополнительно укрепили документацию и 32-bit coverage. Итоговый вердикт — **GO**; обязательных исправлений или ускорений не найдено.
