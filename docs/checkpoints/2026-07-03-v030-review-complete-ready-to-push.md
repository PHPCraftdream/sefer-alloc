# Checkpoint — 2026-07-03 v0.3.0 review complete, ready to push

## Session summary

Продолжение арки sefer-alloc 0.3.0. Сессия началась с `/resume` чекпойнта
`2026-07-02-v030-4agent-review.md`, где 4-агентное ревью нашло 2 memory-safety
блокера (#129/#130) и ~30 находок, блокирующих публикацию 0.3.0. За сессию
**решён ВЕСЬ backlog #128–#143** (стратегия: последовательные под-агенты @sx +
личный zero-trust review + counterfactual каждого фикса + коммит между
фазами). Ключевое: расширение miri/loom-охвата вскрыло **два НОВЫХ реальных
бага, которых в исходном ревью не было** — #142 (aliasing-UB в flagship
cross-thread A1-пути, падал под Stacked И Tree Borrows; фикс — exposed-
provenance) и #143 (реальная утечка в `push_large_deferred_free`: claim-CAS
был внутри head-CAS loop → терял узел на retry; найден loom-моделью #141,
подтверждён 2M-итерационным repro). Затем по просьбе — полное ревью 0.3.0
(`/fxx`), которое нашло ещё 1 leak-баг (#138-митигация сравнивала сырой
`layout.size()` вместо MIN_BLOCK-clamped → over-reject легитимных tiny-фри →
лик) + release-гигиену (свернул CHANGELOG в единый `[0.3.0]`, дописал
#140/#142/#143, поправил loom.mjs и ALLOC_BENCH стейл-доки). Наконец — перф:
перепрогнал criterion-бенчи на финальном дереве, вывел таблицы, обновил
README + ALLOC_BENCH.md свежими 0.3.0-числами (large 13–34× над mimalloc,
small-churn обгоняет mimalloc кроме 256B, cold-tiny — worst-case).

**Текущее состояние: ВСЁ закрыто, дерево чистое, 18 коммитов впереди
origin/main, НЕ запушено, НЕ тегировано, 0.3.0 НЕ опубликован.** babysit снят
(TaskList пуст → self-delete отработал вручную). Свежая полная верификация
финального дерева зелёная по ВСЕМ осям: production suite 92 бинаря 0 FAILED,
clippy -D warnings 0 (production + --all-features), rustdoc 0 (default +
production), fmt чисто, npm run loom PASS (10 моделей вкл. 2 новые), npm run
tsan PASS (WSL, 4 cross-thread, 0 data races), miri (registry + A1 оба фейса
под Stacked+Tree Borrows + heap_cross_thread) зелёный.

Единственный честный внешний хвост: perf-gate (#128) — enforcing-механика
поставлена, но реальные числа/срабатывание порога подтвердит первый Linux-CI
прогон (Valgrind только Linux). Оставшийся untracked-файл — только предыдущий
чекпойнт (намеренно, решать при пуше).

## Active goal

none (стоп-хук "реши все задачи. Все вопросы решай в сторону совершенства"
был выполнен — все задачи закрыты; хук авто-снялся).

## TaskList

Пусто — все задачи completed. Полный список закрытого за сессию:

### completed (эта сессия, #128–#143 + новые)
- #129 🔴 BLOCKER tls_heap TORN-сентинел (stale-LOCAL double-writer teardown)
- #130 🔴 BLOCKER alloc_large reject align≥SEGMENT (лик→abort / misalign UB)
- #131 🟠 ensure_slow OOM rollback+abort (livelock)
- #132 🟠 Heap A1-parity + with_heap no-panic (извлечён alloc_core::deferred_large)
- #133 🟠 per-heap счётчики (убран contended lock xadd) + UB-фикс агрегации (initialised-гейт)
- #134 🟠 span_usable в header (large-cache RSS-амплификация)
- #135 🟡 O(1) SegmentTable register/unregister/recycle (free-list + segment_id) + realloc O(1) + M2 hardening
- #136 🟡 API-полиш (SIZE_CLASS_TABLE→slice, LargeCacheMode non_exhaustive, budget_bytes(0)=disabled, rustdoc 44→0, docs.rs=production)
- #137 🟡 CI: fastbin/production в test-матрице + --no-fail-fast + loom_fallback_init
- #138 🟢 A1 post-reuse митигация (layout_consistent) + loom-аудит + README/INTEGRATION/CHANGELOG точность
- #139 🟡 miri zero-init registry (cfg(miri) write_bytes) — разблокировал miri-валидацию registry
- #140 🟢 exposed-provenance registry (without_provenance sentinel + expose/with_exposed пары); strict-provenance структурно недостижим — задокументировано
- #141 🟢 loom-модели: loom_deferred_large (нашла #143) + loom_free_slots_aba; loom-drift honesty-docstrings
- #142 🔴 NEW aliasing-UB cross-thread thread_free (Stacked+Tree Borrows) — exposed-provenance фикс, miri обе модели зелёные
- #143 🔴 NEW push_large_deferred_free leak (claim-CAS в loop) — hoist claim once; loom + miri verified
- #128 🟢 perf-gate enforcing scaffold (baseline cache + save/compare + IAI_CALLGRIND_REGRESSION); валидация на первом Linux-CI

## Decisions

- **#132: извлечь A1-примитив в общий модуль, НЕ дублировать** — HeapCore
  связан с реестром, полный collapse Heap→HeapCore рискован под hardening;
  унифицировал гарантии через shared alloc_core::deferred_large.
- **#140: strict-provenance НЕ гнать до зелёного** — cross-allocation
  intrusive-стеки структурно несовместимы (сегменты из разных OS-резерваций,
  provenance нельзя упаковать в u64+tag). Exposed-модель + честная секция
  "Provenance model"; sentinel → without_provenance (реально strict-чист).
- **#142: тот же exposed-provenance паттерн** решил aliasing-UB (владелец
  штамповал &mut self-производный provenance; remote reconstruction через
  with_exposed_provenance_mut = wildcard, вне borrow-дерева). Быстрый фикс
  (owner через atomic_ptr_ref) НЕ помог — miri подтвердил, откатил.
- **#143: hoist claim-CAS ОДИН РАЗ до loop** — retry только head-CAS+store;
  loom-модель синхронно поправлена, should_panic снят.
- **Перф-доки: MT-макробенчи НЕ перегонял** (larson/mstress) — помечены
  историческими 0.2.0; single-thread criterion перегнал и обновил.

## Open questions

- **Push + tag + release 0.3.0** — единственное действие, всё готово. 18
  коммитов; тег `sefer-alloc-v0.3.0` триггерит release workflow → crates.io
  publish → GitHub Release из CHANGELOG [0.3.0]. Ждёт явной команды.
- **Yank 0.2.1 после 0.3.0?** — на усмотрение (0.2.1 без критичных для
  production-конфига багов уровня 0.2.0).
- **shamir-db перепрогон на 0.3.0** — после публикации (обещанные opt-3
  замеры пользователя так и не прозвучали).
- **#128 первый Linux-CI прогон** — за ним присмотреть после пуша (валидность
  iai-макросов, что порог Ir=10 реально фейлит; синтаксис env может уточниться).
- **Untracked чекпойнты** (docs/checkpoints/) — включать в пуш или нет.

## Repo state

```
?? docs/checkpoints/2026-07-02-v030-4agent-review.md
(рабочее дерево иначе чистое; этот чекпойнт добавит второй untracked-файл)
```

```
3a2ff11 docs: refresh README + ALLOC_BENCH perf tables with re-measured 0.3.0 numbers
fc84493 fix+docs(review): mitigation clamp symmetry, CHANGELOG fold, tooling drift
f635ee8 docs: fix broken intra-doc link in #140 provenance-model section
2a04197 ci(#128): make the perf-gate enforcing — persist baseline + regression limit
a637faf fix(#143)+test(#141): loom models for A1/free_slots find & fix a push leak
```
(полная лента 18 коммитов впереди origin/main: ec44e1f #142, 8a486a1 #140,
d6f969e #138, a42d449 #137, 29377af #136, ac8da30 #139, 7b45ed9 #135,
d8f9581 #134, 6a25685 #132, 9ce6f36 #131, 997448f #130, f17491a #129,
99ed1e1 #133 — плюс 5 review/docs-коммитов выше)

crates.io: sefer-alloc 0.2.1 live (0.1.0/0.2.0 yanked). Cargo.toml = 0.3.0,
CHANGELOG [0.3.0] - 2026-07-03 готов. Тег sefer-alloc-v0.3.0 НЕ создан. Push
НЕ выполнен.

## Итоговый счёт верификации

За три волны (4-агентное ревью → hardening #128–#143 → финальное /fxx-ревью)
из 0.3.0 извлечено и закрыто **17 дефектов**, включая **5 memory-safety**:
2 блокера (#129 double-writer, #130 align-UB) + #142 aliasing-UB + 2 leak-
класса (#143 push-leak, #138-clamp). Каждый — с counterfactual-регрессией,
прогнанной лично. Все оси верификации зелёные на финальном дереве.
```
