# tagged-index-stack — предрелизное статическое ревью

**Ревьюер:** Sol-codex

**Раунд:** Codex run 17

**Метка пользователя:** 2026-09-03 16:35

**Время фиксации ревью:** 2026-09-03 16:47:40 +02:00

**Проверенная ревизия:** `1ba35de72f04f6d6ab402cb3f0ff1b07737c62f8`

**Диапазон новых правок после моего раунда 16:**
`f48bcaed61b2c419b41721892110b204267d994b..1ba35de72f04f6d6ab402cb3f0ff1b07737c62f8`

## Вердикт

**NO-GO.** В текущем дереве есть два release blocker:

1. поставляемый A/B runner позволяет опцией `--out-dir` выбрать произвольный путь и перед записью
   безусловно рекурсивно удаляет его; `--out-dir .` разрешается в корень репозитория;
2. новая третья клауза unsafe-контракта `push` требует эксклюзивности до физического возврата из
   функции, хотя индекс становится доступен `pop` сразу после успешного CAS. Корректный
   pop→repush другого потока может начаться после линейной точки первого push, но до его возврата,
   то есть обычное допустимое lock-free исполнение нарушает буквальный контракт.

Новой доказанной ошибки самого packed-head Treiber-алгоритма при корректном, линейно понимаемом
владении я не нашёл. Представление, seal до побочного эффекта, невращающийся tag, сохранение tag при
переходе в empty, release sequence и асимметричные CAS orderings выглядят согласованными. Однако
публиковать concurrency-крейт с опасным destructive tool и внутренне противоречивой нормативной
`# Safety` границей нельзя.

Кроме блокеров найдены: принимаемые runner-ом практически бесконечные окна с возможностью
panic+barrier deadlock; ложное Loom-объяснение stale visibility после `join`; неполный sweep
локальных `SAFETY` proofs и workspace-потребителя; остаточный `wrapping_add`-нейрослоп; drift между
CHANGELOG и измерительным протоколом; чрезмерная концентрация review archaeology в нормативном
исходнике.

## Режим и охват

Ревью выполнено лично, без под-агентов. Это новый статический проход по текущему состоянию, а не
копирование предыдущего отчёта. Просмотрены:

- весь production source `src/lib.rs` / `src/imp.rs`, публичные типы и методы, unsafe trait/API;
- packed arithmetic, sentinel/tag границы, push/pop CAS-циклы, retry и error/panic paths;
- manifest, cfg/features/dependencies, package-поверхность, README и CHANGELOG;
- unit/property/compile-fail/threaded/Loom tests, benchmark и example;
- A/B runner и оба Rust-шаблона, относящиеся к нему CI gates и открытые perf-кандидаты;
- единственный production-потребитель `sefer-alloc::Registry` и его loom shim;
- полный diff четырёх коммитов после раунда 16.

В production source отсутствуют async, FFI, raw-pointer dereference, manual `Send`/`Sync`, crypto и
ресурсный `Drop`-протокол; эти классы проверены поиском, но не создают отдельной поверхности аудита.

По требованию пользователя ничего исполняющего код не запускалось: не запускались `cargo`, `rustc`,
fmt, clippy, rustdoc, тесты, Loom, Miri, benchmarks, examples, Node-скрипты, package/publish и
сгенерированные binaries. Выполнялись только чтение файлов, `rg`, Git inspection и статический
`git diff --check`; последний не нашёл whitespace-ошибок в новом диапазоне. Поэтому прежние зелёные
матрицы и perf-цифры в этом раунде не переподтверждались.

## Блокеры

### P1-1 — `--out-dir` позволяет рекурсивно удалить произвольный каталог

**Места:**

- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:103-130`;
- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:213-221`;
- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:334-377`;
- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:688-719`.

CLI принимает `--out-dir` как произвольную строку без валидации. `scratchRoot` преобразует её через
`path.resolve(repoRoot, args.outDir)`, а `freshDir` делает:

```text
fs.rmSync(dir, { recursive: true, force: true });
fs.mkdirSync(dir, { recursive: true });
```

Wall-clock режим вызывает `freshDir(root)` непосредственно. Codegen режим вызывает его для
`froot`, который при default feature set равен тому же `root`. Следовательно:

```text
node .../tis_p3_ab_runner.mjs --mode wallclock --target x86_64-unknown-linux-gnu --out-dir .
```

статически разрешает `root == repoRoot` и начинает с рекурсивного удаления репозитория. `..`,
абсолютный путь или путь к любому существующему каталогу вне dedicated scratch tree дают тот же
класс потери данных. Опция называется просто `out-dir`; пользователь не получает сигнала, что это
разрешение на очистку всего переданного дерева.

Это не только repository-only деталь: runner tracked и лежит внутри package root; manifest исключает
только `tests/compile_fail/`, а root README уже фиксирует, что scripts входят в `.crate`. Комментарий
runner-а на строках 38-41, обещающий запись только под `target/` и `docs/perf/`, при заданном
`--out-dir` также неверен.

**Исправление без компромиссов:** удалить произвольный `--out-dir` либо разрешать только новый
уникальный дочерний каталог внутри канонического `repoRoot/target/tis_p3_ab`. Перед каждым delete:

- канонизировать и проверить containment относительно dedicated scratch root, а не repo root;
- запретить filesystem root, repo root, его родителей и любой путь вне scratch root;
- не очищать существующий пользовательский каталог без marker-файла, созданного этим runner-ом;
- учитывать symlink/junction и Windows case-insensitive path semantics;
- предпочтительно использовать `mkdtemp` и удалять только полученный уникальный child.

Отдельный отрицательный тест resolver-а должен закрепить `.`, `..`, абсолютные пути, sibling,
repo root и symlink escape. CI сейчас вызывает безопасный default path и потому этот дефект не
обнаруживает.

### P1-2 — unsafe-контракт владения привязан к возврату, а алгоритм передаёт индекс в точке CAS

**Места:**

- `crates/tagged-index-stack/src/imp.rs:1092-1168`;
- `crates/tagged-index-stack/src/imp.rs:1598-1599`;
- `crates/tagged-index-stack/src/imp.rs:1681-1682`;
- `crates/tagged-index-stack/README.md:155-163`;
- `crates/tagged-index-stack/CHANGELOG.md:212-223,305-319`;
- `crates/tagged-index-stack/scripts/tis_p3_ab/harness_bin.rs:197-217`.

Клауза 3 требует, чтобы «от invocation until it returns» никакой другой same-index push не
выполнялся и даже не начинался. Передачу authority она помещает «on `Ok(())`», то есть на возврат.
Но фактическая публикация происходит раньше: успешный Release CAS устанавливает index в head, после
чего push немедленно возвращает `Ok(())`. Между этими двумя абстрактными событиями поток может быть
вытеснен.

Допустимое исполнение:

1. A вызывает `push(i)`, успешно публикует `(i, tag + 1)` CAS-ом и останавливается до `return`;
2. B делает safe `pop()`, видит эту публикацию, успешно снимает `i` и получает его из функции;
3. B, обладая единственным индексом, вызывает `push(i)` до того, как A физически вернулся;
4. только затем A возвращает `Ok(())`.

Второй push имеет корректный domain, `i` уже не live и его authority получена единственным
успешным pop. Алгоритм также не повреждается: после successful CAS у A больше нет shared-memory
операций. Тем не менее буквальная клауза 3 объявляет шаг 3 soundness violation, потому что вызов A
ещё активен. У B нет способа узнать, вернулся ли producer того экземпляра, который он уже законно
снял со stack. Значит контракт не композиционен с собственной safe `pop` и не доказуем для обычного
многопоточного pop→repush.

Новые комментарии A/B harness-а, утверждающие, что синхронный repush не может начаться до возврата
другого push, поэтому ложны. Тот же пробел есть в contention benchmark/example/tests; их структура
правильна по ownership epochs, но не удовлетворяет написанному wall-clock-lifetime правилу.

Опасный контрпример из Loom остаётся реальным, но причина должна быть выражена иначе: два потока
дублируют одно исходное право на свежий index и оба начинают публикацию до линейной передачи этого
права stack-у. Запрещать нужно duplicate authority в одном ownership epoch, а не безвредное
перекрытие хвостов функций после intervening successful push+pop.

**Исправление:** перенести ownership transfer на linearization point:

- при входе caller обязан владеть уникальным, не скопированным publish/recycle authority, полученным
  от mint или от конкретного successful pop;
- вызов потребляет это authority;
- при successful head CAS authority переходит stack-у; последующий successful pop может передать
  новый epoch другому caller-у даже до физического возврата предыдущего push;
- при `Err(TagExhausted)` CAS публикации не было, authority остаётся у caller;
- два push, произведённые от одного authority/epoch без intervening successful pop, запрещены.

Лучший breaking design — неподлежащее `Copy` ownership-token API: `pop` возвращает token/lease,
safe repush потребляет его; unsafe остаётся только у создания token для freshly minted index и у
custom storage binding. Если сохранять `u32` API, token semantics всё равно должны быть канонически
записаны, а Loom должен иметь положительный regression на разрешённое перекрытие: A уже
линеаризовал push, B pop-нул и re-push-нул index до выхода A из метода, conservation сохранена.

## Существенные проблемы качества доказательств и tooling

### P2-1 — runner принимает практически бесконечное окно и может повиснуть через worker panic

**Места:**

- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:103-130,695-704,750-768`;
- `crates/tagged-index-stack/scripts/tis_p3_ab/harness_bin.rs:119-135,163-185,228-249,293-311`.

JS-проверки требуют только `Number.isInteger` и нижние границы. Они не требуют
`Number.isSafeInteger`, не ограничивают `windowMs` сверху, не синхронизируют `threads <= 256` с
Rust harness и не ограничивают `samples`. Затем `windowMs` передаётся дочернему процессу строкой.
Rust принимает любой `u64 >= 50` и уже внутри каждого worker вычисляет:

```text
let deadline = timed_start + Duration::from_millis(window_ms);
```

На платформе, где `Instant + Duration` не представим, это panic до `barrier_done.wait()`. Собственный
комментарий harness-а верно предупреждает: worker panic не может пройти через этот barrier и
оставляет координатор в вечном ожидании. На платформе с достаточно широким `Instant` значение всё
равно может означать сотни тысяч или миллионы лет измерения. `spawnSync` runner-а не имеет timeout.
То есть вход считается валидным, но нарушает заявленный fail-fast discipline и может навсегда
занять CI/машину.

**Исправление:** одна одинаковая практическая верхняя граница в JS и Rust; в JS —
`Number.isSafeInteger`, `1 <= threads <= 256`, конечный cap samples; в Rust — checked/hard-capped
duration и вычисление общего deadline до spawn/rendezvous. Runner должен ставить child timeout,
вычисленный из warm-up+window+ограниченного slack, и аварийно завершать весь process group. Ни одна
ошибка worker-а не должна оставлять barrier без участника.

### P2-2 — Loom-комментарий допускает visibility outcome, запрещённый `join` и coherence

**Место:** `crates/tagged-index-stack/tests/loom_aba.rs:950-1095`.

Добавленный per-execution `PUSH_RETRY_COUNT` gate содержательно исправляет замечание раунда 16:
положительная delta в этой двухпоточной fresh-stack модели исключает чисто последовательный
double-push и означает, что оба исходных чтения увидели empty до публикации победителя. Это хорошая
часть изменения.

Но объяснение строк 1067-1085 говорит, что после `ta.join()` и `tb.join()` первый drain-pop может
увидеть head первого push и stale `next[0] = TAIL`, поэтому часть schedules «drains benignly».
Такого outcome здесь нет:

- оба `join` устанавливают happens-before от завершённых действий push-потоков к main;
- на gate-passing schedule последний successful head CAS и последняя запись link — это публикация
  проигравшего retry, установившая `next[0] = 0`;
- atomic write→read coherence не позволяет последующему load main выбрать более раннюю
  модификацию, если более поздняя модификация уже happens-before этому load.

Следовательно, после положительного retry gate первый pop должен читать финальный head и self-loop;
«двух visibility classes» после join нет. Флаг `SAME_INDEX_RETRY_GATE_SEEN` и около сотни строк
объяснения избыточны: если ни один execution не проходит gate или прошедший execution не ловит
self-loop, `model()` завершится без panic и сам `#[should_panic]` уже упадёт. Дополнительно комментарий
флага утверждает, что `MODEL_LOCK` делает его store/assert pair эксклюзивной, хотя `store(false)`
выполнен до входа в `model()`/lock, а post-assert — после выхода; фактическая безопасность держится
на том, что этот флаг использует только один test.

**Исправление:** оставить retry gate, удалить stale-visibility narrative и лишний process-global
флаг либо честно ограничить его роль диагностикой. Сократить proof до причинной цепочки
`retry > 0 -> оба initial reads empty -> loser stores self-loop -> loser publishes final head ->
joins -> drain observes final head/link -> expected panic`.

### P2-3 — новый контракт не проведён через все call-site proofs и workspace consumer

**Места (репрезентативно):**

- `crates/tagged-index-stack/benches/tagged_index_stack_bench.rs:258-260,297-305,412-433,465-496`;
- `crates/tagged-index-stack/examples/backoff_per_call_latency.rs:230-236,274-289`;
- `crates/tagged-index-stack/tests/threaded_conservation.rs:92-97,159-164`;
- `crates/tagged-index-stack/tests/narrow_domain_unchecked_storage.rs:151-165`;
- `crates/tagged-index-stack/README.md:283-293`;
- `src/registry/heap_registry.rs:619-641,788-835`;
- `src/registry/bootstrap.rs:513-535,596-609`;
- `src/lib.rs:167-174`.

Коммит `6aea0ae` добавил третью клаузу в три A/B call site и codegen template, но полный unsafe-call
sweep не сделан. Большинство остальных `unsafe push/push_index` comments по-прежнему перечисляет
только domain+liveness. Production consumer дважды прямо называет контракт «two-clause»; loom shim
повторяет старое описание. Root `src/lib.rs` всё ещё говорит о двух audited unsafe sites крейта,
тогда как сам крейт корректно инвентаризирует восемь allow-регионов, десять unsafe fn и шесть
unsafe blocks.

После исправления P1-2 часть этих коротких proofs станет корректной по сути: «fresh and distinct»
или «just returned by pop and not shared» доказывает уникальный ownership epoch. Но это должно быть
явно проведено через каждый реальный unsafe call; сейчас concurrent A/B формулировки пытаются
доказать невозможное wall-clock требование, а остальные вообще его не упоминают.

**Исправление:** сначала исправить нормативную клаузу, затем механически инвентаризировать все
`unsafe { ...push... }` / `push_index` в crate artifact и единственном consumer; для каждого указать
источник уникального authority и факт его однократного потребления. Синхронизировать root shim/docs.

## Неточности и пахнущий код

### P3-1 — в boundary tests остался ложный `wrapping_add` narrative

**Место:** `crates/tagged-index-stack/tests/stack_unit.rs:176-207,313-336`.

Production уже перешёл на честное `tag + 1` после seal check. Тесты всё ещё вычисляют
`TAG_MAX.wrapping_add(1)` и называют это «wrapping_add past the all-ones tag» / «real bump
arithmetic». При разрешённых ширинах `TAG_MAX <= 2^63-1`; для width 16 это `2^48-1`, поэтому
`wrapping_add(1)` как операция `u64` вообще не wraps. Получается лишь первое значение вне tag field.
Оператор не проверяет и не иллюстрирует тот wrap, который предотвращает seal.

Заменить на обычный `+ 1` и назвать значение «first value outside tag field». Четыре const assertion
в `proptest_pack_unpack.rs:22-37` теперь честно признают узкую область действия, но всё равно
дублируют compile-time свойства фиксированных инстанциаций; это безопасный кандидат на удаление.

### P3-2 — CHANGELOG снова называет измерительное окно точным

**Место:** `crates/tagged-index-stack/CHANGELOG.md:193-203`.

Обновлённый A/B harness честно описывает bounded overshoot и общий denominator до последнего worker.
Bench также документирует overshoot. Но CHANGELOG всё ещё говорит, что contention harness times
каждый worker against one shared exact `[timed_start, deadline)` window. Это сильнее фактического
протокола: ранние workers прекращают numerator раньше общей границы elapsed, а до
`DEADLINE_CHECK_INTERVAL - 1` операций могут попасть за deadline.

Заменить exact interval на shared anchor/deadline target с явно ограниченным overshoot и
last-finisher denominator. Аналогично комментарий runner-а «never modifies any tracked repository
file» неверен: режимы перезаписывают tracked `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE_*` и raw
artifacts. Генератору можно писать tracked evidence, но обещание должно быть правдивым.

### P3-3 — нормативный implementation source остаётся перегружен review archaeology

`src/imp.rs` сейчас содержит 2265 строк, из них статический подсчёт дал 1660 строк, начинающихся с
comment/doc marker, и только около 529 остальных непустых строк. В `src/lib.rs` 460 строк, около 432
comment/doc lines. Последняя Loom-правка добавила 101 строку ради gate, который выражается несколькими
строками, и сразу внесла ложную memory-model историю.

Нужные `# Safety`, ordering и state-transition proofs сокращать нельзя. Но даты раундов, длинные
истории прежних дизайнов, повторные inventories, измерительные журналы и опровержения старых
формулировок должны жить в ADR/review/perf docs. Рядом с hot path нужна одна каноническая причинная
цепочка и короткие ссылки. Текущая плотность текста уже не страхует от ошибок — она их маскирует:
P1-2, P2-2 и stale two-clause consumer формулировки появились при очень большом объёме пояснений.

## Проверка четырёх новых коммитов

### `6aea0ae` — SAFETY proofs и bounded overshoot

- Исправление package/library-target wording в root README корректно.
- Overshoot теперь честно описан непосредственно в A/B harness.
- Exclusive-ownership добавлен только в выбранные шаблоны, не во все unsafe call sites и consumer.
- Добавленные concurrent proofs опираются на некорректное «до возврата никто не начнёт» (P1-2).
- CHANGELOG exact-window формулировка не синхронизирована (P3-2).

### `0087bab` — retry proof, обычное сложение, proptest comments

- Новое двухслучайное объяснение retry overwrite корректно: обычный pop не достигает unpublished
  index; stale pop прошлого epoch имеет навсегда displaced expected благодаря non-wrapping tag.
- `tag + 1` в production выражает доказанный seal precondition лучше прежнего `wrapping_add`.
- Proptest comments теперь честно ограничивают собственный scope; сами assertions малополезны.
- Boundary tests остались со старым `wrapping_add` narrative (P3-1).

### `8c8c019` — Loom retry gate

- Gate действительно исключает sequential double-push и закрывает основное замечание раунда 16.
- Post-join stale visibility reasoning неверно; флаг и большая часть пояснения избыточны (P2-2).
- Контрпример следует перепривязать к duplicate ownership epoch, а не к запрету любого overlap до
  return (P1-2).

### `1ba35de` — stale wrapping comments

- Две ссылки на production `wrapping_add` заменены правильно.
- Сами test expressions и соседнее объяснение всё ещё называют невращающееся `u64`-сложение wrap
  arithmetic, поэтому sweep завершён не полностью (P3-1).

## Общий обзор production-кода

### Representation и арифметика

- `INDEX_BITS` ограничен `1..=16`; tag получает 48–63 бита.
- `INDEX_MASK`, empty sentinel, `TAIL`, shifts и checked public `pack` согласованы на всём диапазоне.
- Private `pack_truncating` вызывается после range/seal proofs; нового silent truncation пути нет.
- Push проверяет `TAG_MAX` до `store_next`, поэтому отказ ничего не публикует; на retry может остаться
  только недостижимая stale link запись.
- Empty transition сохраняет running tag; reset к нулю отсутствует.
- `StackHead` имеет `#[repr(transparent)]`; layout claim теперь обеспечен представлением.

### Concurrency и memory ordering

- Push: Relaxed initial head load, Release link store, Release/Relaxed strong CAS.
- Pop: Acquire head load, Acquire link load, guards, Acquire/Acquire strong CAS.
- Relaxed push failure допустим: failure value используется как числа index/tag, link через него не
  читается. Acquire pop failure обязателен, потому что retry следует по link нового head.
- Все head writes — RMW; release sequence не разрывается plain store.
- Backoff bounded на 64 `spin_loop` за retry, lock-free, но не starvation-free; документация
  признаёт fairness/tail trade-off.
- При исправленной ownership-epoch формулировке новой ABA/conservation ошибки не видно.

### Unsafe/API boundary

- `StackStorage` как unsafe trait уместно переносит head↔links binding и validity обязанности на
  implementor.
- Crate-private `SealedStorage` оставляет единственную точку вызова unsafe hooks; локальные blocks
  существуют под `deny(unsafe_op_in_unsafe_fn)`.
- `ArrayIndexStack` не реализует публичный `StackStorage`, поэтому competing binding к его private
  head/links не строится.
- В production source ровно восемь item-scoped allow-регионов, один unsafe trait, десять unsafe fn,
  ноль unsafe impl и шесть unsafe blocks — текущая crate-local инвентаризация совпадает с кодом.
- Главная проблема поверхности — не расположение unsafe, а неверная temporal contract semantics
  (P1-2).

### Manifest, portability и CI

- Normal build не имеет сторонних dependencies; loom optional и одновременно cfg-gated.
- `test-internals` выключен по умолчанию; test probes отсутствуют в default API.
- `target_has_atomic = "64"` ограничение и invalid-config module gating сформулированы последовательно.
- Package dry-run, packaged tests, clippy/rustdoc/MSRV и template build-check представлены в CI.
- Miri не является главным недостающим доказательством этого крейта: production не разыменовывает
  raw pointers; основной риск — атомарный протокол и семантические unsafe contracts, для которых
  Loom/static proof релевантнее.
- CI не тестирует безопасное разрешение произвольного `--out-dir` и upper-bound входов runner-а.

## Возможности ускорения

Нового безопасно доказанного ускорения, которое следует немедленно вносить перед исправлением
блокеров, не найдено. Текущий fast path уже компактен: один head CAS плюс один link access; проверки
range/self-loop в основном принадлежат cold/contract-violation путям.

После correctness fixes остаются измерительные кандидаты, а не готовые патчи:

1. **Retry `store_next` elision.** На tag-only CAS collision повторный link может совпадать, но guard
   добавит branch/load. Нужен отдельный variant и oracle, различающий unchanged-head-index retries.
2. **Acquire/Release links → Relaxed.** На x86 codegen identity, на AArch64 статически есть
   `ldar/stlr` delta; реальный native-arm64 wall-clock gate ещё нужен до изменения.
3. **Pop CAS success Acquire → Relaxed.** На бумаге matched head уже был Acquire-наблюдён initial
   load/failure path, но отдельного variant/gate нет.
4. **Strong → weak CAS.** На зафиксированном AArch64 toolchain варианты codegen-identical; менять без
   нового toolchain delta бессмысленно.
5. **False sharing ArrayLinks.** Универсальный 64-byte padding увеличит footprint примерно в 16 раз;
   предпочтительнее slot-resident links или consumer-specific mapping только после профиля.

Сначала нужно обезопасить runner: иначе именно инструмент, необходимый для принятия perf-решений,
может уничтожить рабочее дерево или зависнуть. После этого один arm64 dispatch разумно расширить
вариантами для пунктов 1 и 3, чтобы не тратить отдельные hardware runs.

## Что исправить до следующего GO-review

1. Закрыть arbitrary recursive delete из `--out-dir`; закрепить resolver отрицательными тестами.
2. Переписать push safety contract в терминах уникального ownership epoch и успешного CAS как
   linearization/transfer point; обновить Loom narrative и добавить разрешённый overlap regression.
3. Провести новый контракт через все unsafe call-site proofs и production consumer/loom shim.
4. Ограничить runner inputs и исключить worker panic/barrier hang; добавить child timeout.
5. Упростить same-index Loom test, удалив невозможную post-join stale visibility историю.
6. Убрать остаточный `wrapping_add`-нейрослоп и синхронизировать CHANGELOG с bounded overshoot.
7. После исправлений выполнить отдельный динамический gate pass — этот отчёт его намеренно не
   заменяет и ничего не запускал.

## Итог

Алгоритмическое ядро близко к публикационному качеству, и четыре последних коммита исправили большую
часть замечаний раунда 16. Но новый полный проход нашёл более фундаментальную проблему именно там,
где документация объявляет строгую безопасность: transfer authority записан на return вместо
линейной точки, поэтому реальные concurrent consumers не могут доказать буквальную клаузу. Рядом
лежит независимый, безусловный риск потери данных в packaged runner. До устранения обоих — **NO-GO**.
