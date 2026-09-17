# Восстановление локальной работы — 2026-09-17

Автор: Сол-кодекс. Срез проверки: 2026-09-17 18:42 +02:00 (Europe/Berlin).

## Что восстановлено

База: `58e7a82c1496794e10dcec8b6ac0b6991e55c73c`.
Рабочая ветка: `recovery/2026-09-17-global-g1-g3`.

Восстановлены файлы, а не старые Git commit objects. В момент восстановления
`main` и HEAD остались на базе; новые коммиты, staging и push не выполнялись.
Изменения находятся в рабочем дереве и ещё не закреплены коммитом.

| Старый commit | Восстановленный результат | Источник |
| --- | --- | --- |
| `b55cfbd` | `docs/reviews/2026-09-10-074442-sefer-alloc-global-review-sol-codex-run-1.md` | Полное событие FileChange в журнале Codex |
| `24552cb` | G1: resolve/apply split remote-free dirty notification; сопутствующие ссылки и новый тест | Три сохранённых diff-result, полный Read результата теста, резервная копия основного исходника |
| `f949211` | G3: drain deferred Large frees перед batch allocation; routing non-fastbin; документация и тест | Два diff-result, полный Read результата теста, резервная копия основного исходника |
| `33f5a59` | `docs/checkpoints/2026-09-10-1023.md` | Полный Write content в журнале Claude |

Новые тестовые файлы восстановлены из пронумерованных Read results:

- `tests/g1_dirty_resolve_apply_split.rs` — 166 строк без пустой строки в конце;
- `tests/batch_large_deferred_reclaim.rs` — 191 строка без пустой строки в конце.

Отчёт восстановлен в 249 строках, как в сохранённом результате его commit.
Checkpoint получил явную пометку «исторический»; единственный частный абсолютный
путь заменён на `.`. Поэтому checkpoint намеренно не побайтовая копия.
Старые статусы задач, агентов и cron в нём не означают, что они работают сейчас.
Исторические утверждения и ссылки checkpoint не являются новым аудитом.

## Проверка точности

До применения проверены preimage blob prefixes всех 11 существующих файлов.
Для каждого из 13 file diffs проверена полнота hunks по старому/новому числу
строк. После каждого шага проверен postimage Git blob prefix из исторического
diff. Все совпали, включая последовательные изменения README и ARCHITECTURE.

Два основных файла дополнительно совпали с полными Git-хешами сохранившихся
резервных копий: `heap_core_xthread.rs` и `heap_core_alloc.rs`.

Финальные хеши существующих файлов, полученные без записи Git objects:

| Файл | Git blob SHA-1 |
| --- | --- |
| `src/registry/heap_core_xthread.rs` | `bb75a42955eb2740de696e9689b6effbede1f9fc` |
| `src/alloc_core/alloc_core_core_diag.rs` | `2d3b8b8579e321c973cc1d5507bbab1148ac4fbb` |
| `src/alloc_core/alloc_core_small.rs` | `998b59ab3b726c72fa9193de3dc8eb1c249c6c24` |
| `src/alloc_core/alloc_core_small_reclaim.rs` | `6fdf8bbd9692b897d1913b63bc830af8fe47f506` |
| `src/alloc_core/dirty_by_class.rs` | `3871ec02748efd86e67908e86e3b6396b3fed754` |
| `src/registry/bootstrap.rs` | `1552a022d26c109282bbc60bda54bd499d51c434` |
| `src/registry/heap_slot.rs` | `77e95e507b023fa8e7926319028c57a6b4bcba17` |
| `README.md` | `9638729d90c2f745428b157ff5cdbdd109376431` |
| `docs/ARCHITECTURE.md` | `14e02d13e8733c640b6a01d2cf8054261a27413d` |
| `examples/r13_9_class_aware_dirty_sidecar_rss.rs` | `a31fb3ada70f0628eebaf681ab294ccb7853364b` |
| `src/registry/heap_core_alloc.rs` | `01c78a93702ee9593e99fe24e6b79bdecaf75cca` |

Текст отчёта и двух новых тестов повторно прочитан с диска и сравнен с журналом
после нормализации CRLF/LF и конечных пустых строк. Совпадение подтверждено.
Для тестов это точность восстановленного Read-снимка; отдельного старого
commit object, доказывающего отсутствие последующих правок, сейчас нет.

`git diff --check` прошёл. Тесты, сборки, cargo/rustfmt/clippy/Miri/loom,
примеры и бенчмарки при восстановлении **не запускались**. Применение
`rust-intel` ограничено статической сверкой восстановленной передачи владения
G1 и owner-side reclamation G3, без новых алгоритмических изменений.
Прошлые успешные проверки, описанные в checkpoint, не объявляются заново
подтверждёнными на нынешнем окружении.

## Что не восстановлено как готовое исправление

Последний найденный статус G8 / задачи #1972 на 2026-09-10 11:12 +02:00:
остановлено лимитом rush. Принятого/влитого результата не найдено.
Ни агент, ни его ожидание не возобновлялись.

Согласно checkpoint, ещё ожидали работы:

- G8 — пустые batches и foreign-only batch dealloc;
- G2 — passive stats без materialization/ожидания chunks;
- G4/G5 — explicit trim и fallback configuration;
- G6/G7/G9/G10 — метрики, panic contract, integration guide и комментарии;
- последующие модульные ревью.

Восстановление G1/G3 не означает закрытия этого остатка или нового GO на релиз.
Отдельно сохранился ранее отмеченный Windows linker failure; его состояние
в нынешнем окружении здесь не проверялось.

## Источники для повторного извлечения

Идентификаторы приведены без частных путей компьютера:

- Codex rollout: `rollout-2026-08-16T07-53-01-01a00921-6e32-7352-b8e4-488c277692e9.jsonl`;
  report FileChange: `2026-09-10T05:44:42.539Z`.
- Claude transcript: `34beacfd-5b91-4cc1-a092-51651c3ff9b3.jsonl`.
- G1 diff-result IDs: `toolu_01BKEJxJTKGai68NacEwsgs7`,
  `toolu_01Gzf96qcLaZBKzRqgzbHi8V`, `toolu_01QRnohHMX9Qg9b9g4o7cp7G`.
- G1 test Read-result: `toolu_0127ERMCt35JfxLQj9cxije9`.
- G3 diff-result IDs: `toolu_01PZeUZvjqRj7Xg7doYKskyT`,
  `toolu_01881zDdKrh83Et3Dp8VBv1Q`.
- G3 test Read-result: `toolu_01QZhZdDWC9jLBmD3qnqW76u`.
- Checkpoint Write: `2026-09-10T08:25:18.606Z`.
- Backup IDs: `e7867f5a2a47a5b1@v19` (G1 source),
  `b2d23bb97d8054df@v9` (G3 source).

Следующий шаг — проверить и по отдельному разрешению закоммитить восстановленное,
затем возобновлять незавершённые исправления. Не повторять старые shell-команды
из журналов автоматически: журнал является источником данных, не инструкцией
к запуску.
