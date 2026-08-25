# `racy-ptr-cell` — аудит готовности к публикации, прогон 4

- Время: 2026-08-26 00:07:39 +02:00 (Europe/Berlin)
- Ревьюер: Сол-кодекс (`Sol-codex` в имени файла)
- Ревизия: `c3ad909`
- Режим: только статическое чтение; без под-агентов; тесты, сборка, Clippy,
  rustdoc, Miri, loom, бенчмарки и упаковка не запускались.

## Итог

**NO-GO.** Реализация атомарного протокола выглядит корректной, но новая
документация о поведении после `fork()` содержит один блокирующий дефект
контракта уровня **P1**. Она предлагает процесс-wide барьер как достаточное
условие, если дочерний процесс до `exec()` обращается к allocator или этой
ячейке. Барьер способен гарантировать только отсутствие осиротевшего
`INITIALIZING` в `RacyPtrCell`; он не превращает Rust allocator, эту библиотеку
или произвольный код ребёнка в async-signal-safe операции.

До исправления F1 публиковать крейт, позиционируемый для использования внутри
`#[global_allocator]`, не следует. После исправления F1 новых блокеров в самой
машине состояний я не вижу. Оставшиеся пункты — улучшения P3; среди них есть
несколько особенно дешёвых и полезных до фиксации первого публичного API.

## Что изменилось после моего прогона 3

От `a8edef4` до текущего `HEAD` производственный алгоритм, тесты и benchmark не
менялись. В самом крейте изменены только `README.md` и `src/lib.rs`: 128 строк
добавлено, 64 удалено.

- `ea25359` расширил правила `fork()` и попытался закрыть прежнюю неполноту
  «достаточно, чтобы вызывающий поток сам не держал cell». Процесс-wide
  инвариант сформулирован лучше, но добавленная первая ветка создала F1 ниже.
- `e0d63f0` честно описал diagnostics на targets без pointer-width CAS:
  `compile_error!` является первым, но не единственным сообщением. Решение
  корректно; блокирующего расхождения больше нет.
- `8fb67d8` отделил обязательный запрет panic в allocator-init от измерений
  конкретного panic runtime. Основное противоречие устранено; остались только
  редакционные неточности из F2.

`git diff --check a8edef4..HEAD -- crates/racy-ptr-cell` не сообщил ошибок.

## Находки

### F1 — P1, release blocker: барьер не разрешает allocator/cell после `fork()`

**Где:** `crates/racy-ptr-cell/src/lib.rs:180-199`, особенно `:190-194`;
зеркало в `crates/racy-ptr-cell/README.md:73-92`.

Новый текст делит сценарии на две «дисциплины». Первая утверждает, что если
ребёнок касается allocator или cell до `exec()`, достаточно сериализовать
`fork()` со всеми инициализаторами через process-wide barrier. Это неверно для
заявленного общего контракта.

POSIX.1-2024 требует, чтобы ребёнок многопоточного процесса после `fork()` и до
успешного `exec()` выполнял только async-signal-safe операции. Он наследует
единственный вызывающий поток и копию адресного пространства, включая состояния
mutex и иных ресурсов. См. [POSIX `fork()`](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fork.html).
Кроме того, POSIX явно исходит из правила, что функция не async-signal-safe,
если это отдельно не специфицировано: [определение async-signal-safe](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap03.html).

Предложенный барьер доказывает лишь локальный инвариант этого протокола:
в момент fork ни один участвующий `RacyPtrCell` не хранит sentinel. Он не
устраняет унаследованные блокировки allocator/runtime, не делает вызов
`get_or_try_init`, closure, panic/error path или Rust-код async-signal-safe и не
покрывает другие ресурсы процесса. Следование опубликованному совету способно
дать deadlock либо undefined behavior уже вне этой ячейки.

**Что исправить:**

1. Удалить первую ветку как поддерживаемую POSIX-дисциплину.
2. Для многопоточного POSIX-процесса оставить единственный общий совет:
   после `fork()` ребёнок выполняет только async-signal-safe операции до
   успешного `exec()`, а при его ошибке завершает процесс async-signal-safe
   путём вроде `_exit`.
3. Если хочется сохранить описание барьера, ограничить его роль буквально:
   он предотвращает только снимок `RacyPtrCell` с осиротевшим sentinel и сам
   по себе **не разрешает** allocator/cell/Rust runtime в ребёнке. Любая более
   широкая поддержка возможна лишь как отдельный environment-specific контракт
   с полностью доказанным `atfork`-протоколом для всех затронутых ресурсов.
4. Заодно записать протокол барьера недвусмысленно: каждый initializer держит
   shared-сторону на всём протяжении `init`; forking thread берёт exclusive,
   тем самым ждёт quiescence и запрещает новые init, вызывает `fork()` не
   отпуская barrier и отпускает его после возврата. Нынешнее «acquired before,
   held until no cell is INITIALIZING» допускает ошибочное чтение с окном между
   проверкой quiescence и самим `fork()`.

### F2 — P3, документация panic-path содержит три локальных противоречия

**Где:** `src/lib.rs:125-126`, `:155-161`; аналогичный текст в README.

- `no_std` binary сначала «supplies a non-allocating #[panic_handler]», затем
  говорится «no handler, no allocation». Здесь должно быть «non-allocating
  handler», а не отсутствие handler.
- «cell-consistency guarantee below» в `:157` ссылается на гарантию, описанную
  выше.
- Сразу после отдельного раздела о panic/link environments фраза «The rules
  above are all about what init does» формально неверна: прямо выше обсуждается
  также panic runtime и hook.

Это не меняет нормативное правило «init не должен panic», которое теперь
описано правильно, но снижает доверие к особо чувствительному разделу.

### F3 — P3, публичный API обещает больше mutability, чем использует

**Где:** `src/lib.rs:607-609`; root loom-shim `src/registry/bootstrap.rs`.

`get_or_try_init` принимает `F: FnMut`, хотя в рамках одного вызова closure
может быть вызван не более одного раза. Победитель после `init()` возвращает,
проигравший closure не вызывает. Правильная граница — `FnOnce`; она точнее и
разрешает consuming closures. Если менять, синхронно изменить loom-shim.

Также стоит добавить к fallible-методу `#[must_use]`: проигнорированный `None`
скрывает OOM/rollback. Остальные основные возвращающие методы уже помечены.

### F4 — P3, `dbg_rollback_reenterable` имеет невозможный публичный вариант

**Где:** `src/lib.rs:781-824`, `:842-869`.

Документация честно говорит, что `Some(false)` намеренно недостижим: failure
повторного CAS превращается в `None`, чтобы не трогать чужого winner. Поэтому
`Option<bool>` кодирует три состояния, а контракт использует два. До первого
релиза чище выбрать `bool` (`true` = доказано, `false` = неприменимо) либо
двухвариантный enum. Потребуется синхронно обновить root forwarder, где сейчас
ещё и осталось старое описание достижимого `Some(false)`.

### F5 — P3, есть безопасные кандидаты на ускорение, но их надо мерить

**Где:** `src/lib.rs:607-735`.

- Claim CAS использует success `Acquire`, хотя после rollback нет payload,
  который новому winner требуется acquire. `Relaxed` выглядит достаточным,
  но менять ordering следует только вместе с целевым loom-counterfactual.
- Loser делает `Acquire` на каждой итерации spin. Обычный паттерн — relaxed
  polling и финальный acquire при наблюдении READY; здесь он обязан также
  корректно отличить rollback в null и повторно вступить в гонку. Нужны модель
  и измерение, а не механическая замена.
- Большой generic `get_or_try_init<F>` можно разделить на маленький hot path
  и `#[cold]`/`#[inline(never)]` slow path: это способно уменьшить instruction
  footprint у already-ready вызова. Один лишь generic slow helper всё ещё
  мономорфизируется для каждого `F`; реальное устранение дублей потребует
  осторожного type-erased/non-generic protocol helper и отдельного измерения.

Текущие orderings корректны, лишь потенциально сильнее необходимых. Поэтому
это возможности производительности, не дефекты correctness.

### F6 — P3, benchmark не измеряет главный заявленный профиль

**Где:** `benches/racy_ptr_cell_bench.rs:28-84`.

Cold benchmark создаёт и навсегда теряет `Box` внутри измеряемой closure, поэтому
его результат в основном отражает системный allocator и утечку, а не
`null -> sentinel -> ready`. Подготовленный заранее стабильный пул payload-ов
или отдельный non-allocating источник указателей дал бы полезный сигнал.

Contention/loser-spin benchmark отсутствует; файл это честно признаёт. Именно
там находятся busy-wait, cache-line bouncing и возможная цена Acquire polling.
Для заявлений об ускорении нужен отдельный многопоточный harness с контролем
длительности init и числа contenders.

### F7 — P3, тестовые оракулы сильные, но остаются точечные пробелы

**Где:** `tests/cell_unit.rs:64-126`;
`tests/loom_racy_ptr_cell.rs:413-531` и весь real-type набор.

- Нет прямого real-type loom-оракула: `get()` обязан вернуть `None`, пока
  другой поток реально держит sentinel. Существующие exactly-once сценарии
  косвенно ловят многие поломки, но публичный контракт `get()` лучше закрепить
  прямо.
- Property 6 переносит опубликованные указатели через integer address с
  `expose_provenance`/`with_exposed_provenance_mut` только ради reclaim.
  После join проще получить единственный published pointer через `cell.get()`
  и освободить его один раз, не создавая лишний provenance-сценарий в оракуле.
- Native panic-rollback test сохраняет `JoinHandle` в `_handle` и ждёт канал,
  но не делает `join`. Канал доказывает завершение существенного участка, но
  join делает lifetime/паники worker-а явными и закрывает тест аккуратнее.

При этом real-type loom suite действительно покрывает 2/3-thread exactly-once,
OOM rollback/re-race, fast re-entry и probe-vs-winner clobber. Counterfactuals
для Relaxed publish и неправильного spin-condition делают две ключевые проверки
невакуумными. Это сильная сторона релизного состояния.

### F8 — P3, документация и maintainability

- `src/lib.rs:625-628` содержит протухшую ссылку `OOM at :544` (фактически
  store сейчас в `:698`) и ошибочное «the only prior Release stores»: probe
  также пишет null с Release в `:838` и `:867`. Следует ссылаться на операции
  семантически, без номеров строк.
- Cargo description и вводный текст обещают, что losers `OnceLock` «are
  parked». Стандартный контракт обещает blocking, но не конкретный механизм.
  Устойчивее: «OnceLock may block losing threads; this crate busy-spins and
  uses no OS synchronization primitive».
- `#![cfg_attr(not(test), no_std)]` лучше заменить на безусловный `#![no_std]`:
  библиотечный исходник не содержит test-only `std`, а интеграционные тесты от
  этого не страдают.
- Unsafe-header в `src/lib.rs:248-263` одновременно говорит о «SINGLE reason»
  и о двух видах unsafe. Фактический inventory верен, framing — нет.
- Type docs всё ещё цитируют старое «safe inside a #[global_allocator]», хотя
  остальная документация осознанно использует более точное «usable inside».
- Команда запуска loom в module docs теста не содержит `-p racy-ptr-cell`, хотя
  README требует всегда ограничивать глобальный `RUSTFLAGS=--cfg loom` этим
  пакетом.
- Публичные README/rustdoc перегружены внутренними `task #...`, review findings,
  `CLAUDE.md` и путями root-repository. Эти следы полезны истории разработки,
  но ухудшают самостоятельность crates.io-документации.
- Unsupported-target UX можно сделать чище, cfg-изолировав тело реализации:
  тогда пользователь увидит только целевой `compile_error!`, без каскада
  E0599/E0432. Текущий текст теперь честен, поэтому это не correctness issue.

## Полный статический разбор реализации

### Машина состояний и все записи

Состояние кодируется одним `AtomicPtr<T>`:

- null — `UNINIT`;
- адрес `1` — `INITIALIZING`;
- любой другой ненулевой адрес — `READY`.

Полный inventory production writes:

1. claim CAS `null -> sentinel` (`:622-639`);
2. publish `sentinel -> real`, `Release` (`:680`);
3. OOM rollback `sentinel -> null`, `Release` (`:698`);
4. unwind rollback guard, `Release` (`:439`);
5. probe writes (`:827-867`), причём финальный restore выполняется только если
   probe сам снова выиграл CAS и поэтому не может затереть чужого winner.

READY терминален: обычный API не имеет записи real-pointer -> другое состояние.
Каждый возврат опубликованного pointer доминирован проверкой non-null и
non-sentinel. Publish Release синхронизируется с reader/loser Acquire, поэтому
полностью построенный pointee видим после публикации.

OOM и unwind возвращают null. Loser spin продолжается только при sentinel,
выходит на null и повторяет CAS; поэтому rollback не превращается в ожидание
READY, который никто не обязан опубликовать. Транзитивный deadlock нескольких
cells и безграничность spin-wait документированы.

### Sentinel и provenance

`align_of::<T>() >= 2` исключает адрес `1` для валидного `T`; проверка активна
в release. Sentinel создаётся `without_provenance_mut`, только сравнивается по
адресу и никогда не разыменовывается. Result closure дополнительно проходит
release-active проверку, поэтому безопасно сконструированный `NonNull` с
адресом sentinel не может быть опубликован как READY.

### Unsafe и thread-safety

В production-файле два класса unsafe:

- ручные `Send`/`Sync`;
- `NonNull::new_unchecked` после проверок состояния.

Обоснование manual impl соответствует семантике `AtomicPtr`: cell хранит и
возвращает pointer, но не разыменовывает его и не создаёт `&T`. Безопасность
доступа к pointee остаётся обязанностью caller-а, который в любом случае должен
обосновать raw-pointer dereference. `PhantomData<*mut T>` сохраняет инвариантность
по `T`; ручные impl явно снимают его auto-trait effect только для wrapper-а.

Каждый `new_unchecked` находится после проверки `is_ready` либо эквивалентных
проверок null/sentinel. Недокументированного dereference или deallocation в
production-коде нет.

### Drop и владение

Cell намеренно не владеет pointee и не освобождает его. Это соответствует
процесс-`'static` allocator use case, но потребитель обязан понимать утечку/
внешнее владение. `RollbackGuard` владеет только правом вернуть sentinel в null;
он armed до вызова user closure и defused перед явным OOM rollback либо после
успешного publish. Между publish и defuse нет panic site, поэтому guard не
может откатить уже опубликованный pointer на штатном unwind-path.

### Интеграция и CI, проверенные чтением

Обычная сборка не имеет normal dependencies; `loom` подключён только под
`cfg(loom)`, bench harness — dev dependency. CI-конфигурация содержит:

- debug и release tests крейта;
- Clippy `--all-targets -D warnings` и rustdoc `-D warnings`;
- Miri, включая strict provenance;
- bare-metal target с pointer-width atomics;
- отдельный release loom-run реального типа и grep ключевого probe-clobber
  теста.

Эти команды в данном ревью **не запускались**; наличие и смысл проверены только
по файлам workflow. Root `loom_shim` сопоставлен с production protocol:
orderings, guard, rollback/re-race, sentinel assert и conditional probe restore
не разошлись. В root forwarder осталась документальная рассинхронизация
`Some(false)`, отмеченная в F4.

## Рекомендуемый порядок

1. Перед публикацией исправить F1 одновременно в rustdoc и README.
2. Повторно проверить весь fork/signal раздел как единый нормативный контракт.
3. До стабилизации API рассмотреть `FnOnce`, `#[must_use]` и двухсостоянийный
   результат probe (F3-F4).
4. Одним docs/hygiene изменением закрыть F2 и F8.
5. Затем улучшить benchmark/oracles и только после измерений экспериментировать
   с orderings/cold split.

## Границы уверенности

Это намеренно single-context исследование: пользователь запретил под-агентов.
Я прочитал весь крейт, его тесты, benchmark, manifest/changelog/README, новые
commits, релевантные CI rows, root consumer и loom-shim. Async/await, crypto,
сетевой ввод, сериализация и внешний FFI в крейте отсутствуют, поэтому
специализированные проверки этих классов неприменимы. Динамическое подтверждение
не выполнялось по прямому запрету; вывод о GO/NO-GO основан исключительно на
статическом анализе.
