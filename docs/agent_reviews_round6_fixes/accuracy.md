# Ревью исправлений раунда 6 — точность (соответствие утверждений фактам)

Дата: 2026-10-16. Ревизия: `main` @ `ac23d06e` (диапазон `132ab85b..HEAD`).
Режим: только чтение. Ничего не собиралось и не запускалось. Числа пересчитаны
`awk`-ом по закоммиченным `*_summary.csv` и `_raw_r6_*.log`. Факты о коде и истории
получены через `git show/log/diff/blame/merge-base/cat-file` и чтение файлов.

## 1. Резюме

- **Цифры гейт-отчётов в целом верны.** Все 10 CSV совпадают с сырыми iai-логами
  построчно (A_Ir и B_Ir, 106/108 строк в каждом). `delta = B − A` и `delta_pct`
  сходятся везде. Счётчики «delta > 0 / < 0 / = 0» верны. Ячейки `mimalloc_*` = 0.
  Все SHA арм A/B существуют. Арм A — это merge-base. В A..B нет чужих коммитов
  в `src/`. Пороги в отчётах совпадают с `.github/gate-run.json` в дереве арма B.
  Вердикты PASS/FAIL следуют из порогов.
- **Главная системная проблема (P1).** Ни один iai-гейт раунда не читал счётчик
  path-activation: стадия `--oracle` раннера — no-op. Три изменения `perf(runtime)`
  (T01, T10, T11) влиты на гейтах, которые по правилу R30-8 из `CLAUDE.md` не
  годятся как GO-доказательство. Нужные счётчики в коде есть.
- **Документация отстала от кода (P2).** После T05/T06/T15/T28 часть текстов
  противоречит коду: политика отказов в `sefer_alloc/mod.rs`, описание
  `hardened` в `Cargo.toml` и README, карточки 166/201/202. Карточки 236/240/245
  пишут OPEN / «не начато» про задачи, которые уже влиты. T14 и T48 ни в один
  индекс не попали.
- **Отчёт T05 упускает самую большую цену.** `growth16_*`: от +0.75 до +1.36 %,
  т. е. +262144…+1048576 Ir. Кроме того, измеренное дерево отличается от
  влитого.
- Мелкие неточности (P3): неверный знаменатель (T28), устаревшая фраза «only later
  commit» (T18), неполные списки «largest change» (T01/T10/T11/T31), устаревшие
  `SAFETY`/doc (`directory.rs:120`, `mem_impl.rs:448`, `size_classes.rs:330`,
  `fallback.rs:445`), README (число вложенных файлов, «reserves 4 MiB», цена T07),
  неверный PR в карточке 230, неполный абзац о закрытиях в индексе.

Итого: P1 — 1, P2 — 8, P3 — 22 (подтверждённые), гипотез — 3.

## 2. Находки

Формат: **файл:строка** — что сказано → что на самом деле (доказательство) →
исправление. Всё ниже подтверждено, кроме раздела «Гипотезы».

### P1

**P1-1. У гейтов раунда нет path-activation oracle. Три `perf(runtime)` влиты на
гейтах, которые по R30-8 не являются GO-доказательством.**
- Сказано. `docs/perf/R6_T01_OWNER_STAMP_REMOVAL_GATE.md`,
  `R6_T10_LARGE_SWEEP_PROBE_GATE.md`, `R6_T11_OWN_CACHE_PAIR_LAYOUT_GATE.md`,
  `R6_T18_*`, `R6_T28_*` и `R6_T31_T32_*` дают вердикт PASS без раздела «Path
  activation». В T05, T12 и T40 такой раздел есть, но он прямо говорит: «no
  per-cell … counter is read by this gate».
- Факт. `scripts/r20_r6_ab_iai_run.mjs:320-325`: «Extension point, deliberately not
  wired… no path-activation oracle is wired for this runner yet». То же сказано в
  `docs/CI_TIERS.md` (раздел «A/B iai runs on a PR»). Счётчики, которые SUMMARY
  назначил оракулами, в коде есть, но не прочитаны:
  - `LARGE_SIDECAR_SLOT_INSPECTIONS` (`src/alloc_core/alloc_core/sidecar_drain.rs:12`) — для T10;
  - `CONTAINS_BASE_TIER1_HITS` (`src/alloc_core/alloc_core/counters.rs:77`) — для T11.
- По `CLAUDE.md` (R30-8): «A judge whose report cites no per-arm
  mechanism-activation evidence is not usable as GO/NO-GO evidence». У T10 нет и
  оси цены, которую требовал `SUMMARY.md` R6-T10: удержание RSS при cross-thread
  Large free в том же режиме (правило R31-1).
- Исправление:
  1. подключить стадию `--oracle` (снимок счётчиков `bench-internals` на ячейку и
     арм) и перегнать T10/T11/T01;
  2. до этого — в каждом отчёте явно написать «PASS по Ir, без path-activation;
     по R30-8 не является GO-доказательством»;
  3. для T10 — открыть пункт с RSS-осью.

### P2

**P2-1. `src/global/sefer_alloc/mod.rs:189-190` — «production does not validate
free-list links».**
- Факт. После R6-T06 production проверяет каждое звено: диапазон
  `[meta_end, min(bump, SEGMENT))` и выравнивание `MIN_BLOCK`. Иначе вызывается
  `fatal`:
  - `src/alloc_core/platform/node.rs:86-108` (`continuation_offset_or_fatal`,
    `#[cfg(not(feature = "hardened"))] continuation_fatal`);
  - вызовы в `alloc_core_small_impl.rs:447`, `:610`;
  - `src/alloc_core/config/continuation_bounds.rs:12-22`.
- Исправление: «production validates every free-list continuation for range and
  `MIN_BLOCK` alignment (abort on violation); `hardened` adds class-stride
  divisibility and cycle detection».

**P2-2. Карточка 201 (`docs/correctness-open-items/TRACKED_correctness_residuals.md:145`) —
«Production does not validate Small free-list links… In `production` a corrupted
`next` is trusted», Status OPEN.**
- Факт: то же, что в P2-1. Карточка противоречит RESOLVED-записи 210
  (`RESOLVED.md:161-166`: «non-hardened pops validate every continuation…»).
- Исправление: закрыть 201 (или сузить до остатка: класс-кратность и подделка
  внутри сегмента — это уже item 247). Перенести в RESOLVED со ссылкой на PR #57.

**P2-3. Карточка 202 (`TRACKED_correctness_residuals.md:152`) — «Only the `hardened`
free-list abort writes a diagnostic first; the other `std::process::abort()` sites
abort silently», Status OPEN.**
- Факт. После R6-T15 (`85eb1edd`, PR #55)
  `grep -rn "process::abort" src` находит единственный вызов — в
  `src/alloc_core/platform/fatal.rs:7`. Остальные совпадения — комментарии в
  `ensure.rs:126,161`. В `mod.rs:191` сказано: «All abort sites now share the
  single `platform::fatal` helper».
- Исправление: закрыть 202 по R6-T15, указать guard `scripts/verify-no-bare-abort.mjs`.

**P2-4. Карточка 166 (`TRACKED_correctness_residuals.md:136-143`) не обновлена после
R6-T28.**
- Сказано: Status OPEN — «uniform leaf returns a nonzero class for EVERY granule…
  interior pointer… abort()». Последняя запись — «Round 6 (2026-10-15)… Tracked as
  task R6-T28».
- Факт: `cb3794aa` (PR #42) — «Class codes are now per granule only; interior and
  never-issued granules read 0 and the producer drops the record before the
  pending RMW». Это подтверждает README «Honest limitations» («Cross-thread frees
  of interior pointers are dropped»). Механизм карточки устранён. Остался узкий
  случай: смещение совпадает с началом живого блока (README).
- Исправление: обновить current-state card (CLOSED или PARTIAL с этим остатком),
  указать гейт T28 и `tests/r6_u21_foreign_free_interior_granule.rs`.

**P2-5. Описание `hardened` устарело после T05/T06/T28.**
- `Cargo.toml:671-688`. Сказано:
  - «Currently gates the interior-pointer free guard… `off % block_size(class) == 0`…
    hence gated here, not on the production hot path»;
  - «a cross-thread interior free that matches a stale-but-consistent class map can
    still corrupt — round-6 review R6-SEC-2».
- `README.md:1474` (строка Features matrix, на которую теперь ссылается
  `Cargo.toml`). Сказано: «Opt-in own-thread interior-pointer guard, rejecting
  non-block starts; detected Small free-list corruption… aborts».
- Факт:
  - default-сборка уже отвергает смещения, не кратные классу
    (`dealloc_own_base.rs:414-419`, `SizeClasses::divides`), и уже прерывается на
    плохом звене (P2-1);
  - `hardened` добавляет точную сверку класса по class map (`:410-412`),
    кратность звена и детекцию цикла;
  - сценарий «stale-but-consistent class map» снят T28 (`cb3794aa`: «a reset
    segment gets a fresh sidecar», внутренние гранулы читаются как 0).
- Исправление: переписать обе формулировки по текущему коду (default = кратность +
  диапазон звена; `hardened` = точный класс + кратность звена + цикл; interior
  cross-thread free отбрасывается в любой сборке).

**P2-6. Отчёт T05 (`docs/perf/R6_T05_T06_T33_FREE_VALIDATION_GATE.md:79-82`)
занижает цену. Арм B ≠ влитый код.**
- Сказано: «The default build pays +128 Ir on `dealloc_free_only_16b` (0.19 %) and
  +590 Ir on `recycle_alloc_only_256x16b` (0.31 %)».
- Факт (CSV того же отчёта): самые большие относительные дельты всего гейта не
  упомянуты нигде в отчёте. Рост ровно ×2 на каждом шаге: цена растёт вместе с
  работой.

  | ячейка | delta | A_Ir | delta / A |
  |---|---:|---:|---:|
  | `growth16_n` | +262144 | 19241175 | +1.36 % |
  | `growth16_2n` | +524288 | 48924121 | +1.07 % |
  | `growth16_4n` | +1048576 | 139747430 | +0.75 % |
  | `seg_cycle_decommit_256k` | +4821 | — | — |
- Измеренное ≠ влитое. В арме B `49d3dc9c` было `#[inline(always)]` на
  `ClassLeaves::encoded` (добавлено в `050d1b2e`). В арме A `d3941723` и в HEAD его
  нет: после измерения его снял `a6dd5161` («drop the inline-only edit on
  ClassLeaves::encoded»). `encoded` стоит на default-пути публикации и слива:
  `sidecar_bitmap.rs:111`, `bitmap_cut.rs:27`, `small_sidecar.rs:116,127`.
- Исправление:
  - добавить в отчёт строку о `growth16_*` (с числителем и знаменателем);
  - в «Identity» отметить отличие арма B от влитого дерева;
  - перемерить HEAD-эквивалент или обосновать, почему атрибут не влияет на цену.

**P2-7. Карточки пишут OPEN / «не начато» про влитые задачи (правило R34-24
«current-state»).**
- Карточка 240, `docs/correctness-open-items/TRACKED_misc.md:133`. Сказано:
  «R6-T19 long-function work is excluded and not started». Факт: `973a2212`
  «refactor: split segment discovery and outline cold Large paths (R6-T19)»
  влит PR #61 (`dc8e38dc`).
- Карточка 245, `TRACKED_misc.md:173`. Сказано: «OPEN — tracked; queued as round-6
  tasks R6-T20 and R6-T46». Факт: T20 — `c6834e15`, `8d3291b6` (PR #53, `42551685`);
  T46 — `004ad7e0` (PR #59, `145c750e`).
- Карточка 236, `docs/correctness-open-items/TRACKED_publish_readiness.md:998-999`.
  Сказано: «the crate does not enforce `missing_docs`». Факт: в `src/lib.rs:320`
  стоит `#![warn(missing_docs)]` (`004ad7e0`, T49). Doc-hidden модули получили
  `#[allow(missing_docs)]` (`lib.rs:446,478,491`).
- Исправление: перевести три карточки в PARTIAL или CLOSED с SHA и PR. Оставить
  только реальный остаток: T19 — `primordial`, `HeapCore::realloc`, `claim_impl`
  не тронуты (тело `973a2212`); 236 — `allow` на doc-hidden модулях.

**P2-8. Находки T14 (R6-CQ-29, R6-MS-2, R6-SEC-14) и T48 (R6-CQ-30) не внесены ни в
один индекс.**
- Сказано. `docs/CORRECTNESS_OPEN_ITEMS.md:140`: «Findings filed as items 206–246».
  `SUMMARY.md` R6-T21 требует внести все CQ/MS/SEC.
- Факт. Ни одна карточка 206–247 не о паниках на путях `GlobalAlloc` (T14) и не о
  разной реакции abort против `None` (T48). Заголовки карточек проверены по
  `grep -nE "^(20[6-9]|2[1-4][0-9])\. "` во всех `correctness-open-items/*.md`.
  ID `CQ-28/29/30`, `MS-2`, `SEC-14` в индексах не встречаются. T14/T15 влиты
  (`85eb1edd`), T48 — частично (`004ad7e0`, «kind contracts»), но следа закрытия
  нет.
- Исправление: завести карточки сразу в RESOLVED или PARTIAL со ссылками на
  `85eb1edd`/`004ad7e0`. Остаток T14 (debug-assert/indexing, `mod.rs:199-201`)
  вынести в карточку.

### P3

**P3-1. `docs/perf/R6_T28_START_GRANULE_CLASS_LEAVES_GATE.md:47`.** Сказано:
«`multiseg_cold_256k` +3152 of 4028198». Факт: в CSV `A_Ir` = 4025046, а
`B_Ir` = 4028198. Знаменатель взят из B; в остальных строках отчёта — из A.
Исправить: «+3152 of 4025046».

**P3-2. `docs/perf/R6_T18_SIDECAR_DRAIN_UNIFY_GATE.md:16-18`.** Сказано: «The only
later source commit on the PR adds `let _ = nonempty;`». Факт: после арма B
`00605848` в PR #56 два source-коммита: `8fb236de` (`nonempty`) и `578000ea`
(`let _ = (changed_classes, index); let _ = base;` в `sidecar_drain.rs:221-226`).
`578000ea` сделан в 07:29 — после отчёта (`ec9d60a7`, 07:12). Оба cfg-only для
сборок без `alloc-segment-directory`/`alloc-decommit`, на production не влияют.
Исправить: перечислить оба.

**P3-3. `docs/perf/R6_T11_OWN_CACHE_PAIR_LAYOUT_GATE.md:35-38, 42`.**
- Сказано: «Informational cells with the largest relative change: `growth16_n`
  −77814…, `oscillating_live_set_16b` −290…». Факт: самая большая относительная
  дельта — `carve_batch_only_16b` +15 of 2727 (+0.55 %). Это выше уровня 0.5 %,
  которым гейт ограничивал другие ячейки, а в отчёте её нет.
- Сказано: «The expected ~1–2 Ir per lookup shows up». Факт: 64 вызова на раунд
  (`CHURN_OPS = 64`, `benches/perf_gate_iai.rs:102`); в сыром виде
  −155 / 64 ≈ −2.4 Ir. С поправкой на `dealloc_prealloc_only_16b` (+36) выходит
  (−155 − 36) / 64 ≈ −3.0 Ir на lookup. Цифра не выведена.
- В архиве (`OPEN_ITEMS_ARCHIVE.md`, § 92) сказано «cold/recycle carves
  +0.1–0.16%». Факт: `cold_alloc_only_256x16b` +96 of 113121 (+0.085 %).
- Исправить: добавить `carve_batch_only_16b`, вывести Ir/lookup арифметикой, в
  архиве написать «+0.08–0.16 %».

**P3-4. `docs/perf/R6_T31_T32_POOL_DECAY_DIRECTORY_REBUILD_GATE.md:40-43, 48-49`.**
Сказано: «Largest informational costs: `large_cache_eviction_hit_only_41x4mib`…,
`large_cache_free_slot_search_cycle_only`…, the 1088-block free cells +1564 of
about 325000» и «costs a few hundred `Ir` per bench round on the Large and
segment-discovery paths». Факт:
- `growth16_n/_2n/_4n` +47104 / +94208 / +188416 (рост линейный);
  `seg_cycle_decommit_256k` +4155;
- по относительной величине пропущена `large_cache_eviction_prefill_only_41x4mib`
  +1312 of 187971 (+0.70 %);
- в 1088-группе есть +1564 of 458078 (`_n1024`).

«A few hundred Ir» неверно для segment-discovery ячеек. Исправить: добавить
`growth16_*` и уточнить формулировку вердикта.

**P3-5. `docs/perf/R6_T10_LARGE_SWEEP_PROBE_GATE.md:59-60`.** Сказано: «37 of 106
cells grew, the largest by +300 of 77059 (`oscillating_live_set_16b`)». Факт:
это самая большая относительная дельта (+0.39 %). Самая большая абсолютная —
`cold_alloc_free_256x16b_4n` +640. Исправить: «largest relative».

**P3-6. `docs/perf/R6_T01_OWNER_STAMP_REMOVAL_GATE.md:38-39`.** Сказано: «Largest
relative change: the 1088-block free cells, e.g. `dealloc_free_only_1088_16b_n17`
−15441 of 341233». Факт: максимум у `dealloc_prealloc_only_1088_16b` −15404 of
339579 (−4.536 %); у `n17` −4.525 %. Мелочь. Исправить: назвать максимум.

**P3-7. `src/registry/segment_route/directory.rs:120`.** Сказано в SAFETY:
«pointer came from System.alloc_zeroed with this Layout». Факт: с OPT-04 (R6-T40,
`directory.rs:57`) хранилище берётся из `System.alloc`. Довод о том же `Layout`
по-прежнему верен, текст — нет. Попутно: строка `:84` называет тип `Descriptor`,
а тип — `Entry`; это было до раунда. Исправить: «from `System.alloc`»,
`Entry`.

**P3-8. `src/alloc_core/alloc_core/mem/mem_impl.rs:448`.** Сказано: «the same pattern
`SegmentMeta::owner_state_atomic` already uses for cross-thread owner-state reads».
Факт: `owner_state_atomic` удалён в `1ca5b436` (R6-T01 follow-up). Производственных
кросс-поточных чтений `owner_state` нет: `hot.rs:200`, `realloc.rs:75`. Исправить:
сослаться на другой существующий atomic-view или убрать сравнение.

**P3-9. `src/alloc_core/platform/size_classes.rs:330`.** Сказано: «Shared R6-T12
entry; continuation walks consume this predicate with cached parameters». Факт:
R6-T12 — NO-GO, код откатан (`6b148949`). У `divides`/`divides_with` есть только
два вызова: `dealloc_own_base.rs:415` (own-free, T05) и `segment_layout.rs:23`
(`dbg_`). В обходах цепочек вызова нет: кратность звена ушла в `hardened`
(T05-отчёт). Исправить: «R6-T05 own-thread free stride check».

**P3-10. `src/global/fallback.rs:445-452` и perf item 66.**
- В doc `LOCK_TIGHT_SPINS` сказано: «number of tight `core::hint::spin_loop()`
  iterations». Факт после T26 (`a800d040`, `fallback.rs:506-541`): это 64
  TTAS-раунда с паузой `1 << min(backoff, 6)` `spin_loop` каждый. Всего
  1 + 2 + 4 + 8 + 16 + 32 + 58·64 = 3775 пауз до первого `yield`.
- `docs/perf/OPEN_ITEMS.md:2985` (item 66). Сказано: «64 tight `compare_exchange` +
  `spin_loop` attempts, then `yield_now()` per attempt». Это устарело по той же
  причине.
- `OPEN_ITEMS.md:2988`. Сказано: «The threshold is not in the README "Honest
  limitations"». Факт: после T07 в README появился пункт `README.md:1748-1755`.
- Исправить: обновить doc константы и карточку 66.

**P3-11. `src/registry/heap_core/free/dealloc_own_base.rs:344`.** Сказано в SAFETY
(T19 перенёс текст в новый `route_to_large_free`): «reached only from
`HeapCore::dealloc`». Факт: тело `dealloc_own_thread_with_base` вызывается также
из `realloc.rs:425`, `realloc.rs:725` и `diag_probes.rs:288`. Все эти вызовы —
`unsafe`-контексты, так что довод держится, неверен только перечень. Исправить:
«reached from `HeapCore::dealloc`/`realloc` (both `unsafe fn`) and the
`bench-internals` probe».

**P3-12. `README.md:174` и `:1761`.** Сказано: «a 260 KiB request reserves and
commits 4 MiB». Факт: коммитится 4 MiB, но резерв адресного пространства —
~8 MiB на объект. Это показывает и замер T08 (`r6_t08_runs/summary.csv`: A
8192 KiB/объект), и сам README в том же абзаце (`:190-193`). На Windows
двухвызовный путь резервирует `size + align`
(`crates/aligned-vmem/src/os/windows.rs:40-46`). Исправить: «commits 4 MiB
(reserves ~8 MiB of address space)».

**P3-13. `README.md:1427`.** Сказано: «The test tree also contains 15 nested Rust
source files». Факт: `git ls-tree -r HEAD tests | grep -E '^tests/.+/.+\.rs$'` даёт
16 (на `132ab85b` было 15). Остальные счётчики того же абзаца раунд обновил,
а этот — нет.

**P3-14. `README.md:1752-1753` (T07).** Сказано: «the per-allocation cost is one TLS
check plus (… or at most every 64th allocation) one claim retry». Факт: на каждой
аллокации ещё вызывается `ensure()` и делается Acquire-load общего
`SaturationHint` (`tls_heap.rs:135-136`, `saturation.rs:33-35`). Повтор
разрешается, когда пропущено `skips >= 64` попыток (`tls_heap.rs:114`), т. е. на
65-й. Исправить формулировку.

**P3-15. Карточка 230 (`TRACKED_platform_contracts.md:367`).** Сказано: «R6-T26
TTAS/backoff landed (PR #42, `a800d040`…)». Факт: `a800d040` влит PR #46
(`c398bfc7`, r6-u19). PR #42 его не вносил (`git merge-base --is-ancestor` по
первым родителям). Исправить: PR #46.

**P3-16. `docs/CORRECTNESS_OPEN_ITEMS.md:142`.** Сказано: «Items 206–208 are now
CLOSED…». Факт: lookup-таблица (`:573-614`) направляет в `RESOLVED.md` также
209–214, 216–220, 223–225, 232, 233 (`49d3dc9c`, `29a912b0`). Исправить абзац.

**P3-17. Perf-индекс: структурные огрехи по R34-24.**
- `docs/perf/OPEN_ITEMS.md:3025-3031` (item 72): Status CLOSED, но полная карточка
  стоит в tier-разделе `[L]` (`:1178`), и в ней старое «Current-number» без
  измеренных чисел. Указатель в «Recently resolved» при этом тоже есть. Карточку
  нужно убрать из tier-раздела.
- `:3086-3092` (item 96, PF-8): сказано «queued as R6-T39», «Next trigger: task
  R6-T39». T39 влит без PF-8, поэтому триггер устарел.
- `:278` (item 4): сказано, что SAFETY `GlobalAlloc` «wrongly asserts "only null is
  true OOM"». T17 это исправил: `global_alloc.rs:61-74` перечисляет источники null,
  включая `MAX_SEGMENTS`.
- Item 97: сказано «TTAS/backoff landed for the worker CAS path». В
  `try_maintenance_at` есть TTAS-load (`claim.rs:171`), но backoff нет.

**P3-18. Префиксы коммитов (R30-12).**
- `e88c3bce` `perf(runtime)` (T12) и `25c509bb` `perf(runtime)` (T40, OPT-05) остались
  в истории `main`, хотя их runtime-часть откатана (`6b148949`, `e966076b`).
  Читатель `git log` видит два `perf(runtime)`, которые не поставляются. В
  поставку из `25c509bb` ушёл только OPT-04 с вердиктом NULL.
- `a800d040` помечен `fix(perf)`, хотя меняет алгоритм ожидания в shipping-локах и
  частоту exit-пробы: отказ откладывается до 64 yield-раундов. По таксономии
  `fix(perf)` — это «no runtime algorithm… OBSERVABLE behavior changed». Здесь
  точнее `perf(runtime)` с «no speedup claimed».
- `e966076b`: сказано «+~113 Ir per drain batch». Отчёт T40 говорит «per refill,
  including cold cells whose refills find an empty free list» — это не «per drain
  batch».
- Историю не переписывать. Отметить в round-manifest.

**P3-19. Неизменяемая идентичность.**
- T11: арм B `ec8fcbab` недостижим ни из одной локальной ссылки
  (`git for-each-ref --contains` пуст; PR #43 влит squash-ом). Объект подлежит GC.
  Я проверил: src-патч `38bef361..ec8fcbab` побайтно равен патчу
  `4d8e059c..26c1ae16`.
- T08: HEAD `8b05d2a8` (PR #44, squash) тоже недостижим, хотя tree
  `07101c0f…` указан и существует.
- Исправить: в отчётах сослаться на влитой squash-коммит и записать эквивалентность
  патчей. Либо закрепить SHA ссылкой (тег или ветку).

**P3-20. Гейт T40: verdict-лог не закоммичен.** У остальных гейтов есть
`_raw_r6_*_verdict.log`. Сказано: «VERDICT FAIL (2 of 106 cells over threshold)».
Пороги восстанавливаются только из `.github/gate-run.json` в деревьях
`26843755`/`e966076b`. Мой пересчёт совпал: выше 0.5 % два порогированные ячейки —
`cold_alloc_only_256x16b` +1.76 % и `dealloc_free_only_16b` +0.68 %. Исправить:
добавить `_raw_r6_t40_run{1,2}_verdict.log`.

**P3-21. Атрибуция помощника делимости.** Сказано в
`docs/perf/R6_T12_OWNER_DRAIN_CONTEXT_GATE.md:70-71` и `OPEN_ITEMS_ARCHIVE.md`
§ 93: «A division-free class divisibility helper arrives separately with R6-T06».
Факт: в default его использует проверка own-free из R6-T05
(`dealloc_own_base.rs:415`). Кратность звена R6-T06 ушла в `hardened` (T05-отчёт,
`:11-13`, `:75-76`). Исправить: «R6-T05».

**P3-22. Нет round-manifest и записей `CHANGELOG.md` за раунд.** В раунде пять
`perf(runtime)`-коммитов, но `docs/perf/round-manifests/` не содержит манифеста
этого раунда, а `CHANGELOG.md` в `132ab85b..HEAD` не менялся. Если раунд
закрывается позже, это не дефект. Иначе правило «Per-round manifest artifact»
(R34-24) не выполнено.

### Гипотезы (не подтверждены)

- **Г-1. Эффект `#[inline(always)]` из P2-6.** Влияет ли снятый атрибут на
  `ClassLeaves::encoded` на Ir ячеек публикации и слива, не измерено. Это
  возможное расхождение «измерено → влито», не доказанное.
- **Г-2. Interior free в сегмент fallback-владельца.** README («Cross-thread frees
  of interior pointers are dropped… at the producer») может не покрывать этот
  случай. В `routing.rs:35-43` такой free идёт через `heap.dealloc` (own-thread путь
  fallback), а не через `publish_small`. Отбрасывается ли он там и считается ли в
  `foreign_or_unroutable_frees`, не проверено.
- **Г-3. Падение Ir в T18 на 5–16 %.** Причина в отчёте не объяснена. Префикс
  `refactor:` и формулировка «no behavior change» верны, только если те же счётчики
  записей совпадают. Отчёт ссылается на oracle-тесты, но их прогон в артефактах
  гейта не зафиксирован.

## 3. Что проверено и точно

- **CSV ↔ сырые логи.** Все 10 `R6_*_summary.csv` совпадают с
  `_raw_r6_*_iai_{A,B}.log` по каждой ячейке (A_Ir и B_Ir). T10 — по обоим
  прогонам (perf_gate + live16).
- **Арифметика.**
  - `delta = B − A` и `delta_pct` верны во всех 1062 строках.
  - Счётчики `delta >/</= 0` в T05 (45/4/57), T12 (37/16/53), T40 run 1 (47/26/33)
    и run 2 (0/72/34) верны.
  - T10: «37 grew / 35 shrank» верно для run 1. (114091 − 78517) / 16 = 2223 и
    (100308 − 74899) / 16 = 1588 верны.
  - T40: 9728 / 256 = 38. 112, 451, 1807, 7683 ≈ 1, 4, 16, 68 × 113. −9728 / 193028
    = −5.04 %.
  - T08: 8192 / 4360 ≈ 1.88. amplif 31.51 / 16.77. Латентность B/A от 0.999 до
    1.056 (±6 %).
  - T28: −1024 на всех девяти Large-ячейках.
  - T18: семь ячеек ниже −1000 Ir; максимум положительной дельты +8.
  - T31: 72 выросли, 0 упали.
- **SHA.** Все 15 SHA армов существуют (`git cat-file -t` = commit). Арм A = merge-base
  во всех 11 прогонах. `git log A..B -- src` содержит только коммиты своего PR.
  Влитой src-патч совпадает с измеренным:
  - T01, T10, T12 (откат), T31, T40 run 2 (только OPT-04: `directory.rs:57`, `:62`);
  - T11 — побайтно со squash `26c1ae16`;
  - исключения — P2-6 (T05) и P3-2 (T18, cfg-only).
- **Пороги** в отчётах T01, T05, T10, T11, T12, T18, T28, T31 и T40 совпадают с
  `gate-run.json` в дереве арма B. Вердикты PASS/FAIL/NO-GO следуют из порогов. У
  T12 NO-GO стоит при PASS порогов по заявленной причине: нет снижения Ir.
- **Entry layer.** `perf_gate_iai` вызывает `HeapCore::alloc`/`dealloc` через
  `SeferAlloc::new()` (`benches/perf_gate_iai.rs:69,167,305`), как и указано в
  отчётах.
- **Описания кода в отчётах верны:**
  - T01: stamp и `last_stamped_segment` удалены, остался один проход;
  - T10: `LARGE_HOT_BUDGET = 4` (`sidecar_drain.rs:70`), полный sweep — только без
    `fastbin` (`hot.rs:368-373`, `:617-622`);
  - T13: маска `& (TCACHE_CAP - 1)` (`hot.rs:275,422`, `dealloc_own_base.rs:447`) и
    `const`-assert (`tcache.rs:80-83`);
  - T05/T06: `ContinuationBounds`;
  - T28: 256-байтовый лист (`tests/r11_ph3a_issue_transaction_batch.rs:173-177`,
    `LEAF_GRANULES = 256`);
  - item 100: `FALLBACK_IDLE_VISITS = 100` × `PERIOD = 10 ms` ≈ 1 с;
  - T35: таймаут и `fatal` в `MaintenanceService::start`;
  - T40: `System.alloc` в `EntryHandle::new`, тест OPT-05 удалён.
- **Инвентарь unsafe.** README «27 tier-1 (21 src + 6 crates), 97 tier-2 в 31 файле»
  совпадает с `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]'`. Список
  «fifteen» production-швов совпадает с фактическими файлами. В строках tier-2
  верны `tls_heap.rs` = 1 и `diag/queries.rs` = 1. Запись `src/lib.rs:225` о T37
  верна.
- **Счётчики README и ARCHITECTURE:** 421 тест, 96 examples, 33 benches, 19 loom
  (список из 19 файлов совпадает).
- **`docs/CI_TIERS.md`, ссылки на строки:**
  - `ci.yml:2965`, `:1862-1885`;
  - `maintenance_service.rs:48-59`, `:250-320`;
  - `fallback.rs:645`, `:723-733`;
  - `tests/regression_fallback_panic_lock.rs:31`,
    `tests/regression_fallback_init_unwind_guard.rs:51`;
  - `bitmap_scan.rs:4-60`;
  - утверждение «единственная строка `production` без `internals` —
    `msrv-runtime-windows`» подтверждено grep-ом `ci.yml`.
- **Цитируемые тесты существуют:** сценарии `shard_realloc`/`own_oom`/`exact_shard` в
  `con01_exit_stranded_lock.rs`, тесты из RESOLVED 206–225, `r6_t31_*`, `r6_t27_*`,
  `segment_table_own_cache_pair_layout.rs`.
- **SHA в карточках существуют:** `cb3794aa`, `a99440c8`, `4167b7cd`, `dbf46c39`,
  `4d12e199`, `85eb1edd`, `166e3ab1`, `894cdc17`, `e4e46bf6` и другие.
- **Префиксы** `1730c1ca`, `26c1ae16`, `713581b2`, `b86ab03a` (`perf(runtime)`),
  `bench`-отчёты, `refactor:` для T18/T19/T20/T46 соответствуют R30-12. Исключения —
  в P3-18.
- **Правило R26-4** (конфигурационный sweep) к гейтам раунда неприменимо: свипов
  `with_config` нет.

## 4. Ограничения

- Ничего не запускалось. Тесты, oracle-тесты, iai и CI не воспроизводились.
  Номера CI-прогонов (`38871705522`, `38848117681` и др.) и «green on landing SHA»
  офлайн не проверяемы.
- Сырые логи сверены с CSV, но не с бинарями: хэши `builds.json` T08 не
  перепроверялись.
- `src/`-диф раунда (≈ 3.4 тыс. добавленных строк) прочитан выборочно: все
  добавленные `// SAFETY:` и места, на которые ссылаются отчёты и карточки.
  Полной переписи comment-ссылок (~135 мест из R6-T17) не делалось.
- Вне области ревью: R6-T16 (PR #64), T41/43/44/45, T55–T57, T02. Отчёт
  `R6_CROSS_VERSION_BENCH.md` датирован 2026-07-16, он из другого раунда и не
  проверялся.
- Сверка ID находок с карточками сделана grep-ом по ID и заголовкам. Карточка,
  описывающая находку без ID и другими словами, могла остаться незамеченной.
  Вывод P2-8 опирается на проверку заголовков всех карточек 206–247.
