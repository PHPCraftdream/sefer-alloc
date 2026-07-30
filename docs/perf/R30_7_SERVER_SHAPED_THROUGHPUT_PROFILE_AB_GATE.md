# R30-7, Deliverable 4 — does the throughput profile's win hold in an application-shaped scenario?

**Task:** R30-7 (task #456), Round 30, Deliverable 4. Answers the task
brief's explicit question: the `(8, 32 MiB)` small-pool throughput recipe's
~22% win (README, R27-4,
[`R27_4_REAL_DEFAULT_AB_GATE.md`](R27_4_REAL_DEFAULT_AB_GATE.md)) rests on
ONE single-threaded, single-shot 1024 B teardown micro-benchmark. This gate
measures the SAME two configs (`SeferAlloc::new()` vs
`SeferAlloc::with_profile(Profile::Throughput)`) under a more
application-shaped workload — several concurrent "request handler" threads,
each allocating+freeing a MIX of small object sizes repeatedly in a
continuous cycle, not a single burst-then-teardown.

**Verdict: the win does NOT reproduce as a statistically distinguishable
effect in this workload shape — it is indistinguishable from noise, in
EITHER direction.** Paired A/B/B/A (20 pairs, 40 process launches):
`t = -0.119` (mean Δ = default − throughput = -7.44 ms, i.e. `default`
nominally faster on average), sign split 12/8 favoring `default`. Both are
far under `crit(p<0.05) = 2.101`. A same-vs-same honesty control
(`default` vs `default`) shows the SAME noise-band shape (`t = -1.039`,
sign 11/9) — confirming the harness is not manufacturing a false null out
of a real signal; the comparison genuinely has no detectable effect at this
workload's scale on this host. **The mechanism the throughput profile
targets WAS genuinely activated** (`decommit_calls_total = 40` — non-zero
— in every one of the 40 `default`-arm launches, the same activation
oracle R27-3/R27-4 require before trusting a pool-cap comparison), so this
is not a vacuous "the workload never touched the mechanism" null — the
`default` arm's small pool genuinely saturates and churns segments in this
workload too, but the resulting latency cost is swamped by other sources of
variance (thread spawn/join overhead, cross-thread scheduling, the
`large_cache_hits`/segment-reservation contention 8 threads introduce) at a
scale where the single-thread micro-benchmark's cleaner signal does not
carry over.

**This does not retract the README's `(8, 32 MiB)` recipe or the
`Profile::Throughput` preset** — R27-4's finding (single-threaded,
single-shot, one fixed size, `t=8.114`) stands as measured, for the shape
it measured. This gate adds the honest complementary data point the task
brief asked for: the win is workload-shape-dependent, and a caller
choosing `Profile::Throughput` for a genuinely concurrent, mixed-size,
continuous-churn workload should not assume the ~22% figure transfers
unchanged — measure their own workload if the win matters to their
decision.

**Date:** 2026-07-30. **Base revision measured:** `main` @
`1272a522a45acdbb58dd6b0dede946b1ced12fa6` (the paired-ab-runner's own
`git_commit` field, captured automatically at measurement time) + this
task's uncommitted working tree (the profile/example/doc additions this
same task landed) — per CLAUDE.md's R29-6 immutable-source-identity rule,
citing the exact base SHA the provenance JSON recorded is the honest
record available; the working tree is landed in the commit this report is
part of, making the tree state resolvable going forward from that commit. **Platform:** native Windows 10 Pro
x86-64, 11th Gen Intel Core i7-11800H @ 2.30GHz (8 cores / 16 logical) —
the same shared host as R27-3/R27-4/R29-13/R30-6 (this host is shared with
other concurrent agent work during this session; the paired A/B/B/A
protocol plus the same-vs-same control are exactly the machinery this
project uses to separate a real effect from that noise, per R27-4's
established precedent). **Feature set:** `production alloc-stats` (real
`#[global_allocator]`, no `bench-internals` needed — both binaries use only
`SeferAlloc::stats()`, the public always-`production`-available diagnostic
surface, matching R30-6's latency-axis binaries).

---

## 0. Headline numbers

| comparison | n pairs | mean Δ (default − throughput) | t | crit(p<0.05) | significant | sign split (default/throughput) |
|---|---:|---:|---:|---:|---|---|
| default vs throughput | 20 | -7.44 ms | -0.119 | 2.101 | **NO** | 12 / 8 |
| default vs default (control) | 20 | -10.09 ms | -1.039 | 2.101 | **NO** | 11 / 9 |

Both `t` values sit in the same noise band; the control's sign split
(11/9) is not meaningfully tighter than the real comparison's (12/8),
confirming the harness resolves a genuine null rather than manufacturing
one. See `docs/perf/_raw_r30_7_server_shaped_ab.log` (real comparison) and
`docs/perf/_raw_r30_7_server_shaped_control.log` (control) for the full 40
raw process launches each, and
`docs/perf/paired_ab_runs/2026-07-30T12-56-39-878Z.json` /
`2026-07-30T12-58-24-246Z.json` for the complete provenance (every sample,
git commit, rustc version, CPU info, feature set).

**Activation oracle** (the R26-4-style "did the mechanism this comparison
targets actually fire" check, applied here per the task's own
`decommit_calls_total` guidance): every one of the 40 `default`-arm
launches reports `decommit_calls_total = 40` (non-zero, and IDENTICAL
across every launch — the workload's fixed shape drives the same amount of
decommit churn every run), proving the `default` config's small pool
genuinely saturates and cycles segments under this workload — the same
mechanism R27-4 measured a ~22% win from eliminating. This is not a
workload that never touches the pool-cap boundary; the mechanism fires,
the effect on wall-clock is simply not distinguishable from noise at this
concurrency/scale.

---

## 1. Workload — why this is a materially different shape from R27-4's

R27-4's micro-benchmark (`examples/r27_4_real_default_ab_cap4/_cap8.rs`):
ONE thread, ONE fixed size (1024 B), `churn_prefill`/`churn_step`/
`churn_teardown` per labelled batch (allocate 256 objects, churn 1024 ops,
free all 256 — a full teardown every batch), 8 timed batches of 120 cycles
each.

This gate's workload
(`examples/_shared/r30_7_server_shaped_workload.rs`, `include!`d into both
`examples/r30_7_throughput_profile_server_ab_default.rs` and
`_throughput.rs`):

- **8 concurrent threads** (`THREADS = 8`), each simulating an independent
  request handler — genuinely overlapping allocation traffic, not a single
  serial stream.
- **A mix of 4 realistic sizes per object** (`SIZE_CLASSES = [64, 256,
  1024, 4096]` bytes — header/struct/buffer/page-sized), round-robin, not
  one fixed size.
- **A large enough per-round working set to overflow the default pool.**
  `OBJS_PER_ROUND = 18,504` objects/thread/round spread across the 4 size
  classes — peak concurrently-live bytes per thread per round ≈
  `18504 / 4 × (64+256+1024+4096)` ≈ **24.1 MiB**, deliberately calibrated
  to exceed the small pool's default 16 MiB (cap 4) byte ceiling, mirroring
  R27-4's own calibration principle (its batch-120 shape was chosen
  specifically to overflow a 4-segment pool — a working set that fits
  inside the default pool would never activate the mechanism under test).
- **A mid-round churn step** (`CHURN_FRACTION = 1/2`): half the round's
  objects are freed and replaced with a fresh random size before the round
  ends, reproducing a connection-pool-style mix of short- and
  slightly-longer-lived objects rather than every object sharing identical
  lifetime.
- **`ROUNDS = 6` continuous rounds per thread** — a repeated cycle for the
  whole timed region (not one burst-then-teardown), each round fully
  freeing its own working set before the next round starts (bounding peak
  memory per thread while still repeating the alloc/touch/churn/free cycle).
- **End-to-end wall-clock for the WHOLE concurrent run**, timed from just
  before `thread::spawn` to just after every handle is `join`ed — the
  metric an application actually cares about (total time to process a
  fixed amount of concurrent work), not a per-thread average.

Both binaries run the IDENTICAL workload body (`include!`d verbatim,
mirroring this project's established `paired_ab_*` pattern for
guaranteeing the only difference between arms is the installed
`#[global_allocator]` config) — the only difference is which `SeferAlloc`
static is installed:

```text
// default:
static GLOBAL: SeferAlloc = SeferAlloc::new();

// throughput:
static GLOBAL: SeferAlloc = SeferAlloc::with_profile(Profile::Throughput);
```

---

## 2. Why the win vanishes at this scale — a plausible mechanism, not proven root cause

This report does NOT claim to have isolated the exact reason the effect
disappears (that would need a separate, targeted Stage-1 measurement, out
of this task's scope) — but the workload's own structural differences from
R27-4's shape suggest at least three plausible contributors, stated as
hypotheses:

1. **Thread spawn/join overhead and cross-thread scheduling variance are
   themselves large relative to the pool-cap effect at this concurrency.**
   R27-4's single-thread shape has zero thread-lifecycle cost in its timed
   region; this gate's 8-way `thread::spawn`+`join` per launch introduces
   OS scheduling variance (visible in the raw log's wide `elapsed_ns`
   range, e.g. samples from ~136 ms to ~1.9 s in the SAME comparison) that
   is large enough to swamp a ~20 ms mean effect.
2. **8 threads' allocation traffic contends on shared registry/large-cache
   infrastructure** in ways a single thread never does, potentially
   diluting or masking a per-thread pool-cap effect that was cleanly
   visible in isolation.
3. **The mixed-size workload spreads pressure across more size classes**
   than R27-4's single fixed size, potentially changing which segments
   saturate and when relative to the single-size shape's more uniform
   churn pattern.

None of these is confirmed here — they are the honest candidate
explanations for why an activation-proven mechanism (§0's
`decommit_calls_total` check) produces no measurable wall-clock signal at
this scale, offered so a future task that wants to pursue this further has
concrete starting hypotheses rather than none.

---

## 3. Methodology

### 3.1 Two real `#[global_allocator]` binaries, one shared workload body

Mirrors R27-4's and R30-6's established pattern (separate compile-time-
configured binaries, since `SeferAlloc::with_config`/`with_profile` bake
their config into a `static` initialiser — no runtime selection is
possible for the real global-allocator entry point):

- `examples/r30_7_throughput_profile_server_ab_default.rs` — installs
  `SeferAlloc::new()`.
- `examples/r30_7_throughput_profile_server_ab_throughput.rs` — installs
  `SeferAlloc::with_profile(Profile::Throughput)`.
- `examples/_shared/r30_7_server_shaped_workload.rs` — the workload body,
  `include!`d verbatim into both (byte-identical between arms, the same
  discipline `examples/_shared/paired_ab_workload.rs`'s module doc
  establishes and this project's other `paired_ab_*`-family probes follow).

### 3.2 Paired A/B/B/A protocol via `scripts/paired-ab-runner.mjs`

```text
node scripts/paired-ab-runner.mjs --config docs/perf/r30_7_server_shaped_run.json --arms default,throughput
node scripts/paired-ab-runner.mjs --config docs/perf/r30_7_server_shaped_run.json --arms default,default   # same-vs-same control
```

20 pairs (this project's real-claim threshold — not the `--quick` 4-pair
smoke count), A/B/B/A block alternation (averages out monotonic host
drift, R5-R2's established rationale, reused verbatim by R27-4/R30-6), a
hand-rolled paired t-test + sign test computed by the runner itself. The
`docs/perf/r30_7_server_shaped_run.json` config file
(`scripts/paired-ab-runner.mjs`'s `--config` mechanism) names both arms'
prebuilt binary paths and a sanity gate
(`segments_reserved_total > 0` for both arms — both genuinely install a
real allocator and reserve real segments).

### 3.3 Why a within-process-launch A/B via the existing runner, not a new subprocess-per-arm sweep harness

The task brief explicitly permits either "the established paired A/B
methodology... if that fits, or a simpler within-process A/B if the
workload shape makes subprocess isolation unnecessary — your call, but
justify it." This gate uses the EXISTING `paired-ab-runner.mjs`
process-level protocol (each launch is its own fresh OS process, same as
R27-4/R30-6) rather than a same-process sweep, for the same reason
R27-4/R30-6 did: a real `#[global_allocator]` config is baked at compile
time into a `static`, so there is no way to select a config at runtime
within one process — a subprocess-per-arm (here, subprocess-per-LAUNCH,
20 pairs = 40 launches per comparison) is the only way to compare two
different compiled configs at all, not an extra caution layered on top of
an otherwise-avoidable choice. This is NOT the `HeapRegistry`-slot-reuse
hazard R26-4's rule targets (that hazard is specific to same-process
config sweeps via `claim_with_config`); this gate never uses
`claim_with_config` at all — both binaries are ordinary real
`#[global_allocator]` processes, structurally immune to that hazard by the
same reasoning R27-4 §1/R30-6 §1.1 already established for their own
real-allocator latency axes.

### 3.4 Config-identity fields (R26-4 contract, applied to this real-allocator entry point)

1. **REQUESTED** — the `Profile::Throughput` / `SeferAlloc::new()` constant
   compiled into each binary (source-visible, not runtime-configurable).
2. **RESOLVED** — proven structurally: a compile-time `static` has no
   runtime resolution step (identical reasoning to R30-6 §1.6's latency
   axis); each binary's own `decommit_calls_total`/`segments_reserved_total`
   readout (non-zero, consistent per-arm) confirms the compiled config took
   effect (a mis-wired config would show materially different segment
   counts between arms, which the raw log does NOT show — both arms
   reserve comparable segment counts, 62–72, under the same workload).
3. **Config-conflict counter** — does not apply at this entry point (no
   registry-slot reuse is possible; each process is one fixed `static`,
   identical to R27-4's/R30-6's own reasoning for this same entry-point
   shape).
4. **Process identity** — subprocess-isolated (`paired-ab-runner.mjs`
   spawns a fresh process per launch, 40 launches per comparison).

---

## 4. What this gate does NOT claim

- **Does not retract R27-4's finding.** R27-4's ~22% win, measured on its
  own single-threaded single-shot shape, is unaffected — this gate
  measures a DIFFERENT workload shape and reports its own (null) result
  for that shape, not a re-measurement of R27-4's original claim.
- **Not an exhaustive characterization of "does the throughput profile
  help under concurrency."** This gate picked ONE representative
  application-shaped scenario (8 threads, 4-size mix, continuous
  multi-round churn) per the task brief's "one well-chosen additional
  workload shape is sufficient" instruction — a different thread count,
  size mix, or churn pattern could plausibly show a different result
  (including a real win or a real loss); this gate does not claim to have
  swept that space.
- **No root-cause isolation for WHY the effect vanishes** — §2's three
  hypotheses are plausible candidates, not measured/proven causes. A
  future task wanting to pursue this would need a targeted Stage-1
  measurement (per-thread decommit/segment counters, thread-timing
  breakdown) this gate does not attempt.
- **Does not change the `Profile::Throughput` preset or the README
  recipe.** No `src/` default changed by this gate; `Profile::Throughput`'s
  values (`(8, 32 MiB)` + 64 MiB headroom) remain as landed in this same
  task's Deliverable 1.

---

## 5. Files changed

| file | change |
|---|---|
| `examples/r30_7_throughput_profile_server_ab_default.rs` | new — real `#[global_allocator]` arm, `SeferAlloc::new()` |
| `examples/r30_7_throughput_profile_server_ab_throughput.rs` | new — real `#[global_allocator]` arm, `SeferAlloc::with_profile(Profile::Throughput)` |
| `examples/_shared/r30_7_server_shaped_workload.rs` | new — shared multi-thread, mixed-size, continuous-cycle workload body, `include!`d into both binaries above |
| `Cargo.toml` | added two `[[example]]` entries (`required-features = ["alloc-global", "alloc-xthread", "alloc-decommit"]`, matching the r27_4/r30_6_latency sibling pattern) |
| `docs/perf/r30_7_server_shaped_run.json` | new — the `--config` file for `paired-ab-runner.mjs` |
| `docs/perf/R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md` | this report (new) |
| `docs/perf/R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_summary.csv` | machine-readable summary (new) |
| `docs/perf/_raw_r30_7_server_shaped_ab.log` | raw `paired-ab-runner.mjs` stdout, real comparison, 40 launches (`.gitignore`d by default — `git add -f` at commit time) |
| `docs/perf/_raw_r30_7_server_shaped_control.log` | raw stdout, same-vs-same control, 40 launches (`.gitignore`d — `git add -f`) |
| `docs/perf/paired_ab_runs/2026-07-30T12-56-39-878Z.json` / `2026-07-30T12-58-24-246Z.json` | runner provenance JSONs (`.gitignore`d — `git add -f`) |

**No production source default changed.**

---

## 6. Reproduce

```text
cargo build --release --example r30_7_throughput_profile_server_ab_default --example r30_7_throughput_profile_server_ab_throughput --features "production alloc-stats"
node scripts/paired-ab-runner.mjs --config docs/perf/r30_7_server_shaped_run.json --arms default,throughput
node scripts/paired-ab-runner.mjs --config docs/perf/r30_7_server_shaped_run.json --arms default,default   # same-vs-same control
```

40 process launches per comparison (20 pairs × 2 arms), each running an
8-thread × 6-round × ~18,500-object-per-round workload — measured wall
clock for the full 40-launch real comparison plus the 40-launch control:
low single digit minutes total on this 16-core host (well inside
CLAUDE.md's "minutes, not hours" budget for this class of gate).
