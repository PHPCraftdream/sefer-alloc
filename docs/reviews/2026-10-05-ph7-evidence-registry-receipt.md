# Ph7 — реестр доказательств (живой контракт): отчёт приёмки

Дата: 2026-10-06. Задача #2098, шаг 7 фундаментального плана. Реестр, судья, самотест и генератор — **только tooling/доки, `src/` не менялся**.

## 1. Что поставлено

| Артефакт | Назначение |
|---|---|
| `docs/evidence/registry.csv` | источник истины: 179 строк, 21 колонка, стабильные id, словарь статусов |
| `docs/evidence/required_cells.csv` | 148 обязательных клеток (надмножество выводимого из реестра минимума) |
| `docs/evidence/retired_ids.csv` | списанные id (удаление id без записи здесь — RED при ratchet) |
| `docs/evidence/REGISTRY.md` | сгенерирован `scripts/generate-evidence-registry-md.mjs` (`--check` в CI) |
| `scripts/verify-evidence-registry.mjs` | судья: RED (exit 1) / INCOMPLETE (exit 0, явная печать) / GREEN |
| `scripts/evidence-registry-selftest.mjs` | 98 мутантов/контролей: каждое правило судьи краснит по СВОЕМУ маркеру |
| `scripts/pin-evidence-receipts.mjs` | пины receipt_sha256 (содержимое с CRLF→LF) |
| проводка | три хвостовых шага `scripts/check-all.mjs` и отдельный job `evidence-registry` в `ci.yml` (full history, без Rust) |

Текущий вердикт судьи (`--strict`): **INCOMPLETE** (exit 0). Строки по статусам: PASS=102, FAIL=3, KNOWN-DEFECT=6, KNOWN-RED=4, MODEL-LIMIT=4, NOT_RUN=16, CFG_EXCLUDED=1, INCONCLUSIVE=8, CI_PENDING=3, CONDITIONAL=32. INCOMPLETE честен: ждут финального статуса клетки CONDITIONAL/INCONCLUSIVE/NOT_RUN/CI_PENDING (перечень — в выводе судьи и в `REGISTRY.md`).

## 2. Как судья отвергает автоповышения

Каждое правило имеет мутант в самотесте, красный по своему маркеру (таблица выводится скриптом из исходника самотеста):

| Правило (маркер `RED: …`) | Кейсы самотеста |
|---|---|
| `(control: GREEN)` | `green-control`, `green-control-strict`, `green-control-crlf`, `shadow-negation-control`, `mutant-waiver-ok-control`, `fail-resolved-by-decision-control`, `counter-notrun-control`, `isolated-witness-negation-control`, `r2-retired-covers-removal`, `r4-promotion-with-new-receipt` |
| `(INCOMPLETE, exit 0)` | `required-missing-plain`, `identity-unpublished-plain`, `identity-empty-sha-plain`, `r5-baseline-unresolved-skipped` |
| `activation-generic` | `activation-generic-cargo`, `activation-generic-ru` |
| `activation-trivial` | `activation-trivial-dash`, `activation-trivial-short`, `R0-04-activation-dash`, `R0-04-activation-na` |
| `baseline-id-removed` | `r1-baseline-id-removed` |
| `baseline-promotion-without-evidence` | `r3-promotion-without-evidence`, `r7-promotion-marker-only`, `r8-promotion-short-sha-and-whitespace`, `r9-promotion-receipt-token-order` |
| `baseline-required-widened` | `r10-required-widened-without-evidence` |
| `baseline-unresolved` | `r6-baseline-unresolved-strict` |
| `bool-mutant-rejected` | `bool-mutant-rejected` |
| `counter-not-rss` | `counter-not-rss` |
| `csv-column-count` | `csv-column-count`, `R4a-03-extra-column` |
| `csv-unterminated-quote` | `csv-unterminated-quote`, `registry-unterminated-quote` |
| `dup-id` | `dup-id` |
| `evidence-identity-base` | `evidence-identity-base` |
| `evidence-stale` | `evidence-stale` |
| `evidence-test-missing` | `evidence-test-missing`, `strong-row-cites-missing-test` |
| `fail-justified` | `fail-unjustified` |
| `id-format` | `id-format-trailing-space`, `id-format-inner-space`, `R4a-03-id-trailing-space` |
| `identity-empty-sha` | `identity-empty-sha-strict` |
| `identity-shallow` | `identity-shallow-strict` |
| `identity-unpublished` | `identity-unpublished-strict`, `identity-spike-not-ancestor-strict`, `R0-04-unpublished-sha`, `spike-sha-must-be-allowlisted` |
| `isolated-witness-not-sefer` | `isolated-witness-not-sefer` |
| `known-defect-signature` | `known-defect-signature-none`, `known-defect-signature-no-arrow`, `known-defect-signature-missing-file` |
| `miri-model-mismatch` | `miri-model-missing`, `miri-flag-mismatch`, `miri-sb-plus-tb`, `R0-04-miri-dev`, `miri-retag-sb-to-tb`, `miri-retag-tb-to-sb` |
| `mutant-rejected-consistency` | `mutant-empty-mr-true` |
| `mutant-waiver-undocumented` | `mutant-waiver-undocumented` |
| `pass-cfg-unsatisfied` | `pass-cfg-unsatisfied`, `R5b-20-features-production` |
| `pass-forbidden-marker` | `pass-forbidden-marker`, `red-row-promoted-to-pass-ub-marker` |
| `pass-marker-cfg-empty` | `green-by-cfg-exclusion`, `green-by-cfg-exclusion-ru` |
| `pass-requires-activation` | `no-activation` |
| `pass-requires-marker` | `no-positive-marker` |
| `receipt-exists` | `receipt-exists-nope` |
| `receipt-external-strong` | `receipt-external-strong`, `strong-row-external-receipt` |
| `receipt-pin-required` | `receipt-pin-required` |
| `receipt-roots` | `receipt-roots-readme`, `receipt-roots-dotdot`, `R6a-A1-receipt-readme` |
| `receipt-sha` | `receipt-sha` |
| `required-bool` | `required-bool` |
| `required-cell-mutant` | `required-cell-mutant-empty` |
| `required-cell-status` | `R6a-C4-promoted-to-pass`, `p1-box-cell-never-accepts-pass` |
| `required-dup-cell` | `required-dup-cell` |
| `required-expects` | `required-expects-green`, `required-expects-empty` |
| `required-gate` | `required-gate` |
| `required-missing` | `required-missing-strict` |
| `required-not-superset` | `required-not-superset`, `required-header-only` |
| `required-schema` | `required-schema` |
| `requires-mutant-rejected` | `mutant-rejected-false`, `mutant-survived-token`, `R6a-M1-mr-false` |
| `schema-columns` | `schema-columns` |
| `scope-vocabulary` | `unknown-scope` |
| `shadow-loom-not-shipping` | `shadow-loom-not-shipping`, `R0-21-scope-to-shipping`, `R6a-D-entry-installed` |
| `shipping-installed-needs-global-allocator` | `shipping-installed-needs-global-allocator` |
| `status-dictionary` | `unknown-status` |

Режим `--strict` (его зовут `check-all` и CI): отсутствующая обязательная клетка, PASS/KNOWN-* без верифицируемого предка-`source_sha`, shallow-клон, пустой `source_sha` у PASS — RED. Без флага остаётся INCOMPLETE/exit 0 (ручные прогоны). `--baseline <ref>`: ratchet — id не исчезают (кроме `retired_ids.csv`), повышение до PASS требует изменения receipt/source_sha/source_tree/activation/positive_marker.

## 3. Как добавить строку или клетку

1. Строка: `docs/evidence/registry.csv`, новый стабильный id `R<фаза>-<n>`; для PASS обязательны конкретные `activation` и `positive_marker` (не «cargo test»), `source_sha` — коммит, на котором реально снято (предок HEAD), `receipt` — файл под `docs/|tests/|scripts/|benches/|examples/`; Miri — строго одна модель SB или TB в `profile`; `scope` по фактической точке входа.
2. Пины: `node scripts/pin-evidence-receipts.mjs`; MD: `node scripts/generate-evidence-registry-md.mjs`.
3. Обязательная клетка: строка в `required_cells.csv` (`cell_id,class,gate,expects,requires_mutant,note`); для классов protocol/ordering/oom/free при `requires_mutant=false` — `note` начинается с `waiver:` и причины. Всё не-PASS и все мутант-строки судья требует в required сам.
4. Проверка: `node scripts/verify-evidence-registry.mjs --strict`, `node scripts/evidence-registry-selftest.mjs`.

## 4. Изменения после независимого ревью (rush-ревьювер + @ox + прогоны мутантов оркестратора)

Дыры первого варианта, найденные ревью и закрытые: Loom-строка со scope shipping проходила; bogus/чужой `source_sha` давал INCOMPLETE (= базовое состояние, CI не краснел), в shallow-checkout ancestry не проверялась; пустой `required_cells.csv` давал GREEN; `activation=-`/`n/a` проходили PASS; агрегирующие Miri-строки без токена модели обходили SB≠TB; `receipt_sha256` не задан нигде, `receipt=README.md` принимался; незакрытая кавычка поглощала хвост реестра; PASS-строки, чей тест выкомпилирован под заявленными features (`pass-cfg-unsatisfied`), и 10 строк shipping-installed без установленного аллокатора; тесты, переписанные после замера (`evidence-stale`); хеш в генераторе зависел от CRLF. Оркестратор дополнительно: трёхзначная логика cfg (`not(windows)` — неизвестно, а не false), русские формулировки cfg-empty, восстановление трёх оригинальных мутантов самотеста, потерянных при переписывании (cfg-exclusion, no-activation, unknown-status) и покрытие остальных правил без кейса, возвращён случайно удалённый комментарий в `check-all.mjs`.

### 4.1 Раунд 1: изменения статусов относительно первого варианта (выведено скриптом)

Всего: понижений PASS → не-PASS **13**, повышений до PASS **1** (единственное — R6b-03, новое evidence Ph6b), re-scope при том же статусе **10**, новых id **15**. Понижения перепроверит оркестратор на HEAD (тесты переписаны после измеренного коммита); они не «исправлены», а честно помечены.

| id | было | стало | причина |
|---|---|---|---|
| R0-25 | NOT_RUN | CONDITIONAL | смена: агрегат: iai и RSS PASS (R6b-05, R6b-03), wall-clock INCONCLUSIVE/CONDITIONAL (R6b-01, R6b-07), p99/System-вызовы/page faults NOT_RUN (R6b-02/04/06);  |
| R-Ph1b | CONDITIONAL | разделена/заменена (см. NEW) | split по моделям SB/TB/native |
| R-PG1 | PASS | разделена/заменена (см. NEW) | split по моделям SB/TB/native |
| R-PG2 | CONDITIONAL | разделена/заменена (см. NEW) | split по моделям SB/TB/native |
| R-Ph3a | PASS/shipping-installed | PASS/crate-internal | re-scope: тест не ставит Sefer как #[global_allocator] |
| R4a-01 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(miri-log): pass 1 (-Zmiri-strict-provenance) в docs/perf/_raw_ph4a_miri_lease.log пуст — прогон не подтверждён цитируемым логом; переприём |
| R4a-10 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(rewrite): пин r11_ph4a_drop_ordering_pinned.rs переписан в 4e8eec6b (CRLF-safe + inner-bind); семантика Release-CAS сохранена; перепроверк |
| R4b-06 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(rewrite): пины/child-раннер переписаны в 4e8eec6b; перепроверка на HEAD оркестратором |
| R4b-07 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(migration): тест мигрирован на lease-API в 5083e4bc (Ph4c); перепроверка на HEAD оркестратором |
| R4b-09 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(rewrite): пины/child-раннер переписаны в 4e8eec6b; перепроверка на HEAD оркестратором |
| R4b-11 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(rewrite): пины/child-раннер переписаны в 4e8eec6b; перепроверка на HEAD оркестратором |
| R4b-12 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(rewrite): пины/child-раннер переписаны в 4e8eec6b; перепроверка на HEAD оркестратором |
| R4c-01 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(oracle-rewrite): ingress-оракул переписан в fd954856 (bounded pre-drain) и 39e86338 (окно 16 ≤ delta < 32 вместо ==16); мутанты №4/№12/№13 |
| R4c-12 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(oracle-rewrite): ingress-оракул переписан в fd954856 (bounded pre-drain) и 39e86338 (окно 16 ≤ delta < 32 вместо ==16); мутанты №4/№12/№13 |
| R4c-20 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(oracle-rewrite): ingress-оракул переписан в fd954856 (bounded pre-drain) и 39e86338 (окно 16 ≤ delta < 32 вместо ==16); мутанты №4/№12/№13 |
| R4c-21 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(oracle-rewrite): ingress-оракул переписан в fd954856 (bounded pre-drain) и 39e86338 (окно 16 ≤ delta < 32 вместо ==16); мутанты №4/№12/№13 |
| R5b-01 | PASS/shipping-installed | PASS/low-level-api | re-scope: тест не ставит Sefer как #[global_allocator] |
| R5b-02 | PASS/shipping-installed | PASS/low-level-api | re-scope: тест не ставит Sefer как #[global_allocator] |
| R5b-03 | PASS/shipping-installed | PASS/low-level-api | re-scope: тест не ставит Sefer как #[global_allocator] |
| R5b-09 | MODEL-LIMIT | разделена/заменена (см. NEW) | split по моделям SB/TB/native |
| R5b-11 | PASS/shipping-installed | PASS/low-level-api | re-scope: тест не ставит Sefer как #[global_allocator] |
| R5b-16 | PASS/shipping-installed | PASS/low-level-api | re-scope: тест не ставит Sefer как #[global_allocator] |
| R5b-17 | PASS/shipping-installed | PASS/low-level-api | re-scope: тест не ставит Sefer как #[global_allocator] |
| R5b-18 | PASS/shipping-installed | PASS/crate-internal | re-scope: тест не ставит Sefer как #[global_allocator] |
| R5b-20 | PASS/shipping-installed | PASS/crate-internal | re-scope: тест не ставит Sefer как #[global_allocator] |
| R5b-21 | PASS/shipping-installed | PASS/crate-internal | re-scope: тест не ставит Sefer как #[global_allocator] |
| R5c-01 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(cfg-gate): r11_ph5c_registry_saturation_fallback.rs получил cfg-гейт D1 в fd954856; поведение на измеренных наборах то же, байты отличаютс |
| R5c-11 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(cfg-gate): r11_ph5c_registry_saturation_fallback.rs получил cfg-гейт D1 в fd954856; поведение на измеренных наборах то же, байты отличаютс |
| R6a-EX3 | KNOWN-RED | CONDITIONAL | смена: CONDITIONAL(determinization): тест r6_fix_p3_numa_unknown_bucket.rs переписан в e0f9dd9f (детерминизация); краснота была дефектом теста; перепроверка  |
| R6a-EX4 | KNOWN-RED | CONDITIONAL | смена: CONDITIONAL(determinization): тест segment_directory_numa_high_node_ids.rs переписан в 84ddf8f3 (детерминизация); краснота была дефектом теста; перепр |
| R6b-01 | NOT_RUN | CONDITIONAL | смена: wall-clock WSL2: A/A-разброс до 36%; per-id max churn/1024B INCONCLUSIVE, bootstrap-CI включает предел 1.10; условный GO по docs/perf/PH6B_COST_AB.md  |
| R6b-03 | NOT_RUN | PASS | ПОВЫШЕНИЕ (новое evidence): RSS/commit/фрагментация; page faults не измерялись — R6b-06 |

Новые id: R-PG1-native, R-PG1-sb, R-PG1-tb, R-PG1-b-sb, R-PG1-b-tb, R-Ph1b-native, R-Ph1b-sb, R-Ph1b-tb, R-PG2-sb, R-PG2-tb, R5b-09-sb, R5b-09-tb, R6b-05, R6b-07, R6b-06.


### 4.2 Раунд 2 (повторное ревью rush-ревьювера и @ox): что закрыто

Перемаркировка SB<->TB ловится по подсказкам модели во ВСЕХ полях строки (не только по флагам Miri); красные маркеры Miri (`not granting access`, `weakly protected`, `forbidden`, `UB`, exit != 0) запрещены в PASS; клетки P1-box не принимают PASS; тесты резолвятся из `tests/<x>.rs`, `tests::<x>`, `--test <x>` и голых идентификаторов, причём цитируемый несуществующий тест у PASS/KNOWN-* — RED; cfg- и installed-allocator-проверки действуют для ВСЕХ строк-вердиктов, cfg считается по всем внутренним `#![cfg]`; не-strong вердикт с неразрешимым `source_sha` допустим только для коммитов из `docs/evidence/spike_commits.csv` и с `spike-not-merged` в причине; внешний receipt не подтверждает PASS/KNOWN-*; ratchet нормализует поля (полный sha, множество receipt, пины) — короткий sha, пробелы и перестановка токенов не считаются новым evidence, расширение `expects` до PASS без нового evidence — RED (`baseline-required-widened`); baseline-реестр валидируется (кавычки, ширина, дубли); подпись KNOWN-DEFECT — путь в `src|crates|tests|benches|examples|scripts|.github`; PASS-маркер короче 12 символов — RED; пины receipt-ов Ph0 дополнены реальным Ph0-receipt.

Данные: понижены до CONDITIONAL строки, чьи тесты менялись после измеренного коммита, но судья раньше их не видел (R0-06, R0-16, R0-17, R0-19, R0-20, R4a-08, R6a-A10, R-Ph3a (source_sha возвращён к faf253e9), R-Ph3b); цитаты features/scope строк R4b-10, R4c-05..08, R5b-04, R5b-15 исправлены по cfg-гейту/факту теста (features восстановлены из cfg, исходный набор в источнике не записан — оговорено в причинах); R6a-EX3/EX4 больше не несут маркер красноты прежней версии теста; маркер R6b-05 приведён к числам PH6B_COST_AB.md.

### Изменения статусов раунда 2 (выведено скриптом, сравнение с коммитом предыдущего раунда)

Всего: понижений PASS → не-PASS **9**, повышений до PASS **0**, re-scope при том же статусе **2**, новых id **0**. Понижения перепроверит оркестратор на HEAD (тесты переписаны после измеренного коммита); они не «исправлены», а честно помечены.

| id | было | стало | причина |
|---|---|---|---|
| R0-06 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(stale): tests/r8_global_box_provenance.rs изменён после ceffecb5 (95ab549f: cfg out paused-ноги); прежний вердикт PASS не подтверждён на H |
| R0-16 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(stale): tests/r6_terminal_owner_drain.rs и tests/r8_owner_sidecar_miss.rs изменены после ceffecb5 (5083e4bc, fa64f7dd); прежний вердикт PA |
| R0-17 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(stale): те же три теста изменены после ceffecb5 (5083e4bc, fa64f7dd); прежний вердикт PASS не подтверждён на HEAD, перепроверка прогоном;  |
| R0-19 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(stale): tests/r6_terminal_owner_drain.rs изменён после ceffecb5; прежний вердикт PASS не подтверждён на HEAD, перепроверка прогоном; было: |
| R0-20 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(stale): tests/r6_terminal_owner_drain.rs изменён после ceffecb5; прежний вердикт PASS не подтверждён на HEAD, перепроверка прогоном; было: |
| R-Ph3a | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(stale): тесты tests/r11_ph3a_*.rs изменены после faf253e9 (5083e4bc: миграция на lease-API); source_sha возвращён к реально измеренному ко |
| R-Ph3b | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(stale): tests/r11_ph3b_kind_aware_entries.rs изменён после fa88c95f (5083e4bc); тест — изолированная грань HeapCore, не #[global_allocator |
| R4a-08 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(stale): tests/regression_claim_oom_initialised_gate.rs изменён после 0c8d198a (5083e4bc); мутант M4 пойман на прежней версии теста; прежни |
| R5b-04 | INCONCLUSIVE/shipping-installed | INCONCLUSIVE/low-level-api | re-scope: тест не ставит Sefer как #[global_allocator] |
| R5b-15 | CONDITIONAL/shipping-installed | CONDITIONAL/low-level-api | re-scope: тест не ставит Sefer как #[global_allocator] |
| R6a-A10 | PASS | CONDITIONAL | ПОНИЖЕНИЕ: CONDITIONAL(stale): tests/r6_fix_p3_numa_unknown_bucket.rs и tests/segment_directory_numa_high_node_ids.rs переписаны после fd954856 (e0f9dd9f, 84ddf8 |

Новые id: .

## 5. Ph6b-строки

R6b-01..07 и R0-25 заполнены по `docs/perf/PH6B_COST_AB.md`: iai (R6b-05) и RSS (R6b-03) — PASS; wall-clock bench-table (R6b-01) — CONDITIONAL; MT (R6b-07) — INCONCLUSIVE; p99, System-вызовы, page faults (R6b-02/04/06) — NOT_RUN. Ссылка на открытый вопрос — `docs/perf/OPEN_ITEMS.md` item 81.

## 6. Что судья НЕ может (честно)

- Прозовые проверки (`counter-not-rss`, `isolated-witness-not-sefer`, негативные маркеры) эвристичны: ключевые слова и окна отрицаний, не семантика.
- Логи Miri/Loom/iai судья не пересчитывает: он проверяет pin receipt'а, идентичность коммита и неизменность цитируемых test-файлов, но не воспроизводит прогон.
- Идентичность покрывает test-файлы из `entry`; изменение `src/` после `source_sha` судья не видит (поэтому понижения Ph4/Ph5c перепроверяются прогоном на HEAD).
- Исторические логи Ph4c/5b/5c/6a лежат вне репозитория; в receipt цитируются закоммиченные доки.

## 7. Что осталось оркестратору

Полный `npm run check`, пуш и CI на landing SHA (#2106); перепроверка на HEAD клеток, понижённых до CONDITIONAL; заполнение R6a-G/G2/R5b-22 фактическими вердиктами CI (затем `pin-evidence-receipts`).
