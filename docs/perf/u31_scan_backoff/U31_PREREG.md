# U31 — pre-registration (fixed before any measurement of the variant)

Base: `main` @ `29b82b7d` (depot, B1, B2 shipped). Item: `docs/perf/OPEN_ITEMS.md`
item 90 (trigger FIRED, `docs/perf/B4_U31_TRIGGER.md`).

## Base numbers (ИЗМЕРЕНО before this file, WSL callgrind, probe `docs/perf/b4_u31_probes/ut_u31_probe.rs`)

| n | Ir sefer | Ir mimalloc | sefer/mimalloc | find_segment calls | scan words |
|---|---|---|---|---|---|
| 32768 | 11 960 373 | 3 433 976 | 3.48× | 2 048 | 524 800 |
| 65536 | 32 814 135 | 6 361 994 | 5.16× | 4 096 | 2 098 176 |
| 131072 | 102 833 374 | 12 218 729 | 8.42× | 8 192 | 8 390 656 |

`drain_segment_sidecar` inclusive at n = 131072: 76 244 979 Ir (74.1 %); every
one of the 8 192 discovery scans found no nonzero pending word.

## Mechanism

A magazine refill tries the current segment's free list, the class cursor,
then `find_segment_with_free`, then the bump carve. With remote frees
compiled in (`alloc-global` + `alloc-xthread`) the routed table never trusts
a negative directory answer, so every refill miss runs the linear scan, and
the scan drains each small segment's pending sidecar from the payload start
to the bump high water. In a growing heap every refill therefore scans the
whole carved range: the cost is quadratic in the carved range.

Producer-marked summaries were rejected three times (item 82: C′, C″, C‴)
on the producer price (+8 … +16 Ir per remote free). This variant changes
the owner side only.

## Variant `u31_scan_backoff`

Owner-only state: one `u32` bump threshold in the segment header
(`SegmentHeader::sidecar_defer_until`, former tail padding — the header
stays 144 bytes; `HeapCore` has no headroom under its 9216-byte pin, so
the state does not live in `AllocCore`).

- After a linear scan that drained at least one segment's sidecar, reclaimed
  nothing in any segment (every drain `Skipped`) and found no free block,
  the owner sets `small_cur`'s threshold = its bump + (sum of the drained
  segments' bump high water) / 8.
- While `small_cur`'s bump is below its threshold and it still has room for
  one block of the requested class, the linear scan skips the sidecar
  drains. A new `small_cur` has its own threshold (0 for a fresh segment);
  the retain-decommit reset clears it with the bump. Bin heads are still read,
  so locally freed blocks in other segments are still reused.
- The forced rescue scan (OOM path) never defers.
- No producer, publication, atomic or `unsafe` change.

Bound (by construction): a publication pending while the window is open is
drained once the owner has carved at most 1/8 of the drained range more, or
when the current segment fills, or when `small_cur` changes. Each deferred
refill carves at least one block, so the window always ends.

## Thresholds

1. **Win, instructions:** growth16 n = 131072 Ir ≤ 0.70 × 102 833 374
   = 71 983 361.
2. **Win, time:** growth16 n = 131072, native wall clock, variant vs base,
   20 ABBA process pairs: median ratio < 1 and bootstrap 95 % CI upper < 1.
3. **No harm, judge kernels** (J1′ v2 ablation, 20 processes × 2000 blocks,
   effect = variant median − null median, process bootstrap): pair16, pair64,
   larson effect ≤ +1 % and CI upper ≤ +1.5 %; every other kernel: not
   (CI lower > 0 AND effect > +2 %).
4. **No harm, MT** (`examples/ph6b_mt_ab.rs`, quads ABBA/BAAB plus null
   quads, `scripts/mt_regression_quads_run.mjs`): no larson/mstress cell at
   T = 1, 2, 4, 8 slower than the null spread (effect CI lower > 0 and effect
   larger than the largest null |effect|).
5. **Memory:** `segments_reserved_total` (ph6b_mt_ab) and peak RSS
   (WSL `/usr/bin/time`, growth16 and the U31 MT probe) ≤ base × 1.05.

Activation oracle (growth16 n = 131072, `bench-internals` counters): deferred
linear scans > 0; discovery scans ≤ 128 (base 8 192); scan words ≤ 65 536
(base 8 390 656).

GO = 1 ∧ 2 ∧ 3 ∧ 4 ∧ 5 ∧ oracle.
