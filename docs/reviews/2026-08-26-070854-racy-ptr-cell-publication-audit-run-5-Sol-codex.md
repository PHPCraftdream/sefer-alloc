# `racy-ptr-cell` — аудит готовности к публикации, прогон 5

- Время: 2026-08-26 07:08:54 +02:00 (Europe/Berlin)
- Ревьюер: Сол-кодекс (`Sol-codex` в имени файла)
- Ревизия: `782b6fa97e855ca4ae419500d77616f28db98af7`
- Предыдущий прогон: `docs/reviews/2026-08-26-000739-racy-ptr-cell-publication-audit-run-4-Sol-codex.md`
- Режим: только статическое чтение, без под-агентов. Тесты, сборка, Clippy,
  rustdoc, Miri, loom, benchmark, `cargo package` и publish/dry-run не запускались.

## Вердикт

**NO-GO до исправления F1.** Production-алгоритм после последних изменений
выглядит корректным: новых дефектов памяти, provenance, атомарной публикации,
rollback или владения closure не найдено. Прежний P1 по `fork()` закрыт
правильно. Однако crate публично и неоднократно обещает, что ячейка занимает
одно машинное слово, а тип оставлен с `repr(Rust)`. Это обещание сейчас не
является гарантией Rust layout и потому не должно уходить в первый публичный
контракт в таком виде.

После добавления структурной layout-гарантии и проверки её тестом новых
release-blocker'ов по результатам этого статического прогона я не вижу.
Остальные находки имеют уровень P3: они не показывают unsoundness production
пути, но ослабляют точность публичной документации, тестовых доказательств и
benchmark-данных.

## Область исследования

Полностью прочитаны `Cargo.toml`, `README.md`, `CHANGELOG.md`, `src/lib.rs`,
`src/imp.rs`, оба test target и benchmark. Проверены изменения от commit
предыдущего ревью `d31d0b4` до текущего `HEAD`, связанные root-shim/forwarder,
CI-строки для crate и release workflow. Отдельно инвентаризированы все unsafe,
atomic load/store/CAS и публичные элементы API.

Аудит выполнен одним контекстом по прямому требованию без под-агентов. Поэтому
это тщательный, но ограниченный статический обзор, а не независимое
многостороннее доказательство. Async, FFI, crypto/network и production
dependency-код не исследовались как отдельные области: в этом crate таких
поверхностей нет. `loom` и `bench-scale-tool` входят только в cfg/dev-контур;
их внешняя реализация не аудировалась.

`git diff --check d31d0b4..HEAD -- crates/racy-ptr-cell` статически чист.

## Последние изменения

### `3b58bb1` — исправление документации о `fork()`

Исправление корректно. README и crate docs теперь явно разделяют:

- правило POSIX: в дочернем процессе многопоточного процесса до успешного
  `exec()` допустимы только async-signal-safe операции;
- узкий cell-local инвариант: process-wide barrier может только не дать
  унаследовать осиротевший `INITIALIZING`;
- barrier не делает allocator, `RacyPtrCell`, panic/runtime или обычный Rust-код
  безопасными для вызова в ребёнке.

Также точно записано, что exclusive-сторона барьера удерживается через сам
вызов `fork()`. Это полностью закрывает P1 предыдущего прогона и совпадает с
[POSIX `fork()`](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fork.html).

### `4ba0ec4` — docs/hygiene

Исправлены локальные противоречия panic-раздела, wording о `OnceLock` в
основной документации и dead intra-doc link. Основной контракт теперь честно
отделяет нормативный запрет panic внутри allocator-init от измерений одного
panic runtime. В `CHANGELOG.md` осталось одно старое утверждение — F6.

### `e5f018f` — `FnOnce` и `#[must_use]`

Изменение корректно и улучшает API. Closure перемещается только в выигравшую
ветку, вызывается максимум один раз, а проигравший caller возвращается без её
вызова. `FnOnce` точно выражает контракт и разрешает consuming closures.
`#[must_use]` полезен для fallible результата. Root loom-shim обновлён
синхронно.

### `d1379e7` — `RollbackProbe`

Замена трёхсостоянийного `Option<bool>` на закрытый двухвариантный enum
корректна. Реализация по-прежнему восстанавливает `UNINIT` только если повторно
выиграла CAS; при гонке с настоящим winner чужое состояние не затирается.
Root forwarder и тесты обновлены. В документации варианта осталась неточная
фраза — F2.

### `4c70e26` — усиление тестовых оракулов

Добавлены полезные проверки release-active sentinel assertion, align-guard и
обоих ответов rollback probe. Один новый loom-тест назван и описан сильнее,
чем реально доказывает, — F4. Уже явно зафиксированный пробел panic/loser
остаётся — F5.

### `9374078`, `86e9a83` — benchmark и manifest

Вынос `Box::new` из timed cold path и добавление contention-row — правильное
направление. Явный `test = false` делает manifest-намерение прозрачным.
Однако contention baseline фактически всё ещё выполняет cell-вызовы в worker
threads, поэтому заявленная интерпретация разности строк неверна — F3.

### `6bce17e` — cfg-изоляция implementation

Разделение `lib.rs`/`imp.rs` логически корректно: на target без
`target_has_atomic = "ptr"` implementation и re-export не компилируются, а
пользователь получает один намеренный `compile_error!`, а не каскад вторичных
ошибок. На поддерживаемых targets публичная поверхность сохранена.

### `79da4d5` — hot/cold split

Fast path теперь содержит один `Acquire` load и ранний возврат, а медленный
generic path помечен `#[cold]` и `#[inline(never)]`. Перенос не изменил ни
state machine, ни orderings, ни число вызовов closure. Это разумная
оптимизация размера/локальности горячего кода. Её реальный эффект в этом
прогоне не измерялся.

## Находки

### F1 — P2, release blocker: обещание «одно слово» не закреплено layout-контрактом

**Где:** `src/imp.rs:74-90`; `README.md:3-9`, `:163-166`;
`CHANGELOG.md:15-18`; package description в `Cargo.toml:7`.

Документация утверждает, что состояние размещено «over a single
`AtomicPtr<T>`», что cell — «one `AtomicPtr`» и прямо что «cell is one word».
Но `RacyPtrCell<T>` объявлен без `#[repr(transparent)]`:

```rust
pub struct RacyPtrCell<T> {
    ptr: AtomicPtr<T>,
    _marker: PhantomData<*mut T>,
}
```

Для `repr(Rust)` гарантируются лишь минимальные layout-свойства, необходимые
для soundness; порядок полей, padding и стабильное совпадение layout с одним
полем не являются публичным контрактом. Текущий rustc практически наверняка
укладывает этот тип в одно слово, но опубликованная гарантия не должна зависеть
от неоговорённой детали layout. См. [Rust Reference: Type Layout](https://doc.rust-lang.org/reference/type-layout.html).

Это не обнаруженный runtime bug на текущем компиляторе. Это дефект будущего
публичного контракта: размер особенно важен для allocator metadata и массивов
ячеек, а после публикации потребители вправе на него рассчитывать.

**Что исправить:**

1. Добавить `#[repr(transparent)]` к `RacyPtrCell<T>`. Единственное
   non-zero-sized поле — `AtomicPtr<T>`; `PhantomData` этому representation не
   мешает.
2. Добавить compile-time или обычный unit/integration oracle, фиксирующий
   `size_of::<RacyPtrCell<AlignedT>>() == size_of::<AtomicPtr<AlignedT>>()` и
   совпадение alignment. Он защищает не саму гарантию `repr(transparent)`, а
   случайную последующую смену representation/полей.
3. Синхронно зеркалировать representation в root loom-shim, чтобы тестовый
   stand-in не расходился с production-формой типа.

Альтернатива — удалить все обещания физического размера и говорить только об
одном атомарном state field. Для первого релиза и заявленной allocator-ниши
`repr(transparent)` предпочтительнее: дёшево, точно и без компромисса.

### F2 — P3: `RollbackProbe::NotApplicable` обещает невозможное сохранение состояния

**Где:** `src/imp.rs:65-70`.

Документация варианта объединяет два случая и завершает их фразой: «In both
cases the probe leaves the cell exactly as it found it». Для первого случая
(cell уже READY/INITIALIZING на entry CAS) это верно: probe ничего не меняет.
Для второго случая это буквально неверно: probe вошёл при `UNINIT`, сделал
rollback в `UNINIT`, а настоящий caller выиграл окно; к возврату cell может
быть `INITIALIZING` или `READY`. Probe правильно не трогает состояние
конкурентного владельца, но оно уже не обязано совпадать с entry-state.

**Что исправить:** написать: probe либо вообще не изменяет увиденное чужое
состояние, либо при проигрыше postcondition CAS оставляет состояние
конкурентного caller нетронутым. Method-level docs ниже уже описывают это
корректнее.

### F3 — P3: `baseline/barriers_only` выполняет cell protocol в worker threads

**Где:** `benches/racy_ptr_cell_bench.rs:18`, `:151-170`, `:172-222`.

Документация benchmark утверждает, что baseline измеряет тот же barrier pair
«with no cell at all». Но worker threads всегда выполняют бесконечный
`c.round(mine)`. После start barrier `round` берёт `slot` mutex, клонирует
последний `Arc<RacyPtrCell<_>>` и вызывает `get_or_try_init`. Во время baseline
benchmark-thread сам cell не трогает, но три worker'а продолжают проходить
Mutex + Arc clone + warm cell Acquire по cell, оставшейся от последнего
contention round.

Следовательно baseline — не `barriers_only`, а разность строк нельзя описывать
как чистую стоимость cell protocol поверх идентичного scaffolding. Она всё ещё
может быть полезным приближением инкрементальной cold-contention работы, но
опубликованный смысл измерения неверен.

**Что исправить:** передавать worker'ам явный режим раунда (`Contend` /
`BarriersOnly` / `Shutdown`) либо использовать отдельную barrier cohort для
baseline. В baseline worker должен выполнять ровно выбранный контрольный
scaffolding без cell call. Заодно `Shutdown` позволит завершить и join'ить
threads вместо намеренного вечного ожидания до process exit.

### F4 — P3: loom-тест не доказывает временное утверждение из своего имени

**Где:** `tests/loom_racy_ptr_cell.rs:395-465`.

`real_get_returns_none_while_a_winner_holds_the_sentinel` объявляет, что reader
вернёт `None` именно пока winner держит sentinel. Однако reader загружает
`in_init` в `_init_started` и игнорирует значение, после чего принимает как
`None`, так и настоящий READY pointer. Winner после `in_init.store(true)` не
ждёт reader и сразу возвращает payload. Поэтому тест хорошо проверяет более
слабое свойство «`get()` никогда не выдаёт null/sentinel как `Some`, а READY
pointer полностью опубликован», но не заявленную временную постусловную связь.

**Что исправить:** сделать двухсторонний handshake. Winner после установки
`in_init` удерживает closure до сигнала reader; reader ждёт `in_init`, вызывает
`get()` и строго проверяет `None`, затем разрешает winner публиковать. После
join отдельная проверка должна требовать точный READY pointer. Либо честно
переименовать и переписать docs теста под уже доказанное более слабое свойство.

### F5 — P3: не закрыт уже признанный liveness-oracle для unwind и ожидающего loser

**Где:** `src/imp.rs:129-144`; существующий native тест в
`tests/cell_unit.rs`.

Комментарий implementation сам точно фиксирует пробел: тест доказывает лишь,
что последующий caller может инициализировать cell после unwind. Он не
проверяет loser, который уже spinning в момент panic winner. Текущий
безусловный Release rollback делает реализацию корректной, поэтому это не
известный дефект. Но тест пропустит будущую условную регрессию rollback именно
на наиболее опасном liveness-path.

**Что исправить:** native handshake с `catch_unwind`: winner сообщает о входе
в closure и ждёт запуска loser, затем паникует; loser обязан завершиться через
bounded channel/timeout и успешно повторно выиграть CAS. Failure-path нельзя
безусловно join'ить, чтобы дефект сообщался timeout'ом, а не зависшим suite.

### F6 — P3: `CHANGELOG.md` отстал от исправленного API и документации

**Где:** `CHANGELOG.md:7-35`, особенно `:34`.

- Changelog всё ещё говорит, что `OnceLock` «parks the losing threads», хотя
  README/Cargo уже правильно опираются только на документированный контракт
  «may block», не на implementation mechanism.
- Unreleased API summary не называет новый публичный `RollbackProbe`, переход
  `FnMut -> FnOnce` и `#[must_use]`.
- Для первого выпуска changelog содержит много внутренних task/finding IDs,
  полезных репозиторию, но шумных для crates.io-потребителя.

**Что исправить:** синхронизировать механизм/контракт с README и перечислить
фактически публикуемую API-поверхность. Внутреннюю трассировку лучше оставить
в review/open-items, а changelog сделать ориентированным на пользователя.

### F7 — P3: обычный CI не имеет отдельного package/publish gate для crate

**Где:** `.github/workflows/ci.yml:686-705`, `:1802-1844`, `:2274-2308`;
release workflow содержит общий publish path.

CI хорошо покрывает native debug/release tests, Clippy all-targets, rustdoc
warnings, Miri, supported bare-metal target и loom release. Но для
`racy-ptr-cell` в обычном CI нет отдельного `cargo package`/publish dry-run
gate, аналогичного уже существующим package gates других самостоятельных
crates. Ошибку содержимого tarball, manifest normalization или packaged
dependency resolution можно обнаружить только поздно в tag/release path.

**Что улучшить:** добавить до публикационного workflow обычный CI gate,
проверяющий именно упакованный standalone crate и его содержимое. Это не
заменяет release gate, а переносит обнаружение packaging-регрессии ближе к
коммиту.

## Общий обзор production-кода

### State machine и atomics

Состояния однозначны: null = `UNINIT`, адрес `1` = `INITIALIZING`, любой другой
non-null pointer = `READY`. `align_of::<T>() >= 2` исключает законный aligned
pointer по sentinel-адресу; отдельный release-active assertion отвергает
sentinel, который safe closure всё же может искусственно создать через
strict-provenance API.

Все публикационные связи согласованы:

- `Release` publish настоящего pointer в `src/imp.rs:475`;
- `Acquire` reader loads в `get`, fast/slow path и loser loop;
- `Release` rollback при OOM, unwind и probe;
- loser после rollback не возвращает OOM победителя, а выходит из spin и снова
  участвует в CAS, что соответствует заявленному re-race контракту.

CAS claim с success `Acquire` и Acquire polling выглядят сильнее минимально
необходимого, но безопасны. Комментарий честно откладывает ослабление до модели
и измерений на weakly ordered target. В этом прогоне нет оснований менять
ordering ради недоказанной микрооптимизации.

### Panic/rollback

`RollbackGuard` ставится только после выигрыша sentinel и defuse'ится после
обоих нормальных исходов. При unwind он Release-store'ит null. При
sentinel-collision assertion guard также откатывает cell. В allocator-контексте
документация правильно запрещает unwind независимо от того, что cell-local
состояние умеет восстановиться.

### Unsafe и provenance

Unsafe-поверхность мала:

- ручные unconditional `Send`/`Sync` повторяют модель `AtomicPtr<T>`;
- `NonNull::new_unchecked` используется только после проверок
  non-null/non-sentinel;
- crate не разыменовывает, не освобождает и не принимает владение pointee;
- sentinel создаётся через `without_provenance_mut`, не разыменовывается и не
  маскируется под READY;
- raw pointer, возвращённый caller'у, сохраняет исходный provenance publish
  pointer.

Новых soundness-проблем в этих местах не найдено. `PhantomData<*mut T>`
фиксирует инвариантность и сознательно компенсируется ручными auto-trait impl.

### API и lifetime-контракт

API намеренно возвращает `NonNull<T>`, а не ссылку, поэтому не создаёт ложную
lifetime или thread-safety гарантию pointee. `new` const в production и
не-const под loom описан. `Default`, `get`, `get_or_try_init` и debug probe
согласованы. Закрытый `RollbackProbe` уместен: метод действительно имеет ровно
два наблюдаемых ответа. Отсутствие reset/drop API соответствует process-static
allocator niche и исключает reclamation race.

### Производительность

Главный hot path теперь оптимален по форме: один Acquire load, две проверки
адреса и возврат. `get_or_try_init` READY-path не строит slow loop и не вызывает
closure. Инициализация и contention по определению cold; `spin_loop` уместен
для заявленного короткого неблокирующего init, а документация предупреждает о
100% CPU и отсутствии bounded latency.

Реальные кандидаты дальнейшего ускорения — weak CAS, relaxed polling с
финальным Acquire или backoff — зависят от workload/архитектуры и меняют
доказательную поверхность. Без корректного benchmark baseline и измерений на
AArch64/ARM рекомендовать их сейчас нельзя. Сначала следует исправить F3.

## Проверки и публикационная механика

Статически присутствуют:

- 7 native integration tests;
- 8 loom model tests и 2 `#[should_panic]` counterfactuals;
- CI: debug/release, Clippy `--all-targets -D warnings`, rustdoc
  `-D warnings`, Miri плюс strict-provenance mode, loom release и bare-metal
  target с pointer CAS;
- нулевые normal dependencies; `loom` cfg-gated, benchmark dependency dev-only;
- dual license, repository/homepage/documentation/readme/keywords/categories;
- явная portability diagnostic для target без pointer atomics.

Из-за режима этого исследования наличие строк CI не считается доказательством
их текущего зелёного результата. Package tarball также не инспектировался
через Cargo-команды.

## Приоритет исправлений

1. **До публикации:** F1 — `repr(transparent)` плюс layout oracle.
2. **Желательно до публикации:** F2, F3, F4 и F6 — это дешёвые исправления
   публичной точности и доверия к доказательствам/измерениям.
3. **Hardening:** F5 и F7 — закрыть liveness regression oracle и перенести
   packaging failures в обычный CI.

После F1 crate по результатам этого статического исследования можно переводить
в **GO с неблокирующими P3 follow-up**. Для безусловного «совершенства без
компромиссов» логично закрыть все семь пунктов и повторить независимый прогон.
