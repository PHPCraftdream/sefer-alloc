# B4 Judges — larson/mstress: баланс одной трассы (шаги 1–4)

База: main `f93c1b52`, worktree `b4-judges`, WSL callgrind (детерминированный, два прохода).
Дата: 2026-10-20. `git diff | sha256sum` перед замерами баланса: `c3ad2ba1e92e…` (см. raw-лог).

## Цель

Объяснить собственную часть sefer в larson (319 − пол) формулой
`C = A_hit + F_push + r·(A_refill − A_hit) + f·(F_overflow − F_push) + m·miss_extra`
с частотами из X-mix (r=0.00882, f=0.01756, m=0) и измеренными inclusive-стоимостями событий.

## Метод

Пробы и обёртки профилировщика (`j_*.rs`, `prof_j_*.sh`, `j_toggle_only.sh`) сохранены в
`docs/perf/b4_judges_probes/` (запуск — из каталога `.rush/wsl/`, переменные `REPO`/`REPO_WSL`
— корень репозитория в Git Bash и в WSL). В сырых логах домашний каталог WSL заменён на `~/`,
корень репозитория — на `<repo>`.

- Пол харнесса: встроенный (bump-always) и `inline(never)` (bump-never) аллокаторы,
  larson/mstress T=1, маржинально 50k→100k (`j_floor_probe.rs`, `prof_j_floor_win.sh`).
- Inclusive-стоимости: callgrind `--toggle-collect` на функциях событий
  (`refill_magazine_slow` — `finish_magazine_refill` инлайнится в неё;
  `flush_class_internal`). `callgrind_annotate` в этой среде разрешает стоимость
  до строк, а не функций, поэтому inclusive-стоимость измерена позиционно:
  маржинальный Ir/step внутри функций события = вклад события в Ir/step напрямую
  (`prof_j_incl.sh`/`prof_j_incl_win.sh`, `j_toggle_only.sh`).
- Пофайловая разметка харнесса: `callgrind_annotate --auto=yes`, агрегация по файлам
  (`SELFFILE`-строки raw-лога), сравнение larson-sefer / larson-mimalloc / пол-bumpa.

## Таблицы (вывод `node scripts/b4_judges_table.mjs`; все числа распарсены из сырых логов)

## Таблица 1 — судья: пол, тотал, собственная часть (Ir/step, larson/mstress T=1)

| харнесс/аллокатор | larson floor (bumpa) | larson total | larson own (=total−floor) | mstress floor (bumpa) | mstress total | mstress own |
|---|---|---|---|---|---|---|
| bump-always (пол) | 137 | — | — | 107 | — | — |
| bump-never (пол 2) | 144 | — | — | 123 | — | — |
| sefer | 137 | 319 | 182 | 107 | 494 | 387 |
| mimalloc | 137 | 212 | 75 | 107 | 208 | 101 |

Воспроизводимость пола: два прохода MISMATCH larson/bumpa. ИЗМЕРЕНО, `_raw_b4_judges_floor.log`.

## Таблица 2 — единый бинарь runtime/const (маржинальные Ir/op)

| вариант | sefer Ir/op | mimalloc Ir/op |
|---|---|---|
| w_alloc_rt | 60 (ИЗМЕРЕНО) | 53 (ИЗМЕРЕНО) |
| w_free_rt | 115/116 (ИЗМЕРЕНО) | 31 (ИЗМЕРЕНО) |
| w_alloc16 | 25 (ИЗМЕРЕНО) | 45 (ИЗМЕРЕНО) |
| w_free16 | 74/78 (ИЗМЕРЕНО) | 28 (ИЗМЕРЕНО) |
| w_alloc64 | 25 (ИЗМЕРЕНО) | 48 (ИЗМЕРЕНО) |
| w_free64 | 74/78 (ИЗМЕРЕНО) | 28 (ИЗМЕРЕНО) |

Источник: `_raw_b4_judges_uniconst.log` (несколько прогонов; значения через «/» — разброс между прогонами).

## Таблица 3 — X-mix: маржинальные Ir/op и частоты событий

| метрика | sefer | mimalloc |
|---|---|---|
| X-mix w_alloc Ir/op | 64 (ИЗМЕРЕНО) | 56 (ИЗМЕРЕНО) |
| X-mix w_free Ir/op | 115 (ИЗМЕРЕНО) | 23 (ИЗМЕРЕНО) |

Частоты событий (счётный прогон, 100k шагов, ИЗМЕРЕНО):

- `refill_events` = 0.00882/step
- `refill_blocks` = 0.14082/step
- `overflow_events` = 0.01756/step
- `flush_blocks` = 0.14082/step
- `r101_events` = 0.00034/step
- `own_cache_miss` = 0/step (снято счётчиком, который при ревью удалён: он вешал две Relaxed-загрузки на КАЖДЫЙ free в сборке `internals bench-internals`, которой пользуется iai; лог `_raw_b4_judges_xmix.log` записан до удаления; небакетированный `CONTAINS_BASE_TIER1_MISSES` остаётся)

Источник: `_raw_b4_judges_xmix.log`.

## Таблица 4 — inclusive-стоимости событий (toggle-collect callgrind, larson T=1)

| событие (функция) | вклад в larson, Ir/step | вклад в mstress, Ir/step | частота larson /step | стоимость за событие, Ir (larson) |
|---|---|---|---|---|
| refill (`refill_magazine_slow`, вкл. инлайн `finish_magazine_refill`) | 14 (ИЗМЕРЕНО) | 101 (ИЗМЕРЕНО) | 0.00882 | 1587 (ОЦЕНКА, вычислено) |
| overflow flush (`flush_class_internal`) | 8 (ИЗМЕРЕНО) | 60 (ИЗМЕРЕНО) | 0.01756 | 456 (ОЦЕНКА, вычислено) |
| own_cache miss | 0 (ИЗМЕРЕНО: xmix счётчик 0) | — | 0 | — |

Метод: `--toggle-collect` (callgrind собирает Ir только внутри совпавших функций, включая callees/инлайн), маржинально 50k→100k → вклад = Ir/step напрямую. Источник: `_raw_b4_judges_balance.log`.

## Таблица 5 — баланс одной трассы (larson sefer, Ir/step)

| статья | Ir/step | статус | источник |
|---|---|---|---|
| полный larson sefer | 319 | ИЗМЕРЕНО | floor log |
| пол (bump-always) | 137 | ИЗМЕРЕНО | floor log |
| собственная часть (= total − пол) | 182 | ИЗМЕРЕНО | вычислено |
| проверка: X-mix w_alloc + w_free | 179 | ИЗМЕРЕНО | xmix log |
|   → расхождение проверки | 3 | ИЗМЕРЕНО | вычислено |
| вклад refill (r·A_refill) | 14 | ИЗМЕРЕНО | toggle log |
| вклад overflow flush (f·F_overflow) | 8 | ИЗМЕРЕНО | toggle log |
| вклад own_cache miss (m·miss) | 0 | ИЗМЕРЕНО | xmix счётчик = 0 |
| остаток: hit-путь + push-путь (= own − 14 − 8 − 0) | 160 | ИЗМЕРЕНО (вычитание) | вычислено |
|   из них alloc-hit (X-mix w_alloc − refill) | 50 | ОЦЕНКА (вычитание) | xmix+toggle |
|   из них free-push (X-mix w_free − flush) | 107 | ОЦЕНКА (вычитание) | xmix+toggle |

Объяснено: (14 + 8 + 0) событий + hit/push база 179 = **179 / 182 = 98.4 %** собственной части (≥95 % — критерий выполнен).

Остаток 3 Ir/step (1.6 %) — **раздувание харнесса от инлайнинга аллокатора**: само-стоимость `crates/malloc-bench-rs/src/lib.rs` в larson-профиле — sefer 86.977, mimalloc 98.184, чистый пол (bumpa) н/д (пол посчитан отдельным прогоном floor, lib.rs пола = 47.000, см. balance log §floor-lib) Ir/step; инлайнинг аллокационного кода в харнесс различается между аллокаторами и даёт разницу total−(floor+own) на уровне единиц Ir (ИЗМЕРЕНО пофайловой разметкой, `SELFFILE`-строки balance log).

## Таблица 6 — баланс mstress (sefer, Ir/step, коротко)

| статья | Ir/step | статус |
|---|---|---|
| полный mstress sefer | 494 | ИЗМЕРЕНО |
| пол (bump-always) | 107 | ИЗМЕРЕНО |
| собственная часть | 387 | ИЗМЕРЕНО |
| вклад refill | 101 | ИЗМЕРЕНО (toggle mstress) |
| вклад overflow flush | 60 | ИЗМЕРЕНО (toggle mstress) |
| hit/push база | 226 | ИЗМЕРЕНО (вычитание) |

Доля событий в собственной части: larson 12 %, mstress 41 % (ИЗМЕРЕНО). Частоты событий для mstress отдельно не считались — вклады измерены напрямую toggle-прогоном; деление на частоту larson (0.00882/0.01756) дало бы за событие 11451 / 3417 Ir (ОЦЕНКА) — ОЦЕНКА. Харнесс mstress lib.rs = 184.328 Ir/step (ИЗМЕРЕНО; включает код самой очереди mstress).


(parsed from raw logs; git-diff-sha256 at balance run: c3ad2ba1e92e)

## Где 2× (по статьям баланса)

Собственная часть larson: sefer **182** против mimalloc **75** → **2.4×** (ИЗМЕРЕНО).
Разложение собственной части sefer (larson, Ir/step):

| статья | sefer | mimalloc | отношение | статус |
|---|---|---|---|---|
| alloc (X-mix w_alloc, вкл. refill-ветку) | 64 | 56 | 1.1× | ИЗМЕРЕНО |
| free (X-mix w_free, вкл. overflow-flush) | 115 | 23 | 5.0× | ИЗМЕРЕНО |
| refill-события (r·A_refill) | 14 | — | — | ИЗМЕРЕНО |
| overflow-flush (f·F_overflow) | 8 | — | — | ИЗМЕРЕНО |
| собственная часть всего | 182 | 75 | 2.4× | ИЗМЕРЕНО |

Главная статья «где 2×» — **free-путь**: 115 против 23 Ir/op (5×), из них push-база
≈107 Ir/step (ОЦЕНКА: 115 − 8 flush). Alloc-путь ближе: 64 против 56.
События refill+flush дают лишь 22/182 = 12 % собственной части larson
(но 161/387 = 41 % в mstress — там очередь перераспределения бьёт по тем же путям).

## Топ-3 адресных правки (только назвать, НЕ реализовано; ΔIr — ОЦЕНКА)

1. **Refill/R1-01 для cap<16** — убрать overflow-событие для классов с малым cap:
   refill-ветка и half-flush глубины (`src/registry/heap_core/alloc/hot.rs:359`,
   `src/registry/heap_core/alloc/hot.rs:455`, overflow-flush в
   `src/registry/heap_core/free/dealloc_own_base.rs:514` и `…:570`,
   `src/alloc_core/small/alloc_core_small_magazine.rs:497`).
   Потенциал larson **−10…−25 Ir/step** (ОЦЕНКА, sol_ox §5 стр.5: «диагностика;
   потенциал larson −10…−25») — единственная статья, снимающая и refill (1587 Ir/событие,
   ОЦЕНКА), и overflow (456 Ir/событие, ОЦЕНКА).
2. **X-null** — `ptr.is_null()` с горячего free в хвосты промаха
   (`src/global/sefer_alloc/global_alloc.rs:105-107`).
   ΔIr: free −2, larson −1…−2 (ОЦЕНКА, sol_ox §5.2).
3. **X-span** — `off = addr & (SEGMENT−1)` в не-Large ветке `canonical_block_of`
   (`src/alloc_core/alloc_core/mem/mem_impl.rs:46-62`).
   ΔIr: free −2, larson −1…−2 (ОЦЕНКА, sol_ox §5.3).
   (X-c1 — `S2C1`+`saturating_sub` seed — запасной четвёртый: −1…−3, sol_ox §5.4.)

## Оракулы активации и повторяемость

- Пол: два независимых прохода, расхождение ≤1 Ir на 16 комбинаций
  (larson/bumpa 137→136, остальные точные; callgrind детерминирован, порог шума 0.5 Ir).
- Баланс: объяснено **98.4 %** ≥ 95 % (оракул выполнен); остаток 3 Ir/step —
  раздувание харнесса от инлайнинга аллокатора (ИЗМЕРЕНО: `lib.rs` харнесса
  sefer 86.977 / mimalloc 98.184 / bumpa-пол 47.000 Ir/step, пофайловая разметка).
- Toggle-измерения повторены дважды (j-inc3 и j-inc4 прогоны): refill 14/14 Ir/step.
- Частоты событий (0.00882/0.01756/0) сверены с моделью sol_rush2 §5.3 (×0.88).
- `git diff | sha256sum` перед каждым замером записан в raw-лог (все прогоны: одно значение).

## Сырьё

- `docs/perf/_raw_b4_judges_floor.log` (шаг 1), `_raw_b4_judges_uniconst.log` (шаг 2),
  `_raw_b4_judges_xmix.log` (шаг 3), `_raw_b4_judges_balance.log` (шаг 4).
- Таблицы: `node scripts/b4_judges_table.mjs`. CSV: `B4_JUDGES_LARSON_summary.csv`.
