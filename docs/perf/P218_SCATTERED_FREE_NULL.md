# P218 — scattered small-block free: two candidate levers measured, NULL (`bench`, no code change)

Task #218 (2026-11-26). Measurement-only record: neither candidate is merged, and `production` does not change.
The tables are printed by `node docs/perf/p218/report.mjs` from `docs/perf/p218/ab_raw.jsonl`,
`docs/perf/p218/oracle_raw.jsonl` and `docs/perf/p218/identity.json`; the per-pair summary is
`docs/perf/P218_SCATTERED_FREE_NULL_summary.csv`.

## 1. Question

P217 (`docs/perf/P217_ADAPTIVE_PROMOTION.md`, section 3) put most of the `growmix` and `regrow` loss against
mimalloc in the final shuffled free of small blocks: each block of a 16-block magazine flush mostly sits in a
different segment, so both the free and the flush pay a segment lookup (`docs/perf/OPEN_ITEMS.md` item 146).
The question was whether removing those lookups closes part of the gap.

## 2. Candidates (exact sources: `docs/perf/p218/V1.patch`, `docs/perf/p218/V3.patch` over `d5e89119`)

- **V1:** a memo miss on the free path is resolved by one `owned_block_of` table lookup that accepts only
  Small/Primordial blocks. This replaces the `own_cache` second chance, so the memo is no longer re-armed there.
- **V3:** V1 plus a production magazine flush that takes the segment base from the address mask instead of a
  table lookup. Builds with `misuse-checks` or debug assertions keep the lookup, and the L-4 flush guard stays.
  This variant adds `tests/p218_mask_flush.rs`.
- A third idea, committing the bin directly in the freed block, was dropped before measurement. Writing into the
  block inside the caller's live `Box::drop` frame is the UB recorded as R1-01. A deferred variant would need at
  least 16 B of state, and the `HeapCore` size pin (≤ 9216 B) has no room left for it.

## 3. Method

The judge is `examples/g1_wild_judge.rs` on the real allocator statics. Scenarios are `growmix`, `regrow` and
`wild` at seed 1, scale 1. Each block runs 3 repeats of each arm in its own process, and the runner
(`docs/perf/p218/run_ab.mjs`) shuffles the scenario order and rotates the arm order per block. Ratios are the
geometric mean, over 12 blocks, of each block's minimum `ns_per_op`, with a 95 % t interval (df 11). The arms are
the `production` builds of base, V1 and V3, plus the mimalloc arm of the base binary. The host is Windows with a
10 % hard CPU cap on the whole runner, so only the ratios inside this run carry meaning. Binary hashes,
toolchain and host details are in `identity.json`.

## 4. Result

| scenario | v1/base | v3/base | base/mi | v1/mi | v3/mi | median ns/op base / v1 / v3 / mi | median end RSS MiB base / v1 / v3 / mi |
|---|---:|---:|---:|---:|---:|---:|---:|
| growmix | 1.034 (0.922–1.160) | 1.001 (0.907–1.106) | 1.271 (1.158–1.395) | 1.315 (1.181–1.463) | 1.273 (1.156–1.401) | 1798.2 / 1981.5 / 1793.5 / 1433.4 | 1254 / 1254 / 1254 / 1137 |
| regrow | 0.946 (0.820–1.092) | 0.966 (0.872–1.071) | 1.334 (1.169–1.522) | 1.262 (1.143–1.395) | 1.289 (1.213–1.370) | 1368.1 / 1341.3 / 1350.0 / 1079.6 | 68 / 68 / 68 / 572 |
| wild | 1.072 (0.942–1.219) | 1.156 (0.984–1.359) | 1.199 (1.042–1.379) | 1.284 (1.152–1.431) | 1.386 (1.216–1.579) | 207.9 / 211.3 / 212.4 / 146.0 | 34 / 34 / 34 / 49 |

Every v1/base and v3/base interval contains 1, while every arm/mi interval lies wholly above 1. **Verdict: NULL.**
Neither lever moves wall time by more than the run can resolve (interval half-widths are about 10–15 %), and the
gap to mimalloc stays the same. The `wild` point estimates lean slower (V3 1.156), but those intervals contain 1
as well.

## 5. Path-activation oracle (one `production internals bench-internals` run per variant and scenario)

| scenario | base lane_second_chance | v1 lane_second_chance / p218_lane | v3 lane_second_chance / p218_lane / p218_flush | checksum equal |
|---|---:|---:|---:|---|
| growmix | 73,111 | 0 / 1,455,605 | 0 / 1,455,604 / 1,450,899 | yes |
| regrow | 278,920 | 0 / 279,388 | 0 / 279,388 / 151,472 | yes |
| wild | 2,570,318 | 0 / 3,805,890 | 0 / 3,801,717 / 7,609 | yes |

Both candidates took their intended path:

- The old second chance never fires in V1 and V3.
- The new table lookup and the mask flush run millions of times in `growmix`.
- The workload checksum is identical across all three variants.

In `growmix`, V3 skips the flush lookup for 1.45 M blocks with no measurable time change, so the segment lookup
itself is not where the teardown time goes. The remaining candidates are memory-side costs of touching the
metadata of about 311 segments in shuffled order: cache and TLB misses on segment headers and bins. Telling them
apart needs a sampling profile, which runs only with the owner's consent. In `wild`, V1 makes 3.8 M table lookups
where base made 2.57 M second chances, because the memo is no longer re-armed.

## 6. Disposition

- V1 and V3 are not merged. Their patches stay under `docs/perf/p218/` as the exact measured sources.
- The test-oracle findings raised in review apply only to a winning variant, so they are moot.
- Next step on `docs/perf/OPEN_ITEMS.md` item 146: a sampling profile of the `growmix`/`regrow` teardown, Sefer
  against mimalloc, after the owner approves the run.
