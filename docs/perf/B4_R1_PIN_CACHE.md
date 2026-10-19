# B4-R1: TLS-кэш route pins — отчёт Round 4

**Статус:** Native matrix и Loom (включая финальную cache-guard lib-check) завершены родителем. Clippy 8 завершены. CI Miri SB/TB и benchmarks остаются вне подтверждённых receipts. Round 4 включает изменения cache/model/fixture исходников.

## 1. Публикация и ownership

Обычная native fixture issue path: `SegmentTable::issue_small` (`src/alloc_core/segment/segment_table/segment_table_impl.rs:320–329`) → `RouteSlots::issue_small` (`route_slots.rs:140–153`) → `RouteRegistration::issue_small` (`src/registry/segment_route/registration.rs:36`); commit вызывает его через `IssueTransaction` (`issue_transaction.rs:95`).

| Unsafe hook call | Issue witness | Ожидаемое ownership поведение |
|---|---|---|
| `tests/b4_r1_pin_cache.rs:398` | `registration` `:375`, issue offset 0 | miss; ownership сохраняется |
| `:449` | first registration, issue offset 0 `:436` | miss; ownership сохраняется |
| `:476` | second registration, issue offset 0 `:460` | success; issued instance consumed |
| `:867` | `registration_b` `:826`, issue offset 0 | miss; ownership сохраняется |

Loom: distinct pointers `tests/loom_b4_r1_pin_cache.rs:597`; issues=0/16 `:550–551`; exit/new incarnation — issue instance 0 `:686`, ровно один publish `:698`. Miri test `tests/b4_r1_pin_cache_miri.rs:94` использует Box ownership/drop; raw debug-publish hook не вызывается.

Native fixture GlobalAlloc inventory: alloc results null-checked (`tests/b4_r1_pin_cache.rs:192–193, 693–701`); foreign dealloc передаёт уникальный live allocation worker-у и освобождает ровно один раз с exact Layout. Small 256/8 — `:250–267, 301–318, 528–539`; mid 4096/8 — `:692–733`; pointer provenance expose/re-expose отражён в fixture. Routing unsafe entry points `src/registry/heap_core_xthread/routing.rs:20,26,38,61,75,87,103`: caller transfers unique current issued allocation once; cached Miss переходит в slow lookup, publication success передаёт его дальше. Обе ветви обязаны сохранять miss/fallback ownership и terminal publication contract; это не blanket safety closure.

## 2. Loom и seam

`route_pin_cache.rs:123–131` использует `all(loom,test)` seam и native complement: Loom actual-source include harness моделирует TLS; обычная TLS ветка Loom не меняется. Первичный lib-check выявил E0433 (loom — dev-only dependency); seam исправлен.

Финальная Loom baseline — 4/4, exit 0; cache-guard lib-check exit 0. Counterfactual receipts (оба release cfg(loom) full-target, features `alloc-core,alloc-xthread,tagged-index-stack/loom`): gate `if !pin.linked()` → `if false`: actual O2=1, mutant expected O2=0, exit 101; Drop `mem::forget(slot.pin.take())`: actual linked-exit O4=3, mutant expected O4=2, exit 101 без abort. После каждого mutant source восстановлен, SHA-256 `0b5bf44b9b2575e166070982b337e3ce7538864a77102555432042f78c4872c4`; родитель перечитал hash и baseline 4/4 exit 0. Итоговый model SHA-256 `df78790dffc4fde6e027d18ccf2dc4634213755aef7f049dc6f41ad5b1b92330`.

Loom 0.7.2 cleanup требует join/notify до runtime TLS cleanup (`thread.rs:168–169`, `rt/mod.rs:177–196`). 64-yield bounded witness не является общим доказательством и не гарантирует fairness. Необходимость Acquire не установлена: shadow-pair показывает только механизм; ordering не меняли.

## 3. Miri и CI

`scripts/miri-matrix.mjs:49–52`, `.github/workflows/ci.yml:3053–3068`: SB/TB, Miri-core `needs: changes` для seam. Workflow запускается `pull_request` в main либо `workflow_dispatch`; branch push сам по себе не trigger. Planner для source/test/CI yaml, full и seam=true, родитель наблюдал. Активация пути: HITS≥1, INSERTS≥2, RELEASES≥2. Локальный Round4 Miri не запускался. Round3 Windows остановился до тела на unsupported `GetModuleHandleW`; Linux — до тела из-за отсутствующего gcc linker; это не UB и не body-ok. Фактические CI SB/TB jobs: **PENDING**, CI green не заявляется.

## 4. Worker exit и native matrix

Временный worker-exit counterfactual: `drop(worker_reports)` заменён на `core::mem::forget(worker_reports)` после ref assertions, начальная часть не менялась. Проверяемый критерий: exit 101, ожидается 64 sidecar frees (tolerance 16), без abort. Восстановленный native source SHA-256 `9b4e5a127dab8ba9d552203728b72b15f8e0cf291e1d909f277ec49e797d2193`, before/after совпали. Debug exact result 0; release whole-file `b4_r1_pin_cache`: 7/7, exit 0. Ранее найденный restoration mismatch/duplicated loop исправлен; ошибочный старый hash не использовать.

Модель 20 целей выше запускается с тремя feature sets (A `alloc-global alloc-xthread internals bench-internals`; B `production internals bench-internals`; C `production hardened internals bench-internals`) в debug/release — всего 6 summary. Порядок feature sets × profile: cfg=0 соответственно 9/9, 5/5, 5/5. Native totals: 120 target executions, все шесть summary exit 0. Первые пять summary завершились до timeout внешней команды; шестая строка прервана и повторена отдельно, exit 0. cfg=0 являются executions, не PASS-тестами.

```text
b4_r1_pin_cache
b4_r1_pin_cache_miri
fastbin_requires_xthread
no_stale_doc_references
r10_7_alloc_batch_xthread_double_free
r12_01_foreign_free_of_unissued_granule
r3_1_fallback_remote_free
r6_route_directory
r6_route_directory_global
r6_route_directory_model
r6_u21_foreign_free_interior_granule
regression_r4_2_null_base_foreign_guard
regression_r6_t22_foreign_window_key_miri
regression_realloc_foreign_pointer_null
regression_realloc_xthread_stamp
regression_xthread_large_free_no_leak
regression_xthread_small_ring_miri
regression_xthread_thread_free_alias_miri
stats_foreign_or_unroutable_frees
xs_r3_03_dealloc_foreign_pointer_docs
```

`r6_t50` исключён; только отдельная историческая large-reserved-capacity 5/5, не required matrix. Все 8 Clippy commands завершились exit 0:

```text
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --features experimental -- -D warnings
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --all-targets --features 'hardened medium-classes internals' -- -D warnings
cargo clippy --lib --features production -- -D warnings
cargo clippy --all-targets --features 'production internals' -- -D warnings
cargo clippy --lib --features 'numa-aware internals' -- -D warnings
cargo clippy --lib --features 'alloc-global alloc-xthread' -- -D warnings
```


## 5. Уточнения и ограничения

`no_stale_doc_references` входит в 20; `r6_t50` — нет. Удалено неподдержанное утверждение об async TLS на native Windows (бывшие строки 566–567). Старый Loom abort superseded моделируемым TLS и более поздними проверками; его claims отозваны, не выдаются за успех.

Все четыре Node verifier завершились exit 0: `verify-dbg-hook-safety` — reviewed safe 252, reviewed unsafe 35, bench-gated safe 125; `verify-structure-limits` — 21 selftests, 9 dirs, 28 files, grandfathers none grown; `verify-loom-target-wiring` — 20 roots; `verify-miri-target-wiring` — 13 inventory, 32 entries, 30 CI entries. Diff-check exit 0 с CRLF warning.


## 6. Diff, интеграция и perf

Родитель прочитал `git diff b4-r1-pin-cache origin/main --stat`: 227 files, +15681/−3403. `origin/main`=`3f0eab59c336a5c237f229696e324ec5d62e3964`; merge-base=`24ef8be2fb2ad483d9f0fdcba7ccd81285da6a19`. Конфликтный список — inference, не merge result: `.github/workflows/ci.yml` и `scripts/loom.mjs` target/model wiring сохранить обе стороны; README/docs architecture inventories; `scripts/verify-dbg-hook-safety.mjs`; `routing.rs` общий `B4_FOREIGN_PUBLISH_OK` tail; `directory.rs` lookup diagnostics/remove/unlink linked=false под lock; `small_sidecar.rs` accessor вероятно disjoint; `con01` STATE constants vs cfg_attr вероятно disjoint. Новые R1 files, отсутствующие в main, не являются upstream deletions. Git writes не выполнялись.

Кэш ограничен четырьмя pins; оценка footprint около 1.16 MiB/thread без System overhead. Idle retention сохраняется до miss/insert/exit; cache hit не добавляет нового RMW. Замеры оркестратора — §7. Предложение коммита: `perf(runtime): bound cached route-pin retention and tighten publication ownership`.

Остаются pending: фактические CI Miri SB/TB jobs.

Appendix: exact verification commands/results. Для всех cargo commands: отдельный `CARGO_TARGET_DIR` вне worktree. Native: один invocation на комбинацию feature set/profile, команда `cargo test [--release] --features F --test T1 ... --test T20 -- --test-threads=1`; всего 6 cargo invocations и 120 target executions, 6 summaries exit 0. Первые пять завершились; внешняя команда для шестой строки достигла timeout 120 секунд, повтор шестой завершился exit 0. Внешний Python supervisor ограничивал каждый cargo invocation (все 20 targets вместе) таймаутом 100–105 секунд; при его срабатывании предусмотрен taskkill дерева. Внешний лимит инструмента 120 секунд прерывал длинные последовательности; недополученные строки повторены отдельно. После окончательной cfg-правки все шесть native строк и восемь Clippy строк также завершены с exit 0; повторные последовательности тоже пришлось разбить по этому внешнему лимиту. Loom: `RUSTFLAGS='--cfg loom' cargo test --release --features 'alloc-core,alloc-xthread,tagged-index-stack/loom' --test loom_b4_r1_pin_cache` — 4/4, exit 0. Lib check: `RUSTFLAGS='--cfg loom' cargo check --features 'alloc-global,alloc-xthread,tagged-index-stack/loom' --lib` — exit 0.

## 7. Замеры оркестратора

### 7.1 iai A/B (`perf_gate_iai`, `--features "production bench-internals internals"`, GitHub Actions, rustc 1.99.1, Linux 6.17.0-1022-azure)

Run 1 (39219907145): A = `3f0eab59` (main), B = `344c02b6`. Межпоточная публикация −72.5 %, но free своего потока +1 Ir/op: без `#[cold]` на `publish_foreign` поиск в кэше встраивался в `dealloc_routing`. Сырые данные: [`B4_R1_PIN_CACHE_iai_run1_summary.csv`](B4_R1_PIN_CACHE_iai_run1_summary.csv).

| cell | A Ir | B Ir | delta Ir | delta / A |
|---|---:|---:|---:|---:|
| `cold_alloc_free_256x16b` | 106718 | 106974 | +256 | +256 / 106718 = 0.24 % |
| `dealloc_free_only_1088_16b_n1024` | 304428 | 305324 | +896 | +896 / 304428 = 0.29 % |

Run 2 (39222595039): A = `3f0eab59`, B = `811bfbae` (`publish_foreign` снова `#[cold]`, как в main). Сырые данные: [`B4_R1_PIN_CACHE_iai_run2_summary.csv`](B4_R1_PIN_CACHE_iai_run2_summary.csv). Таблица — `node scripts/r6_gate_table.mjs docs/perf/B4_R1_PIN_CACHE_iai_run2_summary.csv <cells>`:

| cell | A Ir | B Ir | delta Ir | delta / A |
|---|---:|---:|---:|---:|
| `r19_prf01_joined_producer_same_word` | 1445928 | 476019 | -969909 | -969909 / 1445928 = -67.08 % |
| `cold_alloc_free_256x16b` | 106718 | 106719 | +1 | +1 / 106718 = 0.00 % |
| `dealloc_free_only_1088_16b_n1024` | 304428 | 304429 | +1 | +1 / 304428 = 0.00 % |
| `alloc_magazine_hit_only_16b` | 51375 | 51428 | +53 | +53 / 51375 = 0.10 % |
| `oscillating_live_set_16b` | 73937 | 74213 | +276 | +276 / 73937 = 0.37 % |
| `large_cache_eviction_hit_only_41x4mib` | 233090 | 233173 | +83 | +83 / 233090 = 0.04 % |

cells: 106; delta > 0: 68; delta < 0: 5; delta = 0: 33. Положительные дельты — постоянные на ячейку (не растут с числом операций): +1 Ir на ячейку в путях free своего потока и надбавки при регистрации маршрута (`Entry` получил поле `linked`; `large_cache_*` ≈ 2 Ir на каждую из 41 регистрации). Сам механизм надбавок +53/+276 профилем не подтверждён.

### 7.2 Однопоточная стоимость (callgrind, WSL, маргинальные Ir/op на 50000→100000 шагов)

Локальный callgrind оркестратора (вне репозитория): main `3f0eab59` против `811bfbae` — larson T=1: 319 / 319, mstress T=1: 494 / 494. Однопоточной цены R1 не имеет.

### 7.3 Wall-clock

Локальный Windows A/B (10 раундов, `malloc_macro` + criterion `global_alloc`, A = `3f0eab59`, B = `811bfbae`) не пригоден как доказательство: машина была нагружена посторонними процессами, mimalloc внутри тех же раундов колебался 22…52 M ops/s; отношение Sefer/mimalloc по медиане не отличается между A и B. Вердикт стоит на iai (§7.1) и callgrind (§7.2).
