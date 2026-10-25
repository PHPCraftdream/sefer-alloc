# Exact-process CPU diagnostic — не повтор calibration

Основание: A/A full-workload calibration FAIL, наблюдаемая inner/outer разность имеет CI вне нуля. Причина неизвестна. Проверяем только возможность разнести elapsed и фактически учтённое CPU-время, не пытаемся получить PASS или speedup.

Ровно6 свежих одинаковых Sefer processes: frozen production PE из calibration1,241repeats, прежний1warmup; binary/source guards. Никаких candidate/mimalloc arms, новой сборки, настройки frequency/affinity/priority, фоновой нагрузки или увеличения repeats.

Parent Python3.12 win32, стандартный ctypes Kernel32: GetProcessTimes для creation/exit/kernel/user FILETIME (100ns units); QueryProcessCycleTime для raw CPUcycles. Запрашивается только открытый process HANDLE своего Popen child, пока parent владеет HANDLE, после блокирующего communicate. Другая сессия/процесс не читается. CPython Windows Popen._handle — private implementation detail: фиксируется версия, результат не переносится на другую runtime без проверки. Native queries не добавляют per-op work в child executable.

Raw sample пишется первым. Метрики различаются: workload_ns исключает warmup/output, process lifetime включает их; parent elapsed дополнительно включает launch/wait overhead. CPUtime — суммарный user+kernel всего child process; raw cycles не переводятся в wall time. GetProcessTimes имеет ограниченную гранулярность: residual wall−CPU не является точным scheduler trace, а причина разницы (off-CPU/frequency/measurement/другая работа) не выводится автоматически.

После6samples только report+CSV+checked generator. Нет последующего retuning или native candidate gate в этой пробе. Предыдущий FAIL сохраняется независимо от результата.
