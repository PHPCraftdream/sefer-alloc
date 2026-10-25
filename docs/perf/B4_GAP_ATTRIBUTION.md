# B4 gap attribution v2 — sefer vs mimalloc (toggle-window callgrind)

**Измерение, без изменения runtime.** Отчёт атрибутирует разрыв времени sefer vs
mimalloc по данным уже выполненных измерений; runtime-код не менялся
(`git diff` в рабочем дереве пуст на момент замеров — см. Identity).

## Методика (v2 — toggle-окно)

- **Проба:** `examples/gp_kernel_probe.rs` — include! ядра
  `examples/_shared/b4_interleave_core.rs`, шим-фейс формы `__rust_alloc`
  (слоты 0/2), без таймеров; оракул корректности
  `expected_block_calls` / `simulate_portion_calls`.
- **Toggle-окно:** callgrind `--toggle-collect` только внутри функции ядра
  (`gp_kernel_probe::kernel_pair | kernel_bulk | kernel_churn | kernel_larson`);
  `--collect-atstart=no`. Окно проверено непустым на смоке
  (5.91M / 6.53M Ir на p40-смоке) — в отличие от v1, полно-процессного окна,
  куда попадали прогрев и отчёт пробы.
- **Сиды/прогрев судьё-точны:** через `judge_warmup_kernel` / `judge_block_seed`
  (`examples/_shared/b4_interleave_core.rs:315–357`) — те же сиды, что у судьи J1′.
- **Две точки p40/p80**, маржинал = (p80 − p40) на op, `ops_delta` из
  шим-счётчиков `SHIM_CALLS`. Warmup выполняется внутри collected-окна, но
  ИДЕНТИЧЕН в p40/p80 → в маржинале сокращается.
- **JSON/оракул — вне окна** (не входят в счётчики).
- **EstCycles = Ir + 10·(D1mr+D1mw+I1mr) + 100·(LLmr+LLmw+ILmr) + 10·Bcm** —
  модель стоимости callgrind, НЕ железо. Пересчитывается скриптом из колонок
  (первичный расчёт, не сверка).
- **Критерии классификации** (константы в
  `scripts/b4_gap_attribution_table.mjs`): time_ratio = exp(ab_median) судьи
  (mi/sefer); ir_ratio = Ir/op(mi)/Ir/op(sefer); est_ratio = EstCycles(mi/sefer);
  класс «instructions»: |ln(est_ratio) − ln(time_ratio)| ≤ 0.1 И
  |ln(ir_ratio)| ≥ 0.7·|ln(time_ratio)|; класс «memory/latency»: те же ≤ 0.1 И
  < 0.7·|ln(time_ratio)|; иначе «not_explained». Guard'ы: |ln(time_ratio)| < 1e-9
  или ratios ≤ 0 → ошибка, не классификация.
- **Скрипт таблиц (один источник таблиц, assert'ы арифметики):**
  `scripts/b4_gap_attribution_table.mjs` v2. Assert'ы: 9 ядер по разу во всех
  CSV; `ops_delta` согласован между marginals/events/calls;
  события `delta_* = per_op_* × ops_delta` (допуск округления 6 знаков);
  calls: `calls_delta = calls80 − calls40`, `calls_per_op × ops = calls_delta`;
  суммы per-function self-Ir против маржинального Ir — < 2 % dev на всех 18
  (kernel, arm) парах (макс. 1.55 %).

## Честная история итераций

- **v1 (полно-процессное окно) — ОТКЛОНЕНА ревью.** Окно включало прогрев и
  отчёт пробы; сиды пробы отличались от судьё-точных. Данные реальны и
  сохранены как `docs/perf/b4_gap/v1_fullprocess_*` (методика устарела).
- **v2 (этот отчёт)** — toggle-окно внутри kernel_*, судьё-точные сиды/прогрев,
  p40/p80 маржиналы.
- **Пороги классификации (0.1 / 0.7) заданы владельцем/оркестратором ДО
  замеров; сам скрипт v2 написан ПОСЛЕ первых (полно-процессных) замеров
  прошлой итерации** — это честная история, приведена также в шапке скрипта.

## Таблица (дословный вывод `scripts/b4_gap_attribution_table.mjs` v2)

Все числа — ИЗМЕРЕНО (кроме помеченных ОЦЕНКА описаний функций).

<!-- TABLE_BEGIN -->

| kernel | time_ratio (mi/sefer) | Ir_ratio | EstCycles_ratio | Ir/op sefer | Ir/op mi | D1mr+D1mw/op | LLmr+LLmw/op | Bcm/op | events/op | class |
|---|---|---|---|---|---|---|---|---|---|---|
| pair16 | 0.893 | 0.647 | 0.648 | 79.53 | 51.44 | 0.000 / 0.000 | 0.000 / 0.000 | 0.00 / 0.01 | все нули (все delta_* = 0 по данным) | not_explained (не объяснено моделью) |
| pair64 | 0.912 | 0.663 | 0.666 | 79.53 | 52.69 | 0.000 / 0.000 | 0.000 / 0.000 | 0.00 / 0.02 | все нули (все delta_* = 0 по данным) | not_explained (не объяснено моделью) |
| larson | 0.911 | 0.686 | 0.700 | 104.34 | 71.63 | 0.805 / 0.625 | 0.000 / 0.000 | 0.08 / 0.14 | dbg_s1_refill_events=94 (0.005/op); dbg_s1_mag_byte_writes=1504 (0.073/op) | not_explained (не объяснено моделью) |
| churn1024 | 0.792 | 3.933 | 3.911 | 84.91 | 333.97 | 0.467 / 0.440 | 0.000 / 0.000 | 0.01 / 1.24 | все нули (все delta_* = 0 по данным) | not_explained (не объяснено моделью) |
| churn256 | 0.694 | 1.018 | 1.055 | 84.65 | 86.14 | 0.218 / 0.228 | 0.000 / 0.000 | 0.00 / 0.32 | все нули (все delta_* = 0 по данным) | not_explained (не объяснено моделью) |
| bulk64 | 0.589 | 0.362 | 0.392 | 126.84 | 45.89 | 0.000 / 0.000 | 0.000 / 0.000 | 0.30 / 0.50 | dbg_s1_refill_events=960 (0.023/op); dbg_s1_mag_byte_writes=15360 (0.375/op) | not_explained (не объяснено моделью) |
| churn64 | 0.546 | 1.585 | 1.585 | 84.65 | 134.21 | 0.000 / 0.000 | 0.000 / 0.000 | 0.00 / 0.00 | все нули (все delta_* = 0 по данным) | not_explained (не объяснено моделью) |
| churn16 | 0.530 | 1.585 | 1.585 | 84.65 | 134.21 | 0.000 / 0.000 | 0.000 / 0.000 | 0.00 / 0.00 | все нули (все delta_* = 0 по данным) | not_explained (не объяснено моделью) |
| bulk16k | 0.428 | 0.353 | 0.426 | 144.34 | 50.96 | 1.206 / 1.179 | 0.000 / 0.000 | 0.34 / 0.53 | dbg_s1_refill_events=40920 (0.031/op); dbg_s1_mag_byte_writes=654720 (0.500/op) | instructions (инструкции) |

<!-- TABLE_END -->

Полные функциональные таблицы (все 9 ядер, sefer top-5) — дословно в
`docs/perf/B4_GAP_ATTRIBUTION_table.generated.md` (сгенерирован скриптом).

## События sefer: двойной источник, кросс-валидация

Refill-события измерены двумя независимыми источниками, которые СХОДЯТСЯ:

| kernel | callgrind calls= (`refill_magazine_slow`) | `dbg_s1_refill_events` |
|---|---|---|
| larson | 94 | 94 |
| bulk64 | 960 | 960 |
| bulk16k | 40920 | 40920 |

Это сильная кросс-валидация toggle-окна и шим-счётчиков (ИЗМЕРЕНО). На остальных
6 ядрах оба источника дают 0.

- **flush/drain:** наблюдателей bench-internals на `flush_class_internal` и
  `try_drain_freelist_batch` НЕТ; их частоты видны только из callgrind calls=
  (ИЗМЕРЕНО): bulk64 `flush_class_internal` 1920 вызовов за окно (0.047/op),
  bulk16k 81840 (0.062/op); larson `try_drain_freelist_batch` 95 (0.005/op).
- **Непокрытые пути (честно):** `try_drain_freelist_batch`
  (`<repo>/src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:525`),
  `flush_class_internal`
  (`<repo>/src/alloc_core/small/alloc_core_small_magazine.rs:565`),
  `drain_large_sidecar_ingress_hot_bounded`
  (`<repo>/src/alloc_core/alloc_core/sidecar_drain.rs:572`) — без собственных
  счётчиков; tcache/large-cache/config-conflict счётчики — все нули по данным на
  всех 9 ядрах (горячие пути batch-freelist их не задевают).

## Сравнение с v1 (Ir_ratio mi/sefer, оба ИЗМЕРЕНО в своих методиках)

| kernel | v1 (полно-процессное окно) | v2 (toggle-окно) |
|---|---|---|
| pair16 | 0.664 | 0.647 |
| pair64 | 0.679 | 0.663 |
| larson | 0.744 | 0.686 |
| churn1024 | 3.211 | 3.933 |
| churn256 | 1.036 | 1.018 |
| bulk64 | 0.387 | 0.362 |
| churn64 | 1.506 | 1.585 |
| churn16 | 1.506 | 1.585 |
| bulk16k | 0.370 | 0.353 |

Направления всех выводов совпадают; на toggle-окне Ir-разрывы чище (прогрев и
отчёт пробы больше не разбавляют окно): bulk Ir_ratio стал меньше (0.387→0.362,
0.370→0.353), churn1024 больше (3.211→3.933).

## Функциональная разбивка: ключевые строки (ИЗМЕРЕНО)

Доли считаются скриптом из Ir: числитель `self_ir_delta`, знаменатель —
маржинальный Ir ядра; полные таблицы с обоими числами — в generated-файле.

- **pair16/pair64 (идентично):** `sefer_a_dealloc` 51.55 %, `sefer_a_alloc`
  28.29 %, `kernel_pair` 11.98 %, libc `__memset_avx2_unaligned_erms` 8.17 % —
  окно почти целиком шим + ядро пробы; batch-функции аллокатора отсутствуют.
- **larson:** `sefer_a_dealloc` 39.46 %, `kernel_larson` 28.34 %,
  `sefer_a_alloc` 21.51 %, `try_drain_freelist_batch` 5.65 %,
  `flush_class_internal` 4.09 %.
- **churn1024/256/64/16:** `sefer_a_dealloc` 48.29–48.43 %, `sefer_a_alloc`
  26.50–26.58 %, `kernel_churn` 24.99–25.21 %; batch refill/flush в top-5
  отсутствуют. На churn64/16 в top-5 попадает `try_carve_batch` с малым
  ОТРИЦАТЕЛЬНЫМ delta (−1.55 %) — шум аннотации на почти пустом счётчике
  (ОЦЕНКА), не интерпретируется как экономия.
- **bulk64:** `sefer_a_dealloc` 33.03 %, `try_drain_freelist_batch` 20.84 %,
  `sefer_a_alloc` 17.54 %, `flush_class_internal` 17.28 %, `kernel_bulk` 8.05 %.
- **bulk16k:** `sefer_a_dealloc` 29.23 %, `try_drain_freelist_batch` 24.46 %,
  `flush_class_internal` 20.20 %, `sefer_a_alloc` 15.35 %, `kernel_bulk` 6.93 %.
  Пара drain/flush ≈ 44.7 % маржинального Ir.

## Ранжированный список механизмов разрыва

Порядок — по величине разрыва в пользу mi (меньше time_ratio = больше разрыв).
Отношения ИЗМЕРЕНО (судья + toggle-callgrind); интерпретации — ОЦЕНКА.

1. **bulk16k (time_ratio 0.428) — класс «instructions».** Ir_ratio 0.353,
   EstCycles_ratio 0.426 (ИЗМЕРЕНО). |ln(0.353)| = 1.041 ≥ 0.7·|ln(0.428)| =
   0.594 и |ln(0.426) − ln(0.428)| = 0.005 ≤ 0.1. Пара drain/flush ≈ 44.7 %
   маржинального Ir (ИЗМЕРЕНО) — главный кандидат на Ir-избыточность (ОЦЕНКА).
2. **bulk64 (time_ratio 0.589)** — Ir_ratio 0.362, EstCycles_ratio 0.392
   (ИЗМЕРЕНО), но |ln(est_ratio) − ln(time_ratio)| = 0.382 > 0.1 → формально
   «not_explained»: направление инструкционное, часть величины — нет (ОЦЕНКА).
   Тот же механизм: drain/flush ≈ 38.1 % маржинального Ir (ИЗМЕРЕНО).
3. **churn16/64 (time_ratio 0.530/0.546) — разрыв НЕ инструкционный.** У sefer
   Ir/op МЕНЬШЕ (Ir_ratio 1.585 — mi тратит в 1.6× больше инструкций, ИЗМЕРЕНО),
   при этом время у sefer больше; EstCycles модель противоречит времени
   (ИЗМЕРЕНО). Механизм — латентность/платформа, callgrind-счётчиками не виден
   (ОЦЕНКА).
4. **churn256 (0.694)** — Ir_ratio 1.018 ≈ паритет, EstCycles_ratio 1.055 ≈
   паритет при time_ratio 0.694 — разрыв целиком вне модели (ОЦЕНКА).
5. **churn1024 (0.792)** — Ir_ratio 3.933 против sefer (mi тратит ~3.9× больше
   инструкций, ИЗМЕРЕНО), Bcm 1.24/op у mi, но время ближе к паритету — модель
   резко противоречит времени; латентный механизм (ОЦЕНКА).
6. **pair16/pair64/larson (0.893/0.912/0.911)** — Ir_ratio 0.647–0.686
   перекрывает временной разрыв, но |ln(est_ratio) − ln(time_ratio)| = 0.211–0.263
   > 0.1 → «not_explained»: часть инструкционная, часть нет (ОЦЕНКА).
7. **Кэш-модель почти пустая:** D1mr+D1mw ≤ 1.206/op (максимум bulk16k sefer),
   LLmr+LLmw+ILmr = 0.000/op на всех ядрах (ИЗМЕРЕНО) — разрыв не виден на
   D1/LL-счётчиках.
8. **События:** refill подтверждён двойным источником (94/960/40920 — сходятся,
   ИЗМЕРЕНО); mag_byte_writes 1504/15360/654720 — только на refill-ядрах.

## Негативный тест оракула (--oracle-break)

Выполнен, rc=1, stderr дословно:

```
[oracle] FAIL: oracle-break (test-only inversion) block=3 portion=0 calls=1024 expected=1024 — correct run inverted-fails by design
```

## Явные ограничения

- callgrind — Linux/WSL: кодген и платформа отличаются от Windows-судьи.
- EstCycles — модель callgrind, НЕ железо (нет латентностей, TLB, store-buffer).
- Инлайнинг: self Ir шимов `sefer_a_alloc/dealloc` (22.5/41–42 Ir/op) — макрос
  `shim_set!` с `#[inline(never)]`; часть аллокатора инлайнится в шим/ядро —
  атрибуция «шим vs аллокатор» по функциям — ОЦЕНКА.
- Two-point линейность: маржинал p80 − p40 предполагает линейность счётчиков.
- Малые счётчики: отрицательные self-Ir delta (try_carve_batch на churn64/16,
  −1.55 %) — шум аннотации, не интерпретировать (ОЦЕНКА).
- Однопоточность: проба однопоточная; MT-поведение не покрыто.
- Суммы per-function против маржиналов: < 2 % dev на всех 18 парах
  (threshold=99 обрезает хвост) — ИЗМЕРЕНО.
- Расщепление core после замеров: `examples/_shared/b4_interleave_core.rs` (999 строк) теперь `include!`-ит `examples/_shared/b4_judge_seed_contract.rs` (52 строки, pure move судьи-контракта сидов/прогрева). Это чистый перенос байт-идентичных items ПОСЛЕ callgrind-прогонов R2: identity в raw (`docs/perf/b4_gap/identity.txt`, DIFF_SHA256=6a6133c6…) относится к дереву НА МОМЕНТ ЗАМЕРОВ (pre-split); измеренному поведению расщепление не соответствует никакой разнице кода (pure move), но байтовое дерево изменилось — при воспроизведении замеров нужен pre-split checkout по identity-хэшу (ОЦЕНКА: pure move не влияет на кодген).

## Identity

- HEAD: `26108a2043f1c78fce2be7c447e61c85e0454a7f` (ветка `b4-gap`).
- DIFF_SHA256: `6a6133c626a75729ee97a54659581012b81bf245b6de42835abb8171f5e9dd86`;
  UNTRACKED_SHA256: `b91094ff604985ea6d2fbdb0b098a42f286433d8b24ddfa52cb980052c9b0366`
  (замеренные данные; идентичность зафиксирована до/во время R2,
  `docs/perf/b4_gap/identity.txt`).
- Манифест raw-данных: `docs/perf/b4_gap/MANIFEST.sha256` (11 файлов,
  `sha256sum -c` = OK, rc=0). Raw в репозитории:
  `identity.txt`, `gp_r2_marginals.csv`, `gp_r2_marginals_per_op.csv`,
  `gp_r2_sefer_events_per_op.csv`, `gp_r2_sefer_calls_per_op.csv`,
  `gp_r2_func_breakdown.csv` (v2, toggle-окно) и `v1_fullprocess_*`
  (отклонённая первая итерация, полно-процессное окно — данные реальны,
  методика устарела).
- Коллектор функциональной разбивки: `.rush/wsl/gp_r3_funcs.py`
  (callgrind_annotate по СУЩЕСТВУЮЩИМ toggle-.out, новых прогонов нет),
  identity передана аргументом.
