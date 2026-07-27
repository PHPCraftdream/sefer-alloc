# Read-only review post-Round-22 follow-ups (`610f915..bc4aacf`)

**Дата:** 2026-07-27  
**Режим:** только чтение истории Git, диффов и файлов. Сборка, тесты,
бенчмарки, примеры и скрипты не запускались. Закоммиченные заявления о
прогонах рассматривались как свидетельства авторов, но независимо не
перепроверялись.

Предыдущее полное ревью Round 22:
`docs/reviews/2026-07-26-r22-readonly-review.md`.

## 0. Итог

После последней проверенной точки `610f915` в `main` появились только два
коммита:

- `f2764f7` — исправление feature-dependent предположения в тесте OPT-H;
- `bc4aacf` — сериализация двух stats-sensitive тестов после реального CI
  flake.

Диапазон меняет три файла (`+330/-154`), и все они находятся в `tests/` и
`docs/`. `src/`, `Cargo.toml`, production features и runtime-код не менялись.

### Ускорили ли код этой новой волной?

**Нет.** В диапазоне нет ни одной runtime-оптимизации и ни одного нового
before/after perf gate для изменённого production-механизма.

Это полезная **test/CI stabilization wave**, но называть её ускорением
allocator нельзя. Все выводы предыдущего ревью о возможных направлениях
ускорения остаются актуальными.

## 1. Review findings

### P1 — follow-up ошибочно переиспользует занятые `R22-15 / task #366`

Новый commit `f2764f7` является post-Round-22 follow-up к task `#353`. Но
внесённые им комментарии шесть раз называют изменение:

```text
R22-15 (task #366)
```

Например:

- `tests/r21_2_opt_h_stage1_precondition_probe.rs:126`;
- `:199`;
- `:266`;
- `:320`;
- `:538`.

Однако `R22-15 / task #366` уже однозначно принадлежит commit `ff48029`:
добавлению mimalloc Ir arm. Это закреплено в:

- `CHANGELOG.md:113`;
- `docs/perf/R22_15_MIMALLOC_IR_ARM_GATE.md`;
- `docs/perf/IAI_BASELINE.md`;
- `docs/perf/OPEN_ITEMS.md`;
- Git history.

Таким образом, новая test-правка получила чужую идентичность и создаёт
двусмысленную историю: поиск по `R22-15` теперь возвращает два несвязанных
изменения.

**Исправление:** заменить новые ссылки на что-то однозначное, например:

```text
post-R22 follow-up to R22-2/task #353 (commit f2764f7)
```

Не выдавать follow-up новый round/task ID задним числом. Для незапланированных
послераундовых исправлений использовать commit SHA или отдельную монотонную
очередь `R22-F1`, `R22-F2`.

### P1 — обязательный CI всё ещё содержит два заведомо flaky wall-clock gate

`docs/CORRECTNESS_OPEN_ITEMS.md` теперь честно фиксирует:

- `backshift_no_latency_spike_at_threshold_boundary` — два падения;
- `own_thread_free_is_subquadratic` — одно падение;
- изолированные повторы проходили.

Оба теста принимают решение по `Instant::now()` под общей нагрузкой CI.
Первый уже имеет три retry и всё равно падал. Второй берёт minimum из пяти
прогонов, но сравнивает два независимо зашумлённых минимума.

Предложенный в open-items вариант «добавить file-scoped `TEST_LOCK`» не решает
заявленную причину:

- mutex сериализует только test functions внутри одного integration-test
  process;
- CPU contention создают также другие test binaries/processes и сам runner;
- именно межпроцессную host-load проблему документ называет вероятной.

Увеличение tolerance или новые retry лишь снижают вероятность flake и
одновременно ослабляют чувствительность к реальной регрессии.

**Рекомендуемое решение:**

1. убрать coarse wall-clock из обязательного correctness pass/fail;
2. correctness assertions оставить в обычных тестах;
3. сложность проверять детерминированно:
   - счётчиком hash probes/backshift moves;
   - счётчиком bitmap/free-list steps;
   - отдельным iai/Callgrind gate;
4. wall-clock оставить как ignored/manual smoke signal либо отдельный
   non-blocking perf job.

### P2 — mutex-fix устраняет flake, но тест всё ещё не доказывает заявленный «no leak»

`bc4aacf` корректно сериализует обе test functions в
`r14_4_promotion_free_correctness.rs`. Process-global atomics действительно
общие только внутри test binary; другие integration-test binaries работают в
отдельных процессах. Поэтому file-scoped mutex соответствует найденной гонке.

Однако исходная семантическая слабость теста осталась:

```text
released_delta <= reserved_delta
```

Это проверяет отсутствие **лишнего release**, то есть скорее double-release,
а не отсутствие утечки. Если grow зарезервирует сегмент и никогда его не
освободит, то `reserved_delta = 1`, `released_delta = 0`, и assertion будет
зелёным.

Второй тест ограничивает `reserved_delta <= 40` для 20 раундов — это ловит
только рост более двух reservations на раунд, но допускает линейную утечку
одного-двух сегментов каждый раунд. Его название
`does_not_leak_unboundedly` точнее, чем headline первого теста, но gate всё
равно очень мягкий.

Это не дефект нового mutex-fix; fix правильно устранил race. Но запись
`RESOLVED` в correctness index не должна создавать впечатление, что
leak-detection теперь строгий.

**Улучшение:** использовать per-heap/per-allocation observable:

- проверить, что freed promoted base больше не зарегистрирован либо находится
  в ожидаемом bounded cache state;
- измерять outstanding reservations:
  `(reserved - released)` до и после контролируемого cache drain/drop;
- либо добавить test-only per-heap counters вместо process-global totals.

### P2 — runtime discovery лучше hardcode, но тест стал чрезмерно сложным

`f2764f7` исправляет реальный test bug: capacity `9` зависела от feature
configuration. Runtime discovery — правильнее, чем ещё одна таблица
`#[cfg]`-литералов.

Сильные стороны:

- capacity определяется на текущей сборке;
- тест отдельно ищет aligned non-tail candidate;
- отсутствие подходящей геометрии даёт явный panic;
- повторная свежая `AllocCore` проверяет воспроизводимость.

Недостатки:

- для замены одного числа добавлено примерно 225 строк и большой объём
  исторической prose;
- helper утверждает, что останавливается «without carving» spill object, хотя
  фактически сначала вызывает `alloc`, обнаруживает новый base и лишь затем
  возвращается;
- tail всё ещё не обнаруживается независимо: тест предполагает, что
  `objs[1]` является tail из-за конкретного LIFO refill order;
- комментарии уже содержат ошибочную round/task маркировку, демонстрируя цену
  такого объёма дублированной истории.

**Улучшение кода теста:**

- оставить рядом с helper только invariant и короткое объяснение;
- исторический рассказ перенести в design/test report;
- вернуть из discovery структурированный результат
  `{first_segment_objects, tail_offset, spilled_ptr}` вместо соглашений
  `Vec + bool + objs[1]`;
- либо добавить test-only accessor фактического bump/tail и проверять tail
  прямо, не выводить его из refill order.

### P3 — correctness index содержит неточную запись о changed files

В resolved entry для urgent fix сказано:

```text
Files changed: tests/r14_4_promotion_free_correctness.rs only.
```

Сам commit `bc4aacf` меняет также `docs/CORRECTNESS_OPEN_ITEMS.md`. Если
подразумевается только runtime/test implementation, это стоит так и написать.
Иначе statement буквально противоречит Git diff.

## 2. Положительная оценка новых правок

### `bc4aacf`

Исправление узкое и соответствует root cause:

- один file-scoped `Mutex`;
- guard берётся в обеих test functions;
- assertion logic не ослаблена;
- poison recovery не превращает последующие тесты в каскадные отказы.

Это правильная реакция на CI failure. В предыдущем ревью риск был предсказан;
реальный GitHub Actions failure подтвердил его сразу после добавления новой
feature row.

### `f2764f7`

Главная техническая идея также правильна: свойства allocator geometry нельзя
фиксировать числом, измеренным только под одним feature set. Runtime-derived
capacity делает scenario переносимее между `production,medium-classes` и
`--all-features`.

Важно лишь не путать улучшение теста с ускорением production и привести в
порядок идентификаторы/объём документации.

## 3. Что ещё можно сильно ускорить

Новые commits не изменили карту кандидатов. Приоритеты предыдущего Round-22
ревью сохраняются.

### P0 — исправить attribution до следующей оптимизации

Текущие headline numbers всё ещё недостаточно изолированы:

- `18.6% contains_base` включает `segment_base_of_ptr`, measurement hooks и
  `black_box`;
- `1.3–2.4x vs mimalloc` получено вычитанием разных 4 MiB bootstrap proxies,
  включающих allocator-specific реальную операцию.

Следующий perf round должен сначала добавить:

1. base-mask-only arm;
2. warm `N`/`2N` delta для Sefer и mimalloc;
3. отдельные hot alloc, hot free, cold alloc и cold free;
4. function-level attribution для refill/carve/bitmap/routing.

Без этого следующая реализация рискует оптимизировать не доминирующую часть.

### P1 — hot free: routing и bitmap oracles

На каждом production free под `alloc-xthread` выполняются:

- segment-base masking;
- own-segment membership check;
- magazine-residency bitmap check;
- alloc/free bitmap check;
- bitmap/tcache update.

Наиболее реалистичный двузначный выигрыш:

- сократить routing prefix без header-before-liveness нарушения;
- исключить повторную membership/metadata работу;
- проверить объединение двух bitmap states в одно слово/cache line/RMW;
- измерить отдельный thread-affine opt-in profile без cross-thread routing,
  если продукт готов явно принять такой контракт.

Это потенциально 10–20% free-half, но не кратное ускорение всего allocator.

### P1 — cold carve/recycle

Закоммиченные сигналы по-прежнему показывают самый большой возможный gap на
cold refill/carve/recycle. Нужен split:

- directory lookup;
- refill;
- `carve_batch`;
- bitmap initialization;
- virgin/zero bookkeeping;
- magazine flush;
- BinTable freelist reuse.

Если один component остаётся примерно 2× дороже аналога после корректного
warm gate, тогда оправдан более крупный page-local metadata/run redesign.

### P1 — переоткрыть Linux sub-region `mremap`

Вывод предыдущего ревью не опровергнут новыми commits:

- medium block spans page-aligned;
- carve ranges не перекрываются;
- последующие blocks не могут занять страницы внутри уже выделенного
  `[start, end)`;
- whole-segment base stability не запрещает перемещение только sub-region.

Остаются реальные задачи: hole bookkeeping, destination registration,
teardown и Linux-only FFI. Но именно этот путь потенциально устраняет
`O(old_size)` memcpy для medium realloc и потому является самым радикальным
асимптотическим кандидатом.

### P2 — Batch API

Batch primitives уже существуют. Дальнейшее ускорение здесь требует не ещё
одного synthetic bench, а реального downstream consumer. Для обычных Box/Vec
эффект остаётся нулевым.

## 4. Что улучшить в проекте

### Немедленно

1. Исправить конфликт `R22-15/task #366` в новом test-комментарии.
2. Убрать два flaky wall-clock теста из blocking correctness gate.
3. Закрыть оставшиеся 11 clippy errors для
   `hardened medium-classes` и добавить `-D warnings` row.
4. Уточнить названия/утверждения leak tests: double-release guard не равен
   no-leak proof.

### Процесс

- После завершения round не переиспользовать его task IDs.
- В commit/report отделять:
  - runtime optimization;
  - correctness fix;
  - test-only fix;
  - measurement;
  - documentation/process.
- Не записывать число прогонов как доказательство корректности метода, если
  сам измеритель принципиально зависит от scheduler wall-clock.
- Ограничить исторические комментарии в source/tests; полную хронологию
  хранить в docs и ссылаться на неё.
- Для feature-sensitive тестов проверять свойства через accessors/invariants,
  а не через косвенную геометрию и hardcoded call order.

## 5. Рекомендуемая следующая очередь

### P0 — hygiene/correctness

1. Исправить ошибочную идентификацию follow-up.
2. Разделить correctness и timing в двух flaky tests.
3. Сделать `hardened medium-classes` clippy-clean.
4. Усилить promoted-free leak oracle.

### P0 — measurement repair

5. `contains_base` base-only decomposition.
6. Matched warm Sefer/mimalloc `N`/`2N`.
7. Hot/cold alloc/free split.

### P1 — implementation после измерений

8. Один safe hot-free prototype против реально доминирующего component.
9. Linux sub-region remap correctness prototype с memcpy fallback.
10. Продвигать изменение только при двузначном wall-clock выигрыше и
    неизменных memory-safety invariants.

## 6. Финальный вердикт

Новые два коммита **не ускорили allocator**, потому что production-код не
изменился. Они улучшили стабильность и переносимость тестов.

`bc4aacf` — хороший точечный CI fix. `f2764f7` содержит правильную идею
runtime-derived test geometry, но слишком разросся и ошибочно присвоил себе
идентификатор уже существующей perf-задачи.

Главные открытые перф-направления прежние:

- правильно измерить hot-free routing/bitmap cost;
- атрибутировать cold carve/recycle gap;
- переоткрыть Linux sub-region remap для устранения medium memcpy;
- не путать test/process waves с runtime acceleration.

