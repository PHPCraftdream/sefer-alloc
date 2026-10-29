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

## Addendum 5 (2026-10-29, before any run of v4)

Run 2 of v3 on the unchanged 11-kernel judge (raws `.rush/g1/j11/`, order
K7, K5, K7, K5, all A/A admitted except where marked): bulk16k ratio
t_mi/t_sefer K7 0.817 / 0.817 vs K5 0.512 / 0.504 (K7 x1.60); bulk64 K7
0.765 / 0.770 vs K5 0.783 / 0.784 (K7 about 2 % slower, CIs disjoint in both
pairs) — a loss by rule 2. Callgrind traced it to floor bookkeeping on every
magazine event (+656k Ir on bulk64, +1.16 %).

Variant v4 — `k7_depot_overflow_v4` = `git diff 2bc830cc 9a73ba5f -- src`
(design rev. 4: K5's floor code verbatim, the overflow as cold tail calls,
one-load refill gate, floor mark byte), patch sha256
`1dfe60f10a686290bc41d0bbc0a459ca2aab22743e58c91707d383949605d4b7`.
Pre-run Callgrind (b4_pmu_probe, single kernel): bulk64 sefer Ir +19.8k of
56.46M (+0.035 %), pair16/pair64/churn16 within +0.03 %.

Runs, in this order:
1. Run 1 (ablation, J1′ v2, 20 × 2000) for G11, GL1, GL2, variant v4.
2. Run 2 (main judge) on the unchanged 11-kernel judge: `g1-k7-j11` =
   `9a73ba5f` with the two judge commits reverted (`2b517e86`; harness
   byte-identical to `2bc830cc`) against `2bc830cc`, order K7, K5, K7, K5.
   bulk64k / bulk16k_32 are judged by run 1 only.
3. Run 3 (RSS) only if runs 1–2 pass rule 2.

Decision rule unchanged (Addendum 3: union of groups).

## Addendum 6 (2026-10-29, before any run of v5)

Approved by the owner on 2026-10-29, before any run of v5, after the
consultation of @oh, the rush reviewer and @ox; the P4 finding below was put
to the owner again and the campaign confirmed on v5 as is. It changes how
rules 1–2 are measured and makes rules 3–5 operational; the acceptance rule
itself is unchanged.

### v4 result (published; enters no decision below)

Raws `.rush/g1/v4/` (`abl_G11`, `abl_GL1`, `abl_GL2`, `mi_k7j11_{1,2}`,
`mi_k5base_{1,2}`). Run 1 (effect %, 95 % CI): pair16 +0.87 [0.56, 1.03],
pair64 +0.89 [0.74, 0.99], pair_rt −0.06 [−0.20, 0.32], bulk64 −0.79 [−1.39,
−0.27], bulk16k −52.11 [−52.89, −51.11], churn16 −1.72 [−5.38, 0.62], churn64
−2.44 [−4.81, −1.22], churn256 −1.08 [−2.19, 0.85], churn1024 −3.41 [−4.97,
−1.74], vec512 −0.01 [−0.47, 1.27], larson +0.34 [−1.41, 1.85]; GL1 bulk64k
−12.32 [−13.61, −11.49]; GL2 bulk16k_32 −26.61 [−27.10, −25.71]. Run 2
(t_mi/t_sefer, order K7, K5, K7, K5): bulk16k 0.827 / 0.501 / 0.829 / 0.506;
larson 0.991 [0.982, 1.000] / 1.192 [1.075, 1.257] / 1.028 / 1.019; vec512
1.249 / 1.810 / 1.449 / 1.390 (K7_1, K7_2, K5_2 not admitted); pair16 0.824 /
0.823 / 0.826 / 0.828. Verdict as registered: NO-GO by rule 2 (run 1 pair16,
pair64; run 2 first pair larson). Rules 3–5 were not reached.

### Why the measurement changes (instrument findings, not K7 results)

1. Run 2's per-run CI omits between-run variance: K5 against itself is
   disjoint across its own two runs on vec512 ([1.65, 1.97] vs [1.25, 1.53])
   and larson ([1.075, 1.257] vs [0.993, 1.050]); v3 showed the mirror image.
   A per-pair "disjoint CIs" veto fires on session drift.
2. Run 1's null copy does not calibrate the copy's own layout change:
   pair16/pair64 read +1.07/+1.18 (v2), +1.27/+1.20 (v3), +0.77/+0.83 (v3
   split), +0.87/+0.89 (v4) — four variants with different executed code —
   while Callgrind gives ≤ +0.03 % Ir (v4), the oracle zero overflow events on
   the pair kernels, and run 2 pair16 K7 0.824/0.826 vs K5 0.823/0.828.
3. One code mechanism was found (C.1): v4 ticks the decay on every segment
   refill (`hot.rs:508`, `:693` → `tcache_flush.rs:167` →
   `depot_overflow.rs:342`), loading `HeapCore::depot` and `Depot.ovf.bytes`
   (18,440 B into `Depot`) — two lines K5's segment refill never touched,
   invisible to Ir. v5 removes it while the overflow is empty; in the G11
   mix the overflow stays full (P4 b), so there v5 ticks like v4 and the
   residual cost is executed K7 code, measured by D.

### Variant v5

`k7_depot_overflow_v5` = `git diff 2bc830cc f2b0c115 -- src` (branch `g1-k7`:
`a9fad071` + `8df3c958` + `f2b0c115`, the merge of `main` in between leaves
`src/` unchanged), patch sha256
`8b28ef60c22c8e47bb64369b2a16f9899c6b52b119b1ebf25bdaf6c8f0353a37`: v4 plus
one change — the segment-refill tick runs only while the overflow holds
bytes (in the G11 mix it always does, P4 b). `PerClass::floor_mark` becomes
`PerClass::marks` with two bits: `MARK_FLOOR` (v4's floor-park epoch mark,
set with one `or`) and `MARK_TICK` ("tick on this class's segment refill");
`PerClass::ovf` stays v4's 0/1 hint, so the reload gate is v4's single
16-bit compare. `DepotOverflow::armed: u64` mirrors the MARK_TICK bits. A
class's first overflow record (its ring was empty) sets MARK_TICK on every
small class whose mirror bit is clear; a class's own segment refill clears
its MARK_TICK when it finds `bytes == 0`; the teardown drain and
`bind_depot` clear all (bytes are 0 there). Invariants: MARK_TICK(c) ⇔
armed bit c; `bytes > 0` ⇒ armed covers every small class. v5 therefore
counts exactly the events v4 counts (v4's tick changes nothing at `bytes ==
0`): the decay schedule is v4's. `OverflowStats` gains `arms` / `disarms`.
C.2 (Small remote ingress skipped during long overflow reload series) is not
in v5; it is open item 130's sub-item and is observed by run 3's W4.

### Layout control U

`k7_depot_overflow_v5_unreached` = `git diff 2bc830cc 9e60cd93 -- src`
(branch `g1-k7-u`, one commit on `f2b0c115`), patch sha256
`9249b704f26fb997cb47e619d7bcfb9c9602d9ea46d57d8875f508800d207d96`: v5 with
every K7 call site restored to K5's source (free gate `parked <
DEPOT_CAP[c]`; floor park without the mark and without the overflow
fall-through; reload without the overflow fall-through; refill gate `parked
!= 0` with `depot_reload`; no tick; teardown drain and `bind_depot` as K5),
every K7 type, field, constructor and function kept, the cold K7 functions
kept linked by two static fn-pointer tables (`KEEP_K7_PATHS`,
`KEEP_K7_TEARDOWN`) referenced by `black_box` from `bind_depot` (once per
slot). U executes K5's code with v5's data layout.

### Before any timing run (deterministic; results recorded here)

P1. v5 green on CI (PR #128 at `f2b0c115`, run cited in the report). v5 adds
    `tests/g1_k7_depot_overflow_tick.rs` — arming and lazy disarm, a decay
    driven only by another class's segment refills, a recycled slot
    starting disarmed — with the mutants no tick, no disarm, teardown keeping
    the flags, arming only the parking class and never arming all red; Kani
    И-O1 checks both invariants over park / reload / pop_oldest / disarm
    sequences with the first-record arming rule. U is measurement-only:
    `cargo clippy --features production` clean; K7's own tests do not apply
    to it (it never parks into the overflow).
    P1 as recorded (before any timing data). CI on `f2b0c115` was red; the
    fixes that made PR #128 green changed tests, CI wiring and evidence rows,
    and in `src/` only (`git diff f2b0c115 035b129b --stat -- src`):
    `src/kani_proofs.rs` −96, `src/registry/heap_core/mod.rs` −7,
    `src/registry/heap_core/state/depot_overflow.rs` −47 / +1,
    `src/registry/heap_core/state/depot.rs` −2. Every removed line lies in a
    `#[cfg(kani)]` item (the И-O1 / И-O2 harnesses, their re-export, the
    `DepotOverflow::empty` / `kani_on_buffer` constructors and the overflow
    field init inside `Depot::empty`), moved to the follow-up Kani PR; the
    one added line is `#[allow(deprecated)]` on the `fetch_update` call.
    Neither changes production code, so the measured trees stay v5 =
    `f2b0c115`, U = `9e60cd93`, run-2 K7 = `3b6b754e`, and P2–P4 stand.
    Hashes: `git diff f2b0c115 035b129b -- src` sha256
    `6fe8fa18a812a8d5bf8a0a2d9377a2eeab4acd901e0af3068572a6c9889051a5`;
    `git diff 2bc830cc 035b129b -- src` sha256
    `12d9bad894470cbb90c6fd52da7dd32a14e6362b531fb0cef6a58ebbb536fd9c`.
    CI green on PR #128 head `035b129b`: run 40865975376 (CI, success);
    the zero-cost gate passed under the fixed base (PR #130, `59bca504`).
P2. Callgrind (`b4_pmu_probe`, single kernels: bulk64, bulk16k, bulk64k,
    bulk16k_32, pair16, pair64, churn16, churn64, churn256, churn1024) for K5
    `2bc830cc`, v4 `9a73ba5f` and v5; raws `.rush/g1/a6/p2/` (`cg_a6.log`:
    K5, v4 and v5's first form `a9fad071`; `cg_a6_v5c.log`: `f2b0c115`).
    @ox's draft criterion was v5 ≤ v4 + 0.01 % Ir on every kernel. It failed
    twice and the code was fixed before any timing data:
    - `a9fad071` (tick flag in `PerClass::ovf`): +5.0 Ir per refill for the
      masked reload gate, bulk64 +0.18 %, bulk16k +0.59 % → the flag moved to
      `PerClass::marks` (`8df3c958`);
    - `8df3c958`: the arming check on every overflow park, ≈ 4 Ir per park,
      bulk16k +0.17 % → only a class's first record arms (`f2b0c115`).
    Result at `f2b0c115` (Ir, v5 − v4): bulk64 +95 (+0.0002 %), pair16 +147,
    pair64 +163, churn16 +529, churn64 +529, churn256 +498, churn1024 +55
    (all ≤ +0.0015 %), bulk64k −86,528 (−0.053 %); bulk16k and bulk16k_32
    +70,264 (+0.085 %) — the `n == 0` arming test, 2 Ir per overflow park
    (34,510 parks), which is the irreducible cost of the flag; the draft
    criterion is NOT met on these two kernels and the excess is accepted
    here, before timing, as a named cost (v5 vs K5 on them: −31.5 % Ir).
    Timing decides it (run 1 E_v and D, run 2).
P3. U valid: `git diff f2b0c115 9e60cd93 -- src` touches no type definition
    (no `struct`, `enum`, `union` or `#[repr]` line), so every allocator data
    layout is v5's; U's oracle shows zero overflow events and zero arms on
    all 13 kernels and on the G11 subset (P4 a, b). Code layout cannot be
    made identical and is not a gate.
P4. Overflow oracle `examples/g1_k7_overflow_oracle.rs`, 200 cycles, raws
    `.rush/g1/a6/p4/` (`oracle_v4*`, `oracle_v5c*`, `oracle_Uc*`); trees v4
    `9a73ba5f`, v5 `f2b0c115`, U `9e60cd93`.
    (a) All 13 kernels round-robin: v4 and v5 give identical bytes, parks,
        reloads, budget_refusals, os_refusals, decay_flushed, ingress_checks
        and decommit_calls on every kernel; U shows zero on every counter,
        arms and disarms included.
    (b) G11 subset (the regime runs 1 and 2 judge): v4 and v5 identical on
        the same counters, U zero; v5 arms = 1 (bulk16k), disarms = 0. From
        bulk16k on the overflow holds bulk16k's 64 B-class records (bytes ≥
        1,040,384 at the end of every kernel) and never empties, so every
        G11 segment refill ticks the decay exactly as in v4. The 64 B class
        parks every cycle, so decay keeps it (decay_flushed 955 records over
        200 cycles in all); churn256, churn1024 and larson meet a full budget
        (budget_refusals 200 / 200 / 1127).
    (c) All 13 kernels, v5: arms = 1 on bulk16k; arms = disarms = 200 on
        bulk64k (the overflow empties and refills there once per cycle); 0
        on every other kernel.
    (d) pair16 + pair64 alone, v5: bytes = 0, arms = 0.
    Recorded before any timing data: v5 removes C.1 only while the overflow
    is empty. In the G11 regime v5 differs from v4 by the MARK_TICK test,
    the arming test and the data layout; the remaining tick cost is
    executed K7 code and is measured by D.
P5. Analysis scripts, committed with this addendum: `scripts/g1_k7_a6_ablation.mjs`
    (E_v, E_u, D; sha256
    `d2a27f995674bc805651d3604a616eb5274994dcf6ca9aed4ffc4a5c8d8ffc87`) and
    `scripts/g1_k7_a6_judge.mjs` (two-level bootstrap, admission,
    judgeability, the speed decision; sha256
    `cd6431c5504e9ebab79566560d6ccc02d44a3c8573b8f1328b0d0c158952e311`). Both
    were exercised on the v4 raws for their code paths only (v4 enters no
    decision); the ablation script reproduces `b4_ablate_vs_null.mjs`'s
    effects on them. `scripts/b4_ablate_run.mjs` now runs an explicit
    `--variants` list in the given order (sha256
    `f33a10c95bfceefbb5eb31bb56e090c295fd121e33fb324186e7609b27f8ba7a`), so
    the two passes can reverse v5 and U.

### Runs, in this order, one session, identical RUSTFLAGS for every build

RUSTFLAGS `-C llvm-args=-align-all-functions=6 -C
llvm-args=-align-all-nofallthru-blocks=5` (as Addenda 3–5).

1. Run 1, G11, `scripts/b4_ablate_run.mjs --reps 20 --blocks 2000
   --no-null-gate --kernels <G11>`, two invocations: pass 1 `--variants
   k7_depot_overflow_v5,k7_depot_overflow_v5_unreached --order-seed
   0x0b4d5eed`; pass 2 `--variants
   k7_depot_overflow_v5_unreached,k7_depot_overflow_v5 --order-seed
   0x0b4d6eed` (the runner runs null first in each). Then GL1 (`bulk64k`)
   and GL2 (`bulk16k_32`), one invocation each, `--variants
   k7_depot_overflow_v5`, 20 × 2000, order seed 0x0b4d5eed.
2. Run 2, the unchanged 11-kernel judge: tree K7 = `g1-k7-j11-v5` @
   `3b6b754e` (v5 with the two judge commits reverted; its `src/` diff
   against `2bc830cc` has v5's patch sha256, the judge harness is
   byte-identical to `2bc830cc`), tree K5 = `2bc830cc`. Each tree in its own
   CARGO_TARGET_DIR; 16 runs of `scripts/b4_interleave_run.mjs --reps 20
   --blocks 2000` in the order K7 K5 K5 K7 K5 K7 K7 K5 K7 K5 K5 K7 K5 K7 K7
   K5; exe_sha256 constant per tree (checked by the judge script).
   bulk64k / bulk16k_32 stay judged by run 1 only.
3. Run 3 (RSS), only if rules 1–2 pass.
4. `npm run iai` against `2bc830cc` (CI), only if rules 1–3 pass.

Raws go to `.rush/g1/a6/`. An invocation that aborts on a hard check is
re-run once with identical arguments, both published; a second abort ends
Addendum 6 as inconclusive (no merge). No run is repeated for any other
reason; nothing is re-run selectively.

### Statistics (P5 scripts only; B = 10,000; 95 % percentile CIs; seed 0x5EED)

Run 1, per kernel, 100·ln units, the per-process statistic of
`b4_ablate_vs_null.mjs`: E_v = median(v5) − median(null), E_u = median(U) −
median(null), D = median(v5) − median(U); each set pools its 40 processes of
both passes; process bootstrap stratified by pass. GL1 / GL2: E_v from 20
processes. The null precision label is reported and changes no rule.

Run 2, per kernel and run: s = the rep-table statistic (median over
processes of the per-process median of ln(t_mi/t_sefer)); admission per
(run, kernel) unchanged (the judge's own `admitted`: A/A sefer and A/A
mimalloc |median| ≤ 1 % and CI half-width ≤ 1 %). A kernel is judgeable iff
each tree has ≥ 4 of 8 admitted runs. M_T = mean of s over T's admitted
runs; CI_T by a two-level bootstrap (runs with replacement, then the 20
processes within each drawn run). Published, not deciding: per-run rows,
M_K7 − M_K5, each tree's between-run spread.

### Decision rule (replaces rules 1–2 for v5; rules 3–5 made operational)

GO (ship as `perf(runtime)`) only if ALL hold:

1. Win: a judgeable run-2 kernel with CI_K7 entirely above CI_K5, and in run
   1 the same kernel has E_v and D with CI upper < 0.
2. No loss on speed:
   a. run 1, G11: no kernel with E_v CI lower > 0, except a kernel with D CI
      lower ≤ 0, E_u CI lower > 0 and judgeable in run 2 — its run-1 signal
      is attributed to the ablation binary's layout and run 2 decides it.
      GL1 / GL2: no E_v CI lower > 0, no exception;
   b. run 2: no judgeable kernel with CI_K7 entirely below CI_K5. A kernel
      that is not judgeable is decided by run 1 alone, without the 2a
      exception.
3. No loss on RSS (run 3): harness `examples/g1_k7_rss_judge.rs`, added
   identically to both trees and committed with its sha256 before any run-3
   data. One subprocess per (arm, workload, repetition); arms K5, v5 at
   E = 0, 512 KiB, 1 MiB, 2 MiB (set at build time), mimalloc (reference,
   not judged); workloads W1 = the oracle's G11 shapes round-robin, W2 =
   bulk64k, W3 = bulk16k_32, each on a worker thread that exits; W4 =
   `examples/rss_probe.rs` unchanged (cross-thread frees), K5 vs v5 at the
   default E only; W5 (phase change, rush reviewer) = bulk16k until the
   overflow is full, then the 64 B class stops for good while the worker
   (i) idles and (ii) runs churn256 segment refills for a fixed number of
   events. Snapshots S1 peak, S2 after the last free (thread alive), S3 1 s
   after thread exit; W5 adds snapshots before the phase change, after the
   fixed number of events, after the idle and after thread exit, with
   overflow bytes / committed record bytes / decay_flushed read back at
   each. 5 repetitions, arm order rotated by one position per round;
   statistic = median of 5. Noise N = max(64 KiB, max_i |K5_i − median K5|)
   per (workload, snapshot). Loss: median(v5, E) − median(K5) > N + A, where
   A = E + the overflow's committed record bytes at S1 / S2 and the W5
   live-thread snapshots, A = committed record bytes at S3, A = 0 for E = 0.
   Retention at S2 (and along W5) is published per E.
4. No loss on Ir: per bench, ΔIr and ΔEstimated Cycles against `2bc830cc`
   and the bench's overflow parks (one bench-internals counter run of the
   same bench body). Loss: on a bench with zero overflow parks, (Ir − B) of
   v5 exceeds the base's by more than 10 (B = `large_alloc_free_cycle`, the
   bootstrap proxy; 10 = the hot-path kill threshold,
   `docs/perf/IAI_BASELINE.md`). Benches with parks > 0 publish ΔIr with the
   park count; their time cost is the run-1 / run-2 evidence.
5. Evidence (R26-4 / R30-8): per arm, requested, resolved and effective
   budget through `#[global_allocator]`
   (`SeferAlloc::dbg_current_depot_overflow`), `config_conflicts_total`
   delta 0, subprocess boundary; activation — overflow parks > 0 and
   reloads ≈ parks on bulk16k / bulk16k_32, budget refusals or ring-full
   gating on bulk64k, decay flushes 0 on the bulk kernels; the counters of
   all 13 kernels (larson, churn*, vec512 included) published; P3 and P4
   hold.

NO-GO otherwise. Overlapping CIs mean no detected difference, not
equivalence; no non-inferiority margin is claimed. GO also refreshes
`bench:table` and `iai` in the same change.

### Finite

Addendum 6 is the last re-measurement of design revisions 4–5. If v5 fails
any rule, G1-K7 is NO-GO for this round; v4 and v5 data stay published; a
later attempt needs a new design and a new pre-registration. Data of v1–v4
never enter a v5 decision.

## Addendum 7 (2026-10-29, after the Addendum 6 speed decision, before run 3)

Written after the run 1 / run 2 data (`G1_K7_A6_VERDICT.md`), so it is an
owner decision on a known result, not a pre-registered rule.

Rule 2a stands. The owner grants one exception to it for v5, on the
measured trade-off: the rule-2a loss is pair16 E_v +0.62 % [0.47, 0.73] and
pair64 +0.73 % [0.61, 0.79] in run 1, not seen by run 2 (pair16 K7/K5 1.0021,
no detected difference; pair64 1.0181, K7 faster), against rule-1 wins of
bulk16k 1.675, churn64 1.081 and churn1024 1.047 in run 2 and bulk64k
−9.88 %, bulk16k_32 −26.82 % in run 1. Owner's words: «правило не снято.
Тут просто разумное исключение. Трейдоф в нашу пользу».

Scope of the exception: exactly these two kernels at these magnitudes, for
v5 (`f2b0c115`, run-2 tree `3b6b754e`). It is not a precedent and does not
change rule 2a for any later candidate.

Everything else in Addendum 6 holds unchanged. Rules 3 (RSS, run 3), 4 (Ir,
`iai` against `2bc830cc`) and 5 (evidence) remain conditions for merging,
in the registered order: run 3's harness `examples/g1_k7_rss_judge.rs` is
committed identically to both trees with its sha256 before any run-3 data.
If rule 3 or 4 fails, G1-K7 is NO-GO as Addendum 6 § Finite says.

## Addendum 8 (2026-10-29, before any run-3 data)

Run 3 as implemented, after two rush reviews (`.rush/stdin/g1-k7-rss-prereg{,2}.answer.md`
on the measurement host). Rule 3's loss rule and N are unchanged; this
fixes the operational details and the deviations from its text.

Identity, committed before any run-3 data:
- harness `examples/g1_k7_rss_judge.rs`, byte-identical in both trees,
  sha256 `f0ea733269fc714ed7ab51c04c980b9589404a27721abdf63aa4eaaa371754df`;
  K5 tree `5d81dc7d` (= `2bc830cc` + the harness, branch `g1-k5-rss`), K7
  tree `1decb4e8` (= run-2 tree `3b6b754e` + the harness; `src/` unchanged);
- driver `docs/perf/b4_ablate/a6/run3/run3.sh` sha256 `da4adf351b3efc977d02b4cd9a307b7acba7debe906ebd61a27f30ef41d5be4d` (refuses to start
  unless both trees are at these commits, clean, and the harness hash
  matches; one retry per invocation, every attempt kept, a second failure
  ends run 3 as inconclusive);
- analysis `scripts/g1_k7_a6_rss.mjs` sha256 `96295f0fea97f188d4e71774b6b3b134d5fb7f30f46a510731a4d2e4ade022b2`.

Arms are fixed at build time (`--cfg g1_k7` + `G1_K7_E`, `--cfg g1_mi`,
none), one subprocess per (arm, workload, repetition), arm order rotated by
one position per round; W4 alternates its two arms.

Workloads (every allocated block gets one byte written):
- W1: the oracle's G11 shapes round-robin, 50 cycles; W2: bulk64k, 50
  passes; W3: bulk16k_32, 50 passes.
- W5 is split into two branches (deviation): W5i = fill, P0, 1 s idle, P2;
  W5c = fill, P0, 50 rounds of 4096 × 256 B allocated then freed, P1. The
  fill is 64 bulk16k passes; it must reach a cohort steady state (the last 4
  per-pass overflow byte counts equal). One bulk16k cohort is 1 MiB, so at
  E = 2 MiB the plateau lies below the budget (deviation: "until the
  overflow is full" is operationalised as this plateau, not a full budget).
  W5c is a bulk256 park/reload workload: its refills come from segments or
  from the overflow (both tick the decay); the number of segment refills is
  not fixed (deviation from "a fixed number of segment refills").
- W4: `examples/rss_probe.rs` unchanged, K5 vs v5 at the default E; it has
  no budget read-back or conflict delta (the probe uses `SeferAlloc::new()`,
  the default budget by construction); its evidence is the binary identity.

Snapshots and allowance A (E = 0: A = 0 everywhere):
- S1 = process peak RSS (PeakWorkingSetSize), A = E + the maximum overflow
  committed record bytes over per-pass checkpoints (not the metadata at the
  instant of the peak);
- S2, P0, P1, P2 (thread alive): A = E + committed record bytes at that
  snapshot;
- S3 (1 s after thread exit): A = 0 (teardown releases the record pages);
- W4 peak and final: A = 0.
All snapshots are decisive. Statistic = median of 5; N as registered.

Evidence (rule 5) is split: P3 / P4 (recorded in Addendum 6) carry the
per-kernel oracle of all 13 kernels in the mixed round-robin regime; run 3
checks its own isolated arms: requested = E, resolved = E rounded down to
256 B, effective = resolved, config_conflicts_delta = 0, every RSS read > 0
and every counter a non-negative integer; E = 0: no overflow parks or bytes;
E > 0: parks > 0 on every workload, "reloads ≈ parks" operationalised as
reloads ≥ 0.9 · parks on W3 (S2) and on the W5 fill (P0), budget refusals > 0
on W2, decay_flushed = 0 on W2 / W3 (S2) and on the W5 fill (P0); K5 and
mimalloc report zero overflow counters. A failed check makes run 3
unusable, not a pass.

## Addendum 9 (2026-10-29, after run-3 data, analysis defect only)

The run-3 campaign completed (no retries; 150 judge processes, 10 W4
probes). The analysis script stopped on its own evidence check
"k7_524288 W2 rep 1: no budget refusal on bulk64k". The check was narrower
than the registered rule: Addendum 6 rule 5 asks for "budget refusals **or
ring-full gating** on bulk64k", and Addendum 8 dropped the second branch.
The data show the second branch in every K7 W2 process: overflow bytes =
the resolved budget exactly (524 288 / 1 048 576 / 2 097 152), parks =
25 600 / 51 200 / 102 400, budget_refusals = 0 — after the overflow fills,
`DepotOverflow::starve` latches the class (`PerClass::limit` lowered) and no
further park is attempted, so nothing is refused. The script now checks
`budget_refusals > 0 || overflow bytes == resolved budget`
(sha256 `3b393518b85343ec22c9235ef7b9018dccbc8b2e08628308ca8c84c33d4bf5d1`); no other check, threshold, statistic or the loss rule
changes. This is a correction of the analysis toward the registered text,
written after the data and before any RSS number was read.

## Addendum 10 (2026-10-29, after the rule-4 data)

Rule 4 (`a6/run4/rule4.log`, gate-run CI run 40901604462, K5 `2bc830cc` vs
PR #128's head commit `4e4bc5b8` at the time; corrected wording, the r20 runner measures HEAD): 8 of 106 benches exceed the +10 Ir bound
after the bootstrap proxy — dealloc_flush_all_tcache_16b +430,
seg_cycle_decommit_256k +421, multiseg_cold_256k +157,
cold_alloc_only_256x16b n / 2n / 4n +31 / +87 / +151, realloc_grow +48,
medium_class_dealloc_churn_16b +25 — none of whose bodies can park into the
overflow; the per-heap bootstrap grows by ≈ 2.4–2.6 k Ir. Wins:
growth16 −17 %, dealloc_free_only_1088_16b_n1024 −13.8 %,
cold_alloc_free_256x16b_4n −13.6 %, recycle_alloc_free −7.9 % (raw Ir).

Rule 4 stands. The owner, shown the full trade-off table (time, Ir, RSS),
grants a second exception for v5, scoped to exactly these 8 benches at
these magnitudes («отлично! Вливаем»). Not a precedent. Rules 1, 3 and 5
hold as measured; G1-K7 v5 ships as `perf(runtime)`. The fixed costs
(flush-all, segment release, bootstrap, segment-refill check) are a
follow-up with their own measurement.
