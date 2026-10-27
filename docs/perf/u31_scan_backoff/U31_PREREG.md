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

## Amendment 1 (before any time measurement)

A preliminary callgrind of draft v1 (`6259076c` before amend; window =
drained bump high water incl. metadata / 8) gave growth16 n = 131072
26 972 679 Ir (ИЗМЕРЕНО, `.rush` scratch, not evidence). Two design changes
were made after it and after the local test suite, before any wall-clock run;
thresholds 1–5 and the oracle are unchanged:

1. The window is sized by the drained **payload** range
   (`bump − payload start` per segment), not the bump high water, so the
   metadata region no longer inflates it.
2. The window is a pair `(start, end)` packed in 128-byte units in the same
   `u32`: drains are deferred only while `start < bump < end`. A repeated
   discovery with no carve since the empty scan always drains (the
   publish-then-discover protocol tests `r10_active_kind_late_publish`,
   `b4_u31_trigger_growth16_oracle` drive no carve in between).
   `tests/r12_o4_routed_miss_counters.rs` carves between the empty scan and
   the publication by construction; it now closes the window through a
   `bench-internals` hook before its legs, so its classification checks run
   on real drain scans unchanged.

Every number used for the verdict is measured on the final variant commit.

## Amendment 2 (after the 11-kernel ablation, before any replication)

The ablation (20 processes × 2000 blocks, `u31-abl@f77b9a0f`) left threshold 3
unresolved on one kernel only: larson effect +1.14 % [−1.25, +3.79], marked
imprecise by the null (null A/B +1.03 % [−0.52, +2.91]); the rule needs
effect ≤ +1 % and CI upper ≤ +1.5 %. Every other kernel meets its rule.

Activation per process (judge built with `bench-internals`, `ab_variant`,
2000 blocks, ИЗМЕРЕНО): larson 98 discoveries in both arms, 75 of them deferred
by the variant; the deferred scans found nothing in the base either (single
thread, no remote frees), so both arms issue the same blocks — the variant
only skips work.

Replication rule, fixed now: one independent larson-only run, 40 processes ×
2000 blocks, fresh order seed; threshold 3 for larson is judged on that run
alone, and both runs are reported. No other kernel is re-run.

## Amendment 3 — v1 verdict and variant v2 (before any v2 measurement)

**v1 (`37bf5a10`) is NO-GO by threshold 5.** Threshold 1 (growth16 Ir
27 102 575 ≤ 71 983 361), 2 (growth16 time ratio 0.339 [0.331, 0.346]),
3 (with the amendment-2 larson replication −0.25 % [−0.59, +0.55]) and 4 (no MT
cell slower) and the oracle passed, and so did `segments_reserved_total`
(median 10 = 10) and growth16 peak RSS (1.000). MT peak RSS failed:
`ph6b_mt_ab` under WSL `/usr/bin/time`, 100 ABBA quads, paired median ratio
1.094 [1.032, 1.158] > 1.05 (an earlier 20-quad run gave 0.997 [0.903,
1.262], imprecise; both are reported). Mechanism: in MT remote frees arrive
between scans; after one empty scan v1 lets the owner carve 1/8 of the whole
drained range while new publications wait.

**Variant v2** (same file, same header `u32`, now `start | end << 15 |
streak << 30` in 256-byte units):

- an empty scan extends a saturating streak, a reclaiming drain resets it
  and closes the window; the window opens only on the third consecutive
  empty scan;
- the window is 1/16 of the drained payload range (was 1/8);
- the no-carve rule and everything else are unchanged.

Thresholds 1–5 and the oracle are unchanged and are judged on v2 alone with
every measurement repeated: callgrind, growth16 time (100 quads), the
11-kernel ablation (20 processes; the amendment-2 rule applies again to any
kernel left imprecise on a no-harm rule), MT time quads (100 + 50 null),
segments, and peak RSS with 100 quads for MT and 20 for growth16.
