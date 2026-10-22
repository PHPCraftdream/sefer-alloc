# D1′: экспериментальный чанковый sidecar

Дата: 2026-10-22. Статус: PROTOTYPE / NOT ACCEPTED. Запрос владельца: попробовать самостоятельно исследованный вариант. Это не GO по безопасности или производительности. PR #94 не считается влитым; его head и старые результаты не удостоверяют этот кандидат.

## База и границы

База — `origin/main` `f80c681b799ef4ef7fa08d79c06896fdf1104fe9`. Из исследовательского дерева D1′ перенесены необходимые source/tests и wiring. Старые perf-отчёты, утверждения о выполненном merge, measurement-артефакты и независимые исправления не перенесены.

Сохраняются индивидуальные starts, owner state byte как источник класса, carve-clear, геометрия/fatal reclaim, один terminal pending RMW и существующий counted route lifetime. U31, новые producer summaries, refill-политика, batch-free, default feature composition, версии и зависимости не меняются.

Необходимая смежная зависимость: отказ от ClassLeaves убирает старый fault-injection witness. Его replacement проверяет реальный carve commit-refusal и нуждается в read-only счётчике принудительных отказов aligned-vmem. Этот узкий счётчик и его тесты включены; прочие изменения aligned-vmem из старого дерева не включены. Счётчик не доказывает отказ ОС.

## Представление

Один System backing содержит 512 чанков. Чанк покрывает 8192 Б адресов сегмента и содержит отдельные pending/starts массивы по 8 AtomicU64. На обычной production-геометрии это 128 Б на чанк и 64 Б ready-map, всего 65 600 Б с alignment 64. Отдельных heap-аллокаций на чанк нет.

Все `UnsafeCell<MaybeUninit<Chunk>>` оболочки и ready-атомики конструируются до публикации sidecar. Неготовый payload не является Chunk и не читается. Ленивость машинных stores проверяется отдельно измерением: семантика MaybeUninit сама по себе не обещает конкретный codegen.

Плотный SidecarBitmap сохранён только под `cfg(test/loom)` как старый unit-reference oracle. Production SmallSidecar всегда использует ChunkedBank; Cargo feature-развилки eager/lazy нет. Actual-source тесты нового storage не заменяются плотным oracle.

## Инварианты unsafe

1. Только owner может конструировать чанк. Производственный SmallIssueRun удерживает borrow уникальной RouteRegistration, которая !Sync; отдельный marker делает issue-run не переносимым как shared owner-capability. Pin не предоставляет безопасного конструктора.
2. Низкоуровневые issue и прямой test-forwarder — unsafe с owner-контрактом. Безопасный test-вход идёт через регистрацию !Sync. Compile-fail regression должен отвергать прямой shared test-hook вызов без unsafe.
3. Owner конструирует все атомики обоих полей до ready Release, затем ставит start конкретного блока Release в прежней commit-точке. Ready не выдаёт будущие блоки batch заранее.
4. Любой reader сначала Acquire-проверяет ready. Только после этого внутри ограниченного access scope создаётся ссылка на Chunk. Из этого scope не выдаются ссылки на payload.
5. Producers после прежних offset/payload/alignment gates проверяют ready, затем конкретный starts и выполняют один pending.fetch_or(AcqRel). Ready не заменяет allocation handoff и start gate.
6. Ready только растёт, не очищается при pool/retain. Constructed атомики никогда не реконструируются. Retain при прежней quiescence очищает только биты маскированными RMW.
7. Последний counted release уничтожает sidecar; pin/scan borrows не могут пережить backing. Drop посещает только ready-чанки.
8. Scanner проверяет исходный high_water и start_word до обхода. Неготовая область выдаёт логический пустой cut без payload-read. Каждый next_cut продвигается ровно на слово; bounded budget не превращается в скрытый полный обход чанков.
9. Геометрия и маски вычисляются production helpers, общими с Kani harnesses. Нельзя образовать typed slice через дырки.

Loom source-includes тот же storage, заменяя атомики и `UnsafeCell` на отслеживаемые типы. Instrumented cell включается лишь под `all(loom, test)`; Loom остаётся dev-dependency, новые library dependencies не добавляются. Это инструментирование доступа, не альтернативный протокол.

## Покрытие — пока не execution receipts

- `tests/chunked_sidecar_storage.rs`: actual-source dirty/unready backing, далёкая выдача без промежуточной материализации, reissue без затирания pending, индивидуальный start gate, masked clear, quiescent reset, Drop. Предназначен также для Miri SB/TB.
- `tests/loom_chunked_sidecar_actual.rs`: новый чанк с valid handoff и точным union двух cuts; safe observer нового чанка без allocation handoff; соседний бит при двух producer и masked clear.
- `scripts/chunked-ready-negative.mjs`: CI-only мутация настоящего ready Acquire в Relaxed; ожидается именно causality failure целевого Loom-теста, затем побайтное восстановление и положительный повтор. Compile error/crash не засчитываются.
- Пять Kani harnesses рядом с ChunkedBank: offset gate, word/backing/ready bounds, общая issue/publish bit-location, исходные scan bounds/empty window, точное membership range mask. Они не доказывают concurrency, владение и reclamation.
- Существующий actual-source pin-cache/unlink Loom target переносится вместе с протоколом, но его прежний receipt не является receipt нового storage.

Для принятия нужны реальные CI receipts всех слоёв, включая отрицательные контроли и существующие D1′/allocator tests. Новые файлы и успешный cargo check не являются этими receipts. Accepted Box-frame defect item 164 остаётся отдельным известным дефектом; новый storage не получает разрешения на новое UB.

## Локальная проверка прототипа

Проверены все строки `PER_PR_ROWS` из `scripts/check-matrix.mjs` с `kind` равным `clippy` или `check`, с их собственными target/features/env и `-D warnings` у Clippy. Тестовые строки намеренно не запускались. Проверены workspace rustfmt, source-included Loom typecheck, Miri/Loom wiring, hook safety, структура, sentinels и синтаксис YAML. Типизация Loom проходит с предупреждениями; это не strict Clippy и не исполнение моделей.

При проверке исправлены два cfg-дефекта: тестовый owner-helper в plain production и лишний BitmapScan re-export в standalone NUMA. В новой actual-source oneclass Loom fixture устранена унаследованная утечка backing на каждой итерации: Box уничтожается после join всех readers. Это не receipt выполнения Drop моделью.

Лично проверены новый storage/owner/scanner, порядок issue/clear/state, reclaim guards, lifecycle/wiring и перенос native/model tests. Review выявил и исправил перечисленные ниже дефекты oracle и подключения. Контрфактуальное исполнение replacement tests и runtime-приёмка требуют CI; статус остаётся NOT ACCEPTED.

Дополнительно исправлены: ordering source-pins теперь читают действующий ChunkedBank, включая ready и оба cut-пути; unlink pin-cache fixture использует payload-offset, чтобы metadata-фильтр не скрывал дефект linked gate; точный size-test учитывает ready-map (65 600 B); ранний batch-handshake замечает завершившийся owner, а scoped thread и ResumeOnDrop исключают detached worker при провале assertion. Inventory документов пересчитан: 451 root test file, 23 root Loom targets, 16 nested Rust fixtures.

Продолжение review выявило пробел replacement rollback witness: его Windows/feature-gated тело не выбиралось ни одной CI-строкой, хотя комментарий утверждал обратное. Добавлены отдельные Windows строки fastbin и scalar с `small-segment-lazy-commit`, fault injection и точными sentinels; это OS-group coverage, не безусловный per-PR gate. Сам тест теперь явно требует small-segment policy, а scalar-ветка дополнительно сверяет неизменность commit frontier. Типизация обеих комбинаций проверяется отдельно; runtime receipts всё ещё требуются. Первоначальный snapshot tree `31ecb1e75db26972b39826fd3db1fc0051f433cc` предшествует этим исправлениям и последующему запросу сравнения с mimalloc; перед измерением нужна новая source identity.

Дальнейший review mutant battery усилил oracle: abort-cases требуют своей точной fatal-строки и отвергают panic exit 101; M-f читает pending отвергнутого metadata-offset, а не offset 0; M-c′ перед retain-reset передаёт все шесть caller-held блоков целевого сегмента. Строгий Clippy этого target и структурный guard проходят. Это не исполнение тестов и не подтверждение mutation sensitivity.

## Измерительная приёмка

До измерений зафиксировать полный candidate source identity и точные toolchain/target/features. Не использовать surrogate bootstrap driver из fix15.

Обязательные оси:

- настоящий perf_gate_iai: raw totals и штатные marginal метрики отдельно; baseline main и candidate на одном runner/toolchain; действующие нерегрессионные правила не ослабляются;
- первый alloc, foreign cached/uncached free, larson/mstress, dense 16-Б fill, sparse большие small-блоки, recycle и registration churn;
- регистрационная диагностика 30/60/120 циклов: фактические registrations/unlinks и линейность, без подмены whole-process intercept стоимостью отдельного вызова;
- отдельный oracle-build: число constructed чанков и логический объём construction, конфигурация и активация пути; чистый timing-build без диагностических RMW;
- wall штатного бинаря с balanced порядком и confidence bounds, внутреннее окно и полное время процесса; не считать p>0.05 PASS;
- Linux RSS/PSS/private-dirty и Windows working set/commit: первый alloc, несколько уровней live segments, post-free и retained pins.

Меньше constructed байтов не означает автоматически меньше resident pages или wall. В dense-пределе банки всё равно целиком constructed; ready gates и поэтапная конструкция имеют цену. Порог +1% wall из прежней рекомендации @oh не объявляется автоматически утверждённым новым разрешением.

Пока производительность НЕ ИЗМЕРЕНА. Никакой speedup, нерегрессия или оптимальность не заявляется. Владелец явно разрешил commit, push отдельной ветки, PR и safety/performance workflows после review. Это разрешение на проверку кандидата, не на merge в main; исходные исследовательские деревья сохраняются. Обязательное performance-сравнение: до / после / mimalloc на одинаковых workloads и runner, с абсолютными значениями, after/before и обоими SeferAlloc/mimalloc отношениями.
