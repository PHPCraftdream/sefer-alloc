# Ревью `src/` — раунд 17 (oxx) — верификационное покрытие и достоверность средств проверки

## 1. Охват, инвентарь, метод, база, вердикт

**База:** `main` @ `453439c123f71a98890408dd0170bd909ad9ecdf` (`docs: close correctness item 175 after green CI and record the red CI of the first push`). CI на этом SHA (только чтение через `gh`): run `37785720332` (CI) — success, run `37785720371` (Kani verification) — success. Семнадцатый раунд серии обзоров `src/`; предыдущий — `docs/reviews/2026-10-08-src-review-oxx-round-16.md` и `docs/perf/round-manifests/SRC_REVIEW_R16_MANIFEST.md`. С базы R16 (`6a0d47f6`) в `src/` вошло 6 коммитов: 22 файла, +130/−122.

**Тема (одна из восьми в раунде):** если в `src/` сломать инвариант, поймает ли это проект, и не вводят ли в заблуждение сами средства проверки: Kani, Miri, Loom, proptest/fuzz, pin-тесты, диагностические счётчики, `debug_assert!`, сканеры `scripts/verify-*.mjs` и meta-тесты, `cfg(miri|loom|kani|test)`-ветки.

**Режим: только чтение, без компиляции и исполнения кода.** По приказу владельца не запускались `cargo` (build/check/test/clippy), Miri, Loom, Kani, бенчмарки и Node-скрипты проекта. Witness-тесты и мутанты не создавались. Каждый контрфактуал («пройдёт ли тест без фикса», «поймает ли гейт мутант») выведен **чтением** кода теста и `src/`. Использовались только чтение файлов (включая `find`/`wc`/`sed`/`awk` для подсчётов), `grep`/`rg`, read-only `git` (`log`, `show`, `diff`, `rev-parse`) и read-only `gh` (`run list`, `run view --log`). Исходники не правились; `git status` перед записью отчёта пуст.

**Инвентарь** (пересчитан командами из приложения A, не скопирован): **168** Rust-файлов, **43 325** физических строк. Из них **17 059** непустых строк, не начинающихся с `//`, **24 303** строки `//`/`///`/`//!` и **1 963** пустые. По группам: `src/alloc_core/` — 88 файлов / 24 802 строки; `src/registry/` — 46 / 10 413; `src/global/` — 18 / 3 809; `src/concurrent/` — 14 / 3 554; `src/lib.rs` + `src/kani_proofs.rs` — 2 / 747. Unsafe-инвентарь по команде из CLAUDE.md: в `src/` **21** модульный allow (tier 1) и **83** item-уровня (tier 2), всего 51 файл (21 + 30). Счётчики темы: **13** `#[kani::proof]`, **13** корневых `tests/loom_*.rs`, **67** `debug_assert*!` в 27 файлах, **110** `std::process::abort()` в 28 файлах, **48** публичных тест-хуков вида `*_for_test(s)`/`*_test` без префикса `dbg_`.

**Метод.**
1. Прочитаны `docs/CORRECTNESS_OPEN_ITEMS.md`, `docs/correctness-open-items/ACTIVE.md` и все девять `TRACKED_*.md`. У `TRACKED_publish_readiness.md`, `TRACKED_platform_contracts.md` и `TRACKED_process_record.md` — только заголовки и Status-блоки: они вне `src/` и вне темы. Прочитаны шапка и `[A]`-тир `docs/perf/OPEN_ITEMS.md`, остальные карточки — по заголовкам. Кроме того прочитаны отчёт и манифест R16, `docs/INVARIANTS.md`, `docs/CROSS_THREAD_STATE_MACHINES.md` (вся), receipt реестра доказательств Ph7 и выдержки Ph6a.
2. **Полностью, построчно:** `src/kani_proofs.rs`; `src/registry/bootstrap/{mod,registry,ensure,loom_shim,saturation}.rs`; `src/registry/heap_registry/stack.rs`; `src/alloc_core/segment/remote_bitmap/{sidecar_bitmap.rs, sidecar_bitmap/leaf_classes.rs, bitmap_scan.rs}`; `src/alloc_core/large/reservation_state.rs`; `src/registry/heap_core_xthread/routing.rs`; `src/alloc_core/segment/segment_table/{harness,issue_transaction}.rs`; `src/registry/segment_route/pin.rs`; `src/global/alloc_stats.rs`.
3. **Выборочно, по диапазонам:** `segment_table/{hash.rs 1–330, segment_table_impl.rs 380–445, 700–800, 955–1000}`; `segment_header/{terminal_words.rs 1–100, segment_header_impl.rs 545–600, 725–760, segment_header_layout.rs 97–200}`; `platform/{os.rs 40–240, 540–585; node.rs 60–145; sidecar.rs 205–240}`; `alloc_core/alloc_core/{counters.rs, mem/mem_impl.rs 80–150, 228–238, bootstrap.rs 160–200, lifecycle.rs 480–545, sidecar_drain.rs}` (места счётчиков); `small/{alloc_core_small_impl.rs 78–112, 255–360; alloc_core_small_magazine.rs 25–50, 130–160; alloc_core_small_pool/decommit.rs 60–300; alloc_core_small_pool_impl.rs 255–300, 655–690; reserve.rs 150–300}`; `registry/heap_core/alloc/hot.rs 600–760`; диф `b707d196` (`free/dealloc_own_base.rs`, `state/tcache_flush.rs`); `registry/heap_registry/{claim.rs 80–100, 195–365, 415–432, 520–545; counters.rs 1–95, 360–420; maintenance.rs 30–60}`; `registry/segment_route/small_sidecar.rs`; `global/{fallback.rs 340–495 и grep, sefer_alloc/mod.rs 85–215, sefer_alloc/diag.rs 160–215, sefer_alloc/core.rs 355–385, tls_heap.rs 480–530}`; `concurrent/epoch/{hand.rs 120–460, epoch_region.rs 330–380, 585–600}`; `registry/mod.rs`; `lib.rs 440–524`.
4. **Скрининг всех 168 файлов** grep-запросами с чтением попаданий: `cfg(miri|loom|kani|test|debug_assertions)` и `cfg!(miri)`; `debug_assert*!`; релизные `expect`/`unwrap`/`panic!`/`unreachable!`/`assert!`; `process::abort()`; все `static … Atomic*` и места их инкремента; все `pub fn dbg_*` и `pub fn *_for_test(s)`; `set_payload_virgin`, `set_bump`, `read_struct_with_atomic_word`.
5. **Сопоставление `src/` со средствами проверки:** 13 корневых loom-моделей (8 полностью, `loom_epoch` и `loom_sharded` частично, 3 по заголовку); pin-тесты `r11_ph4a_drop_ordering_pinned`, `r11_ph4b_teardown_ordering_pinned`, `r11_ph4b_local_no_drop_pinned`, `r11_ph5b_c5_sidecar_ordering_pinned`; `no_panic_doc_accuracy`; Miri-цели (`decommit_miri_cycle`, `regression_r2_06_header_race_miri`, `regression_xthread_small_ring_miri` полностью, остальные по заголовку); `segment_table_backshift_proptest`; `scripts/{verify-dbg-hook-safety, miri, verify-loom-target-wiring}.mjs` полностью; `scripts/verify-evidence-registry.mjs` 340–400, 600–820; Kani-, Loom-, Miri-, TSan-, ASan-, multi-arch-, macOS- и fuzz-разделы `.github/workflows/ci.yml`, а также `kani.yml`.

**Только скринингом (не прочитаны вглубь):** `alloc_core/large/{alloc_core_large,alloc_core_large_cache,alloc_core_large_cache_eviction,large_cache_extended}.rs`, `small/alloc_core_small/{find_segment,directory,dealloc}.rs`, `segment_directory/*`, `config/*`, все `*_diag*.rs` вне названных строк, `global/{maintenance_service,exact_object/*}`, `registry/segment_route/{directory,registration,route_*,shard_lock}.rs`, `concurrent/{lock_free/*,sharded/*,pinning}.rs`. Утверждения «не найдено» относятся только к прочитанному.

**Не запускалось:** вообще ничего, кроме read-only `git`/`gh` (режим только чтение). CI-свидетельства — чтение уже завершённых прогонов на базе (приложение B).

**Вердикт:** **11 находок, все P4.** Десять SOURCE-CONFIRMED, часть дополнительно подтверждена чтением CI-логов; одна ГИПОТЕЗА. Ещё четыре блока **новых доказательств к существующим пунктам** 17, 18, 160 и 167 (§2.2). Новых P0–P3 в прочитанной области не найдено. Общий рисунок такой: средства проверки в основном **честны о своих ограничениях на уровне отдельных файлов**, но после терминального cutover (`a4245965`, 2026-09-30) и правок R12–R16 ряд **сводных** заявлений о покрытии стал неверен. Это pin упорядочений, Kani, инвентарь Miri, реестр доказательств, `INVARIANTS.md` и no-panic контракт: каждое утверждает больше, чем реально исполняется против текущего кода. Принятый P1-box (164) и Miri-остаток 171 не тронуты.

## 2. Находки

Шкала из задания раунда: P3 — ограниченный дефект корректности, ресурса или диагностики, включая счётчик, дающий неверный oracle; P4 — вводящий в заблуждение контракт, пробел покрытия или поддерживаемость без установленного runtime-сбоя. SOURCE-CONFIRMED — путь установлен чтением кода; «CI-лог» — чтение журнала уже завершённого прогона; ГИПОТЕЗА — не подтверждено.

| ID | Sev | Класс доказательства | Достижимость | Суть |
|---|---|---|---|---|
| R17-VER-01 | P4 | SOURCE-CONFIRMED | production `alloc-global`, любой foreign free Small/Primordial | Pin «issue Release» привязан к test-only ветке `ClassMap::Dense`; production-упорядочения `ClassLeaves` (включая несущую публикацию mixed-листа) не пинятся; позитивные кейсы `loom_sidecar_bitmap` нечувствительны к упорядочениям |
| R17-VER-02 | P4 | SOURCE-CONFIRMED + CI-лог | tooling; трактовка 127 PASS-строк | Судья реестра доказательств не видит изменений `src/`: после последней правки реестра — 17 src-коммитов (91 файл, два `perf(runtime)`), CI-гейт зелёный по построению; ограничение есть только в receipt |
| R17-VER-03 | P4 | SOURCE-CONFIRMED + CI-лог | tooling; Miri-покрытие `alloc-global`/`production` | Дрейф Miri-инвентаря: 3 цели `*_miri` не исполняются ни одним раннером (на них стоят 6 PASS-строк), `miri.mjs` «зеркалит CI», но расходится с ним и из-за P1-box всегда красный, README/ARCHITECTURE называют несуществующую цель, часть Miri-тестов описывает удалённые механизмы |
| R17-VER-04 | P4 | SOURCE-CONFIRMED + CI-лог | Kani-job, все сборки | После cutover Kani не доказывает ни одного нетривиального инварианта production: 13 харнессов (а не 19), `pack_proofs` «регрессируют» несуществующий free-list реестра, комментарии ссылаются на теневой loom |
| R17-VER-05 | P4 | SOURCE-CONFIRMED | `experimental` (deprecated) | `loom_epoch` моделирует `AtomicSlot::evict`, удалённый 2026-06-24; реальный `try_evict_at` (CAS generation → swap) покрыт только `loom_sharded` |
| R17-VER-06 | P4 | SOURCE-CONFIRMED | `internals`/`bench-internals` | Сканеры hook-safety отбирают хуки по префиксу `dbg_`: 48 публичных `*_for_test(s)` (1 `unsafe`, несколько мутаторов) вне политики; три «PURE_OBSERVERS» материализуют чанки реестра и имеют путь `abort` |
| R17-VER-07 | P4 | SOURCE-CONFIRMED | публичный rustdoc `SeferAlloc`, все сборки `alloc-global` | No-panic контракт: «the one deliberate process kill on the alloc path» — ложь: 110 `abort()` в 28 файлах, а названный abort из production-пути недостижим; сканер не видит abort и `assert!` |
| R17-VER-08 | P4 | SOURCE-CONFIRMED | спецификация, все сборки | `docs/INVARIANTS.md` («spec every future change must keep green») расходится с кодом в M2, M4, M5, M6; ссылается на 2 несуществующих теста; ни один guard этот файл не читает |
| R17-VER-09 | P4 | SOURCE-CONFIRMED | `alloc-core` и выше; только после порчи метаданных | Debug-only защиты на холодных owner-путях: в release — тихая запись за массив free-list, цикл в списке пула, бесконечный probe в production `hash_insert_identity` |
| R17-VER-10 | P4 | SOURCE-CONFIRMED | `production` (HeapCore/SeferAlloc) | Per-PR рандомизированный дифференциал есть только для `AllocCore`; отгружаемый слой (magazine, overflow-flush, ingress) рандомизирован лишь еженедельным 10-минутным однопоточным fuzz |
| R17-VER-11 | P4 | ГИПОТЕЗА (CI-лог: x86_64-хост + cross-rs) | CI multi-arch | aarch64-строки `cross` (QEMU-user на x86_64) заявлены как weak-memory покрытие; на TSO-хосте аппаратные ARM-переупорядочивания, вероятно, не воспроизводятся. Реальное weak-memory железо — только `test-macos` (arm64) |

### R17-VER-01 — pin упорядочений sidecar защищает test-only ветку, а production `ClassLeaves` не пинится

**Места.** `tests/r11_ph5b_c5_sidecar_ordering_pinned.rs:1–9` (обещание: «These pins bind the model's three atomic edges to the production sites»), `:20–24` (читаются только `sidecar_bitmap.rs` и `bitmap_scan.rs`), `:53–60` (`owner_issue_class_store_is_release`). `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs:46–53` (`ClassMap::encoded`), `:61–69` (`from_initialized` → `ClassMap::Dense`), `:71–77` (`from_leaves`), `:83–97` (`issue`: Dense-ветка `:91–93`, Leaves-ветка `:95`), `:110–119` (`publish`). `src/registry/segment_route/small_sidecar.rs:39–42` (production `bitmap()` → всегда `from_leaves`). `src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs:113` (Release-публикация mixed-листа), `:67`, `:122`, `:139` (Acquire-чтения указателя), `:126`, `:131` (Release-запись класса), `:141`, `:145` (Acquire-чтение класса производителем перед RMW). Единственные вызовы `from_initialized` — `tests/r6_sidecar_bitmap.rs:37,86,142,162,185`. Loom: `tests/loom_sidecar_bitmap.rs:21–35, 39–67, 69–98, 100–120`; `tests/loom_r11_small_sidecar.rs:69–100`.

**Наблюдено.**
1. Pin ищет `.store(class + 1, Ordering::Release)` в теле первого `fn issue(` файла `sidecar_bitmap.rs`. Эта строка — Dense-ветка (`:92`). Dense создаёт только `from_initialized`, а её зовёт исключительно `tests/r6_sidecar_bitmap.rs`, который компилирует модуль через `#[path]`. Production всегда идёт через `from_leaves` и ветку `ClassMap::Leaves` (`:95` → `leaf_classes.rs:117–135`).
2. Ни одно упорядочение `leaf_classes.rs` (`:113`, `:122`, `:126`, `:131`, `:139–145`) не проверяется ни pin-файлом, ни другим тестом с `include_str!` (перечень `include_str!` в `tests/` — приложение A).
3. В обоих позитивных тестах `loom_sidecar_bitmap` класс пишет и читает один и тот же главный поток: `issue` на `:42–43`, `:73`, `:91`; `cut` и `class` на `:48–53`, `:80–83`, `:95–96`. Производители выполняют только `fetch_or`. Проверки `first | second == 3` и `first & second == 0` следуют из атомарности RMW при любом `Ordering`. Негативный тест (`:100–120`) моделирует производителя, который сам делает `issue` после `publish`; в production производитель `issue` не вызывает.
4. Публикацию mixed-указателя моделирует `loom_r11_small_sidecar::model_visibility` (`:69–100`, с Relaxed-контрфактуалом). Но это теневая копия, без привязки к `leaf_classes.rs:113`. Ph6a уже фиксирует корневые loom-модели как MODEL-LIMIT (`docs/reviews/2026-10-05-ph6a-correctness-matrix.md:26,76`); новое здесь — то, что *pin*, призванный закрыть разрыв «shadow ↔ production», для этого ребра пинит не production.

**Выведено.** Мутант `Release → Relaxed` на `leaf_classes.rs:113` или `Acquire → Relaxed` на `:139` не роняет pin-файл (текст `sidecar_bitmap.rs` не меняется) и не роняет loom (модели не зависят от `src/`). На x86 TSO функциональные тесты, по всей вероятности, тоже останутся зелёными. Между тем это ребро несущее. Owner делает spill uniform-листа в mixed, выдавая блок другого класса в тот же лист, а в это время производитель освобождает свой ранее выданный блок и читает класс своей гранулы через `encoded()`. Между этими операциями нет иного happens-before, кроме пары `:113` Release / `:139` Acquire. Ослабление даёт чтение неинициализированных `AtomicU8` свежего System-листа. Поймать мутант могут только TSan (если стресс попадёт в окно spill↔publish) и нативный arm64. Напротив, ребро, которое pin закрепляет («issue Release» в Dense-ветке), по-видимому, для видимости производителю не несущее: happens-before даёт пользовательская передача указателя, а cut и чтение класса выполняет сам owner. Дефекта в текущем коде нет: все порядки в `leaf_classes.rs` при чтении верны.

**Рекомендация.**
1. Расширить pin-файл: читать `leaf_classes.rs` и закрепить `:113` (Release), Acquire-загрузку `mixed[leaf]` в `issue`, `encoded` и `prepared`, Acquire-чтение класса в `encoded`.
2. Закрепить, что `SmallSidecar::bitmap` использует `from_leaves` — иначе Dense-пин проходит вакуумно.
3. В `loom_sidecar_bitmap` разнести issuer и cutter по разным потокам либо снять заявление «ordering model of SidecarBitmap's exact atomic operations».

**Oracle:** мутант `Release → Relaxed` на `leaf_classes.rs:113` краснит расширенный pin (сегодня, по чтению, зелёный).

### R17-VER-02 — реестр доказательств не видит изменений `src/`

**Места.** `scripts/verify-evidence-registry.mjs:353–375` (`rowTests`: из полей строки извлекаются только пути `tests/…`), `:775–790` (правило `evidence-stale`: сравниваются байты цитируемых test-файлов между `source_sha` и HEAD). `docs/reviews/2026-10-05-ph7-evidence-registry-receipt.md:167` («изменение `src/` после `source_sha` судья не видит»). Проводка: `.github/workflows/ci.yml:1385–1396` (job `evidence-registry`, `--strict`), `scripts/check-all.mjs:1118–1120`. Реестр — `docs/evidence/registry.csv`.

**Наблюдено.** Последняя правка `registry.csv` — `11f2c0bc` (2026-10-06). `git log --oneline 11f2c0bc..HEAD -- src/` даёт **17** коммитов, `git diff --shortstat 11f2c0bc HEAD -- src/` — **91 файл, +1336/−854**. Среди них два `perf(runtime)` на always-on путях `production` (`7232598b`, `b707d196`), `perf(opt-in)` `2600b337` и исправления `0228d150`, `486f5ace`, `32cacc97`, `9df9f6b8`, `222e913d`. CI на базе (job `113339817550`): `VERDICT: INCOMPLETE (strict)`, `PASS=127`, job success. Конкретный дрейф: строка R6a-M5 (`registry.csv:154`, PASS) цитирует мутант на `batch.rs:188 filled += n`. На `fd954856` это строка 188, на HEAD — `src/registry/heap_core/alloc/batch.rs:189`: файл после этого меняли `2600b337` и `c8b9344a`.

**Выведено.** 127 PASS-строк — утверждения о коде на 2026-10-05/06. Никакое изменение `src/` после `source_sha` не понижает строку, поэтому CI-гейт зелёный по построению. Ограничение записано только в §6 receipt, ни в одном индексе — это класс R22-3. Правило `evidence-stale` ловит только дрейф цитируемых тестов; дрейф защищаемого ими кода оно не видит.

**Рекомендация.** (а) Сейчас завести карточку `[T]` (например, в `TRACKED_verification_coverage.md`). (б) Дать строке поле src-scope (пути, на которых она держится) и правило `evidence-stale-src`: `git diff --quiet <source_sha> HEAD -- <scope>` → понижение до CONDITIONAL. Минимальный вариант — печатать число PASS-строк, чей `source_sha` предшествует последнему src-коммиту.

**Oracle:** на текущем дереве такое правило помечает PASS-строки с `source_sha` раньше `11f2c0bc` (по чтению — все 127), после переснятия — чисто.

### R17-VER-03 — дрейф Miri-инвентаря и Miri-тесты удалённых механизмов

**Наблюдено (места в каждом пункте).**
1. `tests/r11_ph4a_lease_miri.rs`, `tests/r11_ph4b_lease_miri.rs` и `tests/regression_r2_06_header_race_miri.rs` не упоминаются ни в `.github/`, ни в `scripts/`, ни в `package.json`: ни один Miri-раннер их не интерпретирует. На них стоят PASS-строки реестра R4a-01, R4a-02, R4b-04, R4b-05, R4c-03, R4c-04 (`registry.csv:34,35,47,48,61,62`) и KNOWN-RED R4c-05 (`:63`). Для Loom есть `scripts/verify-loom-target-wiring.mjs` («Every root Loom target must run in CI and the local Loom matrix»), для Miri аналога нет.
2. `scripts/miri.mjs:2–3` заявляет «Mirrors the CI miri matrix», но содержит 4 цели, которых нет в CI: `regression_r2_05_diag_provenance_miri` (`:56`), `regression_realloc_oob_old_layout` (`:74`), `regression_virgin_bitmap_skip` (`:108`), `regression_w3_stats_aliasing_miri` (`:115`). Цель `region_invariants` идёт с `experimental` (`:46`), а в CI — без фич. И наоборот: в CI есть четыре EXPECTED RED paused-шага, включая `production`, а в `miri.mjs` — один `alloc-global` paused.
3. `README.md:1388` и `docs/ARCHITECTURE.md:492`: «`scripts/miri.mjs` includes … `r8_global_box_provenance`». В `miri.mjs` такой цели нет.
4. `tests/regression_r2_06_header_race_miri.rs:6–35, 80, 124–125` описывает гонку `SegmentHeader::read_at` с `push_large_deferred_free`/`deferred_next`. Этих символов в `src/` нет; `read_at` атомарно сращивает теперь `owner_state` (`segment_header_impl.rs:744–749`), а удалённый производитель пишет только в независимый дескриптор (`routing.rs:49–61`). Документированная команда воспроизведения (`:31–35`) завершится `process.exit(2)` («unknown test name», `miri.mjs:162–170`).
5. Тесты CI-job `miri-plain` документируют удалённые протоколы: `tests/regression_xthread_small_ring_miri.rs:7–35` — `RemoteFreeRing`, `regression_xthread_thread_free_alias_miri.rs:1–40` — `thread_free`/`deferred_large`. Их тела уже идут через sidecar (`dbg_drain_sidecar_ingress`).
6. `tests/decommit_miri_cycle.rs:1–11`: «prove the recommit path is sound». Но retain-ветку decommit (`release_follows == false`) вызывает только bench-хук (`decommit.rs:106`); в production пустой сегмент уходит в пул или освобождается (`alloc_core_small_pool_impl.rs:419–429`, `decommit.rs:263–277`). Recommit-пути в production нет.
7. Локальный раннер не знает EXPECTED RED. Цель `miri_global_box_acceptance … paused` (`miri.mjs:127`) по принятому P1-box (item 164) не печатает маркер COMPLETE. Её проверка `code === 0 && completed` (`:237`) поэтому всегда даёт FAIL, а с ней и итог `[miri] overall: FAIL` (`:270–271`) — при любом состоянии остальных целей. В CI тот же случай пинится по сигнатуре и месту (miri-core, шаги EXPECTED RED). Локально новый Miri-регресс неотличим от известного.

**Выведено.** Сводные заявления о Miri-покрытии — README, ARCHITECTURE, реестр (R6a-C1 «Miri suite 15/16 entries») — расходятся с тем, что реально интерпретируется. Три `*_miri`-цели не интерпретируются никогда, а шесть PASS-строк держатся на прогонах, которые сегодня не воспроизводит ни один гейт. Покрытие текущего sidecar-пути под Miri есть (`miri-plain`), но подписано чужими механизмами. `regression_r2_06` по построению не может поймать регрессию своего класса: удалённый поток больше не пишет в память резервации.

**Рекомендация.** Добавить `scripts/verify-miri-target-wiring.mjs` по образцу loom-верификатора: каждый `tests/*_miri.rs` и каждая цель `miri.mjs` ↔ CI, либо явный waiver с причиной. Исправить README и ARCHITECTURE. Переписать `r2_06` под текущий протокол или удалить его. Обновить doc'и `miri-plain`-тестов и `decommit_miri_cycle`. В `miri.mjs` ввести EXPECTED RED с той же сигнатурой и местом, что в CI. Строки R4a-01/02, R4b-04/05 и R4c-03/04 понизить до CONDITIONAL, пока цели не подключены.

**Oracle:** новый верификатор на текущем дереве красный (3 неподключённые цели и 4 только локальные), после подключения или waiver — зелёный.

### R17-VER-04 — Kani после терминального cutover не покрывает production-инвариантов

**Места.** `src/kani_proofs.rs:4–23` (обзор), `:25–146` (9 `node_proofs`), `:148–169` (2 `hand_proofs`, комментарий `:148–153`), `:171–223` (2 `pack_proofs`, комментарий `:171–177`, `:181`). `.github/workflows/ci.yml:3076–3091` (комментарий `:3077–3080`), `.github/workflows/kani.yml:3–6, 37–40`. `docs/correctness-open-items/TRACKED_verification_coverage.md:144–178` (item 18). `src/registry/heap_registry/stack.rs:1`.

**Наблюдено.**
- CI-лог Kani на базе (run `37785720332`, job `113339817828`): «Complete - 13 successfully verified harnesses, 0 failures, 13 total». Ring-доказательства удалены в `a4245965` (`git log -S'ring_wrap_proofs' -- src/kani_proofs.rs`). Item 18 по-прежнему говорит о 6 ring-доказательствах и «all 19 harnesses».
- `kani_proofs.rs:174–176`: «These harnesses ARE the regression tests for the `free_slots` packing». Однако у реестра нет интрузивного free-list (`stack.rs:1`: «Slot discovery without an intrusive free list»), а `TaggedIndex` в `src/` встречается только в `loom_shim.rs` и `kani_proofs.rs`.
- `ci.yml:3077–3080` называет `pack_proofs` «registry pointer packing».
- `kani_proofs.rs:148–153`: конкурентные инварианты `AtomicSlot` «already verified by loom (11 harnesses in CI)». Loom-модели теневые (`loom_epoch.rs:6–11`), и одна из них моделирует удалённую функцию (R17-VER-05).
- Нет ни одного `kani::cover!`. Все `kani::assume` выполнимы, поэтому вакуумности нет. Стабов нет. Unreachable-checks в логе — библиотечные, не пользовательские. `kani.yml:3–6` утверждает, что Kani «kept APART from ci.yml … never gate the main CI gate», но `ci.yml:3076–3091` гоняет Kani на каждом push.

**Выведено.**
- 9 node-доказательств — round-trip над однострочными обёртками `ptr::read`/`write` на локальных буферах, контракты вызывающих не моделируются (так и сказано в `:6–10`).
- 2 hand-доказательства тривиальны и относятся к `experimental`.
- 2 pack-доказательства проверяют тип, которого production не использует.
- Чистая арифметика текущего протокола, идеальная для CBMC, не доказана: `pack_large_state`/`large_phase`/`next_large_generation_bounded` (`terminal_words.rs:47–84`); таблица переходов `LargeReservationState` (`reservation_state.rs:42–161`, последовательная семантика атомиков Kani поддерживается); `SidecarBitmap::granule` и `scan_from` (`sidecar_bitmap.rs:131–150`); индексация `ClassLeaves` (`leaf_classes.rs:62–147`); версия `SaturationHint` (`saturation.rs:46–72`); `hash_index` (`hash.rs:28–31`).
- Часть этого покрыта юнит-тестами, например `tests/r6_terminal_large_state.rs:64` (reduced-width), но не исчерпывающе.

**Рекомендация.** Заменить `pack_proofs` (по варианту (б) item 167) харнессами для перечисленного. Добавить `kani::cover!` на ключевые ветви. Обновить item 18 и комментарии в `kani_proofs.rs` и `ci.yml`. Согласовать `kani.yml` с `ci.yml`.

**Oracle:** мутант `>=` → `>` в `next_large_generation_bounded` (`terminal_words.rs:80`) даёт FAILURE в Kani.

### R17-VER-05 — `loom_epoch` моделирует удалённый `AtomicSlot::evict`

**Места.** `tests/loom_epoch.rs:17–19` («This mirrors `AtomicSlot::read_with` exactly. The writer mirrors … `AtomicSlot::evict`»), `:70–85` (`evict`: `swap(value → VACANT, AcqRel)`, затем load generation `Acquire` и `store(g + 1, Release)`). `src/concurrent/epoch/hand.rs:395–437` (`try_evict_at`: CAS generation `AcqRel/Acquire` на `:418–423`, затем `swap(null, AcqRel)` на `:437`). `tests/loom_sharded.rs:83–104` (модель `try_evict_at` совпадает с кодом). `.github/workflows/ci.yml:3175–3183`.

**Наблюдено.** `fn evict` в `src/` нет: по `git log -S'fn evict(' -- src/concurrent` его добавил `f5565804` и удалил `e0749303` (2026-06-24). CI-sentinel loom-experimental проверяет только контрфактуал `counterfactual_no_recheck_yields_torn_read`.

**Выведено.** Eviction-часть `loom_epoch` верифицирует протокол (сначала swap, потом generation), которого в коде нет уже 3,5 месяца; doc «mirrors exactly» ложен. Реальный протокол покрыт только `loom_sharded`, тоже теневой и без pin. Правку порядков `try_evict_at` не отразит ни одна модель. Тир `experimental`, deprecated.

**Рекомендация.** Переписать writer модели под `try_evict_at` либо удалить eviction-часть со ссылкой на `loom_sharded`. Добавить pin порядков `hand.rs:418–437` по образцу `r11_ph4a_*`.

**Oracle:** pin краснеет на Relaxed-мутанте `hand.rs:421`.

### R17-VER-06 — сканеры hook-safety отбирают по имени `dbg_`; неверная классификация трёх observers

**Места.** `scripts/verify-dbg-hook-safety.mjs:373–375` (единственный селектор — `\bpub\s+(unsafe\s+)?fn\s+(dbg_\w+)\b`), `:16` («Pure observers: read-only, no allocator-state mutation»), `:103–106`. `scripts/verify-alloc-core-dbg-internals-exhaustive.mjs:20–25` (тот же принцип: «every `pub fn dbg_*`»). `src/registry/bootstrap/registry.rs:135–142` (`slot()` → `ensure_chunk`), `:250–274` (abort на `:271`), `:324–335`. `src/registry/heap_registry/counters.rs:407–418`.

**Наблюдено.**
- Перепись (приложение A): **48** публичных хуков `*_for_test(s)`/`*_test` без префикса `dbg_`, из них 1 `pub unsafe fn` (`SmallSidecar::owner_state_for_test`, `small_sidecar.rs:112`). Среди мутаторов: `RouteDirectory::fail_next_registration(s)_for_test` (`directory.rs:718,724`); `maintenance_service::{fail_next_start, pause_start, fail_worker, try_fallback_step}_for_test` (`:206,212,232,185`); `HeapRegistry::maintenance_pass_for_test` (`maintenance.rs:44`); `EpochRegion::_set_slot_generation_for_tests` (`epoch_region.rs:641`); `ShardedRegion::{_reset_my_shard_binding, _set_remote_free_hint}_for_tests` (`sharded_region.rs:719,776`); `SmallSidecar::fail_*_for_test` (`small_sidecar.rs:92,97,131`).
- Ни один из 48 не попадает ни в один из двух сканеров. `pub const fn dbg_*` (`ensure.rs:212`, `diag_probes.rs:51`) регулярное выражение тоже не ловит.
- PURE_OBSERVERS содержит `dbg_slot_state`, `dbg_slot_generation` и `dbg_slot_initialised`. Все три зовут `Registry::slot` → `ensure_chunk`, который на нематериализованном чанке делает ОС-резервирование (`ensure.rs:67–120`), а при отказе — `std::process::abort()` (`registry.rs:271`).

**Выведено.** Политика R30-2 («shape-independent») на деле зависит от имени. Класс R31 P2-12 (item 9, закрыт переименованием одного хука) сохраняется для 48 хуков. Живого нарушения среди просмотренных не найдено: они гейтированы `internals`/`bench-internals`, у `unsafe`-хука есть `# Safety`. Классификация трёх observers неверна: побочный эффект и путь abort, хотя доступ к ним только под `internals`. Для сравнения: для `stats()` ту же материализацию уже убрали переходом на `slot_if_materialised` (R2-11).

**Рекомендация.** Расширить `scanFile` на все `#[doc(hidden)] pub (const )?(unsafe )?fn` в `src/` (минимум — суффиксы `_for_test(s)` и префикс `_`) и классифицировать найденное. Три observer'а перевести на `slot_if_materialised` либо в SAFE_MUTATORS с обоснованием.

**Oracle:** расширенный сканер выдаёт 48 непроверенных хуков до классификации; фикстура `pub fn foo_for_test(&mut self)` с мутацией краснит.

### R17-VER-07 — no-panic контракт неверно описывает abort'ы; сканер их не видит

**Места.** `src/global/sefer_alloc/mod.rs:188–190`: «The one deliberate process kill on the alloc path is a direct `std::process::abort()` (registry chunk-materialisation OOM, `registry/bootstrap/registry.rs`)». `tests/no_panic_doc_accuracy.rs:140–210` (pin-фразы), `:212–222` (7 файлов), `:466–487` (только `.expect(`, `panic!`, `unreachable!`). `src/registry/heap_registry/claim.rs:213` (`slot_or_none`), `counters.rs:375,414` (`slot()` в doc-hidden хуках).

**Наблюдено.**
- По переписи 110 `process::abort()` в 28 файлах `src/`. На путях `GlobalAlloc`: `bitmap_scan.rs:21`; `route_slots.rs` (22 места); `segment_route/directory.rs` (15); `alloc_core/sidecar_drain.rs` (7); `segment_table_impl.rs` (7); `issue_transaction.rs:119`; `claim.rs:522–535` (`HeapLease::drop`: abort при `panicking()` и при проигранном CAS).
- Названный в доке abort (`registry.rs:271`) недостижим из production: claim идёт через `slot_or_none` (`claim.rs:213`), а infallible `slot()` зовут только doc-hidden тест-хуки.
- Сканер пинит фразы о панике, но не об abort. Вне его охвата (7 файлов, 3 вида токенов) остаются релизные `assert!` на путях `GlobalAlloc`: `terminal_words.rs:49` (`pack_large_state`, арифметически недостижим) и `alloc_core_small_magazine.rs:143` (`virgin-zero-skip`, константная граница).

**Выведено.** Публичный контракт даёт ложную картину «единственного» abort. В коде около сотни abort-tripwire'ов на нарушение инвариантов: по стилю проекта они допустимы, но нигде не перечислены. `assert!`-ы формально покрыты catch-all фразой контракта — дефекта нет, но сканер R16-01 (item 174) их не видит. R16 рекомендовал охватить «все файлы, достижимые из `GlobalAlloc`»; фикс ограничился 7.

**Рекомендация.** Заменить фразу точной: abort'ы как tripwire'ы инвариантов на owner-only путях, в протоколе lease и в OOM чанка, достижимом только тест-хуками. Закрепить в `no_panic_doc_accuracy` отсутствие старой фразы. Расширить сканер на семейство `assert!` и на все файлы, достижимые из `GlobalAlloc`, либо явно задокументировать его охват.

**Oracle:** doc-pin на отсутствие «The one deliberate process kill»; расширенный сканер показывает `terminal_words.rs:49`, пока строку не внесут в allowlist.

### R17-VER-08 — `docs/INVARIANTS.md` расходится с кодом

**Места.** `docs/INVARIANTS.md:3–6` («form the spec that every future change must keep green»), `:62–77` (M2), `:95–98` (M4), `:99–107` (M5), `:108–113` (M6). Код: `src/alloc_core/large/alloc_core_large.rs:143, 216, 517–534, 584–631`; `tests/r8_large_alignment.rs:6–17`; `leaf_classes.rs:10, 102`; `decommit.rs:106, 131–132, 263–277`; `alloc_core_small_pool_impl.rs:419–429`.

**Наблюдено.**
- (а) Остаток M2 ссылается на `RemoteFreeRing` и на `tests/regression_xthread_double_free_residual.rs`, `tests/loom_magazine_ring_compose.rs` — обоих файлов нет.
- (б) M4: «Requests with `align >= SEGMENT` are rejected with `null` by design». Но biased Large реализован, и `r8_large_alignment.rs:6–17` утверждает успех и выравнивание для 1×, 2× и 16×SEGMENT.
- (в) Обоснование M5 («no … `std::alloc` on the path — metadata self-hosts in segment memory»): дескрипторы маршрутов, sidecar и mixed-листы выделяются через `System` (`leaf_classes.rs:10,102`; `docs/CROSS_THREAD_STATE_MACHINES.md` §9.1). Само правило — не через глобальный аллокатор — цело.
- (г) M6: «decommitted when its live-block count drops to zero and recommitted on first reuse». У retain/recommit-ветки нет production-вызовов (только bench-хук, `decommit.rs:106`), production — release или пул.
- Ни `no_stale_doc_references`, ни другой guard файл `INVARIANTS.md` не читает.

**Выведено.** «Спецификация» расходится с кодом и тестами в четырёх пунктах; ссылки на несуществующие тесты создают видимость покрытия остатка M2. Это не prose-долг `src/` (item 154): это эталонный документ, на который опираются fuzz-цели (`fuzz/fuzz_targets/heap_core_ops.rs:6–7`) и тесты.

**Рекомендация.** Обновить M2, M4, M5 и M6. Добавить в `no_stale_doc_references` проверку существования всех `tests/*.rs`, упомянутых в `docs/INVARIANTS.md` и в §9 `CROSS_THREAD_STATE_MACHINES.md`.

**Oracle:** такой guard сегодня красный на двух путях.

### R17-VER-09 — debug-only защиты метаданных на холодных owner-путях

**Места.**
- `segment_table_impl.rs:968–979` (`free_list_push`): только `debug_assert!(top < FREE_LIST_CAPACITY)`, затем `Node::write_u32` по `free_list_slot_ptr(top)`. `segment_header_layout.rs:106–116`: `free_top` лежит сразу за массивом.
- `alloc_core_small_pool_impl.rs:256–272` (`release_or_pool_empty_segment`): комментарий обещает «the guard is O(1) and makes the invariant local and robust», но guard — `debug_assert!`. Так же устроен `:665` (`release_empty_current_small_for_trim`).
- `hash.rs:80–87` (production `hash_insert_identity`): проверки переполнения нет вовсе. `debug_assert!` «hash table full» есть только в `hash_insert` (`:61–77`), а его единственный вызывающий — test-only harness (`harness.rs:135–137`).

**Наблюдено.** Все три — холодные owner-only пути (recycle, приём в пул, trim, register).

**Выведено (release).**
- (1) Повторный push при `top == FREE_LIST_CAPACITY` пишет индекс в само слово `free_top`, затем `free_top = top + 1`; дальнейшие pop читают за пределами массива — тихая порча метаданных реестра сегментов.
- (2) Повторное помещение сегмента в пул или освобождение `small_cur`, уже стоящего в пуле, даёт цикл или висячую ссылку в списке пула; дальше — повторная выдача или двойное освобождение сегмента.
- (3) Переполнение хеш-таблицы — бесконечный цикл в `register`.

Все три достижимы только после уже случившейся порчи: двойного recycle или pool, утечки hash-записей. Поэтому P4. Но в соседнем коде проект последовательно делает такие нарушения release-abort'ом (110 мест), а комментарий пула прямо называет debug-only проверку «robust». Цена release-проверки — одно сравнение на холодном пути. Proptest backshift (`segment_table_backshift_proptest.rs`) гоняет harness-only `hash_insert` и test-only ветку `id == 0` в `hash_find` (`hash.rs:255`), поэтому production-вставку `hash_insert_identity` и ветку `base_at(id − 1)` покрывают только интеграционные тесты.

**Рекомендация.** Release `std::process::abort()` в `free_list_push`, `release_or_pool_empty_segment` и `release_empty_current_small_for_trim`. Ограничить probe в `hash_insert_identity` числом `HASH_CAPACITY` с abort. Пометить `hash_insert` как test-only или переключить harness на `hash_insert_identity`.

**Oracle:** subprocess-тест (по образцу `r12_01`) через doc-hidden harness — двойной push в free-list → abort. Сегодня в release это тихая порча, в debug — panic.

### R17-VER-10 — нет per-PR рандомизированного дифференциала для отгружаемого слоя

**Места.** `tests/alloc_core_differential.rs:21, 63` и `tests/heap_differential.rs:1–20, 75` (оба — `AllocCore`). `fuzz/fuzz_targets/heap_core_ops.rs:1–31` (однопоточный; cross-thread «covered by the TSan + aarch64 CI gates and the loom harnesses»). `.github/workflows/ci.yml:3946–4003` (`fuzz-run`: только `schedule`/`workflow_dispatch`, 10 минут на цель), `:3913–3944` (на push — только сборка). `docs/ARCHITECTURE.md:491`.

**Наблюдено.** Все proptest-дифференциалы по M1–M4 работают на `AllocCore` (`sharded*` — тир `experimental`). Слой `HeapCore`/`SeferAlloc` — magazine, overflow-flush, `flush_all_tcache`, magazine-путь `alloc_zeroed`, realloc promotion, sidecar ingress — рандомизирован лишь `heap_core_ops`: однопоточно, раз в неделю. Рандомизированного cross-thread оракула нет вовсе, а loom, на который ссылается fuzz-цель, — теневой (R17-VER-01).

**Выведено.** Правку, ломающую инвариант магазина на always-on пути (того же класса, что `b707d196`), до слияния ловят только ручные интеграционные тесты. Рандомизированный оракул M1–M4 по production-слою срабатывает не раньше чем через неделю.

**Рекомендация.** Per-PR proptest (~64 случая, по правилу скорости) поверх `globalalloc-model` для `HeapCore` (через `dbg_claim_lease`) или для прямых вызовов `SeferAlloc`, с переиспользованием генератора `heap_core_ops`. Плюс малый вариант на 2 потока с drain-оракулом exactly-once.

**Oracle:** мутант в overflow-flush (`dealloc_own_base.rs:482–508`, например пропуск очистки бита резидентности одного слота на `:482` или неверный сдвиг компакции на `:508`) ловится per-PR.

### R17-VER-11 — ГИПОТЕЗА: aarch64 через `cross` не даёт аппаратного weak-memory покрытия

**Места.** `.github/workflows/ci.yml:3376–3378, 3402–3406, 3414–3430` («aarch64 is the load-bearing run here (weak memory model can expose ordering bugs x86_64 hides)»), `docs/ARCHITECTURE.md:496` («relaxed-memory smoke»). Для сравнения: `ci.yml:3439–3441` (сам CI признаёт: «QEMU/cross can provide correctness coverage … but not the relevant acquire/release lowering timing») и `ci.yml:2219–2233` (`test-macos`).

**Наблюдено (CI-лог, только чтение).** Job `113339818244`: `Image: ubuntu-24.04` (x86_64), загружается `cross-x86_64-unknown-linux-musl` и образ `ghcr.io/cross-rs/aarch64-unknown-linux-gnu:0.2.5`. Job `113339817570` (`test macos (production)`): `Image: macos-26-arm64`, `stable-aarch64-apple-darwin`, прогон `production internals`.

**Гипотеза.** Образы cross-rs исполняют тесты aarch64 в QEMU user-mode на x86_64. QEMU отображает обычные `ldr`/`str` гостя на обычные обращения хоста, а x86 TSO запрещает большинство переупорядочиваний, разрешённых ARM. Поэтому аппаратные weak-memory эффекты в этих строках, вероятно, не проявляются; компиляторные переупорядочивания LLVM для aarch64 — проявляются. **Ложна**, если исполнитель cross-шагов — нативный arm64 или используемый QEMU эмулирует более слабую модель гостя на TSO-хосте. Проверить можно только исполнением litmus-теста; не проверялось — режим только чтение.

**Выведено.** Если гипотеза верна, заявление «load-bearing weak-memory run» переоценено, а аппаратное покрытие weak-memory для production даёт только `test-macos`, то есть Apple Silicon, функциональные тесты.

**Рекомендация.** Переименовать cross-строки в «codegen/functional on aarch64» и явно отнести weak-memory покрытие к `test-macos`, либо добавить нативный `ubuntu-24.04-arm` job `production internals`: такой runner уже используется в `ci.yml:3446`.

### 2.2. Новые доказательства к существующим пунктам (статусы не меняю — решение владельца)

| Пункт | Что нового | Предложение |
|---|---|---|
| **17** (`TRACKED_verification_coverage.md`) | (1) Обоснование принятого риска цитирует Miri-покрытие через `reclaim_offset_unit` и `remote_fanin_miri_minimal_retry_ub_check` — обоих тестов нет (`tests/remote_fanin.rs` переписан; ссылки висят и в `tests/regression_paused_owner_{multisegment,wallclock}.rs:59/74`). (2) Довод (б) «fallback … effectively single-threaded» ослаблен: с `c76a4207`/`a4245965` любой поток, освобождающий fallback-owned указатель, синхронно входит в fallback-кучу через `try_with_heap` под `LOCK` (`routing.rs:35–44`, `fallback.rs:389–403`). (3) `ci.yml:3101–3108` подразумевает, что loom-модель `loom_fallback_init` «collapsed into» реальную `OncePtrCell`-сюиту, но fallback использует собственный `INIT_STATE` (`fallback.rs:79, 144–311`); сам `63991cc3` говорит, что fallback «did NOT map cleanly onto the cell». Следовательно, протокол init+LOCK fallback-кучи с 2026-07-17 без модели и без целевого TSan-теста (TSan-шаги `ci.yml:3242–3293` его специально не нагружают) | обновить Current-number-or-verdict; рассмотреть shadow-loom модель `INIT_STATE` + `LOCK` + `try_with_heap` с pin порядков |
| **18** | См. R17-VER-04: в карточке «RESOLVED» сказано, что ring-доказательства есть и гоняются 19 харнессов; по факту ring-доказательства удалены `a4245965`, CI-лог — 13 харнессов | запись-коррекция статуса; новый пункт на production-цели Kani (R17-VER-04) |
| **160** (`TRACKED_misc.md`) | Карточка: «no loom model exists for this hint». Модель есть: `tests/loom_r11_epoch_false_full.rs` (добавлена `692b9a93`, 2026-10-01), подключена в CI loom-experimental с sentinel'ами (`ci.yml:3175–3183`), содержит контрфактуал `old_hint_only_drain_has_a_negative_counterexample`; push/drain и Relaxed-порядки совпадают с `epoch_region.rs:338–371, 585–596` | Next trigger выполнен — закрыть или переформулировать остаток (модель теневая, без pin) |
| **167** | Сильнее, чем в карточке: не только «нет production-имплементора», а прямо ложные отсылки в `src/` — `kani_proofs.rs:171–176` («ARE the regression tests for the `free_slots` packing»), `loom_shim.rs:251–253` («model-checking free_slots' head protocol in the root crate's loom CI jobs»), `:314–317, 377–378` (ссылка на `heap_registry/stack.rs::push_free_slot` и «REAL impl … is an `unsafe impl`» — их нет), `bootstrap/mod.rs:164–166` («`heap_registry`'s cfg-gated `StackStorage` impl» — нет), `bootstrap/registry.rs:13` (несуществующий `loom_xthread_protocol`) | исполнить вариант (б) карточки или хотя бы снять ложные утверждения |

### 2.3. Кандидаты, сознательно не повышенные до находок

- **`b707d196` заменил release-abort (R16-01 fix `9df9f6b8`) на `debug_assert_eq!`** в overflow-flush и `flush_all_tcache` (`dealloc_own_base.rs:482–491`, `tcache_flush.rs:84–95`); так же сделано `hot.rs:43–58` (R12-02). Инвариант «слот магазина ∈ зарегистрированный Small/Primordial» в release больше не проверяется. Это документированный компромисс: коммит и no-panic контракт его называют, а слоты попадают в магазин только после `contains_base`. Не находка; отражено в матрице как «debug-only».
- **`AllocStats::config_conflicts`** считает только registry-конфликты. Конфликты fallback-кучи идут в отдельный `CONFIG_CONFLICTS` (`fallback.rs:82, 375–383`) с собственным accessor'ом `fallback_config_conflicts` (`sefer_alloc/core.rs:372–377`). Doc поля (`alloc_stats.rs:153–173`) точен по scope, но фраза «This is the field to monitor when running multiple SeferAlloc instances» слегка переоценивает его. Судье по правилу R26-4 нужно читать оба счётчика.
- **`r11_ph4b_local_no_drop_pinned.rs`** не вырезает комментарии внутри `thread_local!` (`:23–43`) и берёт первое вхождение `static LOCAL:`. Теоретическая слепая зона; живого случая нет.
- **`debug_assert!` в `tls_heap.rs:516`, `segment_table_impl.rs:627, 667`** на пути `GlobalAlloc` в debug-сборке развернули бы панику наружу (класс R2-08), но достижимы только при нарушении внутреннего инварианта. Это покрыто catch-all фразой контракта.

## 3. Матрица покрытия «модуль/протокол → чем покрыт → пробел»

Обозначения: K — Kani; M — Miri в CI (названия job'ов); L — корневой loom (S = теневая модель); P — proptest/fuzz; U — юнит/интеграционные тесты; T/A — TSan/ASan; Arch — macOS arm64 (нативно) / cross aarch64.

| Модуль / протокол (`src/`) | K | M (CI) | L | P | U (примеры) | T/A, Arch | Главный пробел |
|---|---|---|---|---|---|---|---|
| Small/Primordial terminal publication: `remote_bitmap/{sidecar_bitmap, leaf_classes, bitmap_scan, bitmap_cut}`, `segment_route/small_sidecar` | — | miri-plain `regression_xthread_small_ring_miri` (doc устарел); miri-core `miri_global_box_acceptance narrow` (SB/TB) и `paused` (EXPECTED RED, P1-box) | S: `loom_sidecar_bitmap` (позитивы нечувствительны к порядкам), `loom_r11_small_sidecar`, `loom_terminal_owner_drain` | — | `r6_sidecar_bitmap` (только Dense), `r11_p3_small_sidecar_*`, `r12_01_*`, `r6_terminal_owner_drain`, `remote_fanin` | T: `stress_concurrent_boundaries`, `global_alloc_mt`; Arch ✓ | pin только publish/cut; production-порядки `ClassLeaves` не пинятся (R17-VER-01) |
| Large terminal publication: `large/reservation_state`, `segment_header/terminal_words`, `segment_route/large_state` | — (R17-VER-04) | miri-plain `regression_xthread_large_free_no_leak`, `…thread_free_alias_miri` | S: `loom_terminal_large` (порядки совпадают) | — | `r6_terminal_large_state`, `r6_large_credit*`, `r6_route_large_adapter`, `r8_terminal_global` | T: `regression_realloc_xthread_stamp`; Arch ✓ | нет pin порядков `publish_pending`/`claim_pending`; нет Kani на упаковке и переходах |
| Owner drain/reclaim: `alloc_core/sidecar_drain`, `alloc_core_small_reclaim`, `heap_core_xthread/sidecar_drain` | — | miri-plain `r6_terminal_owner_drain` (1 тест) | S: `loom_terminal_owner_drain` (абстрактный кредит) | — | `r11_ph4c_ingress_consume_exactly_once_oracle`, `r11_ph4b_late_publication_*`, `r8_owner_sidecar_miss` | T ✓; Arch ✓ | нет рандомизированного cross-thread оракула (R17-VER-10) |
| Route directory: `segment_route/{directory, pin, registration, shard_lock, route_*}`, `segment_table/route_slots` | — | косвенно (тесты выше) | S: `loom_r11_small_sidecar::model_route` (refcount последнего pin) | — | `r6_route_directory*`, `r6_route_lifecycle_*`, `r11_p4_1_*`, `r14_sidecar_owner_capability_negative` | T косвенно | нет модели реального refcount/`EntryHandle::drop` и shard-lock |
| Registry lease/claim/maintenance: `heap_registry/*`, `heap_slot`, `bootstrap/saturation` | — | — (lease-Miri не подключены, R17-VER-03) | S: `loom_r11_ph4a_heap_lease`, `loom_r8_maintenance_lease`, `loom_r11_registry_claim`, `loom_r11_ph4b_publish_recycle_drain` | — | `r11_p3_registry_claim_*`, `registry_basic`, `regression_claim_oom_initialised_gate`, `r7_p3_claim_scan` | T: `tls_heap_teardown_ordering_stress` | pin только `HeapLease::drop` и порядок teardown; `MaintenanceLease::drop`, claim Acquire и `SaturationHint::is_saturated` (RMW) не пинятся |
| Registry chunks: `bootstrap/{registry, ensure}` + `once-ptr-cell` | — | M crate-job `once-ptr-cell`; косвенно везде | real-type в крейте; в корне shim вне моделей | — | `regression_bootstrap_oom_sentinel_rollback`, `r7_p1_chunk_oom`, `regression_registry_chunking` | — | — |
| TLS bind/teardown: `global/tls_heap` | — | косвенно (`miri_global_box_acceptance`) | S: `loom_r11_ph4b_publish_recycle_drain` | — | `tls_heap_teardown_*`, `dealloc_only_no_bind*`, `r11_ph4b_*` | T ✓ | pin порядка teardown есть; `LOCAL`-pin не вырезает комментарии |
| Fallback-куча: `global/fallback` | — | — | — (`loom_fallback_init` удалён `63991cc3` без замены) | — | `regression_fallback_*`, `r6_fix_p2_fallback_dealloc`, `r3_1_fallback_remote_free`, `r12_04_*` | — | item 17 + новое доказательство §2.2: cross-thread вход через `try_with_heap` без модели и TSan |
| Small-сегменты: `small/{alloc_core_small/*, magazine, pool/*}` | — | miri-alloc-core (4 теста), miri-fastbin (2), `decommit_miri_cycle` (doc о recommit устарел) | — | P: `alloc_core_differential`, `heap_differential` (AllocCore) | `small_segment_pool`, `decommit_soak`, `regression_magazine_*`, `alloc_zeroed_virgin_small_skip` | A: `asan_alloc_core`; Arch ✓ | debug-only защиты пула (R17-VER-09); skip-пути «OS-zero» под Miri не исполняются (§5) |
| Segment table: `segment_table/{hash, segment_table_impl, active_kind_*, issue_transaction}` | — | косвенно | S: `loom_active_kind_index` (абстракция) | P: `segment_table_backshift_proptest` (harness `hash_insert`) | `segment_table_*`, `r10_active_kind_*`, `r11_ph3a_issue_transaction_*` | — | production `hash_insert_identity` без guard; proptest гоняет test-only двойника (R17-VER-09) |
| Large path/cache: `large/*` | — | miri-alloc-core: `regression_{large_align,page_aligned}_no_segment_exhaustion` | — | P (AllocCore) | `large_cache*`, `r8_large_alignment`, `r17_8_*`, `regression_large_cache_*` | A ✓; Arch ✓ | INVARIANTS M4 устарел (R17-VER-08) |
| HeapCore magazine/tcache: `heap_core/{alloc/*, free/*, state/*}` | — | miri-fastbin (2 теста); P1-box `paused` (EXPECTED RED) | — | только weekly fuzz `heap_core_ops` | `heap_core_tcache*`, `r16_perf84_activation`, `r12_02_*`, `batch_tcache` | T: production-шаг; Arch ✓ | per-PR рандомизации нет (R17-VER-10); корень магазина в release не проверяется (§2.3) |
| Realloc: `mem/realloc_fastpath`, `heap_core/free/realloc` | — | miri-alloc-core `regression_realloc_cross_class_shrink`, `stress_boundary_sweep` | — | P (AllocCore, prefix-оракул) | `realloc_in_place`, `regression_realloc_*`, `r14_4_promotion_*`, `r16_perf83_activation` | T ✓ | — |
| OS/platform seam: `platform/{os, node, sidecar, numa}` | K: 9 node round-trip (тривиальные) | aligned-vmem-miri (крейт); под Miri — другой бэкенд (`System`, явное обнуление) | — | — | `r15_02_*`, `oxx_r2_05_*`, `r8_os_release_oracle`, `segment_meta_page_alignment` | A ✓; macOS — реальный XNU | контракты `Node` вызывающими не моделируются (честно задокументировано) |
| Size classes: `platform/size_classes` | — | — | — | P: `size_classes_proptest` | `size_classes_lookup`, `size_classes_slow_path_equivalence` | — | — |
| Диагностика/статистика: `alloc_stats`, `counters`, `*_diag*` | — | локально `regression_w3_stats_aliasing_miri`, `regression_r2_05_diag_provenance_miri` (не в CI) | — | — | `stats_*`, `regression_r2_11_stats_no_materialize`, `r12_o4_routed_miss_counters` (alloc-stats-гейтированные оракулы) | T: `regression_percounter_perheap_aggregation` | см. §5; три observer'а с побочным эффектом (R17-VER-06) |
| Experimental: `concurrent/{epoch, lock_free, sharded, pinning}` | K: 2 hand (тривиальные) | — (item 171: Miri красный на epoch) | S: `loom_sharded` (✓), `loom_epoch` (eviction устарела, R17-VER-05), `loom_r11_epoch_false_full` (✓) | P: `sharded`, `sharded_remote` | `epoch`, `lock_free`, `sharded*`, `r15_01_*`, `r14_shard_*` | Arch: cross `experimental` | eviction-модель epoch неверна |
| Exact-object proto: `global/exact_object/*` | — | — | — | — | `r11_exact_object_proto` | — | вне `production` |

**Инварианты из `docs/INVARIANTS.md`:**

| Инвариант | Чем покрыт | Пробел |
|---|---|---|
| I1–I7 (Region/Handle, крейт `sefer-region`, реэкспорт) | `region_invariants` (+ Miri в miri-core), `differential` (proptest), `freelist_reuse`, тесты крейта | — |
| M1/M3/M4 validity, no-overlap, alignment | proptest AllocCore; weekly fuzz SeferAlloc; `stress_safe_surface_no_aliasing`, `stress_boundary_sweep` (+Miri), `r8_large_alignment` | M4 в спецификации устарел; рандомизации отгружаемого слоя per-PR нет |
| M2 double-free/UAF | `double_free_guard`, `regression_magazine_oracles` (+Miri fastbin), `heap_core_tcache_m2`, `r11_4_dealloc_batch_same_segment_double_free`, `r10_7_alloc_batch_xthread_double_free` | раздел остатка ссылается на 2 несуществующих теста и удалённый ring |
| M5 reentrancy | `alloc_core_reentrancy` (только AllocCore, нативно), `r11_ph5b_c6_dealloc_no_alloc`, `r11_ph5b_c6_dealloc_system_count` | обоснование «self-hosts in segment memory» неверно (System-sidecars) |
| M6 OS return | `decommit_soak`, `small_segment_pool`, `r8_os_release_oracle`, `decommit_miri_cycle` (Miri) | описание decommit/recommit не соответствует production |
| M7 owner routing, exactly-once | `r6_terminal_*`, `r11_ph4c_ingress_consume_exactly_once_oracle`, `remote_fanin`, `race_repro`/`race_norecycle` (TSan), теневой loom | нет рандомизированного cross-thread оракула |
| M8 generational coherence | `owner_stamp_round_trip`, `regression_counter_wrap`, `r6_terminal_large_state` (no-wrap) | Kani на арифметику поколений нет (R17-VER-04) |
| CROSS_THREAD §9.1–9.6 | см. строки Large/Small/registry выше | §9.7 (P1-box, 164) — принятый дефект, не трогался |

## 4. Вне темы (место + одна строка)

- `tests/src_file_size_cap.rs:3` ссылается на несуществующий `src/registry/heap_core_xthread/overflow.rs`.
- `tests/regression_paused_owner_multisegment.rs:59`, `tests/regression_paused_owner_wallclock.rs:74` ссылаются на несуществующий тест `remote_fanin_miri_minimal_retry_ub_check`.
- `.github/workflows/kani.yml` дублирует Kani-job из `ci.yml`, а его шапка (`:3–6`) утверждает обратное.
- `docs/ARCHITECTURE.md` §8 (`:481–500`): таблица «Verification stack» вообще не упоминает Kani.
- `src/registry/heap_core/alloc/hot.rs:659`: проза про `HeapOverflow` в настоящем времени — уже учтено в закрытии item 22 (R16) и классе item 154.

## 5. Что проверено и не дало находок

- **Kani:** все 13 харнессов вызывают реальный код (`Node`, `AtomicSlot`, `TaggedIndex`), а не копии. `kani::assume` в `:49`, `:61`, `:199–200` выполнимы, стабов нет, `unwind` не нужен (константные циклы), в CI 13/13 SUCCESSFUL.
- **Счётчики как oracle:**
  - `SMALL_/LARGE_ZERO_PASS_CALLS` инкрементируются ровно на ветках с явным обнулением, в `AllocCore` и `HeapCore` (`mem_impl.rs:115–141`, `hot.rs:666–692, 734–740`).
  - Пара `SEGMENTS_RESERVED/RELEASED_TOTAL` согласована: `Drop for Segment` после R15-02 (`os.rs:167–172`), `release_segment` (`:564–573`); ранние выходы biased-пути роняют `Reservation` до инкремента (`:184–216`).
  - `LARGE_REMOTE_RETIREMENTS` — 4 инкремента, все в связке `claim_large_route` + `reclaim_large_segment` (`sidecar_drain.rs:167–170, 249–252, 290–292, 339–341`), других путей reclaim нет.
  - `FOREIGN_OR_UNROUTABLE_FREES` — обе ветки отбрасывания в `routing.rs:27–31, 62–65` безусловны. Повторная Small-публикация сливается молча, что doc и признаёт.
  - Все 12 счётчиков директории и счётчики Tier-1 инкрементируются; оракульные тесты по ним гейтированы `alloc-stats` (проверены 17 файлов).
- **`cfg`-ветки (тема f).**
  - `cfg(test)` в `src/` нет, кроме `cfg_attr(test, allow(dead_code))` для `#[path]`-включения (`sidecar_bitmap.rs:13, 40, 71`).
  - `cfg(miri)`: явное обнуление метаданных (`bootstrap.rs:179–191, 243, 291`, `reserve.rs:425–434`, `sidecar.rs:224–230`); `payload_virgin`/`is_fresh = false` (`reserve.rs:295`, `bootstrap.rs:410`, `alloc_core_large.rs:699`); пауза `TerminalPublicationGate` (`routing.rs:47–73`, `narrow.rs`). Production-семантику это не меняет, но Miri **никогда не исполняет** пути «OS-zero skip». Их корректность держится только на нативных тестах (`alloc_zeroed_virgin_small_skip`, `regression_virgin_bitmap_skip`, `r13_3_*`); это задокументировано (`miri.mjs:96–107`).
  - Инвариант virgin при чтении цел: писатели флага — только свежее резервирование и retain-decommit (очистка); `set_bump` откатывает курсор только на ветках decommit.
  - `cfg(loom)` в `src/` подменяет только ячейку чанка (`registry.rs:28–33`, `ensure.rs:15–19`) и в моделях не участвует, как и заявлено.
- **Pin-тесты:** `r11_ph4a_drop_ordering_pinned` (корректно парсит аргументы `cas_state`, а `cas_state` передаёт `Ordering` без изменений, `heap_slot.rs:250–258`) и `r11_ph4b_teardown_ordering_pinned` (вырезает комментарии) закрепляют то, что заявляют.
- **Верность теневых моделей текущему коду (по чтению):** `loom_terminal_large` ↔ `reservation_state.rs:42–101`, `loom_r8_maintenance_lease` ↔ `claim.rs:83–100, 416–432`, `loom_r11_registry_claim` ↔ `saturation.rs`/`claim.rs:214–215, 435–436`, `loom_sharded` ↔ `hand.rs:395–437`, `loom_r11_epoch_false_full` ↔ `epoch_region.rs:338–371, 585–596`. Упорядочения совпадают; не хватает только pin'ов.
- **Сканеры:** `verify-dbg-hook-safety.mjs` корректно наследует гейты `impl`/`mod` (`:414–446`), отвергает `any`/`not`/`cfg_attr` (`:449–452`) и требует `# Safety` у `unsafe`-хуков (`:435`). `verify-loom-target-wiring.mjs` покрывает все 13 корневых loom-целей (в CI они есть, `ci.yml:3147–3205`). Тяжёлых релизных `assert!` на горячих путях не найдено: `alloc_core_small_magazine.rs:143` и `terminal_words.rs:49` — по одному сравнению на холодных путях.
- **CI на базе:** Kani, все Miri-, Loom-, TSan- и ASan-job'ы, `test macos (production)` и cross aarch64 — success (приложение B).

## 6. Решения по открытым пунктам

Оба индекса прочитаны (§1). Для всех пунктов `docs/perf/OPEN_ITEMS.md` и `docs/CORRECTNESS_OPEN_ITEMS.md` — **LEAVE**: статусы, вердикты и триггеры не меняю. Это read-only ревью одной темы, а не исполнение их гейтов. Новые доказательства к пунктам 17, 18, 160, 167 — §2.2; решение о статусе — за владельцем и оркестратором раунда. Пункты 164 (P1-box) и 171 (Miri epoch) не переоткрываются. Предлагаемые новые карточки (если оркестратор примет находки): `[T]` для R17-VER-01…06, 08…11 в `TRACKED_verification_coverage.md` / `TRACKED_ci_gate_coverage.md`; R17-VER-07 — в `ACTIVE.md` (контракт публичного rustdoc).

## Приложение A. Инструменты и перепись (только чтение)

```text
find src -name '*.rs' | wc -l                                              -> 168
find src -name '*.rs' -exec cat {} + | wc -l                               -> 43325
find src -name '*.rs' -exec cat {} + | grep -cvE '^\s*$|^\s*//'            -> 17059
find src -name '*.rs' -exec cat {} + | grep -cE '^\s*//'                   -> 24303
find src -name '*.rs' -exec cat {} + | grep -cE '^\s*$'                    -> 1963
(группы: find src/<dir> ...)  alloc_core 88/24802, registry 46/10413, global 18/3809,
                              concurrent 14/3554, lib.rs+kani_proofs.rs 747
grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ | grep -c '#!\['          -> 21
grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ | grep -vc '#!\['         -> 83
grep -rlE '^\s*#!?\[allow\(unsafe_code\)\]' src/ | wc -l                   -> 51
grep -c 'kani::proof' src/kani_proofs.rs                                   -> 13
rg -c 'process::abort\(\)' src  (сумма)                                    -> 110 в 28 файлах
rg -c '^\s*debug_assert(_eq|_ne)?!' src (сумма)                            -> 67 в 27 файлах
grep -rnoE '\bpub\s+(const\s+)?(unsafe\s+)?fn\s+[A-Za-z0-9_]*(_for_tests?|_test)\b' src/ \
  | grep -v 'fn dbg_' | wc -l                                              -> 48 (из них unsafe: 1)
grep -oE ',(PASS|...|CONDITIONAL),' docs/evidence/registry.csv | sort | uniq -c
                                                                           -> PASS 127, FAIL 3, KNOWN-DEFECT 6,
                                                                              KNOWN-RED 3, MODEL-LIMIT 4, NOT_RUN 16,
                                                                              CFG_EXCLUDED 1, INCONCLUSIVE 8,
                                                                              CI_PENDING 3, CONDITIONAL 9
git log --format='%h %cI %s' -3 -- docs/evidence/registry.csv              -> последняя правка 11f2c0bc
git log --oneline 11f2c0bc..HEAD -- src/ | wc -l                           -> 17 (perf(runtime): 2)
git diff --shortstat 11f2c0bc HEAD -- src/                                 -> 91 files, +1336/-854
git diff --shortstat 6a0d47f6 HEAD -- src/                                 -> 22 files, +130/-122 (6 коммитов)
git log -S'ring_wrap_proofs' -- src/kani_proofs.rs                         -> 772b36d0 (добавлено), a4245965 (удалено)
git log -S'fn evict(' -- src/concurrent                                    -> f5565804 (добавлено), e0749303 (удалено)
git log --diff-filter=D -- tests/loom_fallback_init.rs                     -> 63991cc3
include_str!/ordering-pin файлы в tests/: r11_ph5b_c5_sidecar_ordering_pinned, r11_ph4a_drop_ordering_pinned,
  r11_ph4b_teardown_ordering_pinned, r11_ph4b_local_no_drop_pinned (+ doc-тесты). Ни один тест не закрепляет
  упорядочения в leaf_classes.rs, reservation_state.rs, hand.rs, saturation.rs, epoch_region.rs; hand.rs читают
  no_stale_doc_references.rs:2357–2410 и xxs_r5_02_epoch_generation_setter_gated.rs:110, но проверяют doc/bounds/гейтинг.
```

**Инциденты (раскрываю честно).**
1. Один временный список файлов был создан в системном временном каталоге вне репозитория и удалён сразу же, до каких-либо выводов.
2. Один фоновый `grep -rn '/\*' src/` получил через повторный глоббинг аргумента рантаймом MSYS (Git Bash на Windows) вместо шаблона список корня диска. Из-за этого он начал рекурсивно **читать** каталоги вне worktree. Записи не было. Процесс остановлен `taskkill` по своему PID, его вывод в отчёте не использовался; нужные проверки повторены инструментом поиска внутри worktree. При финальной проверке процессов тот же механизм сработал ещё раз в нерекурсивном `grep -c -F '/\*'`: прочитаны файлы верхнего уровня корня диска, команда завершилась сама. После этого запущенных `grep` нет (`wmic`), аргументы со `\*` больше не использовались.

Других файлов, кроме этого отчёта, не создавалось.

## Приложение B. CI-свидетельства (`gh`, только чтение)

| Run / job | SHA | Итог | Что извлечено |
|---|---|---|---|
| `37785720332` (CI) | `453439c1` | success | список job'ов: Miri core/alloc-core/fastbin/plain, loom (alloc-core alloc-xthread / alloc-global / experimental / misc), kani proofs, thread-sanitizer, address-sanitizer, evidence-registry, test macos (production), test (aarch64-unknown-linux-gnu) — все success; fuzz-run — skipped (не schedule) |
| job `113339817828` (kani proofs) | `453439c1` | success | «Complete - 13 successfully verified harnesses, 0 failures, 13 total» |
| job `113339817550` (evidence-registry) | `453439c1` | success | «VERDICT: INCOMPLETE (strict)», «PASS=127 FAIL=3 KNOWN-DEFECT=6 KNOWN-RED=3 MODEL-LIMIT=4 NOT_RUN=16 …» |
| job `113339818244` (test aarch64) | `453439c1` | success | `Image: ubuntu-24.04`; `cross-x86_64-unknown-linux-musl`; `ghcr.io/cross-rs/aarch64-unknown-linux-gnu:0.2.5` |
| job `113339817570` (test macos (production)) | `453439c1` | success | `Image: macos-26-arm64`; `stable-aarch64-apple-darwin`, rustc 1.99.0 |
| `37785720371` (Kani verification, `kani.yml`) | `453439c1` | success | дублирующий Kani-workflow |

## Приложение C. Witness и мутанты

Не запускалось: режим только чтение (приказ владельца). Временных тестов (`tests/zz_r17_verif_witness.rs`) и мутантов `src/` не было. Все контрфактуалы в §2 выведены чтением кода и помечены как «Выведено».
