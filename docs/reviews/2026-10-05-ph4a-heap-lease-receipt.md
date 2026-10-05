# Ph4a — HeapLease (чанк C: мутанты, Miri, receipt)

Дата: 2026-10-05 · ветка `ph4a` · task #2091 · чанки A+B (уже в дереве, не коммичены)

## Что сделано

1. **Мутанты M1–M5** — каждый применён временно, пойман красным ловцом, откат вручную, ловец снова зелёный. В финальном дереве мутантов нет.
2. **Miri** — новый `tests/r11_ph4a_lease_miri.rs` (claim → drop → claim через границу потоков), SB + strict-provenance проходы, оба зелёные.
3. **README/ARCHITECTURE** — счётчики integration test files 330 → 331 (оба токена); loom-инвентарь не менялся (файл уже учтён чанками A+B).
4. **Grep-проверка** вызовов обёрток (ниже) — новый код обёртки не вызывает.

## Карта путей claim/recycle → lease

| Путь | Механика |
|---|---|
| `HeapRegistry::claim` / `claim_with_config` | → `claim_impl` → `HeapLease` → `into_raw` (Drop подавлен `forget`) → raw `*mut HeapCore` в TLS (`tls_heap.rs`) — legacy путь |
| `HeapLease::drop` | CAS `LIVE → FREE` (Release), при проигрыше — abort; затем reuse_hint + publish_claimable |
| `dbg_claim_lease()` | safe, internals-only, возвращает живой `HeapLease` (тест-крюк) |
| `try_maintenance` → `MaintenanceLease` | прежний путь; HeapLease не участвует — так задумано: maintenance-lease уже типизирован |
| `recycle` (unsafe) | legacy, без lease: CAS `LIVE → FREE` (Release), при проигрыше тихий no-op |

## SAFETY-доказательство S1–S4 (addendum §2.1, для `HeapLease::core` / `into_raw`)

- **S1 (эксклюзивность):** lease существует только как победитель предоставляющего CAS (EMPTY→INITIALIZING→LIVE — первый claim; FREE→LIVE — re-claim). Владельцы `&mut HeapCore` — ровно {победитель INITIALIZING, lease, MaintenanceLease}; власть предыдущего держателя закончилась на проигранном им CAS.
- **S2 (happens-before):** предыдущий владелец публикует записи Release-CAS-ом `LIVE → FREE` (lease-Drop или `recycle`); наш выигравший Acquire-CAS образует пару.
- **S3 (нет утечки raw-указателя):** единственный legacy-наследник — TLS-кэш того же потока через `into_raw` (`forget` до Drop; слот остаётся LIVE, власть уходит в указатель). `core(&mut self)` — эксклюзивный borrow, алиасов не оставляет.
- **S4 (валидность слота):** `&'static HeapSlot` — чанки реестра не освобождаются всё время процесса; lease создаётся только после публикации/наблюдения `initialised == true`.

## Мутанты (таблица)

| Мутант | Ловец | Красный? | Характер падения |
|---|---|---|---|
| M1: `Ordering::Release`→`Relaxed` в Drop-CAS | loom `tests/loom_r11_ph4a_heap_lease.rs` (`DROP_ORDERING` = Relaxed) | ДА | loom «Causality violation: Concurrent write accesses to UnsafeCell» — потерян HB-эдж owner-payload → claimant |
| M2: drop/вход по проигранному CAS (guard убран) | loom shadow-модель (убран `return` при проигрыше claim-CAS) | ДА | проигравший «дропает» слот, которым не владеет → CAS `LIVE→FREE` в дропе проваливается (abort-аналог) |
| M3: `core()` после Drop | loom shadow-модель (второй payload-доступ после FREE-публикации + снят S1 active-guard) | ДА | loom «Causality violation: Concurrent write accesses to UnsafeCell» (use-after-drop гонка со следующим claimant) |
| M4: claim без гейта `initialised` (всегда re-claim) | `regression_claim_oom_initialised_gate` | ДА | STATUS_ACCESS_VIOLATION (0xc0000005): re-claim-ветка разыменовывает непроинициализированный `slot.heap` |
| M5: `recycle` — CAS безусловным успехом | loom shadow-модель (stale recycler безусловно публикует FREE) | ДА | монопольная власть ломается → authority-CAS maintenance-дропа проваливается (abort-аналог). Нативный `registry_basic::double_recycle_is_safe_noop` НЕ краснеет: проваленный CAS в корректном коде защищает только от повторной публикации, а повторный recycle уже-FREE слота семантически эквивалентен no-op (эффекты hint/claimable — advisory, claimant всё равно требует выигрышный CAS). Нативный сценарий «recycle LIVE-слота по stale-указателю» — нарушение Safety-контракта (`recycle` проверяет state, не владельца) → из tests/ безопасно невоспроизводим; BLOCKED, ловец в лун-модели |

После каждого открата ловец прогнан повторно — зелёный (loom-базовый PASS, `regression_claim_oom_initialised_gate`/`registry_basic` зелёные).

Примечание: временный нативный тест-ловец для M2/M5 (`tests/r11_ph4a_lease_drop_invariants.rs`) был написан, показал красный и на корректном коде (сценарий вне контракта) и был удалён; `git status` его не содержит.

## Miri

- Toolchain: `nightly-x86_64-pc-windows-msvc`, miri 0.1.0 (3659db0d3e 2026-07-05).
- Тест: `tests/r11_ph4a_lease_miri.rs::lease_claim_drop_reclaim_across_threads_under_miri` — warm-claim → thread A claim/drop → join → re-claim на главном потоке (claim→drop→claim через границу потоков; `core()` из tests по safe-API недостижим и lease `!Send`).
- **pass 1 (strict provenance):** `MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly-x86_64-pc-windows-msvc miri test --features "alloc-global alloc-xthread internals" --test r11_ph4a_lease_miri` — ok (7.94s).
- **pass 2 (SB, default):** та же команда без MIRIFLAGS — ok (7.94s).
- Лог: `docs/perf/_raw_ph4a_miri_lease.log` (1.1 KiB). Feature-набор — в стиле ci.yml miri-plain job (без production/global_allocator).

## Полный регресс (последовательно)

| Набор | Лог | Итог |
|---|---|---|
| `cargo test --features "production internals"` | `docs/perf/_raw_ph4a_cargo_test_prod_internals.log` (107 KiB) | 322 бинарника, **0 failed** |
| `cargo test --features "production alloc-stats bench-internals internals"` | `docs/perf/_raw_ph4a_cargo_test_prod_allocstats.log` (116 KiB) | 324 результата, **0 failed** |

Допустимые исключения `r1_04*` и `r6_route_directory_model` на этом прогоне НЕ проявились: оба бинарника зелёные в prod-internals-логе (r1_04: 2 passed + 1 ignored-в-subprocess; r6_route_directory_model: 3 passed). Исключения r1_04_alloc_core_drop_stack_pressure и r6_route_directory_model перепроверены ОРКЕСТРАТОРОМ на базе main HEAD `18d76472` (чистый основной checkout, cargo test --features "production internals", CARGO_TARGET_DIR target-ph4a-base): r1_04 — 2 passed + 1 ignored, r6_route_directory_model — 3 passed. Оба теста зелёные и на базе (main 18d76472), и в ph4a — красное состояние из постановки задачи не воспроизводится; следовательно это не действующие исключения на текущем коде.

## Grep-проверка: вызовы обёрток из src/

```
grep -rn "HeapRegistry::claim\|claim_with_config\|HeapRegistry::recycle\|try_maintenance" src/
```
Реальные вызовы кода (не doc-комментарии, не определения):
- `src/global/tls_heap.rs:216` — `unsafe { HeapRegistry::recycle(heap) }`
- `src/global/tls_heap.rs:406` — `HeapRegistry::claim()`
- `src/global/tls_heap.rs:418` — `HeapRegistry::claim_with_config(config)`
- `src/global/tls_heap.rs:483` — `unsafe { HeapRegistry::recycle(heap) }`
- `src/global/tls_heap.rs:491` — `unsafe { HeapRegistry::recycle(heap) }`
- `src/registry/heap_registry/maintenance.rs:31` — `Self::try_maintenance_at(index)`

Прочие совпадения — определения (`claim.rs:53/55/60/132`) и doc-комментарии. Новый код обёртки не вызывает; `HeapLease` упоминается только в `claim.rs`/`mod.rs`.

## Выбор экспозиции HeapLease

`#[doc(hidden)] pub` под `pub(crate) mod registry` (lib.rs:487), реэкспорт под `internals` в `heap_registry/mod.rs`. Альтернатива `cfg_attr`-переключатель видимости не компилируется: `cfg_attr` не может нести visibility. Семантика `pub(crate)` без `internals` сохраняется автоматически (модуль `pub` только под `internals`).

## Идентичность дерева

`git diff | sha256sum`:
```
c6a67365663bfd9febf3d3edbd72988896cf14ed03818cf01935ea06648085e4  -
```
(хеш над `git diff` отслеживаемых файлов; новые untracked-файлы — 3 теста и этот receipt — в diff не входят)

## Счётчики README/ARCHITECTURE (старое → новое)

- README.md:1373 `**330 integration test files**` → `**331 integration test files**`
- README.md:1383 `tests/*.rs (330 files)` → `tests/*.rs (331 files)`
- docs/ARCHITECTURE.md:489 `tests/*.rs (330 files)` → `tests/*.rs (331 files)`
- Loom-инвентарь: без изменений (файл `loom_r11_ph4a_heap_lease` уже учтён чанками A+B, «12 root Loom models»).

`--test no_stale_doc_references` — зелёный (33 passed).

## Открытые вопросы

1. **Нативный детерминированный HB-ловец для M1 невозможен** из `tests/` (наблюдаемая поверхность lease — только generation/state; `core()` недостижим по safe-API) — честный ловец в лун-модели (`DROP_ORDERING`).
2. **M2/M5 не имеют безопасных нативных ловцов** — оба сценария лежат вне Safety-контракта; ловцы в лун-модели. Хуков в src не добавляли.
3. **Miri TLS-деструкторы не покрыты**: teardown TLS-кэша с lease-дропом в деструкторе потока — вне этого чанка (потенциально Ph4c вместе с миграцией tls_heap на lease).
4. **Legacy `recycle`/`MaintenanceLease::with_core` остаются raw-pointer путями** — долг Ph4b/Ph4c (миграция tls_heap на lease).
5. Исключения r1_04_alloc_core_drop_stack_pressure и r6_route_directory_model перепроверены ОРКЕСТРАТОРОМ на базе main HEAD `18d76472` (чистый основной checkout, cargo test --features "production internals", CARGO_TARGET_DIR target-ph4a-base): r1_04 — 2 passed + 1 ignored, r6_route_directory_model — 3 passed. Оба теста зелёные и на базе (main 18d76472), и в ph4a — красное состояние из постановки задачи не воспроизводится; следовательно это не действующие исключения на текущем коде.

## Приёмка оркестратора (2026-10-05)

- Весь diff `src` прочитан; новые и registry-тесты перегнаны независимо — зелёные; Loom `loom_r11_ph4a_heap_lease` зелёный;
  Miri `r11_ph4a_lease_miri` зелёный в SB (strict-provenance) и TB.
- Замечание: Loom-тест — shadow-модель (реальный `HeapLease` под loom не запускается), поэтому мутант `Relaxed` в
  production-Drop его не краснит (мутант M1 выше — флип `DROP_ORDERING` внутри модели). Пробел закрыт тестом
  `tests/r11_ph4a_drop_ordering_pinned.rs`: он читает исходник и требует `Ordering::Release` у CAS `LIVE→FREE` в
  `HeapLease::drop`; контрфактуально красный на мутанте `Relaxed` (проверено), зелёный на коде.
- Отклонение от решения владельца: `HeapLease` объявлен `#[doc(hidden)] pub` (не `pub(crate)`) ради наблюдателей из `tests/`;
  в сборке без `internals` модуль `registry` крейт-приватный, `core()` остаётся `pub(crate)`. Сужение — Ph4c.
