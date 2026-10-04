# Ph3c шаг 1′ — диагностический стенд фрагментации (frag-stand): base+S (B) против текущего спайка B3 (C)

Дата: 2026-10-04. **ДИАГНОСТИКА — НЕ решающий замер.** Решающий замер — шаг 1′
после правки §3; этот стенд лишь проверяет предсказание аддендума и фиксирует
диагноз на ТЕКУЩЕМ спайке. Протокол и предсказание —
`docs/design/2026-10-02-adr-addendum-ph3c-step1prime.md` §4. Плечо —
`examples/ph3c_frag_stand.rs`, судья — `scripts/ph3c_frag_stand_table.mjs`,
сырые логи — `docs/perf/_raw_ph3c_frag_stand_w1.log` … `w4.log`,
`_raw_ph3c_frag_stand_aa.log`, таблица —
`docs/perf/PH3C_FRAG_STAND_summary.csv`.

## Предсказание ДО замера

Из §4 аддендума, дословно:

> «Предсказание на ТЕКУЩЕМ спайке (до замера): W1 ≈ 2.0 (FAIL), W2 ≈ 1.17
> (FAIL), W4 ≈ 1.00. Если W1 ≤ 1.10 — стенд невалиден как оракул.»

Предсказание зафиксировано до прогона; фактические числа ниже.

## Идентичность

| Что | Значение |
|---|---|
| плечо B (база) HEAD | `c584a3ae2c7a4aebd64c3e03d5cb553cea2c9b54` (tree `b1efd26a363e27c20057f522dc0cb07d7115c1b7`) + патч S, sha256 `94a911665b705c19228689dc0484c618f41c1a7c8860e9aa4bc77ba146a122ee` (изменения `sidecar_drain.rs`, `find_segment.rs`) |
| плечо C (спайк B3) HEAD | `093c9d2374ea6037cec1aacf2beea00744fc87b8` (tree `f05043224f238219782e493aad9ce5a5ff86f6c8`), рабочее дерево чистое |
| rustc | 1.98.1 |
| WSL2 ядро | 6.18.33.2-microsoft-standard-WSL2 |
| features | `production internals bench-internals` |
| CARGO_TARGET_DIR | приватные `/tmp/sefer-frag-stand-base` и `/tmp/sefer-frag-stand-c` |

Свежесть: в build-логах обоих плеч в `docs/perf/` —
`docs/perf/_raw_ph3c_frag_stand_build_base.log` (плечо B) и
`docs/perf/_raw_ph3c_frag_stand_build_c.log` (плечо C) — ровно по одной строке
`Compiling sefer-alloc v0.3.0` со своим путём дерева:
`(/mnt/d/dev/rust/sefer-alloc/worktrees/ph3c-b3s-base)` для B и
`(/mnt/d/dev/rust/sefer-alloc/worktrees/ph3c-b3s)` для C.

## Протокол (§4 аддендума, кратко)

Настоящий `global_allocator` sefer-alloc; 1 свежий процесс на плечо×нагрузку;
5 чередующихся прогонов B,C,B,C,… на плечо, медиана по 5. Метрика:
`frag = (RSS_end − RSS_empty) / live_requested_bytes` (KiB/KiB,
безразмерное), VmRSS из `/proc/self/status`; commit — вторичная метрика.
`config_conflicts` delta = 0 — assert выполнен судьёй во всех 32 блоках
(20 в w1–w4 + 12 A/A). `resolved_config` readback — **НЕдоступен** (нет
публичного API, в логах `resolved_config=unavailable`).

Нагрузки (W1–W4): детерминированные последовательности alloc/free по
фактическим классам SIZE_CLASS_TABLE — {16,32,48,64,80,112} в [16,128];
до 2048 для W2/W4; до 4096 для W3; класс 40 байт (EXTRAS) — литералы в стенде.
`live_requested_bytes` ≈ 64 MiB (±1%; W3 = 67 454 576 из-за округления до
класса) — assert судьёй в каждом блоке.

## Результаты (медианы по 5 прогонам; frag = (RSS_end − RSS_empty)/live, KiB/KiB)

| нагрузка | B медиана | C медиана | C/B (знаменатель = медиана B) | гейт ≤1.10 | сегменты B→C |
|---|---:|---:|---:|---|---|
| W1 | 1.1122 | 4.4637 | 4.013 | **FAIL** | 17→72 |
| W2 | 1.0647 | 1.2016 | 1.129 | **FAIL** | 17→20 |
| W3 | 1.1466 | 1.2321 | 1.075 | PASS | 20→21 |
| W4 | 1.0215 | 1.0238 | 1.002 | контроль ≈1.00 ✓ | 17→17 |

Сегменты — медианы `segments_reserved_total` по 5 прогонам (подтверждены
судьёй при прогоне; продублированы в CSV).

### A/A (B против B, aa-лог, 3 прогона на нагрузку)

| нагрузка | spread (max−min)/median | порог <1% |
|---|---:|---|
| W1 | 0.000% | PASS |
| W2 | 0.000% | PASS |
| W3 | 0.000% | PASS |
| W4 | 0.000% | PASS |

RSS в KиБ побитово стабилен между прогонами. Знаменатель каждого C/B — медиана
B той же нагрузки; все отношения перепроверены guarded-принтерами судьи
(round-trip ±0.01), вбитых чисел в выводе нет.

### Commit-метрика (вторичный столбец)

`commit_empty_kib` / `commit_end_kib` присутствуют в логах каждого блока;
отдельную таблицу не ведём (вторична по протоколу §4).

## Вердикт

**Стенд ВАЛИДЕН как оракул.** W1: C/B = 4.013 ≫ 1.10 — направление и знак
совпали с предсказанием FAIL (и даже сильнее предсказанных ≈2.0). Контроль
W4: C/B = 1.002 ≈ 1.00 ✓. A/A spread < 1% на всех нагрузках.

Текущий спайк B3 — **FAIL** против порога C/B ≤ 1.10 на W1–W3: провалены W1
(4.013) и W2 (1.129); W3 (1.075) формально в пороге, но гейт требует все три.

Диагноз согласуется с §1 аддендума: общий bump-карвинг сжигает хвост
под-листа на каждую смену класса (сегменты W1: 17→72, ×4.2).

Предсказанные ≈2.0 против факта 4.013 — расхождение КОЛИЧЕСТВЕННОЕ, не
качественное (обе стороны порога 1.10, знак и диагноз совпали). HYPOTHESIS:
в спайке каждая смена класса в среднем сжигает почти целый под-лист — прыжок
к следующему 4 KiB под-листу при среднем блоке W1 ≈ 59 Б (≈ 69 блоков на
под-лист) и чередовании 6 классов теряет хвост каждого под-листа, что и даёт
коэффициент ~4, а не ~2.

## Что не измерено

- привязанные под-листы, байты потерянных хвостов (счётчиков в публичном API
  нет; `src` не менялся);
- `resolved_config` readback (нет публичного API);
- Windows-хост RSS (VmRSS доступен только на Linux).

## Как запустить для шага 1′

1. Скопировать `examples/ph3c_frag_stand.rs` в новое исправленное дерево C′.
2. В каждом дереве (WSL): `CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=/tmp/<priv>
   cargo build --release --example ph3c_frag_stand --features "production
   internals bench-internals"`.
3. Тот же цикл прогонов: 5 чередующихся прогонов на плечо, свежий процесс на
   каждый (`<bin> w1`, …, `<bin> w4`); плюс A/A-лог (3 B-прогона на нагрузку).
4. `node scripts/ph3c_frag_stand_table.mjs` — печатает таблицу и пишет
   `docs/perf/PH3C_FRAG_STAND_summary.csv`.

Раннер параметризован путями деревьев через переменные в начале команды,
пример:

```bash
B=/mnt/d/dev/rust/sefer-alloc/worktrees/ph3c-b3s-base
C=/mnt/d/dev/rust/sefer-alloc/worktrees/<c-prime-tree>
for arm in B C; do
  d=$([ $arm = B ] && echo $B || echo $C)
  td=/tmp/sefer-frag-stand-$([ $arm = B ] && echo base || echo c)
  bl=${arm/B/base}; bl=${arm/C/c}   # B->base, C->c
  (cd $d && CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=$td cargo build --release \
    --example ph3c_frag_stand --features "production internals bench-internals") \
    > docs/perf/_raw_ph3c_frag_stand_build_${bl}.log 2>&1
  for w in w1 w2 w3 w4; do
    for rep in 1 2 3 4 5; do
      echo "== RUN ${arm}${rep} ==" >> docs/perf/_raw_ph3c_frag_stand_${w}.log
      $td/release/examples/ph3c_frag_stand $w | sed "s/^RESULT /RESULT /" \
        >> docs/perf/_raw_ph3c_frag_stand_${w}.log
    done
  done
done
node scripts/ph3c_frag_stand_table.mjs
```

(Имена build-логов: `_raw_ph3c_frag_stand_build_base.log` и
`_raw_ph3c_frag_stand_build_c.log` — задать явно через редирект; A/A-лог —
12 блоков `== RUN B<rep> ==`, 3 на нагрузку в порядке w1..w4, в
`_raw_ph3c_frag_stand_aa.log`.)
