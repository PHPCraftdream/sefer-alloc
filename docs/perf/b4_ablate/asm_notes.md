# asm notes — b4-ablate judge shims, v2 (counter without lock)

Измерение asm-накладных расходов шимов судьи. **v2**: счётчик вызовов —
thread-local `Cell<u64>` (const-init, без ленивой инициализации), инкремент —
plain `inc qword ptr`, `lock` из тел шимов полностью убран.

## Метод (v2)

Сборка: `CARGO_TARGET_DIR=<target> cargo rustc --profile release --example
b4_interleave_judge --features production,ablate -- -C save-temps
-C llvm-args=-x86-asm-syntax=intel`, rc=0. Пост-LTO объектник
`release/examples/b4_interleave_judge.<hash>-cgu.0.rcgu.o` дизассемблирован
MSVC dumpbin 14.36 → полный дизасм примера; тела шимов извлечены отдельно.
`mi_free` / `mi_malloc_aligned` — C-код mimalloc, не менялся между v1 и v2 —
взят из прежнего дизасма mimalloc-C-объектника.

Парсер метрик (`ab_asm_metrics_v2.py`) правила v2:

- **Загрузки** — только реальные чтения памяти: `mov r,[mem]`,
  `movzx/movsx`, mem-операнд у `cmp/test/push`, memory-RMW
  (`inc/dec/add/or/and/xchg [mem]` = 1 загрузка + 1 сохранение).
  `lea` (даже с mem-операндом) и `nop` **не считаются** загрузками
  (дефект v1 исправлен). `gs:`-доступы (чтение TLS-базы) посчитаны
  отдельной колонкой **tls** (они же включены в загрузки).
- **Сохранения**: `mov [mem],r/imm`, `movaps/movups [mem]`, запись memory-RMW.
- **Цепочки зависимых загрузок**: `mov r1,r2` переносит длину цепочки r2 → r1;
  загрузка `[base+…]` = цепочка(base)+1, присваивается dest; обрыв —
  `mov imm`, `lea`, `xor r,r`, `pop`; `call` затирает volatile
  (rax,rcx,rdx,r8–r11 → 0). В таблице — максимум по участку. **ОЦЕНКА**:
  статический максимум, не предсказание реального latency; ветвления не
  моделируются (учитываются все ветви участка).
- **Пролог/эпилог** — одного входа и одного возврата (не сумма по ветвям).
- **hit-участок** (slow-ветки срезаны) — линейный участок от входа до
  первого slow-ветвления; границы выбраны вручную по дизасму, **ОЦЕНКА**:
  - `sefer_a_alloc`: 0x00–0x35 вкл.; граница — `jae 0xD8` @0x35
    (heap не привязан → `bind_slow_tagged_with_config`/fallback);
  - `sefer_a_dealloc`: 0x00–0x3C вкл.; граница — `ja 0x40F` @0x3C
    (tag heap-указателя не пройден → slow-путь);
  - mi-шимы и `mi_free`/`mi_malloc_aligned` — полное тело (slow-веток в
    шимах нет; для mi_free/mi_malloc_aligned тело включает все ветви).

## Таблица v2

| сегмент                | insn | ld | tls | st | lock | jcc | pro/epi | chain (ОЦЕНКА) |
|------------------------|-----:|---:|----:|---:|-----:|----:|:-------:|---------------:|
| sefer alloc, полное    |  136 | 13 |   1 | 10 |    0 |  18 |   3/4   |       4        |
| sefer alloc, hit 0x00–0x35 | 12 | 5 |  1 |  1 |    0 |   1 |   3/4   |       3        |
| sefer dealloc, полное  |  406 | 47 |   1 | 31 |    0 |  65 |   9/10  |       6        |
| sefer dealloc, fast 0x00–0x3C | 18 | 5 | 1 | 1 |   0 |   1 |   9/10  |       3        |
| mi_a_alloc             |    5 |  4 |   1 |  1 |    0 |   0 |   0/0*  |       2        |
| mi_a_dealloc           |    5 |  4 |   1 |  1 |    0 |   0 |   0/0*  |       2        |
| mi_free                |   52 |  6 |   1 |  2 |    1 |   6 |   1/2   |       4        |
| mi_malloc_aligned      |   47 |  8 |   1 |  5 |    0 |   9 |   1/2   |       6        |

\* mi-шимы — tail-call `jmp` на `mi_malloc_aligned`/`mi_free`: пролога и
эпилога нет, включён только счётчик (5 инструкций: чтение `_tls_index`,
`gs:` TLS-базы, TLS-слот, `inc` счётчика, tail `jmp`).

lock: у sefer/mi-шимов 0; `mi_free` — 1 `lock cmpxchg` (CAS в
`mi_page_t`-freeing, часть самого mimalloc). Прологи/эпилоги (по одному
возврату): sefer_a_alloc — push rsi/rdi, `sub rsp,88h` (0x00–0x02);
эпилог 0xCE `add rsp,88h`, pop rdi/rsi, `ret` 0xD7. sefer_a_dealloc —
push r15..rbx (8 push), `sub rsp,38h` (0x00–0x0C); эпилог 0x2A1
`add rsp,38h` + 8 pop + tail `jmp` dealloc_large_at_base (0x2B1).
`inc qword ptr SHIM_CALLS_TL...` — plain RMW, слоты +0x00 (sefer_a),
+0x08 (sefer_b), +0x10 (mi_a), +0x18 (mi_b), +0x20/+0x28 (variant_*).

## Сравнение с v1

У шимов исчез `lock inc` атомарного счётчика — все `lock`-инструкции в
телах шимов: N → 0 (в v1 каждая из 18 shim-функций несла `lock inc` в
hot-пути; теперь счётчик — plain `inc` в thread-local слоте, для
sefer/variant-шимов остаточные `lock` есть только в realloc-телах и это
собственные fallback-локи аллокатора, симметрично в sefer и variant).

## Пер-вариантная верификация абляций (раунд 3)

Для каждого варианта ab01–ab12: генерация → сборка release-судьи →
пост-LTO объектник (save-temps) → dumpbin -DISASM → сравнение тел
`variant_a_alloc`/`variant_a_dealloc` с null-базой. МЕТОДИЧЕСКОЕ
ПРЕДУПРЕЖДЕНИЕ: судья линкует вариант по фиксированному пути
`../../.rush/variant/null`, поэтому копию `<variant>/src` нужно
синхронизировать в `.rush/variant/null/` перед каждой сборкой (и имя
варианта для генератора — полный стем патча, напр. `ab01_free_no_state_read`).
Контроль чистоты: `sefer_a_alloc`/`sefer_a_dealloc` во всех 13 сборках
байт-в-байт идентичны (136/406 insn).

| вариант | Δinsn alloc | Δinsn dealloc | исчезнувшая инструкция (адрес в null) | вердикт |
|---|---:|---:|---|---|
| ab01 | 0 | −5 | `movzx ebp,[rdi+r15+1588h]`@0x479 (load state-byte) + `cmp bpl,r8b`@0x486 + reject-`jne`; блок `mov eax,r15d`/`cmp rax,r8`@0x1E7–0x1EA | реальная |
| ab02 | 0 | −2 | `or bpl,40h`@0x4B4 + store `mov byte ptr [rdi+r15+1588h],bpl`@0x4B8 (MAG state) | реальная |
| ab03 | −2 | 0 | store `and byte ptr [rcx+rdx+1588h],3Fh`@0x0C6 (clear_magazine_on_issue) + адресная арифметика 0x0B1–0x0C0 — в alloc-плече | реальная |
| ab04 | 0 | −7 | `cmp qword ptr [rsi+130h],rax`@0x4B (memo) + load own_cache `mov rdi,[rsi+138h]`@0x54 + `movzx eax,[rdi+1Ch]`@0x5B + `cmp eax,2`@0x5F | реальная |
| ab05 | 0 | −21 | `and rax,…C00000`@0x45 + `cmp qword ptr [rsi+130h],rax`@0x4B + lookup `cmp qword ptr [rsi+rdx+30h],rax`@0x79 (own-cache) | реальная |
| ab06 | 0 | −13 | 3× load kind_at `movzx eax,byte ptr [rdi+1Ch]` + `cmp eax,2` (@0x5B, @0x98, @0x156) | реальная |
| ab07 | 0 | −4 | `cmp rax,qword ptr [rdi]`@0x48F; bump-cmp `mov eax,r15d; cmp rax,r8` (@0x1E7–0x1EA, @0x45F–0x462) сжаты до 32-битного `cmp r15d,eax` | реальная |
| ab08 | 0 | −1 | TLS range-check `lea rax,[rsi-1]`@0x34 + `cmp rax,0FFFFFFFFFFFFFFFDh`@0x38 → простой `test rsi,rsi` | реальная |
| ab09 | 0 | −9 | payload_start-цепочка `test al,al; movzx eax,al; cmp eax,2; cmp eax,1; mov r8d,42000h; mov eax,r15d; cmp rax,r8` (@0x1C4–0x1EA) | реальная |
| ab10 | 0 | −2 | store count `inc cl`@0x4C9 + `mov byte ptr [r12],cl`@0x4CB | реальная |
| ab11 | 0 | 0 | store state после dec `mov byte ptr [rax],r8b`@0xA5 исчез (декремент стал 8-битным `dec r8b; and r8b,0Fh`, добавлен `movzx`) — insn-нейтрально | реальная |
| ab12 | 0 | −29 | load `FREE_PARK_CAP[c]`: `movzx r13d,cl`@0x4A3 + `lea rax,[…FREE_PARK_CAP…]`@0x4A7 + `cmp r13b,byte ptr [r14+rax]`@0x4AE → `cmp cl,10h` (константа); выпали `panic_bounds_check` и slow-вызов | реальная |

Все 12 абляций подтверждены asm в теле шима; «НЕ ПОДТВЕРЖДЕНА asm» — нет.
lock-факт (null): alloc/dealloc всех шимов — 0; realloc sefer/variant —
симметрично по 5 (2× `lock cmpxchg` + 3× `lock inc` глобальных fallback-локов
аллокатора, @0xE6/@0x1B6/@0xF1/@0x2AC/@0x302).
Выжимки: `.rush/wsl/ab_r3_asm_<variant>.txt` (полные тела), сводка
`.rush/wsl/ab_r3_asm_summary.txt`, диффы `ab_r3_diff_all.txt`.
Коды возврата валидного прогона: gen/build/rustc/dumpbin rc=0 для всех 13
(null + ab01..ab12).

## Артефакты (в `.rush/wsl/`, имена без путей машины)

- `ab_v2_nolock_disasm.txt` — полный dumpbin-дизасм пост-LTO объектника v2;
- `ab_v2_nolock_shim_bodies.txt` — выжимки тел 18 шимов v2 (lock=0 везде);
- `ab_v2_nolock_summary.txt` — верификационная сводка v2;
- `ab_asm_metrics_v2.py` — скрипт метрик v2 (правила выше), rc=0;
- `ab_asm_mimalloc_c.txt` — дизасм mimalloc C (v1, не менялся;
  `mi_free` @ line 38663, `mi_malloc_aligned` @ line 419).

## Коды возврата / сборка

- сборка v2 (`cargo rustc ... -C save-temps`, см. Метод): rc=0;
- dumpbin -DISASM объектника: rc=0;
- `python ab_asm_metrics_v2.py`: rc=0;
- `git status --porcelain` после задачи: изменён только
  `docs/perf/b4_ablate/asm_notes.md` в репозитории.
