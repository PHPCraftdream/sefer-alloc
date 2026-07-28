# R25-3 — FLUSH_N sweep (4 / 8 / 12 / 16) at fixed TCACHE_CAP=16: NO-GO

**Task #397 (R25-3), Round 25.** A performance EXPERIMENT sweeping the
magazine-overflow half-flush constant `FLUSH_N`
(`src/registry/tcache.rs:124`, currently `TCACHE_CAP / 2 = 8`) across
`{4, 8, 12, 16}` with `TCACHE_CAP` held fixed at 16, gated on all 5 of the
brief's required measurements (in-context Ir bulk-free sweep, free-then-
realloc burst, oscillating live-set boundary stress, ordinary interleaved
churn regression check, and a non-Ir refill-count/hit-rate cross-check).
**Verdict: NO-GO for every swept value — the current `FLUSH_N = 8` baseline
remains the best of the four measured points.** `FLUSH_N = 16` (full flush,
no compaction) shows the ONLY gate-1 win (−1.5% at N=1024) but triggers the
brief's explicit kill condition on gate 3 (a 2.42× Ir regression and a 20×
refill-count regression on the oscillating boundary-stress scenario).
`FLUSH_N = 4` and `FLUSH_N = 12` show no gate-1 win at all (+14.4% and +0.7%
respectively) with no compensating gain elsewhere. **All `src/` production
code is reverted — `git diff HEAD -- src/` is empty.** This is the THIRD
NO-GO in this exact code region this round-cluster (after R24-3's
flush_magazine_class merge and R24-4's bulk-mask primitives), confirming the
region is unusually resistant to the kind of arithmetic-ceiling-driven change
this task deliberately guarded against repeating.

**Date:** 2026-07-28. **Base revision measured:** `main` @
`667cfe76a654a90dbe5101241189c56c532dc3e1` (HEAD; R25-1/R25-2's fixes).
**Platform measured:** WSL2 (Ubuntu, kernel `6.18.33.2-microsoft-standard-WSL2`)
under Windows 10 Pro x86-64, `valgrind 3.22.0`, `iai-callgrind-runner 0.14.2`,
WSL rustc `1.98.0-nightly (bd08c9e71 2026-06-25)` for gates 1/2/3/4 (Ir); the
gate-5 refill-count/hit-rate cross-check ran natively on Windows (release
profile, MSVC toolchain) since it reads a process counter, not an
instruction-count judge.

---

## 0. Headline verdict table

| FLUSH_N | gate 1 (Ir/free @ N=1024) | Δ vs. baseline | gate 2 (realloc burst Ir) | gate 3 (oscillating Ir) | gate 3 (refill count / 20 rounds) | gate 4 (interleaved churn) | verdict |
|---:|---:|---:|---:|---:|---:|---|---|
| **4**  | 123.00 | **+14.4%** | 2,334 | 25,845 | 1 | unchanged (8,051) | **NO-GO** — gate 1 regresses badly |
| **8**  (current) | 107.50 | — (baseline) | 2,588 | 25,719 | 1 | unchanged (8,051) | **BASELINE** |
| **12** | 108.24 | +0.7% | 2,913 | 27,761 | 2 | unchanged (8,051) | **NO-GO** — no win, gate 2/3 both worse |
| **16** | 105.84 | **−1.5%** | 3,033 | **62,183** | **20** | unchanged (8,051) | **NO-GO** — gate 3 kill condition (2.42× Ir, 20× refills) |

**No value beats the baseline once all 5 gates are weighed together.** The
sole apparent win (`FLUSH_N=16`'s −1.5% on gate 1) is more than paid back by
gate 3's refill-thrash catastrophe — exactly the trade-off pattern the task
brief's kill condition was written to catch.

---

## 1. Method

### 1.1 What was swept, and how

`FLUSH_N` (`pub(crate) const`, `src/registry/tcache.rs:124`) was hand-edited
to each of `{4, 12, 16}` in turn (8 is the pre-existing baseline), with a
full `npm run iai` run (60 benches, `--features production`, the CI default)
executed after each edit, then the constant was restored to its original
`TCACHE_CAP / 2` form. `TCACHE_CAP` was never touched. `git diff HEAD --
src/registry/tcache.rs` is empty after the sweep (confirmed in §6).

`FLUSH_N = 16` is the interesting boundary case: `remaining = TCACHE_CAP -
FLUSH_N = 0`, so the post-flush compaction loop
(`heap_core_free.rs:781-783`, `for i in 0..remaining`) is a genuine
zero-iteration no-op — no special-casing was needed for that loop itself.
**However, this sweep point exposed a real, independent, previously-latent
bug** — see §5.

### 1.2 Gate 1 — in-context Ir judge for bulk free (N = 17, 32, 64, 256, 1024)

New shared-prefix-subtraction bench arms in `benches/perf_gate_iai.rs`,
mirroring R24-2's established `dealloc_free_only_16b_nNN` pattern but widened
to reach N=1024: `PREFIX_OPS = 1088` (a multiple of 16, so the shared
`dealloc_prealloc_only_1088_16b` prefix always resets the magazine to
`count == 0` and every sweep point's free loop starts from the same known
state, exactly as R24-2 §1.2 established for its own 64-op prefix).
`free_cost(N) = Ir(dealloc_free_only_1088_16b_nNN) − Ir(dealloc_prealloc_only_1088_16b)`
cancels the shared alloc prefix exactly, at whatever `FLUSH_N` the tree is
compiled with.

### 1.3 Gate 2 — free-then-immediate-reallocate burst (N=17)

`dealloc_realloc_burst_1088_16b_n17`: frees the first 17 pre-allocated
blocks (exactly one overflow event under the baseline FLUSH_N=8 — mirrors
R24-2's n17 point), THEN immediately re-allocates 17 blocks of the same
class, all inside the SAME timed region. `Ir(this) − Ir(prefix)` measures
the full free-then-realloc round trip's cost — a smaller `FLUSH_N` retains
more just-freed blocks resident in the magazine (more potential LIFO hits on
the immediate realloc that follows); a larger `FLUSH_N` (up to 16, emptying
the magazine completely) retains fewer or none, so the reallocs are more
likely to miss and pay `refill_magazine_slow`'s cold-path cost instead.

### 1.4 Gate 3 — oscillating live-set size (8..24, crossing TCACHE_CAP=16)

`oscillating_live_set_16b`: an untimed warm-up brings the live set to 8, then
the TIMED region runs 20 rounds of "grow to 24 (16 allocs, popping the
magazine and/or triggering the overflow-flush-then-refill cycle), shrink back
to 8 (16 frees, pushing into the magazine and potentially firing the
overflow branch)". This directly stresses the boundary a `FLUSH_N` change
could shift: how many times a growth phase re-triggers
`refill_magazine_slow` (the `#[cold] #[inline(never)]` miss path) depends on
how many blocks the PRECEDING shrink phase's overflow-flush left resident.

### 1.5 Gate 4 — ordinary interleaved churn must not regress

The pre-existing `small_churn_16b`, `medium_class_dealloc_churn_16b`,
`aligned_churn_640b_a128`, `churn_256b`, `churn_write_256b` arms (unmodified)
were re-measured at every sweep point. None of these fire the magazine
overflow branch at all (R24-2 §4.6's finding: the interleaved alloc-then-
immediately-free shape never lets `count` exceed 1), so `FLUSH_N` should have
zero effect on them — this gate exists to VERIFY that assumption rather than
take it on faith, per the task brief.

### 1.6 Gate 5 — non-Ir cross-check (refill count / tcache hit rate)

`examples/r25_3_flush_n_oscillating_probe.rs`: a new, minimal example
reusing the EXISTING `alloc-stats`-gated `SeferAlloc::stats().tcache_hits`
counter (no new counter invented, per the task brief's "reuse rather than
inventing" instruction) to reproduce gate 3's exact oscillating workload
natively (no WSL/valgrind — this reads a process counter, not an instruction
count) and derive `misses = total_allocs − tcache_hits`. Since every
non-hit alloc in this bounded, single-threaded, single-class, no-OOM
scenario takes the `refill_magazine_slow` cold path, `misses` is exactly the
refill-event count. Run once per `FLUSH_N` value (hand-edited between runs,
same protocol as the iai sweep).

---

## 2. Results — gate 1 (bulk free, the CPU-savings side)

`free_cost(N) = Ir(arm_N) − Ir(dealloc_prealloc_only_1088_16b)`, raw Ir from
two-run-equivalent-deterministic `npm run iai` (iai's `Ir` is a deterministic
instruction count on the same binary+input — a single run per sweep point is
sufficient, and a follow-up rerun of `oscillating_live_set_16b` at FLUSH_N=16
alone reproduced byte-identical `62,183` Ir, confirming determinism holds for
this suite as it has in every prior `npm run iai` report in this doc tree).

| FLUSH_N | free(17) | free(32) | free(64) | free(256) | free(1024) | Ir/free @ N=1024 |
|---:|---:|---:|---:|---:|---:|---:|
| 4  | 1,079 | 2,699 | 6,675 | 30,531 | 125,955 | 123.00 |
| 8 (baseline) | 1,279 | 2,448 | 5,920 | 26,752 | 110,080 | 107.50 |
| 12 | 1,550 | 2,990 | 5,956 | 26,932 | 110,836 | 108.24 |
| 16 | 1,775 | 2,420 | 5,838 | 26,346 | 108,378 | 105.84 |

Only `FLUSH_N=16` beats the baseline, and only modestly (105.84 vs 107.50
Ir/free, −1.5% at N=1024). `FLUSH_N=4` is a clear loser (123.00 Ir/free,
+14.4%) — more frequent overflow events (every 4 flushed blocks instead of
every 8) means the per-event fixed setup/compaction overhead (the same ~470
Ir non-bitmap-clear remainder R24-2 §4.4 measured as non-isolable) is paid
more often per unit of batch-free work, and is NOT amortized away by the
smaller per-event flush size. `FLUSH_N=12` is flat-to-slightly-worse
(108.24 vs 107.50, +0.7%) — inside the noise band this project's other
reports treat as "no real signal" (R24-3's own regression was +37 Ir/event,
i.e. individually larger than this N=1024 aggregate's whole margin).

**Gate 1 alone would suggest `FLUSH_N=16` as a marginal candidate.** Per the
task brief's explicit instruction, this is NOT sufficient for a GO — gates
2-4 must also clear, and gate 3 does not.

---

## 3. Results — gate 2 (free-then-realloc burst, the LOSS side)

`burst_cost(17) = Ir(dealloc_realloc_burst_1088_16b_n17) − Ir(prefix)`:

| FLUSH_N | burst_cost(17) | Δ vs. baseline |
|---:|---:|---:|
| 4  | 2,334 | −254 (better) |
| 8 (baseline) | 2,588 | — |
| 12 | 2,913 | +325 (worse) |
| 16 | 3,033 | +445 (worse) |

Monotonically increasing with `FLUSH_N`: the more blocks a single overflow
event flushes, the fewer just-freed blocks remain magazine-resident for the
immediate realloc that follows, so more of the 17 reallocs miss the magazine
and pay the substantially more expensive `refill_magazine_slow` cold path
(carve/reclaim, not a simple array pop) instead of a cheap LIFO hit.
`FLUSH_N=16` (full flush, zero retained blocks) is the WORST of the four —
consistent with "smaller FLUSH_N retains more warm blocks, which measurably
helps an immediate-realloc burst" being real, not just a hypothesis. This is
exactly the trade-off direction the task brief predicted, and it moves
OPPOSITE to gate 1's `FLUSH_N=16` preference — the two gates pull against
each other, which is the reason no single-gate verdict is trustworthy here.

---

## 4. Results — gate 3 (oscillating live-set, the KILL-CONDITION gate)

| FLUSH_N | `oscillating_live_set_16b` raw Ir | Δ vs. baseline | refill events / 20 rounds (gate 5) | hit rate (gate 5) |
|---:|---:|---:|---:|---:|
| 4  | 25,845 | +126 (+0.5%) | 1 | 99.69% |
| 8 (baseline) | 25,719 | — | 1 | 99.69% |
| 12 | 27,761 | +2,042 (+7.9%) | 2 | 99.38% |
| 16 | **62,183** | **+36,464 (+141.8%, 2.42×)** | **20** | **93.75%** |

**This is the decisive, kill-condition-triggering result.** At `FLUSH_N=16`,
EVERY SINGLE one of the 20 oscillation rounds triggers exactly one
`refill_magazine_slow` call — a 20× increase in refill events over the
baseline's 1-in-20. Root cause, confirmed by reading the mechanism (not just
inferred from the number): with `FLUSH_N=16`, a magazine overflow during the
shrink phase (24→8 frees) empties the magazine COMPLETELY
(`remaining = TCACHE_CAP - FLUSH_N = 0`). The very next growth phase (8→24
allocs) therefore starts from `count == 0` and its first alloc is
GUARANTEED to miss the magazine and pay the cold refill path — every round,
without exception. At `FLUSH_N=8` (or 4, or 12), the overflow leaves
`TCACHE_CAP - FLUSH_N` (8, 12, or 4) blocks resident, so most growth phases
find a partially-stocked magazine and only occasionally need a real refill —
1-2 times across 20 rounds, not 20.

`refill_magazine_slow` is `#[cold] #[inline(never)]` and does real work
(opportunistic ring/deferred-free drains, then `refill_class_bump_checked`'s
carve/reclaim path) — categorically more expensive than the array-pop
magazine hit it replaces, which is exactly why converting 1-in-20 hits into
20-in-20 misses produces a 2.42× Ir blowup rather than a proportionally
smaller change. This is precisely the "larger-FLUSH_N variants losing more
to refill-thrash than they save" scenario the task brief's kill condition
names explicitly.

**Gate 5 (non-Ir cross-check) independently confirms this is a real
refill-thrash regression, not an Ir-counting artifact:** the refill-COUNT
axis (1 → 1 → 2 → 20) and the Ir axis (25,719 → 25,845 → 27,761 → 62,183)
tell the same story via two independently-measured signals (a Callgrind
instruction count under WSL/valgrind vs. a native relaxed-atomic counter
read on Windows), satisfying the brief's "measure refill count / hit rate,
not only Ir" requirement.

---

## 5. Results — gate 4 (ordinary interleaved churn — verified, not assumed)

| bench | FLUSH_N=4 | FLUSH_N=8 (baseline) | FLUSH_N=12 | FLUSH_N=16 |
|---|---:|---:|---:|---:|
| `small_churn_16b` | 8,051 | 8,051 | 8,051 | 8,051 |
| `medium_class_dealloc_churn_16b` | 8,051 | 8,051 | 8,051 | 8,051 |
| `aligned_churn_640b_a128` | 7,987 | 7,987 | 7,987 | 7,987 |
| `churn_256b` | 8,051 | 8,051 | 8,051 | 8,051 |
| `churn_write_256b` | 8,307 | 8,307 | 8,307 | 8,307 |

**Byte-identical across all four `FLUSH_N` values, for every arm.** Confirms
R24-2 §4.6's finding continues to hold regardless of `FLUSH_N`: the
interleaved alloc-then-immediately-free shape never lets the magazine
`count` exceed 1, so it never reaches the overflow branch this task's
constant governs. Gate 4 PASSES cleanly at every sweep point — this was
verified, not assumed, per the task brief's explicit instruction, and the
verification found no surprise (unlike gate 3).

---

## 6. A real, independent bug found at the FLUSH_N=16 boundary (not part of the verdict, but load-bearing for any future attempt)

While measuring `FLUSH_N=16`, `git checkout`ing the constant to 16 and
building under `--features "production virgin-zero-skip"` (a real,
documented, non-hypothetical feature combination — `virgin-zero-skip`
requires only `alloc-decommit`, which `production` already includes) FAILED
TO COMPILE:

```text
error: this arithmetic operation will overflow
   --> src/registry/heap_core_free.rs:794:25
    |
794 |                         self.tcache.classes[c].virgin_mask >>= FLUSH_N;
    |                         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ attempt to shift right by `16_usize`, which would overflow
    |
    = note: `#[deny(arithmetic_overflow)]` on by default
```

`virgin_mask` is a `u16` (`PerClass::virgin_mask`, `tcache.rs`); shifting a
16-bit integer right by exactly 16 is a shift-by-bit-width, caught at
COMPILE TIME by rustc's `arithmetic_overflow` lint once `FLUSH_N` const-folds
to 16. This reproduced under `cargo build --release` (optimized profile);
notably `cargo check` in the `dev` (unoptimized) profile did NOT catch it —
rustc's const-propagation for this particular lint apparently only completes
under the release/optimized codegen path for this cross-module `const`
reference, meaning a `cargo check`-only CI gate would MISS this error
entirely and only a real `--release` build (or `cargo build`) would surface
it. `production` itself does not include `virgin-zero-skip`, so this task's
own `npm run iai` gate-1..4 measurements (built under plain `production`)
never hit this path — but this project's `cargo-hack` feature-powerset job
(R14-10/task #295, weekly + on-demand) DOES check
`fastbin + virgin-zero-skip` combinations, and would have failed on this if
`FLUSH_N=16` had shipped un-fixed. **This is now moot given the NO-GO
verdict** (the constant is reverted to 8, where `virgin_mask >>= 8` is
well-defined), but is recorded here because any FUTURE attempt to revisit
`FLUSH_N=16` (or any `FLUSH_N == TCACHE_CAP` value) MUST additionally guard
this shift (e.g. `if FLUSH_N < TCACHE_CAP { virgin_mask >>= FLUSH_N } else {
virgin_mask = 0 }`, since a full flush leaving `remaining == 0` means EVERY
bit is out of scope and the mask should simply be zeroed) before it can be
considered production-ready, independent of whatever the performance verdict
turns out to be at that value.

---

## 7. Conclusion: NO-GO for the entire sweep

No swept `FLUSH_N` value (4, 12, 16) beats the current baseline (8) once all
5 required gates are weighed together:

- **`FLUSH_N=4`**: NO-GO. Gate 1 regresses badly (+14.4% at N=1024); the
  smaller per-event flush size is NOT amortized by the fixed per-event
  overhead, contradicting the task brief's arithmetic-ceiling hypothesis for
  this direction of the sweep. Gate 2's minor win (−254 Ir) does not come
  close to compensating.
- **`FLUSH_N=8`**: BASELINE. Unchanged — this is what ships.
- **`FLUSH_N=12`**: NO-GO. Gate 1 is flat-to-slightly-worse (+0.7%, no real
  win); gate 2 (+325 Ir) and gate 3 (+2,042 Ir, +7.9%, 2× the refill count)
  are both measurably worse. No gate offers a reason to prefer this value.
- **`FLUSH_N=16`**: NO-GO. The only value with a real gate-1 win (−1.5%),
  but it triggers the task brief's explicit kill condition on gate 3: a
  2.42× Ir regression and a 20× refill-event-count regression on the
  oscillating boundary-stress scenario, independently confirmed via both the
  Ir judge (gate 3) and a non-Ir process-counter cross-check (gate 5). Also
  carries an independent latent compile-time bug under
  `virgin-zero-skip` (§6), moot here but relevant to any future revisit.

**All `src/` production code is reverted; `git diff HEAD -- src/` is empty**
(confirmed via `git status --short src/` showing no changes). The new
measurement infrastructure (bench arms + the gate-5 probe example) is KEPT,
following the R24-2 precedent of retaining reusable measurement tooling from
a pure-measurement task even when the swept parameter itself does not change
— these arms sweep `whatever FLUSH_N the tree is currently built with` (they
do not hardcode 8), so they remain available for any future FLUSH_N
revisit (e.g. R25-8's conditional run-encoded free-batch design study) with
zero additional bench-authoring cost.

This is the **third** NO-GO in this exact free-path/magazine-overflow code
region within this round cluster (R24-3's `flush_magazine_class` bitmap-clear
merge, R24-4's bulk-mask `clear_many`/`set_many` primitives, now R25-3's
`FLUSH_N` sweep) — reinforcing R24-4 §6.1's conclusion that this region
(`dealloc_own_thread_with_base`'s magazine-overflow branch) is unusually
resistant to further optimization by any of the mechanisms tried so far
(loop-restructuring, bulk-primitive coalescing, half-flush-ratio tuning).
Any future attempt on this region should budget for a real chance of another
NO-GO and should, per this report's own gate-3 finding, pay particular
attention to refill-thrash side effects that an isolated or single-gate
measurement would miss.

---

## 8. Verification performed

- **Read the mechanism first** (§1.1): confirmed `remaining =
  TCACHE_CAP - FLUSH_N` is a genuine zero-iteration no-op at FLUSH_N=16 (no
  special-casing needed for the compaction loop itself), and separately
  discovered the real `virgin_mask >>= FLUSH_N` compile-time hazard at that
  same boundary (§6) by actually building under the combination, not by
  inspection alone.
- **All 5 required gates run, none skipped** — gate 1 (Ir bulk-free sweep,
  N=17/32/64/256/1024), gate 2 (Ir free-then-realloc burst), gate 3 (Ir
  oscillating live-set), gate 4 (Ir ordinary interleaved churn regression
  check), gate 5 (native refill-count/hit-rate cross-check, not Ir-only).
- **Determinism check**: `oscillating_live_set_16b` at FLUSH_N=16 rerun in
  isolation reproduced the exact same `62,183` Ir as the full-suite run —
  confirms the result is not host-contention noise (iai's Callgrind-based
  `Ir` is deterministic on the same binary+input, per this project's
  established convention).
- **Gate 4 verified, not assumed**: explicitly re-measured
  `small_churn_16b`/`medium_class_dealloc_churn_16b`/
  `aligned_churn_640b_a128`/`churn_256b`/`churn_write_256b` at all four
  `FLUSH_N` values and confirmed byte-identical Ir at every point, rather
  than citing R24-2's prior finding without re-checking it under a changed
  constant.
- **Two independently-measured signals for gate 3's kill condition** (Ir via
  WSL/Callgrind, refill-count/hit-rate via a native Windows process-counter
  read) agree on direction and rough magnitude — not a single-metric
  artifact.
- **`git diff HEAD -- src/` is empty** after the sweep — the tree's
  production code is byte-identical to HEAD; only `benches/` (new measurement
  arms) and `examples/` (the new gate-5 probe) carry changes.
- **`cargo fmt --check`** on both touched files — clean.
- **`cargo clippy --example r25_3_flush_n_oscillating_probe --features
  "production alloc-stats" -- -D warnings`** — clean.
- **`cargo check --bench perf_gate_iai --features "production
  bench-internals"` under WSL** — clean (the bench body is
  `#[cfg(target_os = "linux")]`-gated; clippy for it is deferred to the
  reviewing session's own `npm run check` pass on a Linux-capable clippy
  toolchain, matching R24-2 §7's identical documented caveat — this WSL
  toolchain does not have the `clippy` rustup component installed).
- **`cargo check --features production`** on Windows — clean.
- **`production`'s feature composition confirmed unchanged**:
  `grep -n "^production = " Cargo.toml` still returns the same 7-feature list
  as R24-2/R24-9; `Cargo.toml` carries no diff from this task.

---

## Files touched

- `benches/perf_gate_iai.rs` — added `PREFIX_OPS` (1088) +
  `dealloc_prealloc_only_1088_16b` + `dealloc_free_only_1088_16b_n{17,32,64,256,1024}`
  + `dealloc_realloc_burst_1088_16b_n17` + `OSC_ROUNDS` (20) +
  `oscillating_live_set_16b` (8 new bench fns total, `#[library_benchmark]`
  count 54 → 62), registered in the `perf_gate` `library_benchmark_group!`
  list; all 8 run unconditionally under plain `--features production` (no
  extra feature gate needed, unlike e.g. `dealloc_overflow_bitmap_clear_only_16b`
  which needs `bench-internals`) — the suite's actual `npm run iai --features
  production` run count went from 52 to 60 (confirmed via each raw log's own
  "N benchmarks finished" line). Zero changes to any pre-existing bench fn's
  body. **Kept** (measurement infrastructure, not a production-behavior
  change — R24-2 precedent).
- `examples/r25_3_flush_n_oscillating_probe.rs` — new, gate-5 non-Ir
  refill-count/hit-rate cross-check, reusing the existing `alloc-stats`
  `tcache_hits` counter (no new counter added). **Kept.**
- `docs/perf/R25_3_FLUSH_N_SWEEP_GATE.md` — this report.
- `docs/perf/R25_3_FLUSH_N_SWEEP_GATE_summary.csv` — companion
  machine-readable summary (all 5 gates × 4 FLUSH_N values).
- `docs/perf/_raw_r25_3_flush4.log` / `_raw_r25_3_flush8.log` /
  `_raw_r25_3_flush12.log` / `_raw_r25_3_flush16.log` — full 60-bench
  `npm run iai` stdout for each sweep point (`git add -f` needed —
  `.gitignore` excludes `docs/perf/_raw_*.log` by default, R13-10/task #280).
- `docs/perf/OPEN_ITEMS.md` — new item added (see below).
- **`src/registry/tcache.rs`** — **untouched** (confirmed via `git diff HEAD
  -- src/registry/tcache.rs` being empty; the constant was hand-edited 3
  times during measurement and restored to its original
  `TCACHE_CAP / 2` form each time).
- **All other `src/` files — untouched.**

**Files needing `git add -f`** (gitignored by `.gitignore`,
`/docs/perf/_raw_*.log`):

- `docs/perf/_raw_r25_3_flush4.log`
- `docs/perf/_raw_r25_3_flush8.log`
- `docs/perf/_raw_r25_3_flush12.log`
- `docs/perf/_raw_r25_3_flush16.log`
