# Ph5c hardening matrix (#2095)

Дата: 2026-10-05. Worktree `worktrees/ph5c`, ветка `ph5c`. Шаг 1 фазы Ph5c — инвентаризация.
Гейты и матрица поддержки: ADR `2026-10-01-adr-physical-boundary-and-progress.md` (§Гейты C1–C7,
§Матрица поддержки — entry points, §Стоп-линии); порядок teardown TORN → `trim_for_recycle` →
Drop(lease), G4, worker policy — addendum `2026-10-02-adr-addendum-ph4-decisions.md` (§2.1, §2.7–2.10).
Клетки C2/C3/C5/C6/OOM и reuse-после-join уже покрыты матрицей Ph5b
(`2026-10-05-ph5b-hardening-matrix.md`) — здесь на них ссылаемся, не дублируем.

Код входов: `src/global/tls_heap.rs`, `src/global/fallback.rs`,
`src/global/maintenance_service.rs`, `src/global/sefer_alloc/*`,
`src/registry/heap_registry/*`, `src/registry/heap_core/{alloc,free}/*`, `src/alloc_core/large/*`.

## Инвентаризация

Легенда: ✅ = есть тест(ы); ⚠️ = частично; · = нет найденного теста. Файлы — `tests/`.

### (а) Fallback при teardown TLS (после TORN)

|| Вход | Feature | Тесты (файл → что проверяет) | Пробел |
||---|---|---|---|
|| alloc из TLS-деструктора другого thread-local'а после TORN | alloc-global | `tls_heap_teardown_torn_sentinel.rs` → детерминированный hook: TORN → `CurrentHeap::Fallback`, не `Own(TORN)` (контрфактуал: удаление TORN-arms красит); `tls_heap_teardown_ordering_stress.rs` → реальный teardown: `POST::drop` аллоцирует после `GUARD::drop` — smoke/stress, не детерминированный репродутор (TSan/ASAN не прогонялись, отмечено в доке теста); `r6_fix_p2_fallback_dealloc.rs` (case `teardown`) → реальный teardown: alloc в деструкторе после GUARD попадает в fallback-heap, live-каунт подтверждён | реальный (не hook) multi-thread teardown под Miri — принципиально вне Miri (addendum §4); стресс недетерминирован |
|| dealloc fallback-объекта с другого потока | production + alloc-xthread + internals + bench-internals | `r3_1_fallback_remote_free.rs` → кросс-потоковый free fallback-блоков, busy-lock → терминальная публикация, потребление владельцем; `r6_fix_p2_fallback_dealloc.rs` (case `teardown`) → освобождение fallback-блока из следующего TLS-деструктора, fallback-lock ровно 1; (case `busy`) → занятый lock → remote-маршрут без потери free | — |
|| отсутствие повторного bind после TORN | alloc-global | `tls_heap_teardown_torn_sentinel.rs` → resolver возвращает `Fallback`, не `Own`; `r31_10_trim_current_thread_api.rs::ac4b/ac4c` → trim при TORN — no-op, слот не claim-ается; `dealloc_only_no_bind_torn.rs` → dealloc-резолвер TORN → `ForeignNoBind`, fallback-lock не берётся (счётчик `dbg_fallback_lock_acquisitions` не двигается) | повторный bind через `finish_bind` (rollback-ветки `LOCAL.try_with`/`GUARD.try_with` fail — `tls_heap.rs:461-477`) прямого теста нет |
|| dealloc без bind (TORN/never-bound) | production + internals | `dealloc_only_no_bind.rs` (см. также Ph5b C6) → dealloc не материализует слот | — |
|| порядок TORN → trim_for_recycle → Drop(lease) | production | `regression_r4_3_teardown_trim.rs` → `trim_for_recycle` в `AbandonGuard::drop` релизит cached large span (RED→GREEN counterfactual); `r11_ph4b_teardown_ordering_pinned.rs`, `r11_ph4a_drop_ordering_pinned.rs` → source-pin порядка; `loom_r11_ph4a_heap_lease.rs` → lease Drop ‖ claim ‖ maintenance (Loom) | нативный наблюдатель порядка на реальном TLS-teardown — только стресс (см. выше) |
|| fallback bootstrap-устойчивость | alloc-global + internals | `regression_fallback_init_unwind_guard.rs` → unwind при init откатывает `INIT_STATE` в UNINIT (F-8); `regression_fallback_panic_lock.rs` → panic под fallback-lock не клинит LOCK; `r11_ph5b_c6_fallback_oom.rs` → primordial OOM fallback (Ph5b C6) | — |

### (б) Fallback при насыщении registry (MAX_HEAPS)

|| Вход | Feature | Тесты | Пробел |
||---|---|---|---|
|| claim при исчерпании всех слотов → fallback | все | ✅ `r11_ph5c_registry_saturation_fallback.rs` → registry дренируется `dbg_claim_lease()` до `None` (все LIVE-lease удерживаются), свежий поток alloc → fallback-маршрут (`dbg_fallback_lock_acquisitions` растёт), M10 не-null | ЗАКРЫТО (Ph5c) |
|| dealloc при насыщении registry (fallback-объект) | все | ✅ `r11_ph5c_registry_saturation_fallback.rs` → fallback-блок освобождается с другого (never-bound) потока в состоянии насыщения, без паники | ЗАКРЫТО (Ph5c) |
|| возврат к registry после освобождения слота | все | ✅ `r11_ph5c_registry_saturation_fallback.rs` → после drop одного lease свежий поток снова Own (fallback-счётчик приращений == 0) | ЗАКРЫТО (Ph5c) |

### (в) trim_current_thread

|| Вход | Feature | Тесты | Пробел |
||---|---|---|---|
|| trim с живыми объектами | production + alloc-decommit | `r31_10_trim_current_thread_api.rs` → AC1 (эквивалентность `trim_for_recycle`), AC2 (alloc→trim→alloc жив, слот выжил), AC3 (trim A не трогает B), AC4a/c (never-bound не claim-ает слот), AC4b (TORN → no-op) | — |
|| trim после join другого потока | production | ✅ `r11_ph5c_trim_idempotent_after_join.rs` → trim на вызывающей нити при живом peer, trim после join peer, live-объекты и round-trip после всего | ЗАКРЫТО (Ph5c, ср. Ph5b пробел №8: reuse-потребитель — отдельный P3) |
|| повторный trim / идемпотентность | production + alloc-decommit | ✅ `r11_ph5c_trim_idempotent_after_join.rs` → явный оракул: первый trim — произвольная release-дельта, второй подряд — `segments_released_total` дельта строго 0; `r31_10` AC2 и `r11_ph4b` (b) — косвенно | ЗАКРЫТО (Ph5c) |
|| teardown-trim (эквивалент) | production | `regression_r4_3_teardown_trim.rs` (см. (а)) | — |

### (г) Worker: start_maintenance / maintenance_running / ownerless-прогресс

|| Вход | Feature | Тесты | Пробел |
||---|---|---|---|
|| start → running, повторный start | production + internals | `r8_autonomous_maintenance.rs::ownerless` → `start_maintenance` Ok, `maintenance_running`, идемпотентный второй `start_maintenance` Ok; изолированный child-процесс, real `#[global_allocator]` | — |
|| ownerless-прогресс (remote-free завершившегося потока переиспользуется без владельца) | production + internals | `r8_autonomous_maintenance.rs::ownerless` → owner-поток умирает, Box с реальной аллокацией переезжает в главную нить, route убирается + завершённый worker-pass ack (`wait_after_for_test`), повторно не появляется; G4 | — |
|| ошибка старта | production + internals + bench-internals | `r11_ph4b_spawn_failure_retry_releases_leases.rs` → `Err(Spawn)` не оставляет half-activation: lease/slotes свободны, retry → Ok + running; `r8_autonomous_maintenance.rs::startup_failure_and_race` (стартовый сбой и гонка, addendum §1); `StartingGuard` rollback (`maintenance_service.rs:52-59`) | мутанты rollback-веток NOT_RUN (вне этой фазы) |
|| P2 negative control (без worker pending не двигается) | production + internals | `r11_ph4b_no_worker_pending_stays.rs` → оракулы (a)–(d): `maintenance_running()==false`, release-счётчик константен, pending-биты живут, trim не потребляет | — |
|| worker visits fallback (try_with_heap) | production + internals | ⚠️ `FALLBACK_VISITS`-счётчик существует (`maintenance_service.rs:38`), но явного теста «worker обходит fallback-heap через try_with_heap и делает reclaim» не найдено | «worker reclaim на fallback-heap» — пробел (P3) |
|| maintenance vs registry lease протокол | internals + bench-internals | `r8_maintenance_registry.rs`, `loom_r8_maintenance_lease.rs`, `r9_bounded_background_maintenance.rs` (ingress-бюджет) | — |

### (д) Large / high-align

|| Вход | Feature | Тесты | Пробел |
||---|---|---|---|
|| align > 16 .. высоких значений (до >page) | alloc-core + internals | `r8_large_alignment.rs` → align = k·SEGMENT (1,2,16), alloc_zeroed нули, geometry token/root, realloc с сохранением 37-байт префикса, OOM realloc старый жив; `stress_boundary_sweep.rs` → exhaustive size×align grid (native) на AllocCore; `regression_fastbin_aligned_roundtrip.rs` → align=128 roundtrip; `size_classes_lookup.rs` → align=32/128 классификация (small vs Large) | — |
|| page_size() не литерал 4096 | production | `large_reserved_capacity.rs`, `large_cache_occupancy_bitmask_invariant.rs`, `lazy_initial_commit_page_sizes.rs`, `decomp_hooks_forced_page.rs`, `segment_meta_page_alignment.rs`, `r8_6_decommit_boundary.rs` → используют `aligned_vmem::page_size()` (см. Ph5b §C7/§Литералы 4096) | — |
|| realloc со сменой kind Small↔Large, сохранность | alloc-core | `r6_own_provenance.rs` → realloc 7→4097 (S→L) и обратно с narrow reborrow, содержимое; `regression_realloc_cross_class_shrink.rs` → L→S shrink; `regression_inplace_large_realloc.rs`, `r17_4_inplace_grown_large_dealloc_routes_by_kind.rs` → L→L in-place и kind-маршрутизация (см. также Ph5b §C2 realloc-строки) | ~~смена kind при align>16 через установленный `#[global_allocator]`~~ ✅ ЗАКРЫТО (Ph5c) `r11_ph5c_global_realloc_high_align.rs` |
|| alloc_zeroed Large — нули | production | `alloc_zeroed_fresh_large_skip.rs`, `regression_alloc_zeroed_fresh.rs` (см. Ph5b §C2) | — |
|| exact-span-large / large-reserved-capacity geometry | alloc-core + internals | `exact_span_large.rs` → span_usable < SEGMENT при feature, segment_base резолвится, cache-reuse, on/off эквивалентность; `large_reserved_capacity.rs` → reserved_capacity кратен runtime page_size() | — |
|| segment-table exhaustion под Large | alloc-core | `regression_large_align_no_segment_exhaustion.rs`, `regression_page_aligned_no_segment_exhaustion.rs`, `regression_own_thread_large_no_leak.rs`, `r14_7_max_segments_ceiling.rs` | — |

### (е) Narrow-reborrow `Box<[u8; N]>` N=1..7; Box/Vec/String через `#[global_allocator]`

|| Вход | Feature | Тесты | Пробел |
||---|---|---|---|
|| narrow-reborrow N=1..7 через real allocator | Gate/System-свидетели: `r11_box_cap_gate_{va,w1,w2,w3}.rs` + `support/r11_box_cap_gate.rs` / `r11_box_cap_reissue.rs` → narrow `&mut [u8; N]`, reissue sizes=1..7 (это C1/MODEL-LIMIT witnesses, системный Gate — не SeferAlloc, см. Ph5b C1/C4); Sefer: `r8_global_box_provenance.rs` + `support/r8_global_box_witness.rs` → установленный `SeferAlloc`, `Box<[u8;7]>`/`Tiny`, narrow borrows до move, 2 раунда reissue; `r11_w0_box_reuse.rs` → W-0 reuse (MODEL-LIMIT); `miri_global_box_acceptance.rs` → Miri SB/TB expected-red (P1-box, CI) | ~~нативный narrow N=1..6 через установленный SeferAlloc~~ ✅ ЗАКРЫТО (Ph5c) `r11_ph5c_narrow_reborrow_box_vec_string.rs` |
|| by-value Box churn через SeferAlloc | alloc-global | `r11_ph5b_c3_by_value_churn.rs` → Box<[u8]> 17/64/257/1024, by-value возврат из кадра, точный reuse, zeroing (см. Ph5b §C3 — PASS core gate) | hardened/numa/etc параметризация — нет (Ph5b: «broader feature/class proof limited») |
|| Box/Vec/String smoke через global_allocator | alloc-global | `global_alloc.rs`, `global_alloc_installed.rs`, `global_alloc_mt.rs`, `sefer_alloc_examples.rs`, `regression_r2_08_globalalloc_no_unwind.rs` (no-unwind на GlobalAlloc-пути), `stress_safe_surface_no_aliasing.rs` | — |

## Пробелы (приоритеты)

1. ~~**[P1] (б) Насыщение registry → fallback**~~ — ЗАКРЫТО `r11_ph5c_registry_saturation_fallback.rs`: насыщение реальным дренированием `dbg_claim_lease()` (internals) до `None` — без инъекции и без 4096 потоков; fallback-маршрут, кросс-потоковый dealloc fallback-блока, возврат к registry после освобождения слота.
2. ~~**[P2] (д) GlobalAlloc realloc Small↔Large при align > 16:**~~ — ЗАКРЫТО `r11_ph5c_global_realloc_high_align.rs`: align 32/64/256/1024/4096/2·`aligned_vmem::page_size()` (не литерал), S→L (размер ≥ SEGMENT = HUGE_THRESHOLD — гарантированно Large-нога) и L→S через `GlobalAlloc::realloc` (align наследуется от `old_layout`), содержимое побайтно, `alloc_zeroed` нули при высоком align; 256 KiB @ align 64 grow L→L/cross + shrink L→S; полный dealloc + round-trip.
3. **[P2] (а) Rollback-ветки `finish_bind`** (`LOCAL.try_with` Err / `GUARD.try_with` Err → drop lease, откат LOCAL в null): документированы (`tls_heap.rs:461-477`), прямого теста нет.
4. **[P2] Feature-матрица:** все тесты (а)–(г),(е) живут под {production, alloc-global} + internals/bench-internals; hardened, virgin-zero-skip, medium-classes, numa-aware — по сути пробелы для fallback/teardown/worker-входов (сборка проходит, но поведенческих клеток нет).
5. **[P3] (г) Worker reclaim на fallback-heap:** `FALLBACK_VISITS` есть, теста нет.
6. ~~**[P3] (в) Явная идемпотентность повторного `trim_current_thread`**~~ — ЗАКРЫТО `r11_ph5c_trim_idempotent_after_join.rs` (второй подряд trim — no-op release-дельта; плюс trim при живом peer, после join, с >4 МиБ живых малых блоков).
7. ~~**[P3] (е) Нативный narrow N=1..6 через установленный SeferAlloc**~~ — ЗАКРЫТО `r11_ph5c_narrow_reborrow_box_vec_string.rs` (полная вертикаль N=1..7: by-value `consume(Box<[u8;N]>)` с narrow reborrow внутри вызываемого, reissue, `alloc_zeroed`-нога с dirty-reuse оракулом; Vec 8→1024→64 KiB→5 MiB Small→Large по индексам; String UTF-8 через рост; Ph5b P3 (trim→join→reuse) остаётся из Ph5b.

## Новые тесты Ph5c

| Файл | Feature gate | Команда | Что добавляет |
|---|---|---|---|
| `tests/r11_ph5c_registry_saturation_fallback.rs` | `alloc-global` + `internals` | `cargo test --test r11_ph5c_registry_saturation_fallback --features "production internals"` (также зелёный под `--all-features`) | (б) P1: насыщение registry дренированием `dbg_claim_lease()` → alloc свежего потока через fallback (счётчик fallback-lock приращений), кросс-потоковый dealloc fallback-блока, возврат к registry после drop одного lease (приращение == 0, отрицательный оракул). Тело на отдельном потоке, без спин-wait, без утверждений об индексах слотов. Fallback-куча прогревается ДО дрена (фаза −1, `dbg_panic_in_with_heap_releases_lock`): под `numa-aware` primordial каждого `HeapCore` резервируется eager-коммитом 4 МиБ (lazy-commit отключён под `numa-aware`, `src/alloc_core/alloc_core/bootstrap.rs`), дрен materialизует тысячи закоммиченных куч и может исчерпать host commit charge — ленивая инициализация fallback после дрена тогда честно получает true-OOM null (M10 «never null when serviceable» не покрывает исчерпание commit хоста; см. `r11_ph5b_c6_fallback_oom.rs`). |
| `tests/r11_ph5c_trim_idempotent_after_join.rs` | `alloc-global` + `internals` (`#[global_allocator]` установлен) | `cargo test --test r11_ph5c_trim_idempotent_after_join --features "production internals"` | (в) P3: явная «trim; trim» идемпотентность (второй trim — release-дельта 0), trim при живом peer-потоке и после join, живые объекты (32×Box + 96 Ki×64 B ≈ 6 МиБ — гарантированно свежий Small-сегмент, не primordial) целы через все trims, round-trip после. |
| `tests/r11_ph5c_global_realloc_high_align.rs` | `alloc-global` + `internals` (`#[global_allocator]` установлен) | `cargo test --test r11_ph5c_global_realloc_high_align --features "production internals"` | (д) P2: `GlobalAlloc::realloc` S↔L при align 32..4096 и 2·page_size() (не литерал 4096): содержимое побайтно через обе смены kind, `alloc_zeroed` нули при высоком align, 256 KiB @ 64 grow L→L/cross + shrink L→S, post-dealloc round-trip. Kind-смена обеспечена выбором размеров: Small = 512 B (класс из таблицы), Large = 4 МиБ + page (≥ SEGMENT = HUGE_THRESHOLD → `class_for` = None → Large-нога; см. `src/registry/heap_core/alloc/hot.rs` — align>16 сам по себе НЕ уводит в Large). |
| `tests/r11_ph5c_narrow_reborrow_box_vec_string.rs` | `alloc-global` + `internals` (`#[global_allocator]` установлен) | `cargo test --test r11_ph5c_narrow_reborrow_box_vec_string --features "production internals"` | (е) P3: `Box<[u8; N]>` N=1..7 — by-value `#[inline(never)] consume` с narrow reborrow внутри, reissue без адресных утверждений; `alloc_zeroed`-нога (`Box::new([0;N])`, `vec![0;512]`) c dirty-reuse оракулом (alloc→0xff→free→alloc_zeroed→нули); Vec 8→1024→64 KiB→5 МиБ Small→Large с сохранностью по индексам; String UTF-8 рост; Drop-корректность + round-trip. Не дублирует miri-свидетеля C1. |

## Мутанты Ph5c (src → тест → вердикт)

| Мутант (src) | Тест | Вердикт |
|---|---|---|
| `src/global/tls_heap.rs` `finish_bind`: ветка `None` (exhaustion) `return CurrentHeap::Fallback` → `panic!` (fallback-арм удалена) | `r11_ph5c_registry_saturation_fallback` | **KILLED** (процесс красится на saturated alloc; откат — зелёный) |
| `src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs` `release_empty_current_small_for_trim`: убран guard `live_count_of() != 0` (trim может освободить Small-сегмент с живыми блоками) | `r11_ph5c_trim_idempotent_after_join` | **KILLED** (STATUS_ACCESS_VIOLATION на чтении живого блока; откат — зелёный). Замечание: первая версия теста SURVIVED — 32×Box жили в primordial-сегменте (kind != Small), усилено до >4 МиБ живых малых блоков |
| `src/registry/heap_core/free/realloc.rs` own-segment move leg: `let copy = old_layout.size().min(new_size)` → `let copy = 1` (копирование содержимого сломано) | `r11_ph5c_global_realloc_high_align` | **KILLED** (детерминированный красный: процесс тест-бинарника падает в libtest-harness (`getopts` panic) до завершения тестов — 2/2 запуска; откат — зелёный, `git diff --stat -- src` пуст). Замечание: мутант в ДРУГОЙ move-leg (строка ~418, чужой путь) SURVIVED — реальный путь для S↔L при high align — own-segment leg (~336); мутант `src/alloc_core/alloc_core/mem/mem_impl.rs` `AllocCore::alloc_zeroed` (не-zero Small) тоже SURVIVED — под `production` GlobalAlloc-путь идёт через `HeapCore::alloc_zeroed`, а не `AllocCore` |
| `src/registry/heap_core/alloc/hot.rs` `HeapCore::alloc_zeroed` Small-нога (`not(virgin-zero-skip)` arm): `Node::zero(ptr, size)` отключён (`if false`) | `r11_ph5c_narrow_reborrow_box_vec_string` | **KILLED** (`dirty-reuse vec![0; 512] nonzero` — красный; откат — зелёный). Замечание: первая версия теста (без dirty-reuse оракула: alloc_zeroed из свежих/virgin страниц) SURVIVED — усилен alloc→0xff→free→alloc_zeroed |

## Miri

Факты (локально, Windows-хост; `cargo +nightly miri`, свой target-dir,
`MIRIFLAGS` = strict-provenance (`-Zmiri-strict-provenance`) / tree-borrows (`-Zmiri-tree-borrows`)):

| Тест | SB strict | TB | Примечание |
|---|---|---|---|
| `r11_ph5c_narrow_reborrow_box_vec_string::narrow_reborrow_box_arrays` | 1/1 PASS | 1/1 PASS | только box-нога (N=1..7) |
| `r11_ph5c_global_realloc_high_align` | 2/2 PASS | 2/2 PASS | S↔L при high align |

NOT_APPLICABLE под Miri:
- `r11_ph5c_registry_saturation_fallback` — дрен 2216 слотов = материализация ~4096 куч
  + реальные TLS-потоки; вне вычислимости Miri.
- `r11_ph5c_trim_idempotent_after_join` — 6 МиБ живых Small-блоков + cross-thread join;
  Miri не воспроизводит реальный teardown-порядок.
- Vec/String-ноги narrow-теста (рост до 5 МиБ) — вне вычислимости; box-нога покрыта.

## Итоговый статус

Статусы Ph5b не пересматриваются — C2/C3/C5/C6 PASS остаётся за Ph5b-доком
(§«Итоговые статусы (после приёмки)»); Ph5c добавляет только перечисленные входы.

| Гейт | Статус | Ph5c-вклад |
|---|---|---|
| C1 | KNOWN-DEFECT (P1-box) | не тронут; Miri-свидетель `miri_global_box_acceptance` (EXPECTED RED) не менялся |
| C2 | PASS (по Ph5b) | входы Ph5c: (в) trim идемпотентность — `r11_ph5c_trim_idempotent_after_join`; (д) realloc high-align — `r11_ph5c_global_realloc_high_align`. Всё остальное — NOT_RUN вне Ph5b-покрытия |
| C3 | PASS (по Ph5b) | входы Ph5c: (е) narrow-reborrow/Box/Vec/String — `r11_ph5c_narrow_reborrow_box_vec_string` (нативный; Miri — только box-нога, см. выше) |
| C4 | MODEL-LIMIT | новых клеток нет; witnesses не запускались в этом ходе |
| C5 | PASS (по Ph5b) | Ph5c-вклада нет |
| C6 | PASS (по Ph5b) | входы Ph5c: (б) насыщение registry → fallback — `r11_ph5c_registry_saturation_fallback` (падение под `numa-aware` — commit-limit хоста, не дефект; см. warm-up фазу в таблице тестов); (а) TLS teardown fallback — покрыто ранее существовавшими тестами (см. инвентаризацию (а)), новых не требовалось; (г) worker — покрыто ранее (`r8_autonomous_maintenance` и др.) |
| C7 | Windows-хост локально | CI-матрица (Linux x86_64/aarch64 cross/QEMU, macOS arm64) — за пределами этой работы; вердикт по landing SHA |

Мутанты Ph5c: 4/4 KILLED (таблица выше). Промежуточные SURVIVED (realloc чужой
move leg, `AllocCore::alloc_zeroed` — не пути под production) вакуумными не
считались; покрытие заявлено только по убитым.

## Открытые вопросы владельцу

1. **(а) `numa-aware` + дрен registry:** под `numa-aware` primordial eager-commit
   материализует ~8.9 ГиБ commit при дренах registry (2216 слотов). Ожидаемое
   поведение — или нужен guard/документация?
2. **(б) `dbg_panic_in_with_heap_releases_lock()`** использован как warm-up
   материализации fallback-кучи до дрена (легитимный, но side-effect-паттерн) — ок ли?
3. **(в) C2/C3/C5/C6** полный статус остаётся за Ph5b-доком; Ph5c добавляет только
   перечисленные входы.

Инъекторы, доступные для закрытия пробелов: `fallback::dbg_inject_fallback_oom_for_test`,
`fallback::dbg_fallback_lock_acquisitions`, `tls_heap::dbg_teardown_then_resolve_is_fallback` /
`_is_foreign_no_bind`, `HeapCore::dbg_with_fallback_for_test`,
`MaintenanceService::fail_next_start_for_test` / `passes_for_test` / `wait_after_for_test`
(все `bench-internals`/`internals`).

## Приёмка оркестратором (zero-trust, 2026-10-05)

- `r11_ph5c_registry_saturation_fallback.rs`: под `numa-aware` дрен registry коммитит ~`MAX_HEAPS` × 4 MiB
  (eager primordial) ≈ 9 GiB commit charge — это нагрузка на машину, а не проверка маршрутизации, поэтому
  файл исключён `not(feature = "numa-aware")` (маршрут saturation → fallback → recovery от NUMA не зависит);
  warm-up через `dbg_panic_in_with_heap_releases_lock` (хук другого назначения) убран вместе с причиной.
  Вопрос владельцу №1 (eager commit под `numa-aware`) остаётся как наблюдение: поведение задокументировано
  в `bootstrap.rs`, дефектом не является.
- Там же проверка «содержимое пережило передачу» была вакуумной (`as_ref().is_some()`); теперь побайтно
  сверяет паттерн `0xA5` перед межпоточным free.
