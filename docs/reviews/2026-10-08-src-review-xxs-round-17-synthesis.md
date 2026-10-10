# Синтез девяти тематических ревью src/ раунда 17 — текущее дерево (xxs)

## Ревизия, область проверки и ограничения

- **Историческая база обзоров:** `453439c123f71a98890408dd0170bd909ad9ecdf`.
- **Целевое текущее состояние:** `ff3d60bb571f706cd8f034665fac8c3f4b6f0d23`. Чтение `.git/HEAD` и `.git/refs/heads/main` подтвердило main-reference checkout. Ревизии различны: диспозиции установлены по текущим файлам, не перенесены со старых номеров строк. Названия docs-коммитов в reflog не использованы вместо source inspection или byte-identity diff.
- **Входы:** ровно девять файлов ниже. Все прочитаны полностью: detailed findings, evidence, hypotheses, appendices; усечённые/пропущенные участки перечитывались узкими диапазонами. Затем независимо прочитаны/найдены текущие source, tests, manifests, CI, docs и относящиеся к находкам owner-карточки. Цитируемые исходными обзорами другие документы — контекст/evidence, не дополнительные specialist inputs.
- **Первичный review-pass был строго статическим** на target `ff3d60bb`: не запускались build/compiler/tests/lint/formatter/benchmark/scripts/CI/runtime probes; Rust-analyzer не запускался. Source/tests/CI/indexes/changelog/context files не менялись. Это исторический receipt первичного аудита, не утверждение о последующем R18 remediation; его результаты зафиксированы ниже.
- На момент первичного обзора старые raw perf logs читались как **исторические артефакты**; нужный oracle не исполнялся. CI не запрашивался. R18 follow-up с новыми execution receipts описан в отдельном addendum и не переписывает первоначальную статическую методику.
- Это не новый full-repo audit, не release GO/NO-GO, не доказательство отсутствия UB или зелёного CI, не promotion оптимизации. Absence search доказывает definition/wiring scope, не что путь никогда не исполнялся вручную.

### Все девять входов и исходные оценки

| Исходный обзор | Полностью охваченные ID | Исходные ratings |
|---|---|---|
| [Конкурентность/атомики](2026-10-08-src-review-oxx-round-17-concurrency-atomics.md) | R17-CON-01…06 | 1 условный P2; 5 P4 |
| [Производительность](2026-10-08-src-review-oxx-round-17-performance.md) | R17-PRF-01…06; R17-OPT-01…13 сохранены отдельно | 1 P2; 3 P3; 2 P4; OPT без P |
| [Верификационное покрытие](2026-10-08-src-review-oxx-round-17-verification-coverage.md) | R17-VER-01…11, ownership evidence §2.2 | 11 P4 |
| [Качество кода](2026-10-08-src-review-oxx-round-17-code-quality.md) | R17-CQ-01…13, дополнительные evidence/hypotheses | 1 P3; 8 P4; 4 P5 |
| [Unsafe/provenance](2026-10-08-src-review-oxx-round-17-unsafe-provenance.md) | R17-UNS-01…05 | 1 P1; 4 P4 |
| [API/features/portability](2026-10-08-src-review-oxx-round-17-api-features-portability.md) | R17-API-01…09 | 1 P3; 8 P4 |
| [Безопасность](2026-10-08-src-review-oxx-round-17-security.md) | R17-SEC-01…05 | 2 P3; 3 P4 |
| [Логика/инварианты](2026-10-08-src-review-oxx-round-17-logic-invariants.md) | R17-LOG-01…03 | 2 P3; 1 P4 |
| [Panic/lifecycle](2026-10-08-src-review-oxx-round-17-panic-lifecycle.md) | R17-LIF-01…04 | 1 условный P2; 3 P4 |
| **Formal source findings, всего** | **62 ID** | **0 P0; 1 P1; 3 P2 (2 условных); 9 P3; 45 P4; 4 P5** |

Часть исходных текстов называет раунд восемью темами; задание перечисляет **девять**, охвачены все девять. Неоценённые предложения сохранены отдельно от P-дефектов; подробности и остальные локальные гипотезы приведены в приложениях A–C отчёта P5.

## Проверенные приоритетные группы и количества

Считаются **канонические вопросы**, не повторные наблюдения разных специалистов. `R17-API-06` явно разделён: abort-contract paragraphs и counter-overflow implementation — разные проблемы. Из 62 ID получается 63 компонента; 13 duplicate copies объединены → **50 канонических вопросов**. Partial canonical issue может включать confirmed и narrowed компоненты; original-ID ledger ниже сохраняет различие.

| Текущая группа | Канонических вопросов | CONFIRMED CURRENT | PARTIALLY VALID/NARROWED | NOT VERIFIABLE FROM STATIC SOURCE | Отчёт |
|---|---:|---:|---:|---:|---|
| P0 | 0 | 0 | 0 | 0 | Пустой файл не создавался |
| P1 | 1 | 1 | 0 | 0 | [P1](2026-10-08-src-review-xxs-round-17-P1.md) |
| P2 | 1 | 0 | 1 | 0 | [P2](2026-10-08-src-review-xxs-round-17-P2.md) |
| P3 | 10 | 6 | 4 | 0 | [P3](2026-10-08-src-review-xxs-round-17-P3.md) |
| P4 | 34 | 16 | 15 | 3 | [P4](2026-10-08-src-review-xxs-round-17-P4.md) |
| P5 | 4 | 1 | 3 | 0 | [P5](2026-10-08-src-review-xxs-round-17-P5.md) |
| **Всего** | **50** | **24** | **23** | **3** | |

Ни одна целая source finding не установлена как **RESOLVED SINCE REVIEW BASE** или полностью **REFUTED**: оба количества **0**. Есть refuted/narrowed supporting subclaims, не отменяющие основную находку. На уровне **62 исходных ID**: **32 CONFIRMED CURRENT**, **26 PARTIALLY VALID/NARROWED**, **4 NOT VERIFIABLE FROM STATIC SOURCE**. Два Windows ID образуют один canonical uncertain issue, отсюда 4 против 3 uncertain counts.

**Значение диспозиций:**

- **CONFIRMED CURRENT:** конкретный path/contradiction/definition/wiring fact есть в текущем source; не означает выполненный здесь oracle.
- **PARTIALLY VALID/NARROWED:** source поддерживает основу/часть, но не весь масштаб, coverage scope, census, model outcome, attribution или универсальную формулировку.
- **NOT VERIFIABLE FROM STATIC SOURCE:** решающее std/OS/emulator/runtime поведение не устанавливается inspected repo source. Внутренняя предпосылка и нужный неисполненный oracle всё равно описаны.
- **RESOLVED SINCE REVIEW BASE / REFUTED:** только доказанное post-base устранение либо полностью неверная находка. Старые owner-ошибки, исправленные *до* review base, не называются post-base resolutions.

### Нормализация severity

1. Generation table сгруппирована в **P3 по исходной ресурсной severity CQ-01**; исходные **P4 CON-02/SEC-04** сохранены. Hardened-only footprint/writer work, не production speedup/memory-safety exploit.
2. **API-06(c): исходный P4 → предложенный P3**: conditional debug overflow внутри fallback alloc — implementation behavior, не только prose. Причина старых hanging tests не claimed reproduced. API-06(a,b) остаются P4 с общим abort-doc вопросом.
3. **CON-01/LIF-01:** сохранено **исходное P2 при подтверждении**, текущая triage-группа **P4 исследования** из-за неподтверждённой shutdown reachability. Это не confirmed P2 deadlocks.
4. PRF-01 сохраняет **P2 structural cost priority**; historical ratios 132–178×/growth time projections не current measurements. PRF-02…04 сохраняют P3 с явными numerical/attribution limits.
5. Прочие formal ratings сохранены. **Ни одному без-P предложению P-уровень не назначен.** OPT benefit/risk/cost — исходные ожидания, не speedup evidence.

## Без P-оценки

Performance-обзор кроме шести formal findings содержит **13 неизмеренных предложений**. Они сохранены все под исходными ID и без severity, которой источник не задавал; подробности/current premises/oracles находятся в приложении A отчёта [P5](2026-10-08-src-review-xxs-round-17-P5.md).

| Исходный ID | Предложение источника | P-оценка |
|---|---|---|
| R17-OPT-01 | Owner load перед empty-word swap | Отсутствует; не назначена |
| R17-OPT-02 | Summary-слово в независимом sidecar | Отсутствует; не назначена |
| R17-OPT-03 | Zero-credit skip / BinTable до drain | Отсутствует; не назначена |
| R17-OPT-04 | Устранение повторной инициализации SmallSidecar | Отсутствует; не назначена |
| R17-OPT-05 | Проверка route однажды на batch | Отсутствует; не назначена |
| R17-OPT-06 | Сохранение route Large в cache | Отсутствует; не назначена |
| R17-OPT-07 | Снижение foreign-free RMW/TTAS/padding | Отсутствует; не назначена |
| R17-OPT-08 | Альтернатива variable divisibility | Отсутствует; не назначена |
| R17-OPT-09 | Reverse physical PerClass stack | Отсутствует; не назначена |
| R17-OPT-10 | Maintenance budget по nonempty cuts | Отсутствует; не назначена |
| R17-OPT-11 | Не повторять Large sweep в одном realloc | Отсутствует; не назначена |
| R17-OPT-12 | Class-aligned leaves / carving по классам | Отсутствует; не назначена |
| R17-OPT-13 | Несколько registry reuse hints/probing | Отсутствует; не назначена |

Остальные без-P blocks — **CON H1…H7; CQ §2.15(1)…(3); API O-1/O-2; SEC H1…H3; LOG H1/H2; LIF H1…H4 и пятый maintainability block; UNS H1…H4** — сохранены с report-qualified labels в приложениях B–C отчёта P5. Их effects/future safety имеют **NOT VERIFIABLE FROM STATIC SOURCE**; source-confirmed docs части ссылаются на соответствующие formal issues. Пересечения не размножают finding totals.

## Карта дублей и сознательно раздельных вопросов

| Канонический вопрос / группа | Все исходные ID | Граница объединения |
|---|---|---|
| CQ-01, P3 | CQ-01 (P3); CON-02 (P4); SEC-04 (P4) | Одна write-only hardened gen table, не три дефекта |
| CON-01, P4 исследования | CON-01; LIF-01 (условные P2) | Одна Windows shutdown/abandoned-lock гипотеза |
| CON-06, P4 | CON-06; CQ-04 | Общая present-tense removed-protocol prose; специальные contract/counter/inventory issues отдельно |
| VER-06, P4 | VER-06; CQ-09; API-03; UNS-04 | Hidden test surface/scanner selector/неверные reviewed reasons, subparts сохранены |
| LIF-02, P4 | LIF-02; VER-07; CQ-07; API-06(a,b) | «One deliberate process kill»; scanner scope cross-link LIF-03 |
| LOG-03, P4 | LOG-03; VER-08; CQ-03 | Stale INVARIANTS/M2 pins вместе с M4/M5/M6/M7 противоречиями |
| CQ-08, P4 | CQ-08; SEC-05 | Diagnostic docs/oracle semantics, removed increment source/layout-mismatch promise |
| API-06(c), P3 | API-06(c) | **Разделение, не duplicate:** unchecked wait-counter arithmetic |

Canonical key — существующий finding ID, не новая придуманная находка. Частичные пересечения supplemental observations отсылают к этим entries без новых counts.

**Не объединены:** SEC-01 lower payload bound и CQ-02 cfg-dependent upper bump bound — разные guards/features. SEC-03 own Large interior free — не item166 foreign Small publication. PRF-02 own free duplicate lookup и PRF-06 known-base realloc probe/doc — разные paths. Accepted P1-box **164** отличен от safe diagnostic provenance **UNS-01**.

## Полный original-ID ledger диспозиций

`C` = CONFIRMED CURRENT; `P` = PARTIALLY VALID/NARROWED; `N` = NOT VERIFIABLE FROM STATIC SOURCE. Это сокращения точных допустимых диспозиций. Group reports содержат current paths/symbols/lines, owner, scope limits, severity reasoning и unrun oracles.

| Исходный ID | Исходная severity | Текущая диспозиция | Группа / canonical key |
|---|---|---|---|
| R17-CON-01 | P2 при подтверждении | N | P4 исследования / CON-01 |
| R17-CON-02 | P4 | C | P3 / CQ-01 |
| R17-CON-03 | P4 | C | P4 / CON-03: stale absence claim, не actual-type closure |
| R17-CON-04 | P4 | P | P4 / CON-04 |
| R17-CON-05 | P4 | C | P4 / CON-05 |
| R17-CON-06 | P4 | P | P4 / CON-06 |
| R17-PRF-01 | P2 | P | P2 / PRF-01 |
| R17-PRF-02 | P3 | P | P3 / PRF-02 |
| R17-PRF-03 | P3 | P | P3 / PRF-03 |
| R17-PRF-04 | P3 | P | P3 / PRF-04 |
| R17-PRF-05 | P4 | C | P4 / PRF-05 |
| R17-PRF-06 | P4 | C | P4 / PRF-06 |
| R17-VER-01 | P4 | C | P4 / VER-01 |
| R17-VER-02 | P4 | P | P4 / VER-02 |
| R17-VER-03 | P4 | P | P4 / VER-03 |
| R17-VER-04 | P4 | P | P4 / VER-04 |
| R17-VER-05 | P4 | C | P4 / VER-05 |
| R17-VER-06 | P4 | P | P4 / VER-06 |
| R17-VER-07 | P4 | C | P4 / LIF-02; scanner subpart LIF-03 |
| R17-VER-08 | P4 | C | P4 / LOG-03 |
| R17-VER-09 | P4 | P | P4 / VER-09 |
| R17-VER-10 | P4 | C | P4 / VER-10 |
| R17-VER-11 | P4 hypothesis | N | P4 / VER-11 |
| R17-CQ-01 | P3 | C | P3 / CQ-01 |
| R17-CQ-02 | P4 | C | P4 / CQ-02 |
| R17-CQ-03 | P4 | P | P4 / LOG-03 |
| R17-CQ-04 | P4 | P | P4 / CON-06 |
| R17-CQ-05 | P4 | C | P4 / CQ-05 |
| R17-CQ-06 | P4 | C | P4 / CQ-06 |
| R17-CQ-07 | P4 | C | P4 / LIF-02 |
| R17-CQ-08 | P4 | C | P4 / CQ-08 |
| R17-CQ-09 | P4 | P | P4 / VER-06 |
| R17-CQ-10 | P5 | C | P5 / CQ-10 |
| R17-CQ-11 | P5 | P | P5 / CQ-11 |
| R17-CQ-12 | P5 | P | P5 / CQ-12 |
| R17-CQ-13 | P5 | P | P5 / CQ-13 |
| R17-UNS-01 | P1 | C | P1 / UNS-01 |
| R17-UNS-02 | P4 | C | P4 / UNS-02 |
| R17-UNS-03 | P4 | P | P4 / UNS-03 |
| R17-UNS-04 | P4 | C | P4 / VER-06 |
| R17-UNS-05 | P4 | C | P4 / UNS-05 |
| R17-API-01 | P3 | C | P3 / API-01 |
| R17-API-02 | P4 | P | P4 / API-02 |
| R17-API-03 | P4 | C | P4 / VER-06 |
| R17-API-04 | P4; (c) hypothesis | P | P4 / API-04 |
| R17-API-05 | P4 | P | P4 / API-05 |
| R17-API-06 | P4 | P в целом; (a,b) contradiction и (c) arithmetic по source подтверждены | P4 / LIF-02 и P3 / API-06(c) |
| R17-API-07 | P4; target examples hypothetical | P | P4 / API-07 |
| R17-API-08 | P4 hypothesis | N | P4 / API-08 |
| R17-API-09 | P4 | P | P4 / API-09 |
| R17-SEC-01 | P3 | C | P3 / SEC-01; perf84 attribution subclaim опровергнут |
| R17-SEC-02 | P3 | P | P3 / SEC-02 |
| R17-SEC-03 | P4 | C | P4 / SEC-03, documented hardening residual |
| R17-SEC-04 | P4 | C | P3 / CQ-01 |
| R17-SEC-05 | P4 | C | P4 / CQ-08 |
| R17-LOG-01 | P3 | C | P3 / LOG-01 |
| R17-LOG-02 | P3 | C | P3 / LOG-02 |
| R17-LOG-03 | P4 | C | P4 / LOG-03 |
| R17-LIF-01 | P2 при подтверждении | N | P4 исследования / CON-01 |
| R17-LIF-02 | P4 | C | P4 / LIF-02 |
| R17-LIF-03 | P4 | C | P4 / LIF-03 |
| R17-LIF-04 | P4 | P | P4 / LIF-04 |

### Существенные независимые сужения

- **Epoch shadow ≠ actual implementation:** QueueProtocol существует/подключён, но не direct model checking EpochRegion. Полное закрытие item160 не заявлено.
- **Old perf ≠ current cost:** raw Ir перепроверены чтением старых файлов; causal attribution, clean production overhead, current wall-clock/RSS и growth seconds не измерены. Extra Large bench cycle включает free; route physical free может ждать last pin.
- **Proof scope ≠ «proofs нет»:** Node harnesses реально вызывают production-used primitives. Обоснован gap current protocol/caller proofs/stale ring summaries, не бесполезность всего Kani.
- **Wiring ≠ «никогда не исполнялось»:** unselected Miri targets имеют historical manual evidence; missing dedicated TSan fallback targets не доказывают ноль incidental coverage. Нынешний CI не запускался/не queried.
- **Misuse ≠ safe soundness:** SEC-01/02/03, CQ-02/CQ-03 требуют invalid free/UAF либо говорят о defence-in-depth. Не повышены до production-safe-code UB. UNS-01 отличается safe public metadata read с arbitrary provenance.
- **Counter bug ≠ docs/census:** pre-register hit increment — LOG-02 implementation defect; stale counter descriptions — CQ-08/SEC-05 documentation. Abort inventories сохранены **CQ-07 106/25 comment-filtered source-wide; API-06 99/23 GlobalAlloc-file subset без per-site reachability; VER-07 110/28 source-wide token census**. Различные totals не склеены в новый aggregate.
- **CoreId/pub-count blanket assertions сужены:** inference разрешает некоторые callers без прямой core_affinity dependency; pub-token counts не доказывают нарушения sanctioned responsibility exceptions.
- **Точная decay ссылка:** `AllocCore::run_decay_step` расположен в `src/alloc_core/large/alloc_core_large_cache.rs:626–639`. Timer debt — source-derived policy behavior, не свежий замер cache loss.

## Существующие owners и неизменённые acceptance boundaries

Точные owners отделены от связанных карточек; близкая карточка не объявляется владельцем другого дефекта.

- Correctness **154** — source prose/allow/inventory debt; **160** — epoch-model card; **167** — verification-only stack mismatch; retained **18** — obsolete Kani closure; **17** — proof/wiring/fallback residuals.
- **107** — uncovered NUMA-only combination со stale шестью lints; **161** — warnings; **95** — library-oriented powerset decision. Source correction старого warning не passed clippy/check.
- Perf **82** — routed-negative discovery; **70** уже владеет terminal-sidecar foreign-publish contention/cost. Owner-scan PRF-01 — уточнение общей cutover-cost темы с отдельной owner-side осью, не полностью untracked issue. **78(a,b,c)** — service/storage/Large-scan hypotheses; **42** — sparse decay; **67** — inlining/code size; **66** — fallback calibration. Closed **80** не значит empty-word skipping implemented; closed **56** — instrumentation history, не атрибуция нынешнего free.
- Hook **5/7/8/9** — class owners; конкретные старые fixes не доказывают весь hidden surface/scanner. Correctness **78.18** — closed taxonomy record, **не** fresh-first resource gate.
- **162/163** сохраняют triggers. **164** остаётся accepted known P1-box, **166** — foreign Small interior residual, **171** — experimental Miri investigation. Они не regraded/fixed/учтены повторно. **152** — POSIX fork, не доказательство Windows exit. Closed **174/175/176** сохраняют scoped closure trails.

- **На момент первичного статического синтеза** index cards не создавались/не закрывались/не tier-перемещались; это не описывает последующие R18 изменения индексов и их статусы, перечисленные ниже.

## R18 remediation and execution follow-up (2026-10-10)

Предыдущие disposition counts, source paths и severity normalization — audit snapshot исходного раунда 17 на `ff3d60bb`; они не пересчитаны и не выдаются за сегодняшнее состояние. Этот addendum документирует последующую работу раунда 18 и сохраняет открытые acceptance boundaries.

| Группа | R18 disposition | Остаток |
|---|---|---|
| P1 | `R17-UNS-01` закрыта: безопасный decommit diagnostic читает только через canonical stored pointer; regression и strict-provenance Miri/revert receipt находятся в [P1](2026-10-08-src-review-xxs-round-17-P1.md). | Другие unsafe findings остаются в своих приоритетных группах; это не общее доказательство отсутствия UB. |
| P2 | `R17-PRF-01` измерена в dedicated R35 same-binary gate; [R35 report](../perf/R35_SIDE_CAR_SCAN_PREFILTER_GATE.md) и summary CSV содержат machine-derived sample data. Кандидат компилируется только с `bench-internals` и cfg `r18_sidecar_scan_bench`; ordinary production/IAI без cfg сохраняет исходный unconditional `AcqRel` swap без R18 TLS read. | Perf item 82 **OPEN**: нет native latency/coherence/producer measurement или actual-type protocol proof; promotion и speedup не заявляются. PRF-02/03/04 не менялись. |
| P3 | Подтверждённые P3 fixes/contract corrections и их receipts перечислены в [P3](2026-10-08-src-review-xxs-round-17-P3.md) и `CHANGELOG.md`. | Неподтверждённые гипотезы не переведены в defects; отдельные feature/CI coverage decisions остаются OPEN. |
| P4 | Unsafe-hook gates/scanners, API/docs contracts, Atomic64 diagnostic и ограниченные Loom-shadow правки отражены в [P4](2026-10-08-src-review-xxs-round-17-P4.md). Items 185–199 зарегистрированы по thematic owners. | Неисполненные platform/CI/model-refinement questions остаются OPEN; Loom shadows не переименованы в actual-type proof. |
| P5 | Части CQ-12 dead-shape surface удалены и описаны в [P5](2026-10-08-src-review-xxs-round-17-P5.md). | CQ-10/11/13 и неоценённые OPT предложения не объявлены закрытыми. |

Общие остатки: correctness item 154 остаётся широким prose/structure долгом; item 160 сохраняет shadow-vs-implementation gap. R18 evidence не закрывает эти карточки. Точные commands/results находятся в соответствующих P-level reports, в P2/R35 artifacts и в `CHANGELOG.md`; no CI landing-SHA claim is made here.
