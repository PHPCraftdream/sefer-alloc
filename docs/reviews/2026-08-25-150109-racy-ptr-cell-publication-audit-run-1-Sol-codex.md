# `racy-ptr-cell` — publication audit, run 1

Дата отчёта: 2026-08-25 15:01:09 +02:00  
Автор: Sol-codex  
Режим: только чтение, один агент; тесты, сборки, `check`, Clippy, Miri,
`rustdoc`, бенчмарки и `cargo publish --dry-run` не запускались.

## Вердикт

Алгоритм самого `RacyPtrCell` после просмотра с нуля выглядит корректным:
атомарный протокол `null → sentinel → real pointer`, Release/Acquire-публикация,
rollback при `None` и panic, повторная гонка проигравших и защита от sentinel
collision согласованы между кодом и loom-моделью. P0/P1-дефекта памяти,
ownership или ordering в production-коде не найдено.

Публикацию в текущем виде оцениваю как **условный NO-GO**: остаются проблемы
честности verification/documentation и дрейфа интеграционного shim-а. После их
закрытия production-часть будет готова; отдельного изменения алгоритма для
исправления найденных проблем не требуется.

## Findings

### F1 — P1 verification — тест объявлен strict-provenance-clean, но всё ещё делает integer ↔ pointer round-trip

`crates/racy-ptr-cell/tests/cell_unit.rs:91-117` передаёт адрес через канал
`usize`, затем восстанавливает указатель через
`with_exposed_provenance_mut`. Это лучше старого `as *mut T`, но не является
"strict-provenance-clean" в принятой здесь проверочной модели: сам commit
`ead400a` в полном описании признаёт, что `-Zmiri-strict-provenance`
запрещает exposed-provenance механизм. Поэтому комментарий теста обещает более
сильный результат, чем реально обеспечивает.

Это не UB в библиотечном production-коде, но тестовая гарантия публикации
сформулирована неверно. Исправление: не перевозить указатель вообще — после
`recv_timeout` получить его через `cell.get()` в текущем потоке, а из worker
передавать только факт завершения/значение; либо явно переименовать и ослабить
проверочную претензию.

### F2 — P2 integration drift — `cfg(loom)` shim в root-крейте уже не совпадает с опубликованным типом

`src/registry/bootstrap.rs:225-341` содержит отдельный
`loom_shim::RacyPtrCell`. В нём отсутствуют текущие свойства настоящего крейта:

- проверка `align_of::<T>() >= 2`;
- `RollbackGuard` для panic внутри `init`;
- release-active проверка null/sentinel результата;
- защита финального restore в `dbg_rollback_reenterable` от clobber-а
  concurrent winner.

Shim намеренно не попадает в real-type loom interleavings, потому что нужен
`const`-конструктор для root static. Это объясняет существование shim-а, но не
устраняет риск дрейфа: root loom-конфигурация компилирует и потенциально
использует поведение, которое уже не равно поведению `racy-ptr-cell`, тогда как
комментарии называют его identical/behaviourally faithful.

Рекомендация: убрать вторую реализацию протокола или сделать её минимальным
`const`-адаптером с явно проверяемым списком допустимых отличий. Как минимум,
`dbg_rollback_reenterable` в shim нельзя оставлять с безусловным финальным
`store(null)`, а комментарий должен прямо фиксировать, какие production
гарантии shim сознательно не моделирует.

### F3 — P2 contract/docs — busy-wait не ограничен по времени, но документация называет его bounded

`src/lib.rs:44-49` утверждает, что spin window "bounded and short in practice".
Фактически проигравший поток ждёт произвольное пользовательское `init`: его
можно надолго вытеснить, заблокировать на syscall или заставить зависнуть.
Документация метода правильно запрещает re-entry в ту же cell, но не формулирует
общий запрет на blocking/долгую работу.

Нужно либо честно описать примитив как ожидание завершения winner без гарантии
bounded latency, либо ввести отдельную policy/backoff-механику. Простое
переименование в `lock-free` здесь также нежелательно: произвольная closure
удерживает sentinel и может остановить прогресс всех конкурентов.

### F4 — P3 API ergonomics — `FnMut` строже, чем требуется

`RacyPtrCell::get_or_try_init` (`src/lib.rs:371-373`) принимает `FnMut`, хотя
за один вызов closure вызывается не более одного раза: проигравший поток либо
возвращает уже опубликованный pointer, либо один раз становится winner и
завершает вызов. `FnOnce` выразил бы реальный контракт и принял бы consuming
closures без потери совместимости для существующих `FnMut`/`Fn` callers.

### F5 — P3 performance — избыточная ordering на claim CAS и неполный benchmark

Успешный `compare_exchange(null → sentinel)` использует `Acquire`, хотя
первичное получение ownership не читает опубликованный объект; текущий
`Release` publish и `Acquire` loads READY/rollback должны остаться. Стоит
отдельно рассмотреть `Relaxed` на success CAS как микрооптимизацию cold path
после проверки модели памяти.

Текущий bench (`benches/racy_ptr_cell_bench.rs`) измеряет cold path вместе с
`Box::leak` и намеренно оставляет allocation на каждой итерации, поэтому не
отделяет стоимость cell от allocator/heap growth. Он также не измеряет реальную
конкуренцию и loser spin. Для полезных сравнений нужны baseline без cell,
предвыделенный payload/арена и отдельный bounded contention benchmark.

## Что сделано хорошо

- Sentinel строится без provenance и защищён release-active проверкой.
- `RollbackGuard` закрывает wedge при panic, а explicit `None` возвращает cell
  в `UNINIT`.
- Реальный loom suite проверяет exact-once, одинаковый pointer,
  Release/Acquire, OOM re-race и race debug probe; counterfactuals отделены от
  real-type доказательств.
- Исправление `dbg_rollback_reenterable` действительно условно восстанавливает
  состояние и не затирает concurrent winner.
- `RacyPtrCell<T>` не разыменовывает `T`; ownership/lifetime и безопасность
  доступа оставлены на caller, что соответствует raw-pointer API.
- В CI-конфигурации статически видны обычные debug/release тесты крейта и
  отдельный release loom job с sentinel grep; фактическое выполнение в этом
  исследовании не проводилось.
- В пакете присутствуют README, обе лицензии, CHANGELOG, manifest metadata и
  workspace-independent normal dependency surface.

## Неблокирующие решения перед публикацией

Имя `racy-ptr-cell` может быть неверно понято как наличие data races, а
description в `Cargo.toml` чрезмерно длинное для поисковой выдачи crates.io.
Это не дефект алгоритма, но разумно принять сознательное metadata-решение до
публикации. Также два `dbg_*` метода уже являются заявленной стабильной частью
API; это допустимо, но после публикации их нельзя трактовать как private test
hooks.

## Итоговый action list

1. Исправить F1: убрать integer pointer transport из unit test и устранить
   ложное утверждение про strict provenance.
2. Синхронизировать или формально ограничить root `loom_shim` (F2).
3. Переписать bounded-spin contract и явно запретить blocking init (F3).
4. В следующем API/perf проходе рассмотреть `FnOnce`, claim CAS ordering и
   benchmark baseline/contended scenario (F4–F5).

После первых трёх пунктов оснований блокировать публикацию по production
алгоритму не вижу.
