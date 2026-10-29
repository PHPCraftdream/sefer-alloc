# G1-K1 — drain без повторной записи кода класса: гейт (perf(runtime))

Статус: **GO** по предрегистрации (`docs/perf/g1_k1/G1_K1_PREREG.md`,
зафиксирована до прогона, коммит `afe594b4` на измерительной ветке).
Код под замером:
- абляция — ветка `g1-k1-meas`@`afe594b4` (base = main `37780bc7` + набор
  абляции из `51eda7b1` + патч `g1_k1/patches/k1_drain_no_reissue.patch`);
- против mimalloc — ветка `g1-k1`@`c0c4eacf` (чистое дерево).

Кандидат K1 из `docs/perf/G1_IDEAS.md` §4 (пункт OPEN_ITEMS 124).

## Что изменено

`AllocCore::try_drain_freelist_batch`
(`src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs`) больше не
вызывает на каждом выданном блоке `IssueRun::prepare_for` и `commit`, то есть
не записывает заново код класса блока в `ClassLeaves`.

Почему запись была лишней:

1. В список класса c блок попадает только после выдачи этим классом:
   `flush_run` (магазин), `reclaim_sidecar_record` (класс из записи sidecar —
   это и есть код листа), `dealloc_small` (байт `encode(c, LIVE)`).
2. При выдаче (нарезка, drain до K1, `pop_free`) код c+1 записан в лист.
3. Листы `ClassLeaves` только создаются (`promote`) и живут до последнего
   пина маршрута; код никогда не стирается. При сбросе сегмента списки
   обнуляются (`decommit.rs`).

Значит, на drain блок уже несёт код c+1, и повторная Release-запись того же
значения ничего не меняет для читателей других потоков: они синхронизированы
с первой записью через передачу указателя.

Что осталось:

- проверки маршрута `prepare_small_issue_run` (живой слот, совпадение корня,
  Small sidecar) — один раз на партию, как раньше;
- все проверки цепочки (`continuation_offset_or_fatal`; с `hardened` —
  `valid_free_node`, `free_continuation`, проверка возврата к голове, Флойд);
- запись байта состояния;
- запись кода на нарезке (`try_carve_batch`) и в `pop_free`.

Свидетель в отладочной сборке: `debug_assert!(run.code_present(head_off))`
на каждом блоке drain (`IssueRun::code_present` →
`RoutedIssueRun::code_present`, только под `debug_assertions`). Им проверяется
весь отладочный набор тестов CI.

Изменение контракта: drain больше не падает на OOM листа (листа он не
создаёт). Раньше этот отказ был только внедрённым: у блоков из списка лист
уже есть (`tests/r11_ph3a_issue_transaction_batch.rs`: «a drain of
already-prepared blocks allocates no sidecar storage»).

## Тесты

- Новый `tests/g1_k1_drain_class_code.rs`: у каждого блока, выданного drain
  (AllocCore, 300 блоков через два листа) и пополнением магазина из списка
  (HeapCore, классы 16/64/256/1024 по 4096 блоков), код класса на месте.
- Обновлены три теста, закреплявшие старый контракт «drain падает на
  внедрённом отказе prepare»: `r11_p3_small_sidecar_preflight.rs`
  (`batch_drain`), `r11_ph3a_issue_transaction_batch.rs` (сценарий (a)),
  `r11_ph3a_magazine_refill_partial.rs` (отказ взводится на первую нарезку).
  Теперь они проверяют, что drain не тратит взведённый отказ и его получает
  следующая нарезка. Контрфактуально: на исходниках main все три падают
  (ИЗМЕРЕНО локально, отладочная сборка), на K1 — проходят.

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
   --reps 20 --blocks 2000`). Сравнение с прогоном B2
   (`b2_drain/vs_mimalloc/rep_summary_b2.csv`).
3. **Инструкции** — callgrind в WSL (`examples/b4_pmu_probe.rs`, режимы
   `bulk` и `mixed` из PR #124), весь процесс. Это инструкции, а не время.

Оракул активации: судья проверяет точное число вызовов шима каждого плеча
и нули чужих вызовов (`oracle_mismatches=0` во всех 60 процессах прогона
против mimalloc). Активность пути: на bulk16k каждое пополнение берёт блоки
из списка; в callgrind self Ir `route_slots.rs` внутри drain падает с
3 548 440 до 0 (предрегистрация называла 3 458 888 — это была сумма строк
annotate не меньше 200 000 Ir; полная сумма функции — 3 548 440).

## Результат (вывод `scripts/g1_k1_gate_report.mjs` дословно)

```text
# G1-K1 gate — effect = median_processes ln(t_variant/t_sefer) net of null (20 processes x 2000 blocks each); «равно» = CI contains 0

| Ядро | k1_drain_no_reissue: эффект % [CI] | итог | null точен |
|---|---|---|---|
| pair16 | -0.04 [-0.07, 0.01] | равно | yes |
| pair64 | -0.02 [-0.06, 0.03] | равно | yes |
| pair_rt | -0.34 [-0.55, -0.16] | быстрее в 1.003× | yes |
| bulk64 | 1.80 [1.54, 2.00] | медленнее в 1.018× | yes |
| bulk16k | -6.87 [-7.35, -6.41] | быстрее в 1.071× | yes |
| churn16 | -3.10 [-8.00, -1.72] | быстрее в 1.031× | yes |
| churn64 | -1.42 [-3.61, -0.81] | быстрее в 1.014× | yes |
| churn256 | -6.21 [-8.55, -5.38] | быстрее в 1.064× | yes |
| churn1024 | -5.78 [-11.56, -4.49] | быстрее в 1.060× | yes |
| vec512 | 0.51 [0.17, 1.36] | медленнее в 1.005× | yes |
| larson | -0.86 [-1.31, -0.28] | быстрее в 1.009× | yes |

# k1_drain_no_reissue: GO
# k1_drain_no_reissue: faster on 7/11 (pair_rt,bulk16k,churn16,churn64,churn256,churn1024,larson); slower on 2/11 (bulk64,vec512)

# J1′ v2, sefer vs mimalloc: t_mimalloc / t_sefer (> 1: sefer faster); before = main with B2 (b2_drain run), after = main 37780bc7 + K1

| Ядро | до K1 t_mi/t_sefer | допущено | после K1 t_mi/t_sefer | допущено | sefer после против mimalloc | sefer после против до (ОЦЕНКА, разные прогоны) |
|---|---:|---|---:|---|---|---|
| pair16 | 0.8262 | yes | 0.8224 | yes | mimalloc быстрее в 1.216× | медленнее в 1.005× |
| pair64 | 0.9628 | yes | 0.9461 | yes | mimalloc быстрее в 1.057× | медленнее в 1.018× |
| pair_rt | 1.4373 | yes | 1.4369 | yes | sefer быстрее в 1.437× | медленнее в 1.000× |
| bulk64 | 0.8003 | yes | 0.7763 | yes | mimalloc быстрее в 1.288× | медленнее в 1.031× |
| bulk16k | 0.4752 | yes | 0.5114 | yes | mimalloc быстрее в 1.955× | быстрее в 1.076× |
| churn16 | 0.6009 | yes | 0.6065 | yes | mimalloc быстрее в 1.649× | быстрее в 1.009× |
| churn64 | 0.7871 | yes | 0.7627 | yes | mimalloc быстрее в 1.311× | медленнее в 1.032× |
| churn256 | 0.6432 | yes | 0.6697 | yes | mimalloc быстрее в 1.493× | быстрее в 1.041× |
| churn1024 | 0.7287 | yes | 0.7766 | yes | mimalloc быстрее в 1.288× | быстрее в 1.066× |
| vec512 | 1.1816 | yes | 1.2514 | no | не допущено A/A (точка: sefer быстрее в 1.251×) | быстрее в 1.059× |
| larson | 0.8919 | yes | 0.9065 | yes | mimalloc быстрее в 1.103× | быстрее в 1.016× |
# vs-mimalloc identity: head=c0c4eacf21646044d03dfb6ae9bac65826c9948a sha(diff)=e3b0c44298fc exe=304f3685ef0f

# callgrind, b4_pmu_probe, sefer arm, whole-process Ir (instructions, not time); route = route_slots.rs self Ir inside try_drain_freelist_batch

| Форма | Ir база | Ir K1 | изменение | route база | route K1 |
|---|---:|---:|---:|---:|---:|
| bulk16k | 95891733 | 85698115 | -10.63 % | 3548440 | 0 |
| mixed16_64 | 74563597 | 74563276 | -0.00 % | 0 | 0 |
| mixed256_1024 | 54412117 | 49528816 | -8.97 % | 1649978 | 0 |
| bulk64 | 56455208 | 56466604 | 0.02 % | 0 | 0 |
| pair16 | 70063078 | 70072382 | 0.01 % | 0 | 0 |

VERDICT=GO
```

Чтение:

- **Пороги 1–3 выполнены.**
  - bulk16k быстрее в 1.071× (верх CI −6.41 % < 0).
  - pair16, pair64 — «равно»; larson быстрее в 1.009×.
  - Медленнее 2 из 11: bulk64 в 1.018× (CI +1.54…+2.00 %) и vec512 в
    1.005× (vec512 не допущено главным судьёй по A/A). Порог 3 (эффект
    больше +2 % при нижней границе CI выше 0) не нарушен ни там, ни там.
- **bulk64 медленнее при тех же инструкциях:** callgrind bulk64 +0.02 %
  (56 455 208 → 56 466 604); в bulk64 все 64 блока живут в магазине и
  складе, drain не вызывается (route 0 в обоих вариантах). Механизма в
  изменённом коде для этого ядра нет — вероятно, раскладка кода копии.
  Записано в OPEN_ITEMS 125 как отдельный триггер.
- **Сильнее всего выигрывают перенос и длинные цепочки:** bulk16k в
  1.071×, churn256 в 1.064×, churn1024 в 1.060×, churn16 в 1.031×.
- **Против mimalloc** sefer быстрее на pair_rt (в 1.437×). На 9 из 11
  ядер mimalloc быстрее: bulk16k в 1.955×, churn16 в 1.649×, churn256 в
  1.493×, bulk64 в 1.288×, churn1024 в 1.288×.
- **Столбец «sefer после против до» — ОЦЕНКА** по двум прогонам в разное
  время и на разном main (до — main с B2, после — `37780bc7` с U31 и K1).
  Вердикт — по абляции.
- **Callgrind:** bulk16k −10.63 % Ir, mixed 256/1024 −8.97 %; mixed 16/64,
  bulk64, pair16 без изменений.

## Идентичность и сырьё

| Прогон | Код | Идентичность | Сырьё |
|---|---|---|---|
| абляция k1 / null | `g1-k1-meas`@afe594b4 | head + sha(diff) пустой + variant_sha `403184cb46cc` (k1), `ee61bd9972c1` (null) | 120 JSONL, `g1_k1/MANIFEST.txt` |
| против mimalloc | `g1-k1`@c0c4eacf, чистое дерево | head + `sha(diff)=e3b0c442…` + exe `304f3685ef0f…` | 60 JSONL, `g1_k1/vs_mimalloc/MANIFEST.txt` |
| callgrind | база `g1-pmu`@dfb89621, K1 `g1-k1`@c0c4eacf | bin sha256 в первой строке сырья | `g1_k1/_raw_callgrind_*.log` |

Сырые JSONL больше 2 МиБ, поэтому в git только манифесты (sha256 и размер
каждого файла). Сводки: `G1_K1_DRAIN_GATE_summary.csv` и
`G1_K1_DRAIN_GATE_summary_callgrind.csv`; таблица абляции —
`g1_k1/vs_null_k1_drain_no_reissue.txt`, против mimalloc —
`g1_k1/vs_mimalloc/generator_stdout.txt` (пересчитаны из сырья, совпадают с
выводом прогона).

Команды:

```text
# против sefer (ветка g1-k1-meas)
RUSTFLAGS="<выравнивание>" node scripts/b4_ablate_run.mjs --out-dir <dir> --blocks 2000 --reps 20 \
  --kernels pair16,pair64,pair_rt,bulk64,bulk16k,churn16,churn64,churn256,churn1024,vec512,larson \
  --variants k1_drain_no_reissue --no-null-gate
node scripts/b4_ablate_vs_null.mjs --dir <dir> --variant k1_drain_no_reissue --reps 20 --kernels <те же> --csv <csv>
# против mimalloc (ветка g1-k1)
RUSTFLAGS="<выравнивание>" node scripts/b4_interleave_run.mjs --reps 20 --blocks 2000 --out-prefix <dir>/j
# callgrind (WSL; скрипты docs/perf/g1_pmu/ из PR #124)
G1_BASE=<dir> bash g1_pmu.sh <label> <identity>; bash g1_cg.sh <label>; bash g1_cg.sh <label> set2
# отчёт
node scripts/g1_k1_gate_report.mjs --csv docs/perf/G1_K1_DRAIN_GATE_summary.csv
```

## Не измерено

- MT-ядра (удалённые free) и RSS. K1 не меняет хранения; drain — путь
  своего потока, а код листа читают удалённые free (он остаётся записанным
  с первой выдачи).
