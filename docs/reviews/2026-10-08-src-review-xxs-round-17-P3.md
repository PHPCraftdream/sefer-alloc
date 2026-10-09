# Синтез ревью src/ раунда 17 — P3

[Общий индекс](2026-10-08-src-review-xxs-round-17-synthesis.md)

**Цель:** текущее дерево `ff3d60bb571f706cd8f034665fac8c3f4b6f0d23`. Находки обзоров против `453439c123f71a98890408dd0170bd909ad9ecdf` независимо сверены чтением текущих исходников.

**Ограничения:** не выполнялись команды, запуск/запрос rust-analyzer, сборка, компиляция, тесты, линтеры, форматтеры, бенчмарки, скрипты, CI или runtime-пробы. Старые логи — только историческое свидетельство. Выведенная из исходника цепочка не является исполненным witness. Записаны только запрошенные новые отчёты. Здесь **10 канонических вопросов**, а не десять заново обнаруженных дефектов.

## R17-CQ-01 — У hardened generation table есть footprint и писатели, но нет runtime-читателя

- **Исходные ID/severity:** `R17-CQ-01` **P3**; `R17-CON-02` **P4**; `R17-SEC-04` **P4**. **Проверенный приоритет: P3** по ресурсной аргументации CQ. P4 двух остальных отчётов сохранены: они оценивают ту же проблему преимущественно как контракт/поддерживаемость.
- **Диспозиция: CONFIRMED CURRENT.** Категория: opt-in ресурсная цена и устаревшие hardening-доки, не production-soundness.
- **Текущее доказательство:** `src/alloc_core/segment/segment_header/segment_header_gen_table.rs:55–67,98–110,151–158` содержит читатель, атомарный инкремент и полный цикл байтового обнуления. Поиск `gen_at` в `src/` нашёл определение/реэкспорт/документацию, но не runtime-вызов. `segment_header_impl.rs:158–161` задаёт `GEN_TABLE_FOOTPRINT = SEGMENT / MIN_BLOCK`; закреплённая геометрия 4 MiB/16 B (`remote_bitmap/sidecar_bitmap.rs:25–28`) даёт **262 144 байта**. `segment_header_layout.rs:81–94` включает их в Small/Primordial metadata под `hardened`.
- **Живые писатели:** `alloc_core/alloc_core/bootstrap.rs:203–209`, `small/alloc_core_small/reserve.rs:446–452`, `small/alloc_core_small/alloc_core_small_impl.rs:455–461`, `registry/heap_core/alloc/hot.rs:74–79,374–382`, `alloc/batch.rs:133–141,177–184`. Это magazine issue и прямой freelist-pop; не доказано, что каждый возможный прямой virgin carve бампает таблицу.
- **Контракт:** gen-модуль описывает ring consumer с поколением; текущий `small/alloc_core_small_reclaim.rs:21–68` поколения не читает. README `1426` уже сообщает об удалении ring-generation пути. Указанный X7-план (`docs/design/X7_GENERATIONAL_RING_PLAN.md:1–5,97–108`) всё ещё о ring notes и удалённом reclaim, не о живом terminal-sidecar consumer/trigger.
- **Владелец:** точная текущая карточка не найдена; correctness **154** владеет устаревшей прозой, не opt-in расходом ресурса. **Почему P3:** метаданные, явные zero writes и RMW существуют без обещанного runtime-потребителя. Реальные RSS, machine instructions и выигрыш удаления не измерены. **Oracle, не запускался:** hardened layout/issue activation и Ir/RSS на том же слое; production-ускорение не заявляется.

## R17-PRF-02 — Own-thread free повторно разрешает тот же блок

- **Исходная оценка:** `R17-PRF-02` **P3**, performance-обзор. **Проверенный приоритет: P3, сохранён.** Категория: runtime-стоимость с историческими измерениями.
- **Диспозиция: PARTIALLY VALID/NARROWED.** `src/registry/heap_core_xthread/routing.rs:10–15` разрешает `(base, block)` и передаёт их в `dealloc_own_thread_with_base`; `src/registry/heap_core/free/dealloc_own_base.rs:341–352` сразу вызывает `canonical_block_of` снова. Оба идут через `src/alloc_core/segment/segment_table/segment_table_impl.rs:743–758`, включая RMW hit/miss-счётчиков под `bench-internals`.
- **Старые числа:** `_raw_task1999_alloc_zeroed_stub_gap_after_1982.log:41–42,132–133`: prealloc/free-only **8 527 / 9 267 Ir**; `_raw_r16_perf84_A1.log:23–24,51–52`: **63 472 / 64 603 Ir**. Текущий `benches/perf_gate_iai.rs:950–969` добавляет шестнадцать frees к предаллокационному префиксу. Per-free дельта обзора — арифметика старых прогонов, не исполнение текущего HEAD; атрибуция всей churn-регрессии free и остаточных компонентов не доказана.
- **Владелец:** perf **70** — связанная цена cutover; закрытый **56** объясняет instrumentation artifacts, не именно этот повторный probe. Отдельная точная карточка не найдена. **Почему P3:** повторная работа на обычном free-пути и историческая цена требуют внимания; точная цена plain production и выигрыш удаления требуют matched iai/bisect/provenance-контролей, не запускались.

## R17-PRF-03 — Large cache reuse пересоздаёт независимый directory route

- **Исходная оценка:** `R17-PRF-03` **P3**. **Проверенный приоритет: P3, без изменения.** Категория: runtime-цена/архитектурный tradeoff, не установленный сбой корректности.
- **Диспозиция: PARTIALLY VALID/NARROWED.** Deposit делает unregister в `src/alloc_core/alloc_core/mem/mem_impl.rs:398–401`; удаление таблицы роняет route в `segment_table_impl.rs:509–516` и `route_slots.rs:203–212`. Cache hit регистрирует заново в `src/alloc_core/large/alloc_core_large.rs:451–467`. `src/registry/segment_route/directory.rs:69–85,825–834` выделяет `LargeState`/`Entry`, получает incarnation и берёт shard-lock; последний handle освобождает независимую память в `210–223`.
- **Сужение:** без удерживаемых pins этим выделениям соответствуют frees; при живом pin физическое освобождение откладывается до последнего pin. Нельзя утверждать, что два физических free выполняются внутри каждого измеряемого cache cycle. Route-cache методы `registration.rs:55–75` вызывают только route-тесты; одноимённый production reuse работает с physical header state.
- **Историческое свидетельство:** пары prefill/hit **6 575 / 6 953 Ir** (старый лог `384–392`) и **58 939 / 60 242 Ir** (Ph6b candidate `459–467`). Текущий bench `benches/perf_gate_iai.rs:2280–2339` добавляет alloc **и free**, независимо от комментария «extra alloc». Текущая цена/разложение не измерены.
- **Владелец:** точной карточки нет; perf **70** — связанная directory/cutover-стоимость. **Почему P3:** регистрация сохраняется на cache-hit, но retained-route redesign `R17-OPT-06` — неизмеренная гипотеза. Нужны registration activation и matched cycle/provenance/generation-контроли, не запускались.

## R17-PRF-04 — Одна reuse hint и fresh-first claim могут материализовать лишние heaps

- **Исходная оценка:** `R17-PRF-04` **P3**. **Проверенный приоритет: P3, без изменения.** Категория: ограниченная retention/policy-цена и отсутствующее измерение.
- **Диспозиция: PARTIALLY VALID/NARROWED.** `src/registry/heap_registry/stack.rs:108–140` поглощает одну hint, пытается `bump` до скана и сканирует после отказа bump. `claim.rs:435,539` перезаписывает hint при maintenance/owner release. `src/registry/bootstrap/registry.rs:45` ограничивает реестр **4096** heaps. Small trim исключает Primordial (`alloc_core_small_pool_impl.rs:94–105,656–660`).
- **Статическая трасса:** **[INFERENCE]** Если до новой перекрывающейся волны K потоков уже есть K FREE heaps и во время claims новые hints не поступают, только один старый heap подсказан, а до K−1 claims могут получить свежие индексы ниже capacity. Это не универсальная формула роста: scheduling, maintenance, OOM recovery и насыщение меняют reuse. Исторические totals `large_alloc_free_cycle` не изолируют цену materialization от остальной работы.
- **Владелец:** отдельный fresh-first cost gate/card не найден. Correctness **78, подпункт 18** (`TRACKED_process_record.md:333`) упоминает `fe65746` как **закрытую taxonomy-запись**, не как владельца claim latency/retention измерения. **Почему P3:** ограниченный, но потенциально существенный ресурсный tradeoff. Точные RSS/commit, claim latency и кривая волн не проверены. Oracle: контролируемые перекрывающиеся волны, high-water, RSS/commit, resolved config и first-alloc latency; не запускался.

## R17-API-01 — NUMA-only по-прежнему использует SegmentMeta при выключенном импорте

- **Исходная оценка:** `R17-API-01` **P3**. **Проверенный приоритет: P3, без изменения.** Категория: supported-feature build defect по исходнику, не заново скомпилированный отказ.
- **Диспозиция: CONFIRMED CURRENT.** `src/alloc_core/large/alloc_core_large.rs:15–16` импортирует `SegmentMeta` только под `alloc-xthread`, но `478–481` и `692–693` используют unqualified имя под `numa-aware`. `Cargo.toml:166,170,692,720`: NUMA включает alloc-core, не alloc-xthread. Другого подходящего импорта для standalone NUMA в файле нет. Slow-path mismatch действует при NUMA-only; cache-hit mismatch дополнительно требует alloc-decommit.
- **История/сейчас:** старый CI E0433 описан специалистом; здесь CI-лог не загружался и компилятор не запускался. Сам текущий cfg/name-resolution конфликт установлен чтением.
- **Владелец:** correctness **107** владеет той же непокрытой комбинацией `numa-aware internals`, но current-number всё ещё перечисляет шесть старых lint-ошибок (`TRACKED_ci_gate_coverage.md:297–303`). Изменившиеся старые import/magazine-гейты не основание закрыть комбинацию, пока остаётся этот mismatch. **Почему P3:** ограниченная валидная комбинация не разрешает нужный type; production+NUMA включает xthread и не относится к отказу. Oracle: два NUMA-only check-набора из обзора и powerset; не запускались.

## R17-SEC-01 — Magazine Small free не имеет нижней границы payload

- **Исходная оценка:** `R17-SEC-01` **P3**. **Проверенный приоритет: P3, без изменения.** Категория: defence-in-depth после unsafe-caller misuse, не новый safe-code exploit.
- **Диспозиция: CONFIRMED CURRENT.** Полный `small_free_guard` (`src/registry/heap_core/free/dealloc_own_base.rs:195–301`) проверяет kind, optional alignment, residency, bump и free bitmap, но не `off < payload_start`. `mem_impl.rs:34–42`/`routing.rs:10–15` доказывают адресное членство, не block-start geometry. `dealloc_own_base.rs:384–421` может принять metadata-offset и положить root-derived pointer в magazine; batch использует тот же guard (`free/dealloc_batch.rs:316–346`).
- **Сиблинг-контроль:** `src/alloc_core/small/alloc_core_small/dealloc.rs:73–99`, `alloc_core_small_magazine.rs:547–577`, `alloc_core_small_reclaim.rs:31–45` имеют lower-bound guard. `tests/regression_dealloc_metadata_region_guard.rs:49,78,128–132` ведёт сценарий через **AllocCore**, не magazine/SeferAlloc.
- **Сужение исторического довода:** утверждение, что perf84 снял последний детектор именно metadata-адреса, **REFUTED как поддерживающий довод**: metadata-адрес всё ещё разрешается в зарегистрированный canonical root, поэтому проверка членства корня не проверяет payload-boundary (`segment_table_impl.rs:776–807`). Это не опровергает отсутствие lower guard.
- **Владелец:** точная карточка не найдена; correctness **166** — другой foreign/interior residual. **Почему P3:** инвалидный manual free может сделать metadata-адрес следующей выдачей, но исходный free уже нарушает unsafe contract (`mem_impl.rs:198–221`). Предложенный HeapCore/SeferAlloc metadata-offset oracle, hardened/batch arms, не выполнялся.

## R17-SEC-02 — Intrusive freelist continuation и состояние головы защищены недостаточно

- **Исходная оценка:** `R17-SEC-02` **P3**. **Проверенный приоритет: P3, без изменения.** Категория: сдерживание порчи после UAF/overflow, не valid-use soundness.
- **Диспозиция: PARTIALLY VALID/NARROWED.** `src/alloc_core/platform/node.rs:75–118` пишет/читает continuation без аутентификации. `alloc_core_small_impl.rs:374–464,526–637` проверяет сегмент continuation только под hardened, вычисляет offsets из адресов и не проверяет `is_free(head)` до перечисленных pop/drain-мутаций. `segment_bitmap.rs:113–117` ограничивает bitmap index лишь debug_assert. Standalone `prepare_small_issue` возвращает unrouted witness (`segment_table_impl.rs:316–332`).
- **Существенная граница:** routed `SmallSidecar::prepare` проверяет segment range/granule alignment (`small_sidecar.rs:50–56`), не нижнюю границу payload. Это не доказательство end-to-end containment: pop/drain формируют block pointer раньше части issue validation. Конкретная exploitability/returned-pointer цепочка требует corruption-witness; здесь она не исполнялась. Provenance после произвольной порчи байтов тоже не доказывается нативными адресными рассуждениями.
- **Владелец:** точной карточки нет; **164** — другой accepted intrusive-Box aliasing-вопрос. **Почему P3:** асимметрия hardening подтверждена, но атака требует предыдущего инвалидного доступа. `regression_freelist_next_validation` ограничен hardened; нужны non-hardened corruption/head-state oracle, не запускались. Safe-linking benefit `SEC H2` не измерен.

## R17-LOG-01 — Bounded catch-up переносит целые интервалы decay-долга через idle/reuse

- **Исходная оценка:** `R17-LOG-01` **P3**. **Проверенный приоритет: P3, без изменения.** Категория: policy/contract mismatch с потенциальной extra release/refill ценой, не утечка/порча.
- **Диспозиция: CONFIRMED CURRENT.** `src/alloc_core/large/alloc_core_large_cache.rs:523–525` выходит ниже headroom, не обновляя timer. `539–583` ограничивает clock reads; `605–615` ограничивает due steps числом **8**, но двигает старый timer только на `interval * due`. Поиск присваиваний нашёл constructor initialization, эту функцию и test hooks. `alloc_core_large_cache_eviction.rs:69–72` очищает cache без очистки timer; re-claim вызывает trim (`claim.rs:317`).
- **Статический вывод:** **[INFERENCE]** При elapsed > 8 интервалов timer после пакета остаётся позади now на целые интервалы; следующие eligible clock reads могут выдавать новые пакеты без полного нового интервала после предыдущего пакета. Это противоречит minimum-interval wording (`large_cache_config.rs:307–308`) и «never more aggressively» (`config/profile.rs:189–191`). Реальный объём eviction зависит от excess/FIFO (`AllocCore::run_decay_step`, `src/alloc_core/large/alloc_core_large_cache.rs:626–639`; `alloc_core_large_cache_eviction.rs:34–55`). Hit-rate/RSS/latency loss не измерены.
- **Владелец:** perf **42** — связанный sparse-decay/stride механизм, не готовое закрытие debt-after-idle. **Почему P3:** обычная policy может освобождать агрессивнее обещанного; работа одного read ограничена, долг сохраняется. Second-stride/trim-reuse oracle обзора не запускались.

## R17-LOG-02 — Неудачная регистрация cache-hit всё равно увеличивает large_cache_hits

- **Исходная оценка:** `R17-LOG-02` **P3**. **Проверенный приоритет: P3, без изменения.** Категория: точность диагностического oracle.
- **Диспозиция: CONFIRMED CURRENT.** `src/alloc_core/large/alloc_core_large.rs:287–295` увеличивает hits до `register_payload` (`451–467`). При отказе cached reservation освобождается и вызывается alloc_large_slow без rollback счётчика. Публичный контракт — calls «served directly» by cache (`src/global/alloc_stats.rs:57–69`). Table/route prepare fallible (`segment_table_impl.rs:396–408`; `route_slots.rs:74–92`); injection возвращает OOM в `directory.rs:797–806`.
- **Владелец:** точной карточки нет. **Почему P3:** неверный observable counter при реальном registration/OOM отказе; allocation fallback не объявлен сломанным. Write требует alloc-stats, не обычного production bookkeeping. Нужен injected-next-registration сценарий с hit/reserve/release delta, не запускался.

## R17-API-06(c) — Fallback wait counter способен переполниться в debug

- **Исходный ID/severity:** подраздел **(c)** `R17-API-06`, исходно **P4**. Contract-подразделы (a,b) объединены отдельно с `R17-LIF-02` в P4. **Предлагаемый проверенный приоритет: P3 для (c)**: это условный arithmetic panic-путь, не только проза.
- **Диспозиция: CONFIRMED CURRENT для дефекта счётчика.** `src/global/fallback.rs:469–486` задаёт u32, безусловно увеличивает его при каждой неудачной CAS (`475`) и только затем проверяет backoff threshold; saturation/wrapping не заданы. Init-wait (`310–325`) и `ShardLock::lock` (`shard_lock.rs:48–61`) увеличивают counter только ниже tight-spin threshold.
- **Достижимость/пределы:** global fallback allocation диспетчеризуется в `src/global/sefer_alloc/global_alloc.rs:40–47`; fallback берёт guard в `fallback.rs:351,370`. **[INFERENCE]** После достаточного числа failed acquisitions overflow-checking debug может паниковать внутри allocator. Нужен очень долгий wait; его вероятность/время, причина старых hanging-тестов и нынешний CI не воспроизведены. Старый CI overflow специалиста — историческая запись, не receipt этой проверки.
- **Владелец:** точной карточки нет; perf **66** владеет calibration threshold, не arithmetic correctness. **Почему P3:** ограниченный feature/profile no-unwind отказ при extreme wait. Oracle: near-maximum counter/backoff в debug с отличием настоящей fallback-достижимости от скопированной arithmetic fixture; не запускался.

Неоценённые по P оптимизации и другие гипотезы сохранены в приложениях A–C отчёта [P5](2026-10-08-src-review-xxs-round-17-P5.md); им не назначен P-уровень.

## Результат исправлений R18

- **R17-CQ-01 (P3), объединённые R17-CON-02/R17-SEC-04 (P4): ЗАКРЫТО.**
  Удалены hardened-only generation table, `GEN_TABLE_FOOTPRINT`, её layout,
  инициализаторы, writers, exports и только-табличные тесты. Проверка исходников
  не нашла runtime reader. `README.md`, `docs/ARCHITECTURE.md` и текущая
  `docs/DURABILITY.md` отражают новую геометрию. 256 KiB — закрытая формула
  размера, не измеренный RSS/Ir результат.
- **R17-SEC-02 (P3): ЗАКРЫТО в scope hardened-защиты.** `pop_free` и
  batch preflight до мутации проверяют геометрию головы/continuation,
  free bitmap, magazine residency и цикл. `regression_freelist_next_validation`
  прошёл 6/6 в debug и release; batch-drain — 3/3 hardened и 4/4 production;
  `no_stale_doc_references` — 31/31. Удаление free-state validation сделало
  красными обе проверки invalid allocated continuation.
  Удаление out-of-segment проверки сделало scalar-тест красным; batch-мутант
  остановился до assertion с checked-subtraction overflow при старом
  преобразовании `next` в offset, не разыменовав forged address.
  Удаление self-link проверки сделало scalar self-cycle тест красным; batch
  по-прежнему отверг цикл независимым cycle preflight. Все мутанты
  восстановлены; позитивные regression-тесты прошли. Первый прогон обнаружил
  только ошибку oracle-теста: refill оставил существующий хвост freelist,
  который теперь сохраняется и проверяется вместо предположения об empty.
- **R17-SEC-01 (P3): ЗАКРЫТО как defense-in-depth после unsafe-contract misuse.**
  `small_free_guard` теперь отбрасывает смещения до kind-specific payload start;
  тесты Primordial и Small прошли по 2/2 в debug/release для `fastbin internals`
  и `production internals`. Удаление нижней границы и возврат `alloc-decommit`
  cfg на bump-check позволили invalid free войти в magazine (`left: 1`,
  ожидалось `right: 0`); замена `<` на `<=` отвергла valid boundary free
  (`left: 0`, ожидалось `right: 1`). `--nocapture` показал соответствующие
  assertions перед Windows exit `0xc0000409` при unwind; это exit status, не
  диагноз stack-buffer defect. Все мутанты восстановлены; positive tests прошли.

- **R17-LOG-02 (P3): ЗАКРЫТО.** `large_cache_hits` увеличивается после
  успешных `register_payload` и `finish_large_reuse`; при отказе регистрации
  cache reservation освобождается и запускается slow path без ложного hit.
  `cache_registration_refusal_is_not_a_served_hit` прошёл в debug/release;
  перенос счётчика перед регистрацией сделал тест красным (`left: 1`,
  ожидалось `right: 0`). Runtime/performance claim не заявляется; счётчик
  остаётся `alloc-stats`-gated.

- **R17-API-01 (P3): ЗАКРЫТО.** Импорт `SegmentMeta` теперь доступен при
  `alloc-xthread` **или** `numa-aware`. В main прошли оба NUMA-only `cargo check`
  и строгий `cargo clippy`; в изолированном worktree сужение импорта обратно
  до `alloc-xthread` воспроизвело E0433 на `SegmentMeta::new` в NUMA slow path.
  После восстановления оба compile checks прошли. Состав features и production
  defaults не менялись. Точный `cargo clippy --features "numa-aware internals"
  -- -D warnings` также прошёл; correctness item107 остаётся открытым только
  для отдельного Clippy-row/CI coverage, а его прежние шесть ошибок устарели.

- **R17-API-06(c) (P3): ЗАКРЫТО.** Init-state waiting and fallback-lock
  acquisition now share `lock_tight_spin`, which stops incrementing at 64 and
  yields thereafter; no hot-path counter or configurable policy was added.
  Both regressions passed in debug/release. Reverting to unconditional `u32`
  increment made the `u32::MAX` regression fail with `attempt to add with
  overflow`; restoring the helper returned both tests to green.

- **Граница:** расширенная freelist-проверка остаётся `hardened`-gated;
  lower-bound и bump guards работают на fastbin production-path и меняют
  обработку только invalid deallocation inputs. Speed/RSS claim не заявлен.
