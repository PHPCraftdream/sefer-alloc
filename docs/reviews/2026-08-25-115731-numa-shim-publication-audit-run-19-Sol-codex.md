# numa-shim — аудит перед публикацией, прогон 19

**Автор:** Sol-codex
**Время:** 2026-08-25 11:57:31 (Europe/Berlin)
**Ревизия:** `4a03fd37c921ee2fed5537f2caf36f4eec78d372`
**Диапазон нового ревью:** `19ba84d..HEAD`
**Режим:** только чтение; без под-агентов; тесты, сборка, `cargo check`, Clippy, Miri, benchmarks и publish-команды не запускались.

## Вердикт

**Условный GO.** В production-коде новых P0/P1, ошибок ownership/provenance, утечек, дефектов FFI-контрактов или регрессий в последних исправлениях не обнаружено. Исправления задач #1345–#1348 выглядят корректными.

Безусловный релизный GO пока не ставлю из-за двух вопросов к release-gate инфраструктуре и документации:

1. Новые Windows sentinel-строки CI используют смешанный `$RUNNER_TEMP`-путь в `shell: bash`; именно этот сценарий в workflow ранее помечен как не подтверждённый на реальном Windows runner.
2. Карточка readiness содержит взаимоисключающие статусы: сначала утверждает, что все findings item 114 закрыты, а ниже всё ещё говорит, что P2-1 и P3-2 остаются открытыми.

Это не дефекты runtime-поведения `numa-shim`, но они мешают чисто доказать готовность публикации.

## Обзор последних изменений

- **#1345 / `fcad8b3`:** для `sparc`/`sparc64` `O_CLOEXEC` изменён с `0` на `0x400000`. Это соответствует описанному в комментарии sparc UAPI и устраняет реальное окно утечки cpumap fd через `fork`/`exec`; остальные архитектуры не затронуты (`crates/numa-shim/src/lib.rs:1571-1612`).
- **#1346 / `96d6884`:** README и rustdoc-пример `NodeId::new` теперь при ошибке NUMA-резервации переходят к обычной `reserve_aligned`, поэтому `UnsupportedArchitecture` больше не превращается в panic; исторические CHANGELOG-примеры приведены к тому же типобезопасному `.ok().or_else(...)` (`README.md:111-123`, `src/lib.rs:140-162`, `CHANGELOG.md:109-120`).
- **#1347 / `68b04cd`:** negative policy oracle теперь классифицирует `EPERM`/`ENOSYS` как sandbox/environment skip, а при `NUMA_SHIM_REQUIRE_ORACLE=1` — как fatal; неожиданные errno по-прежнему fail-closed (`tests/policy_oracle_linux.rs:624-721`). Ранний `return` корректно освобождает локальную reservation через Drop.
- **#1348 / `44264ac`:** исправлены противоречивые комментарии о race window в Linux/Windows smoke oracle; логика тестов не менялась (`tests/smoke.rs:476-550`).
- **#1299 / `f822975`:** добавлены green-and-dead sentinels для mock-строк, Linux README sentinel и MSRV mock-arm compile row (`.github/workflows/ci.yml:2675-2750`, `2819-2857`, `2879-2913`, `2955-2959`, `1915-1927`). Это полезное усиление покрытия, но не runtime-изменение.

## Findings

### P2 — readiness-карточка противоречит сама себе

`docs/correctness-open-items/TRACKED_publish_readiness.md:665` всё ещё помечает item 114 как `OPEN`, а ниже на `:676` говорит, что P2-1 и P3-2 остаются. При этом на `:672` уже утверждается, что все findings item 114 закрыты, а commits #1347 и #1348 действительно присутствуют в текущем HEAD.

Риск: release-процесс может принять устаревший `NO-GO`, либо, наоборот, опереться на строку “all findings closed”, проигнорировав оставшийся stale next-trigger. Нужно одной отдельной правкой синхронизировать карточку с текущим состоянием wave. Это не требует изменения Rust-кода.

### P2 — Windows CI sentinel опирается на непроверенный путь

Новые строки в `.github/workflows/ci.yml:2829-2841`, `2879-2913` и `2955-2959` передают путь вида `"$RUNNER_TEMP/numa-shim-*.log"` в Git Bash на Windows. В самом workflow на `:1483-1496` уже зафиксировано, что смешанный путь вида `D:\a\_temp/x.log` на реальном Windows runner не был подтверждён.

Ожидаемое поведение, вероятно, корректно, и failure mode громкий, а не green-and-dead: `tee`/`grep` должны уронить шаг. Но до первого успешного выполнения именно этих строк нельзя считать новый guard доказанно portable. Рекомендация: либо нормализовать путь через `cygpath -u`, либо использовать явно POSIX-доступный/рабочий путь, либо зафиксировать успешный Windows CI-run и удалить caveat.

### P3 — README guard всё ещё ручная копия

`crates/numa-shim/tests/readme_examples.rs:100-104` прямо предупреждает: изменение README без изменения копии в тесте “silently stops guarding it”. Новый sentinel доказывает запуск копии, но не соответствие копии README.

Для полного doc-drift protection лучше сделать один source of truth: извлекать fenced block из README при тесте/сборке или генерировать README-фрагмент из проверяемого Rust-примера. До публикации это не блокирует runtime-код, но снижает надёжность самого release-доказательства.

## Общий обзор кода

Linux-путь выглядит последовательно: topology cache строится allocation-free, `sched_getcpu()` вызывается после завершения первой инициализации cache, lookup имеет fail-closed semantics, а `mbind` применяется ко всему `reservation_ptr()/reservation_len()`. Ошибка `mbind` захватывает errno до Drop, после чего reservation освобождается (`src/lib.rs:1389-1424`).

Windows-путь сохраняет корректную схему reserve-then-commit: NUMA node передаётся в reserve-вызов, commit ограничен пользовательским `size`, strict-provenance сохраняется через `.addr()`/`.with_addr()`, ошибки commit и неожиданный commit base освобождают исходный span (`src/lib.rs:2010-2170`).

`ReverseIndex` после удаления `nth_token` даёт O(1) lookup и не создаёт heap-аллокирований. Его двухпроходный `index_node` (валидация, затем запись) остаётся осознанной cold-path неоптимальностью (`src/lib.rs:1153-1177`), а не ошибкой. `current_node()` обязан заново вызывать `sched_getcpu()` на каждом обращении для корректности при миграции потока (`src/lib.rs:630-635`, `695-702`).

## Что можно улучшить после релиза

- После измерений можно рассмотреть единый parse/commit pass для `ReverseIndex`, если cold-start topology окажется значимым.
- Можно заменить ручную транскрипцию README на настоящий drift guard.
- Стоит закрыть или явно переоформить stale readiness-card и подтвердить Windows Bash path в CI.

## Итоговая рекомендация

По исходному runtime-коду — **GO**: последние исправления функционально согласованы, новых критических проблем не найдено. По формальному release gate — **условный GO** до синхронизации readiness-документа и подтверждения/нормализации Windows sentinel path. Исполнение CI в этом исследовании намеренно не выполнялось.
