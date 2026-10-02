# PG-3 (bench, measurement-only): iai-плечо спайка off-body NextTable (cold/recycle, информативно)

Дата: 2026-10-01. Спайк `8a028616` «off-body NextTable replaces intrusive freelist
next (not for merge)». Плечо PG-3 по ADR
`docs/design/2026-10-01-adr-physical-boundary-and-progress.md`: «iai-плечо спайка
на cold/recycle (информативно; EstCycles хуже ≥ 25% без пути улучшения — пересмотр
геометрии)».

Инструмент: `scripts/pg3_iai_table.mjs` (ESM, node v24, запуск из корня pg3-spike).
Машиночитаемый результат: `docs/perf/PG3_OFFBODY_SPIKE_IAI_summary.csv`.

## Идентичность

| Что | Значение |
|---|---|
| base HEAD | `7c0c796b45e352d86e58bd3f2ec993f3735030f6` (worktree `../pg3-base`, дерево чистое, `git status` пуст) |
| spike HEAD | `8a0286166a07c3ff35258836400d94f16c7994cd` (worktree pg3-spike) |
| spike diff | `git diff \| sha256sum` = `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` — пустой diff, патч закоммичен |
| features | `production bench-internals` (cargo-строка в логе: `production bench-internals internals`; `internals` аддитивен и обязателен по `required-features` у `benches/perf_gate_iai.rs`) |
| valgrind | 3.22.0 (callgrind-3.22.0, I1/D1 = 32 KiB, LL = 8 MiB) |
| rustc | 1.98.1 (48a229cea 2026-09-01) |
| судья | `node scripts/iai.mjs` (WSL + callgrind, `Ir` детерминирован) |
| набор | 85 бенчей `perf_gate_iai::perf_gate::*` в обоих прогонах (проверено assert'ом совпадения множеств) |

По соглачению записки все пути относительные: base-лог —
`docs/perf/_raw_pg3_iai_base.log`, spike-лог —
`docs/perf/_raw_pg3_iai_spike.log`.

## Сначала — артефакт: первый spike-прогон отброшен, это НЕ измерение спайка

`node scripts/iai.mjs` гоняет cargo с общим `CARGO_TARGET_DIR=/tmp/sefer-iai`.
Оба ворктрида собирались в одном и том же каталоге, поэтому «свежесть» артефактов
решалась по mtime исходников, а не по содержимому. Первый spike-прогон
(лог `docs/perf/_raw_pg3_iai_spike.log` на тот момент, mtime `22:58:41`) содержал
`Finished ... in 2.91s` и **ни одной строки `Compiling`** — то есть он исполнил
бинарник, собранный из **pg3-base**:

- `/tmp/sefer-iai/release/deps/perf_gate_iai-1eae813e19149d63` — mtime `22:54:08`
  (сборка base в `22:54`), а не время spike-прогона;
- в `.d` этого bench-бинарника записан `CARGO_MANIFEST_DIR`, указывающий на
  ворктрид **pg3-base** (а не pg3-spike);
- callgrind-выходы spike-прогона датированы `22:57:09`–`22:58:36` — раньше, чем
  сам лог, но они сняты с тем же base-бинарником;
- следствие в самих числах: **76 из 85** бенчей побитово совпадают с base
  (`cold_alloc_free_256x16b`: 196387|196387 Ir, 331452|331452 EstCycles), а вторая
  колонка spike-лога равна первой колонке base-лога (например
  `dealloc_own_thread_body_only_16b`: base 81717, spike 81729 при baseline 81717).

Итог: таблица, построенная на том логе, даёт Δ ≈ 0 — это **A/A двумя прогонами
одного и того же бинарника**, а не измерение спайка. Регистрация шума того
(отброшенного) прогона — как контроль уровня: max |ΔEstCycles| по 85 бенчам =
**+0.050%** (`dealloc_own_thread_body_only_16b`), max ΔIr = +0.020%.

**Этот прогон отброшен.** Тот же класс ловушки (общий artifact-cache между
ворктридами → молчаливый replay чужого бинарника) в репозитории уже описан в
`scripts/stale-artifact-diagnosis.mjs` (task #1073: общий `CARGO_TARGET_DIR`,
rustc запекает `CARGO_MANIFEST_DIR` в бинарник, чужой артефакт переигрывается).
В шапке `scripts/pg3_iai_table.mjs` оставлен комментарий, что артефактный прогон
(общий `/tmp/sefer-iai`, без пересборки, 76/85 совпадений) выброшен, а
оставшийся лог — пере-измерение.

### Повторное измерение (команда) — то, что в spike-логе сейчас

```
# запускается из корня pg3-spike; <wsl-root> — тот же корень, отображённый в WSL
wsl -e bash -lc "cd <wsl-root> && unset RUSTC_WRAPPER CARGO_BUILD_RUSTC_WRAPPER && \
  RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= CARGO_TARGET_DIR=/tmp/sefer-iai-verify \
  cargo bench --bench perf_gate_iai --features 'production bench-internals internals'"
```

Приватный `CARGO_TARGET_DIR=/tmp/sefer-iai-verify` гарантирует сборку из pg3-spike.
Подтверждение сборки (проверено в WSL, `/tmp/sefer-iai-verify`):

- `CACHEDIR.TAG` — `23:28`, `release/` — `23:28`, `iai/` — `23:29` (времена
  пере-измерения);
- `.d` bench-бинарника `/tmp/sefer-iai-verify/release/deps/perf_gate_iai-*.d`
  содержит `# env-dep:CARGO_MANIFEST_DIR=/mnt/d/dev/rust/sefer-alloc/worktrees/pg3-spike`
  — то есть бинарник собран именно из **pg3-spike**;
- хвост лога: `Iai-Callgrind result: Ok. 85 without regressions; 0 regressed; 85
  benchmarks finished in 28.3103s` — все 85 бенчей прогнаны заново.

Сам файл `docs/perf/_raw_pg3_iai_spike.log` (44 011 B = 42.98 KiB, 597 строк,
85 бенчей) — это **захват выхода iai-harness**: 85 заголовков
`perf_gate_iai::perf_gate::*`, по 6 строк метрик на бенч и итоговая строка
результата. Преамбулы cargo (`Compiling …`) в этом захвате нет — Build-доказательство
приведено выше по `/tmp/sefer-iai-verify` (mtime + `.d`), а формат метрик сам
доказывает свежесть baseline: строки имеют вид `392243|N/A` (`N|N/A` — свежий прогон,
не с чем сравнивать), тогда как base-лог — `196387|196387 (No change)`.

По соглашению с прошлой версией записки прежний verify-файл
`docs/perf/_raw_pg3_iai_spike_verify.log` удалён: его содержимое теперь и есть
`docs/perf/_raw_pg3_iai_spike.log`.

### Контроль A/A внутри настоящего измерения

mimalloc-плечи (`mimalloc_*`, 11 бенчей) не используют код спайка вовсе и служат
встроенным A/A: ΔIr = **+0.000%** ровно, max ΔEstCycles = **+0.545%**
(`mimalloc_bootstrap_proxy`). Значит шум оси EstCycles на этой машине ~±0.5%, а
сигналы ниже (+9…+92%) далеко за его пределами.

---

## Таблица — настоящее измерение спайка (`docs/perf/_raw_pg3_iai_spike.log`)

Полный вывод `node scripts/pg3_iai_table.mjs` (CSV пишется тем же запуском):

```text
# PG-3 off-body NextTable spike — iai A/B (callgrind, deterministic)

base:  `docs/perf/_raw_pg3_iai_base.log` (pg3-base @ 7c0c796b45e352d86e58bd3f2ec993f3735030f6, tree clean)
spike: `docs/perf/_raw_pg3_iai_spike.log` (pg3-spike @ 8a0286166a07c3ff35258836400d94f16c7994cd, tree clean — `git diff` empty)
features: `production bench-internals` (log header: `production bench-internals internals`)  |  benches: 85

Every metric column is the FIRST number of the iai row (the current run); the
second number is the runner's own baseline (a count, or `N/A` for a fresh
run) and is deliberately ignored.
ΔX% = (spike − base) / base × 100. Every percent goes through a guarded
printer that asserts `Number.isFinite(pct)` and that
`base*(1+pct/100)` reproduces spike to ±0.01%.

| bench | Ir base | Ir spike | ΔIr % | Cycles base | Cycles spike | ΔCycles % | groups |
|---|---:|---:|---:|---:|---:|---:|---|
| small_churn_16b | 60549 | 72847 | +20.311% | 147499 | 161853 | +9.732% | hot-churn |
| small_churn_16b_2n | 67217 | 79503 | +18.278% | 156013 | 170267 | +9.136% | hot-churn |
| dealloc_prealloc_only_16b | 74068 | 123212 | +66.350% | 165091 | 221451 | +34.139% |  |
| dealloc_free_only_16b | 82686 | 131686 | +59.260% | 177624 | 233578 | +31.501% |  |
| dealloc_contains_base_probe_only_16b | 75427 | 124571 | +65.154% | 167018 | 223340 | +33.722% |  |
| dealloc_segment_base_of_ptr_probe_only_16b | 74658 | 123802 | +65.825% | 165885 | 222211 | +33.955% |  |
| alloc_magazine_prefill_only_16b | 59593 | 71867 | +20.596% | 145446 | 159876 | +9.921% | magazine |
| alloc_magazine_hit_only_16b | 60082 | 72368 | +20.449% | 146192 | 160680 | +9.910% | magazine |
| alloc_zeroed_magazine_prefill_only_16b | 59581 | 71867 | +20.621% | 145448 | 159842 | +9.896% | magazine |
| alloc_zeroed_magazine_hit_only_16b | 60119 | 72405 | +20.436% | 146429 | 160789 | +9.807% | magazine |
| dealloc_hash_contains_only_probe_16b | 76449 | 125593 | +64.283% | 168136 | 224390 | +33.457% |  |
| dealloc_own_thread_body_only_16b | 81717 | 130729 | +59.978% | 176283 | 232363 | +31.812% |  |
| dealloc_free_only_16b_n1 | 74137 | 123281 | +66.288% | 165494 | 221782 | +34.012% |  |
| dealloc_free_only_16b_n8 | 74596 | 123740 | +65.880% | 166078 | 222434 | +33.933% |  |
| dealloc_free_only_16b_n9 | 74661 | 123805 | +65.823% | 166195 | 222483 | +33.869% |  |
| dealloc_free_only_16b_n16 | 75116 | 124260 | +65.424% | 166708 | 223064 | +33.805% |  |
| dealloc_free_only_16b_n17 | 75926 | 125046 | +64.695% | 168535 | 224943 | +33.470% |  |
| dealloc_free_only_16b_n32 | 77642 | 126738 | +63.234% | 170872 | 227094 | +32.903% |  |
| dealloc_flush_class_only_16b_prefix | 54470 | 66756 | +22.556% | 139091 | 153485 | +10.349% | flush |
| dealloc_flush_class_only_16b | 54987 | 67249 | +22.300% | 140354 | 154826 | +10.311% | flush |
| alloc_clear_magazine_only_16b_prefix | 54986 | 67272 | +22.344% | 139580 | 154042 | +10.361% | magazine |
| alloc_clear_magazine_only_16b | 55371 | 67645 | +22.167% | 140187 | 154481 | +10.196% | magazine |
| alloc_zeroed_calloc_virgin_64k_prefix | 3 | 3 | +0.000% | 39 | 39 | +0.000% | refill |
| alloc_zeroed_calloc_virgin_64k | 3 | 3 | +0.000% | 39 | 39 | +0.000% | refill |
| alloc_zeroed_calloc_recycled_64k_prefix | 3 | 3 | +0.000% | 39 | 39 | +0.000% |  |
| alloc_zeroed_calloc_recycled_64k | 3 | 3 | +0.000% | 39 | 39 | +0.000% |  |
| dealloc_prealloc_only_1088_16b | 517355 | 1352815 | +161.487% | 769428 | 1718004 | +123.283% |  |
| dealloc_free_only_1088_16b_n17 | 519213 | 1354649 | +160.904% | 772936 | 1721406 | +122.710% |  |
| dealloc_free_only_1088_16b_n32 | 520929 | 1356341 | +160.370% | 775205 | 1723693 | +122.353% |  |
| dealloc_free_only_1088_16b_n64 | 525973 | 1361289 | +158.813% | 781889 | 1730177 | +121.282% |  |
| dealloc_free_only_1088_16b_n256 | 556237 | 1390977 | +150.069% | 822393 | 1769011 | +115.105% |  |
| dealloc_free_only_1088_16b_n1024 | 677293 | 1509729 | +122.906% | 984613 | 1924659 | +95.474% |  |
| dealloc_realloc_burst_1088_16b_n17 | 526403 | 1374053 | +161.027% | 782904 | 1745329 | +122.930% |  |
| oscillating_live_set_16b | 91238 | 115786 | +26.905% | 186886 | 215246 | +15.175% |  |
| carve_batch_only_16b | 3238 | 3238 | +0.000% | 8111 | 8081 | -0.370% | refill |
| carve_batch_only_16b_2n | 9139 | 9139 | +0.000% | 18425 | 18387 | -0.206% | refill |
| dealloc_batch_fresh_16_16b | 3 | 3 | +0.000% | 39 | 39 | +0.000% |  |
| dealloc_batch_fresh_64_16b | 3 | 3 | +0.000% | 39 | 39 | +0.000% |  |
| dealloc_batch_fresh_80_16b | 3 | 3 | +0.000% | 39 | 39 | +0.000% |  |
| dealloc_batch_fresh_81_16b | 3 | 3 | +0.000% | 39 | 39 | +0.000% |  |
| dealloc_batch_fresh_128_16b | 3 | 3 | +0.000% | 39 | 39 | +0.000% |  |
| dealloc_batch_fresh_200_16b | 3 | 3 | +0.000% | 39 | 39 | +0.000% |  |
| dealloc_batch_fresh_512_16b | 3 | 3 | +0.000% | 39 | 39 | +0.000% |  |
| dealloc_batch_fresh_1024_16b | 3 | 3 | +0.000% | 39 | 39 | +0.000% |  |
| dealloc_batch_fresh_0_16b | 3 | 3 | +0.000% | 39 | 39 | +0.000% |  |
| dealloc_batch_fresh_1_16b | 3 | 3 | +0.000% | 39 | 39 | +0.000% |  |
| dealloc_batch_fresh_8_16b | 3 | 3 | +0.000% | 39 | 39 | +0.000% |  |
| dealloc_batch_fresh_17_16b | 3 | 3 | +0.000% | 39 | 39 | +0.000% |  |
| medium_class_dealloc_churn_16b | 60561 | 72835 | +20.267% | 147493 | 161803 | +9.702% |  |
| aligned_churn_640b_a128 | 60555 | 72841 | +20.289% | 147629 | 161883 | +9.655% |  |
| large_alloc_free_cycle | 49912 | 49912 | +0.000% | 133866 | 133930 | +0.048% |  |
| large_cache_prefill_only_4mib | 59190 | 59178 | -0.020% | 146835 | 147003 | +0.114% |  |
| large_cache_hit_only_4mib | 60570 | 60570 | +0.000% | 149395 | 149655 | +0.174% |  |
| large_cache_free_slot_search_prefill_only | 64283 | 64283 | +0.000% | 157793 | 157997 | +0.129% |  |
| large_cache_free_slot_search_cycle_only | 76156 | 76156 | +0.000% | 175501 | 175671 | +0.097% |  |
| realloc_grow | 611862 | 759658 | +24.155% | 3741036 | 3907959 | +4.462% |  |
| cold_alloc_free_256x16b | 196387 | 392243 | +99.730% | 331452 | 553226 | +66.910% | cold |
| cold_alloc_free_256x16b_2n | 351101 | 742765 | +111.553% | 542332 | 985094 | +81.640% | cold |
| cold_alloc_free_256x16b_4n | 658969 | 1442249 | +118.864% | 959453 | 1843705 | +92.162% | cold |
| cold_alloc_only_256x16b | 156474 | 353050 | +125.629% | 277218 | 500858 | +80.673% | cold |
| cold_alloc_only_256x16b_2n | 269812 | 662964 | +145.713% | 432892 | 879562 | +103.183% | cold |
| cold_alloc_only_256x16b_4n | 494928 | 1281232 | +158.872% | 739401 | 1632083 | +120.730% | cold |
| cold_alloc_free_256x64b | 197242 | 393098 | +99.297% | 338914 | 555760 | +63.983% | cold |
| recycle_alloc_free_256x16b | 307160 | 500405 | +62.913% | 477277 | 696475 | +45.927% | recycle |
| recycle_alloc_only_256x16b | 266258 | 460223 | +72.849% | 424175 | 643955 | +51.814% | recycle |
| recycle_alloc_free_256x64b | 308015 | 501260 | +62.739% | 485019 | 699179 | +44.155% | recycle |
| churn_256b | 60549 | 72835 | +20.291% | 147533 | 161829 | +9.690% | hot-churn |
| churn_write_256b | 60741 | 73039 | +20.247% | 147959 | 162275 | +9.676% | hot-churn |
| multiseg_cold_256k | 4200303 | 6076158 | +44.660% | 5924989 | 7540737 | +27.270% | decommit |
| seg_cycle_decommit_256k | 12859029 | 18817444 | +46.336% | 17792203 | 22946775 | +28.971% | decommit |
| mimalloc_small_churn_16b | 16629 | 16629 | +0.000% | 39216 | 39340 | +0.316% |  |
| mimalloc_small_churn_16b_2n | 21557 | 21557 | +0.000% | 45676 | 45800 | +0.271% |  |
| mimalloc_churn_256b | 16130 | 16130 | +0.000% | 37129 | 37249 | +0.323% |  |
| mimalloc_cold_alloc_free_256x16b | 32325 | 32325 | +0.000% | 59743 | 59901 | +0.264% |  |
| mimalloc_cold_alloc_free_256x16b_2n | 58389 | 58389 | +0.000% | 99189 | 99393 | +0.206% |  |
| mimalloc_cold_alloc_free_256x16b_4n | 106653 | 106653 | +0.000% | 170926 | 171104 | +0.104% |  |
| mimalloc_cold_alloc_only_256x16b | 24584 | 24584 | +0.000% | 48639 | 48831 | +0.395% |  |
| mimalloc_cold_alloc_only_256x16b_2n | 42968 | 42968 | +0.000% | 77375 | 77571 | +0.253% |  |
| mimalloc_cold_alloc_only_256x16b_4n | 75872 | 75872 | +0.000% | 127554 | 127804 | +0.196% |  |
| mimalloc_cold_alloc_free_256x64b | 33329 | 33329 | +0.000% | 68186 | 68412 | +0.331% |  |
| mimalloc_recycle_alloc_free_256x16b | 53027 | 53027 | +0.000% | 86726 | 86960 | +0.270% |  |
| mimalloc_recycle_alloc_free_256x64b | 54031 | 54031 | +0.000% | 95131 | 95403 | +0.286% |  |
| mimalloc_bootstrap_proxy | 13050 | 13050 | +0.000% | 34871 | 35061 | +0.545% |  |
| decomp_full_cycle_8x | 680761 | 680761 | +0.000% | 1453499 | 1453607 | +0.007% |  |
| decomp_os_roundtrip_8x | 861 | 861 | +0.000% | 2046 | 2046 | +0.000% |  |

## Сводка против порогов (PG-3 / ADR §Гейты)

hot path (4 benches; `small_churn*`, `churn*`) — EstCycles ≤ +2% :
  max ΔEstCycles = +9.732% (x1.09732), mean Δ = +9.558%  ->  FAIL
  max ΔIr        = +20.311% (x1.20311), mean ΔIr = +19.782%
  ADR hot set (small_churn_16b, churn_256b, churn_write_256b, aligned_churn_640b_a128): max ΔEstCycles = +9.732% (x1.09732)

refill path (4 benches; `carve_batch*`, `alloc_zeroed_calloc_virgin*`) — EstCycles ≤ +5% :
  max ΔEstCycles = +0.000% (x1.00000)  ->  PASS
  max ΔIr        = +0.000% (x1.00000)
  ADR refill/flush set (cold_alloc_free_*/recycle_alloc_free_*/multiseg_cold_256k, 7 benches): max ΔEstCycles = +92.162% (x1.92162)
  ADR refill/flush Ir ≤ 1.10x: max ΔIr = +118.864% (x2.18864)

PG-3 («EstCycles хуже ≥ 25% без пути улучшения — пересмотр геометрии»):
  benches with ΔEstCycles ≥ +25%: 31
    dealloc_prealloc_only_16b: +34.139% (x1.34139)
    dealloc_free_only_16b: +31.501% (x1.31501)
    dealloc_contains_base_probe_only_16b: +33.722% (x1.33722)
    dealloc_segment_base_of_ptr_probe_only_16b: +33.955% (x1.33955)
    dealloc_hash_contains_only_probe_16b: +33.457% (x1.33457)
    dealloc_own_thread_body_only_16b: +31.812% (x1.31812)
    dealloc_free_only_16b_n1: +34.012% (x1.34012)
    dealloc_free_only_16b_n8: +33.933% (x1.33933)
    dealloc_free_only_16b_n9: +33.869% (x1.33869)
    dealloc_free_only_16b_n16: +33.805% (x1.33805)
    dealloc_free_only_16b_n17: +33.470% (x1.33470)
    dealloc_free_only_16b_n32: +32.903% (x1.32903)
    dealloc_prealloc_only_1088_16b: +123.283% (x2.23283)
    dealloc_free_only_1088_16b_n17: +122.710% (x2.22710)
    dealloc_free_only_1088_16b_n32: +122.353% (x2.22353)
    dealloc_free_only_1088_16b_n64: +121.282% (x2.21282)
    dealloc_free_only_1088_16b_n256: +115.105% (x2.15105)
    dealloc_free_only_1088_16b_n1024: +95.474% (x1.95474)
    dealloc_realloc_burst_1088_16b_n17: +122.930% (x2.22930)
    cold_alloc_free_256x16b: +66.910% (x1.66910)
    cold_alloc_free_256x16b_2n: +81.640% (x1.81640)
    cold_alloc_free_256x16b_4n: +92.162% (x1.92162)
    cold_alloc_only_256x16b: +80.673% (x1.80673)
    cold_alloc_only_256x16b_2n: +103.183% (x2.03183)
    cold_alloc_only_256x16b_4n: +120.730% (x2.20730)
    cold_alloc_free_256x64b: +63.983% (x1.63983)
    recycle_alloc_free_256x16b: +45.927% (x1.45927)
    recycle_alloc_only_256x16b: +51.814% (x1.51814)
    recycle_alloc_free_256x64b: +44.155% (x1.44155)
    multiseg_cold_256k: +27.270% (x1.27270)
    seg_cycle_decommit_256k: +28.971% (x1.28971)

худшие 5 бенчей по ΔEstCycles:
  dealloc_prealloc_only_1088_16b             ΔEstCycles +123.283% (x2.23283)  ΔIr +161.487%
  dealloc_realloc_burst_1088_16b_n17         ΔEstCycles +122.930% (x2.22930)  ΔIr +161.027%
  dealloc_free_only_1088_16b_n17             ΔEstCycles +122.710% (x2.22710)  ΔIr +160.904%
  dealloc_free_only_1088_16b_n32             ΔEstCycles +122.353% (x2.22353)  ΔIr +160.370%
  dealloc_free_only_1088_16b_n64             ΔEstCycles +121.282% (x2.21282)  ΔIr +158.813%

худшие 5 бенчей по ΔIr:
  dealloc_prealloc_only_1088_16b             ΔIr +161.487% (x2.61487)
  dealloc_realloc_burst_1088_16b_n17         ΔIr +161.027% (x2.61027)
  dealloc_free_only_1088_16b_n17             ΔIr +160.904% (x2.60904)
  dealloc_free_only_1088_16b_n32             ΔIr +160.370% (x2.60370)
  cold_alloc_only_256x16b_4n                 ΔIr +158.872% (x2.58872)

лучшие 3 по ΔEstCycles (может быть выигрыш):
  carve_batch_only_16b                       ΔEstCycles -0.370%
  carve_batch_only_16b_2n                    ΔEstCycles -0.206%
  alloc_zeroed_calloc_virgin_64k_prefix      ΔEstCycles +0.000%

CSV записан: docs/perf/PG3_OFFBODY_SPIKE_IAI_summary.csv
  заголовок: bench,ir_base,ir_spike,cycles_base,cycles_spike,commit,features
  строк: 85
```

## Вывод против порогов

Все числа — из вывода скрипта выше (настоящее измерение). Знаменатель у каждого
процента: **ΔX% = (spike − base) / base × 100**, X ∈ {Ir, EstCycles}; порог-плечо —
отношение C/B в одном режиме, т.е. `1.00 + Δ/100`.

| Порог | Группа | Результат | Вердикт |
|---|---|---|---|
| Горячий tcache EstCycles ≤ **1.02x** (ADR §Гейты) | `small_churn*`, `churn*` (4 бенча) | max ΔEstCycles = **+9.732%** (x1.09732, `small_churn_16b`), mean = +9.558%; max ΔIr = +20.311% (x1.20311) | **ПРОВАЛ** |
| То же, набор ADR дословно (`small_churn_16b`, `churn_256b`, `churn_write_256b`, `aligned_churn_640b_a128`) | те же | max ΔEstCycles = **+9.732%** (x1.09732) | **ПРОВАЛ** |
| Refill/flush EstCycles ≤ **1.05x**, Ir ≤ 1.10x (ADR §Гейты: `cold_alloc_free_256x{16,64}b`, `recycle_alloc_free_256x{16,64}b`, `multiseg_cold_256k`) | 7 бенчей | max ΔEstCycles = **+92.162%** (x1.92162, `cold_alloc_free_256x16b_4n`), max ΔIr = +118.864% (x2.18864) | **ПРОВАЛ** |
| «refill-путь» в узком смысле (`carve_batch*`, `alloc_zeroed_calloc_virgin*`) | 4 бенча | +0.000% | формальный проход, но это вырожденные бенчи: Ir = 3238/9139/3/3 — реальной нагрузки нет |
| PG-3: EstCycles хуже ≥ **25%** | все 85 | **31 бенч** | порог срабатывает → «пересмотр геометрии» |

Худшие 5 по ΔEstCycles:

| бенч | ΔEstCycles | ΔIr |
|---|---:|---:|
| `dealloc_prealloc_only_1088_16b` | +123.283% (x2.23283) | +161.487% |
| `dealloc_realloc_burst_1088_16b_n17` | +122.930% (x2.22930) | +161.027% |
| `dealloc_free_only_1088_16b_n17` | +122.710% (x2.22710) | +160.904% |
| `dealloc_free_only_1088_16b_n32` | +122.353% (x2.22353) | +160.370% |
| `dealloc_free_only_1088_16b_n64` | +121.282% (x2.21282) | +158.813% |

Лучшие 3 (выигрыш): `carve_batch_only_16b` −0.370%, `carve_batch_only_16b_2n`
−0.206%, `alloc_zeroed_calloc_virgin_64k_prefix` +0.000%.

Итог по геометрии: геометрия «один `u32` на `MIN_BLOCK`-слот всего сегмента»
(1 МиБ на 4 МиБ сегмент) даёт плату за каждый retire/pop блока — хранение/чтение
`next` в разбросанной по 1 МиБ таблице + поддержка двух структур (`alloc_bitmap`
и `NextTable`) вместо одной горячей записи в тело блока. На cold/recycle, где
много сегментов и много боков, это до +92% EstCycles: требование ADR «пересмотр
геометрии» (per-class leaves вместо плоской таблицы на весь сегмент) —
подтверждённое направление, а не гипотеза.

## Механизм цены

Что показал `git show 8a028616 --stat`: `NextTable` — новая метадата сегмента
(`src/alloc_core/segment/next_table.rs`, +77 строк), один `u32` на каждый
`MIN_BLOCK`-слот всего сегмента, т.е. `FOOTPRINT = (SEGMENT / MIN_BLOCK) * 4` =
**1 МиБ на 4 МиБ сегмент**, врезанная в `Layout::owner_metadata_end()`
(`small_meta_end()` вырос на `FOOTPRINT`). Интрузивные
`Node::write_next`/`Node::read_next` убраны с путей retire/pop: цепочка `next`
живёт в метаданных, `BinTable[class].head` остался флагом непустоты
(приёмочная записка спайка — `docs/reviews/2026-10-01-adr-pg2-offbody-spike-receipt.md`).

### Атрибуция (перепроверена заново, `callgrind_annotate --auto=yes`)

Числа ниже — **не** перенесённые из прошлой версии записки, а заново снятые в этом
этапе: bench-бинарники обеих сторон
(`/tmp/sefer-iai/release/deps/perf_gate_iai-1eae813e19149d63` = pg3-base и
`/tmp/sefer-iai-verify/release/deps/perf_gate_iai-1eae813e19149d63` = pg3-spike,
последний — по `.d` с `CARGO_MANIFEST_DIR=.../pg3-spike`) прогнаны под
`valgrind --tool=callgrind --cache-sim=yes
--toggle-collect='*::__iai_callgrind_wrapper_mod::*'` с `--iai-run perf_gate <i> 0
perf_gate_iai::perf_gate` (i = 0 для `small_churn_16b`, i = 3 для
`dealloc_free_only_16b`; индекс подтверждён тем, что в захвате то же имя бенча в
функции-обёртке `__iai_callgrind_wrapper_mod`, а «Collected» Ir = 60561 / 82686
против 60549 / 82686 в base-логе — расхождение ±12 Ir от setup-обвязки раннера,
на функции уровня оно не влияет).

| бенч | функция | base Ir | spike Ir |
|---|---|---:|---:|
| `small_churn_16b` | `AllocCore::drain_segment_sidecar` | 2 309 | 14 597 |
| `small_churn_16b` | `__memset_avx2_unaligned_erms` | 42 180 | 42 180 |
| `small_churn_16b` | `AllocCore::try_carve_batch` | 733 | 733 |
| `small_churn_16b` | `RouteSlots::issue_small` | 879 | 879 |
| `small_churn_16b` | `RouteSlots::prepare_small_issue` | 768 | 768 |
| `small_churn_16b` | `SmallSidecar::issue` | 766 | 766 |
| `small_churn_16b` | `HeapCore::refill_magazine_slow` | 542 | 542 |
| `dealloc_free_only_16b` | `AllocCore::drain_segment_sidecar` | 9 272 | 58 424 |
| `dealloc_free_only_16b` | `__memset_avx2_unaligned_erms` | 42 230 | 42 230 |
| `dealloc_free_only_16b` | `AllocCore::try_carve_batch` | 2 932 | 2 932 |
| `dealloc_free_only_16b` | `RouteSlots::issue_small` | 3 519 | 3 519 |
| `dealloc_free_only_16b` | `RouteSlots::prepare_small_issue` | 3 216 | 3 216 |
| `dealloc_free_only_16b` | `SmallSidecar::issue` | 3 070 | 3 070 |
| `dealloc_free_only_16b` | `HeapCore::refill_magazine_slow` | 2 075 | 2 075 |
| `dealloc_free_only_16b` | тело бенча (`perf_gate_iai::dealloc_free_only_16b`) | 7 498 | 7 498 |

Дополнительно по той же функции (счётчики кэша из тех же прогонов):

| бенч | `drain_segment_sidecar` | base | spike |
|---|---|---:|---:|
| `small_churn_16b` | Ir | 2 309 | 14 597 |
| `small_churn_16b` | Dr / Dw | 398 / 385 | 1 233 / 1 222 |
| `small_churn_16b` | D1 read-miss | 25 | 153 |
| `dealloc_free_only_16b` | Ir | 9 272 | 58 424 |
| `dealloc_free_only_16b` | Dr / Dw | 1 598 / 1 546 | 4 935 / 4 891 |
| `dealloc_free_only_16b` | D1 read-miss | 26 | 154 |

Читается так:

1. **Freelist-пуш/поп сам по себе почти бесплатен.** `try_carve_batch`
   (733 Ir в `small_churn_16b`, 2 932 в `dealloc_free_only_16b`), `issue_small`,
   `prepare_small_issue`, `SmallSidecar::issue`, `refill_magazine_slow`
   (2 075 Ir в `dealloc_free_only_16b`) совпадают до инструкции в обоих прогонах.
   Замена записи `next` в тело блока на запись `u32` в метаданные — тот же один
   store; вычитание указателя (`next - segment`) из pop убрано.
2. **Цена «+1 МиБ метаданных на сегмент» не видна как eager-инициализация.**
   `__memset_avx2_unaligned_erms` идентичен (42 180/42 180 и 42 230/42 230 Ir):
   virgin-skip работает, 1 МиБ NextTable при резерве не обнуляется.
3. **Вся деградация — в `AllocCore::drain_segment_sidecar`** (owner-сторона
   разбора terminal small-sidecar записей, `reclaim_sidecar_record`). На
   `dealloc_free_only_16b` она выросла 9 272 → 58 424 Ir (+49 152) при общем
   приросте бенча +48 860 Ir (131 686 − 82 686), т.е. ≈100% прироста. На
   `small_churn_16b` 2 309 → 14 597 (+12 288) при общем +12 274 — тоже ≈100%.
   При этом трафик памяти внутри функции растёт умеренно
   (Dr 1 598→4 935, Dw 1 546→4 891), а D1 read-miss — 26 → 154 (и 25 → 153 на
   `small_churn_16b`): картина «тот же объём данных, но разбросан по многим
   разным строкам/страницам».
   *(Уточнение к прошлой версии записки: там утверждалось «D1-miss 5 → 153»;
   перепроверка даёт 25 → 153 (`small_churn_16b`) и 26 → 154
   (`dealloc_free_only_16b`) — качественный вывод без изменений, base-значение
   исправлено.)*
4. **Объяснение (вывод из кода, не измерение):** сам спайк это документирует —
   в `alloc_core_small_magazine.rs:571` осталась пометка «the intrusive LIFO chain
   is gone; pops re-enumerate via the bitmap». Пока `next` жил в теле блока,
   retire-запись и последующий pop шли по одному и тому же адресу, который всё
   равно вот-вот уйдёт пользователю (горячая строка/страница). Теперь каждый
   retire пишет `next` в слот таблицы, разбросанный по 1 МиБ метаданных
   (слот = `off >> 4`), а owner-side `reclaim_sidecar_record`
   (`src/alloc_core/small/alloc_core_small_reclaim.rs:19`, вызывается из
   `drain_segment_sidecar`, `src/alloc_core/small/alloc_core_small/find_segment.rs:116`)
   на каждую запись делает `bitmap.is_free`, `next_table().set_next`,
   `bins.set_head`, `bitmap.mark_free`, `meta.dec_live` — две записи в две
   разные структуры (4-байтный слот в 1 МиБ таблице + слово битмапа) вместо
   одной горячей записи в тело блока. cold/recycle (много сегментов и много
   боков) платят больше всего (до +92% EstCycles), churn с одним прогретым
   сегментом — меньше (+9.7%).

Что в спайке выглядит дефектом/риском для следующей итерации (не гейт, спайк
«not for merge»):

- `NextTable::NULL = u32::MAX` (`src/alloc_core/segment/next_table.rs:36`), а
  дисциплина инициализации — virgin-skip (свежие страницы читаются нулём), при
  этом `init_in_place` есть только под `cfg(miri)` и делает `Node::zero` —
  **нули**, которые не равны sentinel'у (её же комментарий «every zero u32 is the
  NULL sentinel» неверен). Никогда не записанный слот читается как смещение 0,
  т.е. как указатель на заголовок сегмента.
- В `src/alloc_core/small/alloc_core_small/dealloc.rs` пуш вызывает
  `bm.mark_free(off)` **два раза** (строки 119 и 121) — дубль, добавленный
  патчем.

## Чего не измерено

- **Wall-clock и реальные столы CPU.** Callgrind — модель: `EstCycles =
  L1 + 5·L2 + 35·RAM` по правилам cost-модели самого callgrind. Ни Mops, ни
  латентность, ни реальные кэш-столы железа не измерялись.
- **Многопоточные сценарии** (larson/mstress, T = 1, 2, 4, 8), remote merge,
  ω-плечо «Remote lag», слова remote merge — всё это в ADR отдельными гейтами,
  здесь не запускалось.
- **RSS/commit и footprint метаданных.** +1 МиБ NextTable на сегмент — это
  арифметика из патча (`FOOTPRINT`), но не измерение; гейт «RSS ≤ 1.05 (или
  +1 MiB)» не проверялся.
- **`bench:table` geomean vs mimalloc / warm bulk ≤ 1.15** — не гонялся
  (в отчёте только iai-плечо и mimalloc-контрольные руки).
- **Протокол ADR «≥ 5 чередующихся прогонов в свежих процессах, медиана».** Для
  Ir одного прогона достаточно (детерминизм), для стендовых осей — нет; здесь
  1 прогон на плечо, контроль шума взят из mimalloc-рук (±0.5% EstCycles) и из
  A/A-артефакта (±0.05%).
- **Другие feature-наборы.** Только `production bench-internals internals`;
  поведение под `hardened` (где был снят валидатор `next` из `pop_free`) и под
  `alloc-segment-directory` не измерялись; путь `alloc_zeroed` под спайком
  feature-гейтингом не менялся и отдельно не проверялся.
- **Корректность** (клетки C1–C7, Miri SB/TB, Loom) — вне PG-3; полученные выше
  два подозрительных места (sentinel vs virgin-skip, двойной `mark_free`) не
  проверялись прогоном тестов.
- **Узкие вырожденные бенчи.** `carve_batch_only_*`,
  `alloc_zeroed_calloc_virgin_64k*`, `dealloc_batch_fresh_*` дают Ir ≈ 3 и не
  несут нагрузки: их «+0.000%» ничего не доказывает.
- **Функциональная атрибуция без строк.** `callgrind_annotate` даёт уровень
  функций: бенч собран без отладочной информации, строчные номера недоступны,
  поэтому «±12 Ir» между ручным прогоном и iai-логом разнесены по функциям
  только на уровне сумм.

## Поправка задним числом (2026-10-02, append-only)

Раздел «Механизм» выше объясняет регрессию ценой геометрии off-body записи. Это
не подтверждено: ненулевая ΔIr односегментных бенчей группируется у целых кратных
12 288 Ir (≈ 1024 слова pending-bitmap × 12 Ir) — спайк положил 1 MiB NextTable в
метаданные перед payload, `high_water` вырос на 1 MiB, и drain сканирует на 1024
слова больше (`sidecar_drain.rs`, `find_segment.rs`, `sidecar_bitmap.rs`). Остаток
после вычета скана: hot −0.003…+0.017%, cold/recycle −1.10…−0.38%. Подробности и
проверка — `docs/design/2026-10-02-adr-addendum-ph3c-escalation.md` §1.1; шаг 0
(PG-3r) перемеряет оба плеча с патчем S. Числа и CSV этого отчёта остаются как
измерено.
