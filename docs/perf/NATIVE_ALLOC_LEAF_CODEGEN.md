# Native alloc leaf — codegen gate, не speedup claim

Вердикт: **PASS только static codegen gate**. Timing и shipping acceptance отсутствуют. Compile-check test target завершён; тесты не исполнялись.

База: f80c681b799ef4ef7fa08d79c06896fdf1104fe9. Source identity и patch ДО build: [attempt1](native_alloc_leaf/attempt1/identity.json). Binary SHA-256: `7aca1221a1a29df885a5f3529841774bf52561a1daeda4cbba692e62e184501d`. Windows-MSVC rustc/LLVM/features/CPU/OS — в identity. Entry point: noinline wrappers над direct SeferAlloc GlobalAlloc facade, не installed global allocator.

| Wrapper | Before bytes | Candidate bytes | Entry pushes | Calls во всём candidate body |
|---|---:|---:|---:|---:|
| alloc_rt | 516 | 356 | 0 | 1 |
| free16 | 690 | 690 | 4 | 3 |
| free64 | 693 | 693 | 4 | 3 |
| alloc16 | 249 | 139 | 0 | 0 |
| alloc64 | 252 | 142 | 0 | 0 |
| free_rt | 1732 | 1732 | 8 | 10 |

Const16/64: hot TLS/count/pop/state-clear → ret, без stack frame и calls. Miss/unbound → отдельные tail jumps. Runtime: прежние три pushes и sub rsp,0x30 заменены нулём pushes и sub rsp,0x28; bounds-panic call остаётся в общем body. Free source не менялся; статический footprint всех трёх free wrappers совпадает с baseline.

Полные тела: [codegen](native_alloc_leaf/attempt1/codegen/summary.csv). Границы подтверждены int3 padding до/после каждого тела; main содержит прямые calls runtime и lea адресов const16/64 для косвенного вызова. Leaf отсутствие unwind row не считается ошибкой. В первой диагностической попытке nearest main direct-call target ошибочно выбирал соседние функции: rejected scratch leaf-functions.json не используется как gate evidence; правильный extraction — leaf-boundaries.mjs. Файлы старой попытки сохранены.

Следующий gate — production-only WSL marginal alloc/free/full pair и refill/cap controls, затем native balanced comparison относительно mimalloc. Из статической экономии не следует native benefit. Новый public measurement hook не добавлялся. Коммитов/push нет.
