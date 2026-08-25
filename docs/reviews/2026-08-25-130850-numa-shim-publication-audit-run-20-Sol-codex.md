# `numa-shim` — аудит готовности к публикации, прогон 20

- Автор: Sol-codex
- Время: 2026-08-25 13:08:50 Europe/Berlin (UTC+02:00)
- Ревизия: `ba98bc375b914e5fbe0ad53022f228c5e3702970`
- Последний собственный прогон: ревизия `4a03fd37c921ee2fed5537f2caf36f4eec78d372`, отчёт прогона 19
- Режим: статический аудит с нуля, один агент, без под-агентов
- Исполнение: тесты, сборка, `cargo check`, Clippy, Miri, benchmarks, rustdoc и package/publish-команды не запускались

## Вердикт

**GO по исходному коду и публикационной поверхности.** На просмотренной ревизии не найдено P0/P1/P2, UB, нарушения provenance, ABI/FFI-контрактов, double free, утечки владения, ошибочного errno/GetLastError timing или новой регрессии в platform dispatch.

Последние правки после прогона 19 не меняют runtime-поведение: это синхронизация readiness-документа, уточнение CI-комментария с уже полученным Windows receipt и исправление границы в rustdoc `read_cpumap_into`. Исправление формулировки `as wide as or wider than out` точно соответствует существующему fail-closed коду.

Статический GO не следует читать как утверждение, что я исполнил release gate: по прямому запрету заказчика ничего исполняемого в этом исследовании не запускалось. Ни одно из найденных ниже P3 не является причиной задерживать публикацию.

## Покрытие исследования

С нуля прочитаны все файлы крейта:

- `Cargo.toml`, `README.md`, `CHANGELOG.md`;
- весь `src/lib.rs` (2346 строк), включая все `cfg`-ветви;
- все десять integration-test файлов;
- `benches/numa_bench.rs` и связанный с ним `bench-iters.txt`;
- все относящиеся к крейту CI-ячейки: Linux real/mock, Windows real/mock, macOS real/mock, macOS+miri, MSRV, Clippy и rustdoc;
- safety-контракт `aligned_vmem::Reservation::from_raw_parts`, которому Windows backend передаёт владение.

Проверка выполнена по категориям `rust-intel`: unsafe/FFI, ownership/Drop, integer boundaries, platform cfg/features, error and cleanup paths, semantic conformance, test-oracle quality, package surface и performance-at-scale. Async, concurrency primitives в production, crypto, network/protocol parsing и secrets к этому крейту неприменимы.

## Обзор последних коммитов

Диапазон после прогона 19 (`1df1b2c..ba98bc3`) не содержит изменений runtime-логики `numa-shim`.

1. `82bb2ab` синхронизировал противоречивые строки item 114. Замечание P2 прогона 19 закрыто корректно.
2. `bda0c87` заменил устаревшее «UNVERIFIED» в CI-комментарии фактическим receipt успешного `tee`/`grep` на Windows runner. Команды CI не менялись.
3. `e47f605` уточнил документацию `read_cpumap_into`: код отклоняет файл длиной ровно `out.len()` до EOF-probe, поэтому «as wide as or wider» — точное описание. Поведение не менялось.
4. Остальные коммиты добавляют/индексируют отчёты и обновляют их disposition.

## Findings

### P3-1 — benchmark зависит от скрытого состояния журнала и порядка workloads

**Места:** `crates/numa-shim/benches/numa_bench.rs:18-26,61-78`; `crates/numa-shim/src/lib.rs:300,401,424-425,525-534`; `bench-iters.txt:7-11`.

`current_node()` под mock не только читает scripted slot, но и записывает `MockCall` в thread-local `Vec` до `CALLS_CAP = 4096`. Harness исполняет зарегистрированные workloads последовательно. Поэтому полный запуск начинает `current_node/first_call` с пустым журналом, в первых вызовах платит за `Vec::push`/рост capacity, заполняет журнал до cap, а `current_node/warm_call` обычно начинает уже с заполненным журналом и платит только за проверку длины. При запуске через фильтр `warm_call` его начальное состояние снова будет другим.

Это делает результаты зависимыми от порядка, фильтра и масштаба запуска. Название `first_call` также не измеряет первый production-вызов: module docs честно говорят об этом, но inline-комментарий на строках 63-64 ошибочно называет mock-read overhead «the same cost the real backend pays after its cache is populated». Реальный warm path выполняет как минимум platform CPU sample, `OnceLock` access и reverse-index lookup; mock path — TLS access и recording.

Дополнительно `bench-iters.txt` всё ещё содержит три удалённых `bind_range/*` workload и не содержит нового `reserve_preferred_on_node/invalid_node_error`. Значит, новый workload JIT-калибруется при первом полном запуске, а старые записи продолжают загрязнять manifest.

**Рекомендация:** перед каждым workload явно нормализовать recording state (`drain` и затем либо отдельный измеряемый режим записи, либо prefill до cap); лучше добавить узкую benchmark-only no-record seam и отдельно измерять recording. Удалить stale IDs и откалибровать текущий набор. До этого числа допустимо использовать только как грубый сигнал mock-dispatch, не как сравнение cold/warm production path.

### P3-2 — compile oracle README остаётся ручной копией

**Место:** `crates/numa-shim/tests/readme_examples.rs:100-104`.

Тест компилирует транскрипцию примера, а не сам fenced block README. Его собственный комментарий точно признаёт дефект: изменение README без синхронного изменения копии silently stops guarding it. Добавленный CI sentinel доказывает, что копия исполняется, но не доказывает её равенство исходному тексту.

**Рекомендация:** оставить один источник истины — извлекать именованный fenced block из README в drift guard либо генерировать README-фрагмент из компилируемого примера. До этого при изменении примеров нужен обязательный ручной diff обоих мест.

### P3-3 — release oracle имеет реальное окно ложного падения

**Места:** `crates/numa-shim/tests/smoke.rs:476-497,523-549`.

После `drop(r)` тест проверяет, что адреса свободны, через `VirtualQuery` или `/proc/self/maps`. Параллельный mapper в том же процессе может занять только что освобождённый диапазон между drop и probe/read. Комментарии теперь честно и подробно описывают эту гонку; это не скрытый production bug. Однако оракул остаётся потенциально flaky и при редком collision сообщает симптом, неотличимый от неполного release.

**Рекомендация:** если такая flake проявится, не ослаблять assertion. Изолировать release oracle в отдельный test binary/process или применить платформенный механизм, исключающий повторное занятие диапазона на время проверки. Пока наблюдаемой flake нет, это неблокирующий test-infrastructure risk.

### P3-4 — заголовок CHANGELOG противоречит содержимому

**Место:** `crates/numa-shim/CHANGELOG.md:36-55`.

Раздел называется `Owner decisions pending`, но обе его записи уже говорят `DECISION MADE`/`DECIDED`. Техническое содержание верно, однако публикационный документ создаёт ложное впечатление незакрытых решений.

**Рекомендация:** переименовать раздел в `Resolved owner decisions` либо перенести записи в `Changed`. Это чистая редактура и не блокирует публикацию.

### P3-5 — две известные cfg-ячейки остаются без отдельного сигнала

**Места:** `.github/workflows/ci.yml:2910-2911,2950-2962`; `crates/numa-shim/CHANGELOG.md:172`.

CI прямо фиксирует отсутствие комбинации macOS+miri+`numa_shim_mock`+`vmem-integration`: miri job проверяет real/default, real/feature и mock/default, но не mock/feature. Также rustdoc не строится с `--cfg numa_shim_mock`, поэтому intra-doc links публичного mock seam не имеют отдельного gate. Обе поверхности test-only и не попадают в docs.rs/default consumer build; production matrix покрыта существенно лучше.

**Рекомендация:** добавить комбинацию mock+feature в существующий miri job и отдельный mock rustdoc row только если стоимость CI приемлема. Это hardening, не release blocker.

## Unsafe, FFI и владение

### Linux detection

- Топология инициализируется до CPU snapshot, поэтому долгий cold sysfs scan больше не расширяет окно migration между snapshot и lookup.
- `ReverseIndex` строится в `OnceLock` без heap allocation; malformed/oversized/missing cpumap fail closed.
- `open/read` корректно обрабатывают bounded consecutive `EINTR`; errno читается до `close`.
- fd закрывается ровно один раз на каждом пути после успешного `open`; `O_CLOEXEC` имеет отдельное корректное значение для sparc/sparc64.
- Warm lookup — O(1), но `sched_getcpu` остаётся на каждом вызове, что необходимо для корректности при миграции потока.

### Linux reservation policy

- Node 64+ отвергается до сдвига; node 63 корректно представим в одном `u64`.
- `maxnode = 65` соответствует kernel/libnuma quirk и не теряет bit 63.
- Policy применяется к полному OS reservation span до первого page fault.
- Ошибка `mbind` сохраняется до Drop/`munmap`; успешная reservation не утекает при policy failure.
- x86_64/aarch64 syscall numbers и ABI-формы согласованы с target gates; прочие Linux-архитектуры возвращают typed `UnsupportedArchitecture`.

### Windows

- 32-bit Windows отвергается compile-time gate; hand-written `PROCESSOR_NUMBER` закреплён size/alignment/offset assertions.
- Detection различает BOOL failure, `MAXUSHORT` sentinel и настоящий node 0.
- `size + align` и alignment rounding checked; pointer provenance сохраняется через `addr`/`with_addr`.
- Reserve/commit failures захватывают `GetLastError` до cleanup; unexpected commit base проверяется и в release build.
- В `Reservation::from_raw_parts` передаются согласованные base/size/raw/reservation_len/alignment; после передачи Drop владеет полным reservation ровно один раз.

### Stubs и mock

- macOS, miri и другие unsupported platforms возвращают явный typed error, а не silently unbound success.
- Mock activation не является Cargo feature и не может быть включена transitive dependency feature-unification.
- Recording bounded; reentrant `record` fail-soft, `CURRENT_NODE_SLOT` использует непаникующий `Cell<u32>`.

## API, документация и упаковка

- Default feature set пуст; default build не тянет crate dependencies.
- Единственная optional production dependency изолирована за `vmem-integration`; её ownership handoff проверен отдельно.
- `NodeId` valid-by-construction относительно `NO_NODE`; platform validity остаётся typed fallible operation.
- `ReserveNumaError` и `NodeResolution` non-exhaustive там, где требуется эволюция API.
- 32-bit Windows policy, platform table, feature gating и best-effort fallback в исходнике/README в основном согласованы.
- Manifest содержит license/readme/repository/homepage/documentation metadata; обе license files присутствуют; `build.rs` и proc-macro surface отсутствуют.
- docs.rs feature set явно включает headline reservation API, а CI source содержит rustdoc rows и для default, и для feature-on конфигураций.

## Производительность

Новых доказанных speedups этот статический аудит не заявляет.

1. Linux hot path уже сведен к CPU sample + `OnceLock` access + O(1) array lookup. Удалять CPU sample нельзя без изменения semantics при scheduler migration.
2. Cold init всё ещё делает до 64 `open/read/close` и дважды парсит каждый успешный cpumap для transactional fail-closed indexing. Это process-once стоимость; оптимизация оправдана только после реального cold-start измерения. Кандидат уже отслеживается в `docs/perf/OPEN_ITEMS.md` item 59.
3. Windows common alignment потенциально допускает one-call `MEM_RESERVE | MEM_COMMIT` через `VirtualAllocExNuma`, с обязательной runtime alignment check и fallback на текущий двухвызовный путь. Кандидат уже корректно описан как неизмеренный в item 60; correctness-first текущий путь менять без A/B не следует.
4. Текущий crate benchmark не может оценивать пункты 1-3, потому что намеренно заменяет platform backend mock-веткой; P3-1 выше нужно закрыть прежде, чем использовать даже mock numbers для тонких сравнений.

## Итоговая рекомендация

**Публикацию исходного кода не задерживать.** Runtime и публичные контракты выглядят готовыми; пять P3 — улучшения benchmark, документации и test/CI evidence, а не correctness blockers. Самая полезная ближайшая правка после публикации — нормализовать benchmark state и очистить `bench-iters.txt`, потому что сейчас названия и числа могут подтолкнуть к неверному performance-выводу.
