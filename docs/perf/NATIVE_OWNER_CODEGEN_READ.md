# Native owner-path: статическая проверка Windows-MSVC codegen

Это чтение уже построенных PE/PDB, не новый benchmark и не native speedup gate. B ниже — прежний непринятый own-free-context, а не база нового кандидата. SHA сверены с original native_binaries.json до disassembly. Runtime source base: f80c681b799ef4ef7fa08d79c06896fdf1104fe9; features production; toolchain — original comparison_identity.json (Windows MSVC).

## Наблюдения

В проверенных alloc/free wrappers TLS разворачивается в загрузку module TLS index, gs:0x58, TLS module base и LOCAL по offset 0x38; затем sentinel compare. Между этими загрузками и own-heap branch нет вызова lazy-init helper. Это native TLS lowering, не доказательство нулевой latency TLS и не основание менять registry binding.

Const alloc hit не вызывает другие функции, но его общий пролог всё равно сохраняет rsi и выделяет shadow space. В той же unwind-функции находятся non-tail first-bind и refill вызовы и обработка null/process-exit. Вывод: предмет следующей гипотезы — граница завершённого hit и call-bearing continuations, не новые inline/cold перестановки прежнего free-кандидата. Статический frame сам по себе ещё не доказывает native выигрыш.

| Phase | Wrapper | Bytes | Pushes в прологе | sub rsp, bytes |
|---|---|---:|---:|---:|
| before | alloc_rt | 516 | 3 | 48 |
| before | free16 | 690 | 4 | 56 |
| before | free64 | 693 | 4 | 56 |
| before | alloc16 | 249 | 1 | 32 |
| before | alloc64 | 252 | 1 | 32 |
| before | free_rt | 1732 | 8 | 56 |
| after | alloc_rt | 516 | 3 | 48 |
| after | free16 | 596 | 2 | 56 |
| after | free64 | 599 | 2 | 56 |
| after | alloc16 | 249 | 1 | 32 |
| after | alloc64 | 252 | 1 | 32 |
| after | free_rt | 1512 | 6 | 56 |

B действительно уменьшает число сохраняемых регистров native free, но не меняет alloc-пролог; это совместимо с прежним Ir-выигрышем, не объясняет причинно canonical ухудшения и не пересматривает NO-GO FOR PROMOTION.

Границы функций берутся из PE exception/unwind directory; публичный PDB не содержит имён локальных generic wrappers. Разметка wrappers выполнена по порядку функций перед own_free_wall::main и проверке layout/class-specific count/state операций в assembly. Исходные адреса и hashes — [companion CSV](NATIVE_OWNER_CODEGEN_READ_summary.csv), исходная копия — [native_owner_codegen/summary.csv](native_owner_codegen/summary.csv). Все шесть отобранных функций каждой стороны сохранены целиком.
