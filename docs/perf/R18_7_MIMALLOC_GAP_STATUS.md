# R18-7 — status of `PERF_PLAN_beat_mimalloc_small_medium.md` vs the live README cold-gap headline

**Task #332 (R18-7), Round 18. READ-ONLY investigation + planning.**
**Date:** 2026-07-26 · **Investigator:** main session (zero-trust, evidence-cited).
**Scope:** inventory what landed from the five-eureka plan
([`docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md`](PERF_PLAN_beat_mimalloc_small_medium.md),
tasks #144–#149), verify each claim against git, reconcile it with the live
README cold-gap headline (`README.md:238`: *"a 2.4–2.7× cold gap at 16 B/256 B
and parity at 64 B"*), and propose — not implement — the cheapest next step.

---

## 0. TL;DR (the headline finding, three sentences)

1. **The premise of this task is factually wrong: Э2–Э5 are NOT dormant.**
   All five original eurekas (Э1–Э5) **landed in Round 7** (commits below),
   plus **six more** (Э6–Э11) across P6/P7. The plan
   `PERF_PLAN_beat_mimalloc_small_medium.md` is **EXHAUSTED**, not open. No
   Round 13–17 task returned to it because there was nothing left of it to do.
2. **The README headline figure (2.4–2.7×, measured 2026-07-23) is a single
   host-drifted wall-clock run and is internally inconsistent** with the
   CHANGELOG's own post-P3 claim (16 B 2.6×→1.60×, 256 B→parity) and with every
   earlier dated cold-direct table in `docs/ALLOC_BENCH.md`. The 256 B "2.71×
   slower" is a clear outlier (256 B cold was 1.06×–1.66× in every prior run).
3. **The cheapest next step is NOT another eureka** — it is (A) an honest
   paired A/B/B/A re-measurement to correct the drifted headline, and (B) wiring
   the existing iai `Ir` gate into CI (it currently runs only locally via
   `npm run iai`; CI has no perf-gate/iai/callgrind job at all).

---

## 1. What landed — every eureka, with the commit hash that proves it

Verified by `git log --all` + `git show`, not by trusting any doc's prose. Each
eureka maps to a real commit on the committed history:

| Phase | Task | Eureka(s) | Landed commit | Date | Status |
|---|---|---|---|---|---|
| P0 | #144 | measurement foundation (iai cold/churn benches) | `4908fce` | 2026-07-03 | ✅ landed |
| P1 | #145 | **Э2** one-branch resolver + **Э4** classify-once + **Э5** counter load/store + exact-256 B class | `38e1a44` (+ floor-fix `184123e`) | 2026-07-03 | ✅ landed |
| P2 | #146 | **Э3** own-segment cache | `3b9123e` | 2026-07-03 | ✅ landed (honestly modest; does not move headline) |
| P3 | #147 | **Э1** bump-direct batched carve (front A's main lever) | `671a81b` | 2026-07-03 | ✅ landed |
| P4 | #148 | (a) flush word-merge/chain-splice · (b) S3 virgin-skip · (c) TCACHE_CAP sweep | — (no single commit) | 2026-07-03..10 | ✅ **disposed by decision**, not dormant: (a) folded into Э8 (`flush_class` batch, P7.3); (b) **honest-rejected** — [`docs/checkpoints/2026-07-10-alloc-zeroed-virgin-skip-reject.md`](../checkpoints/2026-07-10-alloc-zeroed-virgin-skip-reject.md); (c) **honest-rejected** — `e6f5112` (task #206/PERF-2) + earlier `cf22c96` (R7-C1 NO-GO) |
| P5 | #149 | final verdict + README/ALLOC_BENCH/CHANGELOG tables | `2dede7d` | 2026-07-03 | ✅ landed |
| P6 | #150–#152 | **Э6** magazine-oracle rewrite (kills the 256 B churn loss, M2 strengthened) | (see CHANGELOG §4183–4208) | 2026-07-0x | ✅ landed |
| P7 | #159–#163 | **Э7** batch freelist drain · **Э8** batch flush · **Э9** classify+base once · **Э10** branchless M2 scan · **Э11** stamp-dedupe | `8e69bff` `ae7afe1` `e6a1eaf` (+ honest verdict `055061a`) | 2026-07-0x | ✅ landed |

**Conclusion of §1:** the plan's named task chain #144→#149 is **100 % landed**;
#148 (the only "optional" phase) was closed by two documented honest-rejects and
one fold-into-Э8. There is no un-started eureka in this plan. A later, separate
attempt to close the same cold gap — the `alloc-runfreelist` experiment (PERF-3)
— was **also honest-rejected** (`docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md`: it
regressed all 11 iai benches including the four cold/recycle targets). The cold
gap has had two serious swings taken at it; both are settled.

### Э1 is live in the current tree (hot-path verified, not just trusted)

`rg refill_class_bump` finds the Э1 carve path alive and evolved in
`src/alloc_core/alloc_core_small_magazine.rs:117` (`refill_class_bump`) →
`:177` (`refill_class_bump_impl`). The body (read at
`alloc_core_small_magazine.rs:195–275`) confirms the plan's design survived the
last 11 rounds intact and was *extended*, not reverted:
- **Source order preserved** (free-drain `drain_freelist_batch` → ring-draining
  `find_segment_with_free` → bump-carve) — exactly the plan's P3 non-negotiable.
- **Э7** (task #161) batched the freelist drain into one walk (`:217–223`).
- **E1 latch** (task "W4", `:225–234`) skips the per-iteration re-drain once
  `free_exhausted` — a pure tautology-removal layered on top of Э1.
- R13-3 (task #273) added a `virgin-zero-skip`-gated `_virgin_checked` variant
  (`:160–174`) — the carve path is still the active refill lever.

The tautological `carve → write_next → bitmap RMW → head-store → pop → read_next
→ bitmap RMW` round-trip the plan named (~40 instr/block) is **genuinely gone**.
What `refill_class_bump_impl` does for a virgin bump slice is `bump += n·block_size`
with the bitmap left untouched (bit 0 already correct) — the Э1 contract holds.

---

## 2. The cold-direct gap: the evidence across measurement dates

The plan's diagnosis was *"flat ~28 µs across all sizes ⇒ instruction-bound, not
page-fault-bound; mimalloc grows with size, we pay for ceremony."* The question
this investigation had to answer: **did Э1–Э11 actually close that gap, and is
the README's 2.4–2.7× the current truth?**

Pulled every dated cold-direct (`bench_direct_alloc`, no reuse) table from
`docs/ALLOC_BENCH.md` + the README cold-direct table (`README.md:716–725`):

| Run date | Provenance | Sefer 16 B | mimalloc 16 B | **16 B vs mimalloc** | **256 B vs mimalloc** |
|---|---|---|---|---|---|
| 2026-07-03 (post-P3/P5) | CHANGELOG §4246 claim | — | — | **1.60× slower** | **≈ parity (1.03×)** |
| 2026-07-10 (post-PERF-PASS) | ALLOC_BENCH §170+ | ~30.8 ns | ~11.5 ns | 2.67× slower | 1.52× slower |
| 2026-07-14 (post-round4) | ALLOC_BENCH §144–146 | ~30.8 ns | ~11.5 ns | 2.67× slower | 1.52× slower |
| 2026-07-14 (post-round5, R5-R3 method fix) | ALLOC_BENCH §62–66 | 36.4 ns | 14.6 ns | 2.49× slower | 1.66× slower |
| post-P7 (Э7–Э11) | ALLOC_BENCH §660–664 | ~21 (noisy 18–24) | ~14 | **~1.5× slower** | **~1.06× faster** |
| **2026-07-23 (post-Round13) — CURRENT README** | `README.md:716–725` | **70.9 ns** | **30.0 ns** | **2.37× slower** | **2.71× slower** |

**Three observations the table forces:**

1. **The 16 B gap never cleanly closed.** It sits between 1.5× and 2.7× depending
   on host noise — the plan's projected "parity/overtake (~14–18 µs)" was NOT
   durably achieved at the tiniest size. This part of the README is honest: a
   cold-tiny residual genuinely remains.
2. **The 2026-07-23 run is a host-drift outlier, and the README headline amplifies
   it.** Both allocators' absolute cold-direct times roughly *doubled* between
   07-14 and 07-23 (Sefer 16 B: 36.4→70.9 ns; mimalloc 16 B: 14.6→30.0 ns).
   That is the textbook signature of host-level session drift the docs themselves
   already name (ALLOC_BENCH §76–82: *"every absolute column moved up ~20–40 %
   including mimalloc's and System's own — signature of host-level noise"*). A
   change confined to SeferAlloc's code **cannot** move mimalloc's number 2×.
3. **The "256 B 2.71× slower" in the headline is the strongest outlier signal.**
   Cold 256 B was 1.52× / 1.66× / **1.06× faster** in the three prior runs; the
   2.71× figure has no corroboration anywhere else in the record. The headline
   *"2.4–2.7× cold gap at 16 B/256 B"* is constructed from the two worst rows
   (16 B + 256 B) of the single worst run, omitting the 64 B (1.00×) and 1024 B
   (1.26× faster) rows of the same table.

**Verdict:** the README headline figure is **not** the stable current truth — it
is a single noisy run, and its 256 B component specifically contradicts every
other measurement on record. The CHANGELOG's "16 B 2.6×→1.60× / 256 B parity"
is itself optimistic relative to the 07-14 runs (which show 256 B at 1.5–1.66×,
not parity), so *neither* document is fully consistent with the other. The
honest answer is *"cold 16 B is somewhere in 1.5×–2.5×; cold 256 B is ~1.0×–1.7×;
the wall-clock on this host cannot resolve it further."*

---

## 3. Is the "ceremony instruction-bound" diagnosis still valid for the *current* code?

**Partially — and this is the most important nuance the original plan did not
address.** Split into two claims:

### 3a. "We pay for ceremony" — CONFIRMED at the Ir level, and the ceremony WAS cut

The deterministic iai-callgrind `Ir` gate was run **locally** (not in CI — see
§4). `docs/perf/IAI_BASELINE.md` records real numbers, and a real P7 delta:

| bench | pre-P7 Ir | post-P7 Ir | Δ |
|---|---:|---:|---:|
| `cold_alloc_free_256x16b` | 129,863 | 123,516 | **−6,347 (−4.9 %)** |
| `cold_alloc_free_256x64b` | 129,373 | 123,023 | −6,350 |
| `recycle_alloc_free_256x16b` | 182,150 | 175,896 | −6,254 |
| `recycle_alloc_free_256x64b` | 181,678 | 175,418 | −6,260 |

Per-op (`IAI_BASELINE.md:99`): `cold_alloc_free_256x16b` = **204.5 Ir/op**,
`recycle_alloc_free_256x16b` = **207.4 Ir/op**, after subtracting the bootstrap
floor. So Э7/Э8/Э9/Э10 measurably removed ~5 % of the cold-path instruction
count — the "ceremony" framing was real and was partially collected on.

### 3b. "…vs mimalloc, who pays for bytes" — NEVER PROVEN, and structurally unprovable with the current gate

`rg mimalloc benches/perf_gate_iai.rs docs/perf/IAI_BASELINE.md` finds **no
mimalloc arm in the iai gate**. The gate measures SeferAlloc's `Ir` in isolation;
there is **no `mimalloc` Ir/op baseline for the same cold carve** anywhere in the
repo. So the plan's core comparative claim — *"flat ~28 µs = instruction-bound
relative to mimalloc"* — was always a **wall-clock inference**, never a measured
instruction-level fact. We know our cold path is ~204 Ir/op and that P7 shaved
~5 % off it; we do **not** know whether mimalloc's cold carve is 80 Ir/op
(ceremony-bound, headroom remains) or 200 Ir/op (we are already at the honest
per-block floor and the residual 16 B gap is page-fault/map-work, not ceremony).

**This is the single biggest open question the plan left on the table**, and it
is the reason the cold-16 B debate has been having the same wall-clock argument
for 10 rounds: nobody has the deterministic cross-allocator `Ir` number that
would settle it. Every wall-clock re-measurement (including the 2026-07-23 run
the README cites) is re-arguing the same noisy axis.

---

## 4. Why the gap is still "open" in the docs: the iai `Ir` gate is not in CI

`rg "iai|perf-gate|perf_gate|callgrind|valgrind" .github/workflows/ci.yml`
returns **nothing**. The deterministic gate exists as a *local* tool only —
`npm run iai` → `scripts/iai.mjs` (WSL + valgrind + callgrind, see its header
comment: *"the crate's iai-callgrind perf gate is Linux-only … on this Windows
dev host we can PROVE a perf change via WSL instead of waiting for Linux CI"*).
`npm run check` runs it locally, but **CI never regresses on cold-path `Ir`** —
a future change could silently re-add carved-out instructions to the cold path
and only a manual local run would notice.

This is exactly what every "pending the Linux Ir gate" sentence in the CHANGELOG
(`§4278`, `§4295`) and README (`:785–789`) has been deferring to — a gate that
was never wired up. The two existing weekly/dispatch CI jobs
(`numa-shim` at `ci.yml:998`, `feature-powerset` at `ci.yml:1133`) are the
established precedent for "scheduled + `workflow_dispatch`, not per-PR" perf-ish
jobs; an iai job would slot into the same shape.

---

## 5. The cheapest / highest-potential next step (PROPOSED, not implemented)

The task asked which *remaining* eureka is the cheapest next step. The honest
answer is **none** — the plan is exhausted (§1) and the one later attempt at the
same gap (PERF-3 run-freelist) was honest-rejected. The remaining leverage is
**measurement honesty + deterministic regression detection**, in two cheap,
zero-`src/`-risk steps. Recommend doing **A first** (it is the immediate honesty
fix the README needs), then **B** (the structural fix).

### Step A (cheapest, immediate) — paired A/B/B/A re-measurement + README headline correction

The 2026-07-23 headline figure is a single host-drifted run whose 256 B row
contradicts every prior measurement. Re-run cold-direct with the **paired
methodology already established for exactly this noise class**
(`docs/perf/R5_R2_CHURN_REGRESSION_PAIRED_AB.md` — 20 alternating A/B/B/A
repetitions to separate real effect from host drift), then correct
`README.md:238` and the cold-direct table at `README.md:716–725` to the
noise-corrected ratios. Expected outcome (based on the P7 + 07-14 runs): the
headline moves from "2.4–2.7× at 16 B/256 B" toward "1.5× at 16 B, ~1.0–1.7× at
256 B" — i.e. the gap shrinks on paper not because the code changed, but because
the measurement stops being a single worst-case snapshot. **This is the
"trivial README-number fix" the task permits as a separate small commit — do NOT
mix it into this status document's commit.** Risk: ~0 (docs-only, measurement).

### Step B (structural, the real fix) — wire the iai `Ir` gate into CI

Add a `perf-gate-iai` job to `.github/workflows/ci.yml` in the same
`schedule: weekly + workflow_dispatch` shape as the existing `numa-shim` and
`feature-powerset` jobs (NOT per-PR — iai-callgrind under valgrind is
slow). It runs `cargo bench --bench perf_gate_iai --features production` (or the
`npm run iai` equivalent) on `ubuntu-latest` with valgrind installed, and
regresses on the `Ir` of the four cold/recycle benches + `churn_256b` /
`churn_write_256b` against the `IAI_BASELINE.md` numbers. This closes §3b's open
question over time (the *first* run finally produces the deferred Linux `Ir`
baseline) and makes any future cold-path instruction regression a CI signal
instead of a 10-round wall-clock debate. Risk: CI-only, zero `src/` change.

**Why these two and not a new eureka:** every named tautology in the plan is
already removed (§1, §3a). What remains at cold-16 B is either honest per-block
page-map/fault work **or** a residual ceremony we cannot see without the
cross-allocator `Ir` number (§3b). Spending engineering on a *new* eureka before
resolving that measurement gap would be guessing; Step A removes the dishonest
headline now, Step B makes the next eureka (if one is ever warranted) a
data-informed decision instead of a wall-clock argument.

---

## 6. Proposed follow-up task (text, not registered)

> **Task (P2, ~half-day): wire the `perf_gate_iai` Ir gate into CI as a
> weekly + `workflow_dispatch` job, and capture the first Linux `Ir` baseline.**
> Add a `perf-gate-iai` job to `.github/workflows/ci.yml` mirroring the existing
> `numa-shim` / `feature-powerset` job shape (`schedule: cron '0 6 * * 1'` +
> `workflow_dispatch`, `runs-on: ubuntu-latest`, **not** per-PR — valgrind is
> slow). Install valgrind + the pinned `iai-callgrind-runner`, run
> `cargo bench --bench perf_gate_iai --features production`, and fail on an `Ir`
> regression beyond threshold against `docs/perf/IAI_BASELINE.md`'s recorded
> numbers (`cold_alloc_free_256x16b` 123,516; `recycle_alloc_free_256x16b`
> 175,896; `churn_256b` 80,672; etc.). **Deliverable:** the job is green on
> `main`, AND the first run's `Ir` output is committed as the canonical Linux
> baseline (resolving the "pending the Linux Ir gate" caveat repeated across
> CHANGELOG §4278/§4295 and README :785–789). **Explicitly out of scope:** adding
> a `mimalloc` arm to the iai gate (that is the §3b cross-allocator question —
> a separate, larger task; this one only stops the regression-detection gap).
> **Counterfactual:** before the job exists, a +5 % cold-path `Ir` regression
> ships silently; after, CI catches it.

---

## 7. Files inspected (evidence trail)

- `docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md` (full read — the plan,
  Э1–Э5, P0–P5).
- `CHANGELOG.md:4122–4298` (the P0–P7 arc narration, Э1–Э11, measured results).
- `README.md:215–252` (perf claims), `:716–725` (cold-direct table, 2026-07-23),
  `:831–850` (cold first-touch section), `:764–783` (the "where we still trail"
  paragraph that the `:238` headline summarises).
- `docs/ALLOC_BENCH.md:22–110` (post-round5 2026-07-14 cold-direct),
  `:106–168` (post-round4), `:170–225` (post-PERF-PASS 2026-07-10),
  `:655–706` (post-P7 cold-direct + verdict).
- `docs/perf/IAI_BASELINE.md:48–170` (the local iai `Ir` numbers + P7 delta).
- `src/alloc_core/alloc_core_small_magazine.rs:117–275` (Э1 `refill_class_bump`
  + `refill_class_bump_impl`, hot-path verified live + source-order preserved).
- `.github/workflows/ci.yml:555–590` (miri jobs), `:978–1133` (the two existing
  weekly/dispatch jobs that an iai job would mirror); `rg iai|perf-gate|valgrind`
  → empty (no perf-gate job exists).
- `benches/perf_gate_iai.rs:224–353` (the cold/recycle/churn benches exist; no
  `mimalloc` arm).
- `scripts/iai.mjs:1–40` (local-only WSL+valgrind runner).
- `docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md` (the later cold-gap attempt,
  honest-rejected).
- `docs/checkpoints/2026-07-10-alloc-zeroed-virgin-skip-reject.md` (P4(b) reject).

## 8. One-line summary for the final commit message

The mimalloc cold-gap plan is **fully landed** (Э1–Э5 + Э6–Э11, all cited to
commits); the README's 2.4–2.7× headline is a **single host-drifted run** whose
256 B row contradicts every prior measurement; the cheapest next step is a paired
re-measure + README correction (A) and wiring the existing local iai `Ir` gate
into CI (B) — not a new eureka.
