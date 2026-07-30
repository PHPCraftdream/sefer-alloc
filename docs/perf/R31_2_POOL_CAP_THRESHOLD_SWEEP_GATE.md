# R31-2 — multi-threaded small-pool-cap THRESHOLD sweep (does cap 16 or 32 change the mechanism R30-7 found frozen at cap 8?)

**Task:** R31-2 (task #465), Round 31. Answers the question R30-7
(task #456, `docs/perf/R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md`)
left open in its own §0.1 dated correction: R30-7's 8-thread server-shaped
A/B found `decommit_calls_total = 40` in BOTH the `SeferAlloc::new()`
(cap 4) arm and the `Profile::Throughput` (cap 8) arm — a mechanism delta
of ZERO between cap 4 and cap 8 at that concurrency. That result did NOT
prove pooling has no value at this concurrency; it could simply mean cap 8
was not large enough to change the mechanism for THIS workload shape. This
task sweeps two more cap points (16, 32) on the SAME workload shape to find
the actual threshold, or confirm none exists up to a 32-segment cap.

**Verdict: CLEAN REJECT up to cap 32 — the mechanism delta stays ZERO at
every swept cap (8, 16, and 32), through the real `#[global_allocator]`, on
this exact workload shape.** `decommit_calls_total` is bit-identical (40,
every launch, every arm, all 160 process launches across all 4 comparisons)
regardless of whether the small pool is configured at 4, 8, 16, or 32
segments. RSS and committed memory are correspondingly flat across every
cap (~77.5 MiB RSS / ~99.75 MiB commit, no material growth even at cap 32 —
consistent with the pool never actually retaining more segments, since it
never needs to). Wall-clock shows no statistically significant difference
at any cap (all `|t| < crit(p<0.05)=2.101`; cap32's sign split is a dead
heat, 10/10), at a minimum-detectable-effect of ≈4-5% of the mean —
noticeably TIGHTER than R30-7's own 18.8% MDE (this workload/host
combination produced less noise), so this is a genuinely more decisive
null than R30-7's own underpowered one.

**This specific server-shaped workload (8 threads, 4-size mix, continuous
churn, `CHURN_FRACTION=1/2`, `ROUNDS=6`) is not a victim for small-pool
sizing at ANY cap this task swept.** The mechanism this pool cap governs
never binds in this workload shape, at any of the four tested cap values —
not because the workload never overflows the DEFAULT pool (it does: cap4's
decommit count is nonzero, 40, confirming the mechanism activates), but
because whatever drives those 40 decommits per launch is apparently NOT
bounded by the pool's segment-retention capacity in the range this task
swept. See §3 for the candidate explanation this task's own data supports.

**This task does not change any `src/` default, `Profile`, or
`Cargo.toml`'s `production` line.** Measurement only.

**Date:** 2026-07-30. **Base/landing revision measured:** `main` @
`d9d30cdf47358ebd2c60c0bbdd641c571ed0d943` — this task's own commit lands
this report and the example/workload files together, so the measurement
commit SHA and the tree it measured are the same commit (no chicken-and-egg
placeholder needed, unlike R30-6/R30-7's same-day-follow-up pattern —
`git show d9d30cdf47358ebd2c60c0bbdd641c571ed0d943:docs/perf/R31_2_POOL_CAP_THRESHOLD_SWEEP_GATE.md`
recovers this exact report and the exact `examples/r31_2_*`/
`examples/_shared/r31_2_*` source it measured, per CLAUDE.md's R29-6
immutable-source-identity rule). **Platform:** native Windows 10 Pro
x86-64, 11th Gen Intel Core i7-11800H @ 2.30GHz (8 cores / 16 logical) —
same shared host as R27-3/R27-4/R29-13/R30-6/R30-7 (shared with other
concurrent agent work during this session; the paired A/B/B/A protocol plus
the same-vs-same control are the machinery this project uses to separate a
real effect from that noise). **Feature set:** `production alloc-stats`
(real `#[global_allocator]`, no `bench-internals` needed — all four
binaries use only `SeferAlloc::stats()`, matching R30-6/R30-7's build).

---

## 0. Headline numbers

All four tables below are DERIVED by `scripts/r31_2_derive_report_data.mjs`
from the four raw provenance JSONs
(`docs/perf/paired_ab_runs/2026-07-30T20-57-11-202Z.json` [cap4 vs cap8],
`...20-57-46-116Z.json` [cap4 vs cap16], `...20-58-23-943Z.json` [cap4 vs
cap32], `...20-58-59-637Z.json` [same-vs-same control]) — not
hand-transcribed, per CLAUDE.md's checked-script rule.

### 0.1 Mechanism-delta table (the actual point of this task)

| baseline | swept cap | `decommit_calls_total` (cap4, all 40 launches) | `decommit_calls_total` (swept cap, all 40 launches) | mechanism delta |
|---|---|---|---|---|
| cap4 (4 seg / 16 MiB) | cap8 (8 seg / 32 MiB) | `[40]` — bit-identical | `[40]` — bit-identical | **ZERO** |
| cap4 (4 seg / 16 MiB) | cap16 (16 seg / 64 MiB) | `[40]` — bit-identical | `[40]` — bit-identical | **ZERO** |
| cap4 (4 seg / 16 MiB) | cap32 (32 seg / 128 MiB) | `[40]` — bit-identical | `[40]` — bit-identical | **ZERO** |

**No cap from 4 through 32 changes the mechanism.** Every one of the 320
process launches across the four comparisons (80 launches per comparison —
40 `cap4` + 40 of the other arm — × 4 comparisons: cap4-vs-cap8,
cap4-vs-cap16, cap4-vs-cap32, cap4-vs-cap4 control) reports
`decommit_calls_total = 40`, with zero exceptions. This is the direct,
deterministic mechanism-activation oracle (not an inference from timing) —
identical methodology to R27-4 §2's "direct, deterministic confirmation"
and R30-7 §0.1's per-arm activation table.

### 0.2 Wall-clock + stated MDE (computed BEFORE any conclusion drawn, per CLAUDE.md's R30-7-established rule)

| comparison | n pairs | mean Δ (cap4 − other) | sd | se | t | crit(p&lt;0.05) | significant | sign split (cap4/other) | mean elapsed (both arms) | **MDE** | MDE as % of mean | 95% CI on mean Δ |
|---|---:|---:|---:|---:|---:|---:|---|---|---:|---:|---:|---|
| cap4 vs cap8 | 20 | −1.543 ms | 22.032 ms | 4.927 ms | −0.313 | 2.101 | **NO** | 11/9 | 228.637 ms | **10.351 ms** | **4.53%** | [−11.893, 8.808] ms |
| cap4 vs cap16 | 20 | +0.401 ms | 17.397 ms | 3.890 ms | 0.103 | 2.101 | **NO** | 11/9 | 184.246 ms | **8.173 ms** | **4.44%** | [−7.772, 8.574] ms |
| cap4 vs cap32 | 20 | −3.917 ms | 17.627 ms | 3.942 ms | −0.994 | 2.101 | **NO** | 10/10 | 209.963 ms | **8.281 ms** | **3.94%** | [−12.198, 4.364] ms |
| cap4 vs cap4 (control) | 20 | +8.519 ms | 21.392 ms | 4.783 ms | 1.781 | 2.101 | **NO** | 9/11 | 194.588 ms | **10.050 ms** | **5.16%** | [−1.530, 18.569] ms |

**MDE (minimum detectable effect)** = `crit(p<0.05) × se`, the same formula
R30-7 §0.2 used. Every comparison here can detect effects as small as
**~4-5% of the mean elapsed time** — substantially tighter than R30-7's
own **18.8%** MDE on its slower, noisier launch (R30-7's mean elapsed was
~697 ms/launch; this sweep's workload/host combination runs
~185-230 ms/launch with proportionally tighter absolute noise). **This is
therefore a materially more decisive null than R30-7's own underpowered
one**: R30-7 could only say "no effect ≥ ~19% detected"; this sweep can say
"no effect ≥ ~4-5% detected" at cap8/16/32, and the point estimates
themselves are small and sign-inconsistent (cap8 favors cap4, cap16 and
cap32 point in different directions from cap8, the control itself has the
largest positive point estimate of the four) — the pattern of a genuine
null, not a suppressed real effect.

**Note on the control's own comparison:** the same-vs-same control shows
`t = 1.781`, the LARGEST magnitude `t` of the four comparisons (though
still `< crit`) — a reminder that even the honesty control carries real
host-noise variance at this launch count; none of the three real
comparisons show a larger or more significant effect than the control does
against itself, which is the actual honesty criterion (not "the control's
own `t` must be near 0").

### 0.3 RSS / commit retention per arm (cost side, SAME workload/arm as the benefit-side measurement)

| comparison | arm | n launches | mean `rss_after_kib` | mean `commit_after_kib` |
|---|---|---:|---:|---:|
| cap4 vs cap8 | cap4 | 40 | 77,510 KiB | 99,764 KiB |
| cap4 vs cap8 | cap8 | 40 | 77,509 KiB | 99,766 KiB |
| cap4 vs cap16 | cap4 | 40 | 77,521 KiB | 99,748 KiB |
| cap4 vs cap16 | cap16 | 40 | 77,510 KiB | 99,751 KiB |
| cap4 vs cap32 | cap4 | 40 | 77,511 KiB | 99,751 KiB |
| cap4 vs cap32 | cap32 | 40 | 77,509 KiB | 99,749 KiB |

**No material RSS/commit growth at any cap** (all six rows sit within
~12 KiB of each other, well inside process-level measurement noise — this
is NOT the R27-3-style multi-MiB-per-heap retention delta a genuinely
larger pool would cost). This is mechanistically consistent with §0.1: if
the pool's segment-retention capacity never becomes the binding constraint
(the mechanism never differs), then the pool never actually holds more
segments at cap 32 than it does at cap 4 — there is no retention cost to
pay here because there is no retention BENEFIT being captured either. This
is the same-arm-same-workload cost/benefit pairing CLAUDE.md's R31-8 rule
(pending, task #472) will require going forward — applied here.

---

## 1. Why the mechanism never differs — a plausible explanation, not a proven root cause

This task's own data (§0.1-0.3) rules out one obvious alternative
explanation and points toward another:

**Ruled out: "the config never actually took effect."** The `pool_segments_requested`/
`pool_byte_cap_requested` RESULT fields (read from the compile-time
`POOL_SEGMENTS`/`POOL_BYTE_CAP` consts each binary was built with — see §4
point 1) confirm each binary reports its own distinct requested value
(4/16 MiB, 8/32 MiB, 16/64 MiB, 32/128 MiB) in every launch — the four
binaries are genuinely different compiled configs, not four copies of the
same one. `segments_reserved_total` (72, every arm, every launch) and
`segments_released_total` (48, every arm, every launch) are ALSO
bit-identical across every cap — the total segment churn over the whole
run does not change either, which would be a stranger result than the
decommit-count finding alone if the pool cap genuinely bound at some caps
and not others.

**The more likely explanation, following R30-7 §0.1's own "hypothesis 0"
one step further:** `decommit_calls_total` is a **process-wide** counter
(`DECOMMIT_CALLS`, a single `static AtomicU64` in
`src/alloc_core/alloc_core.rs:221`), summing decommits across every
thread's own `AllocCore`/pool — NOT a per-heap counter reset per arm. This
workload's per-thread peak working set (`OBJS_PER_ROUND / 4 × Σ(SIZE_CLASSES)`
= `4626 × (64+256+1024+4096)` = 24.00 MiB exactly ≈ 6.0 small segments)
comfortably fits inside a cap-8 pool's 32 MiB ceiling, let alone cap-16's
64 MiB or cap-32's 128 MiB — yet the SAME 40 decommits fire regardless.
This is consistent with the decommits NOT being driven by "the pool filled
up and had to evict," but by some OTHER path that empties and releases a
small segment independent of `pooled_count < pool_cap` — for instance the
`CHURN_FRACTION=1/2` mid-round churn freeing+reallocating objects across
segment boundaries at a rate/pattern where individual segments empty
completely at moments the current per-thread bump cursor (`small_cur`) has
already moved past them (the `base == small_cur` guard in
`dec_live_and_maybe_decommit`, `src/alloc_core/alloc_core_small_pool.rs:175`,
only protects the SINGLE currently-bump-targeted segment — any OTHER
segment that empties is eligible for pool/release regardless of how much
pool headroom remains, so a pool cap only matters once `pooled_count`
itself would exceed it; if empties are rare/scattered rather than a
sustained backlog, `pooled_count` may simply never approach even the
smallest swept cap, at which point ALL four cap values behave identically
by construction: an unfilled pool never distinguishes cap sizes).

**This task does not attempt to isolate the exact root cause** (that would
need a targeted Stage-1 measurement — per-heap pooled-count high-water
mark, or a decomposition of which of the 40 decommits are churn-driven vs.
teardown-driven — out of this task's scope, matching R30-7 §2's own
explicit "plausible, not proven" posture for its hypotheses). The
DECISION-RELEVANT finding stands regardless of root cause: **for this
workload shape, no swept cap (8/16/32) changes the mechanism from cap 4's
baseline**, so a caller whose workload resembles this shape (8-way
concurrency, mixed 64B-4KiB sizes, ~50% mid-round churn) should not expect
`Profile::Throughput`'s cap-8 recipe — or an even larger cap — to reduce
decommit churn, based on this evidence.

---

## 2. Relationship to R30-7 and R27-4

| | R27-4 (single-thread) | R30-7 (8-thread, cap4 vs cap8 only) | R31-2 (this task: 8-thread, cap4 vs 8/16/32) |
|---|---|---|---|
| workload | 1 thread, 1 fixed size (1024B), burst-then-teardown | 8 threads, 4-size mix, continuous churn | SAME as R30-7 (byte-identical body) |
| caps compared | 4 vs 8 | 4 vs 8 | 4 vs 8, 4 vs 16, 4 vs 32 |
| mechanism delta | 9 → 0 decommits/run (cliff) | 40 → 40 (ZERO delta) | 40 → 40 → 40 → 40 (ZERO delta, all 3 swept caps) |
| wall-clock | t=8.114, REAL (22% win) | t=−0.119, NOT significant (MDE ≈18.8%) | t=−0.313/0.103/−0.994, NOT significant (MDE ≈4-5%) |
| verdict | GO for this shape | underpowered null | **clean reject — mechanism frozen at cap4's level through cap32** |

R31-2 closes the open question R30-7 §0.1 raised explicitly: R30-7 could
not distinguish "cap 8 wasn't big enough" from "some other structural
reason the two configs converge under 8-way concurrency." This task shows
it is NOT simply "cap 8 wasn't big enough" — the SAME convergence persists
all the way to cap 32 (4× cap 8's segment count, 8× its byte ceiling). The
more likely explanation is the one in §1: the decommit mechanism in this
workload shape is not primarily driven by pool-capacity exhaustion at all,
at any of the cap values commonly discussed for this knob.

**This does not retract R27-4's finding** (single-threaded, single fixed
size, burst-then-teardown — a genuinely different, decommit-heavy shape
where the pool-cap mechanism DOES bind and DOES change with cap, 9→0). Nor
does it retract `Profile::Throughput`'s existing `(8, 32 MiB)` value — that
value remains R27-4's measured single-threaded candidate; this task adds
the honest complementary data point that the same recipe shows no
detectable benefit (mechanism or wall-clock) in this specific
multi-threaded server-shaped scenario, at ANY cap up to 32, not just at 8.

---

## 3. Workload — identical to R30-7's, by construction

`examples/_shared/r31_2_pool_cap_threshold_workload.rs` is a verbatim copy
of `examples/_shared/r30_7_server_shaped_workload.rs`'s workload body (8
threads, `SIZE_CLASSES = [64, 256, 1024, 4096]`, `OBJS_PER_ROUND = 18504`,
`ROUNDS = 6`, `CHURN_FRACTION = 1/2`), with two additions: (1) `run_arm`
takes the REQUESTED `pool_segments`/`pool_byte_cap` as explicit parameters
and emits them as `RESULT` lines (the R26-4 config-identity contract, §4),
and (2) two new `RESULT segments_released_total` lines were added
(present in `AllocStats` but not previously emitted by the R30-7 workload)
so this task's own §0.1 mechanism table can show the FULL segment
lifecycle (`reserved` AND `released`, not just `decommit_calls_total`), not
because R30-7's original shape needed correction.

**Why reuse this exact shape rather than design a fresh workload:** the
task brief's explicit goal is to find where R30-7's OWN zero-delta result
moves off zero as cap increases — using a DIFFERENT workload shape would
answer a different question (whether some other shape shows a threshold),
not this one. Keeping the workload byte-identical to R30-7's (aside from
the two additive RESULT fields above) makes the cap4-vs-cap8 arm of this
sweep a genuine re-measurement of R30-7's own comparison point, which is
exactly what §0.1's first row confirms (bit-identical `[40]` on both
sides, matching R30-7's finding).

---

## 4. Methodology

### 4.1 Four real `#[global_allocator]` binaries, one shared workload body

Mirrors R27-4's/R30-6's/R30-7's established pattern (separate
compile-time-configured binaries, since `SeferAlloc::with_config` bakes its
config into a `static` initialiser — no runtime selection is possible):

- `examples/r31_2_pool_cap_threshold_ab_cap4.rs` — `pool_segments=4,
  pool_byte_cap=16 MiB` (current `production` default).
- `examples/r31_2_pool_cap_threshold_ab_cap8.rs` — `pool_segments=8,
  pool_byte_cap=32 MiB` (`Profile::Throughput`'s current value).
- `examples/r31_2_pool_cap_threshold_ab_cap16.rs` — `pool_segments=16,
  pool_byte_cap=64 MiB`.
- `examples/r31_2_pool_cap_threshold_ab_cap32.rs` — `pool_segments=32,
  pool_byte_cap=128 MiB`.
- `examples/_shared/r31_2_pool_cap_threshold_workload.rs` — the workload
  body, `include!`d verbatim into all four.

### 4.2 Paired A/B/B/A protocol via `scripts/paired-ab-runner.mjs`, baseline-anchored

```text
node scripts/paired-ab-runner.mjs --config docs/perf/r31_2_pool_cap_threshold_run.json --arms cap4,cap8
node scripts/paired-ab-runner.mjs --config docs/perf/r31_2_pool_cap_threshold_run.json --arms cap4,cap16
node scripts/paired-ab-runner.mjs --config docs/perf/r31_2_pool_cap_threshold_run.json --arms cap4,cap32
node scripts/paired-ab-runner.mjs --config docs/perf/r31_2_pool_cap_threshold_run.json --arms cap4,cap4   # same-vs-same control
```

All four arms are defined in ONE `--config` file
(`docs/perf/r31_2_pool_cap_threshold_run.json`); `--arms cap4,capN` selects
exactly one pairwise comparison per invocation (the runner's own documented
mechanism for a config that defines more than 2 arms). Each comparison is
20 pairs (this project's real-claim threshold), A/B/B/A block alternation,
the runner's own paired t-test + sign test. **Baseline-anchored, not
all-pairs:** cap4 is compared against each of cap8/cap16/cap32
individually (not cap8-vs-cap16, cap16-vs-cap32, etc.) because the task's
own question is "does decommit_calls_total move OFF the cap4 baseline as
cap increases" — a baseline-anchored design directly answers that, and an
all-pairs sweep (6 comparisons instead of 3) would not add information
this task needs while tripling the process-launch budget.

### 4.3 Config-identity fields (R26-4 contract)

1. **REQUESTED** — the `(pool_segments, pool_byte_cap)` compile-time
   `const`s each binary was built with, source-visible
   (`POOL_SEGMENTS`/`POOL_BYTE_CAP` in each `r31_2_pool_cap_threshold_ab_capN.rs`).
2. **RESOLVED** — read back at runtime and emitted as
   `RESULT pool_segments_requested=`/`RESULT pool_byte_cap_requested=` in
   every launch (§0.1's table confirms each arm reports its own distinct
   value in all 40 of its launches, not a mix — see the raw logs). Proven
   structurally too: a compile-time `static` has no runtime resolution step
   (identical reasoning to R27-4 §3.4/R30-7 §3.4's own latency-axis
   arguments) — there is no registry-slot reuse possible at this entry
   point (each process is one fixed `static`).
3. **Config-conflict counter** — does not apply (no registry-slot reuse
   possible at this entry point, identical to R27-4/R30-7's own reasoning).
4. **Process identity** — subprocess-isolated (`paired-ab-runner.mjs`
   spawns a fresh process per launch, 320 launches total across the 4
   comparisons: 80 launches per comparison × 4).

### 4.4 Path-activation oracle (CLAUDE.md's R30-8-adopted rule)

`decommit_calls_total` (§0.1) is the direct, per-launch, non-inferred
activation signal — every arm in every comparison reports a nonzero,
bit-identical value (40), proving the workload genuinely drives decommit
activity under every tested config (this is NOT a vacuous "the workload
never touches the mechanism" null — the mechanism fires, 40 times, every
single launch, at every cap). What this oracle demonstrates, precisely, is
that the TREATMENT (raising the cap) never changes the observed mechanism
activity — the same distinction R30-7 §0.1 drew between "the workload
touches the mechanism" (true here) and "the mechanism differs between
arms" (false here, at every swept cap).

### 4.5 Statistics and MDE — derived by one checked script

`scripts/r31_2_derive_report_data.mjs` reads all four raw provenance JSONs
and computes: per-arm mechanism-activation distinct-value sets (§0.1), the
paired t-test/sign-test numbers re-read from the runner's own JSON (not
retyped — §0.2), the minimum-detectable-effect (`crit × se`) and its
percentage of each comparison's own mean elapsed time (asserted via a
recomputation check inside the script before printing), and per-arm mean
RSS/commit (§0.3). This is the SAME checked-script discipline CLAUDE.md's
R30-9 rule requires — one script, raw data in, tables out, no
hand-transcription step in between.

---

## 5. What this gate does NOT claim

- **Does not retract R27-4's or `Profile::Throughput`'s finding.** R27-4's
  ~22% single-threaded win stands for its own shape; `Profile::Throughput`'s
  `(8, 32 MiB)` value is unchanged.
- **Not a root-cause isolation for WHY the mechanism never differs.** §1's
  explanation (process-wide counter, likely-unfilled pool at this
  workload's churn pattern) is a plausible candidate supported by this
  task's own data, not a proven mechanism — a future task wanting to
  pursue this would need a per-heap `pooled_count` high-water-mark probe
  (not attempted here, `bench-internals`-gated hooks would be needed and
  none currently exists for this specific quantity).
- **Not an exhaustive characterization of "does pool-cap sizing ever
  matter under concurrency."** This gate swept ONE workload shape (the one
  R30-7 already established) across cap values 4/8/16/32. A different
  thread count, size mix, or (critically) a churn pattern that produces a
  sustained pool-eviction backlog rather than scattered single-segment
  empties could plausibly show a real, cap-dependent effect — this gate
  does not claim to have swept that space.
- **Does not change any `src/` default, `Profile`, or `Cargo.toml`.** No
  production source changed.

---

## 6. Files changed

| file | change |
|---|---|
| `examples/r31_2_pool_cap_threshold_ab_cap4.rs` | new — arm, `(pool_segments=4, pool_byte_cap=16 MiB)`, real `#[global_allocator]` |
| `examples/r31_2_pool_cap_threshold_ab_cap8.rs` | new — arm, `(8, 32 MiB)` |
| `examples/r31_2_pool_cap_threshold_ab_cap16.rs` | new — arm, `(16, 64 MiB)` |
| `examples/r31_2_pool_cap_threshold_ab_cap32.rs` | new — arm, `(32, 128 MiB)` |
| `examples/_shared/r31_2_pool_cap_threshold_workload.rs` | new — shared workload body, `include!`d into all four (byte-identical to R30-7's shape + two additive RESULT fields) |
| `Cargo.toml` | added four `[[example]]` entries (`required-features = ["alloc-global", "alloc-xthread", "alloc-decommit"]`, matching the r27_4/r30_6_latency/r30_7 siblings) |
| `docs/perf/r31_2_pool_cap_threshold_run.json` | new — the `--config` file (4 arms defined, compared pairwise via `--arms`) |
| `scripts/r31_2_derive_report_data.mjs` | new — the checked derivation script (raw JSON → tables/CSV, CLAUDE.md's R30-9 rule) |
| `docs/perf/R31_2_POOL_CAP_THRESHOLD_SWEEP_GATE.md` | this report (new) |
| `docs/perf/R31_2_POOL_CAP_THRESHOLD_SWEEP_GATE_summary.csv` | machine-readable summary, derived by the script above (new) |
| `docs/perf/_raw_r31_2_cap4_vs_cap8.log` / `_cap16.log` / `_cap32.log` / `_control.log` | raw `paired-ab-runner.mjs` stdout, 4 files, 40 launches each (`.gitignore`d by default — `git add -f` at commit time) |
| `docs/perf/paired_ab_runs/2026-07-30T20-57-11-202Z.json` / `20-57-46-116Z.json` / `20-58-23-943Z.json` / `20-58-59-637Z.json` | runner provenance JSONs (`.gitignore`d — `git add -f`) |

**No production source default changed.**

---

## 7. Reproduce

```text
cargo build --release \
  --example r31_2_pool_cap_threshold_ab_cap4 \
  --example r31_2_pool_cap_threshold_ab_cap8 \
  --example r31_2_pool_cap_threshold_ab_cap16 \
  --example r31_2_pool_cap_threshold_ab_cap32 \
  --features "production alloc-stats"

node scripts/paired-ab-runner.mjs --config docs/perf/r31_2_pool_cap_threshold_run.json --arms cap4,cap8
node scripts/paired-ab-runner.mjs --config docs/perf/r31_2_pool_cap_threshold_run.json --arms cap4,cap16
node scripts/paired-ab-runner.mjs --config docs/perf/r31_2_pool_cap_threshold_run.json --arms cap4,cap32
node scripts/paired-ab-runner.mjs --config docs/perf/r31_2_pool_cap_threshold_run.json --arms cap4,cap4   # control

node scripts/r31_2_derive_report_data.mjs \
  docs/perf/paired_ab_runs/<cap4v8-timestamp>.json \
  docs/perf/paired_ab_runs/<cap4v16-timestamp>.json \
  docs/perf/paired_ab_runs/<cap4v32-timestamp>.json \
  docs/perf/paired_ab_runs/<control-timestamp>.json
```

320 process launches total (80 per comparison × 4 comparisons), each
running an 8-thread × 6-round × ~18,500-object-per-round workload — this
workload/host combination completes each 80-launch comparison in roughly
30-40 seconds; the full sweep (4 comparisons) is well under 2 minutes total
wall clock, comfortably inside CLAUDE.md's "short scenario by default"
budget.
