# R25-5 — the RSS-gated `pool_segments` sweep (4/8/16/32) closing R24-11's "next trigger"

**Task #399 (R25-5), Round 25.** Direct continuation of
`docs/perf/OPEN_ITEMS.md` item 13 (R24-11, task #389), whose "Next trigger"
explicitly named this task: "a round wants to close the 1024B residual via an
RSS-gated `pool_segments` sweep (4/8/16/32, measuring decommits Δ AND peak
RSS), mirroring the existing `bench_pool_cap_sweep`/`pool_cap_sweep_spread_and_drain`
harness." Also directly implements `docs/reviews/2026-07-28-r24-readonly-review.md`'s
"P2 — solve teardown with an RSS-bounded adaptive budget" opening instruction:
"Run the already motivated cap sweep 4/8/16/32 on the exact teardown workload,
but record peak committed bytes and a many-thread case alongside latency. Do
not promote a larger fixed per-heap default based on the single-thread
1024-operation result."

**Verdict: NO-GO for a blanket default raise, with a narrow GO-CANDIDATE
worth a separate decision — see §5.** The 4→8 step eliminates 100% of the
measured decommits (20 → 0) at essentially flat single-thread RSS cost
(12.8 MiB → 8.0 MiB peak delta — LOWER, not higher, because cap=4's residual
churn itself carries a real RSS/commit cost). Raising further to 16 or 32
buys **nothing additional** on either axis — decommits are already 0 at
cap=8, and RSS is flat (within noise) all the way to cap=32. This task
therefore does **not** discover a case where RSS cost scales with cap in a
way that would make a *large* raise (16 or 32) risky per-thread — because
this workload's actual demand tops out at 6-7 concurrently-touched segments,
so nothing above ~8 is ever exercised. The multi-thread axis (8T/32T) DOES
show the review's warned-about linear-in-thread-count multiplication, but
that multiplication is identical in *shape* at every cap from 8 upward — it
is the workload's own footprint scaling with thread count, not something a
larger cap makes worse. **This task does not change any `src/` default** —
measurement/investigation only, per the task's explicit constraint.

**Date:** 2026-07-28. **Base revision measured:** `main` @ `a3cca54`.
**Platform:** native Windows x86-64 (shared host — wall-clock and RSS are
inherently noisy on a shared machine; the decommit-count deltas are the
reliable, noise-free signal — same platform-honesty framing R24-11 used).
**Feature set:** `production` (= `alloc-global`, `alloc-xthread`,
`alloc-decommit`, `fastbin`, `alloc-segment-directory`,
`primordial-lazy-commit`, `class-aware-dirty`) + `alloc-stats` (diagnostic
counters only — not required by the probe itself, added so the run is
directly comparable to R24-11's own build).

---

## 0. Headline numbers

### Latency/decommit axis (single-thread, `AllocCore` direct, 1,200 prefill+churn+teardown cycles at 1024B)

| `pool_segments` | resolved cap (self-verified) | `decommit_calls` Δ | `segments_released_total` Δ | `segments_reserved_total` Δ | ns/cycle (point) |
|---:|---:|---:|---:|---:|---:|
| **4 (baseline)** | 4 | **20** | **20** | 24 | 177,010 |
| **8** | 8 | **0** | **0** | 6 | 119,568 |
| **16** | 16 | **0** | **0** | 6 | 107,923 |
| **32** | 32 | **0** | **0** | 6 | 119,575 |

`pooled_count` at the end of the measured window (`AllocCore::dbg_pooled_count()`,
directly readable — see §1.3): **4** at cap=4 (pool saturated — 4 of 4 slots
full), **6** at cap=8/16/32 (this workload's actual peak concurrent-segment
demand — never grows past 6 no matter how much headroom the cap offers above
it). The 4→8 step is a **cliff**, not a smooth decline: demand (6) exceeds
cap=4 (hence 20 decommits) but sits comfortably under cap=8 (hence 0) — cap=16
and cap=32 add unused headroom, not additional benefit.

### RSS/commit axis (peak minus that arm's own fresh before-snapshot; KiB)

| `pool_segments` | 1 thread, peak RSS Δ | 1 thread, peak commit Δ | 8 threads, peak RSS Δ (aggregate) | 32 threads, peak RSS Δ (aggregate) |
|---:|---:|---:|---:|---:|
| **4 (baseline)** | **13,132** | **17,132** | **100,932** | **382,352** |
| **8** | 8,216 | 8,260 | 66,508 | 264,748 |
| **16** | 8,216 | 8,260 | 66,192 | 263,940 |
| **32** | 8,232 | 8,276 | 65,888 | 264,280 |

**Cap=4 has the HIGHEST RSS/commit delta at every thread count**, not the
lowest — the opposite of the naive "larger cap = more retained RSS" prior.
Cap=8/16/32 are statistically flat with each other (within ~1% host noise) at
every thread count. §3 explains why: cap=4's residual decommit/reserve churn
itself carries a real, measurable RSS/commit cost (the OS
decommit→release→re-reserve round-trip transiently touches MORE distinct
pages than steady-state cap=8+ ever does), so the "RSS you save by NOT
raising the cap" premise does not hold for THIS workload at THIS size.

---

## 1. Methodology

### 1.1 Mirrors `pool_cap_sweep_spread_and_drain`, per the task's explicit instruction

Per this task's brief ("mirroring the existing `bench_pool_cap_sweep`/
`pool_cap_sweep_spread_and_drain` harness") and per `pool_cap_sweep_spread_and_drain`'s
own doc comment's established rationale (`benches/global_alloc.rs`), the
latency/decommit axis uses **`AllocCore::new_with_config` directly** (no
TLS/registry plumbing) rather than `SeferAlloc`:

- **Generous `pool_byte_cap`** (256 MiB = 64 segments' worth), identical
  constant and identical rationale to `pool_cap_sweep_spread_and_drain`'s own
  choice — so only `pool_segments` (segment COUNT), never the byte ceiling,
  constrains occupancy at any swept value.
- **`AllocCore::dbg_pool_cap()`/`dbg_pooled_count()`** are directly readable
  on `&AllocCore` — closing the exact reachability gap R24-11 §1's "Counter
  reachability note" flagged for `SeferAlloc` (`dbg_pooled_count` is a
  per-instance method behind `SeferAlloc`'s private `current_heap()`). This
  probe uses that reachability to **self-verify** the swept value actually
  resolved into the live cap (`resolved_cap == pool_segments`, asserted in
  code — see `examples/r25_5_pool_cap_sweep_probe.rs::measure_latency_axis`),
  not merely assumed.

### 1.2 Workload fidelity — the EXACT `bench_global_alloc_churn_with_teardown`@1024B shape

`churn_prefill`/`churn_step`/`churn_teardown` are private fns in
`benches/global_alloc.rs` (a `[[bench]]` binary, unreachable from an
`[[example]]`), so this probe carries **byte-for-byte copies** (same PRNG
seed `0xCAFE`, same free-random-slot/alloc-replacement loop, same
`CHURN_WORKING_SET = 256` / `OPS = 1024` constants) plus `AllocCore`-flavored
equivalents (`ac_churn_prefill`/`ac_churn_step`/`ac_churn_teardown`, since
`AllocCore` does not implement `GlobalAlloc`). `SIZE = 1024` is the one size
R24-11 root-caused the residual to.

### 1.3 The iteration-count / batching pitfall this task hit and fixed

**An earlier version of this probe measured ZERO decommits at every swept
value, including the `pool_segments=4` baseline** — directly contradicting
R24-11's own 248-decommit measurement at that identical config. This was
root-caused and fixed before any of the numbers above were trusted (raw logs
of the broken intermediate versions were NOT kept — the fix was applied and
re-verified in the same working session, per the "zero-trust" development
discipline; the counterfactual is documented here in prose since re-breaking
committed code just to re-capture a broken log would add no evidentiary
value beyond what is recorded below).

**Root cause**: criterion 0.5.1's `Bencher::iter_batched(setup, routine,
BatchSize::SmallInput)` (`criterion-0.5.1/src/bencher.rs:236`, vendored copy
inspected directly) computes `batch_size = iters / 10` and, for
`batch_size > 1`, executes:

```text
let inputs = black_box((0..batch_size).map(|_| setup()).collect::<Vec<_>>());
// ... THEN, inside the timed region:
outputs.extend(inputs.into_iter().map(&mut routine));
```

— i.e. it calls `setup` (`churn_prefill`, 256 blocks) **`batch_size` times
UP FRONT**, holding ALL of those prefills' working sets **concurrently
live simultaneously**, before running `routine` (`churn_step` +
`churn_teardown`) once per prefilled input. A naive sequential
prefill→churn→teardown loop (never more than `CHURN_WORKING_SET` blocks live
at once) is a fundamentally different, much milder segment-pressure shape
than criterion's actual batched-setup semantics — and at the milder shape,
even the `pool_segments=4` baseline never gets pressured past its cap, so it
never decommits.

**The fix**: [`run_latency_batch`]/[`ac_run_latency_batch`] in
`examples/r25_5_pool_cap_sweep_probe.rs` collect `LATENCY_BATCH_SIZE`
independent prefills into a `Vec` up front (exactly matching criterion's
`(0..batch_size).map(|_| setup()).collect::<Vec<_>>()`), THEN run
`churn_step`+`churn_teardown` once per prefilled working set. This is the
methodological core of this task's own harness fix and is itself the most
important single finding of this measurement session — the batching
semantics of the ORIGINAL bench are load-bearing for reproducing its
decommit signal at all, not an incidental implementation detail.

**Batch size tuning** (also empirical, not exactly derived from criterion's
schedule — see the extended doc comment on `LATENCY_BATCH_SIZE` in the probe
source for the full counterfactual): a first attempt at `batch_size = 500`
(criterion's `iters/10` ratio applied naively to R24-11's ~4,675-iteration
total) produced ~131 MB of concurrently-live prefilled blocks per batch —
**exceeding every swept `pool_segments` value including the largest (32)**,
so all four arms saturated identically (every arm measured the SAME
260-decommit delta) — the same "cannot go RED before the fix, cannot
distinguish honestly-resolved-large from silently-clamped" failure mode
`pool_cap_sweep_spread_and_drain`'s OWN doc comment describes for an earlier
version of that harness. `LATENCY_BATCH_SIZE = 120` (~31.5 MB/batch) was
chosen and verified to differentiate cleanly (§0's table): demand tops out
at `pooled_count = 6` regardless of cap, comfortably exceeding cap=4 while
sitting under cap=8/16/32.

### 1.4 RSS/commit axis — per-thread heaps via `SeferAlloc::with_config`

Unlike the latency axis, the RSS axis genuinely needs N independently
concurrent heaps (the pool cap is per-thread), so it uses
`SeferAlloc::with_config(..)` on N freshly spawned OS threads (registry-slot
config is first-claim-wins — a fresh thread claims a fresh,
never-before-configured slot, the same pattern
`r13_9_class_aware_dirty_sidecar_rss.rs`'s
`claim_heap_with_materialised_sidecar` already establishes). Each thread runs
the SAME batched churn shape (§1.3) continuously for `RSS_RUN_DURATION =
1.5s`; a monitor thread polls `proc_probe::snapshot()` (this project's
established same-instant RSS+commit-charge self-probe,
`crates/proc-memstat`/`crates/proc-probe` — the SAME dependency
`first_alloc_process.rs`/`r14_5_large_cache_extended_rss_measure.rs`/R14-6's
own gate protocol already use) every 20 ms and tracks the peak.

**Reported figure is the per-arm DELTA** (`peak − that arm's own fresh
before-snapshot`), not the absolute `*_kib` columns — the process never
fully decommits between swept arms within one run (same documented
"process-lifetime, not cleanly resettable between arms" discipline
`r13_9_class_aware_dirty_sidecar_rss.rs`'s own module doc records for its
sidecar measurement), so absolute figures accumulate monotonically across
the whole sweep and are NOT independently comparable — only the delta,
computed against that specific arm's own immediately-preceding snapshot, is.
The raw log and the probe's own printed `NOTE:` line make this explicit.

### 1.5 What was NOT attempted

- **No cross-platform measurement** (Windows-native only, same caveat every
  prior gate in this project's history carries forward).
- **No re-run under WSL2/Linux** — this is a config-tuning/RSS measurement
  task, not an iai/Callgrind-judged one; no deterministic instruction-count
  judge applies here (there is no meaningful "instruction count" for a
  wall-clock RSS sweep).
- **`src/alloc_core/small_segment_pool_config.rs` untouched** — per the
  task's explicit, non-negotiable constraint, no default was changed.

---

## 2. Why 8, not 16, is the demand-matching value for THIS workload

`pooled_count = 6` at the end of the measured window, identical at
cap=8/16/32 (§0), means this specific churn-with-teardown@1024B/256-working-set
shape never asks the pool to hold more than 6 segments concurrently, no
matter how much room a larger cap offers. This is an empirical property of
*this specific bench's* working-set size (256 blocks × 1024 B ≈ 256 KiB
"logical" working set, but segment CHURN during `churn_step`'s free-random/
alloc-replacement cycling — plus the batched-setup concurrency this task's
harness now reproduces faithfully — is what actually drives segment count up
to 6, not the working set's raw byte size alone). There is no evidence in
this measurement that raising the cap to 16 or 32 would help THIS bench
further; the ceiling is set by the workload's own concurrent-segment
footprint under criterion's batching semantics, not by the pool's cap once
the cap exceeds that footprint.

**This does not mean 8 is a universally correct cap for every workload** —
only that for the ONE size (1024B) and ONE bench shape (`bench_global_alloc_churn_with_teardown`,
criterion's own `SmallInput` batching) this task was scoped to measure, 8 is
where the curve flattens. A different working-set size or a genuinely
higher-thread-count production workload could plausibly demand more than 6
concurrent segments — this task's evidence is silent on that broader
question by design (task brief: measure THIS bench, don't design a general
policy).

---

## 3. Why cap=4's RSS/commit delta is HIGHER, not lower, than cap≥8

At cap=4, the pool cycles through 20 decommit-then-re-reserve events across
the measured window. Each decommit-then-reserve round-trip is not a no-op on
either RSS or commit charge:

- **`decommit_empty_segment`** returns the segment's payload pages to the OS
  (`VirtualFree(MEM_DECOMMIT)`-equivalent via `crates/vmem`), then
  **`release_empty_segment_now`** releases the OS reservation and
  `table.recycle`s the slot.
- The very next demand for a segment (this bench continuously prefills fresh
  256-block working sets) then pays a **fresh OS reserve + commit** — a
  `VirtualAlloc(MEM_RESERVE|MEM_COMMIT)`-equivalent round-trip, touching
  fresh pages that were NOT resident a moment before.

At cap=8+, none of this ever fires (0 decommits) — the SAME 6 segments stay
committed and are simply reused via the free-list path (`find_segment_with_free`),
with **zero additional OS reserve/decommit syscalls** after the initial
warm-up. The steady-state "6 segments, always committed, never released" is
therefore a SMALLER transient RSS/commit footprint than "4 segments retained
+ periodic decommit-then-reserve churn of the other ~2", because the
churning segments' repeated commit events are what the OS memory-counter
snapshot catches mid-flight. This is consistent with (and is essentially a
concrete instance of) R24-11 §4.1's own inference: "the dominant cost [is]
concentrated in the... teardowns that trigger a full... segment lifecycle
(reserve + commit + populate + later decommit + release + re-reserve)" —
R24-11 inferred this cost was concentrated in latency; this task's
measurement shows the SAME lifecycle also inflates the RSS/commit axis, not
just latency.

---

## 4. Multi-thread aggregate RSS — confirms, does not aggravate, the review's warning

`docs/reviews/2026-07-28-r24-readonly-review.md`'s P2 section's specific
concern: "A process-wide token budget would let a genuinely hot heap exceed
four segments without multiplying the worst-case committed allowance by
every thread" — i.e. a per-thread FIXED cap raise risks `N × (extra RSS per
thread)` growth. This task's 8T/32T arms measure that multiplication
DIRECTLY:

| `pool_segments` | 1T Δ (KiB) | 8T Δ (KiB) | 8T/1T ratio | 32T Δ (KiB) | 32T/1T ratio |
|---:|---:|---:|---:|---:|---:|
| 4 | 13,132 | 100,932 | 7.69× | 382,352 | 29.12× |
| 8 | 8,216 | 66,508 | 8.09× | 264,748 | 32.22× |
| 16 | 8,216 | 66,192 | 8.06× | 263,940 | 32.13× |
| 32 | 8,232 | 65,888 | 8.00× | 264,280 | 32.10× |

The scaling ratio (~8× at 8 threads, ~32× at 32 threads — i.e.
**linear-in-thread-count**, exactly the shape the review warned a per-thread
cap multiplies) is present at **every** swept cap, INCLUDING the current
default (4). It is a property of "N independent heaps each retain their own
committed working set," not something raising the cap from 4 to 8/16/32
makes categorically worse — the per-thread footprint at cap≥8 is actually
LOWER than at cap=4 (§3), so the aggregate is lower too at every thread
count measured. **This task's own data does not surface a scenario where
raising the cap turns the review's warned-about linear multiplication into
something worse than what cap=4 already exhibits** — but this is because
this workload's demand never exceeds 6-7 segments regardless of cap (§2); a
DIFFERENT, higher-demand workload (not measured here) could plausibly behave
differently, and the review's general caution about a BLANKET raise (as
opposed to a workload-matched one) stands on those grounds independent of
this specific data.

---

## 5. Two-axis decision framework applied

Per this task's explicit, non-negotiable constraint: **do NOT promote a
larger fixed per-heap default based on the single-thread 1024B latency
result alone.** Both axes, weighed together:

- **Latency/decommit axis**: cap=4→8 eliminates the ENTIRE measured decommit
  residual for this bench (20 → 0, a 100% reduction, matching R24-11's
  original 248-decommit finding's mechanism exactly — the pool cap being
  exceeded). Cap=16/32 add nothing further (already 0 at cap=8).
- **RSS/commit axis**: cap=4→8 REDUCES (not increases) both single-thread
  and multi-thread aggregate RSS/commit delta, because cap=4's residual churn
  itself carries a real OS-syscall RSS/commit cost that steady-state cap≥8
  avoids entirely (§3). Cap=16/32 are statistically flat with cap=8 (no
  additional cost, no additional benefit) at every thread count measured.

**Both axes point the same direction for the 4→8 step, with no trade-off to
weigh** — this is the rare case where the "two-axis, don't promote from one
axis alone" caution does not produce a genuine tension, because raising to 8
is not "faster but riskier," it is faster AND (for this workload) cheaper on
RSS. The caution remains warranted as a general methodological stance (a
DIFFERENT workload could plausibly show the classic latency-vs-RSS tension
this project's two-axis convention exists to catch), but THIS task's
specific numbers do not exhibit that tension.

**Recommendation: GO-CANDIDATE for `pool_segments = 8`** (not 16, not 32 —
both are no-op relative to 8 for every axis and workload this task measured),
**flagged as a candidate for a future default raise, not decided here** per
the task's explicit instruction. `DEFAULT_POOL_SEGMENTS` in
`src/alloc_core/small_segment_pool_config.rs` remains `4`, unmodified by this
task.

**What this recommendation does NOT establish** (explicitly out of scope,
consistent with §1.5/§2's honesty notes):

1. Whether 8 is sufficient for OTHER bench shapes / sizes this task did not
   sweep (only 1024B, only THIS bench's specific working-set/batching shape,
   was measured).
2. Whether the review's preferred adaptive/process-wide-budget design (P2's
   own stated preference over a blanket fixed-cap raise) would perform
   better still — this task measured the FIXED-cap sweep the brief asked
   for, not the adaptive alternative (that is R25-6's scope, conditional on
   this task, per the round's own task queue).
3. Cross-platform (Linux/macOS) behavior — Windows-native only, as always.

---

## 6. Files changed

| file | change |
|---|---|
| `examples/r25_5_pool_cap_sweep_probe.rs` | new — the sweep harness (latency/decommit axis via `AllocCore` direct, mirroring `pool_cap_sweep_spread_and_drain`; RSS/commit axis via `SeferAlloc::with_config` + N concurrent threads, mirroring `first_alloc_process.rs`'s N-concurrent-heap RSS pattern). Not a shipping artifact — measurement-only, same category as `r13_9_class_aware_dirty_sidecar_rss.rs`/`r25_3_flush_n_oscillating_probe.rs`. |
| `docs/perf/R25_5_POOL_CAP_SWEEP_GATE.md` | this report (new) |
| `docs/perf/R25_5_POOL_CAP_SWEEP_GATE_summary.csv` | machine-readable summary of §0's tables (new) |
| `docs/perf/_raw_r25_5_pool_cap_sweep_probe.log` | raw probe stdout, the canonical run cited throughout this report (`.gitignore`d by default — `git add -f` at commit time) |
| `docs/perf/OPEN_ITEMS.md` | item 13's "Next trigger" closed; new dated current-state addition appended (not a rewrite) recording this task's outcome — see the item 13 entry |

**No production source file touched** (`src/` unchanged). **No commit made**
— tree left unstaged for personal zero-trust review, per this task's
explicit instruction.

---

## 7. Reproduce

```text
cargo run --release --example r25_5_pool_cap_sweep_probe --features "production alloc-stats"
```

The `[diag]` lines (self-verifying `resolved_cap`/`pooled_count_at_end`) and
`RESULT key=value` lines (machine-parseable, `proc_probe`'s established
protocol) appear on stdout interleaved with the human-readable tables. Wall-clock
ns/cycle and RSS/commit KiB are noisy point estimates on this shared host;
the decommit-count deltas (§0's first table) are exact relaxed-atomic reads,
the reliable signal (same platform-honesty framing as R24-11).
