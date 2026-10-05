# Ph5b hardening matrix (#2094)

Дата: 2026-10-05. Worktree `worktrees/ph5b`, ветка `ph5b`. Шаг 1 фазы Ph5b — инвентаризация.
Гейты (ADR `2026-10-01-adr-physical-boundary-and-progress.md` §Гейты + addendum
`2026-10-05-adr-addendum-ph3c-path-b.md`): C1 = KNOWN-DEFECT P1-box (не трогать);
C4 = MODEL-LIMIT (ожидаемый красный); C2/C3/C5/C6 are verification targets; phase closure requires verified evidence for each and is not presumed from partial PASS results.

Код входов: `src/registry/heap_core/alloc/{hot,batch}.rs`,
`src/registry/heap_core/free/{dealloc,dealloc_batch,dealloc_own_base,realloc}.rs`;
bootstrap: `src/alloc_core/alloc_core/bootstrap.rs`; diag-хуки: `src/alloc_core/small/alloc_core_small_diag.rs`.

## Инвентаризация

Легенда: ✅ = есть тест(ы); ⚠️ = частично; · = нет найденного теста. Файлы — `tests/`.

### C2 — reuse/zero/copy/realloc/release после join (включая новую инкарнацию на совпавшем VA)

| Вход | Feature | Тесты (файл → что проверяет) | Пробел |
|---|---|---|---|
| Small alloc/dealloc | production | `freelist_reuse.rs` → reuse freed small-блока; `regression_r5_01_alloc_global_cross_thread.rs` → cross-thread free/alloc после join | reuse после join на совпавшем VA (новая инкарнация) не покрыт явно |
| Small alloc/dealloc | alloc-global | `regression_r5_01_alloc_global_cross_thread.rs` → cross-thread через global path | — |
| alloc_zeroed Small | production | `alloc_zeroed_virgin_small_skip.rs` → virgin-skip корректность нулей; `regression_r2_15_virgin_mask_zeroed_and_bounded.rs` → virgin-маска; `r13_3_magazine_virgin_hit_skips_zero.rs` → magazine virgin-hit | zeroed-reuse после join — нет |
| alloc_zeroed Small | virgin-zero-skip | `alloc_zeroed_virgin_small_skip.rs` → on/off семантика скипа | — |
| alloc_zeroed Primordial | production | `regression_bootstrap_oom_sentinel_rollback.rs` → только OOM-rollback bootstrap; primordial zeroed-контент покрывается `alloc_core_invariants.rs` (внутри-крейтовые проверки) | прямой тест zeroed из primordial-сегмента после join — нет |
| alloc_zeroed Large | production | `alloc_zeroed_fresh_large_skip.rs` → fresh-large skip zero; `regression_alloc_zeroed_fresh.rs` → fresh-page нули | Large zeroed после join/release — нет |
| realloc S→S / in-place | production | `realloc_in_place.rs` → in-place grow/shrink; `regression_realloc_inplace_global.rs` → global in-place | realloc после join — нет |
| realloc S→L, L→S | production | `regression_realloc_cross_class_shrink.rs` → cross-class shrink; `oxx_r2_02_realloc_lazy_commit_frontier.rs` → frontier при grow; `r17_4_inplace_grown_large_dealloc_routes_by_kind.rs` → kind маршрутизация после in-place grow | — |
| realloc L→L | production | `regression_inplace_large_realloc.rs` → in-place large realloc | — |
| batch alloc/free | batch-api | `alloc_core_batch.rs` → batch контент после primordial; `batch_tcache.rs` → tcache-путь; `regression_batch_flush.rs`, `regression_batch_freelist_drain.rs` → flush/drain; `r11_ph5b_c2_batch_reuse_after_join.rs` | direct target passed 1/1 under `production batch-api internals` and `production batch-api alloc-stats bench-internals internals`; batch sidecar-reclamation mutant **NOT CAUGHT** (reclamation-disabled test stayed green), so no reclamation claim |
| Primordial alloc/free | production | `alloc_core_batch.rs`, `alloc_core_differential.rs` → primordial-сегмент содержательно; `regression_bootstrap_oom_sentinel_rollback.rs` → rollback | — |
| physical release (small seg release/pool/decommit) | production | `r8_os_release_oracle.rs` → OS-release oracle; route lifecycle and trim tests listed | OS release is N/A for the C2 SAME-VA acceptance: OS contract does not guarantee the next reservation uses the released VA. C2 cache SAME-VA test separately PASS; this is not an OS-release claim |
| Large cache reuse, same VA | production | `r11_ph5b_c2_same_va_cache.rs` | C2 cache SAME-VA PASS 1/1; this is cached-VA reuse, not OS release/unmap |
| trim_current_thread | production | `r31_10_trim_current_thread_api.rs` → API trim; `r17_8_deterministic_trim_releases_cached_large_span.rs` → release кэша | reuse после trim+join — нет |

Смежные reuse/VA-инкарнации уже есть точечно: `r11_box_cap_gate_va.rs` → VA-gate;
`r11_w0_box_reuse.rs` → W-0 reuse; `regression_xthread_large_free_no_leak.rs`,
`regression_xthread_small_ring_miri.rs`, `regression_xthread_thread_free_alias_miri.rs`,
`regression_realloc_xthread_stamp.rs` → xthread-сценарии; `segment_directory_numa_bucket_reuse.rs` → numa reuse.

### C3 — own-thread churn и by-value функции (reuse после возврата кадра)

| Вход | Feature | Тесты | Пробел |
|---|---|---|---|
| own-thread churn + by-value after returned frames | production | `r11_ph5b_c3_by_value_churn.rs` → exact reuse across 4 classes; Miri Stacked Borrows and Tree Borrows each 1 passed | Core C3 gate PASS; broader feature/class/intrusive proof matrix remains limited |
| by-value Box return | production | `miri_global_box_acceptance.rs` → by-value Box acceptance (Miri; сейчас EXPECTED RED = P1-box); `r8_global_box_provenance.rs` → provenance; `r8_global_box_provenance.rs` + `global_alloc_installed.rs` → installed Box | нативный by-value churn (возврат Box из функции + reuse) — есть частично в `miri_global_box_acceptance`; нативной параметризации по классам нет |
| by-value через batch | batch-api | `r11_4_dealloc_batch_mixed_ownership.rs` → mixed ownership; `r24_8_dealloc_batch_multi_flush.rs`/`r25_4_dealloc_batch_multi_flush_oracle.rs` → multi-flush оракул | — |
| own-base free | production | `regression_hardened_large_kind_own_free.rs` → hardened own-free; `dealloc_own_base.rs` (src) покрыт через `r14_4_promotion_free_correctness.rs` → promotion/free | — |

### C5 — протокольные инварианты (оракулы + Loom)

| Мутант/инвариант | Тесты | Пробел |
|---|---|---|
| reuse до retire | `loom_r11_ph4b_publish_recycle_drain.rs` → publish/recycle/drain; `r11_ph4c_ingress_consume_exactly_once_oracle.rs` → exactly-once оракул | — |
| двойной retire/free | `double_free_guard.rs` → double free; `r11_4_dealloc_batch_same_segment_double_free.rs` → batch double free; `r10_7_alloc_batch_xthread_double_free.rs` → xthread batch double free | — |
| ранний free дескриптора при pin | `r11_ph4a_heap_lease.rs` → lease protocol; pin mutant | pin-mutant result was red in prior evidence; preserve as verified mutant result. This does not establish full pinning matrix coverage |
| lost publication | Loom model inventory above | full C5 Loom suite cancelled/incomplete; targeted `loom_terminal_owner_drain` PASS is not a lost-publication mutant result; lost-publication mutant NOT_RUN |

### C6 — OOM-инъекция на границах prepare/commit

| Вход | Тесты | Пробел |
|---|---|---|
| alloc null/частичный | `r11_p3_small_sidecar_issue_oom.rs`, `r11_p3_small_sidecar_zeroed_oom.rs`, `r11_p3_registry_claim_chunk_oom.rs`, `r11_p3_registry_claim_uninit_oom.rs`, `r7_p1_chunk_oom.rs`, `r10_active_kind_prepare_oom.rs`, `regression_claim_oom_initialised_gate.rs`, `r6_route_lifecycle_oom.rs` → null/частичный результат на prepare/commit | fallback OOM: `r11_ph5b_c6_fallback_oom.rs` добавлен; локальный PASS, мутанты NOT_RUN |
| realloc OOM (старый объект жив) | `regression_free_path_chunk_oom_graceful.rs` → graceful free при OOM; `regression_bootstrap_oom_sentinel_rollback.rs` → bootstrap rollback; `r11_ph5b_c6_realloc_oom_old_alive.rs` → GlobalAlloc Small→huge и Large→huge (`alloc-global`), null и сохранность старого pattern | Windows targeted 2/2; all-features selected 2/2. OOM old-alive PASS. Dealloc-before-allocation mutant красный с `STATUS_ACCESS_VIOLATION` (crash-shaped, не assertion). OOM через недостижимый VA/size; errno не предполагается |
| dealloc allocation-counter oracle | `dealloc_sublinear.rs`, `dealloc_only_no_bind.rs`, `r11_ph5b_c6_dealloc_system_count.rs` | baseline (без мутации) system alloc/realloc counter test PASS 1/1 в обоих целевых feature-наборах; only the observed System seam is covered, not every possible internal allocation. Vec-insertion mutant NOT_RUN: попытка проверки зависла (тест не завершился), процесс остановлен, вердикта нет. C6 not fully closed |
| Primordial OOM | `regression_bootstrap_oom_sentinel_rollback.rs` → sentinel rollback | — |

### C7 — OS/архитектуры

| Требование | CI | Пробел |
|---|---|---|
| Linux x86_64 | основной `test (core tiers)`/`test (xthread progression)`/`test (hardened tier)`/`check-matrix` (ubuntu-latest) | — |
| Linux aarch64 | job `test (${{ matrix.target }})` — matrix `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, native x86_64 + Linux aarch64 cross/QEMU CI (`cross test`), включая production | — |
| Windows x86_64 | `test windows (production)` (windows-latest); `MSRV 1.93 runtime ... windows`; numa-shim Windows kernel | — |
| macOS arm64 | `test macos (production)` (macos-latest = arm64); numa-shim macOS; decommit_soak + r6_terminal_owner_drain | — |
| страница 16 KiB macOS | тесты с `aligned_vmem::page_size()`: `lazy_initial_commit_page_sizes.rs`, `large_reserved_capacity.rs`, `r8_6_decommit_boundary.rs`, `segment_meta_page_alignment.rs`, `decomp_hooks_forced_page.rs`, `large_cache_occupancy_bitmask_invariant.rs` и др. | — |

## Инъекторы

- `AllocCore::dbg_*` (test-only diagnostics): `src/alloc_core/small/alloc_core_small_diag.rs` — `dbg_live_count_for`, `dbg_carve_batch`, `dbg_freelist_head_for`, `dbg_is_free_for`, `dbg_corrupt_freelist_head_next`, `dbg_drain_freelist_batch`, `dbg_alloc_bitmap_bytes_for`, `dbg_magazine_bitmap_bytes_for`, `dbg_committed_payload_end_for`, `dbg_stamp_segment_id`/`dbg_stamp_kind_byte` и др.
- Lazy-commit fault-injection: feature `lazy-commit-fault-injection` (Cargo.toml: `alloc-lazy-commit` + `aligned-vmem/fault-injection`, единственный край к fault-injection с R2-19); хуки в `src/alloc_core/small/alloc_core_small_diag.rs` (`#[cfg(feature = "lazy-commit-fault-injection")]`) и арминг в `src/alloc_core/platform/os.rs` (внутри `commit_range`).
- Fallback OOM: `src/global/fallback.rs::dbg_inject_fallback_oom_for_test`.
- Смежный негативный тест: `release_graph_no_fault_injection.rs` → release-граф без инъекции.

## C4 статус

- `tests/*model_limit*` — **пусто** (ни одного файла); как и ожидалось, клетки жили в спайке `archive/ph2-pg2`.
- C4 MODEL-LIMIT остаётся ожидаемо-красным гейтом Ph5b (reissue при паузе; release последнего блока сегмента при паузе; release/кэш Large при паузе; W−0 подтверждён PG-1). ТочечныеWitness: `r11_box_cap_gate_va.rs`, `r11_box_cap_gate_w1..w3.rs`, `r11_w0_box_reuse.rs` (W-0/VA-гейты, не полные C4-клетки).

## CI P1-box шаги

`.github/workflows/ci.yml`, 4 шага «EXPECTED RED - P1» (строки ~2831/2858/2885/2913):
{production, alloc-global} × {strict Stacked Borrows, TreeBorrows}; тест `miri_global_box_acceptance -- paused`;
шаги пинят exit ≠ 0, текст UB и место (`Node::write_next`, `src/alloc_core/platform/node.rs`, вызовы
`reclaim_sidecar_record`/`flush_run`/`dealloc_small`); зелёный witness или красный в другом месте роняет шаг (`fd79861c`).

## C7 OS-покрытие

- ubuntu-latest: rustfmt, package gates, clippy, check-matrix, feature-matrix jobs (core tiers, xthread progression, hardened tier, gated bodies + all-features, feature isolation).
- ubuntu-latest × {x86_64, aarch64}: job `test (${{ matrix.target }})` — experimental + xthread + decommit; x86_64 native, aarch64 via `cross`/QEMU (`ci.yml:3359-3360, 3386-3392`), not native hardware. True native ARM: NOT_RUN.
- windows-latest: `test windows (production)` (`--features "production internals"`), MSRV 1.93 runtime job, numa-shim kernel integration.
- macos-latest (arm64, страница 16 KiB): `test macos (production)` + decommit_soak/r6_terminal_owner_drain; numa-shim unsupported-platform contract.
- Нативного прогона требуют: все OS-джобы выше, decommit/OS-release оракулы (`r8_os_release_oracle.rs`, `decommit_soak.rs`), hugetlb job (Linux), anything зависящий от `aligned_vmem::page_size()`.

## Литералы 4096

~77 файлов tests/ содержат `4096`; большинство — безобидные размеры/классы/комментарии. Статусы кандидатов:
- `tests/alloc_core_invariants.rs:52` — assert `ptr % 4096 == 0` для large исправлен.
- `tests/r6_terminal_header_atomic.rs:19` — `page_map_off == 4096` проверяет внутренний PAGE-aligned `SegmentLayout::page_map_off()`; `segment_header_layout.rs:14-16` задаёт compile-time `PAGE`. Не runtime OS page-size; оставить.
- `tests/regression_dealloc_metadata_region_guard.rs:115` и `tests/regression_hardened_large_kind_own_free.rs:105` — комментарии теперь корректно описывают internal compile-time `PAGE`.
- `tests/decomp_hooks_forced_page.rs:127` — `dbg_decomp_page_size() == 4096`, тест именно про форс 4096.
- `tests/large_cache_occupancy_bitmask_invariant.rs:274` — комментарий/код используют `aligned_vmem::page_size()`.

## Пробелы (кандидаты на новые тесты)

Статусы ниже учитывают фактические локальные проверки и ограничения матрицы; выполненные mutant-проверки указаны отдельно, остальные мутанты NOT_RUN.

1. **[P1] C2 OS release:** N/A as a SAME-VA guarantee; OS does not promise reservation at the released address. Separately, C2 cache SAME-VA test `r11_ph5b_c2_same_va_cache.rs` PASS 1/1. `r11_ph5b_c2_reuse_after_join.rs` covers zeroed behavior only.
2. **[P1] C5 свободный список: PASS по ownership scope** — production `Node::write_next/read_next` только в owner-exclusive paths; foreign frees публикуются в independently pinned sidecars, которые owner drains прежде изменения списка. Native hardened guard tests + Loom sidecar/lease подтверждают scope; это не доказывает lost-publication mutant: C5 mutant checks остальных протоколов NOT_RUN.
3. **[P1] C6 realloc: PASS для OOM old-alive** — `r11_ph5b_c6_realloc_oom_old_alive.rs` проверяет Small→huge и Large→huge через GlobalAlloc realloc размером `1<<62` (`alloc-global`), null и неизменность старого pattern; Windows targeted 2/2 и all-features selected 2/2. Мутант dealloc-before-allocation завершился `STATUS_ACCESS_VIOLATION`: crash-shaped red, не assertion-based результат.
4. **[P1] C3 targeted test: core gate PASS.** `r11_ph5b_c3_by_value_churn.rs` confirms own-thread churn/by-value reuse across 4 classes; bypass-free-list-pop source mutant is red because the exact-pointer assertion fails. Miri Stacked Borrows and Tree Borrows each PASS (1 test). Broader feature/class/intrusive proof remains limited.
5. **[P2] C2: batch reuse после join** — target `r11_ph5b_c2_batch_reuse_after_join.rs` PASS 1/1 under both `production batch-api internals` and `production batch-api alloc-stats bench-internals internals`. Sidecar-reclamation mutant **NOT CAUGHT** (test stayed green with reclamation disabled); do not claim reclamation.
6. **[P2] C5: ранний free дескриптора при pin** — ранее временный `!=1`→`!=2` final-release mutant reportedly red в `r11_p3_small_sidecar_routes`: retained 42,496 bytes вместо expected 0; source restored. Secondary poisoned-mutex test — cascade, не независимое подтверждение.
7. **[P2] C7: литералы 4096** — см. точные статусы выше; два комментария PAGE исправлены, occupancy использует `aligned_vmem::page_size()`, внутренний `page_map_off()` literal оставить.
8. **[P3] C2: trim_current_thread → join → reuse** — trim покрыт API-тестом, без последующего reuse-потребителя.
9. **[P3] C6: alloc_zeroed Primordial после join** — bootstrap OOM покрыт, zeroed-семантика primordial — только косвенно.

**Phase status: NOT FULLY CLOSED.** C2 cache SAME-VA PASS; OS release SAME-VA is N/A. Batch focused target PASS under both batch-api feature sets; sidecar-reclamation mutant NOT CAUGHT. C5 full Loom suite cancelled/incomplete; lost-publication NOT_RUN; terminal-owner targeted PASS is not a lost-publication result. Prior temporary pin final-release mutant reportedly red in `r11_p3_small_sidecar_routes` (42,496 bytes retained vs expected 0); source restored, and the poisoned-mutex test is a cascade. C6 system-count baseline PASS 1/1 in both target feature sets; Vec-insertion mutant NOT_RUN (verification attempt hung, no verdict). Overall closure remains pending.
- **C1:** KNOWN-DEFECT; не изменялся.
- **C2:** cache SAME-VA PASS (`r11_ph5b_c2_same_va_cache.rs`, 1/1). OS release/re-reserve SAME-VA is N/A under OS contract. Join zeroed case PASS; batch join case PASS 1/1 under both reported feature sets. One alternate invocation without batch-api selected 0 tests; the actual batch target passed in both batch-api feature sets. Sidecar-reclamation mutant NOT CAUGHT (test remained green with reclamation disabled); no reclamation claim.
- **C3:** core own-thread churn/by-value-after-returned-frames gate PASS across 4 classes; bypass-free-list-pop source mutant red on exact-pointer assertion; Miri SB/TB each 1 passed. Broader feature/class/intrusive proof remains limited.
- **C4:** MODEL-LIMIT; spike-only evidence; no main-worktree expected-red witnesses were run this turn. `tests/*model_limit*` absent; named witnesses above remain partial, as specified.
- **C5:** free-list owner-exclusive scope documented; full Loom suite cancelled/incomplete. Lost-publication NOT_RUN. `loom_terminal_owner_drain` targeted PASS is not a mutant result. Prior temporary `!=1`→`!=2` final-release mutant reportedly red in `r11_p3_small_sidecar_routes` (42,496 bytes retained vs expected 0); source restored. Secondary poisoned-mutex test is a cascade, not independent evidence; pinning coverage remains incomplete.
- **C6:** realloc OOM old-alive PASS per targeted evidence; fallback OOM PASS per recorded run. `r11_ph5b_c6_dealloc_system_count.rs` baseline PASS 1/1 in both target feature sets, proving no observed System alloc/realloc calls on that path only. Vec-insertion mutant NOT_RUN: the verification attempt hung and was stopped without a verdict. C6 NOT_FULL_PASS pending mutant checks and broader proof.

- **C7:** Windows host only локально; Linux aarch64 CI идёт через cross/QEMU, не native hardware (true native ARM NOT_RUN); macOS NOT_RUN.
## Финальный статус

**Ph5b не закрыта полностью.** Статусы мутантов: KILLED = мутант дал красный тест (ожидаемо); NOT CAUGHT = мутант выжил (тест остался зелёным); NOT_RUN = вердикта нет.

| Гейт | Статус | Доказательство | Открыто |
|---|---|---|---|
| C1 | KNOWN-DEFECT (P1-box) | не изменялся; CI 4 шага «EXPECTED RED - P1» | — (вне scope Ph5b) |
| C2 | PARTIAL | Large-cache SAME-VA PASS 1/1 (`r11_ph5b_c2_same_va_cache.rs`); join zeroed PASS; batch reuse после join PASS 1/1 в обоих batch-api наборах; SAME-VA после OS release — N/A (OS не гарантирует тот же VA) | sidecar-reclamation мутант NOT CAUGHT → claim о reclamation не делается; trim→join→reuse и Large zeroed после join/release не покрыты |
| C3 | PASS (core gate) | by-value churn по 4 классам PASS; bypass-free-list-pop мутант KILLED (assertion на точный указатель); Miri SB/TB по 1/1 | широкая feature/class/intrusive матрица ограничена |
| C4 | MODEL-LIMIT (ожидаемо красный) | только spike-evidence; `tests/*model_limit*` отсутствуют | witnesses в этом ходе не запускались |
| C5 | PARTIAL | owner-exclusive scope free-list задокументирован; `loom_terminal_owner_drain` targeted PASS (не мутант); pin final-release `!=1`→`!=2` мутант reportedly KILLED в `r11_p3_small_sidecar_routes` (42,496 bytes retained vs 0, prior evidence, source restored) | полный Loom-набор cancelled/incomplete; lost-publication мутант NOT_RUN; pin-уточнение частичное (poisoned-mutex — каскад, не независимое подтверждение) |
| C6 | PARTIAL | realloc OOM old-alive PASS (Windows targeted 2/2, all-features selected 2/2); dealloc-before-allocation мутант red, но crash-shaped (`STATUS_ACCESS_VIOLATION`), не assertion; fallback OOM PASS локально; system-count baseline (без мутации) PASS 1/1 в обоих целевых наборах | Vec-insertion мутант NOT_RUN (зависание, см. ниже); мутанты fallback OOM NOT_RUN; покрыт только наблюдаемый System seam |
| C7 | PARTIAL | Windows локально; Linux x86_64/aarch64 (aarch64 через cross/QEMU), Windows, macOS arm64 — CI-джобы есть; 4096-литералы разобраны | native ARM NOT_RUN; macOS локально NOT_RUN |

**C6 Vec-insertion мутант — NOT_RUN из-за зависания.** Попытка проверки (`cargo test --test r11_ph5b_c6_dealloc_system_count --features production --target-dir target`) не завершилась: тестовый бинарь работал ~61 мин (старт 09:45, остановлен 10:46 по PID через `taskkill /PID … /T`). Вердикта мутанта нет. Факты после остановки:
- `git status` не показывает изменений в `src/` — мутации в дереве нет.
- Зависший бинарь собран в 09:45:17 из более ранней ревизии `tests/r11_ph5b_c6_dealloc_system_count.rs` (файл изменён в 09:49:52); содержал ли он мутацию — post-hoc не установить.
- Текущая ревизия теста под одним `--features production` компилируется в 0 тестов (`#![cfg]` требует `alloc-stats`); повторный прогон мутанта должен включать `alloc-stats`.
- Риск в текущем тесте (наблюдение по коду, не подтверждённая причина): неограниченные spin-wait на `READY`/`START`/`DONE` без детекта паники/блокировки worker-потока — мутант, роняющий или блокирующий worker, даст зависание вместо красного теста. Перед повтором мутанта нужен bounded wait + проверка паники worker.

**Подтверждение в этом сеансе (2026-10-05, без мутаций):** `cargo test --features "production alloc-stats bench-internals internals"` на `6ebeceb8` + незакоммиченное дерево (sha256 от `git diff` + содержимого untracked-файлов на момент старта прогона = `6534cd8939468328b457b7692430e17a1fd613c8ec3d7be50ed44f0233fe6144`): EXIT=0, 341 блок `test result:` (все ok), passed=718, failed=0, ignored=7, 3 мин 42 с (сборка из кэша, 3.61 с). `r11_ph5b_c6_dealloc_system_count` 1/1 ok; `r11_ph5b_c2_same_va_cache`, `r11_ph5b_c2_reuse_after_join`, `r11_ph5b_c3_by_value_churn`, `r11_ph5b_c6_dealloc_no_alloc`, `r11_ph5b_c6_fallback_oom` по 1/1 ok; `r11_ph5b_c6_realloc_oom_old_alive` 2/2 ok; `r11_ph5b_c2_batch_reuse_after_join` — 0 тестов в этом наборе (требует `batch-api`).

**Почему фаза неполная:** C2/C3/C5/C6 — verification targets, закрытие требует проверенного доказательства по каждому; не закрыты: C2 sidecar-reclamation мутант NOT CAUGHT; C5 lost-publication NOT_RUN и полный Loom-набор incomplete, pin-уточнение частичное; C6 Vec-insertion NOT_RUN (зависание) и мутанты fallback OOM NOT_RUN; C7 native ARM/macOS локально NOT_RUN.

## Приёмка оркестратором (zero-trust, 2026-10-05)

Агент остановился на provider peak-hours; оставшиеся проверки выполнены оркестратором в этом
worktree. Откат мутантов — перезаписью файла (свежий mtime): откат `cp`, сохраняющий mtime,
оставлял cargo на stale-сборке с мутантом и дал ложный «System.alloc в dealloc» под
`production alloc-stats` — после `touch` baseline зелёный 3/3, находка снята.

| Мутант (src/) | Тест | Результат |
|---|---|---|
| `Vec::<u8>::with_capacity(1)` в `SeferAlloc::dealloc` | `r11_ph5b_c6_dealloc_system_count` (`production alloc-stats`) | KILLED — «System.alloc during dealloc» |
| `publish_foreign` выходит до публикации (потерянная публикация) | `r11_ph5b_c2_batch_reuse_after_join` (`production batch-api internals`) | KILLED — «owner did not reuse a freed block» |
| `SidecarBitmap::publish` `fetch_or` AcqRel → Relaxed | `r11_ph5b_c5_sidecar_ordering_pinned` | KILLED (`producer_publish_is_acqrel_fetch_or`) |
| `bitmap_scan` cut `swap(0)` AcqRel → Relaxed | `r11_ph5b_c5_sidecar_ordering_pinned` | KILLED (`owner_cut_is_acqrel_swap`) |

- `r11_ph5b_c6_dealloc_system_count`: неограниченные spin-wait заменены на `wait_for` с дедлайном 30 с
  (паника/зависание peer → красный тест, не hang; это и было причиной «зависания» Vec-мутанта у агента).
- C5 lost-publication: `tests/loom_sidecar_bitmap.rs` — теневая модель; ослабление production-ordering
  её не краснит, а потеря бита в RMW невыразима. Поэтому C5 закрывается парой: нативный оракул
  потерянной публикации (batch-мутант выше) + source-pin трёх атомарных рёбер модели
  (`tests/r11_ph5b_c5_sidecar_ordering_pinned.rs`, два мутанта выше).
- Исправление агента в `large_cache_occupancy_bitmask_invariant.rs` (комментарий) откатано: исходный
  текст точнее (смещение заголовка — compile-time `PAGE`, на хосте со страницей > 4 KiB тест
  корректно skip-ается калибровкой).

### Итоговые статусы (после приёмки)

| Гейт | Статус |
|---|---|
| C1 | KNOWN-DEFECT P1-box (без изменений) |
| C2 | PASS: Large-cache SAME-VA (cache-hit oracle + нули), batch reuse после join (мутант KILLED), zeroed после join; SAME-VA после OS-release — N/A (ОС не обязана вернуть тот же VA) |
| C3 | PASS (by-value churn, мутант KILLED, Miri SB/TB) |
| C4 | MODEL-LIMIT (клетки — в спайке PG-2; новых не добавлено) |
| C5 | PASS: free-list owner-exclusive; потерянная публикация — нативный мутант KILLED; ordering — pin + 2 мутанта KILLED |
| C6 | PASS: realloc OOM old-alive (мутант KILLED), fallback OOM (мутант KILLED), dealloc не аллоцирует — System-счётчик (мутант KILLED) + OS/slot-оракулы |
| C7 | CI-матрица: Linux x86_64, aarch64 (cross/QEMU), Windows, macOS arm64 — вердикт по landing SHA; локально только Windows; нативный ARM-хост — NOT_RUN |
