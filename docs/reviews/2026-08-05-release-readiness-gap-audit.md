# Отдельный аудит готовности sefer-alloc 0.3.0 к релизу

**Дата статического среза:** 2026-08-05, Europe/Berlin  
**Проверенный `HEAD`:** `7c8628a77e189dc7e406cad5bc992e76ab0fbe6b`  
**Основной диапазон последней волны:** `85dacfc300784cb45ce61c9cfba76dd1a0820870..7c8628a77e189dc7e406cad5bc992e76ab0fbe6b`  
**Ветка:** `main`, на момент финального среза на 152 локальных коммита впереди `origin/main` (`42d8d223f89c9e81dea18a509a433f9aea7a430d`)  
**Целевая версия:** `sefer-alloc 0.3.0`

## 1. Ограничения и метод

Это отдельный release-readiness аудит, а не очередной performance-review.
По просьбе владельца проект рассматривался в readonly-режиме: использовались
только история и метаданные Git и чтение файлов. Я **не запускал** `cargo`,
Node-скрипты, тесты, clippy, miri, loom, sanitizer-ы, benchmark-ы, упаковку,
публикацию и сетевые проверки crates.io/GitHub Actions.

Следовательно, приведённые в коммитах результаты запусков — историческое
свидетельство авторов коммитов, но не новый независимый runtime-proof этого
аудита. Где вывод требует запуска (`cargo package`, `cargo publish --dry-run`,
полный CI), это явно помечено как обязательный следующий gate.

Рабочее дерево принадлежит нескольким агентам. Я не менял и не откатывал
чужие файлы. Единственное изменение этого аудита — данный новый отчёт.

## 2. Краткий вердикт

**Текущий вердикт: NO-GO для немедленного тега и публикации 0.3.0.**

Причина не в найденной новой UB в shipping-коде. Последняя remediation-волна
действительно закрыла несколько реальных build/API/verification-дефектов,
включая false-PASS сканера в `2426dcc`. После чтения последнего runtime-диффа
я не нашёл нового доказанного UAF, double-free, OOB, data race или
неинициализированного чтения, внесённого именно этой волной.

Но текущий репозиторий всё ещё не является воспроизводимым релиз-кандидатом:

1. closing-manifest последней волны уже помечен `FINAL`, но его self-SHA
   указывает на orphaned pre-amend commit, а не на текущий landing commit;
2. `CHANGELOG.md` всё ещё содержит `## [0.3.0] (unreleased)`, а последняя
   самокоррекция `2426dcc` в нём ещё не отражена;
3. release workflow неполно моделирует реальный dependency-DAG workspace и
   допускает ручную публикацию в обход CHANGELOG guard;
4. не получено package/publish dry-run доказательство для root crate и её
   path-зависимостей;
5. локальная ветка на 152 коммита впереди remote, то есть для точного
   release-кандидата нет подтверждённого remote CI результата;
6. собственный обязательный commit-prefix gate проекта известным образом
   красный на двух старых коммитах диапазона;
7. contributor/security/process документация содержит исполняемые команды и
   safety-описания, которые уже не соответствуют дереву;
8. остаются неразобранные concurrency-test flakes и пять tier-1 unsafe seams
   без miri/loom/kani coverage.

**Условный GO** разумен после закрытия P0-чеклиста в §8 и прохождения полного
release-candidate gate на одном неизменном commit SHA. Дальнейший perf-тюнинг
перед 0.3.0 не требуется: сейчас полезнее заморозить runtime-код и довести
упаковку, проверяемость и документацию.

## 3. Что последняя волна реально улучшила

### 3.1. Да, качество релиз-кандидата выросло

Диапазон `85dacfc..7c8628a` содержит в основном docs/test/cfg remediation:

- `3d57a26` устранил реальный CI-red для `production medium-classes`: старый
  compile-time лимит `HeapCore <= 8192` не соответствовал уже существующему
  размеру 8408 B. Новый unconditional лимит 9216 B покрывает измеренный
  максимум текущей структуры, не меняя алгоритм аллокации;
- `7a9b7c7` закрыл ещё одну semver/API-дыру: measurement-only
  `SeferAlloc::dbg_trim_current_thread` теперь требует
  `bench-internals + internals`;
- `60ad847` сериализовал шесть тестов, деливших process-wide trim counters,
  и устранил подтверждённый test-interference flake;
- `2426dcc` исправил особенно важный false-green: прежний сканер принимал
  текст `#![cfg(...internals...)]` внутри `//!` комментария за настоящий
  crate-level attribute. Два medium-class test crate теперь действительно
  требуют `internals`; allowlisted ungated stats-accessors больше не попадают
  в множество gated methods;
- `888c6a9` добавил HS readonly review, а `7c8628a` попытался финализировать
  manifest и checkpoint; оба docs-only и не меняют shipping runtime;
- остальные коммиты исправили orphaned SHA, нумерацию open-items, структуру
  CHANGELOG и комментарии check tooling.

Это реальные улучшения релизной надёжности. Особенно ценно, что `2426dcc`
закрывает не один симптом, а класс «проверка зелёная из-за текста в комментарии».

### 3.2. Нет, эта волна не ускорила shipping runtime

CHANGELOG честно фиксирует `Runtime improvements: 0`. По диффу это верно:

- лимит размера `HeapCore` — compile-time assertion и комментарий;
- `dbg_trim_current_thread` — только сужение cfg-доступности внутреннего hook;
- остальные изменения — тесты, скрипты и документы.

Это не минус. Перед релизом такие изменения полезнее нового рискованного hot
path. Предыдущие perf-улучшения остаются в дереве, но новая волна не является
основанием заявлять дополнительное ускорение 0.3.0.

### 3.3. Что можно сказать о memory safety

В последней волне нет нового shipping pointer-manipulation алгоритма. Поэтому
я не вижу внесённого ею нового подтверждённого UB/UAF/double-free/race/OOB.
Это **не** равнозначно доказательству отсутствия таких дефектов во всём
аллокаторе: статическое чтение и зелёные ordinary tests не заменяют недостающие
miri/loom/kani/sanitizer harnesses, перечисленные в §7.

## 4. P0 — блокеры до тега

### R0. Зафиксировать один release-candidate SHA и получить CI именно для него

На финальном срезе `main` был на 152 коммита впереди `origin/main`; локальные теги
содержали только `sefer-alloc-v0.1.0`, `v0.2.0`, `v0.2.1`. Значит, локальные
заявления о зелёных проверках ещё не являются remote release evidence.

Нужно:

1. закончить remediation и прекратить изменения runtime/CI файлов;
2. убедиться, что tracked working tree чист;
3. push release-candidate commit;
4. дождаться **всех обязательных** GitHub Actions checks именно на этом SHA;
5. только после этого датировать CHANGELOG и создавать tag на том же SHA.

Нельзя считать зелёный CI родительского или промежуточного коммита доказательством
для тега: эта сессия уже несколько раз находила false-green и post-closing
follow-up сразу после «закрытия» волны.

### R1. Исправить self-identity «финального» manifest

Пока отчёт оформлялся, `7c8628a` обновил
`docs/perf/round-manifests/R34_REMEDIATION_4_MANIFEST.md`: документ теперь
помечен `FINAL` и включает 13 строк. Это закрывает прежнюю неполноту таблицы,
но создаёт новый воспроизводимый identity defect:

- текущий landing commit — `7c8628a77e189dc7e406cad5bc992e76ab0fbe6b`;
- manifest называет собственным closing SHA
  `9d62bf6d47345680fd1a59fb5e9f7ff596a6daea`;
- объект `9d62bf6` существует, имеет того же родителя и subject, но **не является
  ancestor текущего HEAD**: это orphaned pre-amend sibling commit;
- следовательно, строка «this table's own upper bound, `9d62bf6`, is the
  wave's true closing SHA» неверна для опубликованной истории.

Это ровно тот landing-SHA drift, от которого manifest должен защищать. SHA
коммита нельзя надёжно вписать внутрь его же content до создания commit без
последующего amend, который снова меняет SHA. Нужно выбрать устойчивую схему:

- не self-cite hash вообще, а использовать `HEAD`/tag + parent-bound range;
- либо закрывать manifest следующим маленьким commit и честно считать его
  post-manifest correction;
- либо хранить externally generated signed/tag identity.

Перед релизом текущий `9d62bf6` нужно заменить на достижимую identity-схему и
повторно сверить таблицу с `git log --reverse 85dacfc..FINAL_SHA`.

### R2. Исправить и финализировать CHANGELOG

Текущий заголовок `## [0.3.0] (unreleased)` правильно блокирует tag workflow,
но означает, что дерево ещё не готово к тегу.

До замены на датированный `## [0.3.0] - YYYY-MM-DD` нужно сначала исправить
содержание:

- wave-4 bullet утверждает, что `b1a9b7b` уже «tightened gate-detection to
  require a real cfg attribute», хотя именно `2426dcc` доказал, что это было
  false и исправил сканер позднее;
- там же `--all-features` назван «provably the ceiling» без ограничения на
  текущую additive-cfg структуру/target/toolchain; `2426dcc` уже сузил это
  утверждение в source comment, но CHANGELOG остался старым;
- отсутствует отдельная append-only correction запись о `2426dcc`;
- нужно включить финальный manifest SHA и все post-closing commits;
- open items 16 и 19 требуют release-note caveats: стандартный caller-contract
  residual для invalid/double free уже освобождённого foreign segment и факт,
  что MSRV сейчас проверяется `cargo check`, а не test/dev-dependency путями.

Только после этого убрать `(unreleased)`, поставить реальную дату и убедиться,
что существует ровно одна секция `[0.3.0]`.

### R3. Доказать публикационный dependency-DAG

Root `Cargo.toml` имеет versioned path dependencies:

| dependency | требуемая registry version | release workflow target |
|---|---:|---|
| `sefer-region` | `0.1` | есть |
| `aligned-vmem` | `0.2` | есть |
| `racy-ptr-cell` | `0.1` | **нет** |
| `size-classes` | `0.1` | **нет** |
| `tagged-index-stack` | `0.1` | **нет** |
| `numa-shim` | `0.1` | есть |

Даже optional path dependency должна разрешаться как registry dependency в
опубликованном package. Локальная история тегов не содержит release-тегов
member crates, а readonly-аудит не обращался к crates.io. Поэтому существование
всех точных версий в registry **не доказано**.

До root publish:

1. проверить на crates.io каждую точную версию;
2. если версии нет — добавить crate в tag/dispatch map release workflow;
3. публиковать в DAG-порядке: независимые leaf crates (`sefer-region`,
   `aligned-vmem`, `racy-ptr-cell`, `size-classes`, `tagged-index-stack`), затем
   `numa-shim` (после `aligned-vmem`), затем `sefer-alloc`;
4. после каждой публикации дождаться доступности версии в index, прежде чем
   публиковать зависимый crate.

README/Cargo.toml также называют все 11 workspace members «реальными crates.io
crate». Если это релизное обещание, workflow должен поддерживать все 11 либо
документация должна честно отделить publishable source packages от уже
опубликованных crates.

### R4. Получить packaging proof, а не только repository-build proof

Root `Cargo.toml` исключает только `scripts/`, `package.json`, `.github/` и
`docs/checkpoints/`. Статический размер tracked content на срезе:

| subtree | files | bytes |
|---|---:|---:|
| `docs/` | 704 | 14,063,722 |
| `docs/perf/` | 463 | 9,857,919 |
| `tests/` | 245 | 3,064,726 |
| `src/` | 72 | 2,209,487 |
| `examples/` | 97 | 993,483 |
| `benches/` | 25 | 650,551 |

Под `docs/perf/_raw_*` отдельно tracked 235 файлов. Это не доказывает точный
размер `.crate` — Cargo учитывает VCS/package rules и compression, — но
показывает высокий риск отправить в registry многомегабайтный исследовательский
архив, замедлить download/docs.rs или упереться в upload limit.

Обязательные, но **не выполненные этим readonly-аудитом** gates:

```text
cargo package -p sefer-alloc --list
cargo package -p sefer-alloc
cargo publish -p sefer-alloc --dry-run
```

То же нужно выполнить для каждого ещё не опубликованного member crate. После
просмотра `--list` стоит заменить широкий default package set на явный
`include` либо расширенный `exclude`, оставив source, Cargo metadata, README,
licenses и только действительно нужные downstream документы. Perf raw logs,
review archive, большинство benches/examples/tests не нужны потребителю.

### R5. Закрыть ручной обход CHANGELOG guard

`.github/workflows/release.yml` запускает CHANGELOG guard только при
`github.event_name == 'push'`. `workflow_dispatch` с `dry-run=false` пропускает
его и доходит до настоящего `cargo publish`. Таким образом, человек может
опубликовать `0.3.0`, пока секция всё ещё `(unreleased)`.

Guard должен выполняться для **всякой non-dry публикации**, независимо от
trigger. Для dry-run можно сознательно разрешить unreleased section, поскольку
его назначение — проверять упаковку до финализации notes.

Для member crates нельзя механически проверять только root `CHANGELOG.md` по
номеру версии: секция root `[0.1.0]` способна ложно подтвердить release другого
crate версии `0.1.0`. Нужна явная политика: per-crate changelog либо root-only
guard для `sefer-alloc` и отдельное документированное правило member releases.

### R6. Разрешить красный обязательный commit-prefix gate

`docs/CORRECTNESS_OPEN_ITEMS.md` item 21 подтверждает, что
`verify-commit-prefixes.mjs` падает на `43115cf` и `5c1142f`: оба CSV-only
коммита имеют `fix(perf)` вместо `docs(config)`.

Поскольку default range — `@{u}..HEAD`, а ветка сильно впереди upstream,
обычный `npm run check` включает эти коммиты и остаётся красным. До релиза
нужен явный maintainer decision:

- либо одобренный reword rebase этих двух коммитов;
- либо узкая, документированная policy exception/изменение lint-range;
- либо официальное снятие этого gate с mandatory статуса.

Нельзя одновременно называть `npm run check` обязательным и выпускать версию
с заранее известным красным шагом. Глубокий rebase нельзя делать автоматически:
он перепишет большой диапазон и требует отдельного разрешения владельца.

## 5. P1 — что исправить до релиза либо явно принять как риск

### R7. Release workflow должен зависеть от полного CI точного SHA

Publish job сам выполняет только default tests и два root feature запуска
(`production --lib`, `production internals`). Он не повторяет и не требует
успеха полного проекта: fmt, шесть clippy rows, all-features, feature isolation,
workspace members, docs, cargo-deny, MSRV, no_std, miri, loom, TSan, ASan,
multi-arch и прочие проверки живут в независимом `ci.yml`.

Tag push не имеет `needs` на branch CI. Поэтому технически publish workflow
может выпустить commit, у которого другая обязательная CI job красная.

Лучшее решение — reusable release-candidate workflow или protected GitHub
Environment, который перед irreversible upload:

- проверяет, что полный required CI suite успешен для **того же SHA**;
- требует ручного approval для non-dry publish;
- запускает package dry-run;
- только затем получает publish credential.

### R8. Полностью тестировать workspace members как самостоятельные crates

Текущий `test-workspace` запускает только `aligned-vmem`, `sefer-region` и
`malloc-bench-rs`; `numa-shim` имеет отдельные jobs. Некоторые lock-free
members получают loom coverage, но нет простой единой гарантии обычного
`cargo test`/doc/package для всех 11 независимо публикуемых crates — в частности
`size-classes`, `globalalloc-model`, `proc-memstat`, `proc-probe` не видны в
обычной member-test строке.

Для release policy нужен explicit package matrix на все advertised crates:
default tests, relevant feature tests, rustdoc и `cargo package`/dry-run.

### R9. Разобрать два оставшихся concurrency flakes

Open items 12 и 14 всё ещё описывают:

- единичное `xthread_large_double_free_no_double_reclaim`: ожидалось 50
  reclaims, наблюдалось 42;
- воспроизводимый при параллельном запуске файла
  `xthread_large_free_tiny_size_huge_align_is_reclaimed`: delta 0, при
  isolated run проходит.

Похожий process-wide-counter flake только что действительно был исправлен в
`60ad847`, поэтому test interference — правдоподобная гипотеза. Но для
safety-first allocator «правдоподобно flake» недостаточно, особенно когда
сигнал выглядит как недовыполненный reclaim.

До релиза нужно root-cause:

- отделить process-global diagnostics от per-test ownership/accounting;
- сериализовать весь конфликтующий test binary либо использовать уникальные
  per-test counters/oracles;
- доказать counterfactual, что тест ловит реальную потерю reclaim;
- только после устойчивых повторов перевести items в Recently resolved.

### R10. Закрыть или явно принять coverage gap пяти tier-1 unsafe seams

`docs/CORRECTNESS_OPEN_ITEMS.md` item 17 перечисляет shipping-sensitive seams
без miri/loom/kani harness:

- `global::sefer_alloc` (`unsafe impl GlobalAlloc`);
- `global::fallback` (`static mut MaybeUninit<HeapCore>` + init state);
- `registry::heap_slot` (`unsafe impl Sync`);
- `alloc_core::sidecar` (production directory/dirty sidecars);
- `alloc_core::large_cache_extended`.

Для проекта, который продаёт себя как safety-first/verified allocator,
предпочтительно до 0.3.0 дать real-type miri/loom coverage хотя бы `sidecar`,
`fallback` и `heap_slot`; `GlobalAlloc` path должен иметь miri/ASan/Valgrind
integration coverage. Альтернатива — не блокировать релиз, но явно ослабить
release claims и записать принятый residual risk. Обычные integration tests
не доказывают отсутствие provenance/race UB.

### R11. Переписать устаревший CONTRIBUTING.md

Документ содержит фактически неверные инструкции:

- утверждает, что unsafe разрешён только в `src/concurrent/hand.rs` и
  несуществующих `src/byte/byte_region.rs`/`byte_allocator.rs`, хотя текущий
  allocator имеет большой именованный tier-1/tier-2 seam inventory;
- предлагает несуществующий `tests/loom_reclaim.rs`;
- предлагает несуществующий fuzz target `fuzz_alloc_dealloc`; реальные targets:
  `region_ops`, `global_alloc_ops`, `heap_core_ops`;
- mandatory commands не соответствуют текущим `internals` gates, шести clippy
  rows и check-matrix.

Это не cosmetic debt: новый contributor, следуя официальной инструкции,
получит failing commands и неверную модель безопасности. Лучше сделать
`npm run check`/CI manifest единственным source of truth и минимизировать
ручное дублирование команд.

### R12. Исправить SECURITY.md и README contact promise

`SECURITY.md` просит указать, требует ли bug `experimental`/`byte`, но feature
`byte` больше нет. README говорит «Security Advisories или email maintainer per
SECURITY.md», однако SECURITY.md не содержит email.

Нужно обновить feature vocabulary (`production`, `experimental`, конкретные
allocator features), либо добавить реальный security email, либо убрать email
promise из README. Иначе у исследователя есть противоречивый disclosure path.

### R13. Синхронизировать CLAUDE.md и check-all prose с реальной матрицей

`CLAUDE.md` в секции «Before every push» по-прежнему говорит о пяти clippy
rows, хотя CI/check matrix содержит шесть. `650b818` исправил часть комментария
в `scripts/check-all.mjs`, а `2426dcc` — его нумерацию, но source-of-truth prose
остаётся рассинхронизированным.

Стоит генерировать counts/names из `scripts/check-matrix.mjs`, не хранить их
в нескольких ручных списках. Для release checklist нужен один машиночитаемый
перечень обязательных gates.

## 6. P2 — полезное hardening после стабилизации

### R14. Сделать panic/unwind guards структурно полными

Open items 22 и 23 не описывают текущий reachable exploit, но фиксируют
будущие ловушки:

- `DrainHeadPublish` может повторно передать in-flight element в reclaim после
  panic-after-mutation;
- `InitStateGuard` не различает unwind до и после записи live `HeapCore`, и
  будущий post-write panic может привести к overwrite без `Drop` и утечке.

Сейчас entry-point panic tripwire abort-ит, а известных post-mutation/post-write
panic sites нет, поэтому это не P0. Однако state-aware guard/two-phase protocol
дешевле добавить до появления fallible code, чем после инцидента.

### R15. Добавить два Kani arithmetic proofs

Open item 18 предлагает хорошие bounded properties:

- ring wrap invariant через `u32::MAX -> 0`;
- `pack_entry`/`unpack_entry` round-trip для hardened/non-hardened диапазонов и
  невозможность получить `RING_SLOT_EMPTY`.

Это чистая арифметика без pointers — подходящий и сравнительно дешёвый Kani
слой. Он не заменяет loom, но закрывает классы overflow/encoding ошибок.

### R16. Усилить MSRV gate

Сейчас rustc 1.88 выполняет только `cargo check --all-features`. Это не ловит
MSRV break в `#[cfg(test)]` и dev-dependencies. Минимум — честный release-note
caveat (уже open item 19); лучше — отдельный ограниченный `cargo test --no-run`
или test subset на MSRV, если dev-dependency graph поддерживает 1.88.

### R17. Перейти на crates.io trusted publishing

Release workflow использует долгоживущий `CARGO_REGISTRY_TOKEN`. Permissions
для `GITHUB_TOKEN` уже минимальны, checkout в release job SHA-pinned — это
хорошо. Следующий supply-chain шаг: protected environment + crates.io OIDC
trusted publishing, убирающий reusable secret. Это hardening, не блокер при
корректно защищённом token.

## 7. Что не нужно делать перед 0.3.0

Не нужно запускать ещё одну большую performance-волну или page-run rewrite.
Последние исследования уже показали:

- текущая remediation не меняет runtime;
- page-run 256 KiB–2 MiB не имеет доказанного реального потребителя;
- medium-class density/realloc trade-off не даёт безопасного общего GO;
- новые hot-path изменения увеличат поверхность, которую придётся заново
  прогонять через miri/loom/sanitizers и release audit.

Правильная стратегия сейчас: **code freeze → release plumbing → exact-SHA
verification → package dry-run → tag**. Новые оптимизации — в 0.3.1/0.4.0 после
появления consumer-driven workload и отдельного A/B gate.

## 8. Практический release checklist

### P0 — обязательно до tag

- [ ] Исправить wave-4 manifest: orphaned self-SHA `9d62bf6` не является
  текущим landing commit `7c8628a`; перейти на нециклическую identity-схему.
- [ ] Добавить в CHANGELOG append-only correction для false-PASS `b1a9b7b` и
  исправить overclaim про `--all-features`.
- [ ] Разрешить open items 16/19 в release notes.
- [ ] Решить commit-prefix item 21: approved reword или явная policy exception.
- [ ] Проверить точные registry versions всех шести root path dependencies.
- [ ] Добавить отсутствующие release targets для необходимых member crates.
- [ ] Исправить manual non-dry bypass CHANGELOG guard и per-crate changelog policy.
- [ ] Выполнить `cargo package --list`, `cargo package`,
  `cargo publish --dry-run` для каждого crate в DAG.
- [ ] Сократить package content по фактическому `--list`, если туда входят
  perf raw logs/review archive/лишние benches и tests.
- [ ] Push release-candidate, дождаться полного required CI на точном SHA.
- [ ] Убедиться, что tracked tree чист и после CI не было follow-up commit.
- [ ] Только теперь заменить `(unreleased)` на дату и создать
  `sefer-alloc-v0.3.0` на проверенном SHA.

### P1 — желательно считать частью release stabilization

- [ ] Root-cause и стабилизировать open concurrency flakes 12/14.
- [ ] Добавить real-type coverage критичных unsafe seams либо письменно принять
  и отразить остаточный риск.
- [ ] Переписать CONTRIBUTING.md и SECURITY.md; синхронизировать README contact.
- [ ] Синхронизировать CLAUDE.md/check-all counts с шестью clippy rows.
- [ ] Привязать non-dry publish к полному CI exact SHA и protected approval.
- [ ] Тестировать/document/package каждый advertised workspace member отдельно.

### Финальный smoke после датирования, перед upload

Следующие команды здесь **не запускались**; это handoff владельцу/релизному
агенту после code freeze:

```text
npm run check
cargo test --all-features --no-fail-fast
cargo test --workspace --no-fail-fast
cargo doc --no-deps --all-features
cargo deny check
cargo package -p sefer-alloc --list
cargo publish -p sefer-alloc --dry-run
```

Плюс полный CI jobs set (miri/loom/TSan/ASan/multi-arch/MSRV/no_std) на одном
SHA. Не следует вручную публиковать через dispatch, пока workflow guard не
исправлен.

## 9. Итог для владельца

Проект близок к релизу по shipping-коду, но ещё не по release engineering.
Новая волна не дала ускорения — и это нормально: она действительно повысила
корректность cfg/API boundary и качество тестовой сетки. После `2426dcc` нет
нового подтверждённого memory-safety дефекта последней волны.

Однако выпускать 0.3.0 сегодня рано. Наиболее опасная оставшаяся категория —
не «ещё один быстрый алгоритм», а возможность опубликовать не тот или
недоказанный package: manifest с orphaned self-SHA, unreleased/частично ложный
CHANGELOG narrative, неполный member-crate DAG, ручной обход guard, отсутствие
package dry-run evidence и отсутствие remote CI для 152 локальных коммитов.

После закрытия P0 список превращается в реалистичный **GO** без новой большой
архитектурной волны. До тех пор строгий verdict остаётся **NO-GO**.
