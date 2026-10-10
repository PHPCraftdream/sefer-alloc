# Синтез ревью src/ раунда 17 — P5

[Общий индекс](2026-10-08-src-review-xxs-round-17-synthesis.md)

**Текущая цель:** `ff3d60bb571f706cd8f034665fac8c3f4b6f0d23`; база исходных обзоров: `453439c123f71a98890408dd0170bd909ad9ecdf`.

**Ограничения:** только статическое чтение/поиск. Не выполнялись команды, запуск/запрос rust-analyzer, сборка, тесты, линтеры, форматтеры, бенчмарки, скрипты, CI или runtime-пробы. Записывались только запрошенные новые отчёты. Оптимизации не реализованы; speedup, снижение RSS, конкретный emitted instruction stream или promotion не заявляются.

Здесь **четыре канонических находки с исходной P5-оценкой**. Все прежние неоценённые предложения и дополнительные ownership-сведения сохранены в приложениях A–C ниже; им **не назначен** P5 или другой P-уровень, и они не увеличивают totals этой группы.

## R17-CQ-10 — Похожие mutation/lifecycle-последовательности остаются продублированными

- **Исходный ID/источник severity:** `R17-CQ-10` **P5**, code-quality-обзор. **Проверенный приоритет: P5, без изменения.** Категория: поддерживаемость.
- **Диспозиция: CONFIRMED CURRENT** для конкретных парных шаблонов, не нового механического duplication census.
- **Текущее доказательство:** magazine bitmap-clear loops остаются в `src/registry/heap_core/free/dealloc_own_base.rs:482–494` и `state/tcache_flush.rs:86–98`. Realloc move sequences — `src/alloc_core/alloc_core/mem/mem_impl.rs:677–694`, `src/registry/heap_core/free/realloc.rs:336–347,418–434`; у own/foreign путей разные pointer proofs, текстовое сходство не делает их безусловно взаимозаменяемыми. Large deposit зеркалится в `mem_impl.rs:432–452` и `src/alloc_core/large/alloc_core_large.rs:802–821`; eviction release — `alloc_core_large_cache_eviction.rs:41–54,235–244`.
- **Граница:** normalized-window totals и все diagnostic validation-copy counts специалиста не пересчитывались. Независимая валидация указателя в нескольких местах может быть намеренным safety evidence, не автоматически «мёртвым весом». Сбой, вызванный нынешними копиями, не показан.
- **Владелец/почему P5:** correctness **154** — связанный prose/structure debt; exact refactoring owner нет. Риск сопровождения, не observed invariant violation. Предложенные helpers требуют сохранить разные proofs и runtime/Ir controls; ничего не написано/не запускалось.

## R17-CQ-11 — Discovery сложен, feature predicates повторяются

- **Исходный ID/источник severity:** `R17-CQ-11` **P5**. **Проверенный приоритет: P5, без изменения.** Категория: структурная поддерживаемость.
- **Диспозиция: PARTIALLY VALID/NARROWED.** `find_segment_with_free_impl` занимает `src/alloc_core/small/alloc_core_small/find_segment.rs:186–612`, имеет cfg-гейтированные позиционные параметры и многоконтекстный finalize (`623–630`). Promotion predicate копируется в `src/registry/heap_core/free/dealloc.rs:69–102`, `dealloc_own_base.rs:219–230,255–264,312–325`, `src/alloc_core/mod.rs:210–217`.
- **Ещё текущий пример:** not-NUMA `unsafe fn init_node_ids_raw` имеет пустое signature-parity тело (`src/alloc_core/segment/segment_directory/segment_directory_impl.rs:302–316`). Это не недоделанная реализация: doc прямо говорит, что в этой конфигурации registration table отсутствует.
- **Сужение:** approximate cyclomatic complexity, nesting, cfg totals и census восемнадцати копий — исторические heuristic metrics, не исполненный анализ сейчас. Из наличия копий не выводится нынешний mismatch/compile failure. Refactoring/build-cfg варианты исходного обзора — предложения, не обязательные fixes, «найденные» этим синтезом.
- **Владелец/почему P5:** correctness **154** — related structure/prose; perf **67** — code-size/inlining исследование, не подтверждённый complexity performance defect. Change/maintenance risk. Metrics/feature-matrix/performance oracle не запускались.

## R17-CQ-12 — Dead-shape параметры и широкие/устаревшие lint suppressions остаются

- **Исходный ID/источник severity:** `R17-CQ-12` **P5**. **Проверенный приоритет: P5, без изменения.** Категория: поддерживаемость, не новый достижимый safe-pointer дефект.
- **Диспозиция: PARTIALLY VALID/NARROWED.** `AllocCore::safe_payload_read_span` сохраняет `own_segment: bool` и описывает неиспользуемую false-ветку (`realloc_fastpath.rs:102–139`); три найденных текущих callers передают true (`mem_impl.rs:677`; `heap_core/free/realloc.rs:336,558`). Wrapper `SizeClasses::is_huge` сохраняет Phase-10 allow и не имеет найденного caller в source/tests/benches/examples (`platform/size_classes.rs:340–343`). Его тело делегирует в member crate; одноимённый member-метод этим не объявляется мёртвым.
- **Примеры suppressions:** `segment_header_views.rs:9`, `terminal_words.rs:36`, `segment_table_impl.rs:706` сохраняют future/test-use reasons при живых callers. Root module allows остаются в `src/lib.rs:447–449,469–471,481–483`; remote_bitmap allow учтён в CQ-05, не вторым P5. File-wide `sidecar_stats.rs:39–48` allow имеет явное feature-cross-product обоснование; его устаревание не установлено.
- **Сужение:** source показывает unused branch/broad suppression, но не какие warnings даст другая attribute-форма. Expect-lint build/warning census не запускались. Safe private helper с raw pointer и owner/mapping предусловиями не доказывает безопасный downstream UB-путь самим фактом сигнатуры.
- **Владелец/почему P5:** correctness **154**, общий prose/allow residual. Пока нет конкретного broken invariant/hidden warning, это P5. Чистка не выполнялась.

## R17-CQ-13 — Alias wiring, терминология и ответственность файлов неоднородны

- **Исходный ID/источник severity:** `R17-CQ-13` **P5**. **Проверенный приоритет: P5, без изменения.** Категория: conventions/navigation/maintenance.
- **Диспозиция: PARTIALLY VALID/NARROWED.** `src/alloc_core/mod.rs:43–104,116–152` сохраняет group aliases/private use. Например `69,75,78` буквально не соответствуют разрешённым для mod.rs mod/pub mod/pub use. Default-equivalent path attributes остаются в `alloc_core/alloc_core/mod.rs:45–56,75–103`, `small/alloc_core_small/mod.rs:18–35`; cross-directory paths — `segment/mod.rs:34–40,56`.
- **Имена/ответственность:** HeapRegistry/MaintenanceLease/HeapLease вместе в `heap_registry/claim.rs:34,366,459`; две policy и Profile в `config/profile.rs:111,160,350`; несколько resolver types/functions в `global/tls_heap.rs`. `heap_slot.rs:93–99` намеренно alias LIVE→OWNED. `docs/GLOSSARY.md:12–27` не раскрывает общие R-review/UBFIX/RAD/domain terms из CQ; X7 и flat HeapCore path там устарели.
- **Сужение/опровержение blanket трактовки:** правило проекта — **одна ответственность**, не pub-token limit; protocol-constant clusters и hidden test forwarders санкционированы явно. Четырнадцать multi-export файлов не доказывают четырнадцать нарушений. Одинаковые state constant names в разных модулях не symbol collision; aliases сохраняют пути намеренно; feature-driven constant value change не автоматически semver type break. Exact path/export/token counts — исторические census, не новый запуск.
- **Владелец/почему P5:** correctness **154** related docs/structure, exact all-wiring owner нет. Runtime defect не установлен. Codemod, rename, новая convention, source edit или formatter не выполнялись.

## Приложение A — R17-OPT-01…13, все предложения performance-обзора

**Исходная severity:** P отсутствует. Таблица performance-обзора оценивает ожидаемый выигрыш/риск/цену проверки (В/С/Н — высокий/средний/низкий), не severity дефекта; эти исходные labels сохранены.

**Диспозиция обещанного эффекта/будущего протокола каждого предложения: NOT VERIFIABLE FROM STATIC SOURCE.** Текущие предпосылки проверены по приведённым участкам; это не проверка ещё не реализованного кандидата, его корректности, эффекта или исходной risk-оценки. Ни один нижеописанный oracle не исполнялся.

| Исходный ID; выигрыш/риск/цена | Текущая предпосылка и границы | Существующий owner/связь | Нужный execution-only oracle, не запускался |
|---|---|---|---|
| R17-OPT-01; В/Н/Н | `bitmap_scan.rs:22` всё ещё обменивает каждое слово; `sidecar_bitmap.rs:117,131–145` делает terminal RMW и fixed-range scan. Load-before-swap требует отдельного strict-cut proof, не измерен. | PRF-01; CON H3; residual закрытого perf80, perf82 | Matched multisegment/growth iai, word activation; paused publication/strict trim и producer latency controls. |
| R17-OPT-02; В/С/В | SmallSidecar имеет pending/classes (`small_sidecar.rs:13–17`); publisher заканчивает terminal RMW (`sidecar_bitmap.rs:99–118`). Summary не реализован. Второй RMW меняет last-state-access contract; descriptor pin lifetime отличен от reservation credit (`directory.rs:210–223`). | PRF-01; related perf70/исторический perf80 | Loom/Miri summary/pending races/lifetime; owner cost и producer p99/HITM в одном режиме. |
| R17-OPT-03; С/С/С | Discovery дренирует до bins (`find_segment.rs:487–506,709–729`). Zero-credit skip/BinTable-first меняют reclamation/hostile-duplicate containment; correctness/RSS equivalence не установлена. | PRF-01; perf82 | Discovery scaling/activation, invalid duplicate controls, retention/latency той же нагрузки. |
| R17-OPT-04; Н/Н/Н | `directory.rs:56–65` zero-allocates, затем строит atomics; `small_sidecar.rs:25–36`, `leaf_classes.rs:46–60` явно инициализируют поля. Замена init требует language-validity/codegen/Miri проверки, подсчёт source writes не гарантирует выигрыш. | PRF-04; perf78(b) | Heap/route construction Ir, native RSS/commit и initialization/provenance controls. |
| R17-OPT-05; С/Н/С | Refill prepares per block (`alloc_core_small_impl.rs:550–575`), commit revalidates (`issue_transaction.rs:108–121`; `route_slots.rs:106–147`). Whole-batch proof/capability только предложен. | Ph3a IssueTransaction; exact отдельного perf owner нет | Refill dose-response shipping layer, required transaction mutants/churn controls. |
| R17-OPT-06; С/С/С | Deposit unregister/cache hit register (`mem_impl.rs:401`; `alloc_core_large.rs:451–467`); route cache wrappers только в тестах. Видимый cached route — protocol/layout change. | PRF-03 | Registration activation, generation/reuse/lifetime tests, MT timing/kill controls. |
| R17-OPT-07; С/С/В | `directory.rs:142–153,868–897` делает lock/pin/recheck; `shard_lock.rs:48–61` повторяет CAS, не load-first wait. Shard array/incarnation adjacent (`directory.rs:698–700`); measured coherence defect не установлен. | CON H1/H2; perf70 | Real GlobalAlloc fan-in latency/HITM, failed CAS/pin activation, fallback lock-order controls. |
| R17-OPT-08; Н/Н/Н | `alloc_core_small_reclaim.rs:36–44` вычисляет class-derived divisibility. Assembly не читался; actual div и instruction saving не проверены. | Exact owner нет | Remote reclaim throughput/Ir, all-class alignment rejection, exactly-once activation. |
| R17-OPT-09; Н/С/С | PerClass count/slots и глубокий top остаются (`tcache.rs:232–288`; `hot.rs:291–340`). Reverse stack меняет compaction/virgin-mask indexing. | PRF-05; related perf18/44, не переоткрыты | Multiclass cache/cycle measurement, prefix/churn controls, mask/flush correctness. |
| R17-OPT-10; С/Н/Н | Bounded drain расходует unit на empty cut (`sidecar_drain.rs:128–145`). Nonempty accounting требует другого ограничения empty inspections. Maintenance SLA violation не установлен. | PRF-01; perf78(a) | FREE-heap retire latency/pass counts, work per visit, retention с service. |
| R17-OPT-11; Н/Н/Н | Large realloc сначала sweeps (`heap_core/free/realloc.rs:181–203`), Large move alloc способен sweep снова (`hot.rs:249–254`). Условно при достижении move leg, не каждый realloc. | perf78(c) | Realloc cost по active Large count, drain activation/overlapping publication correctness. |
| R17-OPT-12; С/В/В | Mixed leaves alloc/copy 256 class bytes (`leaf_classes.rs:88–113`), carving смешивает классы. Class-run alignment/packing — placement policy с возможным tail waste. | perf3/5/78(b) | Mixed-leaf/System activation, density/fragmentation/RSS/latency одного режима. |
| R17-OPT-13; С/С/С | Claim поглощает hint до fresh bump (`stack.rs:108–140`); дополнительные hints/probing не реализованы. | PRF-04; correctness78.18 только closed taxonomy | Thread waves, claim latency/retention и delayed FREE/maintenance/ABA proof. |

**Пути в таблице сокращены только внутри уже указанных групп:** bitmap/leaf — `src/alloc_core/segment/remote_bitmap/`; discovery/refill/reclaim — `src/alloc_core/small/`; routes — `src/registry/segment_route/`; sidecar_drain/mem — `src/alloc_core/alloc_core/`; hot/tcache/realloc — `src/registry/heap_core/`. Это ссылки на текущие symbols, не исполненные сценарии.

## Приложение B — Остальные локальные гипотезы, исходные labels сохранены

Метки H/O повторяются в разных обзорах, поэтому каждая квалифицирована источником. **Исходная P-severity отсутствует; новая не назначается.** Performance/future-safety outcomes — **NOT VERIFIABLE FROM STATIC SOURCE**. Source-confirmed prose contradictions относятся к указанным formal P4 issues, не становятся дополнительными canonical defects.

| Review-local labels | Проверенная предпосылка и предел диспозиции | Owner/canonical mapping; неисполненный oracle |
|---|---|---|
| CON H1, H2 | CAS-waits и checked ref increment под lock остаются (`shard_lock.rs:48–61`; `fallback.rs:469–483`; `directory.rs:142–153`). TTAS/backoff/pin speed gain не доказан. | OPT-07; perf70. Failed CAS/pin activation, fan-in latency/HITM и lifetime model. |
| CON H3 | Unconditional swap и strict scan от 0 (`bitmap_scan.rs:22`; `sidecar_drain.rs:213`; `sidecar_bitmap.rs:126–145`). Completed-publication inclusion — отдельный proof, не следствие предложения load. | OPT-01/PRF-01; closed perf80 residual. Thread-churn counts/publication litmus. |
| CON H4, LIF H4 | Shutdown-aware trim не реализован: `tls_heap.rs:197–224` trims, shutdown flag в source не найден. Ни exit-time benefit, ни Windows reachability не измерены. | Conditional CON-01/LIF-01. Child exit/held-lock oracle, syscall/time evidence. |
| CON H5 | Exact lookup до layout routing любого free (`global_alloc.rs:62–67`). Layout filter меняет misuse outcome, не доказанный выигрыш. | CON-05. Mixed exact/nonexact lock activation. |
| CON H6, LIF H2 | Worker fallback try-lock удерживается через background step/full trim (`maintenance_service.rs:147–150`; `ownership.rs:119–156`). Свободный lock в момент try не доказывает workload-idle heap. | perf78(a)/66. Lock hold/wait, cache-hit/retention A/B. |
| CON H7 | remote_head initialized/reset/read in diagnostics (`terminal_words.rs:17,121–159`; `header_diag.rs:63–76`), не нынешний intrusive producer. Maintenance CAS на initialized slots (`claim.rs:83–102`); owner stamp Acquire/Release (`ownership.rs:75–82`). Dead-state/codegen/false sharing/order benefit не измерены. | Related correctness154/167; exact perf owner нет. Use proof, ordering/model controls, target activation/cost. |
| CQ §2.15(1), SEC H3 | Write-only gen table подтверждена; removal RSS/latency outcome остаётся hardened-only hypothesis. | CQ-01/CON-02/SEC-04. Layout/issue/reserve activation, Ir/RSS. |
| CQ §2.15(2) | Large inline bodies есть (`mem_impl.rs:253–255`; `dealloc_own_base.rs:193–195,339–341`; `alloc_core_small_impl.rs:162–163`); cold outlining code-size/i-cache gain не установлен. | perf67. Code-size/iai/activation, не запускались. |
| CQ §2.15(3) | Duplicate patterns существуют; perf-neutral helper extraction не гарантирует codegen equivalence. | CQ-10. Distinct proofs/correctness и Ir controls. |
| API O-1 | HeapSlot/remote align(64) (`heap_slot.rs:103,150`). Real line size/placement/cross-core sharing Apple/POWER не queried/measured. | Exact owner нет. Native line/field/HITM при alloc-stats activation; align128 win не заявлен. |
| API O-2 | NUMA отключает primordial lazy (`bootstrap.rs:101,126–127`); смена policy не измерена. | API-04; related perf26. Windows committed-byte/path и same-regime cost/benefit. |
| SEC H1 | Lower bound отсутствует; стоимость его добавления «within noise» не проверена. | SEC-01. Shipping free activation/instruction/latency controls. |
| SEC H2 | Raw continuation остаётся; safe-linking benefit/cost не реализованы/не проверены. | SEC-02. Pop activation, corruption controls/shipping-layer performance. |
| LOG H1 | Re-arm при headroom transition добавляет clock work; кандидат/его cost не измерены здесь. | LOG-01; perf42. Crossing/idle-burst activation, hit-rate/release/latency вместе. |
| LOG H2 | Stale span/CachedLarge/hit/one-tick проза остаётся (`segment_header_impl.rs:363–370`; `alloc_core_impl.rs:163–174`; `alloc_core_large.rs:105–114`; `alloc_core_large_cache.rs:424–425`). Header writes до register, cached pages committed. Это source-confirmed docs debt, не optimization result. | CON-06/CQ-04; correctness154. Не новая finding; guard не запускался. |
| LIF H1 | Re-claim full trim (`claim.rs:308–318`; `ownership.rs:108–156`), не ingress-only drain. Сужение может обменять refill cost на retained RSS; обе оси не измерены. | perf13/related69. Spawn/join release/reserve activation, RSS/first-alloc latency одного режима. |
| LIF H3 | Member OncePtrCell normal loser pure-spin (`crates/once-ptr-cell/src/imp.rs:23–38,525–531`) и exact spin (`exact_shard.rs:184–192`) не доказывают measured root tail-latency defect. | CON H1/CON-05; exact standalone owner нет. Oversubscription/sentinel activation/wait latency. |
| LIF §3 maintainability, пятый блок | NUMA reentrancy example, reverse-order TLS prose, whole-slot-retention prose, removed free callers — docs concerns, не доказательство actual reentrant allocation/new lifecycle race. | CON-06/CQ-04; correctness154 и stale site list fork152. |
| UNS H1 | Unregister/recycle typed read (`segment_table_impl.rs:510,640`); field-only alternative не реализован, пересекается с corrupt-enum contract. | UNS-02. Typed-value controls/cold Ir, не запускались. |
| UNS H2 | TORN integer sentinel не dereferenced (`tls_heap.rs:125`). Batch exposure feeds reconstruction (`batch.rs:172–184`); addr cleanup до reconstruction change не доказана безопасной. | UNS-05. Strict-provenance/lint/Ir controls, не запускались/не включались. |
| UNS H3 | Safe private membranes имеют unsafe-entrypoint предусловия (`routing.rs:10–19`; `dealloc_own_base.rs:341–352`; `mem_impl.rs:34–42`; `segment_table_impl.rs:489–510,614–640`). Само наличие не external safe misuse path. | Related safety-boundary docs, exact card нет. Caller-contract/refinement; runtime witness должен сначала доказать reachable caller. |
| UNS H4 | Manual HeapSlot Sync объясняет serialized handoff (`heap_slot.rs:261–300`); future thread-affine field гипотетично, не текущий demonstrated unsound field. | Related acceptance162, exact future-field owner нет. Type-bound/field proof; compile experiment не запускался. |

## Приложение C — Дополнительное evidence существующих карточек и не-находки

Эти distinctions сохранены из подробных разделов обзоров, **не являются новыми P-level findings**. Карточки не редактировались и не закрывались.

- **VER §2.2:** **17** содержит старые reclaim/ring/«effectively single-threaded» аргументы, не описывающие current descriptor/fallback (`routing.rs:26–44`); см. CON-04/VER-03. **18** говорит о nineteen/ring harnesses вопреки нынешним thirteen declarations; **167** — verification-only stack mirror, см. VER-04/CQ-06. У **160** устарело «модели вообще нет», но actual-type/refinement не доказаны (CON-03).
- **CQ §2.14 / API appendix D:** у **107** старые import/magazine deadness sites иначе gated (`alloc_core_small_reclaim.rs:3–14`; `magazine_bitmap.rs:133–146`), однако комбинация имеет API-01 type mismatch. Source correction старого warning не passed clippy closure. У **161** Layout gated/SegmentMeta import и старый predicate reference исчезли; часть mut bindings suppressed, exact-span mut остаётся (`alloc_core_large.rs:584–606`). Current warning total требует исполнения. **158** — current-node-only sync residual, отличный от all-bucket publish_empty; NUMA witness не запускался. **9** old renamed-hook closure не закрывает entire current scanner-name gap.
- **LIF appendix B:** **152** перечисляет снятые overflow/spill/deferred (`TRACKED_platform_contracts.md:313–318`); README `1698–1718` обобщённо называет нынешние frozen locks/ownership. Record correction, не доказательство Windows exit. **23/162/166**, closed **174**, сохраняют scoped status.
- **Сознательно не повышенные security-кандидаты:** high-alignment VA amplification связано с correctness **163**/perf **53**; unsafe misuse не повышено до safe exploit. Numeric hash harness API и gated corruption hooks не объявлены unsound лишь за raw pointer. **164/166/171** не regraded и не claimed fixed.
- **Внетематические/исторические наблюдения:** старые CI fuzz leaks/red gated bodies, counters/bench tables, Darwin advisory behavior и future-platform утверждения не установлены как current runtime outcomes независимо. Они сохраняют контекст исходных обзоров/owners, не превращены в новые invented issues. Конкретные stale paths/contracts консолидированы в formal prose/counter/feature/coverage entries.

**Прочие open cards и гипотезы сохранены без изменений, кроме явно перечисленных ниже R18 updates.** Это не новое исполнение performance/coverage triggers и не release acceptance.

## Обновление R18 — CQ §2.15(1), таблица поколений

Статическое наблюдение о write-only hardened generation table больше не
описывает текущее дерево: таблица и её writers удалены после подтверждения
отсутствия runtime reader. Формальная P3-диспозиция — R17-CQ-01 в отчёте
[P3](2026-10-08-src-review-xxs-round-17-P3.md); полное закрытие занесено в
`docs/perf/OPEN_ITEMS_ARCHIVE.md`. Исходные OPT-гипотезы приложения A остаются
без P-оценок; измерений RSS/Ir и speedup не заявлено.

## Обновление R18 — CQ-12, dead-shape realloc parameter

Только две подтверждённые dead-shape части частично закрыты. У
`AllocCore::safe_payload_read_span` удалён параметр `own_segment` и никогда
невызывавшаяся false-ветвь: все три текущих caller'а сначала доказывают
собственное владение сегментом. Lazy-commit path по-прежнему читает
`committed_payload_end`; eager path ограничивает span `SEGMENT`. Ownership и
plain-store ограничения теперь описаны непосредственно в контракте метода.
Внутренний `SizeClasses::is_huge` wrapper и единственная устаревшая ссылка на
него также удалены; member-crate API не менялся.

Оркестратор независимо запустил три разрешённых тестовых набора:
`oxx_r2_02_realloc_lazy_commit_frontier` — 1 passed, 1 intentional ignored;
четыре production realloc targets — 14 passed; три non-lazy targets — 4
passed. В последнем наборе `regression_inplace_large_realloc` компилируется с
0 тестов из-за отсутствия требуемого `internals`; это не считается покрытием
этого сценария. Единственный revert-control под lazy cfg заменил frontier на
`SEGMENT`: child завершился Windows status `0xc0000005`, родитель отверг
результат сообщением `child must exit cleanly`. Восстановленный тест снова
прошёл 1/1. Targeted rustfmt, `git diff --check`, `cargo check --lib` и строгий
Clippy на `production internals` прошли.

Это **не закрывает CQ-12 целиком**: другие упомянутые review suppressions не
аудировались здесь. CQ-10, CQ-11 и CQ-13 остаются без изменений; ни
рефакторинг дублированных safety-последовательностей, ни измерения speed/code
size не заявлены. Остальные гипотезы приложения A–C не реализованы.

## R18 follow-up — restore the realloc committed-span rejection

The R18 dead-shape cleanup left `try_promote_to_large`'s committed-span `if`
empty, removing its intended `return None`. Strict Clippy caught the empty
conditional. Restored the rejection and added a Windows-lazy
`production medium-classes` subprocess regression: a 512 KiB false old layout
on a 16-byte allocation, growing to 600 KiB, must return null before promotion
copies from the uncommitted tail. The restored test passes; replacing the
return with an empty conditional makes the child terminate with
`0xc0000005`, and the parent test fails.
