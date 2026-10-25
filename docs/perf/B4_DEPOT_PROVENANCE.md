# B4_DEPOT_PROVENANCE — D0 «окупился бы depot magazine?»

**Измерение, без изменения runtime.** Production-код не менялся; всё измерение —
счётная симуляция поверх `bench-internals`-гейтированных счётчиков
(`DEPOT_*` + shadow-агрегаты) в `examples/dp_depot_probe.rs` и
`examples/dp_native_mixed_counters.rs`, и callgrind-прогоны production и
counter-сборок. Фаза R5: скрипт таблицы, отчёт и карточка №9 переписаны под
свежее сырьё — счётчики v4 (новое дерево) + callgrind v3-эры (старое дерево)
с численной валидацией `calls=` из первичного сырья.

## Identity — ДВЕ пары (счётчики v4 и callgrind v3-эры)

- База: `f80c681b` (origin/main, merge PR #95, b4-stack).
- **Счётчики (20 v4-логов)** — новое дерево: `git_diff_sha256`
  = `2c8d7c76333ef2832896e95853b1f6929204fe255e2d10a8d25481ec05be5844`,
  `tree_sha256` = `5441d8c29b7a64179ecada4d1aafdcfe0efc27678db25da09eac47b8370d9e9b`.
- **Callgrind (24 cg_summary)** — v3-эра, старое дерево: `git_diff_sha256`
  = `e7dc9d00b4f766e5783bf3ac431c0daf972a069af94af9cb60ffc21a4f1529f2`,
  `tree_sha256` = `59248117087f4c3d73af54311d9f45b53e220261d242c4c43beee07ec6c6a447`.
- **Почему cg-сырьё валидно для нового дерева:** дельта cg-era→R4 добавляет
  ТОЛЬКО `bench-internals`-хуки (доказательство — `raw/r4_behavior_parity.txt`;
  production-сборка побайтово идентична по поведению). Поэтому `calls=` и Ir
  production-сборки (full/tf/td) остаются валидными для нового дерева;
  счётчиковая cg-сборка отличается только Relaxed-инкрементами. Скрипт
  assert'ит identity-пару на КАЖДЫЙ читаемый файл (20 v4 + 24 cg) и
  существование/непустоту `r4_behavior_parity.txt`.
- features: счётчики `production,bench-internals,internals`; callgrind:
  `production` (full/tf/td) и `production,bench-internals,internals` (ctr);
  rustc 1.98.1 (48a229cea 2026-09-01); CPU: 11th Gen Intel(R) Core(TM)
  i7-11800H @ 2.30GHz; OS: Linux 6.18.33.2-microsoft-standard-WSL2.
- Повторы: larson/mstress T=1 ×1, T=2/T=4 ×3 (медианы и разброс max−min по
  всем повторам); churn ×3 (окно счётчиков repeats=120; второй набор s241 ×3
  прочитан и assert'нут); callgrind: 50000→100000 (larson/mstress), 120→241
  (churn), конфигурации full/tf/td/ctr.
- Манифесты: `raw/tree_manifest.txt` (новое дерево) и
  `raw/tree_manifest_cg_era.txt` (cg-эра); скрипт сам считает sha256
  СОДЕРЖИМОГО каждого манифеста (node:crypto) и assert'ит совпадение с
  записанным tree_sha256 соответствующей пары; в манифест входят 2 строки
  `wsl-snapshot/` (экстракты cg-эры: `dp_depot_cg.rs`, `Cargo.toml.cg-patch`).

## Точки входа под замером (правило «wrong allocator layer»)

- **larson / mstress**: `examples/dp_depot_probe.rs` — пример собирает СВОЙ
  экземпляр аллокатора: фабрика `sefer_alloc::SeferAlloc::new` передаётся в
  `run_counting(...)`, рабочие циклы — vendored VERBATIM из
  `crates/malloc-bench-rs/src/lib.rs` (поток аллокаций идентичен judge-прогонам,
  сброшен только wall-clock). Аллокация через публичный API `SeferAlloc`
  (TLS/HeapCore-цепочка, тот же слой, что у installed-судьи), счётчики —
  process-wide statics через `HeapCore::dbg_depot_*`.
- **churn+Vec**: `examples/dp_native_mixed_counters.rs` (счётчики) и
  `examples/dp_native_mixed_sefer.rs` (callgrind) — installed judge:
  `#[global_allocator] static GLOBAL: sefer_alloc::SeferAlloc`
  (GlobalAlloc-фасад), полный production-слой.

## Ingress-разбивка и оракулы активации (ИЗМЕРЕНО)

Полные per-K таблицы (включая REMOTE events+блоки — discovery покрыт) — в
выводе скрипта ниже. Ключевые медианы:

| нагрузка/T | own overflow (блоков/соб.) | own batch | REMOTE (блоков / событий) | swap_top | teardown | refill drained/carved |
|---|---|---|---|---|---|---|
| larson T=1 | 14048/1756 | 0 | 0 / 0 | 25 | 468 | 13072/1469 |
| larson T=2 | 25872/3234 | 0 | 12283 / 298 | 163 | 653 | 36171/3017 |
| larson T=4 | 51304/6413 | 0 | 24711 / 578 | 349 | 1335 | 71978/6010 |
| mstress T=1 | 101872/12734 | 0 | 0 / 0 | 0 | 468 | 101224/1116 |
| mstress T=2 | 214680/26835 | 0 | 9783 / 184 | 102 | 682 | 223009/2318 |
| mstress T=4 | 430024/53753 | 0 | 18431 / 440 | 189 | 1374 | 445807/5308 |
| churn (reps=120) | 944768/118096 | 0 | 0 / 0 | 0 | 0 | 943792/1104 |

- **SUBSTRATE_PUSHES-инвариант** (assert на каждом прогоне):
  `depot_bin_ingress_pushes == own_overflow + own_batch + remote +
  swap_top + teardown + other` (по блокам), при этом `other_blocks == 0` —
  оракул полноты классификации: каждый push-блок отнесён к известному
  источнику. Large-retirement НЕ проходит через Small-ingress (large-путь
  учитывается отдельно и в эту сумму не входит).
- Оракулы T=1 (assert на каждом T=1-прогоне): `remote_events == 0`,
  `remote_blocks == 0`, `own_batch_events == 0`, `own_batch_blocks == 0` —
  всюду выполнены. MT-прогоны показывают ожидаемый не нулевой REMOTE-ingress
  (блоки, возвращённые чужими потоками), не претендующий на depot-поглощение.
- `unmapped` — трипвайр чужих id (assert `== 0` на каждом прогоне; fallback
  отображается в выделенный ряд и в этот счётчик не попадает).

## Точность счётной симуляции

**Почему подсчёт точен:** depot хранит самые свежие own-блоки класса;
own-free ping-pong переиспользует блоки по LIFO-подобному закону, и
shadow-счётчикам безразличен ПОРЯДОК блоков в полу-magazine — только
КОЛИЧЕСТВО блоков с «возрастом» ≤ K полу-magazine'ов на момент
flush/refill. Симуляция в примере переигрывает реальный поток аллокаций
(не модель), поэтому `absorbed_k`/`refill_from_depot_k` — точные счётчики.
Арифметика assert'ится на каждом прогоне: `Σ_K(absorbed+real) ==
4×own_overflow_blocks`, `Σ_K(refill_from_depot+refill_drain) ==
4×refill_blocks_total`, `drained+carved == blocks_total`,
`absorbed+real == own_overflow_blocks` (per-K), occupied-инварианты
`fully_absorbed ≤ own_overflow_events`, `refill_fully ≤ refill_events`.

**Где симуляция неверна (что не спасает depot-резидентный блок и что модель
цены не учитывает):**

- **reclaim/teardown сегмента**: блоки teardown-флаша
  (`depot_flush_teardown_*`) уходят из-под аллокатора целиком;
  depot-резидентность их не спасает, и в цене они не засчитаны как выгода
  (маржинальные тогглы меряют flush/drain, а не teardown).
- **trim/decommit**: явный trim снимает блоки мимо depot-пути.
- **удалённые free**: REMOTE-ingress блоки принадлежат другому magazine;
  depot в прототипе own-only — эти блоки в симуляции не поглощаются
  (корректно), но их доля растёт с T, снижая потолок выгоды на MT.
- **virgin-carve**: свежевырезанные блоки (`refill_blocks_carved`) не
  проходили flush — их refill depot заменить не может; в доле
  `drain_replace_share` знаменатель — только `blocks_drained`, carve
  исключён корректно.
- **FREE_PARK_CAP (R1-01/G13)**: парковка свободных блоков выше depot в
  стеке путей высвобождения; взаимодействие с depot не моделировалось.

## Модель цены (маржинальные тогглы, без повторного счёта)

Только непересекающиеся МАРЖИНАЛЬНЫЕ вклады:

```
removed_ir/step(K) = absorb_share_K × tf_marginal
                   + drain_replace_share_K × td_marginal
                   − depot_price_K
```

- `absorb_share_K = absorbed_k / own_overflow_blocks` — доля flush-работы,
  исчезающей целиком (блок остался в magazine).
- Две refill-метрики (печатаются ОБЕ, со знаменателями):
  `drain_replace_share_K = refill_from_depot_k / refill_blocks_drained`
  (доля drain-блоков, заменённых выдачей из depot; carve исключён) и
  `refill_depot_share_K = refill_from_depot_k / refill_blocks_total`
  (та же выгода от знаменателя TOTAL).
- «flush-событий исчезло ЦЕЛИКОМ» — печатается и ЧИСЛОМ событий
  (`fully_absorbed_ev_k`, ИЗМЕРЕНО), и долей от `own_overflow_events`;
  аналогично «refill-событий полностью из depot» — `refill_fully_k` (число)
  и доля от `refill_events`. Drain-события обслуживаются целиком.
- `tf_marginal`/`td_marginal` — маржинальные Ir/step тогглов, ИЗМЕРЕНО
  двухточечно: `(Ir@2N − Ir@N)/N`; для churn — Ir/повтор, знаменатель — окно
  120→241 = **121** (числа 6 560 789.60 / 455 920.00 / 486 248.60).
  **Окна:** счётчики churn снимаются на окне 120 повторов, Ir-окно — 121
  повтор; погрешность ≤ 1/121 ≈ 0.83%, метрика помечена ОЦЕНКА.

| нагрузка | точки | FULL (ИЗМЕРЕНО) | tf (ИЗМЕРЕНО) | td (ИЗМЕРЕНО) |
|---|---|---|---|---|
| larson | 50000→100000 (÷50000) | 279.58 Ir/шаг | 7.95 | 9.88 |
| mstress | 50000→100000 (÷50000) | 368.86 Ir/шаг | 59.13 | 73.69 |
| churn | 120→241 (÷121) | 6 560 789.60 Ir/повтор | 455 920.00 | 486 248.60 |

`depot_price_K` — ОЦЕНКА добавляемой цены depot-прототипа:

```
depot_price_K = DEPOT_IR_PER_ABSORBED_BLOCK(3)
                × (absorbed_k + refill_from_depot_k)/steps
              + DEPOT_IR_PER_EVENT(2)
                × (fully_absorbed_ev_k + refill_fully_k)/steps
```

Константы — ОЦЕНКА (basis): 3 Ir/блок = load+store+инкремент индекса типового
push/pop массива из 8 указателей, 2 Ir/событие = ветка «полностью
поглощено / полностью выдано»; отдельно callgrind'ом НЕ измерялось (в
прототипе не существовало). `steps` для churn — окно счётчиков (120 повторов).

Assert модели (панника скрипта, на каждую пару нагрузка×K):
`removed_share ≤ (tf_marginal + td_marginal)/FULL_marginal` И `≤ 1`.
Дополнительно assert: пересчитанные двухточечные маржинальные совпали с
заданными заранее (допуск 0.5 Ir/шаг — округление); `tf+td ≤ full` для
всех трёх нагрузок (доли 0.0638 / 0.3601 / 0.1436).

## calls= prod vs ctr: 18/18 пар равны ЧИСЛЕННО

Метод: скрипт читает пары production (`*.full.N`) и счётчиковых
(`*.ctr.N`) cg-summaries, парсит `GROUP <fn> … CALLS=<n>` ЧИСЛЕННО и
assert'ит `prod_calls == ctr_calls` для всех 3 функций × 2 размера ×
3 нагрузки (18 пар) — никакого доверия текстовым EXACT-файлам (те в
скрипте не читаются вовсе). Обоснование смешения деревьев — см. Identity
(`r4_behavior_parity.txt`).

## Вывод скрипта (дословно)

```text
=== B4_DEPOT_PROVENANCE — D0 «окупился бы depot magazine?» (R5, v4-счётчики + callgrind v3-эры) ===

Identity (ИЗМЕРЕНО, assert на каждый читаемый файл):
  счётчики (20 v4-логов): git_diff_sha256 = 2c8d7c76333ef2832896e95853b1f6929204fe255e2d10a8d25481ec05be5844
                          tree_sha256     = 5441d8c29b7a64179ecada4d1aafdcfe0efc27678db25da09eac47b8370d9e9b
  callgrind (24 cg_summary v3-эры): git_diff_sha256 = e7dc9d00b4f766e5783bf3ac431c0daf972a069af94af9cb60ffc21a4f1529f2
                                    tree_sha256     = 59248117087f4c3d73af54311d9f45b53e220261d242c4c43beee07ec6c6a447
  Почему cg-эра: дельта cg-era→R4 меняет только bench-internals-хуки
  (доказательство — raw/r4_behavior_parity.txt, production-поведение
  побайтово идентично), поэтому calls=/Ir production-сборки валидны для
  нового дерева; счётчиковая сборка отличается только Relaxed-инкрементами.
  base_sha (origin/main) = f80c681b
  features (счётчики): production,bench-internals,internals
  rustc: rustc 1.98.1 (48a229cea 2026-09-01)
  cpu: 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz
  os: Linux 6.18.33.2-microsoft-standard-WSL2
  Повторы: larson/mstress T=1 ×1, T=2/T=4 ×3; churn ×3 (окно счётчиков
  repeats=120, второй набор s241 ×3 — прочитан и assert'нут); callgrind:
  50000→100000 (larson/mstress), 120→241 (churn), конфигурации full/tf/td/ctr.
  Манифесты: sha256(tree_manifest.txt) = tree_sha256 счётчиков ✓;
  sha256(tree_manifest_cg_era.txt) = cg-era tree_sha256 ✓; 2 строки
  wsl-snapshot/ (dp_depot_cg.rs, Cargo.toml.cg-patch) ✓;
  r4_behavior_parity.txt существует и непуст ✓. Все assert'ы пройдены.

=== Маржинальные Ir (ИЗМЕРЕНО, двухточечные (Ir@2N−Ir@N)/N; churn — Ir/повтор, знаменатель 121) ===

| нагрузка | N | FULL Ir/шаг (ИЗМЕРЕНО) | tf Ir/шаг (ИЗМЕРЕНО) | td Ir/шаг (ИЗМЕРЕНО) |
|---|---|---|---|---|
| larson | 50000→100000 | 279.58 | 7.95 | 9.88 |
| mstress | 50000→100000 | 368.86 | 59.13 | 73.69 |
| churn | 120→241 | 6560789.60 | 455920.00 | 486248.60 |

  Точки (ИЗМЕРЕНО из PROGRAM TOTALS / TOGGLE_TOTAL_IR):
    larson full: @50000=15364728, @100000=29343588 → (29343588−15364728)/50000 = 279.58
    larson tf: @50000=450115, @100000=847507 → (847507−450115)/50000 = 7.95
    larson td: @50000=459339, @100000=953259 → (953259−459339)/50000 = 9.88
    mstress full: @50000=19511236, @100000=37954225 → (37954225−19511236)/50000 = 368.86
    mstress tf: @50000=3015155, @100000=5971653 → (5971653−3015155)/50000 = 59.13
    mstress td: @50000=3670629, @100000=7355025 → (7355025−3670629)/50000 = 73.69
    churn full: @120=794351178, @241=1588206719 → (1588206719−794351178)/121 = 6560789.60
    churn tf: @120=55175670, @241=110341990 → (110341990−55175670)/121 = 455920.00
    churn td: @120=58755247, @241=117591327 → (117591327−58755247)/121 = 486248.60
  churn: знаменатель 121 = окно 120→241 (Ir-окно); счётчиковое окно 120
  повторов — погрешность ≤1/121 ≈ 0.83%, метрика ОЦЕНКА (см. модель).
  Assert: пересчитанные маржинальные совпали с заданными заранее (допуск 0.5 Ir/шаг).
  Assert модели: tf+td ≤ full выполнено для всех трёх нагрузок (доли: larson 0.0638, mstress 0.3601, churn 0.1436).

  calls= prod vs ctr (ИЗМЕРЕНО, численно из GROUP … CALLS=, 18/18 равны):
    larson n=50000 flush_class_internal: prod_calls=956 ctr_calls=956 -> EQUAL
    larson n=50000 refill_magazine_slow: prod_calls=1461 ctr_calls=1461 -> EQUAL
    larson n=50000 try_drain_freelist_batch: prod_calls=506 ctr_calls=506 -> EQUAL
    larson n=100000 flush_class_internal: prod_calls=1812 ctr_calls=1812 -> EQUAL
    larson n=100000 refill_magazine_slow: prod_calls=2736 ctr_calls=2736 -> EQUAL
    larson n=100000 try_drain_freelist_batch: prod_calls=935 ctr_calls=935 -> EQUAL
    mstress n=50000 flush_class_internal: prod_calls=6435 ctr_calls=6435 -> EQUAL
    mstress n=50000 refill_magazine_slow: prod_calls=9699 ctr_calls=9699 -> EQUAL
    mstress n=50000 try_drain_freelist_batch: prod_calls=3250 ctr_calls=3250 -> EQUAL
    mstress n=100000 flush_class_internal: prod_calls=12765 ctr_calls=12765 -> EQUAL
    mstress n=100000 refill_magazine_slow: prod_calls=19194 ctr_calls=19194 -> EQUAL
    mstress n=100000 try_drain_freelist_batch: prod_calls=6419 ctr_calls=6419 -> EQUAL
    churn n=120 flush_class_internal: prod_calls=118108 ctr_calls=118108 -> EQUAL
    churn n=120 refill_magazine_slow: prod_calls=177180 ctr_calls=177180 -> EQUAL
    churn n=120 try_drain_freelist_batch: prod_calls=59060 ctr_calls=59060 -> EQUAL
    churn n=241 flush_class_internal: prod_calls=236204 ctr_calls=236204 -> EQUAL
    churn n=241 refill_magazine_slow: prod_calls=354324 ctr_calls=354324 -> EQUAL
    churn n=241 try_drain_freelist_batch: prod_calls=118108 ctr_calls=118108 -> EQUAL

=== Модель цены (только непересекающиеся МАРЖИНАЛЬНЫЕ вклады) ===

  removed_ir/step(K) = absorb_share_K × tf_marginal + drain_replace_share_K × td_marginal − depot_price_K
    absorb_share_K = absorbed_k / own_overflow_blocks (ИЗМЕРЕНО/ИЗМЕРЕНО → ОЦЕНКА доли)
    drain_replace_share_K = refill_from_depot_k / refill_blocks_drained (ИЗМЕРЕНО/ИЗМЕРЕНО → ОЦЕНКА доли; carve исключён)
    refill_depot_share_K = refill_from_depot_k / refill_blocks_total (та же выгода, знаменатель TOTAL — печатаются ОБЕ метрики)
    depot_price_K = 3×(absorbed_k+refill_from_depot_k)/steps + 2×(fully_absorbed_ev_k+refill_fully_k)/steps (ОЦЕНКА)
      3 Ir/блок = load+store+инкремент индекса push/pop массива из 8 указателей — ОЦЕНКА объёма, отдельно НЕ измерялось
      2 Ir/событие = ветка «полностью поглощено / полностью выдано» — ОЦЕНКА, отдельно НЕ измерялось
  steps: larson/mstress — шаги (100000); churn — окно СЧЁТЧИКОВ 120 повторов
  (Ir-окно 121 повтор: погрешность ≤1/121 ≈ 0.83%, ОЦЕНКА).
  Assert на каждую пару (нагрузка×K): removed_share ≤ (tf+td)/full И ≤ 1 — все пройдены.

### larson T=1 (reps=1, steps/repeats=100000, Ir/шаг; медиана по повторам, разброс = max−min)

| K | metric | медиана | разброс | kind |
|---|---|---|---|---|
| 1 | absorb_share | 0.491458 | 0 | ОЦЕНКА |
| 1 | refill_depot_share | 0.419779 | 0 | ОЦЕНКА |
| 1 | drain_replace_share | 0.466952 | 0 | ОЦЕНКА |
| 1 | fully_absorbed_ev | 863 | 0 | ИЗМЕРЕНО |
| 1 | fully_absorbed_ev_share | 0.491458 | 0 | ОЦЕНКА |
| 1 | refill_fully | 0 | 0 | ИЗМЕРЕНО |
| 1 | refill_fully_share | 0 | 0 | ОЦЕНКА |
| 1 | depot_price_ir | 0.407500 | 0 | ОЦЕНКА |
| 1 | removed_ir | 8.111270 | 0 | ОЦЕНКА |
| 1 | removed_share | 0.029013 | 0 | ОЦЕНКА |
| 1 | ingress_own_overflow_blocks | 14048 | 0 | ИЗМЕРЕНО |
| 1 | ingress_own_overflow_events | 1756 | 0 | ИЗМЕРЕНО |
| 1 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 1 | ingress_remote_blocks | 0 | 0 | ИЗМЕРЕНО |
| 1 | ingress_remote_events | 0 | 0 | ИЗМЕРЕНО |
| 1 | flush_swap_top_blocks | 25 | 0 | ИЗМЕРЕНО |
| 1 | flush_teardown_blocks | 468 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_drained | 13072 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_carved | 1469 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_total | 14541 | 0 | ИЗМЕРЕНО |
| 1 | substrate_pushes | 14541 | 0 | ИЗМЕРЕНО |
| 1 | invalidations | 27 | 0 | ИЗМЕРЕНО |
| 1 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 2 | absorb_share | 0.784169 | 0 | ОЦЕНКА |
| 2 | refill_depot_share | 0.679458 | 0 | ОЦЕНКА |
| 2 | drain_replace_share | 0.755814 | 0 | ОЦЕНКА |
| 2 | fully_absorbed_ev | 1377 | 0 | ИЗМЕРЕНО |
| 2 | fully_absorbed_ev_share | 0.784169 | 0 | ОЦЕНКА |
| 2 | refill_fully | 472 | 0 | ИЗМЕРЕНО |
| 2 | refill_fully_share | 0.517544 | 0 | ОЦЕНКА |
| 2 | depot_price_ir | 0.663860 | 0 | ОЦЕНКА |
| 2 | removed_ir | 13.034819 | 0 | ОЦЕНКА |
| 2 | removed_share | 0.046623 | 0 | ОЦЕНКА |
| 2 | ingress_own_overflow_blocks | 14048 | 0 | ИЗМЕРЕНО |
| 2 | ingress_own_overflow_events | 1756 | 0 | ИЗМЕРЕНО |
| 2 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 2 | ingress_remote_blocks | 0 | 0 | ИЗМЕРЕНО |
| 2 | ingress_remote_events | 0 | 0 | ИЗМЕРЕНО |
| 2 | flush_swap_top_blocks | 25 | 0 | ИЗМЕРЕНО |
| 2 | flush_teardown_blocks | 468 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_drained | 13072 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_carved | 1469 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_total | 14541 | 0 | ИЗМЕРЕНО |
| 2 | substrate_pushes | 14541 | 0 | ИЗМЕРЕНО |
| 2 | invalidations | 27 | 0 | ИЗМЕРЕНО |
| 2 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 4 | absorb_share | 0.941913 | 0 | ОЦЕНКА |
| 4 | refill_depot_share | 0.803246 | 0 | ОЦЕНКА |
| 4 | drain_replace_share | 0.893513 | 0 | ОЦЕНКА |
| 4 | fully_absorbed_ev | 1654 | 0 | ИЗМЕРЕНО |
| 4 | fully_absorbed_ev_share | 0.941913 | 0 | ОЦЕНКА |
| 4 | refill_fully | 681 | 0 | ИЗМЕРЕНО |
| 4 | refill_fully_share | 0.746711 | 0 | ОЦЕНКА |
| 4 | depot_price_ir | 0.794060 | 0 | ОЦЕНКА |
| 4 | removed_ir | 15.518595 | 0 | ОЦЕНКА |
| 4 | removed_share | 0.055507 | 0 | ОЦЕНКА |
| 4 | ingress_own_overflow_blocks | 14048 | 0 | ИЗМЕРЕНО |
| 4 | ingress_own_overflow_events | 1756 | 0 | ИЗМЕРЕНО |
| 4 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 4 | ingress_remote_blocks | 0 | 0 | ИЗМЕРЕНО |
| 4 | ingress_remote_events | 0 | 0 | ИЗМЕРЕНО |
| 4 | flush_swap_top_blocks | 25 | 0 | ИЗМЕРЕНО |
| 4 | flush_teardown_blocks | 468 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_drained | 13072 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_carved | 1469 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_total | 14541 | 0 | ИЗМЕРЕНО |
| 4 | substrate_pushes | 14541 | 0 | ИЗМЕРЕНО |
| 4 | invalidations | 27 | 0 | ИЗМЕРЕНО |
| 4 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 8 | absorb_share | 0.989180 | 0 | ОЦЕНКА |
| 8 | refill_depot_share | 0.820301 | 0 | ОЦЕНКА |
| 8 | drain_replace_share | 0.912485 | 0 | ОЦЕНКА |
| 8 | fully_absorbed_ev | 1737 | 0 | ИЗМЕРЕНО |
| 8 | fully_absorbed_ev_share | 0.989180 | 0 | ОЦЕНКА |
| 8 | refill_fully | 709 | 0 | ИЗМЕРЕНО |
| 8 | refill_fully_share | 0.777412 | 0 | ОЦЕНКА |
| 8 | depot_price_ir | 0.823640 | 0 | ОЦЕНКА |
| 8 | removed_ir | 16.052093 | 0 | ОЦЕНКА |
| 8 | removed_share | 0.057416 | 0 | ОЦЕНКА |
| 8 | ingress_own_overflow_blocks | 14048 | 0 | ИЗМЕРЕНО |
| 8 | ingress_own_overflow_events | 1756 | 0 | ИЗМЕРЕНО |
| 8 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 8 | ingress_remote_blocks | 0 | 0 | ИЗМЕРЕНО |
| 8 | ingress_remote_events | 0 | 0 | ИЗМЕРЕНО |
| 8 | flush_swap_top_blocks | 25 | 0 | ИЗМЕРЕНО |
| 8 | flush_teardown_blocks | 468 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_drained | 13072 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_carved | 1469 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_total | 14541 | 0 | ИЗМЕРЕНО |
| 8 | substrate_pushes | 14541 | 0 | ИЗМЕРЕНО |
| 8 | invalidations | 27 | 0 | ИЗМЕРЕНО |
| 8 | unmapped | 0 | 0 | ИЗМЕРЕНО |

### larson T=2 (reps=3, steps/repeats=100000, Ir/шаг; медиана по повторам, разброс = max−min)

| K | metric | медиана | разброс | kind |
|---|---|---|---|---|
| 1 | absorb_share | 0.702226 | 0 | ОЦЕНКА |
| 1 | refill_depot_share | 0.297234 | 0 | ОЦЕНКА |
| 1 | drain_replace_share | 0.322026 | 0.000400 | ОЦЕНКА |
| 1 | fully_absorbed_ev | 2271 | 0 | ИЗМЕРЕНО |
| 1 | fully_absorbed_ev_share | 0.702226 | 0 | ОЦЕНКА |
| 1 | refill_fully | 0 | 0 | ИЗМЕРЕНО |
| 1 | refill_fully_share | 0 | 0 | ОЦЕНКА |
| 1 | depot_price_ir | 0.939900 | 0 | ОЦЕНКА |
| 1 | removed_ir | 7.822384 | 0.003953 | ОЦЕНКА |
| 1 | removed_share | 0.027979 | 0.000014 | ОЦЕНКА |
| 1 | ingress_own_overflow_blocks | 25872 | 0 | ИЗМЕРЕНО |
| 1 | ingress_own_overflow_events | 3234 | 0 | ИЗМЕРЕНО |
| 1 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 1 | ingress_remote_blocks | 12283 | 206 | ИЗМЕРЕНО |
| 1 | ingress_remote_events | 298 | 0 | ИЗМЕРЕНО |
| 1 | flush_swap_top_blocks | 163 | 0 | ИЗМЕРЕНО |
| 1 | flush_teardown_blocks | 653 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_drained | 36171 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_carved | 3017 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_total | 39188 | 0 | ИЗМЕРЕНО |
| 1 | substrate_pushes | 38971 | 206 | ИЗМЕРЕНО |
| 1 | invalidations | 168 | 0 | ИЗМЕРЕНО |
| 1 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 2 | absorb_share | 0.909400 | 0 | ОЦЕНКА |
| 2 | refill_depot_share | 0.391753 | 0 | ОЦЕНКА |
| 2 | drain_replace_share | 0.424428 | 0.000527 | ОЦЕНКА |
| 2 | fully_absorbed_ev | 2941 | 0 | ИЗМЕРЕНО |
| 2 | fully_absorbed_ev_share | 0.909400 | 0 | ОЦЕНКА |
| 2 | refill_fully | 463 | 0 | ИЗМЕРЕНО |
| 2 | refill_fully_share | 0.187601 | 0 | ОЦЕНКА |
| 2 | depot_price_ir | 1.234480 | 0 | ОЦЕНКА |
| 2 | removed_ir | 10.185960 | 0.005210 | ОЦЕНКА |
| 2 | removed_share | 0.036433 | 0.000019 | ОЦЕНКА |
| 2 | ingress_own_overflow_blocks | 25872 | 0 | ИЗМЕРЕНО |
| 2 | ingress_own_overflow_events | 3234 | 0 | ИЗМЕРЕНО |
| 2 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 2 | ingress_remote_blocks | 12283 | 206 | ИЗМЕРЕНО |
| 2 | ingress_remote_events | 298 | 0 | ИЗМЕРЕНО |
| 2 | flush_swap_top_blocks | 163 | 0 | ИЗМЕРЕНО |
| 2 | flush_teardown_blocks | 653 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_drained | 36171 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_carved | 3017 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_total | 39188 | 0 | ИЗМЕРЕНО |
| 2 | substrate_pushes | 38971 | 206 | ИЗМЕРЕНО |
| 2 | invalidations | 168 | 0 | ИЗМЕРЕНО |
| 2 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 4 | absorb_share | 0.971552 | 0 | ОЦЕНКА |
| 4 | refill_depot_share | 0.411555 | 0 | ОЦЕНКА |
| 4 | drain_replace_share | 0.445882 | 0.000554 | ОЦЕНКА |
| 4 | fully_absorbed_ev | 3142 | 0 | ИЗМЕРЕНО |
| 4 | fully_absorbed_ev_share | 0.971552 | 0 | ОЦЕНКА |
| 4 | refill_fully | 539 | 0 | ИЗМЕРЕНО |
| 4 | refill_fully_share | 0.218395 | 0 | ОЦЕНКА |
| 4 | depot_price_ir | 1.311540 | 0 | ОЦЕНКА |
| 4 | removed_ir | 10.814803 | 0.005474 | ОЦЕНКА |
| 4 | removed_share | 0.038683 | 0.000020 | ОЦЕНКА |
| 4 | ingress_own_overflow_blocks | 25872 | 0 | ИЗМЕРЕНО |
| 4 | ingress_own_overflow_events | 3234 | 0 | ИЗМЕРЕНО |
| 4 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 4 | ingress_remote_blocks | 12283 | 206 | ИЗМЕРЕНО |
| 4 | ingress_remote_events | 298 | 0 | ИЗМЕРЕНО |
| 4 | flush_swap_top_blocks | 163 | 0 | ИЗМЕРЕНО |
| 4 | flush_teardown_blocks | 653 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_drained | 36171 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_carved | 3017 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_total | 39188 | 0 | ИЗМЕРЕНО |
| 4 | substrate_pushes | 38971 | 206 | ИЗМЕРЕНО |
| 4 | invalidations | 168 | 0 | ИЗМЕРЕНО |
| 4 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 8 | absorb_share | 0.988559 | 0 | ОЦЕНКА |
| 8 | refill_depot_share | 0.411759 | 0 | ОЦЕНКА |
| 8 | drain_replace_share | 0.446103 | 0.000554 | ОЦЕНКА |
| 8 | fully_absorbed_ev | 3197 | 0 | ИЗМЕРЕНО |
| 8 | fully_absorbed_ev_share | 0.988559 | 0 | ОЦЕНКА |
| 8 | refill_fully | 540 | 0 | ИЗМЕРЕНО |
| 8 | refill_fully_share | 0.218801 | 0 | ОЦЕНКА |
| 8 | depot_price_ir | 1.326100 | 0 | ОЦЕНКА |
| 8 | removed_ir | 10.937595 | 0.005476 | ОЦЕНКА |
| 8 | removed_share | 0.039122 | 0.000020 | ОЦЕНКА |
| 8 | ingress_own_overflow_blocks | 25872 | 0 | ИЗМЕРЕНО |
| 8 | ingress_own_overflow_events | 3234 | 0 | ИЗМЕРЕНО |
| 8 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 8 | ingress_remote_blocks | 12283 | 206 | ИЗМЕРЕНО |
| 8 | ingress_remote_events | 298 | 0 | ИЗМЕРЕНО |
| 8 | flush_swap_top_blocks | 163 | 0 | ИЗМЕРЕНО |
| 8 | flush_teardown_blocks | 653 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_drained | 36171 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_carved | 3017 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_total | 39188 | 0 | ИЗМЕРЕНО |
| 8 | substrate_pushes | 38971 | 206 | ИЗМЕРЕНО |
| 8 | invalidations | 168 | 0 | ИЗМЕРЕНО |
| 8 | unmapped | 0 | 0 | ИЗМЕРЕНО |

### larson T=4 (reps=3, steps/repeats=100000, Ir/шаг; медиана по повторам, разброс = max−min)

| K | metric | медиана | разброс | kind |
|---|---|---|---|---|
| 1 | absorb_share | 0.706534 | 0 | ОЦЕНКА |
| 1 | refill_depot_share | 0.298097 | 0 | ОЦЕНКА |
| 1 | drain_replace_share | 0.322988 | 0.000085 | ОЦЕНКА |
| 1 | fully_absorbed_ev | 4531 | 0 | ИЗМЕРЕНО |
| 1 | fully_absorbed_ev_share | 0.706534 | 0 | ОЦЕНКА |
| 1 | refill_fully | 0 | 0 | ИЗМЕРЕНО |
| 1 | refill_fully_share | 0 | 0 | ОЦЕНКА |
| 1 | depot_price_ir | 1.875500 | 0 | ОЦЕНКА |
| 1 | removed_ir | 6.930517 | 0.000842 | ОЦЕНКА |
| 1 | removed_share | 0.024789 | 0.000003 | ОЦЕНКА |
| 1 | ingress_own_overflow_blocks | 51304 | 0 | ИЗМЕРЕНО |
| 1 | ingress_own_overflow_events | 6413 | 0 | ИЗМЕРЕНО |
| 1 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 1 | ingress_remote_blocks | 24711 | 36 | ИЗМЕРЕНО |
| 1 | ingress_remote_events | 578 | 0 | ИЗМЕРЕНО |
| 1 | flush_swap_top_blocks | 349 | 0 | ИЗМЕРЕНО |
| 1 | flush_teardown_blocks | 1335 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_drained | 71978 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_carved | 6010 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_total | 77988 | 0 | ИЗМЕРЕНО |
| 1 | substrate_pushes | 77699 | 36 | ИЗМЕРЕНО |
| 1 | invalidations | 359 | 0 | ИЗМЕРЕНО |
| 1 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 2 | absorb_share | 0.916732 | 0 | ОЦЕНКА |
| 2 | refill_depot_share | 0.396266 | 0 | ОЦЕНКА |
| 2 | drain_replace_share | 0.429353 | 0.000113 | ОЦЕНКА |
| 2 | fully_absorbed_ev | 5879 | 0 | ИЗМЕРЕНО |
| 2 | fully_absorbed_ev_share | 0.916732 | 0 | ОЦЕНКА |
| 2 | refill_fully | 957 | 0 | ИЗМЕРЕНО |
| 2 | refill_fully_share | 0.194789 | 0 | ОЦЕНКА |
| 2 | depot_price_ir | 2.474800 | 0 | ОЦЕНКА |
| 2 | removed_ir | 9.052561 | 0.001119 | ОЦЕНКА |
| 2 | removed_share | 0.032379 | 0.000004 | ОЦЕНКА |
| 2 | ingress_own_overflow_blocks | 51304 | 0 | ИЗМЕРЕНО |
| 2 | ingress_own_overflow_events | 6413 | 0 | ИЗМЕРЕНО |
| 2 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 2 | ingress_remote_blocks | 24711 | 36 | ИЗМЕРЕНО |
| 2 | ingress_remote_events | 578 | 0 | ИЗМЕРЕНО |
| 2 | flush_swap_top_blocks | 349 | 0 | ИЗМЕРЕНО |
| 2 | flush_teardown_blocks | 1335 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_drained | 71978 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_carved | 6010 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_total | 77988 | 0 | ИЗМЕРЕНО |
| 2 | substrate_pushes | 77699 | 36 | ИЗМЕРЕНО |
| 2 | invalidations | 359 | 0 | ИЗМЕРЕНО |
| 2 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 4 | absorb_share | 0.975207 | 0 | ОЦЕНКА |
| 4 | refill_depot_share | 0.415141 | 0 | ОЦЕНКА |
| 4 | drain_replace_share | 0.449804 | 0.000119 | ОЦЕНКА |
| 4 | fully_absorbed_ev | 6254 | 0 | ИЗМЕРЕНО |
| 4 | fully_absorbed_ev_share | 0.975207 | 0 | ОЦЕНКА |
| 4 | refill_fully | 1098 | 0 | ИЗМЕРЕНО |
| 4 | refill_fully_share | 0.223489 | 0 | ОЦЕНКА |
| 4 | depot_price_ir | 2.619280 | 0 | ОЦЕНКА |
| 4 | removed_ir | 9.574851 | 0.001173 | ОЦЕНКА |
| 4 | removed_share | 0.034248 | 0.000004 | ОЦЕНКА |
| 4 | ingress_own_overflow_blocks | 51304 | 0 | ИЗМЕРЕНО |
| 4 | ingress_own_overflow_events | 6413 | 0 | ИЗМЕРЕНО |
| 4 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 4 | ingress_remote_blocks | 24711 | 36 | ИЗМЕРЕНО |
| 4 | ingress_remote_events | 578 | 0 | ИЗМЕРЕНО |
| 4 | flush_swap_top_blocks | 349 | 0 | ИЗМЕРЕНО |
| 4 | flush_teardown_blocks | 1335 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_drained | 71978 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_carved | 6010 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_total | 77988 | 0 | ИЗМЕРЕНО |
| 4 | substrate_pushes | 77699 | 36 | ИЗМЕРЕНО |
| 4 | invalidations | 359 | 0 | ИЗМЕРЕНО |
| 4 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 8 | absorb_share | 0.990956 | 0 | ОЦЕНКА |
| 8 | refill_depot_share | 0.415243 | 0 | ОЦЕНКА |
| 8 | drain_replace_share | 0.449915 | 0.000119 | ОЦЕНКА |
| 8 | fully_absorbed_ev | 6355 | 0 | ИЗМЕРЕНО |
| 8 | fully_absorbed_ev_share | 0.990956 | 0 | ОЦЕНКА |
| 8 | refill_fully | 1099 | 0 | ИЗМЕРЕНО |
| 8 | refill_fully_share | 0.223692 | 0 | ОЦЕНКА |
| 8 | depot_price_ir | 2.645800 | 0 | ОЦЕНКА |
| 8 | removed_ir | 9.674602 | 0.001173 | ОЦЕНКА |
| 8 | removed_share | 0.034604 | 0.000004 | ОЦЕНКА |
| 8 | ingress_own_overflow_blocks | 51304 | 0 | ИЗМЕРЕНО |
| 8 | ingress_own_overflow_events | 6413 | 0 | ИЗМЕРЕНО |
| 8 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 8 | ingress_remote_blocks | 24711 | 36 | ИЗМЕРЕНО |
| 8 | ingress_remote_events | 578 | 0 | ИЗМЕРЕНО |
| 8 | flush_swap_top_blocks | 349 | 0 | ИЗМЕРЕНО |
| 8 | flush_teardown_blocks | 1335 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_drained | 71978 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_carved | 6010 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_total | 77988 | 0 | ИЗМЕРЕНО |
| 8 | substrate_pushes | 77699 | 36 | ИЗМЕРЕНО |
| 8 | invalidations | 359 | 0 | ИЗМЕРЕНО |
| 8 | unmapped | 0 | 0 | ИЗМЕРЕНО |

### mstress T=1 (reps=1, steps/repeats=100000, Ir/шаг; медиана по повторам, разброс = max−min)

| K | metric | медиана | разброс | kind |
|---|---|---|---|---|
| 1 | absorb_share | 0.254908 | 0 | ОЦЕНКА |
| 1 | refill_depot_share | 0.252726 | 0 | ОЦЕНКА |
| 1 | drain_replace_share | 0.255513 | 0 | ОЦЕНКА |
| 1 | fully_absorbed_ev | 3246 | 0 | ИЗМЕРЕНО |
| 1 | fully_absorbed_ev_share | 0.254908 | 0 | ОЦЕНКА |
| 1 | refill_fully | 0 | 0 | ИЗМЕРЕНО |
| 1 | refill_fully_share | 0 | 0 | ОЦЕНКА |
| 1 | depot_price_ir | 1.619880 | 0 | ОЦЕНКА |
| 1 | removed_ir | 32.281014 | 0 | ОЦЕНКА |
| 1 | removed_share | 0.087516 | 0 | ОЦЕНКА |
| 1 | ingress_own_overflow_blocks | 101872 | 0 | ИЗМЕРЕНО |
| 1 | ingress_own_overflow_events | 12734 | 0 | ИЗМЕРЕНО |
| 1 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 1 | ingress_remote_blocks | 0 | 0 | ИЗМЕРЕНО |
| 1 | ingress_remote_events | 0 | 0 | ИЗМЕРЕНО |
| 1 | flush_swap_top_blocks | 0 | 0 | ИЗМЕРЕНО |
| 1 | flush_teardown_blocks | 468 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_drained | 101224 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_carved | 1116 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_total | 102340 | 0 | ИЗМЕРЕНО |
| 1 | substrate_pushes | 102340 | 0 | ИЗМЕРЕНО |
| 1 | invalidations | 2 | 0 | ИЗМЕРЕНО |
| 1 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 2 | absorb_share | 0.486022 | 0 | ОЦЕНКА |
| 2 | refill_depot_share | 0.481767 | 0 | ОЦЕНКА |
| 2 | drain_replace_share | 0.487078 | 0 | ОЦЕНКА |
| 2 | fully_absorbed_ev | 6189 | 0 | ИЗМЕРЕНО |
| 2 | fully_absorbed_ev_share | 0.486022 | 0 | ОЦЕНКА |
| 2 | refill_fully | 2930 | 0 | ИЗМЕРЕНО |
| 2 | refill_fully_share | 0.457956 | 0 | ОЦЕНКА |
| 2 | depot_price_ir | 3.146860 | 0 | ОЦЕНКА |
| 2 | removed_ir | 61.483359 | 0 | ОЦЕНКА |
| 2 | removed_share | 0.166685 | 0 | ОЦЕНКА |
| 2 | ingress_own_overflow_blocks | 101872 | 0 | ИЗМЕРЕНО |
| 2 | ingress_own_overflow_events | 12734 | 0 | ИЗМЕРЕНО |
| 2 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 2 | ingress_remote_blocks | 0 | 0 | ИЗМЕРЕНО |
| 2 | ingress_remote_events | 0 | 0 | ИЗМЕРЕНО |
| 2 | flush_swap_top_blocks | 0 | 0 | ИЗМЕРЕНО |
| 2 | flush_teardown_blocks | 468 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_drained | 101224 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_carved | 1116 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_total | 102340 | 0 | ИЗМЕРЕНО |
| 2 | substrate_pushes | 102340 | 0 | ИЗМЕРЕНО |
| 2 | invalidations | 2 | 0 | ИЗМЕРЕНО |
| 2 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 4 | absorb_share | 0.735433 | 0 | ОЦЕНКА |
| 4 | refill_depot_share | 0.728474 | 0 | ОЦЕНКА |
| 4 | drain_replace_share | 0.736505 | 0 | ОЦЕНКА |
| 4 | fully_absorbed_ev | 9365 | 0 | ИЗМЕРЕНО |
| 4 | fully_absorbed_ev_share | 0.735433 | 0 | ОЦЕНКА |
| 4 | refill_fully | 4507 | 0 | ИЗМЕРЕНО |
| 4 | refill_fully_share | 0.704439 | 0 | ОЦЕНКА |
| 4 | depot_price_ir | 4.761600 | 0 | ОЦЕНКА |
| 4 | removed_ir | 92.996041 | 0 | ОЦЕНКА |
| 4 | removed_share | 0.252118 | 0 | ОЦЕНКА |
| 4 | ingress_own_overflow_blocks | 101872 | 0 | ИЗМЕРЕНО |
| 4 | ingress_own_overflow_events | 12734 | 0 | ИЗМЕРЕНО |
| 4 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 4 | ingress_remote_blocks | 0 | 0 | ИЗМЕРЕНО |
| 4 | ingress_remote_events | 0 | 0 | ИЗМЕРЕНО |
| 4 | flush_swap_top_blocks | 0 | 0 | ИЗМЕРЕНО |
| 4 | flush_teardown_blocks | 468 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_drained | 101224 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_carved | 1116 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_total | 102340 | 0 | ИЗМЕРЕНО |
| 4 | substrate_pushes | 102340 | 0 | ИЗМЕРЕНО |
| 4 | invalidations | 2 | 0 | ИЗМЕРЕНО |
| 4 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 8 | absorb_share | 0.923983 | 0 | ОЦЕНКА |
| 8 | refill_depot_share | 0.914208 | 0 | ОЦЕНКА |
| 8 | drain_replace_share | 0.924287 | 0 | ОЦЕНКА |
| 8 | fully_absorbed_ev | 11766 | 0 | ИЗМЕРЕНО |
| 8 | fully_absorbed_ev_share | 0.923983 | 0 | ОЦЕНКА |
| 8 | refill_fully | 5793 | 0 | ИЗМЕРЕНО |
| 8 | refill_fully_share | 0.905439 | 0 | ОЦЕНКА |
| 8 | depot_price_ir | 5.981820 | 0 | ОЦЕНКА |
| 8 | removed_ir | 116.762027 | 0 | ОЦЕНКА |
| 8 | removed_share | 0.316549 | 0 | ОЦЕНКА |
| 8 | ingress_own_overflow_blocks | 101872 | 0 | ИЗМЕРЕНО |
| 8 | ingress_own_overflow_events | 12734 | 0 | ИЗМЕРЕНО |
| 8 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 8 | ingress_remote_blocks | 0 | 0 | ИЗМЕРЕНО |
| 8 | ingress_remote_events | 0 | 0 | ИЗМЕРЕНО |
| 8 | flush_swap_top_blocks | 0 | 0 | ИЗМЕРЕНО |
| 8 | flush_teardown_blocks | 468 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_drained | 101224 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_carved | 1116 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_total | 102340 | 0 | ИЗМЕРЕНО |
| 8 | substrate_pushes | 102340 | 0 | ИЗМЕРЕНО |
| 8 | invalidations | 2 | 0 | ИЗМЕРЕНО |
| 8 | unmapped | 0 | 0 | ИЗМЕРЕНО |

### mstress T=2 (reps=3, steps/repeats=100000, Ir/шаг; медиана по повторам, разброс = max−min)

| K | metric | медиана | разброс | kind |
|---|---|---|---|---|
| 1 | absorb_share | 0.276095 | 0 | ОЦЕНКА |
| 1 | refill_depot_share | 0.238658 | 0 | ОЦЕНКА |
| 1 | drain_replace_share | 0.241138 | 0.000245 | ОЦЕНКА |
| 1 | fully_absorbed_ev | 7409 | 0 | ИЗМЕРЕНО |
| 1 | fully_absorbed_ev_share | 0.276095 | 0 | ОЦЕНКА |
| 1 | refill_fully | 0 | 0 | ИЗМЕРЕНО |
| 1 | refill_fully_share | 0 | 0 | ОЦЕНКА |
| 1 | depot_price_ir | 3.539620 | 0 | ОЦЕНКА |
| 1 | removed_ir | 30.554822 | 0.018023 | ОЦЕНКА |
| 1 | removed_share | 0.082836 | 0.000049 | ОЦЕНКА |
| 1 | ingress_own_overflow_blocks | 214680 | 0 | ИЗМЕРЕНО |
| 1 | ingress_own_overflow_events | 26835 | 0 | ИЗМЕРЕНО |
| 1 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 1 | ingress_remote_blocks | 9783 | 410 | ИЗМЕРЕНО |
| 1 | ingress_remote_events | 184 | 0 | ИЗМЕРЕНО |
| 1 | flush_swap_top_blocks | 102 | 0 | ИЗМЕРЕНО |
| 1 | flush_teardown_blocks | 682 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_drained | 223009 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_carved | 2318 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_total | 225327 | 0 | ИЗМЕРЕНО |
| 1 | substrate_pushes | 225247 | 410 | ИЗМЕРЕНО |
| 1 | invalidations | 107 | 0 | ИЗМЕРЕНО |
| 1 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 2 | absorb_share | 0.503671 | 0 | ОЦЕНКА |
| 2 | refill_depot_share | 0.445255 | 0 | ОЦЕНКА |
| 2 | drain_replace_share | 0.449883 | 0.000456 | ОЦЕНКА |
| 2 | fully_absorbed_ev | 13516 | 0 | ИЗМЕРЕНО |
| 2 | fully_absorbed_ev_share | 0.503671 | 0 | ОЦЕНКА |
| 2 | refill_fully | 5819 | 0 | ИЗМЕРЕНО |
| 2 | refill_fully_share | 0.412783 | 0 | ОЦЕНКА |
| 2 | depot_price_ir | 6.640380 | 0 | ОЦЕНКА |
| 2 | removed_ir | 56.292598 | 0.033625 | ОЦЕНКА |
| 2 | removed_share | 0.152612 | 0.000091 | ОЦЕНКА |
| 2 | ingress_own_overflow_blocks | 214680 | 0 | ИЗМЕРЕНО |
| 2 | ingress_own_overflow_events | 26835 | 0 | ИЗМЕРЕНО |
| 2 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 2 | ingress_remote_blocks | 9783 | 410 | ИЗМЕРЕНО |
| 2 | ingress_remote_events | 184 | 0 | ИЗМЕРЕНО |
| 2 | flush_swap_top_blocks | 102 | 0 | ИЗМЕРЕНО |
| 2 | flush_teardown_blocks | 682 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_drained | 223009 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_carved | 2318 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_total | 225327 | 0 | ИЗМЕРЕНО |
| 2 | substrate_pushes | 225247 | 410 | ИЗМЕРЕНО |
| 2 | invalidations | 107 | 0 | ИЗМЕРЕНО |
| 2 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 4 | absorb_share | 0.740600 | 0 | ОЦЕНКА |
| 4 | refill_depot_share | 0.662255 | 0 | ОЦЕНКА |
| 4 | drain_replace_share | 0.669139 | 0.000679 | ОЦЕНКА |
| 4 | fully_absorbed_ev | 19874 | 0 | ИЗМЕРЕНО |
| 4 | fully_absorbed_ev_share | 0.740600 | 0 | ОЦЕНКА |
| 4 | refill_fully | 8704 | 0 | ИЗМЕРЕНО |
| 4 | refill_fully_share | 0.617436 | 0 | ОЦЕНКА |
| 4 | depot_price_ir | 9.818040 | 0 | ОЦЕНКА |
| 4 | removed_ir | 83.281061 | 0.050013 | ОЦЕНКА |
| 4 | removed_share | 0.225780 | 0.000136 | ОЦЕНКА |
| 4 | ingress_own_overflow_blocks | 214680 | 0 | ИЗМЕРЕНО |
| 4 | ingress_own_overflow_events | 26835 | 0 | ИЗМЕРЕНО |
| 4 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 4 | ingress_remote_blocks | 9783 | 410 | ИЗМЕРЕНО |
| 4 | ingress_remote_events | 184 | 0 | ИЗМЕРЕНО |
| 4 | flush_swap_top_blocks | 102 | 0 | ИЗМЕРЕНО |
| 4 | flush_teardown_blocks | 682 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_drained | 223009 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_carved | 2318 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_total | 225327 | 0 | ИЗМЕРЕНО |
| 4 | substrate_pushes | 225247 | 410 | ИЗМЕРЕНО |
| 4 | invalidations | 107 | 0 | ИЗМЕРЕНО |
| 4 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 8 | absorb_share | 0.926178 | 0 | ОЦЕНКА |
| 8 | refill_depot_share | 0.833881 | 0 | ОЦЕНКА |
| 8 | drain_replace_share | 0.842549 | 0.000855 | ОЦЕНКА |
| 8 | fully_absorbed_ev | 24854 | 0 | ИЗМЕРЕНО |
| 8 | fully_absorbed_ev_share | 0.926178 | 0 | ОЦЕНКА |
| 8 | refill_fully | 11185 | 0 | ИЗМЕРЕНО |
| 8 | refill_fully_share | 0.793431 | 0 | ОЦЕНКА |
| 8 | depot_price_ir | 12.322620 | 0 | ОЦЕНКА |
| 8 | removed_ir | 104.527958 | 0.062974 | ОЦЕНКА |
| 8 | removed_share | 0.283381 | 0.000171 | ОЦЕНКА |
| 8 | ingress_own_overflow_blocks | 214680 | 0 | ИЗМЕРЕНО |
| 8 | ingress_own_overflow_events | 26835 | 0 | ИЗМЕРЕНО |
| 8 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 8 | ingress_remote_blocks | 9783 | 410 | ИЗМЕРЕНО |
| 8 | ingress_remote_events | 184 | 0 | ИЗМЕРЕНО |
| 8 | flush_swap_top_blocks | 102 | 0 | ИЗМЕРЕНО |
| 8 | flush_teardown_blocks | 682 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_drained | 223009 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_carved | 2318 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_total | 225327 | 0 | ИЗМЕРЕНО |
| 8 | substrate_pushes | 225247 | 410 | ИЗМЕРЕНО |
| 8 | invalidations | 107 | 0 | ИЗМЕРЕНО |
| 8 | unmapped | 0 | 0 | ИЗМЕРЕНО |

### mstress T=4 (reps=3, steps/repeats=100000, Ir/шаг; медиана по повторам, разброс = max−min)

| K | metric | медиана | разброс | kind |
|---|---|---|---|---|
| 1 | absorb_share | 0.275687 | 0 | ОЦЕНКА |
| 1 | refill_depot_share | 0.239407 | 0 | ОЦЕНКА |
| 1 | drain_replace_share | 0.242257 | 0.000321 | ОЦЕНКА |
| 1 | fully_absorbed_ev | 14819 | 0 | ИЗМЕРЕНО |
| 1 | fully_absorbed_ev_share | 0.275687 | 0 | ОЦЕНКА |
| 1 | refill_fully | 0 | 0 | ИЗМЕРЕНО |
| 1 | refill_fully_share | 0 | 0 | ОЦЕНКА |
| 1 | depot_price_ir | 7.092940 | 0 | ОЦЕНКА |
| 1 | removed_ir | 27.059854 | 0.023649 | ОЦЕНКА |
| 1 | removed_share | 0.073361 | 0.000064 | ОЦЕНКА |
| 1 | ingress_own_overflow_blocks | 430024 | 0 | ИЗМЕРЕНО |
| 1 | ingress_own_overflow_events | 53753 | 0 | ИЗМЕРЕНО |
| 1 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 1 | ingress_remote_blocks | 18431 | 670 | ИЗМЕРЕНО |
| 1 | ingress_remote_events | 440 | 0 | ИЗМЕРЕНО |
| 1 | flush_swap_top_blocks | 189 | 0 | ИЗМЕРЕНО |
| 1 | flush_teardown_blocks | 1374 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_drained | 445807 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_carved | 5308 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_total | 451115 | 0 | ИЗМЕРЕНО |
| 1 | substrate_pushes | 450018 | 670 | ИЗМЕРЕНО |
| 1 | invalidations | 200 | 0 | ИЗМЕРЕНО |
| 1 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 2 | absorb_share | 0.501553 | 0 | ОЦЕНКА |
| 2 | refill_depot_share | 0.445208 | 0 | ОЦЕНКА |
| 2 | drain_replace_share | 0.450509 | 0.000597 | ОЦЕНКА |
| 2 | fully_absorbed_ev | 26960 | 0 | ИЗМЕРЕНО |
| 2 | fully_absorbed_ev_share | 0.501553 | 0 | ОЦЕНКА |
| 2 | refill_fully | 11605 | 0 | ИЗМЕРЕНО |
| 2 | refill_fully_share | 0.411219 | 0 | ОЦЕНКА |
| 2 | depot_price_ir | 13.266900 | 0 | ОЦЕНКА |
| 2 | removed_ir | 49.586993 | 0.043979 | ОЦЕНКА |
| 2 | removed_share | 0.134433 | 0.000119 | ОЦЕНКА |
| 2 | ingress_own_overflow_blocks | 430024 | 0 | ИЗМЕРЕНО |
| 2 | ingress_own_overflow_events | 53753 | 0 | ИЗМЕРЕНО |
| 2 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 2 | ingress_remote_blocks | 18431 | 670 | ИЗМЕРЕНО |
| 2 | ingress_remote_events | 440 | 0 | ИЗМЕРЕНО |
| 2 | flush_swap_top_blocks | 189 | 0 | ИЗМЕРЕНО |
| 2 | flush_teardown_blocks | 1374 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_drained | 445807 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_carved | 5308 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_total | 451115 | 0 | ИЗМЕРЕНО |
| 2 | substrate_pushes | 450018 | 670 | ИЗМЕРЕНО |
| 2 | invalidations | 200 | 0 | ИЗМЕРЕНО |
| 2 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 4 | absorb_share | 0.740108 | 0 | ОЦЕНКА |
| 4 | refill_depot_share | 0.664114 | 0 | ОЦЕНКА |
| 4 | drain_replace_share | 0.672022 | 0.000890 | ОЦЕНКА |
| 4 | fully_absorbed_ev | 39783 | 0 | ИЗМЕРЕНО |
| 4 | fully_absorbed_ev_share | 0.740108 | 0 | ОЦЕНКА |
| 4 | refill_fully | 17440 | 0 | ИЗМЕРЕНО |
| 4 | refill_fully_share | 0.617980 | 0 | ОЦЕНКА |
| 4 | depot_price_ir | 19.680140 | 0 | ОЦЕНКА |
| 4 | removed_ir | 73.602273 | 0.065603 | ОЦЕНКА |
| 4 | removed_share | 0.199540 | 0.000178 | ОЦЕНКА |
| 4 | ingress_own_overflow_blocks | 430024 | 0 | ИЗМЕРЕНО |
| 4 | ingress_own_overflow_events | 53753 | 0 | ИЗМЕРЕНО |
| 4 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 4 | ingress_remote_blocks | 18431 | 670 | ИЗМЕРЕНО |
| 4 | ingress_remote_events | 440 | 0 | ИЗМЕРЕНО |
| 4 | flush_swap_top_blocks | 189 | 0 | ИЗМЕРЕНО |
| 4 | flush_teardown_blocks | 1374 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_drained | 445807 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_carved | 5308 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_total | 451115 | 0 | ИЗМЕРЕНО |
| 4 | substrate_pushes | 450018 | 670 | ИЗМЕРЕНО |
| 4 | invalidations | 200 | 0 | ИЗМЕРЕНО |
| 4 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 8 | absorb_share | 0.927167 | 0 | ОЦЕНКА |
| 8 | refill_depot_share | 0.837374 | 0 | ОЦЕНКА |
| 8 | drain_replace_share | 0.847344 | 0.001123 | ОЦЕНКА |
| 8 | fully_absorbed_ev | 49838 | 0 | ИЗМЕРЕНО |
| 8 | fully_absorbed_ev_share | 0.927167 | 0 | ОЦЕНКА |
| 8 | refill_fully | 22516 | 0 | ИЗМЕРЕНО |
| 8 | refill_fully_share | 0.797846 | 0 | ОЦЕНКА |
| 8 | depot_price_ir | 24.740760 | 0 | ОЦЕНКА |
| 8 | removed_ir | 92.521615 | 0.082718 | ОЦЕНКА |
| 8 | removed_share | 0.250831 | 0.000224 | ОЦЕНКА |
| 8 | ingress_own_overflow_blocks | 430024 | 0 | ИЗМЕРЕНО |
| 8 | ingress_own_overflow_events | 53753 | 0 | ИЗМЕРЕНО |
| 8 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 8 | ingress_remote_blocks | 18431 | 670 | ИЗМЕРЕНО |
| 8 | ingress_remote_events | 440 | 0 | ИЗМЕРЕНО |
| 8 | flush_swap_top_blocks | 189 | 0 | ИЗМЕРЕНО |
| 8 | flush_teardown_blocks | 1374 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_drained | 445807 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_carved | 5308 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_total | 451115 | 0 | ИЗМЕРЕНО |
| 8 | substrate_pushes | 450018 | 670 | ИЗМЕРЕНО |
| 8 | invalidations | 200 | 0 | ИЗМЕРЕНО |
| 8 | unmapped | 0 | 0 | ИЗМЕРЕНО |

### churn T=1 (s120, окно счётчиков 120 повторов) (reps=3, steps/repeats=120, Ir/повтор; медиана по повторам, разброс = max−min)

| K | metric | медиана | разброс | kind |
|---|---|---|---|---|
| 1 | absorb_share | 0.032787 | 0 | ОЦЕНКА |
| 1 | refill_depot_share | 0.032749 | 0 | ОЦЕНКА |
| 1 | drain_replace_share | 0.032787 | 0 | ОЦЕНКА |
| 1 | fully_absorbed_ev | 3872 | 0 | ИЗМЕРЕНО |
| 1 | fully_absorbed_ev_share | 0.032787 | 0 | ОЦЕНКА |
| 1 | refill_fully | 0 | 0 | ИЗМЕРЕНО |
| 1 | refill_fully_share | 0 | 0 | ОЦЕНКА |
| 1 | depot_price_ir | 1612.533333 | 0 | ОЦЕНКА |
| 1 | removed_ir | 29278.240275 | 0 | ОЦЕНКА |
| 1 | removed_share | 0.004463 | 0 | ОЦЕНКА |
| 1 | ingress_own_overflow_blocks | 944768 | 0 | ИЗМЕРЕНО |
| 1 | ingress_own_overflow_events | 118096 | 0 | ИЗМЕРЕНО |
| 1 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 1 | ingress_remote_blocks | 0 | 0 | ИЗМЕРЕНО |
| 1 | ingress_remote_events | 0 | 0 | ИЗМЕРЕНО |
| 1 | flush_swap_top_blocks | 0 | 0 | ИЗМЕРЕНО |
| 1 | flush_teardown_blocks | 0 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_drained | 943792 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_carved | 1104 | 0 | ИЗМЕРЕНО |
| 1 | refill_blocks_total | 944896 | 0 | ИЗМЕРЕНО |
| 1 | substrate_pushes | 944768 | 0 | ИЗМЕРЕНО |
| 1 | invalidations | 0 | 0 | ИЗМЕРЕНО |
| 1 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 2 | absorb_share | 0.065574 | 0 | ОЦЕНКА |
| 2 | refill_depot_share | 0.065497 | 0 | ОЦЕНКА |
| 2 | drain_replace_share | 0.065574 | 0 | ОЦЕНКА |
| 2 | fully_absorbed_ev | 7744 | 0 | ИЗМЕРЕНО |
| 2 | fully_absorbed_ev_share | 0.065574 | 0 | ОЦЕНКА |
| 2 | refill_fully | 3868 | 0 | ИЗМЕРЕНО |
| 2 | refill_fully_share | 0.065497 | 0 | ОЦЕНКА |
| 2 | depot_price_ir | 3289.533333 | 0 | ОЦЕНКА |
| 2 | removed_ir | 58492.013882 | 0 | ОЦЕНКА |
| 2 | removed_share | 0.008915 | 0 | ОЦЕНКА |
| 2 | ingress_own_overflow_blocks | 944768 | 0 | ИЗМЕРЕНО |
| 2 | ingress_own_overflow_events | 118096 | 0 | ИЗМЕРЕНО |
| 2 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 2 | ingress_remote_blocks | 0 | 0 | ИЗМЕРЕНО |
| 2 | ingress_remote_events | 0 | 0 | ИЗМЕРЕНО |
| 2 | flush_swap_top_blocks | 0 | 0 | ИЗМЕРЕНО |
| 2 | flush_teardown_blocks | 0 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_drained | 943792 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_carved | 1104 | 0 | ИЗМЕРЕНО |
| 2 | refill_blocks_total | 944896 | 0 | ИЗМЕРЕНО |
| 2 | substrate_pushes | 944768 | 0 | ИЗМЕРЕНО |
| 2 | invalidations | 0 | 0 | ИЗМЕРЕНО |
| 2 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 4 | absorb_share | 0.131148 | 0 | ОЦЕНКА |
| 4 | refill_depot_share | 0.130994 | 0 | ОЦЕНКА |
| 4 | drain_replace_share | 0.131148 | 0 | ОЦЕНКА |
| 4 | fully_absorbed_ev | 15488 | 0 | ИЗМЕРЕНО |
| 4 | fully_absorbed_ev_share | 0.131148 | 0 | ОЦЕНКА |
| 4 | refill_fully | 7736 | 0 | ИЗМЕРЕНО |
| 4 | refill_fully_share | 0.130994 | 0 | ОЦЕНКА |
| 4 | depot_price_ir | 6579.066667 | 0 | ОЦЕНКА |
| 4 | removed_ir | 116984.027765 | 0 | ОЦЕНКА |
| 4 | removed_share | 0.017831 | 0 | ОЦЕНКА |
| 4 | ingress_own_overflow_blocks | 944768 | 0 | ИЗМЕРЕНО |
| 4 | ingress_own_overflow_events | 118096 | 0 | ИЗМЕРЕНО |
| 4 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 4 | ingress_remote_blocks | 0 | 0 | ИЗМЕРЕНО |
| 4 | ingress_remote_events | 0 | 0 | ИЗМЕРЕНО |
| 4 | flush_swap_top_blocks | 0 | 0 | ИЗМЕРЕНО |
| 4 | flush_teardown_blocks | 0 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_drained | 943792 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_carved | 1104 | 0 | ИЗМЕРЕНО |
| 4 | refill_blocks_total | 944896 | 0 | ИЗМЕРЕНО |
| 4 | substrate_pushes | 944768 | 0 | ИЗМЕРЕНО |
| 4 | invalidations | 0 | 0 | ИЗМЕРЕНО |
| 4 | unmapped | 0 | 0 | ИЗМЕРЕНО |
| 8 | absorb_share | 0.262295 | 0 | ОЦЕНКА |
| 8 | refill_depot_share | 0.261989 | 0 | ОЦЕНКА |
| 8 | drain_replace_share | 0.262295 | 0 | ОЦЕНКА |
| 8 | fully_absorbed_ev | 30976 | 0 | ИЗМЕРЕНО |
| 8 | fully_absorbed_ev_share | 0.262295 | 0 | ОЦЕНКА |
| 8 | refill_fully | 15472 | 0 | ИЗМЕРЕНО |
| 8 | refill_fully_share | 0.261989 | 0 | ОЦЕНКА |
| 8 | depot_price_ir | 13158.133333 | 0 | ОЦЕНКА |
| 8 | removed_ir | 233968.055530 | 0 | ОЦЕНКА |
| 8 | removed_share | 0.035662 | 0 | ОЦЕНКА |
| 8 | ingress_own_overflow_blocks | 944768 | 0 | ИЗМЕРЕНО |
| 8 | ingress_own_overflow_events | 118096 | 0 | ИЗМЕРЕНО |
| 8 | ingress_own_batch_blocks | 0 | 0 | ИЗМЕРЕНО |
| 8 | ingress_remote_blocks | 0 | 0 | ИЗМЕРЕНО |
| 8 | ingress_remote_events | 0 | 0 | ИЗМЕРЕНО |
| 8 | flush_swap_top_blocks | 0 | 0 | ИЗМЕРЕНО |
| 8 | flush_teardown_blocks | 0 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_drained | 943792 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_carved | 1104 | 0 | ИЗМЕРЕНО |
| 8 | refill_blocks_total | 944896 | 0 | ИЗМЕРЕНО |
| 8 | substrate_pushes | 944768 | 0 | ИЗМЕРЕНО |
| 8 | invalidations | 0 | 0 | ИЗМЕРЕНО |
| 8 | unmapped | 0 | 0 | ИЗМЕРЕНО |

### churn T=1 (s241, окно счётчиков 241 повтора; чтение + assert identity/арифметики; краткая сводка, К=2)

  absorb_share=0.0656, refill_depot_share=0.0655, drain_replace_share=0.0656, removed_share=0.0089 — согласуется с s120 (масштаб ~1:2 по счётчикам, доли те же).

=== ВЕРДИКТ (порог задан заранее, НЕ менять) ===

  GO ⟺ ∃ K≤2: removed_share ≥ 5% полного marginal Ir хотя бы в mstress T=1 или installed churn+Vec И absorb_share ≥ 50% в той же паре.
  Для churn знаменатель — FULL Ir/повтор (6560789.60, окно 121).

  mstress T=1, K=1: removed_share=0.0875 (32.28 из FULL 368.86 Ir/шаг) — условие A (≥5%): ДА; absorb_share=0.2549 (25.49% (25968/101872)) — условие B (≥50%): нет → не проходит
  mstress T=1, K=2: removed_share=0.1667 (61.48 из FULL 368.86 Ir/шаг) — условие A (≥5%): ДА; absorb_share=0.4860 (48.60% (49512/101872)) — условие B (≥50%): нет → не проходит
  churn (installed, окно счётчиков 120), K=1: removed_share=0.0045 (29278.24 из FULL 6560789.60 Ir/повтор) — условие A (≥5%): нет; absorb_share=0.0328 (3.28% (30976/944768)) — условие B (≥50%): нет → не проходит
  churn (installed, окно счётчиков 120), K=2: removed_share=0.0089 (58492.01 из FULL 6560789.60 Ir/повтор) — условие A (≥5%): нет; absorb_share=0.0656 (6.56% (61952/944768)) — условие B (≥50%): нет → не проходит

ВЕРДИКТ: NO-GO


[ok] CSV записан: docs/perf/B4_DEPOT_PROVENANCE_summary.csv (649 строк данных, 19 колонок)
```

## Воспроизводимость

- Каждый log/summary первой строкой несёт `git_diff_sha256` + `tree_sha256`;
  `tree_sha256` = sha256 СОДЕРЖИМОГО текстового манифеста «путь sha256»
  (НЕ tar): скрипт пересчитывает sha256 `raw/tree_manifest.txt` и
  `raw/tree_manifest_cg_era.txt` через node:crypto и assert'ит совпадение.
  Манифест нового дерева включает 2 строки `wsl-snapshot/` (экстракты
  cg-эры: `examples/dp_depot_cg.rs`, `Cargo.toml.cg-patch`).
- Raw-сырьё санитизировано: приватные пути машины вычищены
  (`/home/<user>/` → `~/`, корень репо → `<repo>`-подобные формы); grep на
  утечки — часть проверок чанка.
- `.snap`-переиспользование: `dp_prof3.sh` кэширует распакованный
  snapshot-каталог между конфигурациями одной точки (сборка дорога,
  callgrind-прогоны дёшевы). Снапшот фиксировался tar'ом НА МОМЕНТ прогона
  (`.rush/wsl/dp-r2-p*.tar` / `dp-r4-c*.tar`) и проверялся манифестом
  (identity-строки в каждом summary + per-run tree_manifest) — кэш не может
  подменить дерево.
- WSL-сборки в `~/ab/tgt-dp-*`; каждый cargo-вызов в WSL — с внешним
  `timeout 900`, каждый callgrind-запуск — с внешним `timeout 1800`.
- Точные команды замеров (как вызывались фактически):
  - счётчики (v4-логи): `bash .rush/wsl/dp_ctr_win4.sh <cN> <larson|mstress|churn> <T|-> <steps|repeats> [rep]`
    — напр. `bash .rush/wsl/dp_ctr_win4.sh r4-c1 larson 1 100000`,
    `bash .rush/wsl/dp_ctr_win4.sh r4-c2 larson 2 100000 r1`,
    `bash .rush/wsl/dp_ctr_win4.sh r4-c7 churn - 120 r1`;
    внутри — tar-снимок дерева + `dp-r4.identity` + WSL-прогон
    `dp_ctr4.sh` (`dp_depot_probe`/`dp_native_mixed_counters` со счётчиками);
  - callgrind: `bash .rush/wsl/dp_prof3_win.sh <larson|mstress|churn> <full|tf|td|ctr>`
    — прогоняет обе точки размера (larson/mstress 50000→100000, churn
    120→241), внутри — WSL-скрипт `dp_prof3.sh` (сборка production или
    counters, callgrind `--toggle-collect` для tf/td,
    `--compress-strings=no --compress-pos=no`, cg_summary + annotate +
    per-run tree_manifest; snapshot-кэш, см. выше);
  - сравнение calls= prod vs ctr выполняет сам скрипт таблицы
    (численно из GROUP-строк; отдельный compare-скрипт устарел).

## Чтение вердикта и ВЕРДИКТ

Порог задан ЗАРАНЕЕ и не менялся:
`GO ⟺ ∃ K≤2: removed_share ≥ 5% полного marginal Ir хотя бы в mstress T=1
или installed churn+Vec И absorb_share ≥ 50% в той же паре` (для churn
знаменатель — FULL Ir/повтор, окно 121).

Разбор всех пар K≤2 (медианы):

| пара | removed_share (ОЦЕНКА) | усл. A ≥5% | absorb_share (ОЦЕНКА) | усл. B ≥50% | итог |
|---|---|---|---|---|---|
| mstress T=1, K=1 | 0.0875 (32.28/368.86 Ir/шаг) | ДА | 0.2549 (25968/101872) | нет | не проходит |
| mstress T=1, K=2 | 0.1667 (61.48/368.86) | ДА | 0.4860 (49512/101872) | нет (−1.4 п.п.) | не проходит |
| churn, K=1 | 0.0045 (29278.24/6560789.60 Ir/повтор) | нет | 0.0328 (30976/944768) | нет | не проходит |
| churn, K=2 | 0.0089 (58492.01/6560789.60) | нет | 0.0656 (61952/944768) | нет | не проходит |

Поправка против прежней формулировки: **larson T=1 при K=2 условие A НЕ
выполняет** — 4.66% < 5% (0.0466 Ir-доли при absorb 0.7842), т.е. даже
вне-пороговый larson не проходит оба неравенства одновременно. MT-конфигурации
larson/mstress дают высокий absorb (T=2 K=2: 0.9094/0.5037), но не входят в
список порога.

**ВЕРДИКТ: NO-GO** (механика: в mstress T=1 бины проворачиваются слишком
быстро — ~101 872 flush-блоков на 100k шагов против ёмкости depot K×8; в
installed churn own-free повторное использование почти всегда старше K≤2
полу-magazine'ов).

## Raw-файлы

`docs/perf/B4_DEPOT_PROVENANCE/raw/`:

- 20 счётчиковых v4-логов (новое дерево 2c8d7c76/5441d8c2):
  `dp-r4-c1.larson.t1.s100000.v4.log`,
  `dp-r4-c{2,3}.larson.{t2,t4}.s100000.r{1..3}.v4.log`,
  `dp-r4-c4.mstress.t1.s100000.v4.log`,
  `dp-r4-c{5,6}.mstress.{t2,t4}.s100000.r{1..3}.v4.log`,
  `dp-r4-c7.churn.t-.s120.r{1..3}.v4.log`,
  `dp-r4-c8.churn.t-.s241.r{1..3}.v4.log`;
- 24 callgrind-сводки v3-эры (e7dc9d00/592481)
  `dp-r2-p1-larson.{full,tf,td,ctr}.{50000,100000}.cg_summary.txt`,
  `dp-r2-p2-mstress.{full,tf,td,ctr}.{50000,100000}.cg_summary.txt`,
  `dp-r2-p3-churn.{full,tf,td,ctr}.{120,241}.cg_summary.txt`
  (+ `.stdout.txt`, `.annotate_full.txt` на каждую точку);
- доказательство паритета: `r4_behavior_parity.txt`;
- манифесты: `tree_manifest.txt` (sha256 содержимого = 5441d8c2…, включает
  2 строки `wsl-snapshot/`), `tree_manifest_cg_era.txt` (= 592481…),
  `tree_manifest_wsl_extras.txt`, per-run копии `tree_manifest_dp-r2-p*.txt`;
- устаревшее v3/v2/v1 (v3-логи счётчиков, calls_compare-тексты) — в
  `raw_obsolete/`; скриптом не читаются (calls= считаются численно из cg).
