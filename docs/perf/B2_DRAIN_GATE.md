# B2 — drain свободного списка одним проходом: гейт (perf(runtime))

Статус: **GO** по предрегистрации (`docs/perf/b2_drain/B2_PREREG.md`,
зафиксирована до прогона, коммит `8dd5b2b4`).
Код под замером:
- абляции — ветка `b2-drain`@`8dd5b2b4` (base = main `aa523fd1`, склад и B1
  уже в main);
- против mimalloc — эта ветка `b2-drain-prod`@`dbf4f3b0` (чистое дерево).

## Что изменено

`AllocCore::try_drain_freelist_batch`
(`src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs`) раньше
проходил цепочку дважды. Первый проход проверял звенья и готовил листья
(`prepare_for` может отказать по OOM). Второй выдавал блоки и заново читал
`next`. Теперь проход один. Для каждого звена по порядку:

1. Та же проверка, что была в первом проходе. Без `hardened` —
   `continuation_offset_or_fatal`. С `hardened` — `valid_free_node`,
   `free_continuation`, проверка возврата к голове и проверка циклов
   Флойдом. Все проверки аварийно завершают процесс.
2. `prepare_for`.
3. Запись байта состояния и `commit`.

Изменение контракта только на пути OOM при выделении листа:

- отказ `prepare_for` на ПЕРВОМ блоке — `Err`, ничего не изменено (как
  раньше);
- отказ на более позднем блоке — партия заканчивается на нём; уже выданные
  блоки возвращаются как короткий drain, отказавший блок остаётся головой.
  Раньше в этом случае был `Err` и не выдавалось ничего.

Все вызывающие уже обрабатывают короткий drain как «здесь блоков меньше».

С `hardened` медленный указатель Флойда идёт позади головы по звеньям,
которые голова уже проверила и выдала: их байт состояния больше не FREE.
Поэтому он берёт смещение без повторной проверки (`continuation_offset`).
Кроме того, проверка FREE в `valid_free_node` ловит любой повторный заход
в уже выданный блок.

Это отличие от измеренного патча (`b2_drain/patches/b2r1_drain_one_pass.patch`).
Оно касается только `#[cfg(feature = "hardened")]`. Измеренная сборка
`production` без `hardened` совпадает с production-кодом.

Тесты:
- `tests/r11_p3_small_sidecar_preflight.rs` закрепляет новый контракт:
  отказ на блоке 257 → выдано 256, остальное не тронуто, повторный drain
  выдаёт ровно остаток. На старом коде тест краснеет: `left: 0`,
  `right: 256`.
- Отказ на первом блоке закреплён прежним тестом
  `tests/r11_ph3a_issue_transaction_batch.rs`.
- Циклы под `hardened` — тестами гибели процесса
  `tests/regression_freelist_next_validation.rs` (`drain_cycle`,
  `drain_head_loop`, `drain_self_cycle`).

## Что меряется

Реальный `#[global_allocator]` (`SeferAlloc`, `--features production`,
release, RUSTFLAGS выравнивания `-C llvm-args=-align-all-functions=6
-C llvm-args=-align-all-nofallthru-blocks=5`), Windows 10, i7-11800H,
rustc 1.97.0.

1. **Против текущего sefer** — абляция J1′ v2 (`scripts/b4_ablate_run.mjs`).
   В одном процессе две копии sefer, порции ABBA/BAAB по 2000 блоков, 20
   процессов. Эффект = медиана варианта − медиана null-копии, CI 95 % —
   bootstrap по процессам.
2. **Против mimalloc** — main-судья J1′ v2 (`scripts/b4_interleave_run.mjs
   --reps 20 --blocks 2000`). Сравнение с прогоном B1
   (`b1_flush_free/vs_mimalloc/rep_summary_b1ac.csv`).
3. **Инструкции** — callgrind в WSL (`examples/b4_pmu_probe.rs bulk sefer
   64 16384 40`, rustc 1.98.1). Это инструкции, а не время.

Оракул активации: судья проверяет точное число вызовов шима каждого плеча
и нули чужих вызовов (`oracle_mismatches=0` во всех 60 процессах прогона
против mimalloc). В callgrind drain выполняется в обоих вариантах, и вся
разница Ir приходится на него: генератор проверяет, что изменение вне
drain меньше 0.05 Ir на пару.

## Результат (вывод `scripts/b2_drain_gate_report.mjs` дословно)

```text
# B2 gate — effect = median_processes ln(t_variant/t_sefer) net of null (20 processes x 2000 blocks each); «равно» = CI contains 0

| Ядро | b2r1_drain_one_pass: эффект % [CI] | итог | null точен |
|---|---|---|---|
| pair16 | -0.01 [-0.07, 0.04] | равно | yes |
| pair64 | 0.04 [-0.03, 0.08] | равно | yes |
| pair_rt | -0.27 [-0.41, -0.15] | быстрее в 1.003× | yes |
| bulk64 | -0.13 [-0.34, 0.15] | равно | yes |
| bulk16k | -3.50 [-4.04, -2.87] | быстрее в 1.036× | yes |
| churn16 | -1.55 [-2.73, -0.71] | быстрее в 1.016× | yes |
| churn64 | -1.79 [-2.49, -1.29] | быстрее в 1.018× | yes |
| churn256 | -6.05 [-7.13, -5.36] | быстрее в 1.062× | yes |
| churn1024 | -12.45 [-13.83, -11.64] | быстрее в 1.133× | no |
| vec512 | -0.04 [-0.30, 0.30] | равно | yes |
| larson | -1.41 [-2.43, -0.61] | быстрее в 1.014× | yes |

# b2r1_drain_one_pass: GO
# b2r1_drain_one_pass: faster on 7/11 (pair_rt,bulk16k,churn16,churn64,churn256,churn1024,larson); slower on 0/11

# J1′ v2, sefer vs mimalloc: t_mimalloc / t_sefer (> 1: sefer faster); before = main with B1, after = + B2

| Ядро | до B2 t_mi/t_sefer | допущено | после B2 t_mi/t_sefer | допущено | sefer после против mimalloc | sefer после против до (ОЦЕНКА, разные прогоны) |
|---|---:|---|---:|---|---|---|
| pair16 | 0.8252 | yes | 0.8262 | yes | mimalloc быстрее в 1.210× | быстрее в 1.001× |
| pair64 | 0.9778 | yes | 0.9628 | yes | mimalloc быстрее в 1.039× | медленнее в 1.016× |
| pair_rt | 1.4463 | yes | 1.4373 | yes | sefer быстрее в 1.437× | медленнее в 1.006× |
| bulk64 | 0.7934 | yes | 0.8003 | yes | mimalloc быстрее в 1.250× | быстрее в 1.009× |
| bulk16k | 0.4573 | yes | 0.4752 | yes | mimalloc быстрее в 2.104× | быстрее в 1.039× |
| churn16 | 0.5816 | yes | 0.6009 | yes | mimalloc быстрее в 1.664× | быстрее в 1.033× |
| churn64 | 0.7588 | yes | 0.7871 | yes | mimalloc быстрее в 1.270× | быстрее в 1.037× |
| churn256 | 0.6476 | yes | 0.6432 | yes | mimalloc быстрее в 1.555× | медленнее в 1.007× |
| churn1024 | 0.6852 | yes | 0.7287 | yes | mimalloc быстрее в 1.372× | быстрее в 1.064× |
| vec512 | 1.9608 | no | 1.1816 | yes | sefer быстрее в 1.182× | медленнее в 1.659× |
| larson | 1.0205 | yes | 0.8919 | yes | mimalloc быстрее в 1.121× | медленнее в 1.144× |

# callgrind bulk 64 B x 16384: Ir per alloc+free pair (720896 pairs), self cost per function; instructions, not time

| Функция | base | b2r1_drain_one_pass |
|---|---:|---:|
| free | 68.0 | 68.0 |
| free_magazine_full | 7.0 | 7.0 |
| flush_class_internal | 48.3 | 48.3 |
| try_drain_freelist_batch | 63.1 | 53.9 |
| alloc | 42.3 | 42.3 |
| total | 287.9 | 278.7 |
# b2r1_drain_one_pass vs base: total −9.2 Ir/pair, of it drain −9.2

VERDICT=GO
# wrote B2_DRAIN_GATE_summary.csv
# wrote B2_DRAIN_GATE_summary_callgrind.csv
```

Чтение:

- **Пороги 1–3 выполнены.**
  - bulk16k быстрее в 1.036× (верх CI −2.87 % < 0).
  - pair16, pair64 — «равно»; larson быстрее в 1.014×.
  - Ни одно ядро не медленнее (0 из 11).
- **Сильнее всего выигрывают длинные churn:** churn1024 быстрее в 1.133×
  (null неточен, CI эффекта −13.83…−11.64 %), churn256 — в 1.062×. Там
  refill тянет длинные цепочки, и второй проход стоил больше всего.
- **Против mimalloc** sefer быстрее на pair_rt (в 1.437×) и на vec512 (в
  1.182×, в этом прогоне допущено). На остальных 9 ядрах mimalloc
  быстрее: bulk16k в 2.104×, churn16 в 1.664×, churn256 в 1.555×.
- **Столбец «sefer после против до» — ОЦЕНКА** по двум прогонам в разное
  время. Larson (медленнее в 1.144×) и vec512 (медленнее в 1.659×)
  противоречат абляции: там larson быстрее в 1.014×, vec512 — равно. Это
  межпрогонный разброс этих двух ядер (larson против mimalloc: 0.890 →
  1.021 → 0.892 в трёх последних прогонах). Вердикт — по абляции.
- **Callgrind:** drain 63.1 → 53.9 Ir на пару, всего 287.9 → 278.7.

## Идентичность и сырьё

| Прогон | Код | Идентичность | Сырьё |
|---|---|---|---|
| абляция b2r1 / null | `b2-drain`@8dd5b2b4 | `b2_drain/run_identity.txt` (sha256 копий), патч `b2_drain/patches/` | 120 JSONL, `b2_drain/MANIFEST.txt` |
| против mimalloc | `b2-drain-prod`@dbf4f3b0, чистое дерево | head + `sha(diff)=e3b0c442…` + exe `13858acf38d7…` | 60 JSONL, `b2_drain/vs_mimalloc/MANIFEST.txt` |
| callgrind | `b2-drain`@cb50c9c2 ± патч | src и exe sha256 по вариантам в сырье | `b2_drain/_raw_callgrind_bulk.txt` |

Сырые JSONL больше 2 МиБ, поэтому в git только манифесты (sha256 и размер
каждого файла). Сводки: `B2_DRAIN_GATE_summary.csv` и
`B2_DRAIN_GATE_summary_callgrind.csv`. Сводку прогона против mimalloc
`b4_interleave_run.mjs` пишет в
`docs/perf/B4_INTERLEAVE_JUDGE_rep_summary.csv`. Она скопирована в
`b2_drain/vs_mimalloc/rep_summary_b2.csv`, закоммиченный файл восстановлен.

Команды:

```text
# против sefer (ветка b2-drain)
RUSTFLAGS="<выравнивание>" node scripts/b4_ablate_run.mjs --out-dir <dir> --blocks 2000 --reps 20 \
  --kernels pair16,pair64,pair_rt,bulk64,bulk16k,churn16,churn64,churn256,churn1024,vec512,larson \
  --variants b2r1_drain_one_pass --no-null-gate
node scripts/b4_ablate_vs_null.mjs --dir <dir> --variant b2r1_drain_one_pass --reps 20 --kernels <те же> --csv <csv>
# против mimalloc (эта ветка)
RUSTFLAGS="<выравнивание>" node scripts/b4_interleave_run.mjs --reps 20 --blocks 2000 --out-prefix <dir>/j
# отчёт
node scripts/b2_drain_gate_report.mjs --csv docs/perf/B2_DRAIN_GATE_summary.csv
```

## Не измерено

- MT-ядра (удалённые free) и RSS. B2 не меняет хранения; путь drain общий
  для своего потока.
