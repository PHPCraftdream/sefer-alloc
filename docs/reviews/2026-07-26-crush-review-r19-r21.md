# Crush review — Rounds 19–21 (tasks #337–#351)

**Reviewer:** Crush (read-only). **Date:** 2026-07-26.
**Scope:** `git log 46ea2db^..b6af12d` — **15 commits** (R19-1 … R21-2).
**Method:** `git show`/`git log` + чтение `src/`, `docs/perf/`, `tests/`, `Cargo.toml`,
`.github/workflows/ci.yml`. **Ничего не запускалось, не собиралось, рабочее
дерево не менялось.** Отчёт сформирован ДО чтения других ревью-файлов
R19–R21 (`2026-07-26-r19-r21-readonly-review.md`, `2026-07-26-oh-review-r18.md`
и т.д.) — независимые выводы ниже; расхождения с чужими ревью отмечены
отдельно в §6, если я их потом сверял.

> Замечание о нумерации. В постановке сказано «21 коммит»; фактический
> диапазон `46ea2db^..b6af12d` содержит **15** коммитов, и это полный охват
> R19-1..R21-2 / задачи #337–#351. «21» — артефакт приблизительного ориентира
> в задании, не пропуск в охвате.

---

## 1. Мы действительно ускорили код? — **НЕТ.**

Это самый важный и однозначный вывод. Из 15 коммитов диапазона **`src/`
трогают четыре**, и ни один не меняет поведение или производительность
`production`-сборки:

| Коммит | Файл(ы) `src/` | Характер изменения | Влияние на `production` |
|---|---|---|---|
| **R19-1** `46ea2db` | `registry/heap_core_free.rs` | Correctness/security-фикс (hardened-only consistency-gate) | **0** — `cfg!(feature="hardened")` ложен в `production`, новый `if` short-circuit'ит в `true`, доп. работы нет |
| **R19-8** `e7dbe16` | `registry/heap_core_free.rs` | Чистый рефакторинг (макрос `medium_promotion_reachable!`, 4 сайта `#[cfg]`) | **0** — раскрытие макроса байт-в-байт совпадает со старым предикатом |
| **R19-9** `4ca952d` | `alloc_core/dirty_by_class.rs` | **Только комментарий** (переписан literal→формула) | **0** — код не тронут |
| **R21-2** `b6af12d` | `alloc_core/alloc_core.rs`, `alloc_core/alloc_core_core_diag.rs` | Observation-only счётчики `OPT_H_ATTEMPTS`/`OPT_H_HITS` | **0** — весь блок предусловий обёрнут в `#[cfg(feature="alloc-stats")]`, которого в `production` нет |

**Состав `production` не изменился.** Проверено напрямую:
`git log --oneline 46ea2db^..b6af12d -- Cargo.toml` возвращает только один
коммит — `517a85b` (R21-1), и он добавляет лишь `[[example]]`-блок для
бенчмарка, не трогая `[features]`. Текущий список
(`Cargo.toml:399`):
`production = ["alloc-global","alloc-xthread","alloc-decommit","fastbin","alloc-segment-directory","primordial-lazy-commit","class-aware-dirty"]`
— идентичен тому, что было до диапазона.

Остальные 11 коммитов — чисто docs/test/bench:
- R19-2/3/4/5/6 — doc-фиксы (OPEN_ITEMS, R18-7 §7, citation в 912740f,
  CHANGELOG Round 18, методология R14-4);
- R19-7 — фикс watchdog-теста (`tests/race_repro.rs`);
- R19-9 — + tripwire-тест (см. выше, src-часть = комментарий);
- R20-1/2/3/4 — doc-фикс + 3 дизайн/measurement-отчёта;
- R21-1 — bench-harness (`examples/`, без `src/`).

**Итог по Q1.** Диапазон — это преимущественно process/doc/design-работа
плюс **один** реальный code-level эвент: R19-1. И R19-1 — это
**correctness/security-фикс (UAF под hardened), а не perf-оптимизация**,
что в commit-сообщении и в комментариях маркировано честно и точно. Никакого
ускорения (или замедления) production-кода в R19–R21 нет. «Стадия измерения
→ NO-GO» (R21-2) — это валидный отрицательный результат процесса, но он
точно так же не улучшил производительность.

---

## 2. Что ещё можно сильно ускорить? — независимая оценка vs `OPEN_ITEMS.md`

Моя оценка в целом **совпадает** с тем, что зафиксировано в
`docs/perf/OPEN_ITEMS.md`, с двумя уточнениями.

### Совпадает

- **[A] пункт 1 (in-place medium grow / OPT-H) только что получил NO-GO** —
  и это **правильный** вердикт на текущих данных. См. §3 ниже за разбор
  структурной причины.
- **[A] пункт 2 (mimalloc Ir-arm) — теперь высший приоритет.** После NO-GO
  на OPT-H это единственный оставшийся рычаг с реальным аргументом: 10 раундов
  wall-clock-спора о холодном 16 B не разрешим без детерминированного
  кросс-аллокаторного `Ir`. R20-4 доказал FEASIBLE (mimalloc-С стически
  линкуется в тот же бинарник, что Callgrind инструментирует; паттерн
  «без `#[global_allocator]`, прямой `GlobalAlloc::alloc`» уже есть в
  `benches/global_alloc.rs`; C-тулчейн уже ретирован зелёным `clippy
  --all-features` job на том же `ubuntu-latest`). **Это следующий шаг.**

### Уточнение, которого в OPEN_ITEMS нет (или оно смазано)

**OPT-H структурно не может помочь medium-realloc-gate'у — и framing
«genuinely un-promoted harness» в OPEN_ITEMS пункт 1 это слегка затушёвывает.**

Проследил control-flow лично (`heap_core_free.rs:908–999`):
`HeapCore::realloc` вызывает `try_realloc_inplace_known_base` (шаг 2, где
живут OPT-G/OPT-F/**OPT-H**) **ДО** `try_promote_to_large` (шаг 2.5). Значит
OPT-H действительно получает «first refusal». Но `try_promote_to_large`
срабатывает при `new_size >= MEDIUM_REALLOC_PROMOTION_THRESHOLD = 256 KiB`,
а **256 KiB — это и есть самый младший medium-класс** (`size_classes.rs`
EXTRAS). Следовательно **любой** cross-class grow *внутри* medium-лестницы
имеет `new_size ≥ 256 KiB` → после отказа OPT-H (None) немедленно уходит в
promotion. OPT-H может перехватить только grow, у которого `new_size < 256 KiB`
— т.е. **обычный small-class grow**, что есть «incidental secondary benefit»
из R20-3 §3, а не то, что закрывает R10-2.

Отсюда: даже если построить «genuinely un-promoted, walks-the-Small-ladder»
harness (единственный неопробованный вариант в OPEN_ITEMS), он докажет
лишь, что OPT-H помогает **small-class** (sub-256 KiB) grow'ам — это **не**
адресует medium→Large realloc-gate вообще. Фраза в OPEN_ITEMS
«the one remaining unexplored variant if this lever is revisited» технически
корректна, но создаёт ложное впечатление, что есть ещё неопробованный путь
к R10-2-gate'у через OPT-H. По моему анализу — **нет**: medium-классы и
promotion-threshold совпадают по построению, и это структурный, а не
геометрический предел. Стоит зафиксировать это явно, чтобы будущий раунд не
тратил усилия на очередной harness, обречённый на 0% по той же причине.

### Что список пропустил (независимо найдено)

1. **Flaky-тест `canary_survives_promotion_and_free_leaves_no_leak` нигде
   не отслежен.** R19-1 оставил его «as a follow-up» (валидирован flaky на
   pristine-коде, 1 падение из 3), но в `OPEN_ITEMS.md` его нет — а
   `OPEN_ITEMS` по своей же конвенции (§Scope) покрывает только
   `docs/perf/*.md`, т.е. correctness/flaky-долг в него по design'у не
   попадает. **Это та же failure-mode, ради которой OPEN_ITEMS создавался**
   (R14-4's open item завис на 3 раунда), только для correctness-домена.
   См. §4 — нужен параллельный durable correctness/flaky-index.

2. **`hardened medium-classes` — CI-dark комбинация**, в которой жил реальный
   UAF (R19-1). См. §3.1. Это не perf-рычаг, но это прямой пробел в
   coverage, который OPEN_ITEMS (perf-scope) структурно не может содержать.

Эти два пункта — не ускорения, а пробелы в safety-net'е; вынесены отдельно,
потому что вопрос «что улучшить» шире «что ускорить».

---

## 3. Что нужно улучшить в коде — постимальный разбор R19-1 и R21-2

### 3.1 R19-1 (`46ea2db`) — correctness-фикс: sound, но regression-тесты **тёмные в CI**

**Механизм фикса — верный, проверено независимо.**
- Премисс: `AllocCore::dealloc`'s Large-arm освобождает сегмент по `kind`
  (Large), игнорируя `layout`. Branch (A) рутил любой small-layout free
  Large-kind-сегмента в `self.core.dealloc` → под `hardened` фальсифицированный
  small-layout free **реально освобождал** never-promoted Large-сегмент
  (нарушение контракта task #25 «detected no-op»), т.е. **реальный UAF**.
- Фикс: под `hardened` гейтит реальный dealloc на
  `large_layout_consistent(base, layout.size())`
  (`alloc_core/deferred_large/layout_consistent.rs:68`) — той же примитивом,
  что cross-thread-путь. Mismatch → defensive no-op (как branch B).
- **Soundness позитивного пути подтверждён чтением:** `try_promote_to_large`
  (`heap_core_free.rs:1258`) вызывает `alloc_large(new_size, ...)`, который
  штампует `large_size == new_size`; OPT-G grow обновляет `large_size` через
  `set_large_size_at` (`alloc_core.rs:1852/1866`); shrink Large-блока
  возвращает None из inplace-пути → move-leg → блок покидает Large-сегмент.
  Поэтому `large_size` *всегда* равен размеру, который легитимный caller
  передаст в free → `large_layout_consistent` для легитимного free = true.
  Контрфакт «фикс не ломает correctness-required путь» — доказан структурно,
  не только тестом.

**Проблема 3.1.a — regression-тесты не запускаются в CI (РЕАЛЬНЫЙ пробел).**
`regression_hardened_large_kind_own_free.rs` — файл-level
`#![cfg(hardened, alloc-global, fastbin)]`; два новых branch-A-теста добавили
`#[cfg(all(feature="medium-classes", any(not(exact-span-large),
all(large-reserved-capacity, not(numa-aware)))))]`. Значит они компилируются
и едут **только** под `hardened medium-classes` (+ прод-preds). А в
`.github/workflows/ci.yml` job `test-hardened` гоняет ровно три комбинации:
`--features hardened`, `--features "production hardened"`,
`--features "hardened batch-api"` (ci.yml:153–177) — **ни одна не включает
`medium-classes`**. Еженедельный `cargo hack check --feature-powerset
--depth 2 --no-dev-deps` (CLAUDE.md) делает только `check` и с
`--no-dev-deps`, т.е. integration-тесты из `tests/` он вообще не компилирует.
**Итог: тесты, пинирущие конкретный UAF, который этот коммит чинит, в CI
никогда не запускаются и не проверяются.** Коммит это честно признаёт
(«that combo is simply outside today's CI feature matrix, also a follow-up»),
но «follow-up» не попало ни в один durable index (см. §4).

Это ровно тот же класс бага, ради которого завели cargo-hack-powerset
(R13-12/#285: баг, reachable только в непротестированной feature-комбинации) —
но powerset делает `check`, а не `test`, и `--no-dev-deps` исключает
`tests/`. Для correctness-фикса этого недостаточно. **Рекомендация:**
добавить в `ci.yml` job `test-hardened` шаг
`cargo test --features "production hardened medium-classes" --no-fail-fast`
(или хотя бы `hardened medium-classes`), чтобы branch-A-тесты R19-1
регрессионно ехали. Это дешёвый шаг (один `cargo test`), закрывающий пробел
напрямую.

**Проблема 3.1.b — вложенный control-flow чуть избыточен (косметика).**
В branch (A) после `if !cfg!(hardened) || consistent { … dealloc; return; }`
стоит ещё один безусловный `return;` (`heap_core_free.rs:437`) для mismatch-case.
Оба `return;` нужны (первый — после реального dealloc внутри `if`, второй —
после решения о no-op), но два последовательных выхода из одной ветки
читается тяжело. Не баг; при желании можно свернуть в
`if !cfg!(hardened) || consistent { unsafe { self.core.dealloc(ptr,layout) } }`
с одним `return;` в конце ветки (dealloc then fall-through return). На
безопасность не влияет.

**Замечание 3.1.c — вердикт «реальный UAF» обоснован.** Коммит утверждает,
что revert фикса + branch-A no-op-тест под `hardened medium-classes` падает с
`STATUS_ACCESS_VIOLATION` (`large.read()` трогает unmap'нутую память).
Я не могу это воспроизвести (read-only), но структура теста
(`dbg_owner_id_for(large).is_some()` после illegitimate free — реальный
liveness-signal, не payload-byte-read) и логика фикса делают утверждение
правдоподобным и verifiable. Принимаю как INFERRED (подтверждается
контрфактом автора, не моим воспроизведением).

### 3.2 R21-2 (`b6af12d`) — observation-only счётчики на realloc hot path: корректны и добросовестны

**Где сидит код — проверено.** Новая ветка — внутри
`AllocCore::realloc_inplace_fast_path_known_base`, в `else`-плече после
`if new_class == old_class { return Some(ptr); }` (OPT-F decline),
т.е. только для cross-class grow Small/Primordial-сегмента
(`alloc_core.rs:1882–1998`). Вся ветка под `#[cfg(feature="alloc-stats")]`;
функция **всегда** падает в предсуществующий `None` — никакой `Some` не
возвращается, `bump`/bitmap/BinTable не трогаются. Под `production`
(без `alloc-stats`) блок компилируется в nothing — подтверждено чтением
feature-списка (`production` не содержит `alloc-stats`). **Утверждение
«zero behavior change» — CORRECT.**

**Preconditions верны относительно дизайна R20-3 §2.1.** Сверил:
- prec 1 (`block_size(new_class) > block_size(old_class)`) → bump ATTEMPTS;
- prec 3 tail-adjacency `off + old_block_size == meta.bump_of()` — reuses
  `bump_of` (тот же single-field read, что `carve_block`);
- prec 4 alignment `off.is_multiple_of(new_block_size)`;
- prec 5 capacity `off + new_block_size <= SEGMENT`;
- prec 6 `lazy_commit_frontier_ok = true` — **честно задокументированный
  overcount** для lazy-commit-сборок (коммент в `alloc_core.rs:1940+`
  объясняет, почему это принято для observation-only). Для Stage-1
  (zero behavioral consequence) — приемлемо.

**Тест действительно дискриминативный.** Положительный сценарий
(768→1024 KiB grow у tail-блока на offset 3 MiB, 1 MiB-aligned, fills SEGMENT)
→ `dbg_opt_h_hits()` +1; отрицательный (тот же grow у non-tail блока) →
attempts +1, hits +0. Коммит описывает два независимых контрфакта
(force `tail_adjacent=true`, force `new_class_aligned=true`) — серьёзная
rigor для observation-only кода.

**Root-cause 0% hit-rate — проверен и внутренне непротиворечив.**
Я сначала заподозрил противоречие в OPEN_ITEMS («OPT-H's code path is reached
only once per round» vs «0/20 attempts»), но control-flow-трассировка
разрешила его: OPT-H (шаг 2) идёт **до** promotion (шаг 2.5), поэтому на
первом grow'е каждого раунда (256→320) ATTEMPTS инкрементируется (→ 20 за
20 раундов), но carve-position не 320 KiB-aligned → prec 4 fail → 0 HITS.
«0/20» = 0 hits / 20 attempts. **Утверждение корректно.** Это также
опровергает мою первичную гипотезу о том, что promotion перехватывает до
OPT-H — нет, OPT-H получает first refusal, но геометрия не складывается.

**Мелочь 3.2.a.** `SegmentMeta::new(base)` конструируется даже когда
`tail_adjacent` уже заведомо ложен (напр. offset мал). Это лишний
`bump_of`-read на каждом cross-class grow под `alloc-stats`. Поскольку под
`production` блок absent — не perf-значимо; под `alloc-stats` (диагностический
билд) приемлемо. Не менять.

**Итог по R21-2:** код добросовестный, observ-only-контракт выдержан,
тест реальный. Замечаний по существу нет.

---

## 4. Что улучшить в проекте (процесс/методология)

**P1 — CI-dark `hardened medium-classes` (blocker-класс пробела).**
Подробно в §3.1.a. Рекомендация: добавить шаг в `test-hardened` job. Это
самое ценное процессное действие из всего ревью — реальный UAF жил в
комбинации, которую CI не тестировал, и regression-тест фикса тоже не ездит.
cargo-hack-powerset (check-only, --no-deveps) этот пробел **не** закрывает.

**P2 — durable correctness/flaky-index, параллельный `OPEN_ITEMS.md`.**
`OPEN_ITEMS` по своей конвенции (§Scope) покрывает только `docs/perf/*.md` —
намеренно, и это правильно для его роли. Но следствие: correctness/flaky-долги
(вроде R19-1's flaky `canary_survives_promotion_and_free_leaves_no_leak`,
оставленного «follow-up» без записи) **не имеют** session-surviving
index'а. Это ровно та failure-mode, ради которой OPEN_ITEMS создавался
(R14-4-item завис незамеченным через раунды 15–17), перенесённая в
correctness-домен. Нужен параллельный файл (напр.
`docs/correctness/OPEN_ITEMS.md` или расширение конвенции OPEN_ITEMS на
correctness/flaky-флаги из commit-сообщений). Минимум: завести запись про
этот flaky-тест сейчас, чтобы он не завис так же.

**P3 — framing «un-promoted harness» в OPEN_ITEMS пункт 1 (см. §2).**
Зафиксировать явно структурный предел: OPT-H не может адресовать
medium-realloc-gate, потому что promotion-threshold (256 KiB) совпадает с
младшим medium-классом. Иначе будущий раунд рискует построить третий harness,
обречённый на 0% по той же причине, что и R21-1.

**P4 — хорошее (подчеркнуть).** Measure-before-build дисциплина
(Stage-1 observation → NO-GO) сработала именно так, как задумано:
R20-3 (CONDITIONAL-GO дизайн) → R21-1 (harness built-to-order) → R21-2
(0% hit-rate, честный NO-GO). Отрицательный результат зафиксирован с
полным evidence-trail, OPEN_ITEMS обновлён в том же коммите, механизм не
реализован. Это образцовый процесс для «дешёвой проверки перед дорогой
реализацией». Также хорошо: R19-1 исправил branch-(A) doc-комментарий,
который неправильно утверждал, что free на contract violation «correct» —
такой stale-comment в hotspot'е dealloc'а опасен, и поправить его было
правильно.

**P5 — макрос `medium_promotion_reachable!` (R19-8): одобряю с оговоркой.**
Дедупликация 4-х копий предиката — реальное снижение drift-риска. Оговорка:
2 сайта (SegmentKind import cfg, branch B cfg) остались hand-written, т.к.
`#[cfg]` не принимает macro-invocation — это задокументировано честно, но
значит macro **не** исчерпывает цель «single source of truth»; будущий
редактор должен держать их в sync by inspection. Текущий cross-reference
комментарий на обоих сайтах есть — достаточно. Альтернатива (build.rs
cfg-alias) явно отклонена по scope — разумно.

---

## 5. Сводный вердикт

| Коммит | Класс | Оценка |
|---|---|---|
| R19-1 | correctness/security UAF-fix | **GO** — sound; единственная реальная проблема — regression-тесты тёмные в CI (§3.1.a) |
| R19-2..6 | doc-фиксы | GO — рутинная гигиена OPEN_ITEMS/citations/CHANGELOG |
| R19-7 | test-watchdog hardening | GO — `!std::thread::panicking()` guard корректен, закрывает реальный «panic = noise» gap |
| R19-8 | refactor (macro) | GO — честная дедупликация, 2 сайта осознанно оставлены |
| R19-9 | comment-fix + tripwire | GO — v4-tripwire реальный (counterfactual описан) |
| R20-1..4 | doc/measurement/feasibility | GO — R20-2 (NULL) и R20-4 (FEASIBLE) — качественные evidence-отчёты |
| R21-1 | bench-harness | GO — harness-only, но см. §2: не реализовал target-pattern (0% по конструктивной причине) |
| R21-2 | observation-only counters, NO-GO | **GO** — корректный observ-only код, добросовестный тест, честный отрицательный вердикт |

**Диапазон в целом:** workmanlike, process-дисциплинированная волна без
ускорения, но с одним важным security-фиксом (R19-1) и одним
data-driven-отказом (R21-2 NO-GO). Главные риски — не в коде, а в
coverage-net: `hardened medium-classes` тёмная в CI (P1) и
correctness/flaky-долги без durable index (P2).

---

## 6. Свёрка с чужими ревью (post-hoc, после написания §1–5)

*Заполняется после того, как независимый отчёт выше зафиксирован; секция
явно помечена post-hoc, чтобы не загрязнять независимый анализ.*

(Не сверял на момент записи — отчёт §1–5 сформирован до чтения
`2026-07-26-r19-r21-readonly-review.md` и прочих. При желании свёрку можно
добавить отдельным комментарием; намеренно оставляю ядро независимым.)
