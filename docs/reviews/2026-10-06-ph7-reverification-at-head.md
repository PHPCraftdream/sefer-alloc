# Перепроверка реестра доказательств на HEAD — Ph7b

**Дата:** 2026-10-06 · **Задача:** issue #2098, zero-trust перепроверка пониженных клеток `docs/evidence/registry.csv`; возврат в PASS только при фактическом evidence.

**HEAD:** `876429d2a63ce73dd248bbacc4c6d6c528692cbb` · **tree:** `07ba7fb5bedb95c09407d10ca35596bdbd7e5db7`.

**Среды:** Windows — rustc/cargo 1.97.0 (`stable-x86_64-pc-windows-msvc`), `CARGO_TARGET_DIR=target-ph7b`; WSL — rustc 1.98.1, `CARGO_TARGET_DIR=$HOME/target-ph7b-numa`; в обоих случаях `CARGO_BUILD_JOBS=4`. Workspace lint: только `unexpected_cfgs=warn`.

## Методология

- **Фаза A — фиксация контекста:** проверки относятся к указанным HEAD/tree; доказательства — приведённые ниже логи `docs/evidence/`.
- **Фаза B — базовые адресные проверки:** сопоставление каждой клетки с фактическим результатом нужного теста/бинарника, включая Miri и pin-наборы.
- **Фаза C — проверка чувствительности:** точечные мутанты должны провалить ожидаемый оракул по ожидаемой причине; после отката фиксируется baseline.
- **Фаза D — широкие и платформенные проверки:** полный прогон с `numa_shim_mock`, повторения NUMA-бинарников на Windows и WSL. Итог оценивается по бинарникам, а не только по коду завершения общего цикла.

## Вердикты

| ID | Evidence | Наблюдаемый результат | Вердикт |
|---|---|---|---|
| R0-06 | `_raw_reverify_head_r0_06_box_provenance.log` | 1 passed | PASS |
| R0-16 | `_raw_reverify_head_r0_16_terminal.log` | 8 + 4 + 1 passed (у последнего 5 filtered) + 6 passed; `r8_terminal_global` самоперезапускается по child-паттерну, согласуется с исходными 18 | PASS |
| R0-17 | `_raw_reverify_head_r0_17_nonfastbin.log` | 7 + 2 + 1 (5 filtered) + 6 passed; согласуется с исходными 15 | PASS |
| R0-19 | `_raw_reverify_head_r0_19_miri_p.log` | Miri P: 1 passed, 5 filtered, `MIRI_EXIT=0` | PASS |
| R0-20 | `_raw_reverify_head_r0_20_miri_pt.log` | Miri PT: 1 passed, 5 filtered, `MIRI_EXIT=0` | PASS |
| R4a-01 | `_raw_reverify_head_r4a01_miri.log` | Miri strict-provenance + disable-isolation: 1 passed, `MIRI_EXIT=0` | PASS |
| R4a-08 | pins + `_raw_reverify_head_mut_r4a08.log` | baseline `regression_claim_oom_initialised_gate`: 3 passed; M4 initialised-gate bypass — `0xc0000005 STATUS_ACCESS_VIOLATION`, `MUTANT_EXIT=5` | PASS |
| R4a-10 | pins + `_raw_reverify_head_mut_r4a10.log` | pin `r11_ph4a_drop_ordering_pinned`: 2 passed; Release→Relaxed провалил `lease_drop_publishes_live_to_free_with_release` (LIVE→FREE `cas_state`, неверный Ordering), `MUTANT_EXIT=101`; baseline 2 passed | PASS |
| R4b-06 | pins | `r11_ph4b_no_worker_pending_stays`: 1 passed | PASS |
| R4b-07 | `_raw_reverify_head_r4b07_spawn_retry.log` | 1 passed | PASS |
| R4b-09 | pins + `_raw_reverify_head_mut_r4b09.log` | baseline 3 passed; drop(lease) до unpublish провалил `finish_bind_rollback_unpublishes_local_before_the_lease_drop`, `MUTANT_EXIT=101` | PASS |
| R4b-11 | pins + `_raw_reverify_head_mut_r4b11.log` | baseline 2 passed; `mark_local_torn` после drop(lease) провалил `teardown_stamps_torn_then_trims_then_releases_the_slot`, `MUTANT_EXIT=101` | PASS |
| R4b-12 | pins + `_raw_reverify_head_mut_r4b12.log` | baseline 3 passed; после компиляционной итерации Cell→RefCell скомпилированный мутант провалил `guard_stores_the_typed_lease_option`, `MUTANT_EXIT=101` | PASS |
| R4c-01 | `_raw_reverify_head_r4c01_oracle_x20.log` + `_raw_reverify_head_mut_r4c01_large_x2.log` | оракул: 20/20 прогонов по 2 passed; Large fetch_add(1)→(2) провалил `reclaim_trim_consumes_ingress_exactly_once`, observed 32 вместо 16, `MUTANT_EXIT=101`; baseline 2 passed | PASS |
| R4c-12 | `_raw_reverify_head_mut_r4c12.log` | baseline 2 passed; удаление `trim_for_recycle` в re-claim: `reclaim_trim` (left 0, right 1), `MUTANT_EXIT=101` | PASS |
| R4c-20 | `_raw_reverify_head_mut_r4c20.log` | baseline 2 passed; дублирование `background_maintenance_step`: `maintenance_pass_ingress_step_runs_exactly_once_per_maintained_slot` (left 2, right 1), `MUTANT_EXIT=101` | PASS |
| R4c-21 | `_raw_reverify_head_mut_r4c21.log` | baseline 2 passed; ранний return 0 в `drain_sidecar_ingress`: `reclaim_trim` (left 0, right 1), `MUTANT_EXIT=101` | PASS |
| R5c-01 | `_raw_reverify_head_r5c01_saturation.log` | `r11_ph5c_registry_saturation_fallback`: 1 passed | PASS |
| R5c-11 | `_raw_reverify_head_mut_r5c11.log` | baseline 1 passed; fallback→panic завершился кодом 9; повтор с `--nocapture` зафиксировал `mutant: registry exhausted`, `MUTANT_EXIT=9` | PASS |
| R-Ph3a | см. расщепление ниже | единая клетка разделена на native и iai | SPLIT |
| R-Ph3b | `_raw_reverify_head_rph3a_native_rph3b.log` | `r11_ph3b_kind_aware_entries`: 5 passed (не 7, как в старом receipt) | PASS |
| R6a-A10 | `_raw_reverify_head_r6a_a10_numamock_full.log` | `--all-features --no-fail-fast`, `RUSTFLAGS=--cfg numa_shim_mock`: `FULL_EXIT=0`; `rg -c "Running tests"` = 346; `rg -c FAILED` — пусто, exit 1 = 0 красных | PASS |
| R6a-EX3 | Windows и WSL ×20 логи | `r6_fix_p3_numa_unknown_bucket`: 1 passed ×20 на каждой ОС; Windows `RUN_EXIT=0` ×20, WSL общий `RUN_EXIT=101` из-за отдельного r1_04 (см. приложение) | PASS; Windows x86_64 + Linux x86_64 (WSL) |
| R6a-EX4 | те же Windows и WSL логи | `segment_directory_numa_high_node_ids`: 2 passed ×20 на каждой ОС; WSL общий `RUN_EXIT=101` по причине r1_04, не NUMA-тестов | PASS; Windows x86_64 + Linux x86_64 (WSL) |

### Расщепление R-Ph3a

- **R-Ph3a-native — PASS.** Нативные `r11_ph3a_scalar`, `r11_ph3a_batch`, `r11_ph3a_reserve`, `r11_ph3a_magazine`: по 1 passed. Три транзакционных мутанта подтвердили чувствительность:
  - (a) `_raw_reverify_head_mut_rph3a_a.log`: `set_head` до `prepare_small_issue`; `scalar_issue_transactions_roll_back_before_commit_and_retry_after` поймал неверный head (192512 вместо 192528), `MUTANT_EXIT=101`, baseline 1 passed.
  - (b) `_raw_reverify_head_mut_rph3a_b.log`: `inc_live` до prepare; тот же тест поймал live 1 вместо 0, `MUTANT_EXIT=101`, baseline 1 passed.
  - (c) `_raw_reverify_head_mut_rph3a_c.log`: `add_live(out.len())` в batch; `batch_issue_transactions_commit_only_the_prepared_run` поймал 421 вместо 261, `MUTANT_EXIT=101`, baseline 1 passed.
  Фактические имена ловцов приведены из логов и отличаются от имён, предсказанных прежним receipt.
- **R-Ph3a-iai — CONDITIONAL.** Iai-замер не перепроверялся: перезапуск iai запрещён рамками Ph7b; база evidence осталась от прежнего receipt.
- Старый ID `R-Ph3a` выведен в `retired_ids.csv`.

## Протокол мутант-циклов

Для каждого цикла: резервная копия файла в `target-ph7b/*.ph7b-backup` → точечная правка → прогон с маркерами `MUTANT`/`BASELINE` и `MUTANT_EXIT` → ожидание красного по конкретной причине → откат через `cp` и `rm` → проверка пустых `git diff/status -- src tests` → baseline-прогон в тот же лог. Cargo запускался строго последовательно; правки выполнялись только через суб-агентов. Все 13 циклов по реестру и три R-Ph3a-мутанта красные по нужным причинам.

| Ключ | Файл правки | Суть | Тест-ловец | Причина красного | Exit |
|---|---|---|---|---|---|
| R4a-08 / M4 | инициализационный gate regression | обход gate | `regression_claim_oom_initialised_gate` | `STATUS_ACCESS_VIOLATION` | 5 |
| R4a-10 | ordering drop | Release→Relaxed | `lease_drop_publishes_live_to_free_with_release` | Relaxed вместо Release | 101 |
| R4b-09 | bind rollback | drop lease раньше unpublish | `finish_bind_rollback_unpublishes_local_before_the_lease_drop` | нарушен порядок отката | 101 |
| R4b-11 | teardown | `mark_local_torn` после drop lease | `teardown_stamps_torn_then_trims_then_releases_the_slot` | нарушен порядок teardown | 101 |
| R4b-12 | lease storage | Cell→RefCell | `guard_stores_the_typed_lease_option` | после успешной компиляции нарушен ожидаемый тип хранения | 101 |
| R4c-01 Large | ingress counter | fetch_add(1)→(2) | `reclaim_trim_consumes_ingress_exactly_once` | observed 32 вместо 16 | 101 |
| R4c-12 | re-claim | убрать `trim_for_recycle` | `reclaim_trim_consumes_ingress_exactly_once` | left 0, right 1 | 101 |
| R4c-20 | maintenance | продублировать `background_maintenance_step` | `maintenance_pass_ingress_step_runs_exactly_once_per_maintained_slot` | left 2, right 1 | 101 |
| R4c-21 | ingress drain | ранний return 0 | `reclaim_trim_consumes_ingress_exactly_once` | left 0, right 1 | 101 |
| R5c-11 | exhausted leg | Fallback→panic | saturation mutant | наблюдалась `mutant: registry exhausted`; процесс завершился кодом 9 | 9 |
| R-Ph3a (a) | scalar issue | set_head до prepare | `scalar_issue_transactions_roll_back_before_commit_and_retry_after` | head 192512 вместо 192528 | 101 |
| R-Ph3a (b) | scalar issue | inc_live до prepare | тот же scalar-тест | live 1 вместо 0 | 101 |
| R-Ph3a (c) | batch issue | add_live(out.len()) | `batch_issue_transactions_commit_only_the_prepared_run` | 421 вместо 261 | 101 |

## Приложение: r1_04 на WSL

`r1_04_alloc_core_drop_stack_pressure` относится к R6a-EX1 и **не входит** в повышаемые здесь клетки. Windows ×20: 2 passed, 1 ignored, общий `RUN_EXIT=0`. WSL ×20: падают `drop_on_64kib_stack_bare` и `drop_on_64kib_stack_with_many_segments`, лог содержит `fatal runtime error: stack overflow, aborting`. Это ожидаемая регрессия ветки без исправления `784bdfdf`: в main уменьшен `CALLS_CAP` с 4096 до 256 в `crates/numa-shim`; статический TLS мока вырезается glibc из малого стека потока. HEAD ph7b этого фикса не содержит.

Таким образом, WSL `RUN_EXIT=101` ×20 в комбинированном EX3/EX4 прогоне ожидаем и согласован с каноном: он вызван r1_04, а не целевыми NUMA-бинарниками. Привязка по бинарникам в логах подтверждает `r6_fix_p3_numa_unknown_bucket` — 1 passed ×20 и `segment_directory_numa_high_node_ids` — 2 passed ×20 на обеих ОС. Это не расхождение.

## Оговорки

- R4b-12: первая секция лога — итерация, не прошедшая компиляцию (`E0308`: `lease.take()` в `replace`); следующая секция — скомпилированный мутант и ожидаемый тестовый отказ.
- R5c-11: паника наблюдалась в `--nocapture` повторе. Причина точного abort-механизма (double-panic) — **INFERRED**, не непосредственно доказана логом.
- R-Ph3b: наблюдалось 5 passed, хотя старый receipt указывал 7; в отчёте сохранено именно наблюдаемое значение.
- R4c-21: красный только один тест `reclaim_trim`; второй тест оракула этой мутацией не задет и пинжится R4c-20. Для активации строки R4c-21 достаточно, что оракул красный.
- R6a-EX1 не повышается этим отчётом; WSL-падение r1_04 вынесено в приложение и не меняет вердикт целевых NUMA-бинарников.

## Логи-доказательства

Все пути относительно `docs/evidence/`:

```text
_raw_reverify_head_r0_06_box_provenance.log
_raw_reverify_head_r0_16_terminal.log
_raw_reverify_head_r0_17_nonfastbin.log
_raw_reverify_head_r0_19_miri_p.log
_raw_reverify_head_r0_20_miri_pt.log
_raw_reverify_head_r4a01_miri.log
_raw_reverify_head_r4a10_r4b06_r4b09_r4b11_r4b12_pins.log
_raw_reverify_head_mut_r4a08.log
_raw_reverify_head_mut_r4a10.log
_raw_reverify_head_mut_r4b09.log
_raw_reverify_head_mut_r4b11.log
_raw_reverify_head_mut_r4b12.log
_raw_reverify_head_r4b07_spawn_retry.log
_raw_reverify_head_r4c01_oracle_x20.log
_raw_reverify_head_mut_r4c01_large_x2.log
_raw_reverify_head_mut_r4c12.log
_raw_reverify_head_mut_r4c20.log
_raw_reverify_head_mut_r4c21.log
_raw_reverify_head_r5c01_saturation.log
_raw_reverify_head_mut_r5c11.log
_raw_reverify_head_rph3a_native_rph3b.log
_raw_reverify_head_mut_rph3a_a.log
_raw_reverify_head_mut_rph3a_b.log
_raw_reverify_head_mut_rph3a_c.log
_raw_reverify_head_r6a_a10_numamock_full.log
_raw_reverify_head_r6a_ex3_ex4_numa_win_x20.log
_raw_reverify_head_r6a_ex3_ex4_numa_wsl_x20.log
```

## Дополнение оркестратора: R6a-EX1 (r1_04) закрыт

Красный `r1_04_alloc_core_drop_stack_pressure` на WSL, описанный в приложении выше, оказался настоящей регрессией, а не средой: инлайн-лог мока numa-shim (`CALLS_CAP = 4096`, ~128 KiB) лежит в статическом TLS каждого потока, а glibc вырезает статический TLS из стека потока — у потоков теста со стеком 64 KiB не оставалось полезного стека. Исправлено коммитом `784bdfdf` (`CALLS_CAP` 4096 -> 256). Контрфактуал: WSL красный при 4096, зелёный при 256; Windows зелёный; CI job `test (gated bodies + all-features)` (run 37399933378) красный до фикса и зелёный после. Строка R6a-EX1 повышена до PASS (evidence — `docs/evidence/_raw_reverify_head_r1_04_cap256.log`).
