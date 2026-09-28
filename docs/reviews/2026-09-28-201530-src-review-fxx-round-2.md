# Обзор `src/` крейта sefer-alloc — раунд 2 (fxx)

- **Дата:** 2026-09-28 20:15 (Europe/Berlin)
- **Автор:** fxx (Claude, effort=max), одиночный агент, без под-агентов
- **База:** `main` @ `3106720c` (`checkpoint: 2026-09-28-2011`), ветка `fxx-src-review-r2`
- **Область:** весь `src/**` — `alloc_core` (segment/small/large/platform/config/diag), `registry`
  (`heap_core`, `heap_core_xthread`, `heap_overflow`, `heap_registry`, `bootstrap`, `heap_slot`,
  `xthread_fallback_gate`), `global` (`tls_heap`, `fallback`, `sefer_alloc`, `alloc_stats`),
  `concurrent` (epoch/lock_free/sharded/pinning), `kani_proofs.rs`, `lib.rs`
- **Метод:** полное чтение исходников; сверка с предыдущими обзорами
  (`2026-09-21-200601-src-review-xa.md`, `2026-09-22-120730-src-review-xa-round-2.md`,
  `2026-09-24-105734-src-review-xxs-sol-round-3.md`, `2026-09-28-005939-src-review-oxx-round-1.md`,
  `2026-09-28-154558-src-review-oxx-round-2.md`) и с индексами `docs/CORRECTNESS_OPEN_ITEMS.md`
  (+ `docs/correctness-open-items/`) и `docs/perf/OPEN_ITEMS.md`; проверка диффов исправлений
  oxx R2-01..R2-07 (`969933b9`, `9f151ec6`+`72b423ed`, `6a30583d`, `c4c86584`, `543a02b7`,
  `e98538b9`, `23e1d9f8`); один воспроизводящий запуск **существующего** теста
  `tests/global_alloc_installed.rs` на закреплённом MSRV-тулчейне (см. R2-01) — без изменений
  в репозитории. Ничего в `src/` не менялось; версии не поднимались.
- **Тулчейны/источники:** rustc 1.97.0 (stable, x86_64-pc-windows-msvc), rustc 1.88.0 и 1.93.0
  (локально установлены); `library/std/src/sys/thread_local/destructors/list.rs` на тегах
  1.88.0/1.90.0/1.91.0/1.92.0 (GitHub raw) и в локальном rust-src 1.93.0/1.97.0.
- **Нумерация:** «fxx R2-NN» (чтобы не путать с «oxx R2-NN» и «xa R2-NN»).

## 0. Сводка

| ID | Severity | Категория | Одной строкой |
|---|---|---|---|
| fxx R2-01 | **P1** | correctness / platform contract | На заявленном MSRV (rustc 1.88…1.92) `#[global_allocator] SeferAlloc` детерминированно абортит при ПЕРВОЙ аллокации потока на Windows/macOS-классе целей: `finish_bind` инициализирует `Drop`-TLS `GUARD` внутри аллокации, а std ≤ 1.92 регистрирует TLS-деструкторы через `Vec` на глобальном аллокаторе → реентерабельный `rtabort!("the global allocator may not use TLS with destructors")`. Воспроизведено на 1.88.0. CI не видит (job `msrv` — только компиляция, `ubuntu-latest`). |
| fxx R2-02 | P4 | code smell / docs (defence-in-depth) | Комментарий в `heap_core_xthread/ring.rs` утверждает, что запись, попавшая в overflow-ring ЧУЖОЙ кучи, «не hazard, т.к. `reclaim_offset(_checked)` перепроверяет magic/kind/bounds» — но владение (`contains_base`) там не проверяется; реальная защита — инвариант `live_count ≥ 1`. Предложено исправить текст и (опционально) добавить `contains_base_ro` в drain-замыкание. |
| fxx R2-03 | P4 | performance (minor) | `drain_segment_ring` при `Decommitted { pooled: true }` не обновляет `ring_drain_head` → один лишний полный `ring.drain` при следующем посещении pooled-сегмента. |
| fxx R2-04 | P4 | docs (stale comment) | Комментарий в `tls_heap::finish_bind` про отказ `LOCAL.try_with` («re-enters `bind_slow_tagged`, re-claims the same slot») описывает несуществующее поведение: `current_for_alloc` маппит `Err` в `Fallback`; слот простаивает до выхода потока. Недостижимо на native-TLS целях. |

Отдельно (§3): xa **XA-18** (P3) не закрыт и **не проиндексирован** ни в одном индексе; после R13-2
(переиспользование NUMA-bucket'ов) сценарий «unknown → real bucket» стал регулярным.

Ключевой вывод: реальных дефектов памяти/UB в прочитанном коде не найдено; исправления oxx R2-01..R2-07
корректны (§1). Единственная серьёзная находка — платформенно-тулчейновая (R2-01), но она делает
основной продукт крейта неработоспособным на объявленном MSRV на двух из трёх desktop-ОС.

## 1. Проверка исправлений oxx R2-01..R2-07

| oxx | Коммит | Что проверено | Вердикт |
|---|---|---|---|
| R2-01 | `969933b9` | `AllocCore::finalize_old_cursor_if_orphaned` (`src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:424-442`): `old_cur` никогда не null (`small_cur` инициализируется базой primordial в `new_inner`); primordial исключён проверкой `kind == Small`; `live_count != 0 || is_decommitted` → выход; членство в пуле проверяется как `pool_head == old_cur \|\| !pool_prev.is_null()` (голова пула имеет `pool_prev == null`, поэтому первая половина дизъюнкции обязательна — есть); вызов из `reserve.rs:38-58` после `self.small_cur = base`, `old_cur` захвачен до переключения. Двойного pool/release нет (`release_or_pool_empty_segment` вызывается только для не-pooled). | **Корректно** |
| R2-02 | `9f151ec6` + `72b423ed` | `AllocCore::safe_payload_read_span(base, payload, own_segment)` / `small_committed_bound` (`src/alloc_core/alloc_core/mem/realloc_fastpath.rs:125-165`): own-legs передают `true` (`src/registry/heap_core/free/realloc.rs:299`, `:574`), foreign-leg — `false` (`:426`, coarse `SEGMENT`); под `primordial-lazy-commit`/`small-segment-lazy-commit` `committed_payload_end` ставится в `SEGMENT` на eager-путях (primordial: `bootstrap.rs`; обычные small: `reserve.rs`), а grow-on-carve коммитит ДО `set_bump`, поэтому легальный блок никогда не отвергается; Large-ветка использует `span_usable_at`. `72b423ed` только глушит `unused` без lazy-фич. | **Корректно** (foreign-leg по-прежнему опирается на контракт `old_layout` — задокументировано) |
| R2-03 | `6a30583d` | `SeferAlloc::dealloc_batch` (`src/global/sefer_alloc/batch.rs:118-171`): под `alloc-xthread` — `current_for_dealloc`; `ForeignNoBind` → цикл `dealloc_foreign_routing(ptr, base, layout, None)` по non-null; без `alloc-xthread` — прежний `current_heap()`. Зеркально скалярному `dealloc` (`global_alloc.rs:50-121`). | **Корректно** |
| R2-04 | `c4c86584` | Док `AllocStats::ring_overflows` / `cross_thread_frees_lost` (`src/global/alloc_stats.rs:106-151`, `:243-251`): цепочка ring → `HeapOverflow` → retry → spill описана верно по коду `push_with_overflow_retry`; `DBG_RING_PUSH_RETRY_EXHAUSTED` действительно не имеет ни одного writer'а в `src/` (grep `fetch_add\|store` по имени — 0). | **Корректно** |
| R2-05 | `543a02b7` | `SEGMENTS_RESERVE_FAILED_TOTAL` инкрементируется на отказе `aligned_vmem::reserve_aligned` в NUMA-fallback ветке (`src/alloc_core/platform/numa.rs:101-106`) и в общем пути `os::Segment::reserve`; `SEGMENTS_RESERVED_TOTAL` бампится только после успешного `into_parts()` (`:140-141`). Остаток: ветка `NonNull::new(..)?` (`:126-127`, практически недостижима) не считает ни reserved, ни failed — согласованно (резервация освобождается Drop'ом). | **Корректно** |
| R2-06 | `e98538b9` | `SegmentTable::recycle` (`src/alloc_core/segment/segment_table/segment_table_impl.rs:442-524`): `hash_find(base).is_none()` → `SEGMENT_RECYCLE_UNVERIFIED_BASE_TOTAL` + `debug_assert!(false)` + `return` ДО `read_at` (адресный поиск не трогает память сегмента); затем `base_at(id) == base`, иначе линейный `find`. | **Корректно** |
| R2-07 | `23e1d9f8` | Выборочно: `realloc.rs` (own/foreign legs — «defence-in-depth against a contract violation, not a safety requirement»), `bootstrap/overflow_sidecar.rs::deref_overflow_sidecar` («`assert!`, not `debug_assert!`» — совпадает с `heap_overflow_impl.rs:384-387`), `heap_overflow/drain.rs::try_drain` (RAII-публикация `head`). Текст соответствует коду. | **Корректно** |

## 2. Находки

### fxx R2-01 — P1 — correctness / platform contract: на объявленном MSRV `#[global_allocator]` абортит при первой аллокации (Windows/macOS-класс целей, rustc 1.88…1.92)

**Место:**
- `src/global/tls_heap.rs:149` — `static GUARD: AbandonGuard = const { AbandonGuard::new() };` (`thread_local!` с `impl Drop`, т.е. std `EagerStorage` с регистрацией деструктора при первом обращении);
- `src/global/tls_heap.rs:594-621` — `finish_bind`: `GUARD.try_with(|g| g.heap.set(heap))` выполняется **до** публикации `LOCAL` (`:605`, `:619`);
- `src/global/tls_heap.rs:335-360` — `current_for_alloc`: `LOCAL == null → bind_slow_tagged()`;
- `src/global/sefer_alloc/global_alloc.rs:31-47` — `GlobalAlloc::alloc` → `current_heap()`;
- `Cargo.toml:7` — `rust-version = "1.88"`; `README.md:11` (бейдж «MSRV: 1.88»), `README.md:1743-1747`;
- `.github/workflows/ci.yml:2581-2610` — job `msrv`: `dtolnay/rust-toolchain@1.88` на `ubuntu-latest`, шаги `cargo check --all-features` и `cargo test --no-run --all-features` (тесты не исполняются).

**Конфигурация:** любая с `alloc-global` (в т.ч. `production`), rustc **1.88.0 … 1.92.x**, любая цель, на которой std регистрирует TLS-деструкторы через собственный список `DTORS` (`library/std/src/sys/thread_local/destructors/list.rs`): Windows, Apple, FreeBSD/OpenBSD, illumos/Solaris и т.п.; на Linux/Android/NetBSD/DragonFly/Fuchsia std использует `__cxa_thread_atexit_impl` (`destructors/linux_like.rs`) и падает в `list` только если символ отсутствует (не-glibc libc). glibc-Linux не затронут — поэтому CI на `ubuntu-latest` не увидел бы аборт, даже если бы исполнял тесты.

**Механизм.** В std ≤ 1.92 `list::register` выглядит так (проверено по тегам 1.88.0, 1.90.0, 1.91.0, 1.92.0):

```text
static DTORS: RefCell<Vec<(*mut u8, unsafe extern "C" fn(*mut u8))>> = RefCell::new(Vec::new());
pub unsafe fn register(t, dtor) {
    let Ok(mut dtors) = DTORS.try_borrow_mut() else {
        rtabort!("the global allocator may not use TLS with destructors");
    };
    guard::enable();
    dtors.push((t, dtor));          // Vec на ГЛОБАЛЬНОМ аллокаторе, под живым RefMut
}
```

Начиная с 1.93.0 (локальный rust-src 1.93.0 и 1.97.0) `DTORS` — `Vec<_, System>` (`Vec::new_in(System)`), сообщение — «the System allocator may not use TLS with destructors». Именно поэтому на текущем stable (1.97) аборт невозможен, и oxx R1 §4.1 («Регистрация TLS-деструктора в std 1.97 использует `Vec<_, System>` — реентерабельный abort … невозможен») верен **только** для ≥ 1.93.

Цепочка на ≤ 1.92 (первая аллокация в потоке, например в `std::rt::init` до `main`):

1. `SeferAlloc::alloc` → `current_for_alloc` → `LOCAL == null` → `bind_slow_tagged` → `HeapRegistry::claim` (слот A) → `finish_bind(A)`;
2. `GUARD.try_with(..)` → `EagerStorage::get` → `State::Initial` → `initialize()` → `destructors::register` → `DTORS.try_borrow_mut()` (Ok) → `dtors.push(..)` → `RawVec::grow` → `__rust_alloc` → **вложенный** `SeferAlloc::alloc`;
3. `current_for_alloc` → `LOCAL` всё ещё `null` (публикуется только после армирования guard'а, `:619`) → `bind_slow_tagged` → `HeapRegistry::claim` (слот B) → `finish_bind(B)` → `GUARD.try_with(..)` → состояние ещё `Initial` (`Alive` ставится после возврата `register`) → `initialize()` → `register` → `DTORS.try_borrow_mut()` → **Err** → `rtabort!`.

Ни `LOCAL` (`const`, без `Drop` — native `#[thread_local]`, регистрации нет), ни `LAST_STALL_CONCESSIONS`, ни `FALLBACK_LOCK_HELD` (оба `Cell`, без `Drop`) этой проблемы не имеют — `GUARD` единственная `Drop`-TLS на пути аллокатора.

**Воспроизведение (выполнено, ничего не коммитилось, `CARGO_TARGET_DIR` вне worktree):**

```text
$ cargo +1.88.0 test --features production --test global_alloc_installed
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2m 09s
     Running tests\global_alloc_installed.rs
fatal runtime error: the global allocator may not use TLS with destructors, aborting
error: test failed ... (exit code: 0xc0000409, STATUS_STACK_BUFFER_OVERRUN)
```

Та же команда с `cargo +1.93.0` (первый std с `Vec<_, System>`): `test result: ok. 2 passed; 0 failed`;
на rustc 1.97.0 (штатный `npm run check`) тест тоже проходит. Единственное отличие между запусками —
версия std; диапазон отказа по исходникам std — **1.88.0 … 1.92.x** (1.88.0 и 1.93.0 подтверждены
запуском, 1.90.0/1.91.0/1.92.0 — по тексту `list.rs` на тегах).

**Сценарий отказа.** Любой бинарник с `#[global_allocator] static A: SeferAlloc = SeferAlloc::new();`,
собранный rustc 1.88–1.92 под Windows/macOS: процесс абортит до входа в `main` (первая аллокация
главного потока). Ни OOM, ни UB — детерминированный `rtabort!`, т.е. 100 % отказ основного продукта
крейта на объявленном floor'е для пяти stable-релизов.

**Почему не поймано.** (1) Job `msrv` только компилирует (`cargo check`, `cargo test --no-run`) — это
осознанная оговорка item 19 (`docs/correctness-open-items/TRACKED_ci_gate_coverage.md:52-80`: «a construct
that compiles but panics/behaves differently only under 1.88 at runtime would still slip through»),
но там она подана как гипотетический «acceptable caveat», а здесь — конкретный тотальный отказ.
(2) Даже исполнение тестов на `ubuntu-latest` (glibc) ничего бы не показало — дефект виден только на
целях без `__cxa_thread_atexit_impl`. (3) Локальная разработка идёт на 1.97, где std уже починен.

**Что нового относительно известного:** item 19 — класс «поведение отличается на MSRV в runtime», без
конкретики; oxx R1 §4.1 — прямое утверждение «abort невозможен», которое верно лишь для std ≥ 1.93.
Ни один индекс не упоминает зависимость от версии std.

**Исправление** (рекомендуется (a)+(c)+(d); (b) — если (a) не принимается):

- **(a) Структурное, независимое от версии std:** в `finish_bind` публиковать `LOCAL` **до** армирования
  `GUARD`; при отказе `GUARD.try_with` — сбросить `LOCAL` в `null`, `recycle(heap)`, вернуть `Fallback`
  (симметрично нынешней ветке L-6). Тогда вложенная аллокация из `dtors.push` разрешится в
  `CurrentHeap::Own(heap)` и обслужится обычным путём: второго `claim` и второго касания `GUARD` не будет,
  внешний `register` завершится, guard станет `Alive`. Обоснование корректности: во внешнем кадре в этот
  момент нет живого `&mut HeapCore` (`current_heap()` ещё не вернулся, `(*heap).alloc` не вызывался);
  блоки, выданные во «внутреннем» окне, остаются валидными и после возможного `recycle` (сегменты
  остаются со слотом; их освобождение с этого потока идёт через `ForeignNoBind` → штамп владельца →
  ring следующего claimant'а). На ≥ 1.93 поведение не меняется (вложенной аллокации не происходит).
  Инвариант L-6 («слот не выдаётся без живого guard'а») сохраняется, потому что откат синхронный.
- **(b) Контрактное:** поднять `rust-version` до `1.93` (первый std с `Vec<_, System>` для `DTORS`) с
  объяснением в CHANGELOG/README — **только по явному решению владельца** (правило «версии не поднимать
  без запроса»). Без (a) любая заявленная версия < 1.93 — ложная.
- **(c) CI:** исполнять хотя бы один `#[global_allocator]`-тест (например `tests/global_alloc_installed.rs`)
  на закреплённом MSRV-тулчейне на `windows-latest` и/или `macos-latest` (linux-job этот класс не
  наблюдает принципиально); обновить карточку item 19.
- **(d) Индекс:** завести карточку в `docs/CORRECTNESS_OPEN_ITEMS.md` (тема `TRACKED_platform_contracts`
  или `ACTIVE`), с указанием диапазона версий 1.88–1.92 и списка затронутых целей.

**Контрфактический тест:** существующий `tests/global_alloc_installed.rs` под `cargo +1.88.0 test
--features production` на windows-msvc — сейчас RED (аборт), после (a) должен стать GREEN; после (b)
тулчейн выходит из области поддержки (тест на 1.93 GREEN).

### fxx R2-02 — P4 — code smell / docs: «wrong-heap» запись overflow-ring объявлена безопасной из-за перепроверки, которая владение не проверяет

**Место:** `src/registry/heap_core_xthread/ring.rs:37-46` (и повтор `:75-84`);
`src/registry/heap_core_xthread/drain.rs:337-347` и `:358-362` (замыкание `reclaim`);
`src/alloc_core/small/alloc_core_small_reclaim.rs:140-195` (`reclaim_offset_checked`: guard-цепочка
`class_idx < SMALL_CLASS_COUNT` → `magic` → `kind` → `block_size`-alignment → `payload_start` → `bump` →
`is_free` → `is_in_magazine` → [`hardened` gen]).

**Конфигурация:** `alloc-xthread` (в `production`).

**Механизм.** Комментарий утверждает: если stale-чтение `owner_state` направит запись в overflow-ring
другой живой кучи, «the pushed entry sits in the wrong heap's overflow ring, drained on ITS next
opportunistic pass — not a correctness hazard: `HeapOverflow::try_drain`'s `reclaim_offset(_checked)`
call independently re-validates `base`'s `magic`/`kind`/bounds before touching anything». Но ни одна из
перечисленных проверок не проверяет **владение**: в guard-цепочке нет `contains_base`/`contains_base_ro`
(`segment_table_impl.rs:593`). Если бы такая запись действительно была осушена чужой кучей, та бы
выполнила `write_next`/`set_head`/`mark_free` + `dec_live_and_maybe_decommit` над `BinTable`/`live_count`
сегмента **другой** кучи — запись в owner-only метаданные из чужого потока (data race с владельцем, потенциально
`release_or_pool_empty_segment` чужого сегмента).

Реальная причина, по которой это не происходит: блок, чьё освобождение публикуется, держит `live_count ≥ 1`
своего сегмента до момента осушения, поэтому сегмент не может быть освобождён и перештампован между
загрузкой `owner_state` и `push` — сценарий stale-чтения недостижим при соблюдении контракта (достижим
только при double-free через release/re-reserve, что и так UB). Т.е. защита — инвариант живого блока,
а не перепроверка в `reclaim_offset`.

**Сценарий:** при соблюдении контракта — нет. Ценность находки — точность safety-аргумента (та же
категория, что oxx R2-07) плюс дешёвая настоящая defence-in-depth.

**Исправление:** переписать комментарий (`ring.rs:37-46`, `:75-84`), сославшись на инвариант
`live_count ≥ 1`; опционально в `drain_heap_overflow` добавить в замыкание `reclaim` проверку
`self.core.contains_base_ro(base)` перед `reclaim_offset_checked` (O(1) hash-probe на cold-пути
второго уровня; для `try_drain_spill` — то же). Тогда утверждение комментария станет истинным
буквально.

### fxx R2-03 — P4 — performance (minor): `drain_segment_ring` не обновляет `ring_drain_head` на pooled-пути

**Место:** `src/alloc_core/small/alloc_core_small/find_segment.rs:284-295`.

**Конфигурация:** `alloc-xthread` + `alloc-decommit` (в `production`).

**Механизм.** После `ring.drain(..)` (`:243-268`) функция при `decommit_happened` вызывает
`release_or_pool_empty_segment(base)` и возвращает `Decommitted { pooled }` (`:285-288`) **до**
`meta_for_ring.set_ring_drain_head(new_head)` (`:294`). Для `pooled == true` сегмент остаётся живым и
зарегистрированным, `ring.head` уже продвинут drain'ом, а кэш `ring_drain_head` в заголовке остался
до-drain'овским. Следующее посещение (кандидат каталога через `validate_directory_candidate:872-889`,
линейный скан `:706-708`, либо после `unpool_if_present`/`pop_pooled_segment`) видит
`tail_relaxed() != cached_head`, выполняет полный `ring.drain` (Acquire-пара + безусловный
`head.store(Release)`) вхолостую и только тогда обновляет кэш; fast-path `Skipped` теряется один раз.

**Сценарий:** cross-thread frees опустошают сегмент → он уходит в пул → его же переиспользуют: один
лишний холостой drain на событие. Не влияет на корректность.

**Исправление:** вызвать `meta_for_ring.set_ring_drain_head(new_head)` **до** ветки
`release_or_pool_empty_segment` (обязательно до — после release заголовок отображён быть не должен;
для pooled сегмента заголовок жив). Поведение иначе не меняется; проверка — `DIRECTORY_STALE_HITS`/iai
не должны вырасти, а число `ring.drain`-вызовов на pooled-reuse уменьшится на 1.

### fxx R2-04 — P4 — docs: комментарий `finish_bind` про отказ публикации `LOCAL` описывает несуществующее поведение

**Место:** `src/global/tls_heap.rs:613-620` против `:335-360` (`current_for_alloc`:
`Err(_) => CurrentHeap::Fallback`) и `:461-472` (`current_for_alloc_with_config`, то же).

**Механизм.** Комментарий: «If THIS fails … every call on this thread simply re-enters
`bind_slow_tagged` and re-claims (cheap re-claim of the same slot), never reading a stale/unset LOCAL».
На деле отказ `LOCAL.try_with` означает недоступность TLS-значения, поэтому все последующие
`current_for_alloc` попадают в ветку `Err(_) => Fallback`: поток **не** перепривязывается, армированный
слот простаивает до выхода потока (его корректно утилизирует `GUARD`), а все аллокации идут через
spinlock fallback'а. Вторая неточность: «re-claim of the same slot» невозможен в принципе — `claim`
никогда не возвращает LIVE-слот; повторная привязка взяла бы **новый** слот и перезаписала
`GUARD.heap`, осиротив первый.

**Достижимость:** только на `os`-keyed TLS целях (для `const`, `Drop`-less native `#[thread_local]`
`try_with` не может вернуть `Err`) — вне tier-1 целей крейта (64-bit std). Поведение при этом
безопасно (нет утечки: слот вернётся при `AbandonGuard::drop`).

**Исправление:** привести комментарий в соответствие; либо (симметрично ветке guard'а) при
неудаче публикации `LOCAL` сразу `recycle(heap)` + сбросить `GUARD.heap` + вернуть `Fallback`, чтобы
слот не простаивал.

## 3. Статус ранее найденного (не повторяется как новое)

- **xa XA-18 (P3, `2026-09-21-200601-src-review-xa.md:354`) — открыт и не проиндексирован.**
  `grep XA-18` по `docs/CORRECTNESS_OPEN_ITEMS.md`, `docs/correctness-open-items/*.md`,
  `docs/perf/OPEN_ITEMS.md`, `CHANGELOG.md` — 0 совпадений; xa R2 / xxs R3 / oxx R1-R2 к нему не
  возвращались. Что нового после R13-2: механизм переиспользования bucket'ов
  (`segment_directory_impl.rs:357-381`, `:480-513`) делает «продвижение» узла из unknown-bucket в
  реальный **регулярным** событием (bucket освобождается, когда `active_bits_by_node[nb]` доходит до 0),
  а `clear_bit` через read-only `node_bucket` (`:401-409`) чистит уже реальный bucket; биты, поставленные
  узлу в период его жизни в unknown-bucket, снимаются только `clear_slot` при recycle сегмента или
  `clear_bit_all_nodes` на null-base пути (`directory.rs:183-196`). Последствие ограничено:
  `validate_directory_candidate` (`find_segment.rs:891-902`) на каждый скан unknown-bucket'а по этому
  классу платит pre-drain проверку + чтение `bt.head` + `publish_empty` в **не тот** bucket и тикает
  `DIRECTORY_STALE_HITS`, пока сегмент не будет recycled — perf-деградация каталога под `numa-aware`,
  не порча. По правилу CLAUDE.md («каждый flagged-open пункт должен попасть в индекс») — рекомендуется
  завести карточку.
- **Item 22** (at-least-once при unwind в `DrainHeadPublish`): тот же остаток теперь задокументирован и
  для `HeapOverflow::try_drain` (R2-13, `heap_overflow/drain.rs:183-216`); для spill выбран `abort`
  (`spill.rs:24-35`). Согласованно, нового нет.
- **Item 23** (`InitStateGuard` после `write(hc)`): без изменений, документация точна
  (`fallback.rs:514-538`).
- **oxx R1 §4.1** («abort … невозможен»): утверждение верно только для std ≥ 1.93 — см. R2-01.

## 4. Оптимизации (не влияют на корректность; все — вне iai-измеренного hit-пути)

1. **`finish_magazine_refill` — два прохода по refilled-блокам** (`src/registry/heap_core/alloc/hot.rs:104-130`):
   первый цикл считает `segment_base_of_ptr(p)` для stamp-dedupe, второй — заново для
   `mark_magazine`. Их можно слить в один проход (base считается один раз; stamp при смене base; bit для
   `i < n-1`). Экономия — `n-1` mask-операций + накладные расходы цикла на miss-путь (`n ≤ 16`); ожидаемый
   эффект субпроцентный, принимать только через `npm run iai` (miss-путь измеряется churn-гейтами).
2. **`push_with_overflow_retry` — двойной `resolve_dirty_bit_target`** (`src/registry/heap_core_xthread/overflow.rs:666`, `:745`):
   на double-saturation пути снимок вычисляется дважды с одинаковыми входами (`ResolvedDirtyTarget: Copy`);
   можно переиспользовать первый (экономит `segment_id_at` + `owner_state` load + `slot_or_none` на
   каждый двойной оверфлоу; cold-путь).
3. **R2-03** — перенос `set_ring_drain_head(new_head)` до pool/release решения (см. выше).
4. **Не рекомендуется повторять:** любые попытки убрать пере-вычисление `segment_base_of_ptr` в
   overflow-flush магазина (`free/dealloc_own_base.rs:446-462` → `flush_class`) — эта область уже
   измерена как NO-GO (R24-3/R24-4: RMW на горячей L1-строке дешевле дополнительного bookkeeping).

## 5. Проверено — проблем не найдено

1. **Диспетчер `GlobalAlloc`** (`global_alloc.rs`): `alloc`/`alloc_zeroed`/`realloc` через
   `current_heap()`; `dealloc` через `current_for_dealloc` (`Own` / `ForeignNoBind` → `dealloc_foreign_routing(.., None)`);
   `!alloc-xthread` ветки сохраняют старое поведение. Одно-ветвевое различение `null`/`TORN`/real
   (`p.addr().wrapping_sub(1) < usize::MAX - 1`) корректно для всех трёх значений.
2. **TLS teardown** (`tls_heap.rs`): `AbandonGuard::drop` — `mark_local_torn` → `drain_large_deferred_free`
   → `trim_for_recycle` → `recycle` в окне единственного писателя; `TORN` не перепривязывает; never-bound
   поток может привязаться во время teardown (std `run` отпускает `RefMut` перед каждым dtor'ом) —
   корректно; L-6 откат при отказе `GUARD` верен (см. R2-01 для ≤ 1.92 и R2-04 для комментария).
3. **Fallback** (`fallback.rs`): `INIT_STATE` CAS + `InitStateGuard` (rollback при unwind) + OOM-rollback
   в `UNINIT` (losers re-race); `LockGuard` RAII + yield после 64 спинов; R1-06 флаг
   `FALLBACK_LOCK_HELD` ставится после успешного CAS и снимается до `LOCK.store(false)`; `heap_ptr()`
   никогда не возвращает `&mut` через `static mut` без `addr_of_mut!`.
4. **Реестр** (`heap_registry/*`, `bootstrap/*`): `claim_impl` (CAS FREE→LIVE, `initialised`-gate вместо
   `generation == 1`, `push_back_after_oom`, NUMA-cache invalidate); `recycle` (CAS LIVE→FREE, double-recycle
   no-op); tagged Treiber stack с seal (`TagExhausted` → задокументированная утечка одного слота);
   `bump_count` rollback race benign; chunk-материализация через `OncePtrCell` (abort на alloc-пути,
   `None` на free-пути, `slot_if_materialised` для stats-walk — R2-11); `walk_initialised_slots` с
   Acquire на `initialised`.
5. **`HeapOverflow`** (`heap_overflow/*`): `room_check` Stale/Full (R1-05); sidecar материализуется
   **до** CAS резервации (wedge-hazard); публикация `packed`(Relaxed) → `base`(Release) ↔
   `base`(Acquire) → `packed`; `try_drain` под токеном `drain_owner`, `DrainGuard` публикует
   `head` **до** отпускания токена; возвращает фактический `h` (R2-4); `is_likely_empty` учитывает
   `spill_head`; `slot()` — `assert!` перед `deref_overflow_sidecar`; `2^64 % HEAP_OVERFLOW_CAP == 0`,
   wrap курсоров бесшовный.
6. **Intrusive spill** (`spill.rs`): `write_struct` → `swap` (AcqRel, возвращает реального предшественника)
   → `write next` → `ready.store(1, Release)`; consumer: Acquire `head` → Acquire `ready` → `read_struct`
   → CAS-pop → `reclaim`; ABA невозможен без повторной публикации того же блока (double-free); смешанный
   атомарный/неатомарный доступ к `ready` упорядочен happens-before; `SpillReclaimGuard` abort при panic
   (нельзя переочередить возможно уже unmapped узел); `const` asserts на размер/выравнивание `SpillNode`.
7. **Retry-петля** (`overflow.rs`): G1 — все чтения сегмента (`resolve_dirty_bit_target`) до публикации;
   `push` → `push_to_heap_overflow` → progress-detected rounds (`RETRY_STALLED_ROUNDS_GIVE_UP`,
   `RETRY_ROUND_SAFETY_CAP`) → memo-cache `LAST_STALL_CONCESSIONS` (N-way, update-in-place по base) →
   spill; R1-06 обход сна при удержании fallback-lock; `owner_slot_is_live` для `OWNER_ID_FALLBACK` = false;
   `abort()` в конце достижим только при garbled owner-id (chunk легального штампа уже материализован
   claim'ом).
8. **OPT-C stamp cache vs R34-14** (`ownership.rs:134-236`, `alloc_core_large.rs:409-450`): cache-hit Large
   сбрасывает `owner_state` в `pack_owner(LIVE, OWNER_ID_NONE, 0)`; `unpack_owner_id` даёт
   `0x7FFF_FFFF ≠` любого id слота (< 4096) и `≠ OWNER_ID_FALLBACK` → fast-path промахивается →
   slow-path перештамповывает и `owner_state`, и `owner_thread_free`. Pooled small-сегменты сохраняют
   штамп той же кучи — корректно в shard-модели.
9. **Магазин** (`hot.rs`, `dealloc_own_base.rs`, `tcache.rs`): pop очищает residency-bit и virgin-bit;
   push отмечает residency-bit; overflow — clear bits FLUSH_N → `flush_class` → компакция `slots` и
   `virgin_mask >>= FLUSH_N`; `FREE_PARK_CAP` (R1-01) симметрична refill-бюджету; `PerClass` `repr(C)`
   пины; `TCACHE_CAP <= 16` const-assert; `small_free_guard` (F7 A/B, H1, M2 ×2) общий для scalar и
   batch (task #2002), `RouteToLargeFree` для promoted-Large.
10. **Batch API** (`batch.rs`, `dealloc_batch.rs`): отложенная очистка residency-bit'ов в `alloc_batch`
    + предикат без `k == c` shortcut; `dealloc_batch_small` — staging 64, `flush_class` дедуплицирует
    через `is_free`; Large-ветки с drain-прелюдиями.
11. **Realloc** (`realloc.rs`): A1 drain до in-place; OPT-F/OPT-G через `try_realloc_inplace_known_base`;
    promotion под `medium_promotion_reachable!`; own/foreign legs ограничены `safe_payload_read_span`
    (R2-02); F6 — прямой вызов own-thread free с уже доказанным `base` (аргумент «segment живёт, пока блок
    жив» верен: все unregister-пути требуют `live_count == 0`).
12. **`reclaim_offset_checked`** (`alloc_core_small_reclaim.rs:116-246`): порядок guard'ов
    (magic → kind → align → `payload_start` → `bump` → `is_free` → magazine → gen) сохранён, `write_next`
    только после всех; `reclaim_offset` делегирует с always-false предикатом.
13. **Каталог/NUMA** (`segment_directory_impl.rs`, `directory.rs`, `find_segment.rs`): `set_bit` считает
    `active_bits_by_node` только при реальном 0→1; `clear_bit`/`clear_bit_all_nodes`/`clear_slot`
    декрементируют симметрично; `validate_directory_candidate` self-heal через `publish_empty`
    (кроме XA-18-случая, §3).
14. **`concurrent/*`** (legacy tier): `region_id` gate (R2-03 xa) во всех входах; `try_evict_at` —
    CAS-linearization, `Stale` при `old.is_null()`; `remote_free_pending` — флаг ставится после отпускания
    очереди, снимается под замком (без stranded индексов); R2-21 swap вместо `take`; `ErasedGuard`
    аккумулирует все claim'ы; `PinnedRunner::run` усекает воркеров по `shard_count()` (R2-16);
    `LockFreeRegion::remove` валидирует до CoW (R2-20); retirement при `u32::MAX` в обоих tier'ах
    (задокументированное расхождение на 1 reuse).
15. **`kani_proofs.rs`**: гармонии только для чистой арифметики/локальных буферов; ограничения
    (нет моделирования конкурентности) честно описаны; `loom_shim` — const-capable зеркало с явным
    списком расхождений.
16. **`heap_slot.rs`**: `unsafe impl Sync` без `Send` (M6); все поля `pub(crate)` (R4-MS-4);
    `HeapSlotRemote` `align(64)`; `generation` production-dead — задокументировано.
17. **Stack-pressure pin** `size_of::<HeapCore>() <= 9216` (`core.rs:584`) безусловен; `HeapCore::new`
    M5-clean (никакого `std::alloc` до `bind_*`).

## 6. Ограничения обзора

- Статическое чтение + один runtime-репро (R2-01). `npm run check`, miri/loom/TSan не запускались
  (не требовалось задачей; `src/` не менялся).
- Не пересматривались вычисленные perf-числа гейтов и `docs/perf/*` — только код.
- R2-01 воспроизведён на x86_64-pc-windows-msvc; для macOS/BSD/illumos/musl вывод сделан по исходникам
  std (`destructors/mod.rs` cfg-таблица, `linux_like.rs` weak-`__cxa_thread_atexit_impl` fallback), не
  запуском.
