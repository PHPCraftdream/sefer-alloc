# numa-shim — аудит готовности к публикации, прогон 21

- **Автор:** GLM
- **Время начала:** 2026-08-25 13:45:27 Europe/Berlin (UTC+02:00); финализировано 13:58 после учёта параллельно упавших коммитов (см. Часть 1)
- **Ревизия:** `378e49350a386d27e16e77324498302b81c8d81e` (локальный `main`; `origin/main` = `d44b7e8`, на 4 коммита позади). Аудит начат на `6f8b421` — все изменения с тех пор лично перечитаны в diff.
- **Последний чужой прогон:** `ba98bc3` — прогон 20 Sol-codex (item 117), plain GO; до него прогон 20 fm (item 116), CONDITIONAL GO
- **Режим:** статический аудит с нуля, один агент, без под-агентов; по прямому запрету заказчика ничего исполняемое не запускалось — ни тесты, ни сборка, ни `cargo check`/clippy/miri/bench/rustdoc/publish. Источники: чтение исходников, read-only `git log`/`git show`/`git diff`/`git ls-files`, один read-only запрос `gh run list` (статус CI, без запуска чего-либо).

## Вердикт

**GO по исходному коду и публикационной поверхности.** Третий подряд не-NO-GO вердикт кампании (item 115 — CONDITIONAL GO, item 116 — CONDITIONAL GO, item 117 — GO; настоящий отчёт — второй plain GO подряд). Новых P0/P1/P2 не найдено: ни UB, ни нарушения strict provenance, ни ABI/FFI-расхождений, ни double free / утечки владения, ни ошибок в порядке захвата errno/GetLastError, ни регрессий в platform dispatch, ни doc-vs-behavior лжи.

Статический GO — не заявление, что release gate исполнен: по прямому запрету заказчика в этом прогоне не запускалось НИЧЕГО исполняемое. Ни один из нижеперечисленных P3 не блокирует публикацию.

## Покрытие исследования

С нуля, без опоры на память о предыдущих прогонах, прочитано:

- `crates/numa-shim/Cargo.toml` (96 строк), `README.md` (263), `CHANGELOG.md` (378 после #1356);
- весь `src/lib.rs` (2346 строк) построчно, включая все пять `mod platform` блоков (Linux real, Windows real, macOS, miri, fallback), mock-модуль, `cpumap`/`eintr`/`linux` doc-hidden модули и оба crate-root mbind FFI-хелпера;
- все десять файлов `tests/` (cpumap_parser, cpumap_reverse_index, eintr_retry, mock_dispatch, node_id, node_resolution, node_resolution_linux, policy_oracle_linux, readme_examples, smoke);
- `benches/numa_bench.rs` и корневой `bench-iters.txt`;
- все numa-shim-ячейки `.github/workflows/ci.yml` (включая добавленные задачей #1356 уже ПОСЛЕ начала аудита — см. Часть 1): `numa-shim-mock` (Linux: real, mock, mock+vmem, real+vmem c `NUMA_SHIM_REQUIRE_ORACLE=1`, clippy real+mock, rustdoc all-features/docs.rs-derived/default/mock-cfg), `numa-shim-windows` (real, real+vmem, mock, mock+vmem, clippy default/all-features/mock), `numa-shim-macos` (real, real+vmem, mock, mock+vmem), `numa-shim-macos-miri` (real, real+vmem, mock, mock+vmem — новая), MSRV-строки job'а `msrv` (включая mock-arm от task #1299/T12), корневой mock-row;
- safety-контракт `aligned_vmem::Reservation::from_raw_parts`, сквозь который Windows-бэкенд передаёт владение;
- наличие всех CHANGELOG-ссылок на документы (`NUMA_RELEASE_GATE.md`, waiver Phase 2/4, bind-range-контракт, `NUMA_TESTING_OPTIONS.md`) — все четыре файла существуют;
- LICENSE-MIT / LICENSE-APACHE на месте.

Проверка велась по категориям rust-intel: unsafe/FFI, ownership/Drop, целочисленные границы, platform cfg/features, error/cleanup-пути, semantic conformance (ABI-значения констант против UAPI), качество тестовых оракулов, упаковка (package surface), performance-at-scale. Async, продакшен-конкурентность, crypto, network-parsing к крейту неприменимы.

## Часть 1 — дельта после прогона 20 Sol-codex (`d44b7e8..378e493`)

Четыре коммита; **ни один не меняет runtime-код крейта** (лично проверено по `git show --stat` и полным diff'ам):

1. `6f8b421` (task #1355, 13:25) — исполнение P3-4 прогона 20: заголовок `### Owner decisions pending` → `### Resolved owner decisions` в CHANGELOG, 1 строка, docs-only. Лично перечитаны обе записи — обе говорят DECISION MADE/DECIDED, противоречие снято.
2. `5ec5806` (task #1356, 13:44) + merge `a3fbcd2` — исполнение P3-5 прогона 20: новая miri-строка mock+`vmem-integration` в `numa-shim-macos-miri` (те же три sentinel'а, что у не-miri двойника: `reserve_preferred_on_node_returns_valid_span`/`_large_align_round_trip`/`_rejects_zero_size_with_invalid_arguments` — имена сверены мной с реальными тестами в `tests/smoke.rs`) и новый rustdoc-row `RUSTDOCFLAGS="--cfg numa_shim_mock -D warnings" cargo doc` (cfg обязан ехать в RUSTDOCFLAGS: cargo не форвардит RUSTFLAGS в rustdoc — green-and-dead-ловушка, задокументированная в самом коммите с контрфактической проверкой). Затронуты только `ci.yml` (+48/−2) и CHANGELOG (+2). Комментарий «stays uncovered» у не-miri двойника синхронно обновлён.
3. `378e493` — in-place обновление карточки item 117: disposition P3-5 «EXECUTED by task #1356».

**Конкурентный процесс упал в репозиторий ВОВРЕМЯ моего аудита** (я прочитал карточку item 117 до `378e493` — тогда она ещё не знала о #1356; перечитал после — уже знает). Это уже знакомый кампании concurrent-process паттерн (items 110/112/113/115/117). Все три конкурентных коммита мной перечитаны целиком и учтены ниже.

Состояние CI (наблюдено, read-only `gh run list` на момент ~13:45): прогон CI на текущем `origin/main` (`d44b7e8`) — **in_progress**; предыдущий head волны (`774716e`) — completed/success. Локальный `main` на 4 коммита впереди origin — подтверждение CI-зелёности на landing SHA остаётся открытым шагом процесса (см. «Что перед релизом»).

## Часть 2 — общий обзор крейта с нуля

### 2.1 Что проверено независимо и найдено корректным

**Linux FFI.** `SYS_MBIND` = 237 (x86_64) / 235 (aarch64) — сверено с таблицами ядра; `maxnode = 65` корректно компенсирует декремент `get_nodes()` (quirk libnuma, task #697), бит 63 не теряется (вызовители валидируют node < 64 ДО сдвига); nodemask — живая stack `u64` на время syscall; errno захватывается НЕПОСРЕДСТВЕННО на каждом падающем сайте до любого cleanup-FFI (open, read ×2, mbind) — контракт task #1306 выдержан по всем путям; при неудачном mbind успешно созданная reservation освобождается через `drop(r)` ДО возврата исходной ошибки — полу-связанной reservation утечь не может.

**Читатель cpumap.** `read_cpumap_into` закрывает fd ровно один раз на каждом пути (переполнение буфера, ошибка чтения, EOF); bounded-EINTR (лимит 16 подряд, сброс на прогрессе) повторяет open/read идентично семантике POSIX (EINTR-open не создал fd, EINTR-read перенёс 0 байт); файл шириной буфера (включая ровно `out.len()` — doc-уточнение task #1353 проверено против кода: guard `total >= out.len()` срабатывает до EOF-проба) отвергается fail-closed; `O_CLOEXEC` cfg-расщеплён корректно (asm-generic `0o2000000` везде, sparc/sparc64 `0x400000` — сверено по значению, применено task #1345).

**Топология.** Инициализатор `OnceLock` аллокационно-свободен (8 KiB ReverseIndex + 4 KiB scratch на стеке, ~12 KiB задокументировано task #1340 — reentrancy-опасность heap-оси устранена структурно, stack-ось честно названа); init-BEFORE-snapshot порядок (task #1331) на месте в обеих функциях; `parse_each_set_cpu` — один линейный проход `rsplit` (task #1334), порядок слов сверён против оракула `cpu_32_is_bit_0_of_the_leftmost_word`; `parse_hex_u32` отвергает токены длиннее 8 цифр; `MAX_INDEXED_CPUS = 8192` покрывает NR_CPUS обоих поддерживаемых архов; деградация за границей ёмкости задокументирована и оракулена.

**Windows FFI.** `PROCESSOR_NUMBER` закреплён const-assert'ами размера/выравнивания/офсетов; `GetNumaProcessorNodeEx` различает BOOL-провал, MAXUSHORT-sentinel и настоящий node 0; двух-вызовный reserve-then-commit (task #724) с корректно описанной механикой `nndPreferred` (node нагрузоносен на MEM_RESERVE, инертен на MEM_COMMIT — комментарий соответствует контракту Microsoft); strict provenance через `addr()`/`with_addr()`; checked-арифметика переполнения при выводе aligned-base (с освобождением reservation при переполнении); GetLastError до cleanup; неожиданный базис commit'а проверяется в release-сборке (task #1304); `from_raw_parts(base, size, raw, over, align, false)` удовлетворяет контракту siblings-крейта; 32-bit Windows отвергается `compile_error!` (task #1321).

**API/упаковка.** Ни одного `pub unsafe fn`; `#![deny(missing_docs)]` выдержан; `NodeId` valid-by-construction ровно к одному сантинелу (и doc прямо запрещает читать больше валидации, чем есть); `#[non_exhaustive]` там, где нужна эволюция; три doc-hidden модуля объявлены semver-exempt; default features пуст, зависимостей в default-сборке ноль; `unexpected_cfgs` объявлен в `[lints.rust]` с check-cfg на `numa_shim_mock`; MSRV 1.88 совместим с использованием `u64::is_multiple_of` (стабилизирован 1.87); keywords ≤ 5, категории валидны, license-файлы на месте, docs.rs-фичасет = единственная opt-in фича.

**Mock.** Активация только через build-time cfg (не фича — транзитивная активация невозможна); запись capped (`CALLS_CAP`, `Cell<u32>` для слота — паник-невозможна), reentrancy-safe (`try_with`/`try_borrow_mut`); one-shot policy-failure с возвращением «чужого» узла в слот проверен чтением; порядок записей reserve → InstallPolicy → (drop) → PolicyFailureRelease соответствует контракту «release доказан записью ПОСЛЕ Drop».

**Тесты/CI.** Десять файлов с контрфактическими оракулами там, где фикс был (e.g. `current_node_scripted_no_node_yields_none`, `interrupted_error_with_fresh_streak_is_retried` — с честным доказательством нетавтологичности); матрица real/mock × Linux/Windows/macOS/miri закрыта (после #1356 — включая последнюю miri×mock×vmem ячейку); mock-строки снабжены green-and-dead sentinel'ами (task #1299/T8, #1338, #1356); флаговая real-Linux строка `NUMA_SHIM_REQUIRE_ORACLE=1` с тремя grep-sentinel'ами; errno-классификация оракула политики (allowlist ENOMEM/EPERM-skip, остальное panic; preflight и negative control — EPERM/ENOSYS-skip) согласована по всем трём сайтам после task #1336/#1347; MSRV покрывает mock-arm. TODO/FIXME/`unimplemented!` в крейте отсутствуют (grep — пусто).

### 2.2 Findings

Новых P0/P1/P2 нет. Ниже — один новый P3 (процессный), один новый P4, один P3 подтверждён открытым, один P3 закрыт конкурентно (проверен мной), два отслеженных остатка без изменений.

#### F1 (P3, НОВЫЙ, процессный) — карточка item 117 противоречит сама себе после конкурентного обновления: P3-4 так и не отмечен, Status-строка лжёт уже о двух задачах

**Места:** `docs/correctness-open-items/TRACKED_publish_readiness.md`, карточка item 117 (строки ~702-711).

Карточка item 117 (подана 13:16 в `d44b7e8`) гласит в Status-строке: **«OPEN — five P3s, all non-blocking …; none filed as tasks yet»**. С тех пор исполнены уже ДВА из пяти: P3-4 — task #1355 (`6f8b421`, 13:25), P3-5 — task #1356 (`5ec5806`/`a3fbcd2`, 13:44). Конкурентное обновление `378e493` добавило disposition-заметку ТОЛЬКО к P3-5; в результате карточка теперь содержит внутреннее противоречие ровно того класса, который task #1351 закрывал у item 114: Status-строка говорит «задачи не поданы», а собственная P3-5-строка карточки говорит «EXECUTED by task #1356» в шести строках ниже. При этом P3-4-строка по-прежнему не знает о task #1355 (grep трекера на «1355» — ноль совпадений; проверено), хотя заголовок CHANGELOG уже переименован. Это седьмое появление stale-index-card класса (N9 → E6 → T5/T6 → U2/U3/U4 → Status-строка item 103 → три строки item 114), и первое, где рассинхрон создан самим актом ЧАСТИЧНОГО обновления. По правилу CLAUDE.md «OPEN_ITEMS indexes are CURRENT-STATE» нужны две вещи: in-place заметка «CLOSED by task #1355 (`6f8b421`)» на строке P3-4 и синхронизация Status-строки (например: «P3-4 closed by #1355, P3-5 closed by #1356; P3-1/P3-2/P3-3 open»). Двухстрочная правка, не блокирует публикацию (внутренний трекер, не поставляемый артефакт).

#### F2 (P3, подтверждён открытым — P3-1 прогона 20, bench-гигиена) — четыре элемента лично перепроверены

1. `bench-iters.txt:7-9` всё ещё содержит три мёртвых ID `numa_bench::bind_range/{no_node_noop,page_to_node_7,zero_len_noop}` — API удалён task #1306.
2. Существующий workload `numa_bench::reserve_preferred_on_node/invalid_node_error` (`benches/numa_bench.rs:90`) в манифесте ОТСУТСТВУЕТ (grep — ноль) — при первом полном прогоне он JIT-калибруется.
3. Inline-комментарий `benches/numa_bench.rs:62-64` («the same cost the real backend pays after its cache is populated») по-прежнему приравнивает mock-чтение слота к реальному warm-пути — ложно: реальный warm-путь = `sched_getcpu` + `OnceLock`-чтение + O(1)-probe, mock = TLS-чтение + запись в capped `Vec`.
4. Порядочная/фильтровая зависимость никуда не делась: `first_call` в полном прогоне стартует с пустым журналом `CALLS` (платит рост `Vec` до cap), `warm_call` — обычно с заполненным (платит только проверку длины); прогон по фильтру даёт третий набор состояний.

Ни одно из чисел `current_node/*` сегодня нельзя использовать даже как грубый сигнал cold/warm. Рекомендация прежняя: нормализация recording-состояния перед каждым workload (drain + prefill до cap либо отдельный no-record seam), удалить мёртвые ID, откалибровать манифест. Лучший кандидат на «первую полезную правку после публикации».

#### F3 (P3, отслеженный остаток, без изменений — P3-2 прогона 20) — README-оракул остаётся ручной транскрипцией

`tests/readme_examples.rs:100-104` честно признаёт: компилятор проверяет КОПИЮ примера, а не сам fenced-блок README; правка README без синхронной правки копии молча перестаёт его охранять. Пост-релизный housekeeping; не блокирует.

#### F4 (P3, отслеженный остаток, без изменений — P3-3 прогона 20) — race-окно release-оракула

`tests/smoke.rs` release-оракул (Windows `VirtualQuery` / Linux `/proc/self/maps` после `drop(r)`): комментарии после task #1348 точны и самосогласованы (лично перечитаны оба близнеца), структурная гонка с параллельным маппером в том же процессе остаётся; наблюдавшихся flake нет. Не ослаблять assertion при срабатывании; изоляция в отдельный процесс — если когда-нибудь проявится. Не блокирует.

#### F5 (P3 прогона 20 — ИСПОЛНЕН конкурентно задачей #1356; мной проверен diff, остался один residual) — две cfg-ячейки без CI-сигнала

P3-5 больше не открыт: task #1356 добавил miri-строку mock+vmem (три sentinel'а, сверены мной с именами тестов в `tests/smoke.rs`) и mock-rustdoc-строку с cfg в `RUSTDOCFLAGS` (ловушка «cargo не форвардит RUSTFLAGS в rustdoc → green-and-dead» вскрыта и контрфактически проверена самим #1356 — читаю его diff и commit message, утверждения соответствуют коду). **Residual:** обе новые строки ещё ни разу не исполнялись в CI — коммиты не запушены (origin на 4 позади), а реальный macOS+miri на этой Windows-машине невоспроизводим (честно признано в #1356). Первый зелёный прогон CI на landing SHA закроет и это. Не блокирует.

#### F6 (P4, НОВЫЙ, doc-нит) — README-таблица платформ опускает строку «Linux other arch», которую имеет crate-doc матрица

`README.md:10-15` перечисляет Linux (x86_64/aarch64), Windows 64-bit, macOS, miri; матрица в `src/lib.rs:51-60` дополнительно несёт строку «Linux other arch (non-miri): detection работает (sched_getcpu + sysfs), reservation → UnsupportedArchitecture». README нигде не говорит, что ДЕТЕКЦИЯ архитектурно-независима на Linux — читатель таблицы вправе заключить, что на riscv64/s390x `current_node()` не работает, хотя это не так (task #1346 сам строил исправление именно на этом свойстве). Раздел «Linux syscall numbers» покрывает только reservation-половину. Одна строка в таблицу README выровняет его с crate-doc. Косметика.

### 2.3 Производительность («что ускорить?»)

Новых доказанных speedup-кандидатов этот статический аудит не заявляет (ничего не измерялось — по запрету заказчика):

1. Warm-путь `current_node()` уже минимален по форме: `OnceLock`-чтение + `sched_getcpu()` + O(1) probe. Убрать per-call `sched_getcpu` нельзя без смены семантики при миграции потока — это корректность, не накладной расход.
2. Cold-путь (до 64 open/read/close + двойной парсинг на узел при fail-closed индексации) — цена одного раза на процесс; отслежено `docs/perf/OPEN_ITEMS.md` item 59 (measure-first).
3. Windows one-call `MEM_RESERVE | MEM_COMMIT` при малых align — item 60; без A/B-измерения правильность-first путь не менять (позиция крейта корректна).
4. Сами текущие bench-числа (mock) до закрытия F2 нельзя использовать для тонких сравнений — см. F2.

## Итоговая рекомендация

**Публикацию исходного кода не задерживать.** Runtime и публичные контракты готовы; из пяти P3 прогона 20 уже два закрыты (#1355, #1356), оставшиеся — пост-релизный housekeeping; единственный новый P3 — процессная рассинхронизация трекера (F1).

Перед релизом (процесс, не код):

1. In-place обновить карточку item 117: заметка «CLOSED by task #1355 (`6f8b421`)» на строке P3-4 и синхронизация Status-строки с уже имеющейся P3-5-заметкой (сейчас карточка противоречит сама себе) — F1.
2. Запушить 4 незапушенных коммита (`6f8b421`..`378e493`; origin = `d44b7e8`) и подтвердить CI-зелёность на landing SHA — это же даст первый реальный прогон двум новым строкам #1356 (F5-residual). На момент аудита CI для `d44b7e8` ещё in_progress (наблюдено).
3. Принять решение о номере версии: CHANGELOG «Removed» фиксирует четыре breaking-изменения против 0.1.0 → следующий релиз не может быть 0.1.1 (открытый F1-owner-decision, task #1262).
4. Release-gate closure review — статический GO двух прогонов подряд не заменяет этот шаг; waiver на Phase 2/4 остаётся записанным owner-риско-принятием.

После публикации: F2 (первоочередной), затем F3/F6 по вкусу; F4 — только если flake проявится.
