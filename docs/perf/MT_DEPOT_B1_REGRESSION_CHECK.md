# Проверка регресса MT после склада и B1 (bench, без изменения кода)

Статус: **регресса нет.** Ни одна ячейка не медленнее. mstress быстрее
на всех T, larson T=1 быстрее. Остальные ячейки larson — в пределах
разброса null.

## Зачем

Склад магазинов (PR #100) и B1 (PR #102) мерились однопоточным судьёй
J1′. Многопоточный прогон после них не делался: workflow `mt-bench`
запускается только по метке.

## Что меряется

`examples/ph6b_mt_ab.rs` — реальный `#[global_allocator]` (`SeferAlloc`,
`--features "production internals"`, release, RUSTFLAGS выравнивания
`-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5`),
Windows 10, i7-11800H (8 ядер). Две нагрузки, larson и mstress,
T = 1, 2, 4, 8. Каждый 16-й free уходит в другой поток.

- A = main до склада, `cb6371e5`.
- B = main после склада и B1, `aa523fd1`.
- Оба собраны из чистых деревьев; sha256 бинарей — в
  `mt_depot_b1/build_identity.txt`.
- Шум общий для пары: четвёрки запусков в порядке ABBA / BAAB (порядок
  чередуется от четвёрки к четвёрке), 100 четвёрок.
- Калибровка: 50 четвёрок A/A — тот же бинарь A в обоих плечах.
- Статистика: по каждой ячейке медиана по четвёркам величины
  ln(Σt_B / Σt_A), где t — нс на операцию одного запуска. 95 % CI —
  bootstrap по четвёркам.
- Оракул: в каждом запуске харнесс печатает `config_conflicts=0` и
  `segments_reserved_total > 0`; раннер проверяет оба. Набор значений
  `segments_reserved_total` у A и B одинаковый: {9, 10, 11, 12}.

## Результат (вывод `scripts/mt_regression_quads_table.mjs` дословно)

```text
# MT regression: B = main after depot+B1 vs A = main before depot; 100 AB quads, 50 A/A null quads; statistic = median over quads of ln(sum t_B / sum t_A), t = ns/op; 95 % bootstrap over quads
| Ячейка | A/A null % [CI] | B/A % [CI] | B против A |
|---|---|---|---|
| larson_T1 | 1.09 [-0.31, 1.71] | -5.30 [-5.81, -4.92] | быстрее в 1.054× |
| mstress_T1 | -0.13 [-1.21, 0.00] | -22.87 [-23.16, -22.49] | быстрее в 1.257× |
| larson_T2 | -1.59 [-2.79, -0.29] | -2.24 [-3.75, -1.38] | быстрее в 1.023× |
| mstress_T2 | 0.02 [-0.85, 1.21] | -17.90 [-19.46, -16.75] | быстрее в 1.196× |
| larson_T4 | 1.44 [-0.07, 2.70] | -3.24 [-3.76, -1.56] | быстрее в 1.033× |
| mstress_T4 | -0.70 [-1.14, 0.53] | -17.93 [-18.89, -16.96] | быстрее в 1.196× |
| larson_T8 | -6.11 [-16.02, 0.81] | -7.62 [-10.14, -4.55] | быстрее в 1.079× |
| mstress_T8 | -0.64 [-6.16, 7.05] | -17.58 [-18.86, -14.69] | быстрее в 1.192× |
```

Чтение:

- **mstress быстрее на всех T:** в 1.257× (T=1), 1.196× (T=2), 1.196×
  (T=4), 1.192× (T=8). Эффект намного больше разброса null (не больше
  ±1.2 %, на T=8 CI null от −6.2 % до +7.1 %).
- **larson T=1 быстрее в 1.054×**, null +1.09 %.
- **larson T=2, T=4, T=8:** формально «быстрее», но эффект (−2.2 %, −3.2 %,
  −7.6 %) сопоставим с разбросом null (−1.6 %, +1.4 %, −6.1 % с CI до
  −16 %). Вывод только «не медленнее».
- RSS и commit не мерились. Сегментов зарезервировано столько же, сколько
  у A: {9, 10, 11, 12}.

## Сырьё и команды

- Сырьё: `mt_depot_b1/_raw_mt_quads_r1.jsonl.gz` — 600 запусков,
  255 КиБ до сжатия, по правилу tier 2 хранится в gzip.
- Сводка: `MT_DEPOT_B1_REGRESSION_CHECK_summary.csv`.

```text
RUSTFLAGS="<выравнивание>" cargo build --release --example ph6b_mt_ab --features "production internals"   # в worktree на cb6371e5 и на aa523fd1
node scripts/mt_regression_quads_run.mjs <A.exe> <B.exe> 100 50 <quads.jsonl>
node scripts/mt_regression_quads_table.mjs <quads.jsonl[.gz]> docs/perf/MT_DEPOT_B1_REGRESSION_CHECK_summary.csv
```
