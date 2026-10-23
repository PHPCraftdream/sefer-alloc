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

## Safety receipts — CI GREEN; performance приёмка отдельно

CI run [`39869595774`](https://github.com/PHPCraftdream/sefer-alloc/actions/runs/39869595774), head `d031f03cdc14efdcc501840868491de7d8a9891e`, завершился `success`. Сводный receipt и job links: `docs/evidence/R96_D1_CHUNKED_SAFETY_CI_RECEIPT.md`.

- `tests/chunked_sidecar_storage.rs`: actual-source dirty/unready backing, далёкая выдача без промежуточной материализации, reissue без затирания pending, индивидуальный start gate, masked clear, quiescent reset, Drop — 3/3 под Miri SB и 3/3 под Miri TB.
- `tests/loom_chunked_sidecar_actual.rs`: новый чанк с valid handoff и точным union двух cuts; safe observer нового чанка без allocation handoff; соседний бит при двух producer и masked clear — 3/3 release и 3/3 restored debug.
- `scripts/chunked-ready-negative.mjs`: CI изменил production ready Acquire на Relaxed; Loom поймал требуемое causality violation; restoration проверена byte-for-byte, после чего positive debug model прошёл.
- Пять Kani harnesses рядом с ChunkedBank: offset gate, word/backing/ready bounds, общая issue/publish bit-location, scan bounds/empty window, exact range-mask membership. Полный Kani CI отчёт: 21 harnesses, 0 failures, 1 unreachable; Kani не доказывает concurrency или reclamation.
- Windows fastbin и scalar commit-refusal rollback witnesses прошли с точными sentinels; owner compile-fail, native matrix, Loom, Miri, Kani и все required CI jobs завершились зелёными.
- Historical proof-registry rows, чьи source/test bytes изменены, оставлены `CI_PENDING`/`NOT_RUN`, а не переписаны задним числом как PASS; см. registry и correctness item 254.

Это закрывает execution-gate для D1′ safety harnesses на указанном SHA, но **не принимает performance-кандидат**. Accepted Box-frame defect item 164 остаётся отдельным известным дефектом.

## Локальная проверка прототипа

Проверены все строки `PER_PR_ROWS` из `scripts/check-matrix.mjs` с `kind` равным `clippy` или `check`, с их собственными target/features/env и `-D warnings` у Clippy. Тестовые строки намеренно не запускались. Проверены workspace rustfmt, source-included Loom typecheck, Miri/Loom wiring, hook safety, структура, sentinels и синтаксис YAML. Типизация Loom проходит с предупреждениями; это не strict Clippy и не исполнение моделей.

При проверке исправлены два cfg-дефекта: тестовый owner-helper в plain production и лишний BitmapScan re-export в standalone NUMA. CI run 39859543504 выявил несовпадение physical path unsafe seam и `src/lib.rs` inventory; `chunked_bank.rs`/Kani proof перемещены в `remote_bitmap/sidecar_bitmap/`, выровняв source-file inventory. В actual-source oneclass Loom fixture убрана унаследованная утечка backing: Box уничтожается после join readers. Этот локальный typecheck сам по себе не был receipt; Drop/lifetime paths прошли в run `39869595774`.

Лично проверены новый storage/owner/scanner, порядок issue/clear/state, reclaim guards, lifecycle/wiring и перенос native/model tests. Review исправил дефекты oracle и подключения. Run `39869595774` на SHA `d031f03cdc14efdcc501840868491de7d8a9891e` завершился GREEN по требуемым матрицам; receipts перечислены выше. Safety execution gate закрыт на этом коде. Статус кандидата всё ещё NOT ACCEPTED до сравнительной performance-приёмки.

Дополнительно исправлены: ordering source-pins теперь читают действующий ChunkedBank, включая ready и оба cut-пути; unlink pin-cache fixture использует payload-offset, чтобы metadata-фильтр не скрывал дефект linked gate; точный size-test учитывает ready-map (65 600 B); ранний batch-handshake замечает завершившийся owner, а scoped thread и ResumeOnDrop исключают detached worker при провале assertion. Inventory документов пересчитан: 451 root test file, 23 root Loom targets, 16 nested Rust fixtures.

Продолжение review выявило недостижимый replacement rollback witness: его Windows/feature-gated тело не выбиралось CI. Добавлены отдельные Windows строки fastbin и scalar с `small-segment-lazy-commit`, fault injection и точными sentinels; scalar дополнен frontier oracle. Обе строки прошли в run `39869595774`. Первоначальный snapshot tree `31ecb1e75db26972b39826fd3db1fc0051f433cc` исторический; для benchmark будет использован новый committed candidate identity.

Review mutant battery усилил oracle: abort-cases требуют точную production fatal-строку и отвергают panic exit 101; M-f читает pending отвергнутого metadata-offset, а не offset 0; M-c′ перед retain-reset передаёт все шесть caller-held блоков целевого сегмента. Строгий Clippy и structural guard прошли; native positive abort-oracle tests прошли в CI (контрфактуальные mutations этих отдельных oracle не запускались). Отдельно experimental ready-Relaxed mutant реального `ChunkedBank` был запущен и отвергнут ожидаемым Loom causality violation.

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

Исторический статус до измерений: производительность НЕ ИЗМЕРЕНА. **Обновление 2026-10-23:** текущий кандидат получил performance NO-GO / NOT ACCEPTED; отчёт `docs/perf/D1_PRIME_CHUNKED_SIDECAR_GATE.md` разделяет canonical wall, IAI raw/marginal, installed-global и component probes. WSL retained-pin PSS не закрывает native Linux и first/multisegment PSS-оси. Никакой общий speedup, нерегрессия или оптимальность не заявляется. Владелец явно разрешил commit, push отдельной ветки, PR и safety/performance workflows после review. Это разрешение на проверку кандидата, не на merge в main; исходные исследовательские деревья сохраняются. Обязательное performance-сравнение: до / после / mimalloc на одинаковых workloads и runner, с абсолютными значениями, after/before и обоими SeferAlloc/mimalloc отношениями.
