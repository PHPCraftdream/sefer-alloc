# PG-3r (шаг 0, measurement-only): патч S — скан pending-bitmap с первого слова payload (iai A/B)

Дата: 2026-10-02. Шаг 0 (PG-3r) по
`docs/design/2026-10-02-adr-addendum-ph3c-escalation.md` §3: плечо PG-3 A/B
(base `7c0c796b45e352d86e58bd3f2ec993f3735030f6` против spike
`8a0286166a07c3ff35258836400d94f16c7994cd`) снято заново с **патчем S на ОБЕИХ
сторонах**, чтобы отделить цену скана pending-bitmap от цены геометрии off-body
NextTable. Спайк кладёт 1 MiB NextTable в метаданные перед payload (`high_water`
растёт на 1 MiB), из-за чего каждый проход drain шёл на 1024 слова дольше; патч S
убирает именно эти слова, поэтому в этом замере ΔIr — это остаток, а не цена.

Правило решения (только Ir, зафиксировано в §3 ДО просмотра чисел):

- к шагу 1, если hot ≤ 1.02, четыре refill-бенча ≤ 1.10, mimalloc-плечо
  ΔIr = 0.000% ровно и свежесть доказана;
- иначе сразу путь (б);
- гейты прошли, предсказание нет → атрибуция частичная, шаг 1 всё равно
  делается.

Инструмент: `scripts/pg3r_iai_table.mjs` (ESM, node, запуск из корня
pg3r-base). Читает ЧЕТЫРЕ лога: два S-лога этого дерева
(`docs/perf/_raw_pg3r_iai_{base,spike}.log`) и два исторических не-S лога
основного чекаута — по умолчанию `../../docs/perf/_raw_pg3_iai_{base,spike}.log`
(пути относительные, переопределяются флагами `--hist-base`/`--hist-spike`;
S-логи — флагами `--base`/`--spike`). Машиночитаемый результат:
`docs/perf/PG3R_SCAN_PATCH_IAI_summary.csv` (85 строк + заголовок).

## Идентичность

| Что | Значение |
|---|---|
| base HEAD | `7c0c796b45e352d86e58bd3f2ec993f3735030f6` (worktree `../pg3r-base`, дерево = патч S поверх этого коммита) |
| spike HEAD | `8a0286166a07c3ff35258836400d94f16c7994cd` (worktree `../pg3r-spike`, дерево = патч S поверх этого коммита) |
| патч S | `git diff | sha256sum` = `94a911665b705c19228689dc0484c618f41c1a7c8860e9aa4bc77ba146a122ee` в КАЖДОМ дереве — совпадают, патч идентичен |
| write-tree | base `c02f6fa1db817103d61512cade13ab016b6ec823`, spike `3d93a1ea9656f2bfe8a747ac2ad2362a1651a545` |
| features | `production bench-internals internals` (команда замера; преамбула `[iai] features:` в эти два захвата не попала — она есть в исторических PG-3 логах) |
| valgrind | 3.22.0 (callgrind-3.22.0, I1/D1 = 32 KiB, LL = 8 MiB) |
| rustc | 1.98.1 (48a229cea 2026-09-01) |
| WSL | Ubuntu-24.04 |
| судья | iai-callgrind-runner 0.14.2 |
| CARGO_TARGET_DIR | приватные: `/tmp/sefer-pg3r-base` и `/tmp/sefer-pg3r-spike` (разные — ловушка общего `/tmp/sefer-iai` исключена) |
| набор | 85 бенчей `perf_gate_iai::perf_gate::*` в каждом из четырёх логов (совпадение множеств — assert скрипта) |

## Свежесть

Подтверждения из логов и WSL (скрипт их не вычисляет — он их только печатает,
см. раздел «Свежесть» его вывода):

1. в base-логе ровно 1 строка
   `Compiling sefer-alloc v0.3.0 (/mnt/d/dev/rust/sefer-alloc/worktrees/pg3r-base)`;
2. в spike-логе ровно 1 строка
   `Compiling sefer-alloc v0.3.0 (/mnt/d/dev/rust/sefer-alloc/worktrees/pg3r-spike)`;
3. `.d` bench-бинарника содержит `# env-dep:CARGO_MANIFEST_DIR=` с
   соответствующим путём (`.../pg3r-base` и `.../pg3r-spike`) — бинарник каждой
   стороны собран из своего дерева;
4. `Running benches/perf_gate_iai.rs (/tmp/sefer-pg3r-base/release/deps/perf_gate_iai-1eae813e19149d63)`
   и `(/tmp/sefer-pg3r-spike/release/deps/perf_gate_iai-1eae813e19149d63)` — разные
   приватные таргет-диры;
5. 85 бенчей в каждом логе;
6. трейлер: `Iai-Callgrind result: Ok. 85 without regressions; 0 regressed; 85
   benchmarks finished` — время base 96.9599s, spike 94.4842s.

## Патч S

`start_word = payload_start / (MIN_BLOCK * 64)` — округление вниз; payload_start —
page-aligned `small_meta_end`/`primordial_meta_end`, то есть ТОЧНОЕ кратное одному
слову: 64 × MIN_BLOCK = 1 KiB payload на слово. Слова ниже `start_word` при скане
не читаются.

Применён в двух местах, семантика и порядок обхода не менялись:

1. `drain_segment_sidecar` (`src/alloc_core/small/alloc_core_small/find_segment.rs`):
   хелпер `AllocCore::sidecar_payload_start_word(base)` + вызов
   `table.scan_small_route_from(index, base, high_water, start_word)` вместо
   `scan_small_route`, где `start_word = sidecar_payload_start_word(base).min(end_word)`;
2. все сбросы word-курсора в `src/alloc_core/alloc_core/sidecar_drain.rs`:
   хелперы `sidecar_payload_start_word` + `sidecar_word_hint(index)`, clamp
   `if cursor.1 < start_word { cursor.1 = start_word; }` и `.min(end_word)` в drain.

## Корректность

Почему слова ниже `start_word` не могут нести pending-биты payload:

- публикуется только выданный блок: `SidecarBitmap::publish` →
  `publish_foreign` (`src/alloc_core/segment/remote_bitmap/routing.rs:56`), смещение
  считается как `ptr − segment_base`;
- carve/bump начинается с `payload_start`, поэтому смещение выданного блока всегда
  ≥ `payload_start`;
- `reclaim_sidecar_record`.abort'ится при `off < payload_start`.

Смоук на ОБЕИХ сторонах (17/17 зелёные):

| тест | зелёных | сторона |
|---|---:|---|
| `r6_terminal_owner_drain` | 8 | base и spike |
| `r8_owner_sidecar_miss` | 4 | base и spike |
| `r8_terminal_global` | 5 | base и spike |
| итого | 17 | обе |

Отдельно: команда задания `--features "production internals"` даёт **0 тестов**,
потому что все три теста cfg-gated на `bench-internals`; реальный прогон — с
`production bench-internals internals`.

## Предсказание (зафиксировано до замера)

Дословно смысл §3: ΔIr ≤ +0.5% на 4 hot-бенчах ADR и на
`cold_alloc_free_256x{16,64}b`, `recycle_alloc_free_256x{16,64}b`. Оно НЕ влияет на
вердикт гейтов: если гейты прошли, а предсказание нет — помечается «атрибуция
частичная», шаг 1 всё равно делается.

## Таблица — полный вывод `node scripts/pg3r_iai_table.mjs`

CSV (85 строк + заголовок) пишется тем же запуском.

```text
# PG-3r (patch S: pending-bitmap scan starts at the payload first word) — iai A/B

base:  `docs/perf/_raw_pg3r_iai_base.log` (worktree pg3r-base @ 7c0c796b45e352d86e58bd3f2ec993f3735030f6 + patch S)
spike: `docs/perf/_raw_pg3r_iai_spike.log` (worktree pg3r-spike @ 8a0286166a07c3ff35258836400d94f16c7994cd + patch S)
hist (PG-3, no patch S): `../../docs/perf/_raw_pg3_iai_base.log` (base), `../../docs/perf/_raw_pg3_iai_spike.log` (spike)
features: `production bench-internals internals` (log header: `production bench-internals internals`)  |  benches: 85

Every metric column is the FIRST number of the iai row (the current run); the
second half is the runner's own baseline (a count, `N/A`, or a percent note)
and is deliberately ignored.
ΔX% = (spike − base) / base × 100. The PATCH-S columns are a PATCH-S arm
against a PATCH-S arm (patch S is applied on BOTH sides). The «vs PG3»
columns compare against the previous PG-3 log of the SAME side: there the
DENOMINATOR is the historical (no-S) log of that side and the S-run is the
satellite, i.e. Δ = (Ir_S − Ir_hist) / Ir_hist × 100.
Every percent goes through a guarded printer that asserts
`Number.isFinite(pct)` and that `base*(1+pct/100)` reproduces spike to
±0.01% relative error.

| bench | Ir base_S | Ir spike_S | ΔIr % | ΔEstCycles % | Ir Δ vs PG3 base | Ir Δ vs PG3 spike | groups |
|---|---:|---:|---:|---:|---:|---:|---|
| small_churn_16b | 58299 | 58309 | +0.017% | -0.082% | -3.716% | -19.957% | hot-churn |
| small_churn_16b_2n | 64955 | 64965 | +0.015% | -0.147% | -3.365% | -18.286% | hot-churn |
| dealloc_prealloc_only_16b | 65080 | 65075 | -0.008% | -0.007% | -12.135% | -47.185% |  |
| dealloc_free_only_16b | 73698 | 73549 | -0.202% | -0.228% | -10.870% | -44.148% |  |
| dealloc_contains_base_probe_only_16b | 66439 | 66434 | -0.008% | +0.015% | -11.916% | -46.670% |  |
| dealloc_segment_base_of_ptr_probe_only_16b | 65670 | 65665 | -0.008% | +0.018% | -12.039% | -46.960% |  |
| alloc_magazine_prefill_only_16b | 57343 | 57329 | -0.024% | -0.048% | -3.776% | -20.229% | magazine |
| alloc_magazine_hit_only_16b | 57832 | 57830 | -0.003% | -0.007% | -3.745% | -20.089% | magazine |
| alloc_zeroed_magazine_prefill_only_16b | 57331 | 57329 | -0.003% | -0.031% | -3.776% | -20.229% | magazine |
| alloc_zeroed_magazine_hit_only_16b | 57881 | 57867 | -0.024% | -0.095% | -3.723% | -20.079% | magazine |
| dealloc_hash_contains_only_probe_16b | 67461 | 67456 | -0.007% | -0.028% | -11.757% | -46.290% |  |
| dealloc_own_thread_body_only_16b | 72729 | 72592 | -0.188% | -0.151% | -10.999% | -44.471% |  |
| dealloc_free_only_16b_n1 | 65149 | 65144 | -0.008% | -0.029% | -12.124% | -47.158% |  |
| dealloc_free_only_16b_n8 | 65608 | 65603 | -0.008% | +0.016% | -12.049% | -46.983% |  |
| dealloc_free_only_16b_n9 | 65673 | 65668 | -0.008% | -0.029% | -12.038% | -46.959% |  |
| dealloc_free_only_16b_n16 | 66128 | 66123 | -0.008% | +0.016% | -11.965% | -46.787% |  |
| dealloc_free_only_16b_n17 | 66938 | 66909 | -0.043% | +0.049% | -11.838% | -46.492% |  |
| dealloc_free_only_16b_n32 | 68654 | 68601 | -0.077% | -0.069% | -11.576% | -45.872% |  |
| dealloc_flush_class_only_16b_prefix | 52220 | 52218 | -0.004% | -0.032% | -4.131% | -21.778% | flush |
| dealloc_flush_class_only_16b | 52737 | 52699 | -0.072% | -0.061% | -4.092% | -21.636% | flush |
| alloc_clear_magazine_only_16b_prefix | 52736 | 52734 | -0.004% | +0.018% | -4.092% | -21.611% | magazine |
| alloc_clear_magazine_only_16b | 53121 | 53107 | -0.026% | -0.149% | -4.063% | -21.492% | magazine |
| alloc_zeroed_calloc_virgin_64k_prefix | 3 | 3 | +0.000% | +0.000% | +0.000% | +0.000% | refill |
| alloc_zeroed_calloc_virgin_64k | 3 | 3 | +0.000% | +0.000% | +0.000% | +0.000% | refill |
| alloc_zeroed_calloc_recycled_64k_prefix | 3 | 3 | +0.000% | +0.000% | +0.000% | +0.000% |  |
| alloc_zeroed_calloc_recycled_64k | 3 | 3 | +0.000% | +0.000% | +0.000% | +0.000% |  |
| dealloc_prealloc_only_1088_16b | 364623 | 364566 | -0.016% | -0.004% | -29.522% | -73.051% |  |
| dealloc_free_only_1088_16b_n17 | 366481 | 366400 | -0.022% | -0.006% | -29.416% | -72.952% |  |
| dealloc_free_only_1088_16b_n32 | 368197 | 368092 | -0.029% | -0.003% | -29.319% | -72.861% |  |
| dealloc_free_only_1088_16b_n64 | 373241 | 373040 | -0.054% | -0.038% | -29.038% | -72.597% |  |
| dealloc_free_only_1088_16b_n256 | 403505 | 402728 | -0.193% | -0.300% | -27.458% | -71.047% |  |
| dealloc_free_only_1088_16b_n1024 | 524561 | 521480 | -0.587% | -1.082% | -22.550% | -65.459% |  |
| dealloc_realloc_burst_1088_16b_n17 | 371425 | 371271 | -0.041% | -0.009% | -29.441% | -72.980% |  |
| oscillating_live_set_16b | 86742 | 86715 | -0.031% | -0.033% | -4.928% | -25.108% |  |
| carve_batch_only_16b | 3238 | 3238 | +0.000% | +1.302% | +0.000% | +0.000% | refill |
| carve_batch_only_16b_2n | 9139 | 9139 | +0.000% | +0.368% | +0.000% | +0.000% | refill |
| dealloc_batch_fresh_16_16b | 3 | 3 | +0.000% | +0.000% | +0.000% | +0.000% |  |
| dealloc_batch_fresh_64_16b | 3 | 3 | +0.000% | +0.000% | +0.000% | +0.000% |  |
| dealloc_batch_fresh_80_16b | 3 | 3 | +0.000% | +0.000% | +0.000% | +0.000% |  |
| dealloc_batch_fresh_81_16b | 3 | 3 | +0.000% | +0.000% | +0.000% | +0.000% |  |
| dealloc_batch_fresh_128_16b | 3 | 3 | +0.000% | +0.000% | +0.000% | +0.000% |  |
| dealloc_batch_fresh_200_16b | 3 | 3 | +0.000% | +0.000% | +0.000% | +0.000% |  |
| dealloc_batch_fresh_512_16b | 3 | 3 | +0.000% | +0.000% | +0.000% | +0.000% |  |
| dealloc_batch_fresh_1024_16b | 3 | 3 | +0.000% | +0.000% | +0.000% | +0.000% |  |
| dealloc_batch_fresh_0_16b | 3 | 3 | +0.000% | +0.000% | +0.000% | +0.000% |  |
| dealloc_batch_fresh_1_16b | 3 | 3 | +0.000% | +0.000% | +0.000% | +0.000% |  |
| dealloc_batch_fresh_8_16b | 3 | 3 | +0.000% | +0.000% | +0.000% | +0.000% |  |
| dealloc_batch_fresh_17_16b | 3 | 3 | +0.000% | +0.000% | +0.000% | +0.000% |  |
| medium_class_dealloc_churn_16b | 58311 | 58297 | -0.024% | -0.159% | -3.715% | -19.960% |  |
| aligned_churn_640b_a128 | 58305 | 58303 | -0.003% | -0.198% | -3.716% | -19.959% |  |
| large_alloc_free_cycle | 49912 | 49912 | +0.000% | -0.130% | +0.000% | +0.000% |  |
| large_cache_prefill_only_4mib | 59190 | 59178 | -0.020% | -0.065% | +0.000% | +0.000% |  |
| large_cache_hit_only_4mib | 60570 | 60570 | +0.000% | -0.003% | +0.000% | +0.000% |  |
| large_cache_free_slot_search_prefill_only | 64283 | 64283 | +0.000% | +0.022% | +0.000% | +0.000% |  |
| large_cache_free_slot_search_cycle_only | 76156 | 76156 | +0.000% | +0.000% | +0.000% | +0.000% |  |
| realloc_grow | 584906 | 585269 | +0.062% | +0.008% | -4.406% | -22.956% |  |
| cold_alloc_free_256x16b | 160447 | 159710 | -0.459% | -0.722% | -18.301% | -59.283% | cold |
| cold_alloc_free_256x16b_2n | 279225 | 277704 | -0.545% | -0.941% | -20.472% | -62.612% | cold |
| cold_alloc_free_256x16b_4n | 515221 | 512132 | -0.600% | -1.122% | -21.814% | -64.491% | cold |
| cold_alloc_only_256x16b | 120534 | 120517 | -0.014% | -0.062% | -22.969% | -65.864% | cold |
| cold_alloc_only_256x16b_2n | 197936 | 197903 | -0.017% | -0.072% | -26.639% | -70.149% | cold |
| cold_alloc_only_256x16b_4n | 351180 | 351115 | -0.019% | -0.022% | -29.044% | -72.596% | cold |
| cold_alloc_free_256x64b | 161302 | 160565 | -0.457% | -2.342% | -18.221% | -59.154% | cold |
| recycle_alloc_free_256x16b | 271220 | 267872 | -1.234% | -1.085% | -11.701% | -46.469% | recycle |
| recycle_alloc_only_256x16b | 230318 | 227690 | -1.141% | -1.085% | -13.498% | -50.526% | recycle |
| recycle_alloc_free_256x64b | 272075 | 268727 | -1.231% | -2.181% | -11.668% | -46.390% | recycle |
| churn_256b | 58299 | 58297 | -0.003% | -0.125% | -3.716% | -19.960% | hot-churn |
| churn_write_256b | 58491 | 58501 | +0.017% | -0.108% | -3.704% | -19.904% | hot-churn |
| multiseg_cold_256k | 4028057 | 4249669 | +5.502% | +5.544% | -4.101% | -30.060% | decommit |
| seg_cycle_decommit_256k | 12375007 | 13396119 | +8.251% | +8.378% | -3.764% | -28.810% | decommit |
| mimalloc_small_churn_16b | 16629 | 16629 | +0.000% | -0.331% | +0.000% | +0.000% |  |
| mimalloc_small_churn_16b_2n | 21557 | 21557 | +0.000% | -0.284% | +0.000% | +0.000% |  |
| mimalloc_churn_256b | 16130 | 16130 | +0.000% | -0.349% | +0.000% | +0.000% |  |
| mimalloc_cold_alloc_free_256x16b | 32325 | 32325 | +0.000% | -0.147% | +0.000% | +0.000% |  |
| mimalloc_cold_alloc_free_256x16b_2n | 58389 | 58389 | +0.000% | -0.217% | +0.000% | +0.000% |  |
| mimalloc_cold_alloc_free_256x16b_4n | 106653 | 106653 | +0.000% | -0.146% | +0.000% | +0.000% |  |
| mimalloc_cold_alloc_only_256x16b | 24584 | 24584 | +0.000% | -0.181% | +0.000% | +0.000% |  |
| mimalloc_cold_alloc_only_256x16b_2n | 42968 | 42968 | +0.000% | -0.327% | +0.000% | +0.000% |  |
| mimalloc_cold_alloc_only_256x16b_4n | 75872 | 75872 | +0.000% | -0.157% | +0.000% | +0.000% |  |
| mimalloc_cold_alloc_free_256x64b | 33329 | 33329 | +0.000% | -0.240% | +0.000% | +0.000% |  |
| mimalloc_recycle_alloc_free_256x16b | 53027 | 53027 | +0.000% | -0.131% | +0.000% | +0.000% |  |
| mimalloc_recycle_alloc_free_256x64b | 54031 | 54031 | +0.000% | -0.187% | +0.000% | +0.000% |  |
| mimalloc_bootstrap_proxy | 13050 | 13050 | +0.000% | -0.916% | +0.000% | +0.000% |  |
| decomp_full_cycle_8x | 680761 | 680761 | +0.000% | +0.004% | +0.000% | +0.000% |  |
| decomp_os_roundtrip_8x | 861 | 861 | +0.000% | +0.000% | +0.000% | +0.000% |  |

## Сводка против порогов (PG-3r, ADR-addendum §3 — только Ir)

Плечи: base = `docs/perf/_raw_pg3r_iai_base.log`, spike = `docs/perf/_raw_pg3r_iai_spike.log` (обе стороны с патч S).
Знаменатель каждого процента: ΔX% = (spike − base) / base × 100; порог-плечо —
отношение C/B в одном режиме, т.e. `1.00 + Δ/100`. Колонки «vs PG3» — против
прежнего PG-3 лога той же стороны (база = исторический лог стороны).

### Горячий набор ADR (4 бенча) — ΔIr ≤ +2% (x1.02)

  small_churn_16b            Ir base_S       58,299  Ir spike_S       58,309  ΔIr   +0.017% (x1.0002)  ->  PASS
  churn_256b                 Ir base_S       58,299  Ir spike_S       58,297  ΔIr   -0.003% (x1.0000)  ->  PASS
  churn_write_256b           Ir base_S       58,491  Ir spike_S       58,501  ΔIr   +0.017% (x1.0002)  ->  PASS
  aligned_churn_640b_a128    Ir base_S       58,305  Ir spike_S       58,303  ΔIr   -0.003% (x1.0000)  ->  PASS
  max ΔIr = +0.017% (x1.0002)  ->  PASS

### Refill-набор (4 бенча) — ΔIr ≤ +10% (x1.10)

  cold_alloc_free_256x16b    Ir base_S      160,447  Ir spike_S      159,710  ΔIr   -0.459% (x0.9954)  ->  PASS
  cold_alloc_free_256x64b    Ir base_S      161,302  Ir spike_S      160,565  ΔIr   -0.457% (x0.9954)  ->  PASS
  recycle_alloc_free_256x16b Ir base_S      271,220  Ir spike_S      267,872  ΔIr   -1.234% (x0.9877)  ->  PASS
  recycle_alloc_free_256x64b Ir base_S      272,075  Ir spike_S      268,727  ΔIr   -1.231% (x0.9877)  ->  PASS
  max ΔIr = -0.457% (x0.9954)  ->  PASS

### mimalloc-контроль A/A (13 бенчей, префикс `mimalloc`) — ΔIr должна быть ровно 0.000%

  mimalloc_small_churn_16b               Ir base_S    16,629  Ir spike_S    16,629  ΔIr +0.000%
  mimalloc_small_churn_16b_2n            Ir base_S    21,557  Ir spike_S    21,557  ΔIr +0.000%
  mimalloc_churn_256b                    Ir base_S    16,130  Ir spike_S    16,130  ΔIr +0.000%
  mimalloc_cold_alloc_free_256x16b       Ir base_S    32,325  Ir spike_S    32,325  ΔIr +0.000%
  mimalloc_cold_alloc_free_256x16b_2n    Ir base_S    58,389  Ir spike_S    58,389  ΔIr +0.000%
  mimalloc_cold_alloc_free_256x16b_4n    Ir base_S   106,653  Ir spike_S   106,653  ΔIr +0.000%
  mimalloc_cold_alloc_only_256x16b       Ir base_S    24,584  Ir spike_S    24,584  ΔIr +0.000%
  mimalloc_cold_alloc_only_256x16b_2n    Ir base_S    42,968  Ir spike_S    42,968  ΔIr +0.000%
  mimalloc_cold_alloc_only_256x16b_4n    Ir base_S    75,872  Ir spike_S    75,872  ΔIr +0.000%
  mimalloc_cold_alloc_free_256x64b       Ir base_S    33,329  Ir spike_S    33,329  ΔIr +0.000%
  mimalloc_recycle_alloc_free_256x16b    Ir base_S    53,027  Ir spike_S    53,027  ΔIr +0.000%
  mimalloc_recycle_alloc_free_256x64b    Ir base_S    54,031  Ir spike_S    54,031  ΔIr +0.000%
  mimalloc_bootstrap_proxy               Ir base_S    13,050  Ir spike_S    13,050  ΔIr +0.000%
  max |ΔIr| = +0.000%  ->  A/A OK

### Только в отчёт (без вердикта — ADR-addendum §3)

  multiseg_cold_256k         Ir base_S    4,028,057  Ir spike_S    4,249,669  ΔIr +5.502% (x1.0550)  ΔEstCycles +5.544%

### Предсказание (зафиксировано до замера): ΔIr на 4 hot + 4 refill ≤ +0.5%
  max ΔIr = +0.017% (x1.0002)  ->  выполнено (на вердикт гейтов не влияет)

## Итоговый вердикт

  hot (4)   все ΔIr ≤ +2%  -> PASS
  refill (4) все ΔIr ≤ +10% -> PASS
  mimalloc A/A ΔIr = 0.000% ровно -> PASS
  ВЕРДИКТ: PASS

## Разложение остатка (сколько ΔIr снял патч S против прежнего PG-3 ΔIr)

Одно слово pending-bitmap покрывает 1 KiB payload; лишнее слово скана ≈ 12 Ir.
1 MiB NextTable = +1024 слова на проход = 12,288 Ir на проход.
ΔIr_pg3   = (Ir_spike_pg3 − Ir_base_pg3) / Ir_base_pg3 × 100   (исторический лог PG-3, без S)
ΔIr_pg3r  = (Ir_spike_S − Ir_base_S) / Ir_base_S × 100        (этот замер, патч S на обеих сторонах)
снято_Iр = ΔIr_pg3(in Ir) − ΔIr_pg3r(in Ir);  k = round(снято_Iр / 12288).

| bench | ΔIr_pg3 % | ΔIr_pg3r % | снято Ir | k | k·12288 | остаток Ir | |снято| / k·12288 |
|---|---:|---:|---:|---:|---:|---:|---:|
| small_churn_16b | +20.311% | +0.017% | 12,288 | 1 | 12,288 | 0 | 100.00% |
| churn_256b | +20.291% | -0.003% | 12,288 | 1 | 12,288 | 0 | 100.00% |
| churn_write_256b | +20.247% | +0.017% | 12,288 | 1 | 12,288 | 0 | 100.00% |
| aligned_churn_640b_a128 | +20.289% | -0.003% | 12,288 | 1 | 12,288 | 0 | 100.00% |
| cold_alloc_free_256x16b | +99.730% | -0.459% | 196,593 | 16 | 196,608 | -15 | 99.99% |
| cold_alloc_free_256x64b | +99.297% | -0.457% | 196,593 | 16 | 196,608 | -15 | 99.99% |
| recycle_alloc_free_256x16b | +62.913% | -1.234% | 196,593 | 16 | 196,608 | -15 | 99.99% |
| recycle_alloc_free_256x64b | +62.739% | -1.231% | 196,593 | 16 | 196,608 | -15 | 99.99% |
| multiseg_cold_256k | +44.660% | +5.502% | 1,654,243 | 135 | 1,658,880 | -4,637 | 99.72% |

Чтение: «снято Ir» — сколько инструкций патч S убрал относительно прежнего PG-3 ΔIr;
если механизм ADR-аддендума §1.1 верен, оно садится на целые k·12288 (остаток —
прочими незначительными различиями, доля печатается в последней колонке).

## Свежесть (свидетельства из логов и WSL — скрипт их не вычисляет)

Ниже — константы-подтверждения, зафиксированные при снятии логов. Этот скрипт
не имеет доступа к WSL и к `/tmp`, поэтому он их только печатает.

1. base-лог: ровно 1 строка `Compiling sefer-alloc v0.3.0 (/mnt/d/dev/rust/sefer-alloc/worktrees/pg3r-base)`.
2. spike-лог: ровно 1 строка `Compiling sefer-alloc v0.3.0 (/mnt/d/dev/rust/sefer-alloc/worktrees/pg3r-spike)`.
3. `.d` bench-бинарника: `# env-dep:CARGO_MANIFEST_DIR=/mnt/d/dev/rust/sefer-alloc/worktrees/pg3r-base` (base) и
   `# env-dep:CARGO_MANIFEST_DIR=/mnt/d/dev/rust/sefer-alloc/worktrees/pg3r-spike` (spike).
4. Приватные CARGO_TARGET_DIR: `/tmp/sefer-pg3r-base` и `/tmp/sefer-pg3r-spike`
   (разные — ловушка общего `/tmp/sefer-iai` исключена).
5. Бенчей в каждом логе: 85 (совпадение множеств — assert выше).
6. Трейлер: `Iai-Callgrind result: Ok. 85 without regressions; 0 regressed; 85 benchmarks finished`
   (время: base 96.9599s, spike 94.4842s).
7. valgrind 3.22.0, rustc 1.98.1, WSL Ubuntu-24.04, iai-callgrind-runner 0.14.2.
8. `git diff | sha256sum` = 94a911665b705c19228689dc0484c618f41c1a7c8860e9aa4bc77ba146a122ee в КАЖДОМ дереве (патч идентичен);
   `git write-tree`: base c02f6fa1db817103d61512cade13ab016b6ec823, spike 3d93a1ea9656f2bfe8a747ac2ad2362a1651a545.
9. Формат строки метрик: base-лог `N|N (No change)` (таргет-дир сохранил baseline
   первого прогона), spike-лог `N|N/A`. Берётся ПЕРВОЕ число — baseline второй
   половины не является прогоном другого ворктрида.

CSV записан: docs/perf/PG3R_SCAN_PATCH_IAI_summary.csv
  заголовок: bench,ir_base_s,ir_spike_s,cycles_base_s,cycles_spike_s,ir_pg3_base,ir_pg3_spike,ir_hist_base,ir_hist_spike,features
  строк: 85
```

Знаменатели колонок: «ΔIr %»/«ΔEstCycles %» — плечо PATCH-S против плеча PATCH-S
(ΔX% = (spike − base) / base × 100); «Ir Δ vs PG3 base»/«Ir Δ vs PG3 spike» — против
прежнего (не-S) лога PG-3 той же стороны, там база — исторический лог стороны, а
спутник — S-прогон.

## Сводка против порогов и вердикт

Все числа — из вывода скрипта выше. Порог-плечо = 1.00 + Δ/100.

### Горячий набор ADR (ΔIr ≤ +2%, x1.02)

| бенч | Ir base_S | Ir spike_S | ΔIr % | x | порог | вердикт |
|---|---:|---:|---:|---:|---:|---|
| `small_churn_16b` | 58 299 | 58 309 | +0.017% | x1.0002 | x1.02 | PASS |
| `churn_256b` | 58 299 | 58 297 | −0.003% | x1.0000 | x1.02 | PASS |
| `churn_write_256b` | 58 491 | 58 501 | +0.017% | x1.0002 | x1.02 | PASS |
| `aligned_churn_640b_a128` | 58 305 | 58 303 | −0.003% | x1.0000 | x1.02 | PASS |

max ΔIr = +0.017% (x1.0002) → **PASS**.

### Refill-набор (ΔIr ≤ +10%, x1.10)

| бенч | Ir base_S | Ir spike_S | ΔIr % | x | порог | вердикт |
|---|---:|---:|---:|---:|---:|---|
| `cold_alloc_free_256x16b` | 160 447 | 159 710 | −0.459% | x0.9954 | x1.10 | PASS |
| `cold_alloc_free_256x64b` | 161 302 | 160 565 | −0.457% | x0.9954 | x1.10 | PASS |
| `recycle_alloc_free_256x16b` | 271 220 | 267 872 | −1.234% | x0.9877 | x1.10 | PASS |
| `recycle_alloc_free_256x64b` | 272 075 | 268 727 | −1.231% | x0.9877 | x1.10 | PASS |

max ΔIr = −0.457% (x0.9954) → **PASS**.

### mimalloc-контроль A/A

Внутренний A/A: 13 бенчей с префиксом `mimalloc` не используют код SeferAlloc вовсе.
ΔIr = ровно +0.000% у всех 13, max |ΔIr| = +0.000% → **A/A OK**. По тем же 13
бенчам ΔEstCycles лежит в полосе от −0.131% до −0.916% (max |Δ| = 0.916% у
`mimalloc_bootstrap_proxy`): ось EstCycles на этой машине шумит в пределах ~1%, а
ось Ir — нет, поэтому все пороги и итоговый вердикт берутся только по Ir.

### Только в отчёт (без вердикта)

`multiseg_cold_256k`: Ir base_S 4 028 057 → Ir spike_S 4 249 669, ΔIr +5.502%
(x1.0550), ΔEstCycles +5.544%. По §3 ADR-аддендума `multiseg_cold_256k` — только
в отчёт.

### Предсказание

max ΔIr по 4 hot + 4 refill = +0.017% (x1.0002, `small_churn_16b`) ≤ +0.5% →
**предсказание выполнено**.

### Итоговый вердикт

| Условие | Результат |
|---|---|
| hot (4) все ΔIr ≤ +2% | PASS |
| refill (4) все ΔIr ≤ +10% | PASS |
| mimalloc A/A ΔIr = 0.000% ровно | PASS (13 бенчей, +0.000% каждый) |
| свежесть доказана | да (раздел «Свежесть») |
| **ВЕРДИКТ** | **PASS** |

Гейты пройдены и предсказание выполнено → атрибуция полная, переходим к шагу 1
(путь (а), интегрированный спайк B3).

## Разложение остатка

«Снято Ir» = ΔIr_pg3 (в Ir) − ΔIr_pg3r (в Ir), где ΔIr_pg3 — прежний не-S замер PG-3
(spike vs base исторических логов), а ΔIr_pg3r — этот замер. Одно слово
pending-bitmap покрывает 1 KiB payload, лишнее слово скана ≈ 12 Ir, поэтому +1024
слова (1 MiB NextTable) = 12 288 Ir на проход; k = round(снято / 12288).

| бенч | ΔIr_pg3 % | ΔIr_pg3r % | снято Ir | k | k·12288 | остаток Ir |
|---|---:|---:|---:|---:|---:|---:|
| `small_churn_16b` | +20.311% | +0.017% | 12 288 | 1 | 12 288 | 0 |
| `churn_256b` | +20.291% | −0.003% | 12 288 | 1 | 12 288 | 0 |
| `churn_write_256b` | +20.247% | +0.017% | 12 288 | 1 | 12 288 | 0 |
| `aligned_churn_640b_a128` | +20.289% | −0.003% | 12 288 | 1 | 12 288 | 0 |
| `cold_alloc_free_256x16b` | +99.730% | −0.459% | 196 593 | 16 | 196 608 | −15 |
| `cold_alloc_free_256x64b` | +99.297% | −0.457% | 196 593 | 16 | 196 608 | −15 |
| `recycle_alloc_free_256x16b` | +62.913% | −1.234% | 196 593 | 16 | 196 608 | −15 |
| `recycle_alloc_free_256x64b` | +62.739% | −1.231% | 196 593 | 16 | 196 608 | −15 |
| `multiseg_cold_256k` | +44.660% | +5.502% | 1 654 243 | 135 | 1 658 880 | −4 637 |

Читается так: патч S снял ровно те k·12288 Ir, которые предсказывал §1.1
аддендума — на горячих бенчах это один проход (k = 1, остаток 0 Ir), на cold/recycle
16 проходов (k = 16, остаток −15 Ir ≈ −0.008%), на `multiseg_cold_256k` 135 проходов
(остаток −4 637 Ir ≈ −0.28%). По 85 бенчам колонки «Ir Δ vs PG3 base/spike»
показывают, что сторона spike после S дешевле прежнего spike-прогона на 0…73.05%
(0% там, где drain не вызывается, −73.05% на `dealloc_prealloc_only_1088_16b`), а
сторона base — на 0…29.52% (макс. модуль — там же). То есть регрессия PG-3 была
артефактом скана, а не ценой off-body учёта, и целиком снимается патчем S.

## Что не измерено / Notes

- Правило решения — только Ir; EstCycles печатается в отчёт (§3 ADR-аддендума
  разрешал), в порогах не участвует.
- Перемер без S не проводился: историческое плечо взято из логов PG-3 основного
  чекаута (`docs/perf/_raw_pg3_iai_base.log`, `docs/perf/_raw_pg3_iai_spike.log`),
  которые остались без изменений.
- base_S мерялся дважды: первый захват не удался из-за Windows-пути в `tee` внутри
  WSL. Лог — второй прогон; Ir детерминирован, значения совпали с первым прогоном.
- В base-логе формат строки `N|N (No change)`, потому что таргет-дир
  `/tmp/sefer-pg3r-base` сохранил baseline первого прогона — это нормально, скрипт
  берёт ПЕРВОЕ число каждой строки. В spike-логе `N|N/A` — свежий baseline, сравнивать
  было не с чем.
- В base-логе короткая преамбула cargo (6 строк `Compiling`): остальные крейты уже
  были собраны в том же приватном таргет-дире первым прогоном; строка
  `Compiling sefer-alloc` присутствует ровно одна и указывает на pg3r-base.
- Строгость: парсер и защищённый принтер процентов те же, что в
  `scripts/pg3_iai_table.mjs` (совпадение множеств 85 бенчей между всеми логами и
  round-trip `base*(1+p/100)` → spike с точностью 1e-4 проверяются assert'ом), поэтому
  руками вписанное или подправленное число таблицу не проходит.

## Артефакт патча

Патч S сохранён как `docs/perf/PG3R_SCAN_PATCH_S.patch` (байты = вывод `git diff` в обоих деревьях, sha256 выше). В `main` патч НЕ применён: это измерительный артефакт; отдельное решение о `perf(runtime)` — perf item 80 (`docs/perf/OPEN_ITEMS.md`).
