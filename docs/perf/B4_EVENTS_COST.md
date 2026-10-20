# B4 EVENTS COST — стоимость событий refill/flush: анализ и ранжированные сокращения

База: `897463e6` (ветка `b4-events`, worktree `b4-events`).
`git diff | sha256sum` на момент отчёта: `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` (пустой дифф — правок src/tests/benches/Cargo.toml нет).
Дата: 2026-10-20. Этап 2 (анализ); измерения — этап 1, raw-логи:

- `docs/perf/_raw_b4_events_ev-1-mstress-summary.log` (mstress: toggle/calls/cache)
- `docs/perf/_raw_b4_events_ev-2-mstress-auto-100k.log.gz` (mstress: `--auto=yes` построчно)
- `docs/perf/_raw_b4_events_ev-3-larson-auto-100k.log.gz` (larson: toggle + построчно)

Метки: **ИЗМЕРЕНО** — распарсено из raw-лога (вывод скрипта `scripts/b4_events_table.mjs`);
**ОЦЕНКА** — частное/произведение измеренных величин или доля построчной статьи (диапазон).
Все таблицы ниже — вывод `node scripts/b4_events_table.mjs`; machine-readable сводка —
`docs/perf/B4_EVENTS_COST_summary.csv` (пишется тем же скриптом, `--csv`).

## Метод

- Проба: `examples/ev_larson_probe` (production-фичи, release, line-tables), larson/mstress T=1,
  маржинально 50k→100k шагов (вклад = разность полных прогонов, делённая на 50k → Ir/step).
- Вклад события: callgrind `--toggle-collect` по имени функции события (включая инлайны/callees).
- Частота события: `calls=`-счётчики из полного callgraph-прогона на 100k.
- Построчная разметка: `--auto=yes` на 100k, стоимости за весь прогон (не за шаг).
- Cache-sim: полный прогон 100k, весь процесс (харнесс включён).
- Символы сверены с кодом: `refill_magazine_slow` (`src/registry/heap_core/alloc/hot.rs:665`),
  `finish_magazine_refill` (`hot.rs:66`), `try_drain_freelist_batch`
  (`src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:535`),
  `flush_class_internal` (`src/alloc_core/small/alloc_core_small_magazine.rs:497`), `flush_run`.

## Результаты (вывод `node scripts/b4_events_table.mjs`)

## Таблица 1 — статья события → Ir/событие → Ir/step (mstress/larson, T=1)

| статья | Ir/событие (larson) | Ir/событие (mstress) | Ir/step (larson) | Ir/step (mstress) | статус |
|---|---|---|---|---|---|
| refill (refill_magazine_slow, вкл. инлайны) | 1,535 | 7,444 | 14 | 99 | Ir/step ИЗМЕРЕНО (toggle); Ir/событие ОЦЕНКА (÷вызовы) |
| flush (flush_class_internal, вкл. flush_run) | 1,828 | 11,066 | 7 | 55 | Ir/step ИЗМЕРЕНО (toggle); Ir/событие ОЦЕНКА (÷вызовы) |
| refill, на блок (÷15.4 блока/событие) | 100 | 483 | — | — | ОЦЕНКА |
| полный маржинальный Ir/step (для сравнения) | — | — | 302 | 422 | ИЗМЕРЕНО |

Частоты событий (вызовов/100k шагов, ИЗМЕРЕНО `calls=`):

| функция | larson | mstress |
|---|---|---|
| refill_magazine_slow | 912 | 1330 |
| flush_class_internal | 383 | 497 |
| try_drain_freelist_batch (отд. копия) | 530 | 530 |
| try_carve_batch (отд. копия) | 864 | 864 |
| find_segment_with_free_impl | 93 | 93 |

## Таблица 2 — построчная разбивка (Ir за прогон 100k шагов, --auto=yes, ИЗМЕРЕНО)

| строка кода | larson | mstress |
|---|---|---|
| drain: probe-цикл (while probed…) | 26,580 | 207,477 |
| drain: commit-цикл (while k…) | 50,699 | 392,285 |
| drain: перезапись head_off (повторное чтение next) | 104,576 | 809,792 |
| flush: run-детекция (while run_end…) | 50,871 | 358,080 |
| flush_run: span-гвард | 43,623 | 307,020 |
| flush_run: bm.is_free(off) | 29,082 | 204,680 |
| MAG-маркировка n−1 (finish_magazine_refill, hot.rs:81–87) | 20,000 | 205,000 |
| flush: run-детекция (суммарно, §5 этапа 1) | 120,000 | 820,000 |
| flush_run: тело (span+is_free+приём, §5 этапа 1) | 150,000 | 1,000,000 |

## Таблица 3 — cache-sim (полный прогон 100k, весь процесс, ИЗМЕРЕНО)

| метрика | larson | mstress |
|---|---|---|
| Ir | 31,661,760 | 43,298,241 |
| D1mr | 70,298 | 132,930 |
| D1mw | 96,219 | 215,906 |
| ILmr | 3,007 | 3,018 |

## Таблица 4 — ранжированный список сокращений: ΔIr/step (ОЦЕНКА из построчных данных)

| # | сокращение | ΔIr/step larson | ΔIr/step mstress | статус |
|---|---|---|---|---|
| R1 | слияние probe/commit проходов drain (убрать повторное чтение next и перезапись head_off) | −0.5…−0.8 | −4…−6.5 | ОЦЕНКА (доля построчной статьи) |
| R2 | hoist сегмента в MAG-маркировке (hot.rs:81–87: base/Meta/bitmap из цикла) | −0.1…−0.2 | −1.2…−1.7 | ОЦЕНКА (доля построчной статьи) |
| R3 | соседство (q == prev+block_size) вместо segment_base_of_ptr в run-детекции flush | −0.3…−0.4 | −1.8…−2.9 | ОЦЕНКА (доля построчной статьи) |
| R4 | X-leafskip: условный ClassLeaves-store (store только при смене значения) | −0.3…−0.7 | −2…−5 | ОЦЕНКА (доля построчной статьи) |

## Что дорого и почему

### 1. Двойной проход drain (probe + commit) — доминирующая статья refill

`try_drain_freelist_batch` (`alloc_core_small_impl.rs:553–633`) проходит одну и ту же
цепочку free-list дважды:

- **probe-pass** (`:553–606`, ИЗМЕРЕНО 410k mstress / 30k larson за 100k): валидация
  `valid_free_node`, `run.prepare_for` (promote листв только при смене), чтение
  `Node::read_next`, continuation-check, loop-detection (hardened).
- **commit-pass** (`:607–633`, ИЗМЕРЕНО 1.72M mstress / 154k larson): **повторное**
  `Node::read_next` того же блока, `bm.mark_alloc`, `run.commit`, `out[k] = block_ptr`,
  и перевычисление `head_off`.

Соотношение 4:1 (1.72M/410k). Причина не в «лишней валидации», а в том, что commit-pass
**заново читает каждое `next`** и перевычисляет offset: доминирующая строка —
`head_off = if next.is_null() …` (ИЗМЕРЕНО **809,792** mstress / **104,576** larson на 100k) —
это ~47 % commit-pass и ~38 % всего события refill mstress. Dependent load по цепочке
`next` выполняется дважды на блок: probe уже доказал корректность звена, commit повторяет
чтение, чтобы не хранить буфер результатов (буфер probe **уже отклонён** —
`B4_HOT2_OWN_THREAD.md`: w_alloc +1, larson/mstress хуже; LLVM и так держит значения
в регистрах в однопроходном варианте).

Cache-sim: промахи в целом редки (D1mr+D1mw ≈ 1.7/шаг larson, ≈ 3.5/шаг mstress,
ИЗМЕРЕНО, включая харнесс) — стоимость head_off-строки объясняется прежде всего
**удвоением зависимых загрузок** (каждая строка кэша цепочки читается дважды),
а не частотой промахов. Косвенная атрибуция — ОЦЕНКА.

### 2. ClassLeaves-store на каждый блок

`ClassLeaves::issue/issue_in` (`src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs:221–249`):
владелец **безусловно** пишет Release-store байта `class+1` на **каждый** выданный блок —
в т.ч. на все n−1 блоков, укладываемые в magazine при refill, хотя значение почти всегда
не меняется (байт после выдачи не обнуляется до конца жизни маршрута, `leaf_classes.rs:150–154`).
Это идея X-leafskip (`sol_ox.md` §5 п.8): `if leaf[g].load(Relaxed) != class+1 { store }`.
Через существующие структуры доступно без правки схемы данных — seam один (store-сайт).
На T=1 выгода только Ir-store'ы (ОЦЕНКА −2…−5 Ir/step mstress при доле 2–5 % refill-блока);
главная выгода — MT: убирает RFO на линиях листьев (x86 Acquire/Release — 0 Ir, ARM64 — `stlrb`).

### 3. MAG-маркировка n−1 (hot.rs:81–87)

`finish_magazine_refill` для каждого из n−1 magazine-блоков выполняет
`os::segment_base_of_ptr(p)` + `SegmentMeta::new(pbase)` + `magazine_bitmap().mark_magazine(poff)`.
ИЗМЕРЕНО 205k mstress / 20k larson за 100k (из них цикл `for &p in slots[0..new_cnt]` — 147k mstress).
При полном refill почти все n−1 блоков — соседние carve-блоки **одного сегмента**:
base/`SegmentMeta`/bitmap-хэндл можно вынести из цикла (hoist), оставив per-block только
`poff` и store. Не ослабление: те же store'ы в тот же момент.

### 4. Публикация/credit на блок vs пакет

Дренаж уже пакетируется там, где это безопасно: `set_head` — один раз, `add_live(k)` — один
раз (D1), `run.commit` — один Release store на блок (leaf promote только при смене класса).
Батчинг `bm.mark_alloc` невозможен без пессимизации (per-block M2-гварды способны отклонить
блок внутри run) — закрыто шагом 3 `B4_HOT2_OWN_THREAD.md`, не повторять. Остаточная
надбавка «per-block поверх пакета» — это ровно повторное чтение next (см. п.1), т.е. R1
покрывает и этот вопрос.

### 5. Flush run-детекция (0.82M Ir mstress / 0.12M larson — что там построчно)

`flush_class_internal` (`alloc_core_small_magazine.rs:497+`) группирует `blocks` в
same-segment run'ы:

- внешний `while i < blocks.len()` — 102k mstress (ИЗМЕРЕНО), в т.ч. `segment_base_of_ptr(ptr)` на голову run'а;
- внутренний `while run_end < blocks.len()` — ИЗМЕРЕНО **358,080** mstress / **50,871** larson:
  на каждый следующий блок снова `os::segment_base_of_ptr(q)` (маска + сравнение) и null-check.
  Это 44 % run-детекции; при соседних блоках magazine-батча равенство базы почти всегда
  истинно, но вычисляется полной маской адреса каждый раз.
- `recycled_bases[..recycled_n].contains(&candidate)` — ≤16 сравнений на run, незначимо (L-4 гвард).

Дальше `flush_run`: span-гвард 307k, `bm.is_free` 205k, приём блоков ~0.4M (ИЗМЕРЕНО, §5 этапа 1).
Per-block M2-гварды и `mark_free` — гарантирующие проверки; их батчинг отвергнут (R28-1/шаг 3 HOT2).

### 6. Discovery/carve — 0 маржинального Ir

Отдельные (неинлайненные) копии `find_segment_with_free_impl` (93 вызова/100k) и
`try_carve_batch` дают 0 Ir/step в toggle — фактическая работа инлайнена в
`refill_class_bump_internal`. Адресных сокращений здесь нет; «дорогой» refill mstress
(483 Ir/блок против 100 larson, ОЦЕНКА) объясняется смесью путей
(free-list drain vs carve-доминируемый), а не отдельной дорогой функцией.

## Ранжированный список сокращений (НЕ реализовано)

Формат: механизм → Δ-оценка → «не ослабление» → байт состояния (b4-sbyte) → конфликт →
как мерить → порог. Δ — ОЦЕНКА из построчных данных этапа 1 (диапазон = доля статьи,
которую механика реально устраняет).

### R1. Слияние probe/commit проходов drain — Δ larson −0.5…−0.8, mstress −4…−6.5 Ir/step (ОЦЕНКА)

- **Механизм:** `alloc_core_small_impl.rs:553–633` — один проход: для каждого блока
  валидация (`valid_free_node`/continuation), `run.prepare_for`, `Node::read_next` **один раз**,
  затем сразу `bm.mark_alloc` + `run.commit` + `out[k]`; `set_head`/`add_live(k)`/`publish_empty`
  — как сейчас, в конце. Устраняет повторное чтение next и перевычисление head_off
  (809,792 Ir/100k mstress) и второй цикл (392,285). Диапазон: 50–80 % head_off-строки +
  часть commit-цикла.
- **Не ослабление:** каждая проверка срабатывает в тот же момент с той же силой — порядок
  «validate → prepare → mutate» сохраняется per-block; C6 (prepare до мутаций) соблюдается,
  т.к. prepare_for по-прежнему предшествует mark_alloc/commit того же блока; OOM/ошибка
  валидации прерывает до мутации текущего блока (частично-закоммиченный префикс — новая
  семантика, требующая отдельного доказательства «head остаётся консистентным», см. риск).
- **Байт состояния (b4-sbyte):** НЕ снимает байт. `mark_alloc` в drain исчезнет вместе с
  битмапами при его замене — статья уменьшится сама; слияние проходов останется полезным
  (повторное чтение next никуда не денется).
- **Конфликт:** ВЫСОКИЙ — та же функция `try_drain_freelist_batch`, которую правит
  `b4-sbyte`. Реализовывать только после/по согласованию, поверх его ветки.
- **Риск:** commit в том же проходе означает «мутация до полной валидации хвоста цепочки» —
  текущий probe-pass гарантирует валидность всех k блоков до первой мутации. Требует
  явного решения владельца + Kani/тест на частичную валидацию.
- **Мерить:** `bash .rush/wsl/ev_prof_win.sh b4-events ev-r1-toggle` (toggle refill+drain)
  и полный `--auto=yes` 100k (метка `ev-r1-auto`); сверить head_off-строку.
- **Порог:** оставить при −3 Ir/step mstress и ≥0 larson; иначе откат.

### R2. Hoist сегмента в MAG-маркировке — Δ larson −0.1…−0.2, mstress −1.2…−1.7 Ir/step (ОЦЕНКА)

- **Механизм:** `hot.rs:81–87` — вынести `segment_base_of_ptr`/`SegmentMeta::new`/
  `magazine_bitmap()` из цикла `for &p in slots[0..new_cnt]`, вычисляя base один раз
  (блоки одного refill почти всегда одного сегмента; при смене base — пересчитать).
  Устраняет 60–85 % статьи 205k/100k mstress.
- **Не ослабление:** те же `mark_magazine(poff)` в те же моменты; меняется только способ
  получения base (кэш в рамках одного вызова, single-writer).
- **Байт состояния:** если `b4-sbyte` заменяет magazine-битмап байтом состояния, статья
  исчезнет в его реализации (взаимоисключающая выгода); сам байт это сокращение не
  продвигает.
- **Конфликт:** СРЕДНИЙ — та же функция `finish_magazine_refill`; если sbyte переписывает
  маркировку, элемент снимется его работой.
- **Мерить:** `ev_prof_win.sh b4-events ev-r2-toggle` + auto 100k (`ev-r2-auto`).
- **Порог:** оставить при ≥ −1 Ir/step mstress без регресса larson; иначе откат.

### R3. Соседство вместо `segment_base_of_ptr` в run-детекции flush — Δ larson −0.3…−0.4, mstress −1.8…−2.9 Ir/step (ОЦЕНКА)

- **Механизм:** `alloc_core_small_magazine.rs` внутренний `while run_end` — если
  `q == prev + block_size(class_idx)` (или `q > prev` и та же 4-МиБ маска быстрым путём),
  тот же сегмент без полной маски/сравнения; иначе — старый путь. Устраняет 50–80 %
  статьи 358,080 Ir/100k mstress.
- **Не ослабление:** детекция та же: равенство `base(q) == candidate` доказывается
  арифметически для соседних блоков; все непоседние случаи уходят в старый путь.
  Null-терминация run'а сохраняется.
- **Байт состояния:** не трогает refill/flush bitmap-работу sbyte — конфликт низкий
  (строка детекции не относится к битмапам/байту).
- **Конфликт:** НИЗКИЙ — строки рядом с его правками, но другая функция (`flush_class_internal`).
- **Мерить:** `ev_prof_win.sh b4-events ev-r3-toggle` (toggle flush) + auto 100k (`ev-r3-auto`).
- **Порог:** оставить при ≥ −1 Ir/step mstress и ≥ −0.3 larson; иначе откат.

### R4. X-leafskip: условный ClassLeaves-store — Δ T=1 larson −0.3…−0.7, mstress −2…−5 Ir/step (ОЦЕНКА); главный эффект MT

- **Механизм:** `leaf_classes.rs:221–249` — `if leaf[g].load(Relaxed) != class+1 { store(class+1, Release) }`
  в `issue`/`issue_in`. Писатель один (владелец), Relaxed-чтение своего последнего store
  корректно (sol_ox §5 п.8: HB сохраняется более ранним store того же значения).
- **Не ослабление:** producer в P1 видит то же значение во всех исполнениях; валидный
  publish не теряется (мутант «пропуск при отличающемся значении» должен краснеть в Loom).
- **Байт состояния:** НЕ снимает и не снимается байтом — листья ClassLeaves не входят в
  замену «битмапы → байт» у `b4-sbyte`. Конфликт НИЗКИЙ (другой файл), но (в)-дизайн
  меняет publish-протокол вокруг выдачи — сверить с его ADR перед реализацией.
- **Мерить:** T=1 `ev_prof_win.sh b4-events ev-r4-toggle` (Δ ≤ +0.5 Ir/step); счётчик
  `leaf_store_performed/issued` ≈ 0 в стационаре (нужен bench-internals-счётчик — отдельно);
  MT wall-clock CI T=8 — главный критерий.
- **Порог (по sol_ox):** MT-медиана ≥ +2 % при T=8 — оставить; MT нейтрально — откатить.

### Не включено (проверено, ниже порога / закрыто)

- `publish_empty` directory — 467 Ir/100k mstress (ИЗМЕРЕНО) — ниже порога шума.
- `recycled_bases.contains` (L-4) — ≤16 сравнений на вызов, незначимо.
- Публикация/credit: уже пакетные (`set_head`, `add_live(k)`, `run.commit` = 1 Release store);
  батчинг `mark_alloc`/per-block flush_class отвергнут (R28-1, шаг 3 HOT2).
- **Не повторять:** буфер результатов probe (шаг 4 HOT2, хуже на всех метриках),
  однопроходный refill поверх SV ≈ 0, FLUSH_N/cap свип R25-3, per-block flush_class R28-1.

## Ограничения

1. Только callgrind (Ir), детерминированно; wall-clock и MT-когерентность не измерялись —
   R4 (leafskip) оценивается в MT только по CI wall-clock.
2. Ir/событие — всюду частное двух ИЗМЕРЕНО-величин (вклад/вызовы) → ОЦЕНКА; точечных
   счётчиков на событие в коде нет (правки src запрещены).
3. Маржинальная методика 50k→100k: вклады — средние по вторым 50k; распределение внутри
   события (drain vs carve mstress) точно не разделено.
4. Inline-атрибуция: `try_drain_freelist_batch`, `finish_magazine_refill`, `flush_run` —
   `#[inline]`; построчные стоимости — за прогон, фазы не складываются поэлементно.
5. Примесь init/teardown: счётчики 530/864/93 одинаковы в обоих ворклоадах → заметная доля
   этих вызовов — харнесс-init; на refill/flush-частоты (912/1330, 383/497) не влияет,
   но отдельные копии drain/carve могут включать init-Ir.
6. Эталонные 456 Ir flush судьи — знаменатель «overflow-событий» (0.01756/step), а не
   вызовов `flush_class_internal` (0.00383/step): один вызов несёт несколько run'ов.
   Корректное сравнение — Ir/run, не Ir/вызов.
7. Cache-sim — весь процесс; промахи цепочки next изолированы косвенно (ОЦЕНКА).
