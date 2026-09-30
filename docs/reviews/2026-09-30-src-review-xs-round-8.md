# sefer-alloc — независимое ревью `src/`, раунд 8

## 1. Снапшот, метод и итог

**Проверяемый commit:** `2ebb6d80be665a0800d4abf50ae70e9c9ff3f354`.

**Git tree `src/`:** `a9a8c08c3211311367e31e84062cf0ce541e8fe6`.

Инвентаризация текущего `src/`: **170 Rust-файлов, 48 100 строк**, включая wiring, комментарии, диагностику и proof/test-support. Это размер исследуемой поверхности, а не заявление о формальном доказательстве каждой строки. Незавершенный worktree terminal-owner-drain **не входит** в этот отчет. Его будущая реализация не используется как доказательство закрытия находок.

Режим: read-only, native tools; без Rush, без новых подагентов и без исполнения allocator/runtime-путей. По rust-intel это bounded single-context static pass, а не полный распределенный category-by-category audit с независимыми доказательствами по всем модулям. Наиболее глубокое чтение выполнено для allocator lifecycle, canonical roots, remote publication, FREE/MAINTENANCE, pending ingress, TLS/fallback, cold trim и sidecar ownership. Дополнительно исследованы API/feature boundaries, Small/Primordial выдача и возврат credits, Large cache/realloc/error rollback, NUMA directory и legacy concurrent regions. Прошлый отчет прочитан для переоценки, но подтверждения ниже получены из текущих call chains.

**Подтвержденные текущие позиции:** **P0 0 / P1 0 / P2 1 / P3 1 / P4 2**.

P3 ниже — функциональное ограничение доступности over-aligned allocations, **не** нарушение memory-safety контракта `GlobalAlloc`. Оно уже имеет явный fail-closed guard и план реализации, но сама возможность по-прежнему отсутствует. Если считать только ошибки относительно уже работающего публичного контракта, а не открытые функциональные требования кампании, этот P3 следует учитывать отдельно как известное ограничение, а не как новый soundness bug.

**Вердикт:** нулевое состояние кампании не достигнуто. Ownerless remote-free reclamation в проверяемом коде не работает автономно; over-segment alignment пока не реализован. Нового подтвержденного UAF, data race или безопасно вызываемого metadata-corruption API этот статический проход не установил. Это не доказательство отсутствия таких дефектов; особенно остается producer-side provenance boundary старого foreign route.

## 2. Подтвержденные P0–P3

### R8-01 — P2: remote frees ownerless heap остаются без гарантированного consumer

**Места и symbols:**

- `src/global/tls_heap.rs:187-304`, `AbandonGuard::drop`: drain Large head, cold trim, затем recycle.
- `src/registry/heap_registry/claim.rs:324-360`, `HeapRegistry::recycle`: `LIVE -> FREE`, whole `HeapCore` остается в slot.
- `src/registry/heap_registry/claim.rs:39-57`, `HeapRegistry::try_maintenance`: есть lease primitive, но нет production caller, запускающего обслуживание.
- `src/registry/heap_core_xthread/routing.rs:221-300`, `HeapCore::dealloc_foreign_routing`: Large поступает в deferred head, Small — в ring/overflow/spill.
- `src/alloc_core/large/deferred_large/push.rs:20-42`, `push_large_deferred_free`.
- `src/alloc_core/large/deferred_large/drain.rs:25-55`, `drain_large_deferred_free`.
- `src/registry/heap_core/alloc/hot.rs:257-281`, `alloc_with_class`: обслуживание зависит от дальнейших owner allocation paths.

**Корректный достижимый сценарий:** A выделяет Large объекты, передает их B и завершает поток. A выполняет свой teardown drain/trim и переводит slot в FREE. Затем B корректно освобождает объекты — каждый ровно один раз и с исходным Layout. Notes поступают в стабильный deferred head A. Если больше никто не claim-ит именно этот slot, никто не вызывает его consumer. B может продолжать работать с другим heap или вообще не выделять память. Small notes аналогично остаются в rings/overflow/spill и удерживают outstanding credits. Даже notes, находившиеся в Small ingress до выхода A, не обязаны быть обработаны его exit trim: этот путь не является полным ingress sweep.

**Последствия:** уже освобожденные пользователями Large reservations остаются mapped; Small reservations не становятся eligible for finalization, пока notes не retire-нут их credits. Это не фиксированный cache/headroom floor: retained backlog зависит от числа и размера переданных allocations. На отдельных heaps table ограничен, но суммарное удержание может быть большим и сохраняться до конца процесса. Notes не потеряны структурно, однако reclamation не имеет обязательного события запуска.

**Почему lease не закрывает finding:** FREE→MAINTENANCE дает исключительное право на mutable owner state; сам по себе он не создает executor и не вызывает drain. Поиск `try_maintenance` в текущем `src/` находит определение, а не production maintenance loop. Аналогично fallback try-lock используется в dealloc routing, но не является периодическим автономным scavenger.

**Минимальное исправление по сути:** подключить реально выполняемое fair обслуживание initialized FREE slots и fallback с исключительным lease/try-lock, которое обрабатывает pending Small и Large obligations и финализирует пустые segments без нового claimant. Producer paused до publication должен удерживать credit; после terminal publication он должен иметь право задержаться только с независимо живущим sidecar pin. Budgeted work обязан сохранять detached remainder в owner-held памяти и возвращаться к нему в следующих passes. Одно добавление `try_maintenance`, еще один dirty hint или drain только на следующем alloc проблему не решает.

**Отношение к документации:** lazy owner-drain механизм текущий код описывает честно. Но это именно открытый end-to-end reclamation gap кампании; документы нового terminal/sidecar дизайна тоже прямо маркируют ownerless service как еще не реализованную гарантию. Нельзя считать target design работающим исправлением. Finding заново подтверждает R7 P2-1 и связан с незавершенной R6-01/R6-03 интеграцией, но не использует предыдущий текст как доказательство.

### R8-02 — P3: valid alignment `>= SEGMENT` по-прежнему всегда получает allocation failure

**Места и symbols:**

- `src/alloc_core/large/alloc_core_large.rs:136-145`, `AllocCore::alloc_large`.
- `src/alloc_core/alloc_core/mem/mem_impl.rs:38-59`, публичный `AllocCore::alloc` и его обещание null on OOM.
- `src/global/sefer_alloc/global_alloc.rs:18-29,32-47`, `GlobalAlloc` forwarding и сильная формулировка «only true OOM».
- План: `docs/LARGE_ALIGNMENT_ARCHITECTURE_2026-09-30.md:3-6,23-61,76-89`.

**Корректный сценарий:** `Layout::from_size_align(1, 4 * 1024 * 1024)` — valid Layout. Small classifier его не обслуживает; Large path возвращает null на `align >= SEGMENT` еще до обращения к OS. Наличие свободной памяти результата не меняет. Аналогично отвергаются `2 * SEGMENT`, `16 * SEGMENT` и другие допустимые power-of-two alignments.

**Последствия:** функциональная недоступность таких allocations через SeferAlloc/AllocCore. Стандартный allocating consumer может превратить null в allocation-error abort. Это **не** UB от возвращенного pointer: pointer не возвращается. `GlobalAlloc` допускает отказ, и guard специально предотвращает старые misregistration/misalignment ошибки.

**Классификация:** известное fail-closed ограничение, а не новое нарушение trait safety contract. Тем не менее R7 P3-2 еще не закрыт как функциональное требование: архитектурный документ явно говорит, что поддержка пока не реализована. Сильные «null только при true OOM» описания шире фактической возможности; кроме alignment здесь существуют и fallible metadata admission paths.

**Минимальное исправление:** реализовать уже выбранную единую geometry/identity модель: original OS release token, canonical usable root, payload start/offset и numeric payload key — разные сущности. Обновить owner hash/table, route registration, own/foreign/batch/fallback free, realloc/promotion/zeroed/NUMA/rollback. Снимать rejection guard последним. Простое удаление проверки или reserve с более высоким align без миграции lookup/release paths небезопасно. Если поддержка откладывается, сохранить явное публичное ограничение и не выдавать архитектурный план за closure.

## 3. Terminal route lifetime, FREE/MAINTENANCE и sidecar — что установлено по текущему коду

### 3.1. Pin lifetime отделен от reservation lifetime

`src/registry/segment_route/directory.rs:121-135,183-197,458-494`:

- lookup и ref increment выполняются под shard mutex;
- removal использует тот же mutex и исключает новые pins;
- owner registration удерживает counted Entry reference до unlink;
- последний handle освобождает Entry и соответствующий System-backed sidecar;
- descriptor pin не требует дальнейших обращений к reservation bytes.

Поэтому `RoutePin` после unlink может оставаться валидным **как descriptor/sidecar capability**, не доказывая mappedness allocation. Это правильное разделение. Требуемую mappedness до publication отдельно обязан обеспечить allocation credit.

`src/registry/segment_route/pin.rs:23-45`: `publish_small`/`publish_large` уже `unsafe fn` с требованием unique valid free текущего instance. Address lookup не выдается за подтверждение права освобождения.

### 3.2. Safe public sidecar accessor не дает установленного обхода actual allocator owner capability

Наружная граница registry — `src/lib.rs:494-501`: external public module только с `internals`. Внутри `segment_route` имеются public exports, но:

- `RoutePin::entry` закрыт (`pin.rs:6-8`);
- RoutePin возвращает только owner/kind/incarnation, не root/SmallSidecar/scan;
- actual allocator RouteRegistration хранится в private RouteSlots (`route_slots.rs:12-16`);
- `RouteRegistration::small_sidecar()` доступен получателю **своей** успешной регистрации, а duplicate admission не позволяет получить registration уже зарегистрированного allocator route.

Подтвержденного downstream-safe пути «lookup настоящего allocator pointer → mutate classes/consume its pending bits» не найдено. Возможность вручную создавать регистрации в глобальном test substrate — отдельный API/ownership-policy вопрос, не доказанный current-HEAD memory-safety обход.

### 3.3. FREE/MAINTENANCE exclusion работает как capability prerequisite, но cutover не завершен

`claim.rs:177-222`: обработаны EMPTY и FREE; проигранный CAS, включая MAINTENANCE, не дает pointer. Monotonic `initialised` читается после успешного FREE→LIVE CAS; failed first materialization возвращается через INITIALIZING→FREE.

`claim.rs:381-407`: mutable core доступ из MaintenanceLease остается unsafe legacy handoff. Drop возвращает MAINTENANCE→FREE Release, неожиданное unwind/неправильный state — terminal abort. Это не автоматически безопасный публичный typed-owner API; его documented aliasing contract по-прежнему существенен.

`stack.rs:39-56`: maintenance discovery не materializes chunks и учитывает initialized flag. `saturation.rs:37-69`: negative full hint версионирован, availability publication сбрасывает saturation; terminal version отключает optimization вместо wrap. Это устраняет прежнее обязательное повторение полного скана на каждом sustained-full newcomer, не делая остальные discovery paths O(1).

### 3.4. Pending primitives пока не являются authoritative production ingress

`segment_route/mod.rs:1-10`, `remote_inbox/inbox.rs:1-6` и текущий routing согласуются: registration подключена, **foreign free по-прежнему идет старым путем**.

- Small terminal primitive: sidecar `fetch_or` (`remote_bitmap/sidecar_bitmap.rs:62-71`), bounded per-word `swap` (`bitmap_scan.rs:14-25`).
- Detached record копирует class до reclaim (`bitmap_cut.rs:19-36`).
- Intrusive inbox private prepare/terminal CAS/whole-chain detach существует, но не production route.
- Large route sidecar state adapter есть, однако production foreign Large free использует deferred predecessor stack; `alloc_core_large.rs:719-721` это подтверждает прямо.
- `deferred_large/drain.rs:37-39` останавливается на PUBLISHING; spill consumer останавливается на unready head (`heap_overflow/spill.rs:125-126`).

Последние два stopping conditions сейчас предотвращают чтение незавершенного predecessor/node. Но старый ingress не выполняет целевое обещание независимого bounded cut completed publications за paused publisher. Это **не новое отдельное подтверждение UAF** и не отдельный P2 в счетчике; это незакрытая приемка planned cutover. Сохраненный новый ledger/phase scaffolding не означает, что ingress, strict trim и autonomous service уже работают.

### 3.5. Current Small cold trim и owner-private directory действительно исправлены

`ownership.rs:264-271` теперь вызывает и `release_empty_current_small_for_trim`, и `release_directory_for_cold_trim`.

`alloc_core_small_pool_impl.rs:737-757`:

1. допускает только Small;
2. проверяет zero credits и not-decommitted;
3. перенаправляет `small_cur` на primordial **до** release;
4. очищает directory state и recycle-ит table entry.

`directory.rs:103-106` обнуляет raw directory pointer, reset-ит miss streak и drop-ает owner-held VM token. Будущая materialization rebuild-ит состояние из table (`directory.rs:59-96`). Стабильный cross-thread dirty sidecar намеренно не освобождается этим owner-private trim.

`segment_table_impl.rs:586-604` удаляет route/hash/cache identity до OS release. `lifecycle.rs:433-435` закрывает directory admission до Drop-release reservations; primordial reservation освобождается последней (`:505-535`). По прочитанному коду возврата к dangling current cursor или owner-private directory pointer не установлено.

Zero credits здесь — необходимое условие lifetime, **не** обещание осушения всего remote backlog. При pending credits trim корректно удерживает current segment; недостающий consumer относится к R8-01.

## 4. Переоценка всех находок round 7

| Round 7 | Текущий verdict | Текущее основание |
|---|---|---|
| P1-1 chunk OOM abort | **Закрыт по статическому пути** | `claim.rs:171-174` использует fallible `slot_or_none`, publish-ит availability и возвращает null; `registry.rs:274-285` возвращает None вместо обязательного abort на claim path. Infallible accessor остается для доказанно materialized indices/test hooks. Runtime OOM oracle в этом review не запускался. |
| P2-1 late ownerless frees | **Открыт** | R8-01: producer route жив, production maintenance executor отсутствует. |
| P2-2 empty current segment на exit | **Закрыт по статическому пути** | `ownership.rs:267`, `alloc_core_small_pool_impl.rs:737-757`. Не считать pending-credit retention этим прежним багом. |
| P3-1 fallback игнорирует custom policy | **Закрыт по supported single-global-instance пути** | `global_alloc.rs:38-40,114-116,134-136` → instance `with_fallback_heap` → `fallback.rs:421-436`; first initialization получает config (`:265-270`), conflicts observable. First-wins multiple-instance semantics теперь явно описаны в `core.rs:193-239`; это ограничение, не независимость экземпляров. |
| P3-2 alignment >= SEGMENT | **Не реализован; известное fail-closed ограничение / открытая функциональная позиция** | R8-02 и явный architecture plan, не implementation receipt. |
| P3-3 sustained-full scan на каждый claim | **Главный сценарий закрыт** | `stack.rs:93-106` + versioned saturation. First full certification и availability/recovery scans остаются O(high-water), но нельзя повторять прежнее утверждение «каждый saturated newcomer всегда сканирует все slots». |
| P3-4 directory lifetime на recycled slot | **Закрыт для owner-private directory** | `ownership.rs:270-271`, `directory.rs:103-106`. Per-slot cross-thread dirty sidecar и per-segment route class backing — другие структуры и другие lifetime contracts. |
| P4-1 три remote substrates, один active | **Частично superseded, cutover debt остается** | RouteDirectory уже реально участвует в registration/issue. Lookup/publication/consumer cutover еще не подключен; нельзя называть всю RouteDirectory «uncalled». Legacy и staged protocols одновременно увеличивают audit surface. |

Прежняя narrow-provenance hypothesis не закрыта только фактом регистрации canonical roots. Own paths действительно используют table-owned roots, foreign paths пока еще читают headers через masked caller pointer.

## 5. P4 / maintainability и немеренные optimization opportunities

### P4-1 — переходная metadata уже оплачивается, authoritative ingress еще старый

**Места:** `route_slots.rs:19-27,67-83`; `directory.rs:50-66,379-455`; `small_sidecar.rs:11-15`; `remote_bitmap/sidecar_bitmap.rs:17-24`; production route `routing.rs:221-300`.

Каждый routed Small/Primordial segment получает System-backed pending bitmap и byte-per-granule class table; выдача публикует class metadata, но current production free/consumer этот sidecar пока не используют. Логический объем — **32 KiB pending + 256 KiB classes = 288 KiB на segment**, без Entry, allocator/VM rounding и retained route pointer arrays. Это арифметика структуры, не измеренный RSS. Primordial backing переживает whole-slot recycle вместе с самим heap.

Это не новый memory leak по забытым refs: Entries и sidecars имеют counted reclamation, registration before issue — намеренная стадия migration. Но release без завершенного cutover добавляет реальный metadata/admission/error surface, не получая promised terminal/liveness behavior. Минимальное направление: закончить один authoritative protocol; затем убрать obsolete active ring/overflow/spill/deferred stack и unused intrusive alternative из shipped profile. До gates сохранять ясное разграничение «production behavior сейчас» / «target protocol».

**Один связанный doc defect:** `SeferAlloc::trim_current_thread` сейчас обещает отсутствие contention с другими heaps (`diag.rs:153-155`), тогда как route removal использует общий shard mutex (`directory.rs:482-494`). Это уже не буквально owner-local lock-free operation. Исправить описание progress/cost при завершении cutover; не выдавать cold/bounded algorithm за universal wall-clock bound.

### P4-2 — исторические claims в source усложняют актуальный протокол и иногда уже неверны

**Места:** `heap_core_xthread/overflow.rs:55-178,446-648`; `find_segment.rs:196-206,389-443`; `alloc_core_core_diag/directory_diag.rs:437-441`; `alloc_core_large.rs:690-710`.

Большие исторические блоки объясняют прежние эксперименты и прежние protocol assumptions. Например «APPEND-ONLY» NUMA mapping не соответствует bucket reuse; часть NUMA-rescue комментариев говорит о directory-disabled режиме, тогда как текущий NUMA directory path активен; Large-reclaim commentary все еще описывает прежние release/defer варианты.

Минимальное направление: сохранить рядом с executable code актуальный invariant, exact caller obligations, ordering и release sequence; перенести history/measurement receipts в design/perf документы. Это doc/maintainability debt, не заявленный speedup и не основание считать unsafe operations автоматически исправленными.

### Opportunities — advice, без severity и без speedup claims

1. **Route directory scaling:** lookup — O(log routes-in-shard), но insert/remove — O(routes-in-shard) pointer shifts под shared mutex (`directory.rs:257-309,416-455`). При многих heaps и registration/retirement churn измерить contention и tail latency прежде, чем выбирать другой индекс. Shard count не превращает shifting cost в O(1).
2. **Sparse terminal scan:** current bitmap scan visits `ceil(high_water / 1024)` words, до 4096 на полный Small segment. Summary/hints могут ускорять sparse cases только если bounded full sweep/maintenance не начинает зависеть от potentially missing notification. Нужен workload/path oracle.
3. **Directory materialization после cold trim:** threshold использует table high-water, не число active Small segments (`directory.rs:65-66`). Возможна повторная materialization/rebuild после исторически большого heap, который теперь мал. Это кандидат измерения, не доказанный net regression.
4. **Magazine refill:** два обхода в `hot.rs:109-135`; вопрос уже принадлежит perf item 72. Не возвращать отклоненные bitmap-flush эксперименты под видом нового optimization.
5. **Legacy regions:** LockFreeRegion successful write копирует page-pointer vector и одну страницу, ShardedRegion length query — O(shards), EpochRegion remote queue — mutex-backed. Это осознанные research-tier trades вне production allocator. Backlog perf item 73 уже хранит эти вопросы; нужна реальная write/query-heavy форма, не новый generic global counter без contention measurement.

## 6. Гипотезы / доказательства, которые еще нужны — без присвоенной severity

### H1. Producer-side narrow provenance старого foreign route

`global_alloc.rs:65-69`, `routing.rs:189-193,286-299`, `realloc.rs:395-421`, `heap_overflow/spill.rs:52-79` используют masked caller pointer для reservation/header/slack accesses. Numeric `map_addr` не расширяет provenance. Table-owned canonical root исправляет own path, но новый sidecar lookup еще не заменил foreign route. Для установления конкретного soundness verdict нужны actual GlobalAlloc Miri cases с requested size 1–7/narrow reborrow и обоими applicable aliasing models; ничего из этого здесь не исполнялось. Source design сам признает этот boundary, поэтому нельзя ни объявить его безопасным, ни выдать непроверенный UAF за подтвержденный P1.

### H2. NUMA unknown→dedicated residual (#158) нельзя переносить из индекса механически

Текущий `publish_empty` уже очищает **все** buckets (`directory.rs:183-207`), включая stale candidate cleanup (`find_segment.rs:945-951`). Это снимает центральный прежний сценарий бесконечных stale scans после обычного pop-to-empty.

Однако `sync_directory_for_segment_classes` оставляет current-bucket-only empty branch (`directory.rs:255-258`). Для нового подтвержденного finding нужно показать valid production path, который передает этот class в changed mask при уже-empty BinTable, оставляет unknown bit и обходит all-bucket cleanup. По прочитанным successful-reclaim producers такого сценария не установлено; сама разница двух helpers еще не является consumer-visible bug. Card следует переоценить как partially superseded/unclosed synchronization residual; не стирать его и не считать автоматически подтвержденным current P3. Stale APPEND-ONLY prose остается P4.

### H3. Class-aware dirty redundant scans

Class-scoped dirty sweep может drain-ить все classes сегмента, оставляя dirty bits других classes для будущих passes. Это redundant-work candidate, не установленная ошибка reclaim. Safe optimization должна сохранить concurrent producer publication, OOM/coarse latch переход и discovery completeness; один blind clear может добавить lost-notification bug.

## 7. Обязательные open indexes: dispositions без потери deferred records

Исследованы оба canonical indexes и referenced ACTIVE + все девять thematic TRACKED tiers. Исторические runtime/test/CI receipts в них используются как **уже записанные наблюдения**, не как новые результаты этого review. Старые P0/NO-GO заголовки publication campaigns не автоматически означают новый runtime P0 в текущем `src/`.

### Релевантные correctness records

| Record | Disposition в этом read-only раунде |
|---|---|
| #17 verification coverage | **Оставить открытым/accepted residual.** GlobalAlloc/fallback и fully materialized sidecar memory-model coverage не превращаются в доказанную safety только от обычных native tests. Здесь эти gates не запускались. |
| #22 ring unwind / #23 fallback init rollback | **Оставить как residual, не fresh P0–P3.** Нет установленного current production panic-after-mutation trigger. Их documented limits важны при изменении consumer/initializer. |
| #146 Large filler null / #147 class-aware 0/N | **Оставить наблюдения нерешенными.** Нет нового доказательства их причин; не повторять уже rejected registry-collision hypothesis как finding. |
| #152 fork | **Документированное ограничение.** Allocation в child multithreaded fork до exec не поддержана; atfork sketch не дает разрешения обходить POSIX async-signal-safe restriction. Не новый source bug в valid supported usage. |
| #154 history prose | **Открыт**, P4-2 этого отчета — та же maintainability область. |
| #155 Linux small-stack NUMA init | **Сохранить известный confirmed availability record.** Current `new_inner` вызывает `numa::current_node` (`lifecycle.rs:265-268`) без intrinsic warm-up; root wrapper `numa.rs:34-35` не меняет initializer stack. Записанный Linux debug 64 KiB stack overflow не опровергнут. Это dependency-origin риск, не заново исполненный root-src finding и не включен в fresh severity count. Следующий numa-shim edit должен закрыть или точно документировать реальный peak. |
| #156 root all-features rustdoc | **Оставить recorded gate debt**, текущий -D warnings doc build не запускался. Не приписывать this review успешную публикационную документацию. |
| #157 spill observability | **Оставить deferred API decision.** Always-zero legacy loss counter не показывает spill pressure; отсутствие нового public field само по себе не runtime correctness bug. |
| #158 NUMA directory | **Переоценить, не молча повторять и не молча закрывать.** H2: main production empty cleanup уже all-buckets; remaining sync branch требует reachable scenario/oracle. |
| #159 empty affinity list / #160 Relaxed pending hint | **Гипотезы остаются гипотезами.** Нет подтвержденного supported-platform empty list или lost-index schedule. |
| #161 feature warnings; #95 root test powerset; #107 isolated clippy combo | **Сохранить CI/config debt.** Review статический; ни разрешенного rerun, ни closure receipts нет. Не менять root powerset на all-targets, игнорируя записанную gating/compile-fail decision. |
| ACTIVE #1/#2/#11/#13/#62 | **Решения/процесс/host-cache caveats оставить в их scope.** Ничего здесь не является разрешением commit/push, переписывать context rules или считать cross-worktree artifacts trustworthy. |

Остальные прочитанные cards — member-crate publication history, platform execution gaps, gate/process records — оставлены в существующем scope: этот review не проводил их повторный release audit и не отменяет owner decisions/waivers. Закрытые карточки once-ptr-cell/tagged-index-stack и исторические semver findings не переносятся в current source severity count.

### Релевантные perf decisions

- **Items 13/30:** small-pool cap и large-cache-extended — измеренные tradeoffs/opt-in policy, не повод менять production default в read-only review. Narrow-working-set scan cost нельзя выдавать за free.
- **Items 25/26/28/29:** virgin-zero-skip, small lazy commit, exact-span и reserved-capacity promotion остаются decisions с соответствующими triggers; не считать feature presence доказательством general speedup.
- **Items 34/35/45:** Small ≥64-segment workload precondition, отсутствие настоящего batch consumer и rejected merged bitmap mechanism сохраняются. Текущий новый sidecar не разрешает re-open rejected experiment без своей формы и activation evidence.
- **Item 38:** tiered trim API остается не реализованным самостоятельным design вопросом; не подменять ownerless reclaim его удобным частичным вариантом.
- **Item 42:** sparse decay stride retention trade не закрыт адаптивной политикой; pure idle не означает background decay.
- **Items 64/68:** shipped magazine budget и old-cursor finalization имеют функциональные receipts, но не завершенный measured perf/RSS gate; source correctness closure и speedup claim — разные вещи.
- **Item 65:** pool — same-class free-list reserve; cross-class carve reuse не работает по deliberate design. Новая reservation при class switch сама по себе не заново подтвержденный leak.
- **Item 69:** source теперь **реализует** empty-current cold trim (`ownership.rs:267`, pool impl `:737-757`). Карточка «design only / trim leaves current committed» уже stale по code axis. Нужно обновить ее current state, сохранив отдельный **неизмеренный** RSS/cold-restart-cost trigger; нельзя оставить active header, будто реализация отсутствует.
- **Items 66/67/70/72/73/74:** fallback contention, inlining, coarse dirty writes, refill double traversal, region/stats complexity и non-production AB feature arms оставлены как measure-first backlog. Ничего из этого не измерялось в раунде 8.
- Member VM/NUMA/TIS hardware/perf questions остаются у своих recorded owners/triggers; root-src review не производит для них ни hardware receipt, ни waiver, ни GO.

Изменения самих indexes родителю следует сохранить при последующей записи/интеграции: данный агент checkout не редактировал.

## 8. Coverage limits и следующий приемочный барьер

1. Это свежий source-oriented review всего module inventory с глубокими выбранными call chains, **не exhaustive line-by-line proof всех 48 100 строк** и не независимая formal проверка каждого unsafe block.
2. Не выполнялись tests/build/lint/formatters/rustdoc/Miri/Loom/Kani/bench/runtime probes. Ни «компилируется», ни «tests green», ни measured speedup/RSS здесь не заявляются.
3. Реальные Windows decommit/recommit, Unix same-VA reuse, weak-memory interleavings, busy fallback/worker startup failure, requested-size/narrow-borrow provenance и Linux small-stack initializer не были воспроизведены этим проходом.
4. Workspace dependencies не проходили полный отдельный release/supply-chain audit. Прочитанная binding world: root edition 2021, MSRV 1.93; Cargo.lock фиксирует arc-swap 1.9.1, crossbeam-epoch 0.9.20, core_affinity 0.8.3, aligned-vmem 0.2.0, numa-shim 0.2.0, once-ptr-cell 0.1.0, size-classes 0.1.0, sefer-region 0.2.0.
5. Автономная reclamation должна быть доказана на **интегрированном** actual GlobalAlloc path: owner exits → last correct remote free → no future claim/alloc → consumer retires exactly that instance; fallback busy→idle; paused producer по обе стороны terminal publication; strict trim на fixed per-word cuts с ongoing later frees. Здесь перечислены будущие критерии, не выполненные проверки.
6. Over-aligned capability считается закрытой только после реальных successful SEGMENT/2×/16× layout cases, own/foreign/batch/fallback lifecycle, realloc prefix/OOM preservation и release-token fidelity, а не после написания geometry plan.

**Итог:** большинство round-7 path bugs действительно устранены текущим кодом. Главный оставшийся defect — отсутствующий autonomous consumer для ownerless remote backlog. Alignment остается явно незавершенной функциональной возможностью. Новый route/sidecar lifetime substrate выглядит существенно лучше как граница capability, но на проверенном HEAD он еще не заменяет старый authoritative ingress и потому не закрывает promised terminal/strict-trim/ownerless behavior.