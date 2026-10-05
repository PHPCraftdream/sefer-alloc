# Ph4b — миграция tls_heap на HeapLease (чанк D: receipt)

Дата: 2026-10-05 · ветка `ph4b` · task #2092 · база B_4b = Ph4a (чанки A–C в дереве, не коммичены)

## Что сделано

### Карта путей tls_heap → lease

| Путь (было) | Стало |
|---|---|
| `bind_slow_tagged` → `HeapRegistry::claim()` → raw `*mut HeapCore` | → `HeapRegistry::claim_lease()` → `Option<HeapLease>` в `finish_bind` |
| `bind_slow_tagged_with_config` → `claim_with_config(config)` | → `claim_lease_with_config(config)` (тот же N2 config-conflict хук, переехал внутрь lease-версии) |
| `finish_bind(heap: *mut HeapCore)` — публиковал raw в `LOCAL`/`GUARD` | `finish_bind(lease: Option<HeapLease>)` — raw-производная `lease.core() as *mut HeapCore` публикуется в `LOCAL`, сам lease кладётся в `GUARD` |
| `Drop for AbandonGuard`: `(*heap).trim_for_recycle()` + `unsafe { HeapRegistry::recycle(heap) }` | `mark_local_torn()` → `lease.core().trim_for_recycle()` (безопасно) → `drop(lease)` (его `Drop` делает `LIVE → FREE` Release-CAS, abort при проигрыше) |
| rollback-ноги `finish_bind`: `unsafe { HeapRegistry::recycle(heap) }` ×2 | `drop(lease)` — тот же Release-CAS, без unsafe |

### Решение: `GUARD = Cell<Option<HeapLease>>`

`AbandonGuard.heap: Cell<*mut HeapCore>` → `AbandonGuard.lease: Cell<Option<HeapLease>>`. Обоснование:

- `Cell` не имеет borrow-state — reэнтерабельный доступ из TLS-деструктора не может запаниковать (в отличие от `RefCell`);
- `Cell::take()` (`Option::take`) ПЕРЕМЕЩАЕТ lease наружу без вызова `Drop` — CAS `LIVE → FREE` выполняется ровно один раз, в явном `drop(lease)` teardown-пути, а не в момент записи в cell;
- в `finish_bind` в нормальном пути lease кладётся `take`/`set`-парой, `debug_assert!(old.is_none())`; при `try_with::Err` closure не выполняется и владение остаётся у вызывающего (`let Some(lease) = lease else { return Own }`); `Cell::set(Some(lease))` не использован сознательно — он бы дропнул гипотетический ранее сохранённый lease.
- реэнтерабельность GUARD как `thread_local!` с деструктором не изменилась: guard был и остаётся TLS с деструктором, `Cell`-обёртка семантику re-entry не трогает.

### Порядок teardown и rollback (решения владельца §2 п.1, 2, 7)

- `impl Drop for AbandonGuard`: **TORN → trim → Drop(lease)** — `mark_local_torn()` ставит яд в `LOCAL` ДО Release-публикации слота (не полагается на порядок TLS-деструкторов), затем `lease.core().trim_for_recycle()` пока поток ещё единственный владелец, затем `drop(lease)`. После lease-Drop ядро не мутируется.
- rollback-нога `GUARD.try_with = Err`: **`LOCAL := null` ДО `drop(lease)`** — пост-teardown resolver не может увидеть stale-указатель уже освобождённого слота.
- `drop(lease)` намеренно вне `try_with`-closure: `HeapLease::drop` ничего не аллоцирует и не трогает TLS (один CAS + два relaxed-стора) — безопасен на teardown/дтор-пути.
- тихий no-op при проигранном CAS (`recycle`-семантика) сохранён на уровне `HeapLease::drop`/`recycle` — нужен `registry_basic::double_recycle_is_safe_noop`.
- **Горячий путь не тронут** (Э2): resolver'ы `LOCAL` — по-прежнему одна загрузка + сравнение; весь diff сидит на cold-пути `bind_slow_tagged`/teardown. `git diff` не содержит правок resolver'ов.

### Отступление: реэкспорт без `internals`

`src/registry/heap_registry/mod.rs`:
```rust
#[cfg(not(feature = "internals"))]
pub(crate) use claim::HeapLease;
```
Почему: `global::tls_heap` обязан импортировать `HeapLease` в сборках БЕЗ `internals` (typed bind path), но под `cfg(feature = "internals")` реэкспорт отсутствует → `use ... ::HeapLease` в `tls_heap.rs` даёт E0432 на конфигурации без internals. `pub(crate)`-реэкспорт закрывает это; наружу тип по-прежнему не торчит.

## SAFETY-обоснования

- **В `src/global/tls_heap.rs` больше нет unsafe-блоков.** Единственный доступ к ядру — безопасный `HeapLease::core()`; Release-CAS и контракт S1–S4 живут в `claim.rs` от Ph4a. `#![allow(unsafe_code)]` остаётся на файле (документированный raw-pointer TLS-шов `LOCAL`), но lift больше ничего не покрывает.
- Что стало структурно безопаснее:
  - **recycle по stale-указателю невозможен**: `HeapLease::drop` abort-ится при проигранном CAS `LIVE → FREE` — двойной дроп/чужой дроп невозможен как тихий no-op, он детектируется;
  - **S3 — единственный производный raw — `LOCAL` того же потока**: указатель выводится из эксклюзивного borrow `lease.core()`, а TORN-стамп происходит раньше Drop — окно «stale non-null LOCAL при FREE-слоте» закрыто структурно, а не комментарием;
  - rollback-ноги больше не требуют single-caller-контракта `unsafe recycle` («pointer previously returned by claim, not yet recycled») — власть выражена типом, время жизни lease-значением.

## Grep-проверка легаси

```
grep -rn "HeapRegistry::claim\|claim_with_config\|HeapRegistry::recycle\|into_raw" src/
```
Реальные вызовы кода (не doc-комментарии):
- `src/registry/heap_registry/claim.rs:150` — `lease_into_raw(Self::claim_lease())` (легаси-обёртка `claim`);
- `src/registry/heap_registry/claim.rs:181` — `lease_into_raw(Self::claim_lease_with_config(config))` (легаси-обёртка `claim_with_config`);
- `src/registry/heap_registry/claim.rs:565/605` — `HeapLease::into_raw` / вызов в `lease_into_raw` (легаси-drain);
- `src/global/tls_heap.rs:415/426` — typed `claim_lease()` / `claim_lease_with_config(config)` из tls_heap.
- **`HeapRegistry::recycle` — 0 вызовов** (только doc-комментарии). Обаunsafe recycle-сайта в tls_heap удалены.

## Мутанты и loom-мутанты

Статусы — из прогонов чанка C (в чанке D не перепрогонялись; ловцы в дереве зелёные на корректном коде).

| Мутант | Ловец | Красный? | Характер |
|---|---|---|---|
| M-A: `drop(lease)` до `trim_for_recycle` (мутации после освобождения слота) | `tests/r11_ph4b_late_publication_across_recycle_exactly_once.rs` (реальный `#[global_allocator]`) | ДА | **crash дочернего процесса 0xC0000409** (abort по сломанному single-writer протоколу), не oracle-assert |
| M-B: rollback — `LOCAL := null` ПОСЛЕ `drop(lease)` | `tests/r11_ph4b_local_no_drop_pinned.rs` (source pin п. d) | ДА | токен-пин: перестановка ловится, reformat не ломает |
| M-C: удаление/подавление `trim_for_recycle` | тот же exactly-once тест | **НЕТ** | **не пойман**: `trim_for_recycle` идемпотентен — оракул «consume ровно один раз» не различает 0 и 1 trim'а; слабость ловца задокументирована (см. открытые вопросы) |
| M-D: TORN-стамп после release | `tests/r11_ph4b_teardown_ordering_pinned.rs` (source pin) | ДА | пин требует порядок torn → trim → release |
| M-E': `GUARD` — хранение lease не через `take`/`set`-перемещение (double-drop поверхность) | `tests/r11_ph4b_local_no_drop_pinned.rs` (source pin п. b) | ДА | пин `Cell<Option<HeapLease>>` |
| L1: `DROP_ORDERING` Release→Relaxed (Drop-CAS) | loom `tests/loom_r11_ph4b_publish_recycle_drain.rs` | **НЕТ** | модель проверяет целостность данных (payload/pending), а не S2-упорядочивание state-CAS — см. revise-заметку ниже |
| L2: ослабление Acquire claim-CAS | loom, тот же файл | **НЕТ** | аналогично L1 |
| L3: проигравший CAS «дропает»/drain'ит (снят owner-guard `active`) | loom, тот же файл | ДА | нарушение exactly-once mutator-guard в модели |

**Revise-заметка (L1/L2):** doc-комментарий loom-модели обещает, что мутант Relaxed «goes red» — на прогоне чанка C это не подтвердилось: модель транскрибирует упорядочивания, но её оракулы наблюдают data-целостность, а не сам HB-эдж state-CAS. Частично закрывает source-pin `tests/r11_ph4b_teardown_ordering_pinned.rs` (требует `Ordering::Release` у `LIVE → FREE` в исходнике — см. открытые вопросы (b)).

## Тесты / Loom / Miri

### Полные регрессы (последовательно, CARGO_TARGET_DIR=target-ph4b)

| Набор | Лог | Итог |
|---|---|---|
| `cargo test --features "production internals"` | `docs/perf/_raw_ph4b_cargo_test_prod_internals.log` (109 KiB) | 329 бинарников, 566 passed, **0 failed** |
| `cargo test --features "production alloc-stats bench-internals internals"` | `docs/perf/_raw_ph4b_cargo_test_prod_allocstats.log` (118 KiB) | 329 бинарников, 708 passed, **0 failed** |

Известные исключения **не краснели на этом прогоне**: `r1_04_alloc_core_drop_stack_pressure` — 2 passed + 1 ignored; `r6_route_directory_model` — 3 passed (оба в prod-internals-логе).

### Loom

`scripts/loom.mjs` → `loom_r11_ph4b_publish_recycle_drain`, features `alloc-global,alloc-xthread,internals,tagged-index-stack/loom` — **PASS** (`publication_survives_recycle_and_is_consumed_exactly_once ... ok`), лог `docs/perf/_raw_ph4b_loom.log`.

### Miri

Тест `tests/r11_ph4b_lease_miri.rs::lease_teardown_lifecycle_same_thread_under_miri` (claim → lease-surface → drop → re-claim по hint → drop → второй slot; lease `!Send`, lifecycle в одном потоке):
- **strict-provenance**: ok, 7.89s — `docs/perf/_raw_ph4b_miri_strict_provenance.log`;
- **SB (default)**: ok, 7.89s — `docs/perf/_raw_ph4b_miri_sb.log`.
Feature-набор `alloc-global alloc-xthread internals` — в стиле miri-plain job.

### Новые тесты (7 файлов)

| Файл | Ответственность |
|---|---|
| `tests/r11_ph4b_late_publication_across_recycle_exactly_once.rs` | поздняя sidecar-публикация через recycle Consumed-EXACTLY-ONCE (32 Large-блока, `segments_released_total` Δ=32, теги читаются обратно) |
| `tests/r11_ph4b_no_worker_pending_stays.rs` | P2-негативный контроль: без maintenance-worker публикации в FREE-слоте остаются pending (`segments_released_total` константен), изолированный дочерний процесс |
| `tests/r11_ph4b_spawn_failure_retry_releases_leases.rs` | `MaintenanceStartError::Spawn` не оставляет полуактивации: все lease свободно берутся, мгновенный retry успешен |
| `tests/r11_ph4b_local_no_drop_pinned.rs` | source-pin «LOCAL без Drop»: `LOCAL = Cell<*mut>` const-инициализация, `GUARD = Cell<Option<HeapLease>>`, rollback `LOCAL := null` до drop |
| `tests/r11_ph4b_teardown_ordering_pinned.rs` | source-pin teardown-порядка torn → trim → release (вкл. `Ordering::Release` у `LIVE → FREE` в `claim.rs`) |
| `tests/r11_ph4b_lease_miri.rs` | Miri-lifecycle teardown-пути lease (SB + strict-provenance) |
| `tests/loom_r11_ph4b_publish_recycle_drain.rs` | loom shadow-модель publish ‖ recycle ‖ drain: exactly-once consume, CAS-loser не drain'ит |

## Чек-лист

| Проверка | Статус |
|---|---|
| `rustfmt --check --edition 2021` (src) | GREEN |
| clippy × 4 (check-matrix feature-наборы) | GREEN |
| check-matrix (все строки) | **ALL GREEN** |
| `verify-dbg-hook-safety` | PASS: **134** reviewed safe / **30** reviewed unsafe / **74** bench-gated safe (перепроверено в чанке D: `node scripts/verify-dbg-hook-safety.mjs`) |
| `verify-alloc-core-dbg-internals-exhaustive` | **ALL GREEN** (перепроверено в чанке D) |
| `verify-ci-sentinels` | OK — **113 sentinel(s) checked**, floor `MIN_SENTINEL_COUNT` 109 → **113**; карточка TRACKED_ci_gate_coverage item 87 обновлена (re-derived 2026-10-05) |
| `--test no_stale_doc_references` | зелёный, **33 passed** |
| README/ARCHITECTURE счётчики | integration test files **332 → 339** (README.md:1373, README.md:1383, docs/ARCHITECTURE.md:489); loom-инвентарь **12 → 13 root Loom models** (добавлен `loom_r11_ph4b_publish_recycle_drain`) |
| `scripts/loom.mjs` FEATURES | строка `loom_r11_ph4b_publish_recycle_drain` добавлена |
| `.github/workflows/ci.yml` | loom-registry job: `--test loom_r11_ph4b_publish_recycle_drain` + `grep -F` sentinel (форма 109 → 113) |

## Решения владельца — соответствие

1. **LOCAL без Drop / lease в GUARD / hot path** — выполнено: `LOCAL` остался `Drop`-less `const Cell<*mut HeapCore>`; lease хранится в `GUARD: Cell<Option<HeapLease>>`; resolver'ы (горячий путь, одна загрузка + сравнение) не тронуты.
2. **Порядок teardown и rollback** — выполнено: torn → trim → drop(lease); rollback: `LOCAL := null` до `drop(lease)`; тихий no-op recycle сохранён (нужен `registry_basic::double_recycle_is_safe_noop`, зелёный в регрессе).
3. **Routing не трогали** — все маршруты по-прежнему под `try_with_heap`; diff не содержит правок `segment_route`.
4. **G3+ не принят, HELP_BUDGET нет** — `grep -rni "HELP_BUDGET|help budget|guaranteed reuse"` по `README.md` и `src/` = **0 совпадений**; README/rustdoc не содержат упоминаний помощи/гарантий переиспользования. Формулировка честности: в README/rustdoc нет обещаний гарантированного переиспользования — добавление предложения признано ненужным: документация не создаёт ожидания, которое надо ограничивать (публикация pending-битов уже описана как advisory/owner-drain в существующих разделах sidecar), а новая оговорка без поведенческого изменения была бы шумом.
5. **G4-условность не менялась** — тесты r8 зелёные в обоих регрессах + новый P2/P3 (`no_worker_pending_stays`, `spawn_failure_retry_releases_leases`) зелёные.
6. **Grep-гигиена** — новых `pub unsafe`, `unsafe impl Send/Sync`, `DomainId`, `HELP_BUDGET` нет; grep из раздела выше чист. Сужение legacy `pub` (`claim`/`claim_with_config`/`into_raw`) — отложено в Ph4c.

## Идентичность дерева

`git diff | sha256sum` (перепроверено в чанке D):
```
1169e763e5c99bb3dc05d32356f52705d9014fbf0470f52d71ee9dd472d2d1fb  -
```
(хеш над `git diff` отслеживаемых файлов; новые untracked-файлы — 7 тестов, 5 логов и этот receipt — в diff не входят)

## Открытые вопросы

1. **M-C непойман** — нужен неидемпотентный оракул consume (например, счётчик `consume-side-effect` в ядре, которого сейчас нет); предложить как Ph4c.
2. **L1/L2** — loom-модель не доказывает S2 на production state-CAS; source-pin `r11_ph4b_teardown_ordering_pinned` частично закрывает (требует `Release` у `LIVE → FREE`); полный HB-ловец на уровне модели остаётся открытым.
3. **M-A детектируется как crash дочернего процесса (0xC0000409), не как чистый oracle-assert** — ограничение: abort из broken single-writer протокола не превращается в аккуратный fail теста.
4. **Miri не покрывает реальный TLS teardown** (lease-drop внутри деструктора потока) — как и в Ph4a; кандидат на Ph4c.
5. **Сужение legacy pub** (`claim`/`claim_with_config`/`into_raw` после миграции всех вызовов) — Ph4c.

## Приёмка оркестратором

- Diff `src` прочитан оркестратором: миграция tls_heap→HeapLease соответствует решениям владельца (LOCAL без Drop не тронут, lease в GUARD, порядок TORN→trim→Drop(lease), rollback LOCAL:=null до Drop(lease), resolvers/горячий путь без правок).
- Grep-проверка легаси перепроверена независимо: `HeapRegistry::recycle` в src/ — 0 вызовов; `claim`/`claim_with_config`/`into_raw` — только определения/обёртки внутри claim.rs; tls_heap вызывает только typed `claim_lease`/`claim_lease_with_config`.
- sha256(git diff) перепроверен: `1169e763e5c99bb3dc05d32356f52705d9014fbf0470f52d71ee9dd472d2d1fb`.
- Замечание: Loom-модель loom_r11_ph4b — shadow (проверяет целостность данных, не S2-упорядочивание production state-CAS); production-ордеринг закреплён source-pin тестом `r11_ph4b_teardown_ordering_pinned` (Release у LIVE→FREE CAS в `HeapLease::drop` + порядок teardown в `AbandonGuard::drop`).
- Открытые пункты приёмки: M-C (двойной consume ingress) — ловца нет (trim_for_recycle идемпотентен), задокументировано как слабость оракула, не бага; M-A ловится как crash дочернего процесса, не как чистый assert.

## Приёмка оркестратора (2026-10-05)

- Весь diff `src` прочитан; тесты перегнаны независимо — зелёные (5 новых `r11_ph4b_*`, `tls_heap_teardown_*`, `dealloc_only_no_bind*`,
  `race_norecycle`, `registry_basic`); Loom `loom_r11_ph4b_publish_recycle_drain` зелёный; Miri `r11_ph4b_lease_miri` зелёный в SB (strict-provenance) и TB.
- Найден и закрыт пробел: source-pin `r11_ph4b_teardown_ordering_pinned` искал вызовы простым `find` по тексту, поэтому мутант «убрать
  `mark_local_torn()`» (оставив его в комментарии) оставался зелёным. Пин теперь выбрасывает строки-комментарии из тела
  `AbandonGuard::drop`; контрфактуально: чистый код — зелёный, мутант без `mark_local_torn()` — красный.
- Мелкая правка: в doc-комментарии `tls_heap.rs` исправлено дублирование «a a slot».
