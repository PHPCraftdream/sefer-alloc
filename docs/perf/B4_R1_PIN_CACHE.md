# B4-R1: TLS-кэш пинов маршрута — межпоточный free = 1 Acquire-load + 1 RMW (раунд 2)

Статус: **раунд 2**. Реализация R1 из `docs/perf/B1_MT_REGRESSION_ANALYSIS.md` §6.1; отчёт закрывает замечания ревью раунда 1 (soundness тест-хука, loom-модель по теневому образцу вместо production-кода, узкий Miri, утечки/инвентарь/прочее — все P1/P2/P3 закрыты, см. §2). Замеры НЕ выполнялись (запрещено задаче): iai A/B и mt larson/mstress — оркестратор. Промежуточный замер оркестратора **до** раунда 2 (на коде раунда 1): Windows A/B, larson/mstress — **+5…22 % на MT-плече, 26 прогонов без коллапса** (OBSERVED оркестратором, не в этой задаче).

## 1. Статус

- Код раунда 1 сохранён: per-thread TLS-кэш 4 пинов (Small/Primordial), foreign free при попадании = `linked.load(Acquire)` + сверка immutable-полей + один `fetch_or` бита sidecar'а.
- Раунд 2 не меняет хот-путь; все изменения — тест-поверхность, модели, конфигурация и инвентарь (см. §2).
- Приёмка производительности — на оркестраторе (iai A/B + mt larson/mstress).

## 2. Что изменено в раунде 2 (по пунктам ревью; file:line проверены чтением)

### P1-soundness: `dbg_publish_cached_for_test` — честный unsafe-хук

- `src/registry/segment_route/pin.rs:147` — `pub unsafe fn dbg_publish_cached_for_test(address: usize) -> bool` с контрактом `# Safety` (`pin.rs:135-146`, `#[cfg(all(feature = "internals", feature = "bench-internals"))]` на `pin.rs:144`, `doc(hidden)`, item-level `#[allow(unsafe_code)]` на `pin.rs:146`): `address` обязан быть ТЕКУЩИМ выданным Small/Primordial инстансом вызывающего; публикация **потребляет владение** один раз; голый адрес владение не устанавливает → публикация чужого живого адреса = владелец реклеймит живой инстанс = **UAF по построению**. Аналогия с `class_at_global_address_for_test` удалена из комментария (тот хук только читает класс и не создаёт reclaim-обязательства).
- `scripts/verify-dbg-hook-safety.mjs`: явные строки `REVIEWED_SURFACE` — `dbg_publish_cached_for_test` (kind `unsafe`, гейты internals + bench-internals, `:424`), `dbg_cache_insert_for_test` (mutator: «inserts caller-looked-up pin; no publish/reclaim obligation», `:425`), `dbg_route_pin_cache_totals` (observer, `:426`); регистрация в `BENCH_UNSAFE_HOOKS` строкой `:388`.
  - **Особенность (честно)**: id НЕ добавлен в `UNSAFE_HOOKS` — верификатор отвергает дубли (`UNSAFE_HOOKS` ∩ REVIEWED_SURFACE-unsafe, комментарий `:384-385`); регистрация kind-unsafe строкой из REVIEWED_SURFACE СТРОЖЕ: проверяет оба гейта + наличие `# Safety`.
  - Прогон (OBSERVED): `node scripts/verify-dbg-hook-safety.mjs` → `PASS: 252 reviewed safe, 35 reviewed unsafe, 125 genuinely bench-gated safe hooks`, EXIT 0.
- `tests/b4_r1_pin_cache.rs` переписан (715 строк, 6 тестов):
  - hit/miss через **реальный foreign dealloc** (`SeferAlloc` как `#[global_allocator]`, `expose_provenance` между потоками): `foreign_free_real_path_hit_miss_two_bases` (`:243`);
  - delivery + reclaim (32 блока: 1 miss + 31 hit; owner-drain → `pending_at == false`; баланс `SmallSidecar::system_totals_for_test()`): `foreign_free_delivery_reclaim_and_totals` (`:296`);
  - zombie/unlink fixture-тесты через unsafe-хук с явным SAFETY-обоснованием (fixture владеет всей резервацией; реального аллокационного инстанса по адресу не существует): `unlinked_cached_pin_never_publishes_and_releases` (`:361`), `re_registered_route_is_served_after_cache_fallback` (`:416`);
  - thread-exit с точными refs-оракулами (`RouteDirectory::dbg_route_pin_refs`) и totals: `thread_exit_releases_cached_pins_and_balances` (`:488`);
  - stress 4×(4096+4096) с полным учётом: `stress_foreign_frees_full_accounting` (`:603`).
  - **Честные пределы**: `system_totals` процесс-глобальный → баланс проверяется как bounded (±4096 пар / ±32 МиБ; stress ±8192) — ловит массовые утечки, не одиночную пару; TLS-деструкторы Windows асинхронны (~300 мс) → bounded-ожидания с таймаутами; `THREAD_RELEASES ≥ N*4`, а не N*M (4-слотовый direct-mapped кэш); потеря бита без `false`-возврата publish нативно неотличима — закрывает Loom O1.
- `src/registry/heap_core_xthread/route_pin_cache.rs`: module-level `#![allow(unsafe_code)]` **удалён**, остался item-level на `publish_cached` (`:135`); инкремент `PIN_CACHE_THREAD_RELEASES` в `Drop` под `#[cfg(all(feature = "internals", feature = "bench-internals"))]` (`:60-64`) — production Drop ничего не делает (закрывает P3 «teardown-инкремент без cfg»).

### P1-Loom: actual-source модель

- `tests/loom_b4_r1_pin_cache.rs` переписан (737 строк): `#[path]`-включение ПРОДАКШН-файлов (`:381-427`): `route_pin_cache.rs`, `pin.rs`, `directory.rs`, `registration.rs`, `small_sidecar.rs`, `remote_bitmap/{sidecar_bitmap,bitmap_scan,bitmap_cut,bitmap_record}`, `error/kind/route_record/route_cut/route_scan/large_state` (образец — `tests/loom_sidecar_bitmap_actual.rs`).
- Шимы модели: `fatal`; `os::SEGMENT` = реальные 4 MiB (`:185`); size_classes реальные; `AllocCore::large_payload_address_matches` compile-only (`:252` — Large не исполняется); `process_exit_in_progress` = `const false` (`:264`); `record_exit_lock_giveup` = no-op (`:268`); `ShardLock` — loom-yield спин-шим (`:290`) — **замена только примитива блокировки**; тело production `RouteDirectory::remove` (unlink + `linked.store(false, Release)` под блокировкой, `directory.rs:980-983`) исполняется дословно.
- **Урок (инцидент, честно)**: промежуточный вариант с `#[cfg(loom)]`-уменьшенной геометрией в реальном `small_sidecar.rs` ломал lib-сборку под `--cfg loom` с `alloc-global` (E0080: `assert!(SEGMENT == 4*64*MIN_BLOCK)`; `segment_route` под alloc-core-фичами не компилируется, поэтому loom-check воркера это не ловил) — обнаружено верификацией оркестратора. Устранено: cfg-блоков в src больше НЕТ, модель работает на полной геометрии.
- Атомик-швы (импорт-уровень): `segment_route/mod.rs` реэкспортирует core-атомики (`AtomicBool/AtomicU64/AtomicUsize/Ordering`); `directory.rs` — line-neutral (протокольные `linked`/`refs` идут через `super` → loom; `AtomicPtr`/часть `AtomicU64`/`Ordering` остаются core: loom-атомики без const-конструкторов, а `RouteDirectory::new` — `const fn`); `pin.rs` — `linked()` через `super::Ordering`; `small_sidecar.rs` — два импорта.
- Оракулы (проверены чтением модели): O1 полнота доставки (`drained == published_true`, `:623-628`); O2 — никакой публикации happens-after unlink (`:633-638`, зуб против M-A); O3 — третий дренаж пуст (`:640-642`); O4 — refs-инварианты (≥ 2 при живых reg+пинах, `:545-547`; ровно 1 — дренажная ручка после join, `:615-617`).
- Мутант-рецепты в шапке модели (`:126-146`): M-A (`route_pin_cache.rs::publish_cached` — удалить ветку linked) краснит O2 + имеет shadow-контроль `loom_actual_negative_skip_linked_gate_writes_zombie_bit` (`:661`); M-B (`pin.rs::linked()` Acquire→Relaxed) — **честно**: когерентность допускает устаревшее чтение `true` и под Acquire; в этой модели такой бит восстанавливается финальным дренажем (sidecar удержан пинами), поэтому красная различимость M-B в ЭТОЙ модели не гарантирована — механизм (relaxed-гейт теряет witness unlink) демонстрируется `#[should_panic]` shadow-контролем `loom_actual_negative_relaxed_gate_loses_unlink_witness` (`:704`; прецедент — negative controls в `loom_sidecar_bitmap_actual.rs`).
- Резидуал-окно устаревшего гейта для ВАЛИДНЫХ free в проде закрыто credit-ledger-гипотезой (контракт терминальной публикации §2) — **вне атомиков, вне loom**; указано явно в §4 п. 3.
- Пин через весь RMW публикации — структурно (production `publish_cached` держит borrow пина через `publish_small_borrowed`; модель не разрывает); память-уровень — Miri-клетка. Drain при финальном owner-ref drop в модели НЕ добавлен (в production `EntryHandle::drop` его нет — прошлой модели это ставили в вину).
- CI: джоба `loom-xthread` (`ci.yml:3386`), 3 теста b4-модели — 1 позитив (`loom_actual_cached_pin_publish_delivery_survives_unlink_race`, sentinel `ci.yml:3422`) + 2 `should_panic` (`ci.yml:3423-3424`); сентинелы ci.yml обновлены; `scripts/loom.mjs` не менялся (цель/фичи прежние, `loom_b4_r1_pin_cache: 'alloc-core,alloc-xthread,tagged-index-stack/loom'`, `loom.mjs:46`). `node scripts/verify-loom-target-wiring.mjs` → «OK: 20 root Loom targets», EXIT 0 (OBSERVED).
- Локально loom НЕ гонялся (запрещено); компиляция OBSERVED: `RUSTFLAGS="--cfg loom" cargo check --features "alloc-core,alloc-xthread,tagged-index-stack/loom" --test loom_b4_r1_pin_cache` EXIT 0 и критический `RUSTFLAGS="--cfg loom" cargo check --features "alloc-global,alloc-xthread,tagged-index-stack/loom" --lib` EXIT 0.

### P2-Miri: narrow provenance

- `tests/b4_r1_pin_cache_miri.rs` переписан (291 строка): РЕАЛЬНЫЕ `Box<Block>` (256B), владение чужому потоку через `ForeignBlock` Send-обёртку (образец `tests/support/r8_global_box_witness.rs`), узкие reborrows `&mut Block` до move; frees через `#[global_allocator]`: miss+insert → тот же base hit → другой base miss+insert. Оракул активации R30-8: дельта `PIN_CACHE_HITS` ≥ 1 (мутант «publish_cached всегда Miss» оставляет HITS плоским → red), inserts ≥ 2, `PIN_CACHE_THREAD_RELEASES` ≥ 2, `dbg_foreign_or_unroutable_frees() == 0`; доставка через `TerminalPublicationGate` (arm → production `pause_after_publication`, `routing.rs:42` → `pending_at(first) == true`) → owner-drain (`trim_current_thread` + own alloc/free циклы) → `pending == false`. Ложное заявление о конкурентных owner-аллокациях **удалено** (график детерминированный, конкурентность не заявляется).
- Матрица `scripts/miri-matrix.mjs`: plain-строка сохранена (`plainFeatures`, модель plain) + узкие sb и tb (`miri-core`, features `alloc-global internals bench-internals`, `args: ['--','narrow']`, маркер `test narrow_pin_cache_hit_release_and_delivery_miri ... ok`, `miri-matrix.mjs:43-51`). `ci.yml`: два шага в job miri-core по образцу narrow-шагов `miri_global_box_acceptance` (`ci.yml:3058`, `ci.yml:3066`; MIRIFLAGS `-Zmiri-strict-provenance -Zmiri-disable-isolation [-Zmiri-tree-borrows] -Zmiri-report-progress=10000000`, `set -o pipefail`, tee+grep маркера).
- `node scripts/verify-miri-target-wiring.mjs` → inventory=13, entries=32, CI=30, EXIT 0 (OBSERVED). Локально miri НЕ гонялся; cargo check/clippy двух фичекомбо EXIT 0 (см. §7).

### P2-утечки

Закрыты тестом `tests/b4_r1_pin_cache.rs` (см. P1-soundness выше): точные refs-оракулы, system_totals с честными ограничениями, drain → `pending == false`, `foreign_or_unroutable_frees == 0`, точный/ограниченный баланс пинов с обоснованием пределов.

### P2-инвентарь

- module-level allow в `route_pin_cache.rs` удалён → новых tier-1 seam'ов НЕТ; `src/lib.rs` НЕ менялся; `tests/no_stale_doc_references.rs` — 32/32 passed EXIT 0 (включая `lib_rs_seam_inventory_matches_canonical_grep`).
- `README.md`: tier-2 item-scoped unsafe 97→**101** сайтов, 31→**32** файлов; построчно (проверены): `segment_route/pin.rs` 2→4 (`README.md:767`), `heap_core_xthread/routing.rs` 2→3 (`:791`), `global/sefer_alloc/diag.rs` 6→9 (`:794`), + новая строка `route_pin_cache.rs` 1 (`:792`); тесты 424→427 файлов (`:1422`, `:1432`), loom-модели → 20 root (`:57`, список `:1436`). `docs/ARCHITECTURE.md`: 424→427 (`:496`) — счётчики пинят тесты `architecture_test_file_count_matches_reality`/`verification_inventory_matches_docs`: без правки tripwire красный.
- Итоговый tier-2 инвентарь (пересчитан сейчас): канонический `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/` = 128 совпадений в 423 файлах, из них module-level (`#![…]`) — 27; item-scoped (`#[…]`) = **101 сайт / 32 файла** (OBSERVED, совпадает с README).

### P3-прочее

- `src/registry/segment_route/directory.rs`: `impl Default for RouteDirectory` ВОССТАНОВЛЕН (`:996`, конец файла); `#[allow(clippy::new_without_default)]` удалён; файл ровно **1000** строк (`wc -l`; `node scripts/verify-structure-limits.mjs` EXIT 0); комментарий в `remove()` исправлен (`:980-983`): unlink (`guard.remove`) идёт первым под блокировкой шарда, Release-стор `linked=false` публикует unlink, Acquire-читатель `linked==false` гарантированно видит unlink, owner-ссылка дропается после снятия блокировки.

## 3. Новый хот-путь (без изменений с раунда 1)

Было (каждый foreign free): CAS+Release спин-блокировки шарда, 2 бинарных поиска, `refs.fetch_update(+1)`, ре-валидация, `refs.fetch_sub(−1)` — 3 разделяемых RMW, 4–5 общих линий.

Стало при попадании в кэш: `linked.load(Acquire)` (true) → сверка `incarnation`/`base`/span (plain-чтения immutable-полей той же линии) → класс-проверка (внутри `SidecarBitmap::publish`, без изменений) → **`pending[granule/64].fetch_or(AcqRel)` — единственный разделяемый RMW**. Блокировка шарда и оба поиска не выполняются; `refs` +1/−1 не выполняются (пин удерживается в кэше постоянно).

Промах кэша, `linked == false` (мгновенное освобождение мёртвого пина + `UNLINK_FALLBACKS`), несовпадение span/incarnation, недоступный TLS или реентерабельный borrow → старый путь `lookup` с заменой записи кэша (`cache_insert` перемещает пин в слот; вытесненный пин освобождается существующим протоколом refs; lookup-miss эвиктит слот по base). Fallback-owner маршруты в кэш не попадают никогда.

Выход потока: TLS-деструктор `Drop for PinCache` отпускает все удерживаемые пины (существующий протокол `EntryHandle::drop`). Кэш удерживает ТОЛЬКО Entry+sidecar (независимые System-аллокации); reservation сегмента кэш не удерживает. Удержание ограничено: ≤ 4 × (Entry + SmallSidecar) на поток.

## 4. Почему безопасно — полный аргумент

1. **Публикация не попадает в освобождённую память.** Sidecar/Entry освобождаются только при `refs == 0` (`EntryHandle::drop` → `fetch_sub(AcqRel)`). Пин кэша держит `refs ≥ 1` от момента `lookup` до evict/unlink-fallback/выхода потока, и публикация выполняется под этим пином (borrow пина жив через весь RMW — структурно: `publish_cached` → `publish_small_borrowed(&self, …)`) ⇒ набор бита после освобождения исключён. Память-уровень — Miri-клетка (§5). Никакого нового refcount-протокола нет.
2. **Порядок unlink/linked-store (исправленная формулировка раунда 2).** В `RouteDirectory::remove` (`directory.rs:980-983`) под удерживаемой блокировкой шарда сначала выполняется unlink (`guard.remove(key, …)` — маршрут удалён из индекса), затем Release-стор `linked = false` **публикует** unlink; Acquire-читатель, увидевший `linked == false`, гарантированно видит и пост-unlink состояние индекса; owner-ссылка дропается только после снятия блокировки. Флаг никогда не возвращается в `true`; повторная регистрация того же base — новый `Entry` с новой incarnation и обнулённым sidecar.
3. **Окно stale-гейта — честно.** Coherence-легально продюсер может прочитать `linked == true` для ещё не ставшего видимым unlink'а и опубликовать в уже отвязанный, но живой sidecar (refs пина): если владелец успел выполнить финальный drain до RMW, бит умирает вместе с sidecar'ом. Это окно ТОЖДЕСТВЕННО окну старого пути («между `lookup` и publish») и для валидных free закрыто **credit-ledger-гипотезой** (контракт терминальной публикации: unpublished/published producer держит сегментный credit `live_count`, сегмент не может пройти финализацию/recycle, пока его блок не отбит владельцем) — вне атомиков, вне loom; явно указано в модели и здесь. Для невалидных указателей поведение не изменилось (dropped-free счётчик).
4. **`linked == false` → фолбэк, никогда не публикация в мёртвый маршрут.** Acquire-загрузка на хот-пути; при `false` пин немедленно извлекается и выполняется свежий `lookup` под блокировкой шарда. Loom O2 — зуб против мутанта M-A «убрать проверку linked» (CI-pending, §5). Различимость M-B (Acquire→Relaxed) в модели не гарантирована (когерентность допускает устаревшее `true` и под Acquire; бит восстанавливается финальным дренажем) — механизм демонстрируется `#[should_panic]` shadow-контролем; для валидных free residual закрыт п. 3.
5. **Сверки `incarnation`/base** — defense-in-depth: поля immutable, сравнение фиксирует подмену Entry по тому же адресу, что исключено удержанием refs.
6. **Гарантии ADR и контракта не ослаблены.** G1′: терминальная публикация без аллокации (кэш не аллоцирует; слоты предвыделены в `const`-TLS). C6/OOM не затронуты. Exactly-once обеспечивает уникальная передача владения вызывающим (`# Safety` `publish_small_borrowed`); класс-кодирование и hardened-проверки own-пути не менялись. `TerminalPublicationGate` — в общем хвосте после успешной публикации обеих ног. Публичный API не менялся.
7. **Потокобезопасность TLS.** `RefCell` под `try_with`/`try_borrow_mut`: недоступный TLS/реентерабельный borrow → `Miss` без паники; production Drop не делает ничего, кроме отпускания пинов (счётчик — только под internals ∧ bench-internals).

## 5. Доказательная база (тесты, модели, клетки; для каждой — counterfactual)

| Артефакт | Что проверяет | Counterfactual (что выключить → что краснеет) | Статус |
|---|---|---|---|
| `tests/b4_r1_pin_cache.rs` (715 строк, 6 тестов) | hit/miss через реальный foreign dealloc; delivery+reclaim (32 блока: 1 miss + 31 hit, drain → `pending==false`, totals-баланс); zombie/unlink fixture через unsafe-хук; повторная регистрация; thread-exit (точные refs `dbg_route_pin_refs`); stress 4×(4096+4096) | Мутант «publish_cached всегда Miss» → RED по `PIN_CACHE_HITS`; «пропустить проверку linked» → RED `unlinked_cached_pin_never_publishes_and_releases`; «не отпускать мёртвый пин» → RED по refs-оракулу; «кэш не вставляет» → RED по miss-счётчику/балансу | **OBSERVED**: `cargo test --features "production internals bench-internals" --test b4_r1_pin_cache` → 6 passed, EXIT 0 (debug и release, §7) |
| `tests/loom_b4_r1_pin_cache.rs` (737 строк, actual-source) | O1 delivery (drained == published_true); O2 нет публикации после unlink (зуб M-A); O3 третий дренаж пуст; O4 refs-инварианты (≥ 2; ровно 1 после join) | M-A «удалить ветку linked в `publish_cached`» → RED O2; shadow-контроли `#[should_panic]`: `loom_actual_negative_skip_linked_gate_writes_zombie_bit`, `loom_actual_negative_relaxed_gate_loses_unlink_witness` (M-B-механизм; различимость M-B в модели честно не гарантирована) | **CI-pending** (loom-xthread, `ci.yml:3422-3424`); локально запрещено; компиляция обоих фичекомбо EXIT 0 (§7) |
| `tests/b4_r1_pin_cache_miri.rs` (291 строка, narrow) | provenance/reborrows под strict-provenance (SB и TB), реальные `Box<Block>` + Send-обёртка; HITS ≥ 1 (активационный оракул R30-8), inserts ≥ 2, RELEASES ≥ 2, foreign == 0; доставка: arm → `pending_at(first)==true` → owner-drain → false | Мутант «publish_cached всегда Miss» → HITS плоский → red; повреждение narrow-provenance → Miri UB-репорт | **CI-pending** (miri-core narrow sb/tb + miri-plain); локально запрещено; cargo check/clippy EXIT 0 (§7) |
| `node scripts/verify-dbg-hook-safety.mjs` | surface/gates/`# Safety` unsafe-хуков | Убрать гейт/`# Safety`/строку REVIEWED_SURFACE → verifier red | **OBSERVED**: PASS 252 safe / 35 unsafe / 125 bench-gated, EXIT 0 |
| `node scripts/verify-loom-target-wiring.mjs` / `verify-miri-target-wiring.mjs` / `verify-structure-limits.mjs` / `verify-ci-sentinels.mjs` | проводка целей/матриц/лимитов/сентинелов | рассинхрон → red | **OBSERVED**: 20 целей / 13-32-30 / EXIT 0 / «160 OK» (WARNING по floor 141 — pre-existing дрейф, см. §7) |
| `tests/no_stale_doc_references.rs` | инвентарь seam'ов `src/lib.rs` | расхождение → red | **OBSERVED**: 32/32 passed, EXIT 0 |

Ограничения теста (честно): system_totals процесс-глобальный → bounded slack (±4096 пар / ±32 МиБ; stress ±8192) — ловит массовые утечки, не одиночную пару; TLS-деструкторы Windows асинхронны (~300 мс) → bounded-ожидания с таймаутами; `THREAD_RELEASES ≥ N*4`, а не N*M (4-слотовый direct-mapped кэш); потеря бита без `false`-возврата publish нативно неотличима — закрывает Loom O1.

## 6. Что НЕ входит в доказательство

- **Credit-ledger-гипотеза** (закрытие residual-окна stale-гейта для валидных free) — контрактная, вне атомиков, вне loom.
- **System-UAF** (уровень памяти: освобождение sidecar/Entry при refs==0) — покрывается Miri-клеткой (CI-pending), не loom.
- **Различимость M-B в loom-модели** — не гарантирована (когерентность допускает устаревшее `true` и под Acquire; бит восстанавливается дренажем); механизм закрывается shadow-контролем `#[should_panic]`.
- **Границы system_totals** — процесс-глобальный счётчик: ловит массовые, не одиночные утечки.
- **Preemption-покрытие** Miri-клетки — детерминированный график, конкурентность owner-аллокаций не заявляется (ложное заявление раунда 1 удалено).
- **Сегментные credit'ы** не наблюдаются тестами напрямую — аргумент контрактный (§4 п. 3).

## 7. Таблица локальных прогонов (все OBSERVED, CARGO_TARGET_DIR=D:/dev/rust/sefer-alloc/.rush/cargo-target-B)

| Команда | EXIT |
|---|---|
| `cargo test --features "alloc-global alloc-xthread internals bench-internals"` — 19 целей (b4_r1_pin_cache, b4_r1_pin_cache_miri [cfg(miri)→0 тестов локально], no_stale_doc_references, r6_route_directory{,_global,_model}, r3_1_fallback_remote_free, r10_7_alloc_batch_xthread_double_free, fastbin_requires_xthread, regression_realloc_xthread_stamp, regression_xthread_{large_free_no_leak,small_ring_miri,thread_free_alias_miri}, r19_{large_cache_decay_debt,large_cache_decay_extended,large_cache_decay_trim,large_interior_free,prf01_sidecar_oracle,prf02r_realloc_tail}), debug | 0 |
| то же, release: b4_r1_pin_cache (+ b4_r1_pin_cache_miri) | 0 |
| `--features "production internals bench-internals"`: 19 целей debug | 0 |
| то же, release: b4_r1_pin_cache (6 passed) | 0 |
| `--features "production hardened internals bench-internals"`: 19 целей debug | 0 |
| `cargo clippy --all-targets -- -D warnings` (default) | 0 |
| `cargo clippy --all-targets --features experimental -- -D warnings` | 0 |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 |
| `cargo clippy --all-targets --features "hardened medium-classes internals" -- -D warnings` | 0 |
| `cargo clippy --lib --features production -- -D warnings` | 0 |
| `cargo clippy --all-targets --features "production internals" -- -D warnings` | 0 |
| `cargo clippy --lib --features "numa-aware internals" -- -D warnings` | 0 |
| `cargo clippy --lib --features "alloc-global alloc-xthread" -- -D warnings` | 0 |
| `cargo clippy --lib --features "alloc-core internals" -- -D warnings` | 0 |
| `node scripts/verify-dbg-hook-safety.mjs` (252/35/125) | 0 |
| `node scripts/verify-structure-limits.mjs` (directory.rs ровно 1000) | 0 |
| `node scripts/verify-loom-target-wiring.mjs` (20 целей) | 0 |
| `node scripts/verify-miri-target-wiring.mjs` (inventory=13, entries=32, CI=30) | 0 |
| `node scripts/verify-ci-sentinels.mjs` (160 OK) | 0 |
| `RUSTFLAGS="--cfg loom" cargo check --features "alloc-core,alloc-xthread,tagged-index-stack/loom" --test loom_b4_r1_pin_cache` | 0 |
| `RUSTFLAGS="--cfg loom" cargo check --features "alloc-global,alloc-xthread,tagged-index-stack/loom" --lib` | 0 |

Примечания честности: `cargo clippy --features "alloc-global alloc-xthread" --all-targets` падает на намеренном compile-fail `examples/sol_f1_dbg_carve_batch_negative_probe.rs` (task #563) — CI-строка для этого фичекомбо `--lib` (`ci.yml:1479`). `verify-ci-sentinels`: WARNING — floor 141 < 160 — pre-existing дрейф; рекомендация отдельным айтемом поднять `MIN_SENTINEL_COUNT` вне этой задачи.

## 8. Что гнать на CI

- **loom-xthread**: 3 теста b4-модели (1 позитив + 2 `should_panic`-shadow) — сентинелы `ci.yml:3422-3424`; остальная джоба без изменений.
- **miri-core**: narrow sb + narrow tb шаги `b4_r1_pin_cache_miri` (`ci.yml:3058`, `ci.yml:3066`); **miri-plain**: обычная строка матрицы (`ci.yml:3308`).
- Обычные строки тестовой матрицы (19 целей, debug/release, три фичекомбо) — уже в матрице.
- **Оркестратор**: iai A/B (foreign-free плечо) + mt larson/mstress Windows A/B.

## 9. Ожидаемый эффект и метрика приёмки

- MT (larson/mstress, T=4..8, реальный `#[global_allocator]`): каждый foreign free теряет спин-блокировку шарда (конвой §4.3 B1), оба поиска и пару refs-RMW → 1 Acquire-load + 1 RMW + 2 зависимые Acquire-загрузки класс-листа. Промежуточный замер оркестратора (код раунда 1): +5…22 % MT, 26 прогонов без коллапса.
- Метрика приёмки A/B: на MT-плече с повторными free по тем же сегментам доля **`hits/(hits+misses) ≥ 95 %`** (счётчики `PIN_CACHE_HITS`/`PIN_CACHE_MISSES` через `dbg_route_pin_cache_totals`), иначе заявленная экономия не приписывается кэшу; `UNLINK_FALLBACKS`/`EVICTIONS` показывают цену вытеснения при > 4 активных чужих сегментах на поток. Гейты ADR: geomean ≥ 0.95, каждая T ≥ 0.90.
- Однопоточные own-пути не затронуты (изменён только foreign-путь и unlink: одна Release-стор под уже удерживаемой блокировкой).

## 10. Риски

- Различимость M-B в loom-модели не гарантирована (см. §6) — компенсация: shadow-контроль + credit-ledger-контракт; residual-окно для валидных free закрыто контрактом, не атомиками.
- Асинхронные TLS-деструкторы Windows (~300 мс) делают тесты зависимыми от bounded-ожиданий — теоретическая флаки-поверхность на перегруженном раннере.
- Bounded-проверки system_totals не поймают одиночную утечку пары (Entry+sidecar) — покрыто только точными refs-оракулами thread-exit теста.
- Префикс коммита: `perf(runtime)`.
