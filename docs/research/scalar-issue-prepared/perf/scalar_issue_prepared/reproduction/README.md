# Воспроизведение scalar issue comparison

`measure.mjs` — сохранённый runner текущего эксперимента, не standalone CLI для произвольного checkout. Запускать из родительского рабочего каталога, содержащего:

- `worktrees/scalar-issue-baseline/` — отдельное дерево на указанном base с baseline.patch;
- `worktrees/scalar-issue-prepared/` — отдельное дерево на том же base с prepared.patch;
- идентичный сохранённый `scalar_issue_gate.rs` в `examples/` обоих деревьев;
- `.rush/scalar-issue/` — scratch для target directories и build logs.

Runner не переключает ветки, не делает commit/push, не запускает тесты. Он собирает и исполняет только benchmark probe и штатный `bench:table`. Новый запуск требует разрешения на сравнительные замеры по правилам проекта. Binary suffix `.exe` означает Windows; Linux-порт этого runner не измерялся.

Команда из родительского каталога: `node worktrees/scalar-issue-prepared/docs/perf/scalar_issue_prepared/reproduction/measure.mjs`.

Перед новым запуском выбрать новое evidence directory в отдельной копии runner: текущий отказывается перезаписывать scalar.ndjson. Не удалять исторический raw файл ради повторного запуска.

`source_identity.json` создаётся до сборки; source identity — base + SHA-256 точного patch + SHA-256 нового example. Оба patch восстанавливают Cargo declaration; только prepared.patch дополнительно меняет два production-файла. Regression tests и docs не входят в измерительную source identity и не выдаются за часть benchmark binary.

Чистые target directories разделены по arm. Каждая canonical sweep копирует Criterion new/{sample,estimates,benchmark}.json до следующей sweep того же arm. В saved logs строки build output с частными Windows-путями заменены явным TRUNCATED marker; raw samples неизменны.

После измерения: `node scripts/scalar_issue_report.mjs`, затем та же команда с `--check`, из candidate tree. Генератор сверяет hashes, число samples, ABBA order, scalar oracle и Criterion point estimates с sample data; CSV и Markdown получаются из одного вычисления.
