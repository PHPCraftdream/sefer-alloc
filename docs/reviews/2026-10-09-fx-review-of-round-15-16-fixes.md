# Ревью правок по итогам src-раундов 15 и 16 (fx, 2026-10-09)

**Ревьюер:** fx (Claude Fable 5.1, effort=xhigh), zero-trust, один контекст, без суб-агентов, изолированный git worktree, ветка `review-fx`.
**Провенанс:** ревью начато fx и прервано лимитом API (WIP-коммит `4bfeb8b5`, файл `docs/reviews/2026-10-09-full-review.md`); продолжено и завершено oh (Claude Opus 5.5, effort=high) по этому файлу в отдельном worktree на той же базе. Разделы fx: §1.1–§1.2, §2.1–§2.5, §2.7, черновики R-001..R-003. Разделы oh: перепроверка §2.1/§2.5, §2.6 (CI), §2.8–§2.9 (perf 84/83 артефакты), §3 итог, §4, §5–§7 дополнения, §9, приложения. Правки oh в тексте fx помечены «(oh)».
**Ограничение владельца (действует для обоих):** никаких `cargo`/компиляции/тестов/мутантов — только чтение, `git` read-only, `gh` read-only, `rustfmt --check`, `node scripts/... --check` (без сборки).
**База:** `ff3d60bb571f706cd8f034665fac8c3f4b6f0d23` (`docs: record src review round 17`); проверяемый диапазон `fe171d9e..453439c1` (17 коммитов, см. §1).
**Статус документа:** завершён (oh, 2026-10-10). Номера строк — на базе `ff3d60bb` (для файлов, не менявшихся после `453439c1`, совпадают с `453439c1`).

## 0. Ход ревью (чек-лист)

| Слой / измерение | Статус | Заметка |
|---|---|---|
| Отчёты R15/R16, манифесты R15/R16 (§5–§6) | просмотрено | прочитаны целиком |
| `486f5ace` R15-01 `EpochRegion::drop` — diff построчно + соседние Drop | просмотрено | §2.1 |
| `32cacc97` R15-02 `Drop for Segment` — diff + все `Segment::reserve*`/`mem::forget` | просмотрено | §2.2 |
| `222e913d` R16-03 const-assert — diff + все `1u64 <<` в `src/` | просмотрено | §2.3 |
| `9df9f6b8` R16-01 abort + сканер `no_panic_doc_accuracy` | просмотрено | §2.4; остаток R17F-01/R17F-02 |
| `b707d196` perf 84 runtime — diff + инвариант «magazine-указатель = SEGMENT-выровненный корень» | просмотрено | §2.5 |
| `87f486a2` perf 84 инструментарий: отчёт, CSV, identity, скрипты, raw-логи, activation-тест | просмотрено (oh) | §2.8; `--check` OK; identity воспроизведена из git |
| Выборочная перепроверка fx (R15-01 порядок обхода; perf 84 инвариант маски) | просмотрено (oh) | §2.8; ошибок fx нет |
| `edf882cf` perf 83 NO-GO: отчёт, CSV, identity.json.gz (sha256), скрипты, raw-логи, `git diff -- src` | просмотрено (oh) | §2.9; `--check` NO-GO; sha256 патча совпал; R17F-04 |
| `6f568c25`/`eda25f97`/`b5247602` r14-харнесс | просмотрено (код — fx; CI — oh) | §2.6 |
| `c8b9344a` R15-03 / item 154 — diff построчно | просмотрено | §2.7 |
| Документация/индексы (`265a571a`, `549d07b6`, `9831adcc`, `9c846e81`, `453439c1`) — механические пересчёты | просмотрено (oh) | §4: 23 строки — 20 «да», 2 «нет», 1 «частично» |
| CI-логи (`gh run view`, только чтение) | просмотрено (oh) | §2.6; красный `37770561319` и 3 зелёных сверены; R17F-05 |
| r14-харнесс: черновик R-003 доведён | просмотрено (oh) | §3 R17F-03 (сценарий 2'), R17F-05 |
| Достижимость abort/релизных паник для R17F-01/02 | просмотрено (oh) | §3: две трассы abort из `GlobalAlloc`; названный abort недостижим; кандидаты fx частично исключены |
| `rustfmt --check` новых файлов, `node scripts/*_gate_table.mjs --check` | просмотрено (oh) | §6: rustfmt 31 файл exit 0; оба `--check` OK (§2.8, §2.9) |
| Конвенции CLAUDE.md, приватные пути, TODO, out-of-scope, префиксы | просмотрено (oh) | §6: нарушений нет |
| Остаточные риски и пробелы тестов | просмотрено (oh) | §7 |
| Запуск тестов/мутантов | НЕ ЗАПУСКАЛОСЬ | ограничение владельца: «без cargo» (для fx и oh) |

## 1. Охват, метод, что исполнено

### 1.1 Диапазон и классификация коммитов (R30-12)

`git log --reverse --format='%h %s' fe171d9e..453439c1`:

| # | SHA | Префикс | Что | Класс R30-12 |
|---|---|---|---|---|
| 1 | `486f5ace` | `fix` | R15-01 / item 172: изоляция паник деструкторов в `EpochRegion::drop` + тест | `fix` (experimental tier) |
| 2 | `32cacc97` | `fix` | R15-02 / item 173: `Drop for Segment` считает release + тест | `fix` |
| 3 | `222e913d` | `fix` | R16-03 / item 176: const-assert `SMALL_CLASS_COUNT <= 64` | `fix` (без runtime-кода) |
| 4 | `9df9f6b8` | `fix` | R16-01 / item 174: два `expect` → `abort`, текст контракта, сканер в `no_panic_doc_accuracy` | `fix` |
| 5 | `6f568c25` | `test` | R16-02 / item 175: гейт host==target (первая версия, CACHEDIR.TAG) | `test` |
| 6 | `265a571a` | `docs` | счётчик тест-файлов | `docs` |
| 7 | `c8b9344a` | `fix` | item 154: снят `allow(dead_code)` на `registry::segment_route`, правка комментариев | `fix` (комментарии + один атрибут) |
| 8 | `87f486a2` | `bench` | perf 84: iai-пара flush-all, activation-тест, скрипты, отчёт, raw-логи | `bench` |
| 9 | `b707d196` | `perf(runtime)` | perf 84: маска сегмента вместо `canonical_block_of` в двух циклах | `perf(runtime)` |
| 10 | `549d07b6` | `docs` | счётчик тест-файлов | `docs` |
| 11 | `896d32a1` | `checkpoint` | чекпоинт сессии | — |
| 12 | `edf882cf` | `bench` | perf 83: предрегистрированный гейт Large shrink-in-place, NO-GO | `bench` |
| 13 | `9831adcc` | `docs` | счётчики тест-файлов/примеров | `docs` |
| 14 | `9c846e81` | `docs` | закрытие индексов, CHANGELOG, манифест §5 | `docs` |
| 15 | `eda25f97` | `test` | R16-02 CI fix: вердикт E0461 вместо layout-гейта | `test` |
| 16 | `b5247602` | `test` | R16-02 hardening: фильтр устаревших rlib по mtime | `test` |
| 17 | `453439c1` | `docs` | закрытие item 175, запись красного CI | `docs` |

### 1.2 Метод

- Построчно прочитаны все diff'ы `src/` и `tests/` коммитов 1–5, 7, 9, 15, 16; скрипты и отчёт perf 84; текущие версии затронутых файлов `src/` (не только diff): `platform/os.rs` целиком, `heap_core/free/dealloc_own_base.rs` целиком, `heap_core/state/tcache_flush.rs` целиком, `concurrent/epoch/epoch_region.rs` (with_capacity/insert/remove/Drop), `concurrent/epoch/hand.rs` (drop_value, Send/Sync), `epoch_handle.rs`, `heap_core_xthread/routing.rs`, `alloc_core_large.rs:540–690`, `alloc_core_small/reserve.rs:170–270`, `decomp_hooks.rs:55–145`, `lifecycle.rs:240–300`, `dealloc_batch.rs:280–360`, `tests/r14_sidecar_owner_capability_negative.rs` целиком, `tests/compile_fail/r14_positive_probe/src/main.rs`, `tests/no_panic_doc_accuracy.rs` целиком.
- Grep-скрининг всего `src/`: `impl Drop for`, `mem::forget`/`Segment(`, `1u64 <<`, `process::abort()`, релизные `expect(`/`panic!`/`unreachable!`/`assert!`/`unwrap()`, записи в `tcache.classes[c].slots[..]`.
- Исполнено только чтение: `git log/show/diff`, grep. `cargo`-команды (тесты, мутанты) — НЕ запускались по указанию пользователя; контрфактуалы тестов оценены по коду (см. §7 «Пробелы тестов»).
- (oh) Дополнительно: прочитаны отчёты/скрипты/identity perf 84 и perf 83 (`scripts/r16_perf8{3,4}_gate_table.mjs` целиком до запуска `--check`), распакован в памяти `R16_PERF83_…_identity.json.gz`; независимо воспроизведены хэши patch/tree/bench из `git`; пересчитаны числа из raw-логов; прочитаны CI-логи 4 прогонов (`gh run view --log`, только чтение; логи сохранялись во временный каталог ВНЕ репозитория); трассы вызовов `GlobalAlloc` → abort прочитаны по шагам; пересчитаны все счётчики §4 командами проекта. Исполненные команды и результаты — приложение A.

### 1.3 Общий вердикт (oh)

- **Все пять правок раундов 15–16 (items 172, 173, 174, 175, 176) закрывают свои находки по существу.** Item 154 честно оставлен OPEN с частичным прогрессом.
- **perf 84 (`b707d196`, `perf(runtime)`, GO):** изменение корректно (инвариант маски подтверждён двумя ревьюерами); детерминированное Ir-доказательство воспроизводимо из закоммиченных артефактов (`--check` OK, identity совпадает с историей git байт-в-байт); oracle активации и мутанты есть; заявлений о wall-clock/RSS нет. Цена — потеря release-проверки в двух flush-циклах (§7, остаточный риск, не находка).
- **perf 83 (`edf882cf`, NO-GO):** runtime не менялся; sha256 кандидата совпал; вердикт механически следует из записанного критерия. Предрегистрация не доказуема хэшем замороженного судьи, а интерпретация провального контроля сильнее данных (R17F-04).
- **Новые находки: 5, все P4** (R17F-01…R17F-05); P0–P3 не найдено. Две из пяти — неточности документации/контракта (R17F-01, R17F-02), одна — пробел наблюдаемости CI (R17F-05), одна — локальная хрупкость тестовой инфраструктуры (R17F-03, ГИПОТЕЗА), одна — интерпретация в perf-отчёте (R17F-04).
- **Механика документации (§4):** 23 проверенных заявления — 20 совпали, 2 не совпали (обе уже R17F-02/R17F-05), 1 частично (CHANGELOG «5 tests» на обоих SHA).
- **Границы:** «не найдено» относится только к прочитанному и перечисленному в §1.2/прил. A. Ничего не компилировалось и не запускалось (кроме `node … --check`, `rustfmt --check`, `verify-commit-prefixes`); все контрфактуалы — по коду.

## 2. Вердикт по каждой правке

| Правка | Находка / пункт | Закрыта по существу? | Доказательство | Остаточный риск |
|---|---|---|---|---|
| `486f5ace` | R15-01 / item 172 | ДА | §2.1 | `std::mem::forget(payload)` вторичных payload'ов — утечка Box'а по дизайну; outer-unwind сценарий не покрыт тестом |
| `32cacc97` | R15-02 / item 173 | ДА | §2.2 | — |
| `222e913d` | R16-03 / item 176 | ДА | §2.3 | — |
| `9df9f6b8` + `b707d196` | R16-01 / item 174 | ДА (expect'ы удалены) | §2.4 | текст контракта «the one deliberate process kill» неверен (R17F-01); сканер покрывает 7 файлов и 3 вида токенов (R17F-02) |
| `b707d196` + `87f486a2` | perf 84 | ДА (GO обоснован по Ir; без wall-clock/RSS) | §2.5, §2.8 | GO опирается на рукописный receipt `accepted: true`; release-проверка корня в двух циклах снята (§7); doc-фраза в `mod.rs` → R17F-01 |
| `eda25f97`/`b5247602` | R16-02 / item 175 | ДА (red→green подтверждено CI-логами) | §2.6 | skip и выполнение неразличимы в CI (R17F-05); локальные ложные падения E0514 / cfg-неактивный исходник (R17F-03, гипотеза) |
| `edf882cf` | perf 83 (NO-GO, без runtime-изменения) | ДА (вердикт следует из критерия §5.1) | §2.9 | неизменность судьи после заморозки подтверждена только текстом; интерпретация −14 Ir (R17F-04) |
| `c8b9344a` | R15-03 / item 154 (частично) | ДА (перечисленные примеры) | §2.7 | долг 154 остаётся открытым (так и записано) |

### 2.1 `486f5ace` — R15-01

Diff `src/concurrent/epoch/epoch_region.rs:691–711`: цикл по `self.slots` под `catch_unwind(AssertUnwindSafe(|| slot.drop_value()))`; первая паника сохраняется (если `!thread::panicking()` на входе), остальные `mem::forget`; после цикла `resume_unwind(first)`.

- `AtomicSlot::drop_value` (`hand.rs:526–541`): `swap(null)` ДО `drop(into_owned())` — слот опустошён до деструктора, повторный проход невозможен → двойного drop'а нет; `AssertUnwindSafe`-комментарий соответствует коду.
- Контрфактуал (по коду): старый цикл `for slot in &mut self.slots { slot.drop_value(); }` при панике slot 0 раскручивается мимо slot 1 → `later_live_drops == 0` → `first_panic_still_drops_later_live_slot` и `both_panic_...` красные. Тест не вакуумен (по коду; запуск не выполнялся).
- Порядок обхода в тесте: `insert` делает `state.free.pop()` (`epoch_region.rs:438`), free-list `[0,1]` → первая вставка = slot 1, вторая = slot 0; Drop идёт 0→1. Тест проверяет индексы через `Debug` handle'а (`epoch_handle.rs:94–101`, поле `index`) — совпадает.
- Соседние Drop того же класса: `LockFreeRegion`, `ShardedRegion` — собственных `impl Drop` нет (grep `impl.*Drop for` в `src/concurrent`: только `EpochRegion`, `TokenBlock`, `ErasedGuard`); `ShardedRegion` содержит `EpochRegion`-шарды и наследует исправление. Других путей не найдено.
- Нюанс (не дефект): `thread::panicking()` берётся один раз на входе; при outer unwind ВСЕ payload'ы forget'ятся — задокументировано в doc-комментарии.

### 2.2 `32cacc97` — R15-02

`impl Drop for Segment` (`os.rs:167–172`) инкрементирует `SEGMENTS_RELEASED_TOTAL`; `aligned_vmem::Reservation::drop` освобождает mapping.

Проверка «ровно один учёт на резервирование»:
- Все конструкторы `Segment(...)` в `os.rs` (`reserve` :234–235, `reserve_exact` :292–293, `reserve_capacity_exact` :360–361, `reserve_lazy` :408–409, `reserve_small_lazy` :461–462, `reserve_lazy_for_measurement` :495–496) инкрементируют `SEGMENTS_RESERVED_TOTAL` непосредственно перед конструированием — `Segment` без учтённого резервирования не существует.
- `reserve_biased` (`os.rs:178–218`) НЕ создаёт `Segment` (raw `Reservation`, `mem::forget(raw)` + инкремент на :215–216); внутренние `None`-выходы роняют raw `Reservation` без обоих счётчиков — согласовано.
- Потребители `Segment`: `alloc_core_large.rs:641–648` (`mem::forget` после извлечения, затем `release_segment` в `AllocCore::drop`/OOM-rollback :667), `reserve.rs:239–243`, `decomp_hooks.rs:70–77`, `:137–141`, `lifecycle.rs:282` (primordial). Нигде `Segment` не дропается после передачи владения.
- Побочный эффект (улучшение): любой ранний `?`-выход из `bootstrap::primordial()` / `new_inner` после резервирования теперь тоже учитывается.
- Контрфактуал (по коду): без Drop → `reserved_delta=1, released_delta=0` → `assert_eq!(reserved_delta, released_delta)` красный. Не вакуумен.

### 2.3 `222e913d` — R16-03

`size_classes.rs:213–217`: `const _: () = assert!(SMALL_CLASS_COUNT <= u64::BITS as usize, ...)`. Все `1u64 << record.class` (`sidecar_drain.rs:139,220`, `find_segment.rs:156`) защищены. Остальные `1u64 <<` в `src/` — либо `% 64`, либо под собственными const-assert (`LARGE_CACHE_SLOTS <= u64::BITS`, `alloc_core_impl.rs:98`). Закрыто.

### 2.4 `9df9f6b8` — R16-01

Два `.expect(...)` заменены `unwrap_or_else(|| abort())`, затем `b707d196` удалил и abort (маска). Релизных `expect` на этих путях больше нет. Остатки — R17F-01 (текст контракта) и R17F-02 (объём сканера).

### 2.5 `b707d196` — perf 84 runtime (артефакты — §2.8)

Инвариант «каждый указатель в magazine ⇒ `segment_base_of_ptr(p)` = канонический корень»: записи в `tcache.classes[c].slots[..]` только в `dealloc_own_base.rs:420,508,527`, `dealloc_batch.rs:345` (grep); обе точки push проходят `canonical_block_of` (root == base) и `small_free_guard` (`dealloc_own_base.rs:195–301`), который во ВСЕХ feature-комбинациях (Ph3b pre-route / F7(A) / F7(B)) отводит `BlockKind::Large` до push; refill-сторона (`hot.rs`) кладёт только выдачи `AllocCore` из Small/Primordial сегментов (SEGMENT-aligned по контракту `Segment::reserve`/`numa::reserve_aligned_on_node`). Biased Large (`reserve_biased`, корень page-aligned) в magazine попасть не может (`class_for(size, align>=SEGMENT)` → None). `debug_assert_eq!(canonical_root_for(flushed), Some(fbase))` — та же проверка, что в R12-02 `clear_magazine_on_issue` (`hot.rs:43–58`). Провenance: `map_addr`. Изменение корректно.

### 2.6 `6f568c25`/`eda25f97`/`b5247602` — R16-02

См. §3 R17F-03, R17F-05 и §7 (пробелы). Логика финальной версии (`tests/r14_sidecar_owner_capability_negative.rs`):
- `candidate_rlibs` (:210–255): все `libsefer_alloc-*.rlib` в deps; stale = `mtime(candidate) < max(mtime src/**/*.rs, Cargo.toml, build.rs)` (строго меньше); пустой набор после фильтра → hard fail (не skip). Правильно: нет тихого skip.
- `compatible_variants` (:334–420): skip только при `compatible.is_empty() && !candidates.is_empty() && foreign == candidates.len()`; «foreign» = exit 1 + непустые coded-ошибки и ВСЕ = E0461 с точным префиксом. Иначе hard fail. Тихий ложный skip невозможен без того, чтобы rustc выдал E0461 на все кандидаты.
- Ранний `return` при `variants.is_empty()` в трёх фикстурных тестах — достижим только через all-foreign.

**CI-сверка (oh, `gh run list/view`, только чтение):**
- Красный прогон `37770561319` на `9c846e81`: 54 job'а = 49 success + 1 failure + 4 scheduled skipped; единственный провал — `test (aarch64-unknown-linux-gnu)`, шаг `Run cross-thread tests (cross)` (`cross test --target aarch64-unknown-linux-gnu --features "alloc-global alloc-xthread internals"`): лог `R14 sync: target directory=/target/aarch64-unknown-linux-gnu is_target_dir_root=true` → `permit host=x86_64-unknown-linux-gnu target=x86_64-unknown-linux-gnu` → positive probe → E0461 `couldn't find crate \`sefer_alloc\` with expected target triple x86_64-unknown-linux-gnu` → `panicked at …:440:13: positive probe failed incompatibly` → `test result: FAILED. 1 passed; 3 failed`. Запись в манифесте R16 §6 / `RESOLVED.md` («3 of 4 tests failed with E0461 … 49 other jobs and Kani green») — СОВПАЛА.
- Зелёные: `37776932309` (`eda25f97`), `37783258647` (`b5247602`), `37785720332` (`453439c1`) — все `success`, 54 job'а, 4 scheduled skipped; Kani-прогоны `success`. В `453439c1` r14-бинарь запускается: aarch64 cross — 4 раза (1×0 тестов под `experimental`, 3×5 passed), x86_64 — 3 раза (1×0, 2×5), Windows (production) — 3×5, macOS (production) — 1×5.
- **НАБЛЮДЕНО: в зелёных логах нет ни одной строки `R14 …`** — `eprintln!` захватывается libtest для прошедших тестов, `--nocapture` для этого бинаря в `ci.yml` нет. Значит по CI-логам НЕ различимы «проверки выполнены» и «skip через all-foreign» (оба дают `5 passed`). Утверждения карточки item 175 / манифеста R16 §6 «native arm64 … the R14-01 checks execute again» и «cross aarch64 … through the all-foreign skip path» — ВЫВЕДЕНЫ из дизайна (и из красного прогона для aarch64), а не наблюдены в зелёном логе. По коду вывод правдоподобен (на нативных рядах rustc по умолчанию = цель сборки → probe компилируется), но механической гарантии «хотя бы один ряд CI реально исполняет негативные проверки» нет. → R17F-05 (P4).

### 2.7 `c8b9344a` — item 154

17 файлов, из них 16 — только комментарии/doc; один атрибут `#[allow(dead_code)]` снят с `pub mod segment_route` (`registry/mod.rs`). Прочитано построчно: формулировки R15-03 (binding-doc `sharded_region.rs:644–651`, `registry/mod.rs`, `trim_current_thread`) заменены на соответствующие коду (insert — по binding; get_with/remove — по `handle.shard`; мьютексы названы). Out-of-scope правок нет. Префикс `fix` вместо `docs` — объяснён в манифесте (верификатор префиксов отвергает `docs` при некомментарной строке в `src/`).

### 2.8 `87f486a2` + `b707d196` — perf 84 артефакты (oh)

Выборочная перепроверка fx (oh): (1) порядок обхода в тесте R15-01 — подтверждён: `with_capacity` строит `free = [0, 1]` (`epoch_region.rs:219–221`), `insert` берёт `state.free.pop()` (`:438`) → первая вставка = slot 1; `Drop` обходит `&mut self.slots` по возрастанию (`:690–707`); ожидаемые `positions[0]=0`, `positions[1]=1` и payload «первой в порядке обхода» паники согласованы. (2) Инвариант perf 84 — подтверждён: `segment_base_of_ptr` = `ptr.map_addr(|a| a & !(SEGMENT - 1))` (`alloc_core/platform/os.rs:148–150`); Small-сегменты резервируются ровно на `SEGMENT` через `Segment::reserve(SEGMENT)` / `reserve_small_lazy` / `numa::reserve_aligned_on_node(SEGMENT, ..)` (`alloc_core_small/reserve.rs:143,153,208,215,226`); issue-сторона `clear_magazine_on_issue` (`heap_core/alloc/hot.rs:43–58`) использует ту же маску с тем же `debug_assert_eq!`. Ошибок fx не найдено.

Проверено (НАБЛЮДЕНО):
- `node scripts/r16_perf84_gate_table.mjs --check` → `r16_perf84 gate table/CSV check: OK; GO`, exit 0. Исходник прочитан до запуска: `--check` только читает `docs/perf/*` (без `target/`, без сборки, без записи). Скрипт сам сверяет: raw-блок ↔ сводную таблицу в каждом логе, `total = l1+l2+ram`, `est = l1 + 5·l2 + 35·ram`, A1≡A2 и B1≡B2 по всем шести колонкам, sha256 каждого из пяти raw-логов против `identity.json`, таблицу отчёта и CSV байт-в-байт против сгенерированных.
- Арифметика (из `--emit`, пересчитана вручную): n17 65416→65270 = −146 Ir (65270/65416 = 0,997768); n32 67205→66913 = −292 Ir = 2·146 (dose residual 0, допуск max(10; 0,05·292) = 14,6); flush-all 54209→53949 = −260 Ir, prefix 0 → paired 260 > 2T = 24; все контроли Ir-дельта 0 (≤ T); T = max(10; max|C0−A1|) = max(10; 12) = 12 (C0−A1 = 12 Ir на `dealloc_flush_class_only_16b_prefix`, `small_churn_16b`, `small_churn_16b_2n`, `dealloc_flush_all_tcache_16b_prefix`). Все проценты/отношения в отчёте даны как `(num/den)`.
- Immutable identity воспроизводится из истории (oh, независимо): `git rev-parse 265a571a^{tree}` = `75fcf735…` (= `tree_sha`); `git diff b707d196^ b707d196 -- <два файла>` | sha256 = `6b8a7579…4381f` = `candidate_patch_sha256` (и тот же хэш от `265a571a`: `c8b9344a` эти два файла не трогал); `git diff 265a571a 87f486a2 -- benches/perf_gate_iai.rs` | sha256 = `c2950fee…` = `bench_patch_sha256`; `git show 87f486a2:benches/perf_gate_iai.rs` | sha256 = `1bff7077…` = `bench_sha256`. Т.е. измеренный кандидат байт-в-байт равен посадке в двух owned-файлах.
- Path-activation oracle: `tests/r16_perf84_activation.rs` независимо (не из констант SUT) задаёт границы 16→9 и считает 0/1/2 overflow-перехода при 16/17/32 освобождениях + flush-all до 0; в debug-сборке каждый flushed-блок проходит новый `debug_assert_eq!(canonical_root_for, Some(fbase))`, поэтому тест — одновременно оракул инварианта. Мутант-логи (`_raw_r16_perf84_mutant_*.log`) показывают, что mask+16 ловится этим assert'ом, no-op flush-all — финальным count. Dose-response n32 = 2·n17 — косвенное подтверждение активации в самих iai-фикстурах.
- Entry point назван (`HeapCore::dealloc` → `dealloc_own_thread_with_base`; `HeapCore::dbg_flush_all` → production `flush_all_tcache`) и прямо ограничен: «HeapCore evidence, not a real-global-allocator throughput claim». Слой корректен: изменённый код живёт в `HeapCore`, глобальный аллокатор вызывает ту же цепочку. Регим: единая фикстура для cost/benefit (Ir only). Нет wall-clock/RSS-заявлений.
- Префикс `perf(runtime)` у `b707d196` обоснован (production hot-path алгоритм в `production`-scope изменён); `87f486a2` — `bench` (только харнесс/отчёт). Формулировок «ускорение по умолчанию» нет; отчёт: «Churn is Ir-neutral … no general speedup claim».

Наблюдения (не дефекты): (а) вердикт `GO` в судье зависит от рукописного `correctness_receipt.accepted: true` в `identity.json` (скрипт проверяет лишь наличие полей) — это заявление оркестратора, а не механический факт; так и подписано (`"source": "Parent orchestrator supplied …"`). (б) Посадка `b707d196` дополнительно меняет doc-комментарий `src/global/sefer_alloc/mod.rs` (не codegen) — отчёт говорит «B differs only in the two owned src files»; для измеренного B это верно, для коммита — нет, но различие не влияет на измерение. Это именно та правка, что породила R17F-01. (в) Debug-проверки корректности выполнялись на rustc 1.97.0, измерения — на 1.98.1 (раскрыто в §2 отчёта).

### 2.9 `edf882cf` — perf 83 NO-GO (oh)

Проверено (НАБЛЮДЕНО):
- `git show edf882cf --stat -- src` — пусто: runtime-код не менялся; корректностный тест кандидата (`tests/r16_perf83_large_shrink_inplace.rs`) в коммит не попал (как и заявлено в §8 отчёта). Префикс `bench` верен.
- `node scripts/r16_perf83_gate_table.mjs --check` → `r16_perf83 checked: NO-GO`, exit 0 (исходник прочитан: `--check`/`--emit` только читают `docs/perf/*` и `.gz` в памяти; режим без аргументов ПИШЕТ CSV — не запускался). Судья проверяет инвентарь identity-бандла, подписи `.sha256`, совпадение бинарей compile↔pre-run↔post-run, raw-хэши, 108 RSS-строк со свежими PID, арифметику `total`/`est`.
- sha256 `candidate.patch` внутри `R16_PERF83_LARGE_SHRINK_INPLACE_GATE_identity.json.gz` (распаковка в памяти `node`/`zlib`, без записи на диск): записано `d2af1c71…4bef3`, вычислено `d2af1c71315bc72e6d78a006e7bce69d33e76d803fe8128acca5cde86ab4bef3` — совпало; `B.identity.src_patch_sha256` то же; `baseline.patch` пуст; tree A = tree B = `146b0060…` (B отличается от A только `src/alloc_core/alloc_core/mem/realloc_fastpath.rs` — assert судьи). Патч прочитан: условие `grows || (new_eff >= old_eff.div_ceil(2) && class_for(new_eff, align).is_none())` + `grows &&` перед `try_grow_large_reserved_capacity` — соответствует §2 отчёта.
- Числа пересчитаны независимо из raw-логов (grep + `node` по `_raw_r16_perf83_rss.log`, не через судью): цели −694401 Ir (14729652/15424053 = 0,954979) и −521409 Ir (13156788/13678197 = 0,961880); `realloc_grow` A1 = A2 = 582747, C0 = 582759, B1 = B2 = 582733 → Δ = −14, C0−A1 = +12; T = max(10; 12) = 12; |−14| > 12 → единственный провал `Ir gate realloc_grow`. RSS медианы 9 процессов: 8→6 10555392/16855040 = 0,6262; 8→4.5 10547200/15282176 = 0,6902; цикл VmHWM 10563584/16863232 = 0,6264; 108 строк.
- Вердикт NO-GO механически следует из §5.1 отчёта («Every other registered row requires abs(delta)<=T, including … all existing Large/realloc controls»; `realloc_grow` явно перечислен в замороженном списке контролей) и из кода судьи (`WORK.slice(0,2)` — строгая экономия > T; все прочие строки — `|Δ| ≤ T`). Кроме того `correctness_receipt: null` — даже без провала вердикт был бы `PERF-PASS; correctness acceptance required`, не GO. Решение против интереса автора кандидата — косвенный признак, что критерий не подгонялся задним числом.
- R26-4/R30-8/entry-point/regime: per-arm активация (RELOC-дельты, `moved`) в бандле и проверена судьёй; resolved config + `config_conflicts_delta=0` + subprocess-per-sample раскрыты; entry point `HeapCore::realloc` назван и обоснован; RSS и Ir — в одних и тех же пяти сценариях.

Наблюдения (ВЫВЕДЕНО, не меняют вердикт):
1. **Предрегистрация не доказуема из артефактов полностью.** Всё (отчёт §5, судья, данные) пришло одним коммитом; более ранних коммитов с текстом §5 нет (`git log --all -S 'abs(delta)<=T'` → только `edf882cf`). Замороженные identity A/C0/B фиксируют хэш судьи `scripts/r16_perf83_gate_table.mjs` = `d4e37f65…`, драйвера `scripts/r16_perf83_iai.mjs` = `61a843fe…`, RSS-примера = `8cb3dc13…`; закоммиченные версии всех трёх ОТЛИЧАЮТСЯ (сверено с LF и CRLF вариантами; `tests/r16_perf83_activation.rs` и `r16_perf84_gate_table.mjs` — совпадают). Изменение примера раскрыто (§7 п.5), драйвера и судьи — частично (§7 п.2: «The judge checks the same»), но байты замороженного судьи в бандл не положены, поэтому неизменность решающего правила подтверждается только текстом §5, а не хэшем.
2. **«Deterministic rather than noise» — сильнее, чем показывают собственные данные.** A1=A2 и B1=B2 доказывают воспроизводимость одного и того же бинаря, но C0 (тот же исходник, другой путь) на этой же строке `realloc_grow` даёт +12 Ir, т.е. межбинарный (layout) разброс того же порядка, что и −14 у B. Вывод отчёта «comes from the changed code shape» правдоподобен (патч действительно меняет общий Large-branch `try_realloc_inplace_known_base`), но не отличён от layout-эффекта. Вердикт это не меняет (правило двустороннее и предрегистрировано), но рекомендацию «one-sided control rule» стоит формулировать вместе с более чем одним C0-образцом. Оформлено как R17F-04 (P4).

## 3. Новые находки в правках

Шкала: P0 эксплуатируемая memory-safety/UB в production из безопасного кода; P1 UB/memory-safety при нарушении контракта; P2 реальный дефект корректности/утечка/деградация в штатной работе; P3 ограниченный дефект корректности/ресурса/диагностики; P4 вводящий в заблуждение контракт/пробел покрытия/поддерживаемость без установленного сбоя.

| ID | Серьёзность | Класс доказательства | Суть | Перекрёстно (R17, только подсказка) |
|---|---|---|---|---|
| R17F-01 | P4 | SOURCE-CONFIRMED (две трассы по шагам) | no-panic контракт называет недостижимый abort «единственным», достижимые tripwire-abort'ы не названы | R17-LIF-02, R17-CQ-07, R17-VER-07, R17-API-06 |
| R17F-02 | P4 | SOURCE-CONFIRMED | сканер релизных паник: 7 файлов, 3 вида токенов; заявлен охват «GlobalAlloc-reachable files» | R17-VER-07, R17-API-06 |
| R17F-03 | P4 | ГИПОТЕЗА (условия опровержения указаны) | r14-харнесс: локальные ложные hard-fail (E0514; правка cfg-неактивного исходника) | — |
| R17F-04 | P4 | SOURCE-CONFIRMED + пересчёт из raw | perf 83: интерпретация провального контроля сильнее данных; неизменность судьи после заморозки не доказуема | — |
| R17F-05 | P4 | SOURCE-CONFIRMED + НАБЛЮДЕНО (CI-логи) | зелёный CI не различает выполнение негативных проверок r14 и all-foreign skip | — |

Итого: P0 0, P1 0, P2 0, P3 0, P4 5. Проверка завышения: ни одна находка не меняет shipped-поведение и не даёт сбоя у контракт-соблюдающего вызывающего; все abort-трассы R17F-01 срабатывают только после порчи метаданных.

**Не доказано (не включено в находки):** debug-only переполнение в `global/fallback.rs` (R17-API-06, не перепроверялось); поведение cargo/rustc в сценариях R17F-03 (не запускалось); покрытие многосегментного magazine существующими debug-тестами (§7); реальный исход позитивного probe на нативных рядах CI (не наблюдаем — R17F-05).

### P0 — нет
### P1 — нет
### P2 — нет
### P3 — нет
### P4

#### R17F-01 (черновик fx R-001) — P4 — no-panic контракт: «the one deliberate process kill on the alloc path» неверен в обе стороны `[SOURCE-CONFIRMED]`

- Место: `src/global/sefer_alloc/mod.rs:188–191` (на базе `ff3d60bb`, текст восстановлен коммитом `b707d196`): «The one deliberate process kill on the alloc path is a direct `std::process::abort()` (registry chunk-materialisation OOM, `registry/bootstrap/registry.rs`)».
- Факт 1 (oh): названный abort на пути `GlobalAlloc` НЕ достижим. `ensure_chunk` с `std::process::abort()` (`registry/bootstrap/registry.rs:250–271`) вызывается только из `slot()` (`:135–141`); claim использует фаллибельный `reg.slot_or_none(idx)` (`registry/heap_registry/claim.rs:213`, → `try_ensure_chunk`, `None` вместо abort); все вызовы `.slot(` в `src/` — внутри dbg-хуков (`registry.rs:327,334,353` — `dbg_slot_state`/`dbg_slot_generation`/`dbg_slot_preset_generation`; `heap_registry/counters.rs:375` — внутри `dbg_claim_then_simulate_oom`, `:371`).
- Факт 2: другие `std::process::abort()` на пути `GlobalAlloc` ДОСТИЖИМЫ по структуре вызовов (срабатывают только при нарушении внутреннего инварианта — это tripwire'ы, а не ресурсные abort'ы). Две трассы, прочитанные по шагам:
  - (a) холодный bind первой аллокации потока: `GlobalAlloc::alloc` (`global/sefer_alloc/global_alloc.rs:35`) → `SeferAlloc::current_heap` (`global/sefer_alloc/core.rs:341`) → `tls_heap::current_for_alloc` (`global/tls_heap.rs:256`; null-кэш → `bind_slow_tagged`, `:272`) → `HeapRegistry::claim_lease()` (`tls_heap.rs:414` → `heap_registry/claim.rs:113`) → `claim_impl` (`:198`) → `std::process::abort()` на `:259` (проигранный CAS `LIVE→INITIALIZING` у слота, которым поток уже владеет) и `:305` (проигранный CAS `INITIALIZING→LIVE`).
  - (b) горячий small-alloc с промахом magazine (`fastbin` ∈ `production`): `HeapCore::alloc` (`registry/heap_core/alloc/hot.rs:175`) → `alloc_with_class` (`:237`) → `refill_magazine_slow` (`:433` → `:757`) → `drain_large_sidecar_ingress_hot_bounded` (`:761` → `registry/heap_core_xthread/sidecar_drain.rs:18`) → `AllocCore::drain_large_sidecar_ingress_hot_bounded` (`alloc_core/alloc_core/sidecar_drain.rs:302`) → `std::process::abort()` на `:337`, если активный Large-слот таблицы имеет null-базу или не-Large заголовок.
  - Всего `process::abort()` в `src/`: 110 строк grep (включая комментарии/doc).
- История: `9df9f6b8` расширил фразу («registry chunk OOM и missing magazine root»), `b707d196` вернул «The one … (registry chunk OOM)». Оба варианта неполны; второй ещё и называет недостижимый сайт.
- Выведено: тот же класс, что R16-01 (контракт описывает поверхность не так, как код), только для abort. Runtime-поведение не затронуто; вводит в заблуждение читателя контракта — P4. Серьёзность не завышена: ни одна найденная abort-трасса не срабатывает у контракт-соблюдающего вызывающего без предшествующей порчи метаданных.
- Рекомендация: заменить фразу описанием политики (owner-only invariant violations → `std::process::abort()`, ресурсная OOM → null/fallback) и добавить в `no_panic_doc_accuracy` пин на её отсутствие; опционально — лексическую инвентаризацию `process::abort()` по образцу `RELEASE_ALLOWLIST`.
- Перекрёстные ссылки (подсказки, перепроверено по коду выше): R17-LIF-02 (там же факт 1), R17-CQ-07, R17-API-06, R17-VER-07.

#### R17F-02 (черновик fx R-002) — P4 — сканер релизных паник: 7 файлов и 3 вида токенов, а коммит заявляет «the GlobalAlloc-reachable files» `[SOURCE-CONFIRMED]`

- Место: `tests/no_panic_doc_accuracy.rs:214–222` (`RELEASE_SCAN_FILES`: `dealloc_own_base.rs`, `dealloc.rs`, `tcache_flush.rs`, `ownership.rs`, `hot.rs`, `alloc_core_large_cache.rs`, `realloc_fastpath.rs`); сканер ловит только `.expect(`, `panic!`, `unreachable!` (`:474–478` — `text == "expect"` / `matches!(text, "panic" | "unreachable") && macro_call`), не `assert!`/`assert_eq!`/`unwrap()`/`todo!`/`unimplemented!`/индексацию.
- Заявление: тело `9df9f6b8` — «no_panic_doc_accuracy gains a lexical scan of the GlobalAlloc-reachable files whose release expect/panic!/unreachable! sites must equal an explicit allowlist». Факт: пути `GlobalAlloc` проходят и через файлы вне списка (трассы R17F-01: `global/sefer_alloc/global_alloc.rs`, `global/tls_heap.rs`, `registry/heap_registry/claim.rs`, `alloc_core/alloc_core/sidecar_drain.rs`; также `registry/heap_core/free/realloc.rs`, `alloc_core/large/alloc_core_large.rs`, `alloc_core/segment/**`). Новая релизная `expect`/`panic!` в любом из них сканер не заметит.
- Что реально есть вне списка сейчас (oh, `grep -rnE '\.expect\(|panic!\(|unreachable!\('` по `src/global src/registry src/alloc_core`, без комментариев): `global/fallback.rs:192,597` и `global/maintenance_service.rs:141` — тестовые инъекции за `internals`/`bench-internals`-флагами и dbg-переключателем; `registry/bootstrap/loom_shim.rs` — loom-шим; `alloc_core/platform/numa.rs:94` — `expect` на внутренне доказанном `node != NO_NODE` (opt-in `numa-aware`); `global/exact_object/exact_shard.rs:35` — `Layout::from_size_align(cap*32, 8).expect` (opt-in `exact-object-proto`, переполнение только при астрономическом `cap`). Релизные runtime-`assert!` вне сканера: `segment_header_gen_table.rs:61,104` (индекс gen-таблицы; `bump_gen` вызывается из `hot.rs:79` при `hardened`), `terminal_words.rs:49` (`pack_large_state`, generation ≤ 2^61−1 — практически недостижимо), `alloc_core_small_magazine.rs:143` (opt-in `virgin-zero-skip`; вызов `hot.rs:586` передаёт `cur.slots[0..want]`, длина ≤ ёмкости magazine). Все они попадают в категории контракта «bounds-checked indexing / expects on internally-derived indices» — т.е. **сам контракт по факту сейчас не нарушен**; дефект — в заявленном охвате регрессионного гейта.
- Исправлено продолжившим ревьюером (oh): из кандидатов черновика fx исключены `alloc_core_small_impl.rs:86,94,102` — это `const _: () = { assert!(…) }` (compile-time, `:85–106`), а не релизные паники; `global/fallback.rs:192` — `internals`-гейтированная тестовая инъекция за `DBG_INJECT_FALLBACK_INIT_PANIC`, не production-путь.
- Рекомендация: либо переформулировать заявление (в тесте/CHANGELOG — «seven owner hot-path files, three token kinds»), либо генерировать список файлов из графа модулей `global/`+`registry/`+`alloc_core/` с allowlist'ом по образцу `RELEASE_ALLOWLIST` и добавить `assert!`-токены (кроме `const`-контекста).
- Перекрёстные ссылки: R17-VER-07, R17-API-06 (подсказки; R17-API-06 дополнительно указывает на debug-only переполнение в `fallback.rs` — здесь не перепроверялось, в «не доказано»).

#### R17F-03 (черновик fx R-003) — P4 — r14-харнесс: локальные ложные hard-fail (E0514 после смены toolchain; «current build not found» после правки cfg-неактивного исходника) `[ГИПОТЕЗА]`

- Место: `tests/r14_sidecar_owner_capability_negative.rs:365–396` (allowlist E0432/E0433/E0460/E0463/E0603 + E0461), `:233–253` (фильтр по mtime).
- Сценарий 1: в долгоживущем `target/` после обновления rustc старые `libsefer_alloc-*.rlib` (другой hash) новее последней правки `src/` → не отфильтрованы → positive probe даёт E0514 («compiled by an incompatible version of rustc») → `panic!("positive probe failed incompatibly")`. Ложное падение, не вакуумный зелёный. Условие опровержения: rustc при `--extern` на rlib чужой версии выдаёт код, входящий в allowlist (не E0514).
- Сценарий 2: `touch Cargo.toml` (или git-операция, переписывающая его без изменения содержимого) без последующей пересборки lib → все rlib «устаревшие» → hard fail «current build not found». Условие опровержения: cargo пересобирает lib при изменении mtime `Cargo.toml` (тогда свежий rlib появится до запуска теста). (oh) В репозитории нет `build.rs` (проверено), поэтому «build script без rerun-if-changed пересобирает при любом изменении пакета» сюда не относится.
- Сценарий 2' (oh, сильнее и правдоподобнее): `newest_source_modified` сканирует ВСЕ `src/**/*.rs`, а cargo решает о пересборке по dep-info rustc, куда попадают только реально загруженные файлы. Out-of-line модули под ложным `cfg` не загружаются: `#[cfg(kani)] mod kani_proofs;` (`src/lib.rs:523–524`), `#[cfg(feature = "experimental")] mod concurrent;` (`src/lib.rs:408–409`). Правка, например, `src/concurrent/epoch/epoch_region.rs` (ровно то, что делал R15-01) и затем `cargo test --features "alloc-global internals" --test r14_sidecar_owner_capability_negative` (без `experimental`): lib не пересобирается → все кандидаты старше правленого файла → hard fail «current build not found … rebuild the crate», и подсказка не помогает — повторный `cargo test` снова ничего не пересоберёт, пока не тронут скомпилированный файл или не сделан `cargo clean`. Класс: ГИПОТЕЗА (SOURCE-CONFIRMED по сканеру и по `cfg`-объявлениям; поведение cargo/rustc не проверялось запуском). Условие опровержения: после такой правки cargo пересобирает `libsefer_alloc-*.rlib` для набора без `experimental`, либо dep-info rustc включает cfg-неактивные `mod`-файлы.
- Серьёзность P4: только локальная хрупкость тестовой инфраструктуры, падение громкое (не вакуумный зелёный); CI (свежий checkout, mtime исходников ≤ времени сборки) не затронут — 3 зелёных прогона это подтверждают.
- Перекрёстная ссылка: тема mtime-эвристики как «design limit» уже записана в карточке item 175 (`RESOLVED.md`, «Residual») — там названы восстановленный кэш со старыми mtime и скопированный кэш с новыми; сценарии 1 и 2' в ней НЕ названы (`E0514` не встречается ни в `RESOLVED.md`, ни в харнессе).
- Рекомендация: (1) решить явно — добавить E0514 (`found crate \`sefer_alloc\` compiled by an incompatible version of rustc`) в список «skip» с точным префиксом или задокументировать hard-fail как намеренный; (2) считать «свежесть» по dep-info-файлу кандидата (`deps/sefer_alloc-*.d` перечисляет реально скомпилированные исходники) вместо `src/**/*.rs`, либо хотя бы по mtime файлов, перечисленных в нём.

#### R17F-04 — P4 — perf 83: «deterministic rather than noise» сильнее собственных данных; неизменность судьи после заморозки не доказуема `[SOURCE-CONFIRMED + пересчёт из raw]`

- Место: `docs/perf/R16_PERF83_LARGE_SHRINK_INPLACE_GATE.md` §8 («The deviation is in B's favour: A1=A2 and B1=B2 exactly, so it is deterministic rather than noise. INFERRED: it comes from the changed code shape …»); identity-бандл `R16_PERF83_LARGE_SHRINK_INPLACE_GATE_identity.json.gz` (`A.identity.json` → `inputs`).
- НАБЛЮДЕНО: `realloc_grow` C0 − A1 = 582759 − 582747 = **+12 Ir** при тождественном исходнике (другой путь снапшота) — это и задаёт T = 12; B1 − A1 = **−14 Ir**. A1 = A2 и B1 = B2 доказывают лишь воспроизводимость каждого бинаря, а межбинарный разброс того же порядка отчёт сам измерил на этой же строке.
- НАБЛЮДЕНО: замороженные входы фиксируют `scripts/r16_perf83_gate_table.mjs` = `d4e37f65…`, `scripts/r16_perf83_iai.mjs` = `61a843fe…`, `examples/r16_perf83_large_shrink_rss.rs` = `8cb3dc13…`; закоммиченные версии всех трёх имеют другие хэши (LF и CRLF). Изменение примера раскрыто (§7 п.5), драйвера/судьи — лишь фразой §7 п.2; байты замороженного судьи в бандле не сохранены. Предрегистрация решающего правила подтверждается только текстом §5.1 в том же коммите, что и данные (`git log --all -S 'abs(delta)<=T'` → только `edf882cf`).
- ВЫВЕДЕНО: вердикт NO-GO от этого не меняется (правило двустороннее, записано, применено против интереса кандидата). Но рекомендованный в отчёте путь пересмотра («one-sided control rule») опирается на интерпретацию −14 как эффекта формы кода; при той же величине layout-разброса корректнее требовать несколько C0-образцов (или C0 и для B-исходника) до выбора правила.
- Рекомендация: в будущих гейтах класть в identity-бандл сами байты судьи/драйвера на момент заморозки (не только хэш) и фиксировать текст предрегистрации отдельным коммитом до измерений.
- Серьёзность P4 (вводящая в заблуждение интерпретация / пробел провенанса; на shipped-поведение не влияет).

#### R17F-05 — зелёный CI не различает «негативные проверки r14 выполнены» и «all-foreign skip» `[SOURCE-CONFIRMED + НАБЛЮДЕНО в CI-логах]`

- Место: `tests/r14_sidecar_owner_capability_negative.rs` (`compatible_variants`, ветка `foreign == candidates.len()` → `eprintln!` + `return compatible` пустым; три фикстурных теста делают ранний `return`); `.github/workflows/ci.yml` — r14-бинарь запускается без `--nocapture`.
- НАБЛЮДЕНО: в логах зелёных прогонов `37776932309`, `37783258647`, `37785720332` нет ни одной строки `R14 …` ни на одном ряду (захвачено libtest); на всех рядах только `running 5 tests … ok`. Skip и реальная проверка дают идентичный вывод.
- ВЫВЕДЕНО: если на нативном ряду кандидаты по какой-то причине все станут E0461 (например, ряд переведут на `--target` с иным triple, чем `rustc` по умолчанию в PATH, или `RUSTC` укажет на другой хост-компилятор), проверки R14-01 молча перестанут исполняться везде, а CI останется зелёным. Это ровно класс «вакуумный зелёный», который R16-02 и чинил (до R16-02 проверки «0 under the R15 gate» на arm64 тоже не были видны в зелёном CI).
- Документация закрытия item 175 формулирует выведенное как наблюдённое («Evidence: … native arm64 … the R14-01 checks execute again»).
- Рекомендация: на нативных рядах CI выставлять, например, `R14_REQUIRE_CHECKS=1`, при котором all-foreign skip — hard fail; либо запускать r14-бинарь с `--nocapture` и проверять sentinel-строку `R14 <tag>: N compatible candidates` (в репозитории уже есть такой паттерн sentinel-safe `--nocapture`, `ci.yml:1756`). Oracle: зелёный CI с этой проверкой + намеренно сломанный ряд (все кандидаты foreign) должен покраснеть.
- Серьёзность P4 (пробел покрытия/наблюдаемости, установленного сбоя нет).

## 4. Проверка заявлений документации (механика) (oh)

Пересчёт по файлам/истории, не по цитатам. Индексы, README, ARCHITECTURE, CHANGELOG и `tests/`/`examples/`/`benches/` между `453439c1` и базой `ff3d60bb` не менялись (`git diff --stat 453439c1 ff3d60bb -- …` пуст), поэтому факт на базе = факт на `453439c1`.

| Заявление | Где | Заявлено | Факт (команда — прил. A) | Совпало |
|---|---|---|---|---|
| Диапазон §5 воспроизводим | манифест R16 §5 | 13 коммитов `fe171d9e..9831adcc`, SHA и `%cI` в таблице | `git log --reverse --format="%H\|%cI\|%s" fe171d9e74d7…..9831adcc…` — те же 13 SHA, те же `%cI`, тот же порядок | да |
| Диапазон §6 воспроизводим | манифест R16 §6 | 2 коммита `9c846e81..b5247602` | `eda25f97…` `2026-10-08T14:26:01+02:00`, `b5247602…` `2026-10-08T15:15:55+02:00` | да |
| 22 файла `src/` изменены | манифест §5.1 | 22 = 17 (`c8b9344a`, вкл. `os.rs`) + 5 | `git diff --name-only fe171d9e 9831adcc -- src \| wc -l` = 22; 17 файлов `c8b9344a` не включают `size_classes.rs`/`sefer_alloc/mod.rs`/`epoch_region.rs`/`dealloc_own_base.rs`/`tcache_flush.rs` | да |
| `Cargo.*`, `.github/` без изменений | манифест §5.1, §6 | нет diff | `git diff --stat` по `Cargo.toml Cargo.lock .github` в обоих диапазонах — пусто; в §6 и `src/` пусто | да |
| Raw-логи диапазона | манифест §5.1 | 14 файлов, 159525 B | 14 файлов `docs/perf/_raw_r16_perf8*`; `git ls-tree -l 9831adcc` сумма = 159525 | да |
| identity.json.gz | манифест §5.1 | 224311 B | `git ls-tree -l` = 224311 | да |
| Красный CI | манифест §6, `RESOLVED.md` item 175 | run `37770561319`, aarch64 cross, 3 из 4 упали, E0461, 49 прочих job'ов зелёные | `gh run view`: 54 job'а = 49 success + 1 failure + 4 skipped; `FAILED. 1 passed; 3 failed`, E0461 | да |
| Зелёный CI | манифест §6 | `eda25f97` `37776932309`, `b5247602` `37783258647`: 50 success + 4 skipped; Kani success; 4 и 5 тестов r14 | совпадает; на `eda25f97` macOS и aarch64 — `running 4 tests`, на `b5247602`/`453439c1` — `running 5 tests` | да |
| «the R14-01 checks execute again» на native arm64 | `RESOLVED.md` item 175 Evidence, CHANGELOG | наблюдение | в зелёных логах нет строк `R14 …` (захват libtest) — вывод из дизайна, не наблюдение | **нет (выведено подано как наблюдённое)** → R17F-05 |
| «CI is green on both (… each run 5 tests)» | CHANGELOG follow-up | 5 тестов на обоих | на `eda25f97` — 4 теста (тест свежести добавлен в `b5247602`) | **частично** (неточность формулировки, без последствий) |
| «scans the `GlobalAlloc`-reachable files» | CHANGELOG follow-up, тело `9df9f6b8` | все достижимые файлы | 7 файлов, 3 вида токенов | **нет** → R17F-02 |
| «Runtime improvements this round: 1» | CHANGELOG | 1 | ровно один `perf(runtime)` (`b707d196`); `perf(opt-in)` нет | да |
| perf 84 числа | CHANGELOG, манифест §5.1, `OPEN_ITEMS.md` | −146 / −292 / −260 Ir, T = 12 | `--emit` и raw-логи (§2.8) | да |
| perf 83 числа | CHANGELOG, манифест, `OPEN_ITEMS.md` item 83 | −694401 / −521409 Ir, RSS 0,63× / 0,69×, `realloc_grow` −14 vs T = 12 | пересчитано из raw (§2.9): 0,6262 / 0,6902 | да |
| ACTIVE census | `CORRECTNESS_OPEN_ITEMS.md`, манифест §5.3 | 7: 1, 2, 11, 13, 62, 162, 163 | `grep -cE '^[0-9]+[a-z]*\. \*\*' ACTIVE.md` = 7; номера {1, 2, 62, 11, 13, 162, 163} | да |
| TRACKED census | там же | 139 записей = 139 строк lookup на `TRACKED_*.md` | 139 / 139; дополнительно (oh) сверены МНОЖЕСТВА: каждая карточка ↔ строка с тем же файлом, расхождений 0, дублей 0; ещё 12 строк lookup ведут в `RESOLVED.md` (22, 148, 149, 157, 168–170, 172–176) | да |
| Items 172–176 закрыты | `RESOLVED.md` | CLOSED (2026-10-08) | заголовки `### 172…176 — … CLOSED (2026-10-08)` (`RESOLVED.md:18–48`); в `ACTIVE`/`TRACKED_*` карточек с этими номерами нет | да |
| Item 154 OPEN | `TRACKED_misc.md:56` | OPEN — deferred + R16 follow-up note | `Status: OPEN — deferred`; заметка `c8b9344a` присутствует | да |
| Perf item 84 в архиве, GO | `OPEN_ITEMS.md` «Recently resolved», `OPEN_ITEMS_ARCHIVE.md` | GO, `b707d196` | `OPEN_ITEMS.md:3065` (указатель), `OPEN_ITEMS_ARCHIVE.md:1472` (CLOSED, GO); активной карточки `84.` нет | да |
| Perf item 83 `[L]` NO-GO | `OPEN_ITEMS.md:3051` | `[L]`, NO-GO, `edf882cf` | карточка со Status / Current / Next trigger / Evidence | да |
| 372 тест-файла | README «Verification evidence», `docs/ARCHITECTURE.md` | 372 | метод проекта (`tests/no_stale_doc_references.rs:443–466,475–516`: корень `tests/*.rs`, НЕ рекурсивно): `git ls-tree --name-only 453439c1 tests/ \| grep -c '\.rs$'` = 372 (рекурсивно было бы 387) | да |
| 83 примера | README, ARCHITECTURE | 83 | `examples/*.rs` = 83 | да |
| 23 бенча, 13 root Loom | README | 23 / 13 | 23 / 13 | да |

## 5. Проверено и в порядке

- `Segment` без `Drop` ранее не деструктурировался (компилируется с `Drop`) — косвенно: диапазон собирался в CI (oh: все три зелёных прогона `success`).
- `EpochHandle::Debug` содержит `index: N,` — тест R15-01 читает его корректно.
- `small_free_guard` отводит `Large` во всех комбинациях `hardened`/`medium-classes`/`exact-span-large`/`large-reserved-capacity`/`numa-aware` (три взаимоисключающих ветки покрывают всё пространство).
- (oh) Порядок обхода и ожидания теста R15-01 (§2.8); вложенная паника внутри `catch_unwind` в `Drop` при outer unwind не обязана abort'ить (abort только при панике в panic hook — так и сказано в doc-комментарии `Drop`).
- (oh) Small-сегменты резервируются ровно на `SEGMENT` (`reserve.rs:143,153,208,215,226`) → маска perf 84 даёт канонический корень.
- (oh) perf 84: `--check` OK; хэши `tree`/`candidate_patch`/`bench_patch`/`bench` воспроизведены из истории git; арифметика T/dose/paired пересчитана; activation-тест и мутант-логи согласованы с отчётом.
- (oh) perf 83: `src/` не тронут; sha256 `candidate.patch` = `d2af1c71…4bef3`; все заявленные числа пересчитаны из raw; NO-GO следует из записанного правила; `correctness_receipt: null` честно ведёт к не-GO в любом случае.
- (oh) CI: красный прогон и причина записаны в манифесте/`RESOLVED.md` точно; три зелёных прогона подтверждены.
- (oh) Манифест R16 §5/§6: оба диапазона `git log` воспроизводятся байт-в-байт (SHA, `%cI`, порядок); числовые заявления §5.1 совпали (22 файла `src/`, 14 raw-логов / 159525 B, gz 224311 B, `Cargo.*`/`.github` без diff).
- (oh) Индексы: ACTIVE 7, TRACKED 139 ↔ 139 строк lookup (совпадают и множества), 172–176 CLOSED в `RESOLVED.md`, 154 OPEN; perf 84 в архиве GO, perf 83 `[L]` NO-GO; README/ARCHITECTURE 372 / 83 / 23 / 13 по методу проекта.
- (oh) Форматирование/конвенции/префиксы — §6.

## 6. Расхождения документации с кодом; форматирование и конвенции

Расхождения документации с кодом:
- R17F-01 — no-panic контракт `src/global/sefer_alloc/mod.rs:188–191` (названный abort недостижим, достижимые не названы).
- R17F-02 — заявленный охват сканера (CHANGELOG follow-up, тело `9df9f6b8`) шире фактического.
- R17F-05 — `RESOLVED.md` item 175 / CHANGELOG подают выведенное «checks execute» как наблюдение.
- §2.8 (б) — отчёт perf 84 «B differs only in the two owned src files» верно для измеренного B, но коммит `b707d196` дополнительно правит doc-комментарий `mod.rs` (без codegen; не находка).

Форматирование и конвенции (oh, по диапазону `fe171d9e..453439c1`):
- `rustfmt --edition 2021 --check` (rustfmt 1.9.0-stable) на всех 31 добавленных/изменённых `.rs` диапазона (`git diff --name-only --diff-filter=AM fe171d9e..453439c1 -- '*.rs'`): exit 0, 0 `Diff in`.
- Новых файлов в `src/` нет (`--diff-filter=A -- src` пусто) → правило «один файл — один экспорт» не затронуто.
- `mod.rs` в диапазоне (`registry/mod.rs`, `registry/segment_route/mod.rs`, `segment_directory/mod.rs`, `global/sefer_alloc/mod.rs`): единственная не-комментарная строка — удаление `#[allow(dead_code)]` (`registry/mod.rs`). Логики в `mod.rs` не добавлено.
- Не-комментарные изменения `src/` по файлам: `epoch_region.rs` 15, `tcache_flush.rs` 10, `dealloc_own_base.rs` 10, `os.rs` 5, `size_classes.rs` 4, `diag_probes.rs` 2 (обе — trailing-комментарий на строке `#[allow(unsafe_code)]` + doc; сам атрибут не изменён, tier-2 grep его по-прежнему находит), `registry/mod.rs` 1. Всё в рамках заявленных коммитов; out-of-scope правок не найдено.
- Doctest: добавленных ```` ``` ````-ограждений в doc-комментариях `src/` нет.
- Приватные пути машины: в `+`-строках диапазона только синтетические фикстуры самотеста санитайзера (строки вида `/home/example/…` и фиктивный путь с буквой диска `…/private/example.exe` в самотесте `scripts/r16_perf83_iai.mjs`) — не реальные пути. Реальных путей нет.
- `TODO`/`FIXME`/`XXX`/`todo!`/`unimplemented!` в добавленных строках — нет.
- `node scripts/verify-commit-prefixes.mjs fe171d9e..453439c1` → PASS (17 коммитов; скрипт прочитан — только `git log/show`, без cargo).

## 7. Остаточные риски и пробелы тестов

Пробелы тестов (fx):
- R15-01: сценарий «деструктор паникует во время outer unwind» (`already_unwinding == true`) не покрыт тестом; поведение (forget всех payload'ов) задокументировано, но не пинится.
- R15-02: тест покрывает только отказ `attach_owner` через `fail_next_registration_for_test`; roll-back'и Small/Large регистрации (`register`/`register_payload` → None) используют `release_segment` и были согласованы и до правки — не регрессия, но и не покрыты новым тестом.

Остаточные риски и пробелы (oh; ВЫВЕДЕНО чтением, ничего не запускалось):
- **perf 84 — снятая release-проверка.** До `b707d196` (после `9df9f6b8`) magazine-указатель без канонического корня в release приводил к `abort()`; теперь в release `segment_base_of_ptr` безусловно маскирует адрес и `clear_magazine(foff)` пишет в метаданные сегмента по маскированной базе; проверка осталась только `debug_assert_eq!`. Срабатывает лишь при уже нарушенном инварианте (push в magazine прошёл `canonical_block_of` + `small_free_guard`), поэтому это осознанный обмен защиты в глубину на −146/−260 Ir, симметричный issue-стороне R12-02, а не находка. Но если когда-нибудь появится путь, кладущий в magazine не-Small указатель, в release он станет тихой порчей метаданных вместо abort.
- **perf 84 — многосегментный magazine в debug.** Новый activation-тест берёт 64 блока по 16 B (по построению из одного сегмента), поэтому новый `debug_assert_eq!` в обоих циклах проверяется на одном сегменте; отчёт сам указывает, что «dedicated new-loop multi-segment debug scenarios were not run». Покрывают ли этот случай другие существующие тесты в debug — не проверялось.
- **perf 83 — oracle активации остаётся в дереве** (`tests/r16_perf83_activation.rs`): без env он проверяет базовый A-arm `8→3` (move); после отката кандидата это корректный контроль текущего поведения (move при shrink), а не мёртвый тест.
- **R16-02.** Видимость skip в CI (R17F-05); локальная хрупкость (R17F-03); идентичность «связанного» rlib не доказана by design (`RESOLVED.md` Residual).
- **R16-01 / no-panic контракт.** Регрессионный гейт не видит `process::abort()`, `assert!`, `unwrap()` и файлы вне семи (R17F-01/R17F-02). Отдельно не перепроверено: debug-only арифметическое переполнение в `global/fallback.rs`, на которое указывает R17-API-06 (подсказка; в «не доказано»).
- **Соседний класс R15-01, не проверялся:** значения, отданные `remove`/`remote_evict` в `crossbeam-epoch`, уничтожаются отложенно на границе эпохи — паника их деструктора раскрутится в том потоке, который выполняет сбор, вне `EpochRegion::drop`; исправление R15-01 этот путь не затрагивает и не заявляет.
- **Соседний класс R15-01, проверен:** других собственных циклов drop'а значений `T` в `src/concurrent` нет (`impl Drop` только `EpochRegion`, `TokenBlock`, `ErasedGuard`; последние два `T` не держат); стандартный drop срезов/`Vec` продолжает ронять остальные элементы при панике одного.

## 8. Меню проверок (итог)

- [x] perf 84: `--check`, raw ↔ CSV ↔ таблицы, арифметика, activation, entry point, identity из git — §2.8.
- [x] perf 83: `--check`, sha256 `candidate.patch` в `.gz`, `git show edf882cf --stat -- src` пуст, числа из raw, критерий — §2.9.
- [x] CI: `37770561319`, `37776932309`, `37783258647`, `37785720332` — §2.6.
- [x] Доки: 372 / 83 / 23 / 13; census ACTIVE 7 / TRACKED 139 / lookup 139 (множества); диапазоны манифеста §5/§6; SHA и даты — §4.
- [x] `rustfmt --edition 2021 --check` 31 файла; конвенции; префиксы — §6.
- [x] Достижимость abort-сайтов (R17F-01) и релизных паник (R17F-02) — §3.
- [ ] НЕ сделано (ограничение владельца): запуск тестов, мутантов, miri/loom/kani, cargo-сборок; проверка сценариев R17F-03 запуском; debug-переполнение `fallback.rs` (R17-API-06).

## 9. Рекомендованный порядок исправления

1. **R17F-01** — переписать одну фразу no-panic контракта в `src/global/sefer_alloc/mod.rs:188–191` (политика: owner-only invariant violations → `process::abort()`; ресурсная OOM → null/fallback; registry chunk OOM на claim-пути → fallback через `slot_or_none`) + пин в `no_panic_doc_accuracy` на отсутствие старой фразы. Дёшево, чистая документация.
2. **R17F-02** — либо честно сузить заявление (тест/CHANGELOG: «seven owner hot-path files, three token kinds»), либо расширить `RELEASE_SCAN_FILES` до графа `global/`+`registry/`+`alloc_core/` с allowlist'ом и добавить runtime-`assert!` и `process::abort()` в инвентаризацию.
3. **R17F-05** — сделать выполнение негативных проверок r14 наблюдаемым на нативных рядах CI (`R14_REQUIRE_CHECKS=1` или `--show-output` + sentinel-grep) и поправить формулировку Evidence item 175.
4. **R17F-03** — решить судьбу E0514 и привязать фильтр свежести к dep-info кандидата.
5. **R17F-04** — на будущее: сохранять байты замороженного судьи в identity-бандле, коммитить предрегистрацию до измерений, брать ≥ 2 C0-образца перед выбором правила контроля при пересмотре perf 83.

## Приложения

### A. Команды (исполнено) и фактический результат

fx:

```text
git rev-parse HEAD                              -> ff3d60bb571f706cd8f034665fac8c3f4b6f0d23
git log --reverse --format='%h %s' fe171d9e..453439c1   (17 коммитов, §1.1)
git show <sha> для 486f5ace 32cacc97 222e913d 9df9f6b8 b707d196 c8b9344a (полный diff)
grep -rn 'impl.*Drop for' src/ ; grep -rn 'process::abort()' src/ ; grep -rn '1u64 <<' src/
```

oh (все из корня worktree; `gh` — только чтение; CI-логи сохранялись во временный каталог вне репозитория):

```text
git rev-parse HEAD                                            -> ff3d60bb571f706cd8f034665fac8c3f4b6f0d23
git cherry-pick -n 4bfeb8b5 ; git mv <draft> <final>          -> отчёт fx импортирован
node scripts/r16_perf84_gate_table.mjs --check                -> "r16_perf84 gate table/CSV check: OK; GO", exit 0
node scripts/r16_perf84_gate_table.mjs --emit                 -> таблица §2.8 (read-only)
git rev-parse 265a571a^{tree}                                 -> 75fcf7356bb177cf77a75c9ec293148281eff737 (= tree_sha)
git diff b707d196^ b707d196 -- <dealloc_own_base.rs> <tcache_flush.rs> | sha256sum -> 6b8a7579…4381f (= candidate_patch_sha256)
git diff 265a571a b707d196 -- <те же два файла> | sha256sum  -> 6b8a7579…4381f
git diff 265a571a 87f486a2 -- benches/perf_gate_iai.rs | sha256sum -> c2950fee…af0c7 (= bench_patch_sha256)
git show 87f486a2:benches/perf_gate_iai.rs | sha256sum        -> 1bff7077…fe1a2 (= bench_sha256)
git show edf882cf --stat -- src                               -> пусто
node scripts/r16_perf83_gate_table.mjs --check                -> "r16_perf83 checked: NO-GO", exit 0
node scripts/r16_perf83_gate_table.mjs --emit                 -> "Computed gate: NO-GO. Failures: Ir gate realloc_grow."
node -e (zlib.gunzipSync в памяти; sha256 artifacts['candidate.patch'].text)
                                                              -> d2af1c71315bc72e6d78a006e7bce69d33e76d803fe8128acca5cde86ab4bef3 (совпало)
node -e (sha256 текущих файлов vs A.identity inputs)          -> gate_table/iai/rss-пример DIFFERENT; r16_perf83_activation.rs, r16_perf84_gate_table.mjs LF-MATCH
git log --all -S 'abs(delta)<=T'                              -> только edf882cf
grep realloc_grow Instructions в _raw_r16_perf83_{A1,A2,C0,B1,B2}.log -> 582747/582747/582759/582733/582733
node -e (медианы/максимум по _raw_r16_perf83_rss.log)         -> 0.6262 / 0.6902 / 0.6264, 108 строк
gh run list --limit 40                                        -> 9c846e81 CI failure; eda25f97/b5247602/453439c1 CI+Kani success
gh run view 37770561319 --json jobs                           -> 54 jobs: failure "test (aarch64-unknown-linux-gnu)", 4 skipped
gh run view 37770561319 --job 113288826606 --log              -> E0461, "FAILED. 1 passed; 3 failed"
gh run view 37776932309|37783258647|37785720332 --json jobs   -> success, 54 jobs, 4 skipped
gh run view 37785720332 --job {113339818244,113339818236,113339817488,113339817570,113339817810} --log
                                                              -> r14: running 5 tests … ok; строк "R14 …" нет
gh run view 37776932309 --job {113310044853,113310043945} --log -> r14: running 4 tests (+1× running 0 tests)
git log --reverse --format="%H|%cI|%s" fe171d9e74d7…..9831adcc… -> 13 строк, как в манифесте §5
git log --reverse --format="%H|%cI|%s" 9c846e81…..b5247602…      -> 2 строки, как в манифесте §6
git diff --name-only fe171d9e 9831adcc -- src | wc -l         -> 22
git diff --stat fe171d9e 9831adcc -- Cargo.toml Cargo.lock .github -> пусто
git ls-tree -l 9831adcc docs/perf/ (14 _raw_r16_perf8*)       -> сумма 159525; .gz 224311
grep -cE '^[0-9]+[a-z]*\. \*\*' docs/correctness-open-items/ACTIVE.md          -> 7
grep -hE '^[0-9]+[a-z]?\. \*\*' docs/correctness-open-items/TRACKED_*.md | wc -l -> 139
grep -cE '^\| *[0-9]+[a-z]? *\| `TRACKED_[^`]+\.md` \|' docs/CORRECTNESS_OPEN_ITEMS.md -> 139
node -e (множества карточек TRACKED_* ↔ строк lookup)        -> mismatch 0, 12 строк → RESOLVED.md
git ls-tree --name-only 453439c1 tests/ | grep -c '\.rs$'    -> 372 (рекурсивно 387)
git ls-tree --name-only 453439c1 examples/|benches/ …         -> 83 / 23; root loom_* -> 13
rustfmt --edition 2021 --check <31 файл диапазона>            -> exit 0, 0 "Diff in"
git diff fe171d9e..453439c1 | grep (приватные пути / TODO)    -> только синтетические фикстуры санитайзера; TODO нет
node scripts/verify-commit-prefixes.mjs fe171d9e..453439c1    -> PASS (17 commits)
```

### B. Мутанты

Не запускались: ограничение владельца (никаких `cargo`/компиляции/тестов). Контрфактуалы оценены по коду (класс SOURCE-CONFIRMED или ГИПОТЕЗА): R15-01 и R15-02 — §2.1/§2.2 (fx); perf 84 — закоммиченные мутант-логи авторов (`_raw_r16_perf84_mutant_*.log`) прочитаны, но не воспроизводились; perf 83 — мутанты (a)–(c) описаны в отчёте §8, не воспроизводились; R17F-03 сценарии 1 и 2' — ГИПОТЕЗА с условием опровержения.

### C. Пересчёты

| Величина | Источник | Пересчёт oh | Заявлено | Совпало |
|---|---|---|---|---|
| perf 84 n17 Δ | raw A1/B1 | 65270 − 65416 = −146; 65270/65416 = 0,997768 | −146 | да |
| perf 84 n32 Δ, dose | raw | 66913 − 67205 = −292 = 2·(−146); residual 0 ≤ max(10; 0,05·292 = 14,6) | −292, exact | да |
| perf 84 flush-all Δ, paired | raw | 53949 − 54209 = −260; prefix 0 → paired 260 > 2T = 24 | −260 | да |
| perf 84 T | C0−A1 по 15 строкам | max\|·\| = 12 → T = max(10; 12) = 12 | 12 | да |
| perf 83 8→6 | raw | 14729652 − 15424053 = −694401; 0,954979 | −694401 | да |
| perf 83 8→4.5 | raw | 13156788 − 13678197 = −521409; 0,961880 | −521409 | да |
| perf 83 `realloc_grow` | raw | B1−A1 = −14; C0−A1 = +12; T = 12; \|−14\| > 12 | −14 vs T = 12 | да |
| perf 83 RSS 8→6 | rss.log, медиана 9 | 10555392/16855040 = 0,6262 | 0,63× | да |
| perf 83 RSS 8→4.5 | rss.log, медиана 9 | 10547200/15282176 = 0,6902 | 0,69× | да |
| perf 83 VmHWM цикл | rss.log, max 9 | 10563584/16863232 = 0,6264 | (в отчёте как дробь) | да |
| Красный CI | job log | 1 passed + 3 failed из 4; 49 + 1 + 4 = 54 | 3 из 4; 49 зелёных | да |
| Зелёный CI | `--json jobs` | 54 − 4 skipped = 50 success | 50 + 4 | да |
| §4 итог | таблица §4 | 23 строки: 20 да / 2 нет / 1 частично | — | — |

## 10. R18 follow-up — dispositions

This section records changes made after the read-only review above. Historical
measurements and findings remain intact; it does not reclassify the original
five findings.

- **R17F-01 — CLOSED.** `src/global/sefer_alloc/mod.rs` now distinguishes
  fallible allocation OOM from explicit invariant aborts and the infallible
  registry accessor. `no_panic_doc_accuracy::no_panic_doc_is_qualified`
  rejects the stale “one deliberate process kill” claim and pins the
  replacement categories.
- **R17F-02 — CLOSED.** The scanner remains intentionally lexical and scoped
  to its explicit source-file list and three token classes (`expect`,
  `panic!`, `unreachable!`); it is not a total `GlobalAlloc` call-graph or
  abort census. `CHANGELOG.md` now says so, and the test file documents the
  same boundary instead of claiming complete reachability.
- **R17F-03 — FIXED AT THE IDENTIFIED FAILURE BOUNDARIES.** Candidate
  freshness now uses the Rust sources listed by that rlib's matching Cargo
  dep-info `.d`, not every `src/**/*.rs`; cfg-inactive edits and manifest-only
  mtime changes do not stale a candidate. Missing/unreadable dep-info retains
  the candidate for the positive probe. Only the exact E0514 for
  `sefer_alloc` is skipped per candidate; unrelated E0514 errors remain hard
  failures. The helper tests passed (9 total in the focused harness suite).
  Counterfactuals confirmed the tests are load-bearing: adding the newer
  cfg-inactive timestamp to freshness made the test fail with the expected
  active/inactive timestamp mismatch. The exact compiler-version mismatch was
  not reproduced with a second rustc toolchain; its classifier is pinned by
  exact-message tests. Current-linked rlib identity remains the documented
  design limit.
- **R17F-04 — REPORT CORRECTED; NO-GO UNCHANGED.** The perf-83 report and item
  83 now state that C0−A1=+12 Ir and B1−A1=−14 Ir are comparable in magnitude;
  A1=A2/B1=B2 prove only within-binary repeatability, not a deterministic
  cross-binary code-shape effect. No new measurement was run, and the
  preregistered two-sided NO-GO stands.
- **R17F-05 — CLOSED IN THE HARNESS.** An all-foreign candidate set skips only
  when the test target's `rustc --print cfg` differs from the rustc default.
  Same-target and unrecognized-target cases fail closed. The workflow was not
  changed. The skip-predicate test passed; its counterfactual (allowing
  `Some(true)`) failed at the intended assertion. Historical CI logs are
  corrected to say that their captured test count did not prove which path
  ran; a green native CI after this guard is the end-to-end receipt.
- **R18 P5 cleanup regression — FIXED.** Restored
  `try_promote_to_large`'s `return None` when `old_layout.size()` exceeds the
  committed read span. The Windows-lazy `production medium-classes` regression
  passed with the fix; replacing the return with the prior empty `if` made the
  child terminate with `0xc0000005` and the parent test fail.

- **R30-12 pre-push gate — classifier false positive, not a behavior change.**
  The final check flagged `1cb85d3` because a `#[allow(dead_code)]` line's
  trailing explanation changed while the attribute itself remained identical;
  the remaining `src/` delta is comments. Kept the honest `docs:` subject and
  added a per-SHA false-positive exemption with this section as its durable
  record, rather than rewriting the existing commit history. The final gate
  rerun verifies the exemption.

**Focused receipts:** `cargo test --features "alloc-global internals"
--test r14_sidecar_owner_capability_negative` — 9 passed;
`cargo test --features "alloc-global internals" --test no_panic_doc_accuracy`
— 4 passed; `cargo test --features "production medium-classes internals"
--test r14_4_promotion_move_leg_reduction` — 6 passed, 1 intentional ignored.
The four edited Rust files pass direct `rustfmt --edition 2021 --check`.
Strict clippy and the final repository gate are recorded in the R18 closeout
after completion.
