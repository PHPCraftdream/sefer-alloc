# Alloc leaf — native measurement, не shipping acceptance

Direct SeferAlloc GlobalAlloc facade против pinned f80 и mimalloc, production-only Windows-MSVC. Source identities ДО build, binary hashes до/после runs: [native1](native_alloc_leaf/native1/identity.json). Candidate — свежий exact codegen-gated PE attempt1, не пересобран при timing; baseline построен заново тем же rustc. Raw840 process samples: samples.ndjson рядом. 20 balanced blocks каждого режима, 10 A/A blocks runtime. Arithmetic means; paired Student-t95 CI по разности block means B−A. Все samples сохранены; выбросы не исключались. Ratio win = mean(mimalloc)/mean(Sefer), больше лучше; это не ratio отдельных произвольно выбранных samples.

## Полный workload, включая setup/teardown

Нормировка: const n+16 pairs, runtime n+768, bulk64/bulk16k n×64. Не pure alloc latency. Проба не Criterion; отдельной Criterion full-round mean здесь нет.

| Mode | Before ns/pair | After ns/pair | Win mi/Sefer до → после | Paired95 CI delta ns/pair | Verdict |
|---|---:|---:|---:|---:|---|
| 16 | 7.149651 | 7.003418 | 1.039693 → 1.150746 | -0.835272 … 0.542806 | INCONCLUSIVE |
| 64 | 7.198870 | 7.274979 | 1.188990 → 1.070212 | -0.821478 … 0.973696 | INCONCLUSIVE |
| rt | 16.673197 | 15.463691 | 0.749265 → 0.797630 | -2.795203 … 0.376190 | INCONCLUSIVE |
| bulk64 | 21.466064 | 22.144165 | 0.578881 → 0.612724 | -2.226191 … 3.582392 | INCONCLUSIVE |
| bulk16k | 41.173248 | 38.583984 | 1.427494 → 1.605302 | -6.993478 … 1.814950 | INCONCLUSIVE |

## Полный процесс, включая startup/output/exit

| Mode | Before ns/pair | After ns/pair | Paired95 CI delta ns/pair | Verdict |
|---|---:|---:|---:|---|
| 16 | 15.572931 | 15.329185 | -1.085964 … 0.598471 | INCONCLUSIVE |
| 64 | 15.357714 | 15.469767 | -0.965490 … 1.189596 | INCONCLUSIVE |
| rt | 25.057528 | 23.838632 | -2.966568 … 0.528776 | INCONCLUSIVE |
| bulk64 | 478.045197 | 484.076843 | -32.391828 … 44.455121 | INCONCLUSIVE |
| bulk16k | 491.039276 | 486.940765 | -24.429083 … 16.232062 | INCONCLUSIVE |

A/A: workload_ns: delta 0.25313059570250035, CI -2.5500824202247996 … 3.0563436116298; process_ns: delta 0.12169154089659133, CI -2.9662498331208815 … 3.209632914914064.

Это один bounded gate, не доказанная совокупная победа allocator. Отсутствие значимого ухудшения не доказывает нерегрессию; native benefit и WSL Ir — разные оси. Canonical churn16, installed/global, MT и correctness execution не покрываются этим harness. Вердикт этого gate: **INCONCLUSIVE / DO NOT PROMOTE** — все пять workload CI пересекают ноль; полный process не устанавливает benefit. Codegen и Ir механизмы подтверждены, но native-прорыв не установлен. По пререгистрации broad/canonical/installed/MT не запускались: кандидат не прошёл native benefit gate. Это не доказанная регрессия и не доказанная нерегрессия. Повторов до удобного результата и attribute-перебора нет. Следующий эксперимент допустим только с новым причинным evidence; сейчас такой гипотезы не установлено.
