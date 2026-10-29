# Удаление повторяемой работы scalar issue — исследование и границы кандидата

Дата: 2026-10-24. Задача #11. Статус: SOURCE-CONFIRMED DUPLICATION / PERFORMANCE UNMEASURED. Реализация и новые runtime/codegen-прогоны не выполнялись. Это не продолжение D1′ и не обещание общего speedup.

## 1. Принятая база и disposition

GitHub main и локальный origin/main указывают на `f80c681b799ef4ef7fa08d79c06896fdf1104fe9`. D1′ остаётся отдельным unmerged NO-GO на PR #96. Текущая ветка b4-chunks используется только для исследовательской записки; следующий product-кандидат должен начинаться от принятой базы, не наследуя ChunkedBank.

Прочитаны end-to-end `docs/perf/OPEN_ITEMS.md`, `docs/CORRECTNESS_OPEN_ITEMS.md`, ACTIVE и все девять TRACKED тематических файлов. Disposition для каждой существующей карточки — LEAVE её status/trigger/closure неизменными: эта задача не является gate, release, исправлением correctness или новой модельной приёмкой. Особо: perf 112 остаётся NO-GO; 90/U31 и 70/contention не открываются заново; 103/104 используются как контекст расходов, не как доказательство текущей цены scalar issue. Correctness 164/246/247/248 и coverage 186/189/193/197 остаются самостоятельными обязательствами. Не закрываются stale карточки по догадке или по чужому historical receipt.

Проверка product identity ignored baseline clone выполнена против полного SHA main: `git -C .rush/perf-d1/before-repo diff f80c681b799ef4ef7fa08d79c06896fdf1104fe9 -- src Cargo.lock` пуста. Нельзя использовать `origin/main` внутри clone как ссылку на нынешнюю принятую базу: у clone свой remote и старый tracking ref.

## 2. Что отвергнуто до реализации

1. `ensure_chunk → with_chunk` с повторной ready-проверкой есть в D1′, но отсутствует в принятом main. Переносить отвергнутый storage в main ради устранения его собственного overhead нельзя.
2. Load-before-swap для пустых pending words уже присутствует в production `BitmapScan::next_cut`; OPT-01 нельзя повторно выдать за новую оптимизацию.
3. Run-level hoist route/leaf для whole-batch refill уже выполнен B4-R3. `try_drain_freelist_batch` и `try_carve_batch` идут через `IssueRun/RoutedIssueRun/LeafCursor`, а не scalar commit. Повторный hoist этой ветки не является новым кандидатом.
4. Сохранение probe-цепочки в массиве уже проиграло B4_HOT2; новый буфер не предлагается. Prepare-before-mutation не ослабляется ради удаления второго прохода.
5. Повтор Large drain внутри realloc виден как отдельный кандидат OPT-11/perf 88, но стоимость и публикации между проходами требуют своего исследования. В текущий scalar issue diff он не включается.

## 3. Подтверждённая цепочка scalar commit на main

Ссылки относятся к принятому SHA, а не к несовпадающему D1′-файлу:

- `src/alloc_core/segment/segment_table/issue_transaction.rs`, `IssueTransaction::commit`: `table.issue_small(base, offset, class)`.
- `src/alloc_core/segment/segment_table/route_slots.rs`, `RouteSlots::issue_small`: index/live-slot/root guards; затем `sidecar.prepared(offset, class)`; затем `route.issue_small(offset, class)`.
- `src/registry/segment_route/registration.rs`, `RouteRegistration::issue_small`: получает тот же SmallSidecar и вызывает `sidecar.issue`.
- `src/registry/segment_route/small_sidecar.rs`, `SmallSidecar::issue`: ещё раз `prepared(offset,class) || prepare(offset,class)`, затем `bitmap().issue`.
- `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs`, `SidecarBitmap::issue`: проверяет offset/class и вызывает `ClassLeaves::issue`.
- `src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs`, `ClassLeaves::issue`: проверяет granule/class, Acquire-загружает leaf pointer, при null возвращает false, иначе выполняет ровно один Release store `class+1` в start granule.

Таким образом на source-level пути успешного scalar commit leaf pointer читается в двух `prepared` и в завершающем `issue`. После первой успешной prepared-проверки prepare-ветка второго слоя не нужна, но общий owner API сохраняет её для самостоятельного вызова. Это доказанное дублирование исходной цепочки, не измеренное число машинных загрузок: LLVM может устранить часть повторений, и контекст caller/inlining способен оказаться важнее удалённых операций.

## 4. Минимальный предлагаемый механизм

Добавить crate-private `SmallSidecar::issue_prepared(offset,class)` без promotion/prepare: он делегирует непосредственно существующему checked `SidecarBitmap::issue`.

В `RouteSlots::issue_small` сохранить index/live-slot/root checks и прежнюю fatal-строку, но использовать этот commit-only вход через уже полученный sidecar. Проверка существования leaf не исчезает: её выполняет `ClassLeaves::issue` перед единственным Release store. Его false означает прежний commit rejection, без попытки выделить leaf после мутаций.

Публичный owner API `RouteRegistration::issue_small` с самостоятельным prepare остаётся прежним. Не добавлять mode/config/feature, поля в IssueTransaction, сохранённые raw cursors или per-block счётчики. Не расширять публичную диагностическую поверхность.

## 5. Инварианты и граница неизменности

- До mutation все fallible prepares завершены; commit-only вход никогда не выделяет backing.
- Неподготовленный leaf, неверные offset/class, отсутствующий sidecar, пустой route slot и root mismatch не превращаются в тихий success.
- Store `class+1` остаётся Release и индивидуальным для конкретного start granule, в прежней commit-точке. Никакой выдачи будущих starts заранее.
- Producer `ClassLeaves::encoded` Acquire и terminal pending RMW не меняются.
- RouteRegistration owner capability, pin/unlink/final-release lifetime, geometry/fatal guards и rollback accounting не меняются.
- Layout/state-byte geometry, refill count, magazine policy, hot free, pool, class leaves allocation/drop и versions/features не меняются.
- Уже принятый B4-R3 batch path остаётся byte-for-byte без изменений.

## 6. Где ожидать и где не обещать пользу

Scalar callers — `carve_block`, `pop_free`, `prepare_current_carve` и их конкретные входы. Основные fastbin whole-batch refills используют другой run-level путь. Поэтому нельзя обещать ускорение magazine hit, dense16, Larson/mstress или устранение D1′ MT-регрессии на основании этого изменения.

Потенциальная экономия ограничена удалёнными повторными source-level проверками на каждый реально активированный scalar commit; размер экономии в Ir/ns неизвестен. Без runtime activation oracle или пригодного codegen receipt нельзя назначить её долю в общем workload. Существующий Windows baseline binary доступен, но PE symbol table пуст: `objdump -t` не позволил связать targeted functions с кодом; это не отрицательный codegen verdict и не повод объявить дублирование машинных инструкций доказанным.

## 7. Acceptance и kill-gates до измерений

Это новый узкий эксперимент, не ослабление действующих project gates.

### Сначала correctness и активация

- Separate regression в tests/: valid scalar prepare/commit; commit on fresh unprepared leaf должен отвергаться по точной production fatal-строке; положительный prepared control; точный start и отклонение interior/unissued producer publication.
- Существующие scalar OOM/preflight/rollback suites должны продолжать обнаруживать провал prepare до mutation; standalone RouteRegistration::issue_small должен по-прежнему уметь подготовить leaf самостоятельно.
- Проверить actual-source Loom/Miri по затронутому boundary на CI; старый D1′ safety run не удостоверяет новый candidate.
- Compile-check negative feature rows, особенно core-only internals без alloc-global; не маскировать тест zero-tests там, где должен быть positive execution.

### Раннее отсечение

1. Сравнить source-identical A/A и baseline/candidate codegen/marginal для вызова, который реально активирует scalar issue. Нельзя выбирать whole-batch refill и предполагать активацию scalar path.
2. Если машинная цепочка уже эквивалентна, либо marginal scalar issue не стал дешевле воспроизводимо — NULL/NO-GO, остановить promotion. Не вводить буфер/новый storage ради спасения эксперимента.
3. Bootstrap raw totals и N/2N slope раздельно: не принять фиксированную экономию за per-operation ускорение.
4. Проверить hot marginal alloc/free, cold/recycle, dense16, registration30/60/120 и LarsonT4/T8 до полной дорогой матрицы. Действующие пороги нерегрессии остаются; нельзя компенсировать их провал выигрышем другой строки или поменять порог после B.
5. Нестатистически значимый wall delta не является PASS. Если confidence bounds не исключают недопустимый ущерб по заранее выбранному критерию, verdict INCONCLUSIVE.

Только выжившему кандидату — полная baseline/candidate/mimalloc матрица, same config/regime/entrypoint, oracle отдельно от чистого timing build, полный process wall, retention/commit/capacity и exact-head CI. Numeric acceptance budgets для нового целевого scalar microjudge должны быть зафиксированы до его первого B-run; эта записка не утверждает новый +1% wall или другой универсальный допуск.

## 8. Non-goals и полномочия

Не исправлять этим known Box-frame defect, не строить U31/summary producer protocol, не ослаблять atomics, не менять starts representation, не добавлять Large-retention policy и не проводить общий cleanup.

Запрос «продолжай» запускает исследование задач #11–#13, но не выдаёт нового разрешения на локальные build/test/bench/run/doc/Miri/Loom, подагентов, push или merge. Разрешённые локальные no-build checks остаются cargo check/clippy/fmt и cargo-free scripts. Перед существенным product diff требуется согласование конкретного подхода; исполнение моделей/benchmark по умолчанию GitHub CI.

## Источники

- `docs/perf/D1_PRIME_CHUNKED_SIDECAR_GATE.md` — NO-GO и неполнота native Linux axes.
- `docs/perf/B4_R3_BATCH_ISSUE.md` — принятый run-level hoist и нетронутые scalar callers.
- `docs/perf/B4_HOT2_OWN_THREAD.md` §2 — проигравший refill buffer и дорогой cold outlining.
- `docs/perf/R6_T40_OPT04_OPT05_GATE.md` — ~per-refill codegen cost старого OPT-05 не атрибутирован; не объявлять его причиной нового кандидата.
- `docs/perf/B4_R6_SIDECAR_INIT.md` §9 — корректирующая v3 codegen-атрибуция.
- `docs/perf/B4_STATE_BYTE.md`, `B4_MICRO_PACK.md`, `B4_JUDGES_LARSON.md`, `B4_EVENTS_COST.md` — сохранение принятых преимуществ и разделение цены пути/события.
