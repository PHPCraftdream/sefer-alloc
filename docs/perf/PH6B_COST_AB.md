# Ph6b (#2097) — A/B стоимость старого baseline (B0) против кандидата C: measurement-only, `src/` не менялся

## 1. Резюме

Плечи: **B** = commit `7d80d7e1` (worktree `.wt/base`, production-код == родителя), **C** = HEAD `fd954856` (worktree `ph6b`). Все серии чередующиеся (B,C,B,C,…), отдельные B/B-контроли (aa-логи). Судья: `scripts/ph6b_cost_ab_table.mjs` (guarded-принтеры, round-trip ±0.01% на каждом печатном проценте; числитель/знаменатель у каждого отношения; статистика — **median** везде, где указано).

| ось | метрика | B | C | C/B | предел | A/A-шум | вердикт | оракул активации |
|---|---|---|---|---|---|---|---|---|
| 1 iai (callgrind) | HOT Ir, median (4 бенча) | 60629…60821 | 58328…58526 | x0.9620…x0.9623 | Ir≤1.02 И EstCycles≤1.02 | ΔIr mimalloc-плеч = 0.000% ровно (13/13), run2==run1 побитово | **PASS** (все группы hot/refill/zeroed/realloc) | 85/85 набор бенчей в 4 логах + детерминизм + mimalloc ΔIr=0; path-activation: iai-плечи сами измеряют пути — сошлись на наборах |
| 2 bench-table (wall-clock) | geomean C/B по 18 ratio-id (median r на id) | med r B | med r C | **0.9743** | ≤1.05 | полоса **36.461%** (max по 18 id) | **PASS**; max C/B 1.2405 → INCONCLUSIVE (в пределах полосы) | наборы 58 id × 15 прогонов (5 B + 5 C + контроль сборки); control-руки mimalloc/System |
| 2 bench-table | warm-bulk 4 id `global_alloc/SeferAlloc/*` | см. §3.2 | | 0.7776…1.0306 | каждый ≤1.15 | до 26.1% на 16B | **PASS** | те же |
| 3 MT (ph6b_mt_ab) | geomean Mops C/B по 8 ячеек (median Mops на ячейку) | med Mops B | med Mops C | **1.1267** | ≥0.95 | полоса **38.755%** (max по 8 ячеек) | **PASS**; mstress T=8 → INCONCLUSIVE (0.6991, в пределах полосы) | segments_reserved_total median B=24 C=25 (>0); config_conflicts=0 во всех логах |
| 4 RSS frag | frag C/B(w), median по 5 прогонам | 1.0215…1.1449 | идентично | **1.0000** (w1–w4) | ≤1.10 | spread ≤0.005% | **PASS** | config_conflicts delta=0, live=64 MiB ±1% (assert на каждом блоке), A/A spread |
| 4 RSS trim | TRIM-метрики, median KiB по 5 прогонам | 133384/2360/133448/12088 | 133352/2328/133416/12088 | 0.9864…1.0000 | C/B≤1.05 ИЛИ Δ≤1024 KiB | spread ≤4.407% | **PASS** | action_released_delta>0 в 10/10 логов (assert судьи) |

**Общий вердикт по ADR — см. §7.** Единственный не-PASS элемент — ячейка MT mstress T=8 (INCONCLUSIVE по правилу шума, ожидаемо), FAIL (сверх A/A-полосы) — нет; exit code судьи **0**.

## 2. Идентичность

Полный файл: `docs/perf/PH6B_COST_AB_identity.txt`. Ключевое (финальная перепроверка 2026-10-06T00:37:43+02:00):

| параметр | B (`.wt/base`) | C (`ph6b`) |
|---|---|---|
| HEAD | `7d80d7e1fb6127b7747831ee61738b948a9b32f9` | `fd954856acab9aa3015b30d49c87c58d2436d796` |
| `git status --porcelain -- src/ tests/ benches/` | пусто | пусто |
| прочий status | ` M Cargo.toml`, `?? examples/ph3c_frag_stand.rs`, `?? examples/ph6b_mt_ab.rs` | ` M Cargo.toml`, `?? examples/ph6b_mt_ab.rs`, `?? docs/perf/*`, `?? scripts/ph6b_cost_ab_table.mjs`, `?? .wt/` |
| sha256 `examples/ph6b_mt_ab.rs` | `07a92783…d77e6d4a` | `07a92783…d77e6d4a` (совпадают) |
| sha256 binaries (frag/trim/mt/iai/bt) | см. identity-файл; кандид-хеши совпали с pre-registered | там же |
| toolchain/features | `stable-x86_64-unknown-linux-gnu` (WSL), features per-axis — см. identity-файл | те же |

Девиация (зафиксирована до прогонов): дерево B = `5e5da6d1…`, а не ожидавшееся в брифе `70885b71…` при совпадающем SHA коммита; `7d80d7e1` меняет только docs/perf + scripts.

## 3. Оси 1–4 — дословный вывод судьи

Все числа ниже — вывод `node scripts/ph6b_cost_ab_table.mjs` (переносились дословно, не вручную).

### 3.1 Ось 1 — iai

```text
# Ось 1 — iai (callgrind, детерминирован): C против B

Наборы бенчей: 85 = 85 = 85 = 85 (base_run1, cand_run1, base_run2, cand_run2) — OK.
Детерминизм: base_run2 Ir == base_run1 Ir побитово на всех 85; cand_run2 Ir == cand_run1 Ir побитово на всех 85 — OK.
ΔX% = (C − B) / B × 100; B — знаменатель каждого процента (guarded round-trip ±0.01%).

## Контроль A/A: mimalloc-плечи (13, префикс mimalloc_) — ΔIr должно быть 0.000% ровно

- mimalloc_small_churn_16b: Ir B=16629 C=16629 → ΔIr +0.000% OK
- mimalloc_small_churn_16b_2n: Ir B=21557 C=21557 → ΔIr +0.000% OK
- mimalloc_churn_256b: Ir B=16130 C=16130 → ΔIr +0.000% OK
- mimalloc_cold_alloc_free_256x16b: Ir B=32325 C=32325 → ΔIr +0.000% OK
- mimalloc_cold_alloc_free_256x16b_2n: Ir B=58389 C=58389 → ΔIr +0.000% OK
- mimalloc_cold_alloc_free_256x16b_4n: Ir B=106653 C=106653 → ΔIr +0.000% OK
- mimalloc_cold_alloc_only_256x16b: Ir B=24584 C=24584 → ΔIr +0.000% OK
- mimalloc_cold_alloc_only_256x16b_2n: Ir B=42968 C=42968 → ΔIr +0.000% OK
- mimalloc_cold_alloc_only_256x16b_4n: Ir B=75872 C=75872 → ΔIr +0.000% OK
- mimalloc_cold_alloc_free_256x64b: Ir B=33329 C=33329 → ΔIr +0.000% OK
- mimalloc_recycle_alloc_free_256x16b: Ir B=53020 C=53020 → ΔIr +0.000% OK
- mimalloc_recycle_alloc_free_256x64b: Ir B=54024 C=54024 → ΔIr +0.000% OK
- mimalloc_bootstrap_proxy: Ir B=13050 C=13050 → ΔIr +0.000% OK

A/A Ir: все 13 плеч ΔIr = 0.000% ровно — PASS

## Гейты (C/B, знаменатель = base B)

### HOT: Ir ≤ 1.02 И EstCycles ≤ 1.02

- small_churn_16b: Ir B=60629 C=58346 (x0.96234, Δ -3.766%) | EstCycles B=147490 C=144268 (x0.97815, Δ -2.185%) | предел Ir≤1.02 И EstCycles≤1.02 → PASS
- churn_256b: Ir B=60629 C=58346 (x0.96234, Δ -3.766%) | EstCycles B=147520 C=144272 (x0.97798, Δ -2.202%) | предел Ir≤1.02 И EstCycles≤1.02 → PASS
- churn_write_256b: Ir B=60821 C=58526 (x0.96227, Δ -3.773%) | EstCycles B=147904 C=144636 (x0.97790, Δ -2.210%) | предел Ir≤1.02 И EstCycles≤1.02 → PASS
- aligned_churn_640b_a128: Ir B=60635 C=58328 (x0.96195, Δ -3.805%) | EstCycles B=147612 C=144316 (x0.97767, Δ -2.233%) | предел Ir≤1.02 И EstCycles≤1.02 → PASS

### REFILL: EstCycles ≤ 1.05 И Ir ≤ 1.10

- cold_alloc_free_256x16b: Ir B=196467 C=158111 (x0.80477, Δ -19.523%) | EstCycles B=331605 C=279621 (x0.84324, Δ -15.676%) | предел EstCycles≤1.05 И Ir≤1.10 → PASS
- cold_alloc_free_256x64b: Ir B=197322 C=159014 (x0.80586, Δ -19.414%) | EstCycles B=338923 C=287007 (x0.84682, Δ -15.318%) | предел EstCycles≤1.05 И Ir≤1.10 → PASS
- recycle_alloc_free_256x16b: Ir B=307240 C=265409 (x0.86385, Δ -13.615%) | EstCycles B=477290 C=419126 (x0.87814, Δ -12.186%) | предел EstCycles≤1.05 И Ir≤1.10 → PASS
- recycle_alloc_free_256x64b: Ir B=308095 C=266312 (x0.86438, Δ -13.562%) | EstCycles B=485112 C=426890 (x0.87998, Δ -12.002%) | предел EstCycles≤1.05 И Ir≤1.10 → PASS
- multiseg_cold_256k: Ir B=4200265 C=4028010 (x0.95899, Δ -4.101%) | EstCycles B=5924402 C=5682761 (x0.95921, Δ -4.079%) | предел EstCycles≤1.05 И Ir≤1.10 → PASS

### ZEROED: raw Ir ≤ 1.05

(найдено 4 из 5 заявленных; alloc_zeroed_magazine_hit_only_16b_2n отсутствует в наборе)

- alloc_zeroed_magazine_prefill_only_16b: Ir B=59673 C=57540 (x0.96426, Δ -3.574%) | предел raw Ir≤1.05 → PASS
- alloc_zeroed_magazine_hit_only_16b: Ir B=60211 C=58089 (x0.96476, Δ -3.524%) | предел raw Ir≤1.05 → PASS
- alloc_zeroed_calloc_virgin_64k: Ir B=3 C=3 (x1.00000, Δ +0.000%) | предел raw Ir≤1.05 → PASS
- alloc_zeroed_calloc_recycled_64k: Ir B=3 C=3 (x1.00000, Δ +0.000%) | предел raw Ir≤1.05 → PASS

### REALLOC: raw Ir ≤ 1.05 (+ маржинальный Ir/op для realloc_grow ≤ 1.05)

bootstrap-плечо large_alloc_free_cycle: Ir B=49922 C=49951

- realloc_grow: Ir B=611778 C=583182 (x0.95326, Δ -4.674%) | предел raw Ir≤1.05 → PASS | маржинальный Ir/op (=(Ir−Ir[large_alloc_free_cycle])/16): B=35116.00 C=33326.94 (x0.94905, Δ -5.095%) → PASS
- dealloc_realloc_burst_1088_16b_n17: Ir B=526495 C=355793 (x0.67578, Δ -32.422%) | предел raw Ir≤1.05 → PASS

## Все 85 бенчей (ΔIr, ΔCycles; B — знаменатель)

| бенч | Ir B→C (Δ, x) | EstCycles B→C (Δ, x) |
|---|---|---|
| small_churn_16b (hot) | 60629→58346 (-3.766%, x0.96234) | 147490→144268 (-2.185%, x0.97815) |
| small_churn_16b_2n | 67285→65118 (-3.221%, x0.96779) | 155896→152888 (-1.929%, x0.98071) |
| dealloc_prealloc_only_16b | 74148→64265 (-13.329%, x0.86671) | 165142→151419 (-8.310%, x0.91690) |
| dealloc_free_only_16b | 82778→73187 (-11.586%, x0.88414) | 177665→164534 (-7.391%, x0.92609) |
| dealloc_contains_base_probe_only_16b | 75495→65617 (-13.084%, x0.86916) | 166985→153293 (-8.200%, x0.91800) |
| dealloc_segment_base_of_ptr_probe_only_16b | 74738→64848 (-13.233%, x0.86767) | 165876→152198 (-8.246%, x0.91754) |
| alloc_magazine_prefill_only_16b | 59673→57528 (-3.595%, x0.96405) | 145497→142369 (-2.150%, x0.97850) |
| alloc_magazine_hit_only_16b | 60162→58013 (-3.572%, x0.96428) | 146277→143137 (-2.147%, x0.97853) |
| alloc_zeroed_magazine_prefill_only_16b (zeroed) | 59673→57540 (-3.574%, x0.96426) | 145463→142423 (-2.090%, x0.97910) |
| alloc_zeroed_magazine_hit_only_16b (zeroed) | 60211→58089 (-3.524%, x0.96476) | 146410→143339 (-2.098%, x0.97902) |
| dealloc_hash_contains_only_probe_16b | 76517→66639 (-12.910%, x0.87090) | 168031→154411 (-8.106%, x0.91894) |
| dealloc_own_thread_body_only_16b | 81797→71985 (-11.996%, x0.88004) | 176368→162916 (-7.627%, x0.92373) |
| dealloc_free_only_16b_n1 | 74217→64346 (-13.300%, x0.86700) | 165473→151939 (-8.179%, x0.91821) |
| dealloc_free_only_16b_n8 | 74676→64829 (-13.186%, x0.86814) | 166125→152588 (-8.149%, x0.91851) |
| dealloc_free_only_16b_n9 | 74741→64899 (-13.168%, x0.86832) | 166174→152679 (-8.121%, x0.91879) |
| dealloc_free_only_16b_n16 | 75196→65389 (-13.042%, x0.86958) | 166755→153316 (-8.059%, x0.91941) |
| dealloc_free_only_16b_n17 | 76006→66202 (-12.899%, x0.87101) | 168650→155152 (-8.004%, x0.91996) |
| dealloc_free_only_16b_n32 | 77722→67991 (-12.520%, x0.87480) | 170885→157538 (-7.811%, x0.92189) |
| dealloc_flush_class_only_16b_prefix | 54562→52130 (-4.457%, x0.95543) | 139106→135615 (-2.510%, x0.97490) |
| dealloc_flush_class_only_16b | 55067→52650 (-4.389%, x0.95611) | 140439→136879 (-2.535%, x0.97465) |
| alloc_clear_magazine_only_16b_prefix | 55078→52687 (-4.341%, x0.95659) | 139663→136174 (-2.498%, x0.97502) |
| alloc_clear_magazine_only_16b | 55451→53072 (-4.290%, x0.95710) | 140136→136738 (-2.425%, x0.97575) |
| alloc_zeroed_calloc_virgin_64k_prefix (zeroed) | 3→3 (+0.000%, x1.00000) | 39→39 (+0.000%, x1.00000) |
| alloc_zeroed_calloc_virgin_64k (zeroed) | 3→3 (+0.000%, x1.00000) | 39→39 (+0.000%, x1.00000) |
| alloc_zeroed_calloc_recycled_64k_prefix (zeroed) | 3→3 (+0.000%, x1.00000) | 39→39 (+0.000%, x1.00000) |
| alloc_zeroed_calloc_recycled_64k (zeroed) | 3→3 (+0.000%, x1.00000) | 39→39 (+0.000%, x1.00000) |
| dealloc_prealloc_only_1088_16b | 517435→349152 (-32.523%, x0.67477) | 769475→540042 (-29.817%, x0.70183) |
| dealloc_free_only_1088_16b_n17 | 519293→351089 (-32.391%, x0.67609) | 772847→543745 (-29.644%, x0.70356) |
| dealloc_free_only_1088_16b_n32 | 521009→352878 (-32.270%, x0.67730) | 775218→546131 (-29.551%, x0.70449) |
| dealloc_free_only_1088_16b_n64 | 526053→358074 (-31.932%, x0.68068) | 781970→553127 (-29.265%, x0.70735) |
| dealloc_free_only_1088_16b_n256 | 556317→389250 (-30.031%, x0.69969) | 822414→595099 (-27.640%, x0.72360) |
| dealloc_free_only_1088_16b_n1024 | 677373→513966 (-24.124%, x0.75876) | 984502→763163 (-22.482%, x0.77518) |
| dealloc_realloc_burst_1088_16b_n17 (realloc) | 526495→355793 (-32.422%, x0.67578) | 783029→550700 (-29.671%, x0.70329) |
| oscillating_live_set_16b | 91318→87985 (-3.650%, x0.96350) | 186937→182625 (-2.307%, x0.97693) |
| carve_batch_only_16b | 3238→3241 (+0.093%, x1.00093) | 8035→8114 (+0.983%, x1.00983) |
| carve_batch_only_16b_2n | 9139→9142 (+0.033%, x1.00033) | 18357→18432 (+0.409%, x1.00409) |
| dealloc_batch_fresh_16_16b | 3→3 (+0.000%, x1.00000) | 39→39 (+0.000%, x1.00000) |
| dealloc_batch_fresh_64_16b | 3→3 (+0.000%, x1.00000) | 39→39 (+0.000%, x1.00000) |
| dealloc_batch_fresh_80_16b | 3→3 (+0.000%, x1.00000) | 39→39 (+0.000%, x1.00000) |
| dealloc_batch_fresh_81_16b | 3→3 (+0.000%, x1.00000) | 39→39 (+0.000%, x1.00000) |
| dealloc_batch_fresh_128_16b | 3→3 (+0.000%, x1.00000) | 39→39 (+0.000%, x1.00000) |
| dealloc_batch_fresh_200_16b | 3→3 (+0.000%, x1.00000) | 39→39 (+0.000%, x1.00000) |
| dealloc_batch_fresh_512_16b | 3→3 (+0.000%, x1.00000) | 39→39 (+0.000%, x1.00000) |
| dealloc_batch_fresh_1024_16b | 3→3 (+0.000%, x1.00000) | 39→39 (+0.000%, x1.00000) |
| dealloc_batch_fresh_0_16b | 3→3 (+0.000%, x1.00000) | 39→39 (+0.000%, x1.00000) |
| dealloc_batch_fresh_1_16b | 3→3 (+0.000%, x1.00000) | 39→39 (+0.000%, x1.00000) |
| dealloc_batch_fresh_8_16b | 3→3 (+0.000%, x1.00000) | 39→39 (+0.000%, x1.00000) |
| dealloc_batch_fresh_17_16b | 3→3 (+0.000%, x1.00000) | 39→39 (+0.000%, x1.00000) |
| medium_class_dealloc_churn_16b | 60641→58334 (-3.804%, x0.96196) | 147586→144248 (-2.262%, x0.97738) |
| aligned_churn_640b_a128 (hot) | 60635→58328 (-3.805%, x0.96195) | 147612→144316 (-2.233%, x0.97767) |
| large_alloc_free_cycle | 49922→49951 (+0.058%, x1.00058) | 133833→133919 (+0.064%, x1.00064) |
| large_cache_prefill_only_4mib | 58908→58939 (+0.053%, x1.00053) | 146364→146317 (-0.032%, x0.99968) |
| large_cache_hit_only_4mib | 60248→60242 (-0.010%, x0.99990) | 148900→148733 (-0.112%, x0.99888) |
| large_cache_free_slot_search_prefill_only | 63707→63749 (+0.066%, x1.00066) | 156868→156843 (-0.016%, x0.99984) |
| large_cache_free_slot_search_cycle_only | 75260→75282 (+0.029%, x1.00029) | 174074→174080 (+0.003%, x1.00003) |
| realloc_grow (realloc) | 611778→583182 (-4.674%, x0.95326) | 3740845→3701393 (-1.055%, x0.98945) |
| cold_alloc_free_256x16b (refill) | 196467→158111 (-19.523%, x0.80477) | 331605→279621 (-15.676%, x0.84324) |
| cold_alloc_free_256x16b_2n | 351181→274441 (-21.852%, x0.78148) | 542477→438623 (-19.144%, x0.80856) |
| cold_alloc_free_256x16b_4n | 659049→505541 (-23.292%, x0.76708) | 959504→751898 (-21.637%, x0.78363) |
| cold_alloc_only_256x16b | 156554→116991 (-25.271%, x0.74729) | 277227→223247 (-19.471%, x0.80529) |
| cold_alloc_only_256x16b_2n | 269892→190729 (-29.331%, x0.70669) | 432795→324937 (-24.921%, x0.75079) |
| cold_alloc_only_256x16b_4n | 495008→336645 (-31.992%, x0.68008) | 739436→523666 (-29.180%, x0.70820) |
| cold_alloc_free_256x64b (refill) | 197322→159014 (-19.414%, x0.80586) | 338923→287007 (-15.318%, x0.84682) |
| recycle_alloc_free_256x16b (refill) | 307240→265409 (-13.615%, x0.86385) | 477290→419126 (-12.186%, x0.87814) |
| recycle_alloc_only_256x16b | 266338→223826 (-15.962%, x0.84038) | 424188→365605 (-13.811%, x0.86189) |
| recycle_alloc_free_256x64b (refill) | 308095→266312 (-13.562%, x0.86438) | 485112→426890 (-12.002%, x0.87998) |
| churn_256b (hot) | 60629→58346 (-3.766%, x0.96234) | 147520→144272 (-2.202%, x0.97798) |
| churn_write_256b (hot) | 60821→58526 (-3.773%, x0.96227) | 147904→144636 (-2.210%, x0.97790) |
| multiseg_cold_256k (refill) | 4200265→4028010 (-4.101%, x0.95899) | 5924402→5682761 (-4.079%, x0.95921) |
| seg_cycle_decommit_256k | 12858979→12374664 (-3.766%, x0.96234) | 17791308→17100611 (-3.882%, x0.96118) |
| mimalloc_small_churn_16b (mimalloc-AA) | 16629→16629 (+0.000%, x1.00000) | 39320→39318 (-0.005%, x0.99995) |
| mimalloc_small_churn_16b_2n (mimalloc-AA) | 21557→21557 (+0.000%, x1.00000) | 45788→45786 (-0.004%, x0.99996) |
| mimalloc_churn_256b (mimalloc-AA) | 16130→16130 (+0.000%, x1.00000) | 37271→37261 (-0.027%, x0.99973) |
| mimalloc_cold_alloc_free_256x16b (mimalloc-AA) | 32325→32325 (+0.000%, x1.00000) | 59831→59837 (+0.010%, x1.00010) |
| mimalloc_cold_alloc_free_256x16b_2n (mimalloc-AA) | 58389→58389 (+0.000%, x1.00000) | 99369→99273 (-0.097%, x0.99903) |
| mimalloc_cold_alloc_free_256x16b_4n (mimalloc-AA) | 106653→106653 (+0.000%, x1.00000) | 171126→170966 (-0.093%, x0.99907) |
| mimalloc_cold_alloc_only_256x16b (mimalloc-AA) | 24584→24584 (+0.000%, x1.00000) | 48761→48729 (-0.066%, x0.99934) |
| mimalloc_cold_alloc_only_256x16b_2n (mimalloc-AA) | 42968→42968 (+0.000%, x1.00000) | 77551→77417 (-0.173%, x0.99827) |
| mimalloc_cold_alloc_only_256x16b_4n (mimalloc-AA) | 75872→75872 (+0.000%, x1.00000) | 127798→127636 (-0.127%, x0.99873) |
| mimalloc_cold_alloc_free_256x64b (mimalloc-AA) | 33329→33329 (+0.000%, x1.00000) | 68350→68254 (-0.140%, x0.99860) |
| mimalloc_recycle_alloc_free_256x16b (mimalloc-AA) | 53020→53020 (+0.000%, x1.00000) | 86867→86813 (-0.062%, x0.99938) |
| mimalloc_recycle_alloc_free_256x64b (mimalloc-AA) | 54024→54024 (+0.000%, x1.00000) | 95326→95234 (-0.097%, x0.99903) |
| mimalloc_bootstrap_proxy (mimalloc-AA) | 13050→13050 (+0.000%, x1.00000) | 35037→35065 (+0.080%, x1.00080) |
| decomp_full_cycle_8x | 680055→680068 (+0.002%, x1.00002) | 1452296→1452183 (-0.008%, x0.99992) |
| decomp_os_roundtrip_8x | 861→861 (+0.000%, x1.00000) | 2046→2080 (+1.662%, x1.01662) |
```

### 3.2 Ось 2 — bench-table

```text
# Ось 2 — bench-table (criterion wall-clock): C против B

r(id, run) = ns[SeferAlloc]/ns[mimalloc] внутри одного прогона (числитель/знаменатель указаны).
Медиана (статистика — median) по 5 прогонам на сторону; C/B(id) = med(r_cand) / med(r_b).
A/A: 10 прогонов same-vs-same, med(r_odd)/med(r_even) на id; полоса = max|·−1| по 18 id.

A/A-полоса оси 2 (max по 18 id |med(r_odd)/med(r_even) − 1|): 36.461%

| id (18 ratio) | med r B (Sefer/mimalloc) | med r C | C/B | предел | A/A |мед r_odd/мед r_even| | вердикт |
|---|---:|---:|---:|---|---:|---:|---|
| global_alloc/SeferAlloc/16B | 7.3546 | 7.5188 | 1.0223 | ≤1.15 | 26.069% | 6.4716/8.1586 | PASS |
| global_alloc/SeferAlloc/64B | 6.0469 | 6.0176 | 0.9952 | ≤1.15 | 18.371% | 6.3941/7.5687 | PASS |
| global_alloc/SeferAlloc/256B | 5.4866 | 4.2665 | 0.7776 | ≤1.15 | 1.721% | 4.8645/4.7808 | PASS |
| global_alloc/SeferAlloc/1024B | 2.5433 | 2.6212 | 1.0306 | ≤1.15 | 1.769% | 2.3198/2.3608 | PASS |
| global_alloc_churn/SeferAlloc/16B | 2.2928 | 2.0927 | 0.9127 | — | 4.397% | 1.9007/1.8172 | INFO |
| global_alloc_churn/SeferAlloc/64B | 1.6409 | 1.6591 | 1.0111 | — | 8.078% | 1.7063/1.5685 | INFO |
| global_alloc_churn/SeferAlloc/256B | 0.9867 | 0.8213 | 0.8324 | — | 19.116% | 1.1884/1.4155 | INFO |
| global_alloc_churn/SeferAlloc/1024B | 0.2023 | 0.2510 | 1.2405 | — | 13.356% | 0.2243/0.1943 | INFO |
| global_alloc_churn_write/SeferAlloc/16B | 2.1356 | 2.1685 | 1.0154 | — | 30.069% | 2.0429/1.4286 | INFO |
| global_alloc_churn_write/SeferAlloc/64B | 1.9627 | 1.7527 | 0.8930 | — | 25.071% | 2.0270/2.5352 | INFO |
| global_alloc_churn_write/SeferAlloc/256B | 1.2569 | 1.2182 | 0.9692 | — | 20.290% | 1.3030/1.0386 | INFO |
| global_alloc_churn_write/SeferAlloc/1024B | 0.3843 | 0.2994 | 0.7791 | — | 1.556% | 0.2720/0.2677 | INFO |
| global_alloc_churn_with_teardown/SeferAlloc/16B | 2.8533 | 2.7879 | 0.9771 | — | 5.974% | 2.7319/2.5687 | INFO |
| global_alloc_churn_with_teardown/SeferAlloc/64B | 2.7213 | 2.7250 | 1.0014 | — | 33.071% | 2.3311/1.5602 | INFO |
| global_alloc_churn_with_teardown/SeferAlloc/256B | 1.8640 | 1.9997 | 1.0728 | — | 11.121% | 2.0592/2.2882 | INFO |
| global_alloc_churn_with_teardown/SeferAlloc/1024B | 1.4007 | 1.5038 | 1.0736 | — | 36.461% | 1.7659/1.1220 | INFO |
| global_alloc/manual_realloc_sim/SeferAlloc | 1.7677 | 1.6488 | 0.9327 | — | 8.688% | 1.9929/2.1660 | INFO |
| segment_decommit_cycle/SeferAlloc/253KiB | 519.3065 | 581.4226 | 1.1196 | — | 15.574% | 406.2281/469.4933 | INFO |

geomean C/B по 18 id (статистика — median на id) = 0.9743 (числитель — произведение 18 C/B, знаменатель — их геометрическая нормировка 1) | предел ≤1.05 → PASS
max C/B по 18 id = 1.2405 (id global_alloc_churn/SeferAlloc/1024B) | предел ≤1.10 → INCONCLUSIVE (в пределах A/A-полосы) (A/A-полоса 36.461%)

warm-bulk (4 id global_alloc/SeferAlloc/{16B,64B,256B,1024B}), предел ≤1.15 каждый:
- global_alloc/SeferAlloc/16B: med r B=7.3546 med r C=7.5188 → C/B 1.0223 | A/A 26.069% → PASS
- global_alloc/SeferAlloc/64B: med r B=6.0469 med r C=6.0176 → C/B 0.9952 | A/A 18.371% → PASS
- global_alloc/SeferAlloc/256B: med r B=5.4866 med r C=4.2665 → C/B 0.7776 | A/A 1.721% → PASS
- global_alloc/SeferAlloc/1024B: med r B=2.5433 med r C=2.6212 → C/B 1.0306 | A/A 1.769% → PASS
warm-bulk: PASS (все ≤1.15)

Контрольные руки (raw ns, медиана по 5 прогонам; C/B — индикатор межсерийного смещения, не гейт):

| control id | ns B (median) | ns C (median) | C/B |
|---|---:|---:|---:|
| global_alloc/mimalloc/16B | 5382.2 | 5690.0 | 1.0572 |
| global_alloc/mimalloc/64B | 7101.5 | 7014.6 | 0.9878 |
| global_alloc/mimalloc/256B | 9338.5 | 10647.0 | 1.1401 |
| global_alloc/mimalloc/1024B | 19148.0 | 20831.0 | 1.0879 |
| global_alloc_churn/mimalloc/16B | 8223.1 | 9169.1 | 1.1150 |
| global_alloc_churn/mimalloc/64B | 11087.0 | 10046.0 | 0.9061 |
| global_alloc_churn/mimalloc/256B | 15950.0 | 21584.0 | 1.3532 |
| global_alloc_churn/mimalloc/1024B | 80938.0 | 73086.0 | 0.9030 |
| global_alloc_churn_write/mimalloc/16B | 7838.4 | 7914.7 | 1.0097 |
| global_alloc_churn_write/mimalloc/64B | 9299.3 | 10188.0 | 1.0956 |
| global_alloc_churn_write/mimalloc/256B | 15729.0 | 15478.0 | 0.9840 |
| global_alloc_churn_write/mimalloc/1024B | 84298.0 | 80809.0 | 0.9586 |
| global_alloc_churn_with_teardown/mimalloc/16B | 8779.8 | 7395.1 | 0.8423 |
| global_alloc_churn_with_teardown/mimalloc/64B | 8737.6 | 9158.4 | 1.0482 |
| global_alloc_churn_with_teardown/mimalloc/256B | 12668.0 | 14321.0 | 1.1305 |
| global_alloc_churn_with_teardown/mimalloc/1024B | 36723.0 | 19388.0 | 0.5280 |
| global_alloc/manual_realloc_sim/mimalloc | 418.9 | 468.0 | 1.1173 |
| segment_decommit_cycle/mimalloc/253KiB | 3454.8 | 3176.2 | 0.9194 |

| global_alloc/System/16B | 26763.0 | 16530.0 | 0.6176 |
| global_alloc/System/64B | 27261.0 | 26578.0 | 0.9749 |
| global_alloc/System/256B | 45117.0 | 47792.0 | 1.0593 |
| global_alloc/System/1024B | 49500.0 | 51905.0 | 1.0486 |
| global_alloc_churn/System/16B | 14154.0 | 16216.0 | 1.1457 |
| global_alloc_churn/System/64B | 17489.0 | 17613.0 | 1.0071 |
| global_alloc_churn/System/256B | 24032.0 | 42622.0 | 1.7736 |
| global_alloc_churn/System/1024B | 36983.0 | 39268.0 | 1.0618 |
| global_alloc_churn_write/System/16B | 16177.0 | 13762.0 | 0.8507 |
| global_alloc_churn_write/System/64B | 19064.0 | 16827.0 | 0.8827 |
| global_alloc_churn_write/System/256B | 17954.0 | 15597.0 | 0.8687 |
| global_alloc_churn_write/System/1024B | 21454.0 | 17759.0 | 0.8278 |
| global_alloc_churn_with_teardown/System/16B | 16296.0 | 17459.0 | 1.0714 |
| global_alloc_churn_with_teardown/System/64B | 18157.0 | 18721.0 | 1.0311 |
| global_alloc_churn_with_teardown/System/256B | 28514.0 | 29271.0 | 1.0265 |
| global_alloc_churn_with_teardown/System/1024B | 33555.0 | 26534.0 | 0.7908 |
| global_alloc/manual_realloc_sim/System | 537.1 | 595.1 | 1.1081 |
| segment_decommit_cycle/System/253KiB | 2339.0 | 1803.1 | 0.7709 |
```

### 3.3 Ось 3 — MT

```text
# Ось 3 — MT wall-clock (ph6b_mt_ab): C против B

Ячейка (workload,T): медиана ns по 7 прогонам на сторону; Mops = ops/ns·1000;
Mops C/B = med(Mops_B) / med(Mops_C) (throughput: >1 — C медленнее, <1 — C быстрее).
A/A: 10 прогонов same-vs-same, med(Mops_odd)/med(Mops_even) на ячейку; полоса = max|·−1|;
также (max−min)/median по 10 aa-прогонам — информативно.

A/A-полоса оси 3 (max |med(Mops_odd)/med(Mops_even) − 1| по 8 ячеек): 38.755%

| ячейка | med ns B | med ns C | med Mops B | med Mops C | Mops C/B (B/C) | предел | A/A |spread%| | вердикт |
|---|---:|---:|---:|---:|---:|---|---:|---:|---|
| larson T=1 | 13129934 | 9973318 | 61.0465 | 80.3680 | 1.3165 | ≥0.90 | 16.271% | 72.499% | PASS |
| mstress T=1 | 8580702 | 8256693 | 46.5416 | 48.3680 | 1.0392 | ≥0.90 | 3.578% | 48.247% | PASS |
| larson T=2 | 23244070 | 17845117 | 68.8551 | 89.7283 | 1.3031 | ≥0.90 | 3.074% | 40.901% | PASS |
| mstress T=2 | 16404607 | 10274880 | 48.6254 | 77.6978 | 1.5979 | ≥0.90 | 7.737% | 82.247% | PASS |
| larson T=4 | 31336471 | 27562673 | 102.1821 | 116.2157 | 1.1373 | ≥0.90 | 34.338% | 110.412% | PASS |
| mstress T=4 | 18528175 | 16815655 | 86.0757 | 94.9487 | 1.1031 | ≥0.90 | 18.102% | 148.006% | PASS |
| larson T=8 | 52686366 | 50709356 | 121.3214 | 126.0509 | 1.0390 | ≥0.90 | 38.755% | 131.140% | PASS |
| mstress T=8 | 26132113 | 37389234 | 121.9474 | 85.2585 | 0.6991 | ≥0.90 | 6.583% | 128.479% | INCONCLUSIVE |

geomean Mops C/B по 8 ячеек (статистика — median на ячейку) = 1.1267 (числитель — произведение 8 C/B, знаменатель — геометрическая нормировка 1) | предел ≥0.95 → PASS

Оракулы MT: segments_reserved_total медиана B=24 C=25 (обоим >0 — OK); config_conflicts = 0 во всех логах — OK.
```

### 3.4 Ось 4 — RSS (frag + trim)

```text
# Ось 4 — RSS: фрагментация (frag-stand) и trim

frag = (rss_end_kib − rss_empty_kib)·1024 / live_requested_bytes (безразмерное; live в 64 MiB ±1% — assert пройден на каждом блоке; config_conflicts delta=0 — assert пройден).
Медианы (статистика — median) по 5 чередующимся прогонам B/C на нагрузку; C/B(w) = med(frag_C)/med(frag_B), знаменатель = B.
A/A: 3 B-прогонов каждой нагрузки в aa-логе, spread = (max−min)/median.

| w | med frag B | med frag C | C/B (знаменатель B) | гейт ≤1.10 | сегменты B→C | A/A spread % |
|---|---:|---:|---:|---|---|---:|
| w1 | 1.1122 | 1.1122 | 1.0000 | PASS | 17→17 | 0.005% |
| w2 | 1.0655 | 1.0655 | 1.0000 | PASS | 17→17 | 0.000% |
| w3 | 1.1449 | 1.1449 | 1.0000 | PASS | 19→19 | 0.000% |
| w4 | 1.0215 | 1.0215 | 1.0000 | контроль 1.0000 | 17→17 | 0.000% |

## trim (r31_10_trim_rss_gate, arm TRIM): C против B

Медианы (статистика — median) по 5 прогонам на сторону, KiB. Оракул: action_released_delta > 0
у TRIM-ребёнка в каждом из 10 логов (5 B + 5 C) — assert при парсинге пройден (10/10).
Гейт: C/B ≤ 1.05 ИЛИ (C − B) ≤ 1024 KiB. A/A: (max−min)/median по 5 base-прогонам.

| метрика | med B, KiB | med C, KiB | C/B (знаменатель B) | C−B, KiB | гейт | A/A spread % | вердикт |
|---|---:|---:|---:|---:|---|---:|---|
| rss_burst1_kib | 133384.0 | 133352.0 | 0.9998 | -32.0 | C/B≤1.05 ИЛИ C−B≤1024 KiB → PASS | 0.078% | PASS |
| rss_idle_kib | 2360.0 | 2328.0 | 0.9864 | -32.0 | C/B≤1.05 ИЛИ C−B≤1024 KiB → PASS | 4.407% | PASS |
| rss_burst2_kib | 133448.0 | 133416.0 | 0.9998 | -32.0 | C/B≤1.05 ИЛИ C−B≤1024 KiB → PASS | 0.078% | PASS |
| commit_idle_kib | 12088.0 | 12088.0 | 1.0000 | 0.0 | C/B≤1.05 ИЛИ C−B≤1024 KiB → PASS | 0.000% | PASS |

NO_TRIM-рука (справочно): idle_released_delta = 0 во всех логах обеих сторон (трим просто не вызывается).
```

### 3.5 NOT_RUN / NOT_COMPARABLE / отклонённые серии (дословно из судьи)

```text
# NOT_RUN / NOT_COMPARABLE / отклонённые серии

- remote-merge iai-плечо — NOT_COMPARABLE: bench-функций с remote/merge в perf_gate_iai нет ни в B, ни в C.
- remote lag p99 — NOT_RUN: нет наблюдаемого плеча/счётчиков без изменения src/.
- метаданные Small route (байты у System; adversarial ≤304,128) — NOT_RUN: probe отсутствует в обоих деревьях; 304,128 — статическая оценка из ACTIVE.md/ревью, не измерение.
- resolved_config readback — unavailable: нет публичного API (как в PH3C_FRAG_STAND).
- NOISY-история: первая MT-серия (_raw_ph6b_mt_*_run{1..10}_noisy.log) и первый bench-table base-прогон (_raw_ph6b_benchtable_base_run1_noisy.log) под нагрузкой numa-r6/pool_cap_sweep — отклонены, не парсились; единственный разрешённый перемер выполнен (см. _raw_ph6b_load_check_w3/w4/w5.log).
- A/B/B/A не проводился; вместо него чередование B,C,B,C,... внутри каждого лога и B/B-контроль (aa-логи).
```

## 4. NOT_RUN / NOT_COMPARABLE с причинами

- **remote-merge iai-плечо — NOT_COMPARABLE**: bench-функций с remote/merge в `perf_gate_iai` нет ни в B, ни в C (сравнивать нечем без изменения src/).
- **remote lag p99 — NOT_RUN**: нет наблюдаемого плеча/счётчиков без изменения src/.
- **метаданные Small route (байты у System; adversarial ≤304,128) — NOT_RUN**: probe отсутствует в обоих деревьях; 304,128 — статическая оценка из ACTIVE.md/ревью, **не измерение**.
- **resolved_config readback — unavailable**: нет публичного API (как в PH3C_FRAG_STAND).
- **NOISY-история**: первая MT-серия (`_raw_ph6b_mt_{base,cand,aa}_run{1..10}_noisy.log`) и первый bench-table base-прогон (`_raw_ph6b_benchtable_base_run1_noisy.log`) были сняты под чужой нагрузкой (numa-r6 `cargo test` + `pool_cap_sweep`, см. `_raw_ph6b_load_check_w3.log`) — **отклонены и не парсились**; единственный разрешённый перемер выполнен на тихой машине (load-check W4/W5: quiet, power plan Balanced). A/B/B/A не проводился — вместо него чередование B,C,B,C,… внутри каждого лога и B/B-контроль (aa-логи).

## 5. Ограничения

- **Wall-clock шум хоста**: WSL2, power plan Balanced. Измеренные A/A-полосы велики: ось 2 — 36.461%, ось 3 — 38.755% (max по id/ячейкам |med(odd)/med(even)−1|). Именно поэтому правило шума (FAIL только за пределами полосы) перевело `bench-table max` (1.2405 при пределе 1.10) и `mstress T=8` (0.6991 при пределе 0.90) в INCONCLUSIVE, а не FAIL.
- **MT-харнесс без in-run control-руки** (нет одновременной контрольной руки в одном процессе): рекомендация на будущее — добавить in-run interleaved control, чтобы полоса шума считалась внутри каждого прогона.
- **Warm-bulk 16B/64B A/A-разброс до 26%** — абсолютные ns на этих id нестабильны на этом хосте; ratio-нормировка (Sefer/mimalloc внутри одного прогона) снижает, но не устраняет шум.
- **B0+S→C колонка не строилась** (необязательна; патч S в составе C). При необходимости — отдельный прогон с `docs/perf/PG3R_SCAN_PATCH_S.patch` в `.wt/base-s`.
- Базовый лог `_raw_ph6b_benchtable_base_build.log` обрезан по правилу CLAUDE.md (маркер `# TRUNCATED` внутри), сохранён как evidence свежести сборки base; числа оси 2 берутся только из отфильтрованных прогонов `base_run{1..5}`.

## 6. Ограничения интерпретации / что НЕ заявляется

Никаких speedup-заявлений сверх измеренного: каждое отношение в §3 имеет числитель и знаменатель; оси 2–3 — wall-clock с широкими A/A-полосами; регрессия не обменивается на выигрыш (правило НЕ предлагать trade-off).

## 7. Общий вердикт по ADR

По фактическим вердиктам судьи: iai **PASS** (все гейты, A/A = 0.000% ровно), RSS **PASS** (frag C/B = 1.0000 на всех w, trim-гейты PASS, оракулы 10/10), bench-table **PASS** по зарегистрированным гейтам (geomean 0.9743 ≤ 1.05; warm-bulk ≤ 1.15; max 1.2405 — INCONCLUSIVE в пределах A/A-полосы 36.461%), MT **INCONCLUSIVE** (7/8 ячеек в пользу C, geomean Mops C/B = 1.1267 ≥ 0.95; mstress T=8 C/B = 0.6991 в пределах A/A-полосы 38.755%).

**NO-GO оснований не найдено.** Оба INCONCLUSIVE (MT `mstress T=8`, bench-table `global_alloc_churn/*/1024B`) возникли потому, что судья сравнивает ячейку с ГЛОБАЛЬНОЙ A/A-полосой (max по ячейкам, 36–39%), а собственный A/A конкретной ячейки на 5–7 прогонах ненадёжен при разбросе 128–894%. Оркестратор перемерил обе ячейки на тихой машине (§9, критерий Манна—Уитни + bootstrap-доверительный интервал): значимой регрессии нет ни в одной из 8 MT-ячеек (30+30 прогонов; mstress T=8: C/B(ns)=1.084, p=0.929) и на `churn/1024B` (60+60 прогонов: C/B=1.0494, p=0.725). Исходные оценки (mstress T=8 Mops C/B 0.6991; churn/1024B C/B 1.2405) не воспроизвелись и объясняются шумом хоста. **Формально неопределённость остаётся:** bootstrap 95% CI для `churn/1024B` = [0.855, 1.269] включает предел 1.10, т. е. wall-clock на WSL2 не исключает +27%; детерминированная ось (iai) инструкций сверх базы нигде не показывает. **Итог: GO по детерминированным осям (iai, RSS); wall-clock — регрессия не обнаружена, точный размер эффекта на этом хосте не определим (при необходимости — повтор на нативной Windows/тихом хосте по решению владельца).** Вердикт судьи (§3) не переписывается — §9 его дополняет.

## 8. Артефакты

Судья: `scripts/ph6b_cost_ab_table.mjs`; сводка: `docs/perf/PH6B_COST_AB_summary.csv` (136 строк данных + заголовок); identity: `docs/perf/PH6B_COST_AB_identity.txt`.

Сырьё (все < 200 KiB после уборки; `_raw_ph6b_benchtable_base_run1_noisy.log` обрезан маркером `# TRUNCATED` — серия отклонена и не парсилась; все прочие `*_noisy.log` тоже отклонены и не парсились):

- iai: `_raw_ph6b_iai_{base,cand}_run{1,2}.log` (по 85 бенчей; run2 — детерминизм)
- bench-table: `_raw_ph6b_benchtable_{base,cand}_run{1..5}.log`, `_raw_ph6b_benchtable_aa_run{1..10}.log`, `_raw_ph6b_benchtable_base_build.log` (evidence сборки base, `# TRUNCATED`)
- MT: `_raw_ph6b_mt_{base,cand}_run{1..7}.log`, `_raw_ph6b_mt_aa_run{1..10}.log`
- frag: `_raw_ph6b_frag_w{1..4}.log` (по 10 блоков B1..C5), `_raw_ph6b_frag_aa.log` (12 блоков B)
- trim: `_raw_ph6b_trimrss_{base,cand}_run{1..5}.log`
- сборки: `_raw_ph6b_build_{base,cand}_{frag,mt,trim}.log`
- перемер оркестратора (§9): `_raw_ph6b_mt_rerun_quiet.log`, `_raw_ph6b_bt_rerun_quiet.log`; судьи `scripts/ph6b_mt_rerun_table.mjs`, `scripts/ph6b_bt_rerun_table.mjs`
- среда: `_raw_ph6b_load_check_w3.log` (зафиксированная чужая нагрузка → причина отклонения noisy-серий), `_raw_ph6b_load_check_w4.log`, `_raw_ph6b_load_check_w5.log` (тихая машина перед перемером)
- отклонённые (НЕ парсились): `_raw_ph6b_benchtable_{base_run1,cand_run1..5,aa_run1..10}_noisy.log`, `_raw_ph6b_benchtable_base_run{2..5}_noisy.log`, `_raw_ph6b_mt_{base,cand,aa}_run{1..10}_noisy.log`

## 9. Дополнение оркестратора: перемер INCONCLUSIVE-ячеек на тихой машине

Условия: WSL2, load average ≈ 0.15 до серии (`uptime` в начале каждого лога), чередование B,C,B,C,… на ТЕХ ЖЕ бинарях, что в §2 (sha256 в начале логов совпадают с identity-файлом). Статистика и пороги печатаются скриптами (`scripts/ph6b_mt_rerun_table.mjs`, `scripts/ph6b_bt_rerun_table.mjs`), числа ниже — их вывод дословно; регрессия заявляется только при превышении предела И значимом критерии Манна—Уитни (двусторонний, нормальная аппроксимация, средние ранги при совпадениях).

### 9.1 MT (`_raw_ph6b_mt_rerun_quiet.log`, 30+30 прогонов)

```text
# Ph6b MT re-measure (quiet machine): 30 alternating runs per side
ratio = med_ns(C) / med_ns(B) (numerator med C, denominator med B; >1 = C slower); regression iff ratio > 1.1111 AND Mann-Whitney p < 0.00625

| cell | n B/C | med ns B | med ns C | C/B (med C / med B) | min ns B | min ns C | min C/B | MWU p | verdict |
|---|---|---:|---:|---:|---:|---:|---:|---:|---|
| larson T=1 | 30/30 | 20010099.5 | 18112821 | 0.905 | 8622540 | 8790778 | 1.020 | 0.824 | NO DETECTABLE REGRESSION |
| mstress T=1 | 30/30 | 10052140.5 | 10030749 | 0.998 | 6355488 | 6179212 | 0.972 | 0.756 | NO DETECTABLE REGRESSION |
| larson T=2 | 30/30 | 41483333 | 37915226 | 0.914 | 16526492 | 15176061 | 0.918 | 0.605 | NO DETECTABLE REGRESSION |
| mstress T=2 | 30/30 | 25237192 | 23492986.5 | 0.931 | 9207086 | 8637628 | 0.938 | 1.000 | NO DETECTABLE REGRESSION |
| larson T=4 | 30/30 | 59515165 | 57309211.5 | 0.963 | 17973982 | 18738297 | 1.043 | 0.976 | NO DETECTABLE REGRESSION |
| mstress T=4 | 30/30 | 40877362.5 | 36301271.5 | 0.888 | 10872136 | 10029587 | 0.923 | 0.848 | NO DETECTABLE REGRESSION |
| larson T=8 | 30/30 | 132069907.5 | 119988998.5 | 0.909 | 40056994 | 34836003 | 0.870 | 0.690 | NO DETECTABLE REGRESSION |
| mstress T=8 | 30/30 | 82539763.5 | 89434280.5 | 1.084 | 20586488 | 17099654 | 0.831 | 0.929 | NO DETECTABLE REGRESSION |

detected regressions: 0 of 8
```

### 9.2 bench-table `global_alloc_churn/*/1024B` (`_raw_ph6b_bt_rerun_quiet.log`, 60+60 прогонов в двух подряд сериях 20+40)

```text
# Ph6b bench-table re-measure (quiet machine): global_alloc_churn/*/1024B, 60 alternating runs per side
r = ns[SeferAlloc] / ns[mimalloc] within one run (numerator Sefer, denominator mimalloc); statistic = median over runs; C/B = med(r_C) / med(r_B)

| side | n | med Sefer ns | med mimalloc ns | med r | min r | max r |
|---|---:|---:|---:|---:|---:|---:|
| B | 60 | 24217 | 137345 | 0.2046 | 0.0904 | 0.5692 |
| C | 60 | 28015 | 137910 | 0.2147 | 0.0872 | 0.5504 |

C/B = 0.2147 / 0.2046 = 1.0494 | Mann-Whitney p = 0.725 | bootstrap 95% CI of C/B (10000 resamples, seeded) = [0.855, 1.269] | limit max-ratio <= 1.10 -> NO DETECTABLE REGRESSION
```

Замечание о промежуточных результатах (не скрываю): первая серия 20+20 дала C/B = 1.2252 (p = 0.291) — тот же сдвиг, что и исходные 5+5 (1.2405); после добора до 60+60 медианы сошлись (1.0494, p = 0.725). Per-run r на этой ячейке пробегает 0.087…0.569 (6.5×), так что 5–20 прогонов недостаточно; вывод «регрессии нет» опирается на 60+60, а не на красивую подвыборку.

Ограничения интерпретации: (а) отсутствие обнаруженной регрессии ≠ доказанное отсутствие: bootstrap 95% CI для C/B на `churn/1024B` = [0.855, 1.269] (см. вывод скрипта), т. е. эффект до +27% wall-clock этим хостом не исключается; (б) детерминированная ось (iai, §3.1) показывает Ir −3.8…−32% на hot/refill/flush/realloc путях и 0.000% на контрольных mimalloc-плечах; (в) редакция приватных путей: в закоммиченных `_raw_ph6b_*.log` и identity-файле префиксы рабочего каталога заменены на `<repo>`, домашний каталог WSL — на `$HOME`; числа и структура логов не менялись (судья `ph6b_cost_ab_table.mjs` после редакции даёт побайтно тот же вывод).
