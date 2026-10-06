# Ревью `src/` — раунд 13

## 1. Вердикт и границы доказательств

**Новых подтверждённых P0/P1/P2 в рассмотренных production-сценариях не найдено.** Найдены четыре P3 и один P4: неверная семантика счётчика скана, неограниченное удержание shard-token-ов до завершения потока, устаревшие описания терминального протокола, чрезмерный auto-Sync generic lock-guard-а, две ненужные операции unsafe-шва. Это **не** заключение об отсутствии уязвимостей и **не** release GO.

Известный **P1-box остаётся**: интрузивная запись в освобождённый Small-блок пересекает protector живого кадра by-value `Box`. Он принят владельцем, а не исправлен; карточка корректности 164. Этот обзор не меняет принятое решение и не называет аллокатор Miri-clean.

- Дата: 2026-10-06.
- Проверенная база: `e90a3575d1af4131b301ddf9b9b49b78bac88216`.
- Предыдущий источник нумерации: `docs/reviews/2026-10-06-063308-src-review-fxx-round-12.md`; следующего обзора `src/` в просмотренном текущем каталоге нет. Это раунд **обзоров src**, не исторический perf-round R34.
- На старте `git status --short` не вывел изменений. Исходники в этом раунде не исправлялись.
- Область: 168 Rust-файлов, 43 035 строк в `src/` на указанной базе. Полный файловый инвентарь получен через glob; тексты всех файлов прошли скрининг опасных конструкций и инвентаризацию unsafe. Числа рассчитаны из текстов, а не взяты из предыдущего отчёта.
- Вручную углублённо разобраны цепочки GlobalAlloc → TLS/lease/fallback → Small/Large issue/reclaim, lifetime route-pin, bitmap/class-map, bounded/full drains, Large state machine, заголовки/OS geometry, experimental epoch/RCU/sharding и дельта последних исправлений.
- Прочитана полная дельта `git diff b797f213..e90a3575 -- src/ Cargo.toml .github/workflows/ci.yml`: 26 файлов, 325 добавленных и 136 удалённых строк. Изменения tests/docs дополнительно рассматривались выборочно по соответствующим контрактам.
- **Не утверждается сплошное ручное построчное доказательство всех 43 035 строк.** Скрининг всего каталога не равен доказательству soundness каждого unsafe-блока. Матрица ниже различает ручную трассировку, исполненные сценарии и остаточные ограничения.
- Работа выполнена одним исполнителем; подагенты не запускались без явного разрешения. Полный многомодульный fan-out rust-intel не выполнен. Это ограниченный по глубине, но обще-каталожный обзор, не сертификат полного покрытия всех категорий навыка.
- LSP: rust-analyzer был зарегистрирован, но запрос references для `AtomicSlot::read_with` завершился `LSP server exited unexpectedly (code 0)`. Ошибка отправлена в report_issue. После этого использованы чтение исходников и текстовая трассировка; полнота symbol-aware references не заявляется.

### Среда и разрешённые зависимости

`rustc 1.97.0 (2d8144b78 2026-07-07)`, `cargo 1.97.0 (c980f4866 2026-06-30)`, host `x86_64-pc-windows-msvc`, LLVM 22.1.6. Версии из Cargo.lock: sefer-alloc 0.3.0; aligned-vmem 0.2.0; numa-shim 0.2.0; once-ptr-cell 0.1.0; size-classes 0.1.0; sefer-region 0.2.0; tagged-index-stack 0.1.0; arc-swap 1.9.1; crossbeam-epoch 0.9.20; core_affinity 0.8.3. Версии не менялись.

## 2. Находки

| ID | Уровень | Статус доказательства | Суть |
|---|---|---|---|
| R13-01 | P3 | воспроизведено native + код | `directory_words_examined` не считает нулевые слова, хотя именно их сканирует отрицательный поиск |
| R13-02 | P3 | подтверждено владением в коде; объём не измерен | `ErasedGuard` удерживает токены уже уничтоженных ShardedRegion до выхода долгоживущего потока |
| R13-03 | P3 | подтверждено код↔док | публичное описание и часть SAFETY-аргументов продолжают описывать снятые ring/TFS/overflow/deferred-механизмы |
| R13-04 | P4 | поиск всех src + tests | `Node::read_ptr` и `Node::write_ptr` не имеют вызывающих; `allow(dead_code)` скрывает остаток старого протокола |
| R13-05 | P3 | воспроизведён auto-trait actual source; production exploit не найден | `ShardGuard<T>` автоматически Sync уже при `T: Send`, хотя Deref выдаёт `&T` и требует `T: Sync` |

### R13-01 — диагностический счётчик измеряет не то, что обещает

**Место:** `src/alloc_core/small/alloc_core_small/find_segment.rs:368–376`; обещание — `src/alloc_core/segment/segment_directory/directory_stats.rs:48–50` и `src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs:45–50`.

Цикл перебирает каждое слово массива. При `bits == 0` выполняется `continue` **до** инкремента `DIRECTORY_WORDS_EXAMINED`. Таким образом счётчик считает только ненулевые слова, а не «each u64 word inspected». При пустом class-bitmap отрицательный поиск проходит массив, но диагностически показывает отсутствие работы. Это не ошибка выдачи памяти; это ошибка наблюдаемого контракта, способная испортить атрибуцию и выбор оптимизации.

**Нативное воспроизведение:** standalone AllocCore под `production internals alloc-stats`; сначала материализовать директорию крупным Small-классом, проверить, что все биты класса 0 нулевые, затем вызвать реальный `dbg_find_segment_with_free(0)`. Результат:

```text
R13 empty materialized directory scanned; result=None; directory_words_examined_delta=0; configured_words_per_class=64
```

Этот вывод не является latency/Ir/RSS-гейтом. 64 — размер существующего массива на этой конфигурации, 0 — наблюдаемая дельта неверного счётчика. Никакой speedup из этих чисел не выводится. Полный воспроизводящий код находится в приложении; временный example удалён перед коммитом.

**Рекомендация:** определить контракт явно. Если требуются именно проверенные слова — инкрементировать до проверки нуля. Если нужны ненулевые слова — переименовать счётчик и accessor, обновив всех потребителей; не сохранять вводящее в заблуждение имя как alias. Оба варианта требуют обновить документацию «storage only / Reads 0 until A3», которая уже не отражает активный increment.

**Контроль исправления:** пустой materialized bitmap должен отличаться от отсутствующего bitmap, а ненулевой bitmap — показывать число посещённых слов до найденного кандидата. Проверять с `alloc-stats` и без него. Не менять основной production-поиск ради коррекции telemetry.

### R13-02 — ограничен routing-cache, но не TLS-владение токенами

**Место:** `src/concurrent/sharded/sharded_region.rs:38–48,139–154,157–171,182–184,268–286,392–410,588–597`.

`MY_SHARDS` помнит максимум восемь регионов. Однако `ErasedGuard::claims` — отдельный append-only Vec; каждый успешный claim добавляет **сильный** `Arc<[AtomicBool]>`. Уничтожение ShardedRegion не удаляет соответствующий claim и не освобождает token-array: Arc остаётся в TLS до выхода потока. Для N последовательно созданных, привязанных и уничтоженных регионов на одном долгоживущем worker-е остаются N token-array-ов и N ErasedClaim, хотя живых регионов уже нет. При многократной FIFO-перепривязке одного живого региона также можно удерживать несколько его shard-claims — это отдельно и честно описано в исходнике.

**Классификация:** design/retention defect experimental-ветки, не data race, не обнаруженный remote DoS. Документированное сохранение claim до thread-exit объясняет механизм, но не задаёт верхнюю границу для transient-region churn. Количество байт, RSS и скорость в этом обзоре **не измерялись**.

**Рекомендация:** отделить lifetime региона от lifetime TLS-записи. Возможный вариант — Weak на token-array и удаление мёртвых claims при очередной привязке; при thread-exit обновлять только успешно upgraded token-array. Нельзя просто ограничить Vec и выбрасывать сильные claims живых регионов: это потеряет обязательство освобождения занятых shard-ов. Нужно отдельно решить, допустимо ли раннее освобождение старой привязки живого региона.

**Контроль исправления:** на одном ещё живом потоке создавать/привязывать/уничтожать transient-регионы; oracle должен подтвердить освобождение их token-storage **до** thread-exit. Одновременно живые регионы и смена привязки не должны терять токены или получать дублированный claim. Сценарий с восемью постоянными регионами сам по себе не ловит этот дефект.

### R13-03 — cleanup протокола не завершён в документации доверенной базы

**Места и конкретные расхождения:**

1. `src/global/sefer_alloc/core.rs:44–47` обещает cross-thread dealloc через Phase 10 Treiber stack. Фактический `publish_foreign` в `src/registry/heap_core_xthread/routing.rs:26–65` использует RouteDirectory и terminal sidecar.
2. `src/global/tls_heap.rs:431–443` содержит intra-doc ссылку на отсутствующий `HeapCore::bind_thread_free` и объяснение `thread_free == Some`. Нынешний `bind_slot_counters` в `src/registry/heap_registry/claim.rs:574–582` связывает только диагностические счётчики.
3. `src/global/sefer_alloc/mod.rs:36–46` описывает `FALLBACK_TFS` и прежнюю TFS-обвязку как текущее основание M5. Bootstrap не должен рекурсивно обращаться к выбранному глобальному аллокатору; явные `System`-аллокации независимых дескрипторов нельзя смешивать с этим запретом.
4. `src/alloc_core/platform/node.rs:85–89,142–145,179–183` в SAFETY-обоснованиях описывает producer-side overflow/spill-записи в тело блока. Текущий terminal producer таких записей не делает. Это не найденный новый UB: неправильна именно ссылка на отсутствующий механизм, призванная объяснять exclusivity.
5. `src/alloc_core/platform/node.rs:574–585` связывает reservation-atomic liveness с producer LIVE→PENDING и отсутствующим `push_large_deferred_free`. Сегодня producer Large-CAS работает на **независимом** `LargeState`, а физические lifecycle-слова reservation owner-only; смешивать эти две lifetime-области нельзя.
6. `src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs:1` утверждает, что producers никогда не читают class-map. После R12-01 `SidecarBitmap::publish` читает её через `classes.encoded` **до** terminal RMW (`sidecar_bitmap.rs:114–117`). Выполняемое чтение atomic/пинованное; заголовок просто устарел после последнего патча.
7. `src/alloc_core/large/mod.rs:4–5` и `src/alloc_core/mod.rs:25` ещё перечисляют `deferred_large`; `reservation_state.rs:60` говорит об оставшемся legacy deferred stack. Это конкретный незавершённый хвост R12-10(3), не новая архитектурная рекомендация.
8. `src/global/sefer_alloc/core.rs:145–150` говорит «there is no background thread, by design» без оговорки о явном `start_maintenance`. Для inline decay активных heap-ов смысл сохраняется, но процесс может иметь специально активированный worker; область утверждения нужно назвать.

**Последствие:** следующий аудит получает ложную модель владения и может перенести старое обоснование на новый код. Особенно опасны не косметические Phase-номера, а утверждения о том, кто читает reservation и чем удерживается lifetime.

**Рекомендация:** оставить в src текущие инварианты: independent descriptor pin, allocation-instance credit, last-access-before-terminal-RMW, exclusive owner lease, finite sweep. Историю вынести в уже существующие ADR/review-документы. Не заменять старую ссылку на deferred stack просто именем похожего state machine: это разные объекты памяти.

**Проверка:** `cargo rustdoc --features production --lib -- -D warnings` прошёл; значит эта находка **не** заявляется провалом опубликованного docs.rs-build. Green rustdoc проверяет разрешимые ссылки в обработанной поверхности, а не истинность SAFETY-прозы. Полная warning-strict internals/private-doc поверхность здесь не запускалась.

### R13-04 — мёртвые raw-pointer accessors в Node

**Место:** `src/alloc_core/platform/node.rs:359–390`: `read_ptr<T>`, `write_ptr<T>`, оба с `#[allow(dead_code)]`, документированы как обслуживающие `owner_thread_free`.

Поиск `read_ptr|write_ptr` во всём src и tests дал только определения и внутренние doc-ссылки в node.rs. Производственных вызовов нет. Mutable-пара `read_ptr_mut/write_ptr_mut` **используется** для pool-links и в эту находку не входит. `read_struct_with_atomic_word` тоже используется в `SegmentHeader::read_at` и не предлагается к удалению.

**Рекомендация:** удалить две неиспользуемые const-pointer операции и ссылки на них из doc mutable-пары. Это уменьшение обслуживаемой unsafe-поверхности, не измеренный runtime speedup. При удалении проверить все feature/cfg-комбинации и source-inclusion тесты; результаты только src-поиска не являются доказательством отсутствия любого внешнего include!.

### R13-05 — auto-Sync generic guard-а не наследует требование Sync к защищённому T

**Место:** `src/registry/segment_route/shard_lock.rs:14–27,55–67`.

`ShardLock<T>: Sync` правильно требует только `T: Send`: mutex сериализует доступ. Но `ShardGuard<'a,T>` содержит единственное поле `&ShardLock<T>`, поэтому сам автоматически становится Sync при `T: Send`. Его `Deref::deref(&self)` при этом возвращает `&T`. Для `T = Cell<u32>` guard допускает shared access на разных потоках, хотя Cell не Sync. Два таких читателя вправе вызвать `Cell::set` без захвата второго lock-а: они разделяют один уже захваченный guard.

**Подтверждение:** временный example включил **тот же файл**, через `#[path = "../src/registry/segment_route/shard_lock.rs"]`, без копии реализации. `require_sync(&guard)` для `ShardGuard<Cell<u32>>` скомпилировался; scoped-thread также получил `&guard` и прочитал Cell. Вывод:

```text
R13 actual ShardGuard<Cell<u32>> is Sync and shared across a scoped thread
```

Настоящая data race намеренно не исполнялась; достаточное нарушение типовой границы — компилирующееся безопасное межпоточное разделение Cell через generic guard. Полный witness в приложении B.

**Почему P3, а не новая production-уязвимость:** ShardLock/ShardGuard crate-private; текущая инстанциация в RouteDirectory содержит `Shard` с AtomicPtr/обычными owner-полями, а не Cell. Достижимого внешнего safe-Rust counterexample для поставляемого аллокатора не найдено. Дефект находится в обобщённой unsafe-абстракции и становится runtime-UB при появлении внутреннего `T: Send + !Sync` пользователя.

**Рекомендация:** guard должен получать Sync только при `T: Sync` (плюс необходимое Send-ограничение самого lock-а). Например, variance/auto-trait marker `PhantomData<&'a T>` делает это условие структурным; затем явно проверить требуемый Send guard-а. Не сужать `ShardLock<T>: Sync` до `T: Sync`: такой mutex вправе защищать Send-only payload.

**Oracle исправления:** отрицательный compile-case `ShardGuard<Cell<u32>>: Sync` должен перестать компилироваться; положительные `ShardGuard<u32>` и текущий `Shard` — сохраниться. Native round-trip locking test не проверяет этот auto-trait дефект.

## 3. Ревью недавно проделанной работы

| Изменение раунда 12 | Проверка на текущей базе | Вывод |
|---|---|---|
| R12-01 producer-side class-zero guard | прочитаны publish/class leaves/owner geometry; исполнен подпроцессный regression | исправление неотмеченной гранулы присутствует; uniform-leaf interior residual НЕ закрыт |
| R12-02 masked Small magazine root | прочитан hot helper; исполнен root/mask oracle | release-hit больше не содержит прежний `.expect`; debug-проверка канонического корня осталась |
| R12-03 routed negative directory | прочитан trust_negative; исполнен self-healing witness | безопасный отказ от trusted negative сохранён; неверный `WORDS_EXAMINED` независим от него |
| R12-04 init losers yield | прочитан счётчик/hold gate и wait loop; исполнен held-winner witness | после finite tight-spin бюджета действительно есть scheduler yield; fairness/SLA не доказаны |
| R12-05 bounded batch Large probe | прочитаны обе batch ветки и shared rescue; исполнены 3 witnesses | miss bounded, поздняя публикация обслуживается вращающимся курсором; полного latency/RSS-гейта это не заменяет |
| R12-06/07 unsafe seam cleanup | просмотрены дельта/inventory/current tls comment | пустой counters seam удалён; tls unsafe-hook описан точнее |
| R12-08 verification-only dependency | прочитаны target dependency и forwarding loom feature | normal production-граф больше не требует tagged-index-stack; реальные loom/kani jobs здесь не запускались |
| R12-09 zeroed slot documentation | просмотрены дельта chunk/ensure и текущий HeapSlot | перечень полей исправлен; это не новая typed-init проверка всех target-layout-ов |
| R12-10 state tests extraction | просмотрены дельта state files и pub gates | объявления inline tests удалены; test-only exports присутствуют; остаток doc cutover — R13-03 |
| R12 O-4 counters | исполнен exclusivity/classification witness; прочитана дельта NUMA fallback observer | новая классификация событий работает в native non-NUMA сценарии; не означает полного покрытия NUMA branch |

Тесты проверяют поведение, а не только wiring: R12-01 потеряет containment без class-zero guard; R12-02 сравнивает issued-root с каноническим корнем; R12-05 отличает полный L-скан от бюджета и требует retirement поздней публикации; R12 O-4 сравнивает сумму исходов с числом routed scans. В этом раунде мутанты не запускались — это анализ силы oracle, не новая запись «failing before».

## 4. Матрица безопасности и корректности

| Подсистема / обязательство | Что проверено | Остаток |
|---|---|---|
| Public unsafe boundary | `AllocCore/HeapCore::dealloc/realloc` требуют unsafe; RoutePin publication потребляет pin и имеет unique-transfer Safety | defensive no-op не делает произвольный pointer допустимым |
| Provenance | caller pointer используется как key; canonical metadata root берётся из table; masked magazine root ограничен Small/Primordial | Miri SB/TB в этом раунде не запускались |
| Route pin / retirement | refcount растёт под shard lock; final pin освобождает независимые System sidecars, не reservation | pin не заменяет instance credit; при malformed free нет lifetime-доказательства |
| Small publication | preflight/issue → handoff → class read → terminal fetch_or; detached cut съедается до release | P1-box принят; uniform leaf не является exact-issued-address bitmap |
| Large lifecycle | LIVE/PENDING/CONSUMING/CACHED/INITIALIZING/RELEASED, checked generation reuse; descriptor/reservation lifecycle раздельны | exhaustive weak-memory schedules не проверены native-тестом |
| Header atomics | full `SegmentHeader::read_at` исключает owner_state из plain copy; terminal words стоят вне копии | любые будущие cross-thread header reads требуют нового доказательства |
| TLS and lease | TORN до trim/release; HeapLease не Copy и !Send/!Sync; Release/Acquire state transfer; nested bind сохраняет внутреннюю привязку | unwind policy fail-closed через abort; fork-child allocator use не поддержан |
| OOM / rollback | checked biased-window arithmetic; RAII reservation до forget; prepare прежде owner state mutation | платформенная полнота реальных отказов OS вне Windows-сценариев не доказана |
| Realloc | own move leg проверяет readable span, новая allocation distinct, старый credit жив до copy/free; prototype narrow leg сохраняет old block при OOM | hidden invariant `.expect` не автоматически запрещён; valid-input trigger нового unwind не найден |
| Epoch | generation double-check, CAS eviction, defer_destroy bounds Send+'static/Sync; queue mutex — источник истины, Relaxed hint не публикует payload | native tests не заменяют loom; deferred destructors могут ждать epoch progress |
| RCU / LockFreeRegion | rejected removal проверяется до CoW; page tree сохраняет shared pages; returned Arc удерживает value | write-copy cost зависит от PAGE и live readers; generic perf-win не заявляется |
| Sharding/pinning | per-region ids, capped shard ids, tokens освобождаются на thread-exit | R13-02; пустой CoreId-list остаётся старой неподтверждённой карточкой 159 |
| Exact-object prototype | descriptor unlink до System.dealloc; caller-layout check; tombstones / load-factor rehash | opt-in, не production; spin/retention-гипотеза ниже |
| Network/crypto/input parsing | полный src-screen не обнаружил runtime async/network/crypto/parser surface указанного класса | supply-chain/advisory аудит зависимостей и их crates/ исходников не выполнен |

**Не находка:** гипотеза «можно безопасно заменить registry HeapCore и сразу уничтожить его» не получила контрпример. HeapCore constructors и его core-поле crate-private; наличие `&mut HeapCore` в lease само по себе не доказывает достижимый safe drop. Не включено в список уязвимостей.

## 5. Возможности оптимизации — без обещания измеренного выигрыша

| Кандидат | Точный слой | Решение / oracle |
|---|---|---|
| Routed negative-directory work | HeapCore → AllocCore routed Small miss | сначала исправить/уточнить R13-01; новые O-4 counters дают исходы, но не latency; item 82 остаётся НЕ СЕЙЧАС до paused-publication proof |
| Double refill walk | `finish_magazine_refill`, hot.rs:111–136 | два обхода: owner stamp, затем magazine bitmap. Item 72 уже владеет гипотезой; объединять только с сохранением исключения issued top-slot и virgin-mask, с in-context Ir A/B |
| Large full sweep on realloc | heap_core/free/realloc.rs | оставлен намеренно после R12-05. Не переносить bounded policy без oracle cache admission/reuse и сохранения old allocation на OOM |
| Ownerless maintenance cost | bounded ingress + pool/cache trim | budget слова не равен постоянному времени: одно слово содержит до 64 записей, retirement может делать OS-return. Item 78(a) уже владеет проверкой tail-latency/syscall count |
| Sharded transient retention | experimental ShardedRegion TLS | R13-02; прежде скорости — bounded lifetime dead-region token-storage; RSS измерять на том же churn-regime |
| Prototype spin lock | exact_object/exact_shard.rs:184–191,198–218 | `ExactShard::acquire` только spin_loop; rehash O(cap) внутри lock. Под oversubscription это кандидат CPU/tail-latency, а не доказанный hang. Не переносить автоматически порог 64 из fallback; сначала реальный contention oracle |
| Prototype descriptor high-water | ExactShard rehash/tombstones | cap не уменьшается после burst; все 64 static shards живут до process exit. Отличать caching policy от leak; измерить retained bytes + regrowth cost прежде shrink |
| Unsafe/API cleanup | Node const-pointer accessors / старые protocol mirrors | R13-04 и correctness item 167; снижение audit burden, не default runtime speedup |

Не предлагаются повторно отвергнутые FLUSH_N/STAGE_CAP/bitmap-coalescing попытки без нового victim. `production` feature composition и любые tuning defaults этим обзором не меняются. Wall-clock/Ir/RSS A/B не проводился; ни одна гипотеза не получила GO по производительности.

## 6. Исполненная верификация

Все cargo-команды использовали `--locked -j 2` и idle priority. Benchmark/stress sweep не запускался.

1. **Реальный installed allocator:** `cargo run --example global_allocator --features production`. Успех: maintenance startup/running assertion, Vec grow, String/HashMap workload, проверка содержимого. Вывод:
   ```text
   sefer-alloc global allocator OK — summed 100000 ints (=4999950000) and stored 10000 map entries, all through SeferAlloc.
   ```
2. **Published docs feature set:** `cargo rustdoc --features production --lib -- -D warnings` — PASS. Production совпадает с `[package.metadata.docs.rs].features`.
3. **Production lint:** `cargo clippy --features production --lib -- -D warnings` — PASS. Не workspace/all-target/all-feature lint.
4. **R12 regression suite:** `cargo test --features "production internals bench-internals batch-api alloc-stats" --test r12_01_foreign_free_of_unissued_granule --test r12_02_magazine_hit_mask_equals_canonical_root --test r12_03_routed_directory_negative --test r12_04_fallback_init_losers_yield --test r12_05_batch_large_bounded --test r12_o4_routed_miss_counters -- --test-threads=1` — **8 passed, 0 failed**.
5. **Epoch:** `cargo test --features "experimental internals" --test epoch --test r11_epoch_false_full --test regression_r2_21_epoch_drain_buffer_reuse -- --test-threads=1` — epoch **5 passed**, buffer reuse **2 passed**. `r11_epoch_false_full` здесь дал **0 tests**, потому что требует bench-internals; этот нулевой прогон не считается доказательством.
6. **Исправленная активация false-full oracle:** `cargo test --features "experimental internals bench-internals" --test r11_epoch_false_full -- --test-threads=1` — **2 passed**.
7. **R13-01 native witness:** `cargo run --example __r13_directory_word_probe --features "production internals alloc-stats"` — исполнился и подтвердил нулевую дельту при реальном empty-directory scan. Код в приложении.
8. **R13-05 actual-source witness:** `cargo run --example __r13_shard_guard_probe` — PASS, то есть нежелательный `ShardGuard<Cell<u32>>: Sync` действительно разрешён. Временный example удалён; data race/Miri-UB не исполнялись.

Итого **17 исполненных постоянных тестов** плюс реальный runtime example и два diagnostic witness. Это не полный cargo test и не npm run check. Miri, Loom, Kani, TSan, MSRV 1.93, release profile, all-features, macOS/Unix/ARM, huge-page success, dependency advisories, cargo-semver-checks и remote CI в этом раунде не запускались. Не было push, поэтому подтверждение landing-SHA CI не выполнялось.

## 7. Решения по прежним открытым пунктам

Просмотрены оба главных индекса и current-state/trigger карточек всех девяти thematic-файлов плюс ACTIVE. Архивы выборочно, не сплошь. Исторические нарративы многократно закрытых publish-кампаний не являются новыми runtime-блокерами src.

- Для **каждого** существующего пункта perf-индекса: сохранить его status/trigger; этот раунд — ревью, не реализация и не promotion-gate. Нет нового workload/hardware/owner-триггера для изменения прежнего решения. В обоих индексах добавлено явное round-13 disposition вместо молчаливого пропуска.
- В correctness-индексе все прежние пункты также сохраняют статус: запуск выбранных native тестов не является достаточным evidence для полного закрытия platform/model/publication acceptance. Новые R13-01…04 заведены карточкой 168, R13-05 — карточкой 169, без дублей 154/164/166/167.
- 164 / P1-box: **leave accepted known defect**, не маскировать отсутствием новых P1.
- 166 / interior foreign free: **leave hardening residual**, не называть защитой для произвольного указателя.
- 167 / stack mirror without runtime implementor: **leave verification debt**; forwarding loom feature исправляет сборку, не relevance протокола.
- Perf 81: **leave INCONCLUSIVE wall-clock**, native smoke не закрывает статистическую мощность A/B.
- Perf 82 / routed trust-negative: **leave НЕ СЕЙЧАС**; counters не являются доказательством publication visibility.
- Perf 72 / double refill walk и 78 / maintenance/sidecar hypotheses: **leave**, только указаны конкретные места для следующего измерения.
- Correctness 154 / prose debt: **leave**, R13-03 уточняет конкретный terminal-cutover хвост, не закрывает весь docs-долг.

## 8. Порядок действий после отчёта

0. Закрыть latent auto-trait дефект R13-05 до появления нового Send-only payload у ShardLock; negative compile-case обязателен.
1. Уточнить контракт `directory_words_examined`, исправить diagnostic behavior и добавить boundary oracle; shipping allocation algorithm менять не требуется.
2. Завершить терминальный cutover в SAFETY/public rustdoc: убрать старые ring/spill/reservation producer-proof ссылки, не ослабляя реальное owner-credit доказательство.
3. Выбрать bounded lifetime для dead-region shard claims и проверить на долгоживущем transient-region worker-е.
4. Удалить две мёртвые Node-операции; mutable pool-accessors и atomic-word snapshot оставить.
5. Только после этого рассматривать измерительные гипотезы §5. Обзор не выдаёт разрешения на изменение production defaults.

## Приложение A. Код исполненного R13-01 witness

Временный example был создан только для расследования; permanent test/code change не внесён. Для воспроизведения поместить приведённый код в локальный example и выполнить команду §6.7.

```rust
#![forbid(unsafe_code)]
use sefer_alloc::{AllocCore, SegmentLayout};
use std::alloc::Layout;
fn main() {
    let mut core = AllocCore::new().expect("core");
    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();
    for _ in 0..4096 {
        assert!(!core.alloc(layout).is_null());
        if core.dbg_directory_is_materialised() { break; }
    }
    assert!(core.dbg_directory_is_materialised());
    for slot in 0..core.dbg_table_count() as usize {
        assert_eq!(core.dbg_directory_get_bit(0, slot), Some(false));
    }
    let before = AllocCore::dbg_directory_words_examined();
    let result = core.dbg_find_segment_with_free(0);
    let delta = AllocCore::dbg_directory_words_examined() - before;
    assert!(result.is_none());
    assert_eq!(delta, 0, "current counter omits every zero bitmap word");
    println!("R13 empty materialized directory scanned; result=None; directory_words_examined_delta={delta}; configured_words_per_class={}", AllocCore::dbg_words_per_class());
}
```

## Приложение B. Код исполненного R13-05 witness

Это включение actual private source для проверки generic auto-trait, не утверждение внешней достижимости private API. Временный example удалён перед коммитом.

```rust
#[path = "../src/registry/segment_route/shard_lock.rs"]
mod shard_lock;
use std::cell::Cell;
fn require_sync<T: Sync>(_: &T) {}
fn main() {
    let lock = shard_lock::ShardLock::new(Cell::new(0u32));
    let guard = lock.lock();
    require_sync(&guard);
    std::thread::scope(|scope| {
        let shared = &guard;
        scope.spawn(move || assert_eq!(shared.get(), 0));
    });
    println!("R13 actual ShardGuard<Cell<u32>> is Sync and shared across a scoped thread");
}
```

## Приложение C. Unsafe-инвентарь скрининга

Список атрибутов и manual impl получен из всех исходных файлов. Пустой столбец не означает отсутствие raw-pointer работы: безопасные верхние слои могут вызывать unsafe-шов. Атрибуты перечислены по строкам, а не объявлены новыми дефектами.

| Файл | unsafe allowance: строки | manual unsafe impl: строки |
|---|---|---|
| `src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs` | 383 | — |
| `src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs` | 166 | — |
| `src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs` | 281, 308, 336 | — |
| `src/alloc_core/alloc_core/bootstrap.rs` | 207 | — |
| `src/alloc_core/alloc_core/lifecycle.rs` | 422 | — |
| `src/alloc_core/alloc_core/mem/mem_impl.rs` | 223, 253, 551, 627 | — |
| `src/alloc_core/alloc_core/sidecar_test_hooks.rs` | 31 | — |
| `src/alloc_core/large/alloc_core_large_cache.rs` | 86, 133, 277, 331, 375 | — |
| `src/alloc_core/large/large_cache_extended.rs` | 117 | — |
| `src/alloc_core/platform/node.rs` | 37 | — |
| `src/alloc_core/platform/os.rs` | 28 | — |
| `src/alloc_core/platform/sidecar.rs` | 146 | — |
| `src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs` | 2 | — |
| `src/alloc_core/segment/segment_directory/segment_directory_impl.rs` | 286, 314 | — |
| `src/alloc_core/segment/segment_header/segment_header_gen_table.rs` | 54, 97, 150 | — |
| `src/alloc_core/segment/segment_table/route_slots.rs` | 2 | — |
| `src/alloc_core/small/alloc_core_small_diag.rs` | 147, 180, 214, 238, 377 | — |
| `src/alloc_core/small/alloc_core_small_magazine.rs` | 451 | — |
| `src/alloc_core/small/alloc_core_small_pool/decommit.rs` | 96 | — |
| `src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs` | 171, 192, 273, 350, 385 | — |
| `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs` | 459 | — |
| `src/alloc_core/small/alloc_core_small/directory.rs` | 52, 112, 134 | — |
| `src/alloc_core/small/alloc_core_small/find_segment.rs` | 175 | — |
| `src/alloc_core/small/alloc_core_small/reserve.rs` | 449 | — |
| `src/concurrent/epoch/hand.rs` | 51 | 572, 582 |
| `src/global/exact_object/exact_shard.rs` | 4 | 163 |
| `src/global/exact_object/narrow.rs` | 4 | — |
| `src/global/fallback.rs` | 51 | — |
| `src/global/sefer_alloc/batch.rs` | 5 | — |
| `src/global/sefer_alloc/diag.rs` | 16, 261, 324, 370, 412, 452, 495 | — |
| `src/global/sefer_alloc/global_alloc.rs` | 6 | 33 |
| `src/global/tls_heap.rs` | 100 | — |
| `src/registry/bootstrap/ensure.rs` | 1 | — |
| `src/registry/bootstrap/loom_shim.rs` | 14 | 56, 58 |
| `src/registry/bootstrap/registry.rs` | 1 | — |
| `src/registry/heap_core_xthread/routing.rs` | 9, 24 | — |
| `src/registry/heap_core_xthread/sidecar_drain.rs` | 55, 91 | — |
| `src/registry/heap_core/alloc/batch.rs` | 139, 182 | — |
| `src/registry/heap_core/alloc/hot.rs` | 76, 152, 380, 507 | — |
| `src/registry/heap_core/diag/diag_probes.rs` | 248, 320, 376, 472, 493, 514, 636, 657 | — |
| `src/registry/heap_core/free/dealloc_batch.rs` | 186, 195, 210, 238, 277, 326, 371, 386 | — |
| `src/registry/heap_core/free/dealloc_own_base.rs` | 396, 458, 497, 549 | — |
| `src/registry/heap_core/free/dealloc.rs` | 218, 254 | — |
| `src/registry/heap_core/free/realloc.rs` | 127 | — |
| `src/registry/heap_core/state/tcache_flush.rs` | 99 | — |
| `src/registry/heap_registry/claim.rs` | 13 | — |
| `src/registry/heap_slot.rs` | 79 | 283 |
| `src/registry/segment_route/directory.rs` | 3 | — |
| `src/registry/segment_route/pin.rs` | 45, 57 | — |
| `src/registry/segment_route/shard_lock.rs` | 6 | 21, 23 |
| `src/registry/segment_route/small_sidecar.rs` | 2 | — |

### Manual Send/Sync и GlobalAlloc — границы доказательства

- `bootstrap/loom_shim.rs:56,58`: opaque raw-pointer публикация, без shared T API; verification-only mirror. Доказательство реального once-ptr-cell крата этим списком не заменяется.
- `heap_slot.rs:283`: Sync только через атомарную state/initialised публикацию и sole owner; HeapSlot не Send. Ручной read/CAS путь рассмотрен, weak-memory моделирование не запускалось.
- `segment_route/shard_lock.rs:21,23`: lock bounds T:Send допустимы; дочерний auto-trait guard-а ошибочен — R13-05.
- `exact_object/exact_shard.rs:163`: raw System table только под lock; прототип. Rehash выполняет System allocation под lock, а не вызов выбранного GlobalAlloc; отдельно остаётся contention-гипотеза.
- `epoch/hand.rs:572,582`: обе границы T:Send+Sync; defer_destroy и read_with добавляют собственные Send+static/Sync ограничения. Native epoch tests зелёные, Miri/loom не запускались.
- `sefer_alloc/global_alloc.rs:33`: unsafe trait contract проверен native example и рассмотренными paths; P1-box не позволяет трактовать это как общее доказательство soundness.

## Приложение D. Полный файловый инвентарь скрининга

Все строки таблицы относятся к неизменённому `src/` на базе §1. Это перечень прочитанных программным скринингом входов, **не** заявления о ручном построчном доказательстве каждого файла.

| Файл | Строки |
|---|---|
| `src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs` | 469 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs` | 352 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/mod.rs` | 57 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs` | 262 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs` | 366 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/totals.rs` | 48 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/vmem.rs` | 99 |
| `src/alloc_core/alloc_core/alloc_core_impl.rs` | 671 |
| `src/alloc_core/alloc_core/bootstrap.rs` | 433 |
| `src/alloc_core/alloc_core/counters.rs` | 428 |
| `src/alloc_core/alloc_core/lifecycle.rs` | 546 |
| `src/alloc_core/alloc_core/mem/mem_impl.rs` | 692 |
| `src/alloc_core/alloc_core/mem/mod.rs` | 10 |
| `src/alloc_core/alloc_core/mem/realloc_fastpath.rs` | 556 |
| `src/alloc_core/alloc_core/mod.rs` | 104 |
| `src/alloc_core/alloc_core/sidecar_drain.rs` | 372 |
| `src/alloc_core/alloc_core/sidecar_test_hooks.rs` | 66 |
| `src/alloc_core/alloc_core/state.rs` | 233 |
| `src/alloc_core/config/large_cache_config.rs` | 490 |
| `src/alloc_core/config/large_cache_mode.rs` | 39 |
| `src/alloc_core/config/mod.rs` | 26 |
| `src/alloc_core/config/profile.rs` | 457 |
| `src/alloc_core/config/small_segment_pool_config.rs` | 199 |
| `src/alloc_core/large/alloc_core_large_cache_eviction.rs` | 389 |
| `src/alloc_core/large/alloc_core_large_cache.rs` | 640 |
| `src/alloc_core/large/alloc_core_large.rs` | 832 |
| `src/alloc_core/large/large_cache_extended.rs` | 258 |
| `src/alloc_core/large/mod.rs` | 27 |
| `src/alloc_core/large/reservation_state.rs` | 138 |
| `src/alloc_core/mod.rs` | 262 |
| `src/alloc_core/platform/mod.rs` | 24 |
| `src/alloc_core/platform/node.rs` | 603 |
| `src/alloc_core/platform/numa.rs` | 199 |
| `src/alloc_core/platform/os.rs` | 892 |
| `src/alloc_core/platform/sidecar_stats.rs` | 166 |
| `src/alloc_core/platform/sidecar.rs` | 505 |
| `src/alloc_core/platform/size_classes.rs` | 349 |
| `src/alloc_core/segment/bitmap/alloc_bitmap.rs` | 124 |
| `src/alloc_core/segment/bitmap/magazine_bitmap.rs` | 149 |
| `src/alloc_core/segment/bitmap/mod.rs` | 19 |
| `src/alloc_core/segment/bitmap/segment_bitmap.rs` | 118 |
| `src/alloc_core/segment/mod.rs` | 66 |
| `src/alloc_core/segment/remote_bitmap/bitmap_cut.rs` | 36 |
| `src/alloc_core/segment/remote_bitmap/bitmap_record.rs` | 6 |
| `src/alloc_core/segment/remote_bitmap/bitmap_scan.rs` | 29 |
| `src/alloc_core/segment/remote_bitmap/mod.rs` | 9 |
| `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs` | 151 |
| `src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs` | 238 |
| `src/alloc_core/segment/segment_directory/directory_stats.rs` | 106 |
| `src/alloc_core/segment/segment_directory/mod.rs` | 106 |
| `src/alloc_core/segment/segment_directory/segment_directory_impl.rs` | 682 |
| `src/alloc_core/segment/segment_header/block_kind.rs` | 68 |
| `src/alloc_core/segment/segment_header/descriptors.rs` | 338 |
| `src/alloc_core/segment/segment_header/layout_asserts.rs` | 166 |
| `src/alloc_core/segment/segment_header/mod.rs` | 65 |
| `src/alloc_core/segment/segment_header/segment_header_gen_table.rs` | 158 |
| `src/alloc_core/segment/segment_header/segment_header_impl.rs` | 758 |
| `src/alloc_core/segment/segment_header/segment_header_layout.rs` | 264 |
| `src/alloc_core/segment/segment_header/segment_header_meta_fields.rs` | 239 |
| `src/alloc_core/segment/segment_header/segment_header_views.rs` | 265 |
| `src/alloc_core/segment/segment_header/terminal_words.rs` | 164 |
| `src/alloc_core/segment/segment_layout.rs` | 244 |
| `src/alloc_core/segment/segment_table/active_kind_index.rs` | 145 |
| `src/alloc_core/segment/segment_table/active_kind_ops.rs` | 63 |
| `src/alloc_core/segment/segment_table/harness.rs` | 204 |
| `src/alloc_core/segment/segment_table/hash.rs` | 270 |
| `src/alloc_core/segment/segment_table/issue_transaction.rs` | 123 |
| `src/alloc_core/segment/segment_table/mod.rs` | 71 |
| `src/alloc_core/segment/segment_table/route_slots.rs` | 234 |
| `src/alloc_core/segment/segment_table/segment_table_impl.rs` | 994 |
| `src/alloc_core/small/alloc_core_small_diag.rs` | 386 |
| `src/alloc_core/small/alloc_core_small_magazine.rs` | 657 |
| `src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs` | 895 |
| `src/alloc_core/small/alloc_core_small_pool/decommit.rs` | 279 |
| `src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs` | 432 |
| `src/alloc_core/small/alloc_core_small_pool/mod.rs` | 41 |
| `src/alloc_core/small/alloc_core_small_pool/segment_state_account.rs` | 36 |
| `src/alloc_core/small/alloc_core_small_pool/segment_state_reconciliation.rs` | 104 |
| `src/alloc_core/small/alloc_core_small_reclaim.rs` | 69 |
| `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs` | 976 |
| `src/alloc_core/small/alloc_core_small/dealloc.rs` | 146 |
| `src/alloc_core/small/alloc_core_small/directory.rs` | 263 |
| `src/alloc_core/small/alloc_core_small/find_segment.rs` | 750 |
| `src/alloc_core/small/alloc_core_small/mod.rs` | 36 |
| `src/alloc_core/small/alloc_core_small/reserve.rs` | 463 |
| `src/alloc_core/small/alloc_core_small/sidecar_drain_outcome.rs` | 9 |
| `src/alloc_core/small/mod.rs` | 40 |
| `src/alloc_core/small/reserved_small_segment.rs` | 245 |
| `src/concurrent/epoch/epoch_handle.rs` | 102 |
| `src/concurrent/epoch/epoch_region.rs` | 693 |
| `src/concurrent/epoch/hand.rs` | 591 |
| `src/concurrent/epoch/mod.rs` | 10 |
| `src/concurrent/lock_free/lock_free_capacity.rs` | 14 |
| `src/concurrent/lock_free/lock_free_handle.rs` | 98 |
| `src/concurrent/lock_free/lock_free_page_table.rs` | 171 |
| `src/concurrent/lock_free/lock_free_region.rs` | 622 |
| `src/concurrent/lock_free/mod.rs` | 11 |
| `src/concurrent/mod.rs` | 38 |
| `src/concurrent/pinning.rs` | 271 |
| `src/concurrent/sharded/mod.rs` | 8 |
| `src/concurrent/sharded/sharded_handle.rs` | 94 |
| `src/concurrent/sharded/sharded_region.rs` | 665 |
| `src/global/alloc_stats.rs` | 174 |
| `src/global/exact_object/exact_fatal.rs` | 7 |
| `src/global/exact_object/exact_shard.rs` | 266 |
| `src/global/exact_object/exact_table.rs` | 40 |
| `src/global/exact_object/insert_outcome.rs` | 9 |
| `src/global/exact_object/mod.rs` | 11 |
| `src/global/exact_object/narrow.rs` | 122 |
| `src/global/fallback.rs` | 749 |
| `src/global/maintenance_service.rs` | 235 |
| `src/global/maintenance_start_error.rs` | 34 |
| `src/global/mod.rs` | 70 |
| `src/global/sefer_alloc/batch.rs` | 138 |
| `src/global/sefer_alloc/core.rs` | 352 |
| `src/global/sefer_alloc/diag.rs` | 503 |
| `src/global/sefer_alloc/global_alloc.rs` | 167 |
| `src/global/sefer_alloc/maintenance.rs` | 46 |
| `src/global/sefer_alloc/mod.rs` | 200 |
| `src/global/tls_heap.rs` | 650 |
| `src/kani_proofs.rs` | 223 |
| `src/lib.rs` | 524 |
| `src/registry/bootstrap/chunk.rs` | 107 |
| `src/registry/bootstrap/ensure.rs` | 262 |
| `src/registry/bootstrap/loom_shim.rs` | 465 |
| `src/registry/bootstrap/mod.rs` | 190 |
| `src/registry/bootstrap/registry.rs` | 379 |
| `src/registry/bootstrap/saturation.rs` | 73 |
| `src/registry/heap_core_xthread/mod.rs` | 11 |
| `src/registry/heap_core_xthread/routing.rs` | 75 |
| `src/registry/heap_core_xthread/sidecar_drain.rs` | 102 |
| `src/registry/heap_core/alloc/batch.rs` | 305 |
| `src/registry/heap_core/alloc/hot.rs` | 813 |
| `src/registry/heap_core/alloc/mod.rs` | 5 |
| `src/registry/heap_core/core.rs` | 446 |
| `src/registry/heap_core/diag/diag_probes.rs` | 667 |
| `src/registry/heap_core/diag/mod.rs` | 9 |
| `src/registry/heap_core/diag/queries.rs` | 665 |
| `src/registry/heap_core/free/dealloc_batch.rs` | 392 |
| `src/registry/heap_core/free/dealloc_own_base.rs` | 554 |
| `src/registry/heap_core/free/dealloc.rs` | 259 |
| `src/registry/heap_core/free/mod.rs` | 18 |
| `src/registry/heap_core/free/realloc.rs` | 649 |
| `src/registry/heap_core/mod.rs` | 22 |
| `src/registry/heap_core/state/mod.rs` | 8 |
| `src/registry/heap_core/state/ownership.rs` | 156 |
| `src/registry/heap_core/state/tcache_flush.rs` | 124 |
| `src/registry/heap_core/state/tcache.rs` | 359 |
| `src/registry/heap_registry/claim.rs` | 601 |
| `src/registry/heap_registry/counters.rs` | 457 |
| `src/registry/heap_registry/maintenance.rs` | 47 |
| `src/registry/heap_registry/mod.rs` | 66 |
| `src/registry/heap_registry/stack.rs` | 140 |
| `src/registry/heap_slot.rs` | 304 |
| `src/registry/mod.rs` | 81 |
| `src/registry/segment_route/directory.rs` | 909 |
| `src/registry/segment_route/error.rs` | 8 |
| `src/registry/segment_route/kind.rs` | 7 |
| `src/registry/segment_route/large_state.rs` | 70 |
| `src/registry/segment_route/mod.rs` | 39 |
| `src/registry/segment_route/pin.rs` | 63 |
| `src/registry/segment_route/registration.rs` | 75 |
| `src/registry/segment_route/route_cut.rs` | 22 |
| `src/registry/segment_route/route_record.rs` | 6 |
| `src/registry/segment_route/route_scan.rs` | 15 |
| `src/registry/segment_route/shard_lock.rs` | 75 |
| `src/registry/segment_route/small_sidecar.rs` | 158 |
| `src/registry/segment_route/terminal_publication_gate.rs` | 111 |

## Принятые исправления после ревью

**Status: R13-01…05 CLOSED.** Исходный обзор выше остаётся историческим
снимком `e90a3575`; приведённые там старые counter/auto-trait witnesses
описывают именно ту базу, не исправленное дерево. Реализация выполнена по
отдельному запросу владельца через `/wrush`, от базы `021399a3`.

### Реализация и контроль интегратора

- R13-01: increment перенесён перед пропуском нулевого bitmap-слова, под
  прежним `alloc-stats`. API не переименовывался. Новый regression проверяет
  отсутствие директории, пустой materialized bitmap, ранний hit и feature-off.
- R13-02: region владеет Arc<TokenBlock> с Box<[AtomicBool]>; TLS хранит Weak.
  Это не Weak на inline Arc-срез: последний мог бы удерживать всю аллокацию.
  Полный token backing уничтожается при final strong drop; временный upgrade
  во время exit-release может удерживать его до конца этого release.
  Dead weak-claims прунятся на cold bind/claim, живые claims не выбрасываются.
  Pure internals observers проверяют backing lifetime и bounded claim count;
  FIFO/remote routing и освобождение двух живых claims на thread-exit сохранены.
- R13-03: terminal publication, allocation credit, independent descriptor pin,
  owner/cache authority и физические lifecycle-слова описаны раздельно.
  Исправлены также обнаруженные настоящим private-rustdoc гейтом внутренние
  ссылки, cfg-недоступные ссылки и ссылки на удалённые имена. Никаких lint allow
  или расширений visibility ради документации не добавлено. Неразрешимый
  private-only SegmentBitmap в group-doc назван как private helper, без
  фиктивной публичной ссылки.
- R13-04: удалены только Node::read_ptr/write_ptr. Mutable pool-пара и
  read_struct_with_atomic_word сохранены; narrow-feature builds прошли.
- R13-05: PhantomData<&mut T> в ShardGuard сохраняет Send-only payload у lock-а
  и moved guard-а, но shared guard требует Sync. Actual-source fixture
  проверяется локальным rustc (JSON stderr, только coded error diagnostics),
  не копией implementation и не mock-echo.

Шесть Rush-сессий работали в выделенных in-repo worktrees: четыре основные
среза и два private-link среза. Агентские отчёты не принимались за receipts:
интегратор прочитал actual diffs, вернул compile/синхронизационные и
документальные ошибки на доработку, затем сам выполнил проверку.
Тест-проверка буквальных link-строк удалена целиком как incidental source-text
oracle, а не перепинена под новые строки. Behavioral tests не ослаблялись.

### Исполненные проверки после реализации

- Полный native root `cargo test --locked -j 2 --features
  "production internals alloc-stats bench-internals batch-api" --tests
  -- --test-threads=1`: по Rust-harness итогам **804 passed, 0 failed,
  7 ignored** до удаления одного incidental link-text test.
- После окончательного doc/link cutover: тот же feature set, targets
  `r13_directory_words_examined`, `r13_shard_guard_auto_traits`,
  `r13_sharded_dead_region_retention`, `no_stale_doc_references`,
  `r6_drop_large_credit`: **42 passed, 0 failed**.
- Counter feature-off: `production internals`, 1 passed.
  Token release без observers: plain `experimental`, 1 passed.
  Existing sharded/FIFO/remote suites в `experimental internals` прошли.
- Expected-red counterfactuals, затем восстановленные passing implementations:
  старое положение increment даёт `0 != 64`; удаление guard-marker позволяет
  fixture скомпилироваться и роняет negative oracle; no-prune и дополнительная
  strong TLS ownership дают claim-count 3 вместо ≤2; отсутствие exit-release
  даёт shard 2 вместо 0. Никакая data race не исполнялась; failure paths не
  оставили зависших test workers.
- Clippy `-D warnings`: default / experimental / production library rows и
  `--all-features --all-targets` — PASS.
- Narrow library builds `alloc-core alloc-decommit` и
  `alloc-global alloc-xthread` — PASS.
- Strict rustdoc: exact published `production` и
  `--all-features --document-private-items`, `--lib -- -D warnings` — PASS.
  Private-gate failures сначала были реальными, затем исправлены; они не
  подавлены и не записаны в долг на следующий раунд.
- `node scripts/fmt-check.mjs` — PASS, Windows argv-budgeted проверка всех
  workspace targets.
- End-to-end throwaway executable с установленным SeferAlloc, активированной
  maintenance, typed regions и реальным пустым directory scan — PASS:

```text
R13 smoke PASS: installed allocator + maintenance; transient backings=0 before exit; claims=1; empty scan words=64
```

Этот executable удалён после исполнения. Постоянные regression-файлы и
actual-source compile-fail fixture сохранены. Значения — correctness oracles,
не latency/Ir/RSS gate; измеренный speedup не заявляется.

**Без изменений:** production feature composition, defaults, Cargo.lock,
версии проекта/зависимостей. P1-box (164), interior-free hardening residual
(166), verification-only mirror (167), perf 81/82 и общий prose-долг 154 не
закрываются этими проверками. Miri/Loom/Kani, release-profile suite и
performance A/B не запускались; push не выполнялся.
