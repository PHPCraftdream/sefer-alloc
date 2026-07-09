# Ревью поддерживаемости и архитектуры — sefer-alloc

**Дата:** 2026-07-09
**Ревьюер:** независимый ревьюер №7 (угол: поддерживаемость — архитектурные
границы, дрейф документации, сложность как долгосрочный риск). Не пересекается
с параллельными ревью unsafe/инвариантов/perf/чистоты/безопасности/bug-hunt.

## Scope

- Корневой `Cargo.toml` + `crates/{region,vmem,numa,malloc-bench}/Cargo.toml`
  и их реальные исходники — границы workspace-крейтов.
- Граф 14 feature-флагов корневого крейта и поиск незащищённых опасных
  комбинаций (по образцу существующего `compile_error!`-guard'а для `fastbin`).
- Сверка 6 design-документов с актуальным кодом: `ARCHITECTURE.md`,
  `DESIGN.md`, `INVARIANTS.md`, `PHASE35_DECOMMIT_DESIGN.md`,
  `PHASE_NUMA_DESIGN.md`, feature-докстроки `Cargo.toml` (+ выборочно
  `FASTBIN_DESIGN.md`, `docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md`).
- Топ-5 файлов по размеру/сложности в `src/` и `crates/*/src/`.
- Onboarding-проход: README → планы → код на двух конкретных трассах
  (`realloc`, cross-thread free).

## Методология

Только чтение: `Read`/`grep`/`wc -l`/`git log --oneline -- <file>` (счёт
churn-коммитов по файлам), сверка утверждений документов с конкретными
строками кода. Сборки/тесты не запускались, код не изменялся. Все ссылки —
`file:line` на состояние рабочего дерева на 2026-07-09 (ветка `main`,
HEAD a664707).

Severity-шкала: **Critical** — структурная проблема, угрожающая будущим фичам;
**High** — растущий структурный долг, удорожающий каждую следующую фичу;
**Medium** — конкретный дрейф/пробел, вводящий в заблуждение контрибьютора;
**Low** — косметика.

---

## 1. Границы workspace-крейтов

### 1.1 Вердикт: границы чистые, заявление «независимо публикуемый» подтверждается кодом ✅

Проверен полный граф зависимостей всех пяти `Cargo.toml`:

| Крейт | Зависимости | Проверка |
|---|---|---|
| `sefer-region` | только `slotmap` (`crates/region/Cargo.toml:20`) | не знает о sefer-alloc |
| `aligned-vmem` | **ноль** зависимостей (`crates/vmem/Cargo.toml:15`) | самодостаточен |
| `numa-shim` | optional `aligned-vmem` за фичей `vmem-integration` (`crates/numa/Cargo.toml:17,24`), объявлен `path` **+** `version = "0.1"` | публикуем: cross-крейтовая зависимость направлена «вниз» (numa→vmem), не в родителя |
| `malloc-bench-rs` | ноль runtime-зависимостей; `mimalloc` — dev-only (`crates/malloc-bench/Cargo.toml:15-20`) | самодостаточен |
| `sefer-alloc` (root) | `sefer-region` всегда (`Cargo.toml:220`), `aligned-vmem` за `alloc-core` (`Cargo.toml:241`), `numa-shim` за `numa-aware` (`Cargo.toml:245`) | все path-deps имеют `version` — публикуемо |

Обратных утечек **нет**: ни один `crates/*` не зависит от `sefer-alloc` и не
импортирует его типы. Прямых импортов `aligned_vmem::`/`numa_shim::` вне
задокументированных seam-модулей (`src/alloc_core/os.rs`,
`src/alloc_core/numa.rs`, `src/registry/bootstrap.rs`) нет — совпадения в
`tests/registry_basic.rs:342` и
`tests/regression_bootstrap_oom_sentinel_rollback.rs:8` — это упоминания в
комментариях, не код. MSRV `1.88` согласован во всех пяти манифестах. У каждого
под-крейта есть свои `README.md`, `LICENSE-*`, `tests/` — минимальный комплект
для crates.io в наличии. `fuzz/` корректно изолирован как собственный
workspace-root (`fuzz/Cargo.toml:18`), не тянется в родительский граф.

### 1.2 Medium — `malloc-bench-rs` никем в workspace не потребляется; larson/mstress существуют в двух копиях

Корневой крейт не имеет даже dev-зависимости на `malloc-bench-rs`. При этом:

- `crates/malloc-bench/src/lib.rs:331-339` — `Workload::{Larson, Mstress}` +
  `run`/`sweep` (публикуемый харнесс, «the pure-Rust answer to mimalloc-bench»);
- `examples/malloc_macro.rs` (корень) — **вторая, независимая** реализация
  larson+mstress, написанная раньше (коммит 465e3ba, phase 13.7); крейт был
  извлечён позже (fe035d9, #76-79), но пример не переведён на него.

Числа MT-таблицы README (`README.md:611`) берутся из `examples/malloc_macro.rs`,
т.е. из внутренней копии, а не из публикуемого харнесса. Две реализации одного
workload'а неизбежно разъедутся (параметры working-set, cross-thread доля,
PRNG-семена), и тогда «числа README» и «числа, которые получит внешний
пользователь malloc-bench-rs» перестанут быть сравнимыми — при формально
одинаковых названиях workload'ов.

**Рекомендация:** перевести `examples/malloc_macro.rs` на
`malloc-bench-rs` (dev-dependency) либо явно задокументировать в обоих местах,
что это две разные реализации и почему.

### 1.3 Low — пустой каталог `examples_tmp/` в корне

Пустая директория-артефакт рядом с настоящим `examples/`. В git не отслеживается,
в tarball не попадёт — чистая косметика, но у корня репозитория она выглядит как
второй набор примеров.

---

## 2. Граф feature-флагов и незащищённые комбинации

### 2.1 Реальный граф (сверен с `Cargo.toml:57-207`)

```
default ──────────→ std ──→ sefer-region/std
experimental ─────→ std          (+ deps: arc-swap, crossbeam-epoch)
pinning ──────────→ experimental (+ dep: core_affinity)
alloc-core ───────→ std          (+ dep: aligned-vmem)
alloc-xthread ────→ alloc-core
alloc-global ─────→ alloc-core
alloc-decommit ───→ alloc-core
alloc-runfreelist → alloc-core
numa-aware ───────→ alloc-core   (+ dep: numa-shim/vmem-integration)
fastbin ──────────→ alloc-global + alloc-xthread   [guard: src/lib.rs:187-193]
hardened ─────────→ fastbin
production ───────→ alloc-global + alloc-xthread + alloc-decommit + fastbin
alloc-stats ──────→ ∅  (ни одной зависимости)
```

Граф ацикличен, все аллокаторные фичи аддитивны над `alloc-core`, и проектная
дисциплина «layout-стабильность между конфигурациями» (поля `live_count` /
`decommitted` / `node_id` / ring присутствуют в каждой сборке) сознательно
устраняет самый опасный класс feature-комбинаторики — расхождение байтовой
раскладки. Это сильная сторона.

### 2.2 Аналогов fastbin-класса (data race) среди незащищённых комбинаций НЕ найдено ✅

Систематически проверены попарные взаимодействия. Единственная комбинация
уровня «unsound» — `fastbin` без `alloc-xthread` — закрыта дважды
(feature-унификация `Cargo.toml:152` + `compile_error!` `src/lib.rs:187-193`).
Отдельно отмечено **положительно**: перекрёстное взаимодействие
`alloc-runfreelist` × `alloc-decommit` (устаревшие RunStack-дескрипторы после
reset/recommit → двойная выдача блока) уже обработано — Ф4/#211 чистит RunStack
в `decommit_empty_segment` (`src/alloc_core/alloc_core.rs:1235-1258`).

Оставшиеся находки — комбинации «легальные, но с ловушкой», защищённые только
документацией:

### 2.3 Medium — `alloc-global` без `alloc-xthread`: задокументированный footgun без runtime-наблюдаемости

Кросс-тредовый free в этой конфигурации — тихая перманентная утечка
(документировано: `src/lib.rs:56-61`, `src/global/sefer_alloc.rs:146`; код:
`src/registry/heap_core.rs:810-813` → `core.dealloc` → safe no-op для чужого
указателя). `compile_error!` здесь справедливо неприменим (однопоточная
конфигурация легитимна), но в отличие от fastbin-случая у этого footgun'а нет
**ни одного runtime-сигнала**: `AllocStats`
(`src/global/sefer_alloc.rs:290-324`) не имеет счётчика проигнорированных
чужих/нероутабельных free. Прод-система, собранная без `alloc-xthread` по
ошибке, будет течь без единой наблюдаемой метрики — при том что `stats()`
позиционируется как «the field to alert on for a segment leak»
(`src/lib.rs:30-38`).

**Рекомендация:** добавить в `AllocStats` счётчик вида
`foreign_or_unroutable_frees` (инкремент на no-op ветке dealloc). Это
превращает misconfiguration из «невидимой» в «алертуемую» — та же философия,
что уже применена к `ring_overflows`.

### 2.4 Low — `alloc-stats` в одиночку — тихий no-op

`alloc-stats = []` (`Cargo.toml:166`) не требует `alloc-core`; сборка
`--features alloc-stats` без остального стека молча не делает ничего. README
(матрица, `README.md:837`) говорит «add alongside production», но ни guard'а,
ни явного «в одиночку — no-op» нет. Дешёвая правка: зависимость от
`alloc-core` или одна строка в докстроке фичи.

### 2.5 Low — `pinning` тянет `arc-swap` + `crossbeam-epoch`, которые ему не нужны

`PinnedRunner` объявлен «NOT deprecated» (`src/concurrent/mod.rs:12-15`), но
его feature-путь `pinning → experimental` подтягивает обе зависимости
легаси-исследовательского тира (`Cargo.toml:72,78`), хотя самому раннеру нужны
только `core_affinity` + `ShardedRegion`. Пользователь «живого» API платит
деревом зависимостей deprecated-тира. Заодно: `docs/ARCHITECTURE.md:440-441`
(«NUMA benefit pairs naturally with the `pinning` feature») читается как
рекомендация включать `pinning` для аллокатора, тогда как `PinnedRunner`
управляет только воркерами `ShardedRegion`, а не тредами `SeferAlloc`.

### 2.6 Low — `numa-aware` × `alloc-decommit`: гарантия «same node after recommit» не закодирована для Windows

`docs/PHASE_NUMA_DESIGN.md:432-435` утверждает: «After recommit the segment
returns to the same node». На Linux это верно (mbind — VMA-политика,
переживает `MADV_DONTNEED`). На Windows recommit — обычный
`VirtualAlloc(MEM_COMMIT)` без `VirtualAllocExNuma`
(`crates/vmem/src/lib.rs:386-399`, вызов из
`src/alloc_core/alloc_core.rs:3134,3217`): утверждение держится только на
first-touch владельцем и нигде не оговорено. Либо ре-байндить при recommit,
либо смягчить формулировку в доке.

---

## 3. Дрейф документации

Проверено 6 документов + выборочно ещё 2. Общая картина: **checkpoint'ные и
экспериментальные доки ведутся образцово** (CHANGELOG с секцией Unreleased;
`FASTBIN_DESIGN.md` с honest-маркерами «P8 … REVERTED», «P6 … KEEP»;
`PHASE35_DECOMMIT_DESIGN.md` сверен с кодом — live_count owner-only,
decommit при `live==0` не-current, recommit-on-reuse — расхождений не найдено).
Дрейфует именно **обзорный слой** — то, что читает новичок первым.

### 3.1 High — `Cargo.toml:185-202` (`alloc-runfreelist`): описание фичи отстало и от кода, и от вердикта эксперимента

Докстрока фичи утверждает: «the storage/layout phase … will (in later phases
Ф2/Ф3) let the recycle path …», «verdict is deferred to Ф5». Фактически:

- Ф2 (pack) — в коде: `src/alloc_core/alloc_core.rs:2196-2360`;
- Ф3 (drain) — в коде: `src/alloc_core/alloc_core.rs:2897-2947`;
- Ф4 (lifecycle seams) — в коде: `src/alloc_core/alloc_core.rs:1235-1258`;
- Ф5-вердикт **вынесен и это NO-GO**:
  `docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md:41` («Verdict (mechanical
  application …): NO-GO»).

То есть публичное описание фичи (первое, что видит потребитель на crates.io и
docs.rs) описывает «Ф1-заготовку с отложенным вердиктом», тогда как в дереве —
полная реализация, уже **отклонённая** по её же заранее зафиксированным
критериям. Maintainability-последствие серьёзнее самой докстроки: 21
cfg-сайт `alloc-runfreelist` внутри самого горячего файла + `run_stack.rs`
(342 строки) + тесты — это код, который по плану Ф6 включался бы в
`production` только при GO, а при NO-GO его судьба (выпилить? заморозить?
оставить для будущего re-run?) нигде не зафиксирована. Каждый будущий рефакторинг
`alloc_core.rs` будет обязан сохранять и прогонять эти ветви
(`--all-features` в CI их компилирует), оплачивая эксперимент, который уже
проиграл.

**Рекомендация:** явное решение в декомпозиции: либо удалить арк (CHANGELOG
это умеет — прецедент удаления `Heap` есть), либо записать в
`RUN_ENCODED_FREELIST_PLAN.md`/`Cargo.toml` статус «NO-GO, сохранён для
re-run при условии X» — и в любом случае синхронизировать докстроку фичи.

### 3.2 Medium — `docs/ARCHITECTURE.md`: кластер устаревания «30-минутного тура»

Документ явно датирован и привязан к коммиту (`docs/ARCHITECTURE.md:16`:
«Date: 2026-06-28. Current mainline: commit 4e034e5») — с тех пор ~40 коммитов,
включая целые арки. Конкретные расхождения:

1. **Внутреннее противоречие о составе `production`:**
   `docs/ARCHITECTURE.md:69-71` — «production alias bundles
   `alloc-global + alloc-xthread + alloc-decommit`» (без `fastbin`), тогда как
   `docs/ARCHITECTURE.md:122-123` и `Cargo.toml:153` включают `fastbin`.
   §1 писался до включения fastbin в production и не был обновлён.
2. **Два разных числа для одной метрики в одном документе:**
   §6 `docs/ARCHITECTURE.md:361-362` — «4 MiB alloc+free: ~58 ns … ~13×», а
   §9 `docs/ARCHITECTURE.md:508` — «4 MiB alloc+free: 42 ns … 18x». Обновили
   одно место, забыли второе.
3. **Счётчик тестов устарел:** `docs/ARCHITECTURE.md:451` — «tests/*.rs
   (103 files)», фактически 117.
4. **Три из 14 фич и целый hardening-арк отсутствуют:** `hardened`,
   `alloc-stats`, `alloc-runfreelist` и X7 generational ring не упомянуты ни
   разу (grep по документу — 0 совпадений), хотя README их полноценно
   описывает (`README.md:837-838`).

По отдельности каждое — Low; вместе они означают, что у «канонического
обзора» нет ритуала обновления. Новичок, поймавший документ на трёх
противоречиях, перестаёт доверять и остальным 500 строкам.

**Рекомендация:** пункт в чек-лист фазового коммита (CLAUDE.md уже требует
ZERO-TRUST review между фазами): «если фаза меняет фичи/числа/инварианты —
обнови ARCHITECTURE.md и его date-stamp». Дата-штамп в :16 — уже готовый
детектор просрочки.

### 3.3 Medium — `docs/DESIGN.md` — фоссилия, на которую указывает rustdoc корня крейта

`docs/DESIGN.md:55-78` описывает «byte tier (Phase 4)» — тир, который
`ALLOC_PLAN.md` сам объявляет superseded (`docs/ALLOC_PLAN.md:20-21,372-375`),
и обещает «`#![forbid(unsafe_code)]` everywhere except … the one documented
`hand.rs`» / «the unsafe is one screenful» (`docs/DESIGN.md:90-92`). Фактический
инвентарь — **10 confined-модулей в src + 3 unsafe-крейта**
(`src/lib.rs:99-162`, честно каталогизировано в
`docs/ARCHITECTURE.md:120-151`). Аналогичное устаревшее обещание — в
`docs/ALLOC_PLAN.md:285-298` («two thin seams … two screenfuls … in every
configuration»).

Сам по себе исторический план — не криминал. Проблема в маршрутизации: докстрока
корня крейта `src/lib.rs:67-68` отправляет читателя именно в `DESIGN.md`
(«…and `docs/DESIGN.md` for the architecture»), а не в `ARCHITECTURE.md`.
Первое же, что новичок узнает «про архитектуру» — картина мира двухмесячной
давности с недостоверным unsafe-обещанием (чувствительная тема для крейта,
чей маркетинг — честность unsafe-инвентаря).

**Рекомендация:** в `src/lib.rs:68` заменить ссылку на `ARCHITECTURE.md`; в
шапку `DESIGN.md` и `ALLOC_PLAN.md` §6 добавить баннер «исторический документ,
актуальный инвентарь — ARCHITECTURE.md §2» (в `README.md:907` подпись уже
частично honest: «model for `Region<T>`»).

### 3.4 Medium — хвосты удаления `Heap`/`with_heap` (свежий дрейф, легко закрыть)

CHANGELOG (Unreleased) фиксирует полное удаление публичного лица `Heap` /
`with_heap` / фичи `alloc`. Уборка не дошла до:

- `Cargo.toml:172-173` — описание фичи `hardened` всё ещё ссылается на
  «the explicit `Heap`/`with_heap` face» как на действующий фасад;
- `src/alloc_core/node.rs:420` — **битая intra-doc ссылка**
  `[`Heap::dealloc_any_thread`](crate::heap::Heap::dealloc_any_thread)`
  (модуля `crate::heap` больше нет); соседние :422-439 — тот же нарратив;
- `src/alloc_core/mod.rs:17` — «Shared by both public allocator faces
  (`registry::heap_core::HeapCore` and `heap::Heap`)» — второго лица больше
  нет;
- `src/alloc_core/deferred_large/drain.rs:21,47`,
  `src/alloc_core/segment_header.rs:267`,
  `src/alloc_core/alloc_core.rs:3271` — упоминания удалённого фасада как
  живого.

(Не путать с `fallback::with_heap` — `src/global/fallback.rs:187` — это другой,
живой внутренний API.)

### 3.5 Low — `docs/INVARIANTS.md`: M6 сформулирован как будущее

`docs/INVARIANTS.md:60-63`: «(Phase 8 frees all segments at `AllocCore` drop;
eager decommit lands in Phase 10.)» — decommit давно реализован (Phase 35,
фича `alloc-decommit`, по умолчанию в `production`). Файл-спека инвариантов —
единственный документ, который обязан быть вечнозелёным (M2 при этом
образцово обновлён residual-нотой R2/X7); M6 отстал.

### 3.6 Low — `docs/PHASE_NUMA_DESIGN.md` противоречит и коду, и обзорному доку в выборе mbind-политики

`docs/PHASE_NUMA_DESIGN.md:453-454`: «only MPOL_BIND for MVP». Код:
`crates/numa/src/lib.rs:510,529` — `MPOL_PREFERRED`;
`docs/ARCHITECTURE.md:417` — тоже `MPOL_PREFERRED`. Решение поменялось при
имплементации, дизайн-док не догнали. Мелочь, но это ровно тот случай, когда
два дока дают противоположный ответ и арбитром приходится делать код.

---

## 4. Hotspots сложности (топ-5 файлов)

Churn считался как число коммитов, трогавших файл, из 256 всего в репо.

### 4.1 High — `src/alloc_core/alloc_core.rs`: 4114 строк, 81 fn, один `impl` на 3692 строки, 25% всего churn'а репозитория

- Один `impl AllocCore` со строки 351 по 4042; 65/256 коммитов (25%) трогали
  этот файл — это главная конфликт- и ревью-поверхность репозитория.
- Это не «естественная сложность домена», а **гравитация**: каждый новый арк
  ложится в этот же файл — large-cache decay/mode Phase 1-3 (строки 73-350 +
  секции 3639-3924 c test-seams), decommit (`:1201-1258`), NUMA-хуки
  (`:417-427`, `:2656-2671`), PERF-3 Ф2/Ф3/Ф4 (`:2196`, `:2897`, `:1235`),
  realloc fast-path (`:1732`), россыпь `dbg_*` test-seams. 149 cfg-сайтов
  `alloc-decommit` по src (большинство здесь) + 21 `alloc-runfreelist` делают
  файл матрицей конфигураций, которую надо удерживать в голове целиком.
- Правило CLAUDE.md «one file — one export» здесь уже надломлено: файл
  экспортирует **два** публичных типа — `AllocCore` (`:237`) и
  `LargeCacheMode` (`:94`, реэкспорт `src/alloc_core/mod.rs:80`), плюс два
  приватных (`LargeCacheDecayConfig:128`, `CachedLarge:167`).

**Рекомендация (готовые линии разреза):** large-cache подсистема
(`LargeCacheMode`/decay/eviction/test-seams — секции уже размечены
`── Phase 2/3 ──` баннерами) — в собственные файлы-экспорты; runfreelist-ветви —
см. 3.1. Это ~1000+ строк, выносимых без изменения семантики, и возврат файла
под правило репозитория.

### 4.2 Medium — `src/registry/heap_core.rs`: 1762 строки, всего 30 fn (~59 строк/fn), 51/256 коммитов (20%)

Рост — в основном нарративные комментарии-археология. Конкретный симптом:
объяснение MUST-1 (stamp + A1-drain) продублировано **дословно** в doc-comment
(`src/registry/heap_core.rs:1097-1140`) и тут же в inline-комментарии
(`:1160-1183`) — два места для каждой будущей правки одной истории. Сама
структура (magazine + routing + realloc + teardown в одном типе) — оправданная
доменная связность; риск — комментарийный дрейф, а не логика.

### 4.3 Low — `src/alloc_core/segment_header.rs`: 1386 строк, 43 fn

Естественная сложность: field-specific accessors через `offset_of!` (27
сайтов) — это осознанная §11-дисциплина (атомарные почтения отдельных полей
вместо чтения всей структуры), плюс X7 gen-table. Каждое поле честно оплачено
парой accessor'ов и SAFETY-комментарием. Рост пропорционален числу полей —
приемлемо.

### 4.4 Low — `src/registry/heap_registry.rs`: 939 строк

Слот-таблица + claim/release + процесс-wide стат-агрегаторы. Связная единица,
рост под контролем.

### 4.5 Low — `src/alloc_core/segment_table.rs`: 896 строк

Таблица сегментов + OPT-B open-addressing hash + free-slot list с ABA-генами.
Три механизма в одном файле, но все — про одну сущность. Приемлемо.

Отдельно: `crates/numa/src/lib.rs` (796) и `crates/vmem/src/lib.rs` (648) —
однофайловые крейты; для «audit in isolation»-истории это плюс, не минус.

---

## 5. Onboarding-проход

### 5.1 Что работает хорошо ✅

- `README.md` (1001 строка): quick-start, матрица фич (`:825`), карта
  документации (`:900-918`), «Honest limitations» (`:922-950`) — редкий по
  качеству вход.
- `CONTRIBUTING.md:49-99` — per-risk команды верификации (когда гонять
  proptest/loom/miri/TSan) — новичку сразу ясно, чем оплачивается его класс
  изменений.
- `docs/ARCHITECTURE.md` как жанр «30-минутного тура» — правильная идея;
  таблица «Where to read next» (`:529-542`) спасает от утопания в 30+ доках и
  32 checkpoint-файлах (checkpoints к тому же исключены из tarball —
  `Cargo.toml:19-24` — и не засоряют опубликованный крейт).

### 5.2 Трасса №1: «как устроен realloc» — прослеживается за ~15 минут

`SeferAlloc::realloc` (`src/global/sefer_alloc.rs:392`) →
`HeapCore::realloc` (`src/registry/heap_core.rs:1141`; трёхфазный doc-comment
A1-drain / in-place / move-leg на `:1097-1140`) →
`AllocCore::realloc` (`src/alloc_core/alloc_core.rs:1598`) и in-place fast-path
(`:1732`). Цепочка находится grep'ом, doc-comments объясняют не только «что»,
но и «почему» (leak-to-abort история). Вердикт: **проходимо**, комментарии —
актив.

### 5.3 Трасса №2: «как работает cross-thread free» — проходимо с доками

`HeapCore::dealloc` (`src/registry/heap_core.rs:802-814`) → `dealloc_routing` →
ring-push → owner drain (`src/alloc_core/alloc_core.rs:2897+`); поддержано
`docs/ARCHITECTURE.md` §5 и `docs/CROSS_THREAD_STATE_MACHINES.md`. Хорошо.

### 5.4 Medium — идентификаторная археология без глоссария

Комментарии и доки оперируют минимум **девятью** параллельными системами
идентификаторов: фазы 8–13/35/58/78; P0–P8 (внутри FASTBIN);
Ф0–Ф6 (PERF-3); Э-серия (ALLOC_BENCH); X7; OPT-A..H; W3/H1/A1/A2/R1/R2/C2;
MUST-N; SEC-N; плюс task-номера `#103–#211`. Расшифровка требует либо
CHANGELOG.md (2044 строки), либо checkpoint'ов — которых в опубликованном
tarball **нет** (исключены `Cargo.toml:23`): покупатель крейта с docs.rs
получает комментарии вида «Э9 (P7.1, task #160)» со ссылками в никуда.
Единая страница-глоссарий (ID → одна строка → где читать) стоила бы ~50 строк
и сняла бы главный onboarding-налог.

### 5.5 Итог по onboarding

Путь README → ARCHITECTURE → код работает и не требует тонуть в
checkpoint'ах. Два реальных налога: (1) свежесть обзорного слоя (см. 3.1–3.3 —
новичок не может отличить «устарело» от «я не понял»), (2) отсутствие
глоссария идентификаторов (5.4).

---

## Итоговый вердикт

**Critical-находок нет.** Архитектурные границы workspace — образцовые
(четыре под-крейта реально независимы, направление зависимостей строгое,
fuzz изолирован); граф фич корректен, а единственная unsound-комбинация
защищена двойным guard'ом; слой экспериментальных/checkpoint-доков и CHANGELOG
ведутся на уровне, который редко встречается.

Три главных долга по убыванию:

1. **High — траектория `alloc_core.rs`** (4.1): 4114 строк, 25% всего churn'а,
   каждый новый арк прирастает туда же; правило «one file — one export» уже
   нарушено вторым публичным экспортом. Не блокирует следующую фичу, но делает
   каждую следующую дороже. Разрезы очевидны и размечены самим кодом.
2. **High — NO-GO эксперимент `alloc-runfreelist`, вросший в горячий файл**
   (3.1): полная реализация Ф1–Ф4 в дереве, вердикт NO-GO вынесен, публичная
   докстрока фичи описывает состояние двухфазной давности, решение о судьбе
   кода не зафиксировано.
3. **Medium — дисциплина обзорного слоя** (3.2–3.4): ARCHITECTURE.md с
   внутренними противоречиями и date-stamp'ом 11-дневной давности; rustdoc
   корня указывает на фоссилию DESIGN.md; хвосты удаления `Heap` в Cargo.toml
   и пяти src-файлах (включая одну битую intra-doc ссылку).

Быстрые дешёвые победы: 3.4 (хвосты `Heap` — час работы), 3.5/3.6 (две
строки в доках), 2.4 (докстрока `alloc-stats`), ссылка `src/lib.rs:68` →
ARCHITECTURE.md. Среднесрочные: счётчик наблюдаемости для footgun'а 2.3,
глоссарий идентификаторов 5.4, объединение larson/mstress 1.2. Стратегические:
решение по runfreelist и план распила `alloc_core.rs`.
