> Примечание переноса (main): измерительная ветка `b4-ablate`@734dd363 (origin), скрипты прогона живут там; все пути вида `scripts/b4_ablate_*.mjs` ниже — файлы ветки b4-ablate (ветка b4-ablate).

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
