# Новое pre-publication исследование `numa-shim`

**Автор:** Сол-кодекс  
**Время:** 2026-08-24 22:43:23 CEST (Europe/Berlin)  
**Snapshot:** `555deba751be367ad64c5325b484bf3eababfb1b`  
**Предыдущее исследование Сол-кодекс:** `9137c514775ca539a99e454945cf6a6103cc7ecb`  
**Исследованный диапазон:** `9137c51..555deba`; production/test-код последней волны заканчивается на `b275a22`, последующие `5913ee7` и `555deba` меняют только review/tracking-документы.

## Вердикт

**NO-GO к публикации в текущем состоянии.**

Основные production-исправления последней волны сделаны аккуратно: bounded EINTR retry сохраняет errno до cleanup, strict-provenance round-trip убран, mock-state очищается, а 32-bit Windows policy теперь действительно enforced. Однако Linux policy oracle всё ещё не является надёжным release gate: его новая errno-классификация одновременно способна ложно упасть на допустимом окружении и скрыть неизвестную реализационную ошибку; вдобавок вся положительная Linux-проверка может завершиться зелёным `return`, если сломался собственный `current_node()` crate-а. Есть ещё один воспроизводимый по cfg-логике конфликт mock+`vmem-integration`, production race на первом холодном определении node и fd-inheritance дефект.

Перед `GO` рекомендую закрыть P1/P2 ниже. P3 и perf-пункты не все обязаны блокировать публикацию, но для заявленной цели «довести без компромиссов» их лучше закрыть в этой же волне.

## Ограничения и метод

- Исследование выполнено одним агентом, без под-агентов.
- Режим исходников был read-only; единственная запись — этот отчёт.
- Тесты, `cargo test`, `cargo check`, `cargo clippy`, Miri, benches, сборки и package/dry-run **не запускались**.
- Проверены весь crate (`src`, integration tests, bench, Cargo metadata, README/CHANGELOG), новый diff и относящиеся к нему CI jobs.
- Unsafe/FFI, errno timing, provenance, cleanup, cfg-матрица, тестовые oracle, публичные обещания и горячие/холодные пути проверялись по правилам `rust-intel`.
- Async, crypto и network-код отсутствуют и потому неприменимы. Runtime-утверждения ниже — результаты статического анализа, а не новых запусков.

## Что изменилось после прошлого отчёта

| Коммит/волна | Проверка | Оценка |
|---|---|---|
| `546e8d8` — errno classification policy oracle | Убрано безусловное игнорирование любого `ReserveNumaError::Os`, но классификация остаётся логически неполной и местами неверной | **Частично закрыто; P1 остаётся** |
| `f325bb1` — dead-code под mock без `vmem-integration` | Точечный `cfg_attr(..., allow(dead_code))` устраняет предупреждение; безопаснее и чище было бы вообще не компилировать helper без feature | Приемлемо, не blocker |
| `652d505` — bounded EINTR retry | Errno снимается немедленно, open/read повторяются на том же состоянии, streak сбрасывается после progress, retry ограничен | Исправление корректно; остаются P3 по portability/testability |
| `6f242f1` — strict provenance + mock cleanup | Pointer остаётся pointer-typed до syscall; неиспользованный scripted failure очищается | Закрыто корректно |
| `25e25e7` — 32-bit Windows gate | `compile_error!` точно ограничен `windows && target_pointer_width=32`; non-Windows 32-bit не затронут | Закрыто корректно |
| `b275a22` — закрытие tracking item | Документирует волну как закрытую | Преждевременно из-за findings ниже |

## Findings, блокирующие `GO`

### F1 — P1: Linux policy oracle по-прежнему не доказывает headline-контракт

**Места:** `crates/numa-shim/tests/policy_oracle_linux.rs:204-266`, `:325-332`; `.github/workflows/ci.yml:2655`, `:2692`.

Здесь две независимые дырки в oracle.

1. **Новая errno-классификация не соответствует контракту `mbind(2)`.** Код считает `EINVAL` исключительно собственной ошибкой marshalling и всегда паникует. Но Linux документирует `EINVAL` также для корректно сформированного вызова, если ни один node из mask не online, не разрешён текущим cpuset либо не содержит памяти. `current_node()` определяет CPU-node по `cpumap`, но это не доказывает, что тот же node входит в `cpuset_current_mems_allowed` и имеет memory. На CPU-only/memoryless node или при разных cpuset CPU/memory masks тест даст ложный red. Аналогично `ENOSYS` может быть результатом sandbox/seccomp policy, а не только неверного syscall number.

2. **Обратная ветка слишком широкая.** Любой другой `Some(errno)` объявляется environment refusal и превращается в зелёный `return`. Для используемых `flags=0` документированные `EIO` и privilege-`EPERM` не соответствуют форме вызова, а произвольные `EBADF`/`ERANGE`/неожиданные errno тем более нельзя автоматически считать ограничением окружения. Реализационная регрессия всё ещё может стать зелёным skip.

3. **Полная поломка detection также зелёная.** Оба oracle-теста возвращают success при `current_node() == None`. Real-Linux CI запускает обычный `cargo test`, но не требует, чтобы конкретный positive oracle действительно дошёл до `mbind/get_mempolicy`. Регрессия в `sched_getcpu`, sysfs path, parser или topology cache превращает одновременно positive и negative control в два зелёных теста без единой проверки policy.

Linux man-pages прямо перечисляет environment-варианты `EINVAL` и семантику allowed nodemask: <https://man7.org/linux/man-pages/man2/mbind.2.html>. `get_mempolicy(MPOL_F_MEMS_ALLOWED)` предназначен для получения множества nodes, которые поток вправе передавать в последующие `mbind`: <https://man7.org/linux/man-pages/man2/get_mempolicy.2.html>.

**Исправление:**

- перед positive call сделать capability/precondition probe через `get_mempolicy(MPOL_F_MEMS_ALLOWED)` и удостовериться, что выбранный node разрешён и memory-bearing; либо выбрать заведомо разрешённый memory node;
- после выполненного preflight применять **allowlist**, а не «всё, кроме трёх»: ожидаемое исчерпание ресурса можно классифицировать явно, неизвестный errno обязан падать;
- отделить sandbox-отсутствие syscall от неверного номера отдельным probe/явной CI policy;
- добавить strict CI mode (`NUMA_SHIM_REQUIRE_POLICY_ORACLE=1` или отдельный test), где `current_node()==None` и любой skip являются failure; обычный локальный запуск может сохранить диагностический skip;
- CI должен проверять, что strict positive oracle именно выполнился, а не только что test process завершился с кодом 0.

### F2 — P2: `smoke.rs` выбирает ожидания по target OS, хотя backend может быть mock

**Места:** `crates/numa-shim/tests/smoke.rs:131-171`, `:213-253`, `:403-419`; public mock dispatch — `src/lib.rs:802-880`; CI — `.github/workflows/ci.yml:2792-2795`, `:2828-2833`.

Под `--cfg numa_shim_mock --features vmem-integration` public API идёт в mock backend и успешный reserve допустим на любой OS. Но smoke tests используют только `cfg!(target_os/windows/miri)` и на macOS/miri ожидают `UnsupportedPlatform`. Поэтому валидная комбинация:

```text
RUSTFLAGS="--cfg numa_shim_mock" cargo test -p numa-shim --features vmem-integration
```

логически red на macOS и под miri: тест ожидает real unsupported-platform backend, фактически вызывается platform-independent mock. Текущая CI-матрица не пересекает эти условия: macOS mock row идёт без `vmem-integration`, macOS miri mock row — тоже без него.

**Исправление:** real-platform smoke tests компилировать/ветвить с `not(numa_shim_mock)`, а mock-контракт проверять только в `mock_dispatch.rs`; затем добавить macOS `mock + vmem-integration` и, если эта конфигурация обещана, miri-equivalent row.

### F3 — P2: первый Linux snapshot CPU берётся до дорогой инициализации topology

**Места:** `crates/numa-shim/src/lib.rs:1275-1295`, `:1413-1425`, `:1438-1457`.

`current_node_impl()` сначала вызывает `sched_getcpu()`, а затем первая `cpu_to_numa_node()` инициирует до 64 `open/read/close` проходов. Поток может быть мигрирован на другой CPU/node во время этого существенно более длинного окна, и первый вызов вернёт node CPU, на котором поток находился **до** инициализации, хотя к моменту возврата он уже может быть на другом node. На warm path окно мало и принципиально неустранимо для snapshot API, но cold path искусственно увеличивает его на весь topology scan.

**Исправление:** сначала получить `let topology = topology();`, после завершения cache init снять `sched_getcpu()` и выполнить прямой `topology.lookup(cpu)`. То же сделать в `current_node_resolution_impl()`. При желании максимальной строгости — повторно снять CPU после lookup и повторить один раз, если CPU изменился.

### F4 — P2: topology reader открывает fd без close-on-exec

**Место:** `crates/numa-shim/src/lib.rs:1520-1541`, особенно `libc_open(..., 0)` на `:1526`.

Во время первого topology scan процесс держит один cpumap fd. В многопоточном приложении параллельный `fork+exec` может унаследовать этот descriptor, потому что `open` вызывается без `O_CLOEXEC`; выставление `FD_CLOEXEC` отдельным `fcntl` оставило бы race. Для allocator-adjacent library, вызываемой внутри произвольного процесса, это реальный resource-lifecycle дефект, пусть окно и короткое.

**Исправление:** передавать Linux `O_RDONLY | O_CLOEXEC` непосредственно в `open(2)` и локально документировать константу. Это не добавляет syscall и не замедляет normal path.

### F5 — P2: Linux test сравнивает два независимых CPU snapshots как один

**Место:** `crates/numa-shim/tests/node_resolution_linux.rs:33-45`.

Тест сначала вызывает `current_node()`, затем отдельно `current_node_resolution()`. Оба вызова самостоятельно делают `sched_getcpu()`. На настоящей multi-node машине scheduler вправе мигрировать test thread между вызовами, поэтому корректные `Some(0)` и `Resolved(1)` дадут ложный failure. Одноузловой CI скрывает гонку; будущий real multi-NUMA gate как раз сделает её вероятнее.

**Исправление:** не сравнивать конкретный node двух независимых snapshots. Проверять pure mapping одним захваченным raw result через test-only helper либо pin-ить thread affinity на время oracle. Retry-until-two-equal допустим только как диагностический fallback, но хуже структурного single-snapshot oracle.

## P3 / улучшения качества

### F6 — fixed 64-bit output mask ограничивает `get_mempolicy` oracle системами с `nr_node_ids <= 65`

**Место:** `crates/numa-shim/tests/policy_oracle_linux.rs:155-182`.

`maxnode=65` действительно заставляет `copy_nodes_to_user` записать ровно 8 bytes, то есть локального overflow нет. Но современный kernel сначала отвергает запрос, если `maxnode < nr_node_ids`. На машине более чем с 64 addressable node тест получает `EINVAL` от собственного probe независимо от того, что проверяемый crate node находится в диапазоне 0..63.

**Исправление:** дать oracle достаточно большой fixed nodemask для поддерживаемого kernel ceiling и передать согласованный `maxnode`; проверять нужный low bit. Это test-only память, поэтому экономия до одного `u64` здесь не оправдана.

### F7 — EINTR helper привязан к числу errno и не имеет прямого oracle

**Места:** `crates/numa-shim/src/lib.rs:1460-1492`; CI не имеет unit oracle для retry predicate.

Логика bounded retry корректна, но `raw_os_error()==Some(4)` дублирует платформенный ABI и комментарий обосновывает значение только x86_64/aarch64, тогда как detection module компилируется и на других Linux architectures. `std::io::ErrorKind::Interrupted` выражает требуемую семантику прямо и переносимее. Три дешёвых проверки — interrupted below limit, interrupted at limit, non-interrupted — закрепят boundary и не требуют реального сигнала.

**Исправление:** использовать `err.kind() == ErrorKind::Interrupted`, вынести predicate в testable pure location и добавить boundary oracle.

### F8 — default-feature rustdoc не покрывается CI

**Места:** `.github/workflows/ci.yml:2717-2724`; unconditional intra-doc links в `src/lib.rs:51`, `:133`, `:229-235` указывают на feature-only `reserve_preferred_on_node`/`Reservation`.

Обе rustdoc rows фактически одинаковы: единственный feature — `vmem-integration`, и docs.rs metadata включает его. Документация конфигурации, которую получает default consumer, не проверяется с `-D warnings`; именно там условные items исчезают и могут проявиться unresolved links/drift.

**Исправление:** добавить отдельный `RUSTDOCFLAGS="-D warnings" cargo doc -p numa-shim --no-deps` без features и при необходимости использовать `cfg_attr(doc, ...)`/текстовые ссылки для feature-only symbols.

### F9 — комментарий Windows release-oracle остался в старом состоянии policy

**Место:** `crates/numa-shim/tests/smoke.rs:286-288`.

Комментарий всё ещё говорит, что 32-bit Windows policy «undecided» и первая копия struct должна оставаться untouched. После `25e25e7` это неверно: policy решена и enforced. Две ручные копии `MemoryBasicInformation` также создают лишнюю точку layout drift.

**Исправление:** вынести 64-bit test FFI mirror/query helper в общий Windows-only test module и заменить исторический комментарий актуальным контрактом.

### F10 — public performance docs и bench не измеряют реальный backend

**Места:** `src/lib.rs:621-623`, `:683-692`; `benches/numa_bench.rs:48-67`.

- Docs называют warm `current_node()` «pure in-memory» и «no syscalls», но каждый вызов всё равно делает `sched_getcpu()`; libc может обслужить его быстро, однако безусловно обещать отсутствие syscall нельзя.
- Bench `first_call` и `warm_call` работают только на mock backend, где нет topology init, зато есть thread-local call recording и потенциальный `Vec::push`. Оба benchmark-а измеряют почти одну и ту же mock-механику и не дают данных ни о cold sysfs cost, ни о production warm lookup.

**Исправление:** уточнить docs до «topology lookup is cached/in-memory; CPU sampling remains platform call». Добавить отдельные real-Linux cold/warm benches или маленький measurement binary; mock bench честно переименовать в dispatch/recording overhead.

### F11 — cpumap parser имеет квадратичный cold-path проход по tokens

**Места:** `src/lib.rs:953-974`, `:1055-1073`, `:1141-1171`.

`parse_each_set_cpu` для каждого word вызывает `nth_token`, который каждый раз сканирует slice сначала. Получается O(words²); `index_node` делает этот проход дважды (validate + commit), и topology init повторяет его до 64 nodes. Для обычной маленькой машины это незаметно, но на максимально широком cpumap именно cold-start cost становится ненужно большим.

**Исправление:** итерировать tokens линейно справа налево (`rsplit`) с возрастающим word index. Двухпроходную fail-closed схему можно сохранить — оба прохода станут O(words). Ускорение следует подтвердить real-parser benchmark-ом, не mock benchmark-ом.

## Дополнительные наблюдения

- `set_policy_failure` остаётся доступным без `vmem-integration`, хотя его consumer тогда отсутствует, а `take_policy_failure_for` подавляется через `allow(dead_code)`. Чище cfg-гейтить весь policy-failure sub-API/state feature-ом; это test-only surface и не blocker.
- README одновременно говорит «zero C library dependencies» и признаёт прямые вызовы system libc. Точнее: «zero third-party C/C++ dependencies / no libnuma or hwloc»; системная libc всё же является FFI dependency.
- Process-lifetime topology snapshot и лимит nodes 0..63 документированы явно; в этой проверке новых production safety-дефектов в reverse index, Linux `mbind` cleanup, Windows reserve/commit ownership и strict provenance не найдено.
- Windows two-call implementation всё ещё имеет потенциальный fast path для alignment `<= 64 KiB`, аналогичный sibling `aligned-vmem`; применять его стоит только после OS-bookkeeping oracle и измерения, потому что он меняет reservation shape.

## Рекомендуемый порядок исправлений

1. Перестроить Linux policy oracle: preflight allowed/memory node, точная errno allowlist, strict CI no-skip mode.
2. Развести real-platform smoke и mock backend по cfg; закрыть macOS/miri `mock + vmem-integration` клетки.
3. Инициализировать topology до первого CPU snapshot; исправить двухвызовный Linux test oracle.
4. Добавить `O_CLOEXEC` в cpumap `open`.
5. Сделать `get_mempolicy` test mask масштабируемым для >64-node hosts.
6. Закрыть default rustdoc row, ErrorKind/boundary tests и stale Windows comment.
7. Линеаризовать cpumap parser и заменить mock-only performance evidence реальными cold/warm измерениями.

## Условия для `GO`

- F1 больше не допускает ни ложного skip implementation failure, ни ложного red на допустимом cpuset/memoryless окружении.
- На строгой Linux CI-конфигурации positive policy oracle обязан реально выполнить `mbind` и kernel readback.
- Все `numa_shim_mock × vmem-integration × OS/miri` ожидания согласованы с выбранным backend.
- Cold CPU snapshot берётся после topology init; multi-node test не сравнивает независимые snapshots.
- Cpumap fd создаётся atomically close-on-exec.
- После исправлений владельцы проекта выполняют обычную динамическую verification matrix; данный read-only обзор её не заменяет.

После выполнения этих условий production design выглядит достаточно узким и понятным для публикации: safe public API, typed failures, fail-closed detection, RAII cleanup и явно ограниченный platform contract уже находятся на хорошем уровне.
