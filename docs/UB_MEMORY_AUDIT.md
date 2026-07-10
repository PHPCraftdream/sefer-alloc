# Углублённый аудит UB и безопасности памяти

Дата: 2026-07-10  
Ревизия: `c68f64e` (`main`)  
Проверенный код: `src/`, `crates/`, `tests/`, `fuzz/`, allocator-related Cargo features и релевантная проектная документация. Исправления в код не вносились.

## Краткий итог

Подтверждены две серьёзные проблемы публичной safe-границы `AllocCore`, одна аналогичная проблема в публичных `#[doc(hidden)]` test hooks, один реально воспроизводимый residual corruption-сценарий рекомендуемого `production`-профиля при ошибочном double-free, а также две проблемы benchmark-crate. На путях корректного `GlobalAlloc`-использования новых подтверждённых гонок, UAF, double-release или Layout/alignment-нарушений не найдено.

Severity в этом отчёте оценивает достижимость и последствия, а не только вероятность в ожидаемом workload. Для сценариев, требующих нарушения unsafe-контракта `GlobalAlloc`, это явно отмечено.

## Подтверждённые находки

### F1 — CRITICAL — публичный safe `AllocCore::realloc` разыменовывает произвольный или неверно описанный указатель

**Файлы и строки:**

- `src/alloc_core/alloc_core.rs:2160-2192` — safe `pub fn realloc`; membership проверяется только для in-place fast path, после чего любой указатель попадает в alloc/copy/dealloc move path.
- `src/alloc_core/node.rs:146-154` — `Node::copy_nonoverlapping` выполняет настоящий `ptr::copy_nonoverlapping`.
- `src/registry/heap_core.rs:1159-1267` — тот же дефект присутствует в safe `HeapCore::realloc`; foreign path безусловно копирует из `ptr`.
- `src/lib.rs:169-193` — проект прямо признаёт, что safe membrane-функции полагаются на prose-контракты, нарушение которых из safe-кода приводит к UB.
- `src/lib.rs:279-280` — `AllocCore` публично реэкспортирован.

**Почему это опасно:** Rust safe-функция не может возлагать на safe caller невидимую обязанность обеспечить валидность raw pointer, если нарушение этой обязанности позволяет самой функции выполнить UB. `AllocCore::realloc` принимает любой `*mut u8`; при foreign pointer `self.table.contains_base(base)` возвращает `false`, однако функция всё равно выделяет новый блок и читает `min(old_layout.size(), new_size)` байт из `ptr`. Аналогично, указатель на собственный блок с завышенным `old_layout` вызывает чтение за границами старой аллокации. Для вызова самой функции `unsafe` не требуется.

Это не просто отсутствие defensive hardening для `GlobalAlloc`: `AllocCore::realloc` является отдельным публичным safe API. Возможные последствия — invalid read/access violation, OOB read, чтение уже освобождённой памяти и последующая порча allocator state на `dealloc`.

**Минимальный сценарий:**

```rust
use sefer_alloc::AllocCore;
use std::alloc::Layout;

let mut a = AllocCore::new().unwrap();
let old = Layout::from_size_align(8, 8).unwrap();
let bogus = 1usize as *mut u8;

// Вызов целиком safe, но внутри будет чтение 8 байт из адреса 0x1.
let _ = a.realloc(bogus, old, 16);
```

В терминах Rust abstract machine это UB; на обычной ОС наиболее вероятен access violation. Вариант с реальным pointer и неверно большим `old_layout` даёт OOB read без необходимости использовать заведомо unmapped address.

### F2 — HIGH — safe `AllocCore::dealloc` не выполняет заявленный no-op контракт для stale/interior pointers и может повторно выдать живой блок

**Файлы и строки:**

- `src/alloc_core/alloc_core.rs:658-666` — документация обещает: safe entry point, foreign pointer или double-free — no-op, «never UB, never corrupts».
- `src/alloc_core/alloc_core.rs:698-894` — membership проверяет только segment base; принадлежность конкретной активной аллокации не проверяется.
- `src/alloc_core/alloc_core.rs:3910-3966` — small free индексирует bitmap и записывает intrusive `next` в переданный адрес.
- `src/alloc_core/alloc_core.rs:3914-3933` — block-start/interior-pointer проверка включена только под `hardened`.
- `Cargo.toml:186-203` — `hardened` opt-in и не входит в `production`.
- `tests/regression_hardened_interior_ptr.rs:1-141` — сам проект документирует и тестирует, что без guard interior pointer попадает в magazine/freelist и выдаётся как блок.

**Почему это опасно:** bitmap различает «сейчас free» и «сейчас allocated», но не lifetime. После `free(P) -> alloc() == P` старое значение raw pointer `P` становится stale. Повторный safe `dealloc(P, layout)` видит bitmap текущего occupant как allocated, пишет freelist link в его живые пользовательские байты и помечает блок свободным. Следующий `alloc` возвращает адрес уже живого блока второй раз. `hardened` это не исправляет: generation передаётся только через hardened remote-ring protocol, а direct `dealloc` не получает generation/lifetime token.

Отдельный non-hardened вариант — `P.wrapping_add(16)` внутри 48-byte блока. Segment membership проходит, 16-byte-granular bitmap смотрит другой «allocated» bit, после чего mid-block address записывается во freelist. Непосредственное подтверждение этого контрфакта уже содержится в `regression_hardened_interior_ptr`.

**Минимальный сценарий stale-after-reuse (полностью safe до момента фактического доступа к raw allocation):**

```rust
use sefer_alloc::AllocCore;
use std::alloc::Layout;

let mut a = AllocCore::new().unwrap();
let l = Layout::from_size_align(16, 16).unwrap();
let stale = a.alloc(l);
a.dealloc(stale, l);
let live = a.alloc(l);
assert_eq!(live, stale);       // LIFO reuse
a.dealloc(stale, l);           // stale free текущего live occupant
let duplicate = a.alloc(l);
assert_eq!(duplicate, live);   // один блок выдан двум логическим владельцам
```

Этот сценарий был отдельно выполнен против текущего кода: последнее равенство подтвердилось. Следующая запись через любой из двух raw pointers создаёт обычную overlapping-live-allocation corruption ситуацию.

### F3 — HIGH — публичные safe `#[doc(hidden)]` test hooks позволяют UB из внешнего safe-кода

**Файлы и строки:**

- `src/lib.rs:234-242` — весь `alloc_core` сделан `pub`, чтобы integration tests могли достигать test surface; `#[doc(hidden)]` только скрывает документацию и не ограничивает доступ.
- `src/alloc_core/remote_free_ring.rs:487-510` — safe `over_test_buffer`/`init_test_buffer` принимают произвольный raw pointer с prose-only требованием размера/alignment/lifetime.
- `src/alloc_core/remote_free_ring.rs:555-565` — `init_test_buffer` в итоге пишет cursors и все slots по переданному адресу.
- `src/alloc_core/alloc_core.rs:2726-2757` — safe public `flush_class` принимает caller-controlled raw pointers и передаёт их в `flush_run` без проверки `table.contains_base`.
- `src/alloc_core/alloc_core.rs:2777-2851` — `flush_run` немедленно читает/пишет segment metadata и тела блоков по вычисленному base.
- `src/alloc_core/segment_header.rs:1402-1446` — публичные safe `gen_at`/`bump_gen` материализуют atomic view по caller-provided base/offset.

**Почему это опасно:** `#[doc(hidden)]` не является visibility или safety boundary. Любой downstream crate с соответствующей feature может вызвать эти функции без `unsafe`; неверный адрес приводит к invalid atomic reference, out-of-bounds read/write или metadata corruption. Комментарий «test-only» не делает UB обязанностью safe caller.

**Минимальный сценарий:**

```rust
use sefer_alloc::alloc_core::remote_free_ring::RemoteFreeRing;

// Доступно при alloc-xthread, unsafe не требуется.
RemoteFreeRing::init_test_buffer(1usize as *mut u8);
```

Функция попытается записать ring metadata начиная с адреса `0x1`. Аналогичный сценарий возможен через `AllocCore::flush_class(0, &[bogus])`.

### F4 — MEDIUM — `production` сохраняет подтверждённую ring↔magazine stale-note corruption после cross-thread double-free

**Условие:** сценарий начинается с double-free, то есть нарушает unsafe-контракт `GlobalAlloc`. Поэтому это не soundness-баг корректного `SeferAlloc` caller. Это, однако, реальная memory corruption в заявленной allocator defensive/M2 модели, а не теоретическое замечание.

**Файлы и строки:**

- `Cargo.toml:162-163,186-203` — рекомендуемый `production` включает `fastbin`, но не `hardened`.
- `src/registry/heap_core.rs:963-1003` — residual «re-issue-before-drain» прямо описан в production free path.
- `src/registry/heap_core.rs:1420-1459` — generation записывается в remote ring entry только под `hardened`.
- `src/alloc_core/alloc_core.rs:943-1057` — drain/reclaim; без совпадающей generation stale note может выполнить `write_next`, `mark_free` и `dec_live` для нового occupant.
- `tests/regression_xthread_double_free_residual.rs:1-69,105-188` — точный deterministic reproducer оставлен `#[ignore]` для non-hardened профилей.
- `tests/regression_xthread_double_free_residual.rs:367-470` — тот же interleaving проходит с hardened generation check.
- `src/alloc_core/remote_free_ring.rs:242-246` и `tests/regression_gen_wrap_boundary.rs` — даже hardened использует `u8` generation и принимает residual после 256 reissues.

**Механизм:** remote free кладёт `(offset,class)` в ring, own-thread ошибочно освобождает тот же `P` в magazine, затем allocator повторно выдаёт `P`. Поздний drain считает старую ring note актуальной, пишет freelist link в живой `P`, уничтожая пользовательские байты, и делает `P` повторно выдаваемым. В `hardened` note несёт generation и отбрасывается после reissue; в `production` такой информации нет.

**Минимальное подтверждение:** выполнена существующая ignored-регрессия:

```text
cargo test --features production --test regression_xthread_double_free_residual \
  residual_xthread_double_free_no_corruption -- --ignored --exact
```

Результат: **FAILED** в `tests/regression_xthread_double_free_residual.rs:156`; sentinel изменился с `0x5efe5efe5efe5efe` на `0x0000000000000000` после `write_next`. Тот же тестовый набор с `production hardened` прошёл, включая `residual_xthread_double_free_no_corruption_hardened`.

### F5 — MEDIUM — safe generic API `malloc-bench-rs::run` допускает dealloc через другой allocator instance

**Файлы и строки:**

- `crates/malloc-bench/src/lib.rs:187-194` — mailbox освобождает полученный блок через локальный `a`.
- `crates/malloc-bench/src/lib.rs:389-406` — документация честно требует stateless facade/shared global state.
- `crates/malloc-bench/src/lib.rs:435-469` — safe `run`, bounds только `A: GlobalAlloc + Send + 'static`, отдельный `A` создаётся на каждый поток.
- `crates/malloc-bench/src/lib.rs:473-496` — unsafe worker/dealloc вызываются из safe `run`.

**Почему это опасно:** типовая сигнатура не обеспечивает заявленное требование. Полностью корректный stateful `GlobalAlloc`, где каждый `A` владеет отдельной arena, удовлетворяет текущим bounds. Cross-thread handoff освобождает блок через другой instance, нарушая `GlobalAlloc::dealloc` contract и потенциально вызывая foreign free, double-free или corruption. Документированный prose-контракт полезен, но safe public function не может допускать UB при типологически корректном safe вызове.

**Минимальный сценарий:** передать в `run(Workload::Larson, config_with_2_threads, make_alloc)` фабрику, создающую независимый arena allocator на каждый вызов. На первом успешном cross-thread handoff receiver вызовет `free_block(&receiver_allocator, block_from_sender_allocator)`.

Эта находка относится к benchmark-crate, а не runtime `sefer-alloc`.

### F6 — LOW — потеря allocation при ошибке отправки в `malloc-bench-rs`

**Файлы и строки:**

- `crates/malloc-bench/src/lib.rs:245-249` — комментарий обещает освободить блок локально при `send` error, но `SendError<Block>` игнорируется; содержащийся `Block` просто drop-ается без dealloc.
- `crates/malloc-bench/src/lib.rs:305-312` — mstress path явно делает `let _ = send(block)` с тем же результатом.
- `crates/malloc-bench/src/lib.rs:94-103` — `Block` не имеет `Drop`, поэтому raw allocation теряется.

**Последствие:** если receiving worker завершился/паниковал раньше sender, неотправленный блок утечёт. Это не UAF и не double-free; нормальный успешный benchmark run этот путь не достигает. На panic/error path утечка детерминирована.

## Проверенные области без подтверждённой дополнительной проблемы

### Allocation geometry, Layout и realloc

- Проверены small-class table/`SIZE2CLASS`, divisibility walk для повышенных alignments, exact-256 и page-aligned classes.
- Проверены large/huge arithmetic, `checked_add`, `align >= SEGMENT` rejection, span rounding, cache-hit `span_usable`, in-place large grow и cross-class small shrink.
- При корректном `Layout` не подтверждены overflow, misalignment, undersized allocation или неправильный dealloc layout внутри `GlobalAlloc` path.
- `alloc_zeroed` fresh/reused/decommitted paths просмотрены; подтверждённой выдачи незанулённых байт нет.

### Freelist, bitmap, magazine, run stack и segment lifecycle

- Immediate double-free до reuse корректно ловится magazine scan/alloc bitmap.
- Проверены batch refill/flush, bump-direct refill, mixed-segment flush, run-stack drain/reset и bitmap transitions.
- Проверены decommit bump guard, pooled small segments, stale ring entries после reset/recycle, current-segment exclusion, cache invalidation и eager unregister-before-release ordering.
- При корректной lifetime discipline не подтверждены duplicate issue, UAF payload write или double release на этих путях.

### Cross-thread protocols, registry, ABA и TLS

- Просмотрены MPSC remote ring (`head`/`tail` wrap, reserved-but-not-published slot), deferred-large Treiber stack/double-push claim, tagged free-slot stack, registry claim/recycle generation, slot-resident `thread_free`, fallback TFS и TLS `TORN` teardown.
- Проверены field-specific header reads против owner writes и вынесенные slot counters/TFS, закрывающие ранее известные aliasing gaps.
- Whole-heap slot reuse сохраняет stable `thread_free` address и сегменты в том же heap slot; новой ABA/UAF проблемы здесь не подтверждено.
- `u64` heap generation wrap практически недостижим; correctness-load-bearing `u32` ring cursors используют wrapping distance при power-of-two capacity. Diagnostic counters могут wrap, но не управляют владением.

### Concurrent containers и вспомогательные crates

- Просмотрены `AtomicSlot` epoch pinning, generation CAS eviction, deferred destruction, seqlock-style double generation read, saturation/retirement и `Send`/`Sync` bounds.
- `LockFreeRegion` использует audited `arc-swap`; `Region`/`SyncRegion` не содержат собственного unsafe.
- Проверены `aligned-vmem` reservation ownership, Windows reserve-then-commit failure rollback, Unix mmap/trim/release и miri Layout reconstruction.
- Проверены NUMA FFI pointer/length contracts и Windows `Reservation::from_raw_parts` handoff; подтверждённого mismatch/double release нет.

## Выполненные проверки

- `cargo test --all-features --no-run` — успешно; вся all-features test matrix компилируется.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — успешно.
- Целевой native набор под `production hardened alloc-stats` (AllocCore differential/invariants, Heap invariants, realloc regressions, hardened interior/large-kind guards, small pool, decommit stale ring, remote ring, TLS teardown) — успешно; 55 тестов прошли, 1 известный non-hardened residual остался ignored.
- `cargo +nightly miri` доступен; strict Miri для `regression_realloc_cross_class_shrink` — успешно.
- Loom: `loom_remote_ring`, `loom_remote_ring_drain_guard`, `loom_deferred_large`, `loom_free_slots_aba`, `loom_epoch` — успешно, включая counterfactual `should_panic` модели.
- Non-hardened ignored residual test запущен отдельно и ожидаемо **упал**, подтвердив F4.
- Отдельный минимальный safe-API stale-reuse reproducer выполнен против текущего `AllocCore` и подтвердил F2.

Полный runtime `cargo test --all-features` был начат, но не завершился в 120-секундном лимите из-за длинных differential/stress tests; вместо повторного полного soak выполнены compile-all и целевые memory/concurrency наборы выше. TSan не запускался в текущей Windows-среде. Fuzzing не запускался: короткий статический/детерминированный repro уже полностью подтверждает найденные классы, а продолжительный fuzz-run был бы несоразмерен задаче.

## Границы вывода

- F1-F3 и F5 являются проблемами Rust safety boundary: UB достижимо через публичные safe функции при значениях, разрешённых их сигнатурами.
- F4 требует ошибочного double-free через unsafe `GlobalAlloc` interface; это подтверждённая corruption-реакция и несоответствие defensive M2 ожиданию, но не UB, инициируемое корректным caller.
- За пределами перечисленного не найдено новых подтверждённых memory-safety дефектов при соблюдении `GlobalAlloc`/FFI/raw-pointer контрактов. Отсутствие находки не является формальным доказательством полного отсутствия UB для всех платформ и interleavings.
