# R32-8 (task #499) — `maybe_decay_large_cache`'s `Instant::now()` cliff: measured, confirmed, structurally fixed

Date: 2026-08-02.

landing_commit: 74345b8b3323f071b8bc45d38035163c3ac0ffef

## 0. What this is

This task tracks finding **F9** in
`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` ("the large-cache decay
tick's `Instant::now()` fast-path exit is a CLIFF keyed on `used >
headroom`, and the profiles R30-7/R31-9 just shipped are designed to put
workloads on the wrong side of it"). `AllocCore::maybe_decay_large_cache`
(`src/alloc_core/alloc_core_large_cache.rs`) has a fast-path guard: if
`large_cache_used_bytes <= headroom_bytes`, it returns immediately, skipping
a `std::time::Instant::now()` read (a `QueryPerformanceCounter` syscall on
Windows). If the cache is ABOVE headroom, the guard falls through and reads
the clock **unconditionally, on every call**, twice per steady-state large
alloc/free cycle (`alloc_core_large.rs`'s `alloc_large` and `alloc_core.rs`'s
Large `dealloc` branch).

The survey's concern: two shipped, non-default profiles —
`LargeCachePolicy::LowHeadroom` (16 MiB headroom) and
`LargeCachePolicy::Trimmed64MiB` (64 MiB headroom), `src/alloc_core/profile.rs`
— exist SPECIFICALLY to let a heap's working set sit ABOVE its headroom
during normal operation (that is what makes them RSS-saving vs the 256 MiB
`Default`). That means both profiles are, by design, on the "guard fails,
pay the clock read" side of this cliff for their entire intended use case —
an effect neither profile's doc comment disclosed.

**This task measured the effect with a confound-free A/B, confirmed it
reproduces at a real, non-trivial magnitude (~75-138 ns/call, ~150-275
ns/steady-state alloc+free cycle), and shipped the structural fix the survey
outlined: a monotonic op-counter that throttles how often the clock is even
consulted once past headroom.** The fix reduces `maybe_decay_large_cache`'s
own elapsed cost by ~62-73% in the exact above-headroom regime
`LowHeadroom`/`Trimmed64MiB` operate in, while preserving `dbg_force_decay_tick`'s
deterministic single-call-fires-one-tick contract that R29-13's
forced-convergence measurement and `tests/large_cache_decay.rs` both depend
on. `LowHeadroom`'s and `Trimmed64MiB`'s doc comments are updated to
disclose the (now-reduced, but nonzero) residual cost.

## 1. Design — confound-free A/B, per the survey's own recipe

**The problem with a naive headroom sweep.** R31-1
(`docs/perf/R31_1_LARGE_CACHE_HEADROOM_CROSSING_REGIME_GATE.md`) found that
changing `headroom_bytes` ALSO moves the large-cache hit rate by a real 12.5
percentage points at some boundaries. Comparing "headroom=256 MiB" against
"headroom=64 MiB" on the same workload therefore cannot attribute any
latency delta to the clock read alone — some of it could be a hit-rate
effect. CLAUDE.md's "cost and benefit must be measured in the SAME workload
regime" rule (the R30-6/R31-1 postmortem) applies directly.

**Design (a) — hold headroom FIXED, vary only whether the clock read
executes.** A new `bench-internals`-gated, process-wide switch,
`FORCE_DECAY_CLOCK_READ` (`AtomicBool`, `src/alloc_core/alloc_core.rs`,
toggled via `AllocCore::dbg_set_force_decay_clock_read`), makes
`maybe_decay_large_cache` skip its headroom fast-exit and always proceed to
the clock read — WITHOUT touching `headroom_bytes`. Two runs at the
IDENTICAL headroom, one with the switch off (real guarded behavior) and one
with it on (forced), differ ONLY in whether the clock read executes; hit
rate is structurally unchanged by construction (same headroom, same
workload, `run_decay_step`'s own `excess = used.saturating_sub(headroom)`
resolves the same way either way since headroom never moves). This is
design (a) from the survey's own recipe — the preferred/honest option, not
the weaker "report the hit-rate delta as a check" option (b).

**Path-activation oracle (R30-8 rule).** A new `bench-internals`-gated,
process-wide counter, `MAYBE_DECAY_GUARD_PASSED` (`AtomicU64`,
`src/alloc_core/alloc_core.rs`), counts calls that passed the fast-exit and
reached the clock read, read via
`AllocCore::dbg_maybe_decay_guard_passed_count()`. This is the instrument
that proves each arm actually differed in the intended mechanism, not just
in a label — see §2/§3 below for the exact assertions.

Two example harnesses were built, both subprocess-per-arm (fresh OS process
per cell, matching R30-6/R31-1/R29-13's established methodology),
registry-bypass via `HeapRegistry::claim_with_config`, self-verifying
resolved config + zero `config_conflicts_delta` per the R26-4 evidence rule:

1. **`examples/r32_8_large_cache_decay_clock_read_ab_gate.rs`** — isolates
   the RAW per-call clock-read cost. Headroom fixed at 4 MiB, workload
   (repeated 512 KiB alloc/dealloc cycles) never crosses it, so the REAL
   guard's fast-exit fires on (almost) every call; the "guard-forced" arm
   bypasses that fast-exit via the switch. This measures the clock read in
   isolation, independent of the stride-throttle fix (see §4 — the fix's
   stride throttle is itself bypassed whenever the switch is set, by
   design, so this gate measures the SAME raw cost before and after the fix
   landed).
2. **`examples/r32_8_large_cache_decay_stride_fix_gate.rs`** — validates the
   FIX's benefit in the regime it targets: headroom fixed at 64 KiB, one
   512 KiB object kept persistently resident so `used_bytes` genuinely and
   continuously exceeds headroom (the `LowHeadroom`/`Trimmed64MiB` regime).
   `forced=true` bypasses the NEW stride throttle (reproducing the OLD,
   pre-fix unconditional-clock-read-past-headroom shape exactly);
   `forced=false` exercises the real, shipped, stride-throttled path. Both
   arms verify `used_after_timed > headroom_bytes` throughout (the workload
   precondition).

## 2. Gate 1 result — the raw clock-read cost reproduces

Command: `cargo run --release --example r32_8_large_cache_decay_clock_read_ab_gate --features "production alloc-stats bench-internals"`.

7 repetitions/arm, subprocess-per-arm, median reported. Path-activation
oracle: `guard-real`'s `guard_passed_delta` must be 0 (fixed headroom the
workload never crosses); `guard-forced`'s must equal `expected_calls`
(400,000 = 200,000 cycles × 2 calls/cycle) exactly. **14/14 arms passed.**

| arm | median elapsed_ns (200,000 cycles) | ns/cycle | ns/call | guard_passed_delta |
|---|---:|---:|---:|---:|
| guard-real | 15,536,300 | 77.68 | 38.84 | 0 |
| guard-forced | 45,010,000 | 225.05 | 112.53 | 400,000 |

**Headline: 147.37 ns/cycle = 73.68 ns/`maybe_decay_large_cache` call**
(this run — the one cited in `_raw_r32_8_clock_read_ab_gate.log` and the
summary CSV; see the reproducibility note below). Consistent with task
#95's own historical anchor (~105 ns/call, ~2.3x the ~45 ns cache-hit
baseline it was measured against) — same order of magnitude, same
direction, on different hardware/build.

**Reproducibility across 5 independent runs of this same gate** (raw logs
not all individually committed — this table transcribes each run's own
printed headline, cited for the record; the summary CSV/committed raw log
cite run 5, the FINAL run captured against the exact clippy-clean commit
state):

| run | ns/cycle | ns/call |
|---|---:|---:|
| 1 | 148.52 | 74.26 |
| 2 | 152.01 | 76.01 |
| 3 (elevated system noise — concurrent unrelated builds) | 275.86 | 137.93 |
| 4 | 149.54 | 74.77 |
| 5 (cited, `_raw_r32_8_clock_read_ab_gate.log`, summary CSV) | 147.37 | 73.68 |

Four of five runs cluster tightly around ~74-77 ns/call; run 3 was captured
while several unrelated `cargo build`/`cargo test` processes for OTHER
projects were competing for CPU on this shared machine (confirmed via
`tasklist`) — cited here for honesty, not excluded, since it still shows
the same DIRECTION (forced > real) even under noise, just a noisier
magnitude. The oracle passed cleanly in all 5 runs (`guard_passed_delta`
exactly 0 vs exactly `expected_calls`, every time) — confirming the two
arms differed only in the intended mechanism regardless of the noise level.

**Verdict: the effect reproduces at a real, non-trivial magnitude. Proceed
to the structural fix (§4).**

## 3. Gate 2 result — the fix's benefit in the regime it targets

Command: `cargo run --release --example r32_8_large_cache_decay_stride_fix_gate --features "production alloc-stats bench-internals"`.

7 repetitions/arm, subprocess-per-arm, median reported. Workload
precondition (`used_after_timed > HEADROOM_BYTES` i.e. `stayed_above_headroom
== true`) verified on every one of 14/14 arms. Path-activation oracle:
`old-shape`'s `guard_passed_delta` must equal `expected_calls` exactly (the
stride throttle is bypassed); `new-shape`'s must be nonzero (decay logic
still runs occasionally) AND materially below `expected_calls` (below
`expected_calls / 4`, checked by the summary script's own assertion).
**14/14 arms passed.**

| arm | median elapsed_ns (200,000 cycles) | ns/cycle | ns/call | guard_passed_delta |
|---|---:|---:|---:|---:|
| old-shape (pre-fix behavior, `forced=true`) | 46,573,400 | 232.87 | 116.43 | 400,000 |
| new-shape (real, shipped, `forced=false`) | 18,141,600 | 90.71 | 45.35 | 3,125 |

**Headline: old-shape − new-shape = 142.16 ns/cycle = 71.08 ns/call, a
61.0% reduction in `maybe_decay_large_cache`'s own elapsed contribution to
this workload.** `guard_passed_delta` drops from 400,000 to 3,125 — a
**128×** reduction in clock reads, not the naively-expected 64× (the
`DECAY_CLOCK_CHECK_STRIDE` value) — see §3.1 for why.

**Reproducibility across 3 runs:**

| run | old-shape ns/cycle | new-shape ns/cycle | delta ns/call | % reduction |
|---|---:|---:|---:|---:|
| 1 | 232.11 | 87.33 | ~72.4 | 62.4% |
| 2 | 341.58 | 93.30 | 124.14 | 72.7% |
| 3 (cited, `_raw_r32_8_stride_fix_gate.log`, summary CSV) | 232.87 | 90.71 | 71.08 | 61.0% |

All three runs show a large, real, reproducible reduction (61-73% of
old-shape elapsed time attributable to this one function); the exact
percentage varies with ambient system noise (the same shared-machine
contention noted in §2), but the DIRECTION and ORDER OF MAGNITUDE are
stable across all three runs and both gates. `guard_passed_delta` is
byte-identical (400,000 / 3,125) across all runs — the MECHANISM evidence
is completely noise-free even though the wall-clock TIMING evidence has
some run-to-run variance from shared-machine contention.

### 3.1 Why the reduction is ~128×, not ~64× — a mechanism note, not a discrepancy

`DECAY_CLOCK_CHECK_STRIDE = 64` throttles clock reads to 1-in-64 among calls
that reach the throttle check. But in THIS gate's single-resident-object
workload, only HALF of the 400,000 total `maybe_decay_large_cache`
invocations ever reach that check at all: the cached object is
exclusively-owned at any instant — resident in the cache (`used >
headroom`) between ops, but REMOVED from the cache the instant
`alloc_large`'s hit path takes it out, and not yet re-deposited until the
matching `dealloc` completes. Concretely: `alloc_large`'s guard check always
sees `used > headroom` (the prior cycle's `dealloc` just redeposited it),
but the Large-`dealloc` branch's guard check always sees `used == 0` (the
object is mid-flight, not yet redeposited) — so only the ALLOC-side call of
each cycle ever organically passes the headroom check, halving the
population the stride throttle even applies to before the 1-in-64 stride is
layered on top. Net: 200,000 alloc-side calls × 1/64 ≈ 3,125 — exactly what
was measured. This is a property of this gate's SPECIFIC single-object
workload shape, not a general claim about the stride's real-world
reduction factor on a workload with multiple concurrently-resident cached
objects (there, both alloc and dealloc calls would more often see `used >
headroom`, and the reduction would trend closer to the nominal 64×).

## 4. The structural fix

Per the survey's own recipe (§"What would be needed to capture it", step 2):
**a cheap monotonic op-counter throttles how often the clock is even
consulted, once past the headroom fast-exit.**

`src/alloc_core/alloc_core.rs` adds `AllocCore::large_cache_decay_op_count:
u32` (initialized to 0). `src/alloc_core/alloc_core_large_cache.rs` adds:

```text
const DECAY_CLOCK_CHECK_STRIDE: u32 = 64;
```

`maybe_decay_large_cache`'s body, past the headroom fast-exit:

```text
self.large_cache_decay_op_count = self.large_cache_decay_op_count.wrapping_add(1);
if !forced
    && self.large_cache_decay_op_count % DECAY_CLOCK_CHECK_STRIDE != 0
    && self.last_decay_tick.is_some()
{
    return; // not yet due for a clock check this stride
}
// ... reset counter to 0, THEN read the clock ...
```

**Semantic trade, stated explicitly (per the survey's own requirement):**
this trades DECAY-TICK GRANULARITY for fewer clock reads. A decay tick that
becomes due can now fire up to `DECAY_CLOCK_CHECK_STRIDE - 1` (63) large ops
LATE — i.e. up to that many alloc/dealloc calls after the `decay_interval`
wall-clock deadline technically passed — instead of firing on the very next
call as before. It can NEVER fire EARLY (the stride only delays a clock
read, never fabricates elapsed time), so decay is never MORE aggressive
than the pre-fix behavior, only, at most, slightly less prompt. At the
default 1-second `decay_interval`, 63 large ops is a small sliver of time
on any workload with meaningful large-op throughput (exactly the regime
`LowHeadroom`/`Trimmed64MiB` are chosen for); on a sparse-large-op workload
the guard was already mostly idle-triggered (R29-13's own finding: idle
time never even calls `maybe_decay_large_cache` at all), so the added delay
changes little in practice.

**The FIRST-EVER clock-priming call is NOT throttled** —
`self.last_decay_tick.is_some()` is part of the skip condition, so a fresh
heap's very first past-headroom call always falls through to prime the
timer immediately, exactly as before. Delaying that prime would let an
unbounded number of large ops pass before decay logic engages at all on a
cold heap, a materially different (and undesirable) semantic than "ticks
may fire a little late."

**`dbg_force_decay_tick` (tests/R29-13's forced-convergence loop) explicitly
bypasses the stride.** This function is used by `tests/large_cache_decay.rs`
and R29-13's forced-convergence measurement
(`docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md` §1.6), both of which
depend on EVERY call reliably firing exactly one real decay tick — not
whichever call happens to land on a stride boundary by chance. It now
primes `large_cache_decay_op_count = DECAY_CLOCK_CHECK_STRIDE - 1`
immediately before calling `maybe_decay_large_cache`, so the
`wrapping_add(1)` inside lands exactly on a stride boundary and the clock
read is guaranteed on that call — preserving the pre-existing "safe to call
multiple times — each call produces exactly one decay step" contract
byte-for-byte.

**The `bench-internals` measurement switch bypasses the stride too** (by
design — `forced` short-circuits the stride condition the same way it
short-circuits the headroom fast-exit), which is exactly what lets Gate 1
(§2) measure the SAME raw per-call clock-read cost both before and after
this fix landed, and what lets Gate 2 (§3) reproduce the OLD shape on
demand for a clean before/after comparison without a source-level
before/after diff or worktree.

## 5. Correctness verification

- **`tests/large_cache_decay.rs`** (5 tests: `decay_releases_excess_over_target`,
  `decay_respects_headroom`, `decay_skips_when_under_target`,
  `decay_interval_respected`, `config_decay_rate_percent`) — **all pass**
  under `--features production` (release). These tests call
  `dbg_force_decay_tick` a small, fixed number of times per test and assert
  each call's effect deterministically; the stride-bypass in §4 is exactly
  what keeps them passing unchanged.
- **`tests/dbg_hook_safety_tripwire.rs`** — **all 7 pass**. Confirms the two
  new `dbg_*` hooks (`dbg_maybe_decay_guard_passed_count`,
  `dbg_set_force_decay_clock_read`) are not flagged as an unreviewed unsafe
  surface — both are zero-argument (or bool-argument) plain atomic
  accessors, matching the project's established `PURE_OBSERVERS`-style
  sanctioned shape, no raw pointer, no allocator-metadata mutation via
  caller-supplied pointer.
- **`cargo test --release --features "production large-cache-extended"`** —
  full suite run; every test passed EXCEPT one **pre-existing, unrelated**
  flake, independently confirmed to reproduce on the commit immediately
  BEFORE this task's changes (see §6).
- `cargo check` clean under `production`, `production bench-internals`, and
  `--all-features`.

## 6. Pre-existing flaky test discovered during verification (not caused by this task)

`tests/regression_xthread_large_free_layout_mismatch.rs`'s
`xthread_large_free_tiny_size_huge_align_is_reclaimed` failed when run
alongside its 4 sibling tests in the same binary (`delta 0` — a legitimate
cross-thread free was not reclaimed) but passed reliably in isolation (3/3
runs). **Confirmed pre-existing and unrelated to this task's diff:**
reproduced identically on a clean `git worktree add` at commit
`48fed64355f03181c6a89f42cab636b800994c7f` (the commit immediately before
this task's changes), with its own isolated `CARGO_TARGET_DIR`, ruling out
both this task's source diff and cross-contamination from other
concurrently-running agents' builds in this shared workspace. Filed as item
14 in `docs/CORRECTNESS_OPEN_ITEMS.md` per this project's mandatory
correctness-open-items convention. Not investigated further (out of this
task's scope — a decay-guard perf fix, not a cross-thread-reclaim
correctness bug); a future round picks up the root cause from that item.

## 7. Doc-comment disclosure

`src/alloc_core/profile.rs`'s `LargeCachePolicy::LowHeadroom` and
`::Trimmed64MiB` doc comments now disclose the clock-read cost as an
additional axis, alongside their pre-existing (R31-9/R31-1-maintained)
RSS-vs-hit-rate tradeoff documentation, citing this report's measured
magnitude and the fix that reduces (but does not eliminate) it.

## 8. Files changed

- `src/alloc_core/alloc_core.rs` — `MAYBE_DECAY_GUARD_PASSED` (path-activation
  oracle counter), `FORCE_DECAY_CLOCK_READ` (measurement-only override
  switch), both `bench-internals`-gated; `large_cache_decay_op_count: u32`
  field + its `AllocCore::new_with_config` initializer.
- `src/alloc_core/alloc_core_large_cache.rs` — the fix
  (`DECAY_CLOCK_CHECK_STRIDE`, the stride-throttle logic in
  `maybe_decay_large_cache`, the `dbg_force_decay_tick` stride-bypass), plus
  the two new `dbg_*` accessors (`dbg_maybe_decay_guard_passed_count`,
  `dbg_set_force_decay_clock_read`).
- `src/alloc_core/profile.rs` — doc-comment disclosure on `LowHeadroom` /
  `Trimmed64MiB` (see §7).
- `examples/r32_8_large_cache_decay_clock_read_ab_gate.rs` (new) — Gate 1
  (isolation A/B).
- `examples/r32_8_large_cache_decay_stride_fix_gate.rs` (new) — Gate 2 (fix
  validation, above-headroom regime).
- `Cargo.toml` — registers both new examples with `required-features`.
- `scripts/r32_8_decay_clock_read_summary.mjs` (new) — the checked
  summary-derivation script.
- `docs/perf/_raw_r32_8_clock_read_ab_gate.log`,
  `docs/perf/_raw_r32_8_stride_fix_gate.log` (new, committed with `git add
  -f` per the raw-log policy) — cited raw evidence.
- `docs/perf/R32_8_LARGE_CACHE_DECAY_CLOCK_READ_GATE_summary.csv` (new) —
  machine-readable companion to this report.
- `docs/CORRECTNESS_OPEN_ITEMS.md` — item 14 (the pre-existing flaky test
  discovered in §6).
- `docs/perf/OPEN_ITEMS.md` — F9 marked resolved (see that file's own entry
  for the exact wording).

---

## 9. R33-6 (task #511) — retention cost measured in the low-throughput regime

Date: 2026-08-03. This section is an APPEND-ONLY addendum — §0–§8 above are
unchanged. It closes the gap identified by
`docs/reviews/2026-08-03-round32-readonly-review.md` §7 finding F9 [P2]:
"the benefit (ns/call saved) is measured in a high-throughput regime, the
cost (retention) is asserted for a low-throughput regime and never
instrumented." This section measures that cost directly.

### 9.0 What this measures

The stride throttle (`DECAY_CLOCK_CHECK_STRIDE = 64`, §4) trades decay-tick
promptness for fewer clock reads. A decay tick that becomes due can now fire
up to 63 large ops LATE instead of on the very next call. Since decay is
EVENT-DRIVEN ONLY (fires only on a large alloc/free, never from idle — §3 of
R29-13's own finding), a workload that crosses the headroom and then performs
FEWER THAN 64 further large ops now retains cached spans that the pre-change
code would have released on the very next op. §4 argued this away
qualitatively; this section instruments it.

### 9.1 Methodology — R29-13's retention harness + R32-8's FORCE switch

Adapts R29-13's proven subprocess-per-arm retention methodology
(`examples/r29_13_large_cache_retention_gate.rs`) with R32-8's own
`FORCE_DECAY_CLOCK_READ` A/B switch
(`examples/r32_8_large_cache_decay_stride_fix_gate.rs`):

1. Claim a heap with a profile's headroom (`LowHeadroom` = 16 MiB,
   `Trimmed64MiB` = 64 MiB — the two non-default profiles §4's fix targets,
   resolved via `LargeCacheConfig::new().headroom_bytes(n)`, self-verified
   via `dbg_decay_config()`).
2. Fill 8 × 34 MiB objects (touching every 4 KiB page), free them all →
   cache holds ~288 MiB, well above both headrooms (same fill as R29-13).
3. Sleep 1100 ms (> 1000 ms default `decay_interval`) so a decay tick is
   genuinely "due."
4. Set `FORCE_DECAY_CLOCK_READ` = forced (true = old/unthrottled shape,
   false = new/throttled shape).
5. Record `dbg_large_cache_used()` and `dbg_maybe_decay_guard_passed_count()`.
6. Perform exactly `n_ops` alloc+free cycles (each takes one cached span out
   and puts it back — the sparse-large-op workload).
7. Re-measure both.

`forced=true` bypasses both the headroom fast-exit AND the stride throttle
(by design — §4), reproducing the OLD unconditional-clock-read-past-headroom
shape byte-for-byte. `forced=false` is the real shipped stride-throttled path.
Both arms use the identical headroom and workload, so the comparison is clean:
only the stride differs.

**Path-activation oracle (R30-8 rule), two pieces per arm:**

1. **Headroom crossed:** `used_before_ops > headroom_bytes` — proves the arm
   genuinely entered the above-headroom regime the stride applies to. Hard-
   asserted in every child; `headroom_crossed == 1` in all 48 arms.
2. **Stride mechanism exercised:** `guard_passed_delta` (clock reads during
   the N ops) is materially lower for `forced=false` than for `forced=true`.
   For `forced=true`, `guard_passed_delta == expected_calls` exactly (every
   call reads the clock). For `forced=false`, it is `0` (n_ops ≤ 8, stride
   never aligned) or a small nonzero value (n_ops ≥ 32, stride aligned once
   or twice).

**Config-sweep evidence (R26-4 rule):** every child self-verifies
`verified_headroom == headroom_bytes` AND `config_conflicts_delta == 0` (fresh
process ⇒ first claim is unconditionally the arm's config). All 48/48 passed.

### 9.2 Results — cost (this section) and benefit (re-cited from §3) side by side

Per CLAUDE.md's "cost and benefit must be measured in the SAME workload
regime" rule: the two axes below were measured in DIFFERENT regimes by design
(benefit in high-throughput, cost in low-throughput), and are presented
TOGETHER here so the reader sees both sides of the trade — NOT combined into
a single "net win/loss" Pareto claim (which would violate that rule).

#### Retention cost (measured here, low-throughput regime)

Median of 3 repetitions per cell. `retention_cost = median(unforced used_after)
- median(forced used_after)`. Higher = more bytes the throttled arm retains
that the unthrottled arm would have released.

| profile | n_ops | used_before (MiB) | forced used_after (MiB) | unforced used_after (MiB) | retention_cost (MiB) | forced guard_delta / expected | unforced guard_delta / expected |
|---:|---:|---:|---:|---:|---:|---:|---:|
| LowHeadroom | 1 | 288.00 | 252.00 | 288.00 | **36.00** | 2/2 | 0/2 |
| LowHeadroom | 8 | 288.00 | 252.00 | 288.00 | **36.00** | 16/16 | 0/16 |
| LowHeadroom | 32 | 288.00 | 252.00 | 252.00 | 0.00 | 64/64 | 1/64 |
| LowHeadroom | 63 | 288.00 | 252.00 | 252.00 | 0.00 | 126/126 | 2/126 |
| Trimmed64MiB | 1 | 288.00 | 252.00 | 288.00 | **36.00** | 2/2 | 0/2 |
| Trimmed64MiB | 8 | 288.00 | 252.00 | 288.00 | **36.00** | 16/16 | 0/16 |
| Trimmed64MiB | 32 | 288.00 | 252.00 | 252.00 | 0.00 | 64/64 | 1/64 |
| Trimmed64MiB | 63 | 288.00 | 252.00 | 252.00 | 0.00 | 126/126 | 2/126 |

**All numbers byte-identical across 3 repetitions per cell (zero run-to-run
variance) — the mechanism evidence is exact, not noisy.** 48/48 arms passed
the path-activation oracle, config self-verification, and admission assertion.

#### Latency benefit (re-cited from §3, high-throughput regime)

| arm | median elapsed_ns (200,000 cycles) | ns/cycle | ns/call |
|---|---:|---:|---:|
| old-shape (`forced=true`) | 46,573,400 | 232.87 | 116.43 |
| new-shape (`forced=false`) | 18,141,600 | 90.71 | 45.35 |

Benefit: **71.08 ns/call saved** (61.0% reduction), measured in the
high-throughput regime (200k alloc/free cycles, R32-8 §3, cited run 3).

### 9.3 What the numbers say

**The retention cost is REAL and BOUNDED.** For `n_ops` = 1 and 8 (fewer
than ~29 ops after the decay interval elapses), the stride throttle prevents
any clock read during those ops, so NO decay tick fires. The throttled arm
retains the full 288 MiB cache, while the unthrottled arm fires decay on its
very first post-interval call and releases exactly one 36 MiB segment (the
decay step's `evict_at_least` releases whole segments — one 36 MiB span
satisfies the 10%-of-excess target). The retention cost is exactly one
segment = **37,748,736 bytes = 36.00 MiB**, identical for both profiles and
across all 3 repetitions.

For `n_ops` = 32 and 63, the throttled arm catches up: the op counter (which
started at ~7 after the fill) reaches the stride boundary 64 during those
ops, a clock read happens, elapsed ≥ interval, and one decay tick fires.
Both arms converge to 252 MiB. The retention cost drops to **0**.

**The cost is bounded by one segment per missed decay interval.** Decay only
fires once per `decay_interval` (1000 ms default): after the first tick
updates `last_decay_tick`, subsequent clock reads within the same interval
find `elapsed < decay_interval` and do nothing. So even with `n_ops` = 63
(where the throttled arm reads the clock twice), only one decay tick fires
per interval in either arm — the throttle delays the tick, it does not skip
it entirely across multiple intervals.

**The stride-alignment threshold (~29 ops) is workload-specific.** After the
8-object fill + teardown, the op counter rests at ~7 (the first-ever call
past headroom resets it to 0, then the remaining 7 dealloc calls increment
it without a clock read). Each alloc+free cycle increments the counter by 2
(both the alloc-side and dealloc-side calls pass the headroom guard in this
8-cached-span workload). The counter reaches the stride boundary 64 at
`7 + 2 * n_ops ≥ 64`, i.e. `n_ops ≥ 29`. This is why `n_ops = 1` and `8`
show the full 36 MiB cost while `n_ops = 32` and `63` show zero — the
threshold falls between the 8 and 32 arms. A workload with a different fill
shape (more or fewer deallocs) would have a different starting counter and
therefore a different threshold, but the BOUND on the cost (one segment per
missed interval) is invariant.

### 9.4 Path-activation oracle results

All 48/48 arms passed both oracle checks:

1. `headroom_crossed == 1` in every arm (the workload genuinely entered the
   above-headroom regime).
2. For `forced=true`: `guard_passed_delta == expected_calls` exactly in every
   arm (2/2, 16/16, 64/64, 126/126 — every call read the clock, confirming
   the old-shape reproduction is exact).
3. For `forced=false`: `guard_passed_delta < expected_calls` in every arm
   (0/2, 0/16, 1/64, 2/126 — the stride throttle is reducing clock reads,
   not a no-op).

The derive script
(`scripts/r33_6_decay_throttle_retention_summary.mjs`) ASSERTS all three
conditions from the raw log — a failure is a `throw`, not a printed claim.

### 9.5 Conclusion — the cost is real, bounded, and vanishes with enough ops

The review's concern was correct in direction: a workload that crosses the
headroom and performs fewer than ~29 further large ops (the stride-alignment
threshold for this workload shape) retains one full 36 MiB cached segment
that the pre-change code would have released on the very next op. The cost
is not zero.

But the cost is also bounded and transient:

- **Bounded by one segment per missed decay interval** — the throttle delays
  one tick, it does not skip it across multiple intervals. A subsequent burst
  of large ops (or the next decay interval's tick) releases the same bytes.
- **Headroom-independent** — the 36 MiB cost is identical for both
  `LowHeadroom` (16 MiB) and `Trimmed64MiB` (64 MiB), because the cost is one
  whole-segment eviction, and the segment size (36 MiB) is determined by the
  workload's object size (34 MiB), not by the headroom.
- **Vanishes once enough ops accumulate** — at `n_ops ≥ 29` (the stride
  threshold for this workload), the throttled arm fires its delayed tick and
  converges to the same 252 MiB as the unthrottled arm.

This does NOT change the original §4 GO decision: the benefit (71 ns/call
saved in the high-throughput regime, applied to every large alloc/free above
headroom) is a recurring per-call cost reduction, while the retention cost
(36 MiB per missed interval, only in the sub-29-op regime) is a one-time
delay per decay interval. But the cost is now MEASURED, not argued — which
is all the review asked for.

### 9.6 Immutable source identity

Measured on commit `5bd7c04c392aa33fbc2e31362107f75153c33c20` (this task's
measurement-code commit on `main`, base
`b3b18bb637855cf77ec42f317be0a196ca0739bb`). The commit adds only the example
file + Cargo.toml registration — no production source changed. Per CLAUDE.md's
R29-6 rule: the measurement commit SHA is a permanent git object, resolvable
via `git show 5bd7c04` or `git log` for as long as the branch exists.

**Platform:** native Windows 10 Pro x86-64, Intel Core i7-11800H.
**Feature set:** `production alloc-stats bench-internals`.

### 9.7 Files added by this task

- `examples/r33_6_decay_throttle_retention_cost_gate.rs` (new) — the
  subprocess-per-arm retention-cost probe.
- `Cargo.toml` — `[[example]]` entry for the new example.
- `scripts/r33_6_decay_throttle_retention_summary.mjs` (new) — the checked
  derive script (reads raw log, asserts headline numbers, writes CSV).
- `docs/perf/_raw_r33_6_decay_throttle_retention_cost_gate.log` (new,
  `git add -f`) — cited raw evidence.
- `docs/perf/R33_6_DECAY_THROTTLE_RETENTION_COST_summary.csv` (new) —
  machine-readable companion to this section.

No production source changed. `DECAY_CLOCK_CHECK_STRIDE` remains 64.
