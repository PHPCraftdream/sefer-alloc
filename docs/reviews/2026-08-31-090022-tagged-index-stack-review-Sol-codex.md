# `tagged-index-stack` — статический аудит, прогон 2

**Автор:** Сол-кодекс

**Время:** 2026-08-31 09:00:22 +02:00 (Europe/Berlin)

**Ревизия:** `e70b2d906891eed69c13701362c7edc1252952bf`

**Режим:** только чтение; без под-агентов; тесты, `cargo`, clippy, rustdoc,
benchmark и package dry-run не запускались.

## Итог

**NO-GO для публикации текущего публичного API.**

Предыдущий раунд был обработан не только косметически: появились checked
`try_pack`, compile-time cap `INDEX_BITS = 1..=16`, backoff, синхронизация
benchmark, crate-scoped lint gates и более честное описание `Links`. При
повторной проверке основной API-блокер, однако, остался: head стека не связан
с конкретным backing links типами, поэтому корректный safe-код может передать
другой `ArrayLinks` и получать один и тот же индекс бесконечно.

Кроме того, конечный tag всё ещё описан как «structurally defeating ABA», хотя
это лишь практическая вероятностная защита с ограниченным wrap budget. Это
нельзя считать безусловной гарантийной семантикой библиотеки.

Остальные найденные проблемы — build-diagnostics, release-hardening,
точность документации и performance-полировка.

## Объём проверки

Прочитаны заново текущие:

- весь `crates/tagged-index-stack/src/lib.rs`;
- `Cargo.toml`, README, CHANGELOG, лицензии;
- все шесть файлов в `tests/` и benchmark;
- CI/release-маршруты;
- текущий consumer `RegistryLinks` в `src/registry/heap_registry.rs`;
- история изменений крейта от предыдущего отчёта до `e70b2d9`, включая cap,
  `try_pack`, identity-документацию, backoff, loom-oracles и benchmark fixes.

## Блокеры

### P1 — safe API всё ещё допускает смену backing и повторную выдачу индекса

**Где:** `src/lib.rs:467-511`, `754-817`; `push`/`pop` на `834` и `967`.

Документация теперь честно называет использование разных backing’ов
нарушением caller contract, но сигнатура по-прежнему не может его выразить:
каждый вызов независимо generic по `L` и принимает произвольный `&L`.

Детерминированный сценарий без гонки и `unsafe`:

```rust
let a = ArrayLinks::<2>::new();
let b = ArrayLinks::<2>::new();
let stack = TaggedIndexStack::<16>::new();

stack.push(&a, 1);
stack.push(&a, 0); // a[0] = 1, head = 0

assert_eq!(stack.pop(&b), Some(0)); // b[0] == 0
assert_eq!(stack.pop(&b), Some(0)); // CAS current -> current
```

При первом `pop(&b)` новый head строится из `b[0] == 0`, то есть совпадает с
текущим head по index и tag. CAS успешно записывает то же значение и
возвращает `0`, не удалив его. Следующий `pop` повторяет то же самое. В
allocator это превращается в двойную выдачу одного slot.

То, что реальный `RegistryLinks` всегда использует один registry backing,
снижает риск в текущем workspace consumer, но не исправляет опубликованный
generic API. Контрактная документация не является защитой от ошибки safe-кода.

**Рекомендация без компромисса:** связать backing с объектом стека — например,
сделать тип `TaggedIndexStack<L>`, который владеет provider/стабильным handle,
и убрать `&L` из каждой операции. Для slot-resident storage понадобится
отдельный стабильный storage handle, а не self-reference на registry. Если
архитектура принципиально должна принимать backing снаружи на каждом вызове,
операции должны иметь явно unsafe caller contract либо API должен возвращать
bound view, который нельзя смешать с другим backing. Оставлять это как
непроверяемое условие safe API не стоит.

### P1 — tag остаётся конечным и не даёт строгой гарантии отсутствия ABA

**Где:** `src/lib.rs:1-12`, `108-164`, `266-319`; README `1-14`, `83-138`;
CHANGELOG `27-77`.

Cap `INDEX_BITS <= 16` улучшил ситуацию и гарантирует минимум 48 tag bits,
но не делает counter бесконечным. После `2^TAG_BITS` успешных push старое
полное значение `(index, tag)` может повториться; если stale pop пережил это
окно, его CAS снова может пройти. Это bounded generation-tag mitigation.

Текущая документация по-прежнему использует формулировки `structurally
defeats ABA` и «tag cannot repeat within any physically plausible observation
window». Оценка основана на выбранном ceiling `2e8..1e9 RMW/s`, но этот ceiling
не является инвариантом кода, не проверяется на всех targets и не ограничивает
время, которое процесс может быть suspended/debugged. Даже собственная
uncontended оценка около `2e7` push/s означает для 48-bit tag порядка 163 дней
до wrap; у процесса нет гарантии, что stale operation не переживёт такой срок.

**Рекомендация:** описывать защиту как bounded/probabilistic и явно отделять
инженерный risk budget от correctness guarantee. Если необходима строгая
гарантия, нужен protocol lifetime/reclamation или другой способ не допускать
reuse во время stale operation; конечный packed tag этого не доказывает.
Если практический policy floor в 48 bits сохраняется, это следует назвать
policy floor, а не structural proof.

## Существенные остаточные проблемы

### P2 — portability `compile_error!` не изолирует недоступные imports

**Где:** `src/lib.rs:208-239`.

При target без `target_has_atomic = "64"` crate выдаёт intended
`compile_error!`, но затем unconditional для non-loom builds пытается импортировать
`core::sync::atomic::AtomicU64` через `#[cfg(not(loom))]`. На target, где тип
не экспортируется, это добавляет исходную криптичную ошибку unresolved import.

Аналогичная проблема есть у loom guard: при `--cfg loom` без feature `loom`
срабатывает named `compile_error!`, но `#[cfg(loom)] use loom::...` всё равно
компилируется, хотя dependency не включён, и добавляет unresolved import.

Гейт должен включать capability/feature непосредственно в `use`:

```text
core imports:  all(not(loom), target_has_atomic = "64")
loom imports:  all(loom, feature = "loom")
```

Тогда intended diagnostic действительно будет единственным. Это статический
вывод по cfg-структуре; cross-target build не запускался.

### P2 — некорректный `Links::load_next` в release всё ещё тихо портит stack

**Где:** `src/lib.rs:780-800`, `989-1000`.

`load_next` внешнего safe `Links` обязан вернуть `TAIL` или допустимый index,
но проверка — только `debug_assert!`. В release неверное значение проходит в
`pack`, маскируется и может превратиться в live index или empty sentinel,
теряя всю оставшуюся цепь без panic. Документы подробно объясняют условие,
но release-код не делает его fail-closed.

Варианты:

- сделать проверку безусловной и вынести panic message в cold helper;
- изменить trait так, чтобы provider возвращал checked result;
- сделать trait/операции unsafe и поместить invariant под явную safety
  ответственность.

Если выбран safe API, предпочтителен первый вариант: одна проверка в `pop`
дешевле тихой потери free-list. Если runtime cost принципиально запрещён,
не следует называть внешний trait безопасным без более сильной структурной
гарантии.

### P2 — «lock-free stack» не гарантирован для открытого `Links`

**Где:** crate docs `1-13`, `467-511`; `ArrayLinks` `571-647`.

`TaggedIndexStack` выполняет lock-free CAS на head, но открытый `Links` может
реализовать `load_next`/`store_next` через mutex, blocking I/O или бесконечное
ожидание. Значит, end-to-end прогресс generic stack не lock-free; это верно
только при неблокирующем provider. Стоит писать «lock-free head algorithm» и
отдельно гарантировать свойства `ArrayLinks`, либо добавить progress
требование в контракт (которое тип всё равно не проверяет).

### P2 — README содержит stale count hidden API

**Где:** `README.md:182-190`.

README говорит о «two more» items под `--cfg loom` и «all four», но текущий
код имеет три loom-only hidden public functions: `cas_head_for_test`,
`pop_retry_count_for_test`, `push_retry_count_for_test`. Вместе с default
`raw_head` и `TaggedIndex::empty` получается пять, не четыре.

Это небольшой дефект, но именно тот вид drift, который становится виден
потребителю при чтении документации. Исправить число или убрать inventory,
оставив описание назначения каждого item.

### P2 — hidden public test hooks остаются semver surface

**Где:** `src/lib.rs:426-443`, `1051-1198`; README `182-190`.

Документация теперь правильно признаёт, что `doc(hidden)` не блокирует вызов.
Тем самым она одновременно подтверждает API-проблему: `raw_head`, `empty` и
loom hooks реально являются `pub` symbols в соответствующих конфигурациях.
`doc(hidden)` только скрывает навигацию rustdoc и не снимает semver
обязательств.

До первой публикации лучше перенести test-only hooks в feature/test-support
поверхность или сделать их частью намеренного диагностического API. Особенно
`raw_head` раскрывает representation packed head, которую потом трудно
изменить.

## Performance review

### P3 — backoff полезен под contention, но параметр не является переносимой
гарантией

**Где:** `src/lib.rs:253-264`, `904-912`, `1031-1037`.

Экспоненциальные 1/2/4/8/16/32/64 `spin_loop` после проигранного CAS — разумная
локальная оптимизация и не меняет correctness. Однако `BACKOFF_SPIN_CAP = 6`
подобран по одному x86-64 профилю, а `spin_loop` имеет разные свойства на
x86, ARM и виртуализированных CPU. При малом числе CPU, oversubscription или
вытесненном победителе лишние spins увеличивают latency и consume CPU.

Рекомендация — оставить это configurable/internal policy, иметь отдельные
latency и throughput измерения на нужных архитектурах и сравнить с weak CAS.
`compare_exchange_weak` естественно подходит обоим retry-loop и может быть
дешевле на LL/SC targets; текущий отказ от него доказан только отсутствием
локального x86-эффекта, а не общим отсутствием выигрыша.

### P3 — `push` имеет потенциально лишний Acquire на начальном head load

**Где:** `src/lib.rs:839`; сравнение с `pop` `968`.

`push` использует прочитанный head как integer `(cur_idx, tag)` и не читает
link текущего head. Publication собственного `store_next` обеспечивается
его Release CAS head; поэтому начальный `head.load` в `push` выглядит
кандидатом на `Relaxed`. `pop` начальный load оставлять Acquire необходимо,
поскольку после него читается link.

Это не менять вслепую: нужен loom-проход и perf на weakly ordered/LL-SC
архитектуре. Но для заявленного запроса на ускорение это наиболее очевидная
оставшаяся ordering-кандидатура.

### P3 — cache-line layout остаётся caller responsibility и должен быть
частью practical guidance

`TaggedIndexStack` — голый `AtomicU64`, а `ArrayLinks` помещает 16
`AtomicU32` в одну 64-byte line. Документация это признаёт и правильно не
навязывает padding всем пользователям. Для release guidance полезно явно
рекомендовать профиль: hot head изолировать на embedding site, а standalone
`ArrayLinks` не использовать для high-contention индексов из одной 16-slot
группы. Это не correctness bug, а существенная причина расхождения benchmark
с production slot-resident layout.

## Benchmark и тестовые остатки

### P3 — contention benchmark всё ещё имеет небольшой timing skew

**Где:** `benches/tagged_index_stack_bench.rs:164-235`, `287-322`.

Barrier и интервальная проверка часов закрыли две прежние проблемы (spawn/join
и clock read на каждой итерации). Но каждый worker вычисляет собственный
`deadline = Instant::now() + duration` после выхода из barrier, а main начинает
свой `start` отдельным `Instant::now()`. При scheduler skew workers получают
чуть разные окна, а denominator — окно main до последнего join. Это приемлемо
для грубой цифры, но не для точного regression gate.

Лучше один раз вычислить shared absolute deadline после setup и release barrier,
либо использовать две barrier-фазы/общий start timestamp. Для стабильного
perf gate также нужны несколько повторов и явно сохранённые CPU/affinity/
architecture conditions; текущие hardcoded цифры в CHANGELOG нельзя считать
универсальным baseline.

### P3 — compile-fail boundary остаётся ручной проверкой

**Где:** комментарий в `tests/stack_unit.rs` о недоступном compile-fail
coverage.

`INDEX_BITS > 16` — важная safety/performance policy boundary, но её regression
проверяется только тем, что кто-то вручную инстанцировал недопустимый тип.
Добавить compile-fail doctest или отдельный compile-fail harness до публикации.

### P3 — loom-oracles усилили suite, но часть сценариев всё ещё вручную
инжектирует head mutation

Три модели действительно вызывают production `push`/`pop`, остальные часто
разделяют операцию через `cas_head_for_test`. Это полезно для pinning
interleaving, но `cas_head_for_test` позволяет тесту писать произвольный head,
не являясь публичным runtime API. Для каждого hand-inlined scenario должна
оставаться парная end-to-end модель, иначе тестовая копия может расходиться с
production loop при следующем изменении.

## Что в текущем состоянии выглядит хорошо

- cap `1..=16` устраняет прежнюю 32-bit конфигурацию с малым tag budget;
- `try_pack` отделяет checked путь от быстрого trusted `pack`;
- H-2 переход в empty сохраняет running tag;
- `pop` failure ordering `Acquire` соответствует retry link read;
- backoff ограничен локальным per-call counter и не меняет head protocol;
- dedicated link storage и identity invariant теперь явно описаны;
- ordinary/proptest/loom coverage и activation-oracles значительно сильнее
  типичного маленького lock-free crate;
- crate-scoped clippy/rustdoc CI rows теперь присутствуют;
- `RegistryLinks` использует один стабильный slot-resident backing и не
  проявляет найденный generic backing-swap сценарий.

## Приоритет исправлений

1. Структурно связать stack с одним links backing; одной документации
   недостаточно.
2. Сформулировать bounded ABA guarantee без слов о строгом structural proof;
   если нужна строгая гарантия — изменить protocol, а не только cap.
3. Исправить cfg-gated imports, чтобы named compile errors действительно
   заменяли cryptic follow-up errors.
4. Решить release policy для invalid `load_next` и hidden public hooks.
5. Уточнить lock-free claim, count в README и сократить повторяющуюся
   документацию.
6. Отдельно измерить weak CAS/Relaxed push-load/backoff на целевых ISA;
   затем провести обязательную динамическую проверку владельцем.

## Ограничения

Это только read-only статический аудит. Не запускались тесты, loom, clippy,
rustdoc, benchmarks, `cargo metadata`, build или package dry-run. В worktree
были ранее существовавшие untracked checkpoint-файлы; они не читались для
вывода и не изменялись. Изменён только этот отчёт.
