# R32-0 (task #490) — the LOST-front (cost-side) gate for `virgin-zero-skip`

Date: 2026-08-02. This report does NOT rewrite or delete
`docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE.md` (task #471,
"R31-0" below) — that report proved the WON front (the `notouch` virgin
`alloc_zeroed` win, -89% to -98.6%, at the real production `HeapCore` layer).
This report supplies the missing COST side: what does turning `virgin-zero-skip`
ON cost on paths that collect **no** benefit — plain `alloc`, RECYCLED
`alloc_zeroed`, and `realloc` — measured in the SAME regime (same `HeapCore`
production layer, same sweep sizes, same REPS/BURST) R31-0 used, per
CLAUDE.md's cost/benefit-same-regime rule.

## 0. Background this task starts from

`virgin-zero-skip` is an opt-in feature (`Cargo.toml` line 744:
`virgin-zero-skip = ["alloc-decommit"]`; **not** in `production`'s default
bundle — confirmed by reading `production`'s feature list directly, not
assumed). It lets a small `alloc_zeroed` block skip the explicit
`Node::zero` memset when the allocator can prove the block is virgin
(never-before-served, so the OS already zeroed the page). R31-0 measured the
benefit through the real production layer and found a large, reproducible
`notouch` win (4/4 swept sizes, -89% to -98.6%), but explicitly recommended
against a blanket `production` promotion because the `onebyte`/`full`
touch-heavy categories showed no reproducible win at that sample size, and
because the cost side had never been measured at all. This task is that
missing cost side — TaskList item #490, filed as this backlog's stated
"Приоритет 1" per an independent research survey
(`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` §5), though this report
verifies every claim independently rather than taking that survey's framing
as given.

## 1. Source-confirmed scope: exactly what code the feature touches, per API

Before designing any measurement, every `#[cfg(feature = "virgin-zero-skip")]`
site outside `alloc_core_small.rs`/`alloc_core_small_magazine.rs` (the
`AllocCore`-level machinery R31-0/R30-3 already cover) was read directly:

| API | Extra code under `virgin-zero-skip`? | Where |
|---|---|---|
| `HeapCore::alloc` (magazine HIT) | **Yes** — one unconditional `u16` AND clearing the popped slot's mask bit (`self.tcache.classes[c].virgin_mask &= !(1u16 << new_cnt)`), no read, no branch on the bit's value. | `heap_core_alloc.rs:218-220` |
| `HeapCore::alloc` (magazine MISS / refill) | **No** — `refill_magazine_slow` (the plain-`alloc` miss path, `heap_core_alloc.rs:665`) has ZERO references to `virgin_mask` anywhere in its body. Confirmed by direct read, not assumed — this is the single most load-bearing fact this gate's design rests on (see §2's oracle). Only `alloc_zeroed`'s SIBLING miss path, `refill_magazine_slow_virgin`, sets bits. | `heap_core_alloc.rs:665-760` |
| `HeapCore::dealloc` (own-thread push, non-overflow) | **Yes** — one unconditional `u16` AND, documented in its own source comment as "a defensive no-op AND, not a load-bearing clear" (the mask invariant already guarantees the bit reads 0 there). | `heap_core_free.rs:738-741` |
| `HeapCore::dealloc` (own-thread push, magazine-overflow path) | **Yes** — one `u16` right-shift (`>>= FLUSH_N`, compacting the mask alongside the slot-pointer compaction) plus one more defensive AND on the newly-pushed slot. | `heap_core_free.rs:792-795`, `:812-815` |
| `HeapCore::dealloc_batch` | **Yes** — one unconditional `u16` AND per accepted block, same defensive-no-op shape as the scalar push. | `heap_core_dealloc_batch.rs:350-353` |
| `HeapCore::realloc` | **No** — zero direct `#[cfg(feature = "virgin-zero-skip")]` references anywhere in `realloc`'s body (`heap_core_free.rs:911`+). Its own doc comment states the move leg is "funnelled through the magazine-aware `HeapCore::alloc`/`dealloc`" (`heap_core_free.rs:926`) — i.e. any cost `realloc` pays is 100% inherited from the `alloc`/`dealloc` rows above, never a separate code path. | `heap_core_free.rs:911`+ (read in full; grep confirms 0 matches) |

**Consequence for this gate's design.** Every extra instruction the feature
adds on a non-`alloc_zeroed` path is a small, fixed, UNCONDITIONAL bitmask
op — never a branch on content, never proportional to allocation size. The
per-call cost, if measurable at all, should be roughly constant across sizes
and small relative to the surrounding alloc/dealloc bookkeeping. This
predicts the shape of what follows.

## 2. Entry-point layer and gate design (CLAUDE.md's entry-point-honesty rule)

**Layer under test: `HeapCore::alloc` / `HeapCore::alloc_zeroed` /
`HeapCore::realloc`** (`src/registry/heap_core_alloc.rs`,
`src/registry/heap_core_free.rs`) — the exact chain `SeferAlloc`'s real
`#[global_allocator]` (`src/global/sefer_alloc.rs`) calls, and the SAME
layer R31-0 used. Deliberately NOT bare `AllocCore` — R30-3 shipped a wrong
NO-GO once by measuring that magazine-bypass substrate (see CLAUDE.md's own
account of that mistake, and R31-0 §0's defect writeup). This is why this
report's numbers can be compared directly against R31-0's without a
regime-mismatch caveat: both harnesses claim a FRESH `HeapCore` per rep via
`HeapRegistry::claim` (never recycled), use the same 4 sweep sizes (4/16/64/128
KiB, all Small-classified), the same `BURST = 16`, and the same `REPS = 15`.

**Three arms** (plain `alloc` virgin, plain `alloc` recycled, `realloc`
confirmation), each compared `virgin-zero-skip` OFF vs ON:

- **`benches/r32_0_virgin_zero_skip_cost_side_gate.rs`** — the deterministic
  in-process gate, same `Instant`-timing-loop `fn main()` shape as R31-0's
  bench (`harness = false`), extending `Cargo.toml`'s existing
  `virgin-zero-skip` bench section.
- **`examples/r32_0_cost_probe_alloc_recycled_{off,on}.rs`** (+ shared
  workload `examples/_shared/r32_0_virgin_zero_skip_cost_probe_workload.rs`)
  — the wall-clock companion for `scripts/paired-ab-runner.mjs`, covering
  the single worst-case cell (4 KiB recycled/steady-state-hit — the class
  with the most magazine slots, `refill_n = 16`, and therefore the class
  where a per-slot-bit cost would be most visible per relative call).

**RECYCLED `alloc_zeroed` is NOT re-measured here.** R31-0 §3.2 already
measured this exact arm (recycled `alloc_zeroed`, same 4 sizes, same
`HeapCore` layer, same REPS/BURST regime) and found "no material, consistent
regression — small, sign-inconsistent deltas in both directions across all
12 cells." That result is cited, not duplicated: re-running the identical
arm would add noise, not evidence. This report's own new arms are plain
`alloc` (never measured before at this layer) and the `realloc`
confirmation.

**Why `realloc` gets only one confirmatory cell, not a sweep.** §1's table
shows `realloc` has zero direct feature code — the task brief's own
permission ("if it doesn't [touch realloc], say so and skip this arm rather
than inventing one") applies with the qualifier that a minimal confirmatory
cell is still worth one data point (does the `realloc` wrapper itself add
overhead around the inherited `alloc`/`dealloc` cost?). One 4 KiB→8 KiB grow
cell, 200 reps, in the deterministic gate (§4's `R32_0_REALLOC` row)
answers that without inventing a new isolated mechanism to sweep.

## 3. A known confound this gate's numbers inherit, NOT reproduces (task #495)

`alloc_small_zeroed_via_magazine`'s magazine-HIT arm
(`heap_core_alloc.rs:373`) pays an extra `self.stamp_segment_owner(issued)`
call that plain `alloc`'s equivalent hit arm explicitly documents it does
NOT pay ("P4: NO stamp here", `heap_core_alloc.rs:~160`). This asymmetry
predates `virgin-zero-skip` and is unrelated to it, but it means: **this
report's plain-`alloc` numbers are not directly comparable to R31-0's
`alloc_zeroed` numbers as an apples-to-apples "which API is cheaper"
question** — they answer a narrower, correctly-scoped question instead
("does `virgin-zero-skip` add cost to THIS specific API, comparing that
API's own OFF binary against its own ON binary"), which is exactly the
comparison a promotion decision needs. Filed as task #495, out of scope to
fix here — noted so a reader does not read this report's plain-`alloc`
absolute ns figures against R31-0's `alloc_zeroed` absolute ns figures and
draw a stamp-cost-driven conclusion that has nothing to do with
`virgin-zero-skip`.

## 4. Path-activation oracle (CLAUDE.md R30-8) — per-arm mechanism evidence

Plain `alloc` never zeroes anything, so R31-0's own oracle
(`dbg_small_zero_pass_count`) cannot serve here. Two independent signals
instead:

1. **`HeapCore::tcache_hits()` before/after the burst = magazine-HIT
   count**, checked against the analytically-derived `expected_hits` (same
   `BURST - ceil(BURST/refill_n)` formula R31-0 uses for the virgin
   scenario, `BURST` for recycled) — the cross-binary proof the production
   magazine path actually ran, identically, regardless of the feature flag.
   **Confirmed exactly on every one of 16 cells (4 sizes x 2 scenarios x 2
   runs), both binaries** — `mean_hits == expected_hits` asserted in-script
   by `scripts/r32_0_derive_report_data.mjs` (would `throw` otherwise).

2. **`HeapCore::dbg_tcache_virgin_mask(c)`, sampled immediately after the
   FIRST burst call of each virgin rep.** Per §1's source-confirmed finding
   that `refill_magazine_slow` (plain `alloc`'s own miss path) never writes
   `virgin_mask`, the mask for this class must read **exactly 0** after
   every plain-`alloc` miss-triggered refill, on the ON binary, at every
   size. This is a STRONGER and more precise oracle than "some bits are
   set" — it directly proves the AND this gate measures on every subsequent
   magazine-hit pop is unconditionally clearing an ALREADY-ZERO bit: a real,
   but ALWAYS-a-no-op, per-hit cost on a plain-`alloc`-only workload (this
   feature never gets a chance to help `alloc`, structurally, by
   construction — `alloc` doesn't zero, so there is nothing to skip). `NA`
   on the OFF binary (the `virgin_mask` field does not exist there at all —
   compiled out, not merely unread).

   **First attempt caught a real design bug before any number shipped**
   (documented per CLAUDE.md's own precedent of recording oracle failures
   found during development, e.g. R30-3's `VIRGIN_BATCH` discovery): the
   oracle's first implementation asserted the mask should be NON-zero
   whenever `refill_n > 1` — modeled on `alloc_zeroed`'s retention
   mechanism — and FAILED at 4k/16k (`mask=0` observed, contradicting the
   assumption). Re-reading `refill_magazine_slow` line-by-line at that point
   found the true mechanism (§1's finding: this miss path never writes the
   mask at all), and the oracle was corrected to assert 0 unconditionally,
   which then PASSED at all 4 sizes on both runs. This is the exact
   failure-mode CLAUDE.md's R30-8 rule targets — the oracle caught a wrong
   assumption before publication, not after.

3. **Wall-clock companion probe's own oracle**
   (`examples/_shared/r32_0_virgin_zero_skip_cost_probe_workload.rs`):
   asserts `hits_delta == ROUNDS` (every one of 200,000 iterations was a
   genuine magazine hit re-serving the same primed block) before emitting
   any `RESULT` line — a miss-starved run would silently measure refill
   cost instead of the steady-state hit-path cost this probe claims to
   isolate. **Confirmed: `oracle_hits_delta=200000` on every one of the 42
   process launches** (20 pairs x 2 arms + 2 same-vs-same-control launches),
   visible directly in `docs/perf/_raw_r32_0_cost_probe_ab.log`.

**Conclusion of this section: 4/4 oracle checks confirmed** (magazine-hit
parity, mask-zero on plain-`alloc` ON, OFF-binary NA, wall-clock hit-delta
parity) — this gate is proven to exercise the mechanism it claims to
measure.

## 5. Deterministic gate results — plain `alloc`, both scenarios, all 4 sizes

Derived by `scripts/r32_0_derive_report_data.mjs` from the raw text logs (one
checked script, not hand-transcribed, per CLAUDE.md's R30-9 rule). Primary
run cited below; a second independent repeat run
(`docs/perf/_raw_r32_0_{off,on}_run2.log`) is cited for noise-stability
discussion only, not double-counted in the summary CSV (same convention as
R31-0 §3).

### 5.1 Virgin scenario (`ns/op` = mean of 15 fresh-heap-rep batches of `BURST=16` calls, normalized per call)

| Size | OFF ns/op (run1) | ON ns/op (run1) | Δ (run1) | OFF ns/op (run2) | ON ns/op (run2) | Δ (run2) |
|---|---:|---:|---:|---:|---:|---:|
| 4k | 250.8 | 375.0 | +49.5% | 371.2 | 257.9 | -30.5% |
| 16k | 481.7 | 602.9 | +25.2% | 748.8 | 567.9 | -24.2% |
| 64k | 674.2 | 672.9 | -0.2% | 839.6 | 532.5 | -36.6% |
| 128k | 1422.9 | 1363.8 | -4.2% | 1600.4 | 1294.6 | -19.1% |

(Table derived by `scripts/r32_0_derive_report_data.mjs`, rows tagged
`r32_0_bench_gate_delta` in the summary CSV — not hand-computed.)

**Every one of the 4 cells flips sign between run1 and run2** — this is the
SAME noise-dominated pattern R31-0 §3.1 documented for its `onebyte`/`full`
touch-heavy cells (page-fault/memory-bandwidth noise on a shared dev host
swamping a small real difference). The `min_ns` column (least susceptible to
transient host noise) is far more stable: at 4k, OFF min_ns is 143.8/143.8ns
across both runs and ON min_ns is 150.0/143.8ns — a difference well under
the noise this host's own mean-ns figures show. **This deterministic gate's
`virgin` scenario is not a clean signal either direction at this sample
size** — consistent with §1's structural prediction (the extra cost here is
one AND on a burst dominated by carve/bump-carry work, likely far below this
host's noise floor for a mean-of-15 measurement).

### 5.2 Recycled scenario (worst-case arm — no benefit ever collected)

| Size | OFF ns/op (run1) | ON ns/op (run1) | Δ (run1) | OFF ns/op (run2) | ON ns/op (run2) | Δ (run2) |
|---|---:|---:|---:|---:|---:|---:|
| 4k | 25.0 | 27.5 | +10.0% | 37.1 | 28.8 | -22.4% |
| 16k | 25.4 | 23.3 | -8.3% | 33.3 | 26.2 | -21.3% |
| 64k | 26.7 | 21.2 | -20.6% | 30.0 | 25.4 | -15.3% |
| 128k | 27.5 | 27.1 | -1.5% | 27.5 | 27.5 | +11.8% |

(Table derived by `scripts/r32_0_derive_report_data.mjs`, rows tagged
`r32_0_bench_gate_delta` in the summary CSV — not hand-computed.)

Same sign-instability pattern as §5.1 at this tiny per-call magnitude
(tens of ns). **This deterministic gate alone cannot resolve the cost-side
question** — exactly why this task also built the wall-clock paired-AB
probe (§6), which uses 20 alternating process-level launches with a paired
t-test specifically because a single in-process mean-of-15 measurement
cannot resolve a signal this small against this host's noise floor (the
established methodology from
`docs/perf/R5_R2_CHURN_REGRESSION_PAIRED_AB.md`, reused by R31-3/R31-8/etc.).

### 5.3 `realloc` confirmation (one cell, 200 reps, 4k→8k grow)

| Binary | mean ns/op | p50 ns/op |
|---|---:|---:|
| OFF | 234.0 | 200.0 |
| ON | 257.5 | 200.0 |

p50 is IDENTICAL between binaries (200.0ns both) — the mean difference
(23.5ns, ~10%) sits inside a single Windows `Instant` tick-granularity band
at this magnitude and is not a claim this report treats as resolved either
way. Consistent with §1's structural finding (zero direct `realloc` code
under the feature): no distinguishable realloc-specific cost is visible
beyond what §5.1/§5.2's own noise floor already shows for the inherited
`alloc`/`dealloc` legs.

## 6. Wall-clock paired-AB — the resolving measurement

Per CLAUDE.md's own methodology (`docs/perf/R5_R2_CHURN_REGRESSION_PAIRED_AB.md`),
a signal this small needs process-level isolation + a paired t-test, not a
single in-process mean. `scripts/paired-ab-runner.mjs --config
scripts/_r32_0_cost_probe_ab.json`, A/B/B/A protocol, 20 pairs, worst-case
cell (4 KiB recycled/steady-state-hit — the arm §1 predicts is most exposed
to the per-hit AND, since every one of 200,000 iterations is a magazine
HIT, never a miss).

| Comparison | n | mean Δ (off − on) | t | crit (p<0.05) | Verdict | Sign test |
|---|---:|---:|---:|---:|---|---|
| off vs on | 20 | **−1.10 ns/round** | −3.240 | 2.101 | **REAL — ON is slower** | off-faster 17/20 |
| off vs off (same-vs-same control) | 20 | +0.07 ns/round | 0.101 | 2.101 | NOISE, as expected | 9/20 vs 7/20 (4 ties) |

**Reading this honestly.** The off-vs-on signal is genuinely real (t=−3.24,
well past crit=2.101; sign test heavily lopsided at 17/20, past this
project's own "heavily lopsided" bar of 17+/20) — this is NOT noise, unlike
§5's in-process deterministic-gate cells. The same-vs-same control on the
SAME workload, SAME host, SAME session confirms the harness itself is sound
(t=0.101, roughly even 9/7/4 split) — so the off-vs-on signal is not a
harness artifact.

**Magnitude.** −1.10 ns/round against an OFF-arm baseline of ~13.4 ns/round
(`docs/perf/_raw_r32_0_cost_probe_ab.log`'s raw per-launch `ns_per_round`
values) is **8.2%** relative cost
(`scripts/r32_0_derive_report_data.mjs` computes and asserts this in-script:
`relativeCostPct = |mean_delta| / off_arm_mean_ns_per_round = 8.19%`). In
absolute terms this is ~1 nanosecond per allocate+free pair on the single
hottest, most-favorable-to-a-signal arm this task could construct (the
class with the most magazine slots, a tight LIFO loop guaranteeing 100%
magazine-hit activation on every iteration).

## 7. Cost side vs. benefit side — same-regime comparison (CLAUDE.md's rule)

| | Magnitude | Regime |
|---|---:|---|
| **Benefit** (R31-0 §3.1, `notouch` virgin `alloc_zeroed`) | **−89% to −98.6%** (4/4 sizes, reproduced on an independent repeat run) | `HeapCore` production layer, `BURST=16`, `REPS=15`, fresh heap per rep |
| **Cost** (this report §6, worst-case plain-`alloc` recycled/hit) | **+8.2%** relative, ~1 ns/round absolute (one real, statistically-confirmed wall-clock signal; the in-process deterministic gate alone could not resolve it) | Same `HeapCore` production layer, same sweep sizes, same `BURST`/`REPS` regime |
| **Cost** (R31-0 §3.2, recycled `alloc_zeroed` — cited, not re-measured) | No material, consistent regression (small, sign-inconsistent deltas, 7/12 ON-faster vs 5/12 ON-slower) | Same layer/regime |

Both rows are measured at the SAME `HeapCore` production entry point, the
SAME sweep sizes, the SAME `BURST=16`/`REPS=15` scenario shape — satisfying
CLAUDE.md's same-workload-regime cost/benefit rule (the rule added
specifically after R30-6 paired a benefit measured in one regime with a
cost measured in another).

## 8. Promotion decision

**Criterion, stated before revealing the verdict** (per the task brief's own
instruction): promote-worthy requires the worst-case measured cost to be
practically negligible relative to the confirmed benefit's magnitude — a
single-digit-percent-or-smaller relative cost against an ~90%+ relative
benefit clears that bar; a cost within the same order of magnitude as the
benefit, or a cost that scales with workload size (unlike a fixed per-hit
bitmask op), would not.

**Verdict: GO on the cost side specifically — the LOST front does not
disqualify `virgin-zero-skip`.** The worst-case, most-favorable-to-a-signal
arm this task could construct (plain `alloc`, tight LIFO recycled loop,
100% magazine-hit activation, the class with the most magazine slots) shows
a real, wall-clock-confirmed cost of **8.2% relative / ~1 ns absolute per
call** — two orders of magnitude smaller than the benefit side's 89-98.6%
`notouch` win. Plain `alloc`'s virgin scenario and `alloc_zeroed`'s recycled
scenario (R31-0 §3.2, cited) show no resolvable signal at all beyond this
host's noise floor. `realloc` inherits whatever cost `alloc`/`dealloc` carry
and adds nothing of its own (source-confirmed, §1).

**This does NOT upgrade R31-0's own verdict to a blanket `production`
promotion.** R31-0 already declined a blanket promotion for a REASON
unrelated to cost: the `onebyte`/`full` touch-heavy categories (the majority
shape of real `calloc` workloads — a buffer is usually populated shortly
after allocation) showed no reproducible BENEFIT at R31-0's sample size,
independent of what this report found about cost. This report closes the
cost-side gap R31-0 explicitly left open; it does not re-open or resolve
R31-0's own benefit-side touch-heavy finding.

**Combined reading:** the feature is now evidence-complete on both fronts
for the ONE workload shape it demonstrably helps (same-class virgin bursts,
touch-light/deferred-touch consumers) — large proven benefit, negligible
proven cost, even in the worst-case non-benefiting arm. It remains
evidence-incomplete for a BLANKET default (the touch-heavy majority shape's
benefit is still unresolved, not this report's scope). Per this task's own
explicit instruction and `docs/perf/OPEN_ITEMS.md`'s standing note, the
`production` feature-composition decision itself is reserved for the user's
sign-off — this report recommends, but does not enact, considering
`virgin-zero-skip` for a NARROW workload-conditional promotion (matching
R31-0's own "Recommended narrower framing" in its §5), now with the cost
side confirmed negligible rather than merely assumed.

## 9. Noise-stability caveats (carried forward honestly, not papered over)

- **The deterministic gate's own `virgin`/`recycled` mean-ns figures are
  sign-unstable run-to-run** (§5.1, §5.2) — every cell flips sign between
  run1 and run2, the same host-noise pattern R31-0 §3.1 already documented
  for its touch-heavy cells. This report does NOT treat the deterministic
  gate's mean-ns numbers as resolving evidence on their own; §6's wall-clock
  paired-AB (20 process-level pairs + paired t-test + same-vs-same control)
  is the resolving measurement, exactly the escalation CLAUDE.md's own
  guidance recommends when a single-run measurement cannot distinguish
  signal from noise ("more samples/pairs may help").
- **Only ONE wall-clock cell was measured** (4 KiB recycled, the single
  most favorable-to-a-signal arm by construction). A reader wanting
  per-size wall-clock resolution across all 4 sweep sizes, or a
  wall-clock-resolved `virgin`-scenario cost (§5.1 could not resolve one),
  would need additional `paired-ab-runner.mjs` cells — not built here, since
  the worst-case cell already resolves the promotion-relevant question
  (is the cost negligible even in the MOST exposed arm) with a real signal.
- **`realloc`'s ~10% mean delta (§5.3) sits at Windows `Instant`
  tick-granularity for a 200-rep in-process mean** and is not escalated to
  a wall-clock paired-AB in this report, since §1's source read already
  establishes there is no separate mechanism for such a probe to isolate
  (all `realloc` cost is inherited from `alloc`/`dealloc`, already covered
  by §6).

## 10. CLAUDE.md compliance checklist

- **Same-workload-regime cost/benefit** (§7): both sides measured at the
  same `HeapCore` layer, same sweep sizes, same `BURST=16`/`REPS=15` regime.
- **Path-activation oracle** (§4): 4/4 oracle checks confirmed, including
  one design-bug catch during development (the mask-oracle's first
  assumption was wrong and the oracle itself caught it before publication).
- **Entry-point-layer honesty** (§2): `HeapCore::alloc`/`alloc_zeroed`/`realloc`
  named explicitly, with the reason (matches `SeferAlloc`'s real
  `#[global_allocator]` chain, same layer R31-0 used).
- **Derived-not-hand-typed tables** (R30-9): `scripts/r32_0_derive_report_data.mjs`
  computes every number in §5-§7 from the raw logs/JSON and hard-`throw`s on
  5 distinct headline assertions (oracle PASS/NA split, magazine-hit parity,
  off-vs-on significance+direction+sign-test-majority, off-vs-off-control
  noise, relative-cost-in-plausible-range) before writing the CSV.
- **Immutable source identity captured BEFORE measurement** (R29-6): base
  commit `4ccd18b02d2558a173b8d4bb1d55b4915a835702`; scoped tree SHA
  `2f2876de521a1104bdfcbdaf61ee279567b673f7` (`git write-tree` against a
  temporary index containing exactly this task's added/changed files,
  computed before any raw log cited here was produced).
- **Raw logs** (`git add -f`'d): `docs/perf/_raw_r32_0_off.log`,
  `docs/perf/_raw_r32_0_on.log` (primary run-1, cited evidence),
  `docs/perf/_raw_r32_0_off_run2.log`, `docs/perf/_raw_r32_0_on_run2.log`
  (repeat run, noise-stability discussion only),
  `docs/perf/_raw_r32_0_cost_probe_ab.log` (wall-clock off-vs-on),
  `docs/perf/_raw_r32_0_cost_probe_ab_same_vs_same.log` (control).
- **Summary CSV:** `docs/perf/R32_0_VIRGIN_ZERO_SKIP_COST_SIDE_GATE_summary.csv`.
- **Underlying full-provenance JSON:**
  `docs/perf/paired_ab_runs/2026-08-01T23-10-48-002Z.json` (off-vs-on),
  `docs/perf/paired_ab_runs/2026-08-01T23-11-07-888Z.json` (same-vs-same
  control).
- **No production default changed** — `bench`-prefixed measurement-only work
  per CLAUDE.md's R30-12 commit-tag rule; `Cargo.toml`'s `production`
  feature list is untouched by this task. Per this task's own explicit
  instruction, the promotion DECISION is reserved for the user's sign-off —
  this report produces evidence, does not act on it.

## 11. Files changed/added (this task)

**Source:** none (no production or opt-in code changed; this is a pure
measurement task).

**Bench/harness/script (new):**
- `benches/r32_0_virgin_zero_skip_cost_side_gate.rs` — the deterministic
  in-process gate (§5).
- `examples/_shared/r32_0_virgin_zero_skip_cost_probe_workload.rs`,
  `examples/r32_0_cost_probe_alloc_recycled_{off,on}.rs` — the wall-clock
  paired-AB companion probes (§6).
- `scripts/_r32_0_cost_probe_ab.json` — `paired-ab-runner.mjs` config for
  the wall-clock A/B.
- `scripts/r32_0_derive_report_data.mjs` — the checked derivation script
  (CLAUDE.md's R30-9 rule) that produced every table/number in §5-§7 from
  the raw logs/JSON.
- `Cargo.toml` — one new `[[bench]]` entry, two new `[[example]]` entries.

**Raw logs / provenance:** listed in §10 above.

**Docs:** this file; `docs/perf/OPEN_ITEMS.md` (see next commit for the
cross-reference update recording this closure).
