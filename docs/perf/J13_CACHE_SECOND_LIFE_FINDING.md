# J13 side item — CACHE second-life +2.3 %: read-only finding

Date: 2026-11-12. Read-only; no code change. Source: `docs/perf/j13_limited_20261111/` (cell `cache-16MiB`), `docs/perf/J13_DEFAULT_ALWAYS_IAI_summary.csv`.

**No instruction-level difference exists on the second-life path.** The arms differ only in the runtime tuple mode, which is read only on the fresh-mapping path (`advise_fresh_large`). The cache hit (`alloc_core_large.rs` hit branch) and the deposit (`mem_impl.rs` Large free) read no THP tuple or ledger; with budget 0 the header charge is 0 and the Off-path observation token is 0 in both arms. The harness also asserts that a hit emits no fresh/advice events. IAI confirms it: `large_cache_hit_only_4mib` minus `large_cache_prefill_only_4mib` and the payload pair have identical deltas (+1 Ir each, from the fresh prefill), so the hit itself adds 0 Ir.

**Where the gap sits.** Splitting `cache_ns` of the raw `rep[1]` by phase (12 paired blocks):

| Phase | Off median | Always median | Always/Off (95 % CI) |
|---|---|---|---|
| alloc (hit) | 411 ns | 360 ns | 0.877 (0.778–0.990) |
| touch (4096 page stores) | 36115 ns | 37407 ns | 1.032 (1.021–1.043) |
| free (deposit) | 411 ns | 161 ns | 0.459 (0.357–0.590) |

The whole excess is in the payload touch sweep (+1.3 µs, ~0.3 ns per store). Second-life page faults are 0 in both arms, so faulting, zeroing and khugepaged are excluded; the huge pages stay intact (18 MiB AnonHugePages under Always).

**Cause: not named.** The cell's own A/A control is 1.0125 (0.9929–1.0324), and the Off touch time drifts 3.1 % between the AB and AA sessions — the same size as the effect. A memory-system effect of contiguous huge-page frames in a 4 KiB-stride store sweep is possible but unproven. Per the owner rule, no fix without a named cause; none is proposed. Confirming would need a CI run with more blocks and a same-size A/A, or `perf stat` around the touch sweep.
