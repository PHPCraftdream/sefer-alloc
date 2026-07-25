# Независимое READONLY ревью — Rounds 13–17 (`e2d84f7^..HEAD`)

**Режим:** строго read-only. Ничего не собиралось и не запускалось — ни
`cargo`/`npm`, ни бенчей. Все числа взяты ИСКЛЮЧИТЕЛЬНО из уже закоммиченных
файлов: `CHANGELOG.md`, `docs/perf/*.md`, `_raw_*.log`, `Cargo.toml`, исходники.
Где есть только подозрение без доказательства — помечено явно.

**Пройденный диапазон:** `git log --oneline e2d84f7^..HEAD` = 65 коммитов,
Round 13 (`e2d84f7`, R13-1) через Round 17 (`cbebd45`/`a99314b`), задачи
#272–#327. CHANGELOG §Round 13..§Round 17 прочитан целиком.

Два уже существующих ревью этого же диапазона
(`docs/reviews/2026-07-25-oh-review-r13-r17.md`,
`docs/reviews/2026-07-25-r17-readonly-review.md`) **НЕ читались до формирования
собственных выводов** — сверка с ними вынесена в отдельный раздел §6 в конце.

---

## 1. Мы действительно ускорили код?

**Коротко:** для `--features production` — почти нет. Единственный подтверждённый
production-выигрыш за это окно — это **startup-cost** фикс R17-3 (онче-на-процесс
константа, не steady-state). Единственная фича, промотнутая в `production`
(`class-aware-dirty`, R13-9), **не имеет подтверждённого full-round ускорения**
после четырёх независимых замеров. Все крупные, статистически твёрдые выигрыши
этой эпохи — opt-in и не входят в `production`.

`Cargo.toml:399`:
```text
production = ["alloc-global", "alloc-xthread", "alloc-decommit",
               "fastbin", "alloc-segment-directory", "primordial-lazy-commit",
               "class-aware-dirty"]
```
Этот список **байт-в-байт неизменен с Round 13** — CHANGELOG §R14/§R15/§R16/§R17
каждый раз это подтверждает явно (`CHANGELOG.md:214-216`, `:318-319`, `:437-438`).

### 1a. Подтверждённые ускорения production-пути (с числами из gate-отчётов)

- **R17-3 (`b8612bc`)** — два `MAX_SEGMENTS`-масштабированных zero-fill цикла в
  `bootstrap::primordial()` ушли под `#[cfg(miri)]`. `npm run iai` на 12-бенчовом
  production-суите: **ровно −81,966 Ir на КАЖДОМ бенче** (нулевой разброс по всем
  12 — `R17_3...md:139-152`), bootstrap-прокси `large_alloc_free_cycle`
  `85,274 → 3,308 Ir`. Число детерминированное (Callgrind, не шумный wall-clock).
  **НО это one-time bootstrap-cost, не per-op:** CHANGELOG явно отмечает
  «every bench's `Ir/op*` is unchanged» (`CHANGELOG.md:72`, `R17_3...md:25,161-166`).
  Т.е. steady-state marginal `Ir/op` не сдвинулся ни на одном бенче.
- **`alloc-segment-directory`** в `production` с Round 8 (`ec7ac34`) — это
  фон, не достижение R13–17, но единственный однозначный всё ещё стоящий
  production-выигрыш в истории кодобазы.

### 1b. Реальные opt-in ускорения ВНЕ `production` (крупные, статистически твёрдые)

- **R14-6 (`8265b1c`)** — growth factor `large-reserved-capacity` 2×→4×:
  **инвертировал** R13-6-овскую регрессию `realloc_grow` с `+102.3% Ir` /
  `+52.7% Est. Cycles` до **`−22.44% Ir` / `−36.17% Est. Cycles`** vs plain
  `production`, RSS-выигрыш 15.80×→1.06× при 260 KiB сохранён
  (`R14_6...md:35-39,120-127`). **Gate: GO, но рекомендация only** — остался
  opt-in; пользователь явно подтвердил «keep as-is» (Round 16,
  `CHANGELOG.md:303-306`). Реальный выигрыш сознательно НЕ промотнут.
- **`exact-span-large`** (R12-3): RSS-амплификация 15.78×→~1.05× — opt-in.
- **`large-cache-extended`** (R13-7): 88.89%→100% hit rate, ~23 437→237 ns/op
  (~99×) на 9-размерном overflow-workload; CONDITIONAL-GO (R14-5); opt-in.
  Дефолтный бюджет в R17-9 урезан 1280→256 MiB/heap
  (`CHANGELOG.md:174`, `large_cache_config.rs`).
- **R14-4 medium-alloc-promotion** (R14-4, `3fde9f9`): +0.035% Ir (нет
  регрессии), но **не прошёл R10-2 kill-gate** для 16-live/8-slot-workload
  (`R14_4...md §7`). Opt-in (`medium-classes` ∉ `production`).

### 1c. Заявленные, но НЕ подтверждённые ускорения — флагман: `class-aware-dirty`

`class-aware-dirty` (R13-9, `da77b38`) — **единственная** фича, промотнутая в
`production` за эту эпоху. Заголовок «21.71×» был **sub-window** метрикой
(таймер только узкого окна внутри раунда), а не full-round mean criterion.
R14-3 (`6d85db4`) поправил рамку: full-round на том же харнессе на N=8 сдвинулся
только на ~11% (на N=4 ~1.6%) — «бóльшая часть sub-window-выигрыша это
deferred drain work, переместившийся в неизмеряемую pre-alloc/recycle часть
раунда, а не исчезнувший» (`R14_3...md:16-42`).

Затем честный fixed-work process-level A/B протокол прогонялся **четырежды**:

| Замер | mean(off) | mean(on) | paired t | значимо? |
|---|---:|---:|---:|---|
| R14-3 run 1 (`R14_3...md:128-132`) | 57.38 ms | 62.45 ms | −2.148 | **да — ON МЕДЛЕННЕЕ** |
| R14-3 run 2, независимый повтор | 99.49 ms | 122.01 ms | −1.404 | нет |
| R17-7 warm-up run 1 (`_raw_r17_7_warmup_ab_run1.log:407`) | 53.39 ms | 54.62 ms | −0.714 | нет |
| R17-7 warm-up run 2 (`_raw_r17_7_warmup_ab_run2.log:407`) | 53.46 ms | 54.71 ms | −0.683 | нет |

**Ноль из четырёх подтверждает full-round production-ускорение в выигрышную
сторону; одно из четырёх перешло границу значимости в проигрышную.** Все
same-vs-same контроли прошли — харнесс исправен, это подлинное «не знаем».

`class-aware-dirty` остаётся в `production` «на основаниях recoverability (он
закрывает R13-1 lost-wakeup класс), НЕ на подтверждённом ускорении»
(`CHANGELOG.md:136-139`).

**Моё уточнение к формулировке «recoverability» (см. также §3.5):**
lost-wakeup-класс, который R13-1 закрывает, **сам существует только потому, что
включён `class-aware-dirty`** — coarse per-segment dirty-битмап (baseline без
фичи) не имеет sidecar, который может пропустить push. Т.е. фича в production
оправдывается устранением риска, которого без неё просто нет. Это более слабое
оправдание, чем звучит по первому прочтению «recoverability grounds».

### 1d. Исправленные регрессии («быстрее, чем до регрессии» ≠ «быстрее, чем всегда»)

- **R17-3 — действительно «быстрее, чем когда-либо»** (см. §1a): циклы никогда
  не гейтесь ни при каком `MAX_SEGMENTS` (`CHANGELOG.md:65-66`), так что
  −81,966 Ir = pre-existing baseline (~20,480 Ir при `MAX_SEGMENTS=4096`,
  присутствовавший с Task #135) **плюс** raise-increment R14-7 (61,440 Ir).
  Сегодняшний bootstrap-cost ниже, чем в любой точке истории проекта.
- **R15-3 (`4de4ef2`)** — чистое восстановление регрессии: compile-time гейт
  выключает `try_promote_to_large` в zero-headroom тройной комбинации, возвращая
  pre-R14-4 поведение. Не новый выигрыш.
- **R17-4 (`1b761f4`)** — устраняет дефект, который R14-4 и внёс (leak не
  существовал до Stage-2 promotion), так что ~68 MiB post-fix = «как задумано
  при дизайне R14-4», а не новый потолок ниже истории. Живёт под opt-in
  `medium-classes`.

**Вердикт по Q1:** R13–17 — это преимущественно **correctness/soundness-hardening
дуга** (закрытие unsafe-boundary R14-1/R14-2/R14-9/R15-2/R15-4/R17-1/R17-2,
реальный segment-leak R17-4, CI-провалы R13-5/R13-12/R16-1, doc-drift фиксы
R15-5/R16-2/R16-5/R17-6), **не performance-дуга**. Реальные крупные выигрыши —
opt-in. Единственный production-выигрыш — one-time startup-cost (R17-3).

---

## 2. Что ещё можно сильно ускорить?

### 2.1 Оценка R17-10 (batched deferred reclaim) — согласен с самооценкой

`docs/perf/R17_10_BATCHED_DEFERRED_RECLAIM_DESIGN.md` честно корректирует
собственный premise: `sync_directory_for_segment_classes` уже батчится до
одного вызова на сегмент на drain-визит с R8-1 (Task #214) — этого gap-а нет
(`R17_10...md:53-91`). Реальные, более узкие gap-ы: (A) per-block
`dec_live_and_maybe_decommit` вместо существующего proven-identical батч-брата
`dec_live_batch_and_maybe_decommit` (E3/Task W4, `alloc_core_small_pool.rs:115`),
и (B) отсутствие cross-segment батчинга финализации внутри одного sweep.

Самооценка «constant-factor tightening, not an algorithmic class change»
(`R17_10...md:191-193`) **точна, не ложная скромность.** Sub-design A экономит
`(N−1)` decrement+compare на сегмент, где каждый уже O(1) — однозначно
single-digit-nanosecond; а `class-aware-dirty` со своим подлинно алгоритмическим
O(D)→O(D_class) выигрышем **не пробил значимость** при N=20. Двухстадийный план
«сначала померить counter (Stage 1), строить механизм только если Stage 1
оправдает» (`R17_10...md:467-486`) — правильная дисциплина. **Согласен, что это
низко-умеренный потенциал.**

### 2.2 Самая крупная всё ещё открытая цель — cold-carve gap vs mimalloc

README.md:720-725 документирует **актуальный, не закрытый perf-gap:**
- Cold direct 16 B: **2.37× медленнее** mimalloc (70.9 ns vs 30.0 ns)
- Cold direct 256 B: **2.71× медленнее** (84.5 ns vs 31.2 ns)
- Cold direct 64 B: parity (1.00×)

`docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md` содержит уже готовый
instruction-level диагноз («flat ~28 µs instruction-bound, платим за ceremony»)
и пять «эврик» (Э1–Э5) с цепочкой задач (#144–#149). Часть уже приземлилась
(Э1 bump-direct виден в `ci.yml`), но README-цифра показывает: фронт **не закрыт**.
Я не нашёл ни одного коммита R13–17, который этот план пересмотрел — он,
похоже, дормант примерно с Round 6–9. **Это значительно более крупная,
уже-диагностированная возможность, чем R17-10**, и она лежит на compared-фронте
проекта (README `## Measured gap`) как признанное поражение против mimalloc.

### 2.3 Самый сильный immediate-кандидат: перезапуск medium-gate после R17-4

R17-4 доказал, что старый R10-2 realloc kill-gate (который держал
`medium-classes` вне `production`) **замерял код с реальной утечкой 4 MiB Large
сегментов на цикл** (`R14_4...md §2.2`, `CHANGELOG.md:77-108`). После фикса
probe меняется с «0 cache hits / 249 segments / ~1 GiB commit» на
«~68 MiB commit / нормальный cache-reuse» (`R17_4...md` дополнение к
`R14_4...md`). Вердикт «1,700–2,300× медленнее» **больше не описывает
скорректированный код.** Полный R10-2 judge на corrected HEAD **не
прогонялся** — это самый дешёвый эксперимент с самым высоким потенциалом
обнаружить substantial production-speedup (medium alloc/free ранее давали
~31×/~211×). В сочетании с `large-cache-extended` (40 слотов хватит на
16 объектов R10-2 при нынешнем 256 MiB бюджете) это может перевернуть
текущий NO-GO на `medium-classes`.

### 2.4 Сужение R17-4 kind-проверки — см. §3.7 (подтверждено, что сужение sound)

### 2.5 Page-run layer (1.25–2 MiB плотность)

Корректно DEFERRED (`R12_13_PAGE_RUN_LAYER_DEFERRED.md`) — нет измеренной жертвы
в собственных tests/benches/examples. **Согласен с откладыванием;** поднимать без
измеренного workload повторило бы ошибку, от которой защищает дисциплина
«gate on measured pain».

---

## 3. Что нужно улучшить в коде?

### 3.1 (СПЕЦ-ВНИМАНИЕ #1) `STATUS_STACK_BUFFER_OVERRUN` — watchdog, а не corruption

`tests/race_repro.rs:44-63` документирует две независимые краша
`STATUS_STACK_BUFFER_OVERRUN` (0xC0000409) в
`drain_reclaim_uaf_repro_tight_handoff` под тяжёлой concurrent CPU-нагрузкой
(Round 14 / Task #289; Round 17 / Task #326), рабочая гипотеза —
«a rare Windows scheduler/stack-guard artifact, unconfirmed».

**Моя находка:** в том же файле есть watchdog, который намеренно вызывает
`std::process::abort()` после 20-секундного дедлайна:
- `DEADLINE_SECS = 20` (`tests/race_repro.rs:87`)
- цикл ждёт до 20 c, затем `eprintln!(...)` и `std::process::abort()`
  (`tests/race_repro.rs:101-112`)

На Windows/MSVC `std::process::abort()` реализуется через `__fastfail` (механизм
Rust stdlib, не код этого репо — **INFERRED из документированного поведения
Rust/Windows, не подтверждено прогоном в этом репо**), и возникающее исключение
несёт код **ровно `STATUS_STACK_BUFFER_OVERRUN` (0xC0000409)**. Условия обоих
наблюдений совпадают с watchdog-гипотезой **полностью:** обе краша случились
под тяжёлой concurrent нагрузкой, именно тогда, когда нормальный стресс-тест
может превысить фиксированный 20-сек дедлайн. Это гораздо более вероятное
объяснение, чем редкая allocator-corruption, при которой checksum-oracle
(не-вакуумный сигнал порчи, `tests/race_repro.rs:223-227`) оставался зелёным
во всех ~100 воспроизведениях.

**Статус:** OBSERVED (watchdog с abort и 20-c дедлайном — факт в коде) +
INFERRED (что abort именно даёт 0xC0000409 — из документированного поведения
платформы, не из прогона). Это сильно более правдоподобный механизм, чем
«средовой артефакт планировщика».

**Проверяемый refutation-тест, который, судя по отчётам, НЕ был проведён:**
watchdog делает `eprintln!(...)` в строках 107-111 **ДО** `abort()` (строка 112) —
диагностическая строка «TEST EXCEEDED 20s ... Aborting process» оказалась бы в
stderr упавшего теста. Если эта строка **отсутствует** в выводе краша — это
свидетельство против watchdog-гипотезы (хотя под тяжёлой нагрузкой line-buffered
stderr может не сброситься до `__fastfail`, что ослабляет, но не убивает тест).
Если **присутствует** — подтверждение. Этот тест checkable из уже существующих
логов краша; ни R17-9-расследование, ни CHANGELOG не упоминают его проверку.

**Рекомендация:** заменить `std::process::abort()` на однозначно различимый
timeout-exit (напр. `process::exit(124)` — конвенционный timeout-code), сделать
дедлайн конфигурируемым (увеличивать на загруженных CI), выводить elapsed+progress
перед завершением, и не игнорировать результат `join()` watchdog-треда (сейчас
`let _ = h.join()`, `race_repro.rs:125`).

### 3.2 (СПЕЦ-ВНИМАНИЕ #2) «Pad-target decision» — внутреннее противоречие

`src/registry/heap_core_free.rs:936-978` («## Pad-target decision»). Секция
аргументирует выбор `nopad` (без искусственного padding):
- строка 938: «The padded target is simply `new_size` — **no artificial padding**»
- строка 968: «The pad-target decision (`nopad`, **no padding**) therefore stands»

НО в той же секции, строка 972-973: **«Padding is default»** — это прямо
противоречит «no artificial padding» и «`nopad`» двумя предложениями выше.

Это явный typo/пропуск: имелось в виду, очевидно, «**No** padding is default»
или «The default is no padding» — отсутствует отрицание в начале предложения.
Как написано, текст сам себе противоречит в рамках одной документирующей
комментарий-секции. **OBSERVED, текстуально.**

### 3.3 (СПЕЦ-ВНИМАНИЕ #3) `kind_at(base)` безусловно на каждый small free под `medium-classes`

R17-4-фикс (`src/registry/heap_core_free.rs:244-261`):
```rust
#[cfg(feature = "medium-classes")]
{
    if SegmentHeader::kind_at(base) == SegmentKind::Large {
        ... self.core.dealloc(ptr, layout); return;
    }
}
```
`kind_at(base)` (`segment_header_views.rs:22-56`) — это `#[inline(always)]` + один
byte-load по `offset_of!(SegmentHeader, kind)` от `base`. Этот блок стоит
**после** `if let Some(c) = SizeClasses::class_for(size, align)` (строка 174), т.е.
срабатывает для **каждого** small/medium-классифицированного free под
`medium-classes`, включая 16/32/64-байтные освобождения, к промоции отношения
не имеющие.

**Структурно — да, безусловная лишняя стоимость, и её можно сузить.** Я проверил,
что сужение **sound** (в отличие от поверхностного подозрения):

1. Промоция в Large срабатывает только при `new_size >= MEDIUM_REALLOC_PROMOTION_THRESHOLD`
   (256 KiB) на **растущем** realloc (`heap_core_free.rs:781-782`).
2. OPT-G in-place Large **только растёт** (`alloc_core.rs:1701-1703`: «GROW or
   SAME size only ... A shrink falls through to the slow path, which reclaims
   RSS by moving the payload to a smaller segment/class»).
3. Значит realloc-**shrink** Large-блока идёт в move-leg: аллоцирует новый
   (меньший) блок и освобождает старый Large-сегмент. После shrink блок **уже
   не в Large-сегменте** (`heap_core_free.rs:929-933` подтверждает явно).
4. ⇒ Small/medium-классифицированный dealloc-layout **никогда** не соответствует
   Large-сегменту, **кроме** через промоцию+OPT-G-рост (который держит
   `size ≥ 256 KiB`).

**Вывод:** проверку `kind_at` корректно и безопасно гейтить по
`layout.size() >= MEDIUM_REALLOC_PROMOTION_THRESHOLD` перед чтением kind — все
frees меньше порога промоции физически не могут быть promoted-Large.

**Дополнительная находка (расширяет узость):** `#[cfg]` на этом блоке —
**голый `feature = "medium-classes"`** (строка 245), а не узкий
promotion-предикат `medium-classes && (!exact-span-large || (large-reserved-capacity
&& !numa-aware))`, под которым компилируется сама промоция (строки 774-780,
995-1001). Значит: под `medium-classes + exact-span-large` (без
`large-reserved-capacity`) промоция **выключена**, а kind_at-роутинг **всё ещё
компилируется и платится** на каждом small free — хотя в этой конфигурации
Large-сегмента-с-small-layout легитимно **не может существовать вовсе**
(без промоции нет производителя; прямой `alloc_large` даёт layout > SMALL_MAX →
`class_for` возвращает `None` → arm `Some(c)` недостижим). Правильный cfg должен
совпадать с promotion-предикатом 1:1.

**Оценка практической стоимости (калибровка):** `kind` — это байт по малому
offset от `base`; сразу после этого блока `SegmentMeta::new(base)` читает
bitmap/magazine-состояние того же сегмента (строки 407-408), так что cache-line
сегмент-хедера почти наверняка уже горячая. Практическая стоимость — вероятно
близка к нулю (один cache-hot byte-load). **Сужение структурно верно, но его
perf-выгода спекулятивна без замера.**

**И главное — почему это важно даже при малой стоимости:** R17-4-коммит
заявляет «npm run iai on production: identical to the pre-R17-4 baseline»
(CHANGELOG, commit-message) как доказательство «zero hot-path cost». **НО** iai
по умолчанию гоняется под plain `--features production` (`scripts/iai.mjs:60,69`),
где этот блок **вообще не компилируется** (нет `medium-classes`). Так что
заявленная «zero hot-path cost» верифицирована **только для конфигурации, где
код не существует**, а НЕ для `production medium-classes`, где он live.
iai-gate под `production medium-classes` отсутствует (`scripts/iai.mjs`,
`perf_gate_iai.rs` — `medium-classes` там не упоминается). Это реальный
proof-gap: сужение — не только микрооптимизация, но и **предпосылка для честного
medium-classes performance-гейта**, которого сейчас нет.

### 3.4 Unsafe-seam дисциплина — проверено, расхождений нет

Прогон собственной self-verifying команды CLAUDE.md
(`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`): **80 всего =
20 tier-1 (`#![...]`, module-level) + 60 tier-2 (`#[...]`, item-level).**
Точно совпадает с прогрессией CHANGELOG §R17
(`CHANGELOG.md:20-23`: «56 → 58 → 59 → 60» tier-2, tier-1 неизменен 20;
R17-1 +2, R17-2 +1, R17-4 +1 = +4, 56+4=60). Расхождений между заявленным
счётом и реальным grep — нет.

### 3.5 Архитектурные правила (CLAUDE.md)

- **Doctest-запрет:** `grep -rnE '```(rust|compile_fail|no_run|edition)' src/`
  → **0 совпадений** в `src/`. (Совпадения есть только в `crates/*/README.md` —
  это отдельные crate-README, не `src/**/*.rs`; правило про `src/**/*.rs`.)
  Правило чистое.
- **«One file, one export» / mod.rs без кода:** spot-check `mod.rs`-файлов
  (`src/alloc_core/mod.rs`, `src/registry/mod.rs`, `src/global/mod.rs`,
  `src/concurrent/mod.rs`) — логики/типов/функций не содержат, только
  `mod`/`pub use`. `src/global/mod.rs` содержит `//!` module-doc (допустимо — это
  документация, не код). Нарушений не найдено.
- **Tests в отдельной папке:** confirmed — `tests/` отделён от `src/`.

### 3.6 «Stale derived-number literal в комментарии» — рекуррентный класс

Это уже **четвёртый** задокументированный случай этого дефект-класса за R15–17
(R15-5: `WORDS_PER_CLASS=16`/`MAX_SEGMENTS=1024` stale в 5 файлах; R16-2:
mislabeled class-count; R17-5/R17-6: два выше). Корень — захардкоженный
производный литерал в комментарии вместо ссылки на именованную константу.
Автоматического guard-а против этого класса в CI нет (`no_stale_doc_references`
проверяет конкретные известные строки/счёты — tier-2 unsafe count, test-file
count — но не произвольные производные числа в произвольных комментариях). Это
реальный, рекуррентный, низко-серьёзный, но четырежды повторённый класс без
структурного фикса.

### 3.7 Прочие code-уровневые наблюдения

- **R17-1 (`70a8f2f`) и R17-2 (`f65015a`)** — закрытие unsafe-boundary выглядит
  sound при статическом чтении: `reserve_zeroed_with` теперь передаёт raw
  `*mut T` в fixup и не материализует `&mut T` над не-валидными байтами;
  `init_node_ids_raw` пишет через `addr_of_mut!`+`write`; directory raw-read
  helpers стали `unsafe fn` с call-site `// SAFETY:` комментариями.
- **Реальные баги, найденные и исправленные в эту эпоху** (для полноты, не
  open-items): R17-4 segment leak, R13-12 E0599 compile-error, R13-11 test-bug
  (не production-дефект). Все имеют red/green counterfactual.

---

## 4. Что улучшить в проекте/процессе?

### 4.1 R17-7: SD > delta — среда не различает эффект этой величины

В `_raw_r17_7_warmup_ab_run{1,2}.log` стандартные отклонения (7.713 ms, 8.175 ms)
**больше**, чем mean deltas (1.231 ms, 1.249 ms). На фоне raw-данных видны
массивные выбросы (run2: 14.3 ms / 88.0 ms / 91.7 ms при типичных ~53 ms; run1:
93.5 ms / 95.6 ms). При `paired t ≈ −0.7` и `crit = 2.101` это не столько
«эффект отсутствует», сколько **«измерительная среда принципиально не может
разрешить эффект такой величины»**. CHANGELOG честно раскрывает CPU-load
80–100%. Но отсюда следует: любое production A/B-решение по `class-aware-dirty`
нельзя принимать на этой машине в текущем её состоянии — нужен независимо-простой
runner. Важно: sub-window (`window_ns`) при этом стабильно НИЖЕ для on vs off
(~2.8M vs ~3.4M, `_raw_r17_7...`) — так что tail-latency-эффект в узком окне
реален и воспроизводим по направлению, даже если full-round throughput не
подтверждён.

### 4.2 R17-4 «висел открытым 3 раунда» — процесс-находка

Аномалия, которую R17-4 root-caused, сидела как явно названный open question в
`R14_4...md §2.2` с Round 14, пере- restated как ещё-open в §6 того же документа
через Round 16, и закрылась только в Round 17, потому что внешний review-план
явно назначил «root-cause the pad/cache admission anomaly» своей P1-задачей
(R17-4 в `docs/reviews/2026-07-24-r17-plan.md:29`). Ничто в механике бага не
потребовало бы человека/агента, читающего open question и решившего его
догнать — структурной/автоматической проверки нет.

**Лёгкий процесс-фикс:** единый running tracking-файл (или task-list-конвенция),
перечисляющий каждый «open item flagged in a gate report §6 / §Open items» по
всем `docs/perf/*.md`, проверяемый в начале planning-pass каждого нового раунда.
Текущий механизм (внешний review случайно перечитывает нужный файл) — ровно тот
ad-hoc механизм, что позволил R17-4 провисеть три раунда.

### 4.3 Stale doc-literal класс — нужен структурный guard (см. §3.6)

Четыре повтора одного дефект-класса без автоматической защиты — сигнал, что
«поймать руками в следующий раз» недостаточно. Минимальный guard: lint или тест,
который скрейпит `src/**/*.rs` на литералы вида `MAX_SEGMENTS\s*=\s*\d+` /
`WORDS_PER_CLASS\s*=\s*\d+` в `//`-комментариях и сверяет с определением константы.

### 4.4 iai-gate-coverage gap под `medium-classes`

(см. §3.3) — iai-суит верифицирует «zero hot-path cost» только под plain
`production`, где R17-4-бранч компилится-наружу. Под `production medium-classes`
(где он live) iai не гонялся. Перед любым medium-classes production-решением
нужен детерминированный instruction-gate под `production medium-classes` —
не только feature-off-невозмутимость.

### 4.5 perf-gate.yml — известный, сознательный non-blocking дизайн

`perf-gate.yml` гоняет iai-регрессионный гейт только на `schedule`/`workflow_dispatch`/
PR-с-меткой `perf`, никогда на обычном push/PR. С учётом того, что этот проект
коммитит напрямую в `main` по задаче (нет видимого PR-gating), production-path
регрессия, внесённая mid-round, не будет поймана до ближайшего nightly — до 24ч
дрейфа по дизайну джоба. Это известный, задокументированный, ясно-прокомментированный
tradeoff, не скрытый blind-spot — отмечаю для полноты.

### 4.6 Честность-инструментарий — главный актив проекта

Двуосевое wall-clock-правило (R14-3), same-vs-same контроль, раскрытие
environment-load (R17-7), «recommend, orchestrator decides» протокол промоушена —
всё это причина, по которой §1c-овский честный non-finding вообще виден в этом
репо, а не протухший заголовок «21.71×» в README. Round 17 в частности
перегнал замер (R17-7), который легко можно было пропустить (решение о промоушене
`class-aware-dirty` не пересматривалось), чисто чтобы закрыть P2
methodology-концерн, и честно отчитал null-результат. Эта дисциплина — то,
что позволило найти конкретные cited-доказательства для nuanced-ответа на Q1.

---

## 5. Сводная таблица

| Вопрос | Однострочный ответ |
|---|---|
| 1. Ускорили? | Для `production` — почти нет: один startup-cost выигрыш (R17-3, −81,966 Ir, «быстрее чем всегда»); единственная промотнутая фича (`class-aware-dirty`) — 0 из 4 подтверждений full-round; крупные реальные выигрыши — opt-in (R14-6 −22% Ir, exact-span, large-cache-extended). |
| 2. Следующий рычаг? | Самый дешёвый/высокопотенциальный: перезапуск R10-2 medium-gate после R17-4 (старый вердикт измерял дырявый код); крупнее и дормантнее — cold-carve gap 2.4–2.7× vs mimalloc (README:720-725, `PERF_PLAN_beat_mimalloc_small_medium.md`); R17-10 — низко-умеренный, его самооценка точна. |
| 3. Code-level? | (1) `STATUS_STACK_BUFFER_OVERRUN` — почти наверняка watchdog-abort, не corruption (§3.1, refutation-тест не проведён); (2) pad-target-комментарий сам себе противоречит «Padding is default» vs «nopad» (§3.2); (3) `kind_at` безусловно на каждый small free под medium-classes, сужение sound (§3.3) + cfg должен совпадать с promotion-предикатом; unsafe-inventory проверен (80=20+60); doctest/mod.rs-правила чисты. |
| 4. Проект/процесс? | Open-question-tracking отсутствует (R17-4 висел 3 раунда); stale-doc-literal класс — 4-й раз, без guard-а; iai-gate не покрывает medium-classes; SD>delta у R17-7 — среда не разрешает эффект; watchdog-выход неотличим от corruption. |

---

## 6. Сверка с двумя уже существующими ревью

Сформировав выводы выше, я прочитал `docs/reviews/2026-07-25-oh-review-r13-r17.md`
(@oh) и `docs/reviews/2026-07-25-r17-readonly-review.md`. Ниже — согласие /
несогласие / новое.

### 6.1 Соответствие трём спец-пунктам (где я СОГЛАСЕН)

| Спец-пункт | @oh-review | r17-readonly-review | Этот отчёт |
|---|---|---|---|
| #1 watchdog→abort | **НЕ нашёл** (§3.4: «no independent evidence either way», отметил что signature сильнее scheduling-hiccup, но не связал с watchdog) | **НАШЁЛ первым** (P1, строки 29-55): явно назвал watchdog/abort как «most likely explanation» | **НАШЁЛ независимо**, подтверждаю r17-readonly. Дополнительно: добавил refutation-тест (eprintln перед abort — checkable из существующих логов краша, не проведён) и строго разделил OBSERVED (watchdog/20-c — факт) vs INFERRED (abort→0xC0000409 — из документированного поведения платформы) |
| #2 pad-target противоречие | **НЕ упомянуто** | **НАШЁЛ** (P3, строки 155-162): «Padding is default» противоречит nopad | **НАШЁЛ независимо**, подтверждение дословное (строки 938/968 vs 972-973) |
| #3 kind_at безусловно | **НЕ упомянуто** | **НАШЁЛ** (P2, строки 93-122): предложил сужение `size >= threshold && promotion_is_compiled` | **НАШЁЛ независимо** + **подтвердил sound через проверку OPT-G «только растёт», shrink → move-leg** (alloc_core.rs:1701-1703) + **нашёл, что cfg голый `medium-classes` шире promotion-предиката** (компилится даже когда промоция выключена, хотя в той конфигурации Large-with-small-layout легитимно не существует вовсе) + **нашёл proof-gap**: «zero hot-path cost» R17-4 верифицирован только под plain production, где код не существует, а не под medium-classes, где он live |

### 6.2 Где я согласен с @oh (его уникальные находки)

- **Cold-carve gap как дормантный P1** (§2.2 @oh) — полностью согласен, это та
  же моя §2.2 (README:720-725, `PERF_PLAN_beat_mimalloc_small_medium.md`).
- **R14-4 open-item #2 (R10-2 gate re-run vs R14-5 hardened cache) провисел 3
  раунда** (§2.3 @oh) — согласен; но я усиливаю: после R17-4 этот re-run ещё
  важнее, т.к. R17-4 доказал, что старый kill-gate измерял дырявый код (моя §2.3;
  r17-readonly пришёл к тому же «re-run medium gate» как своему #1 независимо).
- **Stale-doc-literal как 4-й раз без guard** (§3.3 @oh) — согласен дословно.
- **Feature-powerset depth-2 покрывает R13-12-баг через feature-dependency-graph**
  (§4.2 @oh) — согласен с проверкой, корректная находка.
- **class-aware-dirty: 0 из 4 подтверждений** (§1c @oh) — согласен, моя §1c
  независимо пришла к той же таблице из тех же raw-логов.

### 6.3 Где я согласен с r17-readonly

- **Перезапуск medium-gate после R17-4 = highest-priority experiment**
  (P1/§1 r17-readonly) — согласен; моя §2.3 пришла туда же.
- **«recoverability» оправдание надо формулировать осторожно** (P2 r17-readonly,
  строки 141-144) — согласен и **поднимаю** в свою §1c/§3.5: lost-wakeup-класс,
  который R13-1 закрывает, существует только при включённом class-aware-dirty;
  baseline coarse-битмап в нём не нуждается. Это делает «recoverability» слабее,
  чем звучит.
- **R17-3 — startup-cost, не steady-state** (Confirmed r17-readonly) — согласен.
- **R17-10 — constant-factor, не radical** (§4 r17-readonly) — согласен, моя §2.1.

### 6.4 Что нашёл я, чего НЕТ ни в одном из двух ревью

1. **Refutation-тест для watchdog-гипотезы** (§3.1): eprintln перед abort —
   checkable из существующих логов краша, не проведён ни в одном расследовании.
2. **Доказательство sound-ности сужения kind_at** через проверку, что OPT-G
   «только растёт», а shrink Large → move-leg (alloc_core.rs:1701-1703) —
   r17-readonly предложил сужение, но не проверил формально, что
   Large-with-small-layout-ниже-порога структурно невозможен.
3. **Cfg на kind_at-блоке шире promotion-предиката** (§3.3): под
   `medium-classes + exact-span-large` (без reserved-capacity) промоция
   выключена, а kind_at-роутинг всё ещё компилируется и платится, хотя
   Large-with-small-layout легитимно не существует вовсе. Правильный cfg должен
   совпадать с promotion-предикатом 1:1.
4. **Proof-gap в «zero hot-path cost» R17-4** (§3.3/§4.4): iai верифицировал
   только plain `production`, где бранч компилится-наружу; под
   `production medium-classes` (где он live) iai-гейт отсутствует.
5. **SD > delta у R17-7** (§4.1): стандартные отклонения (7.7-8.2 ms) больше
   mean deltas (1.2 ms) — это не «эффект отсутствует», а «среда не разрешает
   эффект такой величины»; r17-readonly отметил «не значимо», @oh привёл
   таблицу t-статистик, но ни один не назвал SD-соотношение явно.
6. **Подчёркивание, что sub-window (`window_ns`) стабильно НИЖЕ для on vs off**
   в raw-логах R17-7 (§4.1): tail-latency-эффект в узком окне реален и
   воспроизводим по направлению, даже когда full-round throughput не подтверждён
   — это nuance, который оправдывает «retain as latency policy» (предложение
   r17-readonly P2) конкретными данными.

### 6.5 Где у меня НЕТ доказательств (только подозрение)

- **Точное поведение `std::process::abort()` → `STATUS_STACK_BUFFER_OVERRUN`**
  на конкретно этом тулчейне: INFERRED из документированного поведения
  Rust/Windows (`__fastfail`), не подтверждено прогоном в этом репо.
- **Page-run layer / chained SegmentTable как «крупные цели»**: согласен с обоими
  ревью, что они корректно DEFERRED/CONDITIONAL; не вижу измеренной жертвы,
  оправдывающей их подъём сейчас.

---

## Финальный вердикт

Rounds 13–17 — это качественная **correctness/soundness-hardening дуга** с
образцовой честностью-инструментарием (двуосевые замеры, same-vs-same контроли,
явное раскрытие среды), но **не performance-дуга для `production`**: единственный
промотнутый в production перф-фича (`class-aware-dirty`) не имеет подтверждённого
full-round ускорения после четырёх замеров; реальные крупные выигрыши — opt-in.

Самые ценные конкретные действия, не требующие новых дизайн-документов:
1. **Отличить watchdog-timeout от corruption** в `race_repro.rs` (§3.1) —
   тривиальное изменение `abort()`→`exit(124)`, снимающее неоднозначность.
2. **Перезапустить R10-2 medium-gate на corrected HEAD** (§2.3) — самый дешёвый
   эксперимент с самым высоким потенциалом (старый NO-GO измерял дырявый код).
3. **Сузить kind_at + выровнять cfg с promotion-предикатом** (§3.3) — структурно
   верно, и это предпосылка для честного medium-classes performance-гейта,
   которого сейчас нет.
