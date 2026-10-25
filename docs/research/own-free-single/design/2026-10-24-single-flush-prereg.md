# Раунд single-element flush — prereg до B

Base: f80c681b799ef4ef7fa08d79c06896fdf1104fe9. Ни один отвергнутый frame/split/D1′/scalar prototype не входит в baseline. Round context: оба OPEN_ITEMS indexes и тематические карточки прочитаны в текущей сессии; старые statuses/triggers сохраняются. Новые наблюдения perf115/116 учтены; этот раунд проверяет только изоляцию one-element flush.

## Механизм и эквивалентность

Добавить crate-private unsafe AllocCore::flush_one_magazine_internal(class,ptr), не cold и inline-never: один production caller, cap-eviction. Он выполняет ровно null-check → key → table.canonical_base_of → flush_run(class,root,one-element slice). Тело flush_run с его span/class/state/credit/pool guards не меняется и не копируется. В HeapCore заменить только вызов generic flush_class_internal на этот one-element вход для старого evicted блока; весь остальной free/half-flush/guard/counter порядок прежний.

Для slice длины1 generic outer loop имеет не более одного run: null → no-op; missing canonical root → no-op; иначе тот же flush_run на том же единственном элементе. recycled_bases изначально пуст, а результат recycled_now больше никогда не читается следующей итерацией, потому что следующего run нет. Поэтому run-discovery/duplicate-run bookkeeping можно не создавать, не ослабляя ни одного guard внутри flush_run. Current Box-frame block не передаётся в substrate, как и прежде.

Новый вход unsafe, #Safety требует mapped/owned/current allocation, valid class и unique free; null разрешён. Root берётся только из owner table, не из caller key. Никаких публичных test hooks, feature/config/version/refill/cap изменений. Inline-never фиксирует отдельную scalar operation без расширения main free body; codegen всё равно проверяется, а не обещается.

## Данные и gates

- Reference A: сохранённый pure-production baseline profile из предыдущего раунда own-free-split (source/patch/example hashes и exact toolchain). Повторяется только при несовпадении source/toolchain или control counts. Это reuse неизменяемой baseline reference, не новый A-run и не post-hoc source identity. Native/overflow A/B выполняются свежими в этом раунде.
- B free profile: rt/const16/64, N50000/100000, два прохода, same source probe/features production/toolchain. rt/const не хуже A; Mi/repeat controls ≤0.01Ir/free.
- Цель именно нового one-element пути: bulk16k снижение минимум30Ir/free от принятой базы. Bulk64 не хуже базы. Это другой target, не переименование прежнего проваленного rt−4 gate.
- Actual callgraph after: one-element entry60/round bulk16k,0bulk64; generic flush0/round bulk16k,6bulk64. Before: generic60/6. Проверить по raw, не по label.
- Выжившему кандидату: fresh native same probes, five modes,20balancedblocks A/miA/B/miB/miB/B/miA/A +AA10blocks; workload/process отдельно. Все Mi/Sefer до/после и B/A, nominal paired CI публикуются. Нулевой CI — INCONCLUSIVE, не speedup.
- Далее только survivor — canonical/IAI/Larson/mstress/MT и нужная memory/provenance приёмка. Старые nonreg budgets не ослаблять; не складывать исторические выигрыши. Source snapshots до каждого build. Raw→один checked generator→CSV/report.

Separate tests: normal/full16/cap4 residency/state/credit/reuse; existing scalar flush guards остаются общими через flush_run. Cargo check/Clippy/fmt допустимы, но test execution/Miri/Loom/CI не заявляются без receipts. Пользователь разрешил сравнительные research/probe/benchmark builds/runs, не local tests/agents/commit/push/merge. Все files/commands внутри стартового worktree; background только через run_in_background:true, без polling.
