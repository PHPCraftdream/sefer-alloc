# Новое read-only исследование `numa-shim` перед публикацией

**Автор:** Сол-кодекс (`Sol-codex`)  
**Время:** 2026-08-24 17:00:47 Europe/Berlin (UTC+02:00)  
**Ревизия:** `8394108bba5c32e6aac8a1cd925814dea40c1bec`  
**Ветка:** `main`; на момент снимка `HEAD == origin/main`  
**База предыдущего исследования:** `58d94da`; основная новая реализация — `2ffc9fc`, CI-follow-up — `9c345de` и `8394108`  
**Режим:** только чтение исходников и внешнего состояния CI/crates.io; без под-агентов. Тесты, сборки, `cargo check`, Clippy, Miri, benchmarks и `cargo publish --dry-run` не запускались. Единственная запись в рабочее дерево — этот отчёт.

## Итоговый вердикт

**Нет, текущий SHA ещё не готов к публикации. Вердикт: `NO-GO`.**

Новая архитектура заметно лучше старой: сломанный `bind_range` удалён, мягкая политика названа `reserve_preferred_on_node`, ошибка `mbind` больше не теряется, errno/GetLastError захватывается до cleanup, reservation освобождается при ошибке политики, Linux node `>= 64` и неподдерживаемые платформы больше не маскируются под успех. В новых Linux/Windows FFI-ветках статически не найдено подтверждённого UB, UAF, double-free или ошибочного ownership transfer.

Однако до публикации остаются два разных класса препятствий:

1. **Механический release blocker:** crate всё ещё имеет уже занятую на crates.io версию `0.1.0`, CHANGELOG остаётся `Unreleased`, README и root dependency указывают `0.1`, а release waiver не привязан к SHA. Текущий workflow закономерно не пропустит реальную публикацию.
2. **Остаточный semantic blocker:** Linux detection по-прежнему превращает невозможность определить узел в `Some(0)`. Новый рекомендуемый путь сразу оборачивает это значение в `NodeId` и может успешно поставить политику на node 0, хотя поток выполняется на другом узле. Ошибки policy installation теперь честные, но исходный node всё ещё может быть неверным.

Дополнительно отсутствует независимый тестовый оракул того, что успешный `mbind(MPOL_PREFERRED)` действительно установил ожидаемую VMA policy, а обязательные реальные NUMA-gates не завершены. Поэтому даже после механического version bump я не считаю доказательную базу достаточной для безусловного `GO`.

## Сводка findings

| ID | Приоритет | Суть | Статус для релиза |
|---|---:|---|---|
| F1 | P1 | Linux detection ошибочно выдаёт node 0 при неизвестной топологии | Исправить до публикации |
| F2 | Release blocker | `0.1.0` уже опубликована; версия/CHANGELOG/README/root pin/waiver не подготовлены | Обязательно исправить |
| F3 | P1/P2 | Нет независимого оракула установленной NUMA policy; реальные NUMA-gates не завершены | Закрыть или явно принять риск |
| F4 | P2 | `NodeId` документирует инвариант `!= NO_NODE`, но тип его не обеспечивает | Исправить API до заморозки 0.2 |
| F5 | P2 | 1024-byte cpumap bound обоснован неверно: bitmap индексируется глобальными CPU IDs | Исправить вместе с F1/perf |
| F6 | P2 | Mock Linux-shaped и не умеет моделировать policy failure после reservation | Усилить тестовый seam |
| F7 | P2 docs | «Successfully bound» противоречит soft `MPOL_PREFERRED`; README слишком категоричен про existing ranges | Исправить до публикации |
| F8 | P2/P3 | Реальные smoke tests имеют хрупкие предпосылки и ложный leak oracle | Исправить тесты |
| F9 | P3 perf | Windows common path всегда делает 2 вызова и over-reserve, хотя возможен 1-call fast path | Желательно ускорить |
| F10 | P3 perf | Hot lookup Linux остаётся O(nodes × cpumap bytes), cache занимает ~64 KiB | Желательно переработать |
| F11 | P3 portability | 32-bit Windows не исключён, но тестовый FFI layout жёстко 64-bit и CI не покрывает i686 | Определить поддержку |
| F12 | P3 docs/API | README требует direct `aligned-vmem` для имени `Reservation`, хотя тип реэкспортирован | Исправить документацию |

## Findings подробно

### F1 — P1: `current_node()` может вернуть достоверно выглядящий, но неверный node 0

**Доказательство в коде:**

- `crates/numa-shim/src/lib.rs:903-906` возвращает `FellBackToZero`, если CPU не найден в cached sysfs topology.
- `crates/numa-shim/src/lib.rs:1073-1075` превращает тот же failure в `0`.
- `crates/numa-shim/src/lib.rs:488-500` честно документирует, что sysfs I/O failure, truncated map и реальный node `>= 64` неотличимы от genuine node 0.
- `crates/numa-shim/src/lib.rs:431-441` закрепляет отображение `FellBackToZero -> Some(0)`.
- Новый ergonomic example (`src/lib.rs:102-108`) и README (`README.md:77-85`) превращают результат в `NodeId` и вызывают reservation API.
- Реальный in-tree consumer делает то же: `src/alloc_core/numa.rs:34-35`, затем передаёт значение в `reserve_preferred_on_node` на `:76-86`.

**Сценарий отказа:** sysfs недоступен, чтение прервано, cpumap слишком широк, topology torn/stale или поток находится на node `>= 64`. `current_node()` возвращает `Some(0)`. Если node 0 разрешён cpuset, новый `mbind` может успешно вернуть 0, и библиотека выдаст `Ok(Reservation)`: fail-closed policy API сработал технически правильно, но политика установлена для неверного узла.

Это не просто диагностическая потеря: основная цель вызова — предпочесть память рядом с текущим CPU, а результат может систематически делать обратное.

**Лучшее исправление без сохранения обратной совместимости:**

- изменить `current_node()` на fail-closed mapping: `Resolved(n) -> Some(n)`, `FellBackToZero | Unavailable -> None`;
- убрать `FellBackToZero` либо переименовать в `TopologyUnavailable`/`CpuNotMapped`, не обещая node 0;
- не использовать `unwrap_or(0)` в README и smoke tests; отсутствие определения означает «не выражать предпочтение», а не «предпочесть node 0»;
- для Linux рассмотреть прямой `getcpu(2)`/vDSO node result как основной источник, оставив sysfs лишь fallback. Это одновременно устраняет большую часть F5/F10 и отражает hotplug актуальнее.

Если сохранение старой функции всё-таки потребуется, безопаснее пометить её deprecated и сделать строгую `try_current_node() -> Result<Option<NodeId>, DetectNodeError>` основной.

### F2 — release blocker: publish metadata ещё описывает старый релиз

**Факты:**

- `crates/numa-shim/Cargo.toml:3`: `version = "0.1.0"`.
- crates.io API на 2026-08-24 показывает существующую, не yanked `numa-shim 0.1.0`, опубликованную 2026-06-29.
- `crates/numa-shim/CHANGELOG.md:7`: `## Unreleased`; сам changelog перечисляет многочисленные breaking changes.
- `crates/numa-shim/README.md:33,36,65`: dependency snippets всё ещё используют `0.1`.
- root `Cargo.toml:933`: versioned path dependency всё ещё `version = "0.1"`.
- `docs/NUMA_GATE_2026-08-23_0.2.0_phase24_waiver.md:19-22`: release/tag SHA всё ещё `[TO BE FILLED]`.
- `.github/workflows/release.yml:250-308` требует ровно одну dated CHANGELOG section для публикуемой версии; текущая форма этот guard не проходит.

**Исправление:** подготовить именно `0.2.0` (breaking API уже зафиксирован), синхронно обновить crate version, lockfile/root pin, README snippets, превратить `Unreleased` в `## 0.2.0 - 2026-08-24` на фактическую дату релиза и привязать waiver к окончательному SHA. Затем выполнить package/publish dry-run и release gates уже на неизменяемом кандидате.

### F3 — P1/P2: `Ok` проверяет return code, но не является независимым оракулом NUMA policy

Новые тесты лучше старых «does not panic»: код действительно проверяет `mbind` return (`src/lib.rs:930-943`). Но `tests/smoke.rs:31-38` объявляет сам `Ok` доказательством того, что policy установлена. Это проверяет реализацию её же собственным ответом. Регрессия вида «reservation вернуть, `mbind` случайно пропустить» оставит тест зелёным.

Mock также записывает только публичный вызов (`src/lib.rs:627-650`), а не отдельный policy syscall, поэтому такая регрессия не обнаружится mock suite. Нет проверки через `get_mempolicy(MPOL_F_ADDR)` или `/proc/self/numa_maps`, нет negative control, нет oracle для полного reservation span.

Отдельно release waiver фиксирует:

- Phase 2 real Linux/QEMU — не запускалась;
- Phase 4 real multi-socket — не запускалась и waived;
- Phase 3 Hyper-V virtual NUMA — partial и **не waived**;
- финальный Phase 1 на окончательном SHA — ещё должен быть запущен последним.

На момент этого отчёта GitHub CI для `8394108` ещё выполнялся: 35/41 jobs completed, 0 failed, общий run `in_progress`. Это хороший промежуточный сигнал, но не release evidence.

**Исправление:** добавить Linux integration oracle, который после успешного вызова запрашивает policy по адресу внутри usable span и, отдельно, по alignment slack/границе reservation. Для controlled NUMA VM проверить first-touch page placement. Mock должен уметь script-ить policy success/failure независимо от reservation success и доказывать cleanup на второй стадии. Затем выполнить gate phases на финальном SHA; если инфраструктурный waiver остаётся, вердикт должен называться risk-accepted release, не fully verified release.

### F4 — P2: `NodeId` не обеспечивает заявленный собственный инвариант

`NodeId::new` принимает любой `u32` (`src/lib.rs:86-119`), но документация на `:98-100` говорит, что `NO_NODE` «must NOT be wrapped». CHANGELOG и README формулируют изменение так, будто sentinel больше не принимается reservation API. Фактически `NodeId::new(NO_NODE)` компилируется:

- Linux вернёт `InvalidNode`;
- Windows передаст `u32::MAX` ОС;
- unsupported platform вернёт `UnsupportedPlatform` до какой-либо проверки;
- mock вернёт Linux-shaped `InvalidNode`.

Это половинчатый newtype: он меняет синтаксис, но не делает запрещённое состояние непредставимым.

**Лучшее исправление до первой публикации типа:** закрыть unchecked constructor. Например, `NodeId::new(u32) -> Option/Result<NodeId, InvalidNodeId>` с обязательным отказом для `NO_NODE`; внутренний `const unsafe/new_unchecked` не нужен, если detection тоже возвращает `NodeId`. Platform-dependent существование узла по-прежнему проверяет fallible reservation call. Если Linux cap остаётся, можно ввести platform-specific error, но sentinel-инвариант обязан обеспечивать сам тип.

### F5 — P2: предел cpumap рассчитан по неверной величине

`NODE_CPUMAP_BUF_LEN = 1024` (`src/lib.rs:953-971`) обоснован как ёмкость примерно 3640 CPUs «на одном node». Linux node cpumap — глобальный `cpumask`: bit index является глобальным logical CPU ID. Ядро строит его как `cpumask_of_node(node) & cpu_online_mask` и печатает через `cpumap_print_bitmask_to_buf`; ширина определяется пространством глобальных CPU IDs, а не количеством set bits на конкретном node ([Linux `drivers/base/node.c`](https://github.com/torvalds/linux/blob/master/drivers/base/node.c)).

Следствия:

- 1024 bytes покрывают примерно 3640 глобальных CPU IDs, не 3640 CPUs per node;
- система с множеством nodes × сотнями CPUs или sparse/high CPU IDs может сделать maps шире лимита сразу для всех nodes;
- `read_cpumap_into` вернёт `None`, topology потеряется, а F1 превратит это в node 0;
- утверждение `src/lib.rs:960-964`, что отдельный 64-node ceiling делает суммарный масштаб «far below» 3640, математически неверно.

**Исправление:** не хранить 64 полных текстовых bitmap. Один reusable temporary buffer достаточной для Linux `CPUMAP_FILE_MAX_BYTES` величины можно разбирать при инициализации в компактный reverse index `CPU ID -> node`. Это уменьшит память и ускорит hot path. Ещё лучше — получать node напрямую через `getcpu`, исключив этот искусственный предел из обычного пути.

### F6 — P2: mock не моделирует новую двухстадийную семантику

Mock branch (`src/lib.rs:627-650`) делает реальную `aligned_vmem` reservation и Linux-specific check `node < 64`, но не имеет отдельного события/результата «policy installation». Поэтому невозможно детерминированно проверить главные новые postconditions:

- reservation succeeded, policy failed;
- original errno сохранён после cleanup;
- reservation released exactly once;
- Windows node IDs `>= 64` forwarded, а не отвергнуты;
- unsupported-platform error precedence.

Mock также расходится с real backend: на Windows production передаёт любой `u32` ОС (`src/lib.rs:1354-1361`), а mock заранее возвращает `InvalidNode`; на macOS production всегда `UnsupportedPlatform`, а mock способен вернуть reservation.

**Исправление:** сделать test seam сценарным: отдельно script/record `Reserve`, `InstallPolicy { span, node }`, `Release`, возвращаемый OS error. Platform-independent wrapper tests должны проверять orchestration, а platform contract tests — конкретные различия Linux/Windows/unsupported. Mock не должен молча выдавать Linux semantics за универсальную.

### F7 — P2 docs: soft preference всё ещё называется успешным binding

Противоречие находится рядом в публичном rustdoc:

- `src/lib.rs:547-551`: reservation time назван «the only point where successfully bound is a true statement»;
- `src/lib.rs:553-554`: сразу сказано, что `MPOL_PREFERRED` — soft preference и success не гарантирует placement.

Та же формулировка есть в README `:77-81`, а headline `README.md:3` и crate root `src/lib.rs:1` всё ещё говорят «binding». Для `MPOL_PREFERRED` успешный syscall означает «policy metadata accepted», не «страницы bound to node».

README `:49-54` также утверждает, что binding already-touched object невозможен и placement обязательно запрашивать только при reservation. Это слишком категорично: Linux `mbind` поддерживает migration flags (`MPOL_MF_MOVE`/`MOVE_ALL`). Правильное решение удалить старый небезопасный API остаётся хорошим, но причина должна звучать как осознанное ограничение crate, а не невозможность ОС.

**Исправление:** везде использовать «preferred policy installed before first touch»; слово `binding` оставить только при описании исторического API/системного семейства или явно отделить strict binding от preference. Написать: crate намеренно не предоставляет migration existing pages, потому что это другой привилегированный и частично failing contract.

### F8 — P2/P3: smoke tests переоценивают свои оракулы

1. `tests/smoke.rs:144-153` заявляет, что восемь повторов по 8 MiB быстро OOM-ятся при leak. Это всего около 64 MiB дополнительного VA и почти без physical commit; на 64-bit процессе такой цикл не обнаруживает утечку полного reservation. Комментарий и postcondition ложные.
2. Linux positive tests берут `current_node().unwrap_or(0)` (`:55`, `:110`). В контейнере/cgroup CPU node может не входить в allowed memory nodes; `mbind` вправе отказать, хотя реализация корректна. После F1 fallback 0 делает тест ещё более хрупким.
3. `current_node_returns_valid_or_none` утверждает универсальный `node < 64` (`:14-26`), хотя этот cap — Linux implementation limit reservation mask, а Windows detection API возвращает `u16` node number. Test смешивает detection contract и Linux reservation limitation.
4. Windows `MemoryBasicInformation` test layout (`:226-242`) без cfg включает `_WIN64`-only `PartitionId`, хотя пакет не запрещает 32-bit Windows.

**Исправление:** убрать OOM-based leak claim; использовать наблюдаемый release counter/fault-injection seam или OS query, который доказывает освобождение конкретной reservation. Positive NUMA policy test выполнять только в controlled topology/cpuset. Платформенные bounds разделить. Для Windows тестового FFI сделать cfg layout либо официально сузить target support до 64-bit.

### F9 — P3 performance: Windows common path можно сократить с двух syscalls до одного

`reserve_aligned_numa` всегда резервирует `size + align` (`src/lib.rs:1426-1452`), затем отдельно commit-ит `size` (`:1475-1498`). Для обычного `align <= allocation granularity` kernel-chosen base уже удовлетворяет alignment. Microsoft разрешает `MEM_RESERVE | MEM_COMMIT` одним вызовом, а NUMA preference учитывается именно при создании новой VA region; при commit existing region node parameter игнорируется ([Microsoft `VirtualAllocExNuma`](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-virtualallocexnuma), локализованная официальная страница с тем же контрактом доступна в результатах Microsoft Learn).

Sibling `aligned-vmem` уже реализует такой fast path (`crates/aligned-vmem/src/os/windows.rs:62-204`) и fallback exact-reserve path (`:207-324`). `numa-shim` его пока не повторяет.

**Исправление:** для `align <= WIN_ALLOCATION_GRANULARITY` сначала один `VirtualAllocExNuma(NULL, size, MEM_RESERVE | MEM_COMMIT, ..., node)`, обязательно проверить фактическое alignment в release build; при неожиданном mismatch освободить и перейти к существующему over-reserve path. Это убирает один syscall и alignment slack из наиболее частого случая. В fallback заменить integer pointer round-trip `raw as usize -> base_u as *mut u8` на strict-provenance-friendly `.addr()`/`.with_addr()`, как уже сделано в sibling crate.

### F10 — P3 performance: hot Linux lookup всё ещё повторно парсит до 64 KiB текста

После первого вызова syscalls действительно исчезают, но `cpu_to_numa_node_checked` (`src/lib.rs:1052-1059`) проходит до 64 nodes, а `parse_contains_cpu` (`:739-757`) для каждого заново считает commas и ищет token. Worst-case lookup повторно читает примерно 64 KiB cached text. Сам cache — `[[u8; 1024]; 64]` плюс lengths, то есть около 64.5 KiB, и создаётся локальным `Topology` внутри `OnceLock` initializer (`:1018-1041`).

**Улучшение:** reverse index `cpu_to_node[cpu]` даёт O(1), уменьшает static/stack footprint и разбирает каждую map один раз. Перед выбором измерить direct `getcpu` против vDSO/sched_getcpu + reverse index на реальных allocator workloads; текущий mock benchmark не измеряет real Linux topology path (`benches/numa_bench.rs:12-16,48-67`).

### F11 — P3 portability: фактическая 32-bit Windows policy не определена

Публичная platform matrix говорит просто «Windows», cfg production-кода также `windows`, но тест прямо считает 64-bit единственным «realistic target» и использует 64-bit-only layout. CI `windows-latest` проверяет только host x64; i686 target row не найден.

**Исправление:** либо официально поддержать `target_pointer_width = 32` и добавить compile/cross test с корректными FFI layouts, либо честно ограничить Cargo/docs/compile cfg 64-bit Windows. Не оставлять молчаливую третью политику «возможно работает, но никто не обещает».

### F12 — P3 docs: README противоречит реэкспорту `Reservation`

README `:66-70` говорит, что direct `aligned-vmem` dependency нужна, чтобы назвать возвращаемый тип. Но `src/lib.rs:173-181` специально делает `pub use aligned_vmem::Reservation`, поэтому тип можно назвать `numa_shim::Reservation` без direct dependency. Прямой dependency нужен текущему примеру для `page_size`, `PAGE` и explicit fallback, но не для имени типа.

**Исправление:** разделить утверждения: `Reservation` доступен через re-export; direct `aligned-vmem` требуется только если caller использует его функции/константы или явно строит unbound fallback.

## Что в новых коммитах сделано хорошо

- Удаление `bind_range` вместо косметического ремонта — правильное решение: старый API одновременно имел alignment trap, игнорировал syscall result и вводил в заблуждение относительно already-faulted pages.
- `reserve_preferred_on_node -> Result` с typed errors существенно улучшает наблюдаемость ошибок.
- Linux policy применяется к полному logical reservation span до first touch; `mbind` result проверяется, errno захватывается до `drop`.
- `maxnode = 65` с single `u64` маской для bits `0..=63` соответствует исторической Linux `get_nodes(--maxnode)` семантике: после decrement ядро копирует один 64-bit word. Здесь чтения за объектом не обнаружено.
- Windows commit-base check теперь fail-closed и работает в release, а не только под `debug_assert`.
- Windows cleanup ownership выстроен последовательно: до `from_raw_parts` владеет функция, после него — RAII handle; подтверждённого double-release пути нет.
- Ошибка commit/reserve сохраняется до `VirtualFree`; аналогично Linux errno сохраняется до `munmap` в Drop.
- Unsupported platform и unsupported Linux architecture больше не возвращают unbound reservation как успех.
- Custom `numa_shim_mock` убрал Cargo feature-unification hazard; dedicated CI rows и grep sentinels закрывают vacuous-zero-tests риск.
- CI follow-up `8394108` исправил реальные cfg/macos test regressions новой реализации; текущий workflow на момент снимка не имел failed jobs, хотя ещё не завершился.

## Рекомендуемый порядок доведения до релиза

1. **Исправить F1:** неизвестный Linux node не должен становиться node 0; обновить in-tree consumer и README.
2. **Перед заморозкой API исправить F4:** сделать `NodeId` реально valid-by-construction хотя бы относительно sentinel.
3. **Переработать Linux detection/cache:** direct `getcpu` или reverse index; одновременно закрыть F5/F10, EINTR handling и artificial width problem.
4. **Добавить независимый policy oracle и сценарный mock** (F3/F6); исправить ложные/хрупкие smoke oracles (F8).
5. **Исправить публичные обещания** (F7/F12), отдельно определить 32-bit Windows support (F11).
6. **Сделать Windows one-call fast path** и strict-provenance pointer derivation (F9), затем измерить, а не обещать ускорение без benchmark.
7. **Подготовить release metadata 0.2.0**: Cargo versions/pins, lockfile, README, dated CHANGELOG, waiver SHA.
8. На неизменном release candidate выполнить fmt/check/clippy/tests/miri/package/publish dry-run и обязательные NUMA gates. Финальный Phase 1 должен идти последним. Дождаться полностью зелёного CI для exact SHA.
9. Только после этого ставить `numa-shim-v0.2.0` и публиковать.

## Границы этого исследования

Это single-agent static audit по прямому требованию пользователя. Полностью просмотрены crate manifest, README, CHANGELOG, public API, Linux cpumap/topology/mbind FFI, Windows FFI/ownership, unsupported/miri cfg branches, tests, benchmark, relevant CI/release workflow и in-tree integration seam. Async, crypto, network protocol и lock-free code к crate не относятся. Реальное выполнение OS paths намеренно не проверялось: пользователь запретил запуск тестов; поэтому любые runtime/placement выводы помечены как статические или как отсутствующая доказательность.

## Финальная оценка

**Код reservation path близок к publishable и заметно надёжнее предыдущей версии, но crate целиком ещё не publish-ready.** Минимальный честный release требует F1 + F2 + завершённого exact-SHA CI/gate evidence. Для заявленного направления «совершенство без компромиссов» до 0.2.0 также стоит закрыть F3–F8: именно сейчас можно сломать API правильно, прежде чем новый `NodeId`/`NodeResolution`/reservation contract станут очередным совместимостным грузом.
