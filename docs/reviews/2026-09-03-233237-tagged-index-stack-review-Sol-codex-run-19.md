# tagged-index-stack — предрелизное статическое ревью

**Ревьюер:** Sol-codex

**Раунд:** Codex run 19

**Метка пользователя:** 2026-09-03 23:22

**Время фиксации ревью:** 2026-09-03 23:32:37 +02:00

**Проверенная ревизия:** `222e99be92a20a52d431dd26a21530a2f1c59a5f`

**Диапазон новых правок после раунда 18:**
`1f30ac96eee005f09d6cc0261e2b8009935556cf..222e99be92a20a52d431dd26a21530a2f1c59a5f`

## Вердикт

**NO-GO до исправления одного P2; после него — GO по результатам этого статического
прохода.** P0/P1 и новых дефектов production-алгоритма не найдено. Все шесть содержательных
исправлений замечаний раунда 18 в основном корректны, но правка temp-каталогов механически
сломала существующий symlink regression test: тест заранее создаёт каталог на месте будущей
ссылки, создание ссылки неизбежно завершается ошибкой, после чего тест сообщает `skipping` и
возвращает успех. Таким образом, заявленная проверка symlink/junction escape сейчас никогда не
проверяет runner.

Исправленный production runner теперь создаёт эксклюзивный случайный `mkdtemp` root и не принимает
пользовательский `--out-dir`; прежняя возможность рекурсивно удалить чужое дерево статически
закрыта. Невыполнимая safety-клауза заменена на корректную publication-relative нижнюю границу;
Loom overclaim убран; variant order сбалансирован; harness `Instant` и consumer ordering comment
исправлены. После ремонта P2 ниже оснований удерживать публикацию по этому ревью нет. P3 не
блокируют релиз, но их стоит убрать, чтобы доказательная инфраструктура не продолжала накапливать
ложные гарантии и лишнюю сложность.

## Режим и охват

Ревью выполнено лично, без под-агентов, новым статическим проходом по текущему дереву. Просмотрены:

- весь production source `src/lib.rs` и `src/imp.rs`, публичные API и unsafe-контракты;
- packed arithmetic, sentinel/tag bounds, sealing, push/pop CAS loops, backoff и panic/error paths;
- memory ordering, release sequences, stale-popper и ownership-epoch сценарии;
- manifest, cfg/features/dependencies, README, CHANGELOG и package/CI surface;
- unit/property/compile-fail/threaded/Loom tests, benchmark и example;
- A/B runner, его templates, perf report и регулярный/manual CI gates;
- production consumer `sefer-alloc::Registry` и затронутый ordering comment;
- полный diff новых commits после раунда 18.

В production source отсутствуют async, FFI, raw-pointer dereference, manual `Send`/`Sync`, crypto,
serialization и ресурсный `Drop`-протокол; эти классы не образуют отдельной поверхности крейта.

По требованию пользователя код не исполнялся: не запускались `cargo`, `rustc`, fmt, clippy,
rustdoc, тесты, Loom, Miri, benchmarks, examples, Node-скрипты, package/publish и сгенерированные
binaries. Использовались только чтение файлов, `rg`, Git inspection и статический
`git diff --check`; последний не нашёл whitespace-ошибок в новом диапазоне. Ранее записанные
зелёные результаты и perf-цифры этим раундом не перепроверялись.

## Блокирующее замечание

### P2-1 — symlink regression test всегда уходит в `skip`

**Места:**

- `crates/tagged-index-stack/tests/tis_p3_ab_runner_scratch_guard.rs:96-113`;
- `crates/tagged-index-stack/tests/tis_p3_ab_runner_scratch_guard.rs:371-405`.

`exclusive_temp_dir("link")` атомарно создаёт и возвращает существующий каталог. Сразу после этого
тест передаёт тот же существующий путь как `link` в `make_dir_symlink`:

```rust
let link = exclusive_temp_dir("link");
if !make_dir_symlink(link.path(), real.path()) {
    eprintln!("skipping: directory symlinks/junctions unavailable in this environment");
    return;
}
```

И Unix `symlink(target, link)`, и Windows `symlink_dir`, и fallback `mklink /J` требуют, чтобы
destination path не существовал. Поэтому отказ здесь вызван не отсутствием поддержки symlink, а
самой подготовкой fixture. Ветка строк 402-405 превращает этот детерминированный setup defect в
зелёный skip. Assertions, запускающие настоящий runner и проверяющие canary, недостижимы на любой
платформе.

Это регрессия именно коммита `a537c48`: exclusive creation правильно исправила владение temp root,
но прежний тест использовал один и тот же helper и для каталога, и для ещё не существующего имени
ссылки. Новый тест `scratch_root_junction_redirect_leaves_victim_canary_intact` этой ошибки не имеет:
он создаёт ссылку по несуществующему child path `skeleton_target/tis_p3_ab` и действительно может
дойти до runner.

**Исправление:** эксклюзивно создать и guard-ить родительский temp-каталог, а `link` сделать его
несуществующим child path, например `<owned-parent>/link`. Если создание ссылки не удалось, тест
может пропуститься только после подтверждённой OS/privilege ошибки; существующий destination должен
быть setup failure, а не skip. Желательно отдельно assert-ить `!link.exists()` перед созданием и
`link.symlink_metadata().file_type().is_symlink()` после него (для Windows junction — подходящую
проверку reparse point), чтобы counterfactual path был положительно активирован.

## Неблокирующие проблемы качества

### P3-1 — два «оракула» ratio являются тавтологиями

**Места:**

- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:967-970`;
- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:1197-1203`;
- `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md:121-124`.

Обе проверки сначала вычисляют `r`, а затем сравнивают с `r` побитно то же самое выражение:

```javascript
const r = Math.round((med[v] / med.base) * 1000) / 1000;
assert(Math.round((med[v] / med.base) * 1000) / 1000 === r, ...);
```

Такая assert никогда не выявляет ошибку и создаёт ложное впечатление независимого oracle. В
`modeSummary` следующая проверка против записанного `stated` полезна; тавтологичная строка перед ней
не добавляет покрытия. В producer mode либо убрать assert и честно назвать код вычислением, либо
сравнивать независимо сериализованный summary с ratio, заново выведенным из sample rows. Документ
не должен рекламировать «asserted against itself» как гарантию.

### P3-2 — fresh scratch tree всё ещё очищается рекурсивно без необходимости

**Места:**

- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:72-90`;
- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:308-327`;
- successful cleanup: строки `797`, `1002`, `1084`; hard exit: `212-215`.

После `mkdtemp` каждый используемый child path нов и в текущем control flow создаётся один раз.
Тем не менее `freshDir` сначала делает `rmSync(..., recursive: true, force: true)`, затем mkdir.
Комментарий объясняет это тем, что mode вызывает helper для «different children», но разные новые
children не требуют предварительного удаления. Это лишняя destructive primitive и маскировка
ошибки повторного использования пути. Проще и строже: `mkdir` с fail-if-exists, а рекурсивно
удалять только один root, эксклюзивно созданный этой invocation.

Кроме того, `fail()` вызывает `process.exit(1)`, поэтому любой ожидаемый build/oracle failure
обходит три cleanup-site и оставляет весь scratch tree. Это безопаснее прежнего удаления чужого
пути, но повторяющиеся неуспешные gate runs накапливают крупные build trees. Верхнеуровневый
`try/finally` с удалением собственного `scratchBase` закроет lifecycle; для диагностики можно
оставить явный opt-in `--keep-scratch`, а не leak-by-default.

### P3-3 — документация всё ещё называет удалённый фиксированный scratch path

**Места:**

- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:39-41`;
- `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md:58-61`.

После `a537c48` реальные пути имеют форму `target/tis_p3_ab-<mkdtemp>/<target>/...`, но верхний
комментарий runner всё ещё говорит, что build-check пишет в `target/tis_p3_ab/`, а reproduction
report утверждает, что варианты materialize под `target/tis_p3_ab/<target>/<variant>/`. Это не
ломает код, однако противоречит самой security-модели исправления и может направить диагностику к
неиспользуемому дереву. Обновить обе формулировки; историческое упоминание старого fixed path в
негативном regression test, напротив, корректно и должно остаться.

### P3-4 — standalone benchmark сохранил два unchecked `Instant` addition

**Места:**

- `crates/tagged-index-stack/benches/tagged_index_stack_bench.rs:149`;
- `crates/tagged-index-stack/benches/tagged_index_stack_bench.rs:215`.

Коммит `fe7a542` последовательно закрыл bare `Instant::now() + WARMUP` в A/B harness, но sibling
benchmark всё ещё вычисляет обычным `+` и `timed_start`, и `deadline`. При константах 200 ms/1 s
переполнение практически недостижимо, поэтому это не release blocker; всё же код теперь расходится
с заявленным checked-deadline posture и может panic вместо диагностированной ошибки. Общий helper
`checked_add` для обеих точек уберёт расхождение без влияния на измеряемый цикл.

### P3-5 — доказательная поверхность остаётся чрезмерно большой

Текущий crate содержит 2320 строк в `src/imp.rs`, 460 в почти полностью документальном
`src/lib.rs`, 1605 в одном Loom-файле, 1221 в runner и 503 в scratch regression test. Поиск также
находит 117 строк с номерами review/task/group/P-findings в source, tests, scripts, README,
CHANGELOG и manifest. Нормативные safety-клаузы и короткие ordering proofs оправданы; история
итераций, прежние ошибочные формулировки и self-justification должны жить в ADR/review/perf docs.

Риск уже практический: свежий fixture defect спрятался в 503-строчном security test, а два
тавтологических asserts прямо названы oracle. Сокращение текста и декомпозиция тестов улучшат
проверяемость сильнее, чем дополнительные повествовательные комментарии.

## Проверка новых коммитов после раунда 18

### `d61ac24` — publication-relative `StackStorage` contract

Исправлено корректно. После Acquire-наблюдения конкретно опубликованного head `load_next` обязан
видеть publishing store или более позднюю запись в modification order, но не обязан видеть
wall-clock «последнюю». Поздняя pop+repush запись безопасна: старый tag гарантирует проигрыш CAS.
Формулировка синхронизирована с custom/narrow test implementors.

### `a537c48` — per-invocation scratch root и эксклюзивные test temp dirs

Production fix корректен: `mkdtemp` устраняет принятие заранее подложенного фиксированного root, а
последующие invocation не переиспользуют leftovers. Новый redirected-old-root counterfactual имеет
правильную форму. Но механическая замена helper сломала старый symlink regression test — P2-1.

### `fe7a542` — checked `Instant` в A/B harness

Конкретное замечание раунда 18 закрыто правильно. Отдельный аналогичный долг остался в standalone
benchmark (P3-4), а не в исправленном harness.

### `e5f5a7d` — ordering comment в `Registry`

Исправлено корректно: state CAS упорядочивает предшествующие heap writes; `next_free` публикуется
последующим Release CAS stack head и импортируется Acquire pop-ом. Future write больше не
приписывается предыдущему Release.

### `7559aad` — Loom test rename

Исправлено корректно. `pop_repush_after_publish_conserves` заявляет только реально наблюдаемое
publish → pop → repush conservation и честно не обещает различать физический return исходного push.

### `222e99b` — variant order

Исправлено корректно. При default `samples=3` каждый вариант один раз занимает каждую позицию;
реализованный order пишется в log. `--smoke` с одним sample остаётся validation-only, а не
performance evidence, поэтому отсутствие балансировки там приемлемо.

### `6c6ad2`

Коммит относится к root allocator test flake, а не к `tagged-index-stack`; проверен только на
отсутствие пересечения с рассматриваемой crate surface.

## Общий обзор production-кода

- `INDEX_BITS` ограничен `1..=16`; все shifts/masks определены, live index отделён от empty и
  `TAIL`, публичный `pack` проверяет обе половины.
- Tag строго растёт только на successful push. Проверка `TAG_MAX` выполняется до link store, seal
  постоянен, pop сохраняет running tag при переходе в empty (H-2).
- Push использует Relaxed head load, Release link store и Release/Relaxed CAS. Pop использует
  Acquire head/link loads и Acquire/Acquire CAS. Все изменения head являются RMW, release sequence
  не разрывается.
- Relaxed failure у push достаточен: retry не следует по link наблюдаемого head. Acquire failure у
  pop необходим в текущей схеме: следующая итерация следует по link нового head.
- `StackStorage` корректно `unsafe trait`; hooks и caller-facing push имеют compiler-visible unsafe
  boundary. `ArrayIndexStack` не экспортирует свой head и не реализует открытый storage trait,
  поэтому safe downstream code не строит второй binding вокруг fused stack.
- Release-active bounds/self-loop guards закрывают локально обнаружимые corruption shapes; более
  глубокие cycles и shared-cell violations честно отнесены к unsafe contract.
- Default library build `no_std`, без normal third-party dependencies; 64-bit atomic portability
  guard, optional Loom wiring, test-internals gate, MSRV/package/compile-fail/Loom gates статически
  согласованы.

## Возможности ускорения

Без измерения менять hot path сейчас не следует. Приоритеты остаются такими:

1. **Link `Acquire`/`Release` → `Relaxed`.** Head Release/Acquire уже несёт необходимый HB; aarch64
   codegen показывает реальное удаление `ldar/stlr`. После исправления P2-1 runner пригоден для
   native-arm64 wall-clock решения. Этот раунд измерений не запускал.
2. **Pop CAS success `Acquire` → `Relaxed`.** Нужные данные уже импортированы предыдущим Acquire
   head load либо Acquire failure; CAS остаётся RMW. Нужны отдельный Loom counterfactual и A/B.
3. **Не повторять `store_next` при retry, изменившем только tag, но не head index.** Может убрать
   atomic store на tag-only collision, но branch способен проиграть на обычной смене index — только
   отдельный profile/variant.
4. **False sharing лечить у embedder-а.** Не раздувать каждый `ArrayLinks` элемент в 16 раз;
   измерять конкретный layout и при необходимости изолировать head/горячие links или менять mapping.
5. **Strong → weak CAS сейчас не менять.** На зафиксированном toolchain codegen идентичен; tripwire
   уже переоткрывает вопрос при изменении lowering.

## Что исправить перед публикацией

1. Обязательно починить P2-1: создавать symlink/junction по гарантированно несуществующему child
   path и положительно доказывать, что fixture активирован до запуска runner.
2. Статически убедиться, что после правки setup failure не превращается в разрешённый skip.
3. P3 можно делать независимо: удалить тавтологичные asserts, упростить scratch lifecycle,
   синхронизировать paths в документации, закрыть benchmark `Instant` и сокращать review history.
4. После исправления выполнить обычный динамический gate pass отдельным процессом; настоящий отчёт
   намеренно его не запускал и не заменяет.

## Итог

Алгоритмическое ядро и публичный unsafe-контракт выглядят готовыми. Исправления раунда 18 закрыли
оба прежних P1 и остальные содержательные замечания. Текущий единственный blocker — не production
bug, а детерминированно пустой security regression test, который обязан был доказать отсутствие
symlink escape. После его ремонта — **GO по этому статическому ревью**.
