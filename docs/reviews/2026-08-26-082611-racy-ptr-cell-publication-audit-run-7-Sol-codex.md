# `racy-ptr-cell` — аудит готовности к публикации, прогон 7

- Время: 2026-08-26 08:26:11 +02:00 (Europe/Berlin)
- Ревьюер: Сол-кодекс (`Sol-codex` в имени файла)
- Ревизия: `c1d812d48a166183288da3b70af0e68ca3f38156`
- Предыдущий отчёт: `docs/reviews/2026-08-26-075348-racy-ptr-cell-publication-audit-run-6-Sol-codex.md`
- Режим: статическое чтение, без под-агентов. Тесты, сборка, Clippy, rustdoc,
  Miri, loom, benchmark, package и publish-команды не запускались.

## Вердикт

**GO.** Блокирующих P1/P2-находок нет. Production-реализация выглядит готовой
к публикации: state machine, атомарная публикация, rollback при `None` и unwind,
provenance, layout, lifetime-позиция raw-pointer API и portability-gate согласованы.

Есть одна неблокирующая P3-находка в benchmark-документации и интерпретации
результата. Исправление желательно до финального релиза, если benchmark должен
служить строгим публичным evidence; на корректность или безопасность библиотеки
оно не влияет.

Это вывод статического исследования, а не собственное подтверждение зелёного CI.
Результаты запусков, записанные авторами в commit messages, рассмотрены только как
история изменений и не выдаются за проверки этого прогона.

## Что исследовано

Заново прочитаны:

- `Cargo.toml`, README, CHANGELOG, обе лицензии и весь список публикуемых файлов;
- `src/lib.rs` и `src/imp.rs` целиком, включая все unsafe-сайты и публичный API;
- native- и loom-тесты, их оракулы, handshakes, negative counterfactuals и cleanup;
- benchmark и `bench-iters.txt`;
- package, native debug/release, Clippy, rustdoc, no_std, Miri и loom-строки CI;
- root loom-shim, зеркалящий тип для интеграционного окружения;
- все изменения после отчёта прогона 6 и соответствующие commit messages.

По требованию исследование выполнено одним контекстом без под-агентов. Из
`rust-intel` применены направления unsafe/provenance, concurrency/state,
RAII/drop, API/lifetimes, performance, dependencies/features, test-oracle и CI.
Async, FFI, crypto и network production-поверхностей в crate нет.

`git diff --check 2da6970..HEAD` не сообщил whitespace-ошибок. Рабочее дерево
содержало посторонние untracked `.check-*.log`; они не читались, не менялись и не
включались в коммит отчёта.

## Новые коммиты после прогона 6

### `06e3886` — новые contention baselines

Предыдущее конкретное замечание исправлено: `baseline/scaffolding_only` теперь
использует тот же untimed setup, а worker-потоки проходят общий
`take_round_cell`, поэтому в обеих строках присутствуют одинаковые вызовы
`Mutex::lock` и `Arc::clone`. Главный поток в обоих случаях получает `cell` из
setup и не блокирует `slot` в timed region. Старый barriers-only сценарий честно
переименован в `baseline/barrier_floor`; worker shutdown и join сохранены.

Это делает baseline существенно полезнее. Однако новое описание переходит от
«формы согласованы» к более сильному утверждению об **точной** изоляции стоимости,
которое метод измерения не доказывает — см. F1.

### `43ab70c` — честный контракт unwind-теста

Исправление корректно. Имя
`concurrent_get_or_try_init_started_before_unwind_completes_still_succeeds` и
комментарии теперь различают две допустимые гонки:

1. caller успевает увидеть sentinel и ждёт rollback;
2. winner полностью откатывается раньше первого CAS caller'а, после чего caller
   сразу выигрывает CAS на `null`.

Тест больше не утверждает, что детерминированно вошёл именно в spin-loop.
Связанный комментарий `RollbackGuard` синхронизирован с этой более слабой, но
реально доказанной гарантией. Прежняя находка закрыта.

### `3b0f1f2` — контракт `RollbackProbe` в CHANGELOG

Исправление точное: `Proven` гарантирует восстановление `UNINIT`, а
`NotApplicable` гарантирует отсутствие clobber чужого владельца, но не равенство
итогового состояния состоянию на входе. CHANGELOG теперь совпадает с enum- и
method-документацией.

### `6d8032c` — описание native-тестов и CI

Слово `single-threaded` удалено, описание теперь учитывает concurrency-тесты.
Из CI-комментария убран быстро устаревающий числовой count. Команды CI не
изменены. Прежняя находка закрыта.

### `c1d812d`

Это только запись о закрытии предыдущего review-item в общей документации;
crate не менялся.

## Находки

### F1 — P3: subtraction contention-baseline не является точной изоляцией стоимости протокола

**Где:** `crates/racy-ptr-cell/benches/racy_ptr_cell_bench.rs:7-35`,
`:213-234`, `:272-324`.

Верх файла одновременно утверждает:

- «Every row measures the CELL PROTOCOL — and nothing else»;
- `baseline/*` выполняются без cell call.

Эти утверждения несовместимы уже буквально: baseline-строки протокол не
измеряют, а остальные строки неизбежно включают timer/harness/`black_box`.

Более важная методологическая проблема — повторённое утверждение, что

```text
contention/one_cell - baseline/scaffolding_only
```

«isolates» и «genuinely» даёт собственную contended-стоимость протокола.
Синтаксическая форма worker-путей теперь действительно совпадает, но timed
величина — не сумма независимых операций. Это длительность параллельного раунда
от `start` до `done`, то есть makespan критического пути четырёх потоков:

- три worker'а последовательно конкурируют за `Mutex` и клонируют `Arc`;
- в contention-строке эти действия перекрываются по времени с CAS, closure,
  публикацией и spin-loop других потоков;
- в baseline-строке cell-операций нет, поэтому меняются overlap, расписание и
  сам критический путь;
- вычитание двух makespan не алгебраически удаляет стоимость mutex/Arc;
- в разность входит также заданный пользователем `init_body` с 64
  `spin_loop`, а не только внутренняя реализация `RacyPtrCell`.

Следовательно, baseline — хороший контрольный контекст и differential estimate,
но не точное измерение intrinsic protocol cost. Числа могут надёжно отвечать на
вопрос «насколько целый contended round медленнее контрольного раунда в этом
harness на этой машине», но не на более сильный вопрос «сколько наносекунд
занимает только протокол».

**Что исправить:**

- заменить верхнюю фразу на нейтральную: benchmark содержит рабочие строки и
  соответствующие harness-baselines;
- назвать subtraction «differential estimate under the same harness», а не
  exact/genuine isolation;
- явно указать, что результат — round makespan и включает `init_body`;
- публиковать обе абсолютные величины и их разность, не выдавая разность за
  аддитивно отделённую стоимость;
- если нужен именно сравнительный regression signal, сравнивать саму
  `contention/one_cell` между ревизиями при фиксированных contenders/spin/setup;
  baseline использовать для обнаружения дрейфа harness/машины.

Перестраивать production API или добавлять test-only hooks ради этого не нужно.

## Общий аудит production-кода

### State machine и liveness

Состояния разделены однозначно:

- `null` — `UNINIT`;
- address `1` — `INITIALIZING`;
- любой другой non-null address — `READY`.

`new` требует `align_of::<T>() >= 2`, поэтому корректно выровненный реальный
pointer не совпадёт с sentinel. Safe closure всё же способна синтезировать
`NonNull` с адресом 1; release-active `assert!` перед publish отклоняет его и
rollback guard возвращает cell в `UNINIT` при unwind.

Loser ждёт только пока адрес равен sentinel. После `null` он выходит на re-race,
после реального pointer возвращает его. Поэтому `None` winner'а не оставляет
loser ждать никогда не наступающего `READY`.

`FnOnce` соответствует реальному поведению: один вызов метода вызывает closure
не более одного раза, включая повторный CAS после проигрыша/rollback.

### Atomic ordering

Load-bearing пара публикации корректна:

- winner публикует pointer через `store(Release)`;
- `get`, hot path, rechecked path и loser читают через `load(Acquire)`.

Это даёт happens-before для инициализации pointee. Failure-ordering CAS равен
`Relaxed`, затем состояние перечитывается. Rollback выполняется `Release`, а
новый winner захватывает ownership через CAS `Acquire`.

CAS success и каждое spin-чтение, возможно, сильнее минимально необходимого.
Код сам честно помечает это как открытый вопрос. Ослаблять ordering без loom-
контрфактуала и измерения на weakly ordered target не рекомендую: на x86-64
практического доказательства выигрыша получить нельзя.

### Unwind и Drop

`RollbackGuard` создаётся только после успешного claim CAS. На успешной
публикации и явном `None` он defuse'ится; при unwind выполняет `store(null,
Release)`. Guard не может clobber чужого владельца: пока он armed, ownership
sentinel принадлежит этому winner'у, а переход к чужому owner возможен только
после rollback, который guard выполняет сам.

Документация правильно отделяет гарантию согласованности cell от запрета unwind
через `GlobalAlloc`, а также объясняет поведение при `panic=abort`.

### Unsafe и provenance

Production unsafe-инвентарь ограничен:

- два manual impl: `Send` и `Sync`;
- четыре `NonNull::new_unchecked` после проверки `is_ready`/адреса;
- dereference `T`, free и ownership pointee внутри crate отсутствуют.

Sentinel создаётся через `without_provenance_mut` и только сравнивается по
адресу. Каждый `new_unchecked` доминируется проверкой non-null/non-sentinel.
Unconditional `Send + Sync` соответствует семантике `AtomicPtr`: crate передаёт
raw capability, но не создаёт `&T` и не разыменовывает pointer. Безопасность
последующего доступа остаётся обязанностью caller'а и явно документирована.

`PhantomData<*mut T>` фиксирует связь и инвариантность; ручные auto-trait impl не
переносят на crate обязанность thread-safe dereference pointee.

### Layout, API и portability

`#[repr(transparent)]` закрепляет обещание layout одного `AtomicPtr<T>`;
дополнительное поле — только ZST `PhantomData`. Публичная поверхность мала:
`RacyPtrCell`, `RollbackProbe`, `new`, `get`, `get_or_try_init`, два стабильных
`dbg_*` probe и `Default`.

`#[must_use]` установлен на constructors/accessors/probes, а сообщение
`get_or_try_init` объясняет значение потерянного `None`. Публичные ограничения
re-entry, transitive lock ordering, allocation/block/panic, fork и signal safety
описаны необычно подробно и согласованы между rustdoc и README.

На target без pointer-width atomics implementation module выключен положительным
cfg, а facade выдаёт один целевой `compile_error!`. Для заявленного `no_std`
normal dependencies отсутствуют; `loom` доступен только под `cfg(loom)`,
`bench-scale-tool` — dev dependency.

### Производительность

READY fast path `get_or_try_init` — один `Acquire` load, predicate и branch;
generic slow path вынесен в `#[cold] #[inline(never)]`. `get` также остаётся
одним `Acquire` load. Heap allocation, parking и OS synchronization в
production-коде отсутствуют.

При первом `UNINIT` вызове fast path загружает состояние, затем `init_slow`
загружает его повторно перед CAS. Теоретически второй load можно устранить,
передав исходное наблюдение в slow path или начав с CAS, но это одноразовый cold
path, изменение усложняет re-race control flow, а измеренного выигрыша нет.
Релизным улучшением это не считаю.

Busy-spin — сознательная цена allocator-bootstrap niche. Документация не обещает
fairness или bounded latency и требует короткий non-blocking initializer.

## Тесты и CI — статическая оценка

Native suite покрывает initial state, exactly-once sequential fast path,
`get`, OOM rollback/retry, unwind rollback, concurrent caller around unwind,
sentinel rejection в release, align-1 rejection, probe arms и layout.
Timeout-тесты ограничивают ожидание вызывающего потока; на красном пути могут
оставить spinning worker до завершения test process, что явно документировано и
предпочтительнее безусловного зависания на `join`.

Loom suite запускает реальный тип под atomic shim и проверяет two-/three-thread
publish, OOM/re-race, `get` во время sentinel, probe-vs-real-winner race,
pointer agreement и exactly-once. Counterfactuals для Relaxed publish и
spin-until-READY служат anti-vacuity evidence. Ограничения preemption bounds и
невозможность корректно моделировать unwind признаны, а native-тест не
перепродаёт более сильное доказательство.

CI содержит:

- native debug и release тестовые строки;
- Clippy `--all-targets -D warnings`, охватывающий benchmark;
- rustdoc с `-D warnings`;
- bare-metal `thumbv7em-none-eabi` build;
- обычный и strict-provenance Miri;
- real-type loom и grep критического probe-теста;
- `cargo publish --dry-run` package gate.

Команды не запускались в этом прогоне.

## Итоговый release checklist ревьюера

- Production correctness/soundness: **GO**.
- Public API и docs contract: **GO**.
- no_std/dependency/layout/portability posture: **GO**.
- Test/CI design по статическому чтению: **GO**.
- Benchmark как regression harness: **GO**.
- Заявление о точной аддитивной изоляции benchmark cost: **исправить F1**.

Итог: crate можно публиковать; для безупречного evidence-текста рекомендуется
сначала закрыть единственную P3-находку F1.
