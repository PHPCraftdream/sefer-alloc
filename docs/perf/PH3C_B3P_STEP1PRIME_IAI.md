# Ph3c шаг 1′ — iai A/B исправленного B3 (per-class carve): ВЕРДИКТ FAIL → путь (б)

Дата: 2026-10-05. Решающий замер шага 1′ (замена шага 1 по `docs/design/2026-10-02-adr-addendum-ph3c-step1prime.md`
§2). Пределы пред-регистрированы (addendum escalation §4.1, step1prime §2.5), правка одна (§3), без подбора
параметров. **Measurement-only для `main`: shipping-код не менялся; B3 живёт только в ветке/патче.**

## Вердикт

**FAIL по hot И по refill/flush** (A/A чист: mimalloc ΔIr = 0.000% на 13/13 плеч; детерминизм побитовый).
По step1prime §5 любой красный гейт сверх шума A/A → **путь (б)** (escalation §5): интрузивный slab остаётся,
P1 — принятый известный дефект P1-box, Ph3c отменяется.

| гейт | предел | измерено (худший бенч) | вердикт |
|---|---|---|---|
| hot (`small_churn_16b`, `churn_256b`, `churn_write_256b`, `aligned_churn_640b_a128`) Ir | ≤ 1.02 | x1.03639 (+3.639%, `aligned_churn_640b_a128`) | FAIL |
| hot EstCycles | ≤ 1.02 | x1.02517 (+2.517%, `churn_256b`) | FAIL |
| refill/flush Ir (`cold_alloc_free_256x16b`) | ≤ 1.20 | x1.20902 (+20.902%) | FAIL |
| refill/flush EstCycles (`cold_alloc_free_256x16b`) | ≤ 1.10 | x1.13038 (+13.038%) | FAIL |
| `multiseg_cold_256k` | Ir ≤ 1.20, Cyc ≤ 1.10 | x1.00053 / x1.00134 | PASS |

Каждый процент — знаменатель = значение базы (main c584a3ae + патч S) того же бенча (полная таблица ниже, строится скриптом).
Фрагментация W1–W3 на этом дереве НЕ мерилась: perf-гейты уже красные (решающий критерий (б) сработал), замер моот.
Диагностика дефекта карвинга на предыдущем дереве — `docs/perf/PH3C_FRAG_STAND.md` (W1 C/B 4.013).

## Идентичность (до замера)

| Что | Значение |
|---|---|
| ветка / коммит | `ph3c-b3p` / `fd3cdb9ed654d0cd1d6524cfc11d4c0f97035517` |
| tree | `32d0fe0e98931c28be7c6cfb102626f8dca3478c` |
| база A | main `c584a3ae` + патч S (`docs/perf/PG3R_SCAN_PATCH_S.patch`, sha256 `94a911665b705c19228689dc0484c618f41c1a7c8860e9aa4bc77ba146a122ee`) |
| `git diff 093c9d23 | sha256sum` (правка шага 1′ над спайком) | `e41f0da82370098a4cb117d3e17126ad90af7175b75d7a1b075e0727bd535d0d` |
| патч src+tests B3′ над main (для воспроизведения) | `docs/perf/PH3C_B3P_SRC.patch` |
| features | `production bench-internals internals`, WSL, callgrind 3.22.0, iai-callgrind 0.14.2 |
| команда | `cargo bench --bench perf_gate_iai --features "production bench-internals internals"` (login-оболочка WSL, приватный `CARGO_TARGET_DIR` `/tmp/sefer-b3p`) |
| свежесть | ровно одна строка `Compiling sefer-alloc` с путём этого дерева в `_raw_ph3c_b3p_iai_c_run1.log` |
| прогоны | C′: 2 (`_raw_ph3c_b3p_iai_c_run{1,2}.log`), Ir побитово совпадает на 85/85; база: 3 (`_raw_ph3c_b3s_iai_base_run{1,2,3}.log`, снята на шаге 1 на том же main+S, детерминизм побитовый) |
| судья | `scripts/ph3c_b3p_iai_table.mjs` (копия `ph3c_b3s_iai_table.mjs` с иными путями вывода) |

## Таблицы (дословный вывод судьи; round-trip assert на каждый процент)

# Ph3c step 1 — шаг 1′ (ph3c-b3p: B3 + per-class carve), iai A/B (callgrind, детерминирован)

base:  `docs/perf/_raw_ph3c_b3s_iai_base_run1.log` (main c584a3ae + patch S, sha256 94a911665b705c19228689dc0484c618f41c1a7c8860e9aa4bc77ba146a122ee)
b3:    `docs/perf/_raw_ph3c_b3p_iai_c_run1.log` (main c584a3ae + patch S + B3)
base2:   детерминизм Ir базы — побитово равен прогону 1 (все 85 бенчей)
base3:   детерминизм Ir базы — побитово равен прогону 1 (все 85 бенчей)
b32:   детерминизм Ir B3 — побитово равен прогону 1 (все 85 бенчей)
features: `production bench-internals internals` | бенчей: 85

Пределы (эскалация §4.1, пред-регистрированы): hot Ir/EstCycles ≤ 1.02; refill/flush EstCycles ≤ 1.10 И Ir ≤ 1.20.
ΔX% = (b3 − base) / base × 100; каждый процент черезguarded-принтер с round-trip assert ±0.01%.

## Контроль A/A (mimalloc-плечи, код спайка не исполняется)

- mimalloc_small_churn_16b: ΔIr +0.000% OK (0.000%)
- mimalloc_small_churn_16b_2n: ΔIr +0.000% OK (0.000%)
- mimalloc_churn_256b: ΔIr +0.000% OK (0.000%)
- mimalloc_cold_alloc_free_256x16b: ΔIr +0.000% OK (0.000%)
- mimalloc_cold_alloc_free_256x16b_2n: ΔIr +0.000% OK (0.000%)
- mimalloc_cold_alloc_free_256x16b_4n: ΔIr +0.000% OK (0.000%)
- mimalloc_cold_alloc_only_256x16b: ΔIr +0.000% OK (0.000%)
- mimalloc_cold_alloc_only_256x16b_2n: ΔIr +0.000% OK (0.000%)
- mimalloc_cold_alloc_only_256x16b_4n: ΔIr +0.000% OK (0.000%)
- mimalloc_cold_alloc_free_256x64b: ΔIr +0.000% OK (0.000%)
- mimalloc_recycle_alloc_free_256x16b: ΔIr +0.000% OK (0.000%)
- mimalloc_recycle_alloc_free_256x64b: ΔIr +0.000% OK (0.000%)
- mimalloc_bootstrap_proxy: ΔIr +0.000% OK (0.000%)

A/A Ir: все 0.000% ровно — OK; max |ΔEstCycles| = 0.206%.

## Гейты ADR

- small_churn_16b: Ir 58160→60204 (x1.03514, +3.514%) | EstCycles 143902→147460 (x1.02473) | пределы Ir≤1.02 Cyc≤1.02 → FAIL
- churn_256b: Ir 58160→60204 (x1.03514, +3.514%) | EstCycles 143902→147524 (x1.02517) | пределы Ir≤1.02 Cyc≤1.02 → FAIL
- churn_write_256b: Ir 58352→60396 (x1.03503, +3.503%) | EstCycles 144358→147840 (x1.02412) | пределы Ir≤1.02 Cyc≤1.02 → FAIL
- aligned_churn_640b_a128: Ir 58142→60258 (x1.03639, +3.639%) | EstCycles 144036→147557 (x1.02445) | пределы Ir≤1.02 Cyc≤1.02 → FAIL
- cold_alloc_free_256x16b: Ir 157925→190934 (x1.20902, +20.902%) | EstCycles 279247→315655 (x1.13038) | пределы Ir≤1.2 Cyc≤1.1 → FAIL
- cold_alloc_free_256x64b: Ir 158828→191942 (x1.20849, +20.849%) | EstCycles 286573→317184 (x1.10682) | пределы Ir≤1.2 Cyc≤1.1 → FAIL
- recycle_alloc_free_256x16b: Ir 265223→310466 (x1.17058, +17.058%) | EstCycles 418752→471868 (x1.12684) | пределы Ir≤1.2 Cyc≤1.1 → FAIL
- recycle_alloc_free_256x64b: Ir 266126→311513 (x1.17055, +17.055%) | EstCycles 426554→473512 (x1.11009) | пределы Ir≤1.2 Cyc≤1.1 → FAIL
- multiseg_cold_256k: Ir 4027836→4029989 (x1.00053, +0.053%) | EstCycles 5684025→5691639 (x1.00134) | пределы Ir≤1.2 Cyc≤1.1 → PASS

hot: FAIL | refill/flush: FAIL | A/A: PASS

## Все 85 бенчей (ΔIr, ΔCycles)

- small_churn_16b: Ir 58160→60204 (+3.514%, x1.03514) | Cyc 143902→147460 (+2.473%, x1.02473)
- small_churn_16b_2n: Ir 64944→66976 (+3.129%, x1.03129) | Cyc 152610→155978 (+2.207%, x1.02207)
- dealloc_prealloc_only_16b: Ir 64111→72290 (+12.758%, x1.12758) | Cyc 151410→161529 (+6.683%, x1.06683)
- dealloc_free_only_16b: Ir 73039→81367 (+11.402%, x1.11402) | Cyc 164462→174621 (+6.177%, x1.06177)
- dealloc_contains_base_probe_only_16b: Ir 65470→73649 (+12.493%, x1.12493) | Cyc 153299→163528 (+6.673%, x1.06673)
- dealloc_segment_base_of_ptr_probe_only_16b: Ir 64701→72880 (+12.641%, x1.12641) | Cyc 152166→162395 (+6.722%, x1.06722)
- alloc_magazine_prefill_only_16b: Ir 57370→59414 (+3.563%, x1.03563) | Cyc 142381→145667 (+2.308%, x1.02308)
- alloc_magazine_hit_only_16b: Ir 57864→59903 (+3.524%, x1.03524) | Cyc 143098→146447 (+2.340%, x1.02340)
- alloc_zeroed_magazine_prefill_only_16b: Ir 57382→59426 (+3.562%, x1.03562) | Cyc 142371→145763 (+2.383%, x1.02383)
- alloc_zeroed_magazine_hit_only_16b: Ir 57928→59964 (+3.515%, x1.03515) | Cyc 143280→146642 (+2.346%, x1.02346)
- dealloc_hash_contains_only_probe_16b: Ir 66492→74671 (+12.301%, x1.12301) | Cyc 154349→164578 (+6.627%, x1.06627)
- dealloc_own_thread_body_only_16b: Ir 71841→80089 (+11.481%, x1.11481) | Cyc 162901→172864 (+6.116%, x1.06116)
- dealloc_free_only_16b_n1: Ir 64194→72372 (+12.740%, x1.12740) | Cyc 151870→162060 (+6.710%, x1.06710)
- dealloc_free_only_16b_n8: Ir 64681→72869 (+12.659%, x1.12659) | Cyc 152588→162768 (+6.672%, x1.06672)
- dealloc_free_only_16b_n9: Ir 64751→72939 (+12.645%, x1.12645) | Cyc 152645→162859 (+6.691%, x1.06691)
- dealloc_free_only_16b_n16: Ir 65241→73429 (+12.550%, x1.12550) | Cyc 153320→163496 (+6.637%, x1.06637)
- dealloc_free_only_16b_n17: Ir 66054→74292 (+12.472%, x1.12472) | Cyc 155148→165409 (+6.614%, x1.06614)
- dealloc_free_only_16b_n32: Ir 67843→76099 (+12.169%, x1.12169) | Cyc 157466→167761 (+6.538%, x1.06538)
- dealloc_flush_class_only_16b_prefix: Ir 51994→54022 (+3.900%, x1.03900) | Cyc 135609→138827 (+2.373%, x1.02373)
- dealloc_flush_class_only_16b: Ir 52487→54588 (+4.003%, x1.04003) | Cyc 136842→140204 (+2.457%, x1.02457)
- alloc_clear_magazine_only_16b_prefix: Ir 52551→54580 (+3.861%, x1.03861) | Cyc 136160→139443 (+2.411%, x1.02411)
- alloc_clear_magazine_only_16b: Ir 52912→54965 (+3.880%, x1.03880) | Cyc 136676→140053 (+2.471%, x1.02471)
- alloc_zeroed_calloc_virgin_64k_prefix: Ir 3→3 (+0.000%, x1.00000) | Cyc 39→39 (+0.000%, x1.00000)
- alloc_zeroed_calloc_virgin_64k: Ir 3→3 (+0.000%, x1.00000) | Cyc 39→39 (+0.000%, x1.00000)
- alloc_zeroed_calloc_recycled_64k_prefix: Ir 3→3 (+0.000%, x1.00000) | Cyc 39→39 (+0.000%, x1.00000)
- alloc_zeroed_calloc_recycled_64k: Ir 3→3 (+0.000%, x1.00000) | Cyc 39→39 (+0.000%, x1.00000)
- dealloc_prealloc_only_1088_16b: Ir 348998→487045 (+39.555%, x1.39555) | Cyc 540055→694758 (+28.646%, x1.28646)
- dealloc_free_only_1088_16b_n17: Ir 350940→489047 (+39.353%, x1.39353) | Cyc 543728→698638 (+28.490%, x1.28490)
- dealloc_free_only_1088_16b_n32: Ir 352729→490854 (+39.159%, x1.39159) | Cyc 546114→700990 (+28.360%, x1.28360)
- dealloc_free_only_1088_16b_n64: Ir 357925→496122 (+38.611%, x1.38611) | Cyc 553110→707922 (+27.989%, x1.27989)
- dealloc_free_only_1088_16b_n256: Ir 389101→527730 (+35.628%, x1.35628) | Cyc 595184→749014 (+25.846%, x1.25846)
- dealloc_free_only_1088_16b_n1024: Ir 513805→654162 (+27.317%, x1.27317) | Cyc 763138→913790 (+19.741%, x1.19741)
- dealloc_realloc_burst_1088_16b_n17: Ir 355645→495560 (+39.341%, x1.39341) | Cyc 550692→708109 (+28.585%, x1.28585)
- oscillating_live_set_16b: Ir 87837→91955 (+4.688%, x1.04688) | Cyc 182520→188251 (+3.140%, x1.03140)
- carve_batch_only_16b: Ir 3241→34141 (+953.409%, x10.53409) | Cyc 8118→42075 (+418.293%, x5.18293)
- carve_batch_only_16b_2n: Ir 9142→70924 (+675.804%, x7.75804) | Cyc 18428→85679 (+364.939%, x4.64939)
- dealloc_batch_fresh_16_16b: Ir 3→3 (+0.000%, x1.00000) | Cyc 39→39 (+0.000%, x1.00000)
- dealloc_batch_fresh_64_16b: Ir 3→3 (+0.000%, x1.00000) | Cyc 39→39 (+0.000%, x1.00000)
- dealloc_batch_fresh_80_16b: Ir 3→3 (+0.000%, x1.00000) | Cyc 39→39 (+0.000%, x1.00000)
- dealloc_batch_fresh_81_16b: Ir 3→3 (+0.000%, x1.00000) | Cyc 39→39 (+0.000%, x1.00000)
- dealloc_batch_fresh_128_16b: Ir 3→3 (+0.000%, x1.00000) | Cyc 39→39 (+0.000%, x1.00000)
- dealloc_batch_fresh_200_16b: Ir 3→3 (+0.000%, x1.00000) | Cyc 39→39 (+0.000%, x1.00000)
- dealloc_batch_fresh_512_16b: Ir 3→3 (+0.000%, x1.00000) | Cyc 39→39 (+0.000%, x1.00000)
- dealloc_batch_fresh_1024_16b: Ir 3→3 (+0.000%, x1.00000) | Cyc 39→39 (+0.000%, x1.00000)
- dealloc_batch_fresh_0_16b: Ir 3→3 (+0.000%, x1.00000) | Cyc 39→39 (+0.000%, x1.00000)
- dealloc_batch_fresh_1_16b: Ir 3→3 (+0.000%, x1.00000) | Cyc 39→39 (+0.000%, x1.00000)
- dealloc_batch_fresh_8_16b: Ir 3→3 (+0.000%, x1.00000) | Cyc 39→39 (+0.000%, x1.00000)
- dealloc_batch_fresh_17_16b: Ir 3→3 (+0.000%, x1.00000) | Cyc 39→39 (+0.000%, x1.00000)
- medium_class_dealloc_churn_16b: Ir 58148→60192 (+3.515%, x1.03515) | Cyc 143900→147342 (+2.392%, x1.02392)
- aligned_churn_640b_a128: Ir 58142→60258 (+3.639%, x1.03639) | Cyc 144036→147557 (+2.445%, x1.02445)
- large_alloc_free_cycle: Ir 49765→49762 (-0.006%, x0.99994) | Cyc 133337→133304 (-0.025%, x0.99975)
- large_cache_prefill_only_4mib: Ir 58763→58763 (+0.000%, x1.00000) | Cyc 146172→145912 (-0.178%, x0.99822)
- large_cache_hit_only_4mib: Ir 60103→60103 (+0.000%, x1.00000) | Cyc 148674→148414 (-0.175%, x0.99825)
- large_cache_free_slot_search_prefill_only: Ir 63562→63562 (+0.000%, x1.00000) | Cyc 156660→156494 (-0.106%, x0.99894)
- large_cache_free_slot_search_cycle_only: Ir 75115→75115 (+0.000%, x1.00000) | Cyc 173870→173738 (-0.076%, x0.99924)
- realloc_grow: Ir 582996→599968 (+2.911%, x1.02911) | Cyc 3700857→3723021 (+0.599%, x1.00599)
- cold_alloc_free_256x16b: Ir 157925→190934 (+20.902%, x1.20902) | Cyc 279247→315655 (+13.038%, x1.13038)
- cold_alloc_free_256x16b_2n: Ir 274255→340275 (+24.072%, x1.24072) | Cyc 438151→509622 (+16.312%, x1.16312)
- cold_alloc_free_256x16b_4n: Ir 505355→637397 (+26.129%, x1.26129) | Cyc 751490→892991 (+18.829%, x1.18829)
- cold_alloc_only_256x16b: Ir 116805→149272 (+27.796%, x1.27796) | Cyc 222843→260186 (+16.758%, x1.16758)
- cold_alloc_only_256x16b_2n: Ir 190543→255477 (+34.078%, x1.34078) | Cyc 324601→398119 (+22.649%, x1.22649)
- cold_alloc_only_256x16b_4n: Ir 336459→466327 (+38.598%, x1.38598) | Cyc 523262→669066 (+27.864%, x1.27864)
- cold_alloc_free_256x64b: Ir 158828→191942 (+20.849%, x1.20849) | Cyc 286573→317184 (+10.682%, x1.10682)
- recycle_alloc_free_256x16b: Ir 265223→310466 (+17.058%, x1.17058) | Cyc 418752→471868 (+12.684%, x1.12684)
- recycle_alloc_only_256x16b: Ir 223640→268341 (+19.988%, x1.19988) | Cyc 365129→417450 (+14.329%, x1.14329)
- recycle_alloc_free_256x64b: Ir 266126→311513 (+17.055%, x1.17055) | Cyc 426554→473512 (+11.009%, x1.11009)
- churn_256b: Ir 58160→60204 (+3.514%, x1.03514) | Cyc 143902→147524 (+2.517%, x1.02517)
- churn_write_256b: Ir 58352→60396 (+3.503%, x1.03503) | Cyc 144358→147840 (+2.412%, x1.02412)
- multiseg_cold_256k: Ir 4027836→4029989 (+0.053%, x1.00053) | Cyc 5684025→5691639 (+0.134%, x1.00134)
- seg_cycle_decommit_256k: Ir 12374478→12386071 (+0.094%, x1.00094) | Cyc 17111041→17136191 (+0.147%, x1.00147)
- mimalloc_small_churn_16b: Ir 16629→16629 (+0.000%, x1.00000) | Cyc 39260→39298 (+0.097%, x1.00097)
- mimalloc_small_churn_16b_2n: Ir 21557→21557 (+0.000%, x1.00000) | Cyc 45716→45758 (+0.092%, x1.00092)
- mimalloc_churn_256b: Ir 16130→16130 (+0.000%, x1.00000) | Cyc 37199→37245 (+0.124%, x1.00124)
- mimalloc_cold_alloc_free_256x16b: Ir 32325→32325 (+0.000%, x1.00000) | Cyc 59779→59817 (+0.064%, x1.00064)
- mimalloc_cold_alloc_free_256x16b_2n: Ir 58389→58389 (+0.000%, x1.00000) | Cyc 99245→99291 (+0.046%, x1.00046)
- mimalloc_cold_alloc_free_256x16b_4n: Ir 106653→106653 (+0.000%, x1.00000) | Cyc 170978→170942 (-0.021%, x0.99979)
- mimalloc_cold_alloc_only_256x16b: Ir 24584→24584 (+0.000%, x1.00000) | Cyc 48671→48713 (+0.086%, x1.00086)
- mimalloc_cold_alloc_only_256x16b_2n: Ir 42968→42968 (+0.000%, x1.00000) | Cyc 77423→77431 (+0.010%, x1.00010)
- mimalloc_cold_alloc_only_256x16b_4n: Ir 75872→75872 (+0.000%, x1.00000) | Cyc 127606→127578 (-0.022%, x0.99978)
- mimalloc_cold_alloc_free_256x64b: Ir 33329→33329 (+0.000%, x1.00000) | Cyc 68256→68298 (+0.062%, x1.00062)
- mimalloc_recycle_alloc_free_256x16b: Ir 53027→53027 (+0.000%, x1.00000) | Cyc 86808→86830 (+0.025%, x1.00025)
- mimalloc_recycle_alloc_free_256x64b: Ir 54031→54031 (+0.000%, x1.00000) | Cyc 95209→95247 (+0.040%, x1.00040)
- mimalloc_bootstrap_proxy: Ir 13050→13050 (+0.000%, x1.00000) | Cyc 34965→35037 (+0.206%, x1.00206)
- decomp_full_cycle_8x: Ir 679994→679994 (+0.000%, x1.00000) | Cyc 1452142→1452030 (-0.008%, x0.99992)
- decomp_os_roundtrip_8x: Ir 861→861 (+0.000%, x1.00000) | Cyc 2046→2046 (+0.000%, x1.00000)

CSV: docs/perf/PH3C_B3P_STEP1PRIME_IAI_summary.csv


## Последствия (путь (б), escalation §5)

Ph3c — CANCELLED; P1 — P1-box (принятый известный дефект, не MODEL-LIMIT); Ph5a — CANCELLED; Ph4a стартует на
текущем API drain sidecar; патч S (perf item 80) идёт отдельным `perf(runtime)`. Ветки спайка сохранены как
теги (`archive/ph3c-b3s-093c9d23`, `archive/ph3c-b3p-fd3cdb9e`) и патч `PH3C_B3P_SRC.patch`.
Переоткрытие только по внешней причине (escalation §5): нормативное решение Rust о протекторах `Box`;
нативное воспроизведение; новое решение владельца с новой ценой.
