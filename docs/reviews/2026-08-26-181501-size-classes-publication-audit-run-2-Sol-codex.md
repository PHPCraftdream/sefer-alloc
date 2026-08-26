# `size-classes`: аудит готовности к публикации, прогон 2

**Автор:** Сол-кодекс  
**Время:** 2026-08-26 18:15:01, Europe/Berlin  
**Проверенный HEAD:** `21e805493764c8706edece01e8748a597a582042`  
**База сравнения:** `097d34bc5fbe6f636469a8fce4512a4ec6f255e9` (HEAD прогона 1)  
**Режим:** статическое чтение одним агентом; тесты, сборка, clippy, rustdoc, Miri, benchmarks, packaging и publish-команды не запускались.

## Вердикт

**NO-GO до исправления P2-1; после него — повторная короткая проверка границы и вероятный GO.**

Все пять групп замечаний прогона 1 закрыты. Checked-арифметика теперь устраняет release-wrap, `build_table` сам обеспечивает обещанную монотонность и запрещает zero-class, benchmark измеряет реальные ветви, а CI получил package-level clippy/rustdoc/MSRV/publish-dry-run gates. Новый mask-based slow path корректен на заявленном power-of-two домене и убирает деление из горячего цикла.

Свежий просмотр с нуля обнаружил один новый точный boundary-дефект публичного API: таблица из **ровно 256** классов полностью представима индексами `u8` (`0..=255`), но builder ошибочно отвергает её как непредставимую. Это не UB и не release-only corruption, однако это ложное публичное ограничение и off-by-one на собственной capacity boundary; перед первым publish его лучше не цементировать.

Дополнительно найдены три улучшения уровня P3 и один P4. Ни одно из них не ухудшает типичный hot path прямо сейчас, но первые три разумно решить до публикации, раз обратная совместимость ещё не связывает API.

| Приоритет | Количество | Результат |
|---|---:|---|
| P0/P1 | 0 | UB, unsafe/FFI, races, allocation/locking defects не найдены |
| P2 | 1 | блокирует безусловный GO |
| P3 | 3 | исправить до первого publish желательно |
| P4 | 1 | качество документации/примеров |

## P2-1. `build_size2class` ошибочно запрещает безопасную таблицу из 256 классов

**Где:** `crates/size-classes/src/lib.rs:444-466`, cast — `crates/size-classes/src/lib.rs:531`.

Текущая проверка:

```rust
assert!(
    N < 256,
    "size2class entries are u8; the class count must stay below 256"
);
```

ошибается на единицу. В lookup хранится **индекс класса**, а не количество классов. При `N = 256` допустимые индексы равны `0..=255`, то есть каждый из них точно представим в `u8`. Непредставимый индекс `256` впервые появляется только при `N = 257`.

Контрпример полностью укладывается в остальные контракты builder:

- `min_block = 1`;
- `growth = (0, 1)`;
- `geo_count = 256`;
- `extras = []`;
- итоговая строго возрастающая таблица — `1..=256`;
- `L = size2class_len(256, 1) = 257`;
- последний валидный class index — `255`.

`build_table` способен математически построить такую таблицу без overflow, но `build_size2class::<256, 257>` падает только из-за ложной границы. Rustdoc повторяет ошибку: «`table.len() >= 256`» и «beyond 255 classes would silently truncate». Ровно 256 классов не truncates.

Это также означает, что текущая проверка не соответствует заявленной формулировке «class count fits a `u8`»: count действительно не помещается как значение `u8`, но хранится не count — хранятся индексы. Семантически правильная инварианта: `N - 1 <= u8::MAX`.

**Исправление:** разрешить `N <= u8::MAX as usize + 1` (или эквивалентное `N <= 256`), синхронизировать rustdoc/panic message и добавить прямые boundary-регрессии:

1. `N = 256` успешно строится; последний lookup entry равен `255`.
2. `N = 257` падает с точным сообщением.
3. Oracle сканирует все buckets 256-классной линейной схемы.

Почему P2: публичный builder отвергает корректный и полностью представимый вход на точной границе representation. Исправление до первого publish тривиально; после публикации ложное ограничение легко начинает восприниматься как намеренная часть контракта.

## P3-1. `checked_mul` сообщает ложный overflow, когда математический результат отношения помещается

**Где:** `crates/size-classes/src/lib.rs:184-194`, `322-370`.

Builder вычисляет рациональный шаг так:

```rust
cur.checked_mul(num)?.div_ceil(den)
```

Это защищает от release-wrap, но проверяет промежуточное произведение `cur * num`, а не математический результат `ceil(cur * num / den)`. Произведение может не помещаться в `usize`, хотя после деления итог и следующий `min_block`-шаг полностью представимы.

Точный 64-bit контрпример:

```text
min_block = 2^62
growth = (3, 3)
geo_count = 3
extras = []
```

Математически схема корректна:

```text
[2^62, 2^63, 3 * 2^62]
```

На втором advance `cur = 2^63`; отношение `(cur * 3) / 3` равно `cur`, после чего minimum step даёт `3 * 2^62`, всё ещё меньше `usize::MAX`. Но промежуточное `2^63 * 3` переполняет `usize`, поэтому текущая реализация отвергает представимую схему.

Rustdoc сейчас честно обещает panic на overflow advance step, поэтому это не скрытая поломка существующего задокументированного поведения. Это всё же ненужное ограничение математического домена. Для универсального builder лучше вычислять `ceil(a*b/c)` без intermediate overflow: на поддерживаемых Rust targets можно использовать `u128`-промежуточное значение с checked conversion назад либо quotient/remainder алгоритм с сокращением множителей.

При исправлении нужны два теста: ложный overflow выше должен успешно дать точную таблицу; истинный overflow итогового следующего класса должен продолжать падать.

## P3-2. `Copy` на объекте порядка 16 KiB создаёт дешёвый синтаксис для дорогих копий и фиксирует API

**Где:** `crates/size-classes/src/lib.rs:537-566`; потребитель подтверждает размер и риск `.rodata` duplication в `src/alloc_core/size_classes.rs:175-194`.

`SizeClasses<N, L>` содержит обе массивные таблицы непосредственно и выводит `Copy`. Для SEFER-конфигурации сам rustdoc оценивает объект примерно в 16 KiB. Это позволяет неявно копировать весь объект обычным присваиванием, pattern binding или передачей по значению. Оптимизатор часто устранит копию, но API этого не гарантирует, а неудачная generic/ABI boundary может материализовать её.

Комментарий уже советует передавать объект по ссылке, то есть признаёт, что семантика маленького `Copy`-value здесь вводит в заблуждение. `Clone` оставляет дешёвую доступность явного дублирования, но заставляет call site показать намерение. До первого publish удаление `Copy` также сохраняет свободу позже добавить non-`Copy` representation; после publish это станет breaking change.

**Рекомендация:** оставить `Debug, Clone`, убрать `Copy` с `SizeClasses`; `Params` остаётся уместным `Copy`. Проверить потребителей статически/компиляцией уже в remediation-цикле. Если `Copy` всё же является осознанной публичной гарантией, закрепить compile-time API test и явно принять стоимость/semver one-way door в design note, а не только в rustdoc-предупреждении.

## P3-3. Документация ошибочно утверждает, что interleaving `extras` с геометрией запрещён

**Где:**

- `crates/size-classes/src/lib.rs:209-213`;
- `crates/size-classes/src/lib.rs:379-405`;
- `crates/size-classes/CHANGELOG.md:16-29`.

`build_table` — это sorted merge. Корректная дополнительная точка между двумя геометрическими классами является одной из главных причин существования `extras` и принимается кодом. Текущая SEFER-схема сама использует такой случай: extra `256` вставляется между соседними значениями геометрического ряда.

Но CHANGELOG говорит, что «an `extras` entry that ties **or interleaves** with the geometric run is rejected», а panic message сообщает «overlaps or interleaves». Это неверно. При двух отдельно строго возрастающих входах merge всегда неубывает; финальная strict-monotonicity проверка может обнаружить только равенство/duplicate между рядами. Валидное interleaving остаётся строго возрастающим и не должно отвергаться.

**Исправление:** везде заменить «ties or interleaves»/«overlaps or interleaves» на точное «duplicates/ties a geometric value». Добавить позитивный regression test, где aligned extra лежит строго между геометрическими значениями и сохраняется в результате. Негативный overlap test уже существует.

Это документационный дефект, а не production bug: код ведёт себя правильно. Однако ложное описание фундаментальной возможности generic builder не стоит публиковать как контракт первого релиза.

## P4-1. Публичные docs и исходник перегружены историей ревью

**Где:** особенно `crates/size-classes/src/lib.rs:53-65`, `94-101`, `147-167`, `222-244`, `322-396`, `490-520`, `679-713`, а также README/CHANGELOG.

В 768-строчном `lib.rs` значительная часть текста — task numbers, прежние дефекты, имена ревью и длинные fault narratives. Часть находится в обычных комментариях и мешает сопровождению реализации; часть — в `///` и попадёт непосредственно на docs.rs. README также сообщает читателю, какой старый текст исправил task #731, вместо краткого текущего контракта.

История полезна, но её источник — git commits, review reports и CHANGELOG. Публичный rustdoc должен отвечать на вопросы потребителя: допустимые входы, результат, сложность, panic/precondition и короткое rationale. Сейчас важный контракт приходится вычленять из археологии.

Рекомендую перед publish сделать documentation-only compression:

- оставить точные preconditions и контрпример только там, где без него нельзя понять инварианту;
- task/review IDs и рассказ о старом коде убрать из rustdoc в commit history/design note;
- README-пример построить без magic `MAX_CLASS`: импортировать `build_table`, получить `TABLE[N - 1]`, затем вычислить `L`;
- использовать обычную consumer-oriented формулировку без «этот текст раньше утверждал…».

## Проверка исправлений прогона 1

### P2-1: release-dependent arithmetic — закрыто

Коммит `cc94a46` заменил опасные операции на `checked_add`/`checked_mul`, переиспользовал `size2class_len` в проверке `L` и сделал overflow alignment round-up явным `None`. Добавленные boundary tests покрывают runtime/const формы и hand-built table, где старый wrap действительно менял ответ. При статическом чтении обходного unchecked sibling не найдено.

### P2-2: контракт standalone `build_table` — закрыто

Коммит `b970f52` добавил final merged-table check непосредственно перед возвратом и оставил downstream check как defense in depth. `extras < min_block` теперь запрещены. Тесты разделяют SUT и используют различимые panic prefixes. Кодовое решение корректно; остаётся только неточное слово interleaving в документации (P3-3).

### P3-1: benchmark huge/small boundary — закрыто

Коммит `37c624d` разделил `class_for` около `SEFER_MAX` и `is_huge` около policy threshold. Коммит `34b8ce2` заменил псевдо-slow rows на пары, где seed не кратен align, и добавил path-activation oracle. Структура benchmark теперь соответствует измеряемым ветвям.

### P3-2: panic docs — закрыто

Все deliberate panic axes перечислены значительно точнее: invalid parameters, length overflow, progression overflow, merged duplicate. Новый P2-1 требует лишь поправить capacity boundary.

### P3-3: package-level gates — закрыто

Коммит `f30f2c6` добавил:

- `cargo clippy -p size-classes --all-targets -- -D warnings`;
- crate-specific rustdoc `-D warnings`;
- `cargo publish --dry-run -p size-classes`;
- MSRV check и test graph compile;
- `[lints] workspace = true`.

Bare-metal, debug и release test rows сохранены. Для featureless, safe, zero-production-dependency crate этого набора достаточно; отдельные Miri/loom gates не нужны.

## Обзор остальных последних правок

- `5be0a65`: замена `is_multiple_of` на mask test семантически эквивалентна при документированной power-of-two precondition; проверка остаётся после debug assertion. Это разумное ускорение hot slow-path.
- `e6ed779`: появились адресные overflow regressions; особенно полезен hand-built-table oracle, поскольку он не маскирует старый wrap структурой `build_table`.
- `5ffb85c`…`21e8054`: серия documentation hygiene commits исправила const/runtime wording, attribution checks, extras contract, top-bucket semantics и отсутствие default scheme. Большинство уточнений правильны, но накопительная стратегия добавляла историю исправления прямо в публичный текст; P4-1 предлагает финальную компрессию.
- `8a277f6`/`21e8054`: top-bucket sentinel теперь правильно scoped только к `SizeClasses::class_for`; standalone hand-built tables не обобщены ошибочно.

## Общий обзор кода с нуля

### Safety и зависимости

- `#![no_std]`, `#![forbid(unsafe_code)]`; unsafe, raw pointers, FFI, atomics, locks и interior mutability отсутствуют.
- Production dependencies отсутствуют. Dev dependencies соответствуют тестам/benchmark.
- Нет async, cancellation, Drop/RAII resource lifecycle, serialization или shared-state поверхностей — соответствующие rust-intel модули неприменимы.
- Borrowed `Params::extras` оправдан zero-allocation const-дизайном; lifetime не хранится в построенном `SizeClasses`.

### Корректность алгоритмов

- Merge линейный по числу классов и проверяет обе входные формы плюс итоговую уникальность.
- Geometric advance теперь fail-loud во всех явно проверенных арифметических местах; P3-1 касается излишне узкого домена, не silent wrap.
- `build_size2class` использует monotone pointer: O(buckets + classes), без повторного поиска с нуля.
- `class_for` корректно берёт `max(size, align)`, отвергает область выше `small_max`, использует O(1) seed и jump по следующему power-of-two multiple.
- `checked_add` гарантирует termination на верхней границе адресного пространства.
- `align` как debug-only precondition — приемлемое решение для API, ориентированного на `Layout`; нарушение явно out of contract и не создаёт memory unsafety внутри safe crate.

### Производительность

Основной путь близок к минимуму: сравнение, boundary branch, LUT load, fast-path branch. Allocation, division и linear scan на типичном пути отсутствуют. Slow path теперь проверяет кратность маской и перескакивает через lookup, а не идёт по одному классу.

Новых production-ускорений, которые стоило бы применять без измерения, не найдено. Убирать release-active validation builder ради скорости не нужно: builder обычно const/one-time, а correctness важнее его latency. Реальный perf/API вопрос — только неявное копирование большого `Copy`-объекта (P3-2).

### Тестовые oracle

Сильные стороны:

- независимый `reference_table` и scan-based `reference_class_for`;
- полный практический SEFER sweep;
- jump-vs-walk-vs-scan property checks на трёх схемах;
- точные panic substrings, разделённые по chokepoint;
- debug/release-sensitive regressions;
- u128 oracle для overflow bucket math;
- benchmark path-activation sentinel.

Оставшиеся пробелы соответствуют находкам:

- нет `N=256` success / `N=257` failure boundary;
- нет representable-result/intermediate-product-overflow схемы;
- нет позитивного interleaving regression, который защищает именно разрешённую возможность;
- property tests варьируют запросы внутри трёх фиксированных схем, но не сами параметры — ограничение const generics честно описано.

## Рекомендуемый порядок

1. Исправить `N=256` off-by-one и добавить обе boundary-регрессии — обязательный пункт для GO.
2. Исправить ложный intermediate overflow либо явно принять его как design limit с отдельным тестом/формулировкой.
3. До первого publish решить, действительно ли большой `SizeClasses` должен быть `Copy`.
4. Исправить interleaving wording и добавить позитивный test oracle.
5. Сжать публичную документацию, сохранив точные текущие контракты.

После пункта 1 достаточно короткого статического re-review исправления и соседних границ. При стремлении к «совершенству без компромиссов» рекомендую закрыть также P3-1…P3-3 до выдачи окончательного GO.

## Ограничения исследования

По прямому требованию аудит выполнен без под-агентов и без исполнения проекта. Это bounded single-context применение `rust-intel`: полностью просмотрены применимые области numeric/data, public API/lifetimes, testing/CI, dependencies/ergonomics и semantic conformance; async, concurrency, unsafe/FFI, crypto и resource-drop категории исключены после структурного подтверждения, что соответствующих конструкций в крейте нет.

Единственное разрешённое изменение — этот Markdown-отчёт и его отдельный commit. Все выводы основаны на чтении текущих файлов, истории и конфигурации; зелёный toolchain-прогон данным отчётом не подтверждается.
