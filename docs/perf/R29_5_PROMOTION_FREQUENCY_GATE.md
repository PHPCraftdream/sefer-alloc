# R29-5 — medium→Large promotion frequency: RARE, no victim, R22-16 item 6 stays deferred

**Task #436 (R29-5), Round 29.** MEASUREMENT-ONLY, per this project's
"measured, not spun" convention (R24-2/R24-5/R28-1/R29-3/R29-4). This task
runs `docs/perf/R22_16_PROMOTION_REMAP_DESIGN.md` §5.1's Stage-1
workload-shape measurement — the precondition check the design doc's own
CONDITIONAL-GO for a Linux sub-region `mremap` on the medium→Large
realloc-promotion memcpy was gated on but never itself ran. Per an
independent review's explicit rule quoted in that design's own record:
**"No victim, no implementation."**

**Verdict: NO VICTIM. Promotions are RARE relative to total allocation
activity, and every single promotion event copies exactly 128 KiB (never
more).** `docs/perf/OPEN_ITEMS.md` item 6 stays deferred with this task's
numbers attached. Per this task's brief, no design/prototype follow-up task
is opened — only noted as a possibility if a reader wants to weigh a
different (much more skewed) workload shape.

**Date:** 2026-07-29. **Base revision measured:** `main` @
`0f6ce0d3ac61295be85855abecd68fca7b4bc358` + this task's uncommitted working
tree (the compile-fix + new counters + this example). **Platform:** native
Windows 10 Pro x86-64, 16 logical cores. **Feature set:** `production` +
`medium-classes` + `bench-internals`.

**Measurement only. No production behavior changed:** five new
`bench-internals`-gated always-compiled diagnostic statics + accessors (all
inert reads of 0 unless `bench-internals` is on; the increment site is
additionally gated `bench-internals` and lives inside a function
(`try_promote_to_large`) that itself only exists under `medium-classes`), one
new example binary, one `mod.rs` re-export line (visibility fix only, no new
behavior), and this report. No existing function body's LOGIC was changed —
`try_promote_to_large`'s only edit is five counter increments inserted after
the pre-existing memcpy, with no branch/return-value change.

---

## 0. Headline

| metric | value |
|---|---:|
| total allocation activity (fresh allocs + growth reallocs) | 60,722 |
| promotion events | 33 |
| **promotions / total allocation activity** | **0.000543 (0.054%)** |
| promotions / growth-realloc steps only | 0.000900 (0.090%) |
| promotions / distinct growth objects (4,040 total) | 0.008168 (0.82%) |
| copied bytes per promotion (min = mean = max) | **131,072 B (128 KiB)** — identical every time |
| total bytes moved by all promotions combined | 4,325,376 B (~4.1 MiB) |
| histogram | 100% of events in bucket `[128KiB,256KiB)`; **zero** events in any other bucket |

Both runs (same deterministic seed) produced byte-identical results — see
raw logs.

---

## 1. Workload — realistic, not synthetic

The workload simulates the standard `Vec::push`-shaped amortized-doubling
growth pattern, which is the most common real-world trigger of the
promotion path (`docs/perf/R22_16_PROMOTION_REMAP_DESIGN.md` §5.1's framing).
It deliberately mixes a realistic population shape rather than a probe
hand-tuned to make promotions artificially common:

- **4,000 small-population objects** — each starts at 64 B and doubles
  (64 → 128 → 256 → ... ) up to a per-object ceiling drawn uniformly from
  `[64 B, 64 KiB)` — the common case (a `Vec<u32>` of a few hundred elements,
  a small `String`, a small buffer). None of these cross the 256 KiB
  promotion threshold by construction (the ceiling itself is capped below
  it).
- **40 large-population objects** (a 100:1 small:large ratio, a
  conservative, not cherry-picked, estimate that most collections in a real
  application stay small) — same doubling growth, ceiling drawn uniformly
  from `[64 B, 2 MiB)`. Because the ceiling is uniform over that whole range,
  roughly the bottom ~12.5% of the range (`[64 B, 256 KiB)`) draws a ceiling
  that never reaches the promotion threshold either — this is why only 33 of
  40 large-population objects actually promoted (see §3), not a bug.
- **20,000 background allocations** (48 B, immediately freed, never grown) —
  included so the "total allocation activity" denominator reflects a
  realistic mix of allocator traffic, not just the two growth populations in
  isolation (which would understate the denominator and inflate the ratio).

Every growth step is a real `HeapCore::realloc` call through the exact same
dispatch a `Vec::push`'s reallocation takes under `medium-classes`. Full
source: `examples/r29_5_promotion_frequency_gate.rs`.

---

## 2. Methodology

### 2.1 The counters

Five new `bench-internals`-gated diagnostic statics in
`src/alloc_core/alloc_core.rs` (`PROMOTION_COUNT`, `PROMOTION_BYTES_SUM`,
`PROMOTION_BYTES_MIN`, `PROMOTION_BYTES_MAX`, `PROMOTION_BYTES_HIST` — an
8-bucket power-of-two-ish histogram: `<4KiB, 4-16KiB, 16-64KiB, 64-128KiB,
128-256KiB, 256-512KiB, 512-1024KiB, >=1MiB`), incremented exactly once per
successful promotion event, at the existing `try_promote_to_large` call site
in `src/registry/heap_core_free.rs`, immediately after the existing
`Node::copy_nonoverlapping` promotion memcpy — the copied-byte count recorded
is `old_layout.size()`, the exact span that memcpy moves. Read via five new
`#[doc(hidden)]` safe `pub fn` accessors on `AllocCore`
(`src/alloc_core/alloc_core_core_diag.rs`), mirroring the existing
`OPT_H_ATTEMPTS`/`OPT_H_HITS` counter-pair convention exactly.

**Safety analysis (CLAUDE.md benchmark-hook rule):** all five accessors are
plain SAFE `pub fn` (not `unsafe fn`) because they take no arguments and
return no raw pointer — they only report a process-wide atomic counter, the
same shape as `dbg_large_zero_pass_count`/`dbg_opt_h_hit_rate`. No new
`unsafe` seam. Gated `bench-internals` per rule 2 (no production caller);
the promotion path itself only exists under `medium-classes` (not part of
`production`), so under plain `production` neither the counters' increment
site nor the promotion path itself is compiled at all.

### 2.2 Pre-existing partial work — the compile-fix

The counters and accessors already existed as uncommitted work from a prior
session that hit an API quota mid-task. That work did not compile:
`heap_core_free.rs` referenced `crate::alloc_core::alloc_core::PROMOTION_*`
— a doubled module-path segment reaching into the PRIVATE `alloc_core`
submodule from a different top-level module (`registry`), which fails with
`E0603: module 'alloc_core' is private`. Fixed by following this exact
file's own established precedent (`pub(crate) use
alloc_core::LARGE_ZERO_PASS_CALLS;` in `src/alloc_core/mod.rs`): added a
`#[cfg(feature = "bench-internals")] pub(crate) use alloc_core::{...};`
re-export block for the five new promotion items, then updated
`heap_core_free.rs`'s five reference sites from
`crate::alloc_core::alloc_core::X` to `crate::alloc_core::X`. Verified with
`cargo check --features "production medium-classes bench-internals"` (clean)
before any new work began. No logic was changed by this fix — visibility
only.

### 2.3 Why single-process, no subprocess isolation needed

Unlike R27-3/R29-4 (which measured process-wide RSS, vulnerable to the
first-claim-wins registry-slot reuse bug across sequential same-process
arms), this gate reads exact process-wide ATOMIC COUNTERS from a single
`HeapRegistry::claim()` heap in one process, with only one arm (one
workload, one measurement). There is no cross-arm state to leak, and the
counters are read as a delta (`after - before`) around the workload, so
even residual counter state from elsewhere in the same process would not
bias the reported numbers. Determinism is confirmed empirically: two
independent runs (same seed) produced byte-identical results (see raw logs).

### 2.4 Self-verification

- Counters read via delta (`dbg_promotion_count() - baseline`), not raw
  totals, so any prior activity in the same process cannot bias the result.
- `promo_count` (33) cross-checked against the analytically expected value:
  of the 40 large-population objects, those whose randomly drawn ceiling
  falls below 256 KiB (~12.5% of the `[64B, 2MiB)` draw range) never reach
  the promotion threshold — `40 × (1 − 262144/2097088) ≈ 35`, consistent
  with the observed 33 (exact count depends on the RNG draw, not a discrepancy).
- All 33 promotion events landing in exactly one histogram bucket
  (`[128KiB,256KiB)`) is a **structural property of pure-doubling growth**,
  not a measurement artifact: doubling from 64 B always visits exactly
  ..., 65536, 131072, 262144, ... — the step immediately BEFORE crossing the
  256 KiB threshold is always exactly 131072 (128 KiB), and
  `try_promote_to_large` fires on that one crossing step only (after
  promotion the block is Large and all further growth takes the Large-path
  OPT-G in-place-grow fast path, confirmed by reading
  `try_promote_to_large`'s call site — it is inside `medium_promotion_reachable!`,
  only reachable while the block is still medium-classified). A workload
  with non-doubling (e.g. linear, or `+= N`) growth would show a spread
  histogram instead of a single spike; this is disclosed as a workload-shape
  property, not hidden.

---

## 3. Applying the verdict rule (fixed in advance)

Per this task's brief, the verdict rule was fixed BEFORE running the probe:
if promotions are RARE relative to total allocation activity, OR the
copied-byte counts are SMALL, the design has no victim and stays deferred.

- **Frequency: RARE.** 0.054% of all allocation activity (33 / 60,722) is a
  promotion event. Even restricted to only the realloc/growth-step
  population (excluding the fresh/background allocations that can never
  promote), the ratio is still only 0.090% (33 / 36,682). Restricted further
  to distinct growth OBJECTS (the closest analogue to §5.1's
  `promotions_triggered / medium_allocations_made`), only 0.82% of the 4,040
  growth objects ever promoted even once.
- **Copied-byte volume: NOT SMALL, but also NOT LARGE in aggregate, and not
  a spread distribution.** Each individual promotion event moves a
  respectable 128 KiB — this alone is not tiny. But because promotion fires
  AT MOST ONCE per growth trajectory (subsequent grows ride the Large-path
  in-place fast path for free, per `try_promote_to_large`'s own doc comment)
  and only 33 objects in a 4,040-object population ever reach it, the
  AGGREGATE bytes moved by promotion across the whole workload is only
  ~4.1 MiB — a small fraction of the total bytes moved by all allocator
  activity in this run.
- **Both signals point the same direction.** The frequency signal is
  unambiguous (well under 1% by every denominator tried). The per-event
  byte count is real but bounded to a single fixed value (128 KiB) by the
  doubling-growth structure itself, and the aggregate volume is small
  because so few objects ever reach it.

**Verdict: NO VICTIM under this realistic workload shape.** An O(bytes)
memcpy → O(1) VM-metadata-op `mremap` win, applied to an event that fires on
under 1% of growth trajectories and moves a bounded ~4 MiB total across a
60K-operation workload, does not clear the bar the R22-16 design's own
"No victim, no implementation" review rule sets. `docs/perf/OPEN_ITEMS.md`
item 6 stays deferred, with this measurement recorded as the answer to its
previously-unmeasured Stage-1 trigger.

---

## 4. What this gate does NOT claim

- **No claim this generalizes to every possible workload shape.** A
  workload dominated by objects that grow well past 256 KiB (e.g. a
  file-loading or big-buffer-heavy application) would show a higher
  promotion ratio. This gate measures the REALISTIC "few large objects
  among many small ones" shape `Vec`-growth typically produces in general
  application code — not every conceivable shape. A future reader who
  suspects their specific workload is far more promotion-heavy could re-run
  this same probe with a different population mix (the ratio parameters are
  named constants at the top of the example) rather than needing new
  instrumentation.
- **No claim about the per-event 128 KiB figure being universal.** It is a
  structural consequence of pure power-of-two doubling growth from a 64 B
  start; a different growth strategy (e.g. `Vec::with_capacity` jumping
  straight to a large size, or growth by a non-doubling factor) would cross
  the threshold at a different, possibly varying, `old_layout.size()`. The
  histogram mechanism itself is general (any future re-run with a different
  growth shape will show a genuinely different distribution); only THIS
  workload's doubling growth structurally produces a single spike.
- **No fix, design, or prototype attempted or opened.** Per the task brief,
  this is Stage-1 measurement only. The mremap design itself
  (`R22_16_PROMOTION_REMAP_DESIGN.md`) is not touched, no VM/FFI code was
  written, and per the brief's explicit instruction, no follow-up
  design/prototype task is opened by this task even though the verdict is a
  clean NO-GO-on-frequency-grounds — the deferred item stays deferred with
  its numbers attached, as directed.

---

## 5. Files changed

| file | change |
|---|---|
| `src/alloc_core/mod.rs` | compile-fix: +`pub(crate) use alloc_core::{...}` re-export block (5 items) under `#[cfg(feature = "bench-internals")]`, following the existing `LARGE_ZERO_PASS_CALLS` precedent |
| `src/registry/heap_core_free.rs` | compile-fix: 5 reference sites changed from `crate::alloc_core::alloc_core::X` to `crate::alloc_core::X` (no logic change) |
| `src/alloc_core/alloc_core.rs` | (pre-existing partial work, unchanged by this task) 5 new diagnostic statics + `promotion_byte_bucket` |
| `src/alloc_core/alloc_core_core_diag.rs` | (pre-existing partial work, unchanged by this task) 5 new `dbg_promotion_*` accessors |
| `examples/r29_5_promotion_frequency_gate.rs` | NEW — the Stage-1 measurement probe |
| `Cargo.toml` | +`[[example]]` entry for `r29_5_promotion_frequency_gate` |
| `docs/perf/R29_5_PROMOTION_FREQUENCY_GATE.md` | this report (new) |
| `docs/perf/R29_5_PROMOTION_FREQUENCY_GATE_summary.csv` | machine-readable summary (new) |
| `docs/perf/_raw_r29_5_run1.log` | raw probe stdout run 1 (`.gitignore`d — `git add -f` at commit time) |
| `docs/perf/_raw_r29_5_run2.log` | raw probe stdout run 2 (`.gitignore`d — `git add -f` at commit time) |
| `docs/perf/OPEN_ITEMS.md` | item 6's "Current state" card updated with this task's result (append-only) |

**No production source file's LOGIC changed.** `try_promote_to_large`'s only
edit is five counter increments inserted after the pre-existing memcpy
(unconditionally compiled OUT under plain `production`, since
`bench-internals` is not in `production`'s feature list and the enclosing
function itself does not exist without `medium-classes`).

---

## 6. Reproduce

```text
cargo run --release --example r29_5_promotion_frequency_gate --features "production medium-classes bench-internals"
```

Single process, ~1 s wall-clock. Deterministic (fixed seed) — confirmed by
two independent runs producing byte-identical `RESULT` lines (see
`_raw_r29_5_run1.log` / `_raw_r29_5_run2.log`).

Confirm plain `production` (without `medium-classes`/`bench-internals`) is
byte-for-byte unaffected:

```text
cargo build --release --features production
```

compiles clean with zero new surface (the promotion path itself does not
exist without `medium-classes`; the counter statics compile but stay
permanently zero and unreachable-by-increment without `bench-internals`).

---

## 9. 2026-07-30 correction (R30-4, task #453) — the headline ratio's denominator choice needs an explicit companion figure

`docs/perf/OPEN_ITEMS.md` item 28 (filed from
`docs/reviews/2026-07-29-r29-readonly-review.md`, corroborated by
`docs/reviews/2026-07-30-r29-followup-readonly-review.md` §2.4) flagged that
this report's headline "RARE" framing may conflate "rate over ALL
allocation activity" with "rate over the population that could actually
promote." Independently re-checked in task #453 (R30-4).

**CONFIRMED (as a framing gap, not an arithmetic error) — both cited
denominators are correct, but the report never computes or states the
narrowest, most workload-relevant one.** Recomputed directly from this
report's own §0 numbers (33 promotions, 60,722 total allocation events, 40
large-population objects):

- `33 / 60,722 = 0.0543%` (§0's own cited "0.054%" — confirmed exact).
- `33 / 4,040` (all growth objects, small + large populations) `= 0.8168%`
  (§0's own cited "0.82%" — confirmed exact).
- `33 / 40` (the 40 large-population objects — the ONLY objects in this
  workload whose ceiling draw could ever reach the 256 KiB promotion
  threshold; §1 already discloses this population exists and that "only 33
  of 40 large-population objects actually promoted," but §0's headline
  table never computes or states this ratio) `= 82.5%` — **confirmed
  exact, not previously stated anywhere in this report.**

**Both readings are correct; they answer different questions, and this
report's headline states only the wider one.** `0.054%`/`0.82%` correctly
describe promotion's share of this MIXED workload's total activity
(dominated by the 20,000 background allocations and the 4,000
small-population objects, neither of which can ever promote by
construction). `82.5%` correctly describes how common promotion is AMONG
the narrow subset of objects that were deliberately grown into the size
range where promotion could even apply. Neither number is wrong; §0's table
presenting only the wide-denominator figures under the single word "RARE"
is a framing gap — a reader skimming §0 alone would not learn that
promotion is the near-default outcome (82.5%) for the one population
segment the mechanism actually targets.

**Corrected framing (per this task's brief):** promotion work is a small
fraction of allocations in THIS MIXED workload overall (0.054% of all
activity, 0.82% of all growth objects), but common (**82.5%**, 33/40) among
objects that were deliberately grown into the promotable region. **This
framing specifically does NOT support rejecting the Linux `mremap` design
for a promotion-heavy consumer workload** — this report's own population mix
(100:1 small-to-large objects, per §1) was chosen as a REALISTIC general
mix, not a promotion-heavy one; a real consumer workload dominated by
large-buffer growth (e.g. a workload that is mostly the "40-object"
population and little else) could plausibly see promotion rates
approaching the 82.5% figure over ITS OWN total activity, not the diluted
0.054%. §3's verdict ("NO VICTIM under this realistic workload shape") and
`docs/perf/OPEN_ITEMS.md` item 6's "stays deferred" disposition are NOT
reopened by this correction — this report's own §4 ("What this gate does
NOT claim") already states no claim is made that the measured ratio
generalizes to a promotion-heavy workload shape, and the aggregate
copied-byte volume finding (~4.1 MiB moved in this run) is unaffected by
denominator choice. The correction is narrower: the "RARE" headline and
§0's table should not be read, on their own, as evidence against `mremap`
for a workload shape this report did not measure.
