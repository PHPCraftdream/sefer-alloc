# Solution research: verification-ver (R17-VER-01..04)

Режим: только чтение. Базовый SHA: `31db7f17698459e9ce26c3f8701c314304b9c00b` (проверено `git rev-parse HEAD`).

## §0 Чек-лист хода

- [x] Шаг 0: база = 31db7f17
- [x] Чтение индексов (items 17, 18, 160, 167, 171, 185–199) и отчётов R17
- [x] (1) VER-01 sidecar ordering — §1.1, §2.1, §3.1, §4.1, §5.1 записаны

### Карта владельцев (по индексам на базовом SHA)

| Пункт | Карточка-владелец | Где | Что карточка уже требует | Что НЕ покрыто карточкой |
|---|---|---|---|---|
| VER-01 | **188** OPEN | `docs/correctness-open-items/TRACKED_verification_coverage.md:237–242` | pin production-ориентаций `leaf_classes.rs` + production-only мутант | исправление заявления `tests/loom_sidecar_bitmap.rs` (позитивы не чувствительны к ordering, рекомендация VER-01 п.3) — без владельца; pin `SmallSidecar::bitmap → from_leaves` — упомянут только в отчёте |
| VER-02 | **189** OPEN | `TRACKED_ci_gate_coverage.md` (item 189) | protected-source scope + политика инвалидации + source-only мутант | дрейф номеров строк в тексте строк реестра (R6a-M5 `batch.rs:188`→`:189`) — частный случай, отдельного владельца нет |
| VER-03 | **190** OPEN (+ 17, 164, 171, 197 — раздельно) | `TRACKED_ci_gate_coverage.md` (item 190) | сверка цели/фичи/обёрток, классификация PASS vs KNOWN-RED, описания | машинный верификатор wiring (аналог `verify-loom-target-wiring.mjs`) явно не назван карточкой; переписать/удалить `regression_r2_06` — упомянут только как «stale description» |
| VER-04 | **18** RESOLVED (исторический) + **167** OPEN | `TRACKED_verification_coverage.md:144–221` | 167: вариант (б) — удалить зеркало и `pack_proofs`; 18: снапшот 13 harness уже записан (`:183–199`) | новые Kani-доказательства текущих протоколов (`terminal_words`, `reservation_state`, `sidecar_bitmap::granule`) — **без владельца**; рассогласование `kani.yml` ↔ `ci.yml` — упомянуто в 18 как наблюдение, без next trigger |

- [x] (2) VER-02 evidence registry — §1.2, §2.2, §3.2, §4.2, §5.2 записаны
- [x] (3) VER-03 miri targets — §1.3, §2.3, §3.3, §4.3, §5.3 записаны (новая находка: 2 локальные записи без `internals`)
- [x] (4) VER-04 kani — §1.4, §2.4, §3.4, §4.4, §5.4 записаны
- [x] §4–§7, порядок коммитов, новые находки

## §1 Постановка

### 1.1 VER-01 (item 188)

- Pin: `tests/r11_ph5b_c5_sidecar_ordering_pinned.rs:20–24` читает только `sidecar_bitmap.rs` и `bitmap_scan.rs`; `:53–60` ищет `.store(class + 1, Ordering::Release)` в первом `fn issue(` — это Dense-ветка `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs:91–93`.
- Dense строит только `from_initialized` (`sidecar_bitmap.rs:61–69`); вызывающие — исключительно `tests/r6_sidecar_bitmap.rs:42,99,128,177,233,253,276` (grep по `src/` и `tests/`). Production: `src/registry/segment_route/small_sidecar.rs:39–42` → всегда `from_leaves` → `ClassMap::Leaves`.
- Production-рёбра `src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs`: инициализация свежего System-листа неатомарной записью `:108–112` (`fresh.add(i).write(AtomicU8::new(old))`), Release-публикация `:113`; Acquire-чтения указателя `:67` (`prepared`), `:122` (`issue`), `:139` (`encoded`); Release-запись класса `:126`, `:131`; Acquire-чтения класса `:141`, `:145`. Ни одна из этих строк не читается ни одним `include_str!`-тестом (перечень `include_str!` в `tests/` — 10 файлов, ни один не ссылается на `leaf_classes.rs`).
- Loom: `tests/loom_sidecar_bitmap.rs:21–39` — shadow `Word`; в позитивах `:42–71`, `:73–99` issue/cut/class делает один главный поток, производители только `fetch_or`. `tests/loom_r11_small_sidecar.rs:63–100` — shadow `model_visibility` с Relaxed-контрфактуалом `:109–113`, без привязки к источнику.
- Особенность, важная для оракула: `tests/r6_sidecar_bitmap.rs:1–21` уже компилирует **настоящий** `src/alloc_core/segment/remote_bitmap/mod.rs` (а через `#[path]` в `sidecar_bitmap.rs:12–15` — и `leaf_classes.rs`) внутри интеграционного крейта, подставляя `crate::alloc_core::{os,size_classes}`. `ClassLeaves` переэкспортирован `pub(crate) use sidecar_bitmap::*` (`remote_bitmap/mod.rs:8–9`), т.е. доступен в таком тестовом крейте.

### 1.2 VER-02 (item 189)

- `scripts/verify-evidence-registry.mjs:341–375`: `rowTests` извлекает из `entry/transition/activation/mutant` только `tests/*.rs`, `tests::x`, `--test x` и голые идентификаторы-стемы. `:775–790` (правило `evidence-stale`): для strong-строки с source_sha-предком HEAD сравниваются только эти test-файлы. Пути `src/` не извлекаются и не сравниваются.
- Колонки реестра фиксированы точным заголовком `:37–42`; базовый ratchet сверяет заголовок по равенству (`:876–878`) — добавление колонки делает baseline на старом ref «invalid header» и пропускает ratchet (`:896–897`), CI вызывает без `--strict-baseline` (`.github/workflows/ci.yml:1394`).
- Колонка `source_tree` — свободный текст, не машинный scope (на базовом SHA 11 разных форм: `tree <sha>`, `git diff sha256 …`, «матрица на …», «—»).
- `docs/evidence/REGISTRY.md` генерируется `scripts/generate-evidence-registry-md.mjs` и проверяется `--check` (`ci.yml:1399`, `scripts/check-all.mjs:1128–1130`) — т.е. MD должен быть чистой функцией CSV.
- Самотест: `scripts/evidence-registry-selftest.mjs` (835 строк) уже строит временный git-репозиторий из двух коммитов (`:247–258`, `makeRatchetRepo`) — готовая площадка для оракула нового правила.
- Счёт на базовом SHA (разбор CSV без запуска скриптов проекта; `git log`): 180 строк, PASS = 127; у PASS 10 различных `source_sha`. `git log --oneline <sha>..HEAD -- src/ | wc -l`: `ceffecb5` 53, `876429d2` 32, `0c8d198a` 37, `a0dc988d` 35, `5083e4bc` 33, `80db02af` 32, `ba557ca4` 32, `fd954856` 32, `4382e23b` 48, `07a3652b` 0. Т.е. **124 из 127** PASS пинены к коммитам, после которых `src/` менялся ≥ 32 раз; 3 строки (`07a3652b`, R18-рефреш) имеют `07a3652b:src` = `HEAD:src` (`3900b01e…` оба). `git diff --shortstat fd954856 HEAD -- src/`: 106 файлов, +2298/−2268.
- Конкретный дрейф: R6a-M5 (`registry.csv:154`) — «batch.rs:188 filled += n». На базовом SHA `filled += n` стоит в `src/registry/heap_core/alloc/batch.rs:163` (R17 видел `:189`; с тех пор ещё `54fc2a21`, `f384eb0f`). `git log fd954856..HEAD -- …/batch.rs`: 4 коммита. Ни одна PASS-строка не содержит литерала `src/…/*.rs` (0 из 127) — scope из существующего текста строк **автоматически не извлекается**.

### 1.3 VER-03 (item 190; смежные 17, 164, 171, 197)

Точное состояние на базовом SHA (статически; Miri не запускался):

| Цель (`tests/…`) | CI (`.github/workflows/ci.yml`) | Локально (`scripts/miri.mjs`) | Расхождение |
|---|---|---|---|
| `region_invariants` | miri-core `:2838–2839`, без фич | `:46`, `experimental` | фичи |
| `miri_global_box_acceptance -- narrow` | SB `:2840–2847`, TB `:2848–2855` | `:126` SB; TB только с `--tree-borrows` | TB-ряд |
| `miri_global_box_acceptance -- paused` (P1-box, item 164) | 4× EXPECTED RED с сигнатурой и местом `:2856–2965` (alloc-global SB/TB, production SB/TB) | `:127`, ожидает `COMPLETE` (`:236–238`) | локально всегда FAIL; production-ряда нет |
| `narrow_domain_unchecked_storage` (`-p tagged-index-stack`) | `:2971–2972` | `:123` | — |
| `decommit_miri_cycle` | `:2976–2977` | `:62` | — |
| `segment_directory_a5_miri` | `:2986–2987`, `alloc-segment-directory internals` | `:118`, **без `internals`** | **новое:** файл требует `internals` (`tests/segment_directory_a5_miri.rs:39`), `alloc-segment-directory = ["alloc-core"]` (`Cargo.toml:670`) его не включает → локально 0 тестов → `ranSomething` (`miri.mjs:256`) → FAIL |
| `regression_large_align_no_segment_exhaustion`, `regression_page_aligned_no_segment_exhaustion`, `regression_realloc_cross_class_shrink`, `stress_boundary_sweep` | miri-alloc-core `:3015–3020` | `:63–65`, `:94` | — |
| `regression_magazine_oracles` | miri-fastbin `:3043–3046` | `:84–87` | — |
| `regression_bump_direct_refill` | miri-fastbin `:3043–3046` (с `internals`) | `:88–91`, **без `internals`** | **новое:** файл требует `internals` (`tests/regression_bump_direct_refill.rs:30`, гейт с `b1a9b7b0`, 2026-08-05) → локально 0 тестов → FAIL |
| `regression_xthread_large_free_no_leak`, `…thread_free_alias_miri`, `…small_ring_miri`, `r6_terminal_owner_drain --exact …` | miri-plain `:3063–3072` | PLAIN `:136–142` | фичи `alloc-decommit` у двух (несущественно) |
| `regression_r2_05_diag_provenance_miri` | нет | `:56` | только локально |
| `regression_realloc_oob_old_layout` | нет | `:74` | только локально |
| `regression_virgin_bitmap_skip` | нет | `:108` | только локально |
| `regression_w3_stats_aliasing_miri` | нет | `:115` | только локально |
| `r11_ph4a_lease_miri`, `r11_ph4b_lease_miri` | нет | нет | `#![cfg(all(miri, …))]` (`:15`, `:21`) — вне Miri компилируются в пустоту, т.е. **не исполняются нигде**; на них 6 PASS-строк (`registry.csv`: R4a-01, R4a-02, R4b-04, R4b-05, R4c-03, R4c-04) |
| `regression_r2_06_header_race_miri` | нет | нет | KNOWN-RED под SB (scaffold int→ptr, `registry.csv:63` R4c-05), PASS под PLAIN по тому же источнику; описание механизма устарело (R17-VER-03 п.4) |

Итого: из 10 файлов `tests/*_miri*.rs` в CI — 5, только локально — 2, нигде — 3. Позитивный `cfg(miri)` имеют только два lease-файла (grep `#!\[cfg([^]]*\bmiri\b` по `tests/`). Локальный раннер на базовом SHA, по чтению, даёт `overall: FAIL` по **трём** независимым причинам (paused, `segment_directory_a5_miri`, `regression_bump_direct_refill`), а не по одной, как записано в R17-VER-03 п.7 и item 190.

Прецеденты единого источника в проекте: `scripts/verify-loom-target-wiring.mjs` (28 строк: множество `tests/loom_*.rs` ⊆ CI `--test` ∩ `scripts/loom.mjs`; CI `:1458`); `scripts/check-matrix.mjs` `PER_PR_ROWS` + `scripts/run-check-matrix.mjs` (исполняется и локально, и в CI `:1459`) + `tests/ci_clippy_matrix_consistency.rs` (текстовая сверка вручную перечисленных CI-шагов с манифестом).

### 1.4 VER-04 (items 18, 167)

Инвентарь `src/kani_proofs.rs` (13 `#[kani::proof]`; `kani::cover!` — 0; `kani::unwind` — 0):

| Группа / гейт | Харнесс (`строка`) | Что доказывает | Реальный код или копия | Ценность для production |
|---|---|---|---|---|
| `node_proofs`, `cfg(all(kani, alloc-core))` `:25` | `write_read_next_roundtrip` `:33` | `Node::write_next`/`read_next` round-trip на локальном `[u8;16]` | реальный `crate::alloc_core::node::Node` | низкая: однострочные обёртки `ptr::read/write`, контракт вызывающего (границы, эксклюзивность, `'static`) не моделируется (`:6–10`) |
| 〃 | `deref_in_bounds` `:45`, `offset_in_bounds` `:57` | `deref/offset(base, k) == base.wrapping_add(k)` при `k < 64` | реальный | низкая |
| 〃 | `zero_fills_buffer` `:69`, `copy_nonoverlapping_copies` `:80` | заполнение/копирование конкретных 32/8 байт | реальный, конкретные (не символьные) длины | низкая |
| 〃 | `write_read_u8_roundtrip` `:92`, `write_read_u32_roundtrip` `:103`, `write_read_struct_roundtrip` `:122`, `write_read_usize_roundtrip` `:138` | round-trip символьных значений | реальный | низкая |
| `hand_proofs`, `cfg(all(kani, experimental))` `:154` | `vacant_starts_at_generation_zero` `:159`, `drop_value_vacant_is_noop` `:165` | два однопоточных факта `AtomicSlot` | реальный, но research-tier (`experimental`), не `production` | ~нулевая для shipping; комментарий `:148–153` «verified by loom (11 harnesses)» — loom-модели теневые (`tests/loom_epoch.rs:18–19`) |
| `pack_proofs`, `cfg(all(kani, alloc-global))` `:178` | `tagged_pack_unpack_roundtrip` `:193`, `tagged_unpack_is_clean_split` `:214` | round-trip `TaggedIndex<16>` | **крейт `tagged-index-stack`**, который production не исполняет (`src/registry/heap_registry/stack.rs:1`; зависимость только `cfg(any(loom, kani))`, item 167) | для sefer — нулевая; комментарий `:171–176` («ARE the regression tests for the `free_slots` packing») ложен |

Проводка: `.github/workflows/ci.yml:3076–3091` — job `kani`, `cargo kani --features "alloc-core experimental alloc-global"` (все 13); комментарий `:3077–3078` называет pack «registry pointer packing». `.github/workflows/kani.yml:3–6` заявляет «Kept APART from ci.yml … never gate the main CI gate», а `:37–40` гоняет node и hand повторно — дубль подмножества того, что уже гейтит `ci.yml`. CI-лог 13/13 — в R17-VER-04 (run `37785720332`), здесь не перепроверялся.

Протоколы текущего кода — кандидаты (чистая арифметика/последовательная семантика, CBMC-дружелюбно):
- `src/alloc_core/segment/segment_header/terminal_words.rs:31–85` — кодек `pack_large_state`/`large_generation`/`large_phase` (3 бита фазы, `MAX_LARGE_GENERATION = u64::MAX >> 3`), `next_large_generation_bounded`.
- `src/alloc_core/large/reservation_state.rs:19–167` — `LargeReservationState`: 9 переходов фазового слова через CAS (Live→Pending, Pending→Consuming, Live→Consuming, Consuming→Cached, Consuming→Released, Cached→Released, Initializing→Released, Cached→Initializing(g+1), Initializing→Live). Модуль под `alloc-core` (`src/alloc_core/large/mod.rs:27`). Покрытие сегодня: `tests/r6_terminal_large_state.rs:12–91` (конкретные значения, reduced-width), `tests/loom_terminal_large.rs:10–19` — shadow с переписанными константами фаз.
- `src/registry/bootstrap/saturation.rs:12–72` — `SaturationHint`: версия с битом насыщения, граница `LAST_VERSION = u64::MAX - 3`, поглощающее `DISABLED`.
- `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs:110–150` + `leaf_classes.rs:62–147` — индексная арифметика гранулы/слова/бита/листа на production-пути `from_leaves`; `encoded` (`:137–147`) не проверяет границы сам и полагается на вызывающего.
- `src/alloc_core/segment/segment_table/hash.rs:28–31` — `hash_index` (тривиально маскируется; низкая ценность).


## §2 Анализ

### 2.1 VER-01

НАБЛЮДЕНО:
- Единственное несущее межпоточное ребро для ClassLeaves — пара `:113` Release / `:139` Acquire: неатомарная инициализация `:108–112` должна быть happens-before чтения производителем `:145`. Остальные ordering-сайты либо owner-only (`:67`, `:70`, `:89`, `:122`, `:129` — их читает сам писатель), либо атомарные значения, чьё создание уже HB-упорядочено публикацией маршрута/пользовательской передачей указателя (`:126`/`:131` ↔ `:141`/`:145`: класс блока производителя переносится пользовательской передачей блока, `sidecar_bitmap.rs:79–82`).
- Dense-пин (`:53–60`) закрепляет ребро, которое для production не исполняется и, по разбору R17-VER-01, не несущее.

ВЫВЕДЕНО (не исполнялось):
- Мутанты, которые сегодня проходят все гейты: M1 `:113 Release→Relaxed`; M2 `:139 Acquire→Relaxed`; M3 **перестановка** `:113` перед циклом `:108–112` (ordering-токены не меняются — его не поймает никакой текстовый pin ordering-ов); M4 `small_sidecar.rs:40` переведён на Dense (pin останется зелёным и на Dense, а прод уйдёт из-под Leaves).
- Loom на **реальном** типе, по всей вероятности, к M1/M2 слеп: в production защищаемое значение — *инициализатор* атомика (`AtomicU8::new(old)`, записанный `ptr::write` в сырую память), а не позднейший store. У loom-атомика без последующих store нет «старого» значения, которое мог бы вернуть load, а сырые байты handle'а loom не инструментирует. Поэтому shadow `model_visibility` специально делает `new(0)` + `store(4)` (`loom_r11_small_sidecar.rs:73–79`). Это вывод по устройству loom, не по исполнению — помечен как риск в §6.
- Miri, напротив, ловит именно этот класс детерминированно при наблюдении указателя: data-race-детектор Miri основан на векторных часах и сообщает гонку «non-atomic write ↔ atomic load» независимо от того, какое значение прочитано. Достаточно, чтобы производитель увидел ненулевой `mixed[leaf]` без HB-ребра.
- Чтобы сделать проверку детерминированной, в тест не должно быть второго HB-канала: сигнал «spill сделан» передаётся **Relaxed**-флагом (он не синхронизирует), а `mixed_count()` (`:150–155`, Acquire) использовать нельзя — он сам синхронизирует и маскирует M2.

### 2.2 VER-02

НАБЛЮДЕНО: правило `evidence-stale` инвалидирует строку только при изменении цитируемых тестов; изменения `src/` для 124/127 PASS-строк не видны судье. Узкое уточнение R18-синтеза верно: строку всё ещё могут отклонить receipt/identity/schema-правила, т.е. «зелёный по построению» относится именно к оси «исходник изменился».

ВЫВЕДЕНО:
- Правильная семантика PASS — «истинно на `source_sha`», это не ложь. Нужен не пересмотр истории, а **классификация свежести**: исторический PASS ≠ текущий PASS. Поэтому RED уместен только там, где строка *объявила*, на каком коде она держится, и этот код изменился; для 124 legacy-строк — видимый счётчик, не RED (иначе CI красный сразу и это ретроактивная переоценка, против принципа «не retroactive»).
- Весь-`src/` как scope (дерево `src` в `source_sha` ≠ в HEAD) даёт ~100% ложных срабатываний: любой коммит в `src/` инвалидирует всё. Годится как *информационный* класс для legacy-строк, не как гейт.
- Scope нельзя вывести из импорта теста (`use sefer_alloc::…` даёт публичный API, а не файлы) и нельзя вывести из текста строк (0 литералов `src/`). Нужен явный список. Coverage-scope (llvm-cov) точен, но требует исполнения и инструмента — вне «чистого stdlib-судьи» (`verify-evidence-registry.mjs:7`).
- Номер строки в тексте мутанта (`batch.rs:188`) — отдельная хрупкость: место мутации надёжнее закреплять **якорем-текстом** (`filled += n` встречается ровно один раз в `batch.rs` на базовом SHA, `:163`), чем номером.
- Если вписать дрейф в `REGISTRY.md`, `--check` будет краснеть после каждого `src`-коммита (MD перестанет быть функцией CSV). Значит дрейф — только в выводе/JSON судьи.
- Добавление колонки в `registry.csv` ломает точный заголовок, baseline-парсер, генератор MD и фикстуры самотеста. Отдельный файл `docs/evidence/source_scope.csv` (по образцу уже существующих `spike_commits.csv`, `retired_ids.csv`) этого не задевает.

### 2.3 VER-03

НАБЛЮДЕНО: три разных несогласованных «источника» (CI YAML, `MATRIX`/`PLAIN_MATRIX` в `scripts/miri.mjs`, прозa README/ARCHITECTURE/реестра), без верификатора. Для Loom верификатор есть, для Miri — нет. Ожидаемо-красное (164) в CI закреплено по сигнатуре и месту, локально — не классифицировано вовсе.

ВЫВЕДЕНО:
- Локальный FAIL неинформативен: новый Miri-регресс неотличим от трёх известных причин. Два из трёх — не «ожидаемая краснота», а дефект раннера (потерянный `internals` после `b1a9b7b0`). Это та же «pass-by-absence»-ловушка, которую `ranSomething` (`miri.mjs:256`) поймал — но как FAIL, который все привыкли игнорировать из-за P1-box. Т.е. принятый P1 маскирует дрейф раннера. Серьёзность: **P4** (локальный инструмент, CI не затронут), уверенность — статическая (нужна проверка вывода «running 0 tests»).
- Два lease-файла с позитивным `cfg(miri)` — самый опасный класс: в обычном `cargo test` они не видны даже как «0 passed» с фичами, т.е. их выпадение не ловит ни один нативный гейт.
- Честная модель — **классы ожидания**, а не «всё зелёное»: GREEN (обязан пройти + маркер), EXPECTED-RED (обязан упасть с данной сигнатурой и местом; зелёный = «закройте карточку»), MANUAL/WAIVED (с причиной и ссылкой на карточку). Это ровно то, что CI уже делает для 164 вручную в bash.
- CI-шаги разумно оставить «ручными» (по шагу на цель — читаемость в Actions UI, как и решено для clippy в `tests/ci_clippy_matrix_consistency.rs:14–18`), но сверять их с манифестом. Перенос bash-оракула 164 в JS — риск ослабить его при порте (тот же довод, что item 84 приводит против спешного порта guard'ов), поэтому сигнатура/место хранятся в манифесте как строки-регэкспы, а верификатор проверяет, что CI содержит **те же** строки.

### 2.4 VER-04

НАБЛЮДЕНО: 13 харнессов; 11 из 13 касаются production-кода только через однострочные обёртки `Node`, 2 — research-tier, 2 — непоставляемый крейт. Ни одного `kani::cover!`; несущие протоколы Large-фаз и saturation-версии Kani не покрыты, хотя это ровно класс «исчерпывающе по всем входам, без конкурентности, без контракта вызывающего», который файл сам называет сильной стороной Kani (`:19–21`).

ВЫВЕДЕНО:
- Самый ценный объект — **таблица переходов `LargeReservationState` над реальным кодеком**: ABA-защита Large-реюза держится на том, что (а) кодек инъективен по (фаза, поколение), (б) только `begin_reuse` меняет поколение и только на +1, (в) поколение не переполняется (иначе `pack_large_state` паникует `assert!` `:49` — в аллокаторе это abort-путь), (г) из `Released`/`Unused` нет выхода. Kani моделирует атомики последовательно — для однопоточного свойства «результат каждого метода из любого слова» этого достаточно; конкурентность покрывает loom-shadow, а Kani тогда **связывает** shadow с реальным кодом по константам фаз.
- Второе — `SaturationHint`: ложное «насыщено» после публикации свободного слота = отказ claim'ить доступный слот. Свойство «устаревший снимок не может пометить насыщение после ≥1 `publish_claimable`» и «версия строго растёт до поглощающего `DISABLED` без переполнения» — граничная арифметика у `u64::MAX - 3`, где юнит-тесты (`dbg_with_word_for_test`, `:27–31`) проверяют точки, а Kani — все слова.
- Третье — индексы sidecar на production-пути `from_leaves`: доказывает отсутствие OOB-паники `encoded`/`publish` для всех `u32` offset. Дороже (массивы 4096 `AtomicU64` + 2048 атомиков листа; при spill — цикл 256), поэтому — кандидат на тяжёлый non-gating job.
- `pack_proofs` после этого заменяются, а не дублируются (item 167, вариант (б) — для Kani-части; loom-зеркало `loom_shim.rs` остаётся за 167).

## §3 Варианты

### 3.1 VER-01

| # | Вариант | Механизм | Цена | Ловит | Не ловит |
|---|---|---|---|---|---|
| A | Оставить + документировать | исправить doc `r11_ph5b_c5` («pins Dense test backing only») и `loom_sidecar_bitmap.rs:1` | минуты | ничего нового | M1–M4 |
| B | Текстовый pin, «contains» как сейчас | `include_str!` `leaf_classes.rs`, `contains(".store(fresh, Ordering::Release)")` в `fn prepare(` и `Acquire` в `fn encoded(` | ~40 строк, каждый `cargo test` | M1, M2 | M3; второй сайт с тем же текстом; мутации в `#[cfg]`-выключенном коде засчитываются |
| C | Текстовый pin, **полная инвентаризация** ordering-ов | для каждой не-`cfg(internals)` функции `ClassLeaves` извлечь упорядоченный список `(op, Ordering::X)` и сравнить с ожидаемым **точно** (по образцу точных счётчиков `crates/aligned-vmem/tests/granted_huge_reader_enumeration.rs`); плюс pin `small_sidecar.rs: fn bitmap → from_leaves` и «у `from_initialized(` нет вызовов в `src/`» | ~120 строк, каждый `cargo test` | M1, M2, M4, любые добавленные/удалённые сайты (заставляют ревью) | M3 (перестановка), семантику |
| D | Loom-shadow, параметризованный ordering-ом, **извлечённым из исходника** | тест под `cfg(loom)` парсит ordering на `:113`/`:139` из `include_str!` и подаёт в `model_visibility(order)`; негатив `Relaxed` уже есть | ~60 строк; только loom-джоб | M1, M2 и доказывает, что пиненное ребро несущее | M3, M4; shadow ≠ код |
| E | Loom на реальном `ClassLeaves` через фасад атомиков | заменить `use core::sync::atomic::{AtomicPtr,AtomicU8,Ordering}` в `leaf_classes.rs:9` на крейт-относительный фасад, подставляемый в тестовом крейте `loom::sync::atomic` | правка `src/`, 2048+ loom-атомиков на исполнение | вероятно, НЕ ловит M1/M2 (см. §2.1); ловит логические ошибки протокола | M1/M2 (вероятно), M3 |
| F | **Miri race-оракул на реальном `ClassLeaves`** (новая цель `*_miri`) | `#[path]`-включение как в `r6_sidecar_bitmap.rs`; владелец: `prepare` (spill), затем Relaxed-флаг; производитель: ждёт флаг Relaxed-ом, затем K раз `encoded(g1)` | ~90 строк + строка в источнике Miri-целей (см. §4.3); секунды под Miri (оценка: 2048 атомиков инициализации + 256 листа) | M1, M2, **M3** (неатомарная запись после публикации → гонка) — без правки `src/` | M4; ordering-и на owner-only сайтах (они не несущие); зависит от того, увидит ли производитель указатель (вероятностно при Miri-weak-memory, см. §6) |

### 3.2 VER-02

| # | Вариант | Механизм | Цена | Ловит | Не ловит / ложные срабатывания |
|---|---|---|---|---|---|
| A | Оставить + документировать | вывести в лог судьи счётчик «PASS-строк с src-коммитами после source_sha» | ~15 строк | видимость возраста | ничего не блокирует; source-only мутант не отклоняется (не выполняет acceptance item 189) |
| B | Весь `src/` как scope | `git diff --quiet <sha> HEAD -- src/ Cargo.toml build.rs` → violation | ~20 строк | любой src-дрейф | 124/127 RED сразу; ложные срабатывания на каждый несвязанный коммит; ретроактивно |
| C | Хэш дерева scope в строке | новая колонка `scope_tree` = `git rev-parse <sha>:<path>` / хэш `ls-tree` | схема CSV, генератор, baseline, фикстуры | как D | избыточно: `source_sha` уже неизменяемый коммит; полезно лишь для измерений на незакоммиченном дереве |
| D | **Явный scope в отдельном файле + правило `evidence-src-stale`** | `docs/evidence/source_scope.csv` (`id,paths,scope_base,anchors,note`); для strong-строки со scope: `git diff --name-only <scope_base‖source_sha> HEAD -- <paths>` непусто → violation; `anchors` — текст места мутанта, должен встречаться в HEAD-версии файла ровно 1 раз | ~80 строк судьи + ~6 кейсов самотеста; один `git diff` на (sha, paths) с кэшем | source-only мутант в объявленном scope; исчезновение/дублирование места мутанта | src-изменение вне объявленного scope (недоописанный scope); семантический дрейф через непрямую зависимость |
| E | Coverage-scope | llvm-cov по тестам строки → файлы | исполнение, тулинг, нестабильность по фичам | точнее D | ломает «pure-stdlib, no cargo» судьи; не для per-PR |
| F | Привязка к «revalidated_sha» + `git log` | строка хранит последний коммит перепроверки; судья требует пусто в `git log <revalidated>..HEAD -- <paths>` | как D + колонка | как D | эквивалент D (D использует `source_sha`, т.е. то же самое без новой колонки) |

### 3.3 VER-03

| # | Вариант | Механизм | Цена | Ловит | Не ловит |
|---|---|---|---|---|---|
| A | Оставить + документировать | поправить README/ARCHITECTURE/шапку `miri.mjs`, понизить 6 lease-PASS до CONDITIONAL | правка доков | ложные заявления | будущий дрейф; три нигде-не-исполняемые цели остаются |
| B | Верификатор по образцу loom | `scripts/verify-miri-target-wiring.mjs`: каждый `tests/*_miri*.rs` и каждый файл с позитивным `cfg(miri)` ∈ CI ∩ `miri.mjs`, или в WAIVERS с причиной | ~50 строк + CI-строка | неподключённые цели | рассогласование фич/моделей/ожиданий; локальный FAIL по 164 |
| C | CI исполняет локальный раннер (`node scripts/miri.mjs --job X`) | манифест = единственный исполняемый источник, как `run-check-matrix.mjs` | перенос 4 EXPECTED-RED bash-оракулов в JS; потеря «шаг = цель» в Actions UI | всё дрейфовое по построению | риск ослабить P1-оракул при порте |
| D | **Манифест + классы ожидания + сверка с CI (гибрид)** | данные в `scripts/miri-matrix.mjs` (`{job, test, pkg, features, model: strict|plain, borrows: sb|tb, args, expect: green|expected-red|waived, marker, redMessage, redSite, card}`); `scripts/miri.mjs` читает его и умеет EXPECTED-RED; `scripts/verify-miri-target-wiring.mjs` проверяет (1) инвентарь файлов, (2) что каждая green/expected-red-запись есть в CI с теми же фичами/моделью/`--test`, (3) что строки `redMessage`/`redSite`/маркер буквально присутствуют в соответствующем CI-шаге, (4) что `#![cfg(...)]` файла выполним при фичах записи (переиспользовать `evalCfg` из `scripts/verify-evidence-registry.mjs:245–300` либо его копию) | ~150 строк скриптов + правка `miri.mjs` + 1 CI-шаг | неподключённые цели, потерянные фичи (поймал бы оба новых случая), рассогласование моделей, локальный 164 больше не маскирует регрессы | исполнение под Miri (верификатор статичен); корректность самих сигнатур |
| E | Отдельный ночной полный Miri-job | `schedule` + `workflow_dispatch`, все записи манифеста, включая MANUAL | CI-минуты | долгие цели | дрейф между прогонами |

### 3.4 VER-04

| # | Вариант | Механизм | Цена | Ловит | Не ловит |
|---|---|---|---|---|---|
| A | Оставить + документировать | исправить `kani_proofs.rs:148–153,171–181`, `ci.yml:3077–3080`, `kani.yml:3–6`; item 18 уже несёт снапшот 13 | правка комментариев | ложные заявления | ни одного production-мутанта сверх `Node` |
| B | Вариант (б) item 167 только удалением | удалить `pack_proofs` | −46 строк | честность | ничего нового не доказывает |
| C | **K1 + K2 (gating) вместо `pack_proofs`** | K1: кодек Large-фазы + отношение переходов реального `LargeReservationState`; K2: версионный протокол `SaturationHint` | ~120 строк в `src/kani_proofs.rs`; оценка времени — секунды–минуты (64-битные символьные слова, без циклов кроме `fetch_update`); K2 требует `internals` в фичах Kani-job'а | мутанты фаз/поколений/границы `LAST_VERSION` в production-коде | конкурентность (loom), память (Miri) |
| D | K3 — индексы sidecar на `from_leaves` | символьный `u32` offset через `SidecarBitmap::publish`/`issue` над реальным `ClassLeaves` | новый `unsafe` в `kani_proofs.rs` (обнулённый/инициализированный `ClassLeaves`) → tier-2 запись в инвентарь `unsafe`; массивы 4096+2048 атомиков; `unwind` ≥ 257 — оценка: тяжело | OOB-индекс/переполнение сдвига при правке геометрии | ordering; содержимое после гонок |
| E | Kani для `hash_index`/`Node`-контрактов | — | низкая ценность / нужен контракт вызывающего | мало | — |

## §4 Рекомендация

### 4.1 VER-01

**C + F, плюс A для shadow-тестов.** C — дешёвый детерминированный гейт в каждом `cargo test`, закрывает M1/M2/M4 и любые новые сайты. F — семантический оракул на реальном коде без правки `src/`, закрывает перестановку M3, которую ни один текстовый pin не видит. D не нужен при наличии F (F проверяет реальный тип, D — копию). E отвергнут: правка `src/` ради модели, которая, по разбору, слепа к главному мутанту. Шаг A обязателен в любом случае: `tests/loom_sidecar_bitmap.rs:1` заявляет «ordering model of SidecarBitmap's exact atomic operations», а позитивы ordering-нечувствительны (R17-VER-01 п.3) — это без владельца в item 188.

### 4.2 VER-02

**D + A как классификатор legacy-строк.** Политика (три класса, не ретроактивно):
1. **scoped** (id есть в `source_scope.csv`): дрейф в scope → violation `evidence-src-stale` (RED в `--strict`). Выход из RED — в том же коммите либо переснять доказательство (новый `source_sha` + receipt), либо явно понизить строку до `CONDITIONAL` с `status_reason` «src drift since <sha>: <файлы>». Понижение ratchet не запрещает (`:939–942` проверяет только повышение до PASS).
2. **legacy-unscoped** (id нет в файле, строка была в baseline): не RED; судья печатает `SRC-DRIFT-UNSCOPED: <id> <n> commits` (по всему `src/ Cargo.toml build.rs`) и общий счётчик в JSON (`counters.srcDriftUnscoped`). На базовом SHA это 124 строки — честная видимость без переоценки истории.
3. **новые/с новой evidence** strong-строки (нет в baseline или `newEvidence(base,cur)` по `:863–869`): обязаны иметь scope → violation `src-scope-required`. Это и есть «впредь», как R14-10/R29-6.

Почему не RED на legacy: (а) их PASS — исторически правдивые утверждения на `source_sha`; (б) RED на 124 строках = ретроактивное правило; (в) класс 3 гарантирует, что доля legacy только убывает по мере перепроверок.

### 4.3 VER-03

**D**, с явными классами. Распределение целей:
- GREEN, CI: все нынешние CI-цели; плюс `r11_ph4a_lease_miri`, `r11_ph4b_lease_miri` (strict, `alloc-global alloc-xthread internals` по их шапкам `:13`, `:19`; в miri-core); плюс `regression_r2_05_diag_provenance_miri` и `regression_w3_stats_aliasing_miri` (по своим шапкам — малые); плюс новая цель VER-01 `class_leaves_publication_miri` (strict, `alloc-core`).
- GREEN, plain: `regression_r2_06_header_race_miri` — **только** после переписывания шапки под текущий протокол (иначе — WAIVED/удаление, вопрос владельцу §6); строка реестра R4c-05 (KNOWN-RED под SB) остаётся исторической, новая проверка — в PLAIN, где источник фиксирует PASS.
- EXPECTED-RED (card 164): 4 записи `paused` (alloc-global/production × SB/TB) с `redMessage`/`redSite`, дословно взятыми из CI `:2876–2879`, `:2903–2906`, `:2931–2934`, `:2959–2962`. Локальный `miri.mjs` трактует их так же: зелёный → FAIL «закройте карточку», иной UB/место → FAIL «новый дефект», ожидаемый → OK.
- LOCAL-ONLY/WAIVED с причиной: `regression_realloc_oob_old_layout`, `regression_virgin_bitmap_skip` — до замера времени (§6); `regression_own_segment_cache_invalidation` уже обоснованно вне Miri (`ci.yml:3006–3014`).
- Фичи локальных записей приводятся к CI (исправляет два «всегда FAIL»).
- Смежное, не дублировать: item 197 (hardened+batch) после этого — одна запись манифеста, но остаётся своей карточкой; 171 (crossbeam-epoch) — запись `region`/`epoch` не добавлять без решения 171; 17 — proof-класс, не wiring.

### 4.4 VER-04

**C (K1 + K2) в gating-job `ci.yml`, `pack_proofs` удалить (Kani-часть варианта (б) item 167), A — комментарии, `kani.yml` — убрать дубль.** D (K3) — вторая очередь, только после замера времени и решения владельца о новом tier-2 `unsafe`-сайте; если делать, то в `kani.yml` как non-gating (это дало бы `kani.yml` настоящую причину существовать отдельно). `hand_proofs` и `node_proofs` оставить (дёшево), но переписать заявление о loom.

Что уже покрыто карточками и не дублируется: 18 (исторический RESOLVED + снапшот 13) — только обновить снапшот после замены; 167 — удаление зеркала `loom_shim.rs` остаётся за ней. **Без владельца:** новые доказательства K1/K2/K3 и рассогласование `kani.yml`↔`ci.yml` — требуют новой карточки `[T]` в `TRACKED_verification_coverage.md` (или расширения 167, если владелец предпочтёт не плодить номера).

## §5 План реализации

### 5.1 VER-01

Коммит 1 — `test(sidecar): pin production ClassLeaves orderings` (префикс `test`; рантайм не меняется):
- Файл: `tests/r11_ph5b_c5_sidecar_ordering_pinned.rs` (расширить; новый файл не нужен — один экспорт = один тестовый крейт этой темы).
- Эскиз:

```text
fn leaves_src() -> String { src(include_str!("../src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs")) }
// ordering-инвентарь функции: все вхождения `.load(`/`.store(`/`.swap(`/`.fetch_` с их Ordering, по порядку
fn orderings(body: &str) -> Vec<(&str /*op*/, &str /*Ordering::X*/)>
#[test] fn class_leaves_ordering_inventory_is_exact() {
  let t = leaves_src();
  assert_eq!(orderings(&fn_body(&t, "prepared")), [("load","Acquire"),("load","Acquire")]);
  assert_eq!(orderings(&fn_body(&t, "prepare")),  [("load","Acquire"),("store","Release")]); // :89, :113
  assert_eq!(orderings(&fn_body(&t, "issue")),    [("load","Acquire"),("store","Release"),("load","Acquire"),("store","Release")]);
  assert_eq!(orderings(&fn_body(&t, "encoded")),  [("load","Acquire"),("load","Acquire"),("load","Acquire")]);
}
#[test] fn mixed_leaf_published_after_its_initialization() {
  // текстовый порядок внутри prepare: цикл `.write(AtomicU8::new(old))` строго до `.store(fresh, Ordering::Release)`
}
#[test] fn production_small_sidecar_uses_class_leaves() {
  // small_sidecar.rs: fn bitmap содержит `SidecarBitmap::from_leaves(`; ни один файл src/ кроме sidecar_bitmap.rs не содержит `from_initialized(`
}
```
  Нюанс `fn_body`: поиск `fn prepare(` не совпадает с `fn prepared(` (разные строки с `(`), конец — первая `"\n    }\n"`, у методов `impl ClassLeaves` отступ 4 пробела — подходит. Второй тест ловит M3 текстово только для «store до цикла»; полноценно M3 ловит коммит 2.
- Также в этом коммите: шапка `:1–9` («pins Dense backing for the test-only constructor, and the production ClassLeaves sites») и `tests/loom_sidecar_bitmap.rs:1` → «shadow; positive cases are ordering-insensitive (single issuer/cutter thread); see r11_ph5b_c5 and the ClassLeaves Miri oracle».
- Оракул красный-до/зелёный-после: на базовом SHA новые тесты сначала пишутся и запускаются против мутантов M1 (`:113`→Relaxed), M2 (`:139`→Relaxed), M4 (`small_sidecar.rs:40` → `from_initialized`-путь — достаточно текстовой подмены вызова в рабочей копии, компиляция не нужна для текстового теста, но тест-крейт компилируется) — каждый обязан покраснеть; затем откат — зелёные. Не-вакуумность старого Dense-пина сохраняется.

Коммит 2 — `test(miri): real ClassLeaves publication race oracle`:
- Новый файл `tests/class_leaves_publication_miri.rs`, `#![cfg(feature = "alloc-core")]`, тело — как `tests/r6_sidecar_bitmap.rs:1–21` (shim `mod alloc_core { os, size_classes }` + `#[path = "../src/alloc_core/segment/remote_bitmap/mod.rs"] mod remote_bitmap;`).
- Эскиз теста:

```text
let raw = Box::<MaybeUninit<ClassLeaves>>::new_uninit();  // unsafe initialize_at(raw.as_mut_ptr()), assume_init
let leaves: Arc<ClassLeaves> = ...;
assert!(leaves.issue(g1, C1));                 // uniform[leaf] = C1+1, до spawn → HB через spawn
let spilled = Arc<AtomicBool>(false);
producer = spawn: while !spilled.load(Relaxed) { yield }; for _ in 0..K { assert_eq!(leaves.encoded(g1), C1 + 1) }
owner:   assert!(leaves.prepare(g2, C2)); spilled.store(true, Relaxed);  // g2 в том же листе, C2 != C1 → spill
join
```
  Единственный HB-канал от записи `:111` к чтению `:145` — пара `:113`/`:139`. При M1, M2 или M3 первое же наблюдение ненулевого указателя даёт Miri «Data race detected … non-atomic write … atomic load». Без мутанта — зелёный, и `encoded(g1)` всегда `C1+1` (копия `old`).
- Негативный контроль: отдельного «ожидаемо красного» теста не добавлять (Miri-ошибка не перехватывается `should_panic`); контрфактуал фиксируется в теле коммита как ручной прогон M1/M2/M3 → три красных лога, откат → зелёный.
- Проводка: имя цели добавить в единый источник Miri-целей (§5.3), класс GREEN, фичи `alloc-core`. Конструирование: `Box::new(MaybeUninit::<ClassLeaves>::uninit())` (около 9 КиБ на 64-бит, оценка по `leaf_classes.rs:37–40`), затем `unsafe { ClassLeaves::initialize_at(p) }` — контракт `:43–45` соблюдён (приватная, выровненная, без ссылок до возврата).

### 5.2 VER-02

Коммит 1 — `build(scripts): source-scope invalidation for evidence registry rows` (только скрипты/данные гейта; `scripts/verify-commit-prefixes.mjs` проверяет, что `build(...)` не трогает `src/`):
- Новый `docs/evidence/source_scope.csv`, заголовок `id,paths,scope_base,anchors,note`; `paths` — `;`-список путей/директорий (git pathspec, без глобов `**`, чтобы не зависеть от реализации); `scope_base` — пусто (= `source_sha`) или коммит, на котором дерево scope совпадает с измеренным (для строк, измеренных на «base + незакоммиченный diff», который позже влит: иначе ложное срабатывание от собственного влитого diff'а); `anchors` — `;`-список `path::текст`, текст обязан встречаться в HEAD-файле ровно 1 раз.
- `scripts/verify-evidence-registry.mjs`: рядом с `isListedSpike` (`:322–337`) — загрузка `source_scope.csv` (по тому же `parseCsv`); в ветке `:775–791` после test-staleness:

```text
const sc = scopeById.get(id);
if (sc) {
  const base = sc.scope_base || idn.resolved;
  const changed = git(['diff', '--name-only', base, 'HEAD', '--', ...sc.paths]);   // кэш по `${base}|${paths}`
  if (changed) violation('evidence-src-stale', id, `src scope changed since ${base}: ${changed.split('\n').join(', ')}`);
  for (const [p, text] of sc.anchors) if (count(readHead(p), text) !== 1) violation('evidence-anchor', id, `${p}: anchor "${text}" occurs ${n} times`);
} else if (baselineHas(id) && !newEvidenceVsBaseline(id)) {
  if (git(['diff','--quiet', idn.resolved, 'HEAD','--','src','Cargo.toml','build.rs']) === null) srcDriftUnscoped.push(id);   // info only
} else violation('src-scope-required', id, 'new/refreshed strong row needs a docs/evidence/source_scope.csv entry');
```
  Нюанс порядка: baseline грузится **после** цикла строк (`:894–945`), поэтому в цикле копить «unscoped strong ids», а развилку класс 2 / класс 3 (`src-scope-required`) делать в ratchet-блоке рядом с `:930–943`, где уже есть `b.map` и `newEvidence`. Нюанс: `git(...)` возвращает `null` при ненулевом коде (`:100–107`), а `git diff --quiet` с изменениями выходит с кодом 1 — это и есть сигнал «изменено». Без `--baseline` (локальный запуск без ref) класс 3 неопределим — тогда только классы 1/2 (как уже сделано для ratchet: `scripts/check-all.mjs:308` печатает предупреждение).
- Вывод: `[evidence-registry] src drift (unscoped legacy): N rows` + JSON `counters.srcDriftUnscoped`. В `REGISTRY.md` ничего не добавлять (см. §2.2).
- `scripts/evidence-registry-selftest.mjs`: в `makeRatchetRepo`-фикстуре (`:247–258`) добавить `src/a.rs`, `src/b.rs`; кейсы: (1) scope `src/a.rs`, коммит2 меняет `src/b.rs` → GREEN (несвязанная правка); (2) коммит2 меняет `src/a.rs` → RED `evidence-src-stale`; (3) та же строка понижена до CONDITIONAL → не `evidence-src-stale`; (4) строка без scope присутствует в baseline, `src/a.rs` изменён → не RED, `srcDriftUnscoped` = 1; (5) новая PASS-строка без scope → RED `src-scope-required`; (6) якорь встречается 0 и 2 раза → RED `evidence-anchor`.
- Оракул красный-до/зелёный-после: кейсы (2), (5), (6) должны падать на текущем судье (правил нет → вердикт не RED → самотест падает), проходить после. Реальный контрфактуал: добавить в `source_scope.csv` R6a-M5 → `paths=src/registry/heap_core/alloc/batch.rs; anchors=src/registry/heap_core/alloc/batch.rs::filled += n` → RED (4 коммита после `fd954856`, §1.2). Это доказывает, что правило видит реальный source-only дрейф. Решение по R6a-M5 — переснять мутант или понизить до CONDITIONAL — отдельный коммит `docs(evidence)`.

Коммит 2 — `docs(evidence): scope the R18-refreshed rows` — первые scoped-строки: три PASS на `07a3652b` (дерево `src` = HEAD, значит GREEN сразу и не требует перепрогонов); дальше scope добавляется при каждой перепроверке.

### 5.3 VER-03

Коммит 1 — `build(scripts): single Miri target manifest with expectation classes` (префикс `build(scripts)` есть в истории):
- Новый `scripts/miri-matrix.mjs` (данные, один экспорт `MIRI_TARGETS`), эскиз записи:

```text
{ job: 'miri-core', test: 'miri_global_box_acceptance', features: 'production internals bench-internals',
  model: 'strict', borrows: 'tb', args: ['--', 'paused'], expect: 'expected-red', card: 164,
  marker: '[miri_global_box_acceptance] COMPLETE paused_terminal_owner_retirement',   // присутствие = FAIL
  redMessage: 'Undefined Behavior: write access through .* is forbidden',
  redSite: { frame0: 'sefer_alloc::alloc_core::platform::node::Node::write_next', file: 'node\\.rs:', caller: 'reclaim_sidecar_record|flush_run|dealloc_small' } }
```
- `scripts/miri.mjs`: импортирует `MIRI_TARGETS` вместо `MATRIX`/`PLAIN_MATRIX` (`:45–143`); MIRIFLAGS выводятся из `model`/`borrows` (`strict` = `-Zmiri-strict-provenance -Zmiri-disable-isolation`, `plain` = `-Zmiri-disable-isolation -Zmiri-preemption-rate=0.5`, как CI `:2830`, `:3055`); `--job <name>` фильтр; вердикт `OK/EXPECTED-RED-OK/FAIL` по классу. Шапку `:2–3` заменить на «манифест — `scripts/miri-matrix.mjs`; соответствие CI проверяет `verify-miri-target-wiring.mjs`».
- Новый `scripts/verify-miri-target-wiring.mjs` (по образцу `scripts/verify-loom-target-wiring.mjs`): (1) инвентарь `tests/*_miri*.rs` ∪ файлы с позитивным `cfg(miri)` ⊆ манифест; (2) каждая `green|expected-red`-запись → в CI без комментариев есть строка `cargo miri test` с тем же `--test`, теми же фичами (как множество токенов) и в job'е с совместимым `MIRIFLAGS`; (3) для `expected-red` — `redMessage`, `frame0`, `caller`, `marker` буквально в том же CI-шаге; (4) `#![cfg]` файла выполним при фичах записи (переиспользовать логику `evalCfg`, `scripts/verify-evidence-registry.mjs:245–300`; если брать копией — пометить источник). Подключить в CI рядом с `:1458` и в `scripts/check-all.mjs` рядом с loom-верификатором.
- Оракул красный-до/зелёный-после: верификатор, написанный первым и запущенный на базовом SHA до правки манифеста/CI, обязан покраснеть с ровно этими причинами: 3 файла без проводки (lease×2, r2_06), 4 только-локальные, 2 записи с невыполнимым `cfg` (`segment_directory_a5_miri`, `regression_bump_direct_refill`). Контрфактуалы после озеленения: удалить `internals` из одной записи → RED по (4); удалить одну `--test` строку из CI → RED по (2); изменить `redSite.caller` → RED по (3).

Коммит 2 — `ci(miri): wire lease, diag-provenance and stats-aliasing targets` (`ci`/`fix(ci)` уже используются в истории: 4 и 11 из последних 400 субъектов; в R30-12 это не perf-префикс, рантайм не меняется): добавить в CI шаги для целей, ставших GREEN; переписать шапку `tests/regression_r2_06_header_race_miri.rs:1–35` (если владелец выбрал «оставить»). Исполнение нового CI — обязательный receipt (лог Actions) перед закрытием item 190.

Коммит 3 — `docs(miri): reconcile Miri inventory prose` — README `:1388`, `docs/ARCHITECTURE.md:492` (`r8_global_box_provenance` → `miri_global_box_acceptance`), шапки `regression_xthread_small_ring_miri.rs:7–35`, `regression_xthread_thread_free_alias_miri.rs:1–40`, `decommit_miri_cycle.rs:1–11` (R17-VER-03 пп.5–6). Строки реестра R4a-01/02, R4b-04/05, R4c-03/04 не трогать задним числом: их PASS исторически пинен; после первого CI-прогона — новая строка/перепин с scope (§5.2).

### 5.4 VER-04

Коммит 1 — `test(kani): prove Large phase codec and transition relation` (`test`: только `cfg(kani)`-код, рантайм не меняется; `src/kani_proofs.rs` — санкционированное исключение CLAUDE.md п.4). В `src/kani_proofs.rs` новый модуль `#[cfg(all(kani, feature = "alloc-core"))] mod large_state_proofs` (пути: `crate::alloc_core::segment_header::{pack_large_state, large_phase, large_generation, next_large_generation_bounded, LargePhase, MAX_LARGE_GENERATION}`, `crate::alloc_core::LargeReservationState` — pub-реэкспорт `src/alloc_core/mod.rs:47`):

```text
#[kani::proof] fn large_codec_roundtrip_and_canonical() {
  let g = any(); assume(g <= MAX_LARGE_GENERATION); let code: u64 = any(); assume(code <= 6);
  let p = large_phase(code).unwrap(); let w = pack_large_state(p, g);
  assert!(large_phase(w) == Some(p) && large_generation(w) == g);
  let x: u64 = any(); if let Some(q) = large_phase(x) { assert_eq!(pack_large_state(q, large_generation(x)), x) }  // декодирование каноничное ⇒ кодек инъективен
  cover!(large_phase(x).is_none());
}
#[kani::proof] fn next_generation_never_exceeds_limit() {
  let (g, limit) = (any(), any());
  match next_large_generation_bounded(g, limit) { Some(n) => assert!(n == g + 1 && n <= limit && limit <= MAX_LARGE_GENERATION),
                                                  None => assert!(g >= limit || limit > MAX_LARGE_GENERATION) }
  cover!(next_large_generation_bounded(g, limit).is_some());
}
#[kani::proof] fn large_reservation_state_transition_relation() {
  let w0: u64 = any(); let word = AtomicU64::new(w0); let s = LargeReservationState::new(&word);
  let g: u64 = any(); assume(g <= MAX_LARGE_GENERATION);
  let ok = match any::<u8>() % 9 { 0 => s.publish_pending(g), 1 => s.claim_pending().is_some(), 2 => s.claim_live().is_some(),
     3 => s.cache_consumed(g), 4 => s.release_consumed(g), 5 => s.release_cached(g), 6 => s.release_initializing(g),
     7 => s.begin_reuse().is_some(), _ => s.finish_reuse(g) };
  let w1 = word.load(Relaxed);
  assert_eq!(ok, w1 != w0);                       // успех ⇔ слово изменилось
  if w1 != w0 { let (p0, p1) = (large_phase(w0).unwrap(), large_phase(w1).unwrap());
    assert!(ALLOWED_EDGES.contains(&(p0, p1)));   // 9 рёбер из §1.4; из Unused/Released рёбер нет
    if (p0, p1) == (Cached, Initializing) { assert_eq!(large_generation(w1), large_generation(w0) + 1) }
    else { assert_eq!(large_generation(w1), large_generation(w0)) } }
  cover!(ok);
}
```
- Оракул красный-до/зелёный-после (каждый мутант в рабочей копии → `cargo kani --harness <имя>` = FAILURE, откат → SUCCESS; фиксируется в теле коммита и raw-логе по правилам R14-10/R22-14, т.к. вердикт опирается на исполнение): M-K1 `terminal_words.rs:80` `>=`→`>` (оракул R17); M-K2 `reservation_state.rs:67` `Consuming`→`Cached` (ребро Pending→Cached не разрешено); M-K3 `reservation_state.rs:137` убрать `next_large_generation` (поколение не растёт → ABA); M-K4 `reservation_state.rs:119` `Cached`→`Live` (ребро Live→Released). Шаг `cover!` обязан сообщить SATISFIED — иначе харнесс вакуумен.

Коммит 2 — `test(kani): prove SaturationHint version protocol; retire pack proofs` (`#[cfg(all(kani, feature = "alloc-global", feature = "internals"))]`; конструктор `dbg_with_word_for_test` под `internals`, `saturation.rs:27–31`):

```text
#[kani::proof] #[kani::unwind(5)] fn stale_saturation_snapshot_never_marks() {
  let w0: u64 = any(); let h = SaturationHint::dbg_with_word_for_test(w0); let s = h.snapshot();
  let k: u8 = any(); assume((1..=3).contains(&k)); for _ in 0..k { h.publish_claimable() }
  let w1 = h.snapshot();
  assert!(w1 == u64::MAX || (w1 > w0 && w1 & 1 == 0));  if w0 == u64::MAX { assert_eq!(w1, u64::MAX) }
  h.try_mark_saturated(s); assert!(!h.is_saturated());
}
#[kani::proof] fn fresh_snapshot_marks() { /* без publish: try_mark_saturated(snapshot) ⇒ is_saturated() для чётного w0 ≤ LAST_VERSION; cover! */ }
// Граница, найденная при разборе: чётное w0 = u64::MAX - 1 при пометке даёт w0|1 = u64::MAX = DISABLED (оптимизация
// отключается навсегда — безвредно). Через publish_claimable это слово недостижимо (версии чётные ≤ MAX-3), поэтому
// assume(w0 <= LAST_VERSION) в позитивном харнессе — модель достижимого пространства, а не сокрытие дефекта.
```
- Мутанты: `LAST_VERSION = u64::MAX - 1` (`saturation.rs:6`) → переполнение `old + 2` при `old = MAX-2` → Kani overflow-check FAILURE; `(old + 2) & !1` → `old & !1` → устаревший снимок помечает насыщение → FAILURE.
- Удалить `mod pack_proofs` (`kani_proofs.rs:171–223`) и шапку `:16–23` переписать; `ci.yml:3090` → `--features "alloc-core experimental alloc-global internals"`; комментарий `:3077–3080` — по факту.
- Здесь же: item 18 — заменить снапшот «13 = 9+2+2» новым составом; item 167 — отметить, что Kani-часть варианта (б) выполнена, loom-зеркало остаётся.

Коммит 3 — `ci: drop duplicated Kani subset workflow` — удалить `.github/workflows/kani.yml` (или, если владелец выберет K3, переписать его под тяжёлые non-gating доказательства с честной шапкой). Receipt — первый зелёный CI-прогон Kani-job'а на landing SHA.


### 5.5 Порядок коммитов (сводно)

1. `test(sidecar): pin production ClassLeaves orderings` (VER-01, C+A) — независим.
2. `build(scripts): single Miri target manifest with expectation classes` (VER-03) — до любого нового Miri-таргета.
3. `test(miri): real ClassLeaves publication race oracle` (VER-01, F) — регистрируется в манифесте из п.2.
4. `ci(miri): wire lease, diag-provenance and stats-aliasing targets` (VER-03).
5. `docs(miri): reconcile Miri inventory prose` (VER-03).
6. `build(scripts): source-scope invalidation for evidence registry rows` (VER-02) — после 2–4, чтобы первые scoped-строки ссылались на уже подключённые цели.
7. `docs(evidence): scope the R18-refreshed rows` (VER-02).
8. `test(kani): prove Large phase codec and transition relation` (VER-04).
9. `test(kani): prove SaturationHint version protocol; retire pack proofs` (VER-04).
10. `ci: drop duplicated Kani subset workflow` (VER-04).

Каждый шаг закрывает/обновляет свою карточку (188, 190, 189, 18/167 + новая) в том же коммите, по правилу индексов. Ни один шаг не меняет `production`-композицию — `perf(...)`-префиксы не применимы, `npm run bench:table`/`npm run iai` не требуются.

### Новые находки этого исследования

| # | Серьёзность | Находка | Доказательство | Владелец |
|---|---|---|---|---|
| N1 | P4 | Локальный `scripts/miri.mjs` содержит две записи, которые с 2026-08-05 компилируются в 0 тестов (`regression_bump_direct_refill` без `internals`, `segment_directory_a5_miri` без `internals`) и потому всегда FAIL; маскируются принятым FAIL от item 164 | `scripts/miri.mjs:88–91,118`; `tests/regression_bump_direct_refill.rs:30`; `tests/segment_directory_a5_miri.rs:39`; `Cargo.toml:168,200,414,670` (нет импликации `internals`); `git log` `b1a9b7b0`. Статически, не исполнено | item 190 (уточняет её «Local runner …»-формулировку) |
| N2 | P4 (информационно) | R6a-M5 цитирует `batch.rs:188`; на базовом SHA место мутанта `filled += n` — `:163` (R17 видел `:189`) | `src/registry/heap_core/alloc/batch.rs:163`; `git log fd954856..HEAD -- …/batch.rs` = 4 коммита | item 189 (якоря, §5.2) |
| N3 | — (дизайн, не дефект) | Loom на реальном `ClassLeaves`, по всей вероятности, нечувствителен к ослаблению `:113`/`:139`: защищаемое значение — инициализатор атомика, у которого нет «старого» значения | разбор §2.1; не исполнено | учтено в выборе оракула (§4.1) |

## §6 Не решить без исполнения / вопросы владельцу

Требует исполнения (запрещено в этой фазе):
- VER-01 F: что Miri действительно сообщает «Data race … non-atomic write … atomic load» на M1/M2/M3 и что производитель наблюдает ненулевой указатель за K итераций (Miri weak-memory может долго отдавать `null`; тогда увеличить K или повторять сценарий). Без этого прогона оракул — гипотеза.
- VER-01 N3: слепота real-type Loom выведена из устройства Loom, не проверена.
- VER-03 N1: вывод «running 0 tests» для двух локальных записей; время Miri для `r11_ph4a/ph4b_lease_miri`, `regression_r2_05_diag_provenance_miri`, `regression_w3_stats_aliasing_miri`, `regression_realloc_oob_old_layout`, `regression_virgin_bitmap_skip` (решает CI-класс vs LOCAL-ONLY).
- VER-04: время и SUCCESS K1/K2, SATISFIED у `cover!`, FAILURE у мутантов M-K1..M-K4; стоимость K3.
- VER-02: реальное число строк, которые классифицирует новое правило (по чтению: 124 legacy с дрейфом, 3 текущих).

Вопросы владельцу:
1. VER-02: scoped-дрейф — RED в `--strict` (рекомендовано; выход — переснять или явно понизить до CONDITIONAL в том же коммите) или только INCOMPLETE?
2. VER-02: scope в отдельном `docs/evidence/source_scope.csv` (рекомендовано) или новой колонкой реестра (ломает заголовок/генератор/baseline)?
3. VER-03: `regression_r2_06_header_race_miri` — переписать под текущий протокол и подключить в miri-plain, или удалить (по R17 она не может поймать регрессию своего класса)?
4. VER-03: CI-шаги оставить ручными со сверкой (рекомендовано) или перевести CI на `node scripts/miri.mjs --job …` (единый исполняемый источник, но перенос P1-оракула в JS)?
5. VER-04: добавить `internals` в фичи Kani-job'а (нужно для K2) — допустимо?
6. VER-04: K3 (новый tier-2 `unsafe` в `src/kani_proofs.rs`) — делать ли, и если да, то в non-gating `kani.yml`?
7. VER-04: новые Kani-доказательства — новая `[T]`-карточка или расширение 167?

## §7 Проверено и не дало вывода

- `hash_index` (`src/alloc_core/segment/segment_table/hash.rs:28–31`): маска `& (HASH_CAPACITY - 1)` тривиально в границах; Kani-доказательство не даёт ценности сверх `const`-assert'ов — не рекомендовано.
- `tests/r11_ph4a_drop_ordering_pinned.rs` как образец pin'а: разбирает аргументы CAS (сильнее `contains`), но тоже не видит перестановок — подтверждает, что текстовый pin надо дополнять семантическим оракулом; новых проблем в нём не найдено.
- `regression_xthread_small_ring_miri`: KNOWN-RED под SB (`registry.csv:64`, R4c-06) и GREEN в CI miri-plain — не противоречие, а разные модели; в манифесте §5.3 должно быть явно записано `model: plain`.
- Попытка извлечь protected-scope из текста строк реестра: 0 литералов `src/…/*.rs` среди 127 PASS — автоматическая миграция невозможна, только ручная при перепроверке (поэтому legacy-класс в §4.2).
- `gh run list/view` не использовались: CI-логи для выводов не требовались; цифры CI (13 Kani, `PASS=127`) — из отчётов R17, не перепроверены.
- Разбор CSV реестра выполнен однострочным `node -e` без вызова скриптов проекта (только чтение `docs/evidence/registry.csv`).
