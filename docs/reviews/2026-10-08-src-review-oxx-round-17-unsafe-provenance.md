# Ревью `src/` — раунд 17 (oxx) — unsafe-корректность, provenance, aliasing, UB-модель Rust

## 1. Охват, инвентарь, метод, база, вердикт

**Режим: без компиляции и без исполнения кода.** По приказу владельца, переданному координатором во время работы, не запускались cargo (build/check/test/clippy/rustdoc), Miri, Loom, Kani, бенчмарки, witness-тесты и мутанты. Использовались только чтение файлов, `grep`/`awk`/`wc` и read-only git (`log`, `show`, `diff`, `status`). До приказа исполняемых действий тоже не было. Поэтому у всех выводов класс SOURCE-CONFIRMED или ГИПОТЕЗА; класса «исполненный witness» в отчёте нет.

**Тема раунда:** корректность каждого `unsafe` блока, `unsafe fn` и `unsafe impl`; `// SAFETY:` и `# Safety` против реальных вызывающих путей; provenance (map_addr, int↔ptr, канонические корни), aliasing под Stacked/Tree Borrows, неинициализированная память, layout и валидность значений, `Send`/`Sync`, `static mut`/`UnsafeCell`, время жизни указателей после release/decommit, арифметика указателей.

**База.** `453439c123f71a98890408dd0170bd909ad9ecdf` (HEAD worktree ревью). Более поздний коммит `59159a02` на `main` добавляет только `docs/checkpoints/2026-10-08-1614.md`, `src/` тот же. Относительно базы R16 (`6a0d47f6`) в `src/` изменено 22 файла (+130/−122) коммитами follow-up R16. Существенные для темы: `impl Drop for Segment` (`alloc_core/platform/os.rs:167–172`), изоляция паник по слотам в `EpochRegion::drop` (`concurrent/epoch/epoch_region.rs:687–711`), разрешение корня маской сегмента в overflow-flush и `flush_all_tcache` (perf 84; `registry/heap_core/free/dealloc_own_base.rs:482–494`, `registry/heap_core/state/tcache_flush.rs:86–98`), const-assert `SMALL_CLASS_COUNT <= 64` (`alloc_core/platform/size_classes.rs:213–217`). Остальное — комментарии.

**Инвентарь** (пересчитан, не скопирован; команды в приложении A):

- 168 файлов, 43 325 физических строк: 17 059 непустых строк, не начинающихся с `//`; 24 303 строки-комментария `//`/`///`/`//!`; 1 963 пустые. По группам: `alloc_core/` 88 файлов / 24 802 строки, `registry/` 46 / 10 413, `global/` 18 / 3 809, `concurrent/` 14 / 3 554, `lib.rs` + `kani_proofs.rs` 2 / 747.
- Команда из CLAUDE.md `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`: в `src/` 104 строки — 21 модульная (tier 1, 21 файл) и 83 item-уровня (tier 2, 30 файлов); в `crates/` 6 + 20. Совпадает с R16 (его «30 файлов» — число tier-2 файлов).
- Синтаксис в `src/` (строки, где перед ключевым словом нет `/`): 219 строк с `unsafe {`, 68 объявлений `unsafe fn`, 9 `unsafe impl`: `GlobalAlloc for SeferAlloc`; `Sync for HeapSlot`; `Send`/`Sync` для `ShardLock<T: Send>`; `Sync for ExactShard`; `Send`/`Sync` для `AtomicSlot<T: Send + Sync>`; `Send`/`Sync` для `loom_shim::OncePtrCell<T>` (только `cfg(loom)`).
- Касты `as *mut ` / `as *const ` в `src/`: 86 строк вне комментариев, каждая классифицирована по типу левого операнда. 83 — указатель или ссылка → указатель. Целое → указатель — три: константа `TORN` (`global/tls_heap.rs:125`, вычисляется в CTFE, не разыменовывается), `kani_proofs.rs:36` (только `cfg(kani)`) и единственный исполняемый в runtime каст `registry/heap_core/alloc/batch.rs:184` (под `hardened`, R17-UNS-05). Кастов `as _`, `with_exposed_provenance` и `transmute` в `src/` нет. Числовые ключи хеша, ключи каталога и сентинели строятся через `core::ptr::without_provenance_mut` (`segment_table/hash.rs:81`, `segment_table/harness.rs:192`, `alloc_core/bootstrap.rs:264`, `segment_route/registration.rs:42`, `segment_route/small_sidecar.rs:117`, `registry/bootstrap/loom_shim.rs:132, 206`).

**Метод.**

1. Обязательное чтение индексов (подробно ниже) и отчёта R16 с манифестом. Для R2-05 (xa round 2) прочитаны раздел отчёта `docs/reviews/2026-09-22-120730-src-review-xa-round-2.md` и сообщение исправляющего коммита `4601f890`.
2. Перечень всех tier-1/tier-2 мест. Для каждого `unsafe` блока заявленное в `SAFETY` сверялось с фактическими вызывающими путями (кто вызывает, откуда берётся указатель, какой у него provenance, чем ограничено время жизни памяти).
3. Сплошной grep по `src/` с чтением каждого попадания: `as usize` у указателей, `as *mut`/`as *const`, `map_addr`/`with_addr`/`without_provenance`; создание ссылок из сырых указателей (`&*`, `&mut *`, `as_ref`, `get_unchecked`); `MaybeUninit`/`assume_init`/`transmute`/`mem::zeroed`; пары `mem::forget`/`into_reservation`/`into_parts`/`release_segment`; адресная проверка членства (`contains_base_ro`) с последующим разыменованием; все `pub fn` с параметрами-указателями (единые и многострочные сигнатуры, включая не-`dbg_*`).

**Прочитано полностью построчно (код и комментарии):** `lib.rs`; `alloc_core/platform/{os,node,sidecar,numa}.rs`; `alloc_core/segment/segment_table/{segment_table_impl,hash,route_slots,harness,active_kind_index}.rs`; `alloc_core/segment/remote_bitmap/sidecar_bitmap.rs` и `sidecar_bitmap/leaf_classes.rs`; `alloc_core/segment/bitmap/{segment_bitmap,magazine_bitmap}.rs`; `alloc_core/segment/segment_header/{segment_header_gen_table,terminal_words}.rs`; `alloc_core/alloc_core/{sidecar_test_hooks.rs, mem/mem_impl.rs}`; `alloc_core/alloc_core/alloc_core_core_diag/{header_diag,table_diag}.rs`; `alloc_core/large/alloc_core_large.rs`; `alloc_core/small/{alloc_core_small_magazine,alloc_core_small_diag,alloc_core_small_reclaim}.rs`; `alloc_core/small/alloc_core_small/{dealloc,directory}.rs`; `alloc_core/small/alloc_core_small_pool/decomp_hooks.rs`; вся `registry/segment_route/` кроме `route_cut/route_record/route_scan/error/kind/terminal_publication_gate` (они без `unsafe`, просмотрены); `registry/{heap_slot,mod}.rs`; `registry/heap_registry/{claim,maintenance}.rs`; `registry/bootstrap/{registry,ensure,chunk}.rs`; `registry/heap_core/free/{dealloc_own_base,dealloc_batch,realloc}.rs`; `registry/heap_core/alloc/hot.rs`; `registry/heap_core/state/tcache_flush.rs`; `registry/heap_core_xthread/{routing,sidecar_drain}.rs`; `global/{fallback,tls_heap,mod,maintenance_service}.rs`; `global/sefer_alloc/{global_alloc,diag}.rs`; `global/exact_object/{exact_shard,narrow}.rs`; `concurrent/epoch/hand.rs`.

**Выборочно (диапазоны):** `alloc_core/alloc_core/bootstrap.rs:60–434`; `alloc_core/alloc_core/lifecycle.rs:90–557`; `alloc_core/alloc_core/mem/realloc_fastpath.rs:180–561`; `alloc_core/large/alloc_core_large_cache.rs:60–420`; `alloc_core/large/large_cache_extended.rs:100–258`; `alloc_core/large/reservation_state.rs:1–80`; `alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:360–640`; `alloc_core/small/alloc_core_small/find_segment.rs:160–400`; `alloc_core/small/alloc_core_small/reserve.rs:160–464`; `alloc_core/small/alloc_core_small_pool/decommit.rs:1–140`; `alloc_core/segment/segment_header/{descriptors.rs:120–339, segment_header_impl.rs:163–194, 287–586 (поля), 688–749, segment_header_views.rs:1–60, segment_header_meta_fields.rs:78–96}`; `alloc_core/segment/segment_directory/segment_directory_impl.rs:270–325`; `alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs:150–469`; `alloc_core/alloc_core/state.rs:69–94`; `registry/heap_core/diag/diag_probes.rs:200–400`; `registry/heap_core/diag/queries.rs` (все функции с параметром-указателем); `registry/heap_core/state/ownership.rs:40–100`; `registry/heap_core/free/dealloc.rs:170–259`; `registry/heap_core/alloc/batch.rs:60–302`; `registry/bootstrap/loom_shim.rs:1–200`; `global/sefer_alloc/core.rs` (оглавление функций); `concurrent/epoch/epoch_region.rs` (только `Drop` и диф R16).

**Только скрининг** (grep по шаблонам из п. 3 метода, без сплошного чтения): остальные файлы, в том числе `alloc_core_small_pool_impl.rs`, `alloc_core_large_cache_eviction.rs`, `alloc_core/alloc_core/sidecar_drain.rs` (R16 прочитал полностью), `counters.rs`, `config/*`, `size_classes.rs`, `segment_directory_impl.rs` вне диапазона, `perf_diag.rs`, `vmem.rs`, `totals.rs`, `concurrent/{lock_free,sharded}/*`, `pinning.rs`, `registry/heap_registry/{counters,stack}.rs`, `saturation.rs`, `issue_transaction.rs`, `kani_proofs.rs`. Утверждения «не найдено» относятся только к прочитанному.

**Чтение индексов.** Полностью: `docs/CORRECTNESS_OPEN_ITEMS.md` (с lookup-таблицей), `docs/correctness-open-items/ACTIVE.md`, `TRACKED_correctness_residuals.md`, `TRACKED_verification_coverage.md`, `TRACKED_hook_safety.md`, `TRACKED_misc.md`. `docs/perf/OPEN_ITEMS.md`: шапка с диспозициями R13–R16, все карточки `[A]`, хвост `[L]` 64–83, Recently resolved и таблица F1–F13; середина `[D]`/`[L]` — по current-state карточкам и grep на `provenance|aliasing|UB|unsafe|miri|dangling|uninit` (тематических для раунда пунктов там нет). `TRACKED_platform_contracts.md`, `TRACKED_ci_gate_coverage.md`, `TRACKED_test_flakiness.md`, `TRACKED_process_record.md`, `TRACKED_publish_readiness.md` — по lookup-таблице, заголовкам и точечному grep (`dbg_is_decommitted_for`, `regression_r2_05`, `miri`): их предмет вне `src/` или вне темы; items 73/74 упоминают decommit-хуки только как средство активации oracle. Решения по прежним пунктам: ни один не переоткрывается. P1-box (item 164) не трогается: нового довода нет. Miri-остаток 171 не перезапускался. Связь новых находок с индексами — в конце §2.

**Вердикт.** Пять новых находок: одна P1 и четыре P4.

- **R17-UNS-01 (P1)** — остаток R2-05. Безопасный `AllocCore::dbg_is_decommitted_for` читает метаданные сегмента через provenance вызывающего после адресной проверки членства. Недостижим в `production`: нужен тестовый `internals`.
- **R17-UNS-02 (P4)** — контракт `dbg_stamp_kind_byte` разрешает вызовы, которые типизированно копируют `SegmentHeader` с невалидным `SegmentKind`.
- **R17-UNS-03 (P4)** — `# Safety` unsafe-хуков того же класса не требуют provenance или тега, покрывающего метаданные.
- **R17-UNS-04 (P4)** — устаревшие обоснования безопасности в «единственной авторитетной» таблице `scripts/verify-dbg-hook-safety.mjs`.
- **R17-UNS-05 (P4)** — единственный runtime-каст целого в указатель в `src/` (`HeapCore::alloc_batch` под `hardened`). Законен только благодаря exposure на соседней строке; `SAFETY` об этом молчит, а Miri в режиме проекта по умолчанию этот путь не исполнит.

Новых P0/P2/P3 в прочитанной области не найдено. Это не доказательство отсутствия ошибок, не release GO и не perf GO. Изменения follow-up R16 под углом темы находок не дали (§5).

## 2. Находки

Шкала — как в задании раунда (совпадает с R15/R16). Класс доказательства: SOURCE-CONFIRMED — путь установлен чтением кода (трасса приведена); ГИПОТЕЗА — не подтверждено. Исполненных witness нет: режим только чтение.

| ID | Severity | Класс доказательства | Достижимость | Суть |
|---|---|---|---|---|
| R17-UNS-01 | P1 | SOURCE-CONFIRMED (трасса + `git show 4601f890` + grep тестов/сканера) | `alloc-decommit` + `internals` (например стандартная тестовая строка `production internals`); публичный тип `sefer_alloc::AllocCore`, безопасный `#[doc(hidden)] pub fn dbg_is_decommitted_for`; в чистом `production` недостижим | Остаток R2-05: безопасный хук после проверки членства только по адресу читает заголовок и флаг через указатель с provenance вызывающего. Безопасный код с `ptr::without_provenance_mut(addr)` получает UB |
| R17-UNS-02 | P4 | SOURCE-CONFIRMED | `internals`; `unsafe fn dbg_stamp_kind_byte`, затем названные в его контракте `dbg_unregister`/`dbg_recycle` или read-only `large_geometry_for_test` | Контракт разрешает записать в `kind` любой байт и затем вызывать функции, которые копируют `SegmentHeader` с полем `kind: SegmentKind`. Байт вне {0,1,2,0xFF} даёт невалидное значение enum при `assume_init`, то есть немедленное UB |
| R17-UNS-03 | P4 | SOURCE-CONFIRMED (текст контрактов + тела) | `internals` (часть хуков также `bench-internals`/`hardened`) | Семь unsafe-хуков выводят указатель на метаданные из указателя вызывающего через `segment_base_of_ptr`. Их `# Safety` требует лишь «valid, live allocation pointer», не provenance или тег, покрывающий метаданные. Коммит R2-05 утверждал обратное |
| R17-UNS-04 | P4 | SOURCE-CONFIRMED | Инструментальный гейт `scripts/verify-dbg-hook-safety.mjs` (CI и `npm run check`) | Обоснования `dbg_claim_lease`/`dbg_try_maintenance` — «exposes no core access (core() is pub(crate))» — ложны с Ph4c и противоречат соседней записи той же таблицы. Критерий PURE_OBSERVERS («read-only») не видит класс R2-05, поэтому R17-UNS-01 прошёл гейт |
| R17-UNS-05 | P4 | SOURCE-CONFIRMED (grep всех 86 `as *mut`/`as *const` + чтение контекста) | `alloc-global` + `batch-api` (тянет `experimental`) + `hardened`; `SeferAlloc::alloc_batch` (`pub unsafe fn`) → `HeapCore::alloc_batch` → refill-хвост | `bump_gen_on_issue(base as *mut u8, off)` с `base: usize` — единственный runtime int→ptr в `src/`. Законен только через exposure `as usize` на :172/:179; `SAFETY` и контракт `bump_gen` о provenance молчат; под `-Zmiri-strict-provenance` путь неисполним; `.addr()`-чистка соседних строк (H2) сделала бы его зависимым от чужого exposure |

### R17-UNS-01 — безопасный `dbg_is_decommitted_for` разыменовывает provenance вызывающего (остаток R2-05)

**Места.**

- `src/alloc_core/small/alloc_core_small_pool/decommit.rs:21–36` — `dbg_is_decommitted_for`: гейты `internals` (:21) и `alloc-decommit` (:23); `let base = os::segment_base_of_ptr(ptr)` (:25), проверка `contains_base_ro(base)` (:26), затем `SegmentHeader::kind_at(base)` (:30) и `SegmentMeta::new(base).is_decommitted()` (:35).
- Тот же шаблон в unsafe-соседе `dbg_force_decommit_retain_for` (:97–108). Он не только читает, но и пишет: `decommit_empty_segment_impl(&mut meta, base, false)` на :106.
- Публичность: `src/lib.rs:512` (`pub use alloc_core::{AllocCore, SegmentLayout}` только под `alloc-core`). Механизм гейтинга методов — `Cargo.toml:441–450`.

**Трасса (только безопасный код вызывающего).**

1. `AllocCore::new()` (`alloc_core/alloc_core/lifecycle.rs:110`), затем `AllocCore::alloc(layout)` (`alloc_core/alloc_core/mem/mem_impl.rs:54`) возвращает `p`.
2. Вызывающий строит `q = core::ptr::without_provenance_mut::<u8>(p.addr())` (безопасная функция std) и вызывает `ac.dbg_is_decommitted_for(q)`.
3. `os::segment_base_of_ptr(q)` — это `q.map_addr(...)` (`alloc_core/platform/os.rs:148–150`): адрес базы при отсутствующем provenance.
4. `contains_base_ro(base)` (`segment_table/segment_table_impl.rs:764–766` → `canonical_base_of`, `:801–808`) сравнивает только адреса. Возвращённый сохранённый корень, у которого provenance есть, отбрасывается.
5. `SegmentHeader::kind_at(base)` (`segment_header/segment_header_views.rs:11–15`) → `Node::offset(base, off)` (`platform/node.rs:412–417`, `base.add`) и `Node::read_u8` (`:219–223`, `src.read()`). Это доступ ненулевого размера через указатель без provenance — UB по контракту `without_provenance_mut`. Далее `is_decommitted()` (`segment_header/segment_header_meta_fields.rs:84–87`) — ещё одно такое чтение.

**Наблюдено.**

- Сам крейт описывает этот шаблон как UB. Документация `SegmentTable::canonical_base_of` (`segment_table_impl.rs:776–790`): вызывающий, который выводит `base` из указателя `dbg_*`-аксессора и проверяет `contains_base_ro(base)`, доказал лишь совпадение адреса; «Reading allocator metadata through `base` itself after only an address-membership check is exactly that hazard». Предусловие `kind_at` — «read from the canonical table root» (`segment_header_views.rs:8`).
- Исправление R2-05 (`4601f890`, «read AllocCore/HeapCore dbg_* diagnostic metadata through the table's stored canonical pointer») перечисляет исправленные безопасные аксессоры (`header_diag`, `table_diag`, `alloc_core_small_diag`, `queries`) и утверждает: «Every safe dbg_* accessor with this pattern now reads through canonical_base_of's returned pointer». `dbg_is_decommitted_for` (тогда в `alloc_core_small_pool/mod.rs`) в перечне нет, и код до сих пор использует `contains_base_ro` + caller-derived `base`.
- Класс уже чинился по частям. Sol-ревью 2026-08-05 (`docs/reviews/2026-08-05-sol-remediation-readonly-review.md:173–180`) отметило пару `dbg_live_count_for`/`dbg_is_decommitted_for` и сочло адресную проверку достаточной. R2-05 эту посылку опроверг, но обоих не исправил. `dbg_live_count_for` перевели на `canonical_base_of` отдельным коммитом `872791de` (2026-09-29); `dbg_is_decommitted_for` снова остался. Комментарий `alloc_core_small_pool_impl.rs:707–711` до сих пор называет оба хука примером «validate via `contains_base_ro`» — для `dbg_live_count_for` это уже неверно.
- Тест того исправления, `tests/r6_route_lifecycle_diag_provenance.rs`, нет ни в `scripts/miri.mjs`, ни в `.github/` (grep). Нативно provenance не наблюдаем, поэтому код до и после исправления этот тест не различает (ВЫВЕДЕНО).
- После R2-05 только два места в `src/` сочетают `contains_base_ro`/`contains_base` с последующим разыменованием caller-derived базы: `decommit.rs:26` и `:99` (`grep -rn "contains_base_ro(" src/`). Все остальные безопасные хуки с параметром-указателем читают через `canonical_base_of`/`canonical_root_for`/`canonical_block_of` или через `segment_bases()` (полный перечень в §5).
- Miri-регрессия R2-05 `tests/regression_r2_05_diag_provenance_miri.rs` покрывает восемь аксессоров (:66–132). Её строка в `scripts/miri.mjs:56` собирается с `alloc-core internals` без `alloc-decommit`, так что `dbg_is_decommitted_for` там даже не компилируется. Ни в списке покрытых аксессоров, ни в списке «remaining accessors» модульной документации (:28–40) его нет.
- Сканер `scripts/verify-dbg-hook-safety.mjs:100` числит хук в `PURE_OBSERVERS` («read-only, no allocator-state mutation»). Этот критерий не учитывает provenance (см. R17-UNS-04).
- В дереве уже есть вызов с указателем, полученным int→ptr-кастом: `tests/regression_batch_flush.rs:385` передаёт `base as *mut u8`, где `base: usize` из `seg_base` (:90–92, `ptr as usize`). Корректность этого вызова держится только на exposure provenance в permissive-модели. Под `-Zmiri-strict-provenance` ошибкой станет уже сам каст. Тест в Miri-матрицу не входит (grep `scripts/miri.mjs`, `.github/workflows/ci.yml`).

**Выведено.** Из безопасного кода (без единого `unsafe` у вызывающего) достижимо неопределённое поведение. Тот же класс xa в round 2 оценил как P1 («Safe диагностическая функция может выполнить UB несмотря на успешный ownership guard»). Нативного падения не будет: страница отображена. Дефект — в допустимости доступа, его ловит Miri strict-provenance. P1, а не P0: требуется тестовый `internals`, `production` его не включает (`Cargo.toml:452–460`).

**Рекомендация.**

- В `dbg_is_decommitted_for` заменить :25–28 на `let Some(base) = self.table.canonical_base_of(os::segment_base_of_ptr(ptr)) else { return None };` и читать только через `base`. То же сделать в `dbg_force_decommit_retain_for`: :98–101 должны получать `base` из `canonical_base_of`.
- Исправить устаревший комментарий `alloc_core_small_pool_impl.rs:707–711`.
- Тест `regression_batch_flush.rs:385` перевести на реальный указатель блока, например `blocks[0]`, а не на int→ptr-каст.

**Oracle.**

- Тест `dbg_is_decommitted_for` с указателем без provenance (набросок — приложение B) под `-Zmiri-strict-provenance`, режимом Miri проекта по умолчанию: красный до исправления (UB при чтении в `kind_at`), зелёный после. Нужный набор фич уже есть в Miri-матрице и в CI: `alloc-core alloc-decommit internals` (`scripts/miri.mjs:62`, CI `miri-core`, `.github/workflows/ci.yml:2977`, цель `decommit_miri_cycle`). Строка `miri.mjs:56` с R2-05-тестом этих фич не включает.
- Тест надо подключить именно к CI-job `miri-core` (`ci.yml:2826`). Существующий `regression_r2_05_diag_provenance_miri` там отсутствует и запускается только локальным `npm run miri`/`harden` (`package.json:9, 25`), так что в CI регресс R2-05 сегодня не ловится (см. §4).
- Механически (см. R17-UNS-04): правило сканера «безопасный хук с параметром-указателем, читающий `SegmentMeta`/`SegmentHeader::*_at`, обязан пройти через `canonical_*`». Сегодня оно красное ровно на этом хуке.

### R17-UNS-02 — контракт `dbg_stamp_kind_byte` допускает типизированное чтение невалидного `SegmentKind`

**Места.**

- Контракт и тело: `src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs:109–173`. Хук явно предназначен для байтов «that are NOT one of the three legitimate `SegmentKind` discriminants» (:110–113). Разрешённые на время порчи действия перечислены на :144–163: «read-only `dbg_*` accessors that do not route … may run while the byte is corrupted» и «the segment is consumed ONLY by … `dbg_unregister` / `dbg_recycle`».
- Тип: `SegmentKind` — `#[repr(u8)]` с дискриминантами 0, 1, 2, 0xFF (`segment_header/segment_header_impl.rs:165–194`); поле `pub kind: SegmentKind` (:351) в `#[repr(C)] SegmentHeader`.
- Типизированное чтение: `SegmentHeader::read_at` (:744–749) → `Node::read_struct_with_atomic_word` → `out.assume_init()` (`platform/node.rs:317`).

**Трасса.**

- Названные в контракте поимённо (третий пункт, :160–163):
  - `dbg_unregister` (`table_diag.rs:308–312`) → `SegmentTable::unregister` → `SegmentHeader::read_at(base)` (`segment_table_impl.rs:510`);
  - `dbg_recycle` (`table_diag.rs:336–347`) → `SegmentTable::recycle` → `read_at` (`segment_table_impl.rs:640`).
- Подпадающий под первый пункт по смыслу (read-only, не маршрутизирует; назван без префикса `dbg_`): `large_geometry_for_test` (`header_diag.rs:27–43`) → `read_at` (:32).

**Наблюдено.** При байте `kind` вне {0,1,2,0xFF} — например 0x07, как в цикле `tests/kind_at_strict_decode.rs:117` — каждый из этих путей создаёт значение `SegmentHeader` с невалидным enum-тегом. Это немедленное UB. Текущие тесты так не делают: после порчи они вызывают только `dbg_kind_byte_of`/`dbg_kind_at_tag`, `dbg_contains_base` (без разыменования) и `dealloc` (`canonical_block_of`, затем `kind_at` → `Unknown` → no-op; типизированный `read_at` в `dealloc_at_base` есть только в ветке `Large`, `mem_impl.rs:261, 293`), а перед `Drop` байт восстанавливают (`kind_at_strict_decode.rs:117–152`, `182–222`). Живого нарушения нет.

**Выведено.**

- Контракт противоречит сам себе: следуя ему буквально, вызывающий получает UB внутри крейта.
- Обещание L-5/UBFIX-11 («corrupted kind byte is CONTAINED», `segment_header_impl.rs:178–193`, `segment_header_views.rs:16–39`) выполняется только на путях, которые читают `kind` побайтно через `kind_at`. Любой типизированный `read_at` повреждённого заголовка — `unregister`, `recycle`, `reclaim_large_segment` (`alloc_core_large.rs:736`) — становится UB раньше любой проверки.
- То же с `AllocCore::drop`: он безусловно копирует заголовок каждого зарегистрированного сегмента (`lifecycle.rs:515`). Вводная фраза контракта формально запрещает `Drop` до восстановления байта, но объясняет запрет неверной маршрутизацией. На деле при байте вне {0,1,2,0xFF} это немедленное UB, а не маршрутизация.

**Рекомендация.** Предпочтительно хранить `kind` в `SegmentHeader` как `u8` и декодировать его единственным `kind_at` (типизированная копия станет тотальной, а containment L-5 — настоящим и для `read_at`). Минимум — сузить контракт: разрешать только {0,1,2,0xFF}, либо поимённо перечислить допустимые аксессоры (`dbg_kind_byte_of`, `dbg_kind_at_tag`) и исключить `dbg_unregister`/`dbg_recycle`/«read-only accessors» в целом.

**Oracle.** Miri-тест (набросок в приложении B): записать 0x07, вызвать `large_geometry_for_test`, восстановить байт и освободить блок. До исправления — «constructing invalid value … enum tag»; после — `None` без UB. `large_geometry_for_test` выбран потому, что после него сегмент можно штатно освободить. `unregister` резервацию не освобождает (`segment_table_impl.rs:519`), поэтому вариант с `dbg_unregister` оставил бы утечку, которую Miri без `-Zmiri-ignore-leaks`, вероятно, отметил бы и после исправления.

### R17-UNS-03 — `# Safety` unsafe-хуков класса R2-05 не требуют provenance для метаданных

**Места.** Unsafe-хуки, выводящие базу метаданных из указателя вызывающего через `os::segment_base_of_ptr(ptr)` без `canonical_base_of`:

- `alloc_core/small/alloc_core_small_diag.rs`: `dbg_corrupt_freelist_head_next` (:148–165, `hardened`), `dbg_drain_freelist_batch` (:181–189), `dbg_alloc_bitmap_bytes_for` (:215–225), `dbg_magazine_bitmap_bytes_for` (:239–249), `dbg_payload_start_for` (:378–385);
- `alloc_core/small/alloc_core_small_pool/decommit.rs`: `dbg_force_decommit_retain_for` (:97–108);
- `registry/heap_core/diag/diag_probes.rs`: `dbg_clear_magazine_on_hit` (:376–386 → `alloc/hot.rs:43–58`, `segment_base_of_ptr(issued)` на :47).

Их контракты требуют «valid, live, exclusively-owned allocation pointer» (например :139–144, :174–178, :210–212) или «exact start pointer of a currently-live allocation» (`diag_probes.rs:351–367`). Требования к provenance или к тегу, покрывающему метаданные сегмента, нет.

**Наблюдено.** Коммит `4601f890` оставил эти хуки сознательно, с обоснованием «their existing "# Safety: ptr MUST be a valid, live allocation pointer" contracts already require correct provenance from the caller».

**Выведено.**

- В модели strict provenance (allocation-level) указатель, полученный от `alloc`, несёт provenance всей OS-резервации, и обоснование верно.
- Под Stacked Borrows указатель живой аллокации, полученный через reborrow (`Box::into_raw`, `slice::from_raw_parts_mut(..).as_mut_ptr()`), несёт тег только на байты блока. `map_addr` сохраняет этот тег, и чтение или запись битмапа, `BinTable` или заголовка за пределами блока — UB. Это тот же класс, что item 164, но в тестовой поверхности, и контракты об этом молчат.
- Текущие вызывающие передают сырые указатели от `alloc`/magazine, так что живого нарушения нет. Отсюда P4: контракт вводит в заблуждение.

**Рекомендация.** Перевести эти хуки на `canonical_base_of`/`canonical_block_of`. Это тестовая поверхность, цена не важна, и тогда provenance вызывающего перестаёт иметь значение. Иначе дописать в каждый `# Safety`: «`ptr` — сырой указатель, выданный этим аллокатором (не производный от ссылки или `Box`); его provenance должен покрывать метаданные сегмента».

**Oracle.** Miri SB-тест (набросок в приложении B): над блоком `AllocCore::alloc` сделать reborrow `slice::from_raw_parts_mut(p, 64).as_mut_ptr()` и передать сужённый указатель в `dbg_alloc_bitmap_bytes_for` того же `AllocCore`. До исправления — SB «tag does not exist in the borrow stack», после — зелёный. Под Tree Borrows доступ за пределы диапазона ссылки устроен иначе, поэтому oracle — именно SB-строка.

### R17-UNS-04 — устаревшие и противоречивые обоснования безопасности в таблице сканера хуков

**Места.**

- `scripts/verify-dbg-hook-safety.mjs:128–131` (`dbg_claim_lease`: «… takes no pointer and exposes no core access (core() is pub(crate)) …»).
- `:136–139` (`dbg_try_maintenance`: «… takes no pointer and exposes no core access»).
- `:141–142` (`dbg_with_core`: «the callback receives the exclusive &mut HeapCore …»).
- `:17` и `:100` (критерий `PURE_OBSERVERS` и запись `dbg_is_decommitted_for`).
- Код: `src/registry/heap_registry/claim.rs:512–518` (`#[doc(hidden)] pub fn core(&mut self) -> &mut HeapCore`), `:410–414` (`pub fn dbg_with_core`), `src/registry/mod.rs` (`#[cfg(feature = "internals")] pub use heap_registry::HeapLease`).

**Наблюдено.**

- С Ph4c (`5083e4bc`) `HeapLease::core` — `pub` под `internals`. В конфигурации, где существует `dbg_claim_lease`, безопасный внешний код получает `&mut HeapCore` routed-кучи реестра.
- `MaintenanceLease::dbg_with_core` выдаёт `&mut HeapCore` FREE-кучи. Таблица сама себе противоречит: запись `dbg_try_maintenance` говорит «no core access», а запись `dbg_with_core` строкой ниже описывает именно эту выдачу.
- `HeapLease::core` не попадает под шаблон имени `dbg_*`, поэтому сканер его вообще не видит: хуки обнаруживаются регулярным выражением `\bpub\s+(unsafe\s+)?fn\s+(dbg_\w+)\b` (`verify-dbg-hook-safety.mjs:375`). По той же причине вне поля зрения и тестовые аксессоры с суффиксом `_for_test` без префикса, например `large_geometry_for_test` (R17-UNS-02).
- Критерий `PURE_OBSERVERS` («read-only») пропустил R17-UNS-01.

**Выведено.** Сами хуки, по прочтению, корректны: эксклюзивность обеспечивают S1–S4 lease-дизайна (`claim.rs:486–518`), и `HeapLease` — `!Send`/`!Sync`. Но файл объявлен «ONE authoritative reviewed tables», а его обоснования фактически неверны. Будущий ревьюер оценит новый безопасный `pub fn` на `HeapCore` исходя из ложной посылки «core недоступен извне». Это прямое продолжение класса TRACKED_hook_safety (items 5/7/8/9: «scanner presence does not close the hook-safety residual class»). P4.

**Рекомендация.**

- Переписать обоснования: core доступен через `HeapLease::core`/`dbg_with_core`, безопасен по S1–S4.
- Добавить `HeapLease::core` в инвентарь сканера как реэкспортированный безопасный seam.
- Ужесточить критерий наблюдателя: хук с параметром-указателем, читающий метаданные, обязан пройти через `canonical_base_of`/`canonical_root_for`/`canonical_block_of`.

**Oracle.** Проверка в `verify-dbg-hook-safety.mjs`: для каждого наблюдателя с параметром `*mut`/`*const` тело обязано содержать один из `canonical_*`. Сегодня она красная на `decommit.rs::dbg_is_decommitted_for`, после R17-UNS-01 — зелёная. Тест на устаревшие формулировки: текст обоснования не утверждает «no core access», если в том же модуле есть `pub fn core`/`dbg_with_core`.

### R17-UNS-05 — единственный runtime-каст целого в указатель в `src/`: `bump_gen_on_issue(base as *mut u8, …)` в `HeapCore::alloc_batch`

**Места.**

- `src/registry/heap_core/alloc/batch.rs:169–188` — refill-хвост `HeapCore::alloc_batch` (`:71`, `cfg(all(feature = "fastbin", feature = "batch-api"))`):
  - `let base = os::segment_base_of_ptr(p) as usize;` (:172);
  - `let off = (p as usize) - base;` (:179);
  - `Self::bump_gen_on_issue(base as *mut u8, off);` (:184, `cfg(feature = "hardened")`).
  - `SAFETY` на :180–181: «`base` is a live, exclusively-owned segment; `off` is a MIN_BLOCK-aligned offset».
- Контракты получателя о provenance молчат: `bump_gen_on_issue` (`alloc/hot.rs:68–80`) и `bump_gen` (`alloc_core/segment/segment_header/segment_header_gen_table.rs:87–92`, «`base` MUST be a live, mapped, exclusively-owned segment …»).
- Для сравнения: магазинная ветка той же функции (:133–143) и refill-хвост `alloc/hot.rs:146–156` передают в тот же вызов указатель от `os::segment_base_of_ptr`, не целое.
- Публичный вход: `SeferAlloc::alloc_batch` (`global/sefer_alloc/batch.rs:65–76`, `pub unsafe fn`, `batch-api`), обе ветки (`Own` и `Fallback`) → `HeapCore::alloc_batch`.

**Наблюдено.**

- Перепись всех `as *mut `/`as *const ` в `src/` (§1): :184 — единственный каст целого в указатель, исполняемый в runtime.
- Политика крейта записана в `alloc_core/platform/os.rs:136–142`: старую идиому `segment_base_of(ptr as usize) as *mut u8` заменили на `map_addr`, потому что `ptr as usize` — exposed-address cast, «forbidden under `-Zmiri-strict-provenance`». На :172/:184 эта идиома осталась.
- `-Zmiri-strict-provenance` — режим Miri проекта по умолчанию (`scripts/miri.mjs:207`; CI `miri-core`, `ci.yml:2830`). Ни одна строка Miri в `scripts/miri.mjs` и в CI не включает `hardened` или `batch-api` (grep).

**Выведено.**

- UB сейчас нет. `as usize` на :172 (и на :179) раскрывает provenance `p`, а `as *mut u8` на :184 по семантике exposed provenance может её подобрать. Но законность держится на побочном эффекте соседней строки, а не на чём-то, что записано в `SAFETY` или в контракте `bump_gen`.
- Под `-Zmiri-strict-provenance` каст :184 — неподдерживаемая операция. Miri остановится на ней, а не проверит путь.
- Ловушка: механическая замена `as usize` на `.addr()` в :172/:179 (её предлагает H2, к ней же ведёт политика `os.rs:136–142`) уберёт exposure. Тогда законность :184 будет зависеть от случайного раскрытия той же provenance где-то ещё в программе; если его нет, это UB.
- P4: живого нарушения нет. Дефект — неполный `SAFETY` и расхождение с политикой provenance крейта.

**Рекомендация.** Держать указатель, как в магазинной ветке той же функции: `let seg = os::segment_base_of_ptr(p); let base = seg.addr();` … `Self::bump_gen_on_issue(seg, off)`. Кодоген не меняется, сравнение с `prev_base` остаётся целочисленным. Только после этого переводить :172/:179 на `.addr()` (H2).

**Oracle.**

- Типовая проверка: nightly-линт `fuzzy_provenance_casts` (`#![feature(strict_provenance_lints)]`) в nightly-строке clippy или Miri. Сегодня он должен сработать на :184 и на `TORN` (последнее снимается по H2), после исправления — молчать. Чисто лексическая проверка тип левого операнда не видит, поэтому годится только как allowlist-перепись из §1.
- Miri: строка `alloc-global batch-api hardened internals` с вызовом `SeferAlloc::alloc_batch` при пустом magazine (чтобы сработал refill-хвост) под `-Zmiri-strict-provenance`. До исправления — отказ Miri на int→ptr-касте :184, после — зелёный (набросок в приложении B).

### Кандидаты, сознательно не повышенные до находок

- **Подмена `HeapCore` через `mem::swap(lease_a.core(), lease_b.core())`** (обе lease одного потока, под `internals`). Значения `HeapCore` не самоссылочны: указатели ведут вовне — сегменты, primordial-массивы, System-память `RouteSlots`. Счётчики-хендлы `&'static` указывают на слоты, и после обмена они лишь «переезжают» к чужому слоту (диагностика). `owner` в дескрипторах маршрута используется только для сравнения с `OWNER_ID_FALLBACK` (`heap_core_xthread/routing.rs:35`). UB не найдено.
- **`RouteDirectory::register` — безопасный `pub` под `internals`.** Внешний код может зарегистрировать произвольный диапазон ключей в глобальном каталоге. Корни из записей не разыменовываются (единственный разыменовывающий путь — `unsafe fn owner_state_for_test`). Последствие — отказ последующей настоящей регистрации (`Duplicate` → OOM в тестовом процессе), не UB. Вынесено в §4.
- **`Node::deref` с caller-derived смещением в `AllocCore::canonical_block_of`** (`mem_impl.rs:34–42`). Внутренний указатель из Large-аллокации короче `SEGMENT` (`exact-span-large`) с тем же ключом окна дал бы `add` за пределы резервации. Достижимо только при нарушении контракта `dealloc`/`realloc` (unsafe fn); безопасные вызывающие передают указатели аллокатора. Это пункт поддерживаемости (§3, H3), не находка.

### Связь с индексами

Ни один открытый пункт не переоткрывается. Новые доводы к существующим:

- R17-UNS-01 — новое доказательство того, что закрытие R2-05 (xa round 2, коммит `4601f890`; отдельной карточки в индексах нет) неполно: класс уже дважды закрывали частично (R2-05, `872791de`). Предлагается завести `[A]`.
- R17-UNS-03 и R17-UNS-04 — новые конкретные примеры класса TRACKED_hook_safety (items 5/7/8/9). Предлагается `[T]` в `TRACKED_hook_safety.md`.
- R17-UNS-02 — к L-5/UBFIX-11 (карточки нет). Предлагается `[T]` в `TRACKED_misc.md` или `TRACKED_correctness_residuals.md`.
- R17-UNS-05 — карточки нет. Предлагается `[T]` в `TRACKED_correctness_residuals.md`.
- Miri-цели вне CI (§4) — новые экземпляры класса item 17 (Miri-цель существует, но ни один CI-job её не запускает).

Item 164 (P1-box), 166, 171 — без новых доводов.

## 3. Гипотезы оптимизации и поддерживаемости (не измерены, без GO)

Ни один пункт не измерялся. Это не speedup и не RSS/latency-результат.

- **H1 — полевые чтения вместо `read_at` в `SegmentTable::unregister`/`recycle`.** `unregister` копирует весь 144-байтный заголовок (`segment_table_impl.rs:510`: две `copy_nonoverlapping` плюс атомарная загрузка в `read_struct_with_atomic_word`), а нужны только `payload_offset` и `kind`. `recycle` (:640) нужны `reservation`, `reservation_len`, `segment_id`, `kind`. Полевые аксессоры (`segment_id_at`, `span_usable_at` и подобные) уже есть. Выгода в Ir на холодном пути мала. Главное — исчезает типизированное чтение `SegmentKind` из R17-UNS-02. Судья: iai-бенчи `large_alloc_free_cycle`/decommit-цикл, ±10 Ir kill gate.
- **H2 — гигиена strict provenance.**
  - `TORN` (`global/tls_heap.rs:125`): заменить `usize::MAX as *mut HeapCore` на `core::ptr::without_provenance_mut(usize::MAX)`, как уже сделано в `hash.rs:81` и `bootstrap.rs:264`. UB сейчас нет: константа вычисляется в CTFE и не разыменовывается.
  - Exposing-касты `ptr as usize` на горячих путях — например `free/dealloc_own_base.rs:276,287`, `alloc_core_small/dealloc.rs:42`, `alloc_core_small_magazine.rs:570`, `alloc_core_small_impl.rs:419,574,615`, `alloc/hot.rs:133,149`, `alloc/batch.rs:136,172,179` — заменить на `.addr()`. Тогда можно включить `fuzzy_provenance_casts`/`lossy_provenance_casts` (nightly-линты) в Miri/clippy-строку. Ожидаемый codegen идентичен; проверка — iai без изменения Ir.
  - Порядок обязателен: exposure на `alloc/batch.rs:172,179` сейчас несёт нагрузку — на ней держится законность int→ptr-каста :184. Сначала исправить R17-UNS-05, потом трогать эти две строки.
- **H3 — безопасные `pub(crate)`-мембраны с unsafe-предусловиями не перечислены в `src/lib.rs:289–317`** («soundness boundary is WIDER»). Речь о `HeapCore::dealloc_routing` (`heap_core_xthread/routing.rs:10–20`), `dealloc_own_thread_with_base` (`free/dealloc_own_base.rs:341–556`), `dealloc_own_thread` (`free/dealloc.rs:247–258`), `AllocCore::canonical_block_of` (`mem_impl.rs:34–42`, `Node::deref` с caller-derived смещением), `SegmentTable::unregister`/`recycle`. Их `SAFETY` у внутренних `unsafe {}` опирается на «reached only from unsafe fn …». Предложение: добавить в перечень `lib.rs` или сделать их `unsafe fn` (у всех вызывающих уже есть `unsafe` контекст или доказательство).
- **H4 — `unsafe impl Sync for HeapSlot` фактически передаёт `HeapCore` между потоками** (семантика `Send` через claim/lease), но статической проверки «не-указательные поля `HeapCore` — `Send`» нет: `HeapCore` `!Send` из-за сырых указателей (`heap_slot.rs:261–304`). Будущее thread-affine поле (`Rc`, guard стандартной синхронизации, TLS-хендл) было бы молча признано sound. Предложение: compile-time assert над обёрткой-зондом или поимённый список полей с обоснованием в комментарии к `unsafe impl`.

## 4. Вне темы (место + одна строка)

- `alloc_core/segment/segment_header/segment_header_gen_table.rs:61–64, 104–107` — релизные `assert!` в `gen_at`/`bump_gen` на путях `GlobalAlloc` под `hardened` (выдача блока → `bump_gen_on_issue`: `registry/heap_core/alloc/hot.rs:154, 382, 509`, `batch.rs:141, 184`). Сканер no-panic из закрытия item 174 охватывает 7 файлов и ищет только `.expect(`/`panic!`/`unreachable!` (`tests/no_panic_doc_accuracy.rs:214–221, 474–478`), так что эти `assert!` вне его поля зрения.
- `scripts/miri.mjs:56, 74, 108, 115` — Miri-цели `regression_r2_05_diag_provenance_miri`, `regression_realloc_oob_old_layout`, `regression_virgin_bitmap_skip`, `regression_w3_stats_aliasing_miri` запускаются только локальными `npm run miri`/`harden` (`package.json:9, 25`). В `.github/` их нет (grep); CI-job `miri-core` (`ci.yml:2826`) ведёт отдельный ручной список. Класс item 17.
- `registry/segment_route/registration.rs:49–81` — у `RouteRegistration::{begin_large_reuse, finish_large_reuse_after_reset, release_cached_large, cache_large_consumed}` нет вызывающих в `src/`, только `tests/r6_route_directory.rs` и `tests/r6_route_directory_model.rs`; cache-hit перерегистрирует свежий дескриптор, а `alloc_core_large.rs:251` вызывает одноимённый метод `SegmentMeta`. Вероятно, `pub` API под `internals`, который проверяется тестами, но в производственном пути не участвует.
- `global/tls_heap.rs:481–487` — комментарий называет инициализацию NUMA-топологии примером повторного входа в аллокатор посреди claim. Item 155 документирует стековый инициализатор numa-shim; возможно, пример устарел.
- `global/fallback.rs:336–357` — `with_heap` при повторном входе в том же потоке (например, аллокация из паники внутри замыкания на TORN-потоке) крутится на `LOCK` бесконечно. Не UB; самоблокировка.
- `registry/segment_route/directory.rs:789–865` — безопасный `pub fn register` (под `internals`) позволяет занять произвольный ключ глобального каталога; последствие — `Duplicate`/OOM при настоящей регистрации (DoS тестового процесса), не UB.

## 5. Что проверено и находок не дало

Чтобы следующий раунд не повторял работу:

1. **perf 84 (маска сегмента в overflow-flush и `flush_all_tcache`).** Все производители содержимого magazine хранят указатели с provenance аллокатора:
   - `dealloc_own_thread_with_base` кладёт `block` из `canonical_block_of` (`dealloc_own_base.rs:347, 420, 527`);
   - `dealloc_batch_small` кладёт `block` (`dealloc_batch.rs:264–345`);
   - refill пишет указатели, выведенные от `small_cur` или корня таблицы (`Node::deref(segment, off)`, `alloc_core_small_impl.rs:553, 581, 608`);
   - Large в magazine не попадает ни в одной конфигурации (`small_free_guard`: предмаршрут Ph3b либо ветви F7, `dealloc_own_base.rs:219–271`).

   Корни Small/Primordial выровнены на `SEGMENT`, поэтому маска даёт корень с исходным provenance. `debug_assert_eq!` сравнивает адреса. Корректно.
2. **`Drop for Segment` (R16).** Во всех местах передачи владения `mem::forget` стоит до `release_segment`: `lifecycle.rs:282`, `alloc_core_large.rs:648`, `reserve.rs:243`, `decomp_hooks.rs:76, 141`. `reserve_biased` работает с `vmem::Reservation`, не с `Segment`, и его ранние `?`-выходы освобождают резервацию до инкремента счётчика (`os.rs:184–217`). Двойного освобождения нет.
3. **`EpochRegion::drop` (R16).** `AtomicSlot::drop_value` обнуляет указатель до `into_owned()` (`hand.rs:526–541`): после пойманной паники повторного drop нет. Вторичные payload'ы забываются; во время внешнего unwind вложенная паника ловится `catch_unwind`, пока не вышла из деструктора.
4. **Геометрия biased Large.** `root_offset + useful <= raw.len()` и `committed <= useful` проверяются до commit (`os.rs:198–206`); release-токен и выравнивание `SEGMENT` совпадают с резервацией. NUMA-вариант вычисляет смещение через `checked_*` и откатывается `release_segment` (`numa.rs:148–186`). Регистрация по ключу payload, заголовок по корню; терминальные слова (`TERMINAL_WORDS_OFF` = 144…160) не пересекаются с payload (`hdr_aligned >= page`).
5. **Хеш-таблица и кеш владельца.** Хеш хранит числовые ключи (`without_provenance_mut`), корень берётся из `slots` (`hash.rs:80–87, 244–268`). `own_cache` хранит только корни из `hash_find`, а не ключи вызывающего (`segment_table_impl.rs:743–758`). Вытеснение идёт по корню (`:866–873`).
6. **`RouteDirectory`** (`directory.rs`):
   - счётные `EntryHandle`: `fetch_sub(AcqRel)`, последний освобождает sidecar и `Entry`;
   - pin создаётся только под shard-lock, пока запись связана (`:142–157, 868–889`); `RouteRegistration::drop` отвязывает запись до сброса своей ссылки (`registration.rs:78–82`);
   - `&mut Block` — только под lock, split/absorb над разными аллокациями; освобождённый блок после `recycle` не используется;
   - `root` из записи не разыменовывается вне `unsafe fn owner_state_for_test`;
   - `ShardLock`: границы `Send`/`Sync` и маркер `PhantomData<&mut T>` верны.
7. **`SmallSidecar`/`ClassLeaves`.** Инициализация через `addr_of_mut!` без ссылок на неинициализированное. Mixed-лист публикуется Release после заполнения и освобождается только в `Drop` при последнем pin. Производитель трогает только sidecar.
8. **`RouteSlots`** (`route_slots.rs`). Массив `Option<RouteRegistration>` инициализирован до чтения, перенос через `read`/`write` с dealloc по точному layout; ссылки `&*self.slots.add(i)` привязаны к `&self`, `RouteScan` удерживает `&SegmentTable`, что исключает `remove`/`ensure_capacity` во время скана.
9. **`static mut FALLBACK`** (`global/fallback.rs`). Доступ только через `addr_of_mut!`, ссылок на `static mut` нет. `&mut HeapCore` выдаётся только победителю INITIALIZING или под `LOCK`; `try_with_heap` читает READY с Acquire.
10. **TLS** (`global/tls_heap.rs`).
    - TORN-логика и однопроверочный диапазон корректны.
    - Рассуждение S3 под SB: сырой указатель в `LOCAL` выведен из `lease.core()`. Другие `&mut` от ячейки того же слота создаются только до публикации (`claim.rs:317–335`) или после TORN (`AbandonGuard::drop`), так что тег, лежащий в `LOCAL`, не инвалидируется, пока используется.
11. **Реестр** (`heap_slot.rs`, `claim.rs`, `bootstrap/registry.rs`, `ensure.rs`). `get_unchecked` с индексом `% CHUNK_SLOTS`; индекс чанка проверяется на границы; обнулённый чанк — валидный `RegistryChunk`; пара Release/Acquire на `initialised`; счётчики W3 лежат вне байтового диапазона `HeapCore` (`HeapSlotRemote`), что закрывает чтение под защищённым `&mut`.
12. **`Node::read_struct_with_atomic_word`.** Копии обходят атомарное слово; padding и неинициализированные байты переносит `copy_nonoverlapping`; `assume_init` корректен при валидных полях (ограничение — R17-UNS-02).
13. **Представления `&'static Atomic*`** (`node.rs:430–579`). Разделяемые ссылки на `!Freeze`-типы rustc не помечает `dereferenceable`. На всех прослеженных путях ссылка после `release_segment` не используется: `dealloc_at_base` Large, `reclaim_large_segment`, `AllocCore::drop`.
14. **Sidecar** (`platform/sidecar.rs`, `large_cache_extended.rs`). Во всех вызовах `deref`/`deref_mut` владельцем передаётся `&*self` — весь core (`directory.rs:92, 127, 149`; `alloc_core_large_cache.rs:108, 159, 306, 356, 393`; `lifecycle.rs:473`; `directory_diag.rs:407`). Поэтому сброс токена (`release_directory_for_cold_trim(&mut self)`) не может пересечься с живой ссылкой. Чтения каталога при скане — по значению (`os.rs:709–778`, `find_segment.rs:319–365`).
15. **Битмапы, `BinTable`, `ActiveKindIndex`.** `SegmentBitmap::locate` проверяет индекс только через `debug_assert`, но все вызывающие вычисляют `off` от корня или маски `SEGMENT`-выровненного сегмента либо заранее отсекают `off >= bump` (`dealloc_small`, `flush_run`, `reclaim_sidecar_record`, `small_free_guard`). `BinTable::head`/`set_head` проверяют класс в release; `ActiveKindIndex` проверяет слот и aborts.
16. **`GlobalAlloc` и `HeapCore`/`AllocCore` free/realloc.** Методы только диспетчеризуют. `dealloc`/`realloc` разрешают корень через таблицу (`canonical_block_of`) и используют указатель пользователя лишь как ключ; move-leg читает блок, выведенный от корня, с ограничением `safe_payload_read_span`. Внешняя ветка `realloc` читает через указатель пользователя под контрактом `GlobalAlloc`. Ограничение aliasing-модели для intrusive free-list — принятый item 164, не переоткрывается.
17. **Безопасные хуки с параметром-указателем** проверены поимённо, все читают через корень: `header_diag` (6 + `large_geometry_for_test`, `terminal_header_snapshot_for_test`), `table_diag` (`dbg_node_id_for`, `dbg_page_map_class_for`, `dbg_segment_id_of`, `dbg_contains_base` — без разыменования), `alloc_core_small_diag` (`dbg_live_count_for`, `dbg_freelist_head_for`, `dbg_is_free_for`, `dbg_committed_payload_end_for`), `queries.rs` (`dbg_owner_id_for`, `dbg_directory_bit_for_ptr` и делегаты), `HeapCore::dbg_contains_base` (кеш заполняется корнем). Единственное исключение — R17-UNS-01.
18. **Bootstrap и reserve.** Под Miri (System не обнуляет) явно инициализируется каждая метаданная область, которую читают до записи: битмапы, хеш, free-list, gen-table. Заголовок пишется целиком до первого `read_at`. Под реальной ОС чтение свежих нулевых страниц даёт валидные значения.
19. **`AllocCore::drop`.** `close_routes` идёт первым; primordial освобождается последним, после обхода реестра, который живёт в нём; Large перед unmap проходит `claim_live`/`release_consumed`.
20. **Экспериментальный и прототипный слои.** `AtomicSlot<T: Send + Sync>`; `try_evict_at` проверяет null `old`. `exact_object`: `UnsafeCell<Inner>` под спинлоком, индексы проб маскируются, загрузка ≤ 3/4; дескриптор отвязывается до `System.dealloc`.

## Приложение A. Инструменты и команды (только чтение)

- Перепись: `git ls-files src > files.txt`; затем `xargs awk '{l=$0; sub(/\r$/,"",l); t=l; gsub(/^[ \t]+|[ \t]+$/,"",t); phys++; if(t=="")blank++; else if(substr(t,1,2)=="//")cmt++; else code++} END{...}' < files.txt` → `phys=43325 code_nonblank_not_slashslash=17059 slashslash=24303 blank=1963`. По группам: `grep "^src/<group>/" files.txt | xargs cat | wc -l`.
- Unsafe-инвентарь: команда CLAUDE.md (выше); разделение по tier — `#![` против `#[`; синтаксис: `xargs grep -nE '^[^/]*\bunsafe\s*\{'` (219), `'^[^/]*\bunsafe\s+fn\b'` (68), `'^[^/]*\bunsafe\s+impl\b'` (9).
- Провенанс: `git ls-files src | xargs grep -nE 'as \*(mut|const) '` (86 строк вне комментариев; каждая классифицирована по типу левого операнда), `grep -nE '\bas _\b'` (0 попаданий), `grep -rnE 'without_provenance|map_addr|with_addr|with_exposed_provenance|expose_provenance|transmute' src/`, `grep -rnE 'null(_mut)?(::<[^>]+>)?\(\)\s*\.\s*(wrapping_)?(add|offset|byte_add)' src/` (0), `grep -rn "contains_base_ro(" src/`.
- Ссылки из сырых указателей: `grep -rnE '&mut \*|&\*[a-z_(]|\.as_ref\(\)|\.as_mut\(\)|get_unchecked' src/`.
- Хуки: `grep -rnE 'pub (unsafe )?fn dbg_\w+\([^)]*(\*mut|\*const)' src/` и чтение каждого тела; сверка с таблицами `scripts/verify-dbg-hook-safety.mjs` (`PURE_OBSERVERS` :17, `SAFE_MUTATORS` :127, `UNSAFE_HOOKS` :272, регулярное выражение обнаружения :375).
- Miri-покрытие: `grep -nE "^\s*\['" scripts/miri.mjs` (матрица), `grep -nE "miri test" .github/workflows/ci.yml` и списки `--test` в job `miri-core`, `grep -rn "regression_r2_05_diag_provenance|r6_route_lifecycle_diag_provenance|regression_batch_flush" .github/ scripts/miri.mjs`.
- История: `git show 4601f890` (исправление R2-05), `git log -S"fn dbg_live_count_for" -- src`, `git log -S"canonical_base_of(key)" -- src/alloc_core/small/alloc_core_small_diag.rs` (→ `872791de`), `git log --oneline -S"dbg_is_decommitted_for" -- src/`, `git log -1 5083e4bc`, `git diff --stat 6a0d47f6 HEAD -- src`, `git show --stat 59159a02`.

## Приложение B. Witness и мутанты

**Не запускалось: режим только чтение.** Ниже — неисполненные наброски oracle для исполнителя исправлений. Их результат не наблюдался.

```text
// R17-UNS-01: набор фич `alloc-core alloc-decommit internals` (как у
// decommit_miri_cycle: scripts/miri.mjs:62, CI miri-core) под
// -Zmiri-strict-provenance; подключить к CI-job miri-core.
#[cfg(feature = "alloc-decommit")]
#[test]
fn dbg_is_decommitted_for_sound_under_provenance_less_input() {
    let layout = Layout::from_size_align(64, 8).unwrap();
    let mut core = AllocCore::new().unwrap();
    let p = core.alloc(layout);
    let stale = core::ptr::without_provenance_mut::<u8>(p.addr());
    assert_eq!(core.dbg_is_decommitted_for(p), core.dbg_is_decommitted_for(stale));
    unsafe { core.dealloc(p, layout) };
}
// Ожидание: до исправления — Miri UB (доступ через указатель без provenance в
// SegmentHeader::kind_at); после (`canonical_base_of`) — зелёный.

// R17-UNS-02: Miri.
let layout = Layout::from_size_align(2 << 20, 8).unwrap();
let large = ac.alloc(layout);
unsafe { ac.dbg_stamp_kind_byte(large, 0x07) };    // разрешено текущим контрактом
let _ = ac.large_geometry_for_test(large);         // «read-only, non-routing»
unsafe { ac.dbg_stamp_kind_byte(large, 2) };
unsafe { ac.dealloc(large, layout) };
// Ожидание: до — "constructing invalid value ... enum tag" в read_at;
// после (`kind: u8`) — None без UB.

// R17-UNS-03: Miri, Stacked Borrows.
let p = core.alloc(Layout::from_size_align(64, 16).unwrap());
let narrowed = unsafe { core::slice::from_raw_parts_mut(p, 64) }.as_mut_ptr();
let mut buf = [0u8; 8];
unsafe { core.dbg_alloc_bitmap_bytes_for(narrowed, &mut buf) };
// Ожидание: до — SB "tag does not exist in the borrow stack"; после перевода
// хука на canonical_base_of — зелёный.

// R17-UNS-05: Miri, `alloc-global batch-api hardened internals`,
// -Zmiri-strict-provenance. Сигнатуры — по global/sefer_alloc/batch.rs:66, 115.
let a = SeferAlloc::new();
let layout = Layout::from_size_align(64, 8).unwrap();
let mut out = [core::ptr::null_mut::<u8>(); 8];
let n = unsafe { a.alloc_batch(layout, &mut out) };  // пустой magazine -> refill-хвост
unsafe { a.dealloc_batch(layout, &out[..n]) };
// Ожидание: до — Miri отказывает на int->ptr касте batch.rs:184 (strict
// provenance); после (`seg`-указатель вместо `base as *mut u8`) — зелёный.
```
