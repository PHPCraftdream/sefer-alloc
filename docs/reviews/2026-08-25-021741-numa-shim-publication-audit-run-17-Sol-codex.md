# `numa-shim`: предрелизный аудит, прогон 17

- Автор: Sol-codex
- Время отчёта: 2026-08-25 02:17:41 Europe/Berlin
- Снимок: `623062d059b391a7becb5ed509df1c8e73741eaa`
- База сравнения: `555deba751be367ad64c5325b484bf3eababfb1b` (снимок предыдущего отчёта Sol-codex)
- Режим: статический анализ, только чтение; без под-агентов; тесты, сборка, `cargo check`, Clippy, Miri, benchmark и package/publish-команды не запускались
- Единственная запись в рабочее дерево: этот отчёт

## Вердикт

**NO-GO до исправления одного P2 в публичной документации.**

В production-коде новых Critical/P0/P1-дефектов, UB, нарушений FFI ABI, утечек или ошибок владения не найдено. Последняя волна исправлений закрывает прежние замечания по существу: Linux policy-oracle стал масштабируемым и fail-closed, topology scan теперь предшествует снимку CPU, test oracle использует один CPU snapshot, `open` получил `O_CLOEXEC`, EINTR-предикат стал переносимым и тестируемым, а cpumap-парсер стал линейным.

Публикацию пока останавливают два некомпилируемых примера одного класса в rustdoc. Это не runtime-баг, но это именно пользовательская инструкция к headline API; сейчас документация предлагает код с несовместимыми `Result`/`Option`, а fence `text` не даёт CI обнаружить ошибку.

После исправления F1 и добавления невакуозного compile-oracle для примеров статический вердикт станет **GO с принятыми и уже документированными ограничениями**. Исполнительные гарантии этим read-only проходом не подтверждались.

## Что исследовано

1. Полный diff `555deba..623062d` для `crates/numa-shim`, связанных CI-строк и release/readiness-документов.
2. Весь текущий `crates/numa-shim/src/lib.rs`: публичные типы и функции, все `unsafe`/`extern` участки, cfg-матрица, Linux/Windows/stub backends, mock seam, parser/reverse index.
3. Все тесты крейта, включая новые `eintr_retry.rs`, переработанные `policy_oracle_linux.rs`, `node_resolution_linux.rs` и `smoke.rs`; проверялись оракулы, counterfactual-сила, skip-ветки и platform/backend gating.
4. `Cargo.toml`, README, CHANGELOG, benchmark и NUMA-секции CI.
5. Контракт Linux `get_mempolicy(2)`/`mbind(2)` сопоставлен с man-pages и текущим kernel source: `kernel_get_mempolicy`, `copy_nodes_to_user`, `get_nodes`, `MPOL_F_MEMS_ALLOWED`.

Применён профиль `rust-intel`: unsafe/FFI, RAII и error cleanup, числа/границы, public API/cfg, тестовые оракулы, semantic conformance и performance-at-scale. Не относящиеся к этому синхронному OS-seam крейту модули async, crypto и network/protocol исключены. Из-за прямого запрета на под-агентов это широкий, но ограниченный одним ревьювером проход; независимый fan-out не выполнялся.

## Обзор новых коммитов

### Подтверждено статически как корректное

- **Строгий real-Linux gate (`#1324`)**: `NUMA_SHIM_REQUIRE_ORACLE=1` превращает `current_node() == None` в fatal outcome на назначенной CI-строке. В совокупности со smoke-тестами эта строка не может зелёно пережить `mbind`-ошибку: smoke path использует `.expect`, даже если policy-oracle локально классифицирует некоторые errno как environment skip.
- **Backend-aware smoke (`#1325`)**: `cfg!(any(numa_shim_mock, all(any(target_os = "linux", windows), not(miri))))` соответствует фактической mock-dispatch семантике на macOS/miri и не меняет real-backend ветвление.
- **Default-feature rustdoc (`#1326`)**: три feature-sensitive intra-doc link исправлены и добавлена отдельная default-feature doc-строка CI. Однако эта проверка ловит ссылки, а не `text`-примеры — отсюда новый F1 ниже.
- **EINTR + `O_CLOEXEC` (`#1327`)**: `ErrorKind::Interrupted` сохраняет реальный Linux EINTR-set, лимит ограничивает последовательные прерывания, streak сбрасывается после прогресса. `O_CLOEXEC = 0o2000000` корректен для поддерживаемых Linux x86_64/aarch64.
- **Policy oracle (`#1329`)**: `[u64; 16]` и `maxnode = 1024` согласованы с kernel copy-out; `maxnode >= nr_node_ids` на поддерживаемых конфигурациях, хвост обнуляется ядром. `MPOL_F_MEMS_ALLOWED` вызван с допустимым сочетанием аргументов. Errno catch-all теперь fail-closed.
- **Topology-before-snapshot (`#1331`)**: долгий однократный sysfs scan больше не расположен между `sched_getcpu()` и lookup. Возвращённый `&ReverseIndex` переиспользуется без лишнего входа в `OnceLock`.
- **Single-snapshot test (`#1332`)**: обе стороны mapping-oracle получают один и тот же CPU id; прежняя ложная ошибка при миграции между двумя API-вызовами устранена.
- **Документальная честность (`#1333`)**: системный libc/Win32 FFI больше не описывается как отсутствие любых C-библиотек; warm path больше не объявлен syscall-free; mock benchmark явно не выдаётся за production measurement.
- **Линейный cpumap parser (`#1334`)**: `rsplit(...).enumerate()` сохраняет rightmost-word-is-CPU-0 порядок и malformed-input поведение, убирая повторные ресканы `nth_token`.
- **`#1335`** меняет process verifier и документацию, но не код/контракт `numa-shim`; влияния на публикационную готовность крейта не обнаружено.

## Findings

### F1 — P2, блокирует публикацию: два публичных rustdoc-примера не компилируются

Места:

- `crates/numa-shim/src/lib.rs:145-156` (`NodeId::new`, “Ergonomic path from detection”);
- `crates/numa-shim/src/lib.rs:768-777` (`reserve_preferred_on_node`, “Best-effort fallback”).

Первый пример сводит в один `match`:

```text
Some(_) -> Result<aligned_vmem::Reservation, ReserveNumaError>
None    -> Option<aligned_vmem::Reservation>
```

Второй вызывает `Result::or_else`, но closure возвращает `aligned_vmem::reserve_aligned(...)`, то есть `Option<Reservation>`, а должна вернуть `Result<Reservation, ReserveNumaError>`.

Оба блока помечены `text`, поэтому rustdoc не пытается их типизировать. Добавленная default-feature rustdoc-строка CI закономерно остаётся зелёной: она проверяет ссылки и оформление, но не код в `text` fence. README уже содержит корректный явный `match`, поэтому источник истины расходится с rustdoc именно на пользовательской fallback-инструкции.

Рекомендация:

1. Выбрать честную результирующую модель и привести обе ветви к одному типу. Для best-effort `Option`-формы минимальная корректная форма — `reserve_preferred_on_node(...).ok().or_else(|| aligned_vmem::reserve_aligned(...))`; для сохранения диагностической ошибки лучше оставить явный `match`, как в README.
2. Не оставлять эти фрагменты непроверяемым `text`: сделать compile-checked `no_run` пример под `vmem-integration` либо вынести canonical snippets в compile-only integration oracle. В репозитории действует правило против runnable doctest, поэтому второй путь, вероятно, лучше согласуется с существующим процессом.
3. Одновременно исправить верхний usage-текст `src/lib.rs:20-26`: `None` сейчас печатает “NUMA unavailable or single-node host”, хотя корректно разрешённая single-node Linux/Windows машина возвращает `Some(0)`. Также импорт `NO_NODE` в этом фрагменте не используется.

### F2 — P3, качество API-поверхности: `nth_token` остался shipping-public только ради теста

`cpumap::parse_each_set_cpu` после `#1334` больше не вызывает `nth_token`, но `nth_token` оставлен `#[doc(hidden)] pub` и генерируется в публикуемой библиотеке только потому, что два integration-теста обращаются к нему напрямую. CHANGELOG прямо называет это причиной сохранения.

Это не correctness-баг и модуль явно объявлен semver-exempt, но тестовая архитектура удерживает мёртвый для production примитив в доступной downstream-поверхности. До публикации, когда ломать совместимость дёшево, чище перенести primitive-тесты внутрь crate/unit boundary либо удалить `nth_token` и проверять только реальный interpreter (`parse_each_set_cpu`/`parse_contains_cpu`).

### F3 — P3, defensive cleanup: `CURRENT_NODE_SLOT` должен быть `Cell<u32>`

`CURRENT_NODE_SLOT` только целиком читается и записывается, но использует `RefCell<u32>` и panicking `borrow()`/`borrow_mut()` (`src/lib.rs:396`, `:423-429`). `Cell<u32>` точно выражает контракт, устраняет borrow-state и немного упрощает mock hot path. Это уже записано в correctness backlog item 45; нового дублирующего blocker здесь нет.

### F4 — P3, комментарий manifest противоречит реальному cfg scope

`Cargo.toml:65-68` говорит, что cfg-флаги “do not unify across the build's dependency graph”, тогда как `Cargo.toml:53-56`, README и crate docs корректно предупреждают: `RUSTFLAGS="--cfg numa_shim_mock"` действует graph-wide после явного выбора top-level invoker. Отличие от Cargo feature — отсутствие транзитивного feature-unification, а не локальность cfg для одного экземпляра зависимости.

Рекомендуется переформулировать строку как “not subject to Cargo feature unification; when supplied through global `RUSTFLAGS`, still affects the whole target graph”. Это maintainer-facing комментарий, не runtime defect.

## Unsafe / FFI / lifecycle verdict

- Linux `open/read/close`, `sched_getcpu` и variadic `syscall` имеют согласованные ABI-типы на рассматриваемых 64-bit targets; errno/GetLastError захватываются до cleanup.
- `mbind` использует `maxnode = 65` корректно: kernel `get_nodes` сначала уменьшает значение до 64 и читает ровно один `unsigned long`, поэтому bit 63 сохраняется без чтения второго слова.
- При Linux policy failure свежая `Reservation` уничтожается ровно один раз, исходный errno уже сохранён.
- Windows reserve/commit failure paths освобождают `raw`; strict-provenance derivation через `.addr()`/`.with_addr()` корректна; mismatch committed base проверяется и в release.
- Передача ownership в `Reservation::from_raw_parts` происходит только после полного установления инвариантов. Нового leak/double-free/use-after-free пути не найдено.

## Производительность

Подтверждён только алгоритмический факт `#1334`: parser теперь O(words), а reverse lookup остаётся O(1). Wall-clock ускорение в этом проходе не измерялось и не заявляется.

Остаются неблокирующие кандидаты:

1. Linux cold start: до 64 `open/read/close` и примерно 12 KiB index+scratch; сначала измерить чтение `node/online` или иной способ сократить отсутствующие-node probes.
2. Windows common alignment: измерить одновызовный `MEM_RESERVE | MEM_COMMIT` fast path при `align <= allocation granularity`, сохранив unconditional alignment check и текущие release/commit-charge оракулы.
3. `CURRENT_NODE_SLOT: Cell<u32>` — микроскопическое, но структурно очевидное упрощение mock backend.
4. Не использовать текущие `first_call`/`warm_call` mock benches для production-решений; их новая оговорка корректна, реальный Linux harness всё ещё нужен.

## Принятые ограничения и остаточный риск

- Linux detection/reservation ограничены node ids `0..=63`; topology cache замораживается на первом вызове и не отражает последующий CPU/node hotplug. Оба ограничения явно документированы.
- Реальная multi-socket/phase-2/phase-4 проверка не выполнена в этом проходе; CHANGELOG содержит отдельное owner risk acceptance. Этот отчёт не переопределяет принятое решение и не выдаёт прежние CI-запуски за собственную проверку.
- miri + `numa_shim_mock` + `vmem-integration` остаётся отдельной неисполненной матричной ячейкой; real macOS+mock+vmem уже добавлен. Это coverage improvement, не найденный production defect.
- Async, crypto, network/protocol и supply-chain анализ не применимы к исследованному seam-коду и не проводились.

## Условия GO

Обязательное условие одно: исправить F1 и закрепить хотя бы один compile-oracle, который не позволит снова спрятать типовую ошибку пользовательского примера за `text` fence.

F2-F4 и performance candidates полезно закрыть до первой публикации, пока поверхность можно чистить свободно, но они сами по себе не являются блокерами.

## Источники Linux-контракта

- Linux man-pages: <https://man7.org/linux/man-pages/man2/get_mempolicy.2.html>
- Linux kernel `mm/mempolicy.c`: <https://github.com/torvalds/linux/blob/master/mm/mempolicy.c>
- Linux kernel cpuset implementation: <https://github.com/torvalds/linux/blob/master/kernel/cgroup/cpuset.c>

