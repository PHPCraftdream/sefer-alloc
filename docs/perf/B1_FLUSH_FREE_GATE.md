# B1 — flush одним проходом + холодный хвост free при полном магазине: гейт (perf(runtime))

Статус: **GO** по предрегистрации (`docs/perf/b1_flush_free/B1_PREREG.md`,
зафиксирована до прогона). Шипится комбинированный вариант `b1ac`.
Код под замером: абляции — ветка `b4-bulk`@`cd90113c` (base = main
`1667be06`, склад уже в main); против mimalloc — эта ветка
`b1-flush-free`@`07fe2907` (чистое дерево).

## Что изменено

- **B1-a** (`src/alloc_core/small/alloc_core_small_magazine.rs`):
  `flush_class_internal` больше не делает предварительный проход для
  поиска конца run. Конец run (смена сегмента или null) находится внутри
  цикла `flush_run`, и кредиты снимаются один раз: `длина run − rejected`.
  Счётчик на каждый принятый блок и `Option` исчезли. Проверки на каждом
  блоке (span, байт состояния — класс и состояние, `write_next`, FREE)
  идут в прежнем порядке и прежней силы.
- **B1-c** (`src/registry/heap_core/free/dealloc_own_base.rs`): ветка
  полного магазина (парковка на склад, swap-top R1-01, half-flush при
  переполнении) вынесена в `#[cold] #[inline(never)] free_magazine_full`.
  Быстрый путь push не меняется и больше не сохраняет callee-saved
  регистры. Порядок и проверки те же.
- Production-дифф совпадает с измеренным патчем `b1ac`
  (`b1_flush_free/patches/b1ac.patch`), кроме одной строки: убран хвостовой
  `return;` в конце `free_magazine_full` (clippy `needless_return`,
  поведение то же).

## Что меряется

Реальный `#[global_allocator]` (`SeferAlloc`, `--features production`,
release, RUSTFLAGS выравнивания `-C llvm-args=-align-all-functions=6
-C llvm-args=-align-all-nofallthru-blocks=5`), Windows 10, i7-11800H,
rustc 1.97.0.

1. **Против текущего sefer** — абляционный протокол J1′ v2
   (`scripts/b4_ablate_run.mjs`, ветка `b4-bulk`). В одном процессе две
   копии sefer: A — main, B — main + патч. Порции A и B чередуются
   ABBA/BAAB по 2000 блоков. Внутри процесса берётся медиана
   `ln(t_B/t_A)`, затем медиана по 20 процессам с разными order seeds;
   CI 95 % — bootstrap по процессам. Тот же прогон делается для null-копии
   (пустой патч): эффект = медиана варианта − медиана null.
2. **Против mimalloc** — main-судья J1′ v2
   (`scripts/b4_interleave_run.mjs --reps 20 --blocks 2000`), A = sefer
   `07fe2907`, B = mimalloc. Сравнение с прогоном склада
   (`b1_depot_gate/vs_mimalloc/rep_summary_depot.csv`, `d0bec415`).
3. **Инструкции** — callgrind в WSL (`examples/b4_pmu_probe.rs bulk sefer
   64 16384 40`, ветка `b4-bulk`, rustc 1.98.1). Это инструкции, а не
   время; вердикт выносит судья по времени.

Оракул активации: судья проверяет точное число вызовов шима каждого плеча
и нули чужих вызовов (`oracle_mismatches=0`, `foreign_window_calls=0` во
всех 60 процессах прогона против mimalloc). То, что оба изменённых пути
работают, видно в callgrind: `free_magazine_full` есть только в b1c/b1ac
(7.0 Ir на пару), `flush_class_internal` вызывается на каждом
переполнении (bulk: 16 384 free за проход, flush каждые 8). Учёт кредитов
закреплён тестом `tests/b1_flush_run_accounting.rs`: run с уже свободным
блоком и null снимает ровно принятые блоки. Мутант «снять всю длину run»
краснеет.

## Результат (вывод `scripts/b1_flush_free_gate_report.mjs` дословно)

```text
# B1 gate — effect = median_processes ln(t_variant/t_sefer) net of null (20 processes x 2000 blocks each); «равно» = CI contains 0

| Ядро | b1a_flush: эффект % [CI] | b1c_free_cold: эффект % [CI] | b1ac: эффект % [CI] | b1ac | null точен |
|---|---|---|---|---|---|
| pair16 | 0.17 [0.11, 0.22] | -3.87 [-3.93, -3.81] | -3.95 [-4.01, -3.83] | быстрее в 1.040× | yes |
| pair64 | 0.13 [0.10, 0.16] | -3.78 [-3.84, -3.76] | -3.87 [-3.93, -3.81] | быстрее в 1.039× | yes |
| pair_rt | -0.17 [-0.35, 0.08] | -4.94 [-5.16, -4.76] | -4.27 [-4.46, -3.97] | быстрее в 1.044× | yes |
| bulk64 | 0.45 [0.24, 0.63] | 0.84 [0.60, 1.03] | 0.17 [-0.04, 0.35] | равно | yes |
| bulk16k | -5.89 [-6.52, -5.14] | -2.19 [-3.12, -1.15] | -8.69 [-9.39, -7.82] | быстрее в 1.091× | yes |
| churn16 | -4.29 [-5.52, -2.53] | 0.56 [-2.01, 3.52] | -4.73 [-5.92, -3.11] | быстрее в 1.048× | no |
| churn64 | -1.27 [-2.26, -0.34] | -0.86 [-2.43, 1.05] | -3.68 [-4.77, -2.56] | быстрее в 1.037× | yes |
| churn256 | -0.55 [-1.80, 0.10] | -1.16 [-2.31, 0.54] | -1.70 [-2.67, -1.12] | быстрее в 1.017× | yes |
| churn1024 | -2.54 [-3.61, -1.58] | 1.25 [0.16, 2.27] | -2.58 [-3.42, -1.74] | быстрее в 1.026× | yes |
| vec512 | -0.03 [-0.82, 0.84] | -0.99 [-1.77, -0.38] | -0.74 [-1.52, 0.15] | равно | yes |
| larson | -1.06 [-2.27, 0.38] | -1.72 [-2.61, -0.41] | -1.73 [-2.53, -0.65] | быстрее в 1.017× | yes |

# b1a_flush: GO
# b1c_free_cold: GO
# b1ac: GO
# b1ac: faster on 9/11 (pair16,pair64,pair_rt,bulk16k,churn16,churn64,churn256,churn1024,larson); slower on 0/11

# J1′ v2, sefer vs mimalloc: t_mimalloc / t_sefer (> 1: sefer faster); before = main with the depot, after = + b1ac

| Ядро | до B1 t_mi/t_sefer | допущено | после B1 t_mi/t_sefer | допущено | sefer после против mimalloc | sefer после против до (ОЦЕНКА, разные прогоны) |
|---|---:|---|---:|---|---|---|
| pair16 | 0.7944 | yes | 0.8252 | yes | mimalloc быстрее в 1.212× | быстрее в 1.039× |
| pair64 | 0.9237 | yes | 0.9778 | yes | mimalloc быстрее в 1.023× | быстрее в 1.059× |
| pair_rt | 1.3996 | yes | 1.4463 | yes | sefer быстрее в 1.446× | быстрее в 1.033× |
| bulk64 | 0.7778 | yes | 0.7934 | yes | mimalloc быстрее в 1.260× | быстрее в 1.020× |
| bulk16k | 0.4163 | yes | 0.4573 | yes | mimalloc быстрее в 2.187× | быстрее в 1.098× |
| churn16 | 0.5704 | yes | 0.5816 | yes | mimalloc быстрее в 1.720× | быстрее в 1.020× |
| churn64 | 0.7483 | yes | 0.7588 | yes | mimalloc быстрее в 1.318× | быстрее в 1.014× |
| churn256 | 0.6117 | yes | 0.6476 | yes | mimalloc быстрее в 1.544× | быстрее в 1.059× |
| churn1024 | 0.6551 | yes | 0.6852 | yes | mimalloc быстрее в 1.460× | быстрее в 1.046× |
| vec512 | 1.3083 | no | 1.9608 | no | не допущено A/A (точка: sefer быстрее в 1.961×) | быстрее в 1.499× |
| larson | 0.8901 | yes | 1.0205 | yes | равно (CI содержит 0; точка: sefer быстрее в 1.020×) | быстрее в 1.147× |

# callgrind bulk 64 B x 16384: Ir per alloc+free pair (720896 pairs), self cost per function; instructions, not time

| Функция | base | b1a_flush | b1c_free_cold | b1ac |
|---|---:|---:|---:|---:|
| free | 81.0 | 81.0 | 68.0 | 68.0 |
| free_magazine_full | 0.0 | 0.0 | 7.0 | 7.0 |
| flush_class_internal | 58.1 | 47.2 | 55.4 | 48.3 |
| try_drain_freelist_batch | 63.1 | 63.1 | 63.1 | 63.1 |
| alloc | 42.3 | 42.3 | 42.3 | 42.3 |
| total | 303.7 | 292.8 | 294.9 | 287.9 |
# b1ac vs base: total −15.8 Ir/pair (free incl. free_magazine_full −6.0, flush −9.8)

VERDICT=GO
# wrote B1_FLUSH_FREE_GATE_summary.csv
# wrote B1_FLUSH_FREE_GATE_summary_callgrind.csv
```

Чтение:

- **Пороги 1–3 выполнены для всех трёх вариантов.** bulk16k: b1ac быстрее
  в 1.091× (верх CI −7.82 % < 0). pair16, pair64 и larson не медленнее,
  а быстрее (в 1.040×, 1.039× и 1.017×). Ни одно ядро не медленнее
  (0 из 11). bulk64 и vec512 — «равно».
- **B1-c даёт pair-ядрам почти весь выигрыш** (−3.8…−4.9 %): быстрый путь
  free больше не сохраняет регистры. Сам B1-a на pair16/pair64 медленнее
  на +0.17 / +0.13 % (CI выше 0, но в пределах порога 2 ≤ +1 %), а в
  сумме b1ac быстрее.
- **«null точен = no» только на churn16**: полуширина CI null-копии больше
  1 %, это шум машины. CI эффекта включает эту неопределённость (bootstrap
  обоих наборов процессов); верхний край эффекта −3.11 %.
- **Против mimalloc** sefer после B1 быстрее на pair_rt (в 1.446×).
  На larson — «равно» (CI содержит 0; до B1 mimalloc был быстрее в
  1.123×). vec512 не допущено по A/A. На остальных 8 ядрах mimalloc
  по-прежнему быстрее: bulk16k в 2.187×, churn16 в 1.720×, pair16 в
  1.212×, pair64 в 1.023×.
- **Столбец «sefer после против до» — ОЦЕНКА** по двум прогонам в разное
  время. Каждый прогон — пары в одном шуме, но состояние машины между
  прогонами разное. На larson у обоих плеч тиков на вызов стало в ~1.5×
  больше (sefer 16.96 → 23.70, mimalloc 14.49 → 24.67). Поэтому 1.147× на
  larson и 1.499× на vec512 — не эффект B1. Эффект B1 — таблица
  абляций (larson 1.017×, vec512 «равно»).
- **Callgrind:** b1ac снимает 15.8 из 303.7 Ir на пару alloc+free. Из них
  free −6.0 (81.0 → 68.0 плюс 7.0 в холодном хвосте), flush −9.8. Drain
  (63.1) и быстрый путь alloc (42.3) не менялись. Это следующие цели.

## Идентичность и сырьё

| Прогон | Код | Идентичность | Сырьё |
|---|---|---|---|
| абляции b1a / b1c / b1ac / null | `b4-bulk`@cd90113c | `b1_flush_free/run_identity.txt` (sha256 каждой копии варианта), патчи в `b1_flush_free/patches/` | 240 JSONL, `b1_flush_free/MANIFEST.txt` |
| против mimalloc | `b1-flush-free`@07fe2907, чистое дерево | head + `sha(diff)=e3b0c442…` (пусто) + exe `d525a2333c9b…` | 60 JSONL, `b1_flush_free/vs_mimalloc/MANIFEST.txt` |
| callgrind | `b4-bulk`@cd90113c ± патч | src и exe sha256 по каждому варианту в сырье | `b1_flush_free/_raw_callgrind_bulk.txt` |

Сырые JSONL (по ~4–8 МиБ, > 2 МиБ) в git не входят. Манифесты содержат
sha256 и размер каждого файла. Сводки: `B1_FLUSH_FREE_GATE_summary.csv`
(ядра) и `B1_FLUSH_FREE_GATE_summary_callgrind.csv` (Ir по функциям). Их
пишет тот же генератор; он же проверяет арифметику (`ratio = exp(effect)`,
`effect = variant − null`, число процессов, согласие вердикта судьи с
отношением, число пар в callgrind).

`b4_interleave_run.mjs` пишет сводку в
`docs/perf/B4_INTERLEAVE_JUDGE_rep_summary.csv`. Эта сводка скопирована в
`b1_flush_free/vs_mimalloc/rep_summary_b1ac.csv`, а закоммиченный файл
восстановлен без изменений.

Команды:

```text
# против sefer (ветка b4-bulk)
RUSTFLAGS="<выравнивание>" node scripts/b4_ablate_run.mjs --out-dir <dir> --blocks 2000 --reps 20 \
  --kernels pair16,pair64,pair_rt,bulk64,bulk16k,churn16,churn64,churn256,churn1024,vec512,larson \
  --variants b1a_flush,b1c_free_cold,b1ac --no-null-gate
node scripts/b4_ablate_vs_null.mjs --dir <dir> --variant <v> --reps 20 --kernels <те же> --csv <csv>
# против mimalloc (эта ветка, коммит 07fe2907)
RUSTFLAGS="<выравнивание>" node scripts/b4_interleave_run.mjs --reps 20 --blocks 2000 --out-prefix <dir>/j
# callgrind (WSL, ветка b4-bulk ± патч)
RUSTFLAGS="-C force-frame-pointers=yes -g <выравнивание>" cargo build --release --example b4_pmu_probe --features production
valgrind --tool=callgrind <probe> bulk sefer 64 16384 40; callgrind_annotate --threshold=99 <out>
# отчёт
node scripts/b1_flush_free_gate_report.mjs --csv docs/perf/B1_FLUSH_FREE_GATE_summary.csv
```

## Не измерено

- RSS и commit: B1 не меняет ни структур, ни объёма хранения; не мерилось.
- MT-ядра (удалённые free): изменённые пути — только свой поток; MT-прогон
  в этом гейте не делался.
