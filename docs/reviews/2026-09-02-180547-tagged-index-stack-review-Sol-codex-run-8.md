# `tagged-index-stack`: предрелизное ревью — Sol-codex, прогон 8

- Время начала исследования: 2026-09-02 18:05:47 CEST (`Europe/Berlin`)
- Метка запроса: 18:04
- Проверенный `HEAD`: `f35e4fb6909e1329d6db9d28167f4309f1e2136f`
- Последний просмотренный коммит: `f35e4fb` — `docs(tis): P3-1 — compress StackStorage rustdoc + CHANGELOG out of review-archaeology`
- Режим: новое самостоятельное статическое ревью, без агентов и под-агентов
- Не запускались: `cargo`, тесты, doctest, clippy, rustdoc, loom, Miri, benchmark, examples и scripts
- Единственное изменение репозитория в рамках ревью: этот Markdown-отчёт и его коммит

## Вердикт

**NO-GO к публикации в текущем виде.**

Новая caller-side `unsafe fn`-граница исправляет прежние дыры с более узким link domain и safe double-push. Основной CAS-цикл, H-2 empty transition, приватный truncating pack, release sequence и обычный короткий ABA-сценарий согласованы. Однако полный оборот конечного тега остаётся не просто честно описанным «остаточным риском», а контрпримером к заявленному allocator-grade soundness-контракту: при соблюдении обеих нормативных safety-клауз старый CAS может успешно вернуть в head уже выданный индекс. Для контейнера `u32` это логическая порча, но для allocator consumer, которому документация обещает exclusive issuance как основу memory safety, это путь к UB вне крейта без нарушения записанного caller contract.

До GO необходимо закрыть P1-1 архитектурно либо отказаться от allocator-soundness обещания и сформулировать реально выполнимую дополнительную safety-предпосылку. Также до публикации следует исправить неявные unsafe operations и сломанный последним API-break A/B wall-clock harness.

## Область и полнота проверки

Полностью прочитаны production-файлы `src/lib.rs` и `src/imp.rs`, manifest и публичный surface. Проверены README/CHANGELOG, изменения после прогона 7, новые compile-fail и narrow-domain оракулы, все найденные call sites `push`/`push_index` в крейте, benchmark, latency example, loom/real-thread/custom-storage тесты, measurement driver/template, релевантная CI-секция и единственный production consumer — `sefer-alloc::Registry`.

Это bounded single-context review. Generated fixture lockfile и большие сохранённые perf-артефакты проверялись как dependency/evidence surface, но не перечитывались построчно. После census не углублялись отсутствующие здесь классы: async, FFI, crypto, serde, сетевой/файловый I/O, raw-pointer ownership и RAII внешних ресурсов.

## Блокирующая находка

### P1-1. Полный оборот тега нарушает exclusive issuance при полностью contract-compliant push

Места:

- `crates/tagged-index-stack/src/lib.rs:3-15,139-197` — wrap признан возможным, а «accepting residual risk» назван частью caller contract;
- `crates/tagged-index-stack/src/imp.rs:490-493` — `StackStorage` обещает exclusive index issuance, на котором allocator consumers строят memory safety;
- `crates/tagged-index-stack/src/imp.rs:1037-1070` — нормативный `push_index # Safety` содержит ровно две клаузы: link domain и liveness; stale lost-CAS observer явно не накладывает обязанности;
- `crates/tagged-index-stack/src/imp.rs:1133-1139` — `pop_index` признаёт остаточный full-wrap hazard;
- `crates/tagged-index-stack/src/imp.rs:1346-1407` — tag увеличивается только push-операцией и оборачивается через `wrapping_add` + `pack_truncating`.

Статический контрпример. Пусть ширина тега равна `T`, `M = 2^T`, а исходная цепочка — `A -> B -> TAIL`, head `(A, t)`.

1. Поток P начинает safe `pop_index`: читает `(A, t)`, затем старый `next[A] = B`, и останавливается до CAS.
2. Поток Q законно извлекает сначала `A`, затем `B`; stack пуст с тем же тегом `t`. `B` остаётся у Q.
3. Q `M - 1` раз делает `push(A); pop(A)`, каждый раз re-push ровно того индекса, который только что вернул pop. Затем выполняет M-й `push(A)` на пустой stack и не извлекает его. Все push находятся в домене, `A` перед каждым push не reachable, double-push отсутствует.
4. После полного оборота head снова равен `(A, t)`, но теперь `next[A] = TAIL`, а не `B`.
5. Старый CAS P `(A, t) -> (B, t)` успешно совпадает. P получает `A`, а head становится `B`, хотя `B` всё ещё принадлежит Q. Следующий pop может выдать `B` второму владельцу.

Ни одна из двух safety-клауз push не нарушена. Более того, precision sub-clause прямо разрешает re-push при наличии stale observer. Значит, фраза «accepting residual risk» не переносит ответственность на caller: это не проверяемая precondition, которую caller нарушил, а допускаемая алгоритмом execution. Большой 48-битный минимум делает сценарий практически редким, но не устраняет его из модели Rust soundness. `AtomicU128` лишь отодвинет, а не исправит конечный wrap.

Почему это блокер именно для заявленного продукта:

- сам крейт не разыменовывает индекс и внутри себя немедленного Rust UB не создаёт;
- но его unsafe trait и push docs прямо классифицируют exclusive issuance как soundness promise для allocators;
- после одного корректного unsafe setup/re-push дальнейший safe `pop` может нарушить это обещание;
- root `Registry` имеет дополнительный slot-state CAS, который ограничивает конкретный downstream эффект, но публичный `StackStorage` предназначен и для сторонних allocator implementations, которым такая защита не навязана типом или контрактом.

Рекомендуемое решение без вероятностного компромисса:

1. Запретить повторное использование извлечённого индекса, пока любой popper может удерживать его старый snapshot: hazard pointers, epoch/quiescence или caller-provided announcement slots должны быть частью алгоритма/типа, а не внешней необязательной рекомендацией.
2. Если нужен минимальный промежуточный дизайн, сделать wrap наблюдаемым терминальным состоянием: не публиковать очередной push при исчерпании тега и вернуть ошибку/остановить структуру. Это сохраняет correctness, но жертвует бесконечной доступностью и потому хуже полноценной reclamation-схемы.
3. Вариант «оставить конечный tag и принять риск» допустим только после честного понижения позиционирования: не обещать allocator-grade exclusive issuance. Если такой primitive всё же используется для memory safety, safety contract должен содержать конкретное обязательство «ни один pop не остаётся in-flight через полный tag cycle», а downstream обязан уметь его действительно гарантировать. Текущий двухклаузный контракт этого не делает.
4. Добавить tiny-tag model/counterexample, где wrap достигается за несколько шагов, а затем использовать его как regression oracle для выбранного исправления. Существующие loom-модели покрывают короткий ABA и H-2 reset, но не доказательство поведения на настоящей границе wrap.

## Существенные исправления до публикации

### P2-1. Три unsafe operation скрыты ambient-правилом `unsafe fn`; «self-verifying inventory» этого не видит

Места:

- `crates/tagged-index-stack/src/imp.rs:1323-1376` — `push_index_impl` вызывает unsafe `SealedStorage::store_next` без локального `unsafe {}`;
- `crates/tagged-index-stack/src/imp.rs:1513-1518` — blanket `StackOps::push_index` вызывает unsafe `push_index_impl` без локального блока;
- `crates/tagged-index-stack/src/imp.rs:1655-1661` — `ArrayIndexStack::push` делает то же;
- `crates/tagged-index-stack/src/lib.rs:245-303,345-365` — документация считает восемь `allow`-регионов и называет это точным самопроверяемым unsafe inventory.

На edition 2021 тело `unsafe fn` пока даёт ambient permission, поэтому код компилируется. Но safety-аудит видит локальные блоки только у трёх bridge hook calls, хотя ещё три реальные unsafe операции спрятаны в телах unsafe функций. В крейте отсутствует `#![deny(unsafe_op_in_unsafe_fn)]`.

Утверждение, что grep восьми `#[allow(unsafe_code)]` эквивалентен точному подсчёту unsafe sites, неверно: один item-scoped allow на `impl`/`fn` может покрыть сколько угодно новых unsafe declarations и operations. Текущее состояние само является контрпримером — allow count остаётся восемь при трёх неучтённых unsafe calls. `#![deny(unsafe_code)]` гарантирует лишь, что unsafe не вышел за разрешённые регионы, но не проверяет их содержимое.

Исправление:

- включить `#![deny(unsafe_op_in_unsafe_fn)]`;
- обернуть все три вызова локальными `unsafe {}` с уже почти готовыми соседними `SAFETY`-доказательствами;
- описывать восемь значений как lint-exception regions, а отдельно инвентаризировать unsafe declarations/blocks/operations;
- не называть grep атрибутов точной проверкой содержимого широкого allow-региона.

Это не обнаруженная текущая UB сама по себе: forwarding contracts в трёх местах выглядят согласованными. Это обязательное укрепление audit boundary и защита от тихого расширения unsafe surface.

### P2-2. A/B wall-clock harness не мигрирован после превращения `push` в `unsafe fn`

Места:

- `crates/tagged-index-stack/scripts/tis_p3_ab/harness_bin.rs:19-21` — шаблон заявляет «100% safe code» и включает `#![deny(unsafe_code)]`;
- `crates/tagged-index-stack/scripts/tis_p3_ab/harness_bin.rs:90-94,124-132,142-149` — три bare `stack.push(...)`;
- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:665-715` — wall-clock mode материализует этот шаблон и выполняет `cargo build --release`;
- `.github/workflows/ci.yml:3109-3167` и `CHANGELOG.md:170-180` — harness остаётся документированным workflow/manual gate.

После `3e83b1c` эти вызовы требуют unsafe context. Scratch crate копирует актуальные `lib.rs`/`imp.rs`, поэтому wall-clock mode остановится на E0133 до измерений. Codegen leg использует другой wrapper и из этого факта автоматически не следует, что он сломан; дефект доказан для wall-clock leg.

Исправление: обновить три вызова с локальными SAFETY proofs, убрать ложное «100% safe code»/несовместимый crate-level deny либо ограничить allow ровно helper-функцией, затем добавить статическую сборку harness в обычный gate, чтобы будущий API-break не оставлял редко запускаемый workflow красным.

## Неточности, smell и нейрослоп

### P3-1. Документация продолжает переобещать машинную проверку unsafe-контрактов

`README.md:133-140`, `src/imp.rs:581-592,1082-1087` и несколько compile-fail комментариев называют safety contract «compiler-checked». Компилятор проверяет только наличие unsafe context (E0133), но не link domain, liveness, единственность binding или отсутствие full-wrap execution. Рядом это частично оговорено как «not runtime-checked», однако заголовок всё равно создаёт неверную модель.

Точная формулировка: compiler-enforced unsafe boundary / compiler-enforced acknowledgement; семантические preconditions проверяет автор unsafe call вручную.

### P3-2. В одном canonical hazard appendix осталась внутренняя противоположность clause 1

`src/imp.rs:497-502` нормативно запрещает rebinding head к другому link storage across time. Но `src/imp.rs:813-831` про temporal rebinding утверждает, что clause 1 «holds as stated» и только её headline покрывает shape «in spirit». После последнего уточнения clause 1 это уже неверно: body прямо содержит `never rebound to different link storage across time`.

Нужно удалить старую оговорку и сказать прямо: shape 4 нарушает clause 1. Такая рассинхронизация показывает, что 466-строчный rustdoc одного trait всё ещё слишком велик для надёжного сопровождения, несмотря на полезное сокращение в `f35e4fb`.

### P3-3. Репозиторный unsafe inventory и один тестовый oracle содержат stale/overclaim

- Root `Cargo.toml:51-56` всё ещё маркирует `tagged-index-stack` как `#![forbid(unsafe_code)]`, хотя актуальный crate root — `deny` с восемью allow-регионами.
- `tests/narrow_domain_unchecked_storage.rs:7-35` говорит, что positive in-domain Miri run заставит «any contract-chain breakage» проявиться как OOB. Тест ни разу не передаёт out-of-domain индекс и не может доказать это общее утверждение; он хорошо доказывает только то, что конкретные contract-compliant in-domain cycles не выходят за восемь ячеек. E0133 surface отдельно проверяет compile-fail fixture.
- `tests/stack_unit.rs:393-418` честно объявляет `AlwaysInvalidStorage` нарушителем clause 4, но SAFETY-комментарий push затем говорит, что у него «no link domain bound to breach». По актуальной clause 6 отсутствие объявленного домена/ячейки — не доказательство безопасности, а ещё одна намеренная некорректность fixture.

Исправление — сузить заявления до реально проверяемых свойств и явно пометить unsafe calls в отрицательных fixtures как намеренные нарушения соответствующих clause, а не как sound calls.

### P3-4. Public rustdoc всё ещё перегружен хрупкими census-утверждениями

Текущий `src/imp.rs` — около 2 тысяч физических строк; документация `StackStorage` занимает примерно `474-939`, то есть около 466 строк. В неё входят нормативные clauses, ordering proof, четыре hazard shapes, completeness census публичных путей, сведения о конкретных тестах и ссылки на внутренние ADR. Последний коммит действительно удалил заметную часть review archaeology, но объём всё ещё затрудняет поиск обязательных условий и уже породил P3-2.

Оставить в trait rustdoc:

- короткий нормативный `# Safety`;
- точный ordering contract;
- компактную таблицу hazard/detection boundary;
- ссылки на отдельный design/audit guide для доказательств полноты, dated census и исторических контрпримеров.

README/CHANGELOG также не должны вручную повторять точные unsafe counts без единого генерируемого источника.

## Производительность и возможности ускорения

Нового бесспорного production-hot-path ускорения, которое безопасно рекомендовать немедленно, статическое чтение не выявило.

- Push использует `Relaxed` initial head load и `Release` CAS; это минимально для текущего publication proof.
- Pop использует `Acquire` head/failure/success. Success-CAS теоретически может быть `Relaxed`, потому что matching head уже был Acquire-loaded до `load_next`; сам CHANGELOG признаёт кандидат unmeasured. Проверять его следует только после P1-1 и на native weak-memory target.
- Link-cell `Acquire`/`Release` сильнее минимально необходимого при неизменном head publication proof. На AArch64 уже виден instruction-level delta, но нет native wall-clock решения; ослаблять ordering по статической догадке нельзя. Сначала следует починить P2-2, затем получить измерение.
- Strong/weak CAS на проверенном toolchain был codegen-identical; оснований менять его сейчас нет.
- Backoff cap измерен как throughput/fairness trade-off; default instrumentation правильно feature-gated и не попадает в обычный hot path.
- Dense `ArrayLinks` может давать false sharing, но универсальный padding ухудшит footprint. Открытый `StackStorage` уже позволяет workload-specific padded/sharded layout.

После устранения full-wrap correctness проблема может изменить сам алгоритм и стоимость операций; преждевременная шлифовка CAS orderings до этого даст мало ценности.

## Обзор последних правок

### Сделано хорошо

- `3e83b1c`: `StackOps::push_index` и `ArrayIndexStack::push` стали `unsafe fn` с двумя конкретными обязанностями. Это корректно закрывает прошлый safe out-of-domain path для unchecked implementor и safe double-push.
- Добавлен отдельный compile-fail fixture, проверяющий оба push entry points, и positive narrow-domain unchecked implementation для Miri.
- Все найденные актуальные production/test/bench/example callers, кроме P2-2 template, мигрированы на unsafe calls; у root `Registry` приведено развёрнутое domain/liveness доказательство.
- `f46211f` свёл CHANGELOG к одной актуальной unsafe-архитектуре и исправил overclaim pop guard; `f35e4fb` заметно сократил review archaeology.
- `c56b711` исправил неверное описание latency bracket и добавил baseline, не вычитая его слепо из tail percentiles.
- `TaggedIndex::pack` остаётся checked, truncating helper приватен; ширина `1..=16`, sentinel и H-2 согласованы.
- Все записи head идут через RMW; release-sequence инвариант не нарушен. Link storage выделен отдельно и атомарен.
- Обычная сборка остаётся `no_std`, allocation-free и без normal third-party dependencies; test counters и raw probes gated.

### Что правки не закрыли

- Caller-side unsafe boundary назначила ответственность за domain/liveness, но не добавила необходимую full-wrap/quiescence precondition и не сделала finite tag строгой ABA-защитой.
- Механический unsafe inventory считает allow-регионы, а не unsafe operations.
- Редко запускаемый A/B wall-clock template пропущен массовой миграцией call sites.
- Сжатие документации оставило несколько старых абсолютных формулировок и внутреннюю противоположность clause 1.

## Unsafe/system census

В production `crates/tagged-index-stack/src` статически обнаружены:

- один `unsafe trait`;
- десять `unsafe fn` declarations;
- восемь `#[allow(unsafe_code)]` regions;
- три явных unsafe blocks в `SealedStorage` bridge;
- три дополнительные unsafe calls, выполняемые неявно внутри `unsafe fn` bodies — P2-1;
- нет raw pointers, pointer arithmetic/dereference, FFI, manual `Send`/`Sync`, async и crypto;
- внешний production `unsafe impl` — root `Registry`, прочитан отдельно.

Главный soundness-риск находится не в pointer syntax, а в семантическом обещании exclusive issuance поверх конечного wrapping tag.

## Короткий путь к GO

1. Выбрать строгую модель P1-1: reclamation/quiescence в протоколе либо terminal no-wrap behavior; затем обновить allocator-soundness contract.
2. Добавить tiny-tag wrap counterexample и положительный oracle выбранного решения.
3. Включить `deny(unsafe_op_in_unsafe_fn)`, локализовать три implicit unsafe calls и исправить inventory wording.
4. Мигрировать и статически прикрыть A/B wall-clock harness.
5. Устранить противоречия/overclaims P3-1..P3-4 и сократить normative rustdoc.
6. После изменения алгоритма заново проверить atomic proof, loom model, Miri/custom storage, root Registry и только затем возобновлять weak-memory performance A/B.

До закрытия пункта 1 публикацию не рекомендую.
