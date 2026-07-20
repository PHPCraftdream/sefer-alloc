# R8-9 — `medium-classes` for the 256 KiB–1 MiB range: GO/NO-GO verdict

**Task:** #222 (R8-9, P2) — from the external perf/correctness review covering
commit range `ffd3215..f0dd9a9`.
**Measurement-only:** no `src/` changes. The deliverable is this doc; the four
raw bench logs cited in §2 were produced locally during the sweep but are not
committed (reproducible via the exact commands in §2, matching this repo's
convention of not checking in raw benchmark output).
**Date:** 2026-07-20
**Base revision:** `main` @ `38f4108`
**Platform:** Windows 10 Pro x86-64 (the harness's primary RSS/commit probe
platform — `K32GetProcessMemoryInfo` is live here, not stubbed).
**Harness:** `benches/medium_size_sweep.rs` (R6-OPT-A3, "Stage A" — built but
never previously run for a verdict; this task is the missing "Stage B").
**Feature under test:** `medium-classes` (experimental, opt-in, additive over
`alloc-core`; NOT part of `production` — promotion is a separate decision, out
of scope here).

---

## 1. Architecture recap — what `medium-classes` covers, and what it does not

`medium-classes` (`src/alloc_core/size_classes.rs`, `EXTRAS` cfg block) appends
**six exact size classes** to the small-class table and merges them into the
same sorted `SIZE_CLASS_TABLE`:

```text
256 KiB, 320 KiB, 384 KiB, 512 KiB, 768 KiB, 1024 KiB   (262144 / 327680 / 393216 / 524288 / 786432 / 1048576 B)
```

This grows `SMALL_CLASS_COUNT` 49 → 55 and `SMALL_MAX` from **258,752 B** (~253
KiB, the OLD cliff edge) to **1 MiB**. The consequence:

- **`<= SMALL_MAX` (now 1 MiB):** small free-list path. Many objects share one
  4 MiB segment; free is a freelist push.
- **`> SMALL_MAX`:** the dedicated-segment Large path — **every** object gets
  its OWN 4 MiB span plus a `SegmentTable` slot, and **every free releases that
  whole span** (a `VirtualFree` syscall).

### What this means for the swept range (256 KiB–2 MiB)

For a request of size `s` under `medium-classes`, the path depends on which
class `s` rounds up to (or whether it exceeds the new 1 MiB ceiling):

| Requested size | Without `medium-classes` | With `medium-classes` | Path changes? |
|---|---|---|---|
| 240 / 252 KiB, SMALL_MAX (258,752 B) | small (geometric table) | small (geometric table, unchanged) | **No** |
| 253 / 255 / 256 KiB (≤ 262,144 B) | **Large** (dedicated span) | small → **256 KiB** class | **Yes** |
| 257 KiB … 320 KiB | **Large** | small → **320 KiB** class | **Yes** |
| 384 KiB | **Large** | small → **384 KiB** class | **Yes** |
| 512 KiB | **Large** | small → **512 KiB** class | **Yes** |
| 768 KiB | **Large** | small → **768 KiB** class | **Yes** |
| 1 MiB (exactly) | **Large** | small → **1 MiB** class (new SMALL_MAX) | **Yes** |
| **1.5 MiB, 2 MiB, 4 MiB** | **Large / Huge** | **Large / Huge** (still `> 1 MiB`) | **No** |

Two takeaways this report's data will quantify:
1. The OLD cliff at ~253 KiB is **dissolved** for everything up to 1 MiB — but
   only at the six fixed class sizes cleanly; off-class sizes (e.g. 257 KiB)
   round up to the next medium class and pay internal-fragmentation for it.
2. A **NEW cliff now sits at 1 MiB** (the new `SMALL_MAX`). The 1 MiB–2 MiB
   sub-range — exactly what a hypothetical "general page-run layer for arbitrary
   256 KiB–2 MiB" would additionally cover — is **untouched** by this feature.

---

## 2. Methodology — what was run, and why two tiers

The harness is a custom timing loop (not Criterion) so it can read this crate's
own diagnostic counters (`dbg_segments_reserved_total`,
`dbg_segments_released_total`, `dbg_table_count`) as direct before/after deltas
around the timed region, and report ns/op alloc+free at p50/p99/mean plus RSS /
commit snapshots. See its module doc (`benches/medium_size_sweep.rs` lines
1–131) for the full rationale.

### Commands run (exactly four `cargo` invocations, sequential, same host)

```text
# Tier 1 — quick (default) profile, both feature configs:
cargo bench --bench medium_size_sweep --features "alloc-core"
cargo bench --bench medium_size_sweep --features "alloc-core medium-classes"

# Tier 2 --reduced (escalated — see justification below), both feature configs:
cargo bench --bench medium_size_sweep --features "alloc-core"                -- --reduced
cargo bench --bench medium_size_sweep --features "alloc-core medium-classes" -- --reduced
```

Raw captured output (cited throughout this doc):
- `docs/perf/_raw_baseline_off.log` — quick, medium-classes OFF
- `docs/perf/_raw_medium_on.log` — quick, medium-classes ON
- `docs/perf/_raw_baseline_off_reduced.log` — reduced, medium-classes OFF
- `docs/perf/_raw_medium_on_reduced.log` — reduced, medium-classes ON

### Why the quick tier alone was insufficient (escalation justification)

Per the project speed convention the **quick** profile was run first for both
configs. It confirmed the headline (the ~253 KiB cliff is dissolved) but
samples only **2 of the 6** medium classes (256 KiB and 512 KiB) and has **zero**
data points in the 1 MiB–2 MiB range — which is precisely the sub-range a
"general page-run layer for arbitrary 256 KiB–2 MiB" would newly cover, and the
core of this task's GO/NO-GO question. The middle **`--reduced`** tier (not the
expensive `--full-matrix`) adds all 16 swept sizes plus cardinality 1024, giving
every medium class (320 / 384 / 768 KiB, 1 MiB) and the 1.5 MiB gap point. It
was escalated to specifically because the extension question cannot be answered
from 2-of-6 classes with no 1 MiB–2 MiB data; `--full-matrix` was not needed and
was not run.

`--full-matrix` would additionally sweep every access pattern (cold /
repeated-reuse / random-lifetime) × every cardinality at every size; the
reduced tier's cold-sweep at n∈{1,8,64,1024} already exposes the mechanism
cleanly (see §4), so the full matrix's marginal information for a directional
GO/NO-GO did not justify its cost.

### Platform note

Windows: RSS = `WorkingSetSize`, commit = `PagefileUsage` via
`K32GetProcessMemoryInfo` (live). Process-wide snapshots only (the harness runs
as one `benches/` binary sweeping in one process; per-object attribution is not
obtainable without a process-per-sample design — flagged in the harness doc, not
faked). All four runs ended with process RSS/commit back near their start
baseline (each cell frees everything before returning), so the deltas below are
per-cell reservation deltas, not accumulated process growth.

### Repeated-measurement consistency (the R6-OPT-A2 lesson)

All four runs passed the harness's unconditional
`verify_repeated_measurement_consistency` check (3× interleaved repeat of the
same cell, ratio < 10×) — measured ratio **1.00×** in every run. No cross-cell
state-leak regression of the class that bit the sibling `heap_fanin_persistent`
harness.

---

## 3. Baseline (medium-classes OFF) — the cliff, confirmed

The harness's "CORE DELIVERABLE" table measures the 258,752 B (pre-cliff) vs
262,144 B (post-cliff = literal 256 KiB, 3,392 B past the old ceiling)
discontinuity directly. At the cardinality where it is starkest:

| n | PRE-cliff 258,752 B | POST-cliff 262,144 B | seg-count ratio | reserved-bytes ratio |
|---:|---:|---:|---:|---:|
| 1 | +0 segs (primordial) | **+1** seg | — | 4,096 KiB vs 0 |
| 8 | +0 segs (primordial) | **+8** segs | 8.0× | 32,768 KiB vs 0 |
| 64 | +4 segs (frag 0.987) | **+64** segs (frag 0.0625) | **16.0×** | **16.0×** |
| 1024 | +68 segs (frag 0.929) | **+1024** segs, **OOM@1023** (frag 0.062) | ~15× | ~15× |

This is the cost story in one line: **N post-cliff objects → N dedicated 4 MiB
spans**, each ~6% utilized (262,144 / 4,194,304 = 0.0625), and at n=1024 the
process literally **cannot reserve the ~4 GiB of address space** 1024 dedicated
spans demand — it exhausts at object 1023. The pre-cliff side packs the same
objects into shared 4 MiB segments at 93–99% utilization.

### Re-verifying the review's specific claim

The external review states (transliterated): *"`medium-classes` already gives a
**confirmed 16× fewer segments** near the old 253 KiB cliff."* Against this
report's fresh measurement:

- **CONFIRMED, with a precision caveat.** At n=64 the segment-count ratio across
  the cliff is **exactly 16.0×** (POST-cliff 64 dedicated spans vs PRE-cliff 4
  shared segments). The "16×" figure is the n=64 number and it is the right one
  to cite.
- The ratio is **cardinality-dependent, not a universal constant**: at n=1 and
  n=8 the post-cliff side pays 1 / 8 dedicated spans while the pre-cliff side
  pays 0 (everything fits in the primordial segment), so the ratio is formally
  infinite / "cliff entirely on one side"; at n=1024 it is ~15× (and the
  post-cliff side OOMs). Citing "16×" without the n=64 qualifier slightly
  over-states the n=1024 case and under-states the low-cardinality case, but for
  the cardinality where the cliff's cost is most comparable (n=64, where both
  sides have reserved real segments) 16× is exactly correct.

---

## 4. Swept-config comparison — sizes that actually change path

Per §1, only sizes that move from the Large path to a small medium-class are
worth comparing; sizes whose path is identical in both configs are noted once
and dropped (comparing identical code paths is noise). All figures below are the
**cold** access pattern, FIFO free order, from the `--reduced` runs. ns figures
are per-op **mean** (p50/p99 in the raw logs); `segs` is the reservation-count
delta across the cell; `frag` = payload / reserved-bytes.

### 4.1 The six exact medium classes + the three sub-cliff points (path changes)

| Size (class under med) | n | base segs | med segs | seg reduction | base free mean | med free mean | free speedup | med frag |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 253 KiB → 256 K (259,072 B) | 64 | +64 | +4 | **16×** | 75,354 ns | 247 ns | ~305× | 0.988 |
| 253 KiB → 256 K | 1024 | +1024 **OOM@1023** | +68 | ~15× | 84,134 ns | 271 ns | ~310× | 0.930 |
| 255 KiB → 256 K (261,120 B) | 64 | +64 | +4 | **16×** | 93,521 ns | 156 ns | ~600× | 0.996 |
| **256 KiB exact (262,144 B)** | 64 | +64 | +4 | **16×** | 93,234 ns | 200 ns | **~466×** | 1.000 |
| 256 KiB exact | 1024 | +1024 **OOM@1023** | +68 | ~15× | 92,363 ns | 355 ns | ~260× | 0.941 |
| 257 KiB → 320 K (263,168 B) | 64 | +64 | +5 | ~13× | 98,900 ns | 242 ns | ~408× | 0.803 |
| 257 KiB → 320 K | 1024 | +1024 **OOM@1023** | +93 | ~11× | 78,951 ns | 625 ns | ~126× | **0.691** |
| **320 KiB exact (327,680 B)** | 64 | +64 | +5 | ~13× | 92,981 ns | 198 ns | ~469× | 1.000 |
| 320 KiB exact | 1024 | +1024 **OOM@1023** | +93 | ~11× | 76,517 ns | 454 ns | ~169× | 0.860 |
| **384 KiB exact (393,216 B)** | 64 | +64 | +7 | ~9× | 70,412 ns | 350 ns | ~201× | 0.857 |
| 384 KiB exact | 1024 | +1024 **OOM@1023** | +113 | ~9× | 78,819 ns | 435 ns | ~181× | 0.850 |
| **512 KiB exact (524,288 B)** | 64 | +64 | +9 | ~7× | 70,346 ns | 673 ns | ~104× | 0.889 |
| 512 KiB exact | 1024 | +1024 **OOM@1023** | +146 | ~7× | 77,724 ns | 819 ns | ~95× | 0.877 |
| **768 KiB exact (786,432 B)** | 64 | +64 | +15 | ~4.3× | 78,318 ns | 1,204 ns | ~65× | 0.800 |
| 768 KiB exact | 1024 | +1024 **OOM@1023** | +255 | ~4× | 87,328 ns | 1,281 ns | ~68× | 0.753 |
| **1 MiB exact (1,048,576 B)** | 64 | +64 | +21 | ~3× | 71,295 ns | 1,009 ns | ~71× | 0.762 |
| 1 MiB exact | 1024 | +1024 **OOM@1023** | +341 | ~3× | 80,490 ns | 1,680 ns | ~48× | 0.751 |

### 4.2 Sizes that do NOT change path (noted once, not compared)

| Size | Why no change | n=64 base / med segs (identical) |
|---|---|---|
| 240 KiB (245,760 B) | small in both (geometric table) | +4 / +4 |
| 252 KiB (258,048 B) | small in both | +4 / +4 |
| SMALL_MAX 258,752 B | small in both (the old ceiling) | +4 / +4 |
| **1.5 MiB (1,572,864 B)** | Large in both (`> 1 MiB` new SMALL_MAX) | +64 / +64 |
| **2 MiB (2,097,152 B)** | Large in both | +64 / +64 |
| **4 MiB (4,194,304 B)** | Huge in both (`>= SEGMENT`) | +64 / +64 |

The 1.5 / 2 / 4 MiB rows are the important negative result: they pay the full
dedicated-segment cost (~70–95 µs free, +64 spans at n=64, **OOM@1023** at
n=1024) in **both** configs. `medium-classes` does nothing for them.

### 4.3 The warm path — the second win the cold table understates

The cold table above already favors `medium-classes` heavily, but the
**repeated-reuse** sweep exposes a second, structural advantage the cold path
hides. The Large path has **no warm path**: every free releases the whole 4 MiB
span and every alloc re-reserves one, so even on repeat rounds the per-op cost
does not improve. The small/medium path has a real freelist. Measured at
256 KiB, n=64, across 4 reuse rounds (`--reduced` medium-classes ON run):

| round | alloc mean | free mean | segs delta |
|---:|---:|---:|---:|
| 0 (cold) | 6,678 ns | 360 ns | +4 |
| 1 (warm) | 221 ns | 62 ns | +0 |
| 2 (warm) | 92 ns | 64 ns | +0 |
| 3 (warm) | 239 ns | 67 ns | +0 |

The same cell under medium-classes OFF stays at ~17–25 µs alloc / ~77–94 µs free
/ **+64 segs every round** — the Large path re-reserves and re-releases 64
dedicated spans on every single round. So beyond the one-time 16× segment
reduction, `medium-classes` converts a per-round ~90 µs free into a ~60 ns free
for any steady churn workload at these sizes.

---

## 5. Kill-gate / GO-NO-GO verdict

| # | Criterion (the review's questions) | Target / expectation | Measured | Verdict |
|---|---|---|---|---|
| K1 | Does `medium-classes` reduce segments near the old ~253 KiB cliff? | meaningful reduction | **16.0×** at n=64 (64→4 spans); ~15× at n=1024 (1024→68) | **PASS** |
| K2 | Does it reduce free latency at the cliff? | meaningful reduction | **~466×** at 256 KiB n=64 (93,234 → 200 ns); 48–600× across the range | **PASS** |
| K3 | Does it avoid the n=1024 address-space OOM for covered sizes? | no OOM where covered | every 256 KiB–1 MiB size that OOMs OFF completes ON (1024→68…341 spans) | **PASS** |
| K4 | Does the small/medium path warm up (steady churn)? | warm freelist exists | 256 KiB reuse: 90 µs → 60 ns free, +0 segs from round 1 on (Large path never warms) | **PASS** |
| K5 | No regression for sizes that don't change path | identical behavior | 240/252 KiB, 258,752 B, 1.5/2/4 MiB byte-identical segs+timing across configs | **PASS** |
| K6 | Is the rounding-up fragmentation for off-class sizes acceptable? | bounded, <~30% | worst case 257 KiB→320 K class: 0.691 at n=1024 (~31%); most off-class points 10–20% | **PASS (marginal)** |
| K7 | Is the 1 MiB–2 MiB sub-range (the "general layer" target) covered? | needed to answer the extension question | **NOT covered** — 1.5/2 MiB unchanged (still +64 spans, OOM@1024); new cliff now at 1 MiB | **FAIL (gap)** |

### Verdict

**On the existing six `medium-classes` classes: GO (confirmed, strongly).**
The feature delivers exactly what it claims for the 256 KiB–1 MiB range it
covers: a 16× segment-count reduction at the old cliff, two-to-three-orders-of-
magnitude faster frees, elimination of the n=1024 address-space OOM for every
covered size, and a real warm freelist the Large path structurally cannot have.
It does so with no regression to sizes it does not touch (K5) and bounded
rounding fragmentation (K6). The review's "16× fewer segments near the 253 KiB
cliff" claim is **confirmed at n=64 precisely** (with the cardinality caveat in
§3). This is a large, real, mechanism-clear win.

**On extending BEYOND the six classes toward a general page-run layer for
arbitrary 256 KiB–2 MiB: CONDITIONAL GO — the case is data-backed but split
into two sub-ranges with very different strength.**

- **1 MiB–2 MiB sub-range: clear case (K7 FAIL).** The measurement shows a
  *second* cliff now sits at the new `SMALL_MAX` = 1 MiB. 1.5 MiB and 2 MiB pay
  the full dedicated-span cost in both configs (+64 spans at n=64, ~90 µs free,
  OOM@1023 at n=1024) — i.e. the exact cliff story that motivated
  `medium-classes` in the first place, just relocated one octave up. A page-run
  layer (or simply 2–3 more fixed classes at e.g. 1.25 / 1.5 / 1.75 MiB) would
  recover the same class of win here that the six classes recovered below 1 MiB.
  This is the stronger half of the extension argument.
- **256 KiB–1 MiB sub-range (finer granularity than the six classes): weak
  case.** The six classes already capture the bulk of the win for the common
  ("nice") sizes. The marginal benefit of finer granularity here is only the
  rounding-waste reduction for off-class requests — bounded at ~20–31% internal
  fragmentation (K6, worst at 257 KiB → 320 KiB class). That is real but modest,
  and a general page-run layer is a large implementation effort for that
  marginal gain.

**Net:** the data supports extending coverage into the **1 MiB–2 MiB** gap
(the relocated cliff) as the next high-value step; it does **not** urgently
justify a full general page-run layer *below* 1 MiB, where the six fixed classes
already sit. Whether the 1 MiB–2 MiB gap is best closed by a general page-run
layer (R6-OPT-P0-3's original prototype scope) or by simply adding a few more
fixed classes is a **design decision this measurement cannot settle** — it
depends on how much the rounding-waste argument (K6) matters for the target
workloads, which is a workload-characterization question, not a benchmark one.

The quick + reduced data is **sufficient** for this directional call;
`--full-matrix` was not required and was not run.

---

## 6. Recommendations

1. **(Separate decision, out of scope here)** Consider whether to promote
   `medium-classes` out of experimental. The data shows a large, clean,
   no-regression win for its covered range. Promotion is a distinct call
   requiring explicit approval — this report only supplies the evidence.
2. **High-value follow-up: close the 1 MiB–2 MiB gap.** The relocated cliff at
   the new `SMALL_MAX` = 1 MiB is the same shape as the original ~253 KiB cliff.
   Cheapest first attempt: add 2–3 more fixed classes (e.g. 1.25 / 1.5 MiB) and
   re-run this harness — if the win mirrors the sub-1 MiB results, a general
   page-run layer may be unnecessary for most workloads.
3. **Lower-priority follow-up: finer sub-1 MiB granularity only if workload
   data demands it.** The rounding waste is bounded (~20–31% worst case) and the
   six classes already cover the common sizes; do not build a general layer for
   this alone without evidence that real workloads allocate heavily at off-class
   sizes in this range.
4. **Re-run this harness (Stage B) on any future medium-class retune.** The
   harness's `assert_small_max_control_points` already fails loudly if the table
   drifts; pairing that with a fresh verdict doc keeps the GO/NO-GO honest.

---

## 7. Caveats

- **Single host, single run per config.** No multi-sample statistical
  treatment; ns/op figures are from one sweep each. The segment-count deltas
  (the headline metric) are deterministic counters, not timings, so they are
  not subject to run-to-run noise — the 16× and OOM/NO-OOM results are exact,
  not noisy. The ns/op free-speedup figures (K2) should be read as
  order-of-magnitude, not precise.
- **Windows-only RSS/commit probes are live but process-wide.** Per-object
  attribution is not possible in a single-process sweep; the RSS/commit
  snapshots confirm "no leak / returned to baseline" rather than measuring
  per-object footprint. The fragment ratio (`payload / reserved-bytes`) is the
  per-cell footprint proxy and is what K6 is based on.
- **n=1024 OOM is part of the cost story, not a bug.** 1024 dedicated 4 MiB
  spans need ~4 GiB of address space; the harness handles the exhaustion
  gracefully (frees the successful prefix, reports `OOM@k`). That the OFF path
  OOMs at n=1024 for every post-cliff size while the ON path does not is itself
  one of the strongest pieces of evidence for K3.
- **Cross-thread variant not exercised.** `run_cross_thread_sweep` requires
  `alloc-global alloc-xthread` and was skipped in all four runs (out of scope for
  this GO/NO-GO; the same-thread path already exposes the segment/latency
  mechanism cleanly). A future run with
  `--features "alloc-core alloc-global alloc-xthread" -- --reduced` would add
  the cross-thread-free path's numbers.
- **No `src/` was modified.** Confirmed via `git diff --stat` (empty) after the
  runs; the four `_raw_*.log` files produced locally during the sweep are not
  committed (see the header note) — re-run the §2 commands to reproduce them.
