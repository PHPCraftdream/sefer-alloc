# `tagged-index-stack` — статический аудит перед публикацией

**Автор:** Сол-кодекс

**Время:** 2026-08-30 18:11:47 +02:00 (Europe/Berlin)

**Ревизия:** `57f8da4dd54435770f3bae60ebaa044addd61b38`

**Режим:** только чтение исходников; без под-агентов; тесты, `cargo`, benchmark и иные исполняемые проверки не запускались.

## Вердикт

**NO-GO для первой публикации.**

Сам CAS-цикл Treiber stack, сохранение тега при переходе в empty и ordering на retry-пути `pop` выглядят внутренне согласованными. Нового очевидного дефекта линейризации до оборачивания тега я не нашёл. Однако до фиксации публичного API остаются два блокера:

1. безопасный API позволяет использовать разные экземпляры `Links` между вызовами и детерминированно разрушить абстракцию стека полностью safe-кодом;
2. bounded wrapping tag документирован как практически/структурно устраняющий ABA, хотя при поддерживаемом `INDEX_BITS = 32` окно оборачивания может составлять минуты, а не годы.

После исправления этих двух пунктов нужен ещё один статический проход по новому контракту. Остальные пункты ниже — важные улучшения, но не все по отдельности блокируют публикацию.

## Что изучено

- весь production-код `crates/tagged-index-stack/src/lib.rs`;
- `Cargo.toml`, `README.md`, `CHANGELOG.md`, лицензии;
- все ordinary/proptest/loom tests и benchmark;
- последние изменения крейта от `85e3b37` до `23204eb`;
- CI/release-маршруты крейта;
- реальный потребитель в `src/registry/heap_registry.rs` и его `RegistryLinks`.

## Блокирующие находки

### P1-1. `TaggedIndexStack` не привязан к одному `Links`: safe-код может бесконечно выдавать один индекс

**Где:** `src/lib.rs:340-347`, `539`, `603`.

`push` и `pop` являются независимо generic по `L` и каждый раз принимают произвольный `&L`. Ни тип, ни runtime-состояние не связывают head с тем backing, куда были записаны его links. Документация также не требует использовать один и тот же логический набор link-ячеек на всём протяжении жизни стека.

Детерминированный сценарий, не требующий гонки или `unsafe`:

```rust
let a = ArrayLinks::<2>::new();
let b = ArrayLinks::<2>::new();
let stack = TaggedIndexStack::<16>::new();

stack.push(&a, 1);
stack.push(&a, 0); // a[0] = 1, head = 0

assert_eq!(stack.pop(&b), Some(0)); // b[0] изначально равен 0
assert_eq!(stack.pop(&b), Some(0)); // head так и остался 0
```

При первом `pop(&b)` читается `b[0] == 0`; кандидат нового head совпадает с текущим head `(0, tag)`. CAS `current -> current` успешно завершается, метод возвращает `0`, но элемент не удаляется. Каждый следующий `pop(&b)` снова выдаёт `0`. Для allocator/pool это означает двойную выдачу одного слота.

Даже контракт одного типа `L` проблему не решает: два `ArrayLinks<N>` имеют один тип, но разную идентичность. Аналогично опасны aliasing разных индексов на одну ячейку, нестабильное отображение index→cell и возвращение значения, которое не является `TAIL` или допустимым индексом. Текущий `Links` описывает только ordering операций, но не эти семантические инварианты.

**Лучшее исправление до первого релиза:** структурно связать head и provider links в одном объекте (`TaggedIndexStack<L>` владеет `L` или стабильным handle на него), чтобы `push`/`pop` больше не принимали backing на каждый вызов. Для slot-resident варианта provider может владеть стабильным handle/`Arc` на отдельно размещённое хранилище slots. Если такая архитектура неприемлема, второй по качеству вариант — сделать операции, зависящие от внешнего backing и уникальности pushed index, явно `unsafe` с полным safety contract; простого текстового “caller contract” для safe API недостаточно для структуры, которую будут использовать под unsafe allocator.

Минимальная документационная заплатка должна явно требовать:

- один и тот же логический backing для всей жизни непустого стека;
- стабильное one-to-one отображение каждого допустимого индекса в отдельную link-ячейку;
- coherence `store_next(i, x)`/`load_next(i)`;
- результат `load_next` только `TAIL` либо допустимый индекс;
- backing и ячейки живут и не меняют идентичность, пока head может на них ссылаться.

Но это оставит ошибку доступной safe-коду, поэтому для “без компромиссов” рекомендую именно структурный redesign.

### P1-2. Конечный tag не «defeats ABA» структурно; поддерживаемый 32-bit tag может обернуться за минуты

**Где:** `src/lib.rs:1-25`, `85-98`, `167-172`; `README.md:1-19`, `54-61`; `CHANGELOG.md:29-50`.

Тег имеет `64 - INDEX_BITS` бит и оборачивается. После ровно `2^TAG_BITS` успешных push старое полное значение head может повториться, и припаркованный `pop` снова способен успешно применить stale `next`. Это bounded ABA mitigation, а не безусловное устранение ABA.

Документация усугубляет риск тремя неточностями:

- называет wrapping-счётчик “monotonic”;
- называет 48-bit вариант “structural non-hazard”;
- говорит о push «на одном slot», хотя tag глобален для head и расходуется всеми успешными push; для коллизии нужен тот же head index в момент wrap, но бюджет съедает суммарный churn стека.

Приведённые 100k push/s названы «already unrealistic», однако это не консервативная верхняя граница для одного горячего atomic. Чистая арифметика показывает риск поддерживаемой конфигурации:

| Tag | При 100k push/s | При 10M push/s |
|---:|---:|---:|
| 32 bit (`INDEX_BITS=32`) | ~11.9 часа | ~7.2 минуты |
| 48 bit (`INDEX_BITS=16`) | ~89.3 года | ~326 дней |

Длительная deschedule/pause/debugger-stop на минуты реалистична, поэтому публично поддерживать `INDEX_BITS=32` под обещанием “ABA-defeating” опасно.

**Исправление:** сначала определить честную гарантию API. Для текущей 64-bit схемы:

- везде заменить абсолютные формулировки на bounded generation-tag mitigation;
- документировать формулу `wrap_time = 2^TAG_BITS / aggregate_successful_push_rate` и требование к максимальной паузе операции;
- не называть ни один конечный tag структурным доказательством отсутствия ABA;
- если целевой профиль требует практически большой запас, compile-time ограничить минимальное число tag bits (например, не менее 48), а не поддерживать заведомо слабый 32-bit вариант без сильного предупреждения;
- если нужна математическая гарантия без временного допущения, менять алгоритм: конечный packed tag её дать не может (нужны reclamation/operation-lifetime protocol либо существенно иная state scheme).

Текущее решение «increment только на push» само по себе разумно: возврат прежнего head index требует re-push, а увеличение ещё и на pop лишь быстрее исчерпывало бы бюджет.

## Существенные неблокирующие находки

### P2-1. Ordering контракта `Links` одновременно избыточен, неточен и не выражаем типом

**Где:** `src/lib.rs:310-331`, `341-347`, `395-406`.

Документ сначала утверждает, что stack relies on `Acquire`/`Release` links, затем верно объясняет, что для алгоритма достаточно `Relaxed`: `store_next(Relaxed)` sequenced-before release-CAS head, а `load_next(Relaxed)` sequenced-after acquire-наблюдения head. Кроме того, строки 314-316 приписывают публикацию successful Acquire CAS `pop`, хотя link читается **до** этого CAS; load-bearing acquire — initial head load или Acquire failure-read предыдущего CAS.

После решения P1-1 можно ускорить hot path:

- `ArrayLinks::{load_next,store_next}` → `Relaxed`;
- начальный `head.load` в `push` → `Relaxed` (push использует старый head только как число и не разыменовывает его link);
- оба CAS-loop кандидаты на `compare_exchange_weak`, поскольку spurious failure уже естественно обслуживается retry-loop.

На x86 выигрыш может быть нулевым, на weakly ordered ISA relaxed links убирают лишнюю acquire/release семантику. Изменения orderings надо повторно проверить loom-моделью и измерить отдельно; в рамках этого read-only аудита они не запускались.

### P2-2. `pack` — публичный sharp edge с тихой потерей старших битов

**Где:** `src/lib.rs:230-253`; тест, закрепляющий truncation, `tests/stack_unit.rs:54-85`.

`TaggedIndex::pack` принимает любой `u64`, молча маскирует index и молча оставляет только доступные tag bits. Перед первым релизом лучше не цементировать это поведение: safe checked `try_pack`/asserting `pack` с отдельным private unchecked helper предотвращает превращение ошибочного index в другой live index либо empty sentinel. Документация подробно объясняет ловушку, но это не заменяет API, не допускающий её случайно.

### P2-3. `#[doc(hidden)] pub` не является test-only и не исключает semver-обязательств

**Где:** `src/lib.rs:270-281`, `652-667`; `README.md:88`; `CHANGELOG.md:86-87`.

`TaggedIndex::empty()` и `TaggedIndexStack::raw_head()` входят в публичный metadata/API обычной сборки. `doc(hidden)` только прячет их из обычного rustdoc navigation; downstream-код всё равно может вызвать символы. Фраза “not part of stable public API” технически не обеспечивает этого.

До публикации следует либо сделать их private и перенести нужные тесты внутрь crate, либо дать явный opt-in test-support feature, либо признать и качественно документировать стабильный snapshot API. Особенно `raw_head` раскрывает representation, которую затем будет трудно менять.

### P2-4. Нет crate-scoped `clippy --all-targets -D warnings` и rustdoc gate

**Где:** `.github/workflows/ci.yml:1726-1753`, `1928-1940`, `2391-2418`.

CI запускает ordinary/release tests, no_std build и loom, но точных строк `cargo clippy -p tagged-index-stack --all-targets -- -D warnings` и `RUSTDOCFLAGS="-D warnings" cargo doc -p tagged-index-stack --no-deps` нет. Root `--all-targets` не проверяет bench target dependency-крейта как собственный target.

Это важно уже сейчас: benchmark дважды делает expression-statement `black_box(stack.pop(...));` (`benches/...:51`, `73`). Результат имеет `Option<u32>` с `must_use`, поэтому при прямой строгой lint-сборке ожидается `unused_must_use`; лучше одновременно не игнорировать oracle, а проверять ожидаемый `Some(index)`/`None`.

## Benchmark и тестовая инфраструктура

### P3-1. Contention benchmark в значительной мере измеряет часы и запуск потоков

**Где:** `benches/tagged_index_stack_bench.rs:146-196`, `225-296`.

`start` создаётся до spawn-loop, общей стартовой barrier нет, ранние threads получают более длинное окно, а denominator включает spawn/join. В каждом цикле вызывается `Instant::elapsed()`, что добавляет систематическую цену clock read к двум коротким atomic operations.

Для репрезентативного throughput: заранее создать workers, синхронизировать start barrier, публиковать единый deadline и проверять время блоками (например, раз в 256/1024 операций), а startup/join исключить из измеренного интервала.

### P3-2. `contention/churn` содержит недостижимый fallback и платит за него в hot loop

**Где:** benchmark `202-275`.

Prefill содержит 64 уникальных индекса, threads не более 8, каждый thread одновременно удерживает не более одного popped index. При соблюдении инвариантов стек не может опустеть: минимум 56 элементов остаются внутри. Ветка `None`, `fresh_idx` и `fresh_idx_outstanding` — лишняя сложность и лишние branches в измеряемом пути. Здесь лучше считать `None` нарушением setup/invariant, а не подмешивать другой workload.

### P3-3. Empty-fast-path зависит от внутреннего warm-up поведения внешнего harness

**Где:** benchmark `55-74`.

Строка предварительно pushes один элемент и рассчитывает, что harness гарантированно вызовет closure вне timed region. Для измерения empty fast path достаточно изначально пустого stack; это точнее и не ломается при изменении warm-up semantics зависимости.

### P3-4. Loom exposition harness сам записывает искусственный tag 0

**Где:** `tests/loom_aba.rs:594-683`, особенно `618-620`, `641-653`.

`run_cas_retry` проверяет failure ordering, но оба candidate head hardcode tag 0 вместо наблюдённого tag. Это уже не faithful two-iteration expansion реального `pop` и в успешной ветке действительно меняет running tag; комментарий «nothing pushes after» также не всегда верен, потому что concurrent thread B выполняет push. End-to-end test реального типа выше снижает риск, но exposition harness лучше сделать точной копией исследуемой части, чтобы counterfactual не мог паниковать из-за постороннего tag-reset эффекта.

### P3-5. Известный compile-fail gap оставлен ручным

**Где:** `tests/stack_unit.rs:241-247`.

Ограничение `INDEX_BITS in 1..=32` — важная часть correctness boundary, но тест прямо сообщает, что оно проверено только вручную. Перед публикацией стоит закрепить compile-fail doctest/trybuild-equivalent, особенно если const-evaluation routing снова будет рефакториться.

## Code smell / «нейрослоп» / точность текста

- Production-файл вырос примерно до 695 строк при небольшом алгоритмическом ядре; одни и те же H-2/RAD-1/orderings объясняются многократно в crate docs, item docs, README, CHANGELOG и длинных inline comments. Это повышает вероятность drift — уже есть противоречия вокруг `Links`, monotonic/wrapping и test-only/public.
- Маркетинговые абсолюты (“defeats ABA”, “structural non-hazard”, “already unrealistic”) следует заменить проверяемыми контрактами и формулами.
- `CHANGELOG` перечисляет hidden `empty()` среди helpers и говорит, что cap не даёт index «collide with TAIL»; при `INDEX_BITS=32` empty sentinel численно равен `TAIL`, а запрещён только live index. Формулировка должна различать эти понятия.
- Комментарии с внутренними кодами H-2/RAD-1 полезны как краткие имена инвариантов, но их повторение и uppercase prose уже мешают чтению hot path. Один строгий раздел invariants + короткие ссылки из кода будут устойчивее.

## Что выглядит хорошо

- `INDEX_BITS` принудительно ограничен `1..=32`, и guard протянут через публичные associated items.
- `push` безусловно отвергает reserved empty index.
- Last-element `pop` сохраняет running tag вместо сброса в bootstrap zero.
- Failure ordering `pop` равен `Acquire`, что необходимо, поскольку returned `actual` сразу используется для следующего link read.
- Все изменения head — RMW; текущий аргумент release-sequence для Acquire-only pop CAS согласован при этом инварианте.
- Повторный push уже документирован как запрещённый caller contract.
- Реальный consumer `RegistryLinks` использует стабильные slot-resident `AtomicU32`, один registry backing и совпадающий `TAIL`; найденная P1-1 ошибка в текущем consumer не проявляется, но остаётся в публикуемом общем API.
- Есть ordinary boundary tests, property tests и loom tests с counterfactuals; тестовая база существенно сильнее средней для маленького lock-free crate.
- `no_std`, atomic-width gate, dual license и release route присутствуют.

## Рекомендуемый порядок исправлений

1. Перепроектировать связь stack↔links так, чтобы нельзя было смешать backing’и safe-кодом; одновременно формализовать identity/range/lifetime/coherence invariants.
2. Выбрать честную гарантию против ABA и скорректировать допустимые `INDEX_BITS`, название/описание и расчёт wrap budget.
3. После нового API убрать лишние link barriers, relaxed-load в `push` и попробовать weak CAS; проверить loom и benchmark отдельно.
4. Закрыть public-hidden/checked-pack API до первой публикации.
5. Добавить crate-scoped clippy/rustdoc gates и исправить benchmark methodology/oracles.
6. Упростить повторяющуюся документацию, затем провести финальный статический аудит контрактов.

## Ограничения этого отчёта

Это намеренно статический read-only аудит. Ни один тест, loom model, clippy, rustdoc, package dry-run или benchmark не запускался. Поэтому все динамические проверки после исправлений остаются обязательной работой владельца, но не меняют два детерминированно выведенных API-блокера выше.
