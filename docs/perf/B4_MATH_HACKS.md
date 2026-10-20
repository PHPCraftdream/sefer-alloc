# B4-math: арифметические микро-хаки hot-пути — 7 кандидатов, все NO-GO

Задача: попытка выжать последние Ir/op из мелкого пути alloc/free безопасными арифметическими приёмами (branchless-предикации, witness-поля, маскирование индексов, словная разметка). Инструменты: детерминированный callgrind через WSL-обёртки `prof_ao_win.sh` / `prof_win.sh` (маргинальный N/2N, T=1). Все 7 кандидатов — **NO-GO, откачены**; в `src/` принятых изменений нет, мержить нечего.

## 1. Идентичность

- База: commit `f93c1b52` (main, «Merge pull request #80 …ev1-reverify-runner»), ветка `b4-math`, worktree `worktrees/b4-math`, дерево чистое на старте.
- **Отклонение от протокола**: per-state `git diff | sha256sum` ДО замера не фиксировались. Каждый шаг после замера откатывался вручную к HEAD (`git show HEAD:<файл>` для сверки); в конце работы `git status` подтверждает чистое дерево против `f93c1b52`. Каждое состояние описано ниже (файлы, строки, суть правки) и воспроизводимо из этих diff-описаний.

## 2. Методика

- Критерий принятия: w_free или w_alloc ≥ 1 Ir/op лучше базы **и** ни larson, ни mstress T=1 не хуже предыдущего принятого шага.
- Замеры: `prof_ao_win.sh` (w_alloc / w_free, callgrind ao, маргинальный N/2N) и `prof_win.sh` (larson, mstress), T=1, Windows/WSL.
- Все числа в этом отчёте **ИЗМЕРЕНЫ** из сырых логов `docs/perf/_raw_b4_math_*.log` (7 состояний × 3 лога). Сводные таблицы строятся из сырых данных скриптом:

```text
$ node scripts/b4_math_table.mjs
```

Таблица (вывод скрипта):

| шаг       | w_alloc | w_free | larson | mstress | Δ w_alloc | Δ w_free | Δ larson | Δ mstress | вердикт      |
| --------- | ------- | ------ | ------ | ------- | --------- | -------- | -------- | --------- | ------------ |
| math-base | 63      | 115    | 319    | 494     | —         | —        | —        | —         | база         |
| math-h1   | 63      | 121    | 334    | 500     | 0         | +6       | +15      | +6        | NO-GO: откат |
| math-h4   | 63      | 115    | 319    | 493     | 0         | -6       | -15      | -7        | NO-GO: откат |
| math-h5b  | 63      | 115    | 319    | 494     | 0         | 0        | 0        | +1        | NO-GO: откат |
| math-h5a  | 63      | 120    | 322    | 502     | 0         | +5       | +3       | +8        | NO-GO: откат |
| math-h2   | 63      | 117    | 320    | 498     | 0         | -3       | -2       | -4        | NO-GO: откат |
| math-h6   | 63      | 115    | 319    | 495     | 0         | -2       | -1       | -3        | NO-GO: откат |

Δ — к предыдущей строке таблицы (в порядке замеров), не к базе.

## 3. Контекст

- База в этом прогоне: **w_alloc 63 Ir/op** (ядро — `refill_magazine_slow` 32 Ir), **w_free 115 Ir/op** (ядро — `flush_class_internal` ~4.7 Ir на оп в этом прогоне).
- mimalloc для сравнения: 51 / 23 Ir/op.
- Уже принятые ранее (не в этой ветке): `divides` через обратное умножение+rotate (`src/alloc_core/platform/size_classes.rs:325-336`), const fast path `class_for` (PR #78), маска слота R6-T13.

## 4. Кандидаты (в порядке замеров)

### 4.1 H1+H3 — branchless-цепочка 4 M2-оракулов free + cold re-classifier

- **Что предполагалось**: свернуть 4 защитных сравнения на free-пути (`in_range`, `not_in_mag`, `below_bump`, `not_free`) в одну branchless-предикацию; редкий reject-классификатор вынести в `#[cold]`. Слитие диапазонной проверки (H3) в один предикат отвергнуто ещё на этапе дизайна: инвариант `bump >= payload_start` не доказан — после decommit+reset `bump = 0`, даёт wrap → формула пропускала бы stale-блоки.
- **Реализация** (откачено): `src/registry/heap_core/free/dealloc_own_base.rs` — предикация `in_range & not_in_mag & below_bump & not_free`, `#[cold] classify_small_free_reject`.
- **Замер**: w_alloc 63 (=), w_free 115→121 (+6), larson 319→334 (+15), mstress 494→500 (+6).
- **Вердикт: NO-GO, откачен.** Причина: холодная функция ломает inline-форму хвоста `small_free_guard` — по разметке стоимость внутри `dealloc_own_base.rs` выросла (27.4 vs 30.4 в callers, сравнение логов), net хуже.
- **Counterfactual** (доказан воркером): неверная инверсия `below_bump` пропустила бы блок на `offset == bump` → двойная выдача одного адреса; H3-merge при `bump = 0` давал бы RHS ≈ 2^32 → приём stale-блоков.

### 4.2 H4 — cap в padding-байте PerClass

- **Что предполагалось**: перенести cap tcache из static-таблицы в свободный padding-байт `PerClass`, убрать загрузку из таблицы на hot free-пути.
- **Реализация** (откачена): `tcache.rs` — `pub(crate) cap: u8` @ offset 1; пины `offset_of == 1` / `size == 136`; init из `refill_n_for_class` (тот же источник, что `FREE_PARK_CAP`); чтение в `dealloc_own_base.rs:450` из поля.
- **Замер**: w_free 115 (=), larson 319 (=), mstress 493 (−1, шум).
- **Вердикт: NO-GO, откачен.** Причина: строка `FREE_PARK_CAP[c]` в разметке базы не видна как cost — LLVM уже держал эту загрузку дёшево; выигрыш 0 < критерия ≥ 1.
- **Counterfactual**: `cap = 0` по умолчанию отключил бы tcache на free-пути (упали бы `regression_r1_01_magazine_free_budget`, `r6_t39`).

### 4.3 H5B — residency-witness: probe→mark одним чтением байта

- **Что предполагалось**: на free-пути заменить probe-чтение + mark-запись residency-бита на одно чтение байта с witness-флагом.
- **Реализация** (откачена): `probe`/`set_probe` в `segment_bitmap`, `probe_witness`/`mark_witnessed` в `magazine_bitmap`, witness в `Accept`, push через witness; overflow-arm честно оставлен на повторном чтении.
- **Замер**: w_free 115 (=), larson 319 (=), mstress 494 (=) — **идентично базе**.
- **Вердикт: NO-GO, откачен.** Причина: LLVM уже CSE-ит повторное чтение того же байта (предсказано в `safe_design.md` U5 как «низкая уверенность»).
- **Counterfactual**: witness через мутацию байта затёр бы чужие свежепоставленные биты (потеря residency → пропуск double-free).

### 4.4 H5A — kind/off в OwnedBlock

- **Что предполагалось**: канонизатор блока (`canonical_block_of`) читает `kind_at` один раз, результат (kind, off) едет в `OwnedBlock` как witness, guard потребляет его без повторного чтения заголовка.
- **Реализация** (откачена): witness `kind: SegmentKind, off: u32` в `OwnedBlock`, `canonical_block_of` читает `kind_at` один раз, guard потребляет witness.
- **Замер**: w_free 115→120 (+5), larson 319→322 (+3), mstress 494→502 (+8).
- **Вердикт: NO-GO, откачен.** Причина: +12 байт на Copy-структуру дороже повторного чтения байта заголовка (локальность в HotLoop).
- **Counterfactual**: устаревший kind в witness прочёл бы payload Large-блока как битмапы Small-сегмента → ложные битовые операции по живым данным.

### 4.5 H2 — маска индекса класса (classes[64])

- **Что предполагалось**: паддинг `classes` до степени двойки (`SMALL_CLASS_COUNT_POW2 = 64`) и заменить сравнение-клампинг индекса на `c & CLASS_IDX_MASK` на 10+ горячих сайтах.
- **Реализация** (откачена): `SMALL_CLASS_COUNT_POW2 = 64`, `c & CLASS_IDX_MASK` на горячих сайтах, `debug_assert`, HeapCore-пин 9216→11264 (осознанный рост +2040 B).
- **Замер**: w_free 115→117 (+2), larson 319→320 (+1), mstress 494→498 (+4).
- **Вердикт: NO-GO, откачен.** Причина: сама маска стоит Ir, а +2040 B на HeapCore хуже локальность; безопасный аналог `get_unchecked` НЕ воспроизвёл его выигрыш (larson 323→317 в старом отчёте) — видимо, там выгоду давало само устранение ветки сравнения, а не снятие bounds-check.
- **Counterfactual**: переборные циклы по padded-хвосту читали бы паддинг (недопустимо). НЕ допущено: циклы оставлены `..SMALL_CLASS_COUNT`.

### 4.6 H6A — пословная MAG-маркировка carve-run при refill

- **Что предполагалось**: при refill размечать residency-биты carve-run пословно (один RMW на байт) вместо per-block цикла. Вариант infer+verify: `RunGeo::verify` (точный stride, один сегмент, биты в границах); любой mismatch → старый per-block путь; один RMW на байт через `mark_magazine_mask`/`set_mask`.
- **Реализация** (откачена): `RunGeo` + verify в `src/registry/heap_core/alloc/hot.rs`, `mark_magazine_mask` в `src/alloc_core/segment/bitmap/magazine_bitmap.rs`, `set_mask` в `src/alloc_core/segment/bitmap/segment_bitmap.rs`.
- **Найден и исправлен реальный баг прототипа**: freelist-слоты не адрес-сортированы → `wrapping_sub` + fallback (debug-паника была поймана тестами `heap_core_tcache_m2` на промежуточной версии).
- **Замер**: w_alloc 63 (=), w_free 115 (=), larson 319 (=), mstress 494→495 (+1).
- **Вердикт: NO-GO, откачен.** Причина: verify-проход съедает экономию RMW, а в steady-state w_alloc refill редок (magazine-hit доминирует).
- **Counterfactual**: неверная маска поставила бы residency-бит чужому блоку → проглоченное двойное освобождение; запись за границу битмапа — порча payload.

## 5. Тесты

Прогоняемые наборы (все rc=0):

- `heap_core_tcache_m2` (7)
- `regression_magazine_oracles` (4)
- `double_free_guard` (3)
- `regression_magazine_bump_guard` (1)
- `regression_magazine_free_bounds` (2)
- `regression_dealloc_metadata_region_guard` (1)
- `regression_magazine_scan_bounds` (1)
- `regression_hardened_large_kind_own_free` (4)
- `r19_large_interior_free` (1)
- `regression_hardened_interior_ptr` (2)
- `heap_core_tcache` (3)
- `batch_tcache` (7)
- `regression_tcache_byte_budget` (2)
- `regression_r1_01_magazine_free_budget` (2)
- `r6_t39_free_park_cap_equals_refill` (1)
- `r12_02_magazine_hit_mask_equals_canonical_root` (1)
- `regression_r2_15_virgin_mask_zeroed_and_bounded` (7)
- `r11_ph3a_refill_partial`, `r11_ph3a_issue_transaction_batch`, `r11_ph3a_issue_transaction_scalar`
- `regression_virgin_bitmap_skip`
- `alloc_zeroed_virgin_small_skip`
- `r13_3_magazine_virgin_hit_skips_zero`
- `r6_u21_foreign_free_interior_granule` (1)
- `phase13_3_dealloc_layout_class` (2)

Тесты-доказательства арифметики НЕ добавлены — принятых арифметических изменений нет. Counterfactual'ы подтверждены существующими оракулами (например, H6A-баг не-сортированных слотов реально пойман тестами на промежуточной версии).

## 6. Вывод

Путь мелких блоков в main — **локальный оптимум** для безопасных микро-приёмов:

- повторные чтения LLVM CSE-ит (H5B);
- branchless-свертка ломает форму кода (H1);
- перенос больших значений в структуры дороже чтения (H5A);
- маскирование стоит столько же, сколько сравнение, минус локальность (H2, H6A);
- static-таблицы уже дёшевы (H4).

Закрытые направления, НЕ повторять: R32-6 dual-bitmap, cold-вынос free-веток, clear-at-issue — и эти семь (H1, H2, H4, H5A, H5B, H6A).

## 7. Рекомендации CI

- iai A/B ячейки не требуются: принятых изменений нет, мержить в `src/` нечего.
- Если владелец захочет вернуться к H2 — только в связке с уменьшением `PerClass` (например, u32 slots), иначе +2040 B на HeapCore с высокой вероятностью повторит регресс.

## 8. Risks / notes

- Сырые логи сохранены: `docs/perf/_raw_b4_math_*.log` (7 состояний × 3 файла: ao, larson `-l`, mstress `-m`).
- Замеры единичные (не 3 повтора) — для NO-GO-порога ±2 Ir опускаем: все отклонения, кроме H4 (−1 mstress, шум), выходят за шум либо строго неотрицательны.
- Все откаты сверены `git diff` — в конце работы дерево чистое против `f93c1b52`.
