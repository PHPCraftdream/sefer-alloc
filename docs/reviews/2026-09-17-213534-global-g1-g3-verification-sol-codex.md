# Проверка восстановленных G1/G3

Автор приёмки: Сол-кодекс. Дата: 2026-09-17 21:35:34 +02:00, Europe/Berlin.
Независимый исполнитель: ha / Aristotle, сессия `01a0b0cb-2d6a-7990-bc3c-7a3e9acadbfc`.

База Git: `58e7a82c1496794e10dcec8b6ac0b6991e55c73c`.
Текущая ветка: `recovery/2026-09-17-global-g1-g3`.
На момент приёмки результат находился в рабочем дереве без новых коммитов.
Последующее оформление по запросу пользователя записано ниже.

## Вывод

**G1 и G3 закрыты в рассмотренных механизмах и проверенных конфигурациях.**
Восстановленный G1 сохранял правильное исправление lifetime. Восстановленный
G3 требовал дополнительной доработки: non-fastbin Large batch не обслуживал
Small overflow queue. Эта ветвь исправлена и покрыта регрессией, которая
действительно падала до изменения.

Это проверка двух восстановленных исправлений из
[исходного ревью](2026-09-10-074442-sefer-alloc-global-review-sol-codex-run-1.md).
G2, G4, G5, G6, G7, G8, G9 и G10 в этом задании не закрывались;
общий GO для allocator отсюда не следует.

## Что проверено и доработано

### G1: lifetime remote-free notification

В `src/registry/heap_core_xthread.rs` оба успешных ring-пути получают
`ResolvedDirtyTarget` до публикации free record. Snapshot содержит ссылку
на process-lived HeapSlot и числовые поля. После публикации notification
использует эти данные; повторного чтения заголовка освобождаемого сегмента нет.
Успешные overflow-пути возвращаются без последующего чтения сегмента.

Старый structural test был полезен, но не проверял выполнение notification
после освобождения памяти. Добавлен тест
`delayed_notification_survives_actual_segment_release`:

1. Выделяет блоки до появления non-primordial, non-current Small segment.
2. Оставляет в нём ровно один live block и заранее разрешает notification target.
3. Публикует единственную free record через существующий диагностический ring hook.
4. Выполняет owner drain и pool release; проверяет отсутствие сегмента в таблице.
5. На другом потоке вызывает реальный apply helper из сохранённого snapshot.
6. Проверяет переход coarse dirty bit из 0 в 1.

Test hook `HeapCore::dbg_resolve_dirty_notification` требует `internals`,
`bench-internals`, `alloc-xthread` и `alloc-segment-directory`.
Он объявлен unsafe из-за caller-provided live pointer на этапе resolve.
Возвращаемая closure — `Send + 'static`; сегментный указатель не захватывает.
Тест исключает конкурирующие writers/consumer для проверяемого dirty bit.

Структурный guard теперь фиксирует весь набор полей snapshot; его роль
дополняет runtime test. Это остаётся текстовым guard, а не Rust parser.
Production retry ordering проверен чтением кода и structural test;
принудительная остановка настоящего production producer внутри retry-loop
не реализована. Runtime-сценарий проверяет lifetime реальных resolve/apply
helpers после owner release.

### G3: batch reclamation

Large batch с fastbin и без него вызывает `drain_large_deferred_free`.
Non-fastbin Small batch вызывает `drain_heap_overflow`. При приёмке дополнительно
восстановлена эквивалентность scalar prelude для non-fastbin **Large** batch:
добавлен `drain_heap_overflow` в `alloc_batch_large` при
`alloc-xthread && !fastbin`.

Проверки в `tests/batch_large_deferred_reclaim.rs`:

- 64 цикла batches по 4 Large blocks с освобождением другим потоком;
- проверка уникальности одновременно выданных указателей;
- ограничение high-water таблицы именно тестируемого heap;
- bounded channels и acknowledgment до следующего цикла;
- финальная deferred-партия убирается после assertions, поэтому cleanup
  не может скрыть пропущенный batch drain;
- отдельные Small- и Large-batch сценарии для non-fastbin overflow:
  заполнить segment ring, поместить следующий блок в heap overflow,
  выполнить batch и проверить bitmap именно этого блока.

Process-wide release counter сохранён как дополнительный сигнал;
его рост сам по себе не доказывает работу нужного heap. Основной G3 oracle —
per-heap table high-water. Для overflow oracle выбран иной размер batch,
чтобы batch не мог тут же забрать проверяемый освобождённый блок.

`unsafe impl Send for SendPtr` остаётся только в тесте; перед ним записано
обоснование однократной передачи владения. Callback G1 получает Send
автоматически. Подход `rust-intel` повлиял на проверку времени жизни,
контракт тестового unsafe hook и обязательные отрицательные контроли.

## Отрицательные контроли

До передачи ha оркестратор получил:

- новый non-fastbin Large-overflow test падал на восстановленном коде:
  `Large batch skipped the non-fastbin overflow drain`; после добавления
  drain прошёл;
- отключение Large deferred drain при batch size 1 привело к ожидаемому
  провалу: `baseline=1, max_hwm=65, final=65, bound=9`;
  вызов drain затем возвращён.

По финальному результату ha:

- исключение apply из callback ломает G1 runtime oracle;
- исключение Large deferred drain при batch size 4 даёт
  `baseline=1, max_hwm=257, final=257, bound=9`;
- исключение overflow drain в соответствующей ветви ломает каждый из двух
  non-fastbin overflow tests.

Агент восстановил мутации и повторил положительные проверки.
При приёмке временных отключений в коде не обнаружено.

## Проверки оркестратора после переноса

Окружение: Rust/Cargo 1.97.0, `x86_64-pc-windows-msvc`.

| Проверка | Результат |
| --- | --- |
| Production debug: G1 / G3 / batch_tcache | 5 + 1 + 7 = 13 passed |
| Non-fastbin с decommit и coarse directory: те же targets | 5 + 3 + 7 = 15 passed |
| Учёт диагностических hooks и регрессии scanner | 7 passed |
| Инвентаризация и ссылки документации | 17 passed |
| Production Clippy, G1/G3 targets, warnings as errors | exit 0 |
| Rustfmt check двух тестовых файлов | exit 0 |
| git diff --check | exit 0 |

Это 28 успешных выполнений целевых тестов в двух конфигурациях плюс
24 проверки hooks/документации. Повторные исполнения одних тестов
в разных конфигурациях не считаются уникальными тестами.

Две дополнительные integration-проверки сначала обнаружили недостающую запись
нового hook в `UNSAFE_HOOKS` и устаревший README-счётчик item-scoped allows.
Hook добавлен в реестр с обоснованием lifetime; в README для
`heap_core_xthread.rs` указаны два allow-sites, общий счётчик исправлен
с 93 на 94. Правила самих guards не ослаблялись. Повторный запуск дал
7/7 и 17/17; незакрытых падений этих проверок не осталось.

Точные команды:

```powershell
cargo test --locked -j 2 --features "production batch-api internals bench-internals" --test g1_dirty_resolve_apply_split --test batch_large_deferred_reclaim --test batch_tcache

cargo test --locked -j 2 --features "alloc-global alloc-xthread alloc-decommit alloc-segment-directory batch-api internals bench-internals" --test g1_dirty_resolve_apply_split --test batch_large_deferred_reclaim --test batch_tcache

cargo clippy --locked -j 2 --features "production batch-api internals bench-internals" --test g1_dirty_resolve_apply_split --test batch_large_deferred_reclaim -- -D warnings

rustfmt --edition 2021 --check tests/g1_dirty_resolve_apply_split.rs tests/batch_large_deferred_reclaim.rs
git diff --check
```

Дополнительный запуск после интеграции:

```powershell
cargo test --locked -j 2 --features "production batch-api internals bench-internals" --test dbg_hook_safety_tripwire --test no_stale_doc_references
rustfmt --edition 2021 --check tests/g1_dirty_resolve_apply_split.rs tests/batch_large_deferred_reclaim.rs tests/dbg_hook_safety_tripwire.rs
```

В non-fastbin конфигурации остаются warnings в существующих
`heap_core_diag.rs`, `alloc_core_small.rs` и `magazine_bitmap.rs`.
Тесты прошли; строгий Clippy запускался в production-конфигурации.

## Дополнительная матрица ha

Следующие результаты приняты из финального сообщения агента после просмотра
его diff. Это отдельные запуски в worktree; они не выдаются за дополнительные
запуски оркестратора:

| Режим | Targets / результат |
| --- | --- |
| Production debug | G1/G3/batch_tcache: 13 passed |
| Non-fastbin без decommit/directory | G1/G3/batch_tcache: 14 passed; G1 runtime исключён cfg |
| Non-fastbin + decommit + coarse directory | G1/G3: 8 passed |
| Non-fastbin + decommit + class-aware-dirty | G1/G3: 8 passed |
| Production + hardened | G1/G3/batch_tcache: 13 passed |
| Production + alloc-stats, смежные механизмы | 13 passed |
| Production release | G1/G3/batch_tcache: 13 passed |
| Non-fastbin release + decommit + coarse directory | G1/G3: 8 passed |
| cargo check --lib --features production | exit 0 |
| cargo check --lib --no-default-features --features batch-api | exit 0 |

Смежные test targets: `dirty_segments_a4`, `class_aware_dirty_routing`,
`class_aware_dirty_oom_latch`, `r11_2_overflow_drain_pool_release`,
`r34_14_deferred_next_reset_on_cache_hit`.
Для них агент включал `production alloc-stats internals bench-internals`.

Miri, ASan и полный workspace test suite не запускались.
Новых измерений производительности нет.

## Приёмка и сохранность

Агент изменил относительно переданного ему состояния только три файла:
`src/registry/heap_core_xthread.rs` и два новых теста. Все остальные
восстановленные файлы совпали побайтово. Эти три diff просмотрены
оркестратором и перенесены через apply_patch. После переноса все 16 файлов
задачи совпали с agent worktree. Затем оркестратор исправил один устаревший
комментарий о числе блоков на цикл в batch test и синхронизировал реестр
unsafe hooks (`tests/dbg_hook_safety_tripwire.rs`) и README с новым test hook.

Агент закрыт. Его временный worktree и ветка `ha/global-finish` удалены
после повторной проверки сохранности изменений в текущем рабочем дереве.
Исходники и тесты сохранены; удалённый build cache можно пересобрать.

Финальные Git blob SHA-1 ключевых файлов при записи отчёта:

| Файл | SHA-1 |
| --- | --- |
| `src/registry/heap_core_alloc.rs` | `ac87c60f3ec83263dcc32fc23b1756349c40f9bb` |
| `src/registry/heap_core_xthread.rs` | `c4fd217a70ff5de766b24088c8a2c3faa5deadd4` |
| `tests/batch_large_deferred_reclaim.rs` | `8c6760101044b6e352ec8749b38d637f41f452b4` |
| `tests/g1_dirty_resolve_apply_split.rs` | `d3b94324735dc6adf3b31b0b713b461f3aace3a7` |

Хеши в [записи о восстановлении](../checkpoints/2026-09-17-restoration-sol-codex.md)
относятся к прежнему шагу: последующая проверка намеренно доработала код и тесты.

## Подготовка к отправке в CI

По следующему запросу пользователя работа разделена на коммиты:

- `46b8459b`: историческое ревью и checkpoint;
- `95aceefb`: G1, lifetime notification и его регрессии;
- `36d8f25c`: G3, оба non-fastbin overflow paths и batch-регрессии;
- `34a3466f`: отдельный CI job для G1/G3 с fastbin и без него.

Полный `cargo fmt --all -- --check` обнаружил перенос строки в новом hook;
он исправлен rustfmt. После форматирования blob `heap_core_xthread.rs` —
`cad94a9329683f7b5be16509b384d03512fa4a59`; предыдущая таблица сохраняет
идентичность снимка приёмки до этого изменения.

Новый CI job проверяет выполнение именованных регрессий через шесть
дополнительных sentinels. `node scripts/verify-ci-sentinels.mjs` проверил
88/88; floor и текущая карточка item 87 синхронизированы.
Результат удалённого CI здесь заранее не утверждается.
