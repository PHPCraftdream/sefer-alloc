# B4-R1: TLS-кэш пинов маршрута — межпоточный free = 1 Acquire-load + 1 RMW

Статус: реализация R1 из `docs/perf/B1_MT_REGRESSION_ANALYSIS.md` §6.1. Ветка `b4-r1-pin-cache` (база main `24ef8be2`). Замеры НЕ выполнялись (запрещено задаче): iai A/B и mt larson/mstress — оркестратор. Этот документ — описание изменения, аргумент безопасности и доказательная база.

## 1. Что изменено

| Файл | Изменение |
|---|---|
| `src/registry/segment_route/directory.rs` | `Entry` (+поля `pub(super)`) получил атомарный флаг `linked: AtomicBool` (инициализируется `true` в `EntryHandle::new`); в `RouteDirectory::remove` под блокировкой шарда, до снятия с индекса и до возврата блокировки (то есть до дропа owner-ссылки в `RouteRegistration::drop`), флаг сбрасывается `linked.store(false, Release)` — единственное место сброса; `EntryHandle::entry()` стал `pub(super)`; удалён неиспользуемый `impl Default for RouteDirectory` (файл держится ≤ 1000 строк: ровно 1000); `#[allow(clippy::new_without_default)]` на `RouteDirectory::new`. |
| `src/registry/segment_route/pin.rs` | `RoutePin`: `linked()` (Acquire-загрузка флага), `span_contains_addr(addr)` (точная копия span-валидации `RouteDirectory::lookup` для одного адреса), `incarnation()` освобождён от cfg `internals` (чтение immutable-поля, нужно хот-пути), `publish_small_borrowed(&self, offset)` — заимствующий вариант публикации (пин НЕ потребляется; контракт Safety тот же: каждый вызов передаёт уникальное владение ОДНИМ текущим инстансом), `publish_small(self)` делегирует ему; doc-hidden тест-швы `dbg_cache_insert_for_test(self)` и `dbg_publish_cached_for_test(addr)` (делегируют в продакшн-пути кэша; образец — `RouteRegistration::class_at_global_address_for_test`), `dbg_route_pin_cache_totals()`. |
| `src/registry/heap_core_xthread/route_pin_cache.rs` (новый, 249 строк) | Per-thread TLS-кэш: 4 слота `{base, incarnation, Option<RoutePin>}`, round-robin victim; `publish_cached` (hot path), `cache_insert`, `cache_evict`; TLS-`Drop` отпускает все пины при выходе потока (через существующий протокол `EntryHandle::drop` — `refs.fetch_sub`, никакого нового refcount-кода); 6 счётчиков `PIN_CACHE_*` (Relaxed; инкременты компилируются только при `internals ∧ bench-internals` — в plain `production` хот-путь не платит ни бита). |
| `src/registry/heap_core_xthread/routing.rs` | `publish_foreign` = попытка `publish_cached` → при попадании общий хвост (счётчик dropped-free при `!published`, Miri-гейт `TerminalPublicationGate` при публикации); при промахе — `#[cold] publish_foreign_uncached` со старым телом: `lookup` → fallback-owner ветка без изменений → Large `publish_large` / Small `publish_small_borrowed` → `cache_insert` (только Small/Primordial и не fallback-owner); lookup-miss → `cache_evict(base)`. |
| Тесты/CI | `tests/b4_r1_pin_cache.rs` (5 тестов + counterfactual-мутанты), `tests/loom_b4_r1_pin_cache.rs` (4 теста), `tests/b4_r1_pin_cache_miri.rs` (1 тест); проводка: `scripts/loom.mjs`, `scripts/miri-matrix.mjs`, `.github/workflows/ci.yml` (loom-xthread: `--test` + 4 grep-сентинела; miri-plain: `--test b4_r1_pin_cache_miri`). |

## 2. Новый хот-путь

Было (на каждый foreign free): CAS+Release спин-блокировки шарда, 2 бинарных поиска, `refs.fetch_update(+1)`, ре-валидация, `refs.fetch_sub(−1)` — 3 разделяемых RMW, 4–5 общих линий.

Стало при попадании в кэш: `linked.load(Acquire)` (true) → сверка `incarnation`/`base`/span (plain-чтения immutable-полей той же линии) → класс-проверка (внутри `SidecarBitmap::publish`, без изменений) → **`pending[granule/64].fetch_or(AcqRel)` — единственный разделяемый RMW**. Блокировка шарда и оба поиска — не выполняются; `refs` +1/−1 — не выполняются (пин удерживается в кэше постоянно).

Промах кэша, `linked == false` (мгновенное освобождение мёртвого пина + `UNLINK_FALLBACKS`), несовпадение span/incarnation, недоступный TLS или реентерабельный borrow → старый путь `lookup` с заменой записи кэша (`cache_insert` перемещает пин в слот; вытесненный пин освобождается существующим протоколом refs; lookup-miss эвиктит слот по base). Fallback-owner маршруты в кэш не попадают никогда (горячий путь не выполняет fallback-ветку).

Выход потока: TLS-деструктор `Drop for PinCache` отпускает все удерживаемые пины. Кэш удерживает ТОЛЬКО Entry+sidecar (независимые System-аллокации, переживают release root — см. комментарий на foreign-пути `routing.rs`); reservation сегмента кэш не удерживает — unlink не смотрит на refs, release/recycle владельца идёт как раньше. Удержание ограничено: ≤ 4 × (Entry + SmallSidecar) на поток, отпускается при evict/unlink-fallback/выходе потока.

## 3. Почему безопасно (полный аргумент)

1. **Публикация не попадает в освобождённую память.** Sidecar/Entry освобождаются только при `refs == 0` (`EntryHandle::drop` → `fetch_sub(AcqRel)`; финальный счётчик). Пин кэша держит `refs ≥ 1` от момента `lookup` (insert) до evict/выхода потока, и публикация выполняется под этим пином ⇒ набор бита после освобождения исключён. Никакого нового refcount-протокола нет — только существующий.
2. **Валидный free не теряется.** По контракту терминальной публикации (§2) unpublished/published producer держит сегментный credit (`live_count`): сегмент не может пройти финализацию/recycle, пока его блок не отбит владельцем. Значит, к моменту валидного `dealloc` маршрут блока связан (`linked == true`) и unlink не может завершиться до снятия credit'а. Публикация через кэш попадает в живой, связанный sidecar, который владелец дренирует штатно (drain идёт по segment-table route slots и не зависит от directory-пинов).
3. **Окно «stale-true» честно описано.** Coherence-легально продюсер может прочитать `linked == true` для ещё не ставшего видимым unlink'а и опубликовать в уже отвязанный, но живой sidecar (refs пина): если владелец успел выполнить финальный drain до RMW, бит умирает вместе с sidecar'ом. Это окно ТОЖДЕСТВЕННО окну старого пути «между `lookup` (пин под блокировкой) и publish» и для валидных free закрыто ledger-гипотезой (п.2), а не атомиками; для невалидных указателей поведение не изменилось (dropped-free счётчик). Loom-модель документирует этот скоуп.
4. **`linked == false` → фолбэк, никогда не публикация в мёртвый маршрут.** Флаг сбрасывается `Release` под блокировкой шарда до unlink/owner-drop и никогда не возвращается в `true`; Acquire-загрузка на хот-пути: при `false` пин немедленно извлекается (refs освобождаются) и выполняется свежий `lookup`, который под блокировкой шарда видит актуальный индекс (старый маршрут удалён; повторная регистрация того же base — новый `Entry` с новой incarnation и новым обнулённым sidecar). Сверки `incarnation` и base в кэше — defense-in-depth (поле immutable, сравнение фиксирует подмену Entry по тому же адресу, что исключено удержанием refs).
5. **Гарантии ADR и контракта не ослаблены.** G1′: терминальная публикация без аллокации — `publish_cached`/`cache_insert`/`cache_evict` не аллоцируют (слоты предвыделены в `const`-TLS), пути lookup/registration не тронуты. C6/OOM: не затронуты. Exactly-once публикации: как и раньше, обеспечивает уникальная передача владения инстансом вызывающим (`# Safety` `publish_small_borrowed`); класс-кодирование (encoded 0 → reject) и hardened-проверки own-пути не менялись. Miri-гейт `TerminalPublicationGate` вызывается в общем хвосте после успешной публикации обеих ног. Ожидаемо-красные P1-box Miri-шаги не затронуты. Публичный API не менялся: `RoutePin` живёт за `#[doc(hidden)] pub mod segment_route`, новые методы — internals-поверхность (ADR: «поверхность за #[doc(hidden)] и internals менять можно»).
6. **Потокобезопасность TLS.** Кэш — `RefCell` под `try_with`/`try_borrow_mut`: недоступный TLS или реентрабельный borrow (сам публикационный путь не создаёт реентерабельности: publication не аллоцирует, не берёт локов, не заходит в TLS) деградируют в `Miss` без паники; fallback-owner ветка выполняется вне заимствования кэша. Деструктор потока не берёт локов и использует `System`.

## 4. Доказательная база (тесты, модели, counterfactual)

- `tests/b4_r1_pin_cache.rs` (354 строки; `production internals bench-internals` и `alloc-global alloc-xthread internals bench-internals`): попадание/промах кэша со счётчиками; unlink при закэшированном пине (пин мгновенно освобождён: `dbg_route_pin_refs == 1`; публикация в мёртвый sidecar отсутствует); повторная регистрация после unlink обслуживается свежим маршрутом (публикация в НОВЫЙ sidecar, zombie чист); выход 8 потоков отпускает пины (`THREAD_RELEASES`); 4 потока × ~8 тыс. foreign free через реальный `#[global_allocator]` без единого `foreign_or_unroutable_frees` и с балансом пинов. Counterfactual (запущены, каждый раз краснел на указанном ассерте, затем откат и зелёный прогон):
  - мутант «пропустить проверку linked» → RED `unlinked_cached_pin_is_released_immediately_and_never_publishes` («one unlink fallback»);
  - мутант «кэш отключён» → RED `cached_pin_publishes_and_counts` («cached pin must publish and count a hit») + `many_threads_many_segments_foreign_frees_balance` («same-base repeats must hit the cache»);
  - мутант «не отпускать мёртвый пин» → RED `unlinked_cached_pin_is_released_immediately_and_never_publishes` («cached dead pin was released immediately … left: 2, right: 1»).
- `tests/loom_b4_r1_pin_cache.rs` (340 строк; CI loom-xthread, локально не запускался — запрещено). Теневая модель протокола по рецензированному образцу `loom_route_pin_unlink.rs` (actual-source включение для этой поверхности неоправданно: `directory.rs` тянет System/шард-механику/TLS/`fatal`/`crate::global` — причина записана в доке модели). Свойства: (A1) публикация никогда не попадает в освобождённый sidecar (refs); (A2) публикация через закэшированный пин возможна только после завершённой gate-загрузки, вернувшей true; (A3) ложная gate-загрузка принуждает путь свежего lookup, наблюдающий пост-unlink состояние индекса. Негативные контроли: `negative_skip_gate_publishes_into_unlinked_route` (пропуск проверки — красный) и `negative_relaxed_gate_loses_unlink_witness` (Relaxed вместо Acquire — красный: ложное чтение gate не обязано нести witness снятия с индекса; под Acquire это чередование исключено synchronizes-with). Скоуп-примечание (ledger-гипотеза вне атомиков) — в доке модели, §3 п. 3 этого отчёта.
- `tests/b4_r1_pin_cache_miri.rs` (164 строки; CI miri-plain, plain provenance + SB + preemption 0.5): промах → попадание по тому же base через закэшированный пин под Miri, удержание пина при живом маршруте и освобождение при выходе потока; дельта `foreign_or_unroutable_frees == 0`. Провода: `scripts/miri-matrix.mjs` (MIRI_TARGETS), ci.yml miri-plain.
- Проверки проводки: `verify-loom-target-wiring.mjs`, `verify-miri-target-wiring.mjs`, `verify-structure-limits.mjs`, `verify-dbg-hook-safety.mjs` — зелёные (см. §6).
- Активационный оракул пути (R30-8): `PIN_CACHE_HITS`/`PIN_CACHE_MISSES` (bench-internals, читаются `RoutePin::dbg_route_pin_cache_totals()`). Правило приёмки A/B: на MT-плече с повторными free по тем же сегментам доля `hits/(hits+misses)` обязана быть ≥ 95 %, иначе заявленная экономия не приписывается кэшу; `UNLINK_FALLBACKS`/`EVICTIONS` показывают цену вытеснения при > 4 активных чужих сегментах на поток.

## 5. Что должно ускориться и метрика приёмки

- MT (larson/mstress, T=4..8, реальный `#[global_allocator]`): каждый foreign free теряет спин-блокировку шарда (конвой §4.3 B1), оба поиска и пару refs-RMW → 1 Acquire-load + 1 RMW + 2 зависимые Acquire-загрузки класс-листа (без изменений). Ожидание по B1 §7.4: паритет с mimalloc вероятен после R2; измеряет оркестратор. Гейты ADR: geomean ≥ 0.95, каждая T ≥ 0.90.
- iai foreign-free плечо (новое — вводится отдельной задачей оркестратора): Ir на foreign free должно упасть на ~2 RMW + блокировку + поиски; локальный Windows A/B — larson/mstress.
- Однопоточные own-пути не затронуты (код changed только на foreign-ноге и unlink'е — в `remove` добавлена одна Release-стор под уже удерживаемой блокировкой).

## 6. Что запущено локально

| Команда | Код возврата | Результат |
|---|---|---|
| `node scripts/verify-dbg-hook-safety.mjs` | 0 | PASS: 250 reviewed safe, 34 reviewed unsafe, 126 genuinely bench-gated safe hooks; новые хуки признаны безопасными. |
| `cargo clippy --all-targets -- -D warnings` (CARGO_TARGET_DIR=.rush/cargo-target-B) | 0 | Чисто. |
| `cargo clippy --features "experimental" --all-targets -- -D warnings` | 0 | Чисто. |
| `cargo clippy --all-features --all-targets -- -D warnings` | 0 | Чисто (после замены `i1 - e1 - r1 >= 0` на `i1 >= e1 + r1` и эквивалентной перестановки без вычитания во втором assert, а также замены `for index in 0..workers` на `zip` в `many_threads_many_segments_foreign_frees_balance`). |
| `cargo clippy --features "hardened medium-classes internals" --all-targets -- -D warnings` | 0 | Чисто. |
| `cargo clippy --features "production" --lib -- -D warnings` | 0 | Чисто. |
| Группа P как задано: `cargo test --features "production internals bench-internals" --test r6_route_directory … --test b4_r1_pin_cache` (24 цели) | 101 | FAIL на этапе разрешения целей: `error: target r6_t50_foreign_realloc_committed_span in package sefer-alloc requires the features: alloc-global, alloc-xthread, internals, large-reserved-capacity`. Ни один тест не запускался. |
| Группа P информационно (+ `alloc-global alloc-xthread large-reserved-capacity`) | 0 | 25 целей: все `test result: ok`, 0 failed; 4 цели с 0 тестов (`#![cfg]`-гейты): `r6_route_directory_global`, `r6_u21_foreign_free_interior_granule`, `r6_terminal_header_atomic`, `r19_large_cache_decay_extended`. Итого 62 passed. |
| Группа H как задано: `cargo test --features "production hardened internals bench-internals" --test …` (24 цели) | 101 | Та же ошибка разрешения цели `r6_t50_foreign_realloc_committed_span` (те же требуемые фичи). |
| Группа H информационно (+ `alloc-global alloc-xthread large-reserved-capacity`) | 0 | 25 целей: все ok, 0 failed; те же 4 цели с 0 тестов. |
| Группа X: `cargo test --features "alloc-global alloc-xthread internals bench-internals" --test b4_r1_pin_cache --test con01_exit_stranded_lock --test r3_1_fallback_remote_free --test regression_r5_01_alloc_global_cross_thread --test regression_xthread_large_free_no_leak --test regression_xthread_thread_free_alias_miri --test regression_xthread_small_ring_miri` | 0 | 7 целей: все ok, 0 failed; 2 цели с 0 тестов (`regression_xthread_thread_free_alias_miri`, `regression_xthread_small_ring_miri` — miri-гейты). Итого 20 passed. |
| `cargo test --release --features "production internals bench-internals" --test b4_r1_pin_cache` | 0 | `5 passed; 0 failed`. |
| `node scripts/verify-structure-limits.mjs` | 0 | OK: self-test 21 cases; 9 directories and 28 files grandfathered, none new, none grown. |
| `node scripts/verify-loom-target-wiring.mjs` | 0 | OK: 20 root Loom targets wired in CI and local matrix. |
| `node scripts/verify-miri-target-wiring.mjs` | 0 | OK: mode manifest MIRI_TARGETS, inventory=13, entries=30, CI entries=28. |
| `wc -l` (7 файлов из п. 6) | 0 | directory.rs 1000, pin.rs 148, route_pin_cache.rs 249, routing.rs 102, b4_r1_pin_cache.rs 354, loom_b4_r1_pin_cache.rs 340, b4_r1_pin_cache_miri.rs 164. |

## 7. Что должно быть прогнано на CI

- `loom-xthread`: `loom_b4_r1_pin_cache` (4 теста, 2 из них `should_panic`).
- `miri-plain`: `b4_r1_pin_cache_miri`.
- Существующие строки матрицы (clippy/test-группы, `r6_route*`, xthread-регрессии) — уже покрыты; iai A/B + mt larson/mstress + локальный Windows A/B — оркестратор (в замерах я не участвовал).

## 8. Риски

- **Thrash при > 4 чужих сегментах на поток**: вытеснения вернут блокировку+поиски; счётчики `MISSES`/`EVICTIONS` сделают это видимым в A/B. Подъём N — одна константа `PIN_CACHE_SLOTS`.
- **Удержание ≤ 4 × (Entry+SmallSidecar) на поток** после unlink — ограничено и освобождается при первой попытке публикации по этому base, evict или выходе потока; reservation не удерживается.
- **TLS-деструктор при выходе потока** делает `refs.fetch_sub` и возможный `System.dealloc` — вне SeferAlloc, без локов; при резком завершении процесса деструкторы могут не выполниться (утечка до смерти процесса — как любые TLS-пины).
- **Две разделяемые линии остались**: класс-лист (e8843374) и линия `pending` — R1 их не трогает (это R3/R7).
- **Публичная поверхность**: `RoutePin` получил новые `pub`-методы в doc-hidden модуле; стабильный API не задет (версия не поднимается).

Префикс коммита: `perf(runtime)`.

Измерение производительности в этой задаче не выполнялось (запрещено); все заявления об ускорении — гипотезы по структуре пути (B1 §3.1/§6.1), подлежащие iai/mt-замерам оркестратора.
