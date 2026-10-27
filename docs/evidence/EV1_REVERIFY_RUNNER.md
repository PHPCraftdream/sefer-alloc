# EV-1 reverify runner: schema, crash evidence, and bounded evidence rows

## 1. Схема шага и receipt: до/после (раунд 3)

| Аспект | До (round 2, FAIL) | После (round 3) |
|---|---|---|
| Кодирование вывода шага | runner писал сырые stdout/stderr байты в общий fd и хранил `start/end`; verifier в `frameFor` искал только base64-фреймы `E0_STDOUT`/`E0_STDERR` — форматы писателя и читателя расходились | Единый формат: runner (scripts/r20_evidence_reverify_run.mjs:163-167) при завершении шага дописывает в receipt структурированные фреймы `E0_STDOUT <base64>` через общий экспортированный helper `encodeOutputFrames` (scripts/r20_evidence_reverify_support.mjs:276-285); verifier декодирует тем же парным `decodeOutputFrames` (scripts/r20_evidence_reverify_support.mjs:286-292) в `frameFor` (scripts/r20_evidence_reverify_support.mjs:383-389). Диапазон `start/end` шага расширен и покрывает фреймы |
| Отсутствие/порча диапазона шага | fallback строил «вывод» из `step.matches` — обход верификации | fallback удалён полностью: любой шаг без валидного числового `0 <= start <= end <= receipt.length` отклоняется с `step receipt range malformed` (scripts/r20_evidence_reverify_support.mjs:383-388) |
| Selftest-фикстуры | синтетический receipt `fixture receipt
` без фреймов и диапазонов | фикстуры строят receipt той же функцией `encodeOutputFrames` (`buildFixtureReceipt`); настоящий end-to-end в `--selftest` раннера: рецепт `E2E` (`node scripts/ev1_receipt_fixture.mjs`) исполняется реальным `runSteps`, его receipt проходит `verify`; тот же receipt с одним изменённым байтом payload фрейма и пересчитанными хешами отклоняется, причём не по хешу (проверяется текст ошибки) |
| Runtime-маркер R4c-16 | только Windows-форма `STATUS_ACCESS_VIOLATION|0xc0000005` | платформонезависимый: `process (?:did not|didn't) exit successfully[^\n]*(STATUS_ACCESS_VIOLATION|0xc0000005|signal: (?:11|7|6), SIG(SEGV|BUS|ABRT))` (docs/evidence/reverify-recipes/R4c-16.json:69), согласован с `crashReceipt` в `evaluateStep` (scripts/r20_evidence_reverify_support.mjs:118); агрегатный маркер мутанта тоже принимает Linux-формы (R4c-16.json:77) |
| Scratch/selftest-каталог | `target/e0-selftest` (создание `target/` запрещено в worktree) | уникальный `.rush/e0-selftest-<pid>-<time>`, удаляется в `finally` вокруг всех проверок (`selftest()` в scripts/r20_evidence_reverify_run.mjs); runner-selftest — `.rush/evidence-reverify-runner-selftest-<pid>-<time>` с `finally`. Вне worktree не пишется (правило worktree), поэтому не `os.tmpdir()` |
| Подключение фокус-тестов | не вызывались из раннера | `--selftest` раннера спавнит `node` на каждый файл (scripts/r20_evidence_reverify_run.mjs:597-609: support-selftest на строке 600, runner-selftest на строке 601; spawn+проверка статуса на 603-609) |
| Допуск пути мутанта | шаблон `src/**/*.rs`/`tests/loom_*.rs` проверял только `validateRecipe`; `mutantPreflight` принимал существующий `tests/other.rs` (тест засчитывал ENOENT как отказ) | общий `assertMutantPath` вызывается и в `validateRecipe`, и в начале `mutantPreflight`; тест создаёт все отклоняемые файлы, ENOENT больше не принимается |

## 2. Строки реестра

| Строка | Мутант | Статус реестра | Комментарий |
|---|---|---|---|
| R4a-05 | M1 Release->Relaxed в HeapLease::drop CAS (tests/loom_r11_ph4a_heap_lease.rs) | PASS | рецептура валидна (dry-run ок) |
| R4b-15 | L3 проигравший CAS дропает/drain'ит (tests/loom_r11_ph4b_publish_recycle_drain.rs) | PASS | рецептура валидна |
| R4c-09 | мутант №1 Release->Relaxed (tests/loom_r11_ph4a_heap_lease.rs) | PASS | рецептура валидна |
| R4c-19 | мутант №11 MaintenanceLease::drop Release->Relaxed (tests/loom_r8_maintenance_lease.rs) | PASS | рецептура валидна |
| R4c-16 | мутант №8 без initialised-гейта (src/registry/heap_registry/claim.rs) | PASS | маркер сделан платформонезависимым (п.3) |
| R5b-19 | C6 fallback OOM (src/global/fallback.rs) | PASS | впервые прогнан на CI (r20-gates 39239743215): baseline ok, мутант убит; метаданные строки исправлены по рецепту (features, scope crate-internal), добавлена строка source_scope |
| R4a-09 | нет | PASS (исторический) | мутанта/рецепта нет: файла docs/evidence/reverify-recipes/R4a-09.json в worktree не существует (проверено ls — файл отсутствует), stale-recycler сценарий недоступен; исторический PASS сохраняется без перепроверки |
| R4b-13 | L1 Release->Relaxed в связанной строке drop модели Ph4b (tests/loom_r11_ph4b_publish_recycle_drain.rs) | PASS | G3: в модели появилась core-ячейка держателя аренды — S2-оракул; прежде INCONCLUSIVE |
| R4b-14 | L2 claim без Acquire в связанной строке модели Ph4b | PASS | G3: та же core-ячейка; прежде INCONCLUSIVE |

## 3. Финальные команды этого раунда (Windows, worktrees/ev1)

| Команда | exit code |
|---|---|
| `node --check scripts/r20_evidence_reverify_support.mjs` | 0 |
| `node --check scripts/r20_evidence_reverify_run.mjs` | 0 |
| `timeout 600 node tests/evidence_reverify_support_selftest.mjs` | 0 (symlink-кейс выполнен: «symlink rejection case executed») |
| `timeout 600 node tests/evidence_reverify_runner_selftest.mjs` | 0 |
| `timeout 600 node scripts/r20_evidence_reverify_run.mjs --selftest` | 0 (включает оба фокус-теста через spawn) |
| `timeout 600 node scripts/r20_evidence_reverify_run.mjs --dry-run --config .github/evidence-reverify.json` | 0 (все 6 рецептов распарсены и провалидированы, включая R4c-16 с новым маркером) |
| `grep -c 'target/' scripts/r20_evidence_reverify_run.mjs tests/evidence_reverify_runner_selftest.mjs tests/evidence_reverify_support_selftest.mjs` | 0 вхождений во всех трёх (grep exit 1 = нет совпадений) |
| `timeout 600 node scripts/verify-evidence-registry.mjs` | 0 |
| `timeout 600 node scripts/evidence-registry-selftest.mjs` | 0 (107 cases) |
| `git status --porcelain` | src/ чист — изменены только scripts/, tests/, docs/evidence |

## 4. Лимиты строк файлов

| Файл | Строк | Лимит 1000 |
|---|---|---|
| scripts/r20_evidence_reverify_run.mjs | 947 | ok |
| scripts/r20_evidence_reverify_support.mjs | 479 | ok |
| scripts/ev1_native_cycle.mjs | 80 | ok |
| tests/evidence_reverify_runner_selftest.mjs | 111 | ok |
| scripts/ev1_receipt_fixture.mjs | 4 | ok |
| tests/evidence_reverify_support_selftest.mjs | 209 | ok |

## 5. CI-перепроверка

r20-gates run 39239743215 (HEAD 810c018a, tree 91a2460d): E0 PASS по шести строкам R4a-05, R4b-15, R4c-09, R4c-19, R4c-16, R5b-19; у всех `mutant_rejected=true`. Receipts — `docs/evidence/_raw_reverify_<id>.log`.

## 6. Ограничения и риски

- Loom и Miri локально не гонялись; loom-рецепты подтверждены на CI (§5).
- Linux crash-форма (`signal: 11, SIGSEGV: invalid memory reference` и родственные SIGBUS/SIGABRT) не наблюдалась нативно: regression-покрытие — синтетические selftest-строки в tests/evidence_reverify_runner_selftest.mjs; нативный прогон возможен только в Linux CI.
- ev1_native_cycle.mjs Windows-специфичен (0xc0000005/3221225477-форма и CARGO_TARGET_DIR под .rush); на Linux этот помощник неприменим.
- Scratch-каталоги самотестов — строго внутри worktree (`.rush/e0-selftest-*`, `.rush/evidence-reverify-runner-selftest-*`, `.rush/scratch-*`) с очисткой; вне worktree ничего не пишется.
