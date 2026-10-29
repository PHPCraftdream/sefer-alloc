# Half-flush split: воспроизведение

Архив staged runners, не standalone CLI. Все новые builds/profiles/benchmarks требуют разрешения по CLAUDE.md. Tests, commit, push и merge эти runners не выполняют. Среда: Windows Node/Git Bash/native Rust плюс WSL с уже установленными Cargo/Valgrind/callgrind_annotate/objdump/nm.

## Layout и snapshots

Родительский каталог содержит `worktrees/own-free-before`, `worktrees/own-free-split`, `.rush/own-free-split`. Восстанавливать snapshots только в новых пустых scratch/evidence directories, не перезаписывать исторические результаты. Каждый `.patch.gz` применяется к чистому f80c681b799ef4ef7fa08d79c06896fdf1104fe9, не поверх другого patch. Новые examples добавляются отдельно из сохранённых `own_free_profile.rs`/`own_free_wall.rs`; их SHA-256 записаны в identity-файлах.

- baseline.patch.gz + profile example: baseline stage, production src точно base.
- candidate.patch.gz + profile example: half-flush-only source stage.
- comparison-before/after.patch.gz + wall example: control/native snapshots.
- Regression test/docs не входят в измеренный binary snapshot и не выдаются за executed tests.

`profile.mjs baseline` выбирает own-free-before; `profile.mjs candidate` — own-free-split. Evidence обоих этапов пишется в own-free-split/docs/perf/own_free_split. Все runners запускать из родительского каталога после копирования в новый `.rush/own-free-split`.

## Последовательность

1. `node .rush/own-free-split/profile.mjs baseline`, затем candidate. Pure production, одинаковый probe; free-only Callgrind N50000/100000 × rt/16/64 ×2passes. Отдельные target directories и pre-build identities.
2. `summarize.mjs <profile-dir> <evidence-dir> baseline|candidate` проверяет self-attribution sums/repeatability и вычисляет marginal Ir.
3. `node .rush/own-free-split/compare.mjs`: overflow controls и native Windows. Bulk64/bulk16k — alloc64→free64; Callgrind100/200rounds×2passes; native20balancedblocks/mode +AA10. Source hashes проверяются после всех processes.
4. `node .rush/own-free-split/archive.mjs`: архивировать raw profiles, сжать большие logs, нормализовать только source/build paths. Actual callgraph: flush6/60 per round; half-flush helper6/0 per round. Source preservation отдельно: `node .rush/own-free-split/source-check.mjs` — guard/normal prefix, cap branch и moved half body сравниваются с base.
5. Необычный bulk16k Ir результат потребовал дополнительного sanity, не изменения gate: `node .rush/own-free-split/sanity.mjs` выполняет четыре unfiltered whole-program profiles на тех же control binaries. `nm -S -C` и objdump excerpts связывают фактические caller call sites с изменившимся flush callee. `archive-sanity.mjs` проверяет binary hashes против исходных control recordings и сохраняет sanity/codegen summaries и raw. Сохранённые .caller-asm/.flush-asm excerpts получены через objdump по address/size из nm; перед повторной архивацией их нужно получить для вновь построенных binaries, а не использовать старые адреса. Полная asm-команда: `objdump -d -C --no-show-raw-insn --start-address=0x<addr> --stop-address=0x<addr+size> <binary>`.
6. Из candidate tree: `node scripts/own_free_split_report.mjs`, затем `--check`. Генератор сверяет raw totals, snapshots/hashes, balanced ordering, ratios, unfiltered delta и codegen sizes; CSV/Markdown производятся одним вычислением.

## Граница вывода

Результат текущего кандидата — NO-GO по runtime free; bulk64 control также дороже. Native интервалы этого прогона включают ноль. Нельзя использовать выигрыш bulk16k для компенсации проигравших gates. Single-block codegen specialization — отдельное наблюдение с full-program sanity, не самостоятельный source-level кандидат и не доказанный native speedup.

WSL Ir/free включает wrapper/callees; native ns/pair включает setup/churn/teardown; process_ns дополнительно включает startup/output/exit. Эти оси не смешивать. Nominal per-row CI — не family-wise significance. Широкая promotion-матрица, Miri/Loom/test execution и exact-head CI не получены и не заявляются.
