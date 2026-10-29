# G1-K7 — pre-registration (fixed before any J1′ v2 run of this change)

Shipping candidate: the budgeted depot overflow
(`LargeCacheConfig::depot_budget_bytes`, default 1 MiB per heap), design
`docs/design/2026-10-28-depot-budget-design.md` rev. 2 on branch `g1-k7`.
Base: `g1-k5`@`2bc830cc` (main `37780bc7` + G1-K1 + G1-K5). Measurement
branch `g1-k7-meas` = the ablation harness of `g1-p0-meas` (`aab1a2b0`) +
the two new judge kernels (`bulk64k`, `bulk16k_32`).

Variant (patch in `docs/perf/b4_ablate/patches/`):

- `k7_depot_overflow` — `git diff 2bc830cc 2680410a -- src` (the whole
  K7 change: overflow storage, decay, inline gate, diagnostics, Kani).
  Patch sha256 `84ac238d50e833bd636e1330d21b2b5f7127ffe4c8baa035f4946c6524f941f8`.

Decision source: the owner's acceptance rule (2026-10-28) — «приём — любой
выигрыш против mimalloc без проигрышей на замерах других типов», and
without a safety loss.

## Runs, in this order

1. **Ablation, J1′ v2** (`scripts/b4_ablate_run.mjs`, 20 processes × 2000
   blocks, all 13 kernels: pair16, pair64, pair_rt, bulk64, bulk16k,
   churn16, churn64, churn256, churn1024, vec512, larson, bulk64k,
   bulk16k_32), effect = variant median − null median
   (`scripts/b4_ablate_vs_null.mjs`), 95 % CI by process bootstrap. Arm A
   = K5, arm B = K7 at the default budget, same process.
2. **Main judge against mimalloc** (`scripts/b4_interleave_run.mjs
   --reps 20 --blocks 2000`, all 13 kernels) on the `g1-k7` tree and, in
   the same session right after it, on the `g1-k5` tree with the two new
   kernels (so both trees are judged against mimalloc on the same day).
3. **RSS judge** — subprocess per arm (K5; K7 at E = 0, 512 KiB, 1 MiB,
   2 MiB; mimalloc), only if run 1 and run 2 show a win.

## Decision rule

GO (ship as `perf(runtime)`) only if ALL hold:

1. Win: at least one admitted kernel of run 2 where K7's ratio
   t_mimalloc / t_sefer is higher than K5's with the two process-bootstrap
   CIs not overlapping, and in run 1 the same kernel's effect has CI upper
   < 0.
2. No loss on speed: no kernel of run 1 with CI lower > 0 (K7 slower than
   K5); no kernel of run 2 where K7's ratio is lower than K5's with
   non-overlapping CIs.
3. No loss on RSS (run 3): no snapshot worse than K5 outside noise, except
   retention inside the declared budget (E + overflow metadata), which is
   published as a number.
4. No loss on Ir (`npm run iai` against the base) beyond the run-1
   evidence; the default change refreshes `bench:table` and `iai`.
5. Per-arm evidence (R26-4 / R30-8): requested, resolved and effective
   budget read back through `#[global_allocator]`
   (`SeferAlloc::dbg_current_depot_overflow`), `config_conflicts_total`
   delta 0, process boundary; activation oracle — overflow parks > 0 and
   reloads ≈ parks on bulk16k / bulk16k_32, budget refusals or ring-full
   gating on bulk64k, decay flushes 0 on the bulk kernels.

NO-GO otherwise; a partial result (win with a loss elsewhere) goes to the
@oh and rush consultation, not to a merge.

Expected (engineering target, not a gate): bulk16k ≥ 75 % of the P0
ceiling effect (P0: −56.13 in 100·ln units); bulk64k ≈ K5 (4 MiB live is
over the budget: one class's ring fills and the rest flushes as in K5).
Published caveat: E = 1 MiB covers bulk16k with ≈ 9 KiB to spare (needs
≈ 1,039,360 B) — tuned to the judge; the RSS sweep shows the behaviour
past the threshold.

## Addendum 1 (2026-10-29, before any run of v2)

Run 1 of the variant above (raws `.rush/g1/k7abl/`, table
`vsnull_k7.csv`) failed rule 2: bulk64 +14.19 [13.10, 15.14],
bulk16k_32 +7.47 [6.05, 9.20], pair64 +0.20 [0.01, 0.32], pair_rt +0.26
[0.12, 0.49] (wins: bulk16k −50.58, bulk64k −8.79, churn64 −6.26,
churn256 −4.70, vec512 −1.82). Causes named with @oh and the rush
reviewer: a per-event scan of all depot classes, and cold refusals when
the shared budget is full. Variant v1 is NO-GO as measured.

Variant v2 — `k7_depot_overflow_v2` = `git diff 2bc830cc 980c9b9a -- src`
(inline per-class park limit with a refusal latch, O(1) decay tick,
division-free ring index), patch sha256 `6d2c47163f19430d299da5d11ceb959c75a9174e25e876085d829c3d87aa7dc2`. Same runs, same decision
rule, same 13 kernels.

## Addendum 2 (2026-10-29, before any run of v3)

Run of v2 (raws `.rush/g1/k7abl2/`): bulk16k −71.53, bulk64k −10.88,
churn256 −3.99; losses bulk64 +9.24 [7.82, 10.22], bulk16k_32 +7.18 [5.49,
9.37], pair16 +1.07 [0.57, 2.90], pair64 +1.18 [0.63, 2.17] — NO-GO by rule
2. Isolated mix (pair16, pair64, bulk64, bulk16k_32; raws
`.rush/g1/k7abl2iso/`): bulk16k_32 −28.09, bulk64 +1.35, pair16 +0.58,
pair64 +0.52. Counter oracle: bulk64 runs on the overflow tail (24 overflow
parks/reloads per portion).

Variant v3 — `k7_depot_overflow_v3` = `git diff 2bc830cc 3cef5a58 -- src` (floor
first; floor traffic does not tick the decay), patch sha256 `f1659242205025907fa4d2d8ffea845992e76ae564223fed2cc8586bf29fa85f`. Same
runs, decision rule and kernels.

## Addendum 3 (2026-10-29, before any split-group run)

Run of v3 (raws `.rush/g1/k7abl3/`): bulk16k −59.91, bulk64k −7.62,
churn256 −5.92, churn64 −3.98, vec512 −1.09; losses bulk64 +11.55 [10.92,
12.32], pair16 +1.27 [0.71, 1.62], pair64 +1.20 [0.75, 1.59] (churn16
+4.95, bulk16k_32 +5.56 imprecise) — NO-GO by rule 2 as measured.

Run 2 (main judge, 13 kernels; raws `.rush/g1/k7v3mi/`, `.rush/g1/k5mi13/`)
is void as a judge: 12/13 and 13/13 kernels not admitted, the sefer A/A
median sits at about −10 % on pair16/pair64/bulk64 for BOTH trees, while
the same judge with the original 11 kernels had it at ≈ +0.1 %
(`.rush/g1/k1mi/`, `.rush/g1/k5mi/`). Mimalloc is affected too: churn256
11.4 → 44.4 ticks/call, churn256 A/B −29.8 % → +29.0 % (K5 tree). Cause
named: the two large cohorts (4 MiB / 512 KiB live, one pass per portion)
evict caches and leave allocator state that the next kernel's portions pay
for, unequally between the two copies of one allocator. The same
contamination applies to run 1 (null bias bulk64 −11.16 %), so the v1–v3
losses are not yet attributable to K7's code path; the isolated mix of
Addendum 2 (without bulk64k) showed bulk64 +1.35, pair +0.5.

From here, every run uses three kernel groups, each in its own processes
(`--kernels`, judge commit `9a279d56` on `g1-k7`, cherry-picked as
`417774d4` on `g1-k5-j`; block orders and portion seeds stay keyed by the
registry index, so G11 replays the old 11-kernel portions exactly):

- G11: pair16, pair64, pair_rt, bulk64, bulk16k, churn16, churn64,
  churn256, churn1024, vec512, larson;
- GL1: bulk64k;
- GL2: bulk16k_32.

Order: run 1 (ablation, variant v3, unchanged patch) for G11, GL1, GL2;
then run 2 for G11, GL1, GL2 on `g1-k7`@`3cef5a58`+`9a279d56`, each
followed by the same group on `g1-k5-j`. The decision rule is unchanged and
applies to the union of the three groups (13 kernels). A group whose A/A is
not admitted is reported as not admitted, not re-run selectively.

## Addendum 4 (2026-10-29, before any old-judge run 2)

Split-group results (raws `.rush/g1/split/`): run 1 G11 — bulk16k −51.04,
churn1024 −2.21; losses bulk64 +10.06 [9.53, 10.63], larson +2.99 [2.07,
3.68], pair16 +0.77, pair64 +0.83. GL1 bulk64k −9.93, GL2 bulk16k_32
−25.76. Diagnostics: {pair16, pair64, bulk64} bulk64 +11.90, pair +0.73;
{bulk64, bulk16k} bulk64 +11.66 — the bulk64 loss does not need the large
cohorts. Counter oracle on the G11 mix: zero overflow parks/reloads on
pair16/pair64/pair_rt/bulk64/vec512. Callgrind (`b4_pmu_probe`, K5
`2bc830cc` vs K7 `3cef5a58`): pair16 +2,776 Ir of 70.07 M, pair64 +2,843,
churn16 +6,681 of 35.37 M (≈ 0), bulk64 +656,069 of 56.46 M (+1.16 %;
≈ 26 Ir per magazine event in `depot_reload`, `refill_magazine_slow`,
`depot_park`).

Run 2 with the new judge binary is void for a second reason: the sefer A/A
is about −10 % in every group (even a single kernel), while the unchanged
11-kernel judge binary measured the same session at A/A ≈ +0.1 %
(`.rush/g1/split/oldjudge_k5/`, 6 processes) — a layout artifact of the
judge build, not of the trees.

Run 2 is therefore taken on the 11-kernel judge exactly as it was at
`2bc830cc`: branch `g1-k7-j11` = `g1-k7`@`9a279d56` with the two judge
commits reverted (`64974d79`; harness files byte-identical to `2bc830cc`),
against `2bc830cc` itself. 20 processes × 2000 blocks, order K7, K5, K7, K5
(the repeat measures session drift). bulk64k / bulk16k_32 stay judged by
run 1 only (GL1/GL2); their run-2 rows are reported as not admitted. Decision
rule unchanged.
