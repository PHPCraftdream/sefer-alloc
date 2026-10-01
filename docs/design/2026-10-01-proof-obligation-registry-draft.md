# Реестр обязательств доказательства: черновик (шаг 7 плана, введён в Фазе 0)

Дата: 2026-10-01. План: `docs/design/2026-10-01-113842-src-foundational-refactoring-plan-xs-sol.md` §4 шаг 7. Это **черновик структуры + первые реальные строки** из `docs/reviews/2026-10-01-refactor-ph0-obligations-receipt.md`; автоматической генерации и CI-проверки пока нет. Каждую фазу плана дополняет интегратор.

## 1. Структура записи

Одна запись = одна клетка `transition × entry × features × OS × toolchain × profile`.

| Поле | Содержание |
| --- | --- |
| `id` | `R<фаза>-<n>`, стабильный |
| `transition` | какой переход/обещание доказывается (напр. «terminal publication -> owner retire при paused producer в `Box::drop`») |
| `entry` | точный entry point: `#[global_allocator] SeferAlloc` / `HeapCore::…` / `AllocCore::…` / isolated installed-System / Loom shadow-модель |
| `features` | точный набор Cargo features |
| `OS` | ОС/target (для Miri — host-target и что `System` — аллокатор Miri) |
| `toolchain` | rustc / miri / loom версия |
| `profile` | dev/release, `MIRIFLAGS`/`RUSTFLAGS` (`--cfg loom`), модель borrow |
| `source/binary ID` | базовый commit + tree, `git diff \| sha256sum` незакоммиченного, sha256 затронутых файлов; binary hash, если зафиксирован (иначе `—`) |
| `activation` | наблюдаемый маркер, что именно этот путь исполнен (marker-строка, assert оракула, счётчик); без него `PASS` не ставится |
| `positive/mutant` | положительный свидетель и отрицательная мутация/контроль, ломающие именно это обещание |
| `receipt` | документ + имя сырого лога (+ sha256 лога, если он закоммичен) |
| `status` | `PASS` / `FAIL` / `BUILD_ONLY` / `CFG_EXCLUDED` / `NOT_RUN` / `INCONCLUSIVE` / `CONDITIONAL`; `PASS` для строк-мутантов трактуется как «мутант отвергнут» и помечается `mutant-rejected` |

## 2. Запрещённые автоповышения (правила судьи)

Судья реестра должен отвергать (статус не повышается автоматически):
1. `shadow Loom` -> shipping (Loom не доказывает provenance и реальный путь).
2. `low-level Miri` (прямой Heap/Alloc API) -> installed `Box`/`GlobalAlloc`.
3. `0 tests` / `CFG_EXCLUDED` / `BUILD_ONLY` -> `PASS` (cfg-пустая клетка не зелёная).
4. `System requested bytes` / счётчик шагов -> RSS/latency.
5. isolated System witness (W+1/W+2) -> Sefer acceptance.
6. Strict Miri в одной модели (SB) -> вторая модель (TB), и наоборот.
7. Результат другого commit/binary (реестр обязан отражать исходный identity; иначе verdict не публикуется).
Новая feature-конфигурация или unsafe-переход добавляет обязательную клетку; пока нет receipt, она `NOT_RUN`.

## 3. Общая идентичность первых строк

- База: `ceffecb5c07d45e9d715670dfcaec3de69b2f9d2`, tree `c1fa03eae961c5ae2b442ec963c2e3978c0b3941`.
- Незакоммиченное: `tests/support/r8_global_box_witness.rs` -> sha256 `9a7007c5...c1e4` (полный в записке §1), `git diff | sha256sum` = `0004c9834e6f...c042dc053` (`0004c9834e6fd24bddc84184681038022b71aef0197087b5b5c63507e42dc053`). Строки с префиксом лога `v3-` измерены на нём; без префикса `sb-/tb-/native-` — на HEAD-версии (sha `29fff9c6...`).
- Toolchain: native `rustc 1.97.0 (2d8144b78 2026-07-07)`; Miri `miri 0.1.0 (3659db0d3e 2026-07-05)` / `rustc 1.99.0-nightly (3659db0d3 2026-07-05)`. OS: Windows 10 x86_64 MSVC.
- Логи: `D:/dev/rust/sefer-alloc/target/wt/logs/ph0/` (вне репо; в записке выдержки). Binary hash не фиксировался (`—`).
- Miri-флаги `S` = `-Zmiri-strict-provenance -Zmiri-disable-isolation -Zmiri-report-progress=10000000`; `T` = `S -Zmiri-tree-borrows`; `P` = `-Zmiri-disable-isolation -Zmiri-preemption-rate=0.5`; `PT` = `P -Zmiri-tree-borrows`.

## 4. Записи

Features-сокращения: `AG` = `alloc-global internals bench-internals`; `AGX` = `alloc-global alloc-xthread internals bench-internals`; `STD` = `std`; `AC` = `alloc-core alloc-xthread`.

| id | transition | entry | features | OS | profile | activation | positive/mutant | receipt (лог) | status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| R0-01 | terminal publication -> owner retire (strict trim) пока producer приостановлен после terminal RMW внутри `Box::drop` | installed `#[global_allocator] SeferAlloc`, `Box<u8>`, `miri_global_box_acceptance -- paused` | AG | Win x86_64, host-Miri | Miri SB, `S`, dev | assert `pending_for_test(ptr)` false до `arm`, true после `wait_for_publication`; protector создан в `drop(byte)` (`r8_global_box_witness.rs:113`) | positive-свидетель; мутант-контроль W−1 (R0-12..15) на уровне System | `v3-sb-sefer-paused.log`: UB `not granting access to tag <1479> ... weakly protected` at `node.rs:90`; exit 1 | **FAIL** |
| R0-02 | то же | то же | AG | Win, host-Miri | Miri TB, `T`, dev | то же | то же | `v3-tb-sefer-paused.log`: `write access through <1414> (root of the allocation) ... forbidden`, Reserved->Disabled at `node.rs:90`; exit 1 | **FAIL** |
| R0-03 | то же | то же | AG | ubuntu-latest (CI `miri-core`, `ci.yml:2831-2845`) | Miri SB/TB | CI marker `COMPLETE paused_terminal_owner_retirement` | то же | нет результата CI; R11 manifest описывает тот же отказ | **NOT_RUN** |
| R0-04 | owner retire после join, narrow reborrow, 2 раунда (`Box<u8>`, `Box<[u8;7]>`), reissue | installed SeferAlloc, `miri_global_box_acceptance -- narrow` | AG | Win, host-Miri | Miri SB, `S`, dev | marker `COMPLETE narrow_two_rounds`, `assert_retired` | positive; мутант не определён для этого свидетеля | `v3-sb-sefer-narrow.log` exit 0 | **PASS** (не покрывает паузу внутри кадра) |
| R0-05 | то же | то же | AG | Win, host-Miri | Miri TB, `T`, dev | то же | то же | `v3-tb-sefer-narrow.log` exit 0 | **PASS** (то же ограничение) |
| R0-06 | то же | `r8_global_box_provenance` (libtest, installed SeferAlloc) | AG | Win | native debug | `test ... ok`, 1 из 1 | positive | `v3-native-r8_global_box_provenance.log` | **PASS**; `paused` в этом target **CFG_EXCLUDED** (`cfg(miri)`) |
| R0-07 | producer сам физически освобождает exact System-объект до terminal RMW; marker после join | isolated installed-System `r11_box_cap_gate_w1 -- positive` | STD | Win | native debug | `ROUND ... joined` 14 из 14 + `COMPLETE owner_free=false sizes=1..7 rounds=2` | positive; контрфактуал pause-after-RMW в записке `r11-box-cap-gate-w1-w2-receipt` (не перезапускался) | `native-w1-positive.log` | **PASS** (не Sefer) |
| R0-08 | то же | то же | STD | Win, host-Miri | Miri SB `S` / TB `T` | 14 из 14 ROUND joined + COMPLETE, обе модели | positive | `sb-w1-positive.log`, `tb-w1-positive.log` | **PASS** x2 (не Sefer) |
| R0-09 | producer передаёт cap, owner `System.dealloc(cap, layout)` при паузе внутри `Box::drop`, затем `alloc_zeroed` | isolated installed-System `r11_box_cap_gate_w2 -- positive` | STD | Win | native debug | 14 из 14 ROUND + `COMPLETE owner_free=true sizes=1..7 rounds=2` | positive; контрфактуал dealloc по root вместо cap красный (записка W+1/W+2, не перезапускался) | `native-w2-positive.log` | **PASS** (не Sefer; W+3 не проверен) |
| R0-10 | то же | то же | STD | Win, host-Miri | Miri SB `S` / TB `T` | 14 из 14 + COMPLETE | positive | `sb-w2-positive.log`, `tb-w2-positive.log` | **PASS** x2 (не Sefer) |
| R0-11 | root-write через исходный root-pointer при паузе должен отвергаться (W−1) | isolated installed-System `r11_box_cap_gate_w2 -- root-one` / `root-eight` | STD | Win | native debug | `process::exit(3)` после записи недостижим | **mutant** | — | **CFG_EXCLUDED** (`cfg(miri)`) |
| R0-12 | W−1 1 байт | `root-one` | STD | Win, host-Miri | Miri SB `S` | UB на `r11_box_cap_gate.rs:136`, exit 1 (не 3) | **mutant** | `sb-w2-root-one.log`: `not granting access to tag <9352> ... [Unique for <14995>] weakly protected` | **PASS** `mutant-rejected` |
| R0-13 | W−1 8 байт | `root-eight` | STD | Win, host-Miri | Miri SB `S` | UB на `:144`, exit 1 | **mutant** | `sb-w2-root-eight.log`: `<10067> ... [Unique for <15676>]` | **PASS** `mutant-rejected` |
| R0-14 | W−1 1 байт | `root-one` | STD | Win, host-Miri | Miri TB `T` | UB `write access through <9079> (root of the allocation) ... forbidden`, exit 1 | **mutant** | `tb-w2-root-one.log` | **PASS** `mutant-rejected` |
| R0-15 | W−1 8 байт | `root-eight` | STD | Win, host-Miri | Miri TB `T` | UB `<9777> ... forbidden`, exit 1 | **mutant** | `tb-w2-root-eight.log` | **PASS** `mutant-rejected` |
| R0-16 | foreign terminal free виден strict trim, retire once (прямой SeferAlloc/Heap API, не installed Box) | `r6_terminal_owner_drain`+`r8_owner_sidecar_miss`+`r8_terminal_global` | `production batch-api internals bench-internals` | Win | native debug | 8+4+6 = 18 из 18 `ok` | positive; ограничение: нет паузы в `Box::drop` | `native-production-terminal.log` | **PASS** |
| R0-17 | то же, non-fastbin | то же | `alloc-global alloc-xthread alloc-decommit alloc-segment-directory batch-api internals bench-internals` | Win | native debug | 7+2+6 = 15 из 15 `ok` | positive | `native-nonfastbin-terminal.log` | **PASS** |
| R0-18 | то же | то же (CI `global-regressions`) | как R0-16/17 | ubuntu-latest | native | CI grep-строки `ci.yml:2029-2038` | positive | нет результата CI | **NOT_RUN** |
| R0-19 | retire once + reissue, размеры 1..7 (low-level Miri) | `r6_terminal_owner_drain::requested_sizes_one_through_seven_retire_once_and_reissue` | AGX | Win, host-Miri | Miri SB-без-strict, `P` | `test ... ok`, 1 из 1 | positive | `sb-r6-owner-drain-tiny.log` (48.13s) | **PASS** (low-level != installed Box) |
| R0-20 | то же | то же | AGX | Win, host-Miri | Miri TB-без-strict, `PT` | то же | positive | `tb-r6-owner-drain-tiny.log` (30.18s) | **PASS** (то же) |
| R0-21 | shadow-протокол sidecar bitmap / terminal large / owner drain | `loom_sidecar_bitmap` (3), `loom_terminal_large` (2), `loom_terminal_owner_drain` (2) | AC | Win | Loom, `--cfg loom --release` | 7 из 7 `ok`; 2 теста `should_panic` | **mutant**: `loom_sidecar_negative_class_after_terminal_bit`, `negative_producer_retirement_allows_premature_release` отвергнуты (`should panic ... ok`) | `loom-xthread.log` | **PASS** (shadow, не provenance) |
| R0-22 | pointer publication / last-pin retention / early leaf release | `loom_r11_small_sidecar` (3) | AC | Win | Loom, `--cfg loom --release` | 3 из 3 `ok` | **mutant**: `negative_relaxed_pointer_publication`, `negative_early_leaf_release` отвергнуты | `loom-r11-small-sidecar.log` | **PASS** (shadow) |
| R0-23 | autonomous ownerless progress: last free после ухода owner без новых вызовов; worker start failure | installed SeferAlloc + `start_maintenance()` | `production` | любая | native + Miri | — | — | — | **NOT_RUN** (в Фазе 0 не проверялось; ACTIVE.md:165) |
| R0-24 | W+3: reissue по удержанному exact-layout capability без physical dealloc | isolated/Sefer | — | — | Miri SB/TB | — | — | — | **NOT_RUN** (шаг 1a) |
| R0-25 | cost: CPU/op, RSS/commit, p99, System-вызовы нового backend | installed SeferAlloc, A/B | `production` | — | measurement | activation, resolved config | — | — | **NOT_RUN** (шаг 6b) |
| R0-26 | R0-01..R0-23 на Linux/macOS/aarch64 | — | — | non-Windows | — | — | — | — | **NOT_RUN** |

## 5. Пробелы черновика

- Binary/exe hash и sha256 логов не фиксировались: логи вне репо, в записке только выдержки.
- Значения `PASS` для R0-07/R0-09 опираются на контрфактуалы документа `2026-10-01-r11-box-cap-gate-w1-w2-receipt.md`; в Фазе 0 они не перезапускались.
- Поле `mutant` для installed-Sefer (R0-01..R0-05) пока не определено: мутанты «вернуть root `Node::{write_next,read_next}`», «пропустить terminal RMW», «освободить descriptor при живом pin» требуют правки `src/` и принадлежат шагу 3c.
- Генерация CI/doc-ссылок из реестра и судья «green by cfg exclusion» — шаг 7, не реализованы.
