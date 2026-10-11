# Solution research: CI-покрытие feature-комбинаций и красный fuzz (slug `ci-api02`)

Базовый SHA: `31db7f17698459e9ce26c3f8701c314304b9c00b` (проверен `git rev-parse HEAD`).
Режим: только чтение (без cargo/npm/node; только чтение файлов, grep, git read-only, `gh run view`).

## §0 Чек-лист хода

- [x] Шаг 0: база = 31db7f17 (совпало, checkout не понадобился)
- [x] Прочитаны правила CLAUDE.md / open items 95, 107, 161, 187–190, 195, 198
- [x] Прочитаны отчёты R17 (API-01/02/04/05/07, P3/P4)
- [x] (A) ci.yml feature-powerset / check-matrix / логи run 37315754670 → §1, §2.A
- [x] (A) гипотезы по другим красным комбинациям → §2.A (только гипотезы), §7
- [x] (B) fuzz heap_core_ops: лог LSan, стек (не символизирован), код цели → §2.B
- [x] (C) API-04/05/07 по коду → §2.C
- [x] §3–§7 варианты, рекомендации, план, вопросы владельцу

## §1 Постановка

Владельцы в индексе (на базовом SHA): **195** (`TRACKED_ci_gate_coverage.md:335–340`) — fail-fast powerset + skipped member + negative-feature MSRV, именно эта тема; **95** — осознанное `--no-dev-deps` корня (не трогать); **107** — strict-Clippy `numa-aware internals`; **161** (`TRACKED_misc.md:92–97`) — предупреждения в комбинациях без `-D warnings`; **198** — нет авто-оракула для `experimental` без 64-битных атомиков; **199** — остаток API-05 (`SegmentLayout`). Красный fuzz `heap_core_ops` — **владельца нет** (§2.B п.7). 187–190 — вне темы (TSan/ordering/evidence/Miri), не затрагиваются.

Текущий код (SHA `31db7f17`):
- `.github/workflows/ci.yml:15–17` — `schedule: cron '0 6 * * 1'` + `workflow_dispatch`; `:24–25` — `permissions: contents: read` на весь workflow.
- `.github/workflows/ci.yml:4084–4095` — job `feature-powerset`: `if: schedule || workflow_dispatch`, один шаг `cargo hack check --feature-powerset --depth 2 --no-dev-deps`; без `--keep-going`, без `RUSTFLAGS=-D warnings`, без сводки/артефакта.
- `.github/workflows/ci.yml:4124–4125` — шаг aligned-vmem `cargo hack check --feature-powerset --depth 2 -p aligned-vmem --all-targets` без `if:` → при падении корневого шага пропускается (семантика Actions по умолчанию `success()`).
- `.github/workflows/ci.yml:3946–4003` — job `fuzz-run`: 3 шага `cargo +nightly fuzz run <t> --target x86_64-unknown-linux-gnu -- -max_total_time=600`; без `ASAN_OPTIONS`/`LSAN_OPTIONS`, без символизатора.
- `.github/workflows/ci.yml:2612–2618` — `msrv` (1.93): только `cargo check --all-features` и `cargo test --no-run --all-features`; `:2781–2786` — `msrv-runtime-windows`: `--features production`.
- `.github/workflows/ci.yml:2041–2043` — единственная per-PR NUMA-проверка компиляции: `cargo check --features "production numa-aware"`; `scripts/check-matrix.mjs:235–238` — `test-r8-hardened-numa-directory` (`production hardened numa-aware internals bench-internals`). Сборки `numa-aware` без `alloc-global` per-PR нет.
- `scripts/check-matrix.mjs:43–52` — правило «PER_PR_ROWS маленький, не второй powerset»; сейчас 17 строк (6 clippy + 11 check/test, `:84–253`). Не-clippy строки идут в CI через `check-matrix` job (`ci.yml:1401–1459`, `node scripts/run-check-matrix.mjs --kind check --kind test`) автоматически, без правки `ci.yml`; `tests/ci_clippy_matrix_consistency.rs` сверяет только clippy-строки.
- Корневых фич 30 записей `[features]` (включая `default`), `rust-version = "1.93"` (`Cargo.toml:8`), resolver 2 (`Cargo.toml:88`).

## §2.A Анализ: feature-powerset и матрицы

НАБЛЮДЕНО:
1. Run `37315754670` (schedule, SHA `1175519`, создан 2026-10-05T13:18:16Z) — красные job: `test (gated bodies + all-features)` `111781952753`, `cargo-hack feature-powerset` `111781954361`, `fuzz targets run` `111781954504` (`gh run view 37315754670 --json jobs`).
2. Лог powerset (`gh run view 37315754670 --job 111781954361 --log`): строка 368 — старт шага в 13:22:36.86Z; строка 2442 — `running \`cargo check --no-default-features --features numa-aware\` on sefer-alloc (205/390)` в 13:24:42.32Z; строка 2447 — `error[E0433]: cannot find type \`SegmentMeta\` in this scope`; строка 2462 — `##[error]Process completed with exit code 1.` Позиции 206–390 и шаг aligned-vmem не выполнены.
3. Зелёный предыдущий плановый прогон `36423048188` (SHA `90e7855`), job `108930215742`: корневой шаг 12:40:23Z→12:44:14Z (3 мин 51 с), aligned-vmem 12:44:14Z→12:44:21Z (7 с), job целиком 4 мин 6 с.
4. Скорость: 205 комбинаций за 125.5 с → ~0.61 с/комбинацию (из п.2); полный проход ~3.9 мин (п.3). `--keep-going` поэтому стоит ≤ ~2 мин сверх красного прогона (оценка: оставшиеся 185 × 0.61 с ≈ 113 с).
5. Ошибка исправлена `fc40ab96` (`src/alloc_core/large/alloc_core_large.rs:15` — гейт импорта `any(feature = "alloc-xthread", feature = "numa-aware")`); карточка 107 (R18) цитирует исполненные квитанции: обе no-default NUMA-формы `cargo check` проходят (исполнение R18, не моё).
6. Push-CI на базе зелёный: run `38103112281` (SHA `31db7f17`, 2026-10-11, success). Powerset/fuzz на push не запускаются.
7. История schedule-прогонов (`gh run list --event schedule`): failure `29729051979` (07-20), `31365336108` (08-10), `37315754670` (10-05); остальные 9 последних — success. Ни `37315754670`, ни `111781954361/111781954504` не упомянуты в индексах (R17 ревью §«Наблюдено», подтверждено `rg` по `docs/correctness-open-items`).
8. Последний ПОЛНЫЙ проход powerset — `90e7855` (2026-09-28). `git diff --stat 90e7855d0f..HEAD` по 16 файлам с `feature = "numa-aware"` — 1181 вставок / 2004 удалений (например `find_segment.rs` 583 строки, `core.rs` 450, `segment_header_impl.rs` 317). Объём непроверенных powerset-ом правок в cfg-нагруженных файлах большой.

ВЫВЕДЕНО:
- **`--depth 2` не был причиной пропуска.** R17-API-01 — комбинация глубины 1 (`--no-default-features --features numa-aware`, позиция 205). R13-12 (`alloc-xthread+fastbin+alloc-decommit` без `alloc-segment-directory`) — глубина 2 в терминах cargo-hack, потому что `fastbin = ["alloc-global", "alloc-xthread"]` (`Cargo.toml`) и импликации раскрываются. Пропуск недели вызван fail-fast + отсутствием канала уведомления, не глубиной.
- **Где глубина 2 реально недостаточна (ГИПОТЕЗА о пробеле покрытия, не о дефекте).** Перепись cfg-атрибутов в `src/` (`grep -rzoE '#!?\[cfg(_attr)?\([^]]*\]'`, однострочные: 127 с 1 фичей, 262 с 2, 17 с ≥3; многострочные добавляют ещё десятки с ≥3). Почти все ≥3-фичевые — диагностические гейты (`internals`/`bench-internals`) или пары с импликацией (`alloc-global`+`fastbin` ≡ `fastbin`). Единственный политический 4-фичевый предикат — «HAS_PROMOTION»: `all(medium-classes, any(not(exact-span-large), all(large-reserved-capacity, not(numa-aware))))` (`src/registry/heap_core/free/dealloc.rs`, `realloc.rs`, `dealloc_own_base.rs`, `src/alloc_core/segment/segment_header/segment_header_views.rs`). Ветки: «вкл. через ¬esl» — per-PR `hardened medium-classes internals` (`ci.yml:1346`), `production medium-classes` (`:2006`); «выкл. через esl» — `production medium-classes exact-span-large` (`:2013`); «выкл. через numa» — `--all-features`. **Ветка «вкл. через `large-reserved-capacity` ∧ ¬numa» вместе с `fastbin`** (registry-сторона компилируется только с `alloc-global`) не собирается ни одной строкой: `rg 'features[^#]*large-reserved-capacity'` даёт только `ci.yml:2176` и `check-matrix.mjs:243` — обе без `medium-classes`. В powerset depth 2 это 3 явные фичи (`fastbin`, `medium-classes`, `large-reserved-capacity`) → не покрыто.
- **`--no-dev-deps` на результат проверки lib не влияет.** `cargo check` без `--all-targets` dev-зависимости не компилирует; resolver 2 не унифицирует фичи dev-deps для lib; ни одна dev-зависимость (`crates/malloc-bench-rs`, `globalalloc-model`, `proc-memstat`, `proc-probe`) не зависит обратно от `sefer-alloc` (`grep` по их `Cargo.toml` — только `repository/homepage`). Флаг лишь не даёт перейти к `--all-targets` (cargo-hack отвергает пару, `ci.yml:4110–4112`), а это осознанно заблокировано пунктом 95.
- **Какие комбинации сейчас вероятно красные — только ГИПОТЕЗЫ.** Чтение не выявило конкретной новой E0xxx; проверены: `alloc_core_large.rs` (импорты `Segment` под `not(numa-aware)`, `SegmentMeta` под `any(xthread, numa)`, все сайты `:450,:691,:727` согласованы), новые `internals`-гейты `f384eb0f` в `epoch_region.rs:656`, `sharded_region.rs:719,733` (вызываются только из своих же `internals`-гейтов/тестов). Априорно повышенный риск (по объёму непроверенных правок, п.8): пары с `numa-aware`/`numa-aware-mock` × {`alloc-segment-directory`, `alloc-stats`, `page-map-diag`, `medium-classes`, `bench-internals`, `small-segment-lazy-commit`, `primordial-lazy-commit`, `large-reserved-capacity`}; пары с `page-map-diag`, `pinning`, `virgin-zero-skip`. Это ровно «хвост» 206–390, который не исполнялся с `90e7855`. Ответ даст только исполнение (§6).
- **Предупреждения.** R17-ревью: «В зелёном прогоне `108930215742` — 180 строк предупреждений» (`docs/reviews/2026-10-08-src-review-oxx-round-17-api-features-portability.md:143`). Значит `RUSTFLAGS=-D warnings` в powerset сделает job красным сразу (пункт 161) — включать нельзя до чистки.

## §2.C Анализ: R17-API-04/05/07 на базовом SHA

- **API-04 (a)/(b) задокументированы** `8ac3f93d`: `Cargo.toml:693–696` (doc `numa-aware`: «Selects eager reservation for primordial and ordinary Small segments, overriding both lazy-commit features. Excludes forced Small directory rescue; NUMA Large reservations have no extra reserved-capacity growth headroom.»), README «NUMA policy — eager reservation, no forced Small rescue», `docs/INTEGRATION.md:23–27` (`:27` «disabled by `numa-aware`»). Поведение не менялось (выбор политики, а не дефект).
  - Остаток 1: doc самих фич не содержит перекрёстной оговорки — `primordial-lazy-commit` (`Cargo.toml:772–800`) и `large-reserved-capacity` (`Cargo.toml:335–392`, единственное вхождение «no-op» на `:372` — про другое) про NUMA молчат; оговорка только у `numa-aware` и в README/INTEGRATION. Читатель, начавший с doc своей фичи, не узнает.
  - Остаток 2: устаревший комментарий `src/alloc_core/small/alloc_core_small/find_segment.rs:210–213` («the directory-driven lookup block below … is compiled out entirely (gated `not(feature = "numa-aware")`)») — а блок на `:271` гейтован только `alloc-segment-directory`. Это API-04(c)-проза; владелец класса — 154.
  - Остаток 3: гипотеза (c) (устаревший отрицательный бит директории + отказ ОС → null под standalone NUMA) не проверена исполнением; R18 сузил до hypothesis (`…-P4.md:206–209`).
- **API-05:** исправлено `8ac3f93d` — состав `production` в INTEGRATION, строка `alloc-stats`, строка `internals` в README. Остаётся: (i) item **199** (`SegmentLayout`); (ii) **27 ссылок в комментариях `Cargo.toml` на несуществующие файлы** — пересчитано: 16 полных путей (`Cargo.toml:182,187,393,549,562,563,575,579,624,653,926,927,1351,1656,1831,1957`: `src/global/sefer_alloc.rs`, `src/registry/bootstrap.rs`, `src/alloc_core/large_cache_extended.rs`, `src/alloc_core/alloc_core_small_pool.rs`, `src/registry/heap_core_diag.rs`, `src/alloc_core/alloc_core_core_diag.rs`, `src/alloc_core/size_classes.rs`, `src/alloc_core/remote_free_ring.rs`, `src/alloc_core/alloc_core_small.rs`, `src/registry/heap_registry.rs`) + 11 голых имён (`:272,548,780,1095,1307,1465,1516,1551,1649,1693,2271`); «the 1024 cap» на `:306` (при `MAX_SEGMENTS = 4096`); (iii) `doc(cfg)` и docs.rs-набор — R18 признал «not mandated» (`…-P4.md:215`), не дефект. Рецепт пересчёта (ii): `grep -noE 'src/[A-Za-z0-9_/]+\.rs' Cargo.toml` + проверка `-e`.
- **API-07 закрыт по коду** `22285f56`: `src/lib.rs:366–371` — `#[cfg(all(feature = "experimental", not(target_has_atomic = "64")))] compile_error!(…)`; `Cargo.toml` — doc «Requires `target_has_atomic = "64"`… `pinning` and `batch-api` inherit». Остаток — только item **198** (нет автоматического негативного оракула в CI).

## §2 Анализ

### §2.B Красный fuzz `heap_core_ops` (run `37315754670`, job `111781954504`, SHA `1175519`)

НАБЛЮДЕНО (лог `gh run view 37315754670 --job 111781954504 --log`):
- Шаги `region_ops` и `global_alloc_ops` зелёные (лог строки 1631 `Done 39363575 runs in 601 second(s)`, 2634 `Done 183077 runs in 603 second(s)`).
- `heap_core_ops`: строка 3834 `Done 711505 runs in 601 second(s)` — цикл фаззинга ЗАВЕРШИЛСЯ штатно; только ПОСЛЕ этого строка 3837 `==3062==ERROR: LeakSanitizer: detected memory leaks`, 3839 `Direct leak of 128 byte(s) in 1 object(s) allocated from:`, 3859 `SUMMARY: AddressSanitizer: 128 byte(s) leaked in 1 allocation(s).`
- Стек (строки 3840–3857) НЕ символизирован: 18 кадров, все вида `heap_core_ops+0x1431c4` … `libc.so.6+0x2a1c9/0x2a28a` → `heap_core_ops+0xac1d4` (`_start`). Имён функций в логе нет — «точный стек» по логу получить нельзя (§6).
- Артефакт: `crash-da39a3ee5e6b4b0d3255bfef95601890afd80709` (строка 3878). `da39a3ee…` — SHA-1 пустой строки. Префикс `crash-`, не `leak-`.
- Предыдущий плановый прогон `36423048188` (SHA `90e7855`, 2026-09-28): job `108930215774` шаг `fuzz run heap_core_ops (10 min)` = success.

ВЫВЕДЕНО (высокая уверенность, но без символизации/исполнения):
1. **Это exit-time проверка LSan (ASan `detect_leaks=1` по умолчанию на Linux), а не per-input детектор libFuzzer.** Per-input путь libFuzzer (`TryDetectingAMemoryLeak`) печатает `ERROR: libFuzzer: detected memory leaks` и пишет `leak-<sha>`; здесь отчёт идёт после `Done … runs`, артефакт `crash-…`. Death-callback libFuzzer сбрасывает «текущий юнит», а после конца цикла он пуст → SHA-1 пустого входа. **Формулировка «утечка на пустом входе» — артефакт отчёта.** Пустой вход в этой цели не делает ни одной аллокации (`ops` пуст → цикл `for op in ops` не исполняется, `SeferAlloc::new()` heap не материализует, `fuzz/fuzz_targets/heap_core_ops.rs:105–119`), поэтому воспроизведение `cargo fuzz run heap_core_ops crash-da39…` скорее всего будет ЗЕЛЁНЫМ (гипотеза, не исполнялось). Per-input детектор не срабатывал, потому что libFuzzer повторно прогоняет вход с выключенным LSan и пропускает проверку, если при повторе malloc==free — однократная ленивая инициализация так маскируется (стандартное поведение libFuzzer).
2. **Кандидат на объект 128 B — массив `RouteSlots` (`src/alloc_core/segment/segment_table/route_slots.rs:37–72`).** Начальная ёмкость `max(8)` (`:41`), элемент `Option<RouteRegistration<'static>>`, где `RouteRegistration = { directory: &RouteDirectory, entry: EntryHandle(AtomicPtr), PhantomData }` (`src/registry/segment_route/registration.rs:9–13`) → 16 B (ниша `&T`), 8 × 16 = **128 B** (оценка по чтению layout; Rust не гарантирует layout `repr(Rust)`, но размер 16 типичен). Аллоцируется через `System.alloc` (`:48`) = ASan-`malloc` в процессе фаззера (там глобальный аллокатор — системный, `heap_core_ops.rs:26–29`).
3. **Почему LSan считает его утёкшим.** Указатель `slots` хранится только внутри `SegmentTable::routes` (`segment_table_impl.rs:179,272`) → `AllocCore` → `HeapCore` в слоте реестра, а чанки реестра — `mmap` (`src/registry/bootstrap/mod.rs:105–112`). LSan сканирует глобалы/стеки/TLS/свою кучу, но не анонимный `mmap` → единственная ссылка невидима → «direct leak». Прочие `System`-объекты маршрутного каталога (`Entry` 72 B, `Block`, `BlockIndex`, `SmallSidecar`, `LargeState`, mixed-листья 256 B; `src/registry/segment_route/directory.rs:56,72,85,244,378`, `leaf_classes.rs:102`) достижимы из `static ROUTES` (`directory.rs:711–713`, `.bss`, сканируется) — поэтому в отчёте ровно 1 объект и 0 indirect. Это согласуется с наблюдением (1 direct, 0 indirect), что повышает уверенность.
4. **Это не утечка аллокатора, а ложное срабатывание LSan на метаданные со временем жизни процесса.** `AllocCore::drop` документирует: «Registry heaps never reach this `drop`… lives for the entire process … retained in its slot for reuse» (`src/alloc_core/alloc_core/lifecycle.rs:398–404`); `RouteSlots` освобождается в своём `Drop` (`route_slots.rs:223–233`), ограничен одним массивом на слот реестра (до `MAX_SEGMENTS` элементов). Повторный `attach_owner` при уже существующих routes возвращает `None` (`segment_table_impl.rs:269`), т. е. перезаписи с потерей старого массива нет.
5. **Регрессия по времени точно совпадает.** На `90e7855` (последний зелёный fuzz) `git grep "System\.\(alloc\|alloc_zeroed\)" 90e7855d0f -- src` пуст; `route_slots.rs` отсутствует. Файл добавлен `1d881429` (2026-09-29, «feat: bind route lifetime to HeapCore-owned segments»), каталог — `efec9d16` (2026-09-29); оба в `1175519`, ни один — в `90e7855`. `global_alloc_ops` (standalone `AllocCore`, owner `None` → `routes` = `None`, `lifecycle.rs:405–409`) остался зелёным — тоже согласуется.
6. **Следствие для пользователей (новое, P4, ВЫВЕДЕНО; сценарий редкий — ASan/LSan с кастомным `#[global_allocator]` нетипичен):** downstream, установивший `SeferAlloc` как `#[global_allocator]` и запустивший LSan, получит такой же отчёт ≥128 B на каждый материализованный слот heap (`System` = libc `malloc` и под установленным `SeferAlloc`). В README/fuzz/README про LSan ничего нет (`rg LeakSanitizer|detect_leaks` находит только ASan-job `ci.yml:3340–3344` и `scripts/asan.mjs:141–146`, где LSan уже отключён по той же причине «mmap не отслеживается»).
7. Красный fuzz НЕ записан ни в одном индексе (`rg heap_core_ops|111781954504 docs/correctness-open-items` → только item про покрытие `TRACKED_verification_coverage.md:254`, без упоминания утечки).

8. Сборка fuzz-целей идёт с отладочной информацией (лог строки 428/1652/2655: `release` profile [optimized + debuginfo]), значит символизация возможна, если ASan найдёт символизатор; сейчас он не найден (кадры без имён).
9. Прогноз (ВЫВЕДЕНО): плановый прогон 2026-10-12 снова даст красный `fuzz run heap_core_ops` — проверка на выходе детерминирована, пока heap материализуется, а `RouteSlots` на базе не менялся.

(§2.A и §2.C — выше, между §1 и §2.B.)

## §3 Варианты

### Q1. Fail-fast powerset (item 195a)

| Вариант | Механизм | Цена | Ловит | Не ловит |
|---|---|---|---|---|
| 1a. Оставить + документировать | ничего; правило «смотреть плановый прогон» | 0 | — | весь хвост после первой ошибки (как 2026-10-05: 185/390 + 14 aligned-vmem) |
| 1b. `--keep-going` в корневом шаге + `if: ${{ !cancelled() }}` на шаге aligned-vmem | cargo-hack продолжает после ошибки; второй шаг больше не пропускается | ≤ ~2 мин сверх красного прогона (§2.A п.4); job всё равно красный | все упавшие комбинации за один прогон, независимость члена aligned-vmem | итоговую сводку (только сырой лог 2.5k строк) |
| 1c. 1b + разбор лога в `$GITHUB_STEP_SUMMARY` + артефакт | `tee` лога, скрипт строит таблицу «попытки/упали/первая ошибка/число warning» и грузит лог через `actions/upload-artifact` | +5–15 с (оценка), без новых прав (summary и artifact не требуют записи в репозиторий) | 1b + читаемый список упавших и счёт предупреждений по комбинациям (данные для 161) | уведомление никому не приходит |
| 1d. Матрица `--partition k/N` по job-ам | N параллельных job | ×N накладных расходов на checkout/toolchain (~30 с каждый, оценка) | то же, что 1b, быстрее по wall | дороже, сложнее; полный прогон и так ~4 мин |

### Q2. «Красный плановый прогон никто не заметил»

| Вариант | Механизм | Цена | Ловит | Не ловит |
|---|---|---|---|---|
| 2a. Оставить + документировать | встроенные письма GitHub о провале schedule-workflow (по документации GitHub уходят автору последней правки cron; не проверено) | 0 | — | фактически не сработало: 2026-10-05 ушло незамеченным до R17 (3 дня) |
| 2b. Правило процесса + read-only скрипт | в CLAUDE.md «Round start» добавить шаг: `gh run list --workflow ci.yml --event schedule --limit 1` и карточка в индексе при failure; скрипт `scripts/check-scheduled-ci.mjs` (только чтение `gh run list/view`) | минуты на раунд | любой красный плановый прогон к началу следующего раунда | зависит от дисциплины (как push-правило R33-2 до его нарушения) |
| 2c. Job-сводка 1c | `$GITHUB_STEP_SUMMARY` | 0 доп. | читаемость при открытии run | сама не уведомляет |
| 2d. Job `scheduled-failure-issue` | `needs: [feature-powerset, fuzz-run, numa-real-kernel, test-gated-bodies]`, `if: failure() && github.event_name == 'schedule'`, job-level `permissions: issues: write`; `gh issue create/comment` с меткой `scheduled-ci-red` (дедуп по открытому issue) | ~10 с; расширяет права одного job (противоречит формулировке SEC-4 `ci.yml:19–23` «No job … performs any write») | видимый в репозитории сигнал в момент провала | не создаёт карточку в индексе сам (это остаётся 2b) |

### Q3. Per-PR строка `numa-aware` без `production` (R17-API-01/02, item 107)

| Вариант | Механизм | Цена | Ловит | Не ловит |
|---|---|---|---|---|
| 3a. Оставить (только weekly powerset) | — | 0 | depth-1/2 NUMA-комбинации раз в неделю | регрессию на PR; strict Clippy (107 остаётся открытым) |
| 3b. `check`-строка в `PER_PR_ROWS`: `alloc-decommit numa-aware internals` + `RUSTFLAGS=-D warnings` | автоматически в CI-job `check-matrix` и в `npm run check` без правки `ci.yml` | 1 `cargo check` lib, ~20–40 с (оценка) | E0433-класс на `:481` и `:693`, предупреждения rustc | clippy-линты → item 107 не закрывает |
| 3c. **clippy-строка `libOnly: true`: `numa-aware internals`** + шаг в `ci.yml` job `clippy` | strict Clippy `--lib … -- -D warnings`; `tests/ci_clippy_matrix_consistency.rs` уже поддерживает `libOnly` (`:57–60,107,146–153`) | 1 `clippy --lib`, ~30–60 с (оценка); 17→18 строк | E0433 на `:693`-классе + все clippy-линты точной комбинации 107 | сайт `:481` (нужен `alloc-decommit`) — остаётся weekly powerset (пара `alloc-decommit,numa-aware`) |
| 3d. Строка в `test-feature-isolation` (`ci.yml:1920+`) | рукописный шаг `cargo check --features "numa-aware"` | как 3b | как 3b | не идёт в `npm run check`; строки этого job не сверяются с манифестом |

### Q4. Глубина powerset и значимые комбинации

| Вариант | Механизм | Цена | Ловит | Не ловит |
|---|---|---|---|---|
| 4a. Оставить depth 2 + документировать | — | 0 | все 1- и 2-фичевые (с импликациями) | ветку HAS_PROMOTION через `large-reserved-capacity` вместе с `fastbin` (§2.A) и прочие 3-фичевые политики |
| 4b. Полный depth 3 | `--depth 3` | C(29,3)=3654 доп. комбинаций до прореживания × ~0.6 с ≈ 35 мин+ (оценка) | всё тройное | — ; дорого для пользы, в основном диагностические гейты |
| 4c. **Доп. шаг depth 3 на подмножестве «политических» фич** | `cargo hack check --feature-powerset --depth 3 --include-features fastbin,medium-classes,medium-classes-wide,exact-span-large,large-reserved-capacity,numa-aware,hardened,alloc-decommit,alloc-segment-directory,virgin-zero-skip --no-dev-deps --keep-going` | ≤ 176 комбинаций до прореживания (C(10,3)+C(10,2)+10+1), ~2 мин (оценка) | тройки политик, вкл. HAS_PROMOTION-lrc с `fastbin` | диагностические тройки (`internals`/`bench-internals`) — их держат per-PR строки |
| 4d. Одна явная строка `fastbin medium-classes large-reserved-capacity` | `cargo check`/`test --test r14_4_promotion_move_leg_reduction` | 1 сборка | ровно найденный пробел; с тестом — и поведение | будущие тройки |

### Q5. MSRV и `cfg(not(<фича>))` (item 195b)

| Вариант | Механизм | Цена | Ловит | Не ловит |
|---|---|---|---|---|
| 5a. Оставить + документировать | — | 0 | `--all-features` и `production` на 1.93 | любые ветки `not(X)` для X из `production` |
| 5b. Точечная строка `cargo check --no-default-features --features alloc-global` на 1.93 | документированная форма `docs/INTEGRATION.md` | ~30 с (оценка) | ветки `not(fastbin/decommit/directory/primordial)` при `alloc-global` | прочие `not(X)` |
| 5c. **Weekly MSRV powerset depth 1** | `cargo +1.93 hack check --each-feature --no-dev-deps --keep-going` (каждая фича по одной; вместе с уже существующим `--all-features` покрывает обе полярности каждого одно-фичевого `cfg`) | ~30 комбинаций, ~1–2 мин + установка 1.93 (оценка) | MSRV-нарушение в любой ветке `not(X)` уровня одной фичи | парные `cfg(all(a, not(b)))` (редки) |
| 5d. Weekly MSRV depth 2 | как 5c, но `--feature-powerset --depth 2` | ~4–5 мин (оценка, ≈ основному шагу) | всё, что ловит основной powerset, но на 1.93 | — ; удваивает время job |

### Q6. Красный fuzz `heap_core_ops` (LSan, 128 B)

| Вариант | Механизм | Цена | Ловит | Не ловит / риск |
|---|---|---|---|---|
| 6a. Оставить красным + документировать | карточка в индексе | 0 | — | job каждую неделю красный → обесценивает реальные находки fuzz (обучает «перезапускать») |
| 6b. `detect_leaks=0` для шага `heap_core_ops` | `ASAN_OPTIONS=detect_leaks=0` (прецедент: ASan-job `ci.yml:3344`) | 0 | все OOB/UAF/паники/M1–M4 | любые утечки `System`-метаданных маршрутного каталога (реальный класс: `Entry`/sidecar, не освобождённые после unlink) |
| 6c. `leak_check_at_exit=0` | отключает только проверку на выходе; per-input детектор libFuzzer остаётся | 0 | per-input утечки (то, что растёт от входа к входу) | если per-input проверка когда-либо сработает, она тоже увидит 128 B → латентный флейк |
| 6d. **Символизация + узкое подавление** | `fuzz/lsan.supp`: `leak:route_slots::RouteSlots::ensure_capacity` (через `LSAN_OPTIONS=suppressions=…`); символизатор: `ASAN_SYMBOLIZER_PATH` на `llvm-symbolizer` раннера или `ASAN_OPTIONS=allow_addr2line=1` | ~0; одна строка подавления + обоснование | всё из 6b + утечки любых других `System`-объектов, в т. ч. `Entry`/sidecar/листьев; символизированные стеки для будущих находок | утечку самого массива `RouteSlots` (по дизайну процессный) — её ловит Drop-тест (§5) |
| 6e. Регистрация корней LSan в аллокаторе | под `cfg(fuzzing)` (cargo-fuzz передаёт `--cfg fuzzing`) вызывать `__lsan_register_root_region` для mmap-чанков реестра | новый `unsafe extern` в seam, `check-cfg`, только nightly-сборка с санитайзером, иначе ошибка линковки | точная модель достижимости, без подавлений; помогает и downstream | меняет продуктовый код ради харнесса; правило «unsafe только в seam» |
| 6f. Перенести `RouteSlots` в mmap (aligned-vmem) | LSan его вообще не видит | правка горячего пути/учёта памяти | — | ломает обоснование «System-backed seam», не оправдано |

### Q7. Остатки API-04/05/07

| Вариант | Механизм | Цена | Ловит | Не ловит |
|---|---|---|---|---|
| 7a. Оставить (докам 8ac3f93d достаточно) | — | 0 | — | читателя doc отдельной фичи; устаревший комментарий `find_segment.rs:210–213`; 27 мёртвых ссылок `Cargo.toml` |
| 7b. **Точечные doc-правки** | 1 строка в doc `primordial-lazy-commit` и `large-reserved-capacity` («overridden by `numa-aware`», ссылка на doc `numa-aware`); исправить комментарий `find_segment.rs:210–213`; заменить 27 путей в `Cargo.toml` на ссылки по символам/текущие пути | только docs/comment | расхождение в первоисточнике | гипотезу API-04(c) (нужно исполнение) |
| 7c. 7b + страж | (i) добавить `docs/INTEGRATION.md` в список `sites` стража состава `production` (`tests/no_stale_doc_references.rs:877` — сейчас только `src/lib.rs`, `src/global/sefer_alloc/core.rs`, `README.md`; файл исправлен `8ac3f93d`, но под стражем не стоит — рекомендация R17-API-05 выполнена наполовину); (ii) новая проверка «каждый путь `src/…rs` в комментариях `Cargo.toml` существует» | 1–2 теста, мс | повторный дрейф состава в INTEGRATION и путей в манифесте | голые имена файлов (решать заменой на символы) |
| 7d. Item 198: негативный оракул на 32-битной цели | weekly: `rustup target add powerpc-unknown-linux-gnu`; шаг сам проверяет `rustc --print cfg --target …` (нет `target_has_atomic="64"`, есть `="ptr"`), затем `! cargo check --target … --no-default-features --features experimental` с `grep -F "requires 64-bit atomics"`, плюс контроль `cargo check --target … --no-default-features --features std` = успех | ~1 мин weekly (оценка) | регресс гейта `src/lib.rs:366–371` и неверный выбор цели (самопроверка cfg) | сборку зависимостей `experimental` на этой цели (если упадёт dep — шаг красный с другим текстом, это и будет сигнал) |

## §4 Рекомендации

| Вопрос | Решение | Уверенность | Закрывает |
|---|---|---|---|
| Q1 fail-fast | **1c**: `--keep-going` + `if: ${{ !cancelled() }}` на шаге aligned-vmem + сводка в `$GITHUB_STEP_SUMMARY` + артефакт лога | высокая (дёшево, без новых прав) | item 195(a) после контрольного прогона |
| Q2 незамеченный красный | **2b + 2c сейчас**; **2d — только с согласия владельца** (меняет SEC-4) | средняя: 2b зависит от дисциплины; 2d — единственный «push»-сигнал | новая карточка «scheduled red unnoticed» (R22-3-класс) |
| Q3 per-PR NUMA | **3c**: clippy `libOnly` строка `numa-aware internals` (18-я строка; обоснование — реальный пропущенный P3 R17-API-01, категория (b) правила `check-matrix.mjs:48–52`) | высокая по механизму; чистота `-D warnings` на этой комбинации подтверждена R18 только для `clippy --features "numa-aware internals"` без `--lib`-ограничения (карточка 107) | item 107 |
| Q4 глубина | **оставить depth 2** для основного шага + **4c** (depth 3 на 10 политических фичах, weekly) | средняя: ценность 4c обоснована одним найденным пробелом (HAS_PROMOTION-lrc), дефекта не показано | новый под-пункт покрытия |
| Q5 MSRV | **5c**: weekly `cargo +1.93 hack check --each-feature --no-dev-deps --keep-going` | высокая по покрытию одно-фичевых `not(X)` | item 195(b) после контроля с внедрённой несовместимостью |
| Q6 fuzz | **6d** (символизация + узкое подавление `RouteSlots::ensure_capacity`) + Drop-тест на освобождение массива + README-заметка о LSan; **если символизатор на раннере недоступен — временно 6c** только для шага `heap_core_ops` с явным TODO-номером карточки | высокая для диагноза «ложное срабатывание» (5 независимых согласований §2.B), средняя для точного объекта 128 B (стек не символизирован) | новая карточка «fuzz heap_core_ops LSan» |
| Q7 API-04/05/07 | **7b + 7c(i)** сейчас; 7c(ii) — по желанию; **7d** для item 198 | высокая (docs) / средняя (7d: зависимость `experimental` на ppc32 не проверена) | частично 199-соседнее, item 198 |

Серьёзность новых находок (этого исследования): **P4** — HAS_PROMOTION-lrc-ветка с `fastbin` не собирается ни одной строкой CI (§2.A); **P4** — красный fuzz без владельца + LSan-контракт для downstream (§2.B п.6–7); **P4** — `docs/INTEGRATION.md` не под стражем состава `production` (7c(i)). P3 (ошибок компиляции в поддерживаемой комбинации) чтением не найдено.

## §5 План реализации

Порядок: сначала то, что делает следующий плановый прогон информативным (коммиты 1–3), затем расширения покрытия. Каждый коммит — отдельная фаза с тестом/контролем по правилам CLAUDE.md.

**Коммит 1 — `ci: keep going through the weekly feature powerset and summarize failures`**
- `.github/workflows/ci.yml:4094–4095` →
```yaml
      - name: cargo hack check --feature-powerset --depth 2
        shell: bash
        run: |
          set -o pipefail
          cargo hack check --feature-powerset --depth 2 --no-dev-deps --keep-going 2>&1 | tee powerset-root.log
      - name: Summarize powerset (root)
        if: ${{ !cancelled() }}
        run: node scripts/powerset-summary.mjs powerset-root.log >> "$GITHUB_STEP_SUMMARY"
```
- `:4124–4125`: добавить `if: ${{ !cancelled() }}`, `--keep-going`, `tee powerset-aligned-vmem.log` и такой же шаг сводки; в конце job:
```yaml
      - uses: actions/upload-artifact@v4
        if: ${{ failure() }}
        with: { name: feature-powerset-logs, path: "powerset-*.log", retention-days: 30 }
```
  (+ `actions/setup-node@v5`, как в `check-matrix`, `ci.yml:1452–1454`.)
- Новый `scripts/powerset-summary.mjs`: разбирает строки `running \`(.+)\` on (\S+) \((\d+)/(\d+)\)` (формат подтверждён логом, строка 2442) и следующие за ними `error[`/`error:`/`warning:` до следующего `running`; печатает Markdown: всего/попыток/упавших, таблицу упавших (команда, позиция, первая строка ошибки), счёт warning по комбинациям; exit 0 (статус job задаёт шаг cargo-hack). `--self-test` на встроенном фрагменте лога (строки 2425–2462 run `37315754670`: одна зелёная позиция 204 и красная 205) + синтетическая вторая ошибка; ожидание «attempted 205, failed 1: numa-aware (205/390) E0433».
- Подключить `node scripts/powerset-summary.mjs --self-test` в `check-matrix` job рядом с другими текстовыми сканами (`ci.yml:1467+`) и в `scripts/check-all.mjs` — иначе это «guard без gate-строки» (класс item 80).
- Обновить комментарий job (`ci.yml:4038–4083`) и CLAUDE.md «cargo-hack feature-powerset CI»: `--keep-going`, независимость шага aligned-vmem.
- Контроль (counterfactual): `workflow_dispatch` на ветке, где временно возвращён узкий гейт `#[cfg(feature = "alloc-xthread")]` у импорта `SegmentMeta` (`alloc_core_large.rs:15`): ожидание — лог содержит `(390/390)`, сводка «failed ≥1: numa-aware…», шаг aligned-vmem выполнен (14/14). Только с разрешения владельца (§6).

**Коммит 2 — `fix(ci): suppress the process-lifetime RouteSlots leak in heap_core_ops fuzzing`** (или `test(fuzz):`)
- `fuzz/lsan.supp`:
```text
# Process-lifetime per-heap route slot array: System-allocated, referenced only
# from the mmap'd HeapRegistry chunk that LSan does not scan. Registry heaps
# are never dropped (src/alloc_core/alloc_core/lifecycle.rs, AllocCore Drop doc).
leak:route_slots::RouteSlots::ensure_capacity
```
- `ci.yml:3946–4003` (`fuzz-run`): job-level
```yaml
    env:
      LSAN_OPTIONS: "suppressions=${{ github.workspace }}/fuzz/lsan.supp:print_suppressions=1"
      ASAN_OPTIONS: "symbolize=1:allow_addr2line=1"
```
  либо `ASAN_SYMBOLIZER_PATH` на `llvm-symbolizer` раннера (какой путь существует — §6). `print_suppressions=1` печатает число подавленных объектов → видно, что подавление сработало и только на 128/256 B.
- Тест ограниченности (контракт «не утечка, а время жизни процесса»; подавление не должно прятать рост). Дроп-тест невозможен публично: `AllocCore::new_with_owner` — `pub(crate)` (`lifecycle.rs:120`), а registry-heap по дизайну не роняется. Поэтому: `#[cfg(feature = "internals")]` счётчики alloc/free массивов в `route_slots.rs` (по образцу `ClassLeaves::record_alloc`/`system_totals`, `leaf_classes.rs:197–221`, и форвардера `SmallSidecar::system_totals_for_test`, `src/registry/segment_route/small_sidecar.rs:138–139`) + новый `tests/route_slots_bounded_per_slot.rs`: K циклов «поток → alloc/free через `SeferAlloc` → join» должны дать «живых» массивов ≤ числа реально заклейменных слотов (повторный claim переиспользует `routes`, `segment_table_impl.rs:269`). Нагрузка должна держать > 8 живых сегментов, чтобы сработал рост `ensure_capacity`. Counterfactual: временно убрать `System.dealloc` старого массива в `ensure_capacity` (`route_slots.rs:62–68`) → «живых» массивов > 1 на слот, тест красный. Новый hook → проверить по `scripts/verify-dbg-hook-safety.mjs` (read-only observer).
- Наблюдательный оракул в самом fuzz: `print_suppressions=1` должен показать ровно 1 подавленный объект (один поток → один слот); рост числа — сигнал.
- `fuzz/README.md` и README (раздел про санитайзеры): «LSan reports process-lifetime route metadata as leaks when `SeferAlloc` heaps live in mmap'd registry memory; suppress `RouteSlots::ensure_capacity`».
- Контроль: прогон `fuzz-run` (dispatch) — зелёный, в логе `Suppressions used: … leak:route_slots::RouteSlots::ensure_capacity` с 1 объектом; без файла подавления — красный (уже наблюдено в `37315754670`).

**Коммит 3 — `docs: index the unnoticed scheduled CI failures`** (тот же день, что и коммиты 1–2, или раньше)
- `docs/correctness-open-items/TRACKED_ci_gate_coverage.md`: новая карточка «scheduled run `37315754670` red, unnoticed 3 days; fuzz `heap_core_ops` LSan false positive» со Status/Current/Next/Evidence; дополнить 195 ссылкой на коммит 1; `docs/CORRECTNESS_OPEN_ITEMS.md` — строка в таблице номер→файл.
- CLAUDE.md «Round start: check BOTH open-items indexes»: добавить «and the latest scheduled CI run (`gh run list --workflow ci.yml --event schedule --limit 1`); a red scheduled run gets a card the same day». (Вариант 2b; по желанию — read-only `scripts/check-scheduled-ci.mjs`.)

**Коммит 4 — `ci: add strict clippy for standalone numa-aware internals`**
- `scripts/check-matrix.mjs` после `clippy-production-internals` (`:155`):
```js
  {
    id: 'clippy-numa-aware-internals',
    kind: 'clippy',
    features: 'numa-aware internals',
    libOnly: true,
    note:
      'R17-API-01/item 107: numa-aware without alloc-global/alloc-xthread ' +
      '(standalone AllocCore + NUMA) shipped E0433 for a week; no per-PR row ' +
      'built it. --lib: the dev suite is not gated for this combination (item 95).',
  },
```
- `ci.yml` job `clippy` (`:1275+`), в том же порядке:
```yaml
      - name: clippy (--features "numa-aware internals")
        run: cargo clippy --lib --features "numa-aware internals" -- -D warnings
```
- `tests/ci_clippy_matrix_consistency.rs` менять не нужно (поддерживает `libOnly`); он и есть страж согласованности. Обновить «6 clippy rows» в комментариях `ci.yml:1407–1408`, `check-all.mjs:38/415`, CLAUDE.md «all six CI clippy rows».
- Контроль: локально/в PR — красный при возврате гейта `alloc-xthread` на `alloc_core_large.rs:15`, зелёный на базе. Закрыть item 107 (перенос в RESOLVED по правилу R34-24).

**Коммит 5 — `ci: weekly MSRV each-feature and policy depth-3 sweeps`**
- В job `feature-powerset` после корневого шага:
```yaml
      - name: cargo hack check --depth 3 (allocation policy features)
        if: ${{ !cancelled() }}
        run: >-
          cargo hack check --feature-powerset --depth 3 --no-dev-deps --keep-going
          --include-features fastbin,medium-classes,medium-classes-wide,exact-span-large,large-reserved-capacity,numa-aware,hardened,alloc-decommit,alloc-segment-directory,virgin-zero-skip
      - uses: dtolnay/rust-toolchain@1.93
      - name: MSRV 1.93 cargo hack check --each-feature
        if: ${{ !cancelled() }}
        run: cargo +1.93 hack check --each-feature --no-dev-deps --keep-going
```
  (Порядок: установка 1.93 после stable не меняет `+stable`-вызовы, но шаги выше должны явно идти до неё или использовать `+stable` — проверить, что `dtolnay/rust-toolchain@1.93` делает 1.93 дефолтом.)
- Контроль 5c (требование 195b): временный коммит на ветке с post-1.93 API под `#[cfg(not(feature = "fastbin"))]` в любом модуле `alloc-core` → шаг MSRV красный; без него — зелёный. Только dispatch с разрешения владельца.
- Если 4c/5c покажут красные комбинации — каждую в индекс (новая карточка или под-пункт 195), не «чинить молча».

**Коммит 6 — `docs: cross-reference NUMA overrides and repair manifest file references`**
- `Cargo.toml:772–800` (doc `primordial-lazy-commit`) и `:335–392` (`large-reserved-capacity`): строка «Overridden by `numa-aware` (eager NUMA reservation); see `numa-aware`».
- `src/alloc_core/small/alloc_core_small/find_segment.rs:210–213`: заменить утверждение «compiled out entirely» на фактическое (блок `:271` гейтован только `alloc-segment-directory`; под NUMA он активен с R11-6) — comment-only.
- `Cargo.toml`: 27 мёртвых ссылок (§2.C) → текущие пути/символы; «the 1024 cap» (`:306`) → `MAX_SEGMENTS`.
- `tests/no_stale_doc_references.rs:877`: добавить `"docs/INTEGRATION.md"` в `sites` (контроль: вернуть 4-фичевый состав в INTEGRATION → тест красный).

**Коммит 7 — `ci: assert the experimental 64-bit-atomic diagnostic on a 32-bit target`** (item 198)
```yaml
      - name: experimental requires 64-bit atomics (powerpc-unknown-linux-gnu)
        if: ${{ !cancelled() }}
        shell: bash
        run: |
          set -euo pipefail
          rustup target add powerpc-unknown-linux-gnu
          cfg=$(rustc --print cfg --target powerpc-unknown-linux-gnu)
          grep -qx 'target_has_atomic="ptr"' <<<"$cfg"
          ! grep -qx 'target_has_atomic="64"' <<<"$cfg"
          cargo check --target powerpc-unknown-linux-gnu --no-default-features --features std
          if cargo check --target powerpc-unknown-linux-gnu --no-default-features --features experimental 2>err.txt; then
            echo "expected compile_error"; exit 1; fi
          grep -F 'requires 64-bit atomics' err.txt
```
  Место — weekly job `feature-powerset` (или per-PR `multi-arch`, если владелец хочет per-PR). Закрывает 198 после первого зелёного прогона.

## §6 Что нельзя решить без исполнения / вопросы владельцу

Не исполнялось (запрет владельца на cargo/node/workflow):
1. **Точный стек утечки.** В логе кадры без имён (§2.B). Нужен символизированный прогон: `workflow_dispatch` с `ASAN_OPTIONS=symbolize=1:allow_addr2line=1` (или `ASAN_SYMBOLIZER_PATH`) — подтвердит/опровергнет, что 128 B = `RouteSlots::ensure_capacity`. Какой символизатор есть на `ubuntu-latest` (`llvm-symbolizer` в PATH отсутствует — иначе кадры были бы с именами; версионные `llvm-symbolizer-NN`, `addr2line` из binutils, или компонент rustup) — не проверено.
2. **Воспроизведение «пустого входа».** Гипотеза: `cargo fuzz run heap_core_ops crash-da39…` будет зелёным (пустой вход не материализует heap). Проверка опровергает/подтверждает трактовку «exit-time check».
3. **Состояние powerset на базе.** Сколько из 390 комбинаций красны сейчас (хвост 206–390 не исполнялся с `90e7855`), чисты ли `--include-features`-тройки (4c) и `--each-feature` на 1.93 (5c) — только прогоном.
4. **Формат итоговой сводки `cargo-hack --keep-going`** (печатает ли он список упавших в конце) — по памяти «да», не проверено; скрипт сводки в плане от этого не зависит (парсит строки `running …`).
5. **Флаги cargo-hack** `--include-features`, `--each-feature` — семантика по документации cargo-hack (по памяти); проверить `cargo hack check --help` и `--print-command-list` (число команд) до коммита 5.
6. **`clippy --lib --features "numa-aware internals"` чистота** — R18 (карточка 107) исполнял `cargo clippy --features "numa-aware internals" -- -D warnings`; `--lib` — подмножество, должно быть чисто, но не перепроверено.
7. **Компилируются ли зависимости `experimental` на `powerpc-unknown-linux-gnu`** (`arc-swap`, `crossbeam-epoch`) — от этого зависит, увидит ли оракул 7d именно текст `compile_error!` (иначе другой текст и красный шаг — тоже сигнал, но не тот).
8. **Гипотеза API-04(c)** (устаревший отрицательный бит + отказ ОС → null под standalone NUMA) — нужна инъекция отказа под `alloc-core alloc-segment-directory numa-aware internals bench-internals`.

Вопросы владельцу:
- (В1) Разрешаете ли расширить права одного job до `issues: write` (вариант 2d) вопреки формулировке SEC-4 (`ci.yml:19–23`)? Без этого «уведомление» = только правило процесса (2b).
- (В2) Разрешаете ли `workflow_dispatch` для контрольных прогонов (коммиты 1, 2, 5) — до/после merge и на какой ветке (контрольные мутанты — только на временной ветке)?
- (В3) Q3: точная комбинация item 107 `numa-aware internals` (рекомендовано) или шире `alloc-decommit numa-aware internals` (ловит и сайт `:481`, но не закрывает 107 буквально)?
- (В4) Q6: подавление LSan (6d) приемлемо как постоянное решение, или вы хотите продуктовый путь 6e (`__lsan_register_root_region` под `cfg(fuzzing)`), который снимает проблему и для downstream?
- (В5) Политика API-04(a): оставить первичный сегмент eager под NUMA (сейчас задокументировано) или снять `not(numa-aware)` с гейта primordial lazy-commit (`bootstrap.rs`), как предлагал R17 (выигрыш только RSS на Windows, требует перф-гейта по правилам R26-4/R30-8)?
- (В6) Нужна ли per-PR тестовая строка `production medium-classes large-reserved-capacity internals --test r14_4_promotion_move_leg_reduction` (4d) помимо weekly 4c?

## §7 Проверено и НЕ дало вывода

- Поиск конкретной новой ошибки компиляции в непроверенном хвосте powerset чтением `alloc_core_large.rs` (все `Segment`/`SegmentMeta`/`numa::`-сайты против гейтов импорта) и новых `internals`-гейтов `f384eb0f` (`epoch_region.rs:656`, `sharded_region.rs:719,733`, `segment_layout.rs`, `bootstrap/{ensure,mod}.rs`) — несоответствий не найдено; это не доказывает зелёность (объём правок §2.A п.8).
- `find_segment.rs` (583 строки правок с `90e7855`) — просмотрен только план гейтов (`grep cfg`), без построчной сверки use/def; вывода нет.
- Другие 128-байтные кандидаты: `BlockIndex` ёмкости 16 (16 × 8 B) — достижим из `static ROUTES`, поэтому не «direct leak»; `Entry` (72 B), mixed-лист (256 B), `Block` (8 + 64 × 8 B) — размеры не совпадают. Однозначно исключить без символизации нельзя.
- Влияние `--no-dev-deps` на feature-унификацию — обратных зависимостей dev-крейтов на `sefer-alloc` нет; влияния на lib-проверку не найдено (не «не проверено», а «нет механизма»).
- Глобальный SHA и состояние: `git rev-parse HEAD` = `31db7f17698459e9ce26c3f8701c314304b9c00b` (checkout не потребовался).
