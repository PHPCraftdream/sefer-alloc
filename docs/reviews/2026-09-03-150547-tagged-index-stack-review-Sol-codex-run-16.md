# tagged-index-stack — предрелизное статическое ревью

**Ревьюер:** Sol-codex

**Раунд:** Codex run 16

**Метка пользователя:** 2026-09-03 14:56

**Время фиксации ревью:** 2026-09-03 15:05:47 +02:00

**Проверенная ревизия:** `fa43abc5f9548175b011903e5af9e6e7b1275b91`

**Диапазон новых правок:** `9b64267c549a2adb70458b72014856cc113b2e6e..fa43abc5f9548175b011903e5af9e6e7b1275b91`

## Вердикт

**NO-GO в текущем виде, но без найденного P1 и без новой доказанной ошибки production-алгоритма.**
Главный soundness-дефект раунда 14 исправлен правильно: публичный unsafe-контракт push теперь
явно требует эксклюзивного временного владения индексом на всём протяжении вызова. Повторная
статическая проверка packed representation, seal-протокола, push/pop CAS-циклов, release sequence,
atomic ordering и forwarding unsafe-границ новой ошибки не выявила.

Перед публикацией остаются два P2-дефекта качества релизного доказательства:

1. новый Loom-контрпример не изолирует нарушение третьей клаузы и может пройти за счёт обычного
   последовательного double-push, который уже запрещён второй клаузой;
2. после добавления третьей клаузы часть локальных `SAFETY`-доказательств и проверяемых деклараций
   осталась двухклаузной, хотя CHANGELOG утверждает обратное; одновременно root README неверно
   называет tracked script-шаблон не входящим в публикуемый crate artifact.

Это не наблюдаемая поломка реализации при соблюдении контракта: все просмотренные реальные вызовы
фактически выглядят обладающими уникальным индексом. Однако для unsafe concurrency crate тест,
объявленный доказательством необходимости контракта, и локальные доказательства этого контракта
являются частью релизной границы. При принятом здесь строгом критерии «без компромиссов» ложные
проверяемые утверждения достаточно существенны, чтобы сначала исправить их и только затем дать GO.

## Методика и границы

Ревью выполнено лично, без под-агентов, как ограниченный single-context pass. Статически просмотрены
текущие `src/lib.rs` и `src/imp.rs`, публичная поверхность, manifest/features/dependencies, README,
CHANGELOG, тестовые targets и compile-fail fixtures, Loom-модели, property tests, benchmark/example,
A/B runner и его шаблоны, релевантные CI/performance-документы, а также весь diff после раунда 14.

Глубоко проверялись unsafe/public API contracts, concurrency и memory ordering, состояния и переходы,
арифметика packed word, sentinel/tag exhaustion, retry/error/panic paths, cfg/feature-поверхность,
test oracle activation/vacuity, package/documentation consistency и hot-path opportunities. В
production-коде отсутствуют async, FFI, raw pointers, ручные `Send`/`Sync`, криптография и
Drop/RAII-протокол ресурсов; эти классы проверены поиском на наличие, но не образуют отдельного
механизма для глубокого аудита.

По требованию пользователя ничего исполняющего код крейта не запускалось: не запускались `cargo`,
`rustc`, clippy, rustdoc, тесты, Loom, Miri, benchmarks, examples, Node-скрипты, package/publish и
сгенерированные binaries. Следовательно, runtime-поведение, фактическая platform/toolchain matrix,
содержимое собранного `.crate` и воспроизводимость прежних измерений в этом проходе не подтверждены.
`git diff --check` диапазона новых изменений выполнен как статическая проверка и замечаний не дал.

## Найденные проблемы

### P2-1 — Loom-контрпример не доказывает именно exclusive-ownership clause

**Место:** `crates/tagged-index-stack/tests/loom_aba.rs:938-1006`,
`counterfactual_same_index_concurrent_push_self_loops`.

Тест запускает два потока с `push(0)`, соединяет их и дренирует stack под `#[should_panic]`. Но между
потоками нет rendezvous/test hook, который заставил бы оба вызова прочитать исходный empty head до
публикации победителя; нет и activation oracle, подтверждающего, что один из push действительно
проиграл CAS и вошёл в retry-ветку.

Loom имеет право исследовать расписание, в котором поток A полностью завершает `push(0)` до начала
полезной работы потока B. Тогда при входе B индекс уже live: это обычный последовательный
double-push и прямое нарушение клаузы 2. Он всё равно формирует `next[0] = 0` и даёт ожидаемую
self-loop panic. Поэтому тест может стать зелёным, не продемонстрировав заявленный сценарий, где
оба вызова удовлетворяют клаузам 1 и 2 при входе, а нарушена только клауза 3. Утверждения в doc comment
на строках 938-950, в локальных `SAFETY`-комментариях 966-981, в `src/imp.rs:1125-1140` и в
`CHANGELOG.md:305-319` сильнее фактического oracle.

`#[should_panic]` исключает только полную вакуумность self-loop: какой-то исследованный execution
паниковал. Оно не устанавливает причину паники и не отличает concurrency counterexample от уже
известного sequential double-push. Пояснение о невозможности обычного `model_with_oracle` верно для
after-snapshot после unwind, но не решает проблему активации нужной ветки.

**Исправление:** сделать требуемое перекрытие структурным. Наиболее прямой вариант — test-only hook
между исходным чтением head и первым CAS плюс rendezvous двух push. Более дешёвый вариант — внутри
каждого model execution снять `push_retry_count_for_test()` до запуска, после обоих `join` продолжать
к drain только при положительной delta, а отсутствие хотя бы одного execution с retry+self-loop
заставлять внешний oracle падать. Важно сохранить невакуумность: тест обязан упасть, если целевая
retry-ветка ни в одном расписании не достигнута. После этого формулировка «обе клаузы entry-time
выполнены» станет проверяемым утверждением, а не предположением о scheduler.

### P2-2 — третья клауза не проведена через локальные SAFETY proofs и package-декларации

**Места:**

- `crates/tagged-index-stack/scripts/tis_p3_ab/harness_bin.rs:131-140,181-192,232-240`;
- `crates/tagged-index-stack/scripts/tis_p3_ab/codegen_wrapper.rs.tmpl:40-48`;
- `crates/tagged-index-stack/src/imp.rs:1429-1435`;
- `crates/tagged-index-stack/src/imp.rs:1041-1049`;
- `crates/tagged-index-stack/CHANGELOG.md:289-303`;
- `README.md:673`;
- `crates/tagged-index-stack/Cargo.toml:17-29`.

Три `unsafe { stack.push(...) }` в wall-clock harness доказывают domain и point-in-time liveness, но
не формулируют новое обязательство: текущий worker обладает исключительным recycle/publish authority
и никакой другой push того же индекса не может начаться до возврата. Codegen template аналогично
доказывает только domain+liveness. Код по структуре, судя по статическому просмотру, контракт
соблюдает: prefill однопоточен и использует уникальные свежие индексы, а worker синхронно repush-ит
индекс, только что единолично полученный из успешного pop. Но именно этот недостающий факт и должен
быть записан рядом с unsafe-вызовом.

Это расходится с `CHANGELOG.md:294-297`, где утверждается, что каждый из трёх harness-комментариев
уже аргументирует все три клаузы. Root `README.md:673` честно перечисляет лишь domain+liveness, но
затем говорит, что шаблон «not part of the published crate». Статически это неверно: файл tracked,
лежит внутри package root, а manifest исключает только `tests/compile_fail/`; отдельного `include`
или исключения `scripts/` нет. Шаблон не входит в library target, но по стандартным правилам Cargo
входит в package artifact. Фактический `cargo package --list` не запускался из-за запрета на
исполнение, поэтому вывод ограничен manifest/VCS-анализом.

В production source тот же drift виден в приватном `push_index_impl`: его `# Safety` сначала верно
ссылается на канонический контракт, но итоговая фраза требует разрядить только link-domain и
liveness, пропуская exclusive ownership. У `StackStorage::store_next` parenthetical на строках
1043-1045 также называет лишь первые две клаузы, хотя следующая фраза уже говорит о трёх. Прямая
ссылка на нормативный контракт сохраняет soundness boundary, но локальное резюме не должно быть
слабее того, что оно объявляет пересказанным «same contract».

**Исправление:** во всех четырёх script/template call sites явно доказать exclusive authority;
дополнить приватные safety summaries; синхронизировать CHANGELOG/root README; заменить «not part of
the published crate» на «not part of the published library target» либо явно исключить scripts из
package, если это намерение. Затем статически инвентаризировать все `unsafe push/push_index` вызовы,
чтобы третья клауза была не только канонической прозой, но и локальным фактом каждого call site.

### P3-1 — retry-overwrite proof приписывает displacement проигравшему push

**Место:** `crates/tagged-index-stack/src/imp.rs:1489-1514`.

Дважды утверждается, что pop, прочитавший stale link неудачной retry-итерации, делал это через head,
который «this push's CAS has already displaced» / «already-displaced head». На описываемом пути CAS
этого push как раз **проиграл** и ничего не displaced. Алгоритм от этого не становится неверным, но
причинная цепочка safety proof записана неточно.

Корректный аргумент состоит из двух разных случаев:

- при соблюдении liveness/exclusive-ownership новый `index` до успешной публикации недостижим, поэтому
  обычный pop вообще не может выбрать его link cell;
- stale popper от предыдущего жизненного цикла может прочитать уже перезаписанный link, но его старое
  expected `(index, tag)` было displaced успешным pop, передавшим ownership вызывающему; из-за
  невращающегося tag это expected больше не переустановится, и CAS stale popper гарантированно
  проиграет.

**Исправление:** заменить обе копии объяснения на эту причинную цепочку и оставить одну нормативную
формулировку с короткой ссылкой из второй позиции. Это снижает риск, что следующая оптимизация retry
store будет опираться на неверно названную release/CAS связь.

### P3-2 — `wrapping_add` назван защитой, которой он не обеспечивает

**Место:** `crates/tagged-index-stack/src/imp.rs:296-310,1523`.

Документация правильно признаёт, что при текущем диапазоне `INDEX_BITS=1..=16` обычный `tag + 1`
никогда не переполняет `u64`: unpacked tag максимум 63-битный. Но затем `wrapping_add` объявляется
defense-in-depth на случай будущего ослабления или удаления seal check. При тех же разрешённых
ширинах удаление seal check всё равно не создаёт арифметического overflow `u64`; оно создаёт
перенос за пределы **tag field**, который последующий shift/pack отбрасывает. `wrapping_add` этот
semantic wrap не обнаруживает и не предотвращает. То есть запас прочности заявлен там, где операция
ничего дополнительно не защищает.

**Исправление:** использовать обычный `tag + 1`, подчёркивающий доказанный precondition, либо оставить
операцию, но убрать вымышленную defense-in-depth мотивацию. Реальной защитой остаётся именно seal
check до side effects, а не вид сложения.

### P3-3 — новые const assertions в proptest не закрепляют заявленную будущую границу

**Место:** `crates/tagged-index-stack/tests/proptest_pack_unpack.rs:22-33,119-126`.

Четыре assertion проверяют `TAG_BITS < 64` только для уже существующих ширин 1, 12, 15 и 16. Если в
будущем общий guard разрешит `INDEX_BITS=0`, эти четыре прежние инстанциации останутся ненулевыми и
продолжат успешно собираться; файл не создаёт `TaggedIndex<0>` и потому не упадёт «at exactly the
shifted widths», как обещает комментарий. Это tautological local check, а не future-regression pin.
Кроме того, текст `panic/UB` неточен: invalid constant shift должен проявиться как compile-time
ошибка/const-eval failure, а не как UB этого теста.

**Исправление:** удалить ложное обещание и избыточные assertions. Если требуется зафиксировать
поддерживаемую границу API, уже имеющийся compile-fail oracle для ширины 0 является правильным
местом; если test strategy станет generic по произвольной ширине, её собственный compile-time
precondition должен быть связан именно с этой шириной.

### P3-4 — A/B harness обещает более точное окно, чем измеряет

**Места:** `crates/tagged-index-stack/scripts/tis_p3_ab/harness_bin.rs:4-11,47-51,226-276`;
эталонная оговорка присутствует в `benches/tagged_index_stack_bench.rs:100-114`.

Исправление раунда 14 действительно сделало warm-up неучитываемым: coordinator публикует будущий
`timed_start`, workers прогреваются до него и проверяют lateness. Однако в counted loop часы
проверяются раз в 64 итерации **после** выполненной порции. Каждый worker поэтому может засчитать до
63 repush после `deadline`; общий denominator заканчивается после последнего worker. Модульная
документация всё ещё говорит, что каждый worker считает только repush «inside the window».

Benchmark честнее описывает это как bounded overshoot и включает его в elapsed, но при разных
моментах завершения worker общий denominator всё равно является максимумом, тогда как ранние workers
перестают добавлять numerator раньше. Для длинных симметричных A/B серий ошибка, вероятно, мала и
направленно нейтральна, но протокол не является точным общим `[timed_start, deadline)`.

**Исправление:** как минимум перенести benchmark-оговорку в harness/отчёт runner и не называть окно
точным. Для строгого измерения проверять deadline перед каждой учитываемой итерацией либо отдельно
возвращать per-worker finish/overshoot и нормализовать согласованно. Не использовать этот пункт как
основание для смены atomic ordering без парного real-arm64 gate и записанной погрешности.

### P3-5 — нормативный source перегружен review archaeology

`src/imp.rs` вырос до 2220 строк, из которых статический подсчёт даёт около 1619 comment/doc lines и
525 непустых code lines. `src/lib.rs` содержит 460 строк, из них около 432 comment/doc lines. Полезные
и обязательные `# Safety`, ordering и state-transition proofs соседствуют с историей конкретных
раундов, commit-era объяснениями, повторяющимися inventory narratives и длинными мотивациями
test-only деталей. Уже в этом раунде drift появился сразу в нескольких копиях одной новой клаузы.

**Улучшение:** не сокращать нормативные contracts и memory-order proofs, а вынести review history,
измерительные журналы и длинные counterfactual narratives в ADR/perf docs; разделить implementation
на приватные тематические модули; оставить у операции одну каноническую причинную цепочку и короткие
ссылки. Это не hot-path ускорение, а снижение вероятности следующей semantic/documentation ошибки.

## Обзор новых коммитов

- `ccb48aa` — unsafe-inventory grep ограничен production `src`; предыдущий P2 закрыт корректно.
- `5f34b9c` — A/B harness получил future window anchor, настоящий uncounted warm-up и lateness guard;
  основная ошибка измерения раунда 14 исправлена. Осталась ограниченная неточность P3-4.
- `3df5a2b` — добавлено независимое R1 review; само по себе production не меняет.
- `f65ee31` — канонический push contract получил exclusive temporal ownership, ownership transfer для
  `Ok`/`Err` и forwarding references. Исправление P1 содержательно правильное; новый Loom oracle и
  часть локальных proofs не доведены до заявленной строгости (P2-1/P2-2).
- `e2275cd` — checkpoint новых решений; production не меняет.
- `6085610` — `StackHead` получил `#[repr(transparent)]`, а stale OPEN_ITEMS line references
  синхронизированы. Оба исправления корректны.
- `fa43abc` — семь замечаний R1 обработаны. Разделение test-only backoff verdict сохраняет
  pre-increment semantics и компилируется из production layout; удаление redundant index mask
  допустимо при текущих доказанных callers; cross-reference CAS ordering полезна. Одновременно
  добавлены неточные retry-overwrite proof, `wrapping_add` rationale и tautological proptest guards,
  описанные в P3-1..P3-3.

## Полный обзор production-кода

### Представление и арифметика

- `INDEX_BITS` принудительно ограничен `1..=16`; tag получает 48–63 бита.
- `INDEX_MASK`, empty sentinel, `TAIL`, shifts и checked `pack` согласованы на всём разрешённом
  диапазоне; invalid shift count в production-пути не найден.
- Публичный `pack` отвергает обе переполненные половины; внутренний fast path вызывается только после
  range/seal proofs. Удаление redundant mask не изменяет корректный путь.
- Push проверяет `TAG_MAX` до `store_next`, поэтому `TagExhausted` не публикует индекс и сохраняет
  authority у caller. Невращающийся tag закрывает stale-CAS ABA.
- `StackHead` теперь `#[repr(transparent)]`; точное layout-утверждение обеспечено представлением.

### Lock-free протокол и ordering

- Push: `Relaxed` head load, `Release` link store, strong CAS с
  `Release`/`Relaxed`, bounded per-call backoff. Push не следует по link из прочитанного head, поэтому
  acquire на его load/failure не требуется.
- Pop: `Acquire` head load, `Acquire` link load, guards на out-of-domain/self-loop, strong CAS с
  `Acquire`/`Acquire`. На retry pop использует индекс из failure value и читает его link, поэтому
  failure ordering не симметричен push и должен сохранять acquire.
- Все записи head выполняются RMW; pop RMW продолжает release sequence от публикующего push. Plain
  store, который разорвал бы доказательство, не найден.
- Backoff cap 6 остаётся измеренным throughput/fairness выбором. Test-only counters и новый verdict
  field отсутствуют в default production cfg.

### Unsafe и публичный API

- Публичный `unsafe trait StackStorage` связывает один стабильный head с link domain и задаёт
  ordering/binding obligations; hooks имеют caller-side unsafe contracts.
- `push_index`/`ArrayIndexStack::push` теперь корректно unsafe и требуют domain, non-liveness и
  exclusive temporal ownership. `pop` остаётся safe: нарушение внешнего ownership при pop может
  логически украсть индекс у caller, но не создаёт memory unsafety внутри корректной абстракции.
- `ArrayIndexStack` не реализует публичный `StackStorage`, поэтому competing binding к его private
  head/links закрыт coherence/private surface. Custom unsafe implementors остаются сознательной
  sharp edge и должны выполнять весь trait contract.
- В production source нет raw-pointer dereference, FFI или manual `Send`/`Sync`. Инвентарь восьми
  item-scoped allow regions согласован с текущим `src`.

### Возможности ускорения

Нового безопасно доказанного ускорения, которое следует немедленно менять перед релизом, не найдено.
Hot path уже состоит из одного packed-head CAS и одного link access; дополнительные проверки в
основном release-active guards или panic paths.

Остаются корректно открытыми измерительные кандидаты, а не готовые патчи:

- elision повторного `store_next` на retry — требует отдельного варианта и oracle, поскольку guard
  сам добавляет работу;
- weakening link-cell Acquire/Release до Relaxed — статически даёт ISA delta на AArch64, но
  real-silicon wall-clock ещё не записан;
- weakening success ordering pop CAS — на бумаге выглядит допустимым, но вариант/gate отсутствует;
- strong→weak CAS — на исследованном toolchain был codegen-null, поэтому основания менять нет.

`ArrayLinks` по-прежнему допускает false sharing 16 соседних `AtomicU32` на типичной 64-byte line.
Blanket padding увеличил бы footprint примерно в 16 раз, поэтому это не универсальное ускорение;
slot-resident links или mapping contended indices следует выбирать только по профилю конкретного
consumer.

## Что исправить до GO

1. Сделать Loom counterfactual невакуумным именно относительно concurrent retry и обновить все
   утверждения «обе entry-time clauses выполнены» согласно фактическому oracle.
2. Провести exclusive-ownership clause через каждый локальный `SAFETY` proof в harness/template и
   через внутренние safety summaries; исправить CHANGELOG и package/library-target формулировку.
3. Желательно до публикации убрать неточные retry-displacement, `wrapping_add` и proptest narratives.
4. Честно описать bounded timing overshoot либо сделать измерительное окно строгим.

После пунктов 1–2 ожидаемый вердикт — **GO при отсутствии новых регрессий**. Пункты 3–4 не указывают
на обнаруженную memory-safety ошибку, но их исправление соответствует заявленной цели убрать
нейрослоп и проверяемые неточности до первой публикации.

## Итог

Ключевая библиотечная архитектура после новых коммитов выглядит sound при опубликованном контракте:
packed head не вращает tag, atomic ordering согласован с release-sequence proof, unsafe boundary
теперь формулирует недостававшее exclusive ownership, а fused type закрывает competing binding.
Текущий стоп-фактор — не production CAS loop, а несоответствие между тем, что репозиторий объявляет
доказанным, и тем, что реально устанавливают Loom oracle и локальные SAFETY-комментарии.
