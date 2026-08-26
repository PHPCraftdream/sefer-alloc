# `racy-ptr-cell` — аудит готовности к публикации, прогон 6

- Время: 2026-08-26 07:53:48 +02:00 (Europe/Berlin)
- Ревьюер: Сол-кодекс (`Sol-codex` в имени файла)
- Ревизия: `f891d59753be84d118bb4a01d306831ec532c083`
- Предыдущий отчёт: `docs/reviews/2026-08-26-070854-racy-ptr-cell-publication-audit-run-5-Sol-codex.md`
- Режим: только статическое чтение, без под-агентов. Не запускались тесты,
  сборка, Clippy, rustdoc, Miri, loom, benchmark, package или publish-команды.

## Вердикт

**GO.** Блокирующих P1/P2-находок в текущем состоянии нет. Предыдущий P2 по
необеспеченной однословной layout-гарантии закрыт корректно через
`#[repr(transparent)]`; production state machine, unsafe/provenance, порядок
атомарной публикации, rollback и публичная lifetime-модель выглядят
корректными.

Обнаружены четыре группы P3. Они не являются основанием откладывать
публикацию: две относятся к точности benchmark/test evidence, две — к
документационной гигиене. Если цель — не просто безопасная публикация, а
максимально строгий доказательный контур, F1–F4 стоит закрыть до финального
тега.

Важно: это вывод статического чтения, а не подтверждение зелёного состояния
CI. Указанные в commit messages результаты чужих запусков прочитаны как
история изменений, но не использованы как собственное проверочное
свидетельство этого прогона.

## Покрытие исследования

Заново прочитаны:

- весь публикуемый crate: manifest, README, changelog, `src/lib.rs`,
  `src/imp.rs`, лицензии и список package-файлов;
- оба integration-test target и benchmark;
- все коммиты после отчёта прогона 5;
- связанный root loom-shim/forwarder;
- package, Miri, native, Clippy, rustdoc, no_std и loom строки CI;
- все unsafe blocks, manual `Send`/`Sync`, `PhantomData`, atomic loads/stores/CAS
  и публичные элементы API.

По требованию аудит проведён одним контекстом без под-агентов. Это полный
предметный проход по маленькому crate, но не независимое многоревьюерное
доказательство. Из областей `rust-intel` отдельно применены unsafe/provenance,
concurrency/state, RAII/drop, API/lifetimes, data/performance,
dependencies/features, testing/CI и semantic conformance. Async, crypto,
network и FFI не имеют в crate соответствующих production-поверхностей и
потому не образуют отдельных аудиторских модулей.

Manifest фиксирует Rust 2021/MSRV 1.88. Normal dependencies отсутствуют;
`loom 0.7` наследуется из workspace и активен только под `cfg(loom)`,
`bench-scale-tool 0.1.0` — dev-only по текущему lockfile.

`git diff --check 0e82550..HEAD` для изменённых crate/CI/shim-файлов не
сообщил ошибок.

## Обзор изменений после прогона 5

### `d6511da` — package gate в обычном CI

Исправление F7 выполнено по существу правильно. Новый
`racy-ptr-cell-gates` запускает `cargo publish --dry-run -p racy-ptr-cell` на
обычных CI-событиях, то есть проверяет manifest normalization, содержимое
tarball и isolated verify build до release workflow. Существующие test,
Clippy и rustdoc строки не дублируются.

Отсутствие semver-check до первой публикации логично: сравнивать пока не с
чем. Саму команду dry-run в этом исследовании я не запускал.

### `db3c79b` — `#[repr(transparent)]` и layout oracle

Предыдущий release blocker закрыт полностью:

- `RacyPtrCell<T>` теперь `#[repr(transparent)]`;
- единственное non-ZST поле — `AtomicPtr<T>`;
- `PhantomData<*mut T>` имеет нулевой размер и alignment 1;
- README, crate docs и changelog явно закрепляют layout-контракт;
- root loom-shim зеркалирует representation;
- integration test фиксирует equality size/alignment с `AtomicPtr`.

Representation теперь гарантирует совпадение layout/ABI с полем, а не
полагается на текущий алгоритм `repr(Rust)`. См. [Rust Reference: Type
Layout](https://doc.rust-lang.org/reference/type-layout.html).

### `2745cd5` — документация `RollbackProbe::NotApplicable`

Enum-level docs теперь правильно различают два случая:

1. entry CAS увидел чужое состояние — probe ничего не трогал;
2. настоящий caller выиграл rollback/re-CAS окно — probe не трогает нового
   владельца, но итоговое состояние уже не обязано совпадать с entry-state.

Production-код не менялся и остаётся корректным. Один прежний обобщённый
оборот сохранился в changelog — F3.

### `714393a` — mode-gate и shutdown benchmark workers

Исправлена исходная ошибка: во время `baseline/barriers_only` workers больше
не вызывают warm `get_or_try_init` на оставшейся cell. Mode публикуется до
barrier, workers читают его после barrier; Release/Acquire здесь достаточны.
Добавленный `Shutdown` освобождает workers из start barrier и позволяет
join'ить их после `h.run()`.

Однако baseline теперь исключает слишком много scaffolding и потому всё ещё
не поддерживает заявленную интерпретацию разности — F1.

### `98f29ae` — строгий Loom handshake для `get()`

Исправление корректно. Winner устанавливает `in_init` внутри closure и не
может публиковать до сигнала reader. Reader после Acquire-наблюдения строго
требует `get() == None`, затем разрешает publish. После join проверяются
точный pointer, адрес и полностью инициализированный payload.

Это действительно доказывает временной контракт из имени теста, а не прежнее
более слабое «получили None или уже READY». Ordering handshake согласован.

### `46674c0` — native unwind/loser regression test

Добавленный тест полезен: он создаёт настоящий concurrent caller до unwind,
ограничивает failure-path timeout'ом и не превращает регрессию в навечно
зависший test runner. Но handshake сигнализирует только «loser сейчас начнёт
вызов», а не факт наблюдения sentinel внутри loser loop. Поэтому тест и
обновлённый `RollbackGuard` comment утверждают больше, чем доказано, — F2.

### `655154a` — changelog sync

Исправлены `FnMut -> FnOnce`, `#[must_use]`, публичный `RollbackProbe`, wording
`OnceLock` и внутренние task IDs. Основной changelog теперь соответствует
фактическому API. Осталась одна узкая формулировка F3.

### `4381524`, `f891d59`

Это документационные commits о закрытии предыдущего отчёта и session
checkpoints. Production crate ими не менялся.

## Находки

### F1 — P3: contention baseline исключает не только cell call, но и уникальный scaffolding

**Где:** `benches/racy_ptr_cell_bench.rs:7-20`, `:86-125`, `:173-193`,
`:224-258`.

Документация утверждает, что `baseline/*` измеряет harness scaffolding без
cell, а разность `contention/one_cell - baseline/barriers_only` является
стоимостью самого contended cell protocol.

После mode-fix worker в contention-раунде выполняет:

1. start barrier;
2. `slot.lock()`;
3. `Option<Arc<_>>::clone()`;
4. `get_or_try_init`;
5. done barrier.

В baseline worker выполняет только оба barrier. Следовательно разность
включает не только cell protocol, но и три mutex acquisitions плюс три Arc
reference-count increments/decrements на раунд. Benchmark-thread тоже
получает cell отдельным batched input только в contention row.

Исходная baseline была загрязнена warm cell calls; новая baseline загрязнение
убрала, но одновременно убрала необходимый контрольный scaffolding. Фраза
«protocol's own contended cost is genuinely the DIFFERENCE» всё ещё неверна.

**Что исправить:** baseline должен получить свежую/контрольную cell тем же
untimed setup-путём; каждый worker должен lock'нуть slot, клонировать Arc и
передать его в `black_box`, но пропустить только `get_or_try_init`. Главный
поток аналогично должен сохранить форму timed input и заменить только cell
call на `black_box`. Тогда разность изолирует метод значительно честнее.

Если нужен baseline буквально только для barrier, строку можно оставить, но
её следует назвать `barrier_floor` и запретить трактовать subtraction как
чистую стоимость cell.

### F2 — P3: native test не доказывает, что loser уже наблюдал sentinel и spinning

**Где:** `tests/cell_unit.rs:137-231`; `src/imp.rs:141-159`.

Winner ждёт `loser_about_to_call`. Loser после наблюдения `in_init` делает:

```text
loser_about_to_call.store(true, Release);
cell.get_or_try_init(...);
```

Между store и входом в `get_or_try_init` планировщик может переключиться на
winner. Winner увидит flag, panic/unwind выполнит rollback до null, и только
потом loser впервые вызовет метод и сразу выиграет CAS. Тест пройдёт, хотя
loser никогда не проигрывал CAS и не входил в spin loop.

Комментарии прямо признают, что число spin-итераций не гарантировано, но
затем делают вывод из относительной скорости panic и `spin_loop`. Это timing
assumption, а не deterministic oracle; пять удачных повторов из commit
message не превращают её в доказательство.

Текущий unconditional rollback production-кода остаётся правильным. Дефект
только в силе тестового свидетельства.

**Что исправить:** либо переименовать тест/документацию в честное
«concurrent caller started before unwind completes», либо добавить
инструментированный test hook в loser path, который выставляется только после
фактического наблюдения `INITIALIZING`. Интеграционный test не получает
`cfg(test)` library internals, поэтому hook должен быть спроектирован
осознанно: отдельный non-default verification feature, внутренний unit target
или loom-friendly abstraction, а не расширение обычного production API без
необходимости.

### F3 — P3: changelog всё ещё безусловно обещает восстановить наблюдённое состояние

**Где:** `CHANGELOG.md:71-79`, особенно `:73-76`.

Описание `dbg_rollback_reenterable` говорит, что probe «restores observed
state and never clobbers a concurrent real winner». Первая половина верна
только для `RollbackProbe::Proven` либо для entry-CAS failure, когда probe
ничего не менял. При postcondition-CAS race probe вошёл на `UNINIT`, а к
возврату настоящий caller может оставить cell в `INITIALIZING`/`READY`.

Enum docs уже исправлены именно по этой причине, поэтому changelog снова
расходится с ними.

**Что исправить:** написать «restores `UNINIT` when it returns `Proven`; on
`NotApplicable` it never clobbers a concurrent owner».

### F4 — P3: локальная документация test/CI отстала после добавления девятого теста

**Где:** `tests/cell_unit.rs:1-4`; `.github/workflows/ci.yml:1834-1840`.

- module docs называют файл «single-threaded unit tests», хотя в нём теперь
  два multithreaded unwind/liveness теста;
- CI comment по-прежнему говорит о 7 тестах `cell_unit.rs`, тогда как
  статический подсчёт `#[test]` даёт 9.

Команды CI не зависят от числа и запускают весь target, поэтому coverage не
сломано. Это только stale documentation, но она уже однажды устарела по той
же причине и вводит следующего ревьюера в заблуждение.

**Что исправить:** убрать изменчивые числовые counts из workflow comment и
описать файл как native sequential + concurrency/rollback tests.

## Общий аудит production-кода

### State machine

Код реализует три непересекающихся значения:

- null — `UNINIT`;
- address 1 — `INITIALIZING`;
- любой другой non-null address — `READY`.

`new` требует `align_of::<T>() >= 2`, поэтому реальный aligned pointer не
может совпасть с address 1. Safe caller всё ещё способен синтезировать
`NonNull` с address 1; release-active assertion перед publish закрывает этот
caller-bug и через `RollbackGuard` восстанавливает cell при unwind.

Loser spins только при точном sentinel. При null он выходит на re-race, при
READY возвращает опубликованный pointer. OOM одного winner не ошибочно
распространяется на losers как их собственный `None`.

### Atomic ordering

Инвентаризация production writes/operations:

- publish real pointer — `Release` (`src/imp.rs:490`);
- explicit OOM rollback — `Release` (`:508`);
- unwind rollback guard — `Release` (`:192`);
- probe rollback/restore — `Release` (`:653`, `:682`);
- readers, hot path и loser loop — `Acquire`;
- claim/probe CAS success — `Acquire`, failure — `Relaxed`.

Load-bearing visibility pair — Release publish / Acquire read — сохранён.
Relaxed failure CAS безопасен, потому что код немедленно выполняет отдельный
Acquire load. Claim success Acquire сильнее очевидного минимума после пустого
rollback, но не слабее требуемого.

В комментариях правильно отложено возможное ослабление claim/poll ordering до
model counterfactual и измерения на weakly ordered target. Без AArch64/ARM
данных менять это ради теоретической микрооптимизации не следует.

### Unsafe, provenance и auto traits

Все production unsafe occurrences проверены:

- manual `Send` и `Sync` (`src/imp.rs:121`, `:123`);
- четыре `NonNull::new_unchecked` после non-null/non-sentinel predicate;
- sentinel создаётся через `without_provenance_mut` и никогда не
  разыменовывается.

Manual auto traits sound по модели `AtomicPtr<T>`: cell синхронизирует только
своё atomic state, не создаёт `&T`/`&mut T`, не разыменовывает pointee и
возвращает raw capability, безопасное использование которого остаётся
обязанностью caller. `PhantomData<*mut T>` сознательно задаёт инвариантность,
не заявляет ownership/drop и объяснён рядом с impl. `repr(transparent)` не
меняет эти свойства.

У каждого unsafe блока есть локальный `SAFETY` rationale. Strict-provenance
round trip через integer отсутствует. Cell не освобождает и не принимает
владение pointer, поэтому double-free/use-after-free не создаются самой
абстракцией.

### RAII и panic

`RollbackGuard` создаётся только после успешного claim CAS. Он defuse'ится на
обоих нормальных исходах — publish и explicit OOM rollback. При unwind guard
Release-store'ит null. Порядок defuse/store на OOM не оставляет пути двойного
store, а publish defuse выполняется после Release store реального pointer.

Allocator-specific документация правильно говорит, что cell-local rollback
не делает unwind через `GlobalAlloc` sound. Panic/runtime measurements
отделены от нормативного запрета panic.

### Public API и layout

Публичная поверхность мала:

- `RacyPtrCell<T>`;
- `RollbackProbe::{Proven, NotApplicable}`;
- `new`, `get`, `get_or_try_init`, `dbg_is_ready`,
  `dbg_rollback_reenterable`;
- `Default`, unconditional `Send + Sync`.

`get_or_try_init` принимает точный `FnOnce`, результат помечен `must_use`.
Возврат `NonNull<T>`, а не ссылки, не придумывает lifetime pointee. Закрытый
enum соответствует намеренно закрытому множеству ответов; `non_exhaustive`
здесь не требуется. Debug probe явно объявлен стабильным API и не принимает
raw input от caller.

`repr(transparent)` теперь делает обещание one-word формальным. Отсутствие
Drop/reset API соответствует process-static allocator niche и исключает
reclamation race внутри crate.

### Portability, dependencies и package

На target без pointer-width atomics implementation cfg-isolated и остаётся
один намеренный `compile_error!`. На обычном build crate `no_std`, не
аллоцирует и не имеет normal third-party dependencies. Loom cfg является
сознательным verification-контуром; README предупреждает, что глобальный
`--cfg loom` делает `new` non-const и требует shim от workspace consumer.

Manifest metadata, dual license, README, repository/homepage/docs URL,
keywords/categories и changelog присутствуют. Package dry-run теперь есть в
CI, но в рамках read-only запроса его результат не перепроверялся.

Официальная актуальная документация Rust 1.98 по-прежнему отмечает
`OnceLock::get_or_try_init` как nightly-only `once_cell_try`, поэтому текущая
сравнительная формулировка crate не устарела: [std::sync::OnceLock](https://dev-doc.rust-lang.org/stable/std/sync/struct.OnceLock.html).

## Тесты и CI — статическая оценка

Статически присутствуют 9 native tests и 8 Loom tests, из которых 2 —
`#[should_panic]` counterfactuals. Основные свойства имеют прямые oracles:

- exactly-once и same-pointer под 2/3 callers;
- Release/Acquire visibility;
- OOM rollback/re-race;
- get during a provably held sentinel;
- probe vs concurrent winner;
- sentinel collision и align guard;
- panic rollback и retry;
- layout size/alignment.

CI содержит standalone no_std target build, native debug/release, Clippy
all-targets, rustdoc `-D warnings`, Miri plain/strict-provenance, Loom release
с sentinel grep и новый package dry-run. F2 остаётся единственным заметным
oracle-quality пробелом; это не пробел production coverage целиком, потому
что сам unconditional rollback прост и отдельно проверяется последующим
retry.

## Производительность

Production fast path хорош:

- `get` — один Acquire load и две address checks;
- READY-path `get_or_try_init` — такой же load/check и early return;
- claim/init/spin/rollback вынесены в `#[cold] #[inline(never)]` slow path;
- closure не вызывается проигравшим caller и не создаёт warm-path work;
- normal path не аллоцирует и не берёт lock.

Slow path неизбежно generic по `F`, поэтому monomorphized cold copies остаются,
что честно документировано. Это не runtime bottleneck без evidence.

Возможные relaxed polling, weak CAS и backoff требуют модели и архитектурных
измерений. Busy-spin является частью allocator-safe контракта; scheduler yield
или parking нарушили бы нишу. Единственная конкретная perf-рекомендация этого
прогона — сначала исправить F1, чтобы contention numbers вообще можно было
интерпретировать как изолированную стоимость cell.

## Приоритет

1. Публикацию по результатам этого прогона блокировать не нужно.
2. Перед использованием benchmark как performance gate исправить F1.
3. Для строгого доказательного заявления исправить/переименовать F2.
4. Дешёво синхронизировать F3 и F4, чтобы публичные тексты не расходились с
   уже исправленным контрактом.

После этих четырёх P3 новый проход, вероятно, должен быть коротким closure
review: production-механизм сейчас не показывает незакрытых дефектов.
