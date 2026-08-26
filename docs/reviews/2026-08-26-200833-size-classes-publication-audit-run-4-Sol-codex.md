# `size-classes`: предрелизный аудит, прогон 4

**Автор:** Сол-кодекс

**Время:** 2026-08-26 20:08:33 Europe/Berlin

**Проверенный HEAD:** `74b49c8a6ae727491b73e73b705fbcd47ba92e21`

**База сравнения:** `36daf957b2ce4844003df1b6db4f17361cb8459e` (отчёт прогона 3)

**Режим:** статический анализ в режиме только чтения; без под-агентов; тесты, сборка, `cargo check`, Clippy, rustdoc, Miri, benchmarks и packaging-команды не запускались. Единственные создаваемые артефакты — этот отчёт и его отдельный commit.

## Вердикт

**NO-GO для публикации в текущем виде, но кодовая реализация близка к GO.**

Все четыре группы исправлений после прогона 3 сделаны по существу правильно. Алгоритмических регрессий, новых ошибок hot path, unsafe/FFI/concurrency-рисков или production-зависимостей не найдено. Однако блокирующий alignment-контракт исправлен не полностью: подробный раздел `class_for` теперь корректно требует выровненный carve base, но несколько других публичных мест всё ещё утверждают, что кратность размера класса сама делает каждый блок выровненным. Ещё опаснее итоговая фраза, что нарушение base-precondition не может вызвать unsafety: внутри safe-only крейта — да, но в allocator-потребителе misaligned result нарушает контракт `Layout` и может стать причиной downstream UB.

До публикации необходимо сделать alignment-формулировки непротиворечивыми во всей публичной поверхности. Это локальная правка документации; после неё иных блокеров в просмотренном состоянии я не вижу.

## Охват и ограничения

Просмотрены заново:

- весь `crates/size-classes/src/lib.rs`;
- `Cargo.toml`, `README.md`, `CHANGELOG.md`, обе лицензии;
- весь текст `tests/builder.rs` и `tests/proptest_builder.rs`, без исполнения;
- benchmark и его activation-oracles/итерационные настройки;
- size-classes-секции CI, MSRV, `no_std`, package/release workflow;
- все изменения после прогона 3: `408f985`, `d2fa219`, `30a8be8`, `74b49c8`;
- реальный потребитель `src/alloc_core/size_classes.rs`, forwarding API и гарантия 4 MiB-aligned segment base в `src/alloc_core/os.rs`/NUMA path.

По `rust-intel` полностью применены релевантные модули: numerics/data, public API/semver, tests/oracles, dependencies/features и semantic conformance. Требование пользователя «без под-агентов» имеет приоритет над рекомендованным skill fan-out, поэтому это bounded single-context review. Async, unsafe/FFI, concurrency, security и Drop/RAII не проходились как отдельные тематические аудиты: структурный просмотр подтверждает, что production-код крейта — `#![forbid(unsafe_code)]`, pure const arithmetic без I/O, ресурсов, потоков, atomics, locks, crypto и async.

## Перепроверка последних исправлений

| Коммит | Что изменено | Результат |
|---|---|---|
| `408f985` | stride/base alignment contract и generic-формулировка motivating `512` | **Основная идея исправлена правильно, но закрытие неполное.** README и основной раздел `class_for` теперь точно разделяют stride и address. Остаточные противоречия — P2-1 ниже. |
| `d2fa219` | публичная рекомендация `static`, а не большой `const` | **Корректно.** README, benchmark и mirrored README-test переведены на `static`; production shim уже использовал `static`. Осталась небольшая внутренняя несогласованность — P4-1. |
| `30a8be8` | hand-built LUT больше не приписан недоступному `class_for` path | **Закрыто корректно.** Документ ясно отделяет standalone LUT от `SizeClasses::build`. |
| `74b49c8` | `u128` overflow proof заменён `checked_mul`/`checked_add` | **Корректно на всех существующих Rust target pointer widths.** Representable 64-bit quotient не отклоняется, настоящий выход за `usize` остаётся loud error; runtime query path не изменён. Узкая future-proof оговорка — P4-2. |

## Находки

### P2-1 — BLOCKER: исправленный base-alignment контракт всё ещё противоречит соседним публичным обещаниям

Правильная формулировка теперь находится в `crates/size-classes/src/lib.rs:596-605,645-657`: делимость `block_size` — свойство stride; адреса выровнены только если `base % align == 0`.

Но ей противоречат:

- `src/lib.rs:65-67`: «Every generated class is a multiple of it, so every block is naturally `min_block`-aligned»;
- `src/lib.rs:508-511`: `small_align_max = min_block`, потому что «every block is `min_block`-aligned by construction»;
- `src/lib.rs:607-608`: fast path снова объяснён через «every block is `min_block`-aligned»;
- `CHANGELOG.md:56-59`: то же публичное утверждение;
- внутреннее доказательство у `src/lib.rs:214-216` называет одной лишь кратностью extras достаточной для sound fast path, хотя она достаточна только для stride predicate.

Во всех этих местах истинно более узкое утверждение: каждый **размер класса** кратен `min_block`, поэтому каждый **stride** сохраняет уже существующее выравнивание carve base. Сам адрес блока из этого не следует.

Особенно проблемна фраза `src/lib.rs:654-657`:

> A violation cannot corrupt the scheme or cause unsafety ...

Она верна только в очень узком смысле «этот safe arithmetic crate сам не исполняет UB». Для allocator-потребителя нарушение даёт адрес с `base % align != 0`; возврат такого указателя для валидного `Layout` нарушает allocator contract, а downstream-код вправе выполнять aligned accesses. То есть integration consequence может быть safety-critical.

**Почему реальный SEFER-потребитель корректен:** small segments резервируются с `SEGMENT = 4 MiB` alignment (`src/alloc_core/os.rs:65,152,207` и NUMA counterpart), а carve policy сохраняет требуемую power-of-two alignment. Гарантия существует у потребителя, не возникает из size-class stride.

**Исправление:**

1. во всех перечисленных местах заменить «blocks are aligned» на «class sizes/strides are divisible»;
2. fast-path доказательство сформулировать как две независимые части: `align` делит `min_block`, а caller отдельно гарантирует aligned carve base;
3. заменить «cannot cause unsafety» на: «cannot cause UB inside this safe arithmetic crate, but an allocator that violates the precondition can return a misaligned pointer and violate its own safety contract»;
4. сделать grep по всем `aligned`, `naturally`, `every block`, `sound` в crate docs/README/CHANGELOG/comments, чтобы исправление не оставило ещё один локальный старый тезис.

После этой синхронизации исходный P2 прогона 3 можно считать полностью закрытым.

### P3-1 — публичный raw-LUT accessor не повторяет top-sentinel/clamp контракт

**Где:** `crates/size-classes/src/lib.rs:535-539`.

`SizeClasses::size2class()` документирован одной строкой: «The derived O(1) size→class lookup». Подробный контракт `build_size2class` на `:348-379` честно объясняет формулу, clamp и недостижимый для `class_for` top bucket, но accessor на него не ссылается и не предупреждает о необходимости предварительно проверить `need <= small_max`.

Для таблицы из `SizeClasses::build` `small_max` кратен `min_block`. Если потребитель возьмёт публичный LUT и напрямую применит опубликованную формулу к `size = small_max + 1`, он попадёт в bucket `L - 1` и получит индекс последнего класса — ложный «fits» вместо `None`. Сам `class_for` безопасен: он делает early rejection до LUT.

**Рекомендация:** расширить rustdoc accessor-а явным «low-level; do not classify without the `small_max` guard», дать ссылку на `build_size2class` и формулу либо не экспонировать raw LUT, если внешнему API он не нужен. Это не ошибка `class_for`, но текущая краткая документация облегчает неправильное использование публичного low-level building block.

### P4-1 — рекомендация `static` применена не ко всему собственному корпусу

Исправление `d2fa219` правильно перевело README, benchmark и mirrored example. Однако:

- `CHANGELOG.md:51` всё ещё рекомендует bake into a ``const`/`static``;
- canonical `SEFER_SC` остаётся `const` в `tests/builder.rs:84`;
- property fixtures `A_SC`, `B_SC`, `C_SC` остаются `const` в `tests/proptest_builder.rs:72,83,95` и используются многократно;
- несколько локальных маленьких test schemes тоже `const` — для действительно малых значений это приемлемо и менять их необязательно.

Это не production-баг. Но canonical/repeated fixtures разумно перевести на `static`, чтобы собственный корпус dogfood-ил заявленную модель хранения и debug/property runs не зависели от optimizer promotion большого const temporary. В CHANGELOG стоит оставить только `static` как рекомендуемое размещение готового `SizeClasses`, сохранив `const fn build` как способ compile-time construction.

### P4-2 — «regardless of usize width» шире фактической семантики

**Где:** `src/lib.rs:276-301`.

На существующих целях с `usize <= 64` `(cur as u128) * (num as u128)` всегда представимо, а checked operations делают округление полностью защищённым. Но комментарий говорит, что решение держится «regardless of `usize` width». На гипотетической 128-bit pointer target `cur * num` может переполнить `u128`, хотя quotient после деления помещается в `usize`; `checked_mul` тогда повторит старую семантическую проблему, только шириной выше.

Это не дефект поддерживаемых сегодня target-ов и не блокер. Либо ограничить формулировку фактическим `usize <= 64`, либо когда появится реальная 128-bit цель реализовать overflow-free multiply/divide (с reduction/quotient-remainder), сохраняющий representable quotient без промежуточного wide product.

## Общий обзор production-кода

### Builder и числовые границы

- `size2class_len` валидирует power-of-two `min_block` и защищает `+1` от wrap.
- `build_table` проверяет ненулевые denominator/geo count, точный `N`, shape extras, merged monotonicity и каждый потенциально опасный арифметический переход.
- Sorted merge корректно принимает extras до, внутри и после geometric run и громко отклоняет duplicate geometric/extra.
- `u128` widening сохраняет representable result на текущих targets, а checked range-downcast не допускает silently wrapped `usize`.
- `growth.0 == 0` и вообще ratios, не дающие роста, намеренно переходят на minimum `min_block` step; это документировано и тестируется.

### LUT и classifier

- Monotone-pointer builder имеет `O(buckets + classes)` и точный предел 256 классов для индексов `u8` `0..=255`.
- Hand-built tables защищены strict-monotonicity check; невозможные через normal builder формы описаны отдельно.
- `class_for` fast path — одна LUT access; slow path корректно прыгает к следующему потенциально делимому stride и защищён от wrap при вычислении next multiple.
- Jump эквивалентен step-by-step scan относительно документированного stride predicate.
- `SizeClasses` не `Copy`, immutable после build, поля private, все query methods берут `&self`.
- `huge_threshold` остаётся чистой caller policy, не смешанной с classifier range.

### Производительность

Новых возможностей ускорения production hot path не найдено:

- fast path уже минимален;
- bitmask вместо division оправдан для power-of-two alignment;
- jump path избегает линейного прохода по runs неподходящих классов;
- checked `u128` операции находятся в builder, а не в per-allocation query;
- готовая схема теперь правильно рекомендуется как `static`.

Benchmark после прошлых исправлений использует реальные jump-activating пары и отдельные small-max/huge-threshold paths. Wall-clock результаты не являются CI hard gate; список fixed iterations синхронизирован со всеми девятью benchmark rows.

## Тестовые оракулы и CI — статическая оценка

Набор оракулов сильный:

- hand-derived golden geometric sequence закрывает circularity reference builder-а;
- full SEFER size×power-of-two-align sweep сравнивается с независимым scan predicate;
- standalone LUT проверяется по каждому bucket;
- proptest повторяет jump-vs-walk-vs-scan на трёх разных schemes;
- negative tests закрепляют точные panic substrings;
- отдельно покрыты overflow, top bucket, next-multiple wrap, 256/257 class boundary, interleaving extras и debug-only non-pow2 precondition;
- benchmark activation защищён отдельным тестом;
- README example зеркалирован компилируемым integration test.

После P2-1 адресный тест внутри существующего API невозможен — `class_for` не принимает base. Достаточный regression artifact здесь документальный source guard нежелателен; лучше consumer integration test, который проверяет реальную формулу адресов/segment base для всех обслуживаемых alignment, плюс точные API docs.

CI статически содержит debug и release tests, `no_std` bare-metal build, all-targets Clippy с `-D warnings`, rustdoc с `-D warnings`, MSRV compile of tests/dev-deps и package dry-run. Safe-only arithmetic crate не требует Miri/loom/TSan.

## Публикационный чек-лист

1. **Обязательно:** закрыть все противоречия P2-1 и явно назвать downstream allocator safety consequence.
2. Желательно: усилить контракт публичного `size2class()` accessor-а.
3. Желательно: довести `static` guidance до CHANGELOG и canonical repeated test fixtures.
4. Опционально: сузить future-width claim widened arithmetic.
5. После правок прогнать обычные проектные gates; этот аудит намеренно их не запускал.

## Итог

Реализация алгоритма качественная и готова: safe-only, zero-dependency, `no_std`, с хорошими арифметическими guards, компактным query path и сильными оракулами. Последние четыре commit-а не внесли найденных code regressions. Оставшийся `NO-GO` — не новый алгоритмический дефект, а неполное закрытие критического публичного контракта: один и тот же rustdoc одновременно требует aligned base и обещает alignment «by construction», а затем недооценивает downstream safety consequence. После согласования этих формулировок ожидаемый вердикт — **GO**.
