# Воспроизведение D1′ evidence

## Без повторного benchmark

Из корня репозитория:

```sh
node scripts/d1_prime_chunked_sidecar_report.mjs
```

Генератор проверяет выбранные canonical Criterion sample/estimate JSON, registration timing, MT, installed-global, foreign-free, first-allocation и retained-pin summaries против raw samples, а IAI totals/marginal — против raw stdout logs; затем пишет отчёт и companion CSV. `--check` сверяет уже сохранённые отчёт/CSV без перезаписи. Остальные canonical строки сохранены как историческая полная сводка; генератор не утверждает raw-проверку всех её строк.

## Исходные runner scripts

`.mjs` в этой папке — архив рецептов scratch-layout, не standalone CLI для запуска непосредственно отсюда. Они рассчитаны на размещение в `<candidate>/.rush/perf-d1/`; baseline находится в `before-repo/`, выходы — в `raw/`, target directories раздельные. Для нового прогона требуется отдельный пустой output directory и новый source passport до измерений. Не переиспользовать исторический source identity для изменённого дерева. В Linux runner абсолютный путь передаётся через `D1_WSL_REPO`, без частного пути хоста в репозитории.

Прямые Cargo-команды из архивных runner scripts запускают сборки и benchmark binaries: их наличие не разрешает локальный запуск без запроса пользователя. В этой сессии локальные сравнительные benchmark runs были отдельно разрешены. `cargo test` локально не запускался.

Репродукция исторического MT: взять соответствующий baseline/candidate commit и применить распакованный `mt_harness.patch.gz`; SHA-256 исходного patch был зафиксирован runner до измерения. Более поздние registration tree SHA не являются MT snapshot. Точные изменения остальных measurement trees сохранены в `baseline_*`/`candidate_*.patch.gz`; gzip сохраняет patch bytes, включая пробелы контекста. Применение к базовому commit и `git write-tree` восстанавливает указанный tree identity.

## Сохранённые данные

- Четыре clean canonical run: оригинальные числовые stdout-строки и выбранные `new/{benchmark,sample,estimates}.json` для пяти workload’ов и трёх allocator arms.
- IAI: штатные stdout logs; raw totals и marginal таблицы раздельные.
- Installed-global, foreign-free, MT, registration и retained-pin: per-process NDJSON, при необходимости gzip.
- First-allocation: исходные process samples в `summaries/first_alloc_*_samples.json`; расположение историческое, это raw, не агрегат.
- Source identities относятся к отдельным измерительным деревьям; позднее добавленные поля Linux kernel/toolchain — post-run environment observation, не часть pre-run snapshot.

Новые копии stdout logs исключают строки с частными путями машины. Каждая исключённая строка заменена явным `# TRUNCATED` marker с командой воспроизведения. Числовые benchmark-строки не редактировались; исходники logs в ignored scratch сохранены. Это не ретроактивное изменение ранее committed logs.

## Ограничения

WSL retained-pin oracle не заменяет native Linux и не закрывает Linux first-allocation/multisegment PSS-матрицу. Отрицательное решение по данному кандидату не является утверждением полной приёмки ADR. Никакие новые измерения, mutation runs или тесты не подразумеваются генерацией Markdown.
