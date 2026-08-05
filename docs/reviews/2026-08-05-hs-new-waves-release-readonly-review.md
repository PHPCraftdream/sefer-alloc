# HS: readonly-аудит новых волн и готовности к релизу — 2026-08-05

## Область и метод

- Последний завершённый релевантный аудит перед этой волной —
  `docs/reviews/2026-08-05-wave3-h1h8-remediation-readonly-review.md`; он
  фиксирует `HEAD = 85dacfc300784cb45ce61c9cfba76dd1a0820870` и диапазон
  `2a7f1e6..85dacfc`. Предшествующий Sol release-аудит проверял
  `40241b0..4623dc3`; его три P1 отдельно переоценены ниже.
- Фактический новый диапазон: **`85dacfc300784cb45ce61c9cfba76dd1a0820870..60ad8474a2d62438eefa628e580f83c132da25fb`**, 9 commits:
  `3d57a26`, `b1a9b7b`, `ba18071`, `7a9b7c7`, `addc63d`, `04c2f74`,
  `650b818`, `782b92e`, `60ad847`.
- Это строгий статический/readonly-аудит. Просмотрены история и diff каждого
  commit, текущие production sources, тестовые cfg, gate/release scripts,
  manifests и release-документация. Ничего не запускалось: ни cargo/build,
  ни test/bench/clippy/fmt/miri/loom/kani/fuzz, ни JS/scripts/binaries.
  Поэтому сохранённые в commit messages результаты — свидетельства авторов,
  но не независимо воспроизведённые этим аудитом результаты.
- До записи этого файла рабочее дерево уже содержало чужие untracked
  `.claude/`, checkpoint, review-файлы и
  `src/global/sefer_alloc.rs.tmp.23920.75bad19dff17`; они не изменялись.

## Краткий verdict

**Текущий `HEAD` — `NO-GO` для немедленного release.** В новых production
diff не найдено нового подтверждённого UB/UAF/double-free/data race/OOB или
metadata corruption: единственные production-source изменения — повышение
compile-time size ceiling (`3d57a26`) и narrowing доступности одного
`dbg_*`-метода (`7a9b7c7`), без изменения allocator algorithm или memory
access protocol.

Но release пока блокируют проверяемые проблемы процесса/verification:

1. `b1a9b7b` заявил закрытие всех no-`internals` test fallout, однако его
   новый scanner даёт ложный PASS на двух реальных файлах. В
   `tests/medium_classes_correctness.rs:7-11` и
   `tests/medium_classes_wide_correctness.rs:8-19` строка с
   `feature = "internals"` находится только в doc comment; настоящий
   crate-level `#![cfg]` её не содержит, хотя файлы вызывают gated методы
   (`dbg_table_count`, `dbg_segment_id_of`, `dbg_small_class_count` и др.).
   Это подтверждённый test/build-configuration defect, подробно F1 ниже.
2. Wave 4 не закрыта для release: новый manifest сам помечен **NOT YET FINAL**
   и содержит только 7 из 9 текущих commits
   (`docs/perf/round-manifests/R34_REMEDIATION_4_MANIFEST.md:10-24,28-34`),
   а CHANGELOG не имеет wave-4 closing entry. Верхняя секция всё ещё
   `## [0.3.0] (unreleased)` (`CHANGELOG.md:8`). Tag publish должен и будет
   остановлен `release.yml:175-212`; manual non-dry dispatch, напротив,
   ошибочно обходит этот guard (`:180,185`).
3. Репозиторный pre-push gate всё ещё имеет известный hard-red:
   `43115cf` и `5c1142f` остаются `fix(perf)` commits с CSV-only diffs.
   `docs/CORRECTNESS_OPEN_ITEMS.md:1195-1220,1277-1285` прямо фиксирует
   status OPEN и воспроизведённый lint failure; новые commits их не
   переписали. Default `@{u}..HEAD` охватывает их, поскольку текущий
   `origin/main` — `42d8d22`, то есть существенно старше обоих commits.
   Direct push/tag workflow может обойти именно этот PR-only lint, но
   объявлять release tree green при красном обязательном local gate нельзя.
4. После исправлений обязателен настоящий clean-tree release gate/package
   dry-run. Этот readonly-аудит по условию не мог его выполнить.

## Что именно изменили новые commits и есть ли новый speedup

| Commit | Реальный эффект | Shipping-speed verdict |
|---|---|---|
| `3d57a26` | `HeapCore` size assert: `8192 -> 9216`, удалён feature-exclusion cfg (`src/registry/heap_core.rs:526-581`); runtime test синхронизирован | **Не ускорение.** Уже существующий 8.4–8.8 KiB layout теперь разрешён сборкой; ни поле, ни алгоритм, ни generated runtime work не изменены. |
| `b1a9b7b` | test `#![cfg]` edits и статический JS checker | Test/tooling-only; production runtime нейтрален. Checker имеет новый false-negative, F1. |
| `ba18071` | перестановка CHANGELOG heading и исправление `9 -> 6 files` | Docs-only. |
| `7a9b7c7` | `SeferAlloc::dbg_trim_current_thread` теперь требует `bench-internals + internals` (`src/global/sefer_alloc.rs:626-628`) | **Не ускорение.** Метод исчезает из обычного downstream API surface; его production body не изменён. Feature-gated bench/test builds сохраняют тот же вызов. |
| `addc63d`, `04c2f74` | item/SHA documentation corrections | Docs-only. |
| `650b818` | динамический count в banner `check-all` | Tooling presentation only; gate commands не изменены. |
| `782b92e` | закрытие wave-3 manifest и начало incomplete wave-4 manifest | Docs-only. |
| `60ad847` | сериализация 6 tests вокруг process-wide trim/release counters | Test-only fix pre-existing flake; assertions и production code не изменены. |

**Итог: новые волны не ускорили shipping production code.** В диапазоне нет
ни causal A/B treatment, ни нового throughput/latency результата, ни даже
runtime algorithm diff. `3d57a26` лишь сохраняет прежний layout, а `7a9b7c7`
— feature/API narrowing. Ранее цитируемые 80.76 vs 249.14 ns/cycle / ~67.6%
относятся к сохранённому R32-8 decay-clock stride: сам CHANGELOG точно говорит,
что R34-11 catch-up body в throughput regime не исполнялся
(`CHANGELOG.md:25`). Это не новый причинный speedup. Аналогично повторная
R34-12 проверка shadow-head подтверждает старый R32 effect, а не создаёт его.

## Findings новой волны

### F1 — P1 verification / P2 release: scanner принимает doc comment за crate cfg и пропускает два broken no-`internals` test targets

Commit: `b1a9b7b05fa81e74ab80e4006002d5d8a8e022d3`.

Подтверждённая цепочка:

1. `scripts/verify-alloc-core-dbg-internals-exhaustive.mjs:253-256` ищет
   первый regex match `#![cfg(...)]` **по всему raw text** и не требует,
   чтобы match был настоящим inner attribute, а не текстом внутри `//!`.
2. Этот же commit изменил
   `tests/medium_classes_correctness.rs:7` и
   `tests/medium_classes_wide_correctness.rs:8`, вставив в doc comment
   буквальный текст ``#![cfg(all(..., feature = "internals"))]``.
3. Настоящие attributes в `tests/medium_classes_correctness.rs:11` и
   `tests/medium_classes_wide_correctness.rs:19` по-прежнему требуют только
   `alloc-core + medium-classes[_wide]`, без `internals`.
4. Оба файла реально называют методы, которые существуют только в
   `#[cfg(feature = "internals")] impl AllocCore`
   (`src/alloc_core/alloc_core_core_diag.rs:97-100`): например
   `medium_classes_correctness.rs:229,256,318` вызывает
   `dbg_table_count`/`dbg_segment_id_of`/`dbg_layout_class_for`, а
   `medium_classes_wide_correctness.rs:123,406` —
   `dbg_small_class_count`/`dbg_segment_id_of`; определения находятся в
   `alloc_core_core_diag.rs:217,320,609,614`.

Следовательно, checker вычисляет `hasInternals = true` по комментарию и
молча пропускает файл, но Rust compiler этот комментарий игнорирует. Команды
без `internals` с `medium-classes`/`medium-classes-wide` статически должны
получать E0599. Это тот же bug class, который commit заявлял закрытым.
Текущие CI medium rows добавляют `internals` (`ci.yml:592,599`), поэтому
обычный CI может оставаться зелёным; дефект ломает честный feature-isolation
contract/developer command и ослабляет oracle, а не shipping library ABI.

Исправление: парсить только реальные leading crate attributes (минимум —
line-anchored `^\s*#!\[cfg`, исключив comment/string context; лучше `syn` или
малый Rust-aware tokenizer), добавить обеим настоящим `#![cfg]`
`feature = "internals"`, затем иметь negative matrix для
`production medium-classes` и `production medium-classes-wide` без
`internals`, которая проверяет либо корректный skip, либо library-only build.

### F2 — P3 tooling: ungated allowlist ошибочно включён в `gatedMethodNames`

Commit: `b1a9b7b`; файл
`scripts/verify-alloc-core-dbg-internals-exhaustive.mjs:243-247`.

`gatedMethodNames` наполняется условием
`f.gated || ALLOWLIST.has(...)`. Но четыре allowlisted метода намеренно
**не gated**: три stats accessor в
`src/alloc_core/alloc_core_core_diag.rs:56-95` и
`AllocCore::dbg_decommit_count` (allowlist declaration checker-а). Поэтому
check 2 может назвать test нарушителем и потребовать `internals`, даже если
он обращается только к стабильному stats-backed accessor через crate-root.
Это подтверждённая логическая ошибка классификатора и возможный false-positive/
лишнее сужение test coverage. По текущему диапазону я не доказываю, что один
из 39 механически изменённых файлов был gated **только** из-за allowlist:
просмотренные изменённые файлы также вызывают действительно gated hooks.
То есть present false-negative F1 подтверждён, а present лишний cfg именно
из-за F2 — гипотеза; дефект checker-а реален независимо.

Исправление: два множества — `actuallyGatedMethodNames` для check 2 и
`acceptedPublicMethodNames` только для source-boundary verdict; allowlist не
должен попадать в первое.

### F3 — P2 release process: wave-4 provenance и release metadata намеренно не завершены

Commit `782b92e` создал
`R34_REMEDIATION_4_MANIFEST.md`, но файл сам говорит `NOT YET FINAL`, ожидает
собственный commit и closing checkpoint/CHANGELOG/review
(`:10-17`). Таблица `:28-34` содержит commits 1–7 и не содержит самого
`782b92e` и следующего test-only fix `60ad847`, хотя фактический диапазон
содержит 9 commits. Это честно
документированное промежуточное состояние, не скрытая фальсификация, но оно
несовместимо с release-ready snapshot.

Sol-F2 теперь **в основном исправлен**: `Cargo.toml:3` и единственная верхняя
секция `CHANGELOG.md:8` согласованы на `0.3.0`; старой ложной датированной
второй секции больше нет. Но финальный release ritual ещё не выполнен:
wave 4 не внесена, а секция помечена `(unreleased)`. Tag workflow требует
ровно одну секцию версии и отвергает `(unreleased)` (`release.yml:190-212`).
Дополнительная process hole: весь guard выполняется только на tag push и
пропущен при manual dispatch (`:180,185`), хотя `cargo publish` в конце может
быть non-dry (`:244-258`). Это позволяет ручной публикации обойти контракт.

Исправление: дописать final 8+ closing rows и wave-4 CHANGELOG summary,
датировать `0.3.0` в release commit, а changelog guard применять ко всякому
non-dry publish; для dry-run разрешать `(unreleased)` явно.

### F4 — P2 process: обязательный local gate остаётся известным red

`43115cf77290875933564040810f7f50707a9b5a` и
`5c1142f5b128ef1134e397abd5139327f3135b86` имеют prefix `fix(perf)`, но
меняют только summary CSV. Точная сохранённая ошибка checker-а находится в
`docs/CORRECTNESS_OPEN_ITEMS.md:1208-1220`, status OPEN — `:1195-1199`,
reopen/до сих пор red — `:1273-1285`. Ни один новый commit не меняет их
messages. Поэтому `scripts/check-all.mjs:220-227` по-прежнему включает
hard-failing `verify-commit-prefixes` на default unpushed range.

Это не allocator safety defect и не tag-workflow compile failure; это
release-process blocker, пока политика проекта называет `npm run check`
обязательным green pre-push gate. Нужны либо разрешённый reword rebase, либо
явный maintainer-approved scope/exception, после чего снова прогоняется gate.

### F5 — P4 docs/tooling: после `clippy x6` исправления осталась внутренняя нумерационная/временная несогласованность

`650b818` правильно заменил runtime banner на
`${clippyRows.length}` (`scripts/check-all.mjs:226-227`). Но header теперь
говорит `2-7` для шести clippy rows, а следующую test step всё ещё нумерует
`7` (верх файла, строки 19–25), то есть номера перекрываются. Там же фраза
«the last two are NEW as of R30-5» после добавления `production internals`
неверна; inline explanation ниже правильно говорит, что этот row добавлен
позже (`:107-114`). Не release blocker, но это очередной симптом ручного
дублирования counts/steps; генерировать нужно всю human summary table, не
только одну цифру banner-а.

### F6 — P3 precision: `--all-features` как «provably global maximum» доказан только при текущей additive cfg/layout структуре и target

`3d57a26` пишет, что `--all-features = 8840` — «TRUE global maximum» и
«no composition can exceed it» (commit body;
`src/registry/heap_core.rs:555-565`; wave-4 manifest `:66-70`). Для
**текущих полей `HeapCore`** это разумно: `src/registry/heap_core.rs:320-524`
использует только additive positive feature cfg для полей, поэтому union
features включает их все. Но это не вечная общая теорема Cargo/Rust:
`cfg(not(feature=...))`, mutually-exclusive representations, target cfg,
alignment/ABI/toolchain изменения могут сделать другую composition/target
крупнее.

Сам unconditional `const assert` (`heap_core.rs:581`) хорош: если иной target
превысит 9216, build честно упадёт. Неверно лишь превращать текущий измеренный
maximum одного layout family в безусловное доказательство навсегда. Формулировка
должна быть: «максимум текущей additive feature-field структуры на измеренном
target/toolchain; unconditional assertion проверяет остальные сборки».

Также повышение 8192→9216 устраняет compile-red, но не снижает first-allocation
stack pressure: `HeapCore` всё ещё строится by value
(`heap_core.rs:528-539`). Это conscious budget change, а не performance fix.

## Переоценка трёх прежних Sol P1

1. **Root-reexport / `AllocCore::dbg_*`: CLOSED по текущему source surface.**
   `AllocCore` действительно остаётся root-reexported без `internals`
   (`src/lib.rs:407-408`), но diagnostic block теперь gated непосредственно
   (`alloc_core_core_diag.rs:97-100`), остальные AllocCore files также
   прошли gating wave; четыре ungated stats-backed exceptions явно отделены.
   `7a9b7c7` дополнительно закрыл оставшийся `SeferAlloc`-уровень:
   `dbg_trim_current_thread` gated в `sefer_alloc.rs:626-628`, остальные
   `SeferAlloc::dbg_*` siblings gated в `:671-675,714-719,756-760,793-797,
   833-837`. Новый дефект F1 — в consumer test oracle, не повторное открытие
   shipping API surface.
2. **Version/CHANGELOG: PARTIALLY CLOSED, final release step OPEN.** Дубликат/
   ложная старая дата устранены, package и единственная section согласованы
   на 0.3.0. Но current snapshot прямо `(unreleased)`, wave 4 не внесена, и
   tag guard поэтому обязан отказать. До release — завершить F3.
3. **No-panic prose: CLOSED.** `Cargo.toml:159-174` теперь аккуратно различает
   ordinary null/no-op failure и пять release invariant tripwires плюс прямой
   alloc-path chunk-OOM `abort`; `src/global/sefer_alloc.rs:43-115`
   документирует тот же contract. Blanket promise больше не является
   blocker. Остаточные unwind semantics ниже остаются hardening items, а не
   противоречием публичного prose.

## Статический safety-аудит

### Подтверждённое в новом диапазоне

- **Memory-safety defects:** не найдено. Production diff не добавляет и не
  меняет raw-pointer dereference, allocation/free/realloc, atomic ordering,
  CAS, index arithmetic или metadata write. Поэтому нет причинной цепочки к
  новому UB, UAF, double-free, double-allocation, race, ABA, OOB, provenance
  violation, integer overflow или corrupt metadata.
- **Build/verification defect:** F1 подтверждён исходным текстом и Rust cfg
  semantics; F2 — подтверждённая логическая ошибка checker-а. Они не означают
  memory corruption в shipped library.
- **Panic behavior:** новый unconditional size assert может вызвать compile
  error, не runtime panic. `dbg_trim_current_thread` body не менялся.

### Существующие residual hypotheses, не внесённые этой волной

| Класс | Статический статус |
|---|---|
| UB/UAF/data race/ABA/OOB/provenance | Нового подтверждённого случая нет. R34 ordering/ownership hardening не затронут текущим диапазоном. |
| Double-free / double-allocation | Не найдено. `DrainHeadPublish` residual ниже может повторно вызвать **reclaim closure** после catch-unwind, но текущие closures не имеют известного post-mutation panic; это не подтверждённый reachable double-free. |
| Leak | Два документированных условных окна: replay in-flight drain item (`remote_free_ring.rs:861-900`) и post-write fallback re-init without Drop (`fallback.rs:375-399`). Оба status OPEN residual/no known trigger (`CORRECTNESS_OPEN_ITEMS.md:1291-1325,1333-1361`). Ring-full policy сознательно допускает bounded leak (`remote_free_ring.rs:845-846`). |
| Livelock | `InitStateGuard` закрывает permanent `INITIALIZING` livelock на unwind (`fallback.rs:353-373,419-428`). Post-write cleanup не структурно закрыт, но известного panic site в окне нет. |
| Rare wrap / corrupt ring metadata | `cached_head` proof явно имеет scheduler/time assumption: producer должен быть preempted между двумя adjacent instructions, пока проходит ~`2^32` drains (`remote_free_ring.rs:203-227`). Теоретически возможен premature slot reuse/lost free entry; практически признан астрономическим, не доказанный exploit и не новый defect. |
| Integer overflow | Новый runtime arithmetic отсутствует. Test-only headroom использует signed subtraction; production size check — const comparison. |
| Panic/abort | Сознательно присутствуют: пять invariant tripwires и registry alloc-path OOM abort. Теперь публичный contract это честно раскрывает (`Cargo.toml:164-174`). |

Для release эти residuals допустимы только при сохранении текущего честного
контракта: GlobalAlloc unwind aborts; проект не обещает catch-unwind
transactionality или scheduler-independent formal proof. Они не blockers
этой волны, но не должны рекламироваться как «полностью формально доказано».

## Где ещё можно сильно ускорить

Приоритеты ниже — не результаты новых commits, а следующий доказательный
pipeline по уже сохранённым gates.

### P0 — уже доступный deployment knob: small-pool Throughput profile

Самый сильный подтверждённый lever уже существует: `(4 segments,16 MiB) ->
(8,32 MiB)` дал около **22% latency win**, но удерживает примерно
**+8 MiB/heap**, линейно до ~255 MiB на 32 heaps
(`docs/perf/OPEN_ITEMS.md:101-108`). Ожидаемый предел для уже измеренного
режима — примерно эти 22%, не «бесплатные» 2x. Продвигать как workload recipe
с RSS budget, не universal default. Adaptive global budget — только при
реальном uneven-pressure victim; иначе прежний CONDITIONAL-GO остаётся
теорией.

### P1 — warm bulk 16/256 B: наибольший измеренный внешний gap

README/gates дают примерно 2.37x/2.71x отставание от mimalloc в warm bulk;
следовательно теоретический верхний предел «догнать mimalloc» — около
**58–63% сокращения latency**, но локальные micro-optimizations почти
исчерпаны. Magazine-hit bitmap/provenance bookkeeping — ~12.19 Ir/hit,
54.5% magazine-hit cost (`OPEN_ITEMS.md:1240-1241`), однако R34-25 palette
design заменяет один AND на load/tag/branch и не снимает correctness-required
clear. Следующий разумный шаг: дешёвый disassembly/cache-profile check, затем
только victim-activated redesign magazine residency/cross-thread free.

### P1 для thread-churn/small-stack consumer — in-place `HeapCore` init

Теперь maximum измерен как 8840 B, а ceiling поднят до 9216; by-value
construction остаётся (`heap_core.rs:528-581`). `new_in_place` для
`HeapCore`/`Tcache` может убрать multi-KiB temporary/copy и реальный stack
risk. Upper bound — один first-bind copy/temporary на thread, то есть выигрыш
может быть сильным только при частом создании threads/heaps, почти нулевым в
steady state. Сначала нужен first-allocation/thread-churn causal judge.

### P2 / trigger-only — geometric realloc page-run/adjacent grow

Корректный `64 B -> 4 MiB` результат — около 210–238 µs, лишь 1.8–2.1x
быстрее mimalloc, не старые ~40x (`R34_23_REALLOC_AND_VEC_GATE.md:28-34,
120-125`). Потенциальный upper bound — устранение последнего ~2 MiB copy,
но `large-reserved-capacity` дал NO-GO, а page-run design не нашёл consumer
с существенным 256 KiB–2 MiB realloc volume
(`R34_26_PAGE_RUN_LAYER_DESIGN_GATE.md:14-35,485-521`). Не release work без
trace/victim.

### Что уже NO-GO и не следует снова «ускорять» локально

- `contains_base`/magazine-overflow immediate region: пять последовательных
  NO-GO/exhausted; standalone `flush_class(8)` = 449 Ir и почти вся работа
  correctness-required (`OPEN_ITEMS.md:91-99`).
- bitmap-clear coalescing, FLUSH_N sweep, lazy staging array — NO-GO; повторять
  без нового structural design/victim нельзя.
- `large-reserved-capacity` для geometric realloc — NO-GO в текущем режиме.
- Per-class/per-segment scan hints при малом segment count — многократный
  NO-GO; reopen только при real workload с 64–100+ long-lived small segments
  (`OPEN_ITEMS.md:1300-1318`).
- R34-25 palette и R34-26 full page-run без trigger — lean NO-GO/
  NEED-MORE-DATA, не повод добавлять production complexity.

## Что улучшить в коде и проекте

1. Исправить F1/F2 одним Rust-aware cfg/call-site oracle; добавить fixtures,
   где fake `#![cfg]` находится в doc comment/string, и где test вызывает
   только allowlisted accessor.
2. Не считать `cargo test --features production` достаточной no-internals
   проверкой: отдельные opt-in test crates могут быть cfg'd out. Добавить
   library/no-internals matrix и explicit negative/skip checks для каждого
   shipping opt-in (`medium-classes`, `medium-classes-wide`, NUMA и т.д.).
3. Закрыть wave-4 manifest/CHANGELOG атомарным последним commit, как уже
   требует manifest convention; не оставлять «последний commit сам себя
   допишет позже».
4. Применять Cargo version ↔ dated CHANGELOG guard ко всякому **non-dry**
   publish, включая manual dispatch. Tag-only guard недостаточен.
5. Разрешить commit-prefix debt `43115cf`/`5c1142f`: reword с одобрением либо
   явная scoped exception. Красный обязательный gate нельзя одновременно
   называть нормальным состоянием release branch.
6. В size pin отделить измеренный baseline от invariant: сохранить
   unconditional assert, но не называть `--all-features` вечным global max;
   добавить target/feature identity к зафиксированным 8840 B.
7. Сгенерировать нумерацию/summary `check-all` из массива steps, чтобы исправление
   `x5 -> x6` не оставляло overlap `2-7`/`7` и stale prose.
8. Следующий safety hardening делать только по trigger: write-aware
   `InitStateGuard` при появлении post-write fallible code; двухфазный/
   idempotent drain reclaim при появлении catch-unwind/panicking closure.

## Условия перехода к GO

1. Исправить оба настоящих medium test cfg и сам scanner; статически доказать,
   что doc comments больше не могут дать PASS, а allowlist не считается gated.
2. Завершить wave-4 manifest (включая `782b92e`, `60ad847` и closing commits)
   и CHANGELOG; выбрать release date для 0.3.0,
   удалить `(unreleased)` только в release commit.
3. Закрыть/явно разрешить commit-prefix hard-red согласно maintainer policy.
4. Сделать manual non-dry publish подчинённым тому же CHANGELOG guard.
5. После этих edits на clean tree выполнить полный project/release matrix,
   package dry-run и проверить post-push CI. Это должен сделать следующий
   исполняющий этап; данный аудит намеренно ничего не запускал.

После выполнения 1–5 и отсутствия новых failures verdict может стать **GO**.
Новые production diffs сами по себе не требуют алгоритмического rollback:
`7a9b7c7` корректно закрывает API boundary, а `3d57a26` честно восстанавливает
buildability, хотя его stack-risk narrative нужно сузить и performance benefit
ему приписывать нельзя.
