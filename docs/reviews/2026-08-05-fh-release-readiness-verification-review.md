# Независимая верификация готовности к релизу 0.3.0 (fh, 2026-08-05)

Третье независимое ревью за день. Задача — не пересказать два предыдущих
(`2026-08-05-hs-new-waves-release-readonly-review.md` и
`2026-08-05-release-readiness-gap-audit.md`, по которым заведены таски K1–K18),
а (1) перепроверить их ключевые заявления против текущего HEAD, (2) найти
пропущенное ими, (3) дать свой вердикт. В отличие от прежних readonly-аудитов,
этому ревью было разрешено ЗАПУСКАТЬ команды (до середины сессии; затем
пользователь ограничил работу до read-only — все команды ниже были выполнены
ДО ограничения, это честно помечено).

## 0. Срез

- **HEAD:** `42d4206178a1604dc8a5e87e7ca6a747f2ffd19a`
  («docs: add standalone 0.3.0 release-readiness gap audit»), ветка `main`.
- **Дата/время среза:** 2026-08-05, ~23:00 (локальное).
- **Дерево:** tracked-часть чистая; untracked: `.claude/` (settings.json) и
  9 файлов `docs/reviews/2026-08-0*-*.md` (сегодняшние/вчерашние ревью, не
  закоммичены). Временный файл `src/global/sefer_alloc.rs.tmp.23920.*`,
  видимый в начале сессии, к моменту среза исчез.
- **КРИТИЧЕСКОЕ ИЗМЕНЕНИЕ КОНТЕКСТА: `origin/main == HEAD`.** Посылка обоих
  прежних ревью («~153 локальных коммита впереди origin/main») устарела —
  push произошёл сегодня в 20:50:33Z (`git rev-list --count origin/main..HEAD`
  = 0). Первый полный прогон удалённого CI по всему накопленному бэклогу
  начался в момент этого аудита — и уже красный (см. N1).
- Диапазон последней волны: `85dacfc..HEAD` = 14 коммитов (wave-4 remediation
  F1–F10 + два ревью + gap-audit + финализация манифеста).

## 1. Метод

**Запускал (до read-only-ограничения):**
- `git log/status/rev-list/tag` (read-only);
- `grep 9d62bf6 docs/perf/round-manifests/*.md`;
- чтение шапок `tests/medium_classes_correctness.rs` / `..._wide_...rs`;
- `cargo check --features "production medium-classes" --test medium_classes_correctness`;
- `node scripts/verify-alloc-core-dbg-internals-exhaustive.mjs`;
- `node scripts/verify-commit-prefixes.mjs`;
- `cargo package -p sefer-alloc --list --allow-dirty` (+ подсчёт размера по
  списку: stat + tar czf оценка);
- `cargo package -p sefer-alloc --allow-dirty --no-verify` (реальная попытка
  упаковки — УПАЛА, см. N2);
- `cargo check --no-default-features`;
- crates.io API (curl, с user-agent): версии sefer-alloc, sefer-region,
  aligned-vmem, numa-shim, malloc-bench-rs, racy-ptr-cell, size-classes,
  tagged-index-stack;
- `gh run list` / `gh run view 31045983765` / логи джоба clippy (через API) —
  статус CI на landing SHA.

**НЕ запускал / НЕ проверял (честный список):**
- полный `npm run check`, `cargo test` любой ширины, miri/loom/TSan/kani;
- `cargo publish --dry-run` (упаковка падает раньше — см. N2 — и без
  публикации зависимостей dry-run root-крейта недостижим);
- реальный MSRV 1.88 (нет тулчейна 1.88 под рукой; `rust-version` только
  прочитан из Cargo.toml);
- настоящий no_std-таргет (проверен только host `cargo check
  --no-default-features` — он зелёный, но это не доказательство no_std);
- flaky-тесты (items 12/14), покрытие 5 tier-1 unsafe-seams (item 17), Kani-
  proofs (R15), OIDC (R17) — оставлены как есть в тасках K10/K11/K16/K18;
- содержательную полноту правок CHANGELOG по R2 (проверен только верх файла);
- итоговый статус ВСЕГО CI-рана 31045983765 (на момент среза ещё шёл; джоб
  clippy уже упал — этого достаточно для вывода «красный», но других падений
  может добавиться).

## 2. Таблица перепроверки прежних находок

| # | Находка (источник) | Статус | Доказательство |
|---|---|---|---|
| R1 / K1 | Осиротевший self-SHA `9d62bf6` в R34_REMEDIATION_4_MANIFEST.md | **ЗАКРЫТО** | `grep 9d62bf6 docs/perf/round-manifests/*.md` — 0 совпадений во всех 5 манифестах (закрыто коммитом 6550d68) |
| HS-F1 | `tests/medium_classes_correctness.rs` / `_wide_` без гейта `internals` | **ЗАКРЫТО** | Оба файла имеют настоящий `#![cfg(all(feature="alloc-core", feature="medium-classes", feature="internals"))]`; `cargo check --features "production medium-classes" --test medium_classes_correctness` — чисто (файл cfg'd out) |
| HS-F1-скрипт | Сканер exhaustive-verify давал false-PASS на doc-comment | **ЗАКРЫТО** | `node scripts/verify-alloc-core-dbg-internals-exhaustive.mjs` → «ALL GREEN»: 41 файл, 128 `dbg_*`-методов, 124 gated + 4 allowlisted, 0 нарушений; check 2/2: 244 tests/*.rs, 0 нарушений |
| R6 / K6 | Красный commit-prefix gate на 43115cf/5c1142f | **ПОДТВЕРЖДЕНО** | `node scripts/verify-commit-prefixes.mjs` → FAILED: те же 2 failure (`fix(perf)` на CSV-only диффах) + 18 предупреждений direction-2 |
| R5 / K5 | Manual dispatch обходит CHANGELOG guard | **ПОДТВЕРЖДЕНО** | В `.github/workflows/release.yml` оба guard-шага («tag version must match» и «CHANGELOG must be consolidated») стоят под `if: github.event_name == 'push'`; workflow_dispatch с dry-run=false доходит до `cargo publish` без них (test-gate при этом выполняется) |
| R3 / K3 | Дыра в publish-DAG (racy-ptr-cell / size-classes / tagged-index-stack без release-targets) | **ПОДТВЕРЖДЕНО + УСИЛЕНО** | (а) release.yml знает только 5 крейтов (aligned-vmem, sefer-region, malloc-bench-rs, numa-shim, sefer-alloc) — тегов/опций для трёх названных нет; (б) crates.io: racy-ptr-cell, size-classes, tagged-index-stack **вообще не опубликованы**; (в) `cargo package -p sefer-alloc` падает — см. N2 |
| R4 / K4 | Риск многомегабайтного пакета | **ПОДТВЕРЖДЕНО, с уточнением** | `cargo package --list --allow-dirty`: **1064 файла, ~20.5 MiB** несжатых; gzip-оценка ~**5.2 MiB** — то есть в лимит crates.io (10 MiB) tarball влезает, «hard-block» по размеру НЕТ, но состав — утечка (см. N5): 463 файла docs/perf, 237 `_raw_*`-логов, **85 файлов docs/reviews** (внутренние ревью с вердиктами NO-GO), **`.claude/settings.json`** |
| R2 / K2 | CHANGELOG перед датированием 0.3.0 | **ОТКРЫТО (частично проверено)** | Верх файла: `## [0.3.0] (unreleased)` — не датирован (это ожидаемо до релиза); содержательные неточности из R2 детально не перепроверял |
| R13 / K14 | CLAUDE.md «5 clippy rows» vs 6 реальных | **ПОДТВЕРЖДЕНО** | `scripts/check-matrix.mjs`: 6 строк `id: 'clippy-*'` (default, experimental, all-features, hardened-medium-classes, production, production-internals); CLAUDE.md строки ~416/445/447 всё ещё говорят «all five». 650b818 починил комментарий в check-all.mjs, но не CLAUDE.md |
| R12 / K13 | SECURITY.md устаревший feature-словарь | **ПОДТВЕРЖДЕНО** | SECURITY.md:32-33 предлагает указывать «`experimental`/`byte` features» — фичи `byte` в Cargo.toml не существует |
| R11 / K12 | CONTRIBUTING.md устарел | **ЧАСТИЧНО** | Спот-чек: все ссылки шапки (README, docs/DESIGN.md, docs/INVARIANTS.md, docs/ALLOC_PLAN.md, SECURITY.md) существуют; таблица верификации выглядит правдоподобно. Глубокой сверки процессов не делал — таск K12 оставляю как есть |
| R9/R10/R15/R16/R17 | flakes, unsafe-coverage, Kani, MSRV-gate, OIDC | **НЕ ПЕРЕПРОВЕРЯЛ** | Длинные прогоны / вне time-box; таски K10/K11/K16/K17/K18 остаются в силе |

## 3. Новые находки (пропущено обоими ревью)

### N1 [P0] — CI на запушенном landing SHA КРАСНЫЙ: Linux-only bench не компилируется

Push сегодняшнего HEAD (42d4206) запустил первый полный CI по всему бэклогу
(run 31045983765). Джоб **clippy упал за 46s** на строке
`clippy (--all-features)`:

```text
benches/macro_multiseg_steady_state.rs:322:1
  help: Only the `bench` and the `benches` attribute are allowed
error[E0433]: cannot find `multiseg_steady_state_1t` in `super`  (:344)
error[E0433]: cannot find `multiseg_steady_state_mt4` in `super` (:344)
error: could not compile `sefer-alloc` (bench "macro_multiseg_steady_state") due to 4 previous errors
```

Механика: весь файл под `#[cfg(target_os = "linux")]` (строки 329/341/347),
поэтому на Windows-машине разработчика он НИКОГДА не компилировался — все
локальные прогоны `npm run check` структурно слепы к нему. На ubuntu-раннере
проц-макрос `#[library_benchmark]` (iai-callgrind 0.14.2) отвергает
doc-comment-атрибуты (`///` = `#[doc]`) на функции бенчмарка, из-за чего
функции не эмитятся и `library_benchmark_group!` даёт E0433. Файл добавлен
коммитом `2ea920b` (R32, F3, task #500) и с тех пор ни разу не проходил
Linux-компиляцию. Оба прежних ревью проверяли гейты локально и не могли этого
увидеть. Обобщение (класс бага): **локальный pre-push-гейт на Windows не
покрывает `#[cfg(target_os = "linux")]`-код; таких файлов в репо больше
одного — нужна ревизия** (например `grep -l 'cfg(target_os = "linux")'
benches/ examples/ tests/`). На момент среза ран ещё шёл («test (xthread
progression)» in progress) — возможны ДОПОЛНИТЕЛЬНЫЕ падения; K7 («push, full
remote CI, then tag») уже фактически провален на первом шаге, а не просто
«ожидает».

Починка: убрать/переместить doc-комментарии с `#[library_benchmark]`-функций
(в обычные `//`), проверить компиляцию на Linux (CI workflow_dispatch или
контейнер), и только потом перезапускать RC-процедуру.

### N2 [P0] — `cargo package -p sefer-alloc` падает: релиз сейчас НЕ упаковывается вовсе

Не гипотеза о DAG, а факт:

```text
error: failed to select a version for the requirement `aligned-vmem = "^0.2"`
  candidate versions found which didn't match: 0.1.0
  location searched: crates.io index
  required by package `sefer-alloc v0.3.0`
```

crates.io (проверено API): `aligned-vmem` max = **0.1.0** (локально 0.2.0),
`sefer-region` = 0.1.0, `numa-shim` = 0.1.0, `malloc-bench-rs` = 0.1.0,
`sefer-alloc` = 0.2.1; **racy-ptr-cell, size-classes, tagged-index-stack —
отсутствуют полностью**. После публикации aligned-vmem 0.2.0 упаковка упадёт
на следующем неопубликованном (racy-ptr-cell "0.1" и т.д.). Минимальный
порядок публикации: `aligned-vmem 0.2.0` → `numa-shim` (с bump'ом, см. N3) →
`racy-ptr-cell 0.1.0`, `size-classes 0.1.0`, `tagged-index-stack 0.1.0`
(впервые; для них нужны tag-паттерны/опции в release.yml — сейчас их нет) →
`sefer-alloc 0.3.0`. dev-deps (malloc-bench-rs, globalalloc-model,
proc-memstat, proc-probe — path без version) при publish отбрасываются и DAG
не блокируют.

### N3 [P1] — Дрейф опубликованных member-крейтов под неизменной версией

- **numa-shim:** локальная 0.1.0 ≠ опубликованная 0.1.0. После публикации в
  крейт вошли `9b48844` («perf(numa): cache current_node()», R11-5,
  **2026-07-21**) и зависимость `aligned-vmem = { version = "0.2", ... }`
  (опубликованная 0.1.0 физически не могла зависеть от несуществующего 0.2).
  `sefer-alloc = { numa-shim "0.1", features=["vmem-integration"] }` при
  публикации притянет СТАРУЮ 0.1.0 с crates.io — не тот код, что тестировался
  в дереве. **Нужен bump numa-shim (0.1.1/0.2.0) + публикация до root-крейта.**
- **sefer-region:** локальная 0.1.0 имеет doc-коммит `aab617a` (2026-07-13)
  после публикации (~30 июня). Дрейф docs-only — bump желателен, не блокер.
- **malloc-bench-rs:** дрейф есть (`99e3238`, 2026-07-17, feat), но это
  dev-dep без version — publish-DAG не трогает; влияет только на смысл
  существующего release-target'а в release.yml.

Ни одно из двух ревью не сверяло локальное содержимое уже опубликованных
крейтов с crates.io.

### N4 [P1] — CHANGELOG guard в release.yml проверяет НЕ ТОТ крейт (кросс-крейтовый дефект)

Guard делает `grep "^## \[${VERSION}\]" CHANGELOG.md` — всегда КОРНЕВОЙ
changelog, каким бы ни был публикуемый крейт. Следствия: (а) тег
`aligned-vmem-v0.2.0` даст VERSION=0.2.0, а в корневом CHANGELOG.md ЕСТЬ
`## [0.2.0] - 2026-06-29` (секция про sefer-alloc 0.2.0, строка 6061) — guard
**ложно ПРОЙДЁТ** по чужой записи; (б) первая публикация member-крейта с
версией, которой нет в корневом changelog (например `racy-ptr-cell-v0.1.0` —
`## [0.1.0]` есть, снова ложный проход; а вот `sefer-region-v0.1.1` упал бы,
пока кто-нибудь не заведёт сефер-алоковскую 0.1.1). Guard осмыслен только для
root-крейта; для членов он либо ложно-зелёный, либо ложно-красный. Прежние
ревью зафиксировали только manual-dispatch-обход (R5), но не это.

### N5 [P2] — Состав пакета: утечка внутренних артефактов (уточнение R4)

По `cargo package --list`: в паблик-tarball уходят **85 файлов
`docs/reviews/`** (внутренние ревью, включая сегодняшние NO-GO-вердикты и
этот отчёт), **`.claude/settings.json`** (локальные пути машины пользователя:
`C:/Users/Computer/...`), `CLAUDE.md`, 463 файла `docs/perf/` c 237
`_raw_*`-логами, `docs/checkpoints` исключён — а `docs/reviews` нет.
Размер НЕ блокирует (~5.2 MiB gzip < 10 MiB), но публиковать внутреннюю кухню
и локальные пути — нельзя. Минимальная правка: расширить `exclude` в
Cargo.toml (`docs/reviews/`, `docs/perf/`, `.claude/`, `CLAUDE.md`,
`docs/checkpoints/` уже есть) — либо перейти на явный `include`-список
(надёжнее: белый список src/, crates опубликованные не включаются и так,
README/LICENSE/CHANGELOG).

### N6 [P3] — 9 untracked ревью-файлов и .claude/ в рабочем дереве

`cargo package` без `--allow-dirty` отказывается работать именно из-за них.
Перед RC их надо либо закоммитить, либо переместить — сейчас состояние
«ревью существуют только на диске одной машины» противоречит собственной
конвенции проекта (ревью цитируются тасками K1–K18, но не версионированы).

### N7 [P3] — Устаревшая посылка «153 коммита впереди» в контексте тасков

Push уже произошёл (см. §0). Формулировку K7 стоит обновить: не «запушить и
посмотреть», а «починить красный CI на 42d4206 (N1) и заморозить НОВЫЙ
зелёный SHA».

## 4. Вердикт: **NO-GO** (подтверждаю оба прежних вердикта, с новыми жёсткими основаниями)

Прежние ревью давали NO-GO по совокупности процессных дыр. Теперь есть три
МЕХАНИЧЕСКИХ доказательства невозможности релиза прямо сейчас:

1. **CI на landing SHA красный** (N1) — clippy job упал на компиляции
   Linux-only бенча; полный статус рана на момент среза ещё неизвестен.
2. **`cargo package -p sefer-alloc` не выполняется** (N2) — публиковать
   нечего, пока не опубликованы 4-5 зависимостей в правильном порядке (для
   трёх из них release-механизма не существует, для numa-shim нужен bump —
   N3; напоминаю: бампы версий — только по явному решению пользователя).
3. **Собственные гейты проекта красные** (R6: verify-commit-prefixes FAILED)
   и обходимые (R5 + N4: CHANGELOG guard не срабатывает на manual dispatch и
   проверяет не тот крейт на member-тегах).

## 5. Приоритизированный чеклист (мой порядок, поверх K1–K18)

**P0 — без этого релиза нет физически:**
1. [N1] Починить `benches/macro_multiseg_steady_state.rs` (doc-комментарии на
   `#[library_benchmark]`-функциях → `//`), прогнать Linux-компиляцию;
   ревизия остальных `cfg(target_os = "linux")`-файлов в benches/examples/tests.
2. [N1/K7] Дождаться/добиться полностью зелёного CI на актуальном SHA;
   только он может стать RC.
3. [N2/N3/K3] Согласовать с пользователем план публикации DAG: aligned-vmem
   0.2.0 → numa-shim (bump — решение пользователя) → racy-ptr-cell /
   size-classes / tagged-index-stack 0.1.0 (добавить их в release.yml:
   tag-паттерны + dispatch-опции) → sefer-alloc 0.3.0. Критерий готовности:
   `cargo package -p sefer-alloc` проходит.
4. [N5/K4] Починить состав пакета (exclude/include) ДО первой публикации —
   tarball необратим.
5. [K6/R6] Решение пользователя по 43115cf/5c1142f (гейт красный).

**P1:**
6. [N4+R5/K5] release.yml: убрать `if: push` с CHANGELOG guard (или дать
   manual dispatch эквивалентную проверку) И сделать guard крейт-осведомлённым
   (для member-крейтов — либо свои changelog, либо явный skip с обоснованием).
7. [K2] Датировать CHANGELOG 0.3.0 только после пунктов 1–5.
8. [K9] Прогнать package/publish dry-run для КАЖДОГО публикуемого member'а
   (в порядке DAG), не только root.
9. [K10/K11] Флейки и unsafe-coverage — как в тасках.

**P2/P3:** K12–K18 без изменений; плюс N6 (закоммитить/убрать untracked
ревью), N7 (обновить формулировку K7), R13 (CLAUDE.md «five» → «six»).

## 6. Что этот аудит НЕ покрыл (повтор для честности)

Полные тестовые матрицы, miri/loom/TSan/kani, MSRV 1.88, настоящий
no_std-таргет, содержательная сверка CHANGELOG-текста (R2), flaky items
12/14, unsafe-seam coverage item 17, итоговый статус остальных джобов рана
31045983765. Всё перечисленное остаётся на тасках K2/K9–K11/K16 и на
пост-фикс прогоне CI.
