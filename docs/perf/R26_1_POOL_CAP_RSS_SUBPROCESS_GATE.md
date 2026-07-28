# R26-1 — the corrected RSS/commit `pool_segments` sweep (subprocess-per-arm isolation)

**Task #410 (R26-1), Round 26.** Direct remeasurement of R25-5's invalidated
RSS/commit axis, using **one fresh OS process per `(pool_segments,
thread_count, repetition)` tuple** so the registry-slot-reuse bug that
invalidated R25-5 (`docs/perf/R25_5_POOL_CAP_SWEEP_GATE.md` §8) is eliminated
by construction. Implements `docs/reviews/2026-07-28-r25-readonly-review.md`'s
"Required correction" recipe verbatim.

**Verdict: the RSS-axis claim "cap 4→8 wins on RSS too (lower RSS)" does NOT
reproduce under real isolation.** Under subprocess-per-arm isolation, all four
swept caps (4/8/16/32) produce **statistically identical RSS/commit deltas at
every thread count** (1/8/32) — the inter-cap difference at any cell is well
within the intra-cap spread across repetitions. R25-5's dramatic "cap=8 is 34%
lower RSS than cap=4 at 8 threads" finding was an artifact of sequential
single-process execution (cap=8's threads silently reused cap=4's
already-materialised, already-committed slots, so the RSS delta captured only
incremental cost beyond cap=4's warm segments — not cap=8's own honest
footprint). **This task does not change any `src/` default** — measurement only.

The latency/decommit GO-CANDIDATE for `pool_segments=8` (R25-5 §0's first
table, unaffected by the bug) **still stands**: cap 4→8 eliminates the 20-decommit
residual. But the claim that this step "wins on BOTH axes simultaneously, with
no trade-off" is **refuted for the RSS axis** — raising the cap is RSS-neutral
(neither better nor worse), not RSS-beneficial. The net case for cap=8 is
"eliminates decommits at no RSS cost," which is still a positive but weaker
than R25-5's invalidated "faster AND cheaper on RSS" framing.

**Date:** 2026-07-28. **Base revision measured:** `main` @ `5285e14` + this
task's uncommitted working tree. **Platform:** native Windows x86-64 (shared
host — RSS/commit deltas are noisy point estimates; the cap/conflicts
self-checks are exact). **Feature set:** `production` + `alloc-stats` (matching
R25-5's build for apples-to-apples comparison).

---

## 0. Headline numbers — corrected RSS/commit axis (subprocess-per-arm)

All numbers are the **median of 3 repetitions** per `(pool_segments,
thread_count)` cell, with `(min..max)` range in the raw log. Every cell was
self-verified before its number was trusted (see §1.2).

### Peak RSS Δ (KiB) — median of 3 reps

| `pool_segments` | 1 thread | 8 threads | 32 threads | 8T/1T ratio | 32T/1T ratio |
|---:|---:|---:|---:|---:|---:|
| **4 (baseline)** | **13,368** (13,364..13,388) | **105,976** (105,904..106,064) | **423,448** (423,244..423,656) | 7.93× | 31.68× |
| **8** | 13,372 (13,364..13,392) | 106,036 (105,992..106,044) | 423,444 (423,320..424,140) | 7.94× | 31.67× |
| **16** | 13,360 (13,360..13,368) | 105,956 (105,880..106,012) | 423,444 (423,000..423,620) | 7.93× | 31.69× |
| **32** | 13,384 (13,368..13,384) | 106,052 (105,996..106,132) | 423,520 (423,236..423,752) | 7.93× | 31.65× |

### Peak commit Δ (KiB) — median of 3 reps

| `pool_segments` | 1 thread | 8 threads | 32 threads |
|---:|---:|---:|---:|
| **4 (baseline)** | **17,240** | **133,548** | **532,260** |
| **8** | 17,240 | 133,656 | 532,256 |
| **16** | 17,236 | 133,512 | 532,336 |
| **32** | 17,276 | 133,656 | 532,380 |

**The inter-cap difference at any cell is within the intra-cap noise.** At
1 thread, the cap=4→cap=32 RSS delta differs by 16 KiB (13,368 → 13,384) on a
~13,370 KiB base — 0.12%, smaller than the ±12 KiB spread within a single
cap's 3 reps. At 32 threads, cap=4→cap=32 differs by 72 KiB (423,448 →
423,520) on a ~423,500 KiB base — 0.017%, far smaller than the ±200 KiB
intra-cap spread. **RSS is flat across all caps.**

---

## 1. Methodology

### 1.1 Subprocess-per-arm isolation — the structural fix

R25-5's bug was that sequential arms in one process can silently reuse
registry slots configured by an earlier arm (`heap_registry.rs`
`claim_with_config` first-materialisation-wins at ~:226-246, re-claim
"silently wins" at ~:247-300, `recycle` returns slots to `free_slots` at ~:342,
`pick_slot` pops them first at ~:316-322). The fix is structural: if every
`(pool_segments, thread_count, repetition)` runs in its own freshly-spawned OS
process, there is no possibility of cross-arm slot reuse — a fresh process has
a fresh, empty `HeapRegistry`, so the FIRST claim in that process is
unconditionally the arm's own requested config.

**Implementation** (`examples/r26_1_pool_cap_rss_subprocess_probe.rs`):
- **Orchestrator mode** (default, no env var): iterates the 4×3 grid × 3 reps
  (36 child processes total), re-exec'ing THIS binary via
  `std::env::current_exe()` + `std::process::Command` with env vars
  (`R26_1_POOL_SEGMENTS`, `R26_1_THREAD_COUNT`, `R26_1_REPETITION`), captures
  each child's stdout, parses the `RESULT key=value` lines (this project's
  established probe protocol, `crates/proc-probe`), aggregates to
  (median, min, max) per cell.
- **Child/arm mode** (env vars present): spawns `thread_count` OS threads,
  each claiming its OWN heap via `HeapRegistry::claim_with_config` (NOT
  `SeferAlloc` — this sidesteps `SeferAlloc`'s private `current_heap()` and
  yields a raw `*mut HeapCore` whose `dbg_pool_cap()` is directly readable,
  following `examples/r13_9_class_aware_dirty_sidecar_rss.rs`'s established
  claim/recycle precedent), runs the same batched churn workload as R25-5 for
  `RSS_RUN_DURATION = 1.5s`, while a monitor thread polls
  `proc_probe::snapshot()` for peak RSS/commit.

### 1.2 Self-verification — the two hard asserts (per the review's recipe)

Each child hard-`panic!`s (not soft-logs) on either failure BEFORE its RSS
number is trusted:

1. **Resolved cap equals the requested one** — every claimed heap's
   `HeapCore::dbg_pool_cap()` (the new thin delegation added to
   `src/registry/heap_core_diag.rs` this task, mirroring the existing
   `dbg_pooled_count`) must equal the requested `pool_segments`. This is the
   DIRECT proof the resolved config matches the requested one, not an
   inference — and the exact reachability gap R25-5's RSS axis had (it used
   `SeferAlloc::with_config`, which hides `HeapCore` behind private TLS).

2. **`config_conflicts_total()` delta across this arm's run is exactly 0** — a
   fresh process means the first claim in each slot is unconditionally the
   arm's config, so no conflict is possible. Asserting this anyway (per the
   review's explicit "Required correction" step 3) catches any genuine
   regression worth knowing about.

**Result: all 36 child runs passed both self-checks** (36 `OK` lines in the
raw log, every one with `verified_cap == requested_pool_segments` and
`cfg_conflicts_delta == 0`). No arm's number entered the aggregate table
without its isolation contract being verified.

### 1.3 Workload fidelity — identical to R25-5

`SIZE=1024`, `CHURN_WORKING_SET=256`, `OPS=1024`, batched-setup churn shape
(`RSS_BATCH_SIZE=50`, matching R25-5's `LATENCY_BATCH_SIZE.min(50)` clamp),
`RSS_RUN_DURATION=1.5s`, `proc_probe::snapshot()` every 20ms. The churn
primitives are byte-for-byte copies of `benches/global_alloc.rs`'s (via
R25-5). Same sweep grid: `pool_segments` ∈ {4, 8, 16, 32}, `thread_count` ∈
{1, 8, 32}.

### 1.4 Repetitions and statistical confidence

3 repetitions per cell (36 total child processes). This is modest per this
project's "Speed: short scenario by default" convention — enough to surface
obvious outliers and report median + (min..max) range. The intra-cap spread
across reps is tight (±0.09% at 1T, ±0.1% at 8T, ±0.1% at 32T), and the
inter-cap difference is smaller than the intra-cap spread at every cell — so
the "RSS is flat across caps" finding is not an artifact of insufficient
reps. No silent caps applied.

### 1.5 What was NOT attempted

- **No latency/decommit re-measurement** — R25-5's latency axis
  (`AllocCore::new_with_config` direct, no registry, self-verified via
  `resolved_cap`) was unaffected by the bug and stands unchanged. This task
  measures ONLY the RSS/commit axis.
- **No cross-platform measurement** (Windows-native only, same caveat every
  prior gate carries).
- **`src/alloc_core/small_segment_pool_config.rs` untouched** —
  `DEFAULT_POOL_SEGMENTS` remains `4`.

---

## 2. Why R25-5's "cap=8 is cheaper on RSS" does NOT reproduce

R25-5's RSS axis measured cap=4 first, then cap=8/16/32 sequentially in one
process. When cap=8's threads started, they could pop cap=4's recycled slots
from `free_slots` — slots whose `HeapCore` was already materialised under
cap=4's config and whose segments were **already committed** from cap=4's
preceding run. So cap=8's RSS delta (peak minus its own before-snapshot, taken
after cap=4's run) captured only the **incremental** cost beyond cap=4's
warm, already-resident segments — not cap=8's own honest footprint. The same
applied to cap=16/32. This is why R25-5 saw cap=8/16/32 as dramatically
cheaper than cap=4: they were measuring "how much MORE memory does reusing
cap=4's already-committed segments cost" — which is near-zero — not "how much
memory does a fresh cap=8 heap actually commit."

Under subprocess isolation, each arm starts from a genuinely fresh process
with zero pre-committed allocator segments. Cap=4 and cap=8 both commit the
SAME number of segments for this workload (the workload's actual demand is
~6 segments — R25-5's latency axis confirmed `pooled_count = 6` regardless of
cap). The only difference is that cap=4 churns those 6 segments through
decommit→re-reserve cycles (20 decommits/run on the latency axis) while
cap=8+ holds them steady — but at the RSS-delta level, both end up with the
same number of committed pages resident at peak, because the decommit at
cap=4 is immediately followed by a re-commit when the next batch demands the
segment again. The transient decommit is visible in the decommit COUNTER
(latency axis) but not in the peak RSS snapshot (this axis).

**This refutes R25-5's §3 narrative** ("cap=4's residual churn itself carries
a real OS reserve/decommit round-trip cost that steady-state cap≥8 avoids
entirely") for the RSS axis. The cost is real on the **latency** axis (20
decommit→re-reserve cycles add wall-clock time — R25-5 §0's 177,010 ns/cycle at
cap=4 vs 119,568 at cap=8) but NOT on the **RSS** axis (the peak resident
footprint is the same).

---

## 3. Multi-thread RSS — still linear-in-thread-count, still cap-independent

The review's P2 concern ("a per-thread fixed cap raise multiplies worst-case
committed memory by every thread") is confirmed as present — but it is present
IDENTICALLY at every cap, including the current default (4):

| `pool_segments` | 1T Δ (KiB) | 8T Δ (KiB) | 8T/1T | 32T Δ (KiB) | 32T/1T |
|---:|---:|---:|---:|---:|---:|
| 4 | 13,368 | 105,976 | 7.93× | 423,448 | 31.68× |
| 8 | 13,372 | 106,036 | 7.94× | 423,444 | 31.67× |
| 16 | 13,360 | 105,956 | 7.93× | 423,444 | 31.69× |
| 32 | 13,384 | 106,052 | 7.93× | 423,520 | 31.65× |

The ~8× at 8 threads and ~32× at 32 threads scaling (linear-in-thread-count)
is a property of "N independent heaps each commit their own working set," not
something the cap value changes. Unlike R25-5's invalidated table (where
cap=8/16/32 appeared to have a SMALLER per-thread footprint than cap=4), the
corrected data shows the per-thread footprint is **identical** across all caps
— the workload's segment demand (~6) is what sets the footprint, and the cap
only matters for whether the pool retains or churns those segments.

---

## 4. Corrected vs invalidated — side-by-side

| cell | R25-5 (INVALIDATED) | R26-1 (CORRECTED) | Δ |
|---|---:|---:|---|
| cap=4, 1T RSS | 13,132 | 13,368 | +236 (1.8%) |
| cap=8, 1T RSS | **8,216** | **13,372** | +5,156 (**+63%**) |
| cap=16, 1T RSS | **8,216** | **13,360** | +5,144 (**+63%**) |
| cap=4, 8T RSS | 100,932 | 105,976 | +5,044 (5.0%) |
| cap=8, 8T RSS | **66,508** | **106,036** | +39,528 (**+59%**) |
| cap=4, 32T RSS | 382,352 | 423,448 | +41,096 (10.8%) |
| cap=8, 32T RSS | **264,748** | **423,444** | +158,696 (**+60%**) |

R25-5's cap=4 rows are closest to the corrected values (within ~2-11%) because
cap=4 was the FIRST arm and had no earlier arm's slots to reuse — its
measurement was the least contaminated. R25-5's cap≥8 rows are dramatically
lower than corrected because they silently ran under cap=4's
already-committed segments. The ~60% underreporting at cap≥8 across all thread
counts is consistent with "those arms measured incremental-over-warm-segments
instead of fresh-footprint."

---

## 5. Two-axis decision framework — re-stated under corrected data

- **Latency/decommit axis** (R25-5 §0 first table, unaffected): cap=4→8
  eliminates the entire measured decommit residual (20 → 0). STANDS.
- **RSS/commit axis** (R26-1, corrected): cap=4→8 is **RSS-neutral** — no
  benefit, no penalty. All four caps produce statistically identical RSS at
  every thread count measured. The R25-5 claim "cap 4→8 REDUCES RSS" is
  REFUTED.

**Re-stated verdict:** the GO-CANDIDATE for `pool_segments=8` survives on the
**latency/decommit axis alone** (eliminates 20 decommits/run at no RSS cost —
RSS-neutral, not RSS-beneficial). The "wins on BOTH axes simultaneously, no
trade-off" framing is refuted: there is no RSS axis to "win" on, only a
latency win at RSS-neutral cost. This is still a net positive (eliminating
unnecessary OS decommit/re-reserve churn is good for latency and for OS
syscall overhead) but it is a weaker case than R25-5's invalidated "faster AND
cheaper on RSS" framing suggested.

**What the corrected data does NOT show:** any RSS penalty from raising the
cap. Cap=8 is not more expensive on RSS than cap=4 — they are identical. The
review's P2 concern about per-thread-fixed-cap multiplication IS confirmed
(linear-in-thread-count scaling), but it is identical at every cap including
the current default — raising the cap does not make it worse.

---

## 6. Implications for R25-6 (adaptive/process-wide pool budget)

R25-6 was closed solely because R25-5's (now-refuted) "wins on both axes, no
trade-off" finding appeared to disprove its trigger condition. The corrected
data shows there IS no RSS trade-off to resolve (RSS is flat across caps) —
but there is also no RSS BENEFIT from the fixed-cap raise that would make an
adaptive design unnecessary. The adaptive design's value proposition
(preserve the latency win without granting every thread an unconditional
larger committed allowance) is unchanged: the corrected data shows the
per-thread footprint scales linearly with thread count at every cap, so a
process-wide budget could still bound aggregate committed memory while letting
individual hot heaps exceed 4 segments. R25-6's reopened status (task #418 /
R26-9) stands; its trigger condition should be re-evaluated on the corrected
finding that the fixed-cap raise is RSS-neutral, not RSS-beneficial.

---

## 7. Files changed

| file | change |
|---|---|
| `src/registry/heap_core_diag.rs` | added `HeapCore::dbg_pool_cap(&self) -> usize` — thin safe read-only delegation to `AllocCore::dbg_pool_cap`, gated `alloc-decommit` (same as its sibling `dbg_pooled_count`). NOT `unsafe fn`, NOT `bench-internals`-gated — it is a plain `&self` read of this heap's own resolved cap, same category as `dbg_pooled_count`. |
| `Cargo.toml` | added `[[example]]` entry for `r26_1_pool_cap_rss_subprocess_probe` with `required-features = ["alloc-global", "alloc-xthread", "alloc-decommit"]` (matching the sibling `r25_5_pool_cap_sweep_probe` entry — prevents the E0601 build failure a missing entry causes under plain `--features production`). |
| `examples/r26_1_pool_cap_rss_subprocess_probe.rs` | new — the subprocess-per-arm RSS probe (orchestrator re-execs the same binary once per arm; child mode claims heaps via `HeapRegistry::claim_with_config`, self-verifies resolved cap + zero config conflicts, emits `RESULT key=value`). Measurement-only, same category as `r25_5_pool_cap_sweep_probe.rs`. |
| `docs/perf/R26_1_POOL_CAP_RSS_SUBPROCESS_GATE.md` | this report (new) |
| `docs/perf/R26_1_POOL_CAP_RSS_SUBPROCESS_GATE_summary.csv` | machine-readable summary of §0's tables (new) |
| `docs/perf/_raw_r26_1_pool_cap_rss_subprocess_probe.log` | raw probe stdout, the canonical run cited throughout this report (`.gitignore`d by default — `git add -f` at commit time) |
| `docs/perf/OPEN_ITEMS.md` | item 13's "Current state" bullets updated + new dated paragraph appended (append-only, matching task #411's style) |

**No production source file changed in behavior** (`src/registry/heap_core_diag.rs`
gained one read-only safe test/diagnostic accessor; `DEFAULT_POOL_SEGMENTS`
remains `4`). **No commit made** — tree left unstaged for personal zero-trust
review, per this task's explicit instruction.

---

## 8. Reproduce

```text
cargo run --release --example r26_1_pool_cap_rss_subprocess_probe --features "production alloc-stats"
```

The orchestrator prints each child's `RESULT key=value` lines + `OK ...`
self-check summary, then the aggregated (median, min..max) table. 36 child
processes, ~1.5s each + bootstrap overhead ≈ ~90s total wall-clock on this
host. Each child independently asserts `verified_cap == pool_segments` and
`config_conflicts_delta == 0` — any failure `panic!`s loudly in that child's
stderr and fails the orchestrator.

---

## 9. Correction (2026-07-28, R27-2 / task #420) — peak-live-set flatness is NOT a proof of zero retention cost

**This section is a dated correction appended per this project's append-don't-rewrite convention; §0–§8 above are preserved verbatim, including the §0/§5 "RSS-NEUTRAL / no benefit, no penalty" verdict that this section qualifies.**

The §0 headline and §5 verdict frame the RSS axis as fully "RSS-NEUTRAL" (identical peak RSS across caps 4/8/16/32 ⇒ "no benefit, no penalty"). That conclusion is NOT supported by this probe's own evidence, for two reasons, and is now known to be contradicted by sibling-task data:

**1. This probe never proved victim activation.** The RSS-axis worker loop runs at `RSS_BATCH_SIZE = 50` (`examples/r26_1_pool_cap_rss_subprocess_probe.rs:124`), i.e. `50 × 256 × 1024 bytes ≈ 12.5 MiB` logical prefill — this fits inside the current 4-segment/16 MiB retention region. The probe's two hard asserts (§1.2) prove it RAN each labelled cap (`verified_cap == pool_segments`, `cfg_conflicts_delta == 0`) but prove nothing about whether that capacity was actually USED: it records NO `dbg_pooled_count` / pool-occupancy high-water mark and NO decommit counters, and asserts neither that cap 4 was ever saturated nor that cap 8 ever retained a 5th segment. §2's "the workload's actual demand is ~6 segments" is borrowed from the SEPARATE `LATENCY_BATCH_SIZE=120` axis (R25-5/R26-3's latency probe); the demand proof does not transfer to this RSS axis's batch-50 workload. So identical peak-live-set RSS across all four caps is EXPECTED here even if a real retention cost exists under higher pressure, because the extra capacity was never proven to be exercised. (This is exactly the "victim activation" gap the R26 readonly review's P0 + project-improvement #3 flag: requested/resolved config identity is necessary but not sufficient.)

**2. R26-3's own committed raw log shows cap 8 DOES retain more after teardown.** At the pressure-producing batch-120 workload (the one whose "demand tops out at 6 segments" finding actually holds), `grep 'rss_after_kib=' docs/perf/_raw_r26_3_production_teardown_ab.log` shows cap8 arms deterministically reporting `rss_after_kib=34576` (~33 occurrences) and cap4 arms `rss_after_kib=30476` (~32 occurrences) — a repeatable **+4,100 KiB ≈ exactly one 4 MiB segment** retained after teardown. That is a real, measured retention cost sitting in this project's own committed evidence, and it directly contradicts the §0/§5 "no benefit, no penalty" framing.

**Peak-live-set RSS is also the wrong sole metric for a retention policy.** The pool's actual trade-off appears AFTER teardown (does the cache stay warm/committed while the live working set is gone?) — the axis this probe never measured (it took only ONE peak-during-load snapshot per §1.1) and R26-3 happens to expose as a side effect of its own instrumentation.

**Corrected reading of THIS report:** §0/§5's "RSS-NEUTRAL" finding is valid ONLY as a statement about peak-live-set RSS under this probe's lower-pressure batch-50 shape; it is NOT a statement that cap 8 carries no cap-specific RSS/retention cost. A proper retention gate (tracked as task #421 / R27-3) — subprocess isolation + this probe's config self-verification, but at batch 120, recording peak-live AND post-teardown AND post-idle RSS/commit, final/max `dbg_pooled_count`, and decommit/release/reserve counters, with the cap-4 arm PROVEN to saturate/decommit and the cap-8 arm PROVEN to retain >4 segments — must land before the "RSS-neutral" framing can be re-asserted or the default/adaptive decisions are made. The current-state headline in `docs/perf/OPEN_ITEMS.md` item 13 has been updated this round (R27-2) to stop carrying the unqualified "RSS-neutral" verdict; this report's §0/§5 are preserved verbatim per convention and qualified only by this §9.

**Provenance:** `docs/reviews/2026-07-28-r26-readonly-review.md` P0 ("R26-1 does not prove cap 8 has no retention/RSS cost") + project-improvement #6 ("Fix current-state documentation rather than append another caveat"). Docs-only; no `src/`/`examples/`/`benches/` file touched; no re-measurement attempted.
