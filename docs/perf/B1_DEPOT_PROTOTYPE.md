# B1-D: склад магазинов (depot) — прототип, только измерение

Измерительная ветка `b4-ablate`, runtime не меняется. Прототип — патчи
`docs/perf/b4_ablate/patches/b1_depot4.patch` и `b1_depot8.patch`: для
классов 0..8 до K полных магазинов на класс. При переполнении магазин
целиком уходит в склад, при промахе целиком возвращается; блоки остаются
в состоянии MAG, их тела и метаданные не трогаются. Корректность вне
однопоточного прогона судьи не поддерживается (освобождение памяти,
завершение потока и возврат сегментов склад не видят; ограничение размера
`HeapCore` ослаблено).

## Почему

Пробник `examples/b4_pmu_probe.rs` (ветка `b4-pmu`) показал, что отставание
судьи на churn — это перенос блоков между классами: ядра churn16/churn64
делят одно кольцо слотов. На одном размере sefer быстрее mimalloc; на
смешанном 16/64 каждый перенесённый блок проходит магазин → сброс в список
→ пополнение → выдачу. Сырьё пробника: `_raw_probe_depot_windows.txt`
(тики на операцию, 5 процессов на вариант).

## Метод

Судья J1′ v2: 20 процессов на режим × 2000 блоков, i7-11800H, Windows
10.0.19045, release, выравнивание кода. Эффект варианта считается за
вычетом null-копии (`scripts/b4_ablate_vs_null.mjs`): медиана по процессам
`ln(t_variant/t_sefer)` минус та же медиана для null; 95 % CI — bootstrap
обоих наборов процессов. Причина: A/A sefer×sefer несёт систематический
сдвиг слотов около −0.5…−1.1 % на churn, из-за которого предрегистрированный
гейт `--expect-admitted` остановил первый прогон (null A/A churn16 −1.11 %,
churn64 −1.00 %); второй прогон выполнен с `--no-null-gate`, вычитание null
убирает общий сдвиг. Это отклонение от предрегистрации A2.

## Результат (вывод скрипта, дословно)

# B4 ABLATE — variant net of null: variant=b1_depot4 reps=20 blocks/process=2000 bootstrap=2000
# identity: head=d47924a2f6ec3b353ce6d0c3d82057e109f690a2 sha(diff)=e3b0c44298fc variant_sha=8c05dde564e6 exe=bbdd4b76e227 null_exe=94338761149a
# effect = median_processes ln(t_variant/t_sefer) − median_processes ln(t_null/t_sefer); NET_FASTER iff CI upper < 0, NET_SLOWER iff CI lower > 0

| Kernel | variant A/B % | null A/B % [CI] | effect % [CI] | ratio exp(effect) | precise | verdict |
|---|---:|---|---|---:|---|---|
| churn16 | -3.77 | -0.38 [-0.89, -0.12] | -3.39 [-5.01, -2.51] | 0.9667 | yes | NET_FASTER |
| churn64 | -12.26 | 0.49 [0.12, 0.69] | -12.76 [-13.27, -11.86] | 0.8802 | yes | NET_FASTER |
| pair16 | 2.29 | 0.21 [0.16, 0.30] | 2.08 [1.96, 2.30] | 1.0210 | yes | NET_SLOWER |
| bulk64 | -55.81 | 1.46 [1.29, 1.71] | -57.28 [-57.71, -56.68] | 0.5640 | yes | NET_FASTER |
| bulk16k | 3.53 | 0.76 [0.60, 0.99] | 2.77 [2.07, 4.92] | 1.0281 | yes | NET_SLOWER |

# B4 ABLATE — variant net of null: variant=b1_depot8 reps=20 blocks/process=2000 bootstrap=2000
# identity: head=d47924a2f6ec3b353ce6d0c3d82057e109f690a2 sha(diff)=e3b0c44298fc variant_sha=14d77fc031ac exe=42f095283010 null_exe=94338761149a
# effect = median_processes ln(t_variant/t_sefer) − median_processes ln(t_null/t_sefer); NET_FASTER iff CI upper < 0, NET_SLOWER iff CI lower > 0

| Kernel | variant A/B % | null A/B % [CI] | effect % [CI] | ratio exp(effect) | precise | verdict |
|---|---:|---|---|---:|---|---|
| churn16 | -13.04 | -0.38 [-0.89, -0.12] | -12.65 [-14.67, -11.24] | 0.8811 | yes | NET_FASTER |
| churn64 | -28.08 | 0.49 [0.12, 0.69] | -28.57 [-29.28, -27.51] | 0.7515 | yes | NET_FASTER |
| pair16 | 2.53 | 0.21 [0.16, 0.30] | 2.32 [2.12, 2.57] | 1.0235 | yes | NET_SLOWER |
| bulk64 | -55.74 | 1.46 [1.29, 1.71] | -57.20 [-57.61, -56.70] | 0.5644 | yes | NET_FASTER |
| bulk16k | 2.96 | 0.76 [0.60, 0.99] | 2.19 [1.55, 3.97] | 1.0222 | yes | NET_SLOWER |


## Что это значит

- K=8: bulk64 −57.2 % (отношение 0.564), churn64 −28.6 %, churn16 −12.7 %.
  На bulk64 sefer в v1 был медленнее mimalloc в 1/0.589 раза; если
  умножить его время на 0.564, получается паритет (ОЦЕНКА, прямой замер
  против mimalloc с депо ещё не делался).
- Цена: pair16 +2.3 %, bulk16k +2.2 % (K=8). Причина не измерена; кандидат —
  лишняя ветка в обработчике переполнения внутри горячей функции free и
  раскладка кода.
- K=4 слабее на churn (−3.4 % / −12.8 %), bulk64 тот же.

## Артефакты

- Сырьё: 180 jsonl, tier 3, sha256 и размеры — `MANIFEST.txt`.
- `vsnull_b1_depot{4,8}.csv`, `summary_rep_table.csv`,
  `rep_table_stdout.txt` (таблицы против sefer без вычитания null).
- Воспроизведение: `node scripts/b4_ablate_run.mjs --out-dir <dir> --blocks
  2000 --reps 20 --kernels churn16,churn64,pair16,bulk64,bulk16k --variants
  b1_depot4,b1_depot8 --no-null-gate`, затем `node
  scripts/b4_ablate_vs_null.mjs --dir <dir> --variant <v> --reps 20
  --kernels churn16,churn64,pair16,bulk64,bulk16k`.

## sefer против mimalloc в том же бинаре (ОЦЕНКА для депо)

Режим `ab` судьи (A = sefer без патча, B = mimalloc), 20 процессов ×
2000 блоков, сводка `scripts/b4_rep_stats_quick.mjs` (медиана по процессам,
bootstrap по процессам; ad-hoc, не гейт), вывод —
`docs/perf/b1_depot/sefer_vs_mi_stdout.txt`. t_mimalloc/t_sefer: pair16
0.790, churn16 0.566, churn64 0.560, bulk64 0.473, bulk16k 0.404.

Если умножить время sefer на эффект depot8 (таблица выше), получается
ОЦЕНКА t_mimalloc/t_depot8: bulk64 0.473/0.564 = 0.84, churn64
0.560/0.751 = 0.75, churn16 0.566/0.881 = 0.64, pair16 0.790/1.024 = 0.77,
bulk16k 0.404/1.022 = 0.40. Депо сокращает разрыв, но не закрывает его ни
на одном ядре; bulk16k (16384 блоков, глубже любого склада) не меняется.

## Откуда +2 % на pair16: раскладка или код (ИЗМЕРЕНО)

Вариант `b1_depot8_layout` (патч `patches/b1_depot8_layout.patch`): поля
склада и ослабленный assert размера `HeapCore` есть, логики склада в free и
refill нет. Тот же протокол (20 процессов × 2000 блоков, за вычетом null),
ядра pair16, pair64, churn16, bulk64; вывод — `b1_depot/layout_split/`.

| Ядро | depot8, эффект % [CI] | только раскладка, эффект % [CI] |
|---|---|---|
| pair16 | +1.64 [1.24, 1.91] | 0.00 [−0.27, 0.17] |
| pair64 | +1.96 [1.57, 2.20] | +0.05 [−0.21, 0.23] |
| churn16 | +0.39 [−0.76, 1.62] | +0.24 [−0.54, 1.18] |
| bulk64 | −40.61 [−41.24, −40.14] | +0.32 [−0.11, 0.65] |

- Раскладка структуры не стоит ничего; цена pair16/pair64 — код склада в
  горячей функции free (ветка переполнения встроена в
  `dealloc_own_thread_with_base`). Production-версия должна держать склад в
  вынесенных `#[cold] #[inline(never)]` путях переполнения и пополнения.
- churn16 без churn64 в наборе ядер — одноклассовый (общее кольцо делит
  только churn-ядра): склад ему ничего не даёт, что подтверждает механизм A3.
  bulk64 здесь −40.6 % против −57.2 % в наборе с churn64/bulk16k: выигрыш
  склада зависит от смеси классов в процессе.

## Холодные пути: склад-массив (S2) против склада-цепочки (S3) — DP-2 (ИЗМЕРЕНО)

Вопрос предрегистрации production-склада (`docs/design/2026-10-26-depot-design.md`,
ветка `b4-depot`): где хранить запаркованные магазины, если ограничение
`size_of::<HeapCore>() <= 9216` нельзя ослаблять. Оба прототипа держат
логику склада в `#[cold] #[inline(never)]` функциях `depot_park` /
`depot_reload` (`alloc/hot.rs`); горячий free только вызывает их на ветке
переполнения, пополнение — в начале `refill_magazine_slow`.

- `b1_depot8c` (S2-подобный): массивы указателей в `Tcache`, assert размера
  в прототипе ослаблен на 8192 Б (только для замера) — патч
  `b4_ablate/patches/b1_depot8c.patch`.
- `b1_chain8c` (S3): 16 блоков магазина связываются через `next` в их телах
  (`Node::write_next` / `Node::read_next`), в `Tcache` — голова цепочки и
  счётчик — патч `b4_ablate/patches/b1_chain8c.patch`.

K = 8, классы 0..8. Протокол — тот же, что выше: 20 процессов × 2000 блоков
на вариант, эффект за вычетом null-копии, ядра churn16, churn64, pair16,
pair64, bulk64, bulk16k, larson. Источник: head `259689f5` + патч варианта
(sha256 варианта и бинаря — в шапках `b1_depot/cold_s2_s3/vs_null_*_stdout.txt`).
Сырьё (180 файлов, 1.1 ГиБ) не в git: sha256 и размер каждого файла —
`b1_depot/cold_s2_s3/MANIFEST.txt`. Команда прогона:

```text
RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5" \
node scripts/b4_ablate_run.mjs --out-dir <dir> --blocks 2000 --reps 20 \
  --kernels churn16,churn64,pair16,pair64,bulk64,bulk16k,larson \
  --variants b1_depot8c,b1_chain8c --summary <csv> --no-null-gate
node scripts/b4_ablate_vs_null.mjs --dir <dir> --variant <v> --reps 20 \
  --kernels churn16,churn64,pair16,pair64,bulk64,bulk16k,larson --csv <csv>
node scripts/b1_depot_choice.mjs --s2 vsnull_b1_depot8c.csv --s3 vsnull_b1_chain8c.csv
```

Вывод `scripts/b1_depot_choice.mjs` дословно (`b1_depot/cold_s2_s3/choice_stdout.txt`):

```text
# DP-2 storage choice: S2=b1_depot8c S3=b1_chain8c; effect = ln(t_variant/t_sefer) net of null

| Ядро | S2: эффект % [CI] | S2 | S3: эффект % [CI] | S3 | S3/S2 |
|---|---|---|---|---|---:|
| churn16 | -18.75 [-19.64, -17.74] | быстрее в 1.206× | -10.41 [-11.41, -9.70] | быстрее в 1.110× | 0.555 |
| churn64 | -32.37 [-33.13, -31.53] | быстрее в 1.382× | -4.74 [-5.71, -4.26] | быстрее в 1.049× | 0.146 |
| pair16 | -0.04 [-0.11, 0.11] | быстрее в 1.000× | -0.14 [-0.25, -0.05] | быстрее в 1.001× | 3.494 |
| pair64 | 0.01 [-0.08, 0.08] | медленнее в 1.000× | -0.01 [-0.12, 0.07] | быстрее в 1.000× | -0.673 |
| bulk64 | -59.15 [-59.81, -58.74] | быстрее в 1.807× | -46.04 [-46.81, -45.20] | быстрее в 1.585× | 0.778 |
| bulk16k | 0.83 [0.53, 1.38] | медленнее в 1.008× | 1.28 [0.85, 2.09] | медленнее в 1.013× | 1.546 |
| larson | -6.98 [-8.67, -6.31] | быстрее в 1.072× | 0.21 [-0.91, 1.31] | медленнее в 1.002× | -0.030 |

# bulk64: S3/S2 = -46.04 / -59.15 = 77.8 % (порог 80 %)
# churn64: S3/S2 = -4.74 / -32.37 = 14.6 % (порог 80 %)
# пороги гейта, S2: пройдены
# пороги гейта, S3: churn64: эффект -4.74 % > -22.31 % или верх CI >= 0
CHOICE=S2
```

- **Выбор по предрегистрированному правилу — S2.** S3 даёт 77.8 % эффекта
  S2 на bulk64 (−46.04 / −59.15) и 14.6 % на churn64 (−4.74 / −32.37) при
  пороге 80 %, и не проходит порог гейта на churn64.
- **Вынос в холодные функции снял цену на pair16/pair64:** у S2 pair16
  −0.04 % [−0.11, 0.11], pair64 +0.01 % [−0.08, 0.08] против +1.64 % и
  +1.96 % у встроенного depot8 (раздел про раскладку выше).
- Почему цепочка проигрывает на churn64: парковка и забор пишут и читают
  `next` в 16 телах блоков, которые давно не были в кэше (ОЦЕНКА — PMU не
  снимался); массив трогает одну строку `Tcache`.
- larson: S2 быстрее в 1.072× (−6.98 % [−8.67, −6.31]); bulk16k медленнее в
  1.008× (+0.83 % [0.53, 1.38]) — ниже порога «нет значимого замедления
  больше 2 %».
- Это прототип с ослабленным assert; production-версия хранит массивы вне
  `HeapCore` (дизайн, ветка `b4-depot`) и проходит свой гейт DP-5.
