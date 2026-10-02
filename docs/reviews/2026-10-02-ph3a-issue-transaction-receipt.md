# Ph3a: выдача Small-блока как приватная транзакция «prepare → commit» — приёмочная записка

Дата: 2026-10-02. База: `7d80d7e1fb6127b7747831ee61738b948a9b32f9` (HEAD, чистое дерево до правок). План: docs/design/2026-10-01-113842-src-foundational-refactoring-plan-xs-sol.md §4 шаг «3a».

## 1. Карта входов выдачи (шаг 1)

| вход | фаллибл-ресурсы | мутации | порядок корректен? |
|---|---|---|---|
| scalar pop (`pop_free`) | prepare sidecar (leaf/route) | [set_head, publish_empty, mark_alloc, inc_live, issue, bump_gen(hardened)] | корректен; `Err(())` — до любой мутации |
| scalar carve (`carve_block`) | prepare → recommit/commit_pages | [bump, live, page-map, issue] | корректен; отказ commit_pages/OOM — до bump |
| batch drain (`try_drain_freelist_batch`) | prepare всей цепочки | [set_head×1, publish_empty, mark_alloc per-block, issue per-block, add_live(k)] | корректен |
| batch carve (`try_carve_batch`) | prepare всех n | [commit_pages×1, bump, add_live(n), page-map, issue per-block] | корректен |
| alloc_zeroed/virgin (`alloc_small_with_virgin`) | те же входы | zeroing — после выдачи, нефаллибл | корректен |
| promotion/reserve (`reserve_small_segment[_impl]`) | OS reserve → register (отказ → release reservation, состояние не тронуто) → init header/bintable/bitmaps → maybe_materialize_directory | — | корректен |
| refill через magazine (`refill_class_bump_impl`) | каскад try_drain → find_segment+try_drain → try_carve → reserve → try_carve; на `Err` возвращает `filled` | каждая подтранзакция атомарна → частичный результат согласован | корректен |

Вывод: фаллибл-шаг после мутации НЕ обнаружен ни на одном входе → применён вариант шага 3 плана (минимальный diff, без изменения алгоритмов).

## 2. Изменено и почему

Новый файл `src/alloc_core/segment/segment_table/issue_transaction.rs` (один экспорт `pub(crate) struct IssueTransaction`, non-Copy, safe; свидетелей prepare; `commit(self, table)` и batch-близнец `commit_prepared` с abort-on-violation, унаследованным дословно от прежнего inline-пути). Типизация существующего протокола `prepare_small_issue -> Option<IssueTransaction>`; поведение, порядок вызовов, cfg-гейты не менялись.

Diff: `git diff | sha256sum` = `775f5cd25fe2ae0034cf502a609bec1a388e42acce244d842eaf5f966829e14a`.

Файлы: модифицированы `segment_table/mod.rs`, `route_slots.rs`, `segment_table_impl.rs`, `small/alloc_core_small/alloc_core_small_impl.rs`; созданы `issue_transaction.rs` + 4 теста `tests/r11_ph3a_*.rs`.

Хоты с `#[inline(always)]` (подобрано по iai, см. §5: первая версия без inline давала +4.7..6% Ir на cold/recycle; inline вернул ниже базы).

## 3. Тесты (PASS, если не указано)

- production internals bench-internals (native): r6_route_lifecycle_oom, r8_global_box_provenance, r11_p3_small_sidecar_{preflight,issue_oom,zeroed_oom}, r11_ph3a_{issue_transaction_scalar,issue_transaction_batch,magazine_refill_partial,reserve_promotion_fault}, regression_batch_freelist_drain/batch_flush/carve_batch/magazine_*(4)/r1_01/bump_direct_refill, freelist_reuse, heap_core_tcache/bulk_bypass/dbg_table_count, global_alloc_installed — все зелёные.
- alloc-global internals bench-internals: r11_ph3a_magazine_refill_partial, r11_p3_small_sidecar_*, r6_route_lifecycle_oom, heap_core_*, global_alloc_installed, freelist_reuse, regression_batch_freelist_drain — зелёные (fastbin-нога refill-partial прогонялась под production; без fastbin отказ surfaces как scalar-OOM с полным откатом — задокументировано в тесте).
- alloc-core internals (no-default-features): alloc_core_batch(14), alloc_core_invariants(13), alloc_core_reentrancy, freelist_reuse, regression_carve_batch(6), regression_batch_freelist_drain(3) — зелёные.
- `cargo clippy --all-targets --features "production internals bench-internals" -- -D warnings` — чисто (в т.ч. release-check после inline-правки).
- `rustfmt --check --edition 2021` по всем изменённым файлам — чисто.
- НОВЫЕ тесты покрывают: отказ на prepare-границе каждого входа (scalar pop/carve, batch drain/carve, magazine refill, promotion после register) → null/частичный batch; независимый учёт бит-в-бит (bump/live/head/frontier/directory-бит/system_totals/alloc-бит/census/table) равен до-состоянию либо числу реально выданных; повторный вызов после снятия отказа успешен; zeroed ровно на requested extent; частичные batch (k=100 при out=260; n=room) с точным credit.

FAIL (известный, вне зоны Ph3a): `no_stale_doc_references::{architecture_test_file_count_matches_reality, verification_inventory_matches_docs}` — ждут токен «327 files» в docs/ARCHITECTURE.md и README.md (новых корневых тест-файлов 323→327); оба файла вне зоны задачи — нужен доковый бамп интегратора (+4 в 3 местах).

## 4. Мутанты (временные правки src, откат по sha256; до/после `ced3d2d1763c3026ba92323399ce00c296e9713a467d8db9b060dab54df22095`)

| мутант | тест | результат |
|---|---|---|
| (a) prepare после `bt.set_head` (фаллибл после мутации) | r11_p3_small_sidecar_preflight::scalar_pop; r11_ph3a_issue_transaction_scalar | КРАСНЫЙ / КРАСНЫЙ |
| (b) `meta.inc_live()` на Err-пути (пропущен rollback credit) | те же | КРАСНЫЙ / КРАСНЫЙ |
| (c) `add_live(out.len())` вместо `add_live(k)` (partial без согласования) | r11_ph3a_issue_transaction_batch (частичный drain) | КРАСНЫЙ (live +260 вместо +100) |

## 5. Perf (iai, node scripts/iai.mjs, callgrind, WSL; база — чистый HEAD, тот же судья)

| бенч | Ir base→after | ΔIr | EstCycles base→after | ΔCycles | порог | вердикт |
|---|---|---|---|---|---|---|
| small_churn_16b (hot) | 60484→60270 | −0.354% | 147298→146743 | −0.377% | ≤ +2% | PASS |
| cold_alloc_free_256x16b (refill) | 196322→192658 | −1.866% | 331319→325093 | −1.880% | ≤ +5% | PASS |
| recycle_alloc_free_256x16b (refill) | 307095→299336 | −2.526% | 477004→464267 | −2.670% | ≤ +5% | PASS |

Сырые логи: docs/perf/_raw_ph3a_iai_before.log, docs/perf/_raw_ph3a_iai_after.log. Свежесть сборки: `.d` bench-бинарника /tmp/sefer-iai содержит CARGO_MANIFEST_DIR=.../worktrees/ph3a-issue (ловушка общего target dir учтена; после правок числа изменились → пересборка подтверждена). Примечание: iai-плечи «не измеряют» резерв/other axes ADR — только эти три руки.

## 6. Miri

`cargo +nightly miri test --features "production internals bench-internals" -j 1 --test miri_global_box_acceptance -- paused` (SB, host): ожидаемо красный, exit 1, ровно в прежнем месте — UB в `Node::write_next` (src/alloc_core/platform/node.rs:90, вызван из `reclaim_sidecar_record` → `drain_sidecar_ingress` → `trim_current_thread`). Не ухудшен. TB-нога и P1-закрытие — зона Ph3c, NOT_RUN здесь.

## 7. Что НЕ подтверждено / за пределами

- README.md и docs/ARCHITECTURE.md требуют докового бампа счётчика тест-файлов до 327 (зона интегратора; 2 meta-теста до тех пор красные).
- TB-набор Miri, r11_pg2_model_limit клетки, Loom, мультипоточные оси ADR (larson/mstress), bench:table, RSS — NOT_RUN.
- Отказ register при перегрузке таблицы покрыт r6 (HeapRegistry-уровень); отдельного dbg-хука reserve-отказа в AllocCore нет — зафиксировано в тесте.
- iai «До» снято одним прогоном (Ir детерминирован); формальный ADR-протокол ≥5 прогонов не применялся (perf-страж задачи, не гейт ADR).
