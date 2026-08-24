# Новый pre-publication review: `numa-shim`

**Автор:** Сол-кодекс (`Sol-codex`)  
**Время:** 2026-08-24 20:40:22 Europe/Berlin (UTC+02:00)  
**Ревизия:** `9137c514775ca539a99e454945cf6a6103cc7ecb`  
**Предыдущая база:** `8394108`  
**Режим:** только чтение; тесты, сборка, clippy, miri и бенчмарки не запускались; под-агенты не использовались.

## Итог

Код стал заметно лучше и в основных Linux/Windows unsafe-путях новых подтверждённых UB, double-free или неверного ownership не видно. Однако для публикации без оговорок я ставлю **NO-GO до исправления P1/P2 ниже**.

Главный оставшийся блокер — новый Linux policy-oracle может завершаться `skip` на любом `ReserveNumaError::Os`. Поэтому ошибка маршаллинга `mbind` (`EINVAL`, `EFAULT`, неверный syscall и т. п.) способна выглядеть как успешная проверка. Это не доказывает готовность флагманского API.

## Обзор последних изменений

- `66befd9`: старый повторный поиск по сырым cpumap заменён на однократно построенный reverse index CPU → node. Индекс занимает 8 KiB, lookup — O(1), heap allocation на инициализации убрана.
- `a934f72` и `2b19101`: `current_node()` теперь fail-closed; отсутствие topology, ошибка sysfs и неразрешённый CPU дают `None`, а не ложный node 0. `NodeResolution` переименован в более точный `TopologyUnavailable`.
- `cb63082`: `NodeId::new` больше не принимает `NO_NODE`; platform-specific валидность оставлена fallible reservation API.
- `631fc72`: добавлены kernel policy oracle, mock failure injection и проверки cleanup после ошибки policy; расширено покрытие parser/reverse-index и node resolution.
- `54ff7dd`: исправлены формулировки о soft preference и re-export `Reservation`.
- `7905a0c`: Windows address reconstruction переведён с integer pointer round-trip на `.addr()`/`.with_addr()`; 64-bit Windows оформлен как явная policy.
- `9137c51`: CI safety allowlist дополнен новым debug-hook.

## Findings

### P1 — Linux policy oracle скрывает ошибки реализации

**Место:** `crates/numa-shim/tests/policy_oracle_linux.rs:190-201`.

Тест пропускается для любого `ReserveNumaError::Os`. В эту группу попадают не только ожидаемые ограничения контейнера/cgroup, но и ошибки реализации: `EINVAL` из-за неправильного адреса, длины или `maxnode`, `EFAULT`, `ENOSYS` при неверном syscall и другие.

В результате регрессия, при которой `mbind` всегда ломается, может оставить CI зелёным через `skip`. Для oracle это критично: тест должен падать на неожиданных errno и пропускаться только на явно разрешённом наборе environment-dependent отказов.

**Исправление:** разделить ожидаемые capability/environment ошибки и implementation errors; как минимум `EINVAL`, `EFAULT` и `ENOSYS` должны быть failure, а не skip. Добавить отдельную диагностику errno и проверять, что positive oracle действительно выполнился.

### P2 — `EINTR` во время первой загрузки topology навсегда выключает NUMA detection

**Место:** `crates/numa-shim/src/lib.rs:1452-1492`.

Любой `read()` с `-1` сразу закрывает fd и отбрасывает cpumap. Затем неполный reverse index сохраняется в `OnceLock` на весь процесс. Один сигнал в момент первого вызова может навсегда превратить корректную topology в `None`.

**Исправление:** захватывать errno до `close()` и повторять `open`/`read` при `EINTR`; для остальных ошибок сохранять fail-closed поведение. Желательно добавить bounded retry, чтобы не получить бесконечный цикл при pathological signal storm.

### P2 — новый policy-oracle сам содержит integer pointer round-trip

**Место:** `crates/numa-shim/tests/policy_oracle_linux.rs:215, 273` и `:123-150`.

Адрес переводится `pointer → usize`, затем внутри helper обратно `usize → *mut c_void`. Для syscall это не нужно: helper может принимать raw pointer напрямую. В текущем Rust это обычно работает, но нарушает strict-provenance дисциплину, которую этот же change справедливо применил к Windows production path.

**Исправление:** передавать `*mut c_void` напрямую от `r.as_ptr().add(page).cast()`, не превращая его в integer.

### P2 — mock-тест оставляет scripted failure в thread-local состоянии

**Место:** `crates/numa-shim/tests/mock_dispatch.rs:280-329`.

`policy_failure_script_for_other_node_does_not_fire` намеренно оставляет failure для node 5 armed и не вызывает `clear_policy_failure()` в конце. Поскольку состояние thread-local, последующее выполнение на том же worker thread зависит от порядка тестов. Сейчас некоторые соседние тесты очищают состояние в начале, но это хрупкая и неявная страховка.

**Исправление:** очищать состояние в конце теста через явный guard/cleanup helper. Удалить из теста зависимость от порядка запуска.

### P2 — 64-bit Windows остаётся только документационной policy

**Места:** `crates/numa-shim/Cargo.toml:18-24`, `README.md`, Windows-тест в `tests/smoke.rs:467-479`.

Репозиторий говорит, что 32-bit Windows не поддерживается, но compile-time gate отсутствует. При этом тестовый `MEMORY_BASIC_INFORMATION` layout прямо предполагает `_WIN64`. Пользователь 32-bit target может получить собираемый, но неподдерживаемый артефакт и ошибочную уверенность в совместимости.

**Исправление:** либо реально поддержать 32-bit Windows с отдельными FFI-layouts, либо добавить жёсткий `compile_error!`/target gate для 32-bit Windows. Для заявленной policy второй вариант безопаснее.

### P3 — Windows fast path остаётся неиспользованной возможностью ускорения

`reserve_aligned_numa` всегда проходит через reserve + commit. Для `align <= 64 KiB` возможен one-call `MEM_RESERVE | MEM_COMMIT`, если runtime-проверка фактического выравнивания сохраняет fallback на текущий путь. Кандидат уже зафиксирован в `docs/perf/OPEN_ITEMS.md` item 60, но не измерен.

Это не correctness blocker: оптимизировать следует только после A/B на реальном Windows и проверки commit-charge/tail state.

### P3 — reverse index исправил алгоритмическую стоимость, но выигрыш не измерен

Новая схема действительно убрала O(nodes × cpumap-bytes) на каждом lookup и уменьшила init storage примерно с 64 KiB до 8 KiB index + 4 KiB scratch. Но сравнение с `getcpu(2)`/другими вариантами и реальная стоимость cold start не измерены. На Linux первый вызов всё ещё делает до 64 последовательных `open/read/close`.

Следующий шаг — отдельный benchmark/path-activation oracle, без изменения корректного текущего reverse-index решения до получения чисел.

## Что проверено как исправленное

- `current_node()` больше не маскирует неразрешённую topology под `Some(0)`.
- Reverse index использует единый parser, проверяет malformed input fail-closed и не аллоцирует heap внутри `OnceLock` initializer.
- `NodeId` исключает глобальный `NO_NODE` sentinel.
- Linux `mbind` проверяется по return value, errno сохраняется до cleanup, ошибка policy освобождает reservation.
- Windows strict-provenance reconstruction теперь сохраняет provenance исходной reservation.
- `MPOL_PREFERRED` корректно описан как soft preference; binding применяется к полному OS reservation span.
- Новые `#[non_exhaustive]` и re-export docs в целом согласованы с публичной поверхностью.

## Рекомендованный порядок перед публикацией

1. Сделать policy oracle fail-fast для неожиданных errno.
2. Исправить `EINTR` retry в topology reader.
3. Изолировать и очищать mock failure state; убрать pointer integer round-trip из oracle.
4. Принять технически enforceable решение для 32-bit Windows.
5. После этого отдельным измерением решить вопрос Windows one-call path и cold-start/`getcpu` оптимизации.

**Финальный вердикт:** архитектурно проект близок к публикации, но текущая доказательная база Linux policy и несколько boundary-деталей ещё не соответствуют режиму «без компромиссов». После исправления P1/P2 — кандидат на GO; P3 остаются улучшениями производительности, не блокерами.
