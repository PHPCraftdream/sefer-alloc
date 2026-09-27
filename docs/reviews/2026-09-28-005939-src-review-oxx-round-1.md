# Независимое ревью `src/`, раунд 1 (oxx)

Дата: 2026-09-28 00:59:39 CEST. База: `main` @ `03bc2b2f632e01cf37f487fbf81a8800a27b009c`
(worktree `src-review-oxx-r1`). Исходный код `src/` в ходе ревью не изменялся;
этот файл — единственный артефакт коммита.

## 1. Вердикт и границы

**Вердикт.** Для корректного клиента `GlobalAlloc` в `production` (и `hardened`) на
64-битных Windows/Linux/macOS в этом проходе **не найдено новых P0–P2**
(UB, потеря free, повреждение памяти). Свежие исправления R3-1 (fallback-overflow) и
R3-2 (deferred Large через `swap`) перепроверены и выглядят корректными (§4).
Найдены **4 подтверждённых P3** — удержание RSS в магазине вопреки D3-бюджету,
отсутствие контракта/обработки `fork()`, расхождение поведения small-segment pool с
публичной документацией, переполнение стека в `Drop for AllocCore` — и **7 P4**
(диагностика, liveness fallback-замка, гигиена unsafe-инвентаря и feature-матрицы,
сопровождаемость). Три из P3 подтверждены **динамически** (§1.2).

### 1.1 Что прочитано

- **Построчно, вместе с комментариями и `// SAFETY:`:** `src/lib.rs`,
  `src/global/{mod.rs, tls_heap.rs, fallback.rs, sefer_alloc/global_alloc.rs}`,
  `src/registry/heap_core/{core.rs, alloc/hot.rs, free/dealloc.rs, free/dealloc_own_base.rs, free/realloc.rs}`,
  `src/registry/heap_core_xthread/{routing.rs, overflow.rs}`, `src/registry/heap_overflow.rs`,
  `src/alloc_core/alloc_core/mem/realloc_fastpath.rs`, `src/alloc_core/platform/node.rs` (без `///`).
- **Весь код без doc-комментариев (`//`-строки срезаны, выборочно дочитаны SAFETY):**
  `global/sefer_alloc/{mod,core,batch,diag}.rs`; `registry/{mod.rs, heap_slot.rs}`,
  `registry/heap_core/{mod.rs, state/{tcache,ownership,tcache_flush}.rs, alloc/batch.rs, free/dealloc_batch.rs}`,
  `registry/heap_core_xthread/{ring,stall,drain,mod}.rs`, `registry/heap_registry/{claim,stack}.rs`,
  `registry/bootstrap/{registry,overflow_sidecar}.rs`;
  `alloc_core/alloc_core/{mem/mem_impl.rs, state.rs}` + `lifecycle.rs` (Drop) + `bootstrap.rs` (init сегмента);
  `alloc_core/large/{alloc_core_large.rs, alloc_core_large_cache.rs, large_cache_extended.rs, deferred_large/*}`;
  `alloc_core/small/{alloc_core_small/{alloc_core_small_impl,find_segment,dealloc,reserve,directory}.rs, alloc_core_small_reclaim.rs, alloc_core_small_magazine.rs, alloc_core_small_pool/{alloc_core_small_pool_impl,decommit}.rs}`;
  `alloc_core/segment/{remote_free_ring/*, segment_table/{hash,segment_table_impl}.rs, segment_header/segment_header_views.rs, segment_layout.rs}`;
  `alloc_core/platform/{os,sidecar,dirty_by_class,size_classes}.rs`;
  `alloc_core/config/{large_cache_config,large_cache_mode}.rs` (+ docs `small_segment_pool_config.rs`);
  `concurrent/epoch/{hand.rs, epoch_region.rs}`, `concurrent/sharded/sharded_region.rs`, `concurrent/lock_free/lock_free_region.rs` (частично).
- **Только поиском (grep по unsafe / raw-pointer сигнатурам / паникам / атомикам / cfg):**
  `registry/heap_core/diag/*`, `alloc_core/alloc_core/alloc_core_core_diag/*`, `registry/heap_registry/counters.rs`,
  `registry/bootstrap/{ensure,chunk,loom_shim,mod}.rs`, `segment/{segment_directory,bitmap}/*`,
  `segment_header/{descriptors,segment_header_impl,segment_header_gen_table,meta_fields,layout,layout_asserts}.rs`,
  `small/alloc_core_small_pool/{decomp_hooks,segment_state_*}.rs`, `small/alloc_core_small_diag.rs`,
  `platform/{numa,sidecar_stats}.rs`, `config/profile.rs`, `concurrent/{pinning.rs, *_handle.rs}`, `kani_proofs.rs`.
- **Не покрыто:** реализации crates из `crates/` (`aligned-vmem`, `once-ptr-cell`, `size-classes`,
  `tagged-index-stack`, `sefer-region`, `numa-shim`) — только точечные справки; кодогенерация;
  реальные ARM/weak-memory интерлейвинги; `numa-aware`/lazy-commit/`large-reserved-capacity`/
  `exact-span-large`/`virgin-zero-skip` проверены только по прямым путям, без сертификации.
- Прошлые отчёты (`docs/reviews/2026-09-2{1,2,4}-*src-review*`) и индексы open-items использованы
  **только** для дедупликации (пометки «уже известно»), не как доказательство.

### 1.2 Что запускалось

Тесты/clippy/miri/loom/kani проекта **не запускались** (по условию задачи — параллельный тяжёлый
прогон). Для подтверждения гипотез собраны два **временных** probe-крейта внутри worktree
(свой `[workspace]`, свой `CARGO_TARGET_DIR`, `-j 2`), после прогона **удалены**
(`git status` чист). Toolchain: rustc/cargo 1.97.0, Windows 10 x86_64, `--release`.

1. `sefer-alloc` с `default-features = false, features = ["std","alloc-core","alloc-decommit","internals"]`:
   - поток со `stack_size(64 KiB)`, `drop(AllocCore::new().unwrap())` → код выхода
     `3221225725` (`0xC00000FD`, STATUS_STACK_OVERFLOW), stderr: `thread '<unknown>' has overflowed its stack`;
     контроль `mem::forget` на том же стеке → 0; `drop` на стеке 1 MiB → 0 (**R1-04**);
   - pool-probe: `r0=1 r1=3 r2=4 freed16=507648 pooled_before=1 n48=85835 pooled_after=1` (**R1-03**);
   - `cargo check` этой комбинации → 10 предупреждений (**R1-08**).
2. `sefer-alloc` с `features = ["production","internals","bench-internals"]` (bench-internals нужен
   только для `dbg_tcache_contains`; сам путь — production): 16 alloc + 16 free одного класса
   через `HeapCore` (**R1-01**):
   `size=206992 class=47 parked_in_magazine=16 parked_bytes=3311872 live_count=16`;
   `size=258752 class=48 parked=16 parked_bytes=4140032`; `size=67808 class=42 parked=16 parked_bytes=1084928`.
3. Проверен исходник std 1.97 (`library/std/src/sys/thread_local/destructors/list.rs`):
   список TLS-деструкторов — `Vec<_, System>` (§4, п.1).

Всё остальное в отчёте — статический вывод из кода с указанием строк.

## 2. Сводка

| Приоритет | Подтверждено | Номера |
|---|---|---|
| P0 | 0 | — |
| P1 | 0 | — |
| P2 | 0 | — |
| P3 | 4 | R1-01, R1-02, R1-03, R1-04 |
| P4 | 7 | R1-05 … R1-11 |
| **Итого** | **11** | гипотезы §5 в счёт не входят |

## 3. Подтверждённые находки

### R1-01 — P3 — free-путь магазина игнорирует D3 byte-budget: до `16 × block_size` на класс «паркуется» в потоке и пиннит сегменты

- **Место:** `src/registry/heap_core/free/dealloc_own_base.rs:368-399` (push при `cnt < TCACHE_CAP`
  независимо от размера блока), то же в `src/registry/heap_core/free/dealloc_batch.rs:313-327`;
  бюджет только для refill — `src/registry/heap_core/state/tcache.rs:76-84`
  (`REFILL_BYTE_BUDGET = 64 KiB`, `refill_n_for_class`) и обещание в `src/registry/heap_core/alloc/hot.rs:458-464`
  («one magazine miss cannot park megabytes in a single idle thread's cache»); инвариант D1
  «magazine-resident block COUNTS AS LIVE» — `src/registry/heap_core/core.rs:416-427`.
- **Механизм:** D3 ограничивает, сколько байт *refill* кладёт в магазин (для блоков > 32 KiB —
  ровно 1 блок), и документ прямо называет проблемой «`16 × ~253 KiB ≈ 4 MiB` … real RSS parked in a
  per-thread cache that may sit idle». Но own-thread `dealloc` кладёт в магазин любой small-блок,
  пока `count < 16`; байтового предела на free-стороне нет. Припаркованные блоки считаются живыми
  (`live_count`), поэтому их сегменты не могут опустеть → нет pool/decommit/release.
- **Сценарий:** `production` (и любые сборки с `fastbin`): поток аллоцирует и освобождает ≤16 буферов
  класса > 32 KiB, затем долго работает с другими размерами. Динамически (§1.2 п.2): после 16 free
  класса 47 (206 992 B) все 16 блоков в магазине, `parked_bytes = 3 311 872`, `live_count` сегмента
  = 16; класс 48 — `4 140 032 B` (ровно «≈4 MiB на один класс» из D3).
- **Последствие:** удержание RSS/commit, ограниченное лишь `16 × Σ block_size`: для десяти классов
  > 32 KiB, у которых refill-бюджет = 1 блок (34 704 … 258 752 B), это `16 × 1 154 672 = 18 474 752 B`
  ≈ 17.6 MiB на поток; с `medium-classes` плюс `16 × 3 342 336 = 53 477 376 B` ≈ 51 MiB на поток.
  Освобождается только при переполнении магазина, `trim_current_thread()` или выходе потока.
  Не UB и не утечка — ограниченное, но документированно-нежелательное удержание.
- **Исправление:** применить тот же байтовый бюджет на free-стороне: если
  `count × block_size + block_size > REFILL_BYTE_BUDGET` (или отдельный `FREE_PARK_BUDGET`),
  отдавать блок сразу в substrate (`flush_class(c, &[ptr])`/`dealloc_small`) вместо push; сохранить
  M2-оракулы (они уже выполнены до push). Альтернатива — `TCACHE_CAP` по классу из той же функции.
- **Контрфактический тест:** `production internals bench-internals`, свой `HeapCore`
  (`HeapRegistry::claim`): 16 alloc/free класса 206 992 B → утверждать
  `dbg_tcache_count(c) ≤ refill_n_for_class(bs)` и что сегмент без других живых блоков достигает
  `dbg_live_count_for == 0`. Сейчас красный (16 / live=16), после фикса — зелёный; для класса ≤ 4 KiB
  поведение (16 в магазине) не меняется — это вторая половина оракула.
- **Уже известно?** Нет (в индексах open-items и прошлых src-ревью не найдено).

### R1-02 — P3 — у `SeferAlloc` нет контракта и обработки `fork()`: в дочернем процессе возможны вечные спины (в т.ч. на dealloc) и навсегда заблокированные дренажи

- **Место:** `src/global/fallback.rs:393-402` (`LockGuard::acquire` — неограниченный спин) и
  `:318-320` (спин пока `INITIALIZING`); `src/registry/bootstrap/overflow_sidecar.rs:166-192`
  (проигравший ждёт снятия `SENTINEL` без предела — на **dealloc-пути**, из `HeapOverflow::push_impl`);
  `src/registry/bootstrap/registry.rs:287-337` (`ensure_chunk`/`try_ensure_chunk` → once-ptr-cell loser loop:
  `HeapRegistry::claim` нового потока в ребёнке, если чанк материализовался в момент `fork`);
  `src/registry/heap_overflow.rs:959-969` (spill-drain останавливается на узле с `ready==0`);
  `src/alloc_core/large/deferred_large/drain.rs:38` (drain останавливается на `PUBLISHING`);
  `src/registry/heap_core_xthread/ring.rs:143-171` (слот исчезнувшего потока остаётся `LIVE`).
  Во всём `src/` и `crates/` нет `pthread_atfork`; README не упоминает `fork` вовсе
  (для `racy-ptr-cell` это задокументировано — `TRACKED_publish_readiness.md` item 122/F1 — но не для корневого аллокатора).
- **Механизм:** после `fork()` многопоточного процесса в ребёнке живёт один поток, а всё
  разделяемое состояние аллокатора копируется «как есть»: занятый другим потоком fallback-spinlock,
  `INIT_STATE == INITIALIZING`, `SENTINEL` в `HeapOverflow::sidecar` или в чанке реестра, spill-узел
  с `ready==0`, deferred-Large ссылка `PUBLISHING`, слоты `LIVE` без владельцев.
- **Сценарий:** `production`, Unix, программа делает `fork()` без немедленного `exec` (pre-fork
  серверы, демонизация) в момент, когда другой поток: (а) держит fallback-lock (TLS-teardown), —
  ребёнок, попав в fallback, крутится вечно; (б) материализует overflow-sidecar при насыщении —
  любой remote free в ту же кучу с индексом ≥ `INLINE_CAP` в ребёнке зависает в `ensure_overflow_sidecar_slow`;
  (в) делает spill/deferred-Large push между `swap` и публикацией — все записи за этим узлом в ребёнке
  не дренируются никогда (утечка сегментов); (г) remote free в ребёнке блоков чужих (исчезнувших)
  куч видит `owner_slot_is_live == true` и платит до 128 «зависших» раундов со sleep
  (`overflow.rs:134-135`, ≈0.3–2 с на первую уступку) перед spill.
- **Последствие:** зависание (включая `GlobalAlloc::dealloc`) или постоянное удержание памяти в
  ребёнке. POSIX формально не разрешает `malloc` в ребёнке многопоточного `fork()` до `exec`, но
  glibc/jemalloc/mimalloc это поддерживают через atfork-обработчики, и пользователь drop-in аллокатора
  вправе ожидать того же или явного запрета.
- **Исправление:** минимум — раздел «Fork safety» в README/rustdoc `SeferAlloc` (только `fork`+`exec`,
  без аллокаций в ребёнке). Полноценно — `pthread_atfork`: `prepare` берёт fallback-lock и ждёт
  завершения материализаций; `child` сбрасывает lock/`INIT_STATE`/sentinel-ы и переводит все слоты,
  кроме слота вызвавшего потока, в состояние «покинут» (без ожидания владельца), а незавершённые
  spill/deferred-узлы помечает для восстановления.
- **Контрфактический тест (Unix, subprocess + watchdog):** поток A заходит в
  `HeapCore::dbg_with_fallback_for_test` и блокируется на барьере; главный поток делает `fork()`;
  ребёнок вызывает `with_heap`/аллокацию на fallback-пути под `alarm(5)`. Сейчас ребёнок убит
  сигналом (зависание), после фикса завершается 0. Второй вариант — хук, удерживающий sidecar-`SENTINEL`
  на время `fork`, и remote free в ребёнке.
- **Уже известно?** Нет для корневого крейта.

### R1-03 — P3 — empty-small-segment pool никогда не используется как цель carve на `reserve`, вопреки README и module-doc; сегмент, опустевший при drain кольца, пропускается для текущего запроса

- **Место:** обещание — `README.md:1499-1504` («the next allocation that would otherwise reserve a
  fresh segment pops a pooled one»), `src/alloc_core/config/small_segment_pool_config.rs:17-23` и
  `:38-41` («every retained slot is reusable (popped on the next reserve)»). Реальность —
  `src/alloc_core/small/alloc_core_small/reserve.rs:80-468` (всегда свежая OS-резервация;
  `pop_pooled_segment` вызывается только из `drain_small_pool`, `alloc_core_small_pool_impl.rs:462-475, 670-684`),
  и собственный комментарий `release_or_pool_empty_segment` (`alloc_core_small_pool_impl.rs`, док над `:249`):
  «the pool is a free-list reserve, not a carve reserve». Вторичный механизм:
  `find_segment.rs:273-277` возвращает `Decommitted` и для **pooled** (не освобождённого) сегмента,
  после чего `:690` делает `continue`, `:863` — `return None`.
- **Механизм:** pooled-сегмент полезен только запросу того же класса, у которого в нём есть free-list;
  для другого класса аллокатор резервирует новый 4 MiB-сегмент, а pooled остаётся закоммиченным до
  decay. Кроме того, если drain удалённых free опустошил сегмент прямо в `find_segment_with_free`,
  он попадает в pool, но в этом вызове не рассматривается — даже если в нём есть блоки нужного класса.
- **Сценарий:** `production` (pool включён по умолчанию, 4 сегмента / 16 MiB). Динамически (§1.2 п.1,
  standalone `AllocCore`): после опустошения сегмента X блоками 16 B (`pooled_before=1`) 85 835
  аллокаций по 48 B заполнили текущий сегмент и вызвали новую резервацию (`r1=3 → r2=4`) при
  `pooled_after=1` — пустой закоммиченный X не использован.
- **Последствие:** для смены классов (фазы нагрузки, pipeline producer→consumer) — до `pool_cap × 4 MiB`
  мёртвого закоммиченного RSS плюс лишние reserve-syscall/page faults; пользователь, увеличивающий
  `pool_segments` по совету README ради латентности, этого выигрыша не получает. Не UB.
- **Исправление:** либо исправить документацию (pool = резерв free-list того же класса), либо в
  `reserve_small_segment_impl` при `pooled_count > 0` забирать pooled-сегмент и переинициализировать
  (bump, BinTable, оба bitmap, `payload_virgin=false`) без OS-вызова (см. H1, с измерением);
  во вторичном механизме возвращать pooled-сегмент как hit, если `bt.head(class_idx) != NULL`.
- **Контрфактический тест:** повторить pool-probe в `tests/` (`alloc-decommit internals`) с оракулом
  «новая резервация не происходит, пока `dbg_pooled_count() > 0`» (или, при выборе doc-фикса, —
  docs-tripwire на фразу «pops a pooled»). Второй тест: remote free всех блоков сегмента X, затем промах
  нужного класса при заполненном `small_cur` → ожидать hit в X, а не `segments_reserved_total + 1`.
- **Уже известно?** Нет (история B3 в коде известна, расхождение с README/doc — нет).

### R1-04 — P3 — `Drop for AllocCore` кладёт на стек 64 KiB и переполняет стек малых потоков; противоречит аудиту F-6

- **Место:** `src/alloc_core/alloc_core/lifecycle.rs:441-442`
  (`let mut to_free: [(*mut u8, usize); MAX_SEGMENTS]`, `MAX_SEGMENTS = 4096` → 65 536 B);
  утверждение «This is the ONLY unbounded-growth stack-pressure surface … no other stack buffer larger
  than `emptied_bases: [*mut u8; 64]`» — `src/registry/heap_core/core.rs:574-577`.
- **Механизм:** массив нужен, чтобы не освобождать primordial-сегмент (где живёт `SegmentTable`)
  во время обхода `table.bases()`; но размер буфера — максимум таблицы, а не число сегментов.
- **Сценарий:** публичный `sefer_alloc::AllocCore` (фича `alloc-core`; реэкспорт в корне), обычный
  `AllocCore::new()` + `drop` в потоке со стеком ≤ ~64 KiB. Динамически подтверждено
  (`0xC00000FD`, «has overflowed its stack»), контроли — зелёные (§1.2 п.1). `SeferAlloc` не затронут:
  registry-кучи не дропаются.
- **Последствие:** аварийное завершение процесса при корректном использовании публичного API; сам
  проект считает стеки 16–64 KiB «realistic deployment» (`core.rs:519-527`) и бюджетирует 9 KiB.
- **Исправление:** освобождать все сегменты, кроме primordial, прямо в цикле по `bases()` (читает
  только таблицу в primordial), primordial — последним; либо два прохода без буфера.
- **Контрфактический тест:** subprocess-тест: поток `stack_size(64 * 1024)` создаёт и дропает
  `AllocCore` (плюс вариант с несколькими small/large сегментами) → ожидать код 0. Сейчас — stack overflow.
- **Уже известно?** Нет (item про `AllocCore::drop` в `TRACKED_correctness_residuals.md` — о другом).

### R1-05 — P4 — «устаревший» снимок `tail` трактуется как «кольцо полно»: ложный `ring_overflows` и лишний переход на второй ярус

- **Место:** `src/alloc_core/segment/remote_free_ring/ops.rs:221-254` (`full_check`: `if t < h … Err`)
  с вызовом из `push` `:263-278` (Err → счётчики + `PushOverflow`) и `try_push_uncounted` `:320-331`;
  аналогично `src/registry/heap_overflow.rs:776-786` (`t.wrapping_sub(h) >= CAP` при `t < h`).
- **Механизм:** `t` читается `Relaxed` до `head`. Если продюсер вытеснен между чтением `tail` и
  `full_check`, а за это время другие продюсеры добавили записи и владелец всё осушил, то `t < head`,
  кольцо фактически пусто, но возвращается `Err(PushOverflow)` вместо перечитывания `tail`
  (CAS всё равно провалился бы и цикл перезагрузил бы `t`).
- **Сценарий/последствие:** `alloc-xthread` (`production`) под fan-in и вытеснением: ложные тики
  `DBG_RING_OVERFLOW` (публичный `AllocStats::ring_overflows`, который README предлагает мониторить) и
  per-segment `overflow_count`, free уходит в `HeapOverflow` (дренируется только на magazine-miss),
  реже — в retry-цикл. Потери free нет. (Отлично от закрытого R2-10 — там был ABA при CAS.)
- **Исправление:** различать «stale snapshot» и «full»: при `t < h` — `continue` (перечитать `tail`),
  `Err` только при `t - h >= CAP`; то же в `HeapOverflow::push_impl` до `wrapping_sub`.
- **Контрфактический тест:** loom-модель: P читает `tail`, пауза; Q пушит k записей; владелец дренирует
  всё; P продолжает → утверждать `push == Ok` и неизменный `DBG_RING_OVERFLOW`. Сейчас — `Err` + тик.

### R1-06 — P4 — fallback-spinlock удерживается через ретрай-цикл со `sleep` (до тысяч раундов), остальные пользователи fallback крутятся без backoff

- **Место:** `src/global/sefer_alloc/global_alloc.rs:191-194` (fallback `realloc` в `with_heap`),
  `src/global/sefer_alloc/batch.rs` (fallback `dealloc_batch`), `src/registry/heap_core/free/realloc.rs:385-426`
  (foreign-ветка → `self.dealloc` → routing), `src/registry/heap_core_xthread/overflow.rs:708-873`
  (`sleep(200µs)` между раундами, `RETRY_ROUND_SAFETY_CAP = 4096`), `src/global/fallback.rs:393-402`
  (спин `compare_exchange` + `spin_loop` без yield).
- **Сценарий:** `alloc-xthread`, поток в TLS-teardown (TORN) делает `realloc` чужого блока, владелец
  жив (`LIVE`), но оба кольца насыщены и дренаж идёт медленно: весь ретрай (до 128 «стоящих» раундов
  или до 4096 при прогрессе) выполняется под fallback-замком; другие потоки в teardown/при исчерпании
  реестра жгут CPU на спине всё это время.
- **Последствие:** liveness/CPU-burn в редком углу; не deadlock (владелец fallback не ждёт).
- **Исправление:** выполнять `dealloc` старого чужого блока **после** выхода из `with_heap`
  (вернуть из замыкания признак/указатель), либо под fallback-замком сразу выбирать spill без ожидания;
  в `LockGuard::acquire` — backoff/yield после N итераций.
- **Контрфактический тест:** хуками насытить кольцо+overflow «живого, но стоящего» владельца,
  выполнить fallback-`realloc` чужого блока в потоке A и параллельно fallback-`alloc` в потоке B;
  оракул — время ожидания замка B < 50 ms (сейчас ≈ 128 × sleep-гранулярность).

### R1-07 — P4 — `#![allow(unsafe_code)]` в `mod.rs` молча расширяет seam на неинвентаризированные файлы

- **Место:** `src/registry/bootstrap/mod.rs:188` покрывает весь подмодуль, в т.ч.
  `bootstrap/loom_shim.rs:42,44` (`unsafe impl Send/Sync`) и `:108,124,171` (`NonNull::new_unchecked`),
  которых нет ни в `README.md:661-664`, ни в `src/lib.rs:221-231`; `bootstrap/chunk.rs:45-49`
  утверждает, что ему allow «не нужен», но он под ним; `src/registry/heap_registry/mod.rs:65` — избыточный
  дубль allow дочерних файлов. Комментарий над `bootstrap/mod.rs:172-187` описывает код, которого в
  файле уже нет.
- **Механизм/последствие:** «самопроверяющая» команда `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]'`
  находит `mod.rs`, но аудитор по модели «файл = seam» не проверит `loom_shim.rs`; будущий `unsafe`
  в `chunk.rs` или новом файле подмодуля скомпилируется без нового маркера. Сейчас `loom_shim` только
  под `cfg(loom)` — soundness production не затронута.
- **Исправление:** убрать `#![allow]` из обоих `mod.rs`, поставить его в `loom_shim.rs` (и в каждый файл
  с unsafe), добавить `loom_shim` в инвентарь.
- **Контрфактический тест:** tripwire в `tests/`: каждый файл `src/**` с токеном `unsafe` вне комментариев
  обязан сам содержать tier-1/tier-2 allow; `#![allow(unsafe_code)]` в `mod.rs` запрещён. Сейчас красный на `loom_shim.rs`.
- **Уже известно?** Частично: отсутствие `// SAFETY:` у `new_unchecked` в `loom_shim` — item 120/F8 (`TRACKED_publish_readiness.md`); аспект инвентаря — нет.

### R1-08 — P4 — долг предупреждений в фиче-матрице: `alloc-core + alloc-decommit + internals` → 10 warnings

- **Место:** под `internals` декларация `pub mod alloc_core` (`src/lib.rs:414-416`) лишается
  `#[allow(dead_code, unused_imports)]`, который есть у `pub(crate)`-варианта (`:428-430`).
  Предупреждения: `alloc_core_large.rs:19` (`CachedLarge`), `alloc_core_small_reclaim.rs:33-40`
  (6 unused imports), `lifecycle.rs:181` (`live_config_matches`), `alloc_core_large_cache.rs:683`
  (`evict_all`), `magazine_bitmap.rs:129` (`mark_magazine`/`clear_magazine`),
  `alloc_core_small_pool_impl.rs:341` (`finalize_orphaned_empty_segments`).
- **Подтверждение:** `cargo check` (§1.2 п.1). Weekly `cargo hack check --feature-powerset --depth 2`
  (`.github/workflows/ci.yml:4144-4145`) идёт без `-D warnings` (top-level `env` задаёт только
  `CARGO_TERM_COLOR`), поэтому этот класс не ловится.
- **Исправление:** точнее cfg-гейтить импорты/методы (`alloc-xthread`/`alloc-global`) или добавить
  `RUSTFLAGS=-D warnings` в powerset-job.
- **Контрфактический тест:** CI-строка `RUSTFLAGS="-D warnings" cargo check --no-default-features
  --features "std alloc-core alloc-decommit internals"` — сейчас красная.

### R1-09 — P4 — нет compile-time проверки ширины битовой маски занятости large-cache

- **Место:** `large_cache_occupied: u64` индексируется комбинированным индексом
  (`src/alloc_core/large/alloc_core_large_cache.rs:154,170,351,365` — `1u64 << idx`), но проверено только
  `LARGE_CACHE_SLOTS <= 64` (`src/alloc_core/alloc_core/alloc_core_impl.rs:94`); расширение —
  `LARGE_CACHE_EXTENDED_SLOTS = 32` (`large_cache_extended.rs:115`). Сегодня 8 + 32 = 40 ≤ 64 — дефекта нет.
- **Последствие при изменении константы > 56:** в debug — паника сдвига на пути `dealloc`/`alloc`,
  в release — сдвиг по модулю и порча маски (неверные слоты кэша).
- **Исправление:** `const _: () = assert!(LARGE_CACHE_SLOTS + LARGE_CACHE_EXTENDED_SLOTS <= 64);` под
  `large-cache-extended`. Контрфактика — сборка с `LARGE_CACHE_EXTENDED_SLOTS = 57` должна падать на const-eval.

### R1-10 — P4 — `stats()` не учитывает hit-счётчики fallback-кучи

- **Место:** fallback-init привязывает только `thread_free` и `overflow`
  (`src/global/fallback.rs:269-285`); `tcache_hits` остаётся `None` (`hot.rs:374-380` — инкремент пропускается),
  `large_cache_hits_sink` — `None` (счёт уходит в локальное поле, `alloc_core_large.rs:266-274`);
  агрегаторы `tcache_and_large_cache_hits_total` ходят только по слотам реестра
  (`src/registry/heap_registry/counters.rs:254-266`).
- **Последствие:** под `alloc-stats` «process-wide» `AllocStats::{tcache_hits, large_cache_hits}` занижены
  на активность fallback (teardown, исчерпание реестра). Только диагностика.
- **Исправление:** process-static счётчики для fallback, привязка при init и добавление в агрегаторы.
- **Контрфактический тест:** `alloc-stats internals bench-internals`: magazine-hit через
  `dbg_with_fallback_for_test` → ожидать рост `stats().tcache_hits` (сейчас неизменен).

### R1-11 — P4 — сопровождаемость

- Правило «1000-line file-size cap» (цитируется в `heap_core_xthread/overflow.rs:38`, `stall.rs:4`) не
  проверяется ничем и нарушено: `src/registry/heap_overflow.rs` — 1290 строк.
- `HeapRegistry::claim` (`claim.rs:57-121`) и `claim_with_config` (`:148-227`) — почти дословные копии
  протокола CAS/материализации/bind — риск рассинхронизации (ср. историю R26-4).
- Объём прозы: ≈20 тыс. строк `///`/`//!` против ≈15.6 тыс. строк кода (2.45 MB исходников против
  0.51 MB без комментариев); значительная часть — история задач (`R6-OPT-P0-4`, `task #136` и т.п.),
  дублируемая между файлами. Прошлые R3-4/R2-23 — ровно устаревшая проза; см. также stale-комментарий R1-07.
  Рекомендация: история — в `docs/` (ADR), в коде — инварианты и SAFETY.
- `#[inline(always)]` на крупных телах (`drain_heap_overflow` с замыканиями, `dealloc_own_thread_with_base`)
  раздувает код cold-путей (см. H8).

## 4. Проверенные цепочки без дефекта

1. **GlobalAlloc/TLS:** однобранчевое `p.addr().wrapping_sub(1) < usize::MAX - 1` различает real/null/TORN;
   `finish_bind` армирует `GUARD` до публикации `LOCAL` и откатывает claim; `AbandonGuard` ставит TORN до
   drain/trim/recycle. Регистрация TLS-деструктора в std 1.97 использует `Vec<_, System>` — реентерабельный
   abort «global allocator may not use TLS with destructors» невозможен.
2. **`RemoteFreeRing` (u64):** терминальный `t == u64::MAX`; цепочка `slot.store(EMPTY)` → `head` Release →
   Acquire+Release `cached_head` → Acquire у следующего продюсера транзитивна; RAII-публикация `head`;
   граница u32-кэша `ring_drain_head` отключает shortcut (`tail_relaxed == MAX`, `drain` не возвращает MAX).
3. **`HeapOverflow`:** sidecar материализуется **до** CAS резервации (нет «застрявшей» дыры); публикация
   `packed` → `base`(Release) ↔ `base`(Acquire) → `packed`; drain под токеном с публикацией прогресса.
   **Spill:** `swap` возвращает фактического предшественника (provenance сохраняется), consumer
   останавливается на `ready==0`, CAS-pop до reclaim, abort-guard на unwind; смешанный атомарный/неатомарный
   доступ к `ready` упорядочен happens-before.
4. **Deferred Large (фикс R3-2):** double-push guard `ABANDONED_TAIL→PUBLISHING`, swap-ссылка, drain стоит на
   `PUBLISHING`; ABA `head` CAS в drain невозможен без повторного push узла, который guard запрещает;
   cache-hit сбрасывает `deferred_next`/`owner_state`/`owner_thread_free`.
5. **Fallback remote free (фикс R3-1):** `OWNER_ID_FALLBACK` → статический `FALLBACK_OVERFLOW`;
   `owner_slot_is_live == false` для fallback → без спина; дренаж через `self.overflow` fallback-кучи.
6. **`push_with_overflow_retry`:** G1 — всё чтение сегмента до публикации; `abort` только при
   неразрешимом owner-id (для легального штампа чанк уже материализован claim-ом).
7. **OPT-C stamp-cache:** `OWNER_ID_NONE = 0x7FFF_FFFF` не совпадает ни с одним id слота → после cache-hit
   Large штампы восстанавливаются.
8. **Магазин/flush:** блоки в магазине считаются живыми → `flush_run` не может освободить сегмент, у
   которого ещё есть блоки дальше в том же списке; `alloc_batch` откладывает снятие magazine-бит до конца
   refill (защита предиката); `dealloc_batch` staging дедуплицируется `is_free` в `flush_run`.
9. **realloc:** OPT-F только при равенстве классов; OPT-G только рост, а `class_for` монотонен по размеру при
   фиксированном align (медленный путь идёт до конца таблицы) → Large остаётся Large; move-ветки ограничены
   `safe_payload_read_span`; foreign-ветка проверяет null-base/magic/span.
10. **Отложенная финализация:** `drain_heap_overflow` и `drain_segment_ring` pool/release — только после
    завершения прохода; `recycle` таблицы обнуляет слот до push во free-list (нет двойного push);
    `BinTable::head/set_head` имеют release-guard.
11. **`SegmentHeader::read_at`:** единственное удалённо записываемое поле (`deferred_next`) читается атомарно;
    остальные удалённо читаемые поля пишет только владелец живого сегмента.
12. **Реестр:** `claim`/`recycle` CAS, `bump_count` с откатом, обходчики ограничены `count.min(MAX_HEAPS)`.
13. **Large cache:** предпроверка невыполнимого бюджета, завершение цикла эвикции, FIFO по `seq`,
    `decay_rate_percent` зажат в 1..100.
14. **Experimental:** `EpochRegion` — `region_id`-гейт, `old.is_null() → Stale`, писатели под mutex;
    глобальный `MY_SHARD` в `ShardedRegion` задокументирован и безопасен благодаря mutex в `EpochRegion`.

## 5. Гипотезы и направления оптимизации (не findings, не в счёт)

Ни одна не измерялась; для каждой — план, соответствующий правилам проекта (A/B в одном режиме,
path-activation oracle, raw-логи + summary CSV).

- **H1 — повторное использование pooled-сегментов как цели carve** (связано с R1-03). Переинициализация
  bump/BinTable/двух bitmap (≈2×32 KiB memset) вместо mmap+page faults 4 MiB. Замер: смена классов
  (фазы 16 B → 48 B → 200 B), `segments_reserved_total`, commit/RSS (`proc-memstat`), p99 alloc; oracle —
  счётчик «pool-pop-as-carve».
- **H2 — байтовый бюджет на free-стороне магазина** (фикс R1-01): риск регрессии churn крупных классов.
  Замер: iai `small_churn_*` на классах 64 KiB–253 KiB + RSS после «burst then idle».
- **H3 — чередование alloc-bitmap и magazine-bitmap** (2 бита на блок в одном слове): free-путь сейчас
  трогает magazine-bitmap, заголовок (`bump`) и alloc-bitmap — три линии; hit-путь `alloc` делает RMW
  magazine-bitmap. Замер: iai Ir + cachegrind D1-misses на `small_churn_16b`, `global_alloc`.
- **H4 — false sharing строки 0 `HeapSlotRemote`:** `tcache_hits`, `large_cache_hits`, `thread_free`
  (владелец читает на каждом magazine-miss и Large alloc) делят линию с `dirty_segments[0..5]`
  (удалённый `fetch_or` на каждый cross-thread free в сегменты с id < 320). Замер: fan-in бенч
  (`remote_fanin`-подобный), `perf c2c`/HITM на Linux, до/после выноса `thread_free` на отдельную линию.
- **H5 — in-place shrink Large с decommit хвоста** вместо alloc+copy+free (сейчас OPT-G только растёт).
  Замер: `large_realloc`, `Vec::shrink_to_fit` 100→50 MiB, RSS.
- **H6 — over-aligned small (align ≥ 32 KiB без `medium-classes`) занимает отдельный ≥4 MiB сегмент**
  (`class_for → None`, `alloc_large`): на Windows это 4 MiB commit charge на аллокацию. Замер: N × 1 KiB
  с align 64 KiB, commit charge/RSS; возможное направление — aligned carve внутри small-сегмента.
- **H7 — fallback с материализованной directory не получает dirty-сигнал** (нет dirty-bitmap): удалённые
  free в его кольца видны лишь при периодическом full-scan (`DIRECTORY_MISS_FULL_SCAN_PERIOD`). Уже
  гипотеза 3 раунда-3; остаётся открытой.
- **H8 — `#[inline(always)]` больших cold-тел:** замер `cargo bloat` + iai до/после снятия атрибута.
- **H9 — `HeapOverflow` usize-курсоры с `wrapping_*`** против невращающихся u64 у `RemoteFreeRing` —
  гипотеза 1 раунда-3, не изменилась.

## 6. Готовность по feature surfaces

| Surface | Статический verdict этого прохода |
|---|---|
| `production` (`alloc-global + alloc-xthread + alloc-decommit + fastbin + alloc-segment-directory + primordial-lazy-commit + class-aware-dirty`), 64-bit Windows/Linux/macOS, корректный клиент | Новых P0–P2 нет. **GO с оговорками:** R1-01 (удержание RSS), R1-02 (fork), R1-03 (pool: поведение ≠ docs), R1-05/R1-06/R1-10 (диагностика/liveness). |
| `hardened` | То же, что `production`; gen-table-пути (`segment_header_gen_table.rs`) проверены только поиском. |
| `medium-classes` / `-wide` | R1-01 усиливается (+≈51 MiB/поток); promotion-пути (F7) прочитаны, дефекта нет. |
| Standalone `AllocCore` (`alloc-core`) | **CONDITIONAL:** R1-04 — `drop` требует ≳ 80 KiB стека. |
| `alloc-core + alloc-decommit + internals` без `alloc-xthread` | Собирается с 10 предупреждениями (R1-08). |
| `batch-api` | `alloc_batch`/`dealloc_batch` прочитаны; дефекта нет (кроме R1-01/R1-06 в fallback-ветке). |
| `experimental` / `pinning` (deprecated) | Выборочно; новых дефектов нет; не сертифицировано. |
| `numa-aware`, `*-lazy-commit`, `large-reserved-capacity`, `exact-span-large`, `virgin-zero-skip`, `large-cache-extended` | Прочитаны прямые пути; не сертифицировано. |

## 7. Примечание для следующего шага

По правилу проекта «Round start: check BOTH open-items indexes» новые пункты (R1-01…R1-11) следует
завести в `docs/CORRECTNESS_OPEN_ITEMS.md` (R1-01, R1-03 — возможно также в `docs/perf/OPEN_ITEMS.md`
как perf/RSS). В этом коммите этого сознательно не сделано: задача раунда ограничивает коммит одним
файлом отчёта.
