# Новое исследование `numa-shim` перед публикацией

- Автор: Сол-кодекс
- Время: 2026-08-23 16:42:06 (Europe/Berlin)
- Ревизия: `87424a45a158c4ace1e4ec097d2c7a319a907eb5`
- Режим: только чтение, один агент, без под-агентов.
- Не запускались: тесты, сборка, `cargo check`, Clippy, Miri, бенчмарки, документация и `cargo publish`.

## Вердикт

**NO-GO для публикации текущей ревизии.**

Главный непосредственный блокер — версия crate всё ещё `0.1.0`, а `crates/numa-shim/CHANGELOG.md` содержит только `## Unreleased`. Репозиторный release workflow требует ровно один changelog-раздел с версией из `Cargo.toml` и отклоняет раздел, помеченный `Unreleased` (`.github/workflows/release.yml:215–307`). Сам changelog описывает `0.1.0` как уже опубликованный, поэтому повторная публикация текущего `0.1.0` невозможна: нужен следующий согласованный version bump, changelog и обновление root-пина.

В текущих unsafe/FFI- и ownership-путях нового очевидного UB не найдено. Прошлые проблемы с cpumap truncation, node 63, Windows double-commit, reentrant heap allocation в topology cache и macOS+miri cfg уже исправлены. Однако до следующего релиза нужно принять решения по feature-дизайну и исправить несколько публичных описаний.

## Findings

### F1 — P0: версия, changelog и root dependency не готовы к следующей публикации

Доказательства:

- `crates/numa-shim/Cargo.toml:3`: `version = "0.1.0"`.
- `crates/numa-shim/CHANGELOG.md:7`: `## Unreleased`; текст ниже говорит, что изменения идут после публикации `0.1.0`.
- `Cargo.toml:914`: root crate всё ещё pins `numa-shim` как `version = "0.1"`.
- `.github/workflows/release.yml:289–307`: релиз требует ровно один раздел для версии пакета и запрещает `unreleased`.
- `numa-shim` уже зависит от `aligned-vmem` `0.2` (`crates/numa-shim/Cargo.toml:63`), поэтому повторное выпускание старого `0.1.0` также не отражает текущую dependency/API точку.

Перед публикацией нужно выбрать следующую версию (по текущему changelog логично рассмотреть `0.2.0`), обновить manifest crate, root-пин, dated changelog section и tag/release notes. Это не косметика: с текущими данными release workflow должен остановить публикацию.

### F2 — P1: `mock` остаётся non-additive backend-replacing Cargo feature

`mock = []` (`crates/numa-shim/Cargo.toml:60`) не добавляет тестовый слой, а заменяет реальный platform dispatch на recording backend. Cargo объединяет features во всём dependency graph. Значит, если любой downstream target включает `numa-shim/mock`, другие потребители того же графа могут получить mock вместо реальных NUMA syscalls.

Риск уже подробно описан в manifest, README и crate rustdoc, но предупреждение не меняет семантику Cargo feature. Для публикуемой библиотеки это остаётся скрытым поведением, особенно при `--all-features` или общих dev-dependencies. Соседний `aligned-vmem` уже перевёл аналогичный seam на build-time cfg.

Рекомендация перед следующим публичным релизом: перевести seam на отдельный build-time cfg/test-support механизм или вынести recording backend в непубликуемый test-support crate. Если feature сознательно оставляется, это должно быть явным owner decision с принятием риска; простого README-предупреждения недостаточно для уверенного GO.

### F3 — P1: safety-документация противоречит фактической публичной поверхности

В `crates/numa-shim/src/lib.rs:46` написано: «The public API is safe», хотя `bind_range` — публичная `unsafe fn` (`:353`). Это вводит в заблуждение при чтении crate-level rustdoc.

Есть и второй рассинхрон:

- README (`crates/numa-shim/README.md:101–104`) требует, чтобы диапазон был «valid OS reservation».
- Реальный контракт `src/lib.rs:336–351` допускает любой valid mapped range, включая heap allocation, и отдельно предупреждает о page-granular эффекте `mbind` на соседние данные.

Нужно привести crate-level docs и README к одному контракту. Safety-текст должен ясно разделять короткое замыкание (`node == NO_NODE` или `len == 0`) и обычный путь, который требует живой mapped range, принадлежащий caller.

### F4 — P1: на системах с node ID ≥ 64 возможен тихий неверный результат

Linux backend использует одно `u64` nodemask и в `bind_range_impl_linux` пропускает `node >= 64` (`src/lib.rs:868`). В topology cache сканируются только node 0..63 (`:733–740`). При unreadable/oversized cpumap или реальном CPU на node ≥ 64 mapping сваливается в `Some(0)` (`:241–246`, `:741–744`).

Ограничение документировано, поэтому это не новый undocumented bug. Но для общего NUMA crate оно опасно: `current_node().unwrap_or(0)` может направить последующий `reserve_on_node` к node 0, хотя настоящий CPU находится на другом node, а caller не получает отличия между single-node fallback и ошибкой определения.

Перед публикацией нужно решить семантику: возвращать `None`/ошибку при невозможности определить node, поддержать динамическую nodemask, либо использовать Linux API, который возвращает CPU и node напрямую (кандидат — `getcpu(2)`). Если диапазон 0..63 является сознательной областью поддержки, это следует вынести в явное ограничение crate и добавить API, позволяющий caller обнаружить пропуск binding, а не только читать предупреждение в README.

### F5 — P2: `#[doc(hidden)] pub mod cpumap` всё равно является public semver surface

`src/lib.rs:449` экспортирует parser helpers через `pub mod cpumap`, лишь скрывая их из обычной документации. Это было удобным способом дать integration tests target-independent oracle, но `#[doc(hidden)]` не делает модуль приватным: downstream code может использовать `format_sysfs_path`, `parse_contains_cpu`, `parse_hex_u32` и другие функции.

Перед публикацией следующего major/minor следует решить, является ли этот parser частью API. Более чистые варианты — отдельный test-support crate, внутренняя тестовая прослойка или осознанное закрепление модуля как semver-stable hidden API. Для уже опубликованного `0.1.0` удаление доступного модуля было бы breaking change, поэтому это решение нельзя откладывать без фиксации политики.

### F6 — P2: README-пример с `aligned_vmem::PAGE` не самодостаточен

`crates/numa-shim/README.md:57–68` включает `vmem-integration`, но пример импортирует `aligned_vmem::PAGE`. `aligned-vmem` — транзитивная optional dependency `numa-shim`; включение feature не делает имя `aligned_vmem` доступным как прямой dependency в коде downstream crate.

Пользователь, скопировавший показанный `Cargo.toml` и пример, может получить unresolved import. Исправление: либо добавить в пример отдельную прямую зависимость `aligned-vmem`, либо re-export-нуть нужные `PAGE`/`page_size` через `numa-shim` и использовать этот re-export. Заодно стоит показать runtime `page_size()` там, где alignment должен следовать реальной странице ОС.

### F7 — P2/P3: Windows error-path и арифметика требуют дополнительного hardening review

В `src/lib.rs:1112–1114` выравнивание строится через `(raw_u + align - 1)` без checked addition. Для обычных Windows user addresses это практически недостижимо, но safety proof сильнее, если arithmetic policy явно проверяется и при невозможности возвращается `None`.

В `:1140–1145` при неудаче `MEM_COMMIT` вызывается `VirtualFree`, но результат освобождения игнорируется. При неожиданном отказе cleanup функция возвращает `None`, не имея способа сообщить возможный leak. Дополнительно результат успешного `VirtualAllocExNuma(MEM_COMMIT)` не сравнивается с ожидаемым `base`, хотя `from_raw_parts` далее принимает именно `base` (`:1162–1171`).

Это не подтверждённый текущий production bug: ветка опирается на обычные гарантии Win32 и уже имеет хороший `// SAFETY` proof. Но перед публикацией стоит либо усилить проверки, либо явно зафиксировать, почему эти условия гарантируются API и почему игнорирование cleanup failure допустимо при `Option`-контракте.

### F8 — P1: обязательное runtime-подтверждение NUMA-путей не получено

`docs/NUMA_RELEASE_GATE.md` требует перед релизом `0.x.y`, если diff затрагивает `crates/numa-shim/**`, пройти Phase 1–4: mock dispatch, real Linux kernel, Windows virtual NUMA и отдельную проверку на реальной multi-socket topology. В текущем исследовании по требованию пользователя ни один тест или runtime gate не запускался.

Статически видно, что CI содержит Windows/macOS/mock jobs и отдельный scheduled/on-demand `numa-real-kernel`, но обычный runner single-NUMA не доказывает фактическое размещение между узлами. Отдельный mbind behavioral oracle также остаётся tracked gap: текущие проверки доказывают отсутствие panic/dispatch, но не то, что raw syscall действительно принят ядром и смаршален верно.

Это не повод добавлять небезопасный synthetic test. Перед реальным tag нужен либо успешный release gate на подходящей инфраструктуре, либо явная release-note запись о пропущенной проверке с owner approval. В рамках этого read-only аудита готовность runtime не подтверждается.

## Производительность

1. **Hot-path после topology cache всё ещё не является O(1).** Cache убрал повторные `open/read/close`, но `cpu_to_numa_node` при каждом вызове проходит до 64 node entries и повторно вызывает parser для каждого cpumap (`src/lib.rs:733–740`). Это O(nodes × mask bytes) pure-Rust работа на каждом `current_node()`. Для allocator/latency-sensitive caller перспективнее `getcpu(2)` или заранее построенный обратный CPU→node index.
2. **Cold start дорогой и использует крупный stack frame.** Первый Linux-вызов выполняет до 64 sysfs `open/read/close` троек и инициализирует `Topology` размером примерно 64 KiB (`NODE_CPUMAP_BUF_LEN = 1024` × 64 плюс lengths). Allocation-free redesign правильно устранил reentrant `OnceLock`/global-allocator hazard, но стоимость cold path и stack footprint нужно считать частью API-поведения. Чтение `/sys/.../node/online` один раз или direct `getcpu(2)` может сократить этот путь.
3. **`CURRENT_NODE_SLOT: RefCell<u32>` можно заменить на `Cell<u32>`.** В `mock` slot только читается/заменяется целиком (`src/lib.rs:177–198`), поэтому borrow flag не нужен. Это небольшой defensive/perf improvement и устраняет описанный в tracked item 45 потенциальный panicking borrow.
4. **Windows two-call reserve path уже исправил главный commit-charge дефект.** Сейчас резервируется `size + align`, но commit делается только на `size`; дополнительное уменьшение syscall count потребует измерений и не должно возвращать прежнее перепотребление памяти.
5. **Mock log capped, но feature-on cost остаётся тестовым.** При включённом `mock` каждый вызов всё равно проходит TLS/recording machinery. Feature не следует включать в production dependency graph; это ещё одна причина отделить seam от Cargo feature.

Ни один speedup в этом отчёте не заявляется как измеренный: тесты и бенчмарки намеренно не запускались.

## Что перепроверено без исполнения

- Raw Linux FFI: checked границы, аргументы `mbind`, `maxnode = 65` для node 63, close paths и явные `// SAFETY` комментарии.
- Windows FFI: `PROCESSOR_NUMBER` layout assertions, reserve/commit/release ownership, `from_raw_parts` proof и feature-gated imports.
- Linux cpumap parsing: bounded buffer, loop-to-EOF, fail-closed overflow/invalid-token handling.
- Topology cache: allocation-free initialization, `OnceLock` lifetime, documented hotplug/fallback semantics.
- Cargo metadata: MSRV 1.88, no normal dependencies, optional `aligned-vmem`, license files, docs.rs feature selection and release tag wiring.
- CI/release docs: platform jobs exist, but scheduled/real-hardware claims were not treated as locally observed evidence.

## Рекомендуемый порядок перед публикацией

1. Выпустить согласованную версию вместо текущего уже опубликованного `0.1.0`: bump crate + root dependency, dated changelog и tag.
2. Исправить crate-level/README safety и `vmem-integration` example.
3. Принять owner decision по `mock`: cfg/test-support migration или явное принятие non-additive feature risk.
4. Принять решение по node ≥ 64 и fallback `Some(0)`; для общего NUMA API предпочтительнее обнаруживаемый failure, а не тихое node-0 значение.
5. Решить судьбу public hidden `cpumap` surface.
6. Запустить обязательный NUMA release gate на landing revision; в этом исследовании он не выполнялся.
7. После correctness/release gate отдельно измерить `getcpu`/reverse-index вариант и cold-start/stack cost.

**Итог:** текущая реализация выглядит статически аккуратной и заметно продвинулась по сравнению с прошлым состоянием, но публикацию этой ревизии рекомендовать нельзя из-за F1, нерешённого F2 и отсутствия обязательного runtime подтверждения F8. Остальные пункты — важные исправления документации, API-политики и кандидаты на улучшение, а не доказанные новые UB.
