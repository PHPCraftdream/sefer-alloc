# R27-4 — the REAL-default A/B latency gate (real byte cap, real `#[global_allocator`)

**Task #422 (R27-4), Round 27.** Every prior A/B in this project's history
(R25-5, R26-1, R26-3) measured `pool_segments=4` vs `pool_segments=8` at a
generous **256 MiB** `pool_byte_cap` — a measurement-only ceiling nobody would
ship. Task #419 (R27-1, commit `3425610`) established that the effective pool
cap resolves as `min(pool_segments, pool_byte_cap/SEGMENT)`, so the REAL
prospective default change is the **PAIRED** `(pool_segments=4, pool_byte_cap=16 MiB)
→ (pool_segments=8, pool_byte_cap=32 MiB)`. Task #421 (R27-3, commit `9e96fd3`)
already quantified the **retention** cost of this exact pair (~+8 MiB/heap
post-teardown, victim-activation-proven) via a registry-bypass probe
(`HeapRegistry::claim_with_config`, not the real global allocator). **This
task's job is the LATENCY side: does the ~16% win R26-3 found survive at the
real byte cap (32 MiB, not 256 MiB), through the real un-bypassed entry point?**

**Verdict: YES — the latency win survives at the REAL byte cap, through the real
`#[global_allocator]`.** Through the un-bypassed production entry point, at the
REAL prospective paired default `(pool_segments=8, pool_byte_cap=32 MiB)`, cap8
is **statistically-significantly faster** than the `(4, 16 MiB)` current default
(paired t = **8.114**, df = 19, crit(p<0.05) = 2.101; sign test **19/20**
favoring cap8, zero ties), with a mean per-run saving of **21.404 ms** (cap4
mean 96.71 ms vs cap8 mean 75.31 ms ≈ **22% faster** at cap8, 8 timed batches).
The decommit cliff that R25-5/R26-1/R26-3 identified reproduces **exactly and
deterministically at the process level**: cap4 reported `decommit_calls_total =
9` in **all 40** of its process launches, cap8 reported `0` in **all 40** of its
launches. A same-vs-same control (cap4 vs cap4) shows t = −0.434 (well under
crit) and an even 11/9 sign split — the harness is **not** manufacturing a
signal.

**Combined latency + retention picture for the pending DEFAULT-CHANGE decision**
(`docs/perf/OPEN_ITEMS.md` item 13): the trade is now fully quantified on BOTH
axes, measured at the REAL prospective config (not a generous measurement
ceiling):

| axis | cap 4→8 (the paired `(4,16MiB)→(8,32MiB)` change) | source |
|---|---|---|
| **latency/decommit** | cap8 **~22% faster** (t=8.114 ≫ crit 2.101, sign 19/20); decommit cliff eliminated (9→0 decommits/run, deterministic) — **through the real `#[global_allocator]` at the REAL byte cap** | this task (R27-4) |
| **retention (RSS)** | cap8 retains **~+8 MiB/heap** post-teardown (~2 segments), scaling **linearly** to ~+255 MiB at 32 heaps (~+4 MiB pooled/drainable, ~+4 MiB committed-non-pooled; does NOT decay during idle) | task #421 (R27-3), cited directly |

This is a genuine RSS-vs-throughput trade — **NOT** the cost-free "RSS-neutral"
change earlier rounds' lower-pressure measurements implied, and **NOT** refuted
by the generous-256-MiB-ceiling latency measurement either. The decision
(promote the paired default or not) is a **separate** task; this report supplies
the latency half of the evidence the decision needs.

**This task does not change any `src/` default.** `DEFAULT_POOL_SEGMENTS` /
`DEFAULT_POOL_BYTE_CAP` remain `4` / `16 MiB`. Measurement only.

**Date:** 2026-07-29. **Base revision measured:** `main` @ `9e96fd3` + this
task's uncommitted working tree. **Platform:** native Windows x86-64, 11th Gen
Intel Core i7-11800H @ 2.30GHz (8 cores / 16 logical), Balanced power plan.
**Feature set:** `production alloc-stats` (matching R25-5/R26-1/R26-3/R27-3's
build).

---

## 0. Headline — the runner's own verdict (not eyeballing)

Driven by `scripts/paired-ab-runner.mjs --config docs/perf/r27_4_run.json
--arms cap4,cap8` (the runner's documented real-claim default of **20 pairs** =
80 process launches, A/B/B/A alternation). The runner computes its own paired
t-test and sign test over the per-block representative values (each block = mean
of its 2 same-arm A/B/B/A samples); these are the runner's printed verdict
lines, transcribed verbatim:

```
=== cap4 vs cap8 (A - B, ns) ===
n=20  mean Δ=21.404 ms  sd=11.797 ms  se=2.638 ms  t=8.114  df=19  crit(p<0.05)=2.101  => REAL (rejects null)
sign test: cap4-faster=1/20  cap8-faster=19/20  ties=0
```

| quantity | cap4 (baseline, A) | cap8 (candidate, B) | Δ (A−B) |
|---|---:|---:|---:|
| mean `elapsed_ns` (20 blocks) | 96,713,838 ns (96.71 ms) | 75,310,243 ns (75.31 ms) | **+21,403,595 ns (cap4 slower by 21.40 ms)** |
| range across 20 blocks | 86.6 .. 108.7 ms | 62.6 .. 98.9 ms | — |
| `decommit_calls_total` (distinct across all 40 launches) | **9** (every launch) | **0** (every launch) | −9 |
| `segments_reserved_total` (distinct) | 16 (every launch) | 8 (every launch) | −8 |
| `rss_after_kib` (post-workload snapshot) | ~30,476 KiB | ~34,576 KiB | +~4,100 KiB |
| `commit_after_kib` (post-workload snapshot) | ~29,952 KiB | ~34,052 KiB | +~4,100 KiB |

cap8 is faster in **19 of 20** paired blocks; the runner's same-vs-same control
(§3) confirms the harness resolves no spurious signal when both arms are
identical. **The latency win reproduces at the REAL byte cap, through the real
`#[global_allocator]`.**

---

## 1. What was built (and why this shape)

Two new example binaries, structurally identical to
`examples/r26_3_teardown_ab_cap4.rs` / `_cap8.rs` (each installs a REAL
`#[global_allocator]` `SeferAlloc` via the const-fn `SeferAlloc::with_config`
path and emits `RESULT` lines via `proc_probe::emit_*`), with TWO deliberate
differences from the R26-3 templates:

1. **The REAL prospective byte caps, not the 256 MiB ceiling.**
   `examples/r27_4_real_default_ab_cap4.rs` uses `pool_segments=4,
   pool_byte_cap=16 MiB` (the CURRENT actual default — a genuine
   same-vs-current-default baseline); `examples/r27_4_real_default_ab_cap8.rs`
   uses `pool_segments=8, pool_byte_cap=32 MiB` (the PROSPECTIVE paired default
   from task #419). The churn primitives (`churn_prefill`/`churn_step`/
   `churn_teardown`, `SIZE=1024`, `CHURN_WORKING_SET=256`, `OPS=1024`,
   `LATENCY_BATCH_SIZE=120`, batched-setup shape) are byte-for-byte copies of
   R25-5/R26-1/R26-3's — copied, not reinvented.

2. **The warm-up placement fixed from the start.** R26-3's `main()` does
   `t0 = Instant::now()` BEFORE calling `run_workload()`, but `run_workload()`'s
   first call is a warm-up batch documented as "untimed" — so all nine batches
   (not eight) end up inside the timed interval (this is exactly what
   task #426/R27-8, a separate pending task, will fix in R26-3's own files).
   These are NEW files with no prior baseline to stay comparable to, so the fix
   is applied from the start: the warm-up batch runs BEFORE `t0 =
   Instant::now()`, then the timer starts, then 8 labelled-timed batches run.
   **`elapsed_ns` covers exactly 8 batches × 120 cycles = 960 timed cycles
   (not R26-3's 9/1080).** This is why R27-4's absolute timing numbers are NOT
   directly comparable to R26-3's raw ms figures (see §5), though the
   qualitative direction/significance is checked for consistency (§2).

### Why the effective cap is identical to R26-3 (and why this measurement still matters)

`AllocCore::new_with_config` resolves `pool_cap = min(pool_segments,
pool_byte_cap / SEGMENT)` (`src/alloc_core/alloc_core.rs:839`; `SEGMENT = 4 MiB`).
`pool_byte_cap` is consumed ONLY by that `min()` — it has no separate budget
effect (verified: `grep -rn 'pool_byte_cap' src/` resolves to the builder field
+ the `min()` resolution sites, nothing else). So:

| arm | `pool_segments` | `pool_byte_cap` | resolved `min(seg, bytes/4MiB)` | R26-3's resolved cap (256 MiB) |
|---|---:|---:|---:|---:|
| cap4 | 4 | 16 MiB | min(4, 4) = **4** | min(4, 64) = 4 — identical |
| cap8 | 8 | 32 MiB | min(8, 8) = **8** | min(8, 64) = 8 — identical |

The effective `pool_cap` is byte-identical between R26-3 (256 MiB ceiling) and
R27-4 (real cap). The latency win was therefore EXPECTED to reproduce, and it
does. **This task's value is not finding a new effect — it is closing the
"never measured at the actual shipping config" gap**: task #419 showed the
real decision is a PAIRED change, and no prior A/B had ever measured that exact
pair through the real entry point. This run proves the real byte cap does not
unexpectedly bind (if it had — e.g. a bug making `pool_byte_cap` constrain
something beyond the resolved cap — the decommit count, RSS, and latency would
diverge from R26-3; they do not).

---

## 2. The decommit mechanism — direct, deterministic confirmation

The `decommit_calls_total` RESULT line (read from `SeferAlloc::stats().decommit_calls`,
`src/global/alloc_stats.rs` — gated `alloc-decommit`) is the direct mechanism
confirmation, not an inference from timing:

- **cap4:** `decommit_calls_total = 9` — identical in **all 40** cap4 process
  launches. The pool cap of 4 is exceeded by the ~6-segment working-set demand,
  so emptied segments are decommitted and re-reserved as the batch churns.
- **cap8:** `decommit_calls_total = 0` — identical in **all 40** cap8 launches.
  The cap of 8 absorbs the 6-segment demand with headroom; no segment is ever
  decommitted.

This is the **same cliff** R25-5/R26-1/R26-3 found (nonzero decommits at cap=4,
zero at cap=8), now confirmed at the REAL byte cap. The `segments_reserved_total`
counter corroborates: cap4 reserves 16 segments cumulatively (steady-state 8 +
~8 re-reserves after decommits), cap8 reserves 8 once and holds them.

The decommit count (9) is identical to R26-3's and lower than R25-5's bypass
(~20). The decommit counter is cumulative across the whole process lifetime
(warm-up + 8 timed batches = 9 batches total), so the warm-up-timing fix does
NOT change the reported `decommit_calls_total` — only the timed `elapsed_ns`.
R26-3 §2's hypothesis for the lower-per-run count (the TLS `current_heap()` →
`HeapCore` path changes exact segment-drain timing relative to a raw
`AllocCore::alloc` sequence) applies unchanged here; the direction and the cliff
reproduce exactly, which is what matters.

---

## 3. Same-vs-same control — the harness is honest

The runner's own module doc requires a same-vs-same control as the honesty
check: a run with both arms identical should show t well under `crit` and a
roughly even sign-test split, or the harness is buggy. Run as
`--config docs/perf/r27_4_run.json --arms cap4,cap4` (cap4 vs cap4, 20 pairs):

```
=== cap4(A-slot) vs cap4(B-slot) (A - B, ns)  [SAME-VS-SAME CONTROL] ===
n=20  mean Δ=−1.205 ms  sd=12.429 ms  se=2.779 ms  t=−0.434  df=19  crit(p<0.05)=2.101  => NOT statistically distinguishable from noise (fails to reject null)
sign test: cap4(A-slot)-faster=11/20  cap4(B-slot)-faster=9/20  ties=0
```

t = −0.434 (≪ 2.101), sign split 11/9 (roughly even). The control passes: when
there is no real difference, the harness correctly reports none. The **contrast**
between this control (t = −0.434, even split) and the main run (t = 8.114,
19/20) is the definitive evidence that the cap4→cap8 latency difference is a
real allocator effect, not a measurement artifact.

---

## 4. Methodology

- **Judge:** `scripts/paired-ab-runner.mjs` (the project's established A/B/B/A
  paired judge — see its module doc and
  `docs/perf/R5_R2_CHURN_REGRESSION_PAIRED_AB.md`). `--config` mode drives two
  arbitrary commands as arms; the paired t-test, sign test, A/B/B/A order,
  same-vs-same control, and provenance JSON are identical to the built-in
  sefer-vs-mimalloc path. Used as-is (not modified).
- **Protocol:** A/B/B/A, 20 pairs = 80 process launches (40 per arm). Each
  "block" is one A/B/B/A launch quadruple; the block's representative value is
  the mean of its 2 same-arm samples, yielding 20 paired deltas. A/B/B/A (not
  A/B/A/B) averages out monotonic host drift across each 4-launch block.
- **Real-claim threshold:** the runner's documented default of 20 pairs (not
  `--quick`'s 4-pair smoke count). This task's verdict needs the real threshold.
- **Sanity gate:** `segments_reserved_total > 0` in BOTH arms (both install a
  real SeferAlloc and run a workload that reserves segments). Both arms passed
  in every launch (cap4 = 16, cap8 = 8).
- **Per-process timed region:** 1 untimed warm-up batch (runs BEFORE the timer)
  + 8 timed batches × 120 cycles = **960 timed** prefill+churn+teardown cycles
  @1024B, ~75–110 ms per process. **This is the R26-3 warm-up-placement fix
  applied from the start** (§1.2): R26-3 timed 9 batches (1080 cycles) because
  its `t0` preceded its warm-up; these binaries run the warm-up before `t0`.
  R27-4's absolute ms figures are therefore NOT directly comparable to R26-3's
  raw ms (see §5), but the per-batch delta is (§5).
- **Repetitions / statistical confidence:** 20 paired blocks. The within-arm
  range (cap4: 86.6–108.7 ms; cap8: 62.6–98.9 ms) reflects real per-process
  jitter, but the paired delta is tight enough (sd = 11.797 ms, se = 2.638 ms)
  that t = 8.114 is far past the p<0.05 critical value.
- **Config-identity fields (R26-4 contract):** the resolved effective cap is
  established structurally here, not via a diagnostic read-back, because the
  `#[global_allocator]` const-fn path makes the config a compile-time constant
  (there is no registry-slot reuse possible — each process is one fixed
  `static`): (1) REQUESTED = `(pool_segments, pool_byte_cap)` written as a
  const in the source; (2) RESOLVED = `min(seg, bytes/4MiB)` = 4/8 (proven by
  the decommit cliff reproducing exactly — a mis-resolved cap would change the
  decommit count); (3) no config-conflict counter applies (no registry at this
  entry point); (4) process identity = **subprocess-isolated** (the runner
  spawns a fresh process per launch, so cross-arm state cannot leak by
  construction).

---

## 5. Relationship to R26-3 / R27-3 / the two-axis decision

### Latency axis — consistency check against R26-3

R26-3 measured the latency win at 256 MiB (9 timed batches, including warm-up).
This task measures it at the REAL byte cap (8 timed batches, warm-up excluded).
The absolute ms are not directly comparable, but the **per-batch delta** is the
consistency check:

| | R26-3 (256 MiB, 9 batches) | R27-4 (real cap, 8 batches) |
|---|---:|---:|
| cap4 mean | 147.58 ms | 96.71 ms |
| cap8 mean | 123.91 ms | 75.31 ms |
| total Δ | 23.67 ms | 21.40 ms |
| **Δ per batch** | **2.63 ms/batch** | **2.68 ms/batch** |
| headline % | 16% | 22% |

The per-batch delta is **constant** (2.63 vs 2.68 ms/batch — within noise). The
headline percentage differs (16% vs 22%) purely because the warm-up-fix removed
a slow batch (the first, primordial-bootstrap batch) from the denominator —
NOT a real increase in the per-batch decommit effect. **Both the direction and
the statistical significance are identical** (R26-3 t=12.212/20-of-20;
R27-4 t=8.114/19-of-20). The win survives at the real byte cap.

### The combined picture (the deliverable a default decision needs)

A latency number without its paired retention number is exactly the failure
mode this project shipped in R26 (R27-2/task #420 corrected it). This report
pairs them, citing R27-3's retention cost directly rather than re-deriving it:

- **Latency (this task):** cap8 ~22% faster (21.40 ms/run saved), decommit
  cliff eliminated (9→0), t=8.114, sign 19/20 — **through the real
  `#[global_allocator]` at the REAL byte cap.**
- **Retention (R27-3, task #421):** cap8 retains ~+8 MiB/heap post-teardown
  (~2 segments: ~+4 MiB pooled/drainable via `dbg_drain_small_pool`, ~+4 MiB
  committed-non-pooled), scaling **linearly** to ~+255 MiB at 32 concurrent
  heaps; does NOT decay during idle (event-driven decay only). Victim
  activation PROVEN: cap-4 saturated (`decommit_delta > 0`); cap-8 retained 6
  pooled segments high-water (`pooled_hw_max > 4`).

This run's own `rss_after_kib` side-channel (cap8 ~34,576 vs cap4 ~30,476,
Δ≈+4,100 KiB) is consistent with R27-3's controlled measurement — R27-3's
~+8 MiB/heap is the authoritative figure (this side-channel's +4,100 KiB is the
lower bound R26-3 also observed; R27-3 §2 explains the workload-dependent gap).

**Net for the pending DEFAULT-CHANGE decision (`docs/perf/OPEN_ITEMS.md` item
13):** the paired `(4,16MiB)→(8,32MiB)` change trades ~+8 MiB/heap of
post-teardown retention (linearly scaling) for ~22% lower latency and a
fully-eliminated decommit churn — both measured at the REAL config through the
real entry point. Whether that trade is net-positive is a deployment-context
judgment (how many concurrent heaps, how latency-sensitive the workload, whether
RSS headroom exists) and is the decision a SEPARATE task makes. This report
supplies the latency half; R27-3 supplied the retention half; together they are
the complete evidence the decision needs.

---

## 6. Files changed

| file | change |
|---|---|
| `examples/r27_4_real_default_ab_cap4.rs` | new — A/B arm, paired `(pool_segments=4, pool_byte_cap=16 MiB)`, real `#[global_allocator]`, batched churn-with-teardown@1024B via `std::alloc`, warm-up-before-timer fix. |
| `examples/r27_4_real_default_ab_cap8.rs` | new — A/B arm, paired `(pool_segments=8, pool_byte_cap=32 MiB)`, identical except config + `arm` emit value. |
| `Cargo.toml` | added two `[[example]]` entries (`r27_4_real_default_ab_cap4` / `_cap8`) with `required-features = ["alloc-global","alloc-xthread","alloc-decommit"]` (matches the r26_3/r27_3 siblings — prevents the E0601 build failure a missing entry causes under plain `--features production`). |
| `docs/perf/r27_4_run.json` | new — the `--config` file (committed; documents exactly what was compared). |
| `docs/perf/R27_4_REAL_DEFAULT_AB_GATE.md` | this report (new). |
| `docs/perf/R27_4_REAL_DEFAULT_AB_GATE_summary.csv` | machine-readable summary of §0 + §3 (new). |
| `docs/perf/_raw_r27_4_real_default_ab.log` | the runner's raw stdout for the main + control runs (`.gitignore`d — `git add -f`). |
| `docs/perf/paired_ab_runs/2026-07-28T23-55-35-517Z.json` | runner provenance for the main run (`.gitignore`d — `git add -f`). |
| `docs/perf/paired_ab_runs/2026-07-28T23-56-02-705Z.json` | runner provenance for the control run (`.gitignore`d — `git add -f`). |

**No production source file changed** (`DEFAULT_POOL_SEGMENTS` /
`DEFAULT_POOL_BYTE_CAP` remain `4` / `16 MiB`). `scripts/paired-ab-runner.mjs`
was **not** modified — used as-is. **No commit made** — tree left unstaged for
personal zero-trust review, per this task's explicit instruction.

---

## 7. Reproduce

```text
cargo build --release --example r27_4_real_default_ab_cap4 --example r27_4_real_default_ab_cap8 --features "production alloc-stats"
node scripts/paired-ab-runner.mjs --config docs/perf/r27_4_run.json --arms cap4,cap8
# same-vs-same honesty control:
node scripts/paired-ab-runner.mjs --config docs/perf/r27_4_run.json --arms cap4,cap4
```

The runner prints the paired t-test / sign-test verdict to stdout and writes a
timestamped provenance JSON (raw per-process samples, git commit, rustc/CPU/power
info, feature set) under `docs/perf/paired_ab_runs/`. Each binary may also be
smoke-run directly to inspect its `RESULT` lines (including
`decommit_calls_total`): `./target/release/examples/r27_4_real_default_ab_cap4.exe`
(prints `decommit_calls_total=9`) vs `..._cap8.exe` (prints
`decommit_calls_total=0`).

---

## 8. Pre-push gate

`cargo test --features production` (this project's pre-push gate,
`scripts/check-all.mjs`'s test step): **PASS** — exit 0, all test binaries
report `test result: ok`, no `E0601`/compile regression (the exact bug class
that hit R25-3/R25-5 from missing `[[example]] required-features` entries; the
two new entries in §6 prevent it). Run on the same working tree as the
measurement.
