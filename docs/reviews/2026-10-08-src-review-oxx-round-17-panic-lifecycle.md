# Ревью `src/` — раунд 17 (oxx) — panic-safety, обработка ошибок, реентерабельность, жизненный цикл потока/процесса

## 1. Охват, инвентарь, метод, ограничения, вердикт

**База:** worktree на `453439c123f71a98890408dd0170bd909ad9ecdf` (`docs: close correctness item 175 after green CI and record the red CI of the first push`). С базы R16 (`6a0d47f6`) `src/` изменён коммитами follow-up R16 (`486f5ace`, `32cacc97`, `222e913d`, `9df9f6b8`, `c8b9344a`, `b707d196`): `git diff --shortstat 6a0d47f6..HEAD -- src/` — 22 файла, +130/−122. Это один из восьми тематических отчётов раунда 17; тема этого отчёта — panic-safety, ошибки, реентерабельность, жизненный цикл.

**Режим: только чтение, без компиляции и без исполнения кода.** В ходе раунда владелец отменил разрешение на witness-тесты, мутанты и любые вызовы `cargo`. Не запускались `cargo build/check/test/clippy`, rustc, Miri, Loom, Kani и бенчмарки. Исполненных witness нет, все выводы имеют класс SOURCE-CONFIRMED или ГИПОТЕЗА. Использованы только чтение файлов, grep/rg, read-only git (`log -S`, `show`, `diff`) и Node-скрипт для подсчёта строк. Скрипт лежал во временном игнорируемом каталоге `target/` и удалён до сдачи отчёта.

**Ревьюер:** oxx (Claude Opus 5.5, effort=max). Работал в одном контексте, без суб-агентов. Исходники не правились; единственный создаваемый файл — этот отчёт.

**Инвентарь (подсчитан скриптом, приложение A):** 168 Rust-файлов, 43 325 физических строк. Из них 17 059 непустых строк не начинаются с `//`, 24 303 — строки-комментарии `//`/`///`/`//!`, 1 963 — пустые. По группам: `src/alloc_core/` — 88 файлов / 24 802 строки; `src/registry/` — 46 / 10 413; `src/global/` — 18 / 3 809; `src/concurrent/` — 14 / 3 554; `src/lib.rs` + `src/kani_proofs.rs` — 2 / 747. Unsafe-инвентарь посчитан командой из CLAUDE.md: в `src/` 21 модульный (tier 1, 21 файл) и 83 item-уровня (tier 2, 30 файлов), всего 51 файл; в `crates/` 6 + 20. Код с `process::abort` вне комментариев: **106 мест в 25 файлах** `src/`.

**Метод.**
1. **Индексы.** `docs/CORRECTNESS_OPEN_ITEMS.md` прочитан целиком, вместе с lookup-таблицей. Целиком прочитаны также `ACTIVE.md`, `TRACKED_correctness_residuals.md`, `TRACKED_misc.md`, `TRACKED_test_flakiness.md` и `TRACKED_verification_coverage.md`. Из `TRACKED_platform_contracts.md` прочитаны карточки 59b–152, из `TRACKED_ci_gate_coverage.md` — карточки 19–55. Остальные TRACKED-файлы прошли grep-скрининг по ключевым словам темы (fork/atexit/reentr/teardown/destructor/panic/abort/OOM/poison/unwind). Из `RESOLVED.md` прочитаны 172–176 и шапка. В `docs/perf/OPEN_ITEMS.md` прочитаны целиком шапка, тиры, карточки 1–41 (до строки ~580), 64–83 и весь раздел Recently resolved; длинные нарративы в строках 580–2958 — по заголовкам и тем же grep-запросам темы. Отчёт R16 и манифест `SRC_REVIEW_R16_MANIFEST.md` прочитаны целиком.
2. **Полное построчное чтение** (код и комментарии):
   - `global/`: `sefer_alloc/{mod,global_alloc,core,diag,batch,maintenance}.rs`, `tls_heap.rs`, `fallback.rs`, `maintenance_service.rs`, `maintenance_start_error.rs`, `exact_object/{exact_shard,narrow,exact_table,exact_fatal}.rs`;
   - `registry/heap_registry/{claim,stack,maintenance}.rs`, `registry/bootstrap/{registry,ensure,saturation}.rs`;
   - `registry/heap_core/{core.rs, state/ownership.rs, state/tcache_flush.rs, free/dealloc.rs, free/realloc.rs, alloc/hot.rs}`, `registry/heap_core_xthread/{routing,sidecar_drain}.rs`;
   - `registry/segment_route/{directory,shard_lock,registration,small_sidecar,pin,large_state}.rs`;
   - `alloc_core/alloc_core/{lifecycle,bootstrap}.rs`, `alloc_core/segment/segment_table/route_slots.rs`, `alloc_core/large/reservation_state.rs`, `alloc_core/segment/segment_header/{terminal_words,segment_header_gen_table}.rs`, `alloc_core/platform/numa.rs`, `alloc_core/small/reserved_small_segment.rs`;
   - `concurrent/sharded/sharded_region.rs`, `concurrent/pinning.rs`;
   - `lib.rs:100–524`, `tests/no_panic_doc_accuracy.rs`.
3. **Выборочно, по диапазонам:**
   - `registry/heap_core/free/dealloc_own_base.rs:330–557`;
   - `alloc_core/large/alloc_core_large.rs:120–835`;
   - `alloc_core/platform/os.rs:1–300`;
   - `alloc_core/small/alloc_core_small/reserve.rs:120–464`;
   - `alloc_core/segment/segment_table/segment_table_impl.rs:160–530,614–733`;
   - `alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:600–690`;
   - `alloc_core/alloc_core/mem/mem_impl.rs:240–490` (сайты abort с контекстом);
   - `alloc_core/platform/sidecar.rs:200–399`;
   - `alloc_core/small/alloc_core_small_magazine.rs:100–180`;
   - `alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:60–120,440–470` и сайты `prepare_small_issue`;
   - `concurrent/epoch/epoch_region.rs:640–713`, `concurrent/epoch/hand.rs:170–310,560–591`;
   - `registry/heap_slot.rs:1–120`, `registry/heap_registry/counters.rs:300–460`;
   - вне `src/`, для проверки реентерабельности из `src/`: `crates/numa-shim/src/lib.rs:1500–1590,1682–1700` и `crates/once-ptr-cell/src/lib.rs:55–95`;
   - `README.md:1600–1730` («Honest limitations», «Fork safety», «MSRV»).
4. **Скрининг всех 168 файлов** grep-запросами; каждое попадание на production-путях прочитано. Запросы:
   - релизные `.expect(`, `.unwrap()`, `assert!/assert_eq!/assert_ne!/panic!/unreachable!/todo!/unimplemented!`;
   - `process::abort` (106 мест) и все 23 `impl Drop`;
   - конструкции, аллоцирующие через глобальный аллокатор: `Vec`, `Box`, `format!`, `String`, `Arc`, `collect`, `println!`;
   - `extern "…"`, `catch_unwind`, `resume_unwind`, panic hooks;
   - `thread_local!`, `Instant`, вызовы `slot(`/`slot_or_none(`.

**Не прочитано вглубь** (только скрининг). Утверждения «не найдено» относятся только к прочитанному:
- `alloc_core/alloc_core/sidecar_drain.rs` (R16 читал целиком), `alloc_core_small_reclaim.rs`, `small/alloc_core_small/{find_segment,directory,dealloc}.rs`;
- `large/{alloc_core_large_cache,alloc_core_large_cache_eviction,large_cache_extended}.rs`;
- `segment_directory/*`, `segment_header/*` кроме двух указанных файлов, `config/*`, все `*_diag*`/`diag/*`;
- `registry/heap_core/{alloc/batch.rs, free/dealloc_batch.rs}`;
- `concurrent/lock_free/*`, `concurrent/epoch/epoch_region.rs` вне `Drop`;
- `kani_proofs.rs`, `registry/bootstrap/{loom_shim,chunk,mod}.rs`.

**Вердикт:** найдено **четыре новых пункта**.
- **R17-LIF-01** — ГИПОТЕЗА с условной оценкой P2: возможное зависание выхода процесса на Windows на спин-локах крейта, которые держал поток, убитый `ExitProcess`.
- **R17-LIF-02, R17-LIF-03, R17-LIF-04** — P4, SOURCE-CONFIRMED: дефекты контракта и покрытия no-panic/no-abort и паник публичного API.

Отдельно приведены **пять гипотез оптимизации и поддерживаемости** без измерений (§3) и новые доказательства к открытым пунктам 152 и 154 (приложение B). Новых P0/P1 и исполненных сбоев нет: раунд не исполнял код. Это не release GO, не perf GO и не доказательство отсутствия ошибок.

## 2. Находки

Шкала — как в задании раунда. SOURCE-CONFIRMED: путь установлен чтением кода, трасса перечислена. ГИПОТЕЗА: не подтверждена исполнением; указано условие, при котором она ложна.

| ID | Severity | Класс доказательства | Достижимость | Суть |
|---|---|---|---|---|
| R17-LIF-01 | P2 (условно, если подтвердится) | ГИПОТЕЗА. Трасса внутри крейта — SOURCE-CONFIRMED; поведение ОС и std взято из их документации, не исполнялось | `production` (и любой `alloc-global`) на Windows: процесс завершается (`main` возвращается или `std::process::exit`), пока другие потоки ещё аллоцируют или освобождают | `ExitProcess` убивает остальные потоки **до** `DLL_PROCESS_DETACH`. TLS-деструктор выходящего потока (`AbandonGuard` → trim → `SegmentTable::recycle` → `RouteDirectory::remove`; поздние `free` → `RouteDirectory::lookup`; аллокации после `TORN` → fallback `LOCK`) может вечно крутиться на спин-локе, который держал убитый поток. У локов нет восстановления после смерти владельца |
| R17-LIF-02 | P4 | SOURCE-CONFIRMED + `git log -S` + существующий тест `tests/r7_p1_chunk_oom.rs` | все `alloc-global` | No-panic контракт называет «единственным» преднамеренным abort на пути alloc OOM материализации чанка реестра. Этот abort недостижим из claim с `44b97046` (fallback), а около сотни преднамеренных `process::abort` на путях `GlobalAlloc` не упомянуты даже категорией. Follow-up R16 (`b707d196`) вернул устаревшую фразу |
| R17-LIF-03 | P4 | SOURCE-CONFIRMED | `production` (`assert!` в `pack_large_state`) и opt-in: `hardened`, `virgin-zero-skip`, `numa-aware`, `exact-object-proto` | Сканер, закрывший R16-01, смотрит 7 файлов и 3 формы (`expect`/`panic!`/`unreachable!`). Релизные `assert!`/`expect` на путях `GlobalAlloc` вне этих файлов есть. Перечисление «четырёх tripwire» снова неполное — тот же класс, что R16-01 |
| R17-LIF-04 | P4 | SOURCE-CONFIRMED | `pinning`, публичный `PinnedRunner::run` | `std::thread::Scope::spawn` паникует, если ОС не может создать поток. Раздел `# Panics` у `run` этого не называет и обещает только проброс паник воркеров |

### R17-LIF-01 — Windows: выход процесса может зависнуть в TLS-деструкторе выходящего потока на спин-локе убитого потока

**Места (крейт).**
- `src/global/tls_heap.rs:197–226` — `AbandonGuard::drop`: `mark_local_torn()`, затем `lease.core().trim_for_recycle()` (`:221`), затем `drop(lease)`.
- `src/registry/heap_core/state/ownership.rs:108–115` (`trim_for_recycle`) и `:140–156` (`trim_cold_retention`: `flush_all_tcache`, `drain_small_pool`, `release_empty_current_small_for_trim`, `evict_all`, `release_directory_for_cold_trim`).
- `src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:636–650` (`drain_small_pool` → `self.table.recycle(base)` на `:646`) и `:656–677` (`release_empty_current_small_for_trim` → `recycle` на `:676`).
- `src/alloc_core/segment/segment_table/segment_table_impl.rs:614–701` (`recycle` → `routes.remove(slot_id)`, `:675–678`).
- `src/alloc_core/segment/segment_table/route_slots.rs:203–212` (`slot.take()` роняет `RouteRegistration`), `src/registry/segment_route/registration.rs:78–82` (`Drop` → `directory.remove`).
- `src/registry/segment_route/directory.rs:891–897` (`remove` → `shard.lock()`) и `:868–889` (`lookup` → `shard.lock()`).
- `src/registry/segment_route/shard_lock.rs:48–66` — 64 тихих спина, затем бесконечный `std::thread::yield_now()`. Владелец не отслеживается, тайм-аута нет.
- Поздние `free`: `src/global/tls_heap.rs:321–336` (`TORN` → `ForeignNoBind`) → `src/registry/heap_core_xthread/routing.rs:26–31` (`publish_foreign` → `lookup`); до `AbandonGuard` чужой блок проходит через `dealloc_routing` → `publish_foreign` (`routing.rs:10–20`).
- Поздние аллокации (после `TORN`) идут в fallback: `src/global/fallback.rs:469–486` (`LockGuard::acquire` — бесконечный цикл spin/yield) и `:310–325` (цикл проигравших на `INIT_STATE == INITIALIZING`).
- Первый claim в позднем TLS-деструкторе потока, который ещё не был привязан, может упереться в sentinel материализации чанка (`OncePtrCell` — чистый spin без yield, `crates/once-ptr-cell/src/lib.rs:59–74`).

**Трасса (внутри крейта, SOURCE-CONFIRMED).** Выходящий поток при `production` имеет привязанную кучу: `GUARD` взведён в `finish_bind` (`tls_heap.rs:512–520`). Его `AbandonGuard::drop` вызывает `trim_for_recycle`. Если у кучи есть сегменты в пуле, пустой текущий Small-сегмент или сегменты, опустевшие при `flush_all_tcache`, путь доходит до `SegmentTable::recycle` → `RouteSlots::remove` → `RouteRegistration::drop` → `RouteDirectory::remove` → `ShardLock::lock()` на шарде, соответствующем ключу сегмента. Независимо от этого любой `free` чужого блока из TLS-деструкторов выходящего потока, до `AbandonGuard` или после, вызывает `RouteDirectory::lookup` → `ShardLock::lock()`. Аллокации после `TORN` уходят в `fallback::with_heap_config` → `LockGuard::acquire()`. Ни `ShardLock`, ни fallback `LOCK`, ни `INIT_STATE`, ни sentinel чанка не умеют обнаружить, что владелец мёртв. Если держатель исчез, ожидающий крутится бесконечно.

**Внешнее поведение (НЕ исполнялось, по документации).**
1. Документация Win32 `ExitProcess`: сначала завершаются все потоки процесса, кроме вызывающего, без `DLL_THREAD_DETACH`; затем точки входа DLL и TLS-callback вызываются с `DLL_PROCESS_DETACH` в контексте вызывающего потока. В Remarks прямо названа взаимоблокировка, если убитый поток держал лок, который пытается взять detach-код.
2. Документация `std::thread::LocalKey`, раздел «Platform-specific behavior»: при выходе процесса на Windows TLS-деструкторы выполняются только на потоке, вызвавшем выход, потому что остальные потоки могут быть принудительно завершены. Rust-`main` возвращается в CRT `exit()`, а `std::process::exit` на Windows вызывает `ExitProcess`, так что `AbandonGuard` главного потока выполняется уже после того, как убиты остальные.

**Наблюдено.** Трасса выше. Поиск по репозиторию (`git grep` по `ExitProcess`/`PROCESS_DETACH`/«process exit» в `src/`, `README.md` и индексах) не нашёл обсуждения этого сценария. README «Fork safety» (`README.md:1683–1719`) описывает тот же класс (лок, унаследованный от исчезнувшего потока) только для `fork()`. Про выход процесса сказано лишь «OS process exit stops the worker» (`maintenance_service.rs:10–13`).

**Выведено.** На Windows вероятность зависания при выходе ненулевая, если в момент `ExitProcess` хотя бы один другой поток находится внутри критической секции `ShardLock`: при межпоточном free (`lookup`), регистрации или удалении сегмента, при Large alloc/free. Выходящий поток должен затем взять тот же шард (из 64) в trim или в позднем `free`. Для producer→consumer нагрузок с несвязанными (detached) рабочими потоками, которые к выходу не заканчивают работу, это реальный рабочий сценарий. Последствие — процесс не завершается (spin/yield под loader lock), а не нарушение памяти. На Linux/glibc `exit()` вызывает TLS-деструкторы главного потока, пока остальные ещё живы, поэтому замороженных локов нет. Там гипотеза неприменима.

**Условия, при которых гипотеза ложна.** (1) Используемый std на Windows не выполняет TLS-деструкторы выходящего потока при `DLL_PROCESS_DETACH`. Тогда снимается часть с `AbandonGuard` и поздними TLS-`free`, но остаются C++-статики и atexit смешанных программ, освобождающие память SeferAlloc. (2) Процесс всегда присоединяет (join) или останавливает аллоцирующие потоки до выхода.

**Рекомендация.**
- Минимум — задокументировать в README рядом с «Fork safety» и в rustdoc `SeferAlloc`: «на Windows выход процесса при живых потоках, работающих с аллокатором, может зависнуть; останавливайте их до выхода».
- Структурно — teardown, знающий о shutdown процесса: `AbandonGuard::drop` пропускает trim (при выходе он всё равно бесполезен, см. H4), а `publish_foreign` в этом режиме не ждёт `ShardLock` и считает free «unroutable» (утечка при выходе безвредна). Детектор shutdown на Windows нужно выбрать и проверить отдельно; кандидат — `RtlDllShutdownInProgress` из ntdll, его статус документированности не проверялся.
- Альтернатива — ограниченное ожидание: `ShardLock` и fallback `LOCK` в контексте TLS-teardown (`LOCAL == TORN` или флаг thread-exit) берутся try-lock'ом с бюджетом; при неудаче маршрут или сегмент остаётся зарегистрированным, и его заберёт следующий владелец слота.

**Oracle исправления.** Windows-only тест в подпроцессе. Ребёнок ставит `#[global_allocator] SeferAlloc`, запускает 8 потоков с межпоточным ping-pong `Box` (держат шарды «горячими»), оставляет у главного потока сегменты в пуле и выходит без join. Родитель запускает ребёнка N раз с тайм-аутом T: красный — любой тайм-аут, зелёный — все запуски вышли. Детерминированный вариант — `bench-internals`-хук (по правилу CLAUDE.md о `dbg_*`-хуках), держащий шард конкретного ключа из вспомогательного потока до выхода: до исправления выход зависает, после — завершается за T.

### R17-LIF-02 — no-panic контракт: «единственный» abort на пути alloc давно не достижим, а реальные abort-tripwire не перечислены

**Места.** `src/global/sefer_alloc/mod.rs:188–191`:

```text
//! The one deliberate process kill on the alloc path is a direct
//! `std::process::abort()` (registry chunk-materialisation OOM,
//! `registry/bootstrap/registry.rs`), which neither unwinds nor runs the
//! panic hook.
```

Код: `src/registry/heap_registry/claim.rs:213` (claim использует `reg.slot_or_none(idx)`); `src/registry/bootstrap/registry.rs:250–274` (`ensure_chunk` с `std::process::abort()` на `:271`). Вызывается только из `slot()` (`:135–142`), а `slot()` в `src/` вызывают только `dbg_*`-хуки: `registry/heap_registry/counters.rs:375,414`, `registry/bootstrap/registry.rs:327,334,353`. `README.md:675` прямо говорит обратное: «New claims use fallible `slot_or_none` and can reach fallback on chunk OOM».

**Наблюдено.**
- `git log -S"The one deliberate process kill on the alloc path" -- src/global/`:
  - фраза добавлена в `91b0d98a` (2026-09-23);
  - `44b97046` (2026-09-29, «fix: fall back when registry chunk materialization fails») заменил в claim `reg.slot(idx)` на `reg.slot_or_none(idx)` и при этом добавил в claim новый `std::process::abort()` (сейчас `claim.rs:259`);
  - в `9df9f6b8` (R16-01) фраза была переписана;
  - в `b707d196` она восстановлена дословно.
- Существующий регрессионный тест `tests/r7_p1_chunk_oom.rs::claim_chunk_oom_falls_back_and_preserves_minted_indices` проверяет fallback, а не abort.
- В `src/` 106 мест `process::abort` вне комментариев. На путях `GlobalAlloc` (alloc/dealloc/realloc, холодный bind, teardown) это owner-only tripwire:
  - `route_slots.rs` 22, `directory.rs` 15, `claim.rs` 7 (5 на пути bind/teardown, включая `HeapLease::drop` при `panicking()` и при проигранном CAS, `:523–535`; 2 — `MaintenanceLease::drop`, путь воркера), `segment_table_impl.rs` 7, `alloc_core/sidecar_drain.rs` 7, `active_kind_index.rs` 6;
  - `alloc_core_large.rs` 5, `mem_impl.rs` 4, `alloc_core_small_reclaim.rs` 4, `terminal_words.rs` 2, `alloc_core_large_cache_eviction.rs` 2, `find_segment.rs` 2, `active_kind_ops.rs` 2, `segment_header_meta_fields.rs` 2;
  - `ownership.rs:43` (`stamp_segment_owner` на каждом alloc-miss пути), `small_sidecar.rs:41`, `bitmap_scan.rs`, `bitmap_cut.rs`, `issue_transaction.rs`.
- Контракт не называет эту категорию. Перечислены только «four release tripwires» (паники) и единственный abort про OOM чанка. Фраза «an unrecognised pointer → no-op» (`mod.rs:183`) тоже неточна для одного известного класса: внешний free внутреннего указателя доводит владельца до `abort()` (item 166).

**Выведено.** Это дефект контракта, а не runtime-сбой. Abort-tripwire недостижимы без уже испорченных метаданных. Но читатель контракта, например интегратор, решающий, может ли аллокатор убить процесс, получает неверную картину: «abort возможен только при OOM чанка реестра». На деле этот путь давно превращён в fallback, а abort-категорий десятки. `tests/no_panic_doc_accuracy.rs` эту фразу не пинит, поэтому её устаревание не ловится.

**Рекомендация.** Переписать абзац `mod.rs:179–199`:
- (а) OOM материализации чанка на claim даёт fallback, а abort в `Registry::slot` — tripwire только для уже материализованных индексов (как в README:675);
- (б) явная категория: «нарушения owner-only инвариантов метаданных на путях alloc/dealloc/bind/teardown завершают процесс прямым `std::process::abort()` (route slots, route directory, CAS слота реестра, segment table, sidecar drain/reclaim, Large reservation state, `HeapLease::drop` при panicking); они не разматывают стек и не вызывают panic hook»;
- (в) оговорка про item 166 у «unrecognised pointer → no-op».

**Oracle.** Расширить `tests/no_panic_doc_accuracy.rs`:
1. Старой фразы «The one deliberate process kill on the alloc path» быть не должно. Сейчас тест на этом красный: фраза на месте.
2. Контракт должен содержать абзац о категории abort-tripwire.
3. Лексическая перепись: каждый файл из множества, достижимого из `GlobalAlloc` (см. R17-LIF-03), с кодовым `process::abort` должен входить в явный список «abort-tripwire files» с причиной — по образцу `RELEASE_ALLOWLIST`.

Поведенческий oracle уже есть: `tests/r7_p1_chunk_oom.rs`, он и опровергает текущую фразу.

### R17-LIF-03 — сканер R16-01 узок по файлам и формам; релизные assert/expect на путях `GlobalAlloc` вне его поля зрения

**Места.**
- `tests/no_panic_doc_accuracy.rs:214–222` (`RELEASE_SCAN_FILES` — 7 файлов: `dealloc_own_base.rs`, `dealloc.rs`, `tcache_flush.rs`, `ownership.rs`, `hot.rs`, `alloc_core_large_cache.rs`, `realloc_fastpath.rs`), `:474–478` (ищутся только `.expect(`, `panic!`, `unreachable!`), `:493–532` (`production_release_panic_sites_match_explicit_allowlist`).
- Контракт: `src/global/sefer_alloc/mod.rs:95–139` («Four release-surviving invariant tripwires … All four live in the large-cache slot take/set helpers») и `:192–197`.

**Наблюдено (не `const`-контекст, не тесты, не `dbg_*`).**

| Место | Форма | Как достигается из `GlobalAlloc` | Конфигурация |
|---|---|---|---|
| `src/alloc_core/segment/segment_header/terminal_words.rs:47–51` | `assert!(generation <= MAX_LARGE_GENERATION)` в `const fn pack_large_state`, вызываемой в рантайме | `src/alloc_core/large/reservation_state.rs:42–51,58–73,86–101,128–143,152–161` (`publish_pending`, `claim_pending`, `claim_live`, `begin_reuse`, `transition`) → Large free (`mem_impl.rs:265–270`), cache hit (`alloc_core_large.rs:251,459,483`), reclaim, терминальная публикация (`segment_route/large_state.rs:25–28`) | `production` |
| `src/alloc_core/segment/segment_header/segment_header_gen_table.rs:98–110` | `assert!(idx < GEN_TABLE_FOOTPRINT)` в `bump_gen` | выдача из magazine: `registry/heap_core/alloc/hot.rs:77–80,146–156,374–384,501–511`; `alloc_core_small_impl.rs:461` | `hardened` |
| `segment_header_gen_table.rs:55–67` | то же в `gen_at` | вызовов нет (`#[allow(dead_code)]`) | `hardened` |
| `src/alloc_core/small/alloc_core_small_magazine.rs:136–151` | `assert!(out.len() <= 16)` | `hot.rs:572–612` (`refill_magazine_slow_virgin` → `:586`), путь промаха `alloc_zeroed` | `virgin-zero-skip` + `fastbin` |
| `src/alloc_core/platform/numa.rs:91–95` | `.expect("node != NO_NODE…")` | резервирование сегментов на alloc-пути | `numa-aware` |
| `src/global/exact_object/exact_shard.rs:33–36` | `.expect("descriptor layout")` | `ExactNarrow::alloc` → `ExactTable::register` → `rehash` | `exact-object-proto` |

Ни одно из этих мест не входит в «четыре» и не видно сканеру: файлы не в списке, а `assert!` как форма не ищется вовсе. Даже в отсканированных 7 файлах новый релизный `assert!`, `assert_eq!` или `.unwrap()` прошёл бы тест зелёным. Закрывающая карточка 174 честно говорит «the scanner is lexical, not a proof of no-panic», но не оговаривает узость его области (7 файлов, 3 формы).

**Выведено.** Для корректной работы все эти места недостижимы:
- `pack_large_state`: поколение всегда получено как `word >> 3` или из ограниченного `next_large_generation`. Удалит ли LLVM проверку в release, не проверялось — компиляции не было;
- `bump_gen`: `off < SEGMENT`;
- магазин: `TCACHE_CAP <= 16`;
- NUMA: ветка `node != NO_NODE`.

Поэтому это P4: перечисление контракта неполное («any configuration» с «четырьмя» сайтами), а регрессионный сторож закрывает класс R16-01 только для 7 файлов. Это тот же механизм, которым R16-01 и прошёл незамеченным.

**Рекомендация.** Расширить сканер:
- (а) формы: `assert!`, `assert_eq!`, `assert_ne!`, `.unwrap()`, `unimplemented!`, `todo!`;
- (б) пропускать `const _: () = assert!(…)` и блоки `const { … }` (в самотест добавить такие случаи);
- (в) вместо явного списка из 7 файлов сканировать `src/{global,registry,alloc_core}/**` за вычетом явного списка исключений с причинами (`*_diag*`, `diag/`, `segment_table/harness.rs`, `bootstrap/loom_shim.rs`, `kani_proofs.rs`).

В allowlist внести перечисленные места с одной строкой обоснования, а в контракте заменить «Four … All four live in …» на перечисление, которое генерируется из allowlist или совпадает с ним. `gen_at` — либо удалить, либо внести с причиной.

**Oracle.** На текущем дереве расширенный сканер красный: как минимум `terminal_words.rs:49` для любого набора features и места из таблицы при их features. После внесения в allowlist и правки текста — зелёный. Самотест сканера получает кейсы `assert!` в рантайме (ловится) и `const { assert!(…) }` (не ловится).

### R17-LIF-04 — `PinnedRunner::run` паникует при отказе создания потока, `# Panics` об этом молчит

**Места.** `src/concurrent/pinning.rs:211–214` (`# Panics: Propagates a panic from any worker …`), `:225–256` (`std::thread::scope(|s| … s.spawn(move || …))`).

**Наблюдено.** `std::thread::Scope::spawn` документирован как паникующий, если ОС не смогла создать поток (для восстановления std предлагает `Builder::spawn_scoped`). `run` вызывает `s.spawn` для каждого воркера и не ловит отказ. Комментарий `:243–249` («fails only when the shard is out of range») устарел после R14-04: `bind_current_thread_to_shard` возвращает `false` и при разрушенных TLS-ячейках маршрутизатора. Внутри только что созданного scoped-потока это не проявляется.

**Выведено.** Публичный API под `pinning` может паниковать в рантайме при исчерпании лимита потоков или памяти под стек, а контракт об этом не говорит. Ошибки памяти нет.

**Рекомендация.** Дописать `# Panics` либо перейти на `Builder::spawn_scoped` и вернуть ошибку (`bool`/`Result`). Поправить комментарий `:243–249`.

**Oracle.** Doc-тест на текст в `tests/`; поведенческий oracle потребовал бы принудительного отказа ОС при создании потока и не рекомендуется.

## 3. Гипотезы оптимизации и поддерживаемости (не измерены, без GO)

Ничего ниже не измерялось. Это не speedup и не RSS-результат. Любой судья обязан соблюдать правила R26-4/R30-8/entry-point/regime из CLAUDE.md.

- **H1 — повторный claim делает полный cold trim и отдаёт ОС память, которая нужна новому владельцу.**
  - Механизм: `HeapRegistry::claim_impl` в ветке повторного claim вызывает `trim_for_recycle()` (`src/registry/heap_registry/claim.rs:308–318`), то есть не только `drain_sidecar_ingress`, но и `trim_cold_retention`: `drain_small_pool`, `evict_all`, `release_directory_for_cold_trim` (`state/ownership.rs:140–156`). Пул и кэш уже очищены trim'ом при выходе прежнего владельца (`AbandonGuard`). Сегменты, опустевшие от публикаций, пришедших, пока слот был FREE, попадают в пул и сразу освобождаются, хотя новый поток вот-вот начнёт аллоцировать. Судя по комментарию в `claim.rs:312–314`, намерение было только «reclaim once».
  - Вариант: на повторном claim — только drain.
  - Судить на реальном `#[global_allocator]` при spawn/join churn. Oracle активации — дельта `segments_released_total` внутри claim. Метрики — `segments_reserved_total`, число reserve-syscall и RSS-ось как жертва (удержание между владельцами). Связано с perf item 13 (`churn_with_teardown`).
- **H2 — воркер обслуживания каждые 10 мс делает полный cold trim fallback-кучи под глобальным fallback `LOCK`.**
  - Механизм: `maintenance_service.rs:147–150` → `fallback::try_with_heap(|core| core.background_maintenance_step(…))` → `trim_cold_retention` (`ownership.rs:117–138`). Критерий «idle fallback» из контракта (`sefer_alloc/core.rs:164–169`; `sefer_alloc/maintenance.rs:21–22` — «tries the fallback lock once») в коде сводится к «лок свободен в момент try»: активно используемой fallback-куче (исчерпание реестра на 4096 слотах, TLS-teardown волнами) каждый проход сбрасывают магазины, пул и Large-кэш, а её пользователи стоят в `LockGuard::acquire` на время trim.
  - Oracle: нагрузка только через fallback, maintenance вкл/выкл, время удержания `LOCK`, hit-rate магазина. Связано с perf items 66 и 78(a).
- **H3 — проигравшие на sentinel чанка реестра крутятся без yield.**
  - `OncePtrCell` (`crates/once-ptr-cell/src/lib.rs:59–74`) — чистый `spin_loop`, а R12-04 добавил `yield_now` только в цикл проигравших fallback-init (`fallback.rs:310–325`). При вытеснении победителя (OS reservation чанка) и переподписке CPU проигравшие жгут квант; при SCHED_FIFO возможна инверсия приоритетов. Аналогично чистый spin в `exact_shard.rs:184–192` (прототип).
  - Выигрыш ожидается мал: 64 чанка, разовая материализация.
- **H4 — teardown, знающий о shutdown.**
  - При выходе процесса trim в `AbandonGuard::drop` бесполезен (адресное пространство всё равно освобождается) и тратит время выхода на сотни `release_segment`/`RouteDirectory::remove`. Пропуск trim при shutdown одновременно чинит R17-LIF-01 и ускоряет выход.
  - Мера — время выхода и число syscall при выходе на нагрузке с большим числом сегментов.
- **Поддерживаемость (дрейф прозы; примеры к долгу item 154):**
  - `src/global/tls_heap.rs:481–487`: ветка вложенного bind обосновывается тем, что инициализация NUMA-топологии повторно входит в аллокатор. После task #777 инициализация в `numa-shim` не аллоцирует (`crates/numa-shim/src/lib.rs:1516–1536`), а сам вход использует `open` без `std::fs` (`:1682–1700`). Известного триггера у ветки нет; её стоит оставить как защиту, но поправить пример.
  - `src/global/sefer_alloc/core.rs:60–65`: «the `HeapCore` and ALL its segments stay whole in the slot» противоречит trim при выходе: пул, пустые сегменты и Large-кэш освобождаются (`ownership.rs:90–115`).
  - `tls_heap.rs:133–139`: doc `LOCAL` опирается на «reverse declaration order», а модульный doc (`:51–56`) это порядковое допущение отвергает. В `:117–124` есть ссылка на несуществующий раздел «TLS destructor ordering».
  - `fallback.rs:524–525` ссылается на `src/alloc_core/alloc_core.rs` (теперь `alloc_core/alloc_core/lifecycle.rs`).
  - `registry/bootstrap/registry.rs:155–168` упоминает удалённые `resolve_dirty_bit_target`/`resolve_heap_overflow`.
  - Шапка `claim.rs:5–9` утверждает, что `get_unchecked` ушёл, хотя `registry.rs:141,176,235` его используют (корректно, за счёт `% CHUNK_SLOTS`).

## 4. Вне темы (место + одна строка)

- `src/alloc_core/segment/segment_table/segment_table_impl.rs:627–631,667–671`: `debug_assert!(false, …)` в `recycle` на пути free даёт в debug-сборке панику внутри `dealloc` (для пользователя с double free), в release — утечку. Такое расхождение поведения допускается контрактом.
- `src/registry/segment_route/directory.rs:843–846` и `route_slots.rs:89–91`: `RouteError::Duplicate` (нарушение инварианта) схлопывается через `.ok()` в тот же `None`, что и OOM. Порча неотличима от нехватки памяти.
- `src/global/exact_object/exact_fatal.rs:3–7`: `fatal()` пишет в `std::io::stderr()` из пути `GlobalAlloc` (прототип). Отсутствие аллокаций при этом держится на деталях реализации std.
- `src/concurrent/sharded/sharded_region.rs:486–511`: `ERASED_GUARD.with` (не `try_with`) идёт после проверки `try_with`; правильность держится на том, что между ними TLS не разрушается (экспериментальный ярус).

## 5. Что проверено и НЕ дало находок

- **TLS-протокол `TORN`** (`tls_heap.rs`). Три резолвера, порядок «сначала `LOCAL`, затем `GUARD`» в `finish_bind` и обе ветки отката. На нативном TLS состояние «`GUARD` разрушен при `LOCAL == null`» недостижимо: `GUARD` получает деструктор только при первом взводе, который всегда кладёт lease, а его `Drop` ставит `TORN`. Поэтому `HeapLease::drop` с abort при `panicking()` (`claim.rs:523–525`) на откате `finish_bind` практически недостижим. При `TORN` второй `&mut HeapCore` не создаётся: все резолверы отдают Fallback, ForeignNoBind или None.
- **Реентерабельность.** Лексическая проверка production-путей `alloc_core`, `registry` и `global` нашла конструкции глобального аллокатора только в `Display` у `maintenance_start_error.rs` (не путь alloc) и в тестовой обвязке `segment_table/harness.rs`. Метаданные directory, route slots, leaf classes и exact-object берутся у `System`. Инициализация топологии `numa-shim` не аллоцирует. Повторный вход в `claim_impl` не зацикливается: подсказка поглощается `swap`, а scan пропускает INITIALIZING, LIVE и MAINTENANCE. `std::sync::Mutex` есть только в `maintenance_service` и на путях alloc не используется.
- **Порядок локов.** Единственная вложенность: fallback `LOCK` → `ShardLock` (через `with_heap`, затем alloc или release, затем register или remove). `lookup` отпускает шард до `try_with_heap` (`routing.rs:32–44`), вложенных шард-локов нет, под `ShardLock` вызывается только `System.dealloc` (`directory.rs:618,650`). Цикла нет.
- **Откаты при OOM и согласованность счётчиков.** Проверены:
  - `alloc_large_slow`, все три ветки резервирования: обычная, biased, NUMA (`os.rs:178–218`, `numa.rs:70–168`);
  - ветки неудачи cache hit в `alloc_large`: `begin_large_reuse` → None и `register_payload` → None, оба со снятием `large_cache_used_bytes` до release;
  - `reserve_small_segment` (release при отказе `register`), RAII-сброс `RouteRegistration` в `register_payload`, `attach_owner` (исправление R15-02, порядок полей `Primordial{segment, table}` безопасен: у `SegmentTable` нет `Drop`, касающегося памяти primordial);
  - частичная неудача `EntryHandle::new` и ветки ошибок `RouteDirectory::register` (сброс `SpareBlock`/`BlockIndex`);
  - spill-OOM в `prepare_small_issue` → `Err` → null.
- **Нулевой размер, большой align, realloc с нулём.** Размер округляется до `MIN_BLOCK`. При `align >= SEGMENT` работает biased Large и `checked_add`. В move-ветках realloc есть `Layout::from_size_align`. При `new_size == 0` realloc ведёт себя защитно (выдаёт блок `MIN_BLOCK`). Для пользователя, соблюдающего контракт `GlobalAlloc`, переполнений, в том числе debug-overflow, не найдено.
- **Drop-безопасность.** Просмотрены все 23 `impl Drop`. `RouteRegistration` сначала снимает маршрут, потом отпускает счётчик. `AllocCore::drop` начинает с `close_routes`. Проверены счётчик в `Segment::drop` (R15-02), изоляция паник в `EpochRegion::drop` (R15-01: `catch_unwind` на каждый слот, `forget` вторичных payload), `LockGuard`/`InitStateGuard` (item 23 без изменений) и `StartingGuard`/`WorkerGuard`.
- **Сервис обслуживания.** `RUNNING` пишется под `CONTROL` до `notify_all`, поэтому пробуждение не теряется. Отказ spawn откатывается в IDLE. Размотка или возврат воркера дают abort. `CONTROL` не держится через lease или fallback-лок.
- **`fork()`.** Обработки нет (`pthread_atfork` отсутствует). Это известный контрактный пробел item 152; новое доказательство — в приложении B.
- **Выход процесса на Linux/glibc.** TLS-деструкторы главного потока выполняются при живых остальных, поэтому замороженных локов нет (в отличие от Windows, R17-LIF-01).
- **Арифметика `Instant`.** `duration_since` насыщается (MSRV 1.93), паники нет.
- **Follow-up R16.** `b707d196`: маска корня в flush-циклах верна для блоков в магазине (Small/Primordial, корни выровнены на `SEGMENT`); новых релизных панических мест нет. `486f5ace`, `32cacc97` и `222e913d` прочитаны и соответствуют своим закрытиям.
- **Debug-only паники под fallback `LOCK` или внутри `&mut HeapCore`** через аллокацию payload снова входят в аллокатор (возможен самоблок или aliasing). Это прямо признано контрактом (`mod.rs:174–177,198–199`) и новой находкой не является.

## Приложение A. Инструменты

- Подсчёт строк — Node-скрипт во временном игнорируемом `target/r17lif/census.cjs`, удалён до сдачи. Логика: `git ls-files src`; физические строки — `split(/\r?\n/)` без хвостовой пустой; «код» — непустые строки, `trim()` которых не начинается с `//`. Вывод: `{"files":168,"phys":43325,"code_nonblank_not_slashslash":17059,"blank":1963,"slashslash_comment_lines":24303}`.
- Unsafe: `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/` (по тирам — с `#!\[` и `#\[`).
- Перепись abort: `grep -rn "process::abort" src/ | grep -vE '^[^:]+:[0-9]+:\s*//'` — 106.
- История фразы контракта: `git log -S"The one deliberate process kill on the alloc path" -- src/global/`; `git show 44b97046 -- src/registry/heap_registry/claim.rs`.
- Исполненных witness нет: режим «только чтение» исключал запуск кода.

## Приложение B. Связь с открытыми пунктами индексов

| Пункт | Связь | Что ново |
|---|---|---|
| correctness **152** (fork) | контрактный пробел, тема раунда | Карточка «Current-number-or-verdict» (`TRACKED_platform_contracts.md:316`) перечисляет снятые механизмы: overflow-sidecar sentinel, spill-stack `ready == 0`, deferred-Large `PUBLISHING`, `owner_slot_is_live`. По правилу R34-24 она должна описывать текущее состояние. Текущие места, где в дочернем процессе возможен вечный спин: `ShardLock` × 64 (`shard_lock.rs:48–66`), sentinel `OncePtrCell` чанков (чистый spin), fallback `INIT_STATE` и `LOCK`, `TOPOLOGY` `OnceLock` в `numa-shim` (`numa-aware`), спин шарда exact-object (прототип); плюс слоты, навечно оставшиеся INITIALIZING (не зависание, а утечка). README и rustdoc `SeferAlloc` формулируют это обобщённо и верно. Статус и триггер карточки не меняются, обновить нужно только список. Близкий по классу Windows-сценарий выхода процесса — R17-LIF-01 |
| correctness **154** (проза) | поддерживаемость | новые примеры — §3, последний пункт |
| correctness **166** | контракт «unrecognised pointer → no-op» | отмечено в R17-LIF-02 (в), без нового доказательства |
| correctness **23** | `InitStateGuard` | без изменений, окно после `write(hc)` по-прежнему без источника паники |
| correctness **162** | приёмка terminal ingress (fallback, «last free after owner exit», worker failure policy) | R17-LIF-01 и H2 — дополнительные вопросы к приёмке; статус не меняется |
| correctness **174** (CLOSED) | сканер R16-01 | R17-LIF-03: область сканера уже, чем закрываемый им класс; закрытие не переоткрывается — заводится новый пункт |
| perf **13**, **66**, **78(a)** | H1, H2 | гипотезы, не измерены |

Решения по остальным пунктам обоих индексов в рамках этого тематического отчёта — LEAVE, поскольку тема их не затрагивает. Итоговую сверку статусов делает оркестратор раунда.
