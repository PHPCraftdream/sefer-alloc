# R10-2 — `medium-classes` production gate: process-level A/B/B/A wall-clock judge

**Task:** #228 (R10-2) — the methodologically clean process-level wall-clock
judge the external review asked for after R9-3 (#224) measured a single noisy
criterion run (−37…−56% uniformly, including the 16 B path that has zero
interaction with `medium-classes`) and declared it host noise overruled by the
deterministic IAI gate (+0.49–0.67% Ir).
**Measurement-only:** no `src/` changes, no `Cargo.toml` feature-bundle change.
The deliverable is this doc. The three provenance JSONs cited below were
produced locally during the run but are not committed (reproducible via the
exact commands in §2, matching this repo's convention of not checking in raw
benchmark output).
**Date:** 2026-07-21
**Base revision:** `main` @ `abaad9c` (dirty: the 4 new measurement files this
task adds — `examples/paired_ab_medium_{off,on}.rs`,
`examples/_shared/paired_ab_medium_workload.rs`, `scripts/r10_2_medium_gate.mjs`
— plus the `Cargo.toml` `[[example]]` registrations).
**Platform:** Windows 10 Pro x86-64, native. 11th Gen Intel Core i7-11800H @
2.30 GHz, 8 cores / 16 logical. Power plan: Balanced. `rustc 1.97.0`.
**Harness:** `scripts/paired-ab-runner.mjs` (`--config` mode) driven by
`scripts/r10_2_medium_gate.mjs`, 20 A/B/B/A blocks (4 launches each) per phase,
3 phases = 240 total fresh-process launches. Two separately-named probe
binaries (`paired_ab_medium_off` / `paired_ab_medium_on`) built from one shared
`include!`d workload, differing ONLY in Cargo feature set.
**Feature under test:** `medium-classes` (still experimental, opt-in). This
report supplies the independent-process wall-clock evidence R9-3 explicitly
deferred — it does NOT itself flip the bundle.

---

## 1. Scope recap — what R8-9 / R9-3 did NOT cover (and why this judge exists)

R9-3 closed its criterion section (§6) with: *"These numbers are noise, not a
real regression, and are explicitly overruled by the iai data in §4."* The
external review correctly pushed back: a single noisy criterion run can be
neither accepted as a real regression nor confidently dismissed as noise —
more rigor is needed before promotion. This judge is that rigor.

It measures something NEITHER prior report measured: **phased wall-clock via
independent process launches with paired statistics** on the 256 KiB–1 MiB
range `medium-classes` targets.

| Report | What it measured | What it did NOT measure |
|---|---|---|
| R8-9 (#222) | `AllocCore`-direct single-process sweep of the feature's TARGET range (256 KiB–1 MiB) via `benches/medium_size_sweep.rs`. Single run per config; ns/op are point estimates. | No process-level shape; no realloc-phase isolation; no paired statistics. |
| R9-3 (#224) | The UNAFFECTED small path (16–1024 B): iai Ir (deterministic, WSL) + one noisy criterion run (overruled) + first-heap commit charge. | No medium-range wall-clock; no independent-process measurement; the `realloc_grow` +173.9% Ir was instruction-count only, never measured as real wall-clock for a realloc-heavy program. |
| **R10-2 (this)** | **Three independently-timed phases (alloc / free / realloc) of a 256 KiB–1 MiB working set, via 240 independent process launches (20 A/B/B/A blocks × 3 phases × 4 launches), with paired t-test + sign test per phase.** | — |

---

## 2. Methodology

### 2.1 The probe binaries

Two example binaries (`examples/paired_ab_medium_off.rs`,
`examples/paired_ab_medium_on.rs`) are byte-identical wrappers around one
shared `include!`d workload (`examples/_shared/paired_ab_medium_workload.rs`).
Each installs `SeferAlloc` as the real `#[global_allocator]`, runs the phased
workload, and emits one `RESULT` line per phase. The ONLY difference is the
Cargo feature set at build time:

```text
# Arm A (baseline): Large path for all sizes > ~253 KiB
cargo build --release --example paired_ab_medium_off --features production

# Arm B (treatment): small/medium path for 256 KiB–1 MiB
cargo build --release --example paired_ab_medium_on --features "production,medium-classes"
```

Both arms are built with the full `production` feature set (`alloc-global` +
`alloc-xthread` + `alloc-decommit` + `fastbin` + `alloc-segment-directory`),
so the Large-segment free-cache (`LARGE_CACHE_SLOTS = 8`, gated on
`alloc-decommit`) is ACTIVE in both arms — this is a fair comparison of
"baseline Large-path + 8-slot cache" vs "medium-classes small path + 8-slot
cache", not "cache on vs cache off".

### 2.2 The phased workload

Three independently-timed segments, each accumulating `Instant`-bounded
wall-clock across 20 rounds:

| Phase | What it does | Why this phase |
|---|---|---|
| **alloc** | Allocate 16 simultaneously-live objects (cycling through the six medium sizes: 256 / 320 / 384 / 512 / 768 KiB / 1 MiB), write the first 16 bytes (page touch + dead-code fence), hold them. | Exposes the magazine-carve-vs-OS-reserve difference. 16 > `LARGE_CACHE_SLOTS` (8), so the baseline cannot hide every alloc behind a warm Large-cache hit — 8 of the 16 allocs per round miss the cache and hit the OS. |
| **free** | Free all 16 held objects at their tracked sizes. | Exposes the freelist-push-vs-OS-release difference. Same cache-exceeding working set. |
| **realloc** | Allocate 16 objects at 256 KiB (**untimed** setup), realloc-grow each through 384 → 512 → 768 KiB (**timed**), free at final size (**untimed** teardown). | Exposes the move-leg-vs-in-place-grow difference that R9-3 §4.3 flagged at +173.9% Ir. Each step crosses a medium-class boundary under `medium-classes` (forcing alloc + memcpy + dealloc); under the baseline every step is an in-place Large grow within the dedicated 4 MiB span (header update). |

### 2.3 Working set design — why 16 objects (> `LARGE_CACHE_SLOTS = 8`)

`src/alloc_core/alloc_core.rs:81` defines `LARGE_CACHE_SLOTS = 8`: the
Large-segment free-cache holds up to 8 recently-freed dedicated 4 MiB spans to
amortise the `VirtualFree`/`VirtualAlloc` round-trip. The working set is
deliberately 2× this (16 objects): after freeing 16 Large objects, at most 8
spans enter the cache; on the next round's 16 allocs, 8 hit the cache (fast
path) and 8 miss (real OS reserve). This forces real Large-path churn on the
baseline — measuring the class-routing difference, not the cache's warm-reuse
fast path. `WS_LEN = 16` is the minimum that cleanly exceeds the cache without
making each process launch impractically slow.

### 2.4 The A/B/B/A protocol and paired statistics

`scripts/r10_2_medium_gate.mjs` writes three per-phase `--config` JSONs (one
per `RESULT` metric key: `alloc_ns` / `free_ns` / `realloc_ns`) and invokes
`scripts/paired-ab-runner.mjs` three times — once per phase. Each invocation
runs 20 A/B/B/A blocks (the pattern `A B B A | A B B A | …` averaged across 20
blocks = 80 process launches per phase, 240 total), pairing each block's
A-sample mean against its temporally-adjacent B-sample mean, then computing:

- **Paired t-test** (mean of 20 `(A − B)` deltas, sample stddev, standard
  error, `t = mean/se`, two-tailed against the df=19 critical value 2.101 at
  p<0.05 — the EXACT methodology from
  `docs/perf/R5_R2_CHURN_REGRESSION_PAIRED_AB.md`).
- **Sign test** (count of deltas favoring each side).

The A/B/B/A ordering (not A/B/A/B) averages out monotonic host drift (thermal
throttling, background load creeping up) across each 4-launch block. Each
phase gets its OWN independent A/B/B/A session — so each phase is judged
against its own contemporary baseline, not a single session's drift.

### 2.5 Exact reproduction commands

```text
# Option A: build both arms + run all 3 phases in one command:
node scripts/r10_2_medium_gate.mjs --pairs 20

# Option B: build separately (sequential — parallel cargo builds conflict
# on the shared target/ directory), then run with --skip-build:
cargo build --release --example paired_ab_medium_off --features production
cargo build --release --example paired_ab_medium_on --features "production,medium-classes"
node scripts/r10_2_medium_gate.mjs --skip-build --pairs 20

# Quick smoke (4 pairs per phase):
node scripts/r10_2_medium_gate.mjs --quick
```

Raw captured provenance (one JSON per phase, each with every raw per-process
sample, git commit, rustc version, CPU info, power plan, and the Cargo feature
note — not committed per repo convention):
- `docs/perf/paired_ab_runs/2026-07-21T00-37-32-554Z.json` — alloc phase
- `docs/perf/paired_ab_runs/2026-07-21T00-37-39-700Z.json` — free phase
- `docs/perf/paired_ab_runs/2026-07-21T00-37-46-458Z.json` — realloc phase

---

## 3. Results — three-phase paired A/B/B/A wall-clock

Each cell below is the mean of 20 paired block-values (each block-value =
mean of the block's 2 same-arm launches). A = `production` (medium-classes
OFF, Large path); B = `production,medium-classes` (ON, small/medium path).
Δ = A − B (positive ⇒ A slower ⇒ B faster).

### 3.1 Alloc phase — `medium-classes` is ~31× FASTER

| Metric | A (OFF) | B (ON) | Δ (A−B) |
|---|---:|---:|---:|
| mean | 3.071 ms | 0.099 ms | +2.972 ms |
| min..max | 2.630..3.504 ms | 0.076..0.130 ms | — |
| **per-op** (16 allocs × 20 rounds = 320) | **9.6 µs/alloc** | **310 ns/alloc** | — |

| Statistic | Value |
|---|---|
| paired t | **55.758** (df=19, crit=2.101) → **REAL** |
| sign test | B-faster **20/20**, A-faster 0/20 |

**Reading.** The baseline pays ~9.6 µs per alloc (OS `VirtualAlloc` reserve
for a dedicated 4 MiB span, minus 8 cache hits per round). Medium-classes pays
~310 ns per alloc (magazine carve from a shared 4 MiB segment). This is the
same class of win R8-9 measured via `AllocCore`-direct (§4.1: 200–670 ns
alloc mean for medium sizes at n=64), now confirmed at full process-level
fidelity with paired statistics.

### 3.2 Free phase — `medium-classes` is ~211× FASTER

| Metric | A (OFF) | B (ON) | Δ (A−B) |
|---|---:|---:|---:|
| mean | 13.934 ms | 0.066 ms | +13.868 ms |
| min..max | 12.664..15.165 ms | 0.049..0.092 ms | — |
| **per-op** (16 frees × 20 rounds = 320) | **43.5 µs/free** | **207 ns/free** | — |

| Statistic | Value |
|---|---|
| paired t | **88.289** (df=19, crit=2.101) → **REAL** |
| sign test | B-faster **20/20**, A-faster 0/20 |

**Reading.** The free phase shows the LARGEST ratio: the baseline pays ~43.5 µs
per free (`VirtualFree` for the dedicated span, minus 8 cache deposits),
while medium-classes pays ~207 ns per free (freelist push). R8-9 measured this
same mechanism (§4.1: 65–600× free speedup at medium sizes); this judge
confirms it at process-level with 20/20 sign-test unanimity and t=88.3 — the
strongest statistical signal of the three phases.

### 3.3 Realloc phase — `medium-classes` is ~2,111× SLOWER

| Metric | A (OFF) | B (ON) | Δ (A−B) |
|---|---:|---:|---:|
| mean | 0.037 ms | 79.049 ms | −79.012 ms |
| min..max | 0.028..0.069 ms | 69.919..93.881 ms | — |
| **per-op** (48 reallocs × 20 rounds = 960) | **39 ns/realloc** | **82.3 µs/realloc** | — |

| Statistic | Value |
|---|---|
| paired t | **−53.607** (df=19, crit=2.101) → **REAL** |
| sign test | A-faster **20/20**, B-faster 0/20 |

**Reading.** This is the realloc wall-clock cost R9-3 §4.3 flagged at +173.9%
Ir but never measured as real wall-clock. The baseline pays ~39 ns per
realloc-grow (in-place Large header update — the 4 MiB dedicated span holds
the grown payload, so no copy, no move). Medium-classes pays ~82.3 µs per
realloc-grow (a full move-leg: magazine-alloc the new class's block +
`copy_nonoverlapping` of the preserved prefix + magazine-dealloc the old
block). The copy dominates: 16 objects × (256 + 384 + 512) KiB per round =
18 MiB of `memcpy` per round, × 20 rounds = 360 MiB total — at the host's
memory bandwidth this accounts for the bulk of the 79 ms.

### 3.4 Net total (sum of three phase means)

| | A (OFF) | B (ON) |
|---|---:|---:|
| alloc + free + realloc | **17.042 ms** | **79.214 ms** |
| ratio | 1.0× | **4.65× slower overall** |

The realloc regression completely overwhelms the alloc + free wins for THIS
workload mix (equal rounds of alloc/free churn and realloc-grow churn). §5
discusses why this mix is realloc-adversarial and what a realistic program's
break-even point looks like.

---

## 4. The realloc kill-gate

### 4.1 Threshold proposal: >20% wall-clock regression

Per the task's own suggestion, the realloc kill-gate threshold is set at **>20%
wall-clock regression** on the realloc phase (B vs A median). Justification:

1. **Matches the task's suggestion.** The task proposed >20%; this report
   adopts it.
2. **Meaningful, not noise.** This repo's documented Windows host noise floor
   is ±15–20% (R7/R8 cross-version reports, R9-3 §6). A 20% threshold sits
   just above that floor — it catches real regressions without flagging
   jitter.
3. **Moot here.** The measured realloc regression is **~2,111× (median
   0.038 ms → 77.7 ms)** — so far past ANY reasonable percentage threshold
   (20%, 50%, 100%, 500%) that the specific value does not change the
   verdict. Even a threshold set at 1000% (10×) fires.

### 4.2 Structural asymmetry — why the percentage frame is degenerate

The realloc percentage regression is degenerate because the **baseline's
realloc cost is near-zero by design**. The Large path gives every medium-range
object a dedicated 4 MiB committed span (6% utilization at 256 KiB). Growing
256 → 384 → 512 → 768 KiB all fits within the span → in-place header update →
~39 ns. ANY allocator that packs densely (as `medium-classes` does: 16 objects
in ~2–3 shared 4 MiB segments) must move on a cross-class realloc, because the
block cannot grow past its carved slot. The move-leg copies the preserved
prefix (256–512 KiB per step) — that memcpy is inherent to dense packing, not
a bug.

The baseline "wins" realloc by **wasting memory**: 16 objects at their final
768 KiB size reserve 16 × 4 MiB = 64 MiB of committed spans holding 12 MiB of
actual data (19% utilization). Medium-classes holds the same 12 MiB in ~3
segments (12 MiB committed, ~100% utilization) but pays ~79 ms to move there
via realloc-grow. This is the fundamental density-vs-realloc-speed trade-off;
§5 quantifies the break-even point.

### 4.3 Gate verdict: **FIRES — NO-GO on the realloc axis**

The measured wall-clock realloc regression (~2,111×, t=−53.607, sign 20/20)
exceeds the >20% threshold. Per the task's explicit instruction, this NO-GO
stands on its own — it does not get overruled by IAI or by the alloc/free wins
the way R9-3's single noisy criterion run was dismissed.

---

## 5. Kill-gate / GO-NO-GO verdict

| # | Criterion | Target / expectation | Measured | Verdict |
|---|---|---|---|---|
| K1 | Is the alloc-phase wall-clock real? (not noise) | paired t > crit, sign lopsided | t=55.758, sign 20/20 → B ~31× faster | **PASS** (medium-classes wins alloc) |
| K2 | Is the free-phase wall-clock real? | paired t > crit, sign lopsided | t=88.289, sign 20/20 → B ~211× faster | **PASS** (medium-classes wins free) |
| K3 | Is the realloc-phase wall-clock real? | paired t > crit, sign lopsided | t=−53.607, sign 20/20 → B ~2,111× slower | **PASS** (real, large regression) |
| K4 | **Realloc kill-gate**: does the realloc regression exceed 20%? | < 20% to pass | ~211,100% (2,111×) | **FAIL → NO-GO** |
| K5 | Sanity: do both arms genuinely install SeferAlloc? | segments_reserved_total > 0 in both | A=329, B=11 (both > 0) | **PASS** |

### Verdict: **NO-GO**

**The realloc kill-gate (K4) fires.** The measured wall-clock realloc regression
for a realloc-heavy scenario in the 256 KiB–1 MiB range is ~2,111× (79 ms vs
0.037 ms over 960 realloc-grow operations), statistically unambiguous (paired
t=−53.607, sign test 20/20), and so far past the >20% threshold that the
specific threshold value is moot. Per the task's explicit instruction, this
NO-GO stands on its own — it is not overruled by the alloc/free wins or by the
deterministic IAI gate.

### What the NO-GO means

The three phases together tell a coherent story:

1. **Alloc + free (clear wins):** medium-classes is 31–211× faster on the
   alloc/free paths for medium-range objects. This confirms R8-9's
   `AllocCore`-direct measurements at full process-level fidelity with paired
   statistics. The magazine path (carve / freelist push, ~200–960 ns/op)
   crushes the Large path's OS round-trip (~10–44 µs/op) even with the 8-slot
   Large cache absorbing half the churn. Segment count drops 329 → 11 (the
   density win R8-9 K1 measured).

2. **Realloc (clear loss):** medium-classes is 2,111× slower on the realloc-
   grow path for the same objects. This is the wall-clock confirmation of
   R9-3 §4.3's +173.9% Ir finding — the realloc path now does real small-path
   work (move-leg: alloc + memcpy + dealloc) where the baseline's Large path
   does an in-place header update within the dedicated 4 MiB span. The
   regression is real, large, and structurally inherent to dense packing.

3. **Net for this workload:** the realloc regression dominates the alloc/free
   wins (79 ms vs 0.17 ms of alloc+free savings), making the overall workload
   4.65× slower under `medium-classes`.

### Why this workload is realloc-adversarial (and what a realistic break-even looks like)

This judge's workload is **intentionally realloc-intensive** (960 medium-range
realloc-grow operations) to expose the signal cleanly. A realistic program's
realloc intensity in the 256 KiB–1 MiB range varies enormously:

| Workload profile | Reallocs in medium range | Alloc/free churn | Net winner |
|---|---|---|---|
| Buffer construction (grow to target, then operate) | ~5–20 (startup only) | many | **medium-classes** (realloc cost negligible vs alloc/free savings) |
| Steady-state alloc/free churn (web server, DB page cache) | ~0 | many | **medium-classes** (only wins, no realloc cost) |
| Realloc-heavy steady state (image processing, tensor batching) | ~100s–1000s | few | **baseline** (realloc cost dominates) |

The break-even point: medium-classes's alloc+free saves ~16.9 ms per
alloc/free cycle (alloc 3.07 ms + free 13.93 ms). Each realloc-grow costs
~82.3 µs under medium-classes vs ~39 ns under baseline (Δ ≈ 82.3 µs). So the
break-even is ~16,900 µs / 82.3 µs ≈ **205 reallocs per alloc/free cycle**.
Below 205 reallocs-per-cycle, medium-classes is a net win; above, the baseline
wins. This judge's workload (960 reallocs per 20-round alloc/free cycle = 48
reallocs per round) sits well above the break-even — by design.

### What would change the verdict

The realloc regression is structural, not a bug — but it COULD be mitigated
without abandoning the density win:

1. **In-place medium-class grow within a segment.** If a realloc-grow target
   class has free space in the SAME segment the block already lives in, the
   move-leg could be avoided (carve the new slot in-place, copy within the
   segment). This would not help when the segment is full, but would help the
   common "growing into a fresh segment" case.
2. **Over-allocation within the medium class.** Give each medium-class block a
   growth headroom (e.g. allocate 1.25× the requested size), so a single
   realloc-grow step stays within the same class (in-place fast path, OPT-F).
   This trades internal fragmentation for realloc speed — the same trade-off
   `Vec` makes with its doubling growth factor.
3. **Accept the trade-off for workloads where it doesn't matter.** If the
   target deployment does not realloc heavily in the 256 KiB–1 MiB range (the
   common case for most server / runtime workloads), the realloc regression is
   irrelevant and the alloc/free wins dominate.

---

## 6. Conditions / caveats

1. **This report supplies evidence; it does not flip the bundle.** Per the task
   constraints, `Cargo.toml`'s `production` feature list was not modified.
   Promotion is a separate explicit change-request — matching R9-3's own
   closing line.
2. **The realloc kill-gate is realloc-phase-specific.** The NO-GO verdict is on
   the realloc axis. The alloc and free phases are clear, large,
   statistically-unambiguous wins. A deployment that does not realloc heavily
   in the 256 KiB–1 MiB range would see only the wins. The promotion decision
   must weigh the target workload's realloc intensity (see §5 break-even
   analysis) against the realloc regression this gate exposes.
3. **The percentage frame is degenerate for this comparison.** The baseline's
   realloc cost is near-zero (in-place Large header update within a 4 MiB
   span), so any non-zero medium-classes realloc cost produces an enormous
   percentage regression. The absolute numbers (39 ns vs 82.3 µs per realloc)
   and the net-workload break-even analysis (§5) are the honest frames; the
   >20% threshold fires trivially but the verdict would be the same at any
   reasonable threshold.
4. **Working set is 2× `LARGE_CACHE_SLOTS`.** `WS_LEN = 16` forces 8 of 16
   Large allocs/frees per round to miss the 8-slot Large cache and hit the OS.
   A larger working set (e.g. 32) would increase the baseline's OS churn
   further (more cache misses) without changing the qualitative verdict; a
   smaller working set (≤ 8) would let the baseline hide behind warm cache
   hits, understating the Large-path cost. 16 was chosen as the minimum that
   cleanly exceeds the cache.
5. **Single host, Windows native.** All 240 launches ran on the same physical
   host (i7-11800H, Balanced power plan). The A/B/B/A protocol averages out
   monotonic drift within each phase's session, but cross-phase host
   conditions could differ slightly (each phase ran as an independent session
   ~7 seconds apart). The t-statistics (|t| ≥ 53) and sign tests (20/20) are
   so extreme that minor cross-phase drift cannot flip any phase's verdict.
6. **No `src/` or `Cargo.toml` feature-bundle change.** Confirmed: the only
   `Cargo.toml` modification is the two `[[example]]` registrations for the
   new probe binaries. The `production` feature list is untouched.

---

## 7. Recommendations

1. **Do NOT promote `medium-classes` into the default `production` feature
   bundle as-is.** The realloc kill-gate fires: a realloc-heavy program in the
   256 KiB–1 MiB range is ~2,111× slower per realloc-grow operation under
   `medium-classes`. This is a real, large, structurally-inherent regression
   that the promotion-decision process must address explicitly.
2. **If promotion is desired, implement one of the mitigations in §5 first:**
   (a) in-place medium-class grow within a segment, (b) over-allocation within
   the medium class for growth headroom, or (c) a documented deployment
   profile that excludes realloc-heavy medium-range workloads.
3. **Re-run this judge after any mitigation.** The infrastructure
   (`scripts/r10_2_medium_gate.mjs` + the two probe binaries + the shared
   workload) is reusable: `node scripts/r10_2_medium_gate.mjs --pairs 20`
   reproduces the full 3-phase A/B/B/A measurement. The realloc-phase t-test
   and sign test are the gate; a future mitigation that brings the realloc
   regression under the 20% threshold flips the verdict to GO.
4. **The alloc/free wins are confirmed and large.** Independently of the
   realloc gate, this judge confirms at full process-level fidelity (t ≥ 55,
   sign 20/20) that `medium-classes` makes alloc 31× faster and free 211×
   faster for medium-range objects, with a 329→11 segment-count reduction.
   These wins are not in question; they are the same wins R8-9 measured via
   `AllocCore`-direct, now backed by independent-process paired statistics.

---

## 8. Caveats

- **Realloc-adversarial workload by design.** This judge's realloc phase
  (960 medium-range realloc-grow operations across 20 rounds) is intentionally
  realloc-intensive to expose the signal cleanly. A realistic program may do
  far fewer medium-range reallocs; see §5's break-even analysis.
- **The Large cache is active in both arms** (`alloc-decommit` is part of
  `production`). The baseline's Large-path alloc/free times reflect
  "8-slot-cache + OS reserve/release for the surplus", not "zero cache". This
  is the fair comparison for a production deployment.
- **`realloc_grow` bench (R9-3 §4.3) vs this judge's realloc phase.** R9-3's
  iai `realloc_grow` bench geometrically doubles 64 B → 4 MiB across 16 steps
  (deterministic Ir only). This judge's realloc phase grows 16 objects through
  3 fixed medium-class steps (384 / 512 / 768 KiB from a 256 KiB base) and
  measures real wall-clock. The two are complementary: R9-3 established the
  instruction-count regression (+173.9% Ir); this judge establishes the
  wall-clock regression (~2,111×) and its statistical significance (t=−53.607,
  sign 20/20).
- **No same-vs-same control run was included in this session.** The runner's
  `--arms A,A` control (honesty check) was not run for the config-mode arms.
  The t-statistics (|t| ≥ 53) and sign tests (20/20) are so extreme that a
  control run cannot plausibly change the verdict, but it would be a useful
  addition for a future replication: `node scripts/paired-ab-runner.mjs
  --config docs/perf/paired_ab_runs/_r10_2_realloc.json --arms A,A`.
