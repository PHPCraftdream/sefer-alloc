# tagged-index-stack — предрелизное статическое ревью

**Ревьюер:** Sol-codex

**Раунд:** Codex run 18

**Метка пользователя:** 2026-09-03 21:59

**Время фиксации ревью:** 2026-09-03 22:10:17 +02:00

**Проверенная ревизия:** `0310fdb8383e8f5934005dced204a5b400220225`

**Диапазон новых правок после моего раунда 17:**
`08567e707f03127d7e138043ddbf9c3d4c7f071a..0310fdb8383e8f5934005dced204a5b400220225`

## Вердикт

**NO-GO.** Новые коммиты исправили основные группы замечаний раунда 17, но новый полный
проход нашёл два release blocker:

1. нормативный `unsafe trait StackStorage` требует, чтобы `load_next` видел «самый последний»
   `store_next`, и утверждает, что это гарантирует Acquire/Release. В разрешённом stale-popper
   исполнении это невозможно: атомарный load вправе видеть публикационную запись своего старого
   head или более новую, но не обязан видеть глобально последнюю конкурентную запись;
2. `freshDir()` проверяет только лексическую вложенность и затем рекурсивно удаляет путь. Junction
   или symlink на уровне самого `target/tis_p3_ab` проходит проверку и перенаправляет удаление в
   существующее внешнее дерево.

Новой ошибки в самом packed-head Treiber-алгоритме при соблюдении его фактически необходимого
контракта я не нашёл. Seal до побочных эффектов, невращающийся tag, H-2, lazy links, retry overwrite,
release sequence, CAS orderings и ownership transfer в точке успешного CAS согласованы. Но
публиковать concurrency-крейт с невыполнимой нормативной safety-клаузой и поставляемым destructive
tool, не закрытым от filesystem redirection, нельзя.

Кроме блокеров найдены: Loom-тест, не доказывающий заявленное перекрытие до физического возврата;
предсказуемые и рекурсивно удаляемые временные каталоги в новом regression test; систематическая
ошибка порядка вариантов в A/B runner; одна оставшаяся unchecked операция над `Instant`; ложная
причинная формулировка в workspace consumer и чрезмерная review-археология в исходниках.

## Режим и охват

Ревью выполнено лично, в одном контексте, без под-агентов. Это новый статический проход по текущему
дереву. Просмотрены:

- весь production source `src/lib.rs` и `src/imp.rs`, публичные типы/методы, unsafe trait/API;
- packed arithmetic, sentinel/tag границы, push/pop CAS-циклы, backoff, error/panic paths;
- memory-ordering и release-sequence доказательства, stale-popper и ownership-epoch сценарии;
- manifest, cfg/features/dependencies, package-поверхность, README и CHANGELOG;
- состав unit/property/compile-fail/threaded/Loom тестов, benchmark и example;
- A/B runner, codegen/wall-clock templates, perf report и относящиеся к ним CI gates;
- production consumer `sefer-alloc::Registry`, его loom shim и все production `push_index` call sites;
- полный diff семи коммитов после раунда 17.

В production source нет async, FFI, raw-pointer dereference, manual `Send`/`Sync`, crypto или
ресурсного `Drop`-протокола; эти классы проверены поиском и не образуют отдельной поверхности.

По требованию пользователя ничего исполняющего код не запускалось: не запускались `cargo`, `rustc`,
fmt, clippy, rustdoc, тесты, Loom, Miri, benchmarks, examples, Node-скрипты, package/publish или
сгенерированные binaries. Использовались только чтение файлов, `rg`, Git inspection и статический
`git diff --check`; последний не нашёл whitespace-ошибок в новом диапазоне. Ранее записанные зелёные
результаты и perf-цифры этим раундом не переподтверждались.

## Блокеры

### P1-1 — safety-контракт требует невозможного «most recent store»

**Места:**

- `crates/tagged-index-stack/src/imp.rs:689-694`;
- `crates/tagged-index-stack/src/imp.rs:734-769`;
- `crates/tagged-index-stack/src/imp.rs:839-842`.

Клауза 2 `unsafe trait StackStorage` требует, чтобы `load_next` наблюдал «the most recent
store_next the stack itself performed». Ниже это повторено ещё сильнее: mapping и coherence якобы
«both guaranteed by the Acquire/Release ordering contract». Это неверно для штатного сценария,
который сам крейт подробно разрешает:

1. push P0 пишет `next[i] = x`, затем Release-CAS публикует head `(i, t)`;
2. stale popper A Acquire-читает `(i, t)` и останавливается до `load_next(i)`;
3. B снимает `i`, получает новый ownership epoch и позднее re-push-ит `i`, записав
   `next[i] = y`, затем публикует новый tag;
4. A продолжает `load_next(i)`.

Между вторым `store_next(i, y)` и load потока A нет happens-before. Atomic coherence разрешает A
прочитать старую публикационную запись `x`; Acquire не означает «верни последнюю wall-clock
запись». Это безопасно: head expectation A содержит старый tag, его CAS обязательно проиграет и
прочитанный link не попадёт в head. Но буквальную клаузу 2 не способен гарантировать ни
`ArrayLinks`, ни `Registry`, ни любой другой корректный lock-free implementor. Тем самым каждый
нормальный `unsafe impl StackStorage` формально нарушает заявленный soundness contract.

Реальная необходимая гарантия слабее и точнее: mapping стабилен, ячейка dedicated+atomic; после
Acquire-наблюдения head, опубликованного конкретным Release push, последующий load того link не
может наблюдать запись, предшествующую `store_next` этого push, но может наблюдать именно её или
любую более позднюю запись в modification order. Если это более поздняя lifecycle-запись, tag
делает CAS старого popper-а неуспешным.

**Исправление:** заменить обе формулировки «most recent» на этот publication-relative lower bound.
Отдельно решить, действительно ли link-level Acquire/Release должен быть нормативной обязанностью:
сам алгоритм уже несёт нужный HB через Release head CAS → Acquire head observation, поэтому
Relaxed link cells достаточны. Если defence-in-depth сохраняется, её нельзя описывать как гарантию
глобально последней записи.

### P1-2 — `freshDir()` всё ещё удаляет через symlink/junction в родительском пути

**Места:**

- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:67-76`;
- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:278-310`;
- `crates/tagged-index-stack/tests/tis_p3_ab_runner_scratch_guard.rs:332-397`.

Удаление произвольного `--out-dir` правильно убрано, а `.`/`..` и separators в target правильно
запрещены. Но `path.resolve` и `path.relative` работают лексически: они не канонизируют существующие
компоненты и не проверяют reparse points. После проверки выполняется безусловный:

```text
fs.rmSync(dir, { recursive: true, force: true });
```

Если `<repo>/target/tis_p3_ab` — symlink/junction на `D:\victim`, то путь
`<repo>/target/tis_p3_ab/build-check` лексически находится строго внутри `SCRATCH_ROOT`, но операция
удаляет реальный `D:\victim\build-check`. То же относится к target triple в codegen/wallclock.
Комментарий строк 299-302 рассматривает только junction на `target/` и без доказательства называет
перенаправленное `tis_p3_ab` дерево runner-owned; собственный `SCRATCH_ROOT` как reparse point не
проверяется вообще.

Новый symlink test это не закрепляет: он передаёт symlink в уже удалённую CLI-опцию `--out-dir`,
поэтому parser падает до первого filesystem access. Тестов, где сам `SCRATCH_ROOT` или его
существующий ancestor перенаправлен, нет.

**Исправление без компромиссов:** не рекурсивно очищать предсказуемый reusable каталог. Создавать
новый уникальный child через `mkdtemp`/эквивалент, работать только в нём и удалять только объект,
эксклюзивно созданный этим процессом. Если стабильный путь обязателен, нужны `lstat` каждого
существующего компонента, отказ на symlink/junction/reparse point, канонический containment check,
runner-owned marker с проверкой перед delete и защита от TOCTOU. Regression должен строить
disposable repo с `target/tis_p3_ab` symlink/junction на victim+canary и вызывать реальный
`build-check`/scratch path.

## Существенные проблемы качества доказательств и tooling

### P2-1 — положительный Loom-тест не доказывает overlap до возврата A

**Места:**

- `crates/tagged-index-stack/tests/loom_aba.rs:166-178`;
- `crates/tagged-index-stack/tests/loom_aba.rs:1102-1140`;
- `crates/tagged-index-stack/tests/loom_aba.rs:1144-1199`;
- ссылки из `crates/tagged-index-stack/src/imp.rs:1171-1180`.

`pop_repush_overlaps_unreturned_push_conserves` запускает обычный `stack_a.push(0)` и параллельный
pop→repush. Флаг доказывает только, что хотя бы в одном explored execution B увидел индекс,
опубликованный A. После успешного CAS функция A больше не выполняет shared-memory operation, и в
тесте нет gate/event между CAS и return. Поэтому Loom не может отличить:

- B pop→repush между CAS и физическим return A;
- B pop→repush сразу после return A.

Оба исполнения имеют одинаковый наблюдаемый partial order. Conservation assertion полезен как
проверка «publish → pop → repush», но имя, заголовок и ссылки из нормативного `# Safety` называют
его доказательством именно `unreturned push` overlap. Activation flag этого не доказывает.

**Исправление:** либо честно переименовать тест и убрать claim о физическом overlap (для алгоритма
return действительно несущественен), либо добавить loom-only post-success-CAS hook/gate и удержать
A внутри `push` до завершения B. Второй вариант нужен, если тест должен именно закреплять
формулировку контракта, а не только эквивалентное состояние стека.

### P2-2 — scratch-guard tests используют предсказуемые каталоги и destructive `Drop`

**Места:**

- `crates/tagged-index-stack/tests/tis_p3_ab_runner_scratch_guard.rs:39-63`;
- `crates/tagged-index-stack/tests/tis_p3_ab_runner_scratch_guard.rs:103-118`;
- `crates/tagged-index-stack/tests/tis_p3_ab_runner_scratch_guard.rs:259-267,339-346`.

Имя temp root состоит из process-local счётчика, начинающегося с нуля, и известного test label; PID,
случайности и атомарного exclusive-create нет. `create_dir_all` принимает уже существующий объект,
а `DirGuard::drop` затем делает `remove_dir_all` этого пути.

Два одновременных `cargo test` процесса используют одинаковые имена, могут перезаписывать skeleton
друг друга и удалять дерево ещё работающего теста. В shared temp directory заранее созданный
каталог/symlink также заставляет test записывать и запускать `git init`/Node не в эксклюзивно
принадлежащем ему дереве. Получился тот же класс, от которого тест пытается защищать runner.

**Исправление:** `tempfile::TempDir` либо std-only retry loop с непредсказуемым nonce и атомарным
`create_dir` (ошибка `AlreadyExists` означает выбрать другое имя, не принять существующее). Guard
должен удалять только root, эксклюзивное создание которого этим процессом доказано.

### P2-3 — wall-clock A/B выполняет варианты блоками, а не сбалансированно

**Место:** `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:853-914`.

Цикл выполняет все samples `base`, затем все `links_relaxed`, затем все `cas_weak`. Для будущего
native-arm64 решения это систематический confound: прогрев машины, DVFS, thermal throttling и фоновой
шум коррелируют с вариантом. Median внутри каждого блока не устраняет drift между блоками, а три
samples по одной секунде делают его особенно заметным. Симметричность workload не делает порядок
вариантов direction-neutral.

**Исправление:** внешний цикл по sample, внутри — детерминированно вращаемый balanced order, например
`base→relaxed→weak`, `relaxed→weak→base`, `weak→base→relaxed`; для большего числа samples повторять
цикл или использовать заранее записанный seed. Сохранять фактический order в raw log/CSV и сравнивать
paired samples, а не только три независимых median.

## Неточности и пахнущий код

### P3-1 — deadline preflight не покрывает фактический `Instant + WARMUP`

**Место:** `crates/tagged-index-stack/scripts/tis_p3_ab/harness_bin.rs:339-350`.

До spawn есть checked representability probe, но координатор позднее снова вычисляет
`Instant::now() + WARMUP` обычным `Add`, которое может panic. Практически 200 ms представимы на
поддерживаемых платформах, поэтому это не реальный эксплуатационный blocker, но после правки
«checked deadlines everywhere» оставлять единственный unchecked site нелогично. Использовать
`checked_add(WARMUP).unwrap_or_else(|| die(...))` непосредственно в строке публикации.

### P3-2 — consumer-комментарий приписывает Release CAS публикацию будущей записи

**Места:**

- `src/registry/heap_registry.rs:373-387`;
- `src/registry/heap_registry.rs:478-509`.

Комментарий говорит, что Release `LIVE → FREE` CAS позволяет следующему claimer-у увидеть
`next_free` link, который функция «about to push». Но link store и head publication происходят
после этого state CAS; release не упорядочивает будущие операции назад во времени. Код остаётся
корректным по другой цепочке: `push_free_slot` пишет link, затем Release-CAS-ит stack head, а pop
делает Acquire observation. Исправить следует доказательство, не ordering кода.

### P3-3 — объём review-археологии уже ухудшает проверяемость

Статический подсчёт текущего дерева:

- `src/lib.rs`: 460 строк, из них 440 начинаются как doc/comment;
- `src/imp.rs`: 2288 строк, из них 1788 начинаются как doc/comment;
- `tests/loom_aba.rs`: 1592 строки;
- A/B runner + harness: 1548 строк;
- новый scratch guard: 397 строк.

Unsafe contracts и короткие ordering proofs должны оставаться рядом с кодом. Но номера ревью,
истории прежних формулировок, повторяющиеся inventories, опровержения старых объяснений и
многоабзацные self-justifications следует вынести в ADR/review/perf docs. Это уже не эстетическая
претензия: именно среди большого объёма уверенного текста одновременно выжили невозможная
«most recent» гарантия, ложная публикация future write и Loom overclaim. Нужен один канонический
proof на invariant и короткие ссылки, а не process log внутри API docs и CI YAML.

## Проверка новых коммитов после раунда 17

### `fd9df69` — wrapping narrative и proptest cleanup

Исправлено корректно: `wrapping_add` больше не выдаётся за реальный `u64` overflow, а низкоценные
const-дубли удалены. Нового дефекта в этой группе нет.

### `6cd6c40` — ownership transfer на CAS

Нормативная клауза теперь правильно ставит transfer authority в successful CAS, сохраняет authority
при `TagExhausted` и разрешает новый epoch после successful pop. Это закрывает P1-2 раунда 17.
Добавленный positive regression проверяет conservation, но не заявленное «до return» перекрытие
(P2-1 настоящего раунда).

### `96eb774` — удаление `--out-dir`

Прямой arbitrary-delete закрыт: пользователь больше не задаёт base path, а `.`/`..` не проходят.
Однако проверка осталась лексической и не закрыла ancestor junction/symlink (P1-2).

### `68bc130` — bounds и timeout

Threads/window/samples ограничены на JS и Rust сторонах; child harness получил конечный timeout;
bounded-overshoot wording исправлен. Это содержательно закрывает P2-1/P3-2 раунда 17. Остались
unchecked 200 ms addition (P3-1) и несбалансированный variant order (P2-3).

### `bccfef6` — полный sweep контракта

Третья ownership клауза теперь проведена через bench, example, tests, root consumer и root docs.
Проверенные production call sites дают свежий index либо именно результат successful pop; явного
повторного потребления одного epoch не найдено. В consumer осталась отдельная неверная фраза о
Release state CAS (P3-2).

### `aaa55e0` — packaged `.crate` path

Optional copy workspace-only identity scripts статически устраняет ложное требование «два уровня
выше package root обязательно workspace». Но добавленный test harness сам небезопасно владеет
предсказуемыми temp paths (P2-2).

### `0310fdb` — checkpoint

Код не меняет; выводы checkpoint не заменяют этот независимый статический проход.

## Общий обзор production-кода

### Representation и арифметика

- `INDEX_BITS` принудительно ограничен `1..=16`; shifts и маски определены на всём диапазоне.
- `u32` index surface, empty sentinel и отдельный `TAIL` согласованы; live index не совпадает с ними.
- Public `pack` checked; private `pack_truncating` достигается после доказанных bounds.
- Push проверяет `TAG_MAX` до link store на первом проходе; retry-side stale link не публикуется и
  будет перезаписан перед будущим успешным push.
- Empty transition сохраняет running tag; reset/wrap пути в production нет.

### Concurrency и memory ordering

- Push: Relaxed head load, link store, Release/Relaxed strong CAS.
- Pop: Acquire head observation, link load, validation, Acquire/Acquire strong CAS.
- Relaxed push failure корректен: returned word используется как `(index, tag)`, link по нему push
  не читает. Acquire pop failure нужен: следующая итерация следует по новому head link.
- Все изменения head — RMW, поэтому plain store не разрывает release sequence.
- Ownership epoch теперь заканчивается на successful push CAS; новый epoch создаёт только
  successful pop CAS. Это композиционно и соответствует фактической линейной точке.
- Backoff bounded и не меняет lock-free progress; starvation честно не обещана.

Единственная критическая несогласованность этой части — не исполняемый CAS loop, а завышенная
`StackStorage` coherence-клауза P1-1.

### Unsafe/API boundary

- `StackStorage` как unsafe trait уместно удерживает невыразимые head↔links и domain obligations.
- Crate-private `SealedStorage` централизует вызовы unsafe hooks; локальные blocks соответствуют
  `deny(unsafe_op_in_unsafe_fn)`.
- Fused `ArrayIndexStack` не реализует публичный `StackStorage` и не отдаёт head, поэтому competing
  binding к его private head из safe downstream code не строится.
- Инвентарь production source совпадает с документацией: восемь item-scoped allow-регионов, один
  unsafe trait, десять unsafe fn, ноль unsafe impl и шесть unsafe blocks.
- Все проверенные legitimate push call sites имеют domain/liveness/epoch justification; тестовые
  намеренные нарушения отделены и названы counterfactual.

### Manifest, portability и CI

- Default library build не имеет сторонних normal dependencies; loom optional и cfg-gated.
- `test-internals` выключен по умолчанию; опасный write probe существует только под `cfg(loom)`.
- `target_has_atomic = "64"` guard соответствует фактическому `AtomicU64` head.
- CI имеет default/release/test-internals, clippy, rustdoc, bare-metal, unsupported-target, Loom,
  package и template build-check поверхности. Новые проблемы — семантические и filesystem-level;
  обычная зелёная матрица их не опровергает.

## Возможности ускорения

Нового безусловного production patch, который стоит вносить без измерения, не найдено. Приоритеты:

1. **Link Acquire/Release → Relaxed.** Доказательство head publication уже делает это корректным;
   x86 codegen идентичен, AArch64 static artifact показывает удаление `ldar/stlr`. После исправления
   P1-1 и сбалансирования runner-а нужен native-arm64 wall-clock run. Если выигрыш устойчив — это
   лучший прямой fast-path кандидат.
2. **Pop CAS success Acquire → Relaxed.** Нужные payload/link writes уже импортированы предшествующим
   Acquire head load или Acquire failure; CAS остаётся RMW и не рвёт release sequence. Нужны отдельный
   Loom counterfactual и codegen/native A/B, прежде чем менять.
3. **Не повторять `store_next`, когда retry изменил только tag, но не head index.** Желаемый link
   численно тот же; условный skip может убрать store при tag-only collision, но branch почти наверняка
   проиграет на обычной смене head. Только отдельный variant+profile.
4. **False sharing.** Не выравнивать все `ArrayLinks` по 64 байта глобально; измерять layout
   конкретного embedder-а и разносить head/горячие links либо менять index mapping там, где профиль
   показывает line ping-pong.
5. **Strong → weak CAS.** На зафиксированном toolchain варианты codegen-identical; менять сейчас
   бессмысленно. Оставить tripwire на смену lowering-а.

Сначала нужно сделать сам измерительный runner безопасным и убрать order bias: иначе именно gate,
принимающий perf-решение, остаётся источником потери данных и систематической ошибки.

## Что исправить до следующего GO-review

1. Переписать `StackStorage` clause 2 из «most recent store» в publication-relative atomic/coherence
   гарантию и синхронизировать весь ordering narrative.
2. Убрать recursive clear предсказуемого scratch tree либо закрыть все symlink/junction ancestors,
   ownership marker и TOCTOU; добавить реальный redirected-`SCRATCH_ROOT` regression.
3. Исправить/переименовать Loom positive test, чтобы его oracle не заявлял непроверяемый pre-return
   overlap.
4. Делать test temp roots эксклюзивно и непредсказуемо, без принятия существующего каталога.
5. Сбалансировать порядок A/B вариантов и записывать его в артефакты.
6. Закрыть оставшийся unchecked `Instant` и неверную state-CAS публикационную формулировку.
7. Сократить process/review archaeology, сохранив только нормативные contracts и минимальные proofs.
8. После исправлений выполнить отдельный динамический gate pass — этот отчёт его намеренно не
   заменяет и ничего не запускал.

## Итог

Семь новых коммитов — реальное улучшение: прежние arbitrary `--out-dir`, wall-clock-lifetime
ownership contract, unbounded harness и неполные call-site proofs исправлены. Алгоритмическое ядро
выглядит готовым. Но новый независимый проход обнаружил более старую фундаментальную ошибку в
публичном unsafe-контракте и незакрытый filesystem-redirection вариант destructive runner-а.
До исправления обоих — **NO-GO**.
