# `racy-ptr-cell` — publication audit, run 3

Дата отчёта: 2026-08-25 23:03:10 +02:00

Автор: Сол-кодекс

Режим: новое статическое исследование в одном агенте, без под-агентов. Исходники и
история просматривались только для чтения; изменён только этот отчёт. Тесты, сборки,
`cargo check`, Clippy, Miri, rustdoc, benchmark и package/publish-команды не запускались.

Исследован `HEAD` `4a7189a`; отчёт прогона 2 использован только после независимого
повторного вывода текущих инвариантов.

## Вердикт

**Условный NO-GO.** Сам production-протокол готов к публикации: новой P0/P1-проблемы
в unsafe, lifetime/variance, state machine, panic rollback, Release/Acquire-публикации,
OOM re-race или конкурентном test-probe не найдено. Последние исправления закрыли
блокеры прогона 2 и заметно усилили документацию и CI.

Перед публикацией я бы исправил три P2 в публичном контракте:

1. рекомендация для `fork()` не задаёт необходимое глобальное состояние покоя и не
   запрещает доступ к cell/allocator между `fork()` и успешным `exec()`;
2. `compile_error!` объявлен заменой последующих E0599/E0432, хотя реализация не
   `cfg`-изолирована и компилятор продолжает выдавать эти diagnostics;
3. таблица panic allocation смешивает измерение одного toolchain с контрактом и
   противоречит собственному рецепту с немедленно abort-ящим non-allocating hook.

Это блокеры документации опасного allocator-facing API, а не дефекты атомарного
алгоритма. После F1–F3 оснований удерживать публикацию по correctness не вижу.
F4–F9 — улучшения до замораживания первого публичного API и performance baseline.

## Scope и последние изменения

С нуля просмотрены:

- `crates/racy-ptr-cell/src/lib.rs`, `Cargo.toml`, README, CHANGELOG и лицензии;
- native- и loom-тесты, их оракулы, pointer transport и reclaim;
- benchmark;
- root consumer и `cfg(loom)`-shim в `src/registry/bootstrap.rs`;
- относящиеся к крейту CI/release gates;
- история и diff после прогона 2, включая `f1e71f1`, `b0e94ed`, `9d967d8`,
  `3ce6a29`, `04c92b3`, `a079f10`, `f89e88e`, `6db9abf`, `ececfe0`,
  `3edc477`, `ecf26fe`, `16213de`, `3861b17`, `a390bfc`, `de7da71`.

Изменения после прогона 2 затронули в основном контракт и verification wiring:
уточнены GlobalAlloc/no-unwind обязанности, исправлено сравнение с `OnceLock`,
документированы transitive cell-order deadlock, fork/signal и `cfg(loom)` hazards,
добавлены Miri и rustdoc jobs, portability guard и подробная panic table. Тесты и
benchmark при этом не менялись. `git diff --check` для исследованного диапазона чист.

## Findings

### F1 — P2 liveness/contract — совет про `fork()` недостаточен и может оставить child с вечным sentinel

**Где:** `crates/racy-ptr-cell/src/lib.rs:151-174`,
`crates/racy-ptr-cell/README.md:62-76`.

Диагноз hazard верный: child наследует адресное пространство, но только calling
thread; sentinel другого потока остаётся без владельца. Однако предложенная мера

> `fork()` only from a thread you know holds no cell and treat `exec()` in the child as mandatory

не закрывает описанный сценарий. Локальное знание о calling thread ничего не говорит
о другом потоке, который в момент `fork()` может держать любую cell. Если child до
`exec()` коснётся этой cell прямо или через global allocator, он будет spin-ить вечно.
Само слово “mandatory” также не говорит, что **до успешного exec нельзя выполнять
никакой операции, способной обратиться к allocator/cell**.

POSIX формулирует более сильное правило: после `fork()` многопоточного процесса child
может выполнять только async-signal-safe операции до успешного exec; child содержит
копию всего address space, включая состояния ресурсов других потоков
([POSIX `fork`](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fork.html)).

**Что исправить:** разделить два допустимых режима:

- если child должен использовать allocator/cell до exec — сериализовать `fork()` со
  всеми инициализаторами и доказать, что ни одна cell не `INITIALIZING` (например,
  глобальная fork barrier/lock-order protocol; одного состояния calling thread мало);
- если child только делает exec — после `fork()` не обращаться к Rust allocator,
  cell и вообще non-async-signal-safe API; вызвать async-signal-safe exec напрямую,
  а при его ошибке завершиться через подходящий async-signal-safe путь.

Фразу “publish every cell before the first fork” тоже лучше заменить на устойчивый
инвариант: fork запрещён одновременно с любым init; иначе новые/сброшенные cell после
первого fork снова делают правило ложным.

### F2 — P2 portability/docs — `compile_error!` не заменяет downstream diagnostics, как обещают rustdoc и README

**Где:** `src/lib.rs:201-215, 237-274`, `README.md:119-131`.

Guard расположен перед остальным crate body:

```rust
#[cfg(not(target_has_atomic = "ptr"))]
compile_error!(...);
```

но imports, `RacyPtrCell`, impl и вызовы `AtomicPtr::compare_exchange` не закрыты
`#[cfg(target_has_atomic = "ptr")]`. Поэтому rustc после explicit diagnostic
продолжает проверять body и выдаёт прежние E0599 на no-CAS targets либо E0432 там,
где `AtomicPtr` отсутствует. Это подтверждает и собственное сообщение коммита
`3861b17`: compile error стал **первым**, после него остаются downstream E0599.

Публичный текст сильнее факта: “fails fast ... rather than the ... errors”. Макрос
действительно гарантирует понятный diagnostic и провал сборки
([официальный `compile_error!`](https://doc.rust-lang.org/stable/core/macro.compile_error.html)),
но не short-circuit дальнейшей компиляции.

**Что исправить:** либо честно написать “emits this explicit diagnostic first; rustc
may report follow-on target errors”, либо сделать обещание истинным: поместить весь
supported implementation в `#[cfg(target_has_atomic = "ptr")] mod ...` и re-export
его только под тем же cfg, оставив на unsupported branch один `compile_error!`.
Второй вариант лучше: чище UX и механически исключает случайное обращение к
недоступному atomic API.

Добавить compile-fail/CI target для самого guard полезно, но это рекомендация; в этом
прогоне target build не выполнялся.

### F3 — P2 GlobalAlloc contract — panic-allocation table выдаёт локальное измерение за общий факт и противоречит mitigation

**Где:** `src/lib.rs:102-149`, `README.md:78-117`.

Строки “Every row that reaches the panic runtime allocates in a `std` build” не
согласуются с той же таблицей и следующим рецептом:

- bare-literal panic измерен как `0` allocations **до non-allocating hook**;
- предлагаемый hook сразу вызывает `std::process::abort` и не возвращается;
- следовательно, на этом пути после hook нет стадии, которая обязательно должна
  выполнить allocation. Безусловное “every row allocates” здесь ложно.

Кроме того, `0`, `>= 2`, `2` и `4` — наблюдения для rustc 1.97,
`x86_64-pc-windows-msvc`, конкретного профиля и hook. Они не являются контрактом
Rust panic runtime на MSRV, других std implementations/targets или будущих toolchains.
Квалификатор “measured” есть в prose, но отсутствует в заголовке/ячейках таблицы,
из-за чего таблица читается как нормативная матрица.

**Что исправить:** отделить гарантируемое от наблюдаемого:

- нормативный контракт: panic/unwind недопустим в allocator path; std panic path
  **может** аллоцировать до hook, особенно при formatting, поэтому на отсутствие
  allocation полагаться нельзя;
- measurement appendix: точная версия toolchain, target, flags, hook, сценарий и
  статус “не API guarantee”; числа не должны становиться safety premise;
- удалить “Every row ... allocates” либо заменить на точное “the default std hook was
  observed to allocate; a custom aborting hook avoided pre-hook allocation only for
  the measured bare-literal cases”.

Главная и уже правильная рекомендация — `init` не должен panic — должна остаться
единственным универсальным safety rule.

### F4 — P3 semver/API — `FnMut` уже, чем фактическая семантика вызова

**Где:** `src/lib.rs:575-577`; тот же signature в root loom-shim.

За один invocation closure вызывается максимум один раз: winner после `init()` всегда
возвращает; повторяется только loser loop, где closure ещё не вызывалась. Значит
`FnOnce() -> Option<NonNull<T>>` точнее и принимает consuming closures, которые текущий
API отвергает. До первой публикации это стоит исправить. Реализация обычно хранит
`Option<F>` и `take()`-ит closure только в winner branch; shim надо менять синхронно.

Также `dbg_rollback_reenterable() -> Option<bool>` имеет недостижимый `Some(false)`.
Именованный enum (`Proven`/`Inconclusive`) или `bool`, если двух исходов достаточно,
лучше выражает contract. `dbg_is_ready()` функционально равен `get().is_some()`;
сохранение обоих допустимо, но это сознательное расширение стабильной поверхности.

### F5 — P3 performance — лишние ordering costs и крупный generic fast path

**Где:** `src/lib.rs:575-705`, особенно `:590-606, 679-685`.

- Claim CAS использует success `Acquire`, хотя rollback не публикует payload state,
  которое новый winner обязан увидеть. Комментарий теперь честно называет `Relaxed`
  открытым вопросом. Это хороший кандидат для loom counterfactual/measurement, после
  чего CAS и rollback stores потенциально можно ослабить.
- Каждый оборот loser spin делает `Acquire`. Возможен Relaxed polling с одним
  корректным Acquire при наблюдении READY; такую трансформацию нужно отдельно доказать
  в модели, особенно на слабой архитектуре.
- Большой generic `get_or_try_init<F>` держит winner, panic и spin code рядом с fast
  path. Маленький inline wrapper и `#[cold] #[inline(never)]` slow path могут уменьшить
  monomorphized instruction footprint.

Это не correctness fixes. Не менять ordering только по интуиции: сначала отрицательный
оракул/model, затем профиль на AArch64/ARM и codegen inspection.

### F6 — P3 benchmark validity — cold metric измеряет `Box::new` и рост heap, а contention не измеряется

**Где:** `benches/racy_ptr_cell_bench.rs:35-84`.

`get_or_try_init/cold` аллоцирует и leak-ит новый `Box` внутри timed closure на каждой
итерации. Число поэтому смешивает cell protocol с системным allocator, allocator
metadata и монотонным ростом heap; оценка риска OOM не делает метрику чувствительной
именно к регрессиям cell.

Лучше заранее создать один leaked, выровненный payload и публиковать тот же
`NonNull` в каждой fresh cell; cell не владеет pointee. Добавить baseline harness
overhead, отдельный cold claim/publish и bounded real-contention benchmark. Текущий
комментарий “NOT IMPLEMENTED” честен, но означает, что самый дорогой сценарий крейта —
loser spin — не имеет performance baseline.

### F7 — P3 verification — остаются два слепых места в оракулах

**Где:** `tests/loom_racy_ptr_cell.rs:414-530`; текущие вызовы `get()` после READY.

- Loom property 6 переносит pointer через `expose_provenance() -> usize` и восстанавливает
  его `with_exposed_provenance_mut` для reclaim. Это допустимо в exposed-provenance
  model, но несовместимо с crate-wide strict-provenance режимом и не нужно: после join
  опубликованный pointer можно получить через `cell.get()`, а адреса workers хранить
  только для сравнения.
- Нет real-type loom-oracle, который вызывает публичный `get()` пока другой поток
  удерживает sentinel. Регрессия `get()` predicate, возвращающая sentinel как `Some`,
  не адресована напрямую. Нужен reader-vs-initializer сценарий: `None` во время
  INITIALIZING, затем настоящий pointer после Release publish.

Native panic rollback тест ограничивает ожидание timeout-ом, но не join-ит worker при
успехе/ошибке. Для нормального зелёного пути handle лучше join-ить после сигнала;
на regression path timeout всё равно потребует process-level isolation, если нужно
гарантированно не оставлять spin thread.

### F8 — P3 docs/portability — точный механизм parking у `OnceLock` не является стабильным API-контрактом

**Где:** `src/lib.rs:42-55`, `README.md:18-21`, `Cargo.toml:7`.

Текущий std действительно блокирует конкурентов через `Once`; официальный rustdoc
обещает blocking для соответствующих Once/OnceLock operations и прямо называет
reentrant deadlock текущей реализацией, которая в будущем может измениться
([`OnceLock`](https://doc.rust-lang.org/nightly/std/sync/struct.OnceLock.html#method.get_or_init)).
Однако **PARK** — конкретный механизм реализации, а не стабильное обещание метода.
Для долгоживущего crates.io description точнее: “may block losing threads; this crate
busy-spins and uses no OS parking primitive”. Это сохраняет реальное отличие без
semver-хрупкой характеристики std internals.

### F9 — P3 maintainability/published-doc hygiene

**Где:** `src/lib.rs:113-174, 217-235, 307, 593-603, 707-790`, README.

- `#![cfg_attr(not(test), no_std)]` можно заменить безусловным `#![no_std]`: unit code
  в `src/` отсутствует, integration tests и так подключают library как dependency.
- Type doc всё ещё говорит “safe inside a `#[global_allocator]` niche” (`:307`), тогда
  как новый контракт сознательно перешёл на более точное “usable”.
- Ordering comment содержит дрейфующую ссылку `OOM at :544`; store уже на `:666`.
  Удалить номер и назвать участок семантически.
- После перестановки разделов “rules above are all about what init does” уже стоит
  после материала о link environment; `below` у cell-consistency guarantee указывает
  вверх. README и rustdoc дают блоки fork/panic в разном порядке.
- Публичный rustdoc и README перегружены внутренними `task #...`/`finding F...`, которые
  downstream reader не может разрешить. Историю решений лучше оставить в CHANGELOG и
  repo reports, а user-facing contract сделать автономным.
- Заголовок unsafe seam одновременно говорит о “SINGLE documented reason” и “two
  audited kinds”; фактический inventory корректен, формулировка нет.

## Что сделано хорошо

- `READY` терминален; exhaustive write-site review не нашёл ABA или записи поверх
  опубликованного pointer.
- Sentinel создаётся через `without_provenance_mut`, сравнивается только по адресу и
  никогда не разыменовывается.
- Constructor alignment guard и release-active result guard закрывают обе достижимые
  collision формы, включая synthesized safe `NonNull` с адресом `1`.
- `RollbackGuard` вооружён ровно вокруг caller closure и очищает sentinel при unwind;
  explicit OOM rollback не зависит от Drop.
- Loser ждёт только `INITIALIZING`, выходит на null и re-race-ит; он не ждёт READY,
  который после OOM может не появиться.
- Release publish и все выдающие pointer Acquire-loads образуют нужный happens-before
  для инициализации pointee.
- `unsafe impl Send/Sync` соответствует unconditional модели `AtomicPtr`: crate не
  создаёт `&T`, не разыменовывает pointee и возвращает raw-pointer wrapper; invariance
  зафиксирована `PhantomData<*mut T>`.
- `dbg_rollback_reenterable` восстанавливает null только если собственный второй CAS
  вернул ownership; concurrent winner не clobber-ится.
- Root loom-shim после `04c92b3` имеет локальные SAFETY proofs и на текущем HEAD не
  разошёлся по ключевым переходам с extracted crate.
- Последние CI-изменения добавили отдельные Miri (включая strict provenance), rustdoc
  `-D warnings`, release loom sentinel и all-targets Clippy gates. В этом исследовании
  их наличие проверено статически, результаты не переисполнялись.
- Normal build не имеет сторонних runtime dependencies; loom изолирован cfg-веткой.

## Рекомендуемый порядок исправлений

1. Исправить fork contract (F1), включая правило между fork и успешным exec.
2. Либо cfg-изолировать unsupported-target body, либо ослабить обещание guard (F2).
3. Переписать panic section как “contract vs measured evidence” (F3).
4. До фиксации API решить `FnOnce` и форму стабильных `dbg_*` probes (F4).
5. Исправить benchmark и добавить недостающие оракулы (F6–F7).
6. После появления чистого baseline измерить F5; не ослаблять atomics без модели.
7. Одним docs-cleanup убрать остаточный referential/internal-tracker шум (F8–F9).

После пунктов 1–3 достаточно короткого повторного статического review публичного
контракта. Изменение production state machine для выдачи GO по найденным блокерам не
требуется.
