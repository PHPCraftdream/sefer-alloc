# Широкая проверка выбранной B-композиции — до новых broad данных

Выбор заранее: own-free-context (scalar flush + validated slot), baseline accepted f80c681b. Inline-always variant не доминирует B: небольшой cap gain в обмен на худшие rt/bulk64; B выбран как balanced/меньший scope. A/C и их данные не скрываются.

Последовательность: существующий perf_gate_iai на production bench-internals internals, оба arms built+hashed до measure; source identities доbuild, только существующий runner безinstall/update. Затем native-only resume на frozen comparison identity (безповторныхcontrols) и canonical bench:table четыре sweep A/B/B/A с отдельными targets и сохранёнными Criterion samples.

IAI: сохранить все строки, sets arms обязаны совпасть, Mi controls отдельно; любые positive deltas показать, не прятать geometric mean. Raw totals не считать per-op speedup: N/4N пары и bootstrap/fixed cost рассматриваются отдельно. Существующие micro budgets не меняются; эта широкая таблица — диагностическая и не подменяет точные ao/aoc/batch/Larson budgets или safety CI. Большая/необъяснённая regression блокирует promotion, даже если target выиграл.

Native: тот же заранее заданный protocol20balancedblocks×5modes+AA10, workload/process отдельно, все nominal CIs; при пересечении0 INCONCLUSIVE. Canonical: штатный bench:table формат/units/features, все17 сравнительных строк и immutable raw samples; два sweep наarm описательны, не достаточный paired process GO.

Никаких новых SRC/Cargo changes между сборками/measure. CLI runner --version без library protocol ранее вернул generic0.14.2 version-context error; это не повод обновлять зависимости/устанавливатьtool. Actual cargo bench проверит настоящий library-runner protocol. Все target/tmp directories внутри стартового worktree; никаких root/tmp-style ручных staging directories.

Tests/Miri/Loom/doc/agents/commit/push/merge не разрешены. Bench builds/runs разрешены userresearchrequest. Полная acceptance остаётся ограничена отсутствием execution/CI receipts, MT/retention/прочими непроверенными осями; не заявлять shipping/merge readiness по цифрам.
