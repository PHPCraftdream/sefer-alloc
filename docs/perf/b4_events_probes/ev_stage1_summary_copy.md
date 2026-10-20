# b4-events — этап 1: разметка стоимости событий refill/flush (callgrind, T=1)

Статус: промежуточный результат этапа 1 (данные), НЕ финальный отчёт.
Пробы/скрипты: `docs/perf/b4_events_probes/` (копии `.rush/wsl/ev_prof_win.sh`,
`.rush/wsl/ev_prof.sh`, `.rush/wsl/ev_fix.sh`, `ev_larson_probe.rs`).
Raw-логи: `docs/perf/_raw_b4_events_ev-1-mstress-summary.log`,
`_raw_b4_events_ev-2-mstress-auto-100k.log`, `_raw_b4_events_ev-3-larson-auto-100k.log`.
Метод: worktree-снапшот → `examples/ev_larson_probe` (production, release,
line-tables), callgrind `--toggle-collect` по именам функций события,
маржинально 50k→100k; полный прогон с callgraph → `calls=`-счётчики событий;
`--cache-sim=yes` на 100k; построчная разметка `--auto=yes` (стоимости — за
прогон 100k шагов). Символы сверены с кодом: `refill_magazine_slow`
(`src/registry/heap_core/alloc/hot.rs:665`), `finish_magazine_refill`
(`hot.rs:66`), `try_drain_freelist_batch`
(`src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:535`),
`flush_class_internal`
(`src/alloc_core/small/alloc_core_small_magazine.rs:497`),
`flush_run`, `find_segment_with_free_impl`, `batch_drain_from_class_cursor`,
`try_carve_batch`/`carve_batch`, `reserve_small_segment`.

## 1. Ir/событие (сверка с эталоном 1587 / 456)

| величина | larson T=1 | mstress T=1 | статус |
|---|---|---|---|
| refill, вклад в Ir/step (`--toggle-collect *refill_magazine_slow*`) | 14 | 99 | ИЗМЕРЕНО |
| refill, событий/step (`calls=` в 100k-трассе) | 912/100k = 0.00912 | 1330/100k = 0.0133 | ИЗМЕРЕНО |
| refill, Ir/событие | **1535** | **7444** | ОЦЕНКА (деление двух ИЗМЕРЕНО) |
| flush, вклад в Ir/step (`--toggle-collect *flush_class_internal*`) | 7 | 55 | ИЗМЕРЕНО |
| flush, событий/step (`calls=` `flush_class_internal`) | 383/100k = 0.00383 | 497/100k = 0.00497 | ИЗМЕРЕНО |
| flush, Ir/событие | **1828** | **11066** | ОЦЕНКА (деление) |

- Эталон refill 1587 Ir (вычислен в B4_JUDGES_LARSON.md для larson при 0.00882
  событий/step) подтверждается: здесь 0.00912 × 1535 ≈ те же 14 Ir/step —
  воспроизводится (±3%, другое дерево worktree). ИЗМЕРЕНО+ОЦЕНКА.
- Эталон flush 456 Ir получен делением 8 Ir/step на частоту 0.01756/step.
  Прямой счётчик вызовов `flush_class_internal` даёт 0.00383/step — в 4.6×
  реже, откуда Ir/событие ≈ 1828, не 456. Расхождение — в знаменателе: судья
  считал «overflow-события» (частота 0.01756, блоков 0.14082/step ≈ 8
  блоков/событие), а один вызов `flush_class_internal` несёт НЕСКОЛЬКО
  overflow-событий (сегментных run'ов) — см. §4. Требует сверки на этапе 2.
  ИЗМЕРЕНО (частоты и вклады) + ОЦЕНКА (Ir/событие).
- `refill_magazine_slow_virgin`, `finish_magazine_refill`, `flush_run`,
  `batch_drain_from_class_cursor`, `reserve_small_segment` — NOMATCH в
  toggle-прогонах: инлайнены в точку вызова (их стоимость уже внутри
  `refill_magazine_slow`/`flush_class_internal`), отдельного вклада нет.
  ИЗМЕРЕНО (отсутствие самостоятельных копий).

## 2. Вклад фаз внутри события (marжинальные Ir/step, toggle)

| фаза (функция) | larson Ir/step | mstress Ir/step | статус |
|---|---|---|---|
| всё событие refill (`refill_magazine_slow`, вкл. инлайны) | 14 | 99 | ИЗМЕРЕНО |
| из них free-list drain (`try_drain_freelist_batch`, отдельная копия) | 10 | 77 | ИЗМЕРЕНО¹ |
| discovery (`find_segment_with_free_impl`, отдельная копия) | 0 | 0 | ИЗМЕРЕНО² |
| carve (`try_carve_batch`, отдельная копия) | 0 | 0 | ИЗМЕРЕНО² |
| cursor (`batch_drain_from_class_cursor`) | NOMATCH | NOMATCH | ИЗМЕРЕНО (инлайн) |
| всё событие flush (`flush_class_internal`) | 7 | 55 | ИЗМЕРЕНО |

¹ Больше, чем доля от refill (10 из 14 larson), при том что вызовов drain
(530/100k) МЕНЬШЕ, чем refill-событий (912/100k) → Ir/вызов drain (≈1887)
формально превышает Ir/событие refill (≈1535). Атрибуционная ловушка:
`try_drain_freelist_batch` — `#[inline]`, его тело в основном инлайнено в цикл
`refill_class_bump_internal` (стоимость учтена в refill-toggle), а отдельная
(неинлайненная) копия вызывается ещё и ВНЕ refill. Поэлементное сложение фаз
некорректно — только refill/flush-границы валидны.
² Отдельные (неинлайненные) копии discovery и carve дают 0 маржинального Ir —
их фактическая работа инлайнена в `refill_class_bump_internal` и учтена внутри
refill-вклада. Отдельно «доля discovery/carve в Ir события» из этих прогонов
НЕ извлекается; извлекается только доля вызовов (§3).

## 3. Распределение вызовов (100k-трасса, полный прогон)

| функция | larson | mstress | статус |
|---|---|---|---|
| `refill_magazine_slow` (события refill) | 912 | 1330 | ИЗМЕРЕНО |
| `flush_class_internal` (события flush) | 383 | 497 | ИЗМЕРЕНО |
| `try_drain_freelist_batch` (отдельная копия) | 530 | 530 | ИЗМЕРЕНО |
| `try_carve_batch` (отдельная копия) | 864 | 864 | ИЗМЕРЕНО |
| `find_segment_with_free_impl` (discovery) | 93 | 93 | ИЗМЕРЕНО |
| `batch_drain_from_class_cursor` | 0 | 0 | ИЗМЕРЕНО |
| `reserve_small_segment` | 0 | 0 | ИЗМЕРЕНО |

- discovery реально заходит в scan редко: 93 на 100k при 912–1330 refill
  (≈7–10% событий trial'ят discovery; в остальных free-list/cursor на текущем
  сегменте срабатывают). ИЗМЕРЕНО (вызовы) + ОЦЕНКА (доля).
- Carve в refill идёт пакетно (`try_carve_batch` — один вызов доливает остаток
  run'а), reserve_small_segment внутри refill не вызывается ни разу.
- Идентичность счётчиков 530/864/93 в ОБОИХ ворклоадах указывает, что заметная
  часть этих вызовов — init/teardown харнесса, а не стационара; разделить
  без счётчиков в коде (запрещено) нельзя — ограничение метода.

## 4. Размер события (блоков на событие)

- Размер refill задан таблицей `FREE_PARK_CAP[c]` = `refill_n_for_class(...)`,
  кламп 1..=16 (`src/registry/heap_core/state/tcache.rs:184`) → refill в этих
  трассах = до 16 блоков, из них 1 выдаётся, 15 маркируются MAG.
  ИЗМЕРЕНО (код) + ОЦЕНКА (фактический средний n может быть < cap при
  частичном drain).
- larson: судейские refill_blocks 0.14082/step ÷ 0.00912 событий/step ≈ 15.4
  блока/событие — совпадает с cap 16. ОЦЕНКА (чужие счётчики + наши события).
- Отсюда Ir/блок: larson refill ≈ 1535/15.4 ≈ **100 Ir/блок**; mstress refill ≈
  7444/15.4 ≈ **483 Ir/блок** (если mstress refills тоже полного размера) —
  mstress-событие в ~5× дороже и за блок: у него в 4.6× чаще уходят в
  discovery/carve-доминируемый refill_class_bump (1330 против 912 событий на
  те же 530 drain-вызовов). ОЦЕНКА.
- flush: один вызов `flush_class_internal` = группа same-segment run'ов
  (`flush_run` на каждый run) — поэтому «событие вызова» (383/100k) крупнее
  «события overflow» судьи (1756/100k) и Ir/вызов (≈1828) больше эталонных
  456 Ir/«overflow». Для этапа 2: сравнивать надо Ir/run, а не Ir/вызов.
  ИЗМЕРЕНО (вызовы) + ОЦЕНКА (интерпретация).

## 5. Построчная разметка (стоимости за прогон 100k, `--auto=yes`; ИЗМЕРЕНО)

`try_drain_freelist_batch` (`alloc_core_small_impl.rs`), mstress / larson:
- probe-pass (цикл `while probed…`, строки 553–606): 410k / 30k Ir
- commit-pass (цикл `while k…`, строки 607–633): 1.72M / 154k Ir
  → на проход commit приходится ~4× больше Ir, чем на probe (повторное чтение
  next + `mark_alloc` + `run.commit` + store в out на каждый блок);
  доминирующая строка — запись нового `head_off` (810k mstress).

MAG-маркировка (`finish_magazine_refill`, `hot.rs:66–88`):
- 205k / 20k Ir за 100k (цикл `for &p in slots[0..new_cnt]` = 147k mstress).

`flush_class_internal` + `flush_run` (`alloc_core_small_magazine.rs:497+`):
- run-детекция (два вложенных `while` по `blocks`): ~0.82M / 0.12M Ir
- тело `flush_run` (span-гвард 307k, `bm.is_free` 205k, приём блоков ~0.4M):
  ~1.0M / 0.15M Ir (mstress / larson).

Все построчные числа — ИЗМЕРЕНО (raw: `_raw_b4_events_ev-2/3-*-auto-100k.log`).

## 6. Cache-sim (полный прогон 100k, весь процесс; ИЗМЕРЕНО)

| метрика | larson | mstress |
|---|---|---|
| Ir | 31,661,760 | 43,298,241 |
| D1mr | 70,298 | 132,930 |
| D1mw | 96,219 | 215,906 |
| ILmr | 3,007 | 3,018 |

Промахи в целом редки (≈1.7 D1-промаха/шаг на весь процесс, включая харнесс);
изолировать промахи цепочки next без toggle×cache-sim по базовым блокам в этом
этапе не делалось — ограничение (dep-load `Node::read_next` в probe- И
commit-проходах читает каждую строку кэша дважды, что по Ir-разметке
подтверждается удвоением стоимости чтения next: 809k на строку `head_off`
против 101k на соседние строки прохода). Последнее — ОЦЕНКА (косвенная).

## 7. Аномалии и ограничения

1. Ir/событие — всюду частное двух ИЗМЕРЕНО-величин (вклад Ir/step ÷ вызовы),
   т.е. ОЦЕНКА; точечных счётчиков на событие в коде нет (править src запрещено).
2. Расхождение с эталоном flush 456 Ir: знаменатель судьи (0.01756/step) —
   «overflow-события», не вызовы `flush_class_internal` (0.00383/step).
3. Inline-атрибуция: фазы (drain/discovery/carve/cursor) не складываются в
   refill-toggle; отдельные копии discovery/carve дают 0 Ir/step.
4. Счётчики 530/864/93 одинаковы в обоих ворклоадах → доля harness-init в них
   не отделена.
5. mstress refill 7444 Ir/событие против larson 1535 — главное новое число
   этапа: частота refill mstress в 1.46× выше larson, а вклад в 7× выше; т.е.
   само событие в mstress в ~4.8× дороже (ИЗМЕРЕНО компоненты, ОЦЕНКА
   частное). Гипотеза для этапа 2: в mstress refill чаще проходит через
   discovery/carve-ветку `refill_class_bump_internal` (менее плотный free-list),
   а не по короткому free-list-пути.
6. Полные маржинальные тоталы: larson 302 Ir/step, mstress 422 Ir/step
   (ИЗМЕРЕНО; дерево b4-events, не b4-judges — прямое сравнение с 387 судьи
   некорректно).
