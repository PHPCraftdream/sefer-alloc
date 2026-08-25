# numa-shim — static publication audit, run 18

Дата отчёта: 2026-08-25 09:11:56 (Europe/Berlin)  
Автор: Sol-codex  
Ревизия: `5ff29cbeaecfa86fbbfc736bfad34016fb0d9c10`  
Режим: только чтение, без под-агентов и без запуска тестов.

## Итог

Строгий GO пока не ставлю: найдено два остаточных дефекта, один из них относится к реальному Linux FFI-поведению, второй — к публичным примерам.

P0/P1, нового UB, ошибки владения reservation или ошибки в основном Linux/Windows cleanup-пути не обнаружено. Предыдущий блокирующий F1 из run 17 закрыт: оба `text`-примера в `src/lib.rs` теперь типово согласованы, а их компилируемые копии добавлены в `tests/readme_examples.rs`.

## Новые замечания

### F1 — P2 — `O_CLOEXEC` отключён на sparc/sparc64 вместо использования правильного значения

Места: `crates/numa-shim/src/lib.rs:1565-1598`.

В новом исправлении `O_CLOEXEC` оставлен равным `0` для `target_arch = "sparc"` и `"sparc64"`. Комментарий правильно отмечает, что asm-generic значение там неверно, но решение всё равно отключает close-on-exec именно на архитектурах, для которых Linux-путь detection компилируется. Для sparc UAPI значение `O_CLOEXEC` — `0x400000`.

Последствие — редкая, но реальная race-утечка временного sysfs fd через concurrent `fork()+exec()`. Это не memory-safety баг, однако это регресс/неполнота заявленного hardening для существующих Linux targets. Перед публикацией лучше определить архитектурный константный путь:

```rust
#[cfg(any(target_arch = "sparc", target_arch = "sparc64"))]
const O_CLOEXEC: c_int = 0x400000;
```

Альтернативное отключение hardening приемлемо только вместе с явным исключением этих архитектур из поддерживаемого Linux detection surface. Текущий код такого исключения не делает.

### F2 — P2 — публичные примеры падают на Linux-архитектурах с detection, но без placement backend

Места: `crates/numa-shim/README.md:97-123`, `crates/numa-shim/src/lib.rs:140-157`, а также oracle-копия `tests/readme_examples.rs:64-76,190-201`.

На Linux-архитектуре, отличной от x86_64/aarch64, `current_node()` может вернуть `Some(node)`, после чего `reserve_preferred_on_node(...)` по документированному контракту возвращает `Err(UnsupportedArchitecture)`. Первый README-пример и `NodeId::new`-пример делают `.expect(...)`, то есть копирование официального примера приводит к panic. Сам тестовый файл уже честно признаёт эту caveat, но это означает, что известная проблема оставлена в публичном примере.

Исправление: либо сделать оба примера действительно best-effort через `.ok().or_else(|| reserve_aligned(...))`, либо явно обрабатывать `UnsupportedArchitecture` и переходить к plain reservation. После этого тот же сценарий должен быть отражён в compile/runtime oracle.

## Качество последних правок

Проверен диапазон новых изменений от прошлого аудита до текущего `HEAD`:

- #1336: preflight `get_mempolicy` теперь классифицирует `EPERM/ENOSYS`, а strict CI остаётся fatal.
- #1337: smoke-ожидания стали учитывать Linux architecture gating.
- #1338: реальный Linux policy-oracle получил два green-and-dead sentinel grep.
- #1339: исправлена область действия `O_CLOEXEC`, очищен CHANGELOG; residual описан в F1.
- #1340: уточнены race-комментарий release oracle и stack budget `OnceLock`; добавлен README example oracle.
- #1341: исправлены два некомпилируемых публичных rustdoc-примера и лишний импорт в crate usage.
- #1342: `CURRENT_NODE_SLOT` заменён на `Cell<u32>`, удалён ставший мёртвым `nth_token`, уточнён Cargo cfg-комментарий.

В целом последние изменения адресные и не вносят очевидного регресса в production reservation path. Особенно хорошо закрыты: immediate errno capture перед cleanup, освобождение reservation после неудачного `mbind`, full OS reservation span для policy, strict-provenance Windows pointer path, fail-closed topology lookup и architecture-aware test expectations.

## Остаточный test/CI smell

### F3 — P3 — README compile oracle всё ещё допускает тихий drift

`tests/readme_examples.rs` вручную дублирует README. Его собственный комментарий (`:84-88`) прямо признаёт: изменение README без синхронного изменения копии прекращает проверку README, а CI продолжает быть зелёным.

Это полезный compile oracle для текущей копии, но не настоящий drift guard. Лучше сделать README source-of-truth: извлекать fenced block через небольшой read-only/build-time guard, либо хранить пример в одном `.rs`-файле и включать его в README. Это P3, не блокер текущего production-кода.

## Производительность и упрощения

- `ReverseIndex::index_node` дважды интерпретирует каждый cpumap: сначала validation pass, затем write pass (`src/lib.rs:1152-1171`). Это безопасная transactional-схема, но холодный first-call path получает примерно двойную работу для каждого из до 64 nodes. Ускорять стоит только после измерения; возможный вариант — parse в фиксированный временный список/bitmap и применять изменения после успешного parse.
- После инициализации `current_node()` всё ещё делает `sched_getcpu()` на каждый вызов (`src/lib.rs:1355-1362`). Это необходимо для корректности при миграции thread; не следует заменять его постоянным node-кэшем без нового API-контракта. Нужен real-backend benchmark, а не mock benchmark, прежде чем предпринимать оптимизацию.
- Инициализация `OnceLock` сохраняет фиксированный stack footprint примерно 12 KiB, потенциально до ~20 KiB с transient move. Документация теперь честно это сообщает. Для стандартного стека это нормально; для tiny custom stacks остаётся documented operational constraint.
- Удаление `nth_token` и переход `parse_each_set_cpu` на `rsplit` оставляют parser на линейной сложности по размеру cpumap. Новых очевидных O(n²) участков в production cold path не найдено.

## Проверенные границы

Статически просмотрены public API и rustdoc, Linux raw FFI/syscall marshalling, cpumap parser/reverse index, `OnceLock` initialization, EINTR policy, Windows reservation/commit/release/provenance, mock TLS state, feature/cfg matrix, README/CHANGELOG и CI release-oracle rows. Тесты, сборка, `cargo check`, Clippy, Miri, benchmarks, docs build и publish-команды в этом исследовании не запускались.

`git diff --check` для исследованного диапазона не выявил whitespace-ошибок. Пользовательские незакоммиченные checkpoint/log-файлы не изменялись.

## Рекомендованный порядок

1. Исправить sparc/sparc64 `O_CLOEXEC` или явно убрать эти архитектуры из detection support contract.
2. Сделать строгий `NodeId`/README пример безопасным на `UnsupportedArchitecture`, затем обновить его oracle.
3. По желанию усилить README drift guard и отдельно измерить real-Linux warm/cold path перед performance changes.

После F1 и F2 статический verdict можно поднять до GO по code-level readiness; F3 и performance пункты остаются улучшениями, а не correctness blockers.
