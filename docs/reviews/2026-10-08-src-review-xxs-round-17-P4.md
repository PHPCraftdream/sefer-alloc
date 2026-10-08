# Синтез ревью src/ раунда 17 — P4

[Общий индекс](2026-10-08-src-review-xxs-round-17-synthesis.md)

**Текущая цель:** `ff3d60bb571f706cd8f034665fac8c3f4b6f0d23`; база тематических обзоров: `453439c123f71a98890408dd0170bd909ad9ecdf`. Пути и номера строк ниже независимо прочитаны в текущем дереве. Исходные ID и severity сохранены при объединении.

**Только статическая проверка:** не выполнялись никакие команды, запуск/запрос rust-analyzer, сборка, тесты, линтеры, форматтеры, бенчмарки, скрипты, CI или runtime-пробы. Прогноз контрфактуала — не receipt. Старые CI-описания специалистов не загружались заново. Записывались только запрошенные новые отчёты; source, tests, CI, indexes, changelog и исходные обзоры не менялись.

Здесь **34 канонических вопроса**: **16 CONFIRMED CURRENT**, **15 PARTIALLY VALID/NARROWED**, **3 NOT VERIFIABLE FROM STATIC SOURCE**. P4 означает контракт/покрытие/поддерживаемость или неподтверждённое исследование, не новый установленный production memory-safety exploit. Кроме явно условного Windows-вопроса, исходные оценки всех записей — P4, сохранены. Предложения без исходной P-оценки подробно сохранены в приложениях A–C отчёта [P5](2026-10-08-src-review-xxs-round-17-P5.md); им P не назначен.

## R17-CON-01 — Windows exit и оставленные allocator locks: неподтверждённая платформенная гипотеза

- **ID/источник severity:** `R17-CON-01`, `R17-LIF-01`, оба **P2 при подтверждении**. **Предлагаемый текущий приоритет: P4 исследования**, не подтверждённый P2.
- **Диспозиция: NOT VERIFIABLE FROM STATIC SOURCE.** Внутренняя waiting/teardown-цепочка остаётся; необходимый гипотезе порядок завершения std/ОС не реализован в прочитанных исходниках репозитория.
- **Текущее доказательство:** `src/global/tls_heap.rs:197–224` ставит TORN, выполняет trim и роняет lease. `src/registry/heap_core/state/ownership.rs:108–156` дренирует и cold-trim; `src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:636–676` снимает маршруты при recycle. `src/registry/segment_route/directory.rs:868–897` берёт lock при lookup/remove; `shard_lock.rs:48–66` ждёт без deadline/owner-death recovery. Late free: `tls_heap.rs:321–335` → `heap_core_xthread/routing.rs:26–44`; late alloc использует fallback с ожиданиями `src/global/fallback.rs:310–325,469–486`.
- **Сужение:** CON оставляет std/ExitProcess ordering внешним допущением; LIF пересказывает документацию. Ни один не устанавливает исполненный текущий shutdown schedule. Вечное ожидание выводится *если* держатель больше не способен исполняться; обычный scheduling и process shutdown нельзя смешивать. Debug overflow fallback-counter — отдельный P3.
- **Владелец/приоритет:** correctness **152** — POSIX fork, не Windows exit; **162** — связанная terminal/service acceptance, не точный Windows owner. Точной карточки нет. Условное P2-последствие сохранено, но достижимость требует проверки.
- **Oracle, не запускался:** Windows child process с намеренно удерживаемым известным shard/fallback lock, parent timeout и проверка реального std TLS shutdown. Shutdown detector или новый hook не изобретались и не реализовывались.

## R17-CON-03 — Карточка 160 устарела в части «модели нет», но actual-type coverage не доказано

- **Исходная оценка:** `R17-CON-03` **P4**; также VER §2.2 к correctness **160**. **Диспозиция: CONFIRMED CURRENT** для противоречия индекса/документации.
- **Доказательство:** `docs/correctness-open-items/TRACKED_misc.md:85–90` всё ещё утверждает отсутствие модели. `tests/loom_r11_epoch_false_full.rs:7–63,84–123` содержит **ручной `QueueProtocol`** с Relaxed hint и Loom Mutex/Vec; `.github/workflows/ci.yml:3175–3183` выбирает его и проверяет positive/negative-control sentinels. Соответствующий код — `src/concurrent/epoch/epoch_region.rs:338–395,431–438,583–595`.
- **Граница:** это abstract/shadow model, **не реальный `EpochRegion` или его std Mutex/Vec под Loom**. Модель не включает, например, retired-generation фильтрацию (`epoch_region.rs:383–392`). Сходство исходника и проводка не доказывают refinement, memory safety, liveness, успешный прогон или latency bound. Буквальное «нет никакой модели» неверно; implementation-to-model gap этим не закрыт.
- **Владелец/почему P4:** точная карточка **160**, актуальность durable record, не вновь найденная потеря индекса. Карточка не закрыта/не изменена. Для более сильного закрытия нужны model execution, negative control и refinement/actual-type evidence; не запускались.

## R17-CON-04 — Route lifetime model и целевое worker/fallback sanitizer-покрытие неполны

- **Исходная оценка:** `R17-CON-04` **P4**, (a,b,c); пересекается с VER §2.2 к **17**. **Диспозиция: PARTIALLY VALID/NARROWED.** Категория: coverage/CI contract, не доказанная data race.
- **(a):** `tests/loom_r11_small_sidecar.rs:16–62` начинает с уже полученных registration+producer pin (`refs = 2`), не моделирует приобретение pin под shard-lock против unlink/last pin. Реальные increment/drop — `directory.rs:142–157,868–897,210–223`. `tests/r6_route_directory.rs:57–105` упорядочивает lifecycle шаги каналами. Pin-lifetime модель существует, но не эта acquisition/unlink гонка.
- **(b):** `.github/workflows/ci.yml:3242–3250,3280–3293` выбирает race/global/TLS/counter/realloc/boundary targets, не `r8_autonomous_maintenance`, `r9_bounded_background_maintenance`, `r3_1_fallback_remote_free`, `r11_ph5c_registry_saturation_fallback`, `r8_terminal_global`. В выбранных файлах не найден явный maintenance startup. Целевой worker/fallback visit — `maintenance_service.rs:144–150`.
- **Сужение:** отсутствие dedicated targets не доказывает, что fallback **никогда** не затрагивается TSan. Выбранный TLS stress обсуждает fallback (`tests/tls_heap_teardown_ordering_stress.rs:27–28,56–58`) и создаёт teardown waves (`145–166`); побочное std/TLS поведение нельзя исключить рассуждением по списку targets. История прогонов/path activation не проверялась.
- **(c):** CI `3101–3108,3126–3128` называет `loom_fallback_init` заменённым real-type OncePtrCell coverage. Fallback имеет собственные INIT_STATE, lock, InitStateGuard (`fallback.rs:310–325,389–402,541–570`); real-type OncePtrCell model не моделирует эту комбинацию.
- **Владелец/почему P4:** correctness **17** — accepted fallback proof gap; **162** — related worker acceptance. Точного отдельного owner acquisition-model/targeted TSan нет. Протокол не показан сломанным. Model negative control и TSan workload/path oracle не запускались.

## R17-CON-05 — Exact-object prototype берёт shard spin-lock и для не-exact frees

- **Исходная оценка:** `R17-CON-05` **P4**. **Диспозиция: CONFIRMED CURRENT.** Категория: opt-in prototype cost/maintenance.
- **Доказательство:** `src/global/sefer_alloc/global_alloc.rs:62–67` пробует exact dealloc до общей маршрутизации любого non-null free. `src/global/exact_object/narrow.rs:63–84` вызывает `ExactTable::take`; `exact_table.rs:6–17,37–39` выбирает один из 64 shards. `exact_shard.rs:240–265` не берёт lock только при нулевом live_hint. Ожидание — pure spin_loop (`184–192`); rehash alloc/free System storage (`45–76`) идёт в insert critical section.
- **Владелец/почему P4:** точной карточки нет. Не-дефолтный prototype добавляет serialization; доля занятых shards, wasted CPU и tail latency — неизмеренные workload-гипотезы. Layout filter CON H5/backoff не признаны выигрышем. Mixed-workload acquisition/latency oracle не запускался.

## R17-CON-06 — Проза в настоящем времени продолжает описывать удалённые протоколы

- **Объединены:** `R17-CON-06` **P4**, `R17-CQ-04` **P4**. **Диспозиция: PARTIALLY VALID/NARROWED.** Категория: docs/contract/prose maintenance.
- **Доказательство:** `src/alloc_core/platform/sidecar.rs:64–93`, `platform/mod.rs:16–17` описывают cross-thread PerClassDirty; определения не найдены. `src/registry/bootstrap/registry.rs:144–168` называет удалённых foreign-free callers. `segment_header_impl.rs:299–350` приписывает foreign path чтение owner_state/kind, противореча собственному magic-doc и descriptor-routing (`heap_core_xthread/routing.rs:26–65`). `descriptors.rs:316–328` содержит исправленную прозу и затем устаревший caller clause.
- **Ещё текущие примеры:** `src/alloc_core/small/mod.rs:31–32` — ring/generation reclaim, в действительности `alloc_core_small_reclaim.rs:21–68`. `alloc_core_small_pool/decommit.rs:110–128` рассуждает о stale ring notes. `large_cache_extended.rs:39–51,84–87` называет storage leaked, а `134–170` описывает owning AccountedSidecar. `header_diag.rs:331–332` ссылается на dbg_force_decommit_retain без реального суффикса `_for`. `directory_diag.rs:282–288` называет удалённые dirty types/constants/example. `ownership.rs:29–30` называет Release store MFENCE-equivalent; исходник не заменяет assembly measurement.
- **Дополнительные примеры:** `terminal_words.rs:36` называет живые Pending/Consuming будущей стадией; `src/lib.rs:246` говорит Sync/Send у HeapSlot, хотя `heap_slot.rs:283–300` намеренно только Sync. LIF-примеры остаются в `tls_heap.rs:133–139,481–487`; NUMA initializer no-heap documented в `crates/numa-shim/src/lib.rs:1516–1536`, allocation-free open — `1682–1701`.
- **Сужение:** CQ totals **94/45/46**, missing-file/ring-line census — старые heuristic результаты, не новый прогон. Исторические цитаты удалённых имён законны: `mem_impl.rs:406–418` прямо говорит, что magic_at удалён. Не каждое lexical mention — текущий defect. Phase-0 terminal contract явно design, не implementation (`docs/REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md:3`); история не переоценивается как runtime failure. Специализированные no-panic/counter/module/inventory/M2 вопросы ниже не считаются дублями заново.
- **Владелец/почему P4:** correctness **154**, misleading current contracts. Comprehensive guard/census/codegen/performance evidence не исполнялись.

## R17-PRF-05 — Обещанная one-line locality PerClass не описывает установившийся churn

- **Исходная оценка:** `R17-PRF-05` **P4**. **Диспозиция: CONFIRMED CURRENT.** Категория: layout-performance docs.
- **Доказательство:** `src/registry/heap_core/state/tcache.rs:188–229` обещает same-line и называет depth 1–3 common churn; `232–288,307–323` закрепляет count offset 0, slots offset 8, capacity 16 (`63`). `hot.rs:122–124,291–299,340` оставляет n−1 и читает stack top; free push ограничен refill-derived park cap (`dealloc_own_base.rs:408–421`; `tcache.rs:127–139,176–185`). После refill alternating small alloc/free использует depth 15↔16 и slots[15] offset 128, не 64-byte линию count.
- **Владелец/почему P4:** correctness **154** — проза; perf **18/44** — related tuning history, не доказательство locality. Статическое противоречие, cache misses/latency не измерены. OPT-09 не объявлен оптимизацией с доказанным эффектом.

## R17-PRF-06 — Known-base realloc повторяет probe вопреки doc

- **Исходная оценка:** `R17-PRF-06` **P4**. **Диспозиция: CONFIRMED CURRENT** для doc/body mismatch. Категория: контракт и unmeasured cost.
- **Доказательство:** `src/alloc_core/alloc_core/mem/realloc_fastpath.rs:262–280,544–559` обещает отсутствие duplicate probe, но вызывает canonical_base_of(key); callers уже разрешают блок (`mem_impl.rs:644–655`; `src/registry/heap_core/free/realloc.rs:143–148,201–203`).
- **Владелец/почему P4:** correctness **154**; точного optimization owner нет. Probe также даёт canonical-root validation/reconstruction, поэтому имя само по себе не доказывает возможность удаления. Ir-estimate/выигрыш нуждаются в provenance/perf-контролях; не запускались.

## R17-VER-01 — Ordering pin закрепляет Dense test backing, не production ClassLeaves publication

- **Исходная оценка:** `R17-VER-01` **P4**. **Диспозиция: CONFIRMED CURRENT.** Категория: связь verification/source; нынешние orderings не объявлены неверными.
- **Доказательство:** `tests/r11_ph5b_c5_sidecar_ordering_pinned.rs:20–29,53–68` читает только sidecar_bitmap/bitmap_scan и ищет Dense-store (`sidecar_bitmap.rs:91–95`). Production `SmallSidecar::bitmap` использует from_leaves (`small_sidecar.rs:39–41`); несущая mixed-pointer publication — `leaf_classes.rs:108–145`, Release `113`, Acquire `139`, не затронутые pin.
- **Модели:** `tests/loom_sidecar_bitmap.rs:39–98` пишет/читает classes в main thread, producers только publish bits. `loom_r11_small_sidecar.rs:69–112` содержит нужный mixed-pointer shadow с negative control, но без source-ordering pin. Поиск pin-тестов не нашёл leaf_classes binding.
- **Владелец/почему P4:** точного owner нет; **162** — related cutover acceptance. Production mutant способен оставить copied models/text pin неизменными по конструкции. Исполнение mutant/model/runtime-контролей не проводилось.

## R17-VER-02 — Evidence freshness проверяет цитируемые tests, не защищаемый src-scope

- **Исходная оценка:** `R17-VER-02` **P4**. **Диспозиция: PARTIALLY VALID/NARROWED.** Категория: границы evidence tool.
- **Доказательство:** `scripts/verify-evidence-registry.mjs:353–375` извлекает test references; `775–790` сравнивает test blobs с source SHA. Header `docs/evidence/registry.csv:1` не имеет explicit protected-source-scope. R6a-M5 всё ещё называет batch.rs:188 filled += n (`registry.csv:154`), statement сейчас `src/registry/heap_core/alloc/batch.rs:189`. Ограничение честно записано в `docs/reviews/2026-10-05-ph7-evidence-registry-receipt.md:163–168`.
- **Сужение:** «CI green по построению» чрезмерно: receipt/identity/status/tests checks могут отклонить строку. Слепая зона — source-only behavioral change при сохранении проверяемого evidence. Старые 17-коммитный/127-PASS census и CI verdict не пересчитаны. PASS — историческое source-pinned утверждение, не фикция лишь из-за отсутствия source invalidation.
- **Владелец/почему P4:** точной карточки нет; риск переоценить freshness. Нужный source-only mutant/checker oracle не запускался.

## R17-VER-03 — Miri inventory/docs расходятся с automation

- **Исходная оценка:** `R17-VER-03` **P4**; также UNS §4. **Диспозиция: PARTIALLY VALID/NARROWED.** Категория: coverage/runner/record.
- **Доказательство:** две lease-цели существуют, cfg(miri)-gated (`tests/r11_ph4a_lease_miri.rs:15`; `r11_ph4b_lease_miri.rs:21`); regression_r2_06_header_race_miri тоже существует. В `.github/`, `scripts/`, `package.json` не найдены эти три имени. Шесть исторических PASS относятся к **двум lease-файлам** (`docs/evidence/registry.csv:34–35,47–48,61–62`); header-race row `63` — KNOWN-RED, не один из шести PASS.
- **Runner:** `scripts/miri.mjs:2–6,45–127` обещает CI mirror, но четыре targets (`56,74,108,115`) только локальные. Local region_invariants включает experimental, CI `2838–2839` — нет. Local paused требует success/COMPLETE (`222–239`), CI имеет четыре expected-red signature/site wrappers (`2856–2964`). README `1388`, ARCHITECTURE `492` называет r8_global_box_provenance, не выбранный harness-free miri_global_box_acceptance.
- **Смена механизмов:** `tests/regression_r2_06_header_race_miri.rs:6–35,123–130` описывает удалённые deferred-header writes; нынешний foreign code пишет independent descriptor (`routing.rs:49–61`). Small-ring/thread-free headers описывают снятый мир, сами CI targets сохранены (`ci.yml:3063–3070`). `decommit_miri_cycle.rs:1–11` обещает recommit, production pool/release отличается (`alloc_core_small_pool_impl.rs:167–215,428–429`); retain decommit только из hook (`decommit.rs:93–108`).
- **Сужение:** отсутствие automation **не означает**, что targets никогда не запускались: реестр описывает ручные исторические прогоны. Local runner ожидаемо FAIL **пока accepted item164 красный**, не логически «всегда при любом будущем tree/toolchain». Interpreter outcomes здесь не воспроизведены.
- **Владелец/почему P4:** **17** — proof/wiring class, **164** — accepted Box failure, **171** — отдельный experimental Miri. Мисматч описаний и классификации результатов, не новый Miri failure. Wiring/expected-red oracle не запускался.

## R17-VER-04 — Kani scope мал; registry packing/ring summaries устарели

- **Исходная оценка:** `R17-VER-04` **P4**; VER §2.2 к **18/167**, CQ §2.14 к **167**. **Диспозиция: PARTIALLY VALID/NARROWED.** Категория: proof coverage/record.
- **Доказательство:** `src/kani_proofs.rs:25–146` — 9 local-buffer Node harnesses; `154–169` — 2 vacant AtomicSlot; `178–223` — 2 TaggedIndex packing. Видны **13 proof declarations**, без ring harnesses/cover calls. Node caller-contract limit прямо в `6–10`, но `171–181` называет packing regression реестра free_slots. `heap_registry/stack.rs:1,108–140` ищет numeric slots без intrusive free list. `bootstrap/loom_shim.rs:251–253,315–317,373–378` ссылается на отсутствующий production implementor; `tests/loom_registry_free_slots.rs:23` импортирует настоящий member stack, не mirror.
- **Docs/CI:** **18** всё ещё описывает 6 ring harnesses/19 total (`TRACKED_verification_coverage.md:158–178`). CI `3076–3091` называет packing registry coverage. `kani.yml:3–6` обещает отделение от main CI, но ci.yml имеет Kani-job, standalone workflow `37–40` гоняет ещё subsets.
- **Сужение:** blanket «нет нетривиального production-инварианта» не обоснован: реальные production-used Node primitives проверяются. Gap — caller/lifetime/current terminal-state/sidecar arithmetic, не отсутствие полезного Kani вообще. Current success/vacuity/mutants не исполнялись.
- **Владелец/почему P4:** **167** — точный verification-only mismatch, **18** — stale retained closure. Scope/claims, не failed allocator invariant. Proof/counterfactual oracle не запускался.

## R17-VER-05 — Epoch shadow eviction моделирует старый swap-then-generation

- **Исходная оценка:** `R17-VER-05` **P4**. **Диспозиция: CONFIRMED CURRENT.** Категория: experimental model fidelity.
- **Доказательство:** `tests/loom_epoch.rs:17–19,76–85` обещает exact mirror AtomicSlot::evict, меняет value до generation. Настоящий `src/concurrent/epoch/hand.rs:395–437` делает generation CAS до value swap. `tests/loom_sharded.rs:83–104` содержит эту вторую shadow-последовательность; CI `3175–3183` выбирает обе. Pin текущих orderings в проверенных pin-тестах не найден.
- **Владелец/почему P4:** точного owner нет; **171** — другой interpreter residual. Старый copied model не отслеживает implementation change; отдельная модель/нынешний CAS не объявлены неправильными. Mutant/pin/model oracle не запускались.

## R17-VER-06 — Hidden test surface шире селектора scanner; часть reviewed reasons неверна

- **Объединены:** `R17-VER-06`, `R17-CQ-09`, `R17-API-03`, `R17-UNS-04`, все **P4**. **Диспозиция: PARTIALLY VALID/NARROWED** для composite; API reachability и неверные UNS reasons по отдельности source-confirmed.
- **Scanner:** `scripts/verify-dbg-hook-safety.mjs:373–395` узнаёт `pub (unsafe)? fn dbg_*`, не const/другие имена. AllocCore checker описывает такой же dbg-only scope (`verify-alloc-core-dbg-internals-exhaustive.mjs:20–25`). Вне шаблона: `SmallSidecar::owner_state_for_test` (`small_sidecar.rs:110–126`), gated injections (`directory.rs:718–725`; `maintenance_service.rs:206–233`), const dbg (`bootstrap/ensure.rs:212`; `heap_core/diag/diag_probes.rs:51`). Пропуск scanner не доказывает unsoundness этих gated hooks.
- **Actual stable-type leaks:** root reexports (`src/lib.rs:490–497,511–512`) открывают `AllocCore::refill_class/refill_class_bump/flush_class` (`alloc_core_small_magazine.rs:37–39,112–150,440–443`), SegmentLayout test forwarders (`segment_layout.rs:183–235`), Epoch buffer observer (`epoch_region.rs:655–661`), Sharded reset/buffer observer (`sharded_region.rs:710–734`) без internals gate. Reset чистит bindings, не occupied tokens; следующий claim способен взять ещё token (`474–526`). Это protocol/test/API boundary, не доказанный UB. Payload-start constants (`segment_layout.rs:23–34`) намеренно public protocol geometry; скрывать ли их — policy, не ещё один runtime defect.
- **Неверные reasons:** PURE_OBSERVERS включает slot state/generation/initialized (`verify-dbg-hook-safety.mjs:103–106`), вызывающие Registry::slot (`registry.rs:326–334`; `counters.rs:407–415`) → materialization/OOM abort (`registry.rs:135–141,250–271`). «no core access» (`script:129–142`) противоречит `HeapLease::core` (`claim.rs:486–518`) и MaintenanceLease::dbg_with_core (`410–414`). Lease exclusivity proof существует; сам доступ не soundness-hole. Observer label не доказывает provenance (P1 UNS-01).
- **Другие части:** `segment_table/mod.rs:58–59`, `segment_table_impl.rs:8–9` компилируют Vec-backed harness без internals cfg, но внешний module path доступен только с internals. Route cache wrappers (`registration.rs:55–75`) имеют только test callers. Для семи CQ hooks вызовы не найдены, но metadata/allowlists содержат некоторые имена: «нет caller» не равно «нет текстового упоминания». CQ totals **48+12/≥60** могут двойно считать underscore/suffix множества; нового census не было.
- **Владелец/почему P4:** **5/7/8/9** — hook-safety class; **9 P2-12** описывает старое переименование одного метода, не полное закрытие селектора (`TRACKED_hook_safety.md:746–756,810–825`). Живые scanner/API/reason gaps есть, old counts/blanket unsafe assertions сужены. Negative downstream/scanner fixtures не выполнялись.

## R17-LIF-02 — «One deliberate process kill» не описывает нынешние abort-tripwires

- **Объединены:** `R17-LIF-02`, `R17-VER-07`, `R17-CQ-07`, `R17-API-06(a,b)`, все **P4**. **Диспозиция: CONFIRMED CURRENT.** Категория: публичный no-panic/no-abort контракт.
- **Доказательство:** `src/global/sefer_alloc/mod.rs:179–199` выделяет registry chunk OOM. Claim использует fallible slot_or_none (`src/registry/heap_registry/claim.rs:213`); infallible abort — `bootstrap/registry.rs:250–271`, до него идут diagnostic slot methods. Другие source paths имеют invariant abort, например `route_slots.rs:106–121`, `directory.rs:882–885`, `alloc_core_small_reclaim.rs:24–45`, `claim.rs:523–534`. Наличие нескольких категорий показано без нового total census.
- **Сохранённые разные исходные инвентари:** CQ-07 — **106 sites / 25 files**, source-wide census с исключением comment-only matches (CQ appendix B.8). API-06 — **99 / 23**, non-comment строки в файлах, классифицированных как GlobalAlloc paths, **без per-site reachability audit**; maintenance_service/exact-object не включены. VER-07 — **110 / 28**, source-wide token census process::abort() (VER appendix A), не тот же явно comment-filtered перечень CQ. Это разные методы/области; единый новый aggregate здесь **не публикуется**, повторного scanner/script census не было.
- **Сужение:** invariant tripwire после порчи — не ordinary OOM failure. Item166 ограничивает unqualified «unrecognized pointer → no-op». API-06(c) arithmetic — отдельный P3, не duplicate prose.
- **Владелец/почему P4:** **154** — stale prose, **174** — прежний closed panic remediation, не доказательство правильности abort-doc. Текст создаёт ложные ожидания, не runtime failure сам по себе. Abort guard/behavior oracle не запускался.

## R17-LIF-03 — Release-panic scanner не охватывает формы и reachable files

- **Исходная оценка:** `R17-LIF-03` **P4**; пересечение scanner-части VER-07. **Диспозиция: CONFIRMED CURRENT.** Категория: lexical regression coverage.
- **Доказательство:** `tests/no_panic_doc_accuracy.rs:214–222` выбирает семь файлов; `474–487` ищет expect/panic/unreachable, не assert-family/unwrap. Enumeration закреплён на four (`140–195`), контракт — `sefer_alloc/mod.rs:95–139`.
- **Невидимые sites:** runtime-callable generation assert (`terminal_words.rs:48–50`), hardened issue assert (`segment_header_gen_table.rs:98–109`), virgin-output assert (`alloc_core_small_magazine.rs:136–150`), expect вне scan (`platform/numa.rs:91–95`; `exact_object/exact_shard.rs:33–35`). gen_at assert есть, production-caller не найден. Эти места ограничены invariant/constant предусловиями; legal alloc не показан падающим.
- **Владелец/почему P4:** **174** остаётся closed для исходного дефекта; точного owner расширенного residual нет. Gate не ловит классы новых panic sites. Expanded scan/mutants/runtime failures не исполнялись.

## R17-LOG-03 — INVARIANTS и остаточные M2 pin-обещания расходятся с текущими путями

- **Объединены:** `R17-LOG-03` **P4**, `R17-VER-08` **P4**, `R17-CQ-03` **P4**. **Диспозиция: PARTIALLY VALID/NARROWED.** LOG/VER document contradictions source-confirmed; CQ misuse residual не valid-use soundness.
- **Доказательство:** `docs/INVARIANTS.md:62–85` называет RemoteFreeRing и два отсутствующих tests (проверено поиском файлов). `95–98` отвергает все align≥SEGMENT, но biased reservation реализован (`src/alloc_core/large/alloc_core_large.rs:630–639`), тесты утверждают успех/выравнивание при успешном резерве (`tests/r8_large_alignment.rs:6–48`). `INVARIANTS.md:99–107` преувеличивает self-hosting/no std::alloc: metadata routes/leaves выделяются через independent System (`directory.rs:69–85`; `leaf_classes.rs:98–113`). M5 no-installed-global recursion — не запрет System allocations.
- **M6/M7:** spec `108–116` обещает retain-decommit/recommit и mask=root. Нынешний pool сохраняет commit или releases (`alloc_core_small_pool_impl.rs:167–215,428–429`); retain reset — test hook (`decommit.rs:93–108`). `mem_impl.rs:34–42` трактует mask как key, восстанавливает table root; это существенно для biased Large.
- **Тот же M2 pin gap:** `dealloc_own_base.rs:179–191` называет удалённый reclaim/test/X7 tracker. Publish/issue (`sidecar_bitmap.rs:83–119`) не несёт generations; reclaim (`alloc_core_small_reclaim.rs:46–66`) перед mutation проверяет текущие bitmap/residency. **[INFERENCE]** Намеренный double-free/reissue способен обойти current-state guards; исходный double-free уже вне unsafe contract (`mem_impl.rs:198–221`; README `1352–1364`). Schedule не исполнялся; он не доказывает отказ M1/M3 safe callers.
- **Владелец/почему P4:** **154** — source-doc часть; exact INVARIANTS/M2 pin owner нет. **163** — related alignment acceptance; **164/166** — другие residuals. Неверна spec/coverage проза, не вновь сломана гарантия законного double-free. File/spec/misuse oracle не запускались.

## R17-CQ-02 — Magazine upper tail bound остаётся под alloc-decommit

- **Исходная оценка:** `R17-CQ-02` **P4**. **Диспозиция: CONFIRMED CURRENT** для асимметрии. Категория: hardening/contract, не legal-pointer correctness.
- **Доказательство:** `src/registry/heap_core/free/dealloc_own_base.rs:170–173,292–295` обещает parity, но cfg-гейтит bump bound; substrate `src/alloc_core/small/alloc_core_small/dealloc.rs:93–99` делает его unconditional. `Cargo.toml:412,687` допускает fastbin/hardened без decommit. Shared scalar/batch calls: `dealloc_own_base.rs:384`, `dealloc_batch.rs:316`.
- **Владелец/почему P4:** **154** — misleading prose, exact hardening owner нет. Вход — uncarved/stale pointer вне unsafe contract; production включает decommit. Это **не duplicate** SEC-01 lower bound. Tail-address reissue oracle не запускался.

## R17-CQ-05 — Production modules всё ещё помечены experimental и подавляют dead_code

- **Исходная оценка:** `R17-CQ-05` **P4**. **Диспозиция: CONFIRMED CURRENT.** Категория: docs/lint scope.
- **Доказательство:** `src/alloc_core/segment/mod.rs:12–18,58–60` называет directory off-by-default, remote_bitmap not production; последний имеет module allow(dead_code). `Cargo.toml:415` включает directory в production; `small_sidecar.rs:39–41`, `find_segment.rs:269–270,398–506` показывают живое использование.
- **Владелец/почему P4:** **154**. Неточная классификация и blanket suppression; наличие реально скрытого warning требует компиляции, не проверено.

## R17-CQ-06 — Вторичные unsafe inventories не совпадают с каноническими attributes

- **Исходная оценка:** `R17-CQ-06` **P4**. **Диспозиция: CONFIRMED CURRENT.** Категория: audit docs, не сломанная confinement.
- **Доказательство:** `src/alloc_core/mod.rs:5–15` пропускает leaf_classes с allow на `leaf_classes.rs:4`. `src/registry/heap_registry/mod.rs:17–20` считает claim/counters/stack tier-1; anchored search находит claim (`claim.rs:13`), не counters/stack allow. `bootstrap/mod.rs:164–166`, `loom_shim.rs:315–317,377–378` ссылаются на отсутствующий stack implementor. `src/lib.rs:103–112` обещает six runtime companions и одновременно verification-only tagged-index-stack; Cargo `933–936` подтверждает verification-only. Предложение `lib.rs:173–183` повреждено. `lib.rs:246`/README `678` ещё говорят HeapSlot Send вопреки `heap_slot.rs:283–300`.
- **Владелец/почему P4:** **154** — prose, **167** — mirror. Неверная audit-картина, compile confinement не показана сломанной. Full inventory/count test/build не запускались.

## R17-CQ-08 — Counter docs содержат фантомы, stale zero-claims и неверную семантику

- **Объединены:** `R17-CQ-08`, `R17-SEC-05`, оба **P4**. **Диспозиция: CONFIRMED CURRENT.** Категория: diagnostic contract, не отдельный LOG-02 write bug.
- **Доказательство:** `directory_stats.rs:16–29` называет удалённые dirty counters и помечает живые storage-only; `33–46` утверждает unwired increments. Реальные increments: `find_segment.rs:407,417,426,471,597–607,634–637,691–745`; rescue: `alloc_core_small_impl.rs:250,348`, `alloc_core_small_magazine.rs:314`. Removed-counter definitions не найдены. `perf_diag.rs:21–38,211–225` повторяет zero-claims и ошибочно относит directory trust к 78(c), фактически perf **82**.
- **Ещё:** `vmem.rs:81–97` обещает reset six/four forwarders; `crates/aligned-vmem/src/bench_internals/reset.rs:15–50` перечисляет sixteen, vmem имеет combined forwarder `53`. `src/lib.rs:42–44`, `alloc_core/alloc_core/counters.rs:365–381`, `alloc_core_core_diag/totals.rs:17–21` обещают layout-mismatch counting/removed dealloc_foreign_routing. Increments (`routing.rs:27–31,62–65`; `mem_impl.rs:230–237`) Layout не сравнивают. Public field doc (`global/alloc_stats.rs:137–151`) уже корректно описывает route rejection.
- **Владелец/почему P4:** **154**, ошибочная owner-ссылка perf **82**. Неверная oracle/monitoring проза, не измеренная counter value или speedup. Counter/spec guards не запускались.

## R17-VER-09 — Cold metadata guards непоследовательны в release

- **Исходная оценка:** `R17-VER-09` **P4**. **Диспозиция: PARTIALLY VALID/NARROWED.** Категория: corruption containment/coverage.
- **Доказательство:** `segment_table_impl.rs:968–977` только debug-assert capacity перед raw write; free_top сразу за массивом (`segment_header_layout.rs:106–115`). `alloc_core_small_pool_impl.rs:269–272,665` только debug-assert no duplicate pool. `hash.rs:80–87` не ограничивает full-table probe; отдельный hash_insert debug guard — `61–77`.
- **Покрытие:** proptest `tests/segment_table_backshift_proptest.rs:92–101` вызывает SegmentHashHarness::insert → hash_insert (`harness.rs:135–136`). Но harness также открывает **real hash_insert_identity** (`141–145`); «нет test seam для production insert» неверно. Более узкий mismatch самого proptest остаётся.
- **Владелец/почему P4:** exact owner нет. Последствия требуют предварительного invariant violation; valid register сохраняет spare hash capacity. Release corruption/mutant oracle не выполнялись.

## R17-VER-10 — Per-PR op-stream differential находится ниже shipping magazine

- **Исходная оценка:** `R17-VER-10` **P4**. **Диспозиция: CONFIRMED CURRENT.** Категория: randomized coverage, не отсутствие production tests вообще.
- **Доказательство:** `tests/alloc_core_differential.rs:49–80`, `tests/heap_differential.rs:27–49,74–76` адаптируют AllocCore. Поиск proptest/RawAllocator/op_strategy нашёл substrate/region/geometry, не HeapCore/SeferAlloc op-stream differential. `fuzz/fuzz_targets/heap_core_ops.rs:20–33` ведёт direct SeferAlloc в одном потоке; CI fuzz-build только builds (`3913–3944`), fuzz-run schedule/manual (`3971–4003`).
- **Владелец/почему P4:** exact owner нет, **162** — related end-to-end acceptance. Конкретного pre-merge random oracle нет; deterministic/MT tests остаются. Не утверждается, что любой mutant пройдёт все gates. Differential/mutation oracle не запускались.

## R17-VER-11 — Hardware weak-memory coverage cross/QEMU статически не установлено

- **Исходная оценка:** `R17-VER-11` **P4 hypothesis**. **Диспозиция: NOT VERIFIABLE FROM STATIC SOURCE.** Категория: platform/coverage interpretation.
- **Доказательство:** `.github/workflows/ci.yml:3375–3430` задаёт ubuntu-latest/cross aarch64 и называет weak-memory load-bearing; `3437–3441` различает native timing/QEMU correctness. README `1392` предупреждает о неполной модели ARM у TCG; ARCHITECTURE `496` говорит relaxed-memory smoke. test-macos (`2219–2238`) выбирает реальные Darwin workloads, но macos-latest не пинит будущую hardware architecture.
- **Сужение:** старый runner log не устанавливает текущий hosted runner/QEMU memory model. «Никаких ARM reorderings» и «только macOS даёт hardware coverage» не выводятся из YAML. Отдельный native-arm tagged-index-stack job — member measurement, не general root acceptance.
- **Владелец/почему P4:** exact root owner нет. Исследование/документация, не доказанная ordering bug. Нужны actual runner/emulator identity и weak-memory litmus; не запускались.

## R17-UNS-02 — Kind-corruption contract разрешает teardown с копированием invalid enum

- **Исходная оценка:** `R17-UNS-02` **P4**. **Диспозиция: CONFIRMED CURRENT.** Категория: unsafe contract contradiction.
- **Доказательство:** `header_diag.rs:109–173`, dbg_stamp_kind_byte, допускает arbitrary corrupt bytes и явно разрешает dbg_unregister/dbg_recycle. SegmentHeader.kind — enum (`segment_header_impl.rs:165–194,351`); read_at (`744–749`) идёт в typed copy/assume_init (`node.rs:294–317`). Unregister/recycle копируют его (`segment_table_impl.rs:510,640`); read-only large_geometry_for_test тоже (`header_diag.rs:27–33`). Byte decode kind_at не делает typed copy valid.
- **Владелец/почему P4:** exact card нет; **5/7/8/9** — related hooks. Не новый safe-only P1: caller сначала использует unsafe corruption hook; kind-тесты восстанавливают поле вокруг ограниченного decode. Разрешённая contract-последовательность противоречива по source; Miri invalid-tag witness не выполнялся.

## R17-UNS-03 — Unsafe metadata hooks не оговаривают tag extent caller pointer

- **Исходная оценка:** `R17-UNS-03` **P4**. **Диспозиция: PARTIALLY VALID/NARROWED.** Категория: unsafe/provenance contract precision.
- **Доказательство:** `alloc_core_small_diag.rs:137–165,172–188,208–249,369–385` требует valid/live/exclusive pointer, но выводит metadata base из caller pointer. `decommit.rs:63–108`, `heap_core/diag/diag_probes.rs:349–385` имеют ту же форму. Safety blocks не требуют явно reference-derived tag на segment metadata.
- **Граница:** allocation-wide raw и raw от narrow reference не эквивалентны автоматически в experimental borrowing models. Source подтверждает риск неполного контракта; exact SB/TB outcome каждого narrowed-tag сценария требует Miri. Это не семь исполненных UB-witness и не ещё один safe-API P1. Проверенные специалистом callers передают allocator raw pointers.
- **Владелец/почему P4:** **5/7/8/9** — class, **164** — related model sensitivity, не exact hooks owner. Нужен narrow-slice SB case с canonical/raw controls; не запускался.

## R17-UNS-05 — Hardened batch refill восстанавливает pointer из exposed integer

- **Исходная оценка:** `R17-UNS-05` **P4**. **Диспозиция: CONFIRMED CURRENT** для cast/contract проблемы. Категория: provenance hygiene/feature coverage, не установленный current UB.
- **Доказательство:** `src/registry/heap_core/alloc/batch.rs:169–184` делает pointer→usize→pointer для gen bump; Safety рядом говорит live/alignment, не exposure dependency. `hot.rs:68–79` передаёт такой contract. `os.rs:133–149` описывает map_addr policy и strict-provenance несовместимость старой идиомы. Local Miri (`scripts/miri.mjs:45–127`) и CI Miri не выбирают hardened+batch-api.
- **Сужение:** рядом действительно есть as usize exposure; нынешний cast не объявлен обязательно provenance-less UB. «Единственный runtime int→ptr» — исторический typed census специалиста, не новый total type analysis. Замена exposure на addr без изменения reconstruction — future-edit hazard.
- **Владелец/почему P4:** exact card нет. Contract/policy/Miri reachability, не P1. Нужен strict-provenance batch-refill oracle, не запускался.

## R17-API-02 — Powerset fail-fast и MSRV negative-feature gaps сохраняются

- **Исходная оценка:** `R17-API-02` **P4**. **Диспозиция: PARTIALLY VALID/NARROWED.** Категория: CI coverage/process.
- **Доказательство:** `.github/workflows/ci.yml:4084–4095`: weekly/manual Linux depth-two no-dev-deps без keep-going/warning-deny. MSRV checks/test-builds all-features (`2587–2618`); Windows runtime production (`2772–2786`). NUMA в explicit relevant row только с production (`scripts/check-matrix.mjs:235–238`). NUMA-only mismatch — отдельный P3 API-01.
- **Сужение:** old **205/390**, 185-unchecked/warning totals — историческое CI-свидетельство обзора, не current powerset execution. Следующий aligned-vmem шаг skipped при previous failure по workflow semantics, exact current counts не вычислены. Negative-feature MSRV gap не доказывает MSRV-incompatible код.
- **Владелец/почему P4:** **107** — NUMA combination, **161** — warnings, **95** — осознанно below all-targets root powerset. Exact scheduled-incident/fail-fast owner нет. Dispatch/powerset/MSRV oracle не запускались.

## R17-API-04 — NUMA composition выбирает eager/no-rescue paths

- **Исходная оценка:** `R17-API-04` **P4**, (c) — hypothesis. **Диспозиция: PARTIALLY VALID/NARROWED.** Категория: feature contract/policy, не новый runtime null.
- **Доказательство:** `bootstrap.rs:78–101,126–127` отключает primordial lazy с NUMA. `alloc_core_large.rs:652–653` делает reserved_capacity=usable; grow отвергает end выше capacity (`realloc_fastpath.rs:514–518`). Forced Small rescue not-NUMA (`find_segment.rs:111–114`; `alloc_core_small_impl.rs:246,344`; `alloc_core_small_magazine.rs:309–315`), хотя directory NUMA-capable (`find_segment.rs:269–270`), вопреки `208–211`. `tests/segment_directory_numa.rs:317–324` пинит rescue exclusion.
- **Сужение:** Cargo уже сообщает об общей NUMA/lazy exclusion (`Cargo.toml:721–727`), значит «совсем не документировано» чрезмерно; README trap (`1432–1451`) называет лишь ordinary Small. Stale-negative+OOM harm standalone NUMA — hypothesis, не observed failure.
- **Владелец/почему P4:** **154** — stale rationale; perf **26/29** — related opt-in policies. Windows committed-byte/NUMA stale-negative OOM oracle не запускались.

## R17-API-05 — Feature docs расходятся с manifest/public render

- **Исходная оценка:** `R17-API-05` **P4**. **Диспозиция: PARTIALLY VALID/NARROWED.** Категория: docs/public feature contract.
- **Доказательство:** `docs/INTEGRATION.md:23–26` даёт 4 production members вместо 6 (`Cargo.toml:415`), туда направляет public doc. README `1425` не даёт alloc-stats dependency, Cargo `524` — alloc-core; рядом нет объяснения internals. Cargo цитирует removed flat paths (`180,185,420,650–657,922–925`), старый cap (`304`). SegmentLayout forwards feature-sensitive geometry (`segment_layout.rs:23–34,62–92`; `segment_header_layout.rs:81–89`). Root source search не нашёл doc(cfg); docs.rs выбирает production (`Cargo.toml:37–38`), исключая другие APIs.
- **Сужение:** curated table не обязана перечислять все 30 features; docs.rs production-only не defect сам по себе. Проверены конкретные неверные dependency/composition и недоговорённые sensitive constants/availability; 27-comment census не исполнялся. Doc-build failure не заявлен.
- **Владелец/почему P4:** **154** related source prose, внешние guide/manifest не целиком в scope; **156** — closed strict-rustdoc wiring. Exact all-feature-doc card нет. Doc/feature/downstream oracle не запускались.

## R17-API-07 — Experimental требует AtomicU64 без явного atomic-width gate

- **Исходная оценка:** `R17-API-07` **P4**. **Диспозиция: PARTIALLY VALID/NARROWED.** Категория: target requirement/diagnostic coverage.
- **Доказательство:** unconditional AtomicU64 — `epoch_region.rs:68,87`, `lock_free_region.rs:18,36`, `sharded_region.rs:137,181,303`. Allocator pointer-width gate (`src/lib.rs:391–401`) только alloc-core; experimental/pinning edges (`Cargo.toml:135,151`) его не включают. target_has_atomic=64 gate в root concurrent/lib не найден.
- **Сужение:** companion gates требуют **pointer-width CAS**, не обязательно AtomicU64 (`crates/sefer-region/src/lib.rs:58–64`; `once-ptr-cell/src/lib.rs:310–326`). Target cfg/compiler/dependencies не queried; конкретные PowerPC/MIPS/RISC-V E0432 примеры не подтверждены для complete supported build.
- **Владелец/почему P4:** exact card нет; **43/60** — другие platform contracts. Неописанное требование/coverage, не observed cross-build failure. Нужен std target с ptr atomics без 64-bit atomics compile oracle, не запускался.

## R17-API-08 — OS-keyed TLS recursion остаётся внешней платформенной гипотезой

- **Исходная оценка:** `R17-API-08` **P4 hypothesis**. **Диспозиция: NOT VERIFIABLE FROM STATIC SOURCE.** Категория: portability/bootstrap uncertainty.
- **Доказательство:** `src/global/tls_heap.rs:127–160,256–280` использует const TLS и LOCAL до bind, описывает OS-keyed teardown, но std first-access allocation implementation отсутствует. Vmem допускает Unix families (`crates/aligned-vmem/src/os/unix.rs:704–725`), это не доказательство native TLS/root support. README `423` ограничивает claims other 64-bit targets.
- **Владелец/почему P4:** **43/60** — related reasoned-from-spec, не exact TLS owner. Поддерживаемая цель с отказом не установлена. Нужны target TLS cfg/std allocation path/installed first-access oracle; ничего не запускалось/не queried.

## R17-API-09 — Pinning раскрывает внешний CoreId без реэкспорта

- **Исходная оценка:** `R17-API-09` **P4**. **Диспозиция: PARTIALLY VALID/NARROWED.** Категория: public API/semver coupling.
- **Доказательство:** `src/concurrent/pinning.rs:67,160–171` использует core_affinity::CoreId в public signatures, `Cargo.toml:905` — 0.8; `src/lib.rs:496–497` реэкспортирует PinnedRunner, не CoreId. CoreId reexport не найден.
- **Опровергнутый blanket subclaim:** direct core_affinity dependency **не обязательна каждому caller**: возвращённые ids можно передать обратно с type inference. Explicit naming/construction требует доступа к external type; identity связывает будущую dependency migration с API compatibility. Version bump/break не произошёл.
- **Владелец/почему P4:** exact owner нет; **159** — отдельный empty-core-list risk. Недокументированное external coupling nondeprecated API; downstream fixture не выполнялась.

## R17-LIF-04 — Panics у PinnedRunner не описывает отказ spawn

- **Исходная оценка:** `R17-LIF-04` **P4**. **Диспозиция: PARTIALLY VALID/NARROWED.** Категория: public API docs.
- **Доказательство:** `src/concurrent/pinning.rs:211–254` описывает worker panic, вызывает std::thread::scope/Scope::spawn без Result/error handling. Bind-комментарий говорит только out-of-range (`243–249`), а `sharded_region.rs:678–684` также проверяет intact TLS.
- **Граница:** **[INFERENCE]** По документированной std семантике Scope::spawn OS creation failure тоже паникует. Invoked std implementation/OS-refusal witness в repo нет; resource limits не менялись. Подтверждены delegation/doc omission, не наблюдённый spawn failure.
- **Владелец/почему P4:** exact owner нет; **159** — другой constructor condition. Contract accuracy nondeprecated opt-in API, не allocator soundness. Doc oracle не запускался, новый fix не предписан.

## R17-SEC-03 — Own Large free не проверяет exact payload, как contract уже предупреждает

- **Исходная оценка:** `R17-SEC-03` **P4**. **Диспозиция: CONFIRMED CURRENT** как documented hardening asymmetry, не valid-use runtime defect.
- **Доказательство:** `src/alloc_core/alloc_core/mem/mem_impl.rs:261–293,363–487` claims Large и caches/releases reservation без сравнения адреса с прочитанным payload_offset. Foreign lookup требует exact payload (`directory.rs:876–880`), in-place realloc тоже (`realloc_fastpath.rs:292–295`). `mem_impl.rs:198–221` явно запрещает interior pointer и предупреждает о release whole span; address-only resolution — `34–42`.
- **Владелец/почему P4:** exact card нет; **166** про foreign Small, не own Large. Existing contract-respecting limitation/hardening opportunity, не новая нарушенная гарантия. Isolated interior-free/lifetime oracle не выполнялся; one-comparison cost/speed claim не делается.

## Результат исправления R18 — R17-UNS-05

- **Статус: ЗАКРЫТО (2026-10-09).** Pointer-to-integer-to-pointer conversion
  был частью hardened generation-table bump в `alloc_batch`; этот bump и сама
  таблица удалены как путь без runtime-читателя.
- **Текущее доказательство:** `alloc_batch` по-прежнему использует
  `segment_base_of_ptr(p) as usize` только для дедупликации stamp по числовому
  адресу сегмента; этот ключ больше не реконструируется как указатель. Поиск по
  текущим `src/`/`tests/` не находит `bump_gen`, `gen_at` или
  `segment_header_gen_table`.
- **Граница:** для удалённого batch-generation пути отдельный Miri witness больше
  не применим. Это не закрывает другие hardened+batch verification-coverage
  вопросы из `R17-VER-03`; runtime/производительный результат не заявляется.
