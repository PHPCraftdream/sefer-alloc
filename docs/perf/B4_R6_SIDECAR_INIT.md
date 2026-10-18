---
# B4 / R6: дешёвые init и Drop sidecar (System.alloc_zeroed + битмап присутствия листьев)

Дата: 2026-10-18. Волна B4, задача `b4-r6-sidecar-init` (worktree `worktrees/perf-cand`, ветка `b4-r6-sidecar-init`, база `4ba6d44a`). План-источник: `docs/perf/B1_MT_REGRESSION_ANALYSIS.md` §6.2 R6. Режим: правки + адресные тесты локально; бенчмарки НЕ запускались (их прогоняет оркестратор).

## 1. Что изменено

Проблема (после e8843374): регистрация маршрута = `System.alloc(32 KiB)` + 4096+1024 явных store'ов инициализации, а `ClassLeaves::Drop` обходил 1024 слота. Это фиксированная часть ×4 в iai `small_churn_16b` (attach) и churn+teardown ×1.5–1.7 медленнее mimalloc.

1. **`src/registry/segment_route/directory.rs:53-68`** — `EntryHandle::new` для `Small|Primordial`: `System.alloc` → `System.alloc_zeroed(Layout::new::<SmallSidecar>())`; вызов `SmallSidecar::initialize_at` удалён; `ClassLeaves::record_alloc(size, false → true)` (учёт zeroed-байтов честный). Восстановлено до-e8843374 поведение (там был `alloc_zeroed` всего sidecar).
2. **`src/registry/segment_route/small_sidecar.rs:24-30`** — `initialize_at` удалён; вместо него doc-контракт: backing строится из `System.alloc_zeroed`, все-нулевые байты — опубликованное валидное начальное состояние (null-указатели листьев, нулевые pending-слова, нулевые биты присутствия).
3. **`src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs`**:
   - модульный doc: контракт построения из нулевых байт, явного конструктора нет;
   - `:24` `const PRESENT_WORDS: usize = LEAVES / u64::BITS as usize;` (=16);
   - `:49` поле `present: [AtomicU64; PRESENT_WORDS]` рядом с `mixed` (компактный битмап присутствия 1024 листьев);
   - `:99-107` promotion (`prepare`): бит присутствия ставится `fetch_or(..., Release)` СТРОГО после `self.mixed[leaf].store(fresh, Release)` — будущий читатель бита видит и полностью инициализированный лист;
   - `:217-247` `Drop` — обход только установленных битов (16 слов, `trailing_zeros`), `System.dealloc` только для них + `record_free` на каждый; под `debug_assertions` — полная проверка инварианта «бит ⇔ ненулевой указатель» по всем 1024 слотам;
   - удалён `ClassLeaves::initialize_at` и неиспользуемый `use core::ptr;`.
4. **`src/lib.rs:257-263`, `README.md:722-723`** — описания unsafe-слоёв small_sidecar/leaf_classes приведены к новой механике.
5. **`scripts/check-matrix.mjs`** — note строки `test-r6-t40-sidecar-system-alloc` обновлён (описание теста).

Размер `SmallSidecar`: 40 960 → 41 088 байт (+128 = 16 слов битмапа присутствия).

## 2. Почему это безопасно

- **Нулевое состояние валидно.** Все поля — атомики над plain-типами: `AtomicU64` (pending, present), `AtomicU8` (байты классов — в System-листьях, не в backing), `AtomicPtr<AtomicU8>` (mixed). Для целочисленных атомиков ноль — валидное значение; для указателей гарантия Rust документирована: все-нулевая битовая комбинация валидна и даёт null. Семантика нуля: pending-бит 0 = «ничего не опубликовано»; `SidecarBitmap::publish` отвергает encoded-класс 0 (`sidecar_bitmap.rs:117`), т.е. обнулённый sidecar не публикует ничего — ровно состояние «до первого issue». Так было до e8843374.
- **Promotion — owner-only.** `ClassLeaves::prepare` достигается только через `RouteRegistration::{prepare_small, issue_small}` (владелец, `!Sync` через `PhantomData<Cell<()>>` — registration.rs:13) и owner-путь `RouteSlots::prepare_small_issue`. Продюсеры (`RoutePin::publish_small` → `SidecarBitmap::publish`) читают только указатель листа и байт класса (Acquire) и делают RMW `pending`; биты присутствия они не читают и не пишут. Гонок на `present` нет: один писатель-владелец, читатель — только owner-Drop после unlink и последнего пина (Relaxed-загрузки достаточны).
- **Инвариант «бит ⇔ указатель» поддерживается в одном месте** — `prepare` (бит после Release-публикации указателя) — и проверяется полным debug-обходом в `Drop` (leaf_classes.rs:239-247); для release-путей инвариант держится конструкцией: единственный писатель бита — promotion, единственный писатель указателя — promotion, биты не сбрасываются.
- **Drop полон.** Освобождаются ровно листья с установленными битами — по построению то же множество, что раньше определялось `!p.is_null()` (множества совпадают по инварианту), поэтому quiescent-оракулы баланса System-totals остаются точными (тесты ниже).
- **OOM-семантика C6 не тронута.** Leaf-spill по-прежнему fallible `System.alloc(256 B)` + 256 store'ов внутри `prepare` (это promotion, не attach; инъекции `fail_next_spills`/`fail_spill_after` не менялись); prepare-до-мутаций в issue-транзакциях не менялся. `tests/r11_p3_small_sidecar_{oom,zeroed_oom,preflight,routes}` зелёные.
- **Протокол публикации не менялся:** терминальная RMW-точка продюсера, owner-drain, ledger, incarnation — без изменений; новый RMW (`present`) — owner-приватный, вне протокола производитель↔владелец.

## 3. Тесты

Обновлены (кодируют новую механику; старые ассерты были красными на новом дереве by construction — `zeroed delta == 0` vs новый `== size`):
- `tests/r6_t40_sidecar_system_alloc.rs` — zeroed-delta == size (System.alloc_zeroed backing), имя теста обновлено;
- `tests/r11_p3_small_sidecar_routes.rs` — `40_960` → `size_of::<SmallSidecar>()` (3 места), zeroed-delta == size;
- `tests/sidecar_granule_index_exhaustive.rs`, `tests/class_leaves_publication_miri.rs`, `tests/loom_sidecar_bitmap_actual.rs` — конструирование `ClassLeaves` из нулевых байт (`Box::new_zeroed().assume_init()`), без `initialize_at`;
- `tests/r11_ph5b_c5_sidecar_ordering_pinned.rs` — pin-инвентарь `prepare` = `store(Release)` + `fetch_or(Release)`; новые пины `promotion_sets_presence_bit_after_leaf_publication` (бит строго после публикации указателя) и `drop_walks_presence_bits_not_the_full_slot_array`.

Новые:
- `tests/r6_b4_sidecar_registration_zero_state.rs` (`alloc-global internals`) — (а) после регистрации маршрута ВСЕ pending-биты читаются нулём (exhaustive по 262 144 гранулам; под Miri — выборка) и все класс-коды = None, `mixed_leaves_for_test == 0`. Counterfactual: удаление обнуления (alloc без zeroed-контракта) делает ассерты вероятностно красными (на свежих OS-страницах — нет); детерминированный контракт несут обновлённые totals-ассерты r6_t40/r11_routes (старые `== 0` красные на новом дереве) и pin-тесты (красные без изменения исходника).
- `tests/r6_b4_class_leaves_drop_frees_promoted_leaves.rs` (`alloc-global internals bench-internals`) — (б) Drop освобождает ровно установленные листья: promotion листьев {0,1,63,64,1023} (слова present 0/1/15), totals: alloc == zeroed+5·256, free == 0 при живом пине, после последнего пина free == alloc == size+5·256, counts 6/6. Counterfactual (исполнен, записан): мутант «не ставить бит в prepare» → RED `leaf presence invariant broken: leaf 0` (и totals); мутант «Drop идёт не по битам/освобождает не всё» → красный по totals (листья в словах 1 и 15).

## 4. Локальные прогоны (Windows, CARGO_TARGET_DIR=.rush/cargo-target-B)

- Все адресные цели зелёные, exit 0: `r6_t40_sidecar_system_alloc` (production,internals,bench-internals), `r11_p3_small_sidecar_routes` (2), `r11_p3_small_sidecar_issue_oom`, `r11_p3_small_sidecar_zeroed_oom` (alloc-global,internals,bench-internals), `r11_p3_small_sidecar_preflight` (production,internals,bench-internals), `r6_route_directory` (9), `r6_route_directory_global`, `r6_route_directory_model` (3), `r6_class_issue` (3) (alloc-global,internals), `sidecar_granule_index_exhaustive`, `class_leaves_publication_miri` (нативно), `r6_sidecar_bitmap` (5) (alloc-core), `r11_ph5b_c5_sidecar_ordering_pinned` (8, дефолт), оба новых теста, `loom_sidecar_bitmap_actual` собирается (0 тестов без `--cfg loom`).
- `cargo check` production и production+hardened, `cargo clippy --lib -D warnings` (production,internals,bench-internals) — зелёные. Клайппи `--tests` на той же фиче-комбо падает в НЕтронутом `tests/con01_exit_stranded_lock.rs` (`ExactShard` never constructed) — pre-existing cfg-комбинация (файл требует bench-internals и не входит в CI-clippy-строки); вне scope.
- `cargo fmt` в этом worktree падает с OS error 206 (лимиты путей Windows); использован прямой `rustfmt --edition 2021` по изменённым файлам. CI-прогон `cargo fmt --check` на коротких путях раннера не затронут.

## 5. Что прогнать на CI (оркестратор)

- Miri: `cargo miri test --features "alloc-core" --test class_leaves_publication_miri` (ci.yml:3182) — оракул Release-публикации листа на реальном ClassLeaves (новая механика конструирования + promotion).
- Loom: `tests/loom_sidecar_bitmap_actual.rs` (scripts/loom.mjs: `alloc-core,alloc-xthread,tagged-index-stack/loom`) — реальные `leaf_classes.rs` под loom-атомиками (новый `fetch_or` на present — owner-only, новых межпоточных рёбер нет); `loom_r11_small_sidecar.rs` (ci.yml:3453).
- Тест-строки check-matrix/CI: `test-r8-production`, `test-r8-hardened-numa-directory` (подхватят оба новых теста и обновлённые), `test-r6-t40-sidecar-system-alloc`, hardened-строка `r11_p3_small_sidecar_preflight` (ci.yml:1940).
- iai (замеряет оркестратор): `small_churn_16b` — фиксированная часть (attach: 5120 store → alloc_zeroed) и teardown (Drop 1024 load → ≤16 слов); критерии B1 §7.4 R6: фикс. часть и teardown-константа ↓ в 2–4 раза, throughput ~без изменений. Плечи criterion teardown (wall-clock) — churn+teardown ×1.7/1.6/1.5 должны сократиться.

## 6. Риски и ограничения

- Release-Drop при нарушенном инварианте (бит без указателя) делал бы `dealloc(null)` = UB; инвариант owner-only, полная debug-проверка в Drop + пины исходника; недостижимо из пользовательского кода (нет входов, кроме owner-promotion).
- `alloc_zeroed` на Windows (HeapAlloc HEAP_ZERO_MEMORY) может мемсетить внутри — выигрыш attach в первую очередь на Linux calloc (iai-гейт); wall-clock Windows-бенчи получат меньше.
- +128 Б на маршрут RSS; исторические гейт-скрипты с жёстким 40 960 (`scripts/r20_prf01_c3_gate.mjs`, `scripts/r20_prf01_c3_report_tables.mjs`) относятся к прошлым раундам и уже расходились с деревом после R6_T40 (41 984 → 40 960) — не обновлялись по прецеденту.
- Counterfactual (б-a) «alloc без zeroing» не детерминирован локально: accounting-флаг статичен, свежие страницы Windows нулевые; контракт кодируется totals-ассертами и pin-тестами (см. §3).
- Файл отчёта не содержит замеров — приёмка по метрике iai у оркестратора (B1 §7.4 строка R6).

## 7. Коммит

Префикс: `perf(runtime)`.
Предлагаемое сообщение:
```
perf(runtime): cheap sidecar init and drop via zeroed backing and presence bits

Route registration now constructs SmallSidecar with System.alloc_zeroed
(the all-zero byte pattern is the published valid initial state: publish
rejects encoded class 0), removing the 5120 explicit init stores, and
ClassLeaves gains a 16-word owner-private presence bitmap: promotion sets
the bit Release-after the leaf pointer publication, Drop walks only the
set bits (debug builds verify the bit<->pointer invariant exhaustively).
size_of::<SmallSidecar>() 40960 -> 41088 (+128 B). OOM/C6 semantics,
publication protocol and producer path unchanged.

Targets: iai small_churn_16b fixed part and teardown; churn+teardown
wall-clock. Evidence: r11_ph5b_c5 pins, new r6_b4_* tests (presence-mutant
RED recorded), r11_p3/r6_route_directory suites green locally.
```
