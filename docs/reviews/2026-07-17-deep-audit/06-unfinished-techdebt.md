# Deep audit — тема 6: недоработки и техдолг

**Дата:** 2026-07-18. **Метод:** read-only статический аудит (grep/чтение,
без cargo/build/test). **Скоуп:** `src/`, `crates/`, `tests/`, `scripts/`,
`.github/workflows/`, `docs/reviews/`, `README.md`, рабочее дерево на момент
аудита (незакоммиченный сплит `alloc_core.rs`).

## Сводная таблица

| # | Severity | file:line | Долг | Риск |
|---|---|---|---|---|
| 1 | **HIGH** | `README.md:301-350` | «Workspace: four independently-publishable companion crates» — по факту в `crates/` 11 крейтов, 7 не упомянуты в README вовсе | Онбординг/аудит по README получает искажённую картину поверхности; 4 из 7 неупомянутых крейтов несут `#![allow(unsafe_code)]` и должны быть в таблице «Where unsafe lives», но их там нет |
| 2 | **MEDIUM** | `tests/loom_remote_ring_drain_guard.rs`, `loom_heap_overflow.rs`, `loom_heap_overflow_drain_guard.rs`, `loom_overflow_first_retry.rs`, `loom_dirty_publish.rs`, `loom_dirty_multi_segment.rs` (2479 строк суммарно, кроме первого файла) | 6 из «семи in-tree shadow-model» loom-файлов, которые `.github/workflows/ci.yml:495-499` утверждает как «collapsed into one suite» (`ring-mpsc --test loom_ring_mpsc`), физически остались в `tests/` и НЕ упомянуты ни в одном `--test` CI-шаге — мёртвые файлы, не запускаются нигде | Ложное чувство покрытия (комментарий в CI говорит «схлопнуто», а файлы просто осиротели); 2479 строк неисполняемого loom-кода висят как техдолг; при следующей правке протокола кольца эти файлы легко перепутать с живыми |
| 3 | **LOW-MEDIUM** | `.github/workflows/ci.yml:522` vs `.github/workflows/ci.yml:503-504` | `loom_remote_ring` (in-tree, `--features "alloc-core alloc-xthread"`) и `-p ring-mpsc --test loom_ring_mpsc` (крейт) **оба** реально выполняются в CI — дублирующее покрытие одного и того же протокола, комментарий про «collapse» не соответствует факту частично (файл не удалён, а его job не убран) | Не баг, но два раза платим CI-время за одну и ту же модель; расхождение между тем, что комментарий обещает, и тем, что происходит |
| 4 | **LOW** (tracked) | `tests/lazy_commit_b4_matrix.rs:1175` (`eager_path_is_pure_noop`), `tests/lazy_commit_b2_grow.rs:303` (`commit_failure_leaves_state_unchanged`) | F2 из `docs/reviews/2026-07-17-followup-batch-review.md:25,44-55` — тесты допускают недостижимую ветку `unix ∧ alloc-lazy-commit ∧ ¬numa-aware`, где ожидают `frontier == SEGMENT`, хотя код (`alloc_core_small.rs:1528-1536`) намеренно занижает frontier без platform-гейта на этом плече (b3 уже разделил numa/¬numa для другого теста, эти два — нет) | Уже заведено как задача **#191 (pending)** — не забыто, но не закрыто. В текущем CI-матче не проявляется (Linux CI гонит `--all-features` → `numa-aware` включена; Windows этот блок компилирует). Спящая ловушка для будущего локального прогона `cargo test --features "production alloc-lazy-commit"` на Linux |
| 5 | **LOW** | `crates/vmem/tests/fault_injection.rs` (весь файл) | F1 из follow-up-review — 4 теста делят процесс-глобальные `FAIL_NEXT`/`FAIL_AT_*` атомики без сериализации между тестовыми потоками libtest; `arm_fail_next(0)`-дизарм одного теста может погасить только что взведённый хук другого | CONFIRMED by construction, не воспроизведено (4/0 в последнем известном прогоне) — flaky-by-construction, не логическая ошибка; предложенный фикс (`Mutex`-гвард или `--test-threads=1`) не применён |
| 6 | **LOW** | `Cargo.toml:307-313` | F3 — `alloc-lazy-commit` безусловно включает `aligned-vmem/fault-injection` для ЛЮБОГО потребителя (не только тестового профиля), компилируя в non-test бинарь публичный process-wide armable commit-kill-switch | Дизайн-выбор, не баг; цена измерена как «два relaxed load на commit» — но поверхность управления есть в проде без явной документации об этом в README |
| 7 | **NIT** | `src/alloc_core/os.rs:148-150` vs `src/alloc_core/bootstrap.rs:94` | F4 — doc `reserve_lazy` заявляет debug_assert и на «non-zero multiple of PAGE», и на «<= SEGMENT»; фактический `debug_assert!` в `bootstrap.rs:94` проверяет только `<= SEGMENT` | Расхождение doc/enforcement; page-multiplicity держится «by construction» + vmem перепроверяет и возвращает `Err`, так что не эксплуатируемо, но документация лжёт о том, что именно проверяет ассерт |

## TODO/placeholder

Полный grep `TODO|FIXME|XXX|HACK|placeholder|unimplemented!|todo!` по `src/`,
`crates/`, `tests/` — **ни одного настоящего TODO/FIXME/XXX/HACK/`todo!()`/
`unimplemented!()`** не найдено. Единственные хиты — слово «placeholder» в
доккомментариях, все безобидные:

- `src/alloc_core/alloc_core.rs:390` — ссылка на «placeholders» внутри
  прозы, указывающей на `docs/reviews/2026-07-12-round3-remediation-plan.md`
  (не сам код-плейсхолдер).
- `src/alloc_core/alloc_core_large.rs:169,190` — законный шаблон
  «зарезервировать поле placeholder-значением (`u32::MAX`), пропатчить
  однословной записью после `register()`» — стандартный паттерн
  init-then-patch, не недоделка.
- `src/alloc_core/segment_header.rs:639` — конструктор ставит 0 как
  placeholder, вызывающий код обязан переписать; задокументированный
  контракт, не забытый TODO.
- `src/concurrent/lock_free_region.rs:422` — «Slot 0 — placeholder Vacant,
  overwritten below» — тот же паттерн внутри одной функции.

Ни `panic!("not ...")`, ни `unimplemented!()`, ни `todo!()` в проверенных
директориях не найдено — эта категория техдолга в проекте пуста.

## Мёртвый / полу-подключённый код

Все найденные `#[allow(dead_code)]` **имеют** документированную причину и
триггер снятия — не забытый мусор, а осознанно удерживаемый код на будущую
фазу или на другую feature-комбинацию:

| file:line | Причина (по комментарию) | Оценка |
|---|---|---|
| `src/alloc_core/node.rs:271,286,303,321,337` | `read_u32`/`read_ptr`/`write_ptr`/`read_ptr_mut`/`write_ptr_mut` — мёртвые ТОЛЬКО под `alloc-core` без `alloc-xthread` (используются на cross-thread пути) | Легитимно, feature-conditional, не техдолг |
| `src/alloc_core/node.rs:375` | «wired in X7 Ф1; consumed by Ф2/Ф3 + gen-table layout test» | Проверить при аудите фаз X7 — если Ф2/Ф3 уже полностью влиты, комментарий можно закрыть, но признаковброшенности нет |
| `src/alloc_core/os.rs:178` | «Substrate API; Phase 9+ heaps read it» | Явно будущая фаза, не текущий долг |
| `src/alloc_core/remote_free_ring.rs:687,866` | без явного комментария-обоснования (просто `#[allow(dead_code)]`) — единственные два места в списке БЕЗ пояснительной причины | **Стоит проверить отдельно** — не соответствует общему стилю проекта (везде рядом есть причина); см. предложение ниже |
| `src/alloc_core/segment_header.rs:141,888` | «wired in Ф1; consumed by Ф2/Ф3» / «compile-time sanity only» | Легитимно |
| `src/alloc_core/segment_header_gen_table.rs:52,95` | «wired in Ф1; consumed by Ф2/Ф3 + layout test» | Легитимно |
| `src/alloc_core/segment_header_views.rs:20,197` | «Phase 9+ cross-thread routing» / без причины (:197) | :197 без комментария — та же заметка, что и remote_free_ring |
| `src/alloc_core/segment_table.rs:424` | «Substrate introspection; tests / Phase 9 use it» | Легитимно |
| `src/alloc_core/size_classes.rs:111,190` | «Phase 10 (M6 decommit policy) consumes this» | Легитимно, будущая фаза |
| `src/global/tls_heap.rs:339` | «alloc face uses `current_for_alloc` (tagged). Kept for direct API» | Легитимно, alt-API |
| `crates/vmem/src/lib.rs:1376` | «unused under the `mock` feature (syscalls bypassed)» | Легитимно, feature-conditional |

**Предложение:** `remote_free_ring.rs:687,866` и `segment_header_views.rs:197`
— три `#[allow(dead_code)]` без сопроводительного «зачем/когда снимется»
комментария, в отличие от всех остальных 15 случаев в кодовой базе, где
причина явно написана. Дешёвая уборка: либо добавить такой же
причинный комментарий (сверить с Round4/Round7 историей, вероятно те же
Ф2/Ф3-хуки), либо, если функция реально осиротела, удалить.

### Полу-подключённые фичи — не найдено новых

- `LargeCacheMode::{Background,Both}` no-op (round2 T5) — **закрыто**:
  `src/alloc_core/large_cache_mode.rs` enum больше не содержит эти варианты
  (см. doc-комментарий на `:20-23`, явно фиксирующий, что они были удалены).
- `alloc-runfreelist` feature + `run_stack.rs` (round2 T6) — **закрыто**:
  `grep alloc-runfreelist Cargo.toml` — 0 хитов; `src/alloc_core/run_stack.rs`
  не существует; README.md подтверждает удаление (R6-CQ-4).
- Abandon/adopt substrate (round4 #97: `push_abandoned_segment`,
  `pop_abandoned_segment`, `try_adopt`) — **закрыто**: `grep -rn
  "abandoned_seg\|try_adopt" src/registry/*.rs` — 0 хитов, substrate
  полностью удалён.

## Stale-ссылки

- `.github/workflows/ci.yml:495-499,522` — см. п.2-3 сводной таблицы: CI-
  комментарий заявляет «SEVEN shadow models collapsed into one `ring-mpsc`
  suite», но 6 файлов остались нереференсированы CI, седьмой (`loom_remote_ring`)
  выполняется дважды (in-tree + крейт). Комментарий не был обновлён после
  фактического стейта.
- `scripts/tsan.mjs` — **проверено чисто**: заявленный в задаче #188 стейл
  `heap_cross_thread` фактически исправлен (см. комментарии на
  `scripts/tsan.mjs:29-34,50-53`, явно документирующие удаление теста и
  замену на `regression_xthread_large_free_no_leak`); других stale-имён в
  DEFAULT_TESTS/PROD_TESTS не найдено — все перечисленные `tests/*.rs`
  существуют (проверено по списку: `region_invariants`,
  `decommit_miri_cycle`, `reclaim_offset_unit`,
  `regression_large_align_no_segment_exhaustion`,
  `regression_page_aligned_no_segment_exhaustion`,
  `regression_realloc_cross_class_shrink`, `stress_boundary_sweep`,
  `regression_magazine_oracles`, `regression_bump_direct_refill`,
  `regression_xthread_large_free_no_leak`,
  `regression_xthread_thread_free_alias_miri`, `decommit_soak`,
  `decommit_stale_ring` — ни один не отсутствует в `tests/`).
- CI feature-джобы (`.github/workflows/ci.yml`, полный проход по всем
  `cargo test --features ...` шагам) — все упомянутые фичи существуют в
  `Cargo.toml`, стейл-фич-ссылок не найдено.

## Docs-drift

1. **README.md «four companion crates» (HIGH, п.1 сводной таблицы).**
   `README.md:301-313` перечисляет ровно 4 крейта
   (`sefer-region`/`aligned-vmem`/`numa-shim`/`malloc-bench-rs`); фактически
   `ls crates/` даёт 11: `globalalloc-model`, `malloc-bench`, `numa`,
   `proc-memstat`, `proc-probe`, `racy-ptr-cell`, `region`, `ring-mpsc`,
   `size-classes`, `tagged-index-stack`, `vmem`. Задачи `#171-#180`
   (CRATE-P1…P10, все `completed`) документируют, что это намеренное
   расширение экосистемы — README workspace-раздел просто не обновили
   после этой фазы.
2. **README «Where unsafe lives» — 4 внешних крейта не перечислены.**
   `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/` показывает
   `#![allow(unsafe_code)]` в 7 крейтах (`globalalloc-model`, `malloc-bench`,
   `numa`, `proc-memstat`, `racy-ptr-cell`, `ring-mpsc`, `vmem`); README's
   external-crates unsafe-таблица (`README.md:343-350`) перечисляет только 4
   — `proc-memstat`, `racy-ptr-cell`, `ring-mpsc`, `globalalloc-model`
   отсутствуют, хотя несут тот же module-level allow, что и остальные.
   Нарушает собственную методологию README («полный список» — но список не
   полный).
3. **Round3 residuals — намеренно не заведены, не техдолг.** N5 (PerClass
   layout hypothesis, `docs/reviews/2026-07-12-round3-synthesis.md:80-83` /
   `round3-remediation-plan.md:72-75`) и P0-1/P0-2 (O(S)-скан / retry-storm
   архитектурные остатки, `round3-synthesis.md:53`) явно помечены «не
   заводится без нового замера» — задокументированное решение НЕ чинить, не
   забытая задача.
4. **`src/lib.rs` self-verifying grep вместо хардкода — подтверждено ОК.**
   Проверено: `src/lib.rs` больше не хардкодит «14 файлов», список ведётся
   по факту через тот же grep-паттерн, что и CLAUDE.md предписывает — N4 из
   round3 закрыт (`3947b9b`).
5. **Незакоммиченный файловый сплит `alloc_core.rs` не отражён нигде в
   docs.** Рабочее дерево на момент аудита показывает `alloc_core.rs`
   изменён + 4 новых нетрекнутых файла
   (`alloc_core_large.rs`/`alloc_core_large_cache.rs`/`alloc_core_small.rs`/
   `alloc_core_small_pool.rs`) — это, по всей видимости, продолжение задачи
   `#102`/round4 «монолиты» (README/CLAUDE.md ничего про этот сплит пока не
   говорят, т.к. он не закоммичен). Не баг, просто состояние «в процессе» на
   момент снятия снимка аудита — стоит доучесть при следующем docs-sync
   после коммита.

## Незакрытые задачи из планов/ревью

| Источник | Задача | Статус на момент аудита |
|---|---|---|
| `docs/reviews/2026-07-17-followup-batch-review.md` F1 | Гвард на `crates/vmem/tests/fault_injection.rs` от параллельного глушения хуков | **Не исправлено** (не заведено как task, помечено non-blocking) |
| `docs/reviews/2026-07-17-followup-batch-review.md` F2 | Platform-гейт для 2 lazy_commit тестов на unreachable-ветке | **Заведено как task #191, статус `pending`** — не закрыто |
| `docs/reviews/2026-07-17-followup-batch-review.md` F3 | Скоупнуть `aligned-vmem/fault-injection` в dev-dependency вместо безусловного включения | **Не исправлено**, помечено как дизайн-решение к рассмотрению |
| `docs/reviews/2026-07-17-followup-batch-review.md` F4 | doc/enforcement mismatch `reserve_lazy` debug_assert | **Не исправлено**, nit |
| `docs/reviews/2026-07-13-round4-remediation-plan.md` #102 | Разбить монолитные модули `alloc_core.rs` и др. на подфайлы | **В процессе** — видно по незакоммиченному рабочему дереву (см. Docs-drift п.5); не завершено и не закоммичено на момент аудита |
| CI loom-коллапс (см. Stale-ссылки) | Удалить/обновить 6 осиротевших loom-файлов или актуализировать CI-комментарий | **Не заведено как задача нигде** — новая находка этого аудита |
| README «4 крейта» / unsafe-таблица | Docs-sync после CRATE-P1…P10 | **Не заведено как задача** — новая находка этого аудита |

## Дублирование, помеченное на дедуп, но не выполненное

- Раунд2 T6 (`AllocBitmap`/`MagazineBitmap` дублирование) и раунд4 #98
  (bitmap-дедуп в общий `segment_bitmap`) — **проверено закрыто**:
  `src/alloc_core/mod.rs` документирует `segment_bitmap` как «shared
  per-segment bitmap mechanism... task #98 / R4-6 dedup», `AllocBitmap`/
  `MagazineBitmap` — newtype-обёртки поверх него. Не техдолг.
- Loom shadow-model дублирование (см. п.2/3 выше) — единственный
  подтверждённый случай «дедуп заявлен, но не выполнен полностью»: код
  крейта `ring-mpsc` дублирует функциональность, а 6 старых файлов не
  удалены и висят мёртвым весом в `tests/`.

## Общая оценка

Проект в целом дисциплинирован в части техдолга: настоящих
TODO/FIXME/unimplemented! нет вообще, все `#[allow(dead_code)]` за
исключением трёх мест снабжены причиной, крупные архитектурные развилки
(abandon/adopt, LargeCacheMode no-op, alloc-runfreelist) закрыты и
подтверждены удалёнными. Главные находки этого прохода — **два новых**, не
зафиксированных ранее: (1) README чувствительно устарел относительно
недавней фазы дробления на крейты (CRATE-P1…P10) — «4 крейта» вместо
фактических 11, с пробелом в unsafe-инвентаре; (2) CI-комментарий про
«коллапс семи loom shadow-моделей» не соответствует состоянию `tests/` — 6
файлов остались, не запускаются, и физически не удалены. Плюс подтверждение,
что F1-F4 из `2026-07-17-followup-batch-review.md` остаются открытыми (F2
уже отслежена как task #191; F1/F3/F4 — нет).
