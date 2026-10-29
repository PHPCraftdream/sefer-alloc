# Validated metadata context + scalar flush — prereg до B

Baseline — accepted f80c681b799ef4ef7fa08d79c06896fdf1104fe9, не предыдущий prototype. Композиция включает отдельный one-element flush из раунда22 и новую схему передачи контекста overflow; полный состав сравнивается с main, выигрыши не складываются.

## Механизм

Guard и normal push оставить прежними. Только после successful small_free_guard и cnt>=cap получить адрес CURRENT state-byte из StateByte view, построенного из stored root и checked off. Передать private unsafe overflow helper текущий block, class, count snapshot и адрес metadata-byte — без root/raw offset. Helper сохраняет старый cap-eviction/half-flush/compaction/counter/mask порядок, но не разрешает count/root/offset повторно. Cap повторно читается из той же immutable FREE_PARK_CAP table, чтобы точно сохранить старый split даже для defensive states. Cap-eviction использует новый scalar flush, full magazine — прежний batch flush.

Гипотеза: уменьшить register live ranges обычного free и размер контекста, удерживаемого через вложенный flush; scalar операция должна компенсировать call/context tax частого eviction. Вычисление metadata address находится только в overflow leg, не добавляется к каждому hit.

## Обязательная liveness/provenance proof

- Адрес выводится из StateByte.bytes (stored-root provenance), не из caller payload pointer; формула та же, что в существующем StateByte::write.
- off уже прошёл physical-kind/payload/alignment/exact LIVE-class/bump guards; metadata byte принадлежит mapped Small/Primordial root.
- CURRENT block имеет outstanding live credit и ещё не передан в substrate. LIVE-vs-MAG oracle и magazine invariant исключают его равенство resident evicted entries при корректном вызове.
- Flush возвращает только прежние resident blocks. Credit CURRENT блока остаётся, поэтому его root и state-byte mapping не могут быть released этим flush. Metadata slot disjoint от caller payload и HeapCore/tcache layout.
- Pointer нигде не сохраняется после синхронного helper call и не публикуется другим threads. Store MAG остаётся после resident flush в прежней точке; current payload не трогается. Guard/counter order и live/state/virgin invariants не ослабляются.
- Helper unsafe/private с полным контрактом; никакого safe public raw-pointer metadata API или benchmark hook. StateByte getter crate-private, возвращает только тот адрес, который прежние read/write уже вычисляли; его callers подчиняются той же owner/offset domain.

При невозможности доказать любой пункт — не писать затронутый unsafe path. Actual-source Miri/CI позже обязательны для acceptance, не заменяются этим reasoning.

## Gates до B

Same production-only probes/toolchain и pre-build source identities. A free reference reused immutable из раунда22 (same accepted f80/harness/compiler), B two-pass N50000/100000 rt/const16/64. Цель rtfree минимум−4Ir кmain; constнехуже; bulk64нехуже; bulk16k минимум−30Ir кmain. Mi/repeat controls≤0.01Ir. Нерегресс-бюджеты старой полной матрицы не ослабляются. Scope/source/codegen win не считать ns speedup.

Fresh overflow controls100/200rounds alloc64→free64, два проходаsource/allocator arm. Actual after callgraph: overflow helper6/60, generic flush6/0, scalar flush0/60 perround bulk64/bulk16k. Native survivor:5modes20balancedblocks A/miA/B/miB/miB/B/miA/A +AA10; workload/process отдельно, per-row nominal paired CI. Все цифры и Mi/Sefer до/после сохраняются.

Отдельные tests: exact current/resident state+identity+credits+reuse full16 и smallercap. Test execution/Miri/Loom/doc/agents/commit/push/merge локально не разрешены; cargo check/clippy/fmt и cargo-free scripts разрешены, comparative benchmark builds/runs разрешены user. Никаких feature/config/версий, producer atomics, class geometry или persistent metadata changes.
