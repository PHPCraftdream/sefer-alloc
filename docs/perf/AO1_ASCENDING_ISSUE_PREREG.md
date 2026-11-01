# AO-1 confirmation campaign — preregistration (`perf(runtime)` candidate)

Written before the confirmation data below is collected. Candidate: commit
`f8fb5638` (branch `g1-asc`) over base `82626f9c` (`main`). The change
reverses the filled prefix of a magazine refill in
`HeapCore::refill_magazine_slow`, so a fresh same-segment carve run is
issued in ascending address order.

Entry point under test: `SeferAlloc`'s `GlobalAlloc` methods on a static
instance (`examples/g1_wild_judge.rs`, the `__rust_alloc` boundary), the
layer `#[global_allocator]` users reach.

## Why a campaign

Exploratory runs (`.rush/g1/wg1`, `.rush/g1/ao1wj`, `.rush/g1/ao1bt`,
summarised in the report) showed grow64 0.69–0.80 and grow1k 0.68–0.79
AO-1/base on Windows, grow64 0.78–0.82 on Linux, and mixed signs elsewhere
(Windows wild median +4.5 %, Linux growmix median +3.9 %, large noisy).
The paired `bench:table` magnitude rule (`scripts/g1_magnitude_verdict.mjs`)
returned NO-GO on both sets (s1 sefer-time gm +1.0 %, s2 −0.4 %; largest
agreed loss +1.8 %). Those runs are exploratory and are not reused as
evidence for the verdict below.

## Design

- Arms: `base` (judge built at `82626f9c`), `base2` (a byte-identical copy
  of the base executable: the A/A control), `x` (judge built at
  `f8fb5638`). Feature set `production`, release, one arm per process.
- Blocks: 8 blocks per platform. Each block runs every scenario once per
  arm in a block order taken from a fixed rotation of the six permutations
  of (base, base2, x); the scenario order inside a block is shuffled by a
  fixed seed (`docs/perf/ao1/run.mjs`).
- Scenarios: grow64, grow1k, regrow, growmix, wild, phase, large (Windows,
  scale 1); grow64, grow1k, growmix, wild, regrow (Linux WSL, scale 0.5).
- Endpoint: whole-run ns/op as printed by the judge (no split timer).
- Statistic: per block and scenario, d = ln(x / base) and the A/A control
  a = ln(base2 / base); mean over the 8 blocks with a two-sided 95 %
  t-interval (7 degrees of freedom), reported as exp(mean) and exp(CI).
- Path-activation oracle: an `internals` build of both trees records
  `seg_reserved` / `seg_released` per scenario (equal counts expected), and
  the address-delta histogram of grow64 (ascending in `x`, a sawtooth in
  `base`).

## Gate (fixed now)

1. A/A control: every scenario's a-interval contains 0. If not, the
   campaign is void (HOLD) and is not rerun until the cause is named.
2. Target: grow64 on Windows: exp(mean d) ≤ 0.90 and the upper CI bound
   < 1.00.
3. Non-inferiority, every other cell of both platforms: upper CI bound of
   exp(d) ≤ 1.03.
4. Kill: any cell whose lower CI bound of exp(d) > 1.03 → NO-GO.
5. Otherwise (a control cell with upper bound > 1.03 but lower bound
   ≤ 1.03) → HOLD; the campaign is not rerun until a favourable result.
6. `bench:table` is a non-regression control for this change (the reversal
   runs only on a magazine refill; the tight churn cells are served from the
   magazine). Two paired sets are rerun after the judge campaign; the
   magnitude rule's improvement clause is not applied, and this is an
   explicit exception: pass requires, in both sets, sefer-time gm ≤ +1.0 %,
   largest agreed loss ≤ +3 % and no new LOSS cell against mimalloc.
   MT control: callgrind Ir of `g1_mt_one` mstress/larson T=1 within
   ±0.1 % (exploratory: +0.01 %).

GO only if 1–4 and 6 hold.

## Campaign 1 result (measured before this amendment)

HOLD (`docs/perf/ao1/campaign1_tables.md`, `AO1_ASCENDING_ISSUE_summary.csv`):
grow64 Windows 0.797 (0.732–0.867), Linux 0.794 (0.731–0.863), Windows grow1k
0.834 (0.745–0.933); no cell met the kill criterion; the other intervals
crossed +3 % and the A/A control was as wide (up to 0.343–1.412), so the
campaign could not resolve non-inferiority. Campaign 1 measured v1
(`f8fb5638`, tag `archive/ao1-v1-measured`).

## Amendment 1 — campaign 2 (written before its data)

Reason: campaign 1 was underpowered (A/A width), and review (@ox, rush
reviewer) moved the change to its narrowest form, v2: `try_carve_batch`
writes a magazine refill's run top-down (no extra pass, free-list order
unchanged). The candidate is the v2 commit on branch `g1-ao1` over the same
base `82626f9c`. Changes to the design, nothing else:

- 12 blocks per platform; each (block, arm, scenario) runs 3 times back to
  back (`run.mjs --repeats 3`) and the block statistic uses the minimum of
  the 3 per arm. Statistic, intervals (t, 11 degrees of freedom), A/A
  control and the non-inferiority / kill rules (gate items 1, 3, 4, 5) are
  unchanged.
- Target (item 2) widens: grow64 on Windows and on Linux, and grow1k on
  Windows, each exp(mean d) ≤ 0.90 with the upper bound < 1.00.
- Oracle: `WJ_ORDER=1` (judge, outside timed runs) prints the share of
  `+size` steps in grow64/grow1k: ≈ 1/16 in base, ≈ 1 in x.
- Controls (item 6) for v2: two paired `bench:table` sets, the explicit
  exception as written above; callgrind Ir of `g1_mt_one` mstress/larson T=1
  within ±0.1 %.
- If campaign 2 still reads HOLD, the decision goes to the owner with both
  campaigns' data; there is no campaign 3.
