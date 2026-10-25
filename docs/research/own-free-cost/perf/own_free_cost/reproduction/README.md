# Own-free experiment: воспроизведение

Это архив staged runners, не standalone benchmark CLI. Нужны Git Bash/Node на Windows, WSL с уже установленными Cargo/Valgrind/callgrind_annotate, native Windows Rust toolchain. Новые build/profile/benchmark запуски требуют разрешения по CLAUDE.md; этот текст их не запускает и не заменяет разрешение. Tests/commit/push/merge runners не выполняют.

## Layout

Родительский рабочий каталог содержит `worktrees/own-free-cost`, `worktrees/own-free-before` и scratch `.rush/own-free`. Сохранённые runners скопировать в новый пустой `.rush/own-free`. Не запускать поверх исторического scratch/evidence: profile/control scripts создают одноимённые outputs; native runner дополнительно отказывается перезаписывать native.ndjson.

Все source snapshots основаны на `f80c681b799ef4ef7fa08d79c06896fdf1104fe9`. Каждый `.patch.gz` распаковывается и применяется к ЧИСТОЙ базе, а не поверх другого patch. Новые examples не входят в git diff patches и добавляются отдельно из сохранённых `.rs`; их hashes проверяются identity-файлами. Regression test и документация не входят в измерительный binary snapshot и не выдаются за executed test evidence.

## Этапы

1. Baseline profile: own-free-cost в состоянии `baseline.patch.gz` + `examples/own_free_profile.rs`. `node .rush/own-free/profile.mjs baseline` из родительского каталога. Production source совпадает с base. Binary с обоими allocators, WSL production-only, Callgrind на free wrappers, N=50000/100000 × rt/16/64 × два прохода.
2. Candidate profile: отдельное восстановление own-free-cost из `candidate.patch.gz` + того же own_free_profile.rs; `node .rush/own-free/profile.mjs candidate`. Baseline outputs сохранить отдельно. Измеренная разница — только filled-magazine outlining; source identity и binary hashes записываются до измерений.
3. Control/native: own-free-before из `comparison-before.patch.gz`, own-free-cost из `comparison-after.patch.gz`, в обоих идентичный `examples/own_free_wall.rs`. `node .rush/own-free/compare.mjs`. Сначала два WSL overflow-heavy profiles, затем обе native сборки, затем последовательные balanced native processes без конкурирующих компиляций.
4. `summarize.mjs <scratch-profile-dir> <evidence-dir> baseline|candidate` вычисляет marginal Ir и self-attribution. `archive.mjs` нормализует только source/build paths и сохраняет raw profiles; callgraph проверяет точные helper/flush counts и пишет activation.json. Числовые строки не меняются.
5. В candidate tree: `node scripts/own_free_report.mjs`, затем `node scripts/own_free_report.mjs --check`. Один генератор сверяет raw profile totals и balanced native samples, identities и ratio arithmetic и выдаёт отчёт/CSV.

## Оси и ограничения

- `baseline_identity.json`/`candidate_identity.json` относятся к первому WSL free profile; `comparison_identity.json` — к следующему control/native harness, не взаимозаменяемые source identities.
- WSL Ir/free включает wrapper и callees только внутри выбранного free; native ns/pair включает весь workload с setup/churn/teardown. Их ratios не являются одной метрикой.
- Native full process дополнительно включает startup/output/exit. A/A выполнен отдельно; его нулевой CI не доказывает отсутствие любого шума.
- Path-activation evidence — raw callgraph (не предположение по размеру): bulk64 имеет шесть flush calls/round, bulk16k — 60. Candidate helper вызывается ровно столько же; full/source controls проверены.
- Результат NO-GO по overflow control. Не запускать дорогую promotion-матрицу лишь ради пересмотра уже заданного control budget. Прототип не принят в main; существование файлов не означает correctness/CI acceptance.
