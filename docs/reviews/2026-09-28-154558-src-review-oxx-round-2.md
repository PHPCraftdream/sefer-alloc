# Независимое ревью `src/`, раунд 2 (oxx)

Дата: 2026-09-28 15:45:58 CEST. База: `main` @ `f6e40fbc2520b61dc80c8b21b8ecd9add3441e85`
(worktree `src-review-oxx-round-2`). Исходный код `src/` в ходе ревью не изменялся;
этот файл — единственный артефакт коммита.

**Об идентификаторах.** `R2-NN` в этом отчёте — второй раунд серии **oxx**
(продолжение `docs/reviews/2026-09-28-005939-src-review-oxx-round-1.md`, R1-01…R1-11).
Их не следует путать с `R2-01…R2-24` из `docs/reviews/2026-09-22-120730-src-review-xa-round-2.md`,
на которые уже ссылаются комментарии в `src/` (например, «R2-09», «R2-12», «R2-22»). Ниже
ссылки на ту серию всегда помечены «xa R2-NN»; при цитировании находок этого отчёта в коде и
коммитах рекомендуется писать «oxx R2-NN».

## 1. Вердикт и границы

**Вердикт.** Для корректного клиента `GlobalAlloc` в `production` на 64-битных
Windows/Linux/macOS в этом проходе **новых P0–P2 не найдено** (UB, потеря free, повреждение
памяти). Исправления раунда 1 (R1-01, R1-03…R1-06, R1-11) перепроверены по коду и выглядят
корректными (§4). Найдены **1 подтверждённая P3** — пустой бывший `small_cur` становится
«сиротой»: память остаётся закоммиченной вне лимита пула, и её не освобождают ни
`trim_current_thread()`, ни trim при выходе потока — и **6 P4**: пробел defence-in-depth в
ограничителе чтения `realloc` при lazy-commit (в `production`), привязка слота реестра в
`dealloc_batch`, устаревшая документация счётчика потерь в `AllocStats`, закреплённая
тестом-растяжкой, неучтённые отказы резервации small-сегмента при lazy-commit, опасная
«защитная» ветка `SegmentTable::recycle`, расхождения документации с кодом (включая ложное
«`AllocCore` is `Send`»). Три находки (R2-01, R2-02, R2-03) и одна часть R2-07 подтверждены
**динамически** (§1.2). По открытым пунктам 145 и 146 добавлены новые сведения (§3.8).

### 1.1 Что прочитано

- **Построчно, вместе с комментариями и `// SAFETY:`:**
  `src/alloc_core/small/alloc_core_small/{reserve.rs, directory.rs}`,
  `src/alloc_core/small/alloc_core_small_pool/{alloc_core_small_pool_impl.rs, segment_state_reconciliation.rs}`,
  `src/alloc_core/large/{alloc_core_large.rs, large_cache_extended.rs}`,
  `src/alloc_core/alloc_core/{lifecycle.rs (Drop), bootstrap.rs, mem/{mem_impl.rs, realloc_fastpath.rs}}`,
  `src/alloc_core/platform/{numa.rs, os.rs (конструкторы резервации, счётчики), sidecar_stats.rs}`,
  `src/alloc_core/segment/segment_table/{hash.rs, segment_table_impl.rs (recycle)}`,
  `src/alloc_core/segment/segment_directory/segment_directory_impl.rs` (битовые операции, `rebuild_from_table`),
  `src/alloc_core/segment/segment_header/{segment_header_views.rs, segment_header_impl.rs (read_at)}`,
  `src/alloc_core/segment/bitmap/alloc_bitmap.rs`, `src/alloc_core/small/reserved_small_segment.rs`,
  `src/registry/heap_registry/{claim.rs, stack.rs}`, `src/registry/heap_slot.rs` (состояния/CAS),
  `src/registry/heap_core/{core.rs (retry-константы и legacy-счётчики), state/ownership.rs, free/realloc.rs}`,
  `src/registry/heap_core_xthread/{overflow.rs (dirty-бит), drain.rs}`,
  `src/registry/heap_overflow/{heap_overflow_impl.rs, push.rs, spill.rs}`, `src/registry/bootstrap/overflow_sidecar.rs`,
  `src/global/{alloc_stats.rs, fallback.rs, tls_heap.rs (teardown)}`,
  `src/global/sefer_alloc/{batch.rs, global_alloc.rs, diag.rs, core.rs}`, `src/concurrent/epoch/hand.rs`.
- **Проверка исправлений раунда 1:** `src/registry/heap_core/free/{dealloc_own_base.rs, dealloc_batch.rs}` (R1-01),
  `src/alloc_core/small/alloc_core_small/find_segment.rs` (R1-03), `lifecycle.rs` (R1-04),
  `src/alloc_core/segment/remote_free_ring/ops.rs` и `heap_overflow/push.rs` (R1-05),
  `src/registry/xthread_fallback_gate.rs` и `fallback.rs` (R1-06), `claim.rs` и `heap_overflow/` (R1-11).
- **Только поиском (grep по unsafe / сигнатурам с сырыми указателями / паникам / атомикам / cfg):**
  `src/alloc_core/alloc_core/alloc_core_core_diag/*`, `src/registry/heap_core/diag/*`,
  `src/alloc_core/small/{alloc_core_small_diag.rs, alloc_core_small_pool/{decomp_hooks,decommit}.rs}`,
  `src/alloc_core/segment/segment_header/{descriptors,segment_header_layout,layout_asserts,segment_header_gen_table,segment_header_meta_fields}.rs`,
  `src/alloc_core/platform/node.rs` (тела `read_struct*` и атомарных видов прочитаны), `src/registry/bootstrap/{mod,loom_shim}.rs`,
  `src/concurrent/{lock_free,sharded,pinning}*`, `src/kani_proofs.rs`; все `mod.rs` — на наличие кода,
  отличного от объявлений (нарушений нет).
- **Не покрыто:** реализации crates из `crates/` (только точечные справки о контрактах
  `aligned-vmem`/`once-ptr-cell`); кодогенерация; weak-memory интерлейвинги на ARM;
  `numa-aware` на реальном многоузловом железе; `hardened` gen-table пути — только поиском.
- Прошлые отчёты (`docs/reviews/*src-review*`), индексы open-items и `CHANGELOG.md` использованы
  **только** для дедупликации (пометки «уже известно»), не как доказательство.

### 1.2 Что запускалось

Тесты/clippy/miri/loom проекта **не запускались**. Для подтверждения гипотез собраны
**временные** probe-крейты внутри worktree (`tmp/`, покрыт `.gitignore`; свой `[workspace]`,
свой `CARGO_TARGET_DIR`, `-j 2`, `--offline` с `Cargo.lock` соответствующей ревизии); после
прогона они **удалены**, `git status` чист. Toolchain: rustc/cargo 1.97.0, Windows 10 x86_64.

1. `tmp/probe_oxx_r2`: `sefer-alloc` с `default-features = false, features = ["std","production","internals","bench-internals"]`
   (для п. «batch» добавлены `experimental`, `batch-api`), dev-профиль с `opt-level = 1`.
   - **R2-01, standalone `AllocCore`:** 60 × 200 000 B, освобождение всех, 1 × 250 000 B:
     `after free all: prim=1 active=1 pooled=2 orphan=0` →
     `after alloc c2 x1: prim=1 active=1 pooled=2 orphan=1 (orphan_committed=4194304)` →
     после `dbg_drain_small_pool`: `pooled=0 orphan=1 (orphan_committed=4194304)`, `drained=2 released_delta=2`.
     Смена семи классов (140 000…250 000 B, по одному alloc/free) после прогрева:
     `accumulate: prim=1 active=1 pooled=0 orphan=3 (orphan_committed=12582912)`.
   - **R2-01, production-путь `HeapCore`** (`HeapRegistry::claim`, магазин, trim-последовательность
     `dbg_flush_all` + `dbg_drain_small_pool`): `K=19 total_blocks=57`;
     `hc after free all + trim: prim=1 active=1 pooled=0 orphan=0` →
     `hc after alloc c2 x1: ... orphan=1 (orphan_committed=4194304)` →
     `hc after second trim: ... orphan=1 (orphan_committed=4194304)`.
   - **R2-02:** блок 16 B в primordial standalone `AllocCore`:
     `off=192512 committed_end=454656 guard_bound=4001792 committed_from_p=262144 kind=0`;
     затем `realloc(p, Layout(327680, 8), 327696)` (отдельный запуск одного `#[ignore]`-теста):
     напечатано `bogus=327680 committed_from_p=262144`, процесс убит нарушением доступа
     (Git Bash: `Segmentation fault`, код 139); `realloc` не вернулся.
   - **R2-03:** статический экземпляр `SeferAlloc` (не `#[global_allocator]`): поток-владелец
     выделяет 8 × 64 B и остаётся жив; другой поток делает 4 скалярных `dealloc`, третий —
     `dealloc_batch` из 4: `count: start=1 after_scalar_dealloc_thread=1 after_dealloc_batch_thread=2`.
   - **R2-07:** `fn assert_send<T: Send>() {}; assert_send::<AllocCore>();` → `error[E0277]`
     («`*mut u8` cannot be sent between threads safely», «within `AllocCore`»; то же для
     `*mut SegmentDirectory`).
   - **Пункт 145:** 20 раундов «claim в тестовом потоке → spawn → claim → recycle»: `collisions=0`.
2. `tmp/probe_145_head` (HEAD) и `tmp/probe_145_old` (исходники `6658d8e8` — коммита, заведшего
   пункт 145, — извлечены `git archive` в `tmp/`), `features = ["std","production","internals"]`
   (= строка `production internals`), dev-профиль по умолчанию: копия
   `tests/regression_xthread_large_free_layout_mismatch.rs` без crate-level `#![cfg]`, с печатью
   id/состояния слота и того, совпал ли **первый** claim удалённого потока с владельцем. Для
   каждой ревизии: 5 прогонов × 5 тестов (параллельный libtest) + 3 прогона с `--test-threads=1` →
   **0 совпадений**; состояние слота владельца в момент удалённого claim — `LIVE`.

Всё остальное в отчёте — статический вывод из кода с указанием строк.

## 2. Сводка

| Приоритет | Подтверждено | Номера |
|---|---|---|
| P0 | 0 | — |
| P1 | 0 | — |
| P2 | 0 | — |
| P3 | 1 | R2-01 |
| P4 | 6 | R2-02 … R2-07 |
| **Итого** | **7** | гипотезы §5 и сведения по открытым пунктам §3.8 в счёт не входят |

| ID | Приоритет | Кратко | Подтверждение |
|---|---|---|---|
| R2-01 | P3 | Пустой бывший `small_cur` после смены курсора не попадает ни в pool, ни в release: «сирота» (до 4 MiB commit каждый) вне `pool_segments`, не освобождается trim | динамически (standalone + `HeapCore`) |
| R2-02 | P4 | `safe_payload_read_span` считает Small/Primordial полностью закоммиченными; при `primordial-lazy-commit` (production) фиктивный `old_layout` проходит guard и читает за commit-frontier → access violation | динамически (числа + падение) |
| R2-03 | P4 | `dealloc_batch` резолвит кучу через alloc-путь `current_heap()`: поток, только освобождающий память, занимает и материализует слот реестра (или берёт fallback-spinlock) | динамически |
| R2-04 | P4 | Rustdoc `AllocStats::ring_overflows` велит алертить по `cross_thread_frees_lost`, который после spill (xa R2-09) всегда 0; ссылка на несуществующий тест; устаревшая формулировка закреплена тестом-растяжкой | статически |
| R2-05 | P4 | При `small-segment-lazy-commit` без `numa-aware` отказ ОС в резервации small-сегмента не учитывается в `SEGMENTS_RESERVE_FAILED_TOTAL` | статически |
| R2-06 | P4 | «Защитная» ветка `SegmentTable::recycle` вопреки doc («no-op») освобождает резервацию по непроверенному заголовку и оставляет висячий слот | статически (+ признано в тесте) |
| R2-07 | P4 | Расхождения документации/аргументов безопасности с кодом (`AllocCore: Send`, «forbid», «нет unsafe», «safe pub fn», «panics (debug only)» и др.) | статически + компиляция |

## 3. Подтверждённые находки

### R2-01 — P3 — пустой `small_cur`, брошенный при смене курсора, становится «сиротой»: не попадает в pool/release, не освобождается `trim_current_thread()` и trim при выходе потока, не ограничен `pool_segments`

- **Место:** `src/alloc_core/small/alloc_core_small/reserve.rs:35-39` (`reserve_small_segment` публикует
  `self.small_cur = base`; предыдущий курсор ничем не финализируется);
  `src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:91-99`
  (`dec_live_and_maybe_decommit`: `if live != 0 || base == small_cur || meta.is_decommitted() { return false; }`);
  единственная процедура, финализирующая такие сегменты, — `finalize_orphaned_empty_segments`
  (`alloc_core_small_pool_impl.rs:365-393`), вызывается только из
  `src/registry/heap_core_xthread/drain.rs:389-391` (переполнение dedup-буфера `drain_heap_overflow`);
  `trim_for_recycle` (`src/registry/heap_core/state/ownership.rs:256-269`) = `flush_all_tcache` +
  `drain_small_pool` + `evict_all`. Обещания: `src/global/sefer_alloc/diag.rs:133-140`
  («wants its retained memory released to the OS now»); `segment_state_reconciliation.rs:43-46`
  (orphan — «transitional» состояние); `docs/perf/R29_4_SEGMENT_STATE_RECONCILIATION_GATE.md:209-217`
  («A segment at `live_count == 0` that is NOT `small_cur` is ALWAYS either pooled or recycled…
  The ONLY way a `small_empty_orphan` can exist is … drain-buffer overflow»).
- **Механизм:** small-сегменты — общие bump-цели для всех классов. Если все блоки текущего
  `small_cur` освобождены, пока он ещё курсор, `dec_live_and_maybe_decommit` его (правильно в тот
  момент) пропускает. Когда следующий запрос относится к классу, для которого нет ни свободного
  блока ни в одном сегменте, ни места в хвосте `small_cur`, `reserve_small_segment` делает курсором
  новый сегмент, а старый остаётся зарегистрированным, закоммиченным, с `live_count == 0`,
  вне пула и не decommitted. Переход 0 → «pool/release» для него больше никогда не наблюдается;
  выйти из этого состояния он может только если позднее будет запрошен один из классов, свободные
  блоки которых в нём лежат (`find_segment_with_free` поднимет `live_count`, и следующий переход в 0
  отработает штатно).
- **Сценарий:** смена профиля размеров по фазам (пакетная обработка: фаза наполняет сегмент
  классом A и всё освобождает, следующая фаза работает классом B). Динамически (§1.2 п.1): одна
  смена класса даёт `orphan=1 (orphan_committed=4194304)`, который переживает `drain_small_pool`
  и повторный trim на production-пути `HeapCore`; семь смен классов — `orphan=3 (12582912 B)`.
- **Последствие:** удержание commit/RSS до 4 MiB на каждого «сироту» (на Windows — commit charge
  всего сегмента; на Unix — фактически тронутые страницы) **вне** лимита `pool_segments`
  (по умолчанию 4) и вне досягаемости `drain_small_pool`, `maybe_decay_small_pool`,
  `trim_current_thread()` и trim при выходе потока; при переиспользовании слота «сироты»
  наследуются следующим владельцем кучи. Верхняя граница — только число таких фаз, классы которых
  больше не запрашиваются (структурный предел — `MAX_SEGMENTS = 4096` на кучу). Не UB и не потеря
  free. Заодно неверен вывод R29-4 §3 о структурной пустоте `small_empty_orphan`.
- **Исправление:** финализировать старый курсор в точке смены: в `reserve_small_segment`
  (`reserve.rs:35-39`) после успешного `reserve_small_segment_impl` проверить предыдущий
  `small_cur` — `kind == Small`, `live_count == 0`, `!is_decommitted`, не в пуле (тот же предикат, что
  в `finalize_orphaned_empty_segments`) — и вызвать `release_or_pool_empty_segment(old)`. Это O(1) на
  холодном пути, а все три производственных вызывающих (`alloc_small`, `alloc_small_with_virgin`,
  `refill_class_bump_impl`) идут через эту обёртку. Безопасность та же, что у sweep-а:
  `live_count == 0` исключает блоки в магазине (D1) и ожидающие remote-заметки. Дополнительно
  (опционально) — позволить `trim_for_recycle` отпускать ПУСТОЙ `small_cur` со сбросом курсора.
  Обновить R29-4 §3.
- **Контрфактический тест:** `production internals bench-internals`, свой `HeapCore`
  (`HeapRegistry::claim`): заполнить текущий `small_cur` K блоками 200 000 B (K определить по смене
  базы), освободить всё, `dbg_flush_all()`, `dbg_drain_small_pool()`, выделить один блок 250 000 B →
  утверждать `dbg_segment_state_reconciliation().small_empty_orphan.count == 0` и что старый сегмент
  стал `small_pooled` (или `segments_released_total` вырос на 1). Сейчас красный (`orphan=1`,
  `committed_bytes = 4194304`), после фикса — зелёный. Та же проверка на standalone `AllocCore`.
- **Уже известно?** Нет. В индексах open-items отсутствует; R1-03 (pool как цель carve) — другой
  механизм; R29-4 утверждает обратное.

### R2-02 — P4 — `safe_payload_read_span` считает Small/Primordial «полностью закоммиченными», что ложно при `primordial-lazy-commit` (в `production`) и `small-segment-lazy-commit`: guard пропускает фиктивный `old_layout`, и копирование читает за commit-frontier

- **Место:** `src/alloc_core/alloc_core/mem/realloc_fastpath.rs:36-43` (doc: «the segment is exactly one
  `SEGMENT` (4 MiB), fully committed on reserve», «without faulting»), `:53-64` (для Small/Primordial
  граница = `os::SEGMENT - off`); потребители guard-а: `src/alloc_core/alloc_core/mem/mem_impl.rs:558`,
  `src/registry/heap_core/free/realloc.rs:290, :405, :552`. Lazy commit: primordial —
  `src/alloc_core/alloc_core/bootstrap.rs:107-118` (`Segment::reserve_lazy`) и `:371-387`
  (штамп `committed_payload_end`); обычные small под `small-segment-lazy-commit` на Windows —
  `src/alloc_core/small/alloc_core_small/reserve.rs:361-370`.
- **Механизм:** цель guard-а (R2-1-эпохи, `realloc_fastpath.rs:26-31`) — «a caller bug that passes a
  bogus `old_layout.size()` must not turn into an out-of-bounds read». Но на Windows (где lazy commit
  реален) у primordial закоммичены только метаданные, первый чанк и то, что уже выросло через
  grow-on-carve; страницы выше `committed_payload_end` — только `MEM_RESERVE`. Граница
  `SEGMENT - off` их не исключает.
- **Сценарий (динамически, §1.2 п.1):** production-фичи, блок 16 B в primordial:
  `off=192512`, `committed_end=454656`, guard пропускает до `4001792` байт, закоммичено от `p` —
  `262144`. `AllocCore::realloc(p, Layout(327680, 8), 327696)` проходит guard, и процесс падает с
  нарушением доступа (по построению — на `Node::copy_nonoverlapping`, единственном чтении за
  фронтиром) вместо возврата null.
- **Последствие:** затрагивает только вызывающих, уже нарушивших контракт `unsafe fn realloc`;
  корректные клиенты не страдают. Но заявленная гарантия defence-in-depth в `production`
  не выполняется, а п. 9 §4 отчёта раунда 1 («move-ветки ограничены `safe_payload_read_span`»)
  был проверен неполно (без учёта lazy-commit).
- **Исправление:** для own-segment ветвей ограничивать Small/Primordial через
  `SegmentMeta::committed_payload_end_of()` (поле есть во всех раскладках; при eager-пути равно
  `SEGMENT`) или, строже, через bump-фронтир carve. Для foreign-ветви (`realloc.rs:405`) фронтир
  пишет другой поток plain-store-ами — там нужен атомарный вид поля либо явная документация, что
  эта ветвь опирается только на контракт `old_layout`. Исправить doc (`:36-43`).
- **Контрфактический тест:** `production internals bench-internals`, `cfg(windows)`: через
  bench-internals-аксессор сравнить `safe_payload_read_span(base, p)` с `committed_end - off` для блока
  в primordial (сейчас `4001792 > 262144`); после фикса — поведенческий тест
  `realloc(p, bogus) == null` с неизменным `p` (до фикса он роняет процесс — демонстрация выше).
- **Уже известно?** Нет (в индексах нет; раунд 1 счёл цепочку проверенной).

### R2-03 — P4 — `SeferAlloc::dealloc_batch` резолвит кучу через alloc-путь `current_heap()`: поток, который только освобождает память, занимает и материализует слот реестра; при `TORN`/исчерпании реестра — глобальный fallback-spinlock

- **Место:** `src/global/sefer_alloc/batch.rs:104-117` (и `:109`: `let _ = fallback::with_heap(...)`);
  `current_heap` — `src/global/sefer_alloc/core.rs:307-316`; контраст — скалярный `dealloc`
  через `current_for_dealloc` с обоснованием R6-OPT-P0-1 (`src/global/sefer_alloc/global_alloc.rs:48-119`,
  `:53-59`); прецедент исправления того же класса — task #492 для `trim_current_thread`
  (`src/global/sefer_alloc/diag.rs:148-165`, пассивный `current_for_trim`).
- **Механизм:** `current_heap()` = `current_for_alloc[_with_config]` — привязывает TLS:
  `HeapRegistry::claim[_with_config]` плюс первая материализация `HeapCore` (резервация primordial,
  регистрация слота). Для `TORN`/исчерпанного реестра — `CurrentHeap::Fallback` → весь батч под
  глобальным `LockGuard`; `None` от `with_heap` (OOM инициализации fallback) отбрасывается `let _ =`,
  и блоки батча молча не освобождаются.
- **Сценарий (динамически, §1.2 п.1):** владелец жив и держит слот; поток со скалярными
  `dealloc` слот не занимает (`count` 1 → 1), поток с `dealloc_batch` — занимает новый (1 → 2).
- **Последствие:** лишний claim и материализация кучи (primordial-резервация, давление на
  `MAX_HEAPS`) на каждый поток-потребитель в producer/consumer-схемах; сериализация на
  fallback-spinlock у потоков в teardown. Рассогласование с политикой R6-OPT-P0-1 и task #492.
  Только опциональная `batch-api` (требует `experimental`).
- **Исправление:** под `alloc-xthread` резолвить через `current_for_dealloc()`: `Own(heap)` →
  `HeapCore::dealloc_batch`; `ForeignNoBind` → для каждого ненулевого блока
  `HeapCore::dealloc_foreign_routing(ptr, segment_base_of_ptr(ptr), layout, None)` — ровно как
  скалярный путь. Без `alloc-xthread` — оставить как есть.
- **Контрфактический тест:** `production internals bench-internals experimental batch-api`: после
  `dealloc_batch` в свежем потоке `bootstrap::count_for_test()` не меняется, а блоки возвращаются
  владельцу (его следующий alloc того же класса получает их из кольца). Сейчас красный (+1 слот).
- **Уже известно?** Нет (раунд 1, §6: «batch-api … дефекта нет»).

### R2-04 — P4 — rustdoc `AllocStats::ring_overflows` велит «проверять и алертить» по `cross_thread_frees_lost`, который после intrusive spill (xa R2-09) всегда 0; doc ссылается на несуществующий тест; тест-растяжка закрепляет устаревшую формулировку

- **Место:** `src/global/alloc_stats.rs:120-131` («The field to check (and alert on) for an
  actually-discarded cross-thread free is `cross_thread_frees_lost`: it increments only when EVERY
  tier of that chain failed»; ссылка на `remote_fanin_owner_starved_residual_is_exactly_accounted`);
  в той же структуре противоположное — `src/global/alloc_stats.rs:222-232` («stays at zero for legal
  frees in the current protocol») и `src/registry/heap_core/core.rs:211-212, :279-290` («no longer
  incremented»). Писателей у `DBG_RING_PUSH_RETRY_EXHAUSTED` в `src/` нет (только объявление
  `core.rs:288` и чтение `src/global/sefer_alloc/diag.rs:126`). Тест переименован:
  `tests/remote_fanin.rs:363` (`…_is_bounded`), старое имя осталось в сообщениях `:533, :552` и в
  `tests/r2_22_ring_overflows_doc_semantics.rs:29-31`. Растяжка
  `tests/no_stale_doc_references.rs:2343-2356` требует, чтобы doc `ring_overflows` называл
  `cross_thread_frees_lost` «the field to check for an actual loss».
- **Механизм:** фикс xa R2-22 перенаправил пользователей на терминальный счётчик; затем фикс xa R2-09
  заменил терминальный drop на spill, и у счётчика не осталось инкремента. Doc `ring_overflows` не
  обновлён, а растяжка R2-22 его «заморозила».
- **Последствие:** оператор алертит по полю, которое не может сдвинуться (вечно «зелёный» сигнал);
  давление третьего яруса (spill) через публичный `AllocStats` не видно. Только диагностика.
- **Исправление:** переписать doc `ring_overflows`: у легального free нет терминального пути потерь,
  третий ярус — spill; `cross_thread_frees_lost` — legacy, всегда 0 (кандидат на deprecation в
  ближайшем semver-окне); при желании добавить публичный счётчик spill (`AllocStats` уже
  `non_exhaustive`). Обновить растяжку и устаревшее имя теста в doc и сообщениях.
- **Контрфактический тест:** растяжка: блок doc `ring_overflows` не должен рекомендовать алертинг по
  полю, задокументированному как всегда-нулевое; поведенческий тест: owner-starved burst сверх
  ёмкости кольца и overflow → `stats().cross_thread_frees_lost == 0` при ненулевом spill-ledger
  (`internals`), что и подтверждает новую формулировку.
- **Уже известно?** Частично: это неполная синхронизация закрытых xa R2-22 и xa R2-09
  (`docs/correctness-open-items/ARCHIVE.md:97` описывает промежуточное состояние R2-09, в котором
  счётчик ещё рос). В открытых индексах нет.

### R2-05 — P4 — при `small-segment-lazy-commit` без `numa-aware` отказ ОС в резервации small-сегмента не учитывается в `SEGMENTS_RESERVE_FAILED_TOTAL`

- **Место:** `src/alloc_core/small/alloc_core_small/reserve.rs:161-185` и `:197-207` — прямой вызов
  `aligned_vmem::reserve_aligned_lazy(...)` с `.inspect(...)`, который увеличивает только
  `SEGMENTS_RESERVED_TOTAL`; `seg?` на `:223` молча возвращает `None`. Контракт счётчика —
  `src/alloc_core/platform/os.rs:59-80` («`aligned_vmem::reserve_aligned{,_lazy}` call that returned
  `None` on a segment-reservation path»); учитывающие конструкторы — `os.rs:175-176, 233-234,
  300-301, 348-349, 378-379`, `src/alloc_core/platform/numa.rs:82-85, 102-105`; потребительский
  контракт — `src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs:259-278` («zero means
  every null came from the allocator's own bookkeeping»).
- **Механизм:** дублированный inline-код резервации в `reserve.rs` обходит учитывающие обёртки
  `os.rs` (у primordial есть свой учитывающий `Segment::reserve_lazy`, у обычных small — нет).
- **Последствие:** в сборках `small-segment-lazy-commit`/`alloc-lazy-commit` без `numa-aware`
  (например `production alloc-lazy-commit`) оракул пунктов 143/146 («null при нулевом приросте
  отказов ⇒ виноват аллокатор») для small-аллокаций несостоятелен: ограничение машины будет
  приписано аллокатору. Текущие потребители оракула (`tests/r14_7_max_segments_ceiling.rs`,
  `tests/large_cache_extended_*`) используют Large и пока не затронуты. В `--all-features` включён
  `numa-aware`, и small-резервации идут через учитывающий `numa.rs` — там пробела нет.
- **Исправление:** инкрементировать счётчик отказов в обеих lazy-ветвях или завести учитывающий
  `os::Segment::reserve_small_lazy` (единый seam, как `reserve_lazy` для primordial) и вызывать его.
- **Контрфактический тест:** инъекции отказа резервации нет (есть только для commit), поэтому —
  растяжка по исходникам: каждый вызов `reserve_aligned{,_lazy}(` в `src/` вне `crates/` должен
  сопровождаться инкрементом `SEGMENTS_RESERVE_FAILED_TOTAL` в ветви `None`; сейчас красный на
  `reserve.rs:182` и `:202`.
- **Уже известно?** Связано с пунктами 143/146 (`docs/correctness-open-items/TRACKED_test_flakiness.md`),
  но как дефект не заведено. Необъяснённый случай пункта 146 этим **не** объясняется — см. §3.8.

### R2-06 — P4 — «защитная» ветка `SegmentTable::recycle` вопреки doc («no-op») освобождает резервацию по непроверенному заголовку и оставляет висячий слот

- **Место:** `src/alloc_core/segment/segment_table/segment_table_impl.rs:375-376` (doc: «If `base` is
  not found in the table … this is a no-op — a defensive guard») против `:422-456` (код:
  `hash_remove` + `own_cache_clear` + `os::release_segment(reservation, reservation_len)`);
  `:382` — `SegmentHeader::read_at(base)` до какой-либо проверки членства. Сам репозиторий признаёт
  последствие: `tests/segment_table_recycle.rs:318-336, :415-420` («If `AllocCore::drop` ran normally …
  a real double-free of an OS resource» → `mem::forget(ac)`).
- **Механизм:** ветвь срабатывает при `slots[segment_id] != base` (испорченный `segment_id`, никогда
  не регистрировавшийся base, повторный recycle). (a) Повторный recycle: сегмент уже unmapped, и
  `read_at` на `:382` падает раньше ветви — заявленный случай «double-recycle» не обрабатывается.
  (b) Испорченный `segment_id` живого сегмента: резервация освобождается, но истинный слот
  по-прежнему указывает на base → обходы `base_at(i)` (fallback-скан `find_segment_with_free_impl`,
  `drain_dirty_segments`, `finalize_orphaned_empty_segments`, reconciliation) читают unmapped память, а
  `Drop` освобождает резервацию второй раз. (c) Никогда не регистрировавшийся base: поля резервации
  берутся из произвольного заголовка → `release_segment` произвольного диапазона.
- **Последствие:** достижимо только после нарушения внутреннего инварианта или порчи памяти, но
  «защитная» ветвь превращает безопасный отказ (утечку) в use-after-unmap/двойное освобождение.
  Обоснование «we do not know which slot, if any, legitimately corresponds to `base`» неверно: слот
  находится однозначно по **значению** (`slots[i] == base`, база уникальна).
- **Исправление:** проверять членство (`hash_find(base)`) до `read_at`; при несовпадении
  `slots[segment_id]` искать слот линейно по значению — нашёлся → штатный путь (hash/cache, release,
  NULL слота, `free_list_push`); не нашёлся → **не** освобождать (утечка + диагностический счётчик,
  `debug_assert!`). Исправить doc.
- **Контрфактический тест:** `alloc-core alloc-decommit internals`: сценарий из
  `tests/segment_table_recycle.rs::recycle_defensive_tail_evicts_hash_and_cache`, но **без**
  `mem::forget(ac)`: после фикса `Drop` проходит, а `segments_released_total` вырос ровно на число
  живых сегментов (сейчас — двойное освобождение резервации `a`).
- **Уже известно?** Частично: остаточное свойство L-3/UBFIX-11 описано в doc теста как
  «orthogonal, pre-existing»; в индексах open-items не заведено.

### R2-07 — P4 — расхождения документации и аргументов безопасности с кодом

- **`AllocCore` «is `Send`»:** `src/alloc_core/alloc_core/mod.rs:35-37`,
  `src/alloc_core/alloc_core/alloc_core_impl.rs:672-676`. Фактически `!Send` (сырые указатели, без
  `unsafe impl`), как и пишут `src/alloc_core/alloc_core/lifecycle.rs:479-484` и
  `src/alloc_core/large/large_cache_extended.rs:55-57`; подтверждено компиляцией (E0277, §1.2).
- **«Never crosses a thread boundary»:** `src/alloc_core/large/large_cache_extended.rs:57-62, :99-101`
  обосновывают plain `*mut`-sidecar тем, что `AllocCore` никогда не пересекает границу потока. В
  `production` `AllocCore` внутри `HeapCore` переходит между потоками при переиспользовании слота
  (`src/global/tls_heap.rs:267-281`, `src/registry/heap_registry/claim.rs:177-182`), а fallback-куча
  используется любым потоком под `LockGuard` (`src/global/fallback.rs:365-387`). Корректный аргумент —
  эксклюзивность, упорядоченная CAS слота (Release в `recycle`, AcqRel в claim) или спинлоком.
- **«This module is `#![forbid(unsafe_code)]`»:** `alloc_core_impl.rs:473-474` — такого атрибута нет;
  в корне `#![deny(unsafe_code)]`, `forbid` — только без `experimental`/`alloc-core` (`src/lib.rs:304-308`).
- **«There is NO `unsafe` block in this file»:** `src/alloc_core/alloc_core/bootstrap.rs:11-17` против
  `:207-215` (`hardened`: `#[allow(unsafe_code)] unsafe { init_gen_table_in_place(base) }`).
- **«The confined-`unsafe` seams are `os` and `node`; every other file is pure safe code»:**
  `src/alloc_core/mod.rs:5-8` — под `src/alloc_core/` 5 tier-1 seam-файлов
  (`large/large_cache_extended.rs`, `platform/{dirty_by_class,node,os,sidecar}.rs`) и 19 файлов с
  tier-2 `#[allow(unsafe_code)]` (команда из `CLAUDE.md`: `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/alloc_core`).
- **«There is NO `unsafe` here»:** `src/alloc_core/segment/remote_free_ring/mod.rs:26-30` против
  `remote_free_ring/ops.rs:106, :134` (tier-2 `unsafe fn`).
- **«This is a SAFE `pub fn`» / «a safe caller» / «under a safe fn»:**
  `src/registry/heap_core/free/realloc.rs:283-288, :364-367, :381-382` против `:123`
  (`pub unsafe fn realloc`).
- **«# Panics (debug only) … the caller already `debug_assert`s»:**
  `src/registry/bootstrap/overflow_sidecar.rs:275-279` против
  `src/registry/heap_overflow/heap_overflow_impl.rs:384-387` — вызывающий использует `assert!`,
  активный и в release, на пути remote-free/drain (паника в контексте `GlobalAlloc::dealloc`).
  Инвариант, по doc, выполняется конструктивно, так что это расхождение политики, а не
  достижимая паника.
- **Последствие:** неверная ментальная модель unsafe-инвентаря и Send-рассуждений; будущая правка
  (например, «добавить `unsafe impl Send for AllocCore`, раз doc так говорит») может внести
  неsoundness.
- **Исправление:** привести тексты к коду; для `assert!` выбрать политику (abort/`debug_assert!`) и
  согласовать doc.
- **Контрфактический тест:** фразовые растяжки в `tests/no_stale_doc_references.rs` для перечисленных
  утверждений.
- **Уже известно?** Частично — тот же класс, что xa R2-23 и R1-11 (другие места); эти конкретные
  утверждения не упомянуты.

### 3.8 Новые сведения по уже открытым пунктам (не новые находки, в счёт не входят)

- **Пункт 145** (`docs/correctness-open-items/TRACKED_test_flakiness.md:127-170`, «`HeapRegistry::claim()`
  can hand a spawned thread the heap another live thread is already using — observed 20/20»).
  На **зафиксированных** исходниках не воспроизводится ни на HEAD, ни на `6658d8e8` (коммит, заведший
  пункт): инструментированная копия того же тестового файла в строке `production internals` — первый
  claim удалённого потока отличен от владельца в 25/25 (5 × 5) и 15/15 (`--test-threads=1`, 3 × 5)
  случаях для каждой ревизии, состояние слота владельца — `LIVE`; минимальный цикл — 20/20 различных.
  Вероятнее всего, наблюдение «20/20» получено на состоянии, которого нет в закоммиченных исходниках
  (например, на промежуточной ревизии теста); механизм по-прежнему не установлен. Кроме того, гипотеза карточки о «generation check в `recycle`» в
  нынешнем API неприменима: при переиспользовании слота `*mut HeapCore` **одинаков** у устаревшего и
  текущего держателя (`claim.rs:236` возвращает тот же адрес ячейки), так что `recycle(ptr)` не может
  их различить без API, несущего поколение claim. Рекомендация: заменить молчаливый повтор в
  `claim_remote_distinct_from` (`tests/regression_xthread_large_free_layout_mismatch.rs:104-120`) жёстким
  `assert_ne!` на первом claim (настоящий регресс станет громким) и, если остаётся зелёным, закрыть
  пункт как невоспроизводимый.
- **Пункт 146** (null при нулевом приросте отказов). По чтению кода при HEAD: в `--all-features`
  включён `numa-aware`, Large-резервация идёт через `numa::reserve_aligned_on_node`, и все её отказы
  учитываются (`numa.rs:82-85, 102-105`). Не-ОС null-ветви `alloc_large` — `align >= SEGMENT`
  (`alloc_core_large.rs:141-143`), переполнение `checked_add` (`:160-163`), `table.register → None`
  (`:623-629`; ветвь cache-hit `:459-464` уходит в slow path) и `NonNull::new(..)?` (`numa.rs:126-127`).
  Для standalone-`AllocCore` теста (несколько десятков сегментов при `MAX_SEGMENTS = 4096`, align 8)
  ни одна не может сработать. R2-05 касается только small-пути и в `--all-features` отсутствует.
  Итог: при HEAD ни одна ветвь `alloc_large` не объясняет наблюдение; следующий шаг карточки
  (снимок `dbg_table_count()` и счётчиков в момент null) остаётся в силе.

## 4. Проверенные цепочки без дефекта

1. **R1-01:** байтовый бюджет `FREE_PARK_CAP` применяется на free-стороне в own-thread `dealloc`
   (`src/registry/heap_core/free/dealloc_own_base.rs:368-427`) и в `dealloc_batch`
   (`src/registry/heap_core/free/dealloc_batch.rs:319-335`) — тот же бюджет, что у refill.
2. **R1-03:** `release_or_pool_empty_segment` возвращает pooled/released
   (`alloc_core_small_pool_impl.rs:262-312`); `find_segment` различает исходы и не касается
   освобождённого сегмента (`find_segment.rs:692-710, 788-822, 873-889`).
3. **R1-04:** `Drop for AllocCore` (`lifecycle.rs:395-476`) — сначала large cache и extension, затем
   живые сегменты по ходу обхода, primordial (хост реестра) — последним; буфера на стеке нет.
4. **R1-05:** `full_check` Room/Stale/Full (`remote_free_ring/ops.rs:252-290, 320-369`) и `room_check`
   (`heap_overflow/push.rs:118-127, 154-224`): Stale → повтор с перечитыванием, прогресс гарантирован.
5. **R1-06:** free под fallback-замком пропускает спящий stall-retry
   (`src/registry/xthread_fallback_gate.rs`), `LockGuard` уступает после `LOCK_TIGHT_SPINS = 64`
   (`src/global/fallback.rs:433-491`).
6. **R1-11 / реестр:** `claim_impl` (`claim.rs:162-238`) — единая последовательность CAS FREE→LIVE,
   bump поколения, материализация по `initialised`, откат при OOM; состояний слота ровно два
   (`heap_slot.rs:99-100`), CAS строгий (`:511-519`), так что вторичный владелец возможен только если
   слот был FREE.
7. **Хеш `SegmentTable`:** backward-shift deletion с условием `dist_hole <= dist_j` корректен; поиск
   завершается при загрузке ≤ 50 % (`segment_table/hash.rs`).
8. **Dirty-бит при `class-aware-dirty`:** грубый бит ставится всегда, латч OOM — Release/Acquire
   (`heap_core_xthread/overflow.rs:336-403`, `alloc_core_small/directory.rs:349-506`); устаревшие биты
   отсеиваются проверками null-base/kind/`segment_id`.
9. **Spill:** `SpillNode` (16 B на 64-бит) статически ≤ `MIN_BLOCK` и выровнен
   (`heap_overflow/spill.rs:11-22`).
10. **Large cache-hit:** заголовок переписывается до `register` (`alloc_core_large.rs:385-465`),
    R34-14 сбрасывает `owner_state`/`deferred_next`/`owner_thread_free`.
11. **NUMA-резервация:** fallback на обычную резервацию при любой ошибке `numa-shim`, отказы учтены
    (`numa.rs:76-110`).
12. **`SegmentHeader::read_at`:** единственное удалённо изменяемое поле `deferred_next` читается
    атомарно (`node.rs:263-307`, `segment_header_impl.rs:901-906`); все вызовы — со стороны владельца.
13. **`AtomicSlot` (experimental):** ограниченные `Send`/`Sync`, `old.is_null() → Stale`, ретирование при
    насыщении поколения (`concurrent/epoch/hand.rs:391-513, 568-578`).

## 5. Гипотезы и направления оптимизации (не findings, не в счёт)

Ни одна не измерялась; план — по правилам проекта (A/B в одном режиме, path-activation oracle,
raw-логи + summary CSV).

- **H1 — финализация старого курсора при смене** (фикс R2-01). Ожидание: меньше commit на
  фазовых нагрузках. Замер: нагрузка со сменой классов по фазам; оракул —
  `small_empty_orphan.count` и `segments_released_total`; commit/RSS через `proc-memstat`.
- **H2 — `trim_current_thread()` может отпускать и пустой `small_cur`** (со сбросом курсора). Цена —
  холодная резервация на следующем alloc. Замер в постановке `R31_10_TRIM_CURRENT_THREAD_RSS_GATE`.
- **H3 — грубый `dirty_segments` при `class-aware-dirty` в установившемся режиме write-only:**
  продюсер делает спорный `fetch_or` на каждую remote-заметку (`overflow.rs:344-346`), а потребитель
  читает грубую карту только до материализации sidecar или после OOM-латча
  (`directory.rs:371-406`); устаревшие биты копятся до срабатывания латча (затем разовый всплеск
  скана). Исключить запись нельзя просто так: неудачный конкурентный `get_or_try_init` может взвести
  латч уже после материализации — нужен, например, один полный per-class проход при переходе латча.
  Замер: fan-in бенч, HITM (`perf c2c`) на Linux, Ir.
- **H4 — `dealloc_batch` без привязки** (фикс R2-03): p99 первого `dealloc_batch` и RSS для N
  потоков-потребителей.

## 6. Готовность по feature surfaces

| Surface | Статический verdict этого прохода |
|---|---|
| `production` (`alloc-global + alloc-xthread + alloc-decommit + fastbin + alloc-segment-directory + primordial-lazy-commit + class-aware-dirty`), 64-bit Windows/Linux/macOS, корректный клиент | Новых P0–P2 нет. **GO с оговорками:** R2-01 (удержание RSS вне pool-лимита), R2-02 (defence-in-depth только для нарушающих контракт), R2-04/R2-07 (документация). |
| `hardened` | То же, что `production`; gen-table-пути — только поиском. |
| `batch-api` (`experimental`) | **CONDITIONAL:** R2-03. |
| `small-segment-lazy-commit` / `alloc-lazy-commit` без `numa-aware` | R2-02 распространяется на все small-сегменты (Windows); R2-05. |
| Standalone `AllocCore` (`alloc-core`) | R2-01 воспроизводится так же; R2-06 — только после порчи метаданных. |
| `numa-aware`, `large-reserved-capacity`, `exact-span-large`, `virgin-zero-skip`, `large-cache-extended` | Прочитаны прямые пути; не сертифицировано. |
| `experimental` (concurrent) | `hand.rs` перечитан; новых дефектов нет. |

## 7. Примечание для следующего шага

По правилу «Round start: check BOTH open-items indexes» новые пункты R2-01…R2-07 следует завести:
R2-01 — в `docs/CORRECTNESS_OPEN_ITEMS.md` и/или `docs/perf/OPEN_ITEMS.md` (RSS/commit), R2-02…R2-07 —
в `docs/CORRECTNESS_OPEN_ITEMS.md`; сведения §3.8 — дописать в карточки 145 и 146; в
`docs/perf/R29_4_SEGMENT_STATE_RECONCILIATION_GATE.md` §3 — пометку о R2-01. В этом коммите это
сознательно не сделано: задача раунда ограничивает коммит одним файлом отчёта.
