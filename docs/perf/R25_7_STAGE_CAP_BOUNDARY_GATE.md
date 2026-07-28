# R25-7 — `STAGE_CAP` boundary gate: STAGE_CAP=64 CONFIRMED CLEAN at every measured N (16→1024)

**Task #401 (R25-7), Round 25.** A real A/B measurement of `dealloc_batch`'s
`STAGE_CAP` constant across batch sizes that exercise the mid-loop multi-flush
path R24-8 introduced — closing the evidence gap R24-8's own report explicitly
scoped away (it measured only N≤64, never N>64). **Verdict: CONFIRMED CLEAN.
`STAGE_CAP=64` beats `STAGE_CAP=512` at EVERY measured N from 16 to 1024, on
both the Ir axis and the Estimated Cycles axis. No crossover exists in the
measured range; the crossover projects at approximately N≈2700, far beyond the
"tens to low hundreds" R23-7 frames as this project's realistic batch size.
No code change. `src/` is byte-identical to HEAD.**

**Date:** 2026-07-28. **Base revision measured:** `main` @ `6a75874`.
**Platform/methodology:** WSL2 (Ubuntu 24.04, kernel `6.x-microsoft-standard-
WSL2`) under Windows 10 Pro x86-64, `valgrind 3.22.0`,
`iai-callgrind-runner 0.14.2`, WSL rustc `1.98.0-nightly`. Same harness, same
features (`production batch-api bench-internals`), same fresh-carve same-segment
16 B methodology R24-8's own `dealloc_batch_fresh_{16,64}_16b` arms established
(see `benches/perf_gate_iai.rs:1205-1255` for the two pre-existing arms; this
task's six new arms at `:1276-1408` extend them to N=80/81/128/200/512/1024).

**P2 framing (per task brief + R23-7):** the batch API has no downstream
consumer (`batch-api` is `["experimental","alloc-core"]`, NOT part of
`production`; no in-tree caller exists per R23-7's grep audit). This is evidence-
gap closure, not a user-visible optimization. Time-boxed accordingly.

---

## 0. Headline

| question | answer |
|---|---|
| Does STAGE_CAP=64 still win at N>64 (the multi-flush regime R24-8 never measured)? | **YES — at every measured N (16 through 1024).** |
| Is there a crossover where STAGE_CAP=512 starts winning? | **NO crossover in the measured range.** Projects at ~N≈2700 (§3), far beyond realistic. |
| Does the win hold on Estimated Cycles (cache-aware), not just Ir? | **YES — STAGE_CAP=64 is cheaper in cycles at every N too (ΔCyc +6,076 to +8,168).** |
| Action / code change? | **NONE.** `STAGE_CAP=64` kept. `git diff HEAD -- src/` is empty. |
| What changed in this task? | **6 new bench arms** (`dealloc_batch_fresh_{80,81,128,200,512,1024}_16b` + no-op stubs) — reusable regression infra for any future STAGE_CAP change, same precedent as R24-2/R24-8/R25-3's retained arms. |

---

## 1. The gap this closes

R24-8 (commit `839b4af`, task #386) reduced `dealloc_batch_small`'s on-stack
staging array `STAGE_CAP` from 512 to 64, with an LLVM-IR-proven, measured
constant win of **−4,065 Ir/call** at N=16 and N=64. R24-8's own report
(`docs/perf/R24_8_DEALLOC_BATCH_INTERNALS_GATE.md` §2.3) explicitly scoped
itself to those two sizes:

> "With STAGE_CAP=64, a batch of N blocks does: first 16 → magazine, remaining
> N−16 → staged in chunks of 64 (intermediate `flush_class` flush at each
> 64-block boundary). Batches up to 80 blocks (STAGE_CAP + TCACHE_CAP) still
> fit in one flush. Larger batches do multiple flushes. [...] For the
> experimental API with no downstream consumer (R23-7), realistic batch sizes
> are tens to low hundreds — well within 1–3 flushes."

That is a **reasoned assertion, not a measurement** — R24-8 measured N=16 and
N=64 (both ≤ STAGE_CAP+TCACHE_CAP=80, i.e. ZERO intermediate flushes at either
STAGE_CAP value) and never measured any N>64. An independent read-only review
(`docs/reviews/2026-07-28-r24-readonly-review.md`, "Opt-in batch API" + P4)
flagged this as a real, disclosed evidence gap:

> "the evidence currently covers only batches fitting the new stage. For N>80
> the implementation starts performing repeated 64-entry flushes. [...] Before
> calling 64 the final cap, measure at least: N = 16, 64, 80, 81, 128, 200,
> 512, 1024."

This task runs exactly that sweep — the eight N values the review named — as a
real A/B (`STAGE_CAP=64` vs `STAGE_CAP=512`), not an analytical derivation. The
brief was explicit that deriving the STAGE_CAP=512 comparison point
analytically from the known per-call memset delta alone "would repeat the exact
mistake R24-3/R24-4 warned against" (a standalone/estimated cost can mislead
when the real in-context behavior differs).

---

## 2. Methodology — real A/B, same harness as R24-8

### 2.1 The eight bench arms

The two pre-existing arms (`dealloc_batch_fresh_16_16b`,
`dealloc_batch_fresh_64_16b`, `benches/perf_gate_iai.rs:1216,1237`) and the six
new arms added by this task (`:1276,1298,1321,1343,1365,1387`) all share ONE
workload shape: fresh heap, allocate N consecutive 16 B blocks (all land in ONE
freshly-carved segment → N same-base `contains_base` calls, the ideal
same-segment batch), then free them all in ONE `dealloc_batch` call. The N
values and their flush behavior at `STAGE_CAP=64`:

| N | staged (N−16) | intermediate flushes @64 | final flush @64 | why this N |
|---|---:|---:|---:|---|
| 16 | 0 | 0 | 0 | within TCACHE_CAP — no staging at all (R24-8 anchor) |
| 64 | 48 | 0 | 48 | overflow but < STAGE_CAP — one flush, no intermediate (R24-8 anchor) |
| **80** | 64 | 0 | 64 | **boundary**: exactly fills stage, zero intermediate — largest single-flush N |
| **81** | 65 | 1 (64) | 1 | **boundary**: smallest N triggering the mid-loop multi-flush path |
| 128 | 112 | 1 (64) | 48 | one intermediate + one final |
| 200 | 184 | 2 (64+64) | 56 | two intermediate + final (matches R24-8/R25-4's correctness-test N) |
| 512 | 496 | 7 (64×7) | 48 | seven intermediate + final |
| 1024 | 1008 | 15 (64×15) | 48 | fifteen intermediate + final — max flush count measured |

### 2.2 The A/B protocol

For each N, both STAGE_CAP values were measured under the identical harness
(`npm run iai -- --features 'production batch-api bench-internals'`, the feature
set R24-8 itself used — `batch-api` is required for `dealloc_batch` to compile,
`bench-internals` for the two re-gated unsafe hooks R24-6 moved off
`production`):

- **A run (STAGE_CAP=64, current):** raw log `docs/perf/_raw_r25_7_stage64.log`.
- **B run (STAGE_CAP=512, the value R24-8 changed FROM):** `STAGE_CAP` was
  edited to 512 in `src/registry/heap_core_dealloc_batch.rs:256`, the full suite
  re-run, then **reverted back to 64**. Raw log
  `docs/perf/_raw_r25_7_stage512.log`. `git diff HEAD -- src/` is empty (§5).

Callgrind's `Ir` is deterministic run-to-run on the same binary+input
(`scripts/iai.mjs` header comment), so a single A and single B run is the
project's established standard (R22-15, R23-2, R24-2, R24-5, R24-8, R25-3 all
cite single-run Ir). The two R24-8 anchor arms (N=16, N=64) reproduce R24-8's
OWN published numbers EXACTLY under both STAGE_CAP values — a full
methodology cross-check:

| arm | R24-8 published (STAGE_CAP=64) | this task, A run (STAGE_CAP=64) | R24-8 published (STAGE_CAP=512 baseline) | this task, B run (STAGE_CAP=512) |
|---|---:|---:|---:|---:|
| `dealloc_batch_fresh_16_16b` | 4,449 | **4,449** ✓ | 8,514 | **8,514** ✓ |
| `dealloc_batch_fresh_64_16b` | 12,692 | **12,692** ✓ | 16,757 | **16,757** ✓ |

Both anchors byte-identical to R24-8 §2.2's table — the harness, the build, and
the measurement chain are confirmed equivalent to R24-8's own before any new N's
numbers are cited.

### 2.3 Reference-arm cross-check (no STAGE_CAP dependence)

The 58 non-`dealloc_batch` arms (small_churn, cold_alloc, mimalloc_*, etc.) are
byte-identical across the A and B runs in `Ir` (they don't touch
`dealloc_batch_small`, so STAGE_CAP is invisible to them) — confirming the A/B
differed ONLY in the one `const` line. E.g. `small_churn_16b`=8,051,
`large_alloc_free_cycle`=3,308, `mimalloc_bootstrap_proxy`=13,050 in both runs.
(The L1/L2/RAM cache-sim columns differ by ±1–3 between runs — that is
callgrind's cache-simulation nondeterminism on the shared bootstrap path, not a
signal; `Ir` is the deterministic judge and is byte-identical.)

---

## 3. Result — full N × STAGE_CAP table

### 3.1 Ir (the deterministic judge)

| N | A: STAGE_CAP=64 | B: STAGE_CAP=512 | ΔIr (B−A) | Δ% | verdict @ this N |
|---|---:|---:|---:|---:|---|
| 16 | 4,449 | 8,514 | **+4,065** | 47.74% | 64 wins (reproduces R24-8) |
| 64 | 12,692 | 16,757 | **+4,065** | 24.26% | 64 wins (reproduces R24-8) |
| 80 | 15,395 | 19,460 | **+4,065** | 20.89% | 64 wins (boundary: 0 intermediate flushes either way) |
| 81 | 17,164 | 21,120 | **+3,956** | 18.73% | 64 wins (first N with a 64-side intermediate flush) |
| 128 | 23,613 | 27,569 | **+3,956** | 14.35% | 64 wins |
| 200 | 36,678 | 40,525 | **+3,847** | 9.49% | 64 wins |
| 512 | 93,008 | 96,310 | **+3,302** | 3.43% | 64 wins |
| 1024 | 184,250 | 186,789 | **+2,539** | 1.36% | 64 wins (max flush count: 15 intermediate) |

**STAGE_CAP=64 is cheaper at every measured N.** The win shrinks monotonically
as N grows (because STAGE_CAP=64 accumulates more intermediate `flush_class`
calls), but it never reaches zero within the measured range — at N=1024, the
largest, most flush-heavy case, STAGE_CAP=64 is still **2,539 Ir cheaper**.

### 3.2 Estimated Cycles (cache-aware — `L1 + 5·L2 + 35·RAM`)

| N | A: STAGE_CAP=64 | B: STAGE_CAP=512 | ΔCyc (B−A) | verdict @ this N |
|---|---:|---:|---:|---|
| 16 | 21,617 | 29,751 | **+8,134** | 64 wins |
| 64 | 32,891 | 41,059 | **+8,168** | 64 wins |
| 80 | 36,450 | 44,614 | **+8,164** | 64 wins |
| 81 | 38,698 | 46,716 | **+8,018** | 64 wins |
| 128 | 47,113 | 55,139 | **+8,026** | 64 wins |
| 200 | 64,217 | 72,085 | **+7,868** | 64 wins |
| 512 | 140,386 | 147,524 | **+7,138** | 64 wins |
| 1024 | 262,077 | 268,153 | **+6,076** | 64 wins |

STAGE_CAP=64 wins on Estimated Cycles at every N too. The cycle delta
(+6,076 to +8,168) is consistently ~2× the Ir delta (+2,539 to +4,065): the
eliminated 4096-byte memset is pure L1-resident stack store traffic, so its
cycle cost (touching 64 cache lines) is real and roughly double its pure
instruction count — the cache-aware metric confirms, not undermines, the
Ir-only verdict. (This is the inverse of the X-arc memcpy case `scripts/iai.mjs`
warns about, where cycles revealed an Ir-blind regression; here both axes agree.)

### 3.3 Cache columns (L1/L2/RAM Hits) — no cache-miss regression

The L2/RAM hit counts are within ±3 of each other across A and B at every N
(callgrind cache-sim noise on the shared bootstrap; the staging array is
stack-local and L1-resident, so it never reaches L2/RAM). There is NO
cache-miss regression from STAGE_CAP=64 — the win is pure instruction/​L1-store
elimination, exactly as R24-8's LLVM-IR memset proof predicted.

---

## 4. Why there is no crossover — the per-flush-cost decomposition

The ΔIr shrinks as N grows because STAGE_CAP=64 does more intermediate
`flush_class` calls than STAGE_CAP=512. But the shrinkage is perfectly linear
and the breakeven is far beyond any realistic N.

### 4.1 Each extra intermediate flush costs exactly 109 Ir

For each N, count the number of intermediate `flush_class` calls
(`if staged == STAGE_CAP { flush; staged = 0; }` mid-loop hits) under each
STAGE_CAP, take the difference ("extra flushes" STAGE_CAP=64 does beyond
STAGE_CAP=512), and compare to the ΔIr shrinkage from the N=80 constant
(+4,065):

| N | interm. @64 | interm. @512 | extra flushes | 4,065 − ΔIr | per-extra-flush cost |
|---|---:|---:|---:|---:|---:|
| 80 | 0 | 0 | 0 | 0 | — (baseline) |
| 81 | 1 | 0 | 1 | 109 | **109** |
| 128 | 1 | 0 | 1 | 109 | **109** |
| 200 | 2 | 0 | 2 | 218 | **109** |
| 512 | 7 | 0 | 7 | 763 | **109** |
| 1024 | 15 | 1 | 14 | 1,526 | **109** |

**Perfect linear fit: each extra intermediate flush costs exactly 109 Ir.** The
decomposition `ΔIr(N) = 4,065 − 109 × (extra_flushes(N))` holds to the unit at
all five multi-flush data points. (Intermediate-flush counts verified by hand-
tracing `dealloc_batch_small`'s loop at `src/registry/heap_core_dealloc_batch.rs:259-388`;
the `if staged == STAGE_CAP` branch at `:359` is the sole intermediate-flush
site.) This 109 Ir/flush is the per-call fixed overhead of `flush_class` — the
`SegmentMeta`/`bin_table`/`bump_of` setup that method hoists per run, independent
of run length — exactly the cost category R24-8 §2.3's prose asserted would be
"well within 1–3 flushes" at realistic sizes. The measurement confirms that
assertion quantitatively.

### 4.2 Crossover projection — N ≈ 2,700, far beyond realistic

STAGE_CAP=512 would start winning only when the extra-flush overhead exceeds
the memset savings:

```
109 × extra_flushes(N) > 4,065
extra_flushes(N) > 37.3
```

For large N, `extra_flushes(N) ≈ N/64 − N/512 = 7N/512`. Solving:

```
7N/512 > 37.3  →  N > 2,729
```

So the crossover projects at **N ≈ 2,700** — and even there it is only
break-even, not a meaningful win for STAGE_CAP=512. The measured maximum
(N=1024) has extra_flushes=14, less than 40% of the breakeven threshold; ΔIr is
still a comfortable +2,539 in favor of STAGE_CAP=64.

### 4.3 Why this matches R24-8's reasoning

R24-8 §2.3 argued the constant 4,065 Ir memset savings would "dominate any
per-flush overhead at these sizes." This task confirms that prediction holds not
just at "tens to low hundreds" but all the way to N=1024 (15 intermediate
flushes), and quantifies the per-flush cost (109 Ir) that makes the dominance
hold. The concern the review raised — "whether there's a crossover point where
the added per-chunk `flush_class` overhead starts eating into it" — has a
definitive measured answer: the crossover exists in principle (N≈2700) but is
not reachable at any batch size a real consumer would issue.

---

## 5. Decision: KEEP STAGE_CAP=64 — no code change

Per the task brief's decision framework:

- **STAGE_CAP=64 beats STAGE_CAP=512 at every measured N (16→1024)** on both Ir
  and Estimated Cycles → this is the "CONFIRMS R24-8's choice was correct at
  realistic batch sizes too" branch.
- The crossover (N≈2700) is **far beyond** "tens to low hundreds of blocks per
  batch," which R23-7 (`docs/perf/R23_7_BATCH_API_CONSUMER_STATUS.md`) and
  R24-8 §2.3 both establish as this project's framing of a realistic batch size.
  No recommendation to change the constant, revert, or make it workload-tunable
  is warranted — the evidence does not meet the "crossover happens at an N
  plausible for a real future consumer" bar the brief set for any action.
- Per the brief's P2/time-boxing guidance, this confirmatory report is a
  complete, sufficient outcome — no additional investigation is manufactured.

**`git diff HEAD -- src/` is empty.** `STAGE_CAP` is restored to 64 (verified:
`grep -n 'const STAGE_CAP' src/registry/heap_core_dealloc_batch.rs` → `256:
const STAGE_CAP: usize = 64;`). The only tree change is the six new bench arms
in `benches/perf_gate_iai.rs` (§6).

---

## 6. Files touched

- `benches/perf_gate_iai.rs` — **6 new iai arms** (`dealloc_batch_fresh_80_16b`
  `:1276`, `_81_16b` `:1298`, `_128_16b` `:1321`, `_200_16b` `:1343`,
  `_512_16b` `:1365`, `_1024_16b` `:1387`) + 6 no-op stubs (`:2301-2337`, for
  `library_benchmark_group!` resolution when `batch-api` is absent) + 6
  `library_benchmark_group!` list entries (`:2368-2373`). These arms measure
  whatever `STAGE_CAP` the tree is built with (they do NOT hardcode a value),
  so they double as reusable regression infrastructure for any future STAGE_CAP
  change — same precedent as R24-2/R24-8/R25-3's retained bench arms.
- `src/registry/heap_core_dealloc_batch.rs` — **byte-identical to HEAD.**
  (`STAGE_CAP` was temporarily set to 512 for the B run, measured, then reverted
  to 64; `git diff HEAD -- src/` is empty.)
- `docs/perf/R25_7_STAGE_CAP_BOUNDARY_GATE.md` — this report.
- `docs/perf/R25_7_STAGE_CAP_BOUNDARY_GATE_summary.csv` — companion summary.
- `docs/perf/_raw_r25_7_stage64.log` — A run raw iai output (STAGE_CAP=64).
  (`.gitignore` excludes `docs/perf/_raw_*.log`; `git add -f` at commit time.)
- `docs/perf/_raw_r25_7_stage512.log` — B run raw iai output (STAGE_CAP=512).
- `docs/perf/OPEN_ITEMS.md` — updated (item 1's R24-8 paragraph extended with
  this gate's closure of the N>64 evidence gap).

---

## 7. Evidence

- **Raw logs:** `docs/perf/_raw_r25_7_stage64.log` (A run, STAGE_CAP=64,
  66 benches, exit 0) / `docs/perf/_raw_r25_7_stage512.log` (B run,
  STAGE_CAP=512, 66 benches, exit 0). Both full unfiltered `npm run iai` runs;
  the 58 non-`dealloc_batch` arms are byte-identical in `Ir` across the two,
  confirming the A/B differed only in the one `const` line.
- **Summary CSV:** `docs/perf/R25_7_STAGE_CAP_BOUNDARY_GATE_summary.csv`.
- **Prior reports this extends/cites:** `R24_8_DEALLOC_BATCH_INTERNALS_GATE.md`
  (the gate whose N≤64 scope this closes), `R23_7_BATCH_API_CONSUMER_STATUS.md`
  (the realistic-batch-size framing), the R24 readonly review's P4 (the flag).
- **Source under test:** `src/registry/heap_core_dealloc_batch.rs:256` (`const
  STAGE_CAP`) and `:259-388` (`dealloc_batch_small`, whose `:359` mid-loop flush
  is the multi-flush path whose overhead §4 quantifies).
