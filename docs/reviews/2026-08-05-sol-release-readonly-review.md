# Sol release readonly review — 2026-08-05

## Область и ограничения

- Диапазон: `40241b0810b42c672f3f7c507f21b2de762b782b..4623dc3a87742d4dc416398d64cf673cb1986e33`.
- Проверено 45 non-merge коммитов, включая все изменения production-source в R34-3/R34-6/R34-11/R34-14/R34-15/R34-17/R34-18, связанные тесты, CI/release workflow, gate-отчёты и committed raw summaries.
- Это строго статический аудит. Build/test/bench/scripts/binaries не запускались. Поэтому ранее записанные результаты тестов и измерений рассматриваются как артефакты репозитория, а не как независимо воспроизведённые мной результаты.
- До записи отчёта `main` был ahead `origin/main` на 108 коммитов; в worktree уже были чужие untracked `.claude/` и Markdown-отчёты. Они не изменялись.

## Краткий verdict

**К релизу текущий tree не готов (`NO-GO`), но не из-за найденного нового подтверждённого UB.** В production-диффах диапазона я не нашёл доказанного нового UB, UAF, double-free, data race или OOB. Напротив, R34 исправляет утечку large-cache reuse, закрывает ordering proof gap, смягчает free-path OOM и добавляет unwind guards.

Release-blockers сейчас процессные и API-контрактные:

1. R34-3 не создал заявленную полную границу `internals`: module paths закрыты, но `AllocCore::dbg_*` остаются доступны без `internals` через crate-root re-export.
2. `Cargo.toml` всё ещё имеет версию `0.3.0`, тогда как новые изменения лежат в `[Unreleased]`, а отдельная датированная секция `[0.3.0]` уже существует ниже. Нужно однозначно выбрать и оформить release version.
3. Cargo-комментарий обещает «never panic/abort», хотя shipped source документирует пять release tripwires, а registry alloc-path явно вызывает `abort()` на chunk OOM.

**Ускорения именно в этом диапазоне нет.** R34-11 меняет memory-retention policy и сохраняет ранее полученный выигрыш R32-8; это не новый speedup. Остальные production-изменения — correctness/API/build hardening, иногда с дополнительной работой на редких путях.

## Findings

### F1 — P1, release blocker: `internals` не скрывает inherent `AllocCore::dbg_*`

#### Факты

- Commit `27879af` делает `alloc_core/global/registry` `pub(crate)` без `internals`: `src/lib.rs:306-379`.
- При этом `AllocCore` намеренно остаётся публичным crate-root re-export без `internals`: `src/lib.rs:395-408`.
- Диагностические impl-модули компилируются без `internals`: `src/alloc_core/mod.rs:14-23` (`alloc_core_core_diag`, `alloc_core_small_diag`, `alloc_core_small_reclaim` и др.).
- Их методы — inherent `pub`-методы публичного `AllocCore`, например:
  - safe state-mutating `AllocCore::dbg_carve_batch`: `src/alloc_core/alloc_core_small_diag.rs:20-29`;
  - unsafe freelist mutation `AllocCore::dbg_drain_freelist_batch`: `src/alloc_core/alloc_core_small_diag.rs:117-139`;
  - unsafe release/recycle seam `AllocCore::dbg_recycle`: `src/alloc_core/alloc_core_core_diag.rs:527-557`.
- Закрытие родительского module path не закрывает inherent methods реэкспортированного типа: downstream может именовать их как `sefer_alloc::AllocCore::dbg_*`.
- Cargo-комментарий утверждает обратное — «every `dbg_*` hook» перемещён за `internals`: `Cargo.toml:401-433`.
- Новый boundary test проверяет только наличие стабильных root-типов и методов `SeferAlloc`, но не отсутствие `AllocCore::dbg_*`: `tests/r34_3_internals_boundary_api.rs:24-48`, `:66-111`. Сам тест прямо говорит, что negative compile property не автоматизирован.

#### Вывод

R34-3 реально сузил module-path surface, но не выполнил центральное обещание о diagnostic method surface. Это не само по себе UB: опасные методы помечены `unsafe`. Но это реальный semver/API-boundary дефект и несоответствие release-документации, поэтому исправить его нужно до публикации.

#### Рекомендация

- Gate диагностические impl-блоки/методы `#[cfg(feature = "internals")]` (а measurement-only subset дополнительно оставить за `bench-internals`), либо перестать root-reexport’ить `AllocCore`, если он не должен быть стабильным API.
- Добавить настоящий compile-fail oracle именно для `sefer_alloc::AllocCore::dbg_*` без `internals`, а не только для `sefer_alloc::alloc_core::*`.

### F2 — P1, release blocker: release version/changelog не сведены

#### Факты

- Package version: `Cargo.toml:1-7` — `0.3.0`.
- Все новые волны находятся под `## [Unreleased]`: `CHANGELOG.md:1-12`.
- Ниже уже существует датированная секция `## [0.3.0] - 2026-07-04`: `CHANGELOG.md:4966-4974`.
- Release workflow проверяет совпадение tag и Cargo version (`.github/workflows/release.yml:154-173`) и запускает package/test gates (`:175-218`), но не проверяет, что `[Unreleased]` перенесён в секцию выпуска.
- R34-3 меняет externally reachable module paths и тем самым может ломать downstream-код, который использовал doc-hidden paths; commit сам классифицирован как `feat(api)`.

#### Вывод

Нельзя честно выпустить текущий tree, не разрешив, что именно означает версия `0.3.0`: либо это всё ещё готовящийся 0.3.0 и верхний Unreleased надо объединить/датировать, либо прежняя секция 0.3.0 уже была выпуском и требуется новая версия (с учётом API-сужения разумно отдельно решить patch/minor policy). Workflow технически не заменяет это release-решение.

#### Рекомендация

Зафиксировать целевую версию, обновить Cargo/workspace dependency versions при необходимости, перенести release notes из `[Unreleased]` в датированную секцию и добавить CI guard: Cargo version должна иметь ровно одну соответствующую release-секцию, а `[Unreleased]` перед tag publish должен быть пустым либо явно разрешённым.

### F3 — P1, release blocker: публично заявленный no-panic/no-abort контракт противоречит коду

#### Факты

- `Cargo.toml:159-166` говорит: «Every entry point is no-panic» и checked branches «never panic/abort».
- `src/global/sefer_alloc.rs:62-107` перечисляет пять release-surviving panic tripwires, достижимых из `GlobalAlloc` path; `:109-115` объясняет, что escaping panic превращается в abort через nounwind shim.
- Registry allocation path сохраняет прямой process abort на chunk-materialisation OOM: `src/registry/bootstrap.rs:631-651`.
- Free-path теперь корректно использует fallible `slot_or_none`: `src/registry/bootstrap.rs:567-601`, `src/registry/heap_core_xthread.rs:340-359`; это улучшение R34-15, но оно не делает alloc path never-abort.

#### Вывод

Документация `sefer_alloc.rs` стала существенно честнее, но Cargo feature contract остался старым и буквально ложным. Это особенно важно для global allocator: потребитель должен знать, где null/no-op policy заканчивается и начинается invariant/OOM abort policy.

#### Рекомендация

Свести формулировку к точному контракту: обычные failure paths возвращают null/no-op; пять invariant tripwires и registry-chunk alloc OOM terminate process. Не обещать «never panic/abort» без изменения самих путей.

### F4 — P2, evidence/docs: R34-11 переутверждён как «no observable behavior» и «unbounded spin» fix

#### Факты

- Код R34-11 вычисляет до восьми due intervals и вызывает `run_decay_step()` в цикле: `src/alloc_core/alloc_core_large_cache.rs:514-545`.
- Gate показывает наблюдаемое изменение retention: в events=1 final gap 3→1 segment, releases 1→3, но peak остаётся 4 segments и ≥3 persistence остаётся 29/40 = 72.5%: `docs/perf/R34_11_CATCHUP_DECAY_GATE.md:127-151`, `:225-252`; CSV `docs/perf/R34_11_CATCHUP_DECAY_GATE_summary.csv:3-6`.
- Throughput comparison 80.76 vs 249.14 ns/cycle сравнивает сохранённый stride=64 с unthrottled shape. Сам отчёт говорит, что catch-up body в throughput regime не достигается и это не новый speedup: `docs/perf/R34_11_CATCHUP_DECAY_GATE.md:200-218`.
- `CHANGELOG.md:12` одновременно говорит, что observable behavior production default не изменилось.
- Manifest называет R34-11 предотвращением «unbounded spin»: `docs/perf/round-manifests/R34_MANIFEST.md:168-180`. В этом механизме нет spin-loop; новый цикл жёстко ограничен восьмью шагами. Изменяется отложенное освобождение памяти.
- Source comment «gap drops to 0 on the first clock read» (`src/alloc_core/alloc_core_large_cache.rs:429-437`) описывает промежуточное состояние внутри операции; committed gate показывает externally observed post-operation residual в один segment.

#### Вывод

Сам код выглядит bounded и осмысленно улучшает retention, но narrative смешивает три разных утверждения: новый retention fix, старый R32-8 speedup и несуществующий unbounded spin. Это не correctness blocker, однако для verification-first проекта и release notes формулировки нужно исправить.

#### Рекомендация

Писать: «observable retention policy changed; final gap reduced 3→1 in measured sparse arm; peak still 4; existing stride speedup preserved; no new throughput speedup measured». Удалить «unbounded spin» и пояснить, что промежуточный gap=0 снова становится externally observed gap=1 после deposit текущего free.

### F5 — P2 residual hardening: `DrainHeadPublish` не делает reclaim транзакционным

#### Факты

- Guard публикует только последнее полностью advanced `h`: `src/alloc_core/remote_free_ring.rs:839-866`.
- В drain порядок такой: `reclaim(off)` → clear slot → advance/publish cursor: `src/alloc_core/remote_free_ring.rs:1288-1306`.
- Если closure успела изменить allocator/external state и затем panic’нула, slot не очищен, а `h` не продвинут. После `catch_unwind` следующий drain повторно передаст тот же offset.
- Текущие production closures вызывают проверенный `reclaim_offset[_checked]` и затем bookkeeping: `src/alloc_core/alloc_core_small.rs:905-921`, `:1138-1149`, `:2691-2709`; статически я не нашёл в этих конкретных телах нового reachable panic после успешной мутации. На `GlobalAlloc` escape panic всё равно abort’ит процесс.

#### Вывод

Guard исправляет «успешно завершённые предыдущие элементы потеряны при panic», но не обеспечивает exactly-once для паникнувшего элемента. Это residual panic-safety/API-hardening, не доказанный текущий production double-free. Формулировка «panic-safe drain» должна содержать precondition: closure либо не паникует, либо не оставляет mutation перед panic.

#### Рекомендация

Документировать контракт closure. Если catchable unwind должен поддерживаться, нужен двухфазный/идемпотентный reclaim protocol либо явный poison/skip policy; одной cursor RAII недостаточно.

### F6 — P2 residual hardening: `InitStateGuard` может оставить написанный `HeapCore` без Drop

#### Факты

- Guard переводит `INITIALIZING → UNINIT` при любом unwind: `src/global/fallback.rs:187-212`, `:373-403`.
- `HeapCore` записывается в `FALLBACK` до bind и READY: `src/global/fallback.rs:217-254`.
- Если panic случится после `write(hc)` и до READY, guard разрешит следующему победителю снова писать поверх `FALLBACK`, не вызывая Drop старого `HeapCore`.
- `AllocCore::Drop` освобождает принадлежащие heap reservations: `src/alloc_core/alloc_core.rs:2582-2625` и далее; пропуск Drop способен утечь reservation/cache resources.
- Текущий injection panic расположен до `HeapCore::new` (`src/global/fallback.rs:200-212`), поэтому новый тест не покрывает post-write окно. Текущий `bind_thread_free` — простое присваивание (`src/registry/heap_core_ownership.rs:25-37`), а в просмотренном production path нет ожидаемого panic после write.

#### Вывод

Guard устраняет вечный `INITIALIZING`, но не полностью unwind-safe после materialisation. Сегодня это residual/future-hardening (и при escape через global allocation процесс abort’ит), а не подтверждённый обычный leak. Тем не менее комментарий «anything between ... write/bind ... panic-safe» шире реально обеспеченной гарантии.

#### Рекомендация

Guard должен знать, был ли `HeapCore` записан, и на catchable unwind либо `drop_in_place` его до публикации UNINIT, либо переводить state в terminal poisoned/aborted состояние. Добавить post-write injection oracle отдельно от pre-new injection.

### F7 — P3, формальный residual: cached-head имеет явное wrap/preemption assumption

#### Факты

- R34-6 разумно усиливает shadow load/store до Acquire/Release: `src/alloc_core/remote_free_ring.rs:1101-1131`; это закрывает ordering proof gap без дополнительной cross-core `head` load на fast path.
- Module proof сам признаёт, что stale-low theorem зависит от того, что между `head.load` и `cached_head.store` не произойдёт около `2^32` drain advances; иначе shadow может стать modularly stale-high и допустить premature slot reuse: `src/alloc_core/remote_free_ring.rs:203-226`.

#### Вывод

Это не новый дефект R34 и практически астрономический сценарий, но формально алгоритм опирается на scheduler/time assumption, а не только на Rust memory model. Наиболее вероятный эффект — lost remote-free entry/leak, не доказанный UAF. Для релиза он не blocker, если это сознательно принятый documented risk; для заявлений «формально verified» его нельзя замалчивать.

## Реально ли новые волны ускорили код

### Факты

| Commit/волна | Production effect | Speed verdict |
|---|---|---|
| `27879af` R34-3 | visibility/API reorganisation | runtime-neutral |
| `a9edc87` R34-6 | `cached_head` Relaxed → Acquire/Release | correctness/order hardening; на слабых архитектурах потенциально дороже, нового speedup нет |
| `73dceca` R34-11 | до 8 decay/release steps за clock check | retention fix; старый R32-8 speedup сохранён, новый не измерен |
| `7ef5a46` R34-14 | три дополнительных reset writes при large-cache hit | исправляет реальную утечку/deferred-list ошибку; не ускорение |
| `49929d0` R34-15 | fallible free-path registry lookup | убирает abort на редком OOM; hot fast path остаётся Acquire lookup, speedup не заявлен |
| `c270b0c` R34-17 | RAII head/init publication | panic hardening; обычный head store не исчезает |
| `3281ebc` R34-18 | compile-time `HeapCore <= 8 KiB` assert | runtime-neutral pin |

R34-12 лишь повторно подтвердил speedup shadow-head, который был внесён в R32, а R34-23 исправил завышенную старую realloc цифру (~40× → ~1.8–2.1×): `docs/perf/R34_23_REALLOC_AND_VEC_GATE.md:28-34`, `:120-125`.

### Вывод

**Ответ: нет, рассматриваемый диапазон не дал нового подтверждённого ускорения shipped runtime.** Он сделал allocator корректнее и честнее измерил прежние выигрыши. Называть 67.6% из R34-11 новым ускорением нельзя: catch-up loop в измеренном throughput path вообще не исполнялся.

## Где ещё есть шанс на большой выигрыш

### 1. Сначала workload knob, а не новая архитектура: small-pool Throughput

Факт: уже измеренный переход pool 4/16 MiB → 8/32 MiB даёт около 22% latency win, но удерживает примерно +8 MiB/heap и масштабируется до ~255 MiB на 32 heaps: `docs/perf/OPEN_ITEMS.md:105-108`. Это сильнейший подтверждённый доступный рычаг, но не универсальный default.

Вывод: для потребителя с известным RSS budget лучше продвигать профиль/recipe и реальные deployment traces. Adaptive global budget имеет смысл открывать только при неравномерной per-heap нагрузке; без неё это усложнение без victim.

### 2. Warm bulk burst 16/256 B остаётся главным измеренным проигрышем

Факт: README показывает 16 B 70.9 vs 30.0 ns и 256 B 84.5 vs 31.2 ns, то есть 2.37×/2.71× медленнее mimalloc: `README.md:1007-1018`; при обычном churn картина существенно лучше (`README.md:985-1005`). R34-25 оценивает ~54.5% magazine-hit Ir как bitmap/provenance bookkeeping, но заключает, что palette заменяет один AND более дорогими load/tag/branch и steady-state clear correctness-required: `docs/design/R34_25_SMALL_MAGAZINE_PROVENANCE_DESIGN.md:44-53`, `:468-512`.

Вывод: большой выигрыш здесь потребует не ещё одной локальной bitmap micro-optimization, а изменения протокола magazine residency/cross-thread free. До кода разумен дешёвый disassembly check, предложенный самим R34-25 (`:516-540`), затем только victim-activated prototype.

### 3. Geometric realloc: последний copy остаётся крупным, но текущие простые идеи уже NO-GO

Факт: реальная `64 B → 4 MiB` цепочка около 210–238 µs и лишь ~1.8–2.1× быстрее mimalloc; `large-reserved-capacity` гипотеза дала NO-GO: `docs/perf/R34_23_REALLOC_AND_VEC_GATE.md:28-34`, `:120-125`, `:195-212`.

Вывод: следующий большой рычаг — page-run/adjacent-run grow, но только после trace реального consumer с существенным 256 KiB–2 MiB alloc+realloc volume. R34-26 такого consumer не нашёл и оставил дизайн NEED-MORE-DATA/lean NO-GO: `docs/design/R34_26_PAGE_RUN_LAYER_DESIGN_GATE.md:14-35`, `:485-521`. Без victim это не release work.

### 4. First-allocation stack/copy path

Факт: `HeapCore` имеет измеренный размер 7576 B и всё ещё строится by value; R34-18 добавил только верхнюю границу 8192 B: `src/registry/heap_core.rs:540-564`, `:566-605`.

Вывод: `new_in_place` для `HeapCore`+`Tcache` может убрать крупный временный объект/копию и снизить first-allocation stack risk, но это пока непомеренный speed target. Приоритет повышается для small-stack/thread-churn consumers, а не для steady-state throughput.

## Что улучшить в коде и проекте

1. Исправить F1 и автоматизировать negative API tests. Проверять надо и module path, и inherent methods root-reexported типов.
2. Ввести release metadata gate: Cargo version ↔ единственная датированная CHANGELOG section ↔ tag; запрет non-dry manual publish при несведённом `[Unreleased]`.
3. Создать единый машинно-проверяемый panic-policy inventory. Текущий тест считает пять message strings, но не охватывает прямой `abort()` и не синхронизирует Cargo/README wording.
4. Разделять в manifest три категории: runtime latency, observable memory-policy change и correctness-only. R34-11 показывает, что «no speedup» не означает «no observable behavior».
5. Для release headline benchmarks закрепить свежий dedicated runner и versioned baseline. Текущая README смешивает даты/режимы и прямо предупреждает о ±15–20% шуме (`README.md:1020-1025`); R34-23 уже обнаружил физически невозможную ~40× цифру. Это проблема доверия к release claims, даже если allocator code корректен.
6. Не превращать residual hardening F5/F6 в широкий refactor до формулировки нужного unwind contract: через GlobalAlloc panic aborts, а прямой/internal use может catch unwind. Контракт должен различать эти режимы.

## Safety matrix по диапазону

| Класс | Результат статического аудита |
|---|---|
| UB | нового подтверждённого UB не найдено |
| UAF | не найдено |
| double-free | не найдено в обычном production path; F5 оставляет повтор panicking item после catchable unwind как residual |
| data race | не найдено; R34-6 усиливает ordering и закрывает proof gap |
| OOB | не найдено; новые `slot_or_none` callers range-check owner id до lookup |
| leak | R34-14 исправляет реальный dropped deferred large free; F6 оставляет post-write unwind leak window; RemoteFreeRing overflow/rare wrap имеет документированную leak policy/risk |
| abort/panic | присутствуют сознательно; поэтому blanket no-panic/no-abort wording неверен |

## Release decision

Перед release обязательны:

1. закрыть или явно перескопировать F1 с корректной semver-декларацией;
2. выбрать release version и свести CHANGELOG;
3. исправить no-panic/no-abort contract;
4. исправить R34-11 release narrative;
5. после изменений уже вне рамок этого readonly-аудита прогнать полный project release gate и package dry-run на clean tree.

F5/F6/F7 можно принять как документированный residual hardening, если release policy не обещает catch-unwind transactional semantics/formal scheduler-independent proof. Но первые три пункта — не residual: это текущие проверяемые несоответствия release surface и контракта.
