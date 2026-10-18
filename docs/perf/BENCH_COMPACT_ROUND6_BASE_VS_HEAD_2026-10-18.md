# Ужатый прогон бенчей: до раунда 6 против `main` после R6-T16 (2026-10-18)

**Тип работы: `bench`** — только измерение. Код не менялся, состав `production` не затронут, заявлений об ускорении здесь нет. Это отчёт о прогоне, а не GO/NO-GO по фиче, поэтому имя файла намеренно не `R*_*.md` (такие файлы проверяет `scripts/verify-gate-report.mjs`).

## 0. Идентичность и условия

| поле | значение |
|---|---|
| `base` | `3dbfb3013aede489170f40ce3951a0e9a4787007` — `main` до правок раунда 6 (`src/` идентичен `132ab85b`); отдельный detached worktree, `git status --short` пуст |
| `head` | `dab9686f9975acefd364ee0366aaccc43e4f1e2d` — `main` после слияния #64 (R6-T16); отдельный detached worktree, `git status --short` пуст |
| хост | настольный ПК, не ВМ; Windows 10 Pro 10.0.19045; Intel Core i7-11800H (8 ядер / 16 потоков); ≈ 47.8 GiB RAM (51308191744 байт); план питания «Balanced» (runbook просит «High performance», не менялся); машина не простаивала — на ней работали другие нагрузки владельца |
| инструменты | `rustc 1.97.0 (2d8144b78 2026-07-07)`, `cargo 1.97.0`, `node v24.12.0`; паспорт хоста снят 2026-10-17, до восстановления системы из резервной копии, версии инструментов перепроверены 2026-10-18 |
| время | ужатый набор шёл 7 мин 32 с (11:04:39 → 11:12:11 UTC); каждый шаг завершился с `rc=0` |

Набор — ужатая версия `docs/perf/EXTERNAL_MACHINE_BENCH_RUNBOOK.md`. Отличия от runbook: A1 запущен как `cargo bench --bench global_alloc -- "^global_alloc"` (критерий-группы `global_alloc*`, те же что в `npm run bench:table`; диагностическая группа `pool_cap_sweep` пропущена — она в таблицу не входит, но занимает часы); A3 — 20 пар; A4 — один запуск на коммит. Порядок A1: первый запуск `base`, затем `head`; второй — `head`, затем `base` (дрейф хоста усредняется по порядку). Не выполнялись: A2, A5, B1, B2, C1, C2 и повтор A4.

**Точка входа и слой (правило CLAUDE.md про слой под замером):**

| набор | что запускалось | фичи | слой |
|---|---|---|---|
| A1 | `benches/global_alloc.rs`, один процесс, один поток | `production internals bench-internals` | три арма вызываются напрямую через `GlobalAlloc`, НЕ установлены как `#[global_allocator]` — меряется горячий путь аллокатора, не диспетчеризация глобального аллокатора |
| A3 | `scripts/paired-ab-runner.mjs`, примеры `paired_ab_*` | `alloc-global` | SeferAlloc установлен реальным `#[global_allocator]`; сравнение аллокаторов внутри одного коммита, НЕ `base` против `head` |
| A4 | `examples/malloc_macro.rs`, потоки не закреплены (unpinned) | `alloc-global alloc-xthread` (без `production`) | `SeferAlloc::new()` вызывается напрямую через `GlobalAlloc` из рабочих потоков |

Оракул активации пути (правило R30-8) не применялся: отчёт не делает заявлений о механизме, он сравнивает по времени немодифицированный код.

## 1. Выводы

**В1. Многопоточная пропускная способность SeferAlloc на этом хосте очень низкая — на обоих коммитах, то есть это не следствие раунда 6.** A4, таблицы «A4» ниже. На `base` larson падает с 26.84 M пар/с на 1 потоке до 2.92 M на 2 и 0.66 M на 4 потоках (на `head`: 27.20 → 2.29 → 0.67). mimalloc и System в том же процессе и тем же харнессом не проседают, поэтому харнесс и фоновая нагрузка хоста коллапс не объясняют. На 4 потоках разрыв с mimalloc — 136.9× (larson, `base`) и 511.6× (mstress, `base`); на `head` 130.3× и 807.5× (колонка «gap» в таблице масштабирования). Даже на 1 потоке mstress у SeferAlloc 3.04 M против 54.86 M у mimalloc (18.0×, `base`).
Это противоречит README: там MT-таблица (larson/mstress) помечена как исторические числа 0.2.0, «не перепрогонялись для 0.3.0», и показывает SeferAlloc быстрее mimalloc при T ≥ 2. На этом хосте (Windows, набор фич без `production`) картина обратная.
Причина не установлена. Гипотезы, ни одна не проверена: (а) Windows-специфичная цена пути межпоточного освобождения / выбора слота кучи / резервирования сегментов — по крайней мере MT-замеры Ph6b шли на WSL2 (perf item 81); (б) регрессия между 0.2.0 и `3dbfb301`; (в) зависимость от набора фич (`production` в этом прогоне не включён). Разделить гипотезы этим набором нельзя.

**В2. Раунд 6 на однопоточном A1: 13 из 17 сценариев — в пределах шума, 4 из 17 — сигнал «медленнее», и 3 из них составляют когерентное семейство.** «Burst» `global_alloc/*` и `manual_realloc_sim` не изменились (head/base 0.896–1.009, ни один не сигнал). Семейство `global_alloc_churn_write` медленнее на всех четырёх размерах: head/base 1.203 (16B), 1.108 (64B), 1.284 (256B), 1.236 (1024B). На 16B, 256B и 1024B это сигнал по правилу таблицы (оба парных отношения по одну сторону от 1 и дальше 5%, среднее выше дрейфа контрольных арм); на 64B второй запуск даёт всего 1.006, поэтому формально шум. Четвёртый сигнал — `global_alloc_churn_with_teardown/256B` (1.126) — одиночный: соседние размеры 0.977, 1.015, 1.017, поэтому считаю его вероятным шумом.
Это кандидат на регресс +11…+28% для сценария «аллокация + запись 16 байт + освобождение», но не доказанный: по два запуска на коммит; шум между запусками на одном коммите — медиана 3.3–8.2% по арме и коммиту, максимум 40.4% (System, `base`); механизм неизвестен. По направлению это совпадает с замечаниями ревью (alloc+free 16 B подорожала с 80 до 88 Ir; +8.75 Ir на блок после #55), но то были Ir, а здесь время — прямого подтверждения нет.

**В3. Память (A3) почти не изменилась.** RSS SeferAlloc 3698 → 3757 KiB, commit 4803 → 4851 KiB (`base` → `head`, средние по 80 сэмплам, последняя таблица); у контрольных арм (mimalloc, System) сдвиг порядка 10 KiB. Сдвиг у SeferAlloc чуть больше контрольного, но на одиночном запуске по 20 пар отделить его от шума нельзя.

**В4. Сборка `head` с `alloc-global alloc-xthread` печатает 7 предупреждений, сборка `base` — ни одного** (`_raw_bench_compact_round6_head_A4.log` против `_raw_bench_compact_round6_base_A4.log`). Два `unused import` (`mem::OwnedBlock`, `heap_registry::config_conflicts_total`) и пять `dead_code` (`canonical_root_for`, `release_initializing`, `SEGMENT_RECYCLE_UNVERIFIED_BASE_TOTAL`, `CONFIG_CONFLICTS`, функция `config_conflicts_total`). Причина: R6-T16 снял blanket `allow(dead_code)`, а `-D warnings` в per-PR CI проверяется только на шести строках clippy-матрицы (default, `experimental`, `--all-features`, `hardened medium-classes internals`, `production`, `production internals`); комбинации `alloc-global alloc-xthread` без остальных фич среди них нет. Для зависимости из registry cargo предупреждения не показывает, но сборки по пути и команда из README (`cargo run --example malloc_macro --features "alloc-global alloc-xthread"`) их выводят.

**В5. Положение однопоточного SeferAlloc относительно System и mimalloc (A1, `head`, таблица «arms against each other»).** SeferAlloc быстрее System в 17 из 17 сценариев (отношение 0.18–0.59). Против mimalloc он медленнее в burst на всех четырёх размерах (1.51–4.79) и на 16B во всех трёх churn-группах (1.44, 1.69, 1.61), быстрее в churn на 256B и 1024B (0.83 и 0.12) и в `churn_write` на 256B и 1024B (0.70 и 0.16), примерно на уровне на 64B (1.08 и 0.99). Значение mimalloc на 1024B churn (74.5 ns против 16.1 в burst) выглядит аномалией самого mimalloc, а не SeferAlloc.

## 2. Таблицы (генерируются `scripts/bench_compact_round6_summary.mjs --write` из `_raw_bench_compact_round6_*.log`, вручную не правятся)

<!-- BEGIN GENERATED (scripts/bench_compact_round6_summary.mjs --write) -->

### A1 — SeferAlloc, `base` -> `head` (ns per alloc+free pair, mean of 2 runs; manual_realloc_sim: ns per chain)

Control bar = max(5%, larger base->head drift of the mimalloc and System arms in the same scenario; both run identical code on both commits). A change is a signal only if SeferAlloc's mean ratio beats the bar and both run pairs (run1 vs run1, run2 vs run2) lie on the same side beyond 5%.

| scenario | base | head | head/base | pair1 | pair2 | control drift | verdict |
|---|---:|---:|---:|---:|---:|---:|---|
| global_alloc/16B | 23.5 | 23.8 | 1.009 | 1.084 | 0.937 | 2.7% | within noise |
| global_alloc/64B | 25.0 | 24.9 | 0.994 | 1.079 | 0.913 | 16.5% | within noise |
| global_alloc/256B | 25.5 | 23.9 | 0.939 | 0.950 | 0.928 | 26.4% | within noise |
| global_alloc/1024B | 25.7 | 24.2 | 0.942 | 0.930 | 0.955 | 7.8% | within noise |
| manual_realloc_sim | 431.9 | 387.0 | 0.896 | 0.910 | 0.882 | 10.8% | within noise |
| global_alloc_churn/16B | 8.3 | 8.9 | 1.077 | 1.109 | 1.047 | 6.6% | within noise |
| global_alloc_churn/64B | 9.8 | 9.2 | 0.938 | 1.165 | 0.764 | 15.5% | within noise |
| global_alloc_churn/256B | 8.8 | 10.0 | 1.134 | 0.938 | 1.368 | 2.2% | within noise |
| global_alloc_churn/1024B | 8.6 | 9.3 | 1.077 | 1.179 | 0.985 | 3.6% | within noise |
| global_alloc_churn_with_teardown/16B | 11.6 | 11.4 | 0.977 | 1.073 | 0.898 | 2.8% | within noise |
| global_alloc_churn_with_teardown/64B | 12.0 | 12.2 | 1.015 | 1.022 | 1.008 | 3.3% | within noise |
| global_alloc_churn_with_teardown/256B | 12.0 | 13.6 | 1.126 | 1.089 | 1.168 | 5.5% | SLOWER |
| global_alloc_churn_with_teardown/1024B | 13.5 | 13.7 | 1.017 | 0.932 | 1.109 | 12.6% | within noise |
| global_alloc_churn_write/16B | 9.2 | 11.1 | 1.203 | 1.294 | 1.120 | 7.0% | SLOWER |
| global_alloc_churn_write/64B | 9.2 | 10.2 | 1.108 | 1.223 | 1.006 | 13.9% | within noise |
| global_alloc_churn_write/256B | 8.9 | 11.4 | 1.284 | 1.295 | 1.273 | 24.8% | SLOWER |
| global_alloc_churn_write/1024B | 9.5 | 11.7 | 1.236 | 1.240 | 1.232 | 10.0% | SLOWER |

Signals: 4 of 17 scenarios (denominator: scenarios = group x size, plus manual_realloc_sim).

### A1 — noise: run1 vs run2 on the same commit (|run1-run2| / mean, over all 17 scenarios per arm)

| arm | commit | median | max |
|---|---|---:|---:|
| SeferAlloc | base | 4.3% | 26.3% |
| SeferAlloc | head | 4.3% | 19.5% |
| mimalloc | base | 4.5% | 21.5% |
| mimalloc | head | 8.2% | 34.9% |
| System | base | 4.9% | 40.4% |
| System | head | 3.3% | 27.0% |

### A1 — arms against each other on `head` (mean of 2 runs, ns per pair; ratio = SeferAlloc / other)

| scenario | SeferAlloc | mimalloc | System | Sefer/mimalloc | Sefer/System |
|---|---:|---:|---:|---:|---:|
| global_alloc/16B | 23.8 | 5.0 | 50.6 | 4.79 | 0.47 |
| global_alloc/64B | 24.9 | 7.0 | 56.0 | 3.56 | 0.44 |
| global_alloc/256B | 23.9 | 8.2 | 59.6 | 2.93 | 0.40 |
| global_alloc/1024B | 24.2 | 16.1 | 62.1 | 1.51 | 0.39 |
| manual_realloc_sim | 387.0 | 387.6 | 658.7 | 1.00 | 0.59 |
| global_alloc_churn/16B | 8.9 | 6.2 | 39.3 | 1.44 | 0.23 |
| global_alloc_churn/64B | 9.2 | 8.5 | 40.6 | 1.08 | 0.23 |
| global_alloc_churn/256B | 10.0 | 12.0 | 44.7 | 0.83 | 0.22 |
| global_alloc_churn/1024B | 9.3 | 74.5 | 52.1 | 0.12 | 0.18 |
| global_alloc_churn_with_teardown/16B | 11.4 | 6.7 | 43.9 | 1.69 | 0.26 |
| global_alloc_churn_with_teardown/64B | 12.2 | 7.4 | 45.8 | 1.65 | 0.27 |
| global_alloc_churn_with_teardown/256B | 13.6 | 9.3 | 46.3 | 1.46 | 0.29 |
| global_alloc_churn_with_teardown/1024B | 13.7 | 17.8 | 64.6 | 0.77 | 0.21 |
| global_alloc_churn_write/16B | 11.1 | 6.9 | 40.0 | 1.61 | 0.28 |
| global_alloc_churn_write/64B | 10.2 | 10.3 | 41.2 | 0.99 | 0.25 |
| global_alloc_churn_write/256B | 11.4 | 16.2 | 48.1 | 0.70 | 0.24 |
| global_alloc_churn_write/1024B | 11.7 | 72.7 | 58.7 | 0.16 | 0.20 |

### A4 — malloc_macro (single run per commit, Mops/s of alloc+free pairs, higher is better)

| workload | threads | arm | base | head | head/base |
|---|---:|---|---:|---:|---:|
| larson/unpinned | 1 | SeferAlloc | 26.84 | 27.20 | 1.01 |
| larson/unpinned | 1 | mimalloc | 62.41 | 55.98 | 0.90 |
| larson/unpinned | 1 | System | 15.85 | 16.24 | 1.02 |
| larson/unpinned | 2 | SeferAlloc | 2.92 | 2.29 | 0.78 |
| larson/unpinned | 2 | mimalloc | 54.04 | 62.87 | 1.16 |
| larson/unpinned | 2 | System | 14.27 | 15.86 | 1.11 |
| larson/unpinned | 4 | SeferAlloc | 0.66 | 0.67 | 1.02 |
| larson/unpinned | 4 | mimalloc | 90.35 | 87.29 | 0.97 |
| larson/unpinned | 4 | System | 28.14 | 28.69 | 1.02 |
| mstress/unpinned | 1 | SeferAlloc | 3.04 | 2.66 | 0.88 |
| mstress/unpinned | 1 | mimalloc | 54.86 | 70.44 | 1.28 |
| mstress/unpinned | 1 | System | 6.37 | 5.88 | 0.92 |
| mstress/unpinned | 2 | SeferAlloc | 0.14 | 0.10 | 0.71 |
| mstress/unpinned | 2 | mimalloc | 112.52 | 95.95 | 0.85 |
| mstress/unpinned | 2 | System | 12.59 | 11.97 | 0.95 |
| mstress/unpinned | 4 | SeferAlloc | 0.27 | 0.22 | 0.81 |
| mstress/unpinned | 4 | mimalloc | 138.13 | 177.64 | 1.29 |
| mstress/unpinned | 4 | System | 17.51 | 19.84 | 1.13 |

A4 control drift (mimalloc and System arms, identical code on both commits): median 11.1%, max 28.6% over 12 cells.

### A4 — SeferAlloc scaling and gap to mimalloc (per commit; scaling = SeferAlloc(T) / SeferAlloc(1 thread); gap = mimalloc / SeferAlloc at the same T)

| commit | workload | threads | SeferAlloc (M) | scaling vs 1 thread | mimalloc (M) | gap (mimalloc / SeferAlloc) |
|---|---|---:|---:|---:|---:|---:|
| base | larson/unpinned | 1 | 26.84 | 1.000 | 62.41 | 2.3 |
| base | larson/unpinned | 2 | 2.92 | 0.109 | 54.04 | 18.5 |
| base | larson/unpinned | 4 | 0.66 | 0.025 | 90.35 | 136.9 |
| base | mstress/unpinned | 1 | 3.04 | 1.000 | 54.86 | 18.0 |
| base | mstress/unpinned | 2 | 0.14 | 0.046 | 112.52 | 803.7 |
| base | mstress/unpinned | 4 | 0.27 | 0.089 | 138.13 | 511.6 |
| head | larson/unpinned | 1 | 27.20 | 1.000 | 55.98 | 2.1 |
| head | larson/unpinned | 2 | 2.29 | 0.084 | 62.87 | 27.5 |
| head | larson/unpinned | 4 | 0.67 | 0.025 | 87.29 | 130.3 |
| head | mstress/unpinned | 1 | 2.66 | 1.000 | 70.44 | 26.5 |
| head | mstress/unpinned | 2 | 0.10 | 0.038 | 95.95 | 959.5 |
| head | mstress/unpinned | 4 | 0.22 | 0.083 | 177.64 | 807.5 |

### A3 — paired A/B/B/A, 20 pairs, within each commit (sefer vs mimalloc / system; NOT base vs head)

| commit | comparison | statistic | sign test |
|---|---|---|---|
| base | sefer vs mimalloc | n=20 mean Δ=-212.405 us sd=234.503 us se=52.437 us t=-4.051 df=19 crit(p<0.05)=2.101 => REAL | sign test: sefer-faster=20/20  mimalloc-faster=0/20  ties=0 |
| base | sefer vs system | n=20 mean Δ=-1.528 ms sd=192.231 us se=42.984 us t=-35.557 df=19 crit(p<0.05)=2.101 => REAL | sign test: sefer-faster=20/20  system-faster=0/20  ties=0 |
| head | sefer vs mimalloc | n=20 mean Δ=-143.690 us sd=106.831 us se=23.888 us t=-6.015 df=19 crit(p<0.05)=2.101 => REAL | sign test: sefer-faster=19/20  mimalloc-faster=1/20  ties=0 |
| head | sefer vs system | n=20 mean Δ=-1.533 ms sd=202.346 us se=45.246 us t=-33.888 df=19 crit(p<0.05)=2.101 => REAL | sign test: sefer-faster=20/20  system-faster=0/20  ties=0 |

| commit | arm | samples | mean elapsed (us) | mean RSS (KiB) | mean commit (KiB) |
|---|---|---:|---:|---:|---:|
| base | sefer | 80 | 1007.5 | 3698 | 4803 |
| base | mimalloc | 40 | 1222.3 | 4709 | 4005 |
| base | system | 40 | 2533.5 | 3430 | 1077 |
| head | sefer | 80 | 1066.2 | 3757 | 4851 |
| head | mimalloc | 40 | 1190.3 | 4719 | 4007 |
| head | system | 40 | 2619.2 | 3440 | 1078 |

<!-- END GENERATED -->

## 3. Чего этот прогон не показывает

- Одна машина, не простаивавшая, план питания «Balanced». Числа применимы к этому хосту; на Linux/WSL2 картина может быть иной.
- A1 — по два запуска на коммит; эффекты порядка 10% и меньше этим набором не разрешаются (шум между запусками — в таблице «noise»).
- A4 — один запуск на коммит. Дрейф можно оценить только по контрольным армам, тождественным на обоих коммитах (медиана и максимум — строка «A4 control drift»). Поэтому различия `head/base` для SeferAlloc в A4 (0.71–1.02) не интерпретируются; интерпретируется только порядок величины коллапса (десятки и сотни раз).
- A3 не сравнивает `base` и `head` по времени: он сравнивает аллокаторы внутри коммита. Процесс длится около миллисекунды, и в нём заметна доля запуска.
- Инструкции (Ir) не измерялись: valgrind локально недоступен, остаётся CI. Причины замеченных эффектов не установлены.
- Не выполнялись A2 (причинный харнесс), A5, B1, B2, C1, C2: базовой точки «до раунда 6» по ним нет.
- Сырые логи очищены от путей хоста (замена на `<repo>` и `<cargo-home>`); sha256 исходных и закоммиченных файлов — в `*_manifest.csv`.

## 4. Следующие шаги (всё на GH CI, без локальных сборок)

1. **MT-коллапс (В1), приоритет выше остальных.** Разрыв в десятки и сотни раз заметно выше шума общих виртуальных машин, поэтому `malloc_macro` можно гонять на CI-раннерах: Windows и Linux, наборы фич `alloc-global alloc-xthread` и `production`, закреплённые и незакреплённые потоки. Это отделит платформу, набор фич и регрессию; затем бисекция по SHA на CI. Нужна исследовательская задача: если коллапс специфичен для Windows, найти его причину и лучшее исправление, а не гадать.
2. **`churn_write` (В2).** A/B на CI по Ir для `churn_write_256b` (iai-бенч уже есть), `base` против `head`, с чтением счётчика активации пути, затем бисекция по слитым PR раунда 6. Это закрывает и подозрение на +8.75 Ir на блок.
3. **Предупреждения (В4).** Поправить 7 предупреждений (удалить мёртвый код или сузить `cfg`) и добавить в CI строку `-D warnings` для `alloc-global alloc-xthread`. Естественное место — задача R6-T43 (мёртвый код) в U26.
4. После шага 1 обновить MT-раздел README: сейчас он содержит только исторические числа 0.2.0.

Карточки в индексы open-items (`docs/perf/OPEN_ITEMS.md` для В1 и В2, `docs/CORRECTNESS_OPEN_ITEMS.md` для В4) этим коммитом **не заведены**: владелец просил закоммитить только файлы выводов, а индексы жёсткого формата и проверяются скриптами. Их нужно завести в следующем коммите; до этого источник — этот отчёт.

## 5. Воспроизведение

```text
git worktree add --detach worktrees/bench-base 3dbfb3013aede489170f40ce3951a0e9a4787007
git worktree add --detach worktrees/bench-head dab9686f9975acefd364ee0366aaccc43e4f1e2d
# в каждом worktree (F = production internals bench-internals):
cargo bench --features "$F" --bench global_alloc -- "^global_alloc"     # A1, два запуска, порядок base,head и head,base
node scripts/paired-ab-runner.mjs --pairs 20                              # A3
cargo run --release --example malloc_macro --features "alloc-global alloc-xthread"   # A4
# таблицы и CSV из сырых логов:
node scripts/bench_compact_round6_summary.mjs --ingest <каталог с base/ и head/>
node scripts/bench_compact_round6_summary.mjs --write
```

Артефакты: `docs/perf/_raw_bench_compact_round6_{base,head}_{A1_run1,A1_run2,A3,A4,sha}.*`, `BENCH_COMPACT_ROUND6_BASE_VS_HEAD_2026-10-18_{summary,A4_summary,manifest}.csv`.
