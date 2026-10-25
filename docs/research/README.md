# Архив исследований owner-path — 2026-10-24/25

Это архив документации и воспроизводимости, **не принятие экспериментального allocator-кода**. Shipping source, Cargo feature composition, версии, examples и tests основной ветки этим коммитом не меняются. Файлы исходного кода и patches внутри архива — измерительные snapshots, не подключённые к сборке проекта.

## Исследования

| Snapshot | Итог | Архивируемых документов и артефактов | Stored bytes |
|---|---|---:|---:|
| [native-alloc-leaf](native-alloc-leaf/perf/NATIVE_ALLOC_LEAF_NATIVE.md) | INCONCLUSIVE / DO NOT PROMOTE | 420 | 2565533 |
| [native-mixed-judge](native-mixed-judge/perf/NATIVE_MIXED_CALIBRATION.md) | JUDGE FAIL / PRODUCT CANDIDATE NOT OPENED | 55 | 776979 |
| [own-free-context](own-free-context/perf/OWNER_FREE_RESEARCH_GATE.md) | NO-GO FOR PROMOTION; native speedup не установлен | 2995 | 6116850 |
| [own-free-cost](own-free-cost/perf/OWN_FREE_OVERFLOW_OUTLINE_GATE.md) | NO-GO: overflow control | 355 | 5952845 |
| [own-free-inline](own-free-inline/design/2026-10-25-scalar-mir-prereg.md) | Не доминирует B; не выбран | 18 | 124885 |
| [own-free-single](own-free-single/perf/OWN_FREE_SINGLE_FLUSH_GATE.md) | Сокращение Ir; shipping acceptance отсутствует | 351 | 5256918 |
| [own-free-split](own-free-split/perf/OWN_FREE_HALF_FLUSH_SPLIT_GATE.md) | NO-GO: free micro-gate | 383 | 7299060 |
| [scalar-issue-prepared](scalar-issue-prepared/perf/SCALAR_ISSUE_PREPARED_GATE.md) | INCONCLUSIVE / DO NOT PROMOTE; correctness execution не подтверждена | 1928 | 1930333 |

Общий размер payload: 30023403 stored bytes, 32624474 uncompressed bytes; 6505 файлов из восьми исследовательских trees. Метаданные manifests, этот индекс и verifier считаются отдельно. Все количества выведены из [inventory.json](inventory.json), не оценены вручную.

## Что сохранено

- Все изменённые, новые и ignored файлы внутри docs/ исследовательских trees, включая пререгистрации, отчёты, companion CSV, raw samples, failed attempts, source identities, patches, asm и reproduction scripts.
- Изменённые ARCHITECTURE/OPEN_ITEMS snapshots вложенных trees — историческое состояние эксперимента. Они **не заменяют** действующие [основной perf index](../perf/OPEN_ITEMS.md) и [correctness index](../CORRECTNESS_OPEN_ITEMS.md).
- Для каждого tree: pinned base SHA и `manifest.json` либо `manifest.json.gz`, связывающий исходный docs/path с архивной копией. Отдельные зависимости воспроизводимости — [supplements.json](supplements.json).
- Основная [общая картина](../design/2026-10-25-owner-path-global-picture.md) и checkpoints остаются вне исторических snapshots. Checkpoints не переписывались: их ссылки отражают состояние на момент записи; этот архив появился позже.

## Нормализация и storage policy

В 424 архивных копиях удалены machine-specific workspace/Cargo-home prefixes. Это исправление архивирования, не изменение измеренных чисел; оригинальные файлы в ignored trees не менялись. Manifests содержат original SHA-256, original logical SHA-256, archive SHA-256 и archive logical SHA-256; normalized copies не выдаются за byte-identical originals. Старые внутренние raw-hash manifests остаются историческими и могут ссылаться на original hash: для проверки нынешней копии использовать внешний archive manifest.

Локальный `.gitattributes` отключает EOL-конверсию Git для архива: SHA-256 относятся к точным сохранённым байтам. Это не меняет настройки исходного кода проекта.

Raw/JSON artifacts размером от 200 KiB дополнительно gzip-сжаты; `extra_gzip` отмечен в manifest. Уже сжатые snapshots сохраняются либо перепакованы только после нормализации private prefixes. Ни один raw artifact не превышает 2 MiB uncompressed; длинные Markdown indexes — документы, не raw logs. Архив не содержит построенных executables или target/ caches.

## Проверка без сборок и без benchmark execution

```sh
node docs/research/verify.mjs
```

Verifier проверяет каждый archived hash, decompression, JSON/NDJSON, размеры, приватные home prefixes и inventory totals. Это проверка сохранности архива, **не повтор correctness/performance acceptance**. Предыдущие checks генераторов выполнялись в исходных trees до архивирования; повторные build/test/bench сейчас не выполнялись.

## Восстановление контекста отчёта

Base всех snapshots: f80c681b799ef4ef7fa08d79c06896fdf1104fe9. Это pinned принятая база исследования, не заявление о текущем remote main.

Архивная директория `docs/research/<slug>/` соответствует исходной `worktrees/<slug>/docs/`. Для отдельного воспроизведения сначала создать НОВЫЙ изолированный worktree от base в разрешённом workspace, затем по manifest восстановить каждый `source` относительно корня нового worktree и распаковать `extra_gzip` entries до исходного имени. Поле `source` уже содержит префикс `docs/`; добавлять его повторно не нужно. Для supplements использовать `restore_to`. Не копировать архив поверх текущих dirty trees.

Исторические scripts и source identities сохранены с исходным namespace worktrees/<slug> и .rush/. Они не являются универсальными portable commands из текущего положения архива: сначала восстановить layout, описанный конкретным report/reproduction manifest. Для восстановления измеренного product source использовать записанный source patch и archived inputs; такой шаг не входит в docs-only commit и не означает разрешения выполнять benchmark или тесты. Абсолютные машинные пути из старого окружения намеренно не восстанавливаются.

Runtime изменений и принятых speedup claims в этой документной поставке: **0**. Итоги NO-GO/INCONCLUSIVE/FAIL и пробелы tests/CI сохранены.
