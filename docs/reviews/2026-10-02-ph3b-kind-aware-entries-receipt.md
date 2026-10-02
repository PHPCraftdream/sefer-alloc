# Ph3b: kind-aware входы выдачи/освобождения — приёмочная записка
Дата: 2026-10-02. База: `3962933b` (HEAD ph3b-kind, чистое дерево до правок). План: §4 шаг «3b». ADR: 2026-10-01-adr-physical-boundary-and-progress.md.

## 1. Карта (шаг 1) — кратко
Инвентарь источников физического вида блока: `SegmentHeader::kind_at` (авторитетный, segment_header_views.rs), RouteDirectory `RouteKind` (авторитетный для foreign), sidecar `ClassLeaves` (авторитетный класс для cross-thread reclaim), `SizeClasses::class_for` (запрос, не авторитет). Единого определителя не существовало.
- alloc / alloc_zeroed / alloc batch / fallback alloc: kind = class_for(Layout) — легитимно (блока ещё нет); дубли: Large-рука копирована трижды (hot.rs:719, realloc.rs:544, batch.rs:266); не-fastbin batch отбрасывал предвычисленный класс.
- dealloc own: class_for(Layout) ПЕРВЫМ, kind_at вторым и под cfg (dealloc_own_base.rs) — инверсия порядка относительно субстрата (mem_impl.rs:260 читает kind_at первым).
- F7-A: «Large?» = `(hardened || layout.size() >= 256 KiB) && kind_at==Large` — порог по Layout подчинял заголовок; под plain production обе F7-ветки не компилировались → Large-блок со small-Layout (маскированный/ошибочный вызов) шёл в magazine-push как small.
- dealloc foreign: pin.kind() из маршрута (авторитетно); fallback-owned при свободном локе — Layout-класс, при занятом — sidecar-класс.
- dealloc_batch: один Layout на весь батч; RouteToLargeFree → self.dealloc (повторный routing) vs scalar → core.dealloc.
- realloc: in-place (realloc_fastpath.rs:281) читает kind_at, промо-гейт (realloc.rs:261) заново выводит из old_layout; move-leg — третья деривация.
Наблюдатели: A1–A7, B1–B4, C1–C2, D1–D5 (полная карта в истории сессии; doc-drift: large_layout_consistent, dealloc_foreign_routing, realloc.rs:430 — вне зоны, не правлены).

## 2. Изменено и почему
- НОВЫЙ src/alloc_core/segment/segment_header/block_kind.rs: один экспорт `pub(crate) enum BlockKind { Primordial, Small{class_idx}, Large, Unknown }` + `BlockKind::of(base, class: Option<usize>)` — решает ТОЛЬКО по `SegmentHeader::kind_at(base)`; class (из caller Layout) — только payload для Small-ветки, `None` при Small-заголовке → Unknown (тот же отказ, что прежний classify-путь). `#[cfg_attr(not(debug_assertions), inline(always))]`. Реэкспорт в segment_header/mod.rs.
- dealloc_own_base.rs: F7 A/B через `matches!(BlockKind::of(...), Large)`; порог-прокси из решения УДАЛЁН; добавлен pre-route (mutually-exclusive cfg: not hardened && not promotion) — под plain production физический Large со small-Layout теперь маршрутизируется в `core.dealloc` тем же путём (раньше — magazine-push; фикс A1/A2); kind_at читается ровно один раз в любой конфигурации.
- dealloc_batch.rs: RouteToLargeFree → `self.core.dealloc` (как scalar; фикс B1 — путь/дедуп, исход идентичен).
- realloc.rs: промо-гейт по BlockKind вместо class_for(old_layout) (фикс A3).
- mem_impl.rs: рука Small|Primordial dealloc_at_base через BlockKind (дедуп classify).
- alloc/batch.rs (не-fastbin): `alloc_with_class(layout, class)` вместо повторной классификации (A6/D1).
- routing.rs (foreign): кросс-чек pin.kind() vs kind_at НЕ добавлен (RoutePin не экспонирует root; потребовал бы новый аксессор/unsafe — вне зоны).
Diff: `git diff | sha256sum` = `93de18622a37e52b32893b2dbd0f69aa4b58a715e3afe2eeaf2a06c62b5cbe43`. Файлы: block_kind.rs (новый), segment_header/mod.rs, mem_impl.rs, alloc/batch.rs, free/dealloc_batch.rs, free/dealloc_own_base.rs, free/realloc.rs.

## 3. Тесты
Новых tests/*.rs: 1 — tests/r11_ph3b_kind_aware_entries.rs (7 тестов, features: production+internals+bench-internals; наблюдатели только числовые dbg_*: dbg_kind_at_tag, dbg_active_kind_census, dbg_owner_id_for, dbg_class_for, dbg_table_count, dbg_tcache_*, dbg_live_count_for и т.п.; ключевой тест promoted_then_grown_large_block_routes_its_free_by_kind_not_by_layout под medium-classes: promoted+OPT-G grown 512 KiB (Layout классифицирует Small) обязан освободиться по kind и дерегистрировать сегмент).
ЗЕЛЁНЫЕ прогоны (PASS, native):
- production internals bench-internals: 27 наборов (r11_ph3b_kind_aware_entries(5..7), r6_route_directory, r6_route_lifecycle_{basic,oom,fallback}, r6_class_issue, r8_global_box_provenance, r8_standalone_alloc_core, r8_large_alignment, r11_ph3a_issue_transaction_{scalar,batch}, r11_p3_small_sidecar_{preflight,issue_oom}, r11_4_dealloc_batch_hardened_guards, heap_core_{tcache,bulk_bypass}, batch_tcache, batch_large_deferred_reclaim, global_alloc_installed, regression_{batch_freelist_drain,carve_batch,r1_01_magazine_free_budget,bump_direct_refill,dealloc_batch_promoted_large_free}, r17_4_inplace_grown_large_dealloc_routes_by_kind, r14_4_promotion_free_correctness, regression_hardened_large_kind_own_free, r1_04_alloc_core_drop_stack_pressure) — 0 failed (часть наборов 0 тестов под данным feature-набором: r6_route_directory, r8_*, r11_4, regression_dealloc_batch_promoted_large_free гейтуются фичами).
- medium-classes (+exact-span-large): r11_ph3b — 6 passed; hardened: 5 passed; release-профиль production: 5 passed; production+medium-classes+batch-api: 7 passed.
- alloc-global internals bench-internals: 9 наборов (включая r11_ph3b, r11_ph3a_magazine_refill_partial, r6_route_lifecycle_oom, heap_core_*, batch_tcache, global_alloc_installed, freelist_reuse, regression_batch_freelist_drain) — 0 failed.
- alloc-core internals (no-default-features): alloc_core_batch(14), alloc_core_invariants(13), alloc_core_reentrancy, freelist_reuse, regression_carve_batch(6), regression_batch_freelist_drain(3) — 0 failed.
- clippy --all-targets --features "production internals bench-internals" -- -D warnings: чисто.
- rustfmt --check --edition 2021 (все изменённые src + новый тест): чисто.
- Miri SB: cargo +nightly miri test --features "production internals bench-internals" -j 1 --test miri_global_box_acceptance -- paused — ожидаемо КРАСНЫЙ ровно в прежнем месте: UB в Node::write_next (src/alloc_core/platform/node.rs:90, через reclaim_sidecar_record → drain_sidecar_ingress → trim_current_thread). Не ухудшен. TB и Ph3c — NOT_RUN.

## 4. Мутанты (временные правки src; откат по sha256; база/после `93de1862…`)
| мутант | суть | фич-набор | тест | результат |
|---|---|---|---|---|
| (a) kind из Layout | F7-A: решение без BlockKind (`if false && matches!(BlockKind::of...)`) — kind не консультируется | production internals bench-internals medium-classes | r11_ph3b_kind_aware_entries::promoted_then_grown_large_block_routes_its_free_by_kind_not_by_layout | **КРАСНЫЙ** — паника «PH3B PRE-ROUTE BROKEN … must reach the substrate Large free» (tests/r11_ph3b_kind_aware_entries.rs:1052) |
| (a′) порог-прокси (прежний до-Ph3b вид) | `(hardened \|\| size>=THRESHOLD) && BlockKind==Large` | тот же | тот же | зелёный (тест освобождает grown-layout 512 KiB ≥ порога — порог-мутант этим тестом не ловится; ловится только полный отказ от kind) |
| (b) batch иначе | RouteToLargeFree → self.dealloc (повторный routing) | production internals bench-internals medium-classes batch-api | r11_ph3b (7 тестов, вкл. scalar_and_batch и batch-twin promoted) | зелёный — повторный маршрут физически идентичен (тот же substrate free; наблюдаемый исход/счётчики равны) |
| (c) realloc по новому/Layout | промо-гейт → class_for(old_layout).is_some() | production internals bench-internals medium-classes | r11_ph3b + r14_4_promotion_* + regression_realloc_cross_class_shrink | зелёный — OPT-G in-place отрабатывает раньше гейта; промоция уже-Large блока даёт корректный исход (доп. Large-churn), нативно не наблюдаемо |
Все мутанты откатаны: `git diff | sha256sum` = `93de1862…`; тесты после откатов зелёные.

## 5. Perf (iai, WSL+callgrind, приватные CARGO_TARGET_DIR до/после: /tmp/sefer-iai-ph3b-{before,after}; `.d` обоих артефактов содержит CARGO_MANIFEST_DIR=…/worktrees/ph3b-kind; полный набор 85 бенчей на плечо; до снят на чистом HEAD)
| бенч | Ir до→после | ΔIr | EstCycles до→после | ΔCycles | порог | вердикт |
|---|---|---|---|---|---|---|
| small_churn_16b (hot) | 60270→60410 | +0.232% | 146739→147035 | +0.202% | ≤ +2% | PASS |
| dealloc_free_only_16b (dealloc) | 81717→82039 | +0.394% | 175799→176482 | +0.388% | — | PASS |
| realloc_grow (realloc Ir) | 609986→609952 | −0.006% | 3737322→3737307 | −0.000% | ≤ +5% Ir | PASS |
| cold_alloc_free_256x16b (refill) | 192658→193877 | +0.633% | 325089→327389 | +0.707% | ≤ +5% | PASS |
| recycle_alloc_free_256x16b (refill) | 299336→301163 | +0.610% | 464263→466772 | +0.540% | ≤ +5% | PASS |
Причина дельт: +1 чтение kind_at на free-путях под production (pre-route), ожидаемо и в пределах порогов. Сырые логи: docs/perf/_raw_ph3b_iai_before.log / _after.log (по 44 011 B).

## 6. Что НЕ подтверждено / за пределами
- README.md / docs/ARCHITECTURE.md: счётчик тест-файлов (нужен бамп интегратора; новые корневые тест-файлы +1).
- Мутанты (b)/(c) нативно не красные: расхождения B1/A3 — гигиена/дедуп с идентичным наблюдаемым исходом; их защитная ценность покрывается единым BlockKind, а не красным тестом (честно зафиксировано).
- Кросс-чек pin.kind() vs kind_at в foreign-пути не добавлен (нет доступа к root без нового API) — кандидат в отдельный шаг.
- TB-набор Miri, Loom, larson/mstress, bench:table, RSS — NOT_RUN. iai до/после — по одному прогону (Ir детерминирован; ADR-протокол ≥5 прогонов не применялся).

## 7. Вердикт
GO: единый приватный определитель BlockKind введён, свобод-входы (scalar/batch/realloc/substrate) переведены минимальным diff-ом, найденное production-расхождение (Large-блок со small-Layout под plain production шёл в magazine-push) закрыто и покрыто контрфактическим красным мутантом (a), hot/refill/realloc/dealloc перф в порогах, Miri-красный не сдвинулся.
