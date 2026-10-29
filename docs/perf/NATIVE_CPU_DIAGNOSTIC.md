# Exact-process CPU diagnostic — не native speedup judge

Пререгистрация: ../design/2026-10-25-native-cpu-diagnostic-prereg.md. Шесть одинаковых свежих процессов, exact frozen production Sefer PE,241full-workload repeats. Ни candidate A/B, ни изменение frequency/affinity/priority, ни новая сборка не выполнялись. Native calibration FAIL не пересматривается.

GetProcessTimes100nsunits, QueryProcessCycleTime rawcycles: [cpu2 source/raw/identity](native_mixed/cpu2/identity.json). Все6samples сохранены. Process HANDLE своего child удерживается Popen до query; Python version pinned в identity.

| Sample | Workload ns | Full lifetime ns | Accounted CPU ns | Raw cycles | Lifetime−accounted CPU ns | CPU/lifetime |
|---|---:|---:|---:|---:|---:|---:|
| 0 | 121338700 | 132523300 | 140625000 | 292789688 | -8101700 | 1.0611341552768456 |
| 1 | 125057800 | 131712000 | 125000000 | 298509390 | 6712000 | 0.9490403304178814 |
| 2 | 107565800 | 111853500 | 125000000 | 254645638 | -13146500 | 1.1175332019114288 |
| 3 | 167024000 | 172507800 | 156250000 | 342676508 | 16257800 | 0.9057561455192171 |
| 4 | 132898000 | 138469400 | 140625000 | 277111260 | -2155600 | 1.0155673383433452 |
| 5 | 117279400 | 122521800 | 109375000 | 259707560 | 13146800 | 0.8926982790001453 |

CPUtime иcycles относятся ко всему child process, workload_ns — только warmed work. Остаток lifetime−CPU не является точным off-CPU trace; GetProcessTimes имеет ограниченную гранулярность, CPUtime включает user+kernel всех threads. Из6samples нельзя установить причину старого ABBA position effect или пересчитать cycles в wall time без clock/frequency premise. Нет CI/GO для latency по этой диагностике.

Attemptcpu1failed ДО child launch: явное чтение UTF-8 script через Windows defaultcp1252 вызвало UnicodeDecodeError; ошибка воспроизведена изолированным read_text без выполнения benchmark и исправлена read_text(encoding='utf-8'). Cpu1identity сохранена, failed attempt не является sample.

Вердикт: DIAGNOSTIC ONLY; пригодный native judge и causal bottleneck allocator этой пробой не установлены. Candidate по-прежнему не открыт; retuning до PASS не выполняется.
