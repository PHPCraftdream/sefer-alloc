# Ph4a/Ph4b: перенос власти над состоянием heap'а с thread на domain (transferable exclusive lease)

Статус: **ЧЕРНОВИК / дизайн-записка, дата 2026-10-02.** Записка описывает целевой дизайн шагов 4a/4b плана `docs/design/2026-10-01-113842-src-foundational-refactoring-plan-xs-sol.md:40` («Перенести власть над состоянием с thread на domain») и его условия приёмки. Это **не** разрешение начать реализацию, не ревью и не измерение: записка написана по чтению исходников и существующих оракулов, без запуска сборок, тестов и бенчей. Все ссылки `file:line` проверены чтением файла в этом worktree. Непроверенные утверждения помечены «гипотеза».

Источники, на которых записка стоит:

- ADR `docs/design/2026-10-01-adr-physical-boundary-and-progress.md` — гарантии **G1** (`:41`), **G2** (`:42`), **G3** и **G3+** (`:43`), **G4** (`:44`); стоп-линии «не писать autonomous без успешного `start_maintenance()`» (`:100`), «не делать протокольные типы `pub unsafe`» (`:99`, `:91`), «никакой claim под чужим `&mut HeapCore`» (план `:40`); гейты прогресса **P1–P4** (`:59`); класс MODEL-LIMIT (`:3`, `:11`).
- План: правило фазы — один изолированный worktree, личное чтение diff координатором, реальные positive и mutation-negative witnesses (`:27`); требование независимого оракула на каждое обещание и явный список того, чего модели не доказывают (`:29`); открытые вопросы владельца (`:49`–`:53`).

---

## 1. ТЕКУЩАЯ модель (всё ниже — проверенное чтение)

### 1.1 Переходы состояния слота registry

Состояния слота: `STATE_EMPTY=0`, `STATE_OWNED(=STATE_LIVE)=1`, `STATE_INITIALIZING=2`, `STATE_FREE=3`, `STATE_MAINTENANCE=4` — `src/registry/heap_slot.rs:89`–`src/registry/heap_slot.rs:99`. Единственная точка перехода — `HeapSlot::cas_state` (`src/registry/heap_slot.rs:250`–`src/registry/heap_slot.rs:258`); diagnostic-ридеры и хинты (`reuse_hint`, `scan_cursor`, `saturation`) права не дают.

| Переход | Кто выполняет сегодня | Гарантия / механизм | Файл:строка |
|---|---|---|---|
| `EMPTY → INITIALIZING` | поток, выигравший CAS в `claim_impl` (первая выборка пустого слота) | `AcqRel`/`Acquire` CAS, единственный победитель; проигравший `continue` | `src/registry/heap_registry/claim.rs:204`–`claim.rs:216` |
| `INITIALIZING → LIVE` | тот же winner: `HeapCore::new` → `heap.write` → `bind_slot_counters` → `initialised=true` (Release) → CAS (Release/Relaxed) | публикация готовности строго после записи; при ошибке CAS — `std::process::abort()` | `claim.rs:248`–`claim.rs:287` |
| `FREE → LIVE` (re-claim) | поток, выигравший CAS | сразу после CAS, **до** выдачи указателя, выполняется `trim_for_recycle` и хук `on_already_initialised` (config-conflict) | `claim.rs:220`–`claim.rs:225`, `claim.rs:288`–`claim.rs:300` |
| `FREE → LIVE → INITIALIZING` | winner при `FREE`-слоте с `initialised == false` (частично брошенный слот) | второй CAS; ошибка → abort | `claim.rs:226`–`claim.rs:243` |
| `LIVE → FREE` (recycle) | **исключительно поток-владелец** через `AbandonGuard::drop` (см. §1.4) | `Release`/`Relaxed` CAS; ошибка = тихий no-op; далее `reuse_hint` + `publish_claimable` | `src/global/tls_heap.rs:216`, `src/registry/heap_registry/claim.rs:375`–`claim.rs:386` |
| `FREE → MAINTENANCE` | любой вызывающий `try_maintenance_at` (сейчас — worker maintenance) | требует `initialised` под Acquire; `AcqRel`/`Acquire` CAS; неудача → `None`, доступа нет | `claim.rs:48`–`claim.rs:73` |
| `MAINTENANCE → FREE` | `Drop` `MaintenanceLease` | `Release`/`Relaxed` CAS; паника внутри → abort; затем `reuse_hint` + `publish_claimable` | `claim.rs:414`–`claim.rs:434` |
| `INITIALIZING → FREE` (OOM-rollback) | winner материализации | `Release` CAS; ошибка → abort | `claim.rs:479`–`claim.rs:492` |
| fallback `UNINIT → INITIALIZING` | поток, выигравший CAS в `heap_ptr_impl` | `Acquire`/`Relaxed` CAS; на выигрыше — RAII-гард отката | `src/global/fallback.rs:160`–`fallback.rs:181` |
| fallback `INITIALIZING → READY` | тот же winner после `write(hc)` и биндинга счётчиков | `Release`-store `READY`; затем disarm гарда | `fallback.rs:271`–`fallback.rs:276` |
| fallback `INITIALIZING → UNINIT` | OOM резервации (`fallback.rs:288`) или unwind (Drop `InitStateGuard`, `fallback.rs:540`–`fallback.rs:551`) | возврат `UNINIT` вместо залипания: проигравшие пере-гоняют CAS, а не крутятся вечно | `fallback.rs:288`, `fallback.rs:540`–`fallback.rs:551` |
| chunk registry `UNINIT → INITIALIZING → READY` | победитель CAS в `OncePtrCell` | reserve-OS-pages → Release-publish; loser спин под Acquire; OOM → откат sentinel | `src/registry/bootstrap/mod.rs:69`–`mod.rs:102`, `src/registry/bootstrap/ensure.rs:41`–`ensure.rs:60` |

Дополнительно: `generation` слота бампается на каждом успешном (re)claim (`claim.rs:247`) и в production **не читается** никем, кроме диагностических аксессоров (`src/registry/heap_slot.rs:160`–`heap_slot.rs:198`); ключом owner-identity сегмента служит `pack_owner`/`OWNER_ID_FALLBACK` (`src/registry/heap_core/core.rs:365`–`core.rs:367`, `src/global/fallback.rs:196`–`fallback.rs:202`).

### 1.2 Кто владеет `&mut HeapCore`

`HeapSlot` синхронизируется только атомарным `state` и single-writer протоколом: `unsafe impl Sync` с доказательством «не более одного писателя — CAS-winner» — `src/registry/heap_slot.rs:261`–`heap_slot.rs:283`. Намеренно **нет** `unsafe impl Send` — by-value перемещение слота с живым ядром не доказано и не нужно (`heap_slot.rs:285`–`heap_slot.rs:304`).

| Носитель `&mut HeapCore` | Как получается | Границы | Файл:строка |
|---|---|---|---|
| поток-владелец слота | выигранный `EMPTY→INITIALIZING`/`FREE→LIVE` CAS; `claim` возвращает **сырой** `*mut HeapCore` (doc: «legacy TLS handoff; stage 4 must replace it with a typed owner capability») | до `recycle` (LIVE→FREE) | `claim.rs:83`–`claim.rs:87`, `claim.rs:317`–`claim.rs:318` |
| maintenance lease | `FREE→MAINTENANCE` CAS + `initialised` Acquire | до `Drop` lease (MAINTENANCE→FREE) | `claim.rs:57`–`claim.rs:73`, `claim.rs:406`–`claim.rs:411` |
| fallback heap | process-static `static mut FALLBACK` под спинлоком `LOCK` | внутри `with_heap`/`with_heap_config` | `src/global/fallback.rs:76`, `fallback.rs:316`–`fallback.rs:337`, `fallback.rs:342`–`fallback.rs:358` |
| fallback, неблокирующе | `try_with_heap` — `try_acquire`, при занятости `None` (никогда не ждёт) | используется чужим dealloc-путём | `fallback.rs:369`–`fallback.rs:383`, `src/registry/heap_core_xthread/routing.rs:35`–`routing.rs:44` |
| alloc-face fast path | `CurrentHeap::Own(p)` из TLS; `&mut` формируется только под этим инвариантом | резолв только на own-thread | `src/global/sefer_alloc/global_alloc.rs:44`–`global_alloc.rs:55`, `src/global/tls_heap.rs:226`–`tls_heap.rs:236` |

Важная деталь lock-free дисциплины: `LOCK_ACQUISITIONS` — единственный наблюдаемый извне счётчик захватов fallback-спинлока (`fallback.rs:109`, `fallback.rs:597`), которым тест доказывает **отрицание** («этот dealloc не брал fallback-lock»).

### 1.3 Выбор слота (claim при нехватке/после OOM)

`pick_slot` = хинт → capped bump high-water → полный скан; политика **fresh-first**: новый индекс mint'ится раньше, чем переиспользуется старый FREE (`src/registry/heap_registry/stack.rs:108`–`stack.rs:140`, ADR `:24`). Скан не авторитетен до CAS: `scan_claimable_slot` (`stack.rs:12`–`stack.rs:35`), `scan_free_slot` только для initialised FREE (`stack.rs:39`–`stack.rs:57`), `scan_claim_recovery_free` при chunk-OOM (`stack.rs:62`–`stack.rs:85`). Отрицательный скан может быть зафиксирован (saturated) только с capacity и с версионной проверкой `SaturationHint` (`src/registry/bootstrap/saturation.rs:36`–`saturation.rs:72`). При chunk-OOM индекс остаётся retryable (`claim.rs:194`–`claim.rs:199`).

### 1.4 Выход владельца: TORN, reuse_hint, trim, TLS-teardown

1. На первой аллокации потока: `current_for_alloc` → `null` → `bind_slow_tagged` → `HeapRegistry::claim()` → `finish_bind` (`src/global/tls_heap.rs:248`–`tls_heap.rs:273`, `tls_heap.rs:405`–`tls_heap.rs:408`, `tls_heap.rs:470`).
2. `finish_bind` публикует **LOCAL раньше**, чем армирует GUARD: причина документирована — при std ≤ 1.92 регистрация деструктора `thread_local!` пушит в `DTORS` через глобальный аллокатор и реэнтерит его на том же потоке (`tls_heap.rs:440`–`tls_heap.rs:451`). Порядок LOCAL-then-GUARD (`tls_heap.rs:478`–`tls_heap.rs:495`).
3. Откат `finish_bind` (TLS или GUARD недоступны) — recycle и переход на fallback (`tls_heap.rs:480`–`tls_heap.rs:492`).
4. На выходе потока `AbandonGuard::drop`: `mark_local_torn()` → `trim_for_recycle()` → `HeapRegistry::recycle()` (`tls_heap.rs:187`–`tls_heap.rs:217`). `TORN = usize::MAX` (`tls_heap.rs:124`) — never dereferenced, только сравнение; два sentinel'а (null и TORN) разведены одной беззнаковой проверкой (Э2, `tls_heap.rs:250`–`tls_heap.rs:261`).
5. Резолверы после recycle: `current_for_alloc` → `Fallback` (`tls_heap.rs:261`–`tls_heap.rs:272`); `current_for_dealloc` → `ForeignNoBind` без fallback-lock (`tls_heap.rs:313`–`tls_heap.rs:328`); `current_for_trim` → `None` (`tls_heap.rs:378`–`tls_heap.rs:393`).
6. Re-claim переиспользует тот же `HeapCore`: перед выдачей вызывается `trim_for_recycle()` (`claim.rs:298`; реализация `src/registry/heap_core/state/ownership.rs:107`–`ownership.rs:110`), плюс инвалидация NUMA-кэша при `numa-aware` (`claim.rs:301`–`claim.rs:316`).

Чистота same-thread: `TORN` — это «poison-then-check в своём же TLS», а не межпоточное сравнение generation (`src/registry/heap_slot.rs:17`–`heap_slot.rs:26`). Реальный порядок разрушения TLS не предполагается: LOCAL — `const`-Cell без `Drop`, доступность TLS монотонна в program order (`tls_heap.rs:48`–`tls_heap.rs:68`).

### 1.5 TLS-teardown vs foreign free

Кросс-поточный free идёт **не через тело ядра**: `dealloc_routing` пытается own-thread fast path, иначе `publish_foreign` через независимые sidecars (`src/registry/heap_core_xthread/routing.rs:10`–`routing.rs:20`, `routing.rs:26`–`routing.rs:74`); припиненные sidecars переживают recycle (`src/global/tls_heap.rs:212`–`tls_heap.rs:214`). Если адрес принадлежит простаивающему fallback — синхронный reclaim через `try_with_heap`; занятый fallback не ждёт и не рекурсивно не занимается (`routing.rs:33`–`routing.rs:46`). Потребление публикаций — только хозяином: `drain_sidecar_ingress` (`src/registry/heap_core_xthread/sidecar_drain.rs:8`–`sidecar_drain.rs:10`), полный sweep — в `trim_for_recycle`, ограниченный — в `background_maintenance_step` (`ownership.rs:107`–`ownership.rs:127`).

### 1.6 start_maintenance и maintenance_pass

- Активация: `start_maintenance()` идемпотентна после `RUNNING`, параллельный старт даёт `InProgress`, ошибка spawn — `Spawn` и **ретраебельна**; на время старта стоит `StartingGuard` (откат в `IDLE` при любой ошибке) — `src/global/maintenance_service.rs:71`–`service.rs:107`, `service.rs:54`–`service.rs:59`; тип ошибки `src/global/maintenance_start_error.rs:11`–`maintenance_start_error.rs:15`. Публичная обёртка — `src/global/sefer_alloc/maintenance.rs:36`–`maintenance.rs:38`.
- Lock-order сервиса зафиксирован в шапке модуля: `CONTROL` (std `Mutex`) только для startup/тест-ack, никогда вокруг registry lease, fallback-lock и spawn'а (`maintenance_service.rs:3`–`service.rs:8`); poisoned `Mutex` → abort (`service.rs:123`–`service.rs:127`).
- Worker: круговая попытка lease'ов по `SLOT_BUDGET = 32` индексов за проход (`service.rs:29`), период `park_timeout(10 мс)` (`service.rs:30`, `service.rs:167`), на каждый слот — один `background_maintenance_step(BACKGROUND_INGRESS_BUDGET = 64)` (`src/registry/heap_registry/maintenance.rs:20`–`maintenance.rs:53`, `src/registry/heap_core/state/ownership.rs:14`, `ownership.rs:118`–`ownership.rs:127`); один неблокирующий `try_with_heap` на проход (`service.rs:144`–`service.rs:150`). Любой неожиданный выход/unwind worker'а — терминальный abort (`service.rs:63`–`service.rs:68`, `service.rs:130`).
- Вывод прохода/посещение fallback считаются только после освобождения всех lease и lock'ов (`service.rs:151`–`service.rs:161`) — тест-ack не создаёт ложного «прогресса».

### 1.7 Где заканчивается текущая модель

Текущие гарантии G2/G3/G4 все выражены через **поток**: владелец = поток, reclaim = поток (refill/trim/exit) или worker. Домена (domain) как носителя права в коде нет вообще: нет типа «domain id», нет таблицы доменов, нет передачи права между ними. Всё, что «переносимо» сегодня, — это `*mut HeapCore` как число, и перенос сделал бы инвариант невозможным.

---

## 2. ГДЕ AUTHORITY = THREAD, А НЕ DOMAIN (список)

Полный перечень мест, где право владеющего привязано к потоку, а не к перечислимому домену. Все строки проверены чтением.

| # | Место | Смысл привязки к потоку | Файл:строка |
|---|---|---|---|
| 1 | модульный док tls_heap | «single-writer invariant: the ONLY mutator of a heap's bins is its **owning thread**»; «Every resolver in this module is called only on the owning thread (it reads its own TLS)» | `src/global/tls_heap.rs:24`–`tls_heap.rs:35` |
| 2 | `LOCAL: Cell<*mut HeapCore>` | кэш права живёт в TLS конкретного потока; `null` = «поток ещё не связан» | `src/global/tls_heap.rs:141` |
| 3 | `GUARD: AbandonGuard` | recycle по выходе **потока**, `AbandonGuard::drop` | `src/global/tls_heap.rs:149`, `tls_heap.rs:187`–`tls_heap.rs:217` |
| 4 | `TORN` sentinel | защита от устаревшего кэша **этого же** потока после recycle; «same-thread poison-then-check» | `src/global/tls_heap.rs:124`, `src/registry/heap_slot.rs:17`–`heap_slot.rs:26` |
| 5 | `HeapRegistry::claim()` | возвращает сырой `*mut HeapCore`, названный в доке «legacy TLS handoff», требуемый заменить на типизированную capability в «stage 4» | `src/registry/heap_registry/claim.rs:75`–`claim.rs:87` |
| 6 | `HeapRegistry::recycle` (unsafe fn) | право на free-слота выводится из «pointer previously returned by claim» — то есть из TLS-кэша потока; все production-вызовы идут из `tls_heap` | `src/registry/heap_registry/claim.rs:336`–`claim.rs:386` |
| 7 | вызовы recycle | 3 production-места: `AbandonGuard::drop` и два rollback-отката `finish_bind` | `src/global/tls_heap.rs:216`, `tls_heap.rs:483`, `tls_heap.rs:491` |
| 8 | `MaintenanceLease::_not_send` | `PhantomData<*mut ()>` делает lease **неперееносимым** между потоками | `src/registry/heap_registry/claim.rs:392`–`claim.rs:396` |
| 9 | `MaintenanceLease::with_core` | `&mut self` + `unsafe` — доступ оформлен как заимствование текущим держателем, а не как право, которое можно передать | `claim.rs:398`–`claim.rs:412` |
| 10 | резолверы `current_for_*` | все три резолвят право через TLS-«поток», а не через идентификатор домена | `src/global/tls_heap.rs:248`, `tls_heap.rs:313`, `tls_heap.rs:378` |
| 11 | alloc/dealloc fast path | `CurrentHeap::Own` трактуется как own-thread lock-free путь; SAFETY-комментарий апеллирует к single-writer потока | `src/global/sefer_alloc/global_alloc.rs:44`–`global_alloc.rs:55`, `global_alloc.rs:65`–`global_alloc.rs:77` |
| 12 | `trim_current_thread` | публичный API триммит **текущий** поток через `CurrentHeapForTrim` | `src/global/sefer_alloc/diag.rs:195`–`diag.rs:210` |
| 13 | fallback `with_heap` | право выдается через process-global spinlock, а не через домен; «fallback» — не домен, а аварийный режим | `src/global/fallback.rs:316`–`fallback.rs:337` |
| 14 | `HeapSlot` без `Send` | by-value перемещение ядра между потоками не доказано (осознанно) | `src/registry/heap_slot.rs:285`–`heap_slot.rs:304` |
| 15 | `trim_for_recycle` | док прямо привязан к «thread exit» и к «this thread is still the slot's sole owner» | `src/registry/heap_core/state/ownership.rs:92`–`ownership.rs:99` |
| 16 | тестовый хук `dbg_restore_local_for_test` | контракт: «HEAP THIS **EXACT THREAD** legitimately owns … NEVER be sent across threads» | `src/global/tls_heap.rs:584`–`tls_heap.rs:596` |
| 17 | `dbg_mark_local_torn_for_test` | poison применим только к текущему потоку | `src/global/tls_heap.rs:566`–`tls_heap.rs:578` |

Сопутствующий факт миграции: `HeapRegistry::claim()` используют 69 файлов в `tests/`, `HeapRegistry::recycle` — 72 файла; `HeapRegistry::try_maintenance()` — 4 файла (`tests/r6_terminal_lease_registry.rs:25`, `tests/r8_maintenance_registry.rs:34`, `tests/r11_p3_registry_claim_chunk_oom.rs:65`, `tests/registry_basic.rs:224`). Любая замена сырого `*mut HeapCore` на типизированный lease — массовая правка тестов, а не одна сигнатура.

---

## 3. ДИЗАЙН PH4a (HS-4a)

Scope по плану (`docs/design/2026-10-01-113842-src-foundational-refactoring-plan-xs-sol.md:40`): `src/registry/{heap_registry,heap_core_xthread}/`. Запрет из плана соблюдается буквально: **никакой claim под чужим `&mut HeapCore`**; право выдается только победителем CAS.

### 3.1 Тип: HeapLease

Новый приватный протокольный тип в `src/registry/heap_registry/` (рабочее имя `HeapLease`), заменяющий сырой `*mut HeapCore` на всех нетривиальных путях.

```text
pub(crate) struct HeapLease {
    slot: &'static HeapSlot,     // 'static: чанки Registry живут процесс (bootstrap/mod.rs:88-91)
    index: u32,                  // pack_owner/route identity (heap_core/core.rs:365)
    generation: u64,             // снят на успешном claim (claim.rs:247)
    _not_send: PhantomData<*mut ()>,   // базовый режим: как MaintenanceLease (claim.rs:395)
}

impl HeapLease {
    pub fn slot_index(&self) -> u32;
    pub fn generation(&self) -> u64;
    pub(crate) fn core(&mut self) -> &mut HeapCore;   // единственный SAFETY-шов
    pub(crate) fn try_transfer(self, to: DomainId) -> Result<(), Self>;  // см. 3.4
}
impl Drop for HeapLease { /* LIVE → FREE через slot CAS, как recycle сегодня */ }
```

Обязательные свойства:

1. **Единственный authority — CAS.** Никакой путь, кроме выигранного `EMPTY→INITIALIZING` / `FREE→LIVE` / `FREE→MAINTENANCE`, не создаёт lease. Хинты, `reuse_hint`, `scan_cursor`, `saturation` по-прежнему кандидаты (`src/registry/bootstrap/saturation.rs:10`).
2. **Non-`Copy` + `Drop`.** Два живых lease на слот невозможны по типу: `Drop` возвращает слот в `FREE` (тот же CAS, что `recycle` сегодня, `claim.rs:375`–`claim.rs:381`), и `Drop` не может «потеряться» — это делает «потерянный lease» немыслимым, в отличие от сырого указателя.
3. **Нет `pub unsafe`.** ADR запрещает делать протокольные lease/claim `pub unsafe` (`...adr...:91`, `...adr...:99`). `HeapLease` — `pub(crate)`; для тестов — только безопасные числовые наблюдатели по образцу `dbg_slot_*` (`src/registry/bootstrap/registry.rs:321`–`registry.rs:367`) и `MaintenanceService::*_for_test`.
4. **Route identity.** `index` + `generation` — идентичность lease; повторный claim того же индекса с новым generation устаревает прежний (кросс-поточный stale-lease мутант).

### 3.2 TLS становится «предпочтительным доменом / кэшем», а не владельцем

`LOCAL` перестаёт быть хранилищем права: он кэширует `Option<HeapLease>` (или `Option<(index, generation)>` + ленивый lease) **предпочтительного домена**. Домен — перечислимый идентификатор (main, worker-pool, именованный), и право принадлежит домену. Следствия:

- «Выход владельца» = выход домена (пул переиспользуется, слот не перерабатывается), а не смерть потока.
- `TORN` остаёется, но меняет смысл: он теперь poison'ит кэш **конкретного** домена при передаче/отзыве lease, а не «своей смерти потока».
- Re-claim уже существующего ядра остаётся с `trim_for_recycle` (`claim.rs:288`–`claim.rs:300`) — это и есть «claim при owner exit» в сегодняшней модели; в целевой он же обслуживает смерть домена.

### 3.3 Диаграмма переходов (текст)

Текущая модель (authority = поток):

```text
                  ┌──────────────── claim() ───────────────┐
                  │  pick_slot: hint → bump → scan          │
                  ▼                                         │
   [EMPTY] ──CAS(EMPTY→INITIALIZING)──▶ INITIALIZING        │
                  │  HeapCore::new / write / bind_counters   │
                  │  initialised = true (Release)            │
                  ▼  CAS(INITIALIZING→LIVE)                 │
              [LIVE] ─────── owner = поток ───────┐          │
                  ▲                              │          │
   [FREE] ──CAS(FREE→LIVE)──── re-claim ──────────┘          │
       ▲                                                  │
       │  AbandonGuard::drop: TORN → trim_for_recycle → recycle(LIVE→FREE, Release)
       └────────────────── поток вышел ─────────────────────┘

   [FREE] ──CAS(FREE→MAINTENANCE)──▶ [MAINTENANCE] ──Drop lease──▶ [FREE]
                                          │
                                          └─ один bounded background_maintenance_step
```

Целевая модель (authority = domain, lease типизирован):

```text
                 DOMAIN TABLE                    REGISTRY (единственный authority)
   domain A ──▶ lease{index, generation}  ◀── CAS(FREE→LIVE) ──▶ slot LIVE
      │  lease: Non-Copy, Drop→FREE            (loser CAS вообще не получает core)
      │  core() -> &mut HeapCore  (единственный SAFETY-шов)
      │
      ├── try_transfer(lease, domain B) ──▶ A: empty, B: lease   (два домена никогда
      │                                       не держат lease одновременно; передача
      │                                       = атомарная смена держателя в domain table)
      │
      └── drop(lease) ──▶ CAS(LIVE→FREE) ──▶ slot FREE
                                     (также: смерть домена, unwind, отзыв)

   [FREE] ──CAS──▶ [MAINTENANCE] (help/worker lease, bounded) ──Drop──▶ [FREE]

   ownerless/dead-lease reclaim: [FREE] уже claimable → G3 при следующем claim
                               + (Ph4b) ограниченная помощь перед ростом
```

### 3.4 Передача lease между доменами — два варианта (решение владельца)

| Вариант | Механизм | Подытог по ADR | Цена |
|---|---|---|---|
| **A. Безопасная передача через domain-table** | `try_transfer(self, to)`: lease двигается в «почтовый ящик» домена; держатель снимается атомарно; получение — `DomainTable::take(domain_id)` | не добавляет `unsafe`; lease остаётся `!Send`; трансфер сериализуется одним CAS/RMW в таблице доменов | нужна таблица доменов (новый process-global объект в scope `heap_registry`) и правило «кто мёртв» |
| **B. `unsafe impl Send for HeapLease`** | lease можно двигать между потоками напрямую | это `unsafe impl` на `pub(crate)`-типе — не «pub unsafe API», но **новое утверждение о звуковости**, требующее proof'а и явного решения владельца | канал/очередь даёт трансфер «из ниоткуда»; аудит сложнее; ADR `:91` требует отдельного обоснования |

Рекомендация записки: **вариант A**. Причина: план требует, чтобы «Типизированные transitions (A) и переносимый exclusive domain lease (B) ... не открывали новых unsafe-предпосылок» (`docs/design/...plan...:23`), а ADR запрещает расширять `pub unsafe` (`...adr...:91`). Вариант A оставляет `HeapLease: !Send` (как `MaintenanceLease` сейчас) и выражает «transferable» через типизированную передачу, а не через тип-систему Send.

### 3.5 Инварианты Ph4a — кто проверяет

| ID | Инвариант | Кто проверяет (сегодня) | Что нужно в Ph4a |
|---|---|---|---|
| A1 | Не более одного `&mut HeapCore` на слот; authority = CAS-winner | `tests/loom_r8_maintenance_lease.rs`, `tests/loom_r11_registry_claim.rs`, `tests/r6_terminal_lease_registry.rs` | сохранить; добавить мутант «второй `core()` под тем же lease» |
| A2 | Проигравший CAS не получает доступ | `loom_r8_maintenance_lease.rs` (сценарии с неудачным CAS), `claim.rs:213`, `claim.rs:224`, `claim.rs:66` | Loom-модель с `HeapLease` вместо payload-модели |
| A3 | Двойной recycle/двойной `Drop` lease невозможен | `MaintenanceLease::drop` abort при ошибке CAS (`claim.rs:419`–`claim.rs:430`) | `Drop for HeapLease` — идемпотентный no-op при не-LIVE, abort при state-мутанте |
| A4 | Re-claim инвалидирует прежнего владельца (trim + NUMA + config-conflict) | `claim.rs:288`–`claim.rs:316`, `tests/regression_claim_oom_initialised_gate.rs`, `tests/regression_registry_initialised_gate.rs` | тот же путь через lease; мутант «пропустить `trim_for_recycle` при re-claim» |
| A5 | `initialised` — единственный шлюз материализации | `tests/regression_claim_oom_initialised_gate.rs`, `tests/r6_terminal_lease_registry.rs`, `tests/r8_maintenance_registry.rs` | сохранить; `HeapLease` не создаётся без `initialised` |
| A6 | Stale-TLS/TORN-указатель не разыменовывается | `tests/tls_heap_teardown_torn_sentinel.rs`, `tests/tls_heap_teardown_ordering_stress.rs` | TORN экстраполируется на «кэш домена», а не «кэш потока» |
| A7 | Передача lease не создаёт двух писателей | **отсутствует** | новый Loom-модель transfer/take |
| A8 | Переживаем публикаций sidecar (late free) | `src/global/tls_heap.rs:212`–`tls_heap.rs:214`, `tests/race_norecycle.rs`, `tests/r9_bounded_background_maintenance.rs` | учесть в Drop lease: recycle не должен «уводить» слот из-под публикаций |
| A9 | Saturation/negative-scan не выдаёт права | `src/registry/bootstrap/saturation.rs:36`–`saturation.rs:72`, `tests/r11_p3_registry_claim_selector.rs` | сохранить: authority только у CAS |

### 3.6 Lock-order (Ph4a)

Порядок фиксируется и не меняется:

1. **chunk CAS** (`OncePtrCell`, `bootstrap/mod.rs:69`–`mod.rs:102`) — неблокирующий, внутренний, spin до READY.
2. **slot CAS** (`EMPTY↔INITIALIZING`, `FREE↔LIVE`, `FREE↔MAINTENANCE`, `LIVE↔FREE`) — единственный authority, неблокирующий.
3. **Drop/transfer lease** — только RMW над состоянием слота и domain-table; внутри никогда не берётся fallback `LOCK`.
4. **fallback `LOCK`** — захватывается **только** когда lease на registry-слот НЕ держится (см. `routing.rs:33`–`routing.rs:46`: занятый fallback не ждёт и не рекурсивно не берётся).
5. **`CONTROL` (Mutex) maintenance-сервиса** — никогда вокруг lease и fallback-lock (`maintenance_service.rs:3`–`service.rs:8`).

Жёсткое правило: **слот-lease и fallback-lock взаимно исключающие**; **domain-table RMW и slot CAS** — вложенность только «domain-table снаружи, slot CAS внутри».

### 3.7 «Что может пойти не так → какой оракул ловит» (Ph4a)

| Риск | Последствие | Оракул |
|---|---|---|
| `HeapLease` даёт `core()` вне LIFE-state | `&mut` на чужое/неинициализированное ядро = UB | Loom: failed-CAS не входит в `core()` (`loom_r8_maintenance_lease.rs`); Miri SB: исключительно у CAS-winner |
| `Drop` lease дважды / на чужих состояниях | двойной `LIVE→FREE`, второй владелец | native: `Drop` при чужом state → abort (`claim.rs:419`–`claim.rs:430`); Loom |
| Re-claim прежнего владельца после передачи | два `&mut` (UAF) | новый Loom transfer/take; native `r6_terminal_lease_registry.rs` (claim→maintenance→reclaim handoff) |
| Потеря lease без recycle (домен-владелец умер) | утечка слота до повторного claim/насыщения | native-счётчик: `heaps_claimed_high_water` (`counters.rs:68`) + скан `scan_free_slot`; negative control P2 |
| TORN-логика сломана при domain-кэше | stale-разыменование | `tls_heap_teardown_torn_sentinel.rs`, `tls_heap_teardown_ordering_stress.rs` |
| Lease с `initialised == false` | чтение `MaybeUninit::uninit()` гонкой с `write` (именно этот класс закрывал task #133) | `regression_claim_oom_initialised_gate.rs`, `regression_registry_initialised_gate.rs`, Miri |
| chunk-OOM на claim | прежняя модель: retryable индекс | `r11_p3_registry_claim_chunk_oom.rs`, `r11_p3_registry_claim_uninit_oom.rs` |
| «Autonomous» без `start_maintenance()` | нарушение ADR-стоп-линии | ревью diff + native-оракул: PASSES не двигается без активации (`r8_autonomous_maintenance.rs`) |
| Распухший diff / смешение ownership и API | непросматриваемое изменение (план `:40`) | process: отдельные коммиты на 4a и 4b, личное чтение diff (план `:27`) |

---

## 4. ДИЗАЙН PH4b (HS-4b)

Scope по плану: `src/global/{tls_heap,fallback,sefer_alloc/maintenance}/`. Политика уже во многом реализована; Ph4b = явно сформулировать и покрыть оракулами то, что сейчас распределено по комментариям.

### 4.1 Политика worker / startup-failure / retry / direct free

| Аспект | Текущее поведение (проверено) | Целевое в Ph4b |
|---|---|---|
| Активация | `start_maintenance()` после `RUNNING` идемпотентна; `InProgress` без ожидания; `Spawn` ретраебельна | без изменений; добавить оракул «Spawn-ошибка не оставляет полу-активации» |
| Отказ worker'а | возврат/unwind → `WorkerGuard` → abort | без изменений; ADR: «гибель worker'а — abort» (`:44`) |
| Spawn-ошибка | `StartingGuard` возвращает `IDLE`, гарантия не активна | `MaintenanceStartError::Spawn` + повтор |
| Direct free (не-worker) | `publish_foreign` в sidecar; идющий fallback рекламится синхронно через `try_with_heap` (`routing.rs:33`–`routing.rs:46`) | сохранить: это G1-путь, не зависит от worker |
| Выбор слота | fresh-first: bump раньше скана (`stack.rs:116`–`stack.rs:134`) | **конфликт с G3+**, см. §4.2 и вопрос №7 |
| Teardown-рекламация | trim + recycle в `AbandonGuard::drop` | при domain-модели то же на выходе домена |
| Текущий поток без own-heap | `current_for_dealloc` → `ForeignNoBind` без fallback-lock (`tls_heap.rs:313`–`tls_heap.rs:328`) | сохранить; доказано `dealloc_only_no_bind*.rs` |

### 4.2 G3+ — «ограниченная помощь перед ростом»

Формулировка ADR: при уходе владельца и отсутствии worker'а, **прежде чем резервировать новый сегмент или минтить слот**, выполнить ограниченную помощь на уже существующих FREE-heap'ах (`...adr...:43`). Сегодня эта помощь есть только в трёх местах: при re-claim (однократный `trim_for_recycle`, `claim.rs:298`), в worker-проходе (`maintenance.rs:20`–`maintenance.rs:53`) и в `trim_current_thread()` (`diag.rs:195`–`diag.rs:210`).

Целевой порядок на холодном пути роста (refill / нового слота):

```text
   cold alloc / refill / new-slot request
      │
      ├─(1) reuse_hint (не authority)                      stack.rs:116-118
      ├─(2) scan_claimable_slot / scan_claim_recovery_free  stack.rs:12-35, stack.rs:62-85
      │        → если найден FREE+initialised слот:
      │             CAS(FREE→MAINTENANCE)   claim.rs:57-67
      │             один bounded background_maintenance_step   ownership.rs:118-127
      │             Drop(lease) → CAS(MAINTENANCE→FREE)  claim.rs:419-430
      │             ← помощь выполнена БЕЗ нового резерва/минтинга
      │
      ├─(3) реальный рост: bump_count → chunk reservation → HeapCore::new
      │        (только если помощь не дала памяти)
      │
      └─(4) help-budget исчерпан → обычный рост (G3 без G3+)
```

Явные условия, без которых G3+ не включается:

- **Бюджет `HELP_BUDGET`**: не более K слотов и один bounded `background_maintenance_step` на помощь; K — решение владельца (вопрос №3). Никакой неограниченной помощи: иначе холодный путь превращается в фоновый worker внутри `GlobalAlloc::alloc`.
- **Реентерабельность**: `background_maintenance_step` и `trim_cold_retention` — только OS-вызовы (decommit/release), без `std::alloc`, без `Node::write_next` (ADR `:9`, `:22`). Поэтому помощь не может рекурсивно войти в аллокатор. Требование сохраняется инвариантом «внутри help-lease не вызывается ничего, что аллоцирует».
- **Не берётся fallback `LOCK`**: помощь адресует registry-слоты; fallback — отдельный, см. §4.3.
- **Не берётся `CONTROL`**: никакой ack/wakeup внутри `GlobalAlloc` (иначе реентерабельность спинлока/мьютекса на том же потоке — deadlock).

### 4.3 Fallback reentrancy

- Инициализация: `INIT_STATE` машина со спинлоками; **никогда не dropped** (`fallback.rs:76`, док `:23`–`fallback.rs:30`); RAII-гарды и на `LOCK` (`fallback.rs:407`–`fallback.rs:473`), и на `INIT_STATE` (`fallback.rs:475`–`fallback.rs:552`).
- Занятый fallback: `try_with_heap` → `None`, **не ждёт**; вызывающий обязан отказаться от синхронного пути и уйти в публикацию (`routing.rs:45`–`routing.rs:46`). Это единственная форма «реентерабельности», допускающая отсутствие блокировки.
- Рекурсия через аллокатор внутри lock: `with_heap` держит `LOCK` во время `f`; если `f` аллоцирует тем же путём, повторный вход в `with_heap` = спинлок на себе = deadlock. Сегодня это структурно исключено: fallback-путь — аварийный, а `alloc` внутрь fallback-этапа не приводит (`routing.rs:36`–`routing.rs:43`). Ph4b должен зафиксировать это инвариантом и оракулом (вопрос: как его детерминированно проверять — см. §5, Loom этого не моделирует).
- Счётчик `LOCK_ACQUISITIONS` (`fallback.rs:109`) — инструмент отрицательных тестов: «dealloc не тронул fallback-lock».
- Тестовые инъекции: `dbg_panic_in_with_heap_releases_lock` (`fallback.rs:571`), `dbg_panic_in_fallback_init_rolls_back` (`fallback.rs:675`), `dbg_inject_fallback_oom_for_test` (`fallback.rs:402`).

### 4.4 Диаграмма (Ph4b)

```text
                     ┌──────────── start_maintenance() ────────────┐
                     │ IDLE ──CAS──▶ STARTING ──spawn ok──▶ RUNNING│
                     │  ▲                │                        │
                     │  └─ StartingGuard (любой err)               │
                     └─────────────────────────────────────────────┘
   worker loop (10 мс):  maintenance_pass(cursor, 32)      maintenance.rs:20-53
        └─ на слот: try_maintenance_at → lease → bounded step → Drop lease
        └─ fallback::try_with_heap (ОДИН раз, неблокирующе)   service.rs:147-150
        └─ Любой выход/unwind worker'а → abort               service.rs:63-68

   NO-WORKER путь (G3 / G3+):
        alloc cold ─▶ scan FREE ─▶ help-lease (K слотов, bounded) ─▶ рост
        owner exit  ─▶ trim_for_recycle ─▶ recycle(LIVE→FREE) ─▶ доступен для
                                                                  (re)claim / help

   fallback:  UNINIT ─CAS▶ INITIALIZING ─write+READY▶ READY(навсегда)
                 └─ unwind/OOM ─▶ UNINIT (InitStateGuard) ─▶ re-race
```

### 4.5 Инварианты Ph4b

| ID | Инвариант | Проверка |
|---|---|---|
| B1 | `start_maintenance` до `Ok` не даёт автономной гарантии | native: `r8_autonomous_maintenance.rs` (дочерние процессы, ack по PASSES); ревью diff |
| B2 | Отказ spawn ретраебельен, активация не «залипает» | `maintenance_start_error.rs:11`–`:15`, `StartingGuard` `service.rs:54`–`service.rs:59`; тест — нужен (см. §5) |
| B3 | Смерть worker'а = abort, не silent degradation | `service.rs:63`–`service.rs:68`; тест FAIL_WORKER |
| B4 | Один проход ≤ 32 слотов, один bounded step на слот, один `try_with_heap` на проход | `service.rs:29`, `maintenance.rs:20`–`maintenance.rs:53`; `tests/r8_maintenance_registry.rs`, `tests/r9_bounded_background_maintenance.rs` |
| B5 | G3: без worker pending остаётся до повторного claim | **отрицательный контроль** (P2) — нужен новый оракул |
| B6 | G3+: помощь перед ростом **ограничена** и без аллокаций | новый оракул: счётчики reserve/release сегментов + `heaps_claimed_high_water` не растёт при help-only сценарии |
| B7 | Занятый fallback не блокирует чужой free | `routing.rs:45`–`routing.rs:46`, `dealloc_only_no_bind_torn.rs` (LOCK_ACQUISITIONS не двигался) |
| B8 | Unwind в fallback не оставляет ни вечный `INITIALIZING`, ни висячий `LOCK` | `regression_fallback_init_unwind_guard.rs`, `fallback.rs:540`–`fallback.rs:551` |
| B9 | Panic внутри `with_heap`/`heap_ptr` не кедведит процесс (не клинит lock/state) | `fallback.rs:571`, `fallback.rs:675` |
| B10 | TORN-поток не берёт fallback-lock и не биндит слот заново | `dealloc_only_no_bind_torn.rs`, `dealloc_only_no_bind.rs`, `tls_heap_teardown_torn_sentinel.rs` |
| B11 | `trim_current_thread()` — только текущий домен/поток, ничего не claim'ит | `tests/r31_10_trim_current_thread_api.rs`, `diag.rs:195`–`diag.rs:210` |

### 4.6 Lock-order (Ph4b) — явно

| Уровень | Замок | Где берётся | Что НЕЛЬЗЯ делать под ним |
|---|---|---|---|
| 1 | chunk CAS (`OncePtrCell`) | `slot`/`slot_or_none` (`registry.rs:135`, `registry.rs:170`) | держать `&mut HeapCore` в потенциально-не-READY слоте |
| 2 | slot CAS (`FREE↔LIVE/MAINTENANCE`, `LIVE↔FREE`) | claim/recycle/maintenance | держать fallback `LOCK`; вызывать аллоцирующий код |
| 3 | help-lease / maintenance-lease | bounded step (`maintenance.rs:40`–`maintenance.rs:49`) | вызывать `claim`; брать `CONTROL`; брать fallback `LOCK` |
| 4 | fallback `LOCK` | `with_heap`, `try_with_heap` (`fallback.rs:331`, `fallback.rs:379`) | рекурсивно входить в `with_heap`; держать slot-lease |
| 5 | `CONTROL` (Mutex) | startup + тест-ack (`service.rs:123`) | держать lease или fallback-лок; spawn'ить |

Пере-становка 2 и 4 (fallback-lock под slot-lease) — deadlock risk; пере-становка 3 и 4 — блокировка холодного пути на fallback'е, которым в это время может владеть тот же поток через `publish_foreign` (`routing.rs:36`).

### 4.7 «Что может пойти не так → какой оракул» (Ph4b)

| Риск | Оракул |
|---|---|
| G3+ помощь затыкает холодный путь / превращается в worker внутри `alloc` | iai/бенч-плечо cold/recycle (ADR `:66` пределы EstCycles ≤ 1.05, Ir ≤ 1.10) + новый счётчик help-вызовов; NO-GO при пробое |
| Помощь рекурсивно аллоцирует | Miri SB / TB witness'ы ADR C1 (`:51`); read-audit: `background_maintenance_step` без `std::alloc` |
| HELP_BUDGET слишком велик → RSS/commit рост | ADR RSS-гейт `:72`, оракул «число reserve/release сегментов» |
| Worker завис на `try_with_heap` | `service.rs:147` неблокирующий; оракул: `fallback_visits_for_test` (`service.rs:177`) |
| Потерянный wakeup / проход без reclaim | `service.rs:194` `wait_after_for_test`, `tests/r8_autonomous_maintenance.rs` (ack, не sleep) |
| Unwind в инициализации fallback → livelock | `regression_fallback_init_unwind_guard.rs` |
| Fallback-lock взят dealloc-only путём | `dealloc_only_no_bind_torn.rs` + `LOCK_ACQUISITIONS` |
| Fork/unload при активированном worker'е | **не покрыто**: ADR/доки объявляют unsupported (`maintenance_service.rs:12`–`service.rs:13`, `sefer_alloc/maintenance.rs:34`–`maintenance.rs:35`) |

---

## 5. МАТРИЦА ТЕСТОВ PH4a/PH4b ПО ГЕЙТАМ P1–P4

Правило плана: у каждого обещания — свой оракул; `PASS/FAIL/NOT_RUN` не сворачиваются в одно «зелёное» (`docs/design/...plan...:29`). «Модели Loom не доказывают pointer provenance; Miri проверяет конкретный путь; native — конкретную ОС/ветку».

### 5.1 Гейты ADR (`...adr...:59`)

| Гейт | Обещание | Positive (переиспользуемый) | Negative / мутант | Новое |
|---|---|---|---|---|
| **P1** worker reclaim ≤ K проходов без вызовов allocator'а | `tests/r8_autonomous_maintenance.rs`, `tests/r8_maintenance_registry.rs`, `tests/r9_bounded_background_maintenance.rs`; для инварианта lease — `tests/loom_r8_maintenance_lease.rs` | ранняя FREE-публикация; reclaim до фактической публикации; worker берёт чужой OWNED слот | `ph4_p1_lease_no_alloc_reclaim` — доказать, что reclaim-проход не вызывает `GlobalAlloc` повторно |
| **P2** без worker pending остаётся (отрицательный контроль) | частично: `tests/r8_maintenance_registry.rs` (bounded scan не берёт занятые/неинициализированные слоты), `tests/loom_r8_maintenance_lease.rs` (только lease даёт доступ) | «автономный reclaim без worker» — обязан оставаться красным/NOT-RUN, а не зелёным | `ph4_p2_no_worker_pending_stays` — без `start_maintenance()` reclaim НЕ происходит (счётчики release/слоты не двигаются) |
| **P3** отказ spawn и гибель worker'а | `tests/r8_autonomous_maintenance.rs` (внешние процессы, FAIL_START/FAIL_WORKER) | «worker умер — тихо продолжаем» (должно быть abort) | `ph4_p3_spawn_failure_retry_releases_leases` — после `Spawn` нет half-activated состояния, lease свободен |
| **P4** G3+ (если принято) | — | помощь выполняется **до** резерва сегмента/минтинга слота; помощь без бюджета (неограниченная) | `ph4_p4_help_before_growth` + мутант «помощь пропущена» → рост mint'ит новый слот при наличии FREE |

### 5.2 Корректностные гейты, на которые опирается Ph4 (ADR C1–C6)

| Переход/обещание | Positive | Negative/мутант | Инструмент |
|---|---|---|---|
| claim/reclaim handoff | `tests/r6_terminal_lease_registry.rs` (EMPTY→INITIALIZING→OOM→FREE→LIVE→MAINTENANCE→FREE) | двойной claim под LIVE; claim под чужим `&mut HeapCore` (план `:40`) | native + Loom (`loom_r11_registry_claim.rs`) |
| селектор/hint/saturation | `tests/r11_p3_registry_claim_selector.rs`, `tests/r11_p3_registry_claim_first_bind.rs` | отрицательный скан помечает saturated при конкурентном publish | Loom (`loom_registry_free_slots.rs`), native |
| chunk-OOM / uninit-OOM | `tests/r11_p3_registry_claim_chunk_oom.rs`, `tests/r11_p3_registry_claim_uninit_oom.rs` | `slot()` на несуществующем чанке → abort | native |
| `initialised` gate | `tests/regression_claim_oom_initialised_gate.rs`, `tests/regression_registry_initialised_gate.rs` | гейт по `generation == 1` (M-5) | native + Miri |
| TORN / teardown | `tests/tls_heap_teardown_torn_sentinel.rs`, `tests/tls_heap_teardown_ordering_stress.rs` | «TORN → Own» разыменование | native (counterfactual через реальный `mark_local_torn`) |
| dealloc-only без bind | `tests/dealloc_only_no_bind.rs`, `tests/dealloc_only_no_bind_torn.rs` | fallback-lock взят на foreign-free | native (`LOCK_ACQUISITIONS`) |
| fallback init | `tests/regression_fallback_init_unwind_guard.rs` | отмена `InitStateGuard` → залипший `INITIALIZING` | native |
| trim API | `tests/r31_10_trim_current_thread_api.rs` | trim claim'ит слот / трогает чужой heap | native |
| terminal lease | `tests/loom_r8_maintenance_lease.rs` | неудачный CAS даёт `&mut` | Loom |
| recycle-граница | `tests/race_norecycle.rs` (контроль-эксперимент: producer'ы живы → нет recycle) | перенос blame на recycle | native |

### 5.3 Что Loom **может** и чего **не может**

| Может доказать | Не может доказать |
|---|---|
| порядок CAS'ов и их результат на всех interleav'ах (`loom_r8_maintenance_lease.rs`, `loom_r11_registry_claim.rs`, `loom_registry_free_slots.rs`) | pointer provenance: для этого нужен Miri SB/TB (`...plan...:29`) |
| что проигравший CAS не входит в критическую секцию (два писателя) | физическое освобождение байтов резервации — это ОС/`segments_released_total`, не Loom |
| lease handoff между двумя «держателями» с заведомо сериализованной передачей | реальный TLS teardown: «нельзя форсировать выход конкретного потока в безопасном тесте» (`tests/tls_heap_teardown_torn_sentinel.rs:17`–`:25`) |
| отсутствие «второго `&mut`» в абстрактной модели слота | реентерабельность fallback-lock через глобальный аллокатор (модель не выполняет настоящий `GlobalAlloc`) |
| модель селектора `pick_with_saturation` (hint + negative-scan) | поведение `Mutex` poisoning/spawn-ошибок ОС (нужен native/child-process оракул) |
| целостность chunk-CAS publish | модель schedule ≠ реальное планирование; «shadow Loom→shipping» автоповышение запрещено |

---

## 6. ЧЕГО НЕ ЗНАЕМ + ВОПРОСЫ ВЛАДЕЛЬЦУ

Незакрытые неизвестные (части — прямо из плана и ADR):

1. **Send для `HeapLease`.** «Transferable» в типе данных Rust обычно значит `Send`. ADR запрещает делать протокольные lease/claim `pub unsafe` (`...adr...:91`, `:99`), но `unsafe impl Send` на `pub(crate)`-типе формально не «pub unsafe API» — и всё равно новое soundness-утверждение. Разрешён ли вариант B (§3.4), или только безопасная передача через domain-table?
2. **Drop lease без потока / смерть домена.** Если домен умер с живым lease (или lease отозван), нужен ли «GC-подобный» reclaim на следующем claim, или достаточно Drop→FREE? Что делать с late-publication'ами из sidecar'ов, если слот уже FREE и новый claim уже начался? (`src/registry/heap_registry/maintenance.rs:1`–`maintenance.rs:4` и `claim.rs:296`–`claim.rs:298` утверждают, что публикаторы не заимствуют ядро — это надо подтвердить тестом переживаемости, а не только комментарием.)
3. **Бюджет G3+.** Каков `HELP_BUDGET` (слотов и bounded-step'ов) на холодном пути, чтобы (а) не превратить `alloc` в worker и (б) не пробить perf-floor ADR refill (EstCycles ≤ 1.05, Ir ≤ 1.10 — `...adr...:66`)? Это решение владельца, не инженерное.
4. **Принимается ли G3+ вообще.** ADR формулирует её условно («если принято в Ph4b», `...adr...:43`, гейт P4 `...adr...:59`). Без положительного решения Ph4b остаётся на G3.
5. **Конфликт fresh-first vs G3+.** `pick_with_saturation` сегодня mint'ит новый индекс раньше скана старых FREE (`stack.rs:116`–`stack.rs:134`, ADR `:24`). G3+ требует противоположного при росте. Переключать порядок (reclaim-first) или делать выбор по политике/конфигу?
6. **Существующий `pub`- unsafe контур.** `HeapRegistry::claim` — `pub fn` (возвращает сырой указатель), `HeapRegistry::recycle` — `pub unsafe fn`, `MaintenanceLease::with_core` — `pub unsafe fn` (`claim.rs:83`, `claim.rs:354`, `claim.rs:406`). На них опираются 69+72 файла тестов. Можно ли в Ph4a сузить их до `pub(crate)`, мигрировав тесты на lease/наблюдатели, или это отдельное решение о тест- поверхности?
7. **Что такое domain в публичной модели.** Нужен ли публичный/приватный `DomainId` (main/worker-pool/назначенный), и связывается ли он с `Profile`/`LargeCacheConfig` (config-conflict на re-claim, `claim.rs:96`–`claim.rs:144`)? Иначе «transfer» некорректно сочетать с политикой кэша.
8. **Fork и unload.** Активированный worker делает fork/unload неподдерживаемыми (`maintenance_service.rs:12`–`service.rs:13`, `sefer_alloc/maintenance.rs:34`–`maintenance.rs:35`). Меняется ли это в Ph4b, если worker стартует лениво/по домену?
9. **Perf/RSS-база и идентичность.** ADR требует неизменяемых SHA для B/C (`...adr...:61`). Записка не фиксирует SHA'ы — их нужно привязать до первого runtime-коммита Ph4a/Ph4b.
10. **Отказ spawn в G4-модели.** ADR: «`Err` — гарантия не активна» (`...adr...:44`). Если приложение не ретраит, память остаётся — публикуется ли это как «условная» гарантия в README, или G4 вообще не объявляется без worker?

---

### Приложение: что в этой записке доказано, а что нет

**Проверено чтением кода** (все `file:line` выше открывались в этом worktree): состояния слота и их CAS-переходы; кто формирует `&mut HeapCore`; механизм TORN/finish_bind/rollback; fallback `INIT_STATE`+`LOCK` и их гарды; `start_maintenance`/`maintenance_pass` и их границы; селектор слота и saturation; состав переиспользуемых тестов-оракулов; число тестов, зависящих от `claim()`/`recycle()`.

**Не проверено / гипотеза:**
- Имена `HeapLease`, `DomainId`, `HELP_BUDGET`, `try_transfer` — рабочие имена варианта дизайна, не существующий код.
- Плотность тестового покрытия (что именно зелёное) не запускалась: запрещено запускать сборки/тесты в этом окружении.
- Оценки стоимости help на холодном пути (§4.2) — рассуждение, не измерение; perf-гейт ADR определяет пригодность.
- Побочные эффекты domain-модели на `MAX_HEAPS`/насыщение — не анализировались количественно.
