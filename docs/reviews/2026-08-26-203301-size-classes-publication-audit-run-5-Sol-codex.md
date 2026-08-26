# `size-classes`: предрелизный аудит, прогон 5

**Автор:** Сол-кодекс

**Время:** 2026-08-26 20:33:01 Europe/Berlin

**Проверенный HEAD:** `9e5117f099b6fe32699bdba911444fe282d19157`

**База сравнения:** `2679f29e` (отчёт прогона 4)

**Режим:** новый статический анализ в режиме только чтения; без под-агентов. Тесты, сборка, `cargo check`, Clippy, rustdoc, Miri, benchmarks, packaging и publish-команды не запускались. Единственные создаваемые артефакты — этот отчёт и его отдельный commit.

## Вердикт

**NO-GO для публикации в текущем виде из-за одной ошибки публичного low-level контракта. Production-реализация алгоритмов готова к релизу.**

Все четыре исправления после прогона 4 сделаны по существу правильно. Alignment-контракт теперь последовательно отделяет делимость stride от выравнивания адреса; рекомендация хранить большую схему в `static` доведена до документации и повторно используемых fixtures; формулировка про ширину `usize` больше не обещает лишнего.

Однако новое описание `SizeClasses::size2class()` слишком широко описывает поведение при `size > small_max`: не любой такой размер попадает в clamped top bucket. Только ближайший неполный диапазон сверх границы возвращает ложный последний класс; более далёкий размер вычисляет индекс `>= L` и паникует при индексировании. Кроме того, raw-формула требует `size >= 1`, чего accessor не сообщает. Поскольку этот текст добавлен именно как защита от неправильного использования публичного raw LUT, неточная защита должна быть исправлена до публикации.

## Охват и ограничения

Заново просмотрены:

- весь production-код `crates/size-classes/src/lib.rs`;
- `Cargo.toml`, `README.md`, `CHANGELOG.md` и package contents;
- `tests/builder.rs` и `tests/proptest_builder.rs` как статические оракулы;
- `benches/size_classes_bench.rs`, включая реальную активацию jump path;
- size-classes-секции CI: package dry-run, debug/release tests, bare-metal `no_std`, all-targets Clippy, rustdoc и MSRV compile;
- изменения `1957957`, `a3645c6`, `41149ed`, `9e5117f` после прогона 4;
- реальный потребитель `src/alloc_core/size_classes.rs` и поиск прямого использования raw LUT вне крейта.

Применены релевантные части `rust-intel`: numeric boundaries, public contracts, semantic conformance, test oracles, dependencies/features и performance-at-scale. Требование «без под-агентов» соблюдено. Отдельные unsafe/FFI, concurrency, async, Drop/RAII и security-аудиты не требовались: production-код — `#![forbid(unsafe_code)]`, pure const arithmetic без I/O, ресурсов, потоков, atomics, locks, crypto и внешних зависимостей.

## Проверка последних изменений

| Коммит | Изменение | Оценка |
|---|---|---|
| `1957957` | устранены остаточные утверждения «blocks aligned by construction» | **Закрыто корректно.** Публичный контракт и CHANGELOG теперь говорят о кратности class size/stride и отдельно требуют aligned carve base; downstream safety consequence описан явно. |
| `a3645c6` | расширен контракт `SizeClasses::size2class()` | **Регрессия в документации.** Правильное намерение, но overgeneralized out-of-range behavior и пропущена нижняя граница raw-формулы — P2-1. |
| `41149ed` | `static` guidance применён к CHANGELOG и canonical/repeated fixtures | **Закрыто корректно.** Большие повторно используемые схемы больше не показываются как `const`; локальные маленькие test-only схемы допустимо оставлены `const`. |
| `9e5117f` | обещание widened arithmetic сужено до `usize <= 64` | **Закрыто корректно.** Текст точно отражает существующие Rust targets и честно фиксирует работу, которая понадобилась бы гипотетической 128-bit цели. |

## Находки

### P2-1 — BLOCKER: `size2class()` неверно описывает out-of-range индексирование и не задаёт нижнюю границу

**Где:** `crates/size-classes/src/lib.rs:547-559`, в особенности `:553-557`.

Текущий rustdoc утверждает:

> Indexing it directly for a size past small_max lands on the clamped top bucket and returns the LAST class index.

Это верно только для размеров сверх `small_max`, чей вычисленный индекс всё ещё равен `L - 1`. Для большего размера raw array не делает clamp индекса: формула `(size - 1) >> min_block_shift` даёт `L` или больше, после чего обычное индексирование массива паникует.

Точный контрпример для нормальной generated table:

- `min_block = 16`, `small_max = 64`, следовательно `L = 64 / 16 + 1 = 5`;
- `size = 65..=80` даёт индекс `4 == L - 1` и действительно возвращает clamped last-class sentinel — ложный «fits»;
- `size = 81` даёт индекс `5 == L` и паникует out-of-bounds, а не возвращает последний класс.

Для hand-built table, где `small_max` не кратен `min_block`, top bucket дополнительно содержит корректный in-range префикс, уже хорошо описанный в `build_size2class`; это не отменяет границу самого массива.

Есть и вторая пропущенная обязанность: опубликованная формула содержит `size - 1`, поэтому raw caller обязан перед индексированием обеспечить `size >= 1`. При `size == 0` subtraction паникует при включённых overflow checks, а при wrapping semantics превращается в огромный индекс и затем паникует по bounds. `class_for` этой проблемы для валидного `align` не имеет, потому что индексирует по `need = max(size, align)`, а power-of-two `align >= 1`.

**Почему это блокер:** `size2class()` — публичный accessor, а новый текст является его основным misuse barrier. Он не просто неполон, а обещает конкретное поведение там, где реально возможна panic. Пользователь, полагающийся на документацию, может заменить `class_for` прямым lookup и получить профиль/вход-зависимое аварийное завершение либо ложную классификацию.

**Исправление:** сформулировать контракт через обязательные preconditions прямого lookup:

1. `size >= 1`;
2. для обычной классификации `size <= small_max`;
3. после выполнения обеих проверок индекс вычисляется как `(size - 1) >> min_block_shift`;
4. без верхней проверки ближайшие размеры, всё ещё попадающие в bucket `L - 1`, могут вернуть ложный последний класс, а более далёкие размеры дают out-of-bounds panic;
5. raw LUT не учитывает alignment predicate — для `(size, align)` следует предпочитать `class_for`.

Можно дополнительно рассмотреть отдельный safe size-only query вместо выдачи raw array, если внешнему коду не требуется introspection. Удалять accessor необязательно: точного контракта достаточно.

### P4-1 — hardening: тест явно закрепляет top-bucket contents, но не контракт raw indexing domain

`sefer_size2class_matches_scan_for_every_bucket` проверяет каждую существующую ячейку LUT, включая clamped top bucket. Это сильный oracle для содержимого массива, но он естественно не проверяет прямое индексирование `size == 0`, ближайшего out-of-range размера и первого размера с индексом `L`.

После исправления rustdoc полезно добавить маленький contract test, который документирует три зоны на конкретной схеме: valid domain, false-fit sentinel window и first out-of-bounds index. Это не должно превращать panic в желаемый classifier API; тест нужен как защита точного low-level контракта. Пункт не блокирует публикацию отдельно от P2-1.

## Общий обзор production-кода

### Builder и числовые границы

- `size2class_len` проверяет power-of-two `min_block` и защищает `+1` от wrap.
- `build_table` проверяет `geo_count`, denominator, точный `N`, shape extras и monotonicity merged result.
- Extras до, между и после geometric entries корректно sorted-merge-ятся; duplicates с geometric run отклоняются в chokepoint самого builder-а.
- Geometric multiply/divide выполняется в checked `u128`, затем результат проверяется на representability в `usize`; min-step fallback также checked.
- Для существующих целей с `usize <= 64` representable quotient не отклоняется из-за промежуточного `usize` overflow. Ограничение будущей 128-bit цели теперь документировано точно.
- Runtime и const-eval получают одинаковые явные contract failures вместо release-only wrap в валидно выглядящую таблицу.

### LUT и classifier

- LUT builder использует monotone pointer с `O(buckets + classes)` const-eval, а не повторный scan каждого bucket.
- Ограничение `N <= 256` точно соответствует индексам `u8` `0..=255`; 256 классов валидны, 257 отклоняются.
- `class_for` делает early rejection до lookup и для in-contract input не достигает sentinel как ответа.
- Fast path — одна lookup access; делимость гарантируется кратностью class size, а address alignment честно оставлен precondition потребителя.
- Slow path через bitmask и re-seed эквивалентен линейному поиску по predicate, но перескакивает runs неподходящих классов; `checked_add` закрывает overflow следующего кратного.
- `SizeClasses` immutable, поля private, не `Copy`; готовую большую схему правильно рекомендуется хранить в `static`.

Алгоритмических ошибок, UB, новых panic paths для in-contract `class_for`, semantic drift с документированным fit predicate или лишней работы в hot path не найдено.

## Производительность и возможности ускорения

Обязательных оптимизаций перед публикацией не найдено:

- per-allocation fast path уже сводится к `max`, boundary branch, shift/index и return;
- slow path избегает division и линейного продвижения по классам;
- checked widened arithmetic выполняется только при построении схемы;
- benchmark разделяет small hit, реально активированный jump path, границы `small_max` и policy query;
- activation test защищает slow-path benchmark от незаметного превращения в single-check case.

Не стоит усложнять layout ради удаления последней boundary branch без новых измерений. Замеченная у in-tree shim отдельная копия standalone `SIZE2CLASS` рядом со встроенной в `SC` — осознанная compatibility trade-off потребителя, а не дефект публикуемого крейта; сам потребитель не индексирует результат `SC.size2class()` напрямую.

## Тестовые оракулы и CI — статическая оценка

Корпус остаётся сильным:

- hand-derived golden sequence снижает риск circular reference oracle;
- полный SEFER sweep сравнивает `class_for` с независимым scan predicate;
- три property schemes сравнивают jump, walk и scan;
- отдельно покрыты overflow boundaries, 256/257 classes, extras interleaving, debug-only non-power-of-two precondition и README example;
- benchmark rows имеют activation oracle;
- CI содержит package dry-run, debug/release tests, bare-metal `no_std`, all-targets Clippy `-D warnings`, rustdoc `-D warnings` и MSRV compile с dev-dependencies.

Safe-only deterministic arithmetic crate не нуждается в Miri, Loom или sanitizer gate. Фактическую зелёность перечисленных gates этот аудит не подтверждает: по требованию пользователя ничего не запускалось.

## Что исправить перед публикацией

1. **Обязательно:** переписать контракт `SizeClasses::size2class()` по точным нижней и верхней границам и разделить false-sentinel window от out-of-bounds panic.
2. Желательно вместе с правкой добавить компактный raw-domain contract test.
3. После изменения выполнить обычные проектные gates; этот аудит намеренно их не запускал.

## Итог

Кодовая часть `size-classes` выглядит зрелой: компактная safe-only реализация, защищённая арифметика, корректный `no_std` дизайн, быстрый classifier и хорошие независимые оракулы. Все замечания прогона 4, кроме породившего новую неточность расширения raw-LUT docs, закрыты качественно.

Текущий вердикт — **NO-GO только из-за P2-1**. После точного описания raw indexing domain и при зелёных внешних gates ожидаемый вердикт — **GO**; иных release blockers или требующих вмешательства performance problems в просмотренном состоянии не найдено.
