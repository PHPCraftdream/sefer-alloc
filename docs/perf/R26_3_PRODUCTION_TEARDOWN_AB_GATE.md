# R26-3 — production-entry-point A/B/B/A judge for the `pool_segments=8` latency win

**Task #412 (R26-3), Round 26.** R25-5/R26-1 measured the latency/decommit axis
(`pool_segments` 4→8 eliminates a ~20-decommit/run residual) via a deliberate
**bypass**: `AllocCore::new_with_config` direct, no registry, on a fresh OS
thread — chosen specifically because it made `dbg_pool_cap()`/`dbg_pooled_count()`
directly readable so the swept value could be self-verified
(`docs/perf/R25_5_POOL_CAP_SWEEP_GATE.md` §1, `docs/perf/R26_1_POOL_CAP_RSS_SUBPROCESS_GATE.md`
§1.5). That bypass is honest for *isolating* the mechanism, but it is NOT the
code shape a real program hits: a real binary routes every `alloc`/`dealloc`
through the `#[global_allocator]` indirection (TLS heap lookup, not a directly-
called `&A: GlobalAlloc` reference). **This task's job is to confirm the SAME
latency win holds through that un-bypassed production entry point**, judged by
an A/B/B/A alternating-process paired t-test (not a single sequential run) so
systematic host drift is cancelled.

**Verdict: CONFIRMED.** Through the real `#[global_allocator]` `SeferAlloc`,
`pool_segments=8` is **statistically-significantly faster** than the
`pool_segments=4` baseline (paired t = **12.212**, df = 19, crit(p<0.05) =
2.101; sign test **20/20** favoring cap8, zero ties), with a mean per-run
saving of **23.667 ms** (cap4 mean 147.58 ms vs cap8 mean 123.91 ms ≈ **16%
faster** at cap8). The decommit cliff that R25-5/R26-1 identified reproduces
**exactly and deterministically at the process level**: cap4 reported
`decommit_calls_total = 9` in **all 40** of its process launches, cap8 reported
`0` in **all 40** of its launches. A same-vs-same control (cap4 vs cap4) shows
t = −0.282 (well under crit) and an even 8/12 sign split — the harness is **not**
manufacturing a signal.

**This task does not change any `src/` default.** `DEFAULT_POOL_SEGMENTS` stays
`4`. Measurement only; the promote-4→8 decision remains a separate pending item
(`docs/perf/OPEN_ITEMS.md` item 13).

**Date:** 2026-07-28. **Base revision measured:** `main` @ `779474e` + this
task's uncommitted working tree. **Platform:** native Windows x86-64, 11th Gen
Intel Core i7-11800H @ 2.30GHz (8 cores / 16 logical), Balanced power plan.
**Feature set:** `production alloc-stats` (matching R25-5/R26-1's build).

---

## 0. Headline — the runner's own verdict (not eyeballing)

Driven by `scripts/paired-ab-runner.mjs --config docs/perf/r26_3_run.json
--arms cap4,cap8` (the runner's documented real-claim default of **20 pairs** =
80 process launches, A/B/B/A alternation). The runner computes its own paired
t-test and sign test over the per-block representative values (each block = mean
of its 2 same-arm A/B/B/A samples); these are the runner's printed verdict
lines, transcribed verbatim:

```
=== cap4 vs cap8 (A - B, ns) ===
n=20  mean Δ=23.667 ms  sd=8.667 ms  se=1.938 ms  t=12.212  df=19  crit(p<0.05)=2.101  => REAL (rejects null)
sign test: cap4-faster=0/20  cap8-faster=20/20  ties=0
```

| quantity | cap4 (baseline, A) | cap8 (candidate, B) | Δ (A−B) |
|---|---:|---:|---:|
| mean `elapsed_ns` (20 blocks) | 147,581,243 ns (147.58 ms) | 123,914,448 ns (123.91 ms) | **+23,666,795 ns (cap4 slower by 23.67 ms)** |
| range across 20 blocks | 131.9 .. 161.8 ms | 113.5 .. 146.2 ms | — |
| `decommit_calls_total` (distinct across all 40 launches) | **9** (every launch) | **0** (every launch) | −9 |
| `segments_reserved_total` (distinct) | 16 (every launch) | 8 (every launch) | −8 |
| `rss_after_kib` (post-workload snapshot) | ~30,476 KiB | ~34,576 KiB | +~4,100 KiB |
| `commit_after_kib` (post-workload snapshot) | ~29,948 KiB | ~34,052 KiB | +~4,100 KiB |

cap8 is faster in **all 20** paired blocks; the runner's same-vs-same control
(§3) confirms the harness resolves no spurious signal when both arms are
identical. **The latency win reproduces through the real `#[global_allocator]`.**

---

## 1. What was built (and why this shape)

Two new example binaries, near-identical copies of `examples/paired_ab_sefer.rs`'s
*structure* (each installs a REAL `#[global_allocator]` `SeferAlloc` and emits
`RESULT` lines via `proc_probe::emit_*`), but running the **exact**
`bench_global_alloc_churn_with_teardown`@1024B workload shape R25-5/R26-1
measured — byte-for-byte copies of `benches/global_alloc.rs`'s
`churn_prefill`/`churn_step`/`churn_teardown` (same PRNG seed `0xCAFE`, same
`CHURN_WORKING_SET=256`, same `OPS=1024`):

- `examples/r26_3_teardown_ab_cap4.rs` — `#[global_allocator]` with `pool_segments = 4`.
- `examples/r26_3_teardown_ab_cap8.rs` — `#[global_allocator]` with `pool_segments = 8`.

The **only** difference between the two is the `POOL_SEGMENTS` constant threaded
into `SeferAlloc::with_config(LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(N).pool_byte_cap(256 MiB)))`
(`SeferAlloc::with_config` is a `const fn` and every builder in the chain is
`const`, so this composes as a `static` initializer — `src/global/sefer_alloc.rs:260`).
The 256 MiB byte cap mirrors R25-5/R26-1's `GENEROUS_POOL_BYTE_CAP` exactly, so
`pool_segments` alone is the constraint.

**The critical difference from R25-5/R26-1's latency axis:** those tasks called
`AllocCore::alloc`/`dealloc` (or `alloc.alloc` via a generic `&A: GlobalAlloc`
reference) **directly**. These two binaries call `std::alloc::alloc`/`dealloc`,
which route through the installed `#[global_allocator]` SeferAlloc — the
un-bypassed production entry point (TLS `current_heap()` lookup → `HeapCore` →
`AllocCore`). That is this task's entire point.

### Workload shape — batched, not sequential (load-bearing)

The batched `run_latency_batch` shape is **required** to reproduce the
segment-fan-out pressure that trips cap=4. R25-5's module doc documents that a
naive sequential one-cycle-at-a-time loop (never more than `CHURN_WORKING_SET`
blocks live at once) measures **zero** decommits at every cap, including cap=4
— a vacuous counterfactual. The fix (R25-5): collect `LATENCY_BATCH_SIZE`
prefills into a `Vec` UP FRONT (all `batch_size × CHURN_WORKING_SET` blocks
concurrently live), THEN churn+teardown each — reproducing criterion 0.5.1's
`iter_batched`/`SmallInput` actual `inputs = (0..batch_size).map(||
setup()).collect()` batching. `LATENCY_BATCH_SIZE = 120` settles the live pooled
segment count at 6 (R25-5's verified diag), comfortably exceeding cap=4 while
cap=8+ absorbs it with room to spare. This binary replicates that shape exactly
(`LATENCY_BATCH_SIZE = 120`, 8 timed batches + 1 untimed warm-up = 960 timed
cycles), only swapping the alloc/dealloc call site from `alloc.alloc` to
`std::alloc::alloc`.

---

## 2. The decommit mechanism — direct, deterministic confirmation

This task's `decommit_calls_total` RESULT line (read from `SeferAlloc::stats().decommit_calls`,
`src/global/alloc_stats.rs:74` — gated `alloc-decommit`, NOT `alloc-stats) is
the direct mechanism confirmation, not an inference from timing:

- **cap4:** `decommit_calls_total = 9` — identical in **all 40** cap4 process
  launches (the runner spawns 20 pairs × 2 A-slots = 40 cap4 launches). The pool
  cap of 4 is exceeded by the ~6-segment working-set demand, so emptied segments
  are decommitted and re-reserved as the batch churns.
- **cap8:** `decommit_calls_total = 0` — identical in **all 40** cap8 launches.
  The cap of 8 absorbs the 6-segment demand with headroom; no segment is ever
  decommitted.

This is the **same cliff** R25-5/R26-1 found (nonzero decommits at cap=4, zero
at cap=8), now confirmed through the real global allocator. The
`segments_reserved_total` counter corroborates: cap4 reserves 16 segments
cumulatively (steady-state 8 + ~8 re-reserves after decommits), cap8 reserves 8
once and holds them — the decommit/re-reserve churn at cap=4 is what inflates
its reservation counter.

### Honest nuance — the per-run count is lower than R25-5's bypass

R25-5/R26-1 reported **~20 decommits/run** via the `AllocCore`-direct bypass
(10 timed batches × ~2 decommits/batch). This task measures **9 decommits/run**
through the real global allocator (9 batches × ~1 decommit/batch). The
**direction and the cliff reproduce exactly** (nonzero at cap=4, identically
zero at cap=8); only the per-batch rate is roughly halved. **Hypothesis
(unverified):** the global allocator routes allocations through the TLS
`current_heap()` → `HeapCore` path (which wraps `AllocCore` and adds size-class
routing / magazine caching under `fastbin`), and that intervening layer changes
the exact segment-drain timing enough that fewer full-drain→decommit events fire
per batch than a raw `AllocCore::alloc` call sequence does. This was not
instrumented here (it would require per-event decommit tracing, out of scope for
a confirm-through-the-real-entry-point task). What is **confirmed**: the
decommit-driven slowdown is real, large, and statistically unambiguous at the
latency axis (§0), even at the lower per-run decommit count — so the lower
count does not weaken the verdict, only refines the mechanism's magnitude.

---

## 3. Same-vs-same control — the harness is honest

The runner's own module doc requires a same-vs-same control as the honesty
check: a run with both arms identical should show t well under `crit` and a
roughly even sign-test split, or the harness is buggy. Run as
`--config docs/perf/r26_3_run.json --arms cap4,cap4` (cap4 vs cap4, 20 pairs):

```
n=20  mean Δ=−0.522 ms  sd=8.280 ms  se=1.852 ms  t=−0.282  df=19  crit(p<0.05)=2.101  => NOT statistically distinguishable from noise (fails to reject null)
sign test: cap4(A-slot)-faster=8/20  cap4(B-slot)-faster=12/20  ties=0
```

t = −0.282 (≪ 2.101), sign split 8/12 (roughly even). The control passes: when
there is no real difference, the harness correctly reports none. The
**contrast** between this control (t = −0.282, even split) and the main run
(t = 12.212, 20/20) is the definitive evidence that the cap4→cap8 latency
difference is a real allocator effect, not a measurement artifact.

---

## 4. Methodology

- **Judge:** `scripts/paired-ab-runner.mjs` (the project's established A/B/B/A
  paired judge — see its module doc and `docs/perf/R5_R2_CHURN_REGRESSION_PAIRED_AB.md`).
  `--config` mode drives two arbitrary commands as arms; the paired t-test, sign
  test, A/B/B/A order, same-vs-same control, and provenance JSON are identical
  to the built-in sefer-vs-mimalloc path.
- **Protocol:** A/B/B/A, 20 pairs = 80 process launches (40 per arm). Each
  "block" is one A/B/B/A launch quadruple; the block's representative value is
  the mean of its 2 same-arm samples, yielding 20 paired deltas. A/B/B/A (not
  A/B/A/B) averages out monotonic host drift across each 4-launch block.
- **Real-claim threshold:** the runner's documented default of 20 pairs (not
  `--quick`'s 4-pair smoke count). This task's verdict needs the real threshold.
- **Sanity gate:** `segments_reserved_total > 0` in BOTH arms (both install a
  real SeferAlloc and run a workload that reserves segments — unlike the
  built-in sefer-vs-mimalloc profile where only the sefer arm is nonzero). Both
  arms passed in every launch (cap4 = 16, cap8 = 8).
- **Per-process timed region:** warm-up batch + 8 timed batches × 120 cycles =
  960 timed prefill+churn+teardown cycles @1024B, ~110–170 ms per process —
  comfortably multi-millisecond for stable `Instant`-based wall-clock, short
  enough that 80 launches finish in ~25 s (this project's "Speed: short
  scenario by default"). No silent caps applied.
- **Repetitions / statistical confidence:** 20 paired blocks. The within-arm
  range (cap4: 131.9–161.8 ms; cap8: 113.5–146.2 ms) reflects real per-process
  jitter, but the paired delta is tight enough (sd = 8.667 ms, se = 1.938 ms)
  that t = 12.212 is far past the p<0.05 critical value — this is not an artifact
  of insufficient pairs.

---

## 5. Relationship to R25-5 / R26-1 / the two-axis decision

R26-1 re-stated the `pool_segments=8` case as surviving on the **latency/decommit
axis alone** (the RSS axis was refuted as RSS-neutral, not RSS-beneficial). This
task **strengthens** that latency axis by removing the one caveat a reviewer
could still raise against it: that R25-5/R26-1 measured it through an
`AllocCore` bypass, not the real global allocator. With that caveat removed:

- **Latency/decommit axis:** cap 4→8 eliminates the decommit residual (9→0
  here, 20→0 in R25-5's bypass units) AND produces a real, large, statistically
  significant wall-clock speedup (16% faster, t = 12.212, 20/20 sign test) —
  **now confirmed through the un-bypassed `#[global_allocator]` entry point.**
- **RSS/commit axis:** R26-1's "RSS-neutral under sustained load" finding is
  unchanged. The post-workload snapshots here (cap8 ~4 MiB higher residual RSS)
  are a **different measurement** from R26-1's peak-delta-under-sustained-churn
  methodology and are the expected consequence of cap8 retaining its larger pool
  rather than churning it (cap8 holds 8 segments resident; cap4 returns the
  churned ones to the OS). They are not a contradiction of R26-1 and are not the
  verdict basis for this task (latency is).

**Net for the pending DEFAULT-CHANGE decision (`docs/perf/OPEN_ITEMS.md` item
13):** the latency case for cap=8 is now confirmed through the real entry point
at full statistical rigor. The decision itself (promote `DEFAULT_POOL_SEGMENTS`
4→8) remains pending and is not made by this measurement-only task.

---

## 6. Files changed

| file | change |
|---|---|
| `examples/r26_3_teardown_ab_cap4.rs` | new — A/B arm, `pool_segments=4`, real `#[global_allocator]`, batched churn-with-teardown@1024B via `std::alloc`. |
| `examples/r26_3_teardown_ab_cap8.rs` | new — A/B arm, `pool_segments=8`, identical except `POOL_SEGMENTS` and the `arm` emit value. |
| `Cargo.toml` | added two `[[example]]` entries (`r26_3_teardown_ab_cap4` / `_cap8`) with `required-features = ["alloc-global","alloc-xthread","alloc-decommit"]` (matches the r25_5/r26_1 siblings — prevents the E0601 build failure a missing entry causes under plain `--features production`). |
| `docs/perf/r26_3_run.json` | new — the `--config` file (committed; documents exactly what was compared). |
| `docs/perf/R26_3_PRODUCTION_TEARDOWN_AB_GATE.md` | this report (new). |
| `docs/perf/R26_3_PRODUCTION_TEARDOWN_AB_GATE_summary.csv` | machine-readable summary of §0 + §3 (new). |
| `docs/perf/_raw_r26_3_production_teardown_ab.log` | the runner's raw stdout for the main run (`.gitignore`d — `git add -f`). |
| `docs/perf/paired_ab_runs/2026-07-28T16-25-31-476Z.json` | runner provenance for the main run (`.gitignore`d — `git add -f`). |
| `docs/perf/paired_ab_runs/2026-07-28T16-26-30-521Z.json` | runner provenance for the control run (`.gitignore`d — `git add -f`). |

**No production source file changed** (`DEFAULT_POOL_SEGMENTS` remains `4`).
`scripts/paired-ab-runner.mjs` was **not** modified — used as-is. **No commit
made** — tree left unstaged for personal zero-trust review, per this task's
explicit instruction.

---

## 7. Reproduce

```text
cargo build --release --example r26_3_teardown_ab_cap4 --example r26_3_teardown_ab_cap8 --features "production alloc-stats"
node scripts/paired-ab-runner.mjs --config docs/perf/r26_3_run.json --arms cap4,cap8
# same-vs-same honesty control:
node scripts/paired-ab-runner.mjs --config docs/perf/r26_3_run.json --arms cap4,cap4
```

The runner prints the paired t-test / sign-test verdict to stdout and writes a
timestamped provenance JSON (raw per-process samples, git commit, rustc/CPU/power
info, feature set) under `docs/perf/paired_ab_runs/`. Each binary may also be
smoke-run directly to inspect its `RESULT` lines (including
`decommit_calls_total`): `./target/release/examples/r26_3_teardown_ab_cap4.exe`
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

---

## CORRECTION (2026-07-28, R27-1, task #419)

§1's abstract and §6's closing sentence phrase the pending default-change as
"promote `DEFAULT_POOL_SEGMENTS` 4→8". That is a literal NO-OP as written:
`AllocCore::new_with_config` resolves the effective pool cap as
`min(pool_segments, pool_byte_cap / SEGMENT)` (`src/alloc_core/alloc_core.rs:837-839`),
and `DEFAULT_POOL_BYTE_CAP = 16 MiB` (`src/alloc_core/small_segment_pool_config.rs:117`,
`SEGMENT = 4 MiB`) already resolves to `16 MiB / 4 MiB = 4`, so `min(8, 4) = 4` —
editing only `DEFAULT_POOL_SEGMENTS` from 4 to 8 leaves the allocator
byte-identical. The real decision is a PAIRED change
`(pool_segments, pool_byte_cap) = (4, 16 MiB) → (8, 32 MiB)`, which doubles the
per-heap retained committed pool ceiling (16 MiB → 32 MiB; at 32 concurrent
heaps, up to 1 GiB). This task's A/B arms are UNAFFECTED — both cap4 and cap8
used a 256 MiB byte cap so the byte ceiling never bound, and they measured the
EFFECTIVE cap, not the one-constant default edit, so every number in this report
stands. Only the framing of the *separate* default-change decision this report
defers to was malformed. See `docs/perf/OPEN_ITEMS.md` item 13's 2026-07-28
R27-1 note for full detail.
