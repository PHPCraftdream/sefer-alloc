# `racy-ptr-cell` — publication audit, run 2

Дата отчёта: 2026-08-25 16:42:00 +02:00

Автор: Сол-кодекс

Режим: статическое исследование только для чтения, без под-агентов. Тесты, сборки,
`cargo check`, Clippy, Miri, rustdoc, benchmark и package/publish-команды не запускались.

## Вердикт

**Условный NO-GO.** Сам production-протокол выглядит корректным: новой P0/P1-дыры в
ownership, lifetime, sentinel state machine, panic rollback или Release/Acquire-публикации
не найдено. Исправления после прогона 1 действительно закрыли его F1–F3, а последние
изменения корректно убрали `lock-free`, уточнили sentinel collision и добавили
`--all-targets` Clippy gate.

Перед публикацией остаются два контрактных блокера:

1. обещание безопасности внутри `#[global_allocator]` неполно и местами предлагает
   несовместимые между собой `std`/`no_std` меры;
2. сравнение с `OnceLock` фактически неверно: `OnceLock::get_or_try_init` не poison-ит
   ячейку при `Err`, а после panic ячейка остаётся неинициализированной.

Это не требует переделки атомарного алгоритма. После исправления F1–F2 я не вижу
production-причин блокировать публикацию; F3–F9 — реальные улучшения, которые разумно
решить до фиксации первого публичного API, но они не являются найденной UB в самом
примитиве.

## Scope и просмотренные изменения

Повторно с нуля просмотрены:

- `crates/racy-ptr-cell/src/lib.rs`, manifest, README, CHANGELOG и обе лицензии;
- native- и loom-тесты, включая оракулы, reclaim и strict-provenance transport;
- benchmark;
- потребитель и `cfg(loom)`-shim в `src/registry/bootstrap.rs`;
- связанные CI/release-gates;
- новые изменения `7e20a4a`, `9c49337`, `29c2b2f`, `870001e`, `c2a2e78`,
  `d3c9cfd`, `016adb7` и их интеграция в текущий `HEAD`.

`git diff --check` для изменённой области чист. Фактическая исполнимость не проверялась
по требованию режима без тестов и сборок.

## Findings

### F1 — P1 contract/safety — обещание `safe inside #[global_allocator]` не задаёт обязательный no-unwind/no-reentry контракт

**Где:** `crates/racy-ptr-cell/src/lib.rs:33-45, 368-391`,
`crates/racy-ptr-cell/README.md:10-18`, `crates/racy-ptr-cell/Cargo.toml:7`,
`crates/racy-ptr-cell/CHANGELOG.md:14-29`.

После `870001e` обещание ограничено «success paths», что лучше прежнего безусловного
текста, но всё ещё недостаточно для главного заявленного use case:

- winning `init` — часть успешного вызова и является произвольным caller code; он может
  аллоцировать или транзитивно вызвать тот же global allocator. Поэтому формулировка
  «success paths allocate nothing» верна только для внутренних операций cell, но не для
  `get_or_try_init` целиком;
- документация запрещает только прямой вызов той же cell и не требует от `init` быть
  allocation-free / allocator-reentrancy-free;
- rustdoc обещает, что panic из `init` распространяется наружу после rollback. Для
  обычного кода это корректно, но unwind из методов `GlobalAlloc` сейчас является UB;
- совет «`panic = \"abort\"` plus a non-allocating `#[panic_handler]`» смешивает две
  среды: при линковке с `std` panic handler предоставляет `std`; свой `#[panic_handler]`
  требуется для конечного `no_std` binary. Один рецепт не покрывает оба случая;
- «two panic paths» неточно: кроме sentinel assert и caller panic существует runtime
  panic конструктора/`Default` для `align_of::<T>() == 1`.

Официальный контракт `GlobalAlloc` прямо говорит, что unwind глобального аллокатора —
undefined behavior: [Rust `GlobalAlloc` safety](https://doc.rust-lang.org/core/alloc/trait.GlobalAlloc.html#safety).
Разделение panic runtime между `std` и конечным `no_std` artifact описано в
[Rust Reference: panic handlers](https://doc.rust-lang.org/stable/reference/panic.html#the-panic_handler-attribute).

**Что исправить:** явно сузить обещание до «внутренние непанические операции cell не
аллоцируют и не паркуются» и вынести обязательства caller-а в отдельный раздел:

- `init` при вызове из allocator path не должен аллоцировать, прямо или транзитивно
  входить в этот allocator, блокироваться или panic-овать;
- panic не должен unwind-иться через `GlobalAlloc`;
- abort/non-allocating panic strategy должна описываться отдельно для `std` и `no_std`;
- collision должен быть заявлен как нарушенный precondition/fatal bug caller-а, а не как
  штатно восстанавливаемая allocator-ошибка.

Пока публичная документация говорит «safe inside», но не сообщает ключевое safety-
условие главного сценария, публикацию считаю заблокированной.

### F2 — P2 contract/docs — противопоставление `OnceLock` основано на неверной семантике

**Где:** `src/lib.rs:23-25, 43-45, 184-193`, `README.md:16-18`,
`Cargo.toml:7`, `CHANGELOG.md:22-29`.

Крейт многократно утверждает, что `OnceLock` poison-ит/блокирует failed initializer и
что его `get_or_try_init` «can never recover». Это неверно. При `Err` ячейка остаётся
неинициализированной; при panic инициализатора panic передаётся caller-у, а ячейка также
остаётся неинициализированной. Это показано непосредственно в официальном rustdoc:
[std::sync::OnceLock::get_or_try_init](https://doc.rust-lang.org/std/sync/struct.OnceLock.html#method.get_or_try_init).

Реальное отличие `racy-ptr-cell` от `OnceLock` — не recoverability после `Err`, а
`no_std`, отсутствие parking/внутренней аллокации, raw-pointer/lifetime posture и
busy-spin + per-caller `Option` semantics. `core::cell::OnceCell` тоже не poison-ит
failed init, но не является `Sync`.

**Что исправить:** удалить claims про poison/no recovery из всех четырёх поверхностей и
сравнивать честные свойства. Это особенно важно в crates.io description и CHANGELOG:
ложное конкурентное обещание быстро становится архитектурным основанием для клиентов.

### F3 — P2 liveness contract — предупреждение о reentrancy слишком узкое

**Где:** `src/lib.rs:368-376`; README вообще не формулирует это ограничение рядом с
примером.

Запрещён только прямой вход в ту же cell. Возможен обычный цикл из двух cell:
инициализатор A ждёт B, а параллельный инициализатор B ждёт A. Оба потока бесконечно
spin-ят, даже если каждый callback никогда не вызывает собственную cell напрямую. Для
allocator bootstrap это естественная форма зависимости.

**Что исправить:** документировать транзитивный запрет циклов и фиксированный порядок
инициализации нескольких cell, как lock-order graph. Вместе с F1 это должно быть видно
до первого примера использования, а не только глубоко в rustdoc метода.

### F4 — P3 verification — strict-provenance cleanup завершён только наполовину, а `get()` не проверен во время sentinel

**Где:** `tests/loom_racy_ptr_cell.rs:414-530`; coverage публичного `get()` в
`:256-303` происходит уже после READY.

`9c49337` правильно убрал pointer → `usize` → pointer transport из `cell_unit.rs`, но
структурно тот же exposed-provenance detour остался в loom-тесте: worker-ы кладут
`expose_provenance()` в `Vec<usize>`, затем main восстанавливает pointer через
`with_exposed_provenance_mut` для reclaim. Это определено exposed-provenance model, но
не совместимо с crate-wide strict-provenance проверкой и уже не нужно: после join
опубликованный pointer доступен через `cell.get()`; worker-ам достаточно передать
адреса только для сравнения.

Отдельно loom-suite не вызывает `get()` конкурентно с активным initializer. Регрессия,
при которой `get()` ошибочно вернёт sentinel как `Some`, может оставить все текущие
тесты зелёными. Нужен небольшой real-type loom-сценарий reader-vs-initializer,
проверяющий `None` во время sentinel и real pointer после Release publish.

### F5 — P3 integration/unsafe hygiene — исправленный root loom-shim всё ещё является незащищённой второй реализацией

**Где:** `src/registry/bootstrap.rs:225-423`, особенно unsafe blocks на
`:320`, `:334`, `:378`.

`29c2b2f` корректно синхронизировал четыре потерянные гарантии из прогона 1:
alignment assert, panic rollback guard, sentinel-result assert и guarded restore.
Нового поведенческого drift в текущей версии не найдено.

Остались два риска:

- три `NonNull::new_unchecked` не имеют локальных `// SAFETY:` доказательств, хотя
  заголовок файла на `:187` утверждает, что доказательство есть у каждого unsafe block;
- shim использует core atomics и настоящий busy-spin. Если будущий root loom-тест всё
  же достигнет chunk cell, interleaving не будет моделироваться, а тест может зависнуть.
  Запрет сейчас существует только в комментарии.

Минимум — добавить три локальных proofs и fail-loud механическую границу для loom.
Лучшее долгосрочное решение — перестать вручную копировать state machine или вынести
общий protocol core так, чтобы const-адаптер не дублировал алгоритм.

### F6 — P3 API design — перед первой публикацией можно точнее выразить реальные контракты

**Где:** `src/lib.rs:392-395, 517-648, 651-655`.

- `get_or_try_init` вызывает переданную closure максимум один раз за invocation, поэтому
  `FnOnce` точнее и принимает больше корректных consuming closures, чем `FnMut`.
- `dbg_rollback_reenterable() -> Option<bool>` никогда не возвращает `Some(false)`;
  `Option<()>`, enum с именованными исходами или простой `bool` выражают результат без
  недостижимого значения.
- `dbg_is_ready()` полностью дублирует `get().is_some()`. Сохранение обоих `dbg_*` как
  стабильного production API допустимо, но это сознательная постоянная поверхность для
  тестовых probes; окно для упрощения — до первой публикации.
- `Default` может panic для align-1 `T`, но impl не документирует это. Стоит либо убрать
  `Default`, либо явно закрепить panic posture рядом с type-level API.

### F7 — P3 performance/maintainability — лишние Acquire и неверное объяснение ordering

**Где:** `src/lib.rs:392-515`, особенно CAS `:407-417` и spin `:489-510`.

Success ordering CAS `null -> sentinel` установлен в `Acquire`, а комментарий говорит,
что он «pairs with a later winner's Release publish» и что pairing важен для будущих
readers. Atomic acquire не синхронизируется с будущим release и ordering claim-CAS не
задаёт ordering будущих reader loads. Полезная Release/Acquire пара — publish store на
`:458` и reader/loser loads.

Для claim ownership достаточно рассмотреть `Relaxed` success CAS: при rollback нет
payload state, которое новый winner обязан приобрести. Это сначала нужно зафиксировать
в модели/комментарии, затем измерить на слабой архитектуре.

В loser-loop каждый оборот делает `Acquire`. На AArch64 это дороже Relaxed load; можно
spin-ить Relaxed и выполнять один корректно привязанный Acquire load/fence при выходе.
Также большой generic `get_or_try_init<F>` смешивает fast path и cold winner/spin/panic
код. `#[inline]` fast wrapper + `#[cold] #[inline(never)]` slow path уменьшит
monomorphized hot-path footprint. Это profile-gated оптимизация, не correctness fix.

### F8 — P3 benchmark validity — cold benchmark преимущественно измеряет allocator и собственную утечку

**Где:** `benches/racy_ptr_cell_bench.rs:35-50`.

Payload аллоцируется и leak-ится внутри timed closure на каждой итерации. Поэтому
результат смешивает три atomic operations с `Box::new`, системным allocator и
монотонным ростом heap. Комментарий оценивает только риск исчерпания RAM, но не дрейф и
непригодность числа как cell-regression signal.

Предварительно создать один leaked payload вне timed region и публиковать его в каждой
fresh cell достаточно: cell не владеет pointee и не изменяет его. Затем полезны baseline
без cell, отдельный warm path и bounded real-contention benchmark. Wall-clock не следует
делать жёстким общим CI gate; для регрессии лучше детерминированные counters либо
выделенное стабильное железо.

### F9 — P3 crate hygiene — `no_std` и опубликованная README сформулированы не до конца автономно

**Где:** `src/lib.rs:102`, `README.md:55-96`.

- `#![cfg_attr(not(test), no_std)]` делает headline-свойство условным для lib unit-test
  target без пользы: тесты находятся в `tests/`, а интеграционные targets и так собирают
  библиотеку как dependency без `cfg(test)`. Безусловный `#![no_std]` проще и раньше
  ловит случайный std-only import.
- README объясняет стабильность probes ссылками на root `CLAUDE.md`,
  `tests/dbg_hook_safety_tripwire.rs` и `src/registry/bootstrap.rs`, которые не входят в
  package этого крейта. Следует пометить их как механизмы upstream repository или
  перенести внутреннее обоснование в repo-docs, оставив опубликованной README сам
  пользовательский контракт.
- README-пример не предупреждает об `align_of::<T>() >= 2`, хотя это первое ограничение,
  которое пользователь может встретить как const-eval failure.

## Что в реализации сделано хорошо

- Sentinel создаётся через `without_provenance_mut` и никогда не разыменовывается.
- Release-active collision assert закрывает достижимый из safe code адрес `1`.
- `RollbackGuard` корректно очищает sentinel при unwind; explicit `None` rollback не
  полагается на неявный Drop.
- Loser re-race после null корректен и не ждёт READY, который может не появиться.
- Release publish и Acquire observation дают нужный happens-before для pointee writes.
- Unconditional `Send + Sync` обоснованы моделью `AtomicPtr`: crate не создаёт `&T` и
  не разыменовывает pointee; unsafe access остаётся обязанностью caller-а.
- `dbg_rollback_reenterable` восстанавливает null только после собственного повторного
  выигрыша CAS и не clobber-ит concurrent owner.
- Реальный тип моделируется loom-атомиками; counterfactuals проверяют невакуозность двух
  главных protocol claims.
- Normal dependency surface минимальна; no-std cross-build, debug/release tests,
  all-targets Clippy и отдельный loom job статически присутствуют в CI.

## Проверка последних исправлений

| Изменение | Результат статической перепроверки |
|---|---|
| `7e20a4a` bounded-spin docs | Закрывает run-1 F3; F3 этого отчёта уточняет транзитивный deadlock |
| `9c49337` unit-test provenance | Закрывает run-1 F1; аналог остался в loom-тесте (F4) |
| `29c2b2f` root loom-shim fidelity | Закрывает run-1 F2; residual hygiene/duplication описаны в F5 |
| `870001e` allocator/reentrancy wording | Частичное улучшение, но safety contract всё ещё неполон (F1) |
| `c2a2e78` удаление `lock-free` | Корректно |
| `d3c9cfd` Clippy `--all-targets` | Корректно; bench теперь хотя бы компилируется в статическом gate |
| `016adb7` sentinel docs | Корректно различает aligned pointer и safe-синтезированный address 1 |

## Рекомендуемый порядок действий

1. До публикации исправить F1 и F2 во всех четырёх публичных поверхностях.
2. Явно документировать transitive cell-order/reentrancy contract (F3).
3. Закрыть verification debt F4 и unsafe-hygiene часть F5.
4. До замораживания первого API решить `FnOnce`, форму `dbg_*` probes и `Default` (F6).
5. Отдельным perf-проходом исправить benchmark, затем измерить F7 на слабой архитектуре.
6. Упростить unconditional `no_std` и сделать README автономной (F9).

После пунктов 1–2 достаточно повторного статического review контракта; изменения
атомарного protocol для выдачи GO по найденным блокерам не требуются.
