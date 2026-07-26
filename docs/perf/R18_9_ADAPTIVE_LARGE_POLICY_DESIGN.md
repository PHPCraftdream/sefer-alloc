# R18-9 — Adaptive Large policy: design (NOT implementation)

**Task:** R18-9 (task #335, P3 "design → prototype"). **DESIGN-ONLY.** No
`src/` change, no `Cargo.toml` change, no `tests/` change, no benchmark run.
This document proposes a design for a FUTURE round's measurement + (possibly)
refactor; it does not implement or benchmark anything itself. Modelled on
`docs/perf/R17_10_BATCHED_DEFERRED_RECLAIM_DESIGN.md` (the established
design-only precedent in this series).

**Date:** 2026-07-26. **Base revision:** `main` @ `912740f` (R18-2 just
landed — the fresh, leak-fixed numbers this design is built on).

---

## 0. Where this task comes from

The Round 18 plan (`docs/reviews/2026-07-25-r18-plan.md:109`, row R18-9)
names this a design task for a unified "adaptive Large policy" — *"measure
medium-to-Large promotion + geometric reserved capacity + lazy commit + cache
extension + budget AS A COORDINATED SYSTEM, not independent feature toggles,
since their interactions have repeatedly overturned isolated conclusions
(R14-4 vs R17-4's leak — the classic example)."* The plan explicitly scopes
this round to **design only, by analogy with R17-10**.

The plan's motivating premise is sound in its core claim: isolated feature
gates HAVE repeatedly produced conclusions that later interaction findings
overturned. The canonical instance, restated for grounding:

- **R14-4** (`docs/perf/R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md` §0/§5, task
  #289) measured `production,medium-classes` realloc at **~1,700–2,300×
  slower** than baseline and attributed it to "cache-slot pressure (16
  promoted objects, 8 slots)."
- **R17-4** (task #321, commit `1b761f4`) then found that number was
  **confounded by a real segment leak**: the dealloc-dispatch in
  `HeapCore::dealloc_own_thread_with_base` keyed on
  `SizeClasses::class_for(layout.size())` instead of segment `kind`, so a
  promoted-then-in-place-grown Large segment (whose contract-correct dealloc
  layout classifies small under `medium-classes`) was misrouted into the
  small magazine path and **leaked its 4 MiB span every round** — inflating
  COMMIT (1.3 GiB) without the gate's TIME number being attributable to the
  mechanism R14-4 blamed.
- **R18-2** (`docs/perf/R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md` §7.1/§10,
  task #331, this round) re-ran the exact R10-2 harness on post-R17-4 code:
  the leak is gone (commit 1.3 GiB → 49 MiB, OBSERVED), but the gate is
  **still RED** — ~1,180× slower for `production,medium-classes`,
  ~380× slower for `production,medium-classes,large-cache-extended`. The
  residual is now cleanly attributable to **structural promotion-copy cost**
  (the 256 KiB `memcpy` dense packing forces on a cross-class realloc-grow),
  NOT the leak and NOT cache-slot pressure alone.

**Finding up front, stated honestly (see §1):** part of the plan's premise —
that these are "five independently-gating feature mechanisms" that need
coordinating — is **not accurate as read from current `Cargo.toml`**. Two of
the five are not independent toggles at all, and one is not even a
Large-allocation mechanism. §1 inventories the mechanisms precisely, each
against the exact `Cargo.toml`/source line, before this design proposes how
to coordinate them.

---

## 1. Inventory — what is actually gated, and where

The task brief lists five mechanisms. Reading the current code, the honest
inventory is:

### 1.1 Mechanism 1 — `medium-classes` (the medium→Large promotion + medium density)

**Compile-time cargo feature, opt-in, NOT in `production`.**

- Feature definition: `medium-classes = ["alloc-core"]` (`Cargo.toml:474`).
- What it gates: `SMALL_MAX` 16 KiB → 1 MiB, `SMALL_CLASS_COUNT` 49 → 55
  (58 under `medium-classes-wide`, `Cargo.toml:475`), the `SIZE2CLASS` O(1)
  lookup table growing ~16 KiB → ~64 KiB `.rodata` (`Cargo.toml:463-469`).
- Stage-2 promotion: `MEDIUM_REALLOC_PROMOTION_THRESHOLD: usize = 256 * 1024`
  (`src/registry/heap_core_free.rs:75`), the promotion call site at
  `src/registry/heap_core_free.rs:854`/`:863`, and the
  `try_promote_to_large` private helper at `src/registry/heap_core_free.rs:1074`
  — all gated `#[cfg(feature = "medium-classes")]` (R14-4, task #289).

### 1.2 Mechanism 2 — `large-reserved-capacity` (+ its required `exact-span-large`)

**Compile-time cargo feature PAIR, opt-in, NOT in `production`.**

- `exact-span-large = ["alloc-core"]` (`Cargo.toml:312`): makes
  `alloc_large` round to the exact page-rounded request instead of a whole
  `SEGMENT` (4 MiB) multiple. This is the feature that *creates* the
  OPT-G in-place-grow headroom loss that `large-reserved-capacity` exists to
  counteract.
- `large-reserved-capacity = ["exact-span-large", "aligned-vmem/lazy-commit"]`
  (`Cargo.toml:357`): reserves (but does NOT commit) a geometric
  `reserved_capacity` span so a growing realloc commits the missing tail via
  ONE `VirtualAlloc(MEM_COMMIT)` and returns the SAME pointer, no copy.
- The two constants: `LARGE_RESERVED_CAP_GROWTH_FACTOR: usize = 4`
  (`src/alloc_core/alloc_core_large.rs:89`, raised 2→4 by R14-6/task #291)
  and `LARGE_RESERVED_CAP_BYTES: usize = 16 * SEGMENT`
  (`src/alloc_core/alloc_core_large.rs:42`, the cap). Used at
  `src/alloc_core/alloc_core_large.rs:421-422`.
- R14-6 (`docs/perf/R14_6_ADAPTIVE_RESERVED_CAPACITY_GATE.md` §0) showed the
  2x→4x change **inverted** the iai `realloc_grow` regression from +102.3%
  Ir to **−22.4% Ir** (treatment now FASTER than baseline) while leaving
  every RSS/commit/cache-hit axis numerically unchanged.

These two are a **feature PAIR, not one toggle**: `large-reserved-capacity`
REQUIRES `exact-span-large` (the mechanism exists to counteract exactly that
feature's OPT-G headroom loss — `Cargo.toml:347-350`). They must be
evaluated together; `exact-span-large` alone is the "problem arm"
(`docs/perf/R14_6_...` §2.2: 4 move legs / 0 in-place, +61% wall-clock).

### 1.3 Mechanism 3 — `large-cache-extended` (40-slot cache + finite budget default)

**Compile-time cargo feature, opt-in, NOT in `production`.**

- `large-cache-extended = ["alloc-decommit"]` (`Cargo.toml:371`): widens the
  fixed 8 base slots (`LARGE_CACHE_SLOTS`) via a lazily-materialised sidecar
  of 32 additional slots (40 total).
- R14-5 (`docs/perf/R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md`, task #290)
  hardened it: budget-vs-materialisation ordering (§1), finite default budget
  (§2), RSS/commit gate (§3), narrow-working-set correctness (§4),
  mixed-size/FIFO (§5), turnover-profile A/B (§6, `t=195.759`, sign 15/15 —
  a real win for turnover profiles; no benefit for static live-sets).
- **The budget is a RUNTIME knob, not a feature toggle** — see §1.5.

### 1.4 Mechanism 4 — `primordial-lazy-commit` — ALREADY IN `production`, and NOT a Large mechanism

**This is where the plan/brief's framing is inaccurate, and this design
flags it rather than silently re-bundling it.**

- `primordial-lazy-commit` is listed IN `production`:
  `production = ["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin",
  "alloc-segment-directory", "primordial-lazy-commit", "class-aware-dirty"]`
  (`Cargo.toml:399`). It was promoted by R12-9 (task #260,
  `docs/perf/R12_9_PRIMORDIAL_LAZY_COMMIT.md` §6: "GO... the orchestrator
  has NOT been asked to include `primordial-lazy-commit` in `production`" —
  it has since been included). **It is not an opt-in toggle whose
  coordination is an open question; it is already on by default.**
- More importantly, it is **not a Large-allocation mechanism at all.**
  `docs/perf/R12_9_PRIMORDIAL_LAZY_COMMIT.md` §1-§2 establishes it affects
  exactly the ONE-TIME primordial segment's initial reservation + the
  shared grow-on-carve frontier for SMALL segments; it is "structurally
  excluded from the decommit/pool lifecycle" and the primordial segment
  "is reserved exactly once at process start and lives for the process's
  entire lifetime" (§2). Its measured win is a ~5.1× smaller first-heap
  commit (§3.1), orthogonal to anything in the Large path.
- The split that produced it (R12-9) deliberately SEPARATED it from
  `small-segment-lazy-commit` (the opt-in sibling that DOES carry decommit/
  recommit surface on ordinary small segments) precisely so the
  Large-irrelevant, structurally-safe half could be promoted on its own.

**The lazy-commit concept that IS relevant to Large** is
`large-reserved-capacity`'s `aligned_vmem::reserve_aligned_lazy` /
`commit_range` substrate (`Cargo.toml:349`), but that is already counted as
mechanism 2's implementation, not a sixth mechanism. So in the honest
inventory, `primordial-lazy-commit` is **out of scope for Large-policy
coordination** — it neither participates in Large allocation nor is it an
open opt-in question.

### 1.5 Mechanism 5 — the finite byte budget (`DEFAULT_EXTENDED_BUDGET_BYTES`)

**Runtime-tunable knob (`.budget_bytes(n)`), NOT a separate feature.**

- `pub(crate) const DEFAULT_EXTENDED_BUDGET_BYTES: usize =
  DEFAULT_HEADROOM_BYTES` (`src/alloc_core/large_cache_config.rs:147`, =
  256 MiB since R17-9/task #326 cut it from the R14-5 5× default of 1280 MiB).
- This is the FALLBACK applied only when `large-cache-extended` is compiled
  in AND the caller never called `.budget_bytes(..)` — see
  `resolved_budget_bytes()` at `src/alloc_core/large_cache_config.rs:373`,
  which branches `#[cfg(feature = "large-cache-extended")]`.
- An explicit `.budget_bytes(n)` call (any `n`, including `0` or
  `usize::MAX`) always overrides the default — `LargeCacheConfig::budget_bytes`
  builder at `src/alloc_core/large_cache_config.rs:278`.

So mechanism 5 is **not an independent toggle**: it is the budget dimension
of mechanism 3 (`large-cache-extended`). It is, however, the ONE knob in
this whole set that is already runtime-configurable — which matters for §4's
policy-abstraction realism.

### 1.6 Honest re-count

Of the "five independently-gating mechanisms" the brief names, the accurate
picture is:

| # | Mechanism | Gate kind | In `production`? | Large-relevant? |
|---|---|---|---|---|
| 1 | `medium-classes` | compile-time feature | NO (opt-in) | YES (promotion + density) |
| 2 | `exact-span-large` + `large-reserved-capacity` | compile-time feature PAIR | NO (opt-in) | YES (in-place grow headroom) |
| 3 | `large-cache-extended` | compile-time feature | NO (opt-in) | YES (cache slots) |
| 5 | `.budget_bytes` / `DEFAULT_EXTENDED_BUDGET_BYTES` | runtime knob (part of #3) | (follows #3) | YES (cache RSS ceiling) |
| 4 | `primordial-lazy-commit` | compile-time feature | **YES** (R12-9) | **NO** (bootstrap/small-segment only) |

**Three opt-in Large features + one runtime knob** need coordinating. The
fifth item the brief names is either already-promoted-and-orthogonal (#4) or
not a separate toggle (#5 is #3's budget dimension). This does not weaken
the case for coordination — three features with non-trivial pairwise
interactions is still a real coordination problem — but it does mean the
"five switches" framing overstates the combinatorial space by ~40%, and any
measurement matrix built on the literal "five" would include a row
(`primordial-lazy-commit` ON, already the production default) that is a
no-op duplicate of the baseline.

---

## 2. The interaction problem — why isolated gates have misled

The plan's motivating claim ("interactions have repeatedly overturned
isolated conclusions") is correct. Three concrete instances, each with the
exact interaction that a single-feature gate would have missed:

### 2.1 R14-4 vs R17-4 — promotion × OPT-G in-place-growth × dealloc routing

The classic case. R14-4's "~1,700–2,300× realloc regression" was blamed on
cache-slot pressure. R17-4 (`docs/perf/R14_4_...` §2.2, now resolved) found
the real cause was a three-way interaction: `medium-classes` promotion
diverts a medium block to a 4 MiB Large segment; OPT-G then grows it in
place to a size ≤ `SMALL_MAX` while it STAYS a Large segment; the dealloc
layout classifies small; the fastbin dispatch keyed on `class_for` (not
`kind`) misrouted it into the small magazine path and **leaked**. No single
feature's own gate would have isolated this — it required all three of
(promotion + in-place growth + dealloc routing) to co-fire.

### 2.2 R18-2 — `large-cache-extended` materially changes the realloc verdict but cannot close it

This round's fresh data. R18-2 (`docs/perf/R14_4_...` §7.1, §10) measured
the SAME R10-2 harness against the leak-fixed code, for two feature
compositions:

| Arm B vs Arm A = `production` | realloc Δ (A−B) | realloc per-op (B) | B/A | `segments` | `commit` | hit-rate proxy |
|---|---:|---:|---:|---:|---:|---:|
| `production,medium-classes` | −66.06 ms | 67.6 µs | ~1,180× | 172 | 49 MiB | ~46% |
| `production,medium-classes,large-cache-extended` | −19.38 ms | 19.6 µs | ~380× | 20 | 81 MiB | ~94% |

(Dual-axis, same-vs-same control, SD/Δ resolvability all in §10; the
control's same-vs-same `t=0.364 ≪ crit` confirms the harness has no spurious
self-difference. Full numbers in
`docs/perf/R18_2_MEDIUM_REALLOC_GATE_RERUN_summary.csv`.)

**The interaction finding:** `large-cache-extended` is a **3.5× realloc
help** (66→19 ms) and raises the cache-hit proxy 46%→94% — but at the cost
of HIGHER resident commit (49→81 MiB, the RSS-for-fewer-OS-round-trips
trade-off), and it does NOT clear the 20% kill-gate (still ~380×). An
isolated `large-cache-extended` gate (R14-5) measured only the
**turnover profile** (§6, a clean win) and never this realloc-promotion
interaction — the interaction only surfaces when `medium-classes` is ALSO
on, which R14-5 never combined. This is exactly the class of finding a
coordinated matrix is meant to surface systematically rather than
discover round-by-round.

### 2.3 R14-6 — `large-reserved-capacity` growth-factor × doubling-cadence workload

R13-6 gave `exact-span-large`+`large-reserved-capacity` a CONDITIONAL-GO
with a +102.3% iai `realloc_grow` regression blamed on the fixed 2× ceiling.
R14-6 (`docs/perf/R14_6_...` §1.1) showed the regression was an interaction
between the 2× ceiling and a SPECIFIC workload shape (geometric doubling),
and raising the factor to 4× **inverted the sign** to −22.4%. The isolated
gate (R13-6) had measured the wrong factor for the wrong workload; the fix
was a constant change informed by modelling the actual doubling cadence.

### 2.4 Summary of what interactions have taught

Every one of these overturned an isolated-gate conclusion by exposing an
effect that only appears when two-or-more features co-fire. The pattern is
consistent enough that designing a SINGLE coordinated measurement matrix —
rather than rediscovering each interaction as a surprise in its own round —
is the methodological motivation for this whole task.

---

## 3. The coordinated measurement matrix

The goal: measure the three opt-in Large features (§1.6) as a coordinated
system, so interactions surface systematically. The matrix is over
**feature combinations**, not over individual toggles.

### 3.1 The feature combinations to measure

Five treatment arms, each paired against the same baseline arm A =
`production` (which already includes `primordial-lazy-commit`, §1.4 — it is
NOT a variable here). Combinations already measured by R18-2 are marked;
the rest are the gaps this matrix would close:

| Combo | Arm B features | Already measured? | What it isolates |
|---|---|---|---|
| C0 | `production` (= arm A) | control only (R18-2 §10.5) | same-vs-same harness honesty |
| C1 | `production,medium-classes` | **YES — R18-2** | promotion alone (the realloc-RED baseline) |
| C2 | `production,exact-span-large,large-reserved-capacity` | **NO** | reserved-capacity alone (the R14-6 axis, on current code) |
| C3 | `production,medium-classes,large-cache-extended` | **YES — R18-2** | promotion + extended cache (the 3.5× realloc interaction) |
| C4 | `production,medium-classes,exact-span-large,large-reserved-capacity` | **NO** | promotion + reserved-capacity (does headroom help the promotion memcpy?) |
| C5 | `production,medium-classes,exact-span-large,large-reserved-capacity,large-cache-extended` | **NO** | all three Large features together |

**Two clarifications on this matrix's design:**

1. **Why no row for `exact-span-large` alone or `large-cache-extended`
   alone.** R14-5 measured `large-cache-extended` on the turnover profile
   (§6) and R14-6 measured `exact-span-large`-alone as the "problem arm"
   (§2.2: 4 move legs / 0 in-place). Those isolated numbers exist; the
   matrix's job is the INTERACTIONS, so each row adds at least one feature
   to a row already measured. A future round can add the isolated rows if a
   regression is suspected, but they are not the coordination gap.
2. **Why C4 is the highest-information missing row.** C4
   (`medium-classes` + `large-reserved-capacity`) is the combination that
   directly tests R18-2's open question: does giving the promoted Large
   segment geometric reserved capacity reduce the residual memcpy? R10-2
   §5's mitigation #2 ("over-allocation within the medium class for growth
   headroom") is structurally what `large-reserved-capacity` provides for
   Large reallocs — but `large-reserved-capacity` operates on the
   Large-segment `reserved_capacity` field, and R14-4 §2.1 explicitly
   argued `alloc_large` already rounds to a whole `SEGMENT` (4 MiB) under
   `production`, so padding is "moot" UNLESS `exact-span-large` shrinks the
   span first. C4 turns `exact-span-large` ON, which is exactly the
   condition under which `large-reserved-capacity` could give a promoted
   block genuine growth headroom. **C4 is the single most likely row to
   move the R10-2 verdict** — and it has never been measured.

### 3.2 The workloads (three shapes, reused not reinvented)

Each combo is measured against THREE workload shapes, because the five
mechanisms help/hurt different shapes and a single workload has already
proven misleading (R14-5 §6.3's own caveat: a static live-set shows ~0 hits
and no difference; only the turnover profile shows the cache-extended win):

- **W1 — R10-2 realloc-heavy (16 objects, 8→40 cache slots, 256 KiB→768 KiB
  grow).** Reuse `examples/_shared/paired_ab_medium_workload.rs` and
  `scripts/r10_2_medium_gate.mjs` verbatim (the exact harness R18-2 ran,
  zero source/script changes). This is the realloc-kill-gate scenario.
- **W2 — R14-5 turnover profile (24 distinct Large sizes, batch
  alloc-all/dealloc-all, 200 timed rounds).** Reuse
  `examples/_shared/paired_ab_large_cache_extended_turnover_workload.rs`
  and `scripts/_r14_5_large_cache_extended_turnover_ab.json`. This is the
  cache-extended win scenario.
- **W3 — R14-6 doubling-cadence realloc chain (64 B → 4 MiB, 16 steps).**
  Reuse `benches/perf_gate_iai.rs::realloc_grow` (the iai judge) and the
  R12-4 3-arm harness. This is the reserved-capacity growth-factor scenario.

The full matrix is 3 combos-to-measure × 3 workloads = **9 primary A/B
sessions**, plus the 2 already-measured (C1, C3 on W1) re-usable as-is and
C0 same-vs-same controls per workload. At 20 A/B/B/A pairs × 80 launches
per phase (R18-2's protocol), each session is ~a few minutes of bench
time; the whole matrix is bounded, not open-ended.

### 3.3 What each combo is expected to reveal (hypotheses, to be confirmed not assumed)

- **C2 (reserved-capacity alone):** should reproduce R14-6's −22.4% iai
  win on W3 and R14-5's RSS parity. Low surprise risk; mostly a
  re-baseline on current code.
- **C4 (promotion + reserved-capacity):** THE open question. If
  `large-reserved-capacity`'s geometric headroom lets a promoted block
  grow in place past the 256 KiB promotion threshold without a second
  copy, the W1 realloc cost should drop materially below C1's 67.6 µs/op.
  If it does NOT drop, that is direct evidence the residual memcpy is
  structural to the FIRST promotion (the 256 KiB copy that happens AT
  promotion time, before any reserved-capacity headroom can help — because
  reserved-capacity is set on the FRESH Large segment, not retroactively
  on the medium block being moved out of). R18-2 §10.7 already leans this
  way ("the per-promotion cost is still... + a 256 KiB `copy_nonoverlapping`
  of the preserved prefix"), but it has not been MEASURED with
  reserved-capacity on.
- **C5 (all three):** the net. If C4 already closes most of the gap, C5
  adds the cache-extended 3.5× on top (commit trade-off included). If C4
  does NOT help, C5 ≈ C3 (the cache-extended number R18-2 already has).

---

## 4. Measurement protocol (reusing the established discipline)

Per CLAUDE.md's wall-clock-gate rules and the accumulated methodology
corrections (R14-3/task #288, R17-7, R18-2), every cell in the §3 matrix
must report:

1. **Dual-axis — sub-window AND full-round, same harness.** Per the
   "wall-clock gate must report both" rule (CLAUDE.md "Phased delivery"):
   W1's realloc phase is the sub-window that decides the kill-gate, but the
   full round (alloc+free+realloc) is the net. R18-2 §10.3 already showed
   combo C3 is ~break-even on the full round (19.02 ms vs 18.33 ms) while
   still ~380× on the realloc sub-window — a material gap that IS a result,
   not a detail to omit.
2. **Fixed-work, process-level A/B/B/A with in-process warm-up.** Reuse
   `scripts/paired-ab-runner.mjs`, 20 pairs (80 launches) per phase, with
   `PAIRED_AB_WARMUP_ROUNDS` (default 3, discarded — R17-7's correction to
   R14-3's original single-round design) before the single measured round.
   TWO independent repeats minimum (R14-3's two runs disagreed on
   significance; a single run is not sufficient evidence).
3. **SD/mean-delta resolvability check (the R17-7 check).** Report SD/|Δ|
   for every real phase. R18-2 §10.4 had SD/Δ = 2.9–11.7% for every real
   phase (Δ was 8–34× its own SD); the same-vs-same control correctly
   read "not resolvable" (SD/Δ = 1229%, t≈0). Any cell where SD > |Δ| is
   an honest "host could not resolve this effect," not a null result.
4. **Same-vs-same control (off vs off) per workload.** The harness-honesty
   check R14-3 §2.2 / R17-7 §2.4 / R18-2 §10.5 all required. Run alongside
   every A/B comparison.
5. **Environment-load disclosure BEFORE measuring.** Per R17-7's fix:
   check `wmic cpu get loadpercentage` (or equivalent) and report it
   alongside the numbers. R18-2 ran at 66–94% CPU (high, shared dev-host)
   and reported it rather than re-chasing a clean run.
6. **iai/Callgrind deterministic instruction count** on W3 (the
   doubling-cadence chain) — the deterministic judge this project
   designates as PASS/FAIL authority when wall-clock and iai disagree in
   magnitude (`scripts/iai.mjs`'s module doc). R14-6's whole
   constant-change argument rested on this axis.
7. **Raw logs + machine-readable summary, per the raw-log policy.** Cited
   `_raw_*.log` files `git add -f`'d alongside the report (truncation
   allowed per R14-10/task #295), plus a companion
   `_summary.csv` with commit/features/CPU/sample-count/key numbers
   (CLAUDE.md "machine-readable summary" rule; see
   `docs/perf/R18_2_MEDIUM_REALLOC_GATE_RERUN_summary.csv` for the
   established shape).

---

## 5. Policy abstraction — could these become a unified runtime profile?

The plan asks this design to evaluate "how a 'policy' abstraction at the
API/config level might look if these mechanisms were unified — e.g. a
single tunable profile (`LargePolicy::Balanced`/`::Throughput`/`::Memory`)
instead of N independent cargo features." This section assesses realism
honestly, by mechanism.

### 5.1 What is already runtime-tunable (the precedent)

`LargeCacheConfig` (`src/alloc_core/large_cache_config.rs:177`) is the
established runtime-config pattern in this codebase — a `const`-buildable
builder threaded through `AllocCore::new_with_config` /
`SeferAlloc::with_config`. Its current knobs (all `alloc-decommit`-gated):

- `.budget_bytes(n)` (`:278`) — the cache RSS ceiling (mechanism 5).
- `.headroom_bytes(n)` (`:291`) — the decay anti-thrashing floor.
- `.decay_interval_ms(ms)` (`:305`) / `.decay_rate_percent(pct)` (`:321`)
  — the decay cadence.
- `.mode(LargeCacheMode::Lazy)` (`:339`) — `#[non_exhaustive]`, room for a
  future background-scavenger variant.
- `.pool(SmallSegmentPoolConfig)` (`:353`) — the empty-segment hysteresis
  pool (Mechanism 2, task #51).

**This IS the pattern a unified `LargePolicy` would generalise.** A
`LargePolicy` enum whose variants resolve to a `LargeCacheConfig`-shaped
builder is architecturally consistent with what already exists — for the
knobs that are already runtime.

### 5.2 What is structurally compile-time (the hard part)

Three of the mechanisms are NOT runtime knobs and cannot become so without
material rework, because they affect **codegen, struct layout, and const
tables**:

- **`medium-classes`** changes `SMALL_MAX` (16 KiB → 1 MiB),
  `SMALL_CLASS_COUNT` (49 → 55/58), and the `SIZE2CLASS` O(1) lookup table
  (~16 KiB → ~64 KiB `.rodata`, `Cargo.toml:463-469`). That table is a
  `const` built at compile time and indexed on the hot alloc fast path. A
  runtime `medium-classes` would mean replacing the const table with a
  runtime-computed `class_for` (or a runtime-built table) on EVERY small
  allocation — a hot-path regression the feature exists to AVOID. The
  promotion call site (`heap_core_free.rs:854`) is also a
  `#[cfg(feature = "medium-classes")]` branch, not a runtime flag.
- **`large-cache-extended`** the slot COUNT (8 base `[Slot; 8]` vs the
  lazily-materialised 32-slot sidecar). The base array is fixed-size in
  `AllocCore`'s layout; the sidecar is already dynamic, so the COUNT is
  the closest to runtime-tunable of the three — but the base 8 is still a
  layout constant. (The budget already IS runtime, §5.1.)
- **`exact-span-large` + `large-reserved-capacity`** change the `usable`
  rounding in `alloc_large` (whole-SEGMENT vs exact) and add the
  `reserved_capacity` field + reserve/commit-range calls. The growth
  FACTOR (`LARGE_RESERVED_CAP_GROWTH_FACTOR`, `alloc_core_large.rs:89`)
  is a single `const` multiply in the slow path — this one COULD plausibly
  move to a runtime config field relatively cheaply. But
  `exact-span-large`'s rounding change is a codegen fork (`#[cfg]`), and
  the `reserved_capacity` field's presence/absence affects `SegmentHeader`
  layout.

### 5.3 Realism verdict

A fully-unified runtime `LargePolicy` is a **worthy north star but NOT a
near-term refactor.** Concretely:

- **Feasible now (incremental, low-risk):** unify the already-runtime knobs
  — cache budget, headroom, decay, and plausibly the reserved-capacity
  growth factor — into a `LargePolicy`-shaped profile that resolves to a
  `LargeCacheConfig`. This generalises the existing pattern, adds no new
  `unsafe` surface, and lets a caller pick `Throughput` (larger budget +
  wider growth factor) vs `Memory` (smaller budget, tighter factor) at
  construction time. **Estimated effort: one config-type extension +
  resolution logic, mirroring `LargeCacheConfig`'s own shape. No struct-
  layout change, no hot-path change.**
- **NOT feasible without major rework:** making `medium-classes`,
  `exact-span-large`, or the cache slot COUNT runtime-tunable. Each touches
  const tables, struct layouts, or hot-path codegen. These would have to
  stay as cargo features OR be replaced by an "always-compiled, runtime-
  gated medium path" — which means paying the medium path's codegen/table
  cost in EVERY build (the opposite of what the `#[cfg]` gate achieves) and
  is a multi-phase refactor of its own, larger than this round.

**Recommendation:** do NOT attempt the full runtime unification in the next
round. Instead, after the §3 matrix is measured, consider a NARROW
`LargePolicy` that covers only the already-runtime knobs (§5.3 feasible-now
subset), documented as "the runtime-tunable subset; the compile-time
features remain independent opt-ins a caller composes at build time." This
matches the codebase's existing precedent (the `large-cache-extended` budget
is already a runtime knob layered on a compile-time feature) rather than
inventing a new abstraction tier.

### 5.4 Sketch (illustrative, not a spec)

For concreteness only — this is NOT a proposed API, and no signature here
is committed. The shape a narrow runtime `LargePolicy` might take, building
on `LargeCacheConfig`:

```text
#[non_exhaustive]
enum LargePolicy {
    /// Default. Finite cache budget, moderate growth factor.
    /// Resolves to today's LargeCacheConfig::DEFAULT under large-cache-extended.
    Balanced,
    /// Larger cache budget + wider reserved-capacity growth factor,
    /// for turnover-heavy / realloc-grow-heavy workloads. Trades RSS for hits.
    Throughput,
    /// Smaller cache budget + tighter factor, for memory-constrained deploys.
    Memory,
}

impl LargePolicy {
    const fn resolve(self) -> LargeCacheConfig { /* maps to budget_bytes / growth_factor */ }
}
```

The `growth_factor` dimension does not exist on `LargeCacheConfig` today
(`LARGE_RESERVED_CAP_GROWTH_FACTOR` is a module const) — adding it is the
one small extension beyond the existing pattern, and only makes sense AFTER
§3's C4 measurement confirms a runtime growth factor is worth exposing (it
may turn out the compile-time 4× is fine for everyone, making the knob
unnecessary).

---

## 6. The structural memcpy barrier — what a unified policy does NOT solve

**This is the most important caveat in this document, and the plan brief
explicitly asks it be marked.**

R18-2's central finding (`docs/perf/R14_4_...` §7.1, §10.7) is that the
residual ~19–67 ms realloc cost is **structural promotion-copy cost** —
the 256 KiB `copy_nonoverlapping` of the preserved prefix that dense
packing forces on a cross-class realloc-grow — and it is NOT removable by
any of the three opt-in Large features this design coordinates:

- `large-cache-extended` removes the OS-reservation miss penalty (94% hits,
  66→19 ms) but **cannot remove the memcpy** — every promotion STILL copies
  256 KiB (R18-2 §10.7: "the per-promotion cost is still... + a 256 KiB
  `copy_nonoverlapping` of the preserved prefix").
- `large-reserved-capacity` gives the FRESH Large segment geometric
  headroom for SUBSEQUENT grows, but the promotion copy happens AT
  promotion time (moving the prefix OUT of the medium segment INTO the
  fresh Large span) — before any reserved-capacity headroom can help. C4
  (§3.1) is the measurement that would confirm or refute this, but R18-2's
  mechanism analysis already leans "does not help the first copy."
- `exact-span-large`, `primordial-lazy-commit`, and the budget are
  orthogonal to the copy entirely.

**Therefore: a unified `LargePolicy` — however well-coordinated — does NOT
close the R10-2 realloc kill-gate by itself.** Coordination of the existing
levers is about getting the BEST achievable trade-off among them (cache
hits vs RSS vs growth headroom), not about eliminating the structural copy.
Eliminating the copy requires a NEW mechanism, none of which the five
existing mechanisms provide:

Per R10-2 §5 (`docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md:338-356`),
"What would change the verdict" names exactly two mitigations plus one
accept-the-trade-off option:

1. **In-place medium-class grow within a segment** (`R10_2...:343-347`):
   if a realloc-grow target class has free space in the SAME segment the
   block already lives in, carve the new slot in place and copy within the
   segment — avoiding the Large-segment round-trip entirely.
2. **Over-allocation within the medium class for growth headroom**
   (`R10_2...:348-352`): give each medium-class block growth headroom
   (e.g. 1.25× requested) so a single realloc-grow step stays within the
   same class (in-place fast path, OPT-F) — the `Vec` doubling trade-off,
   internal fragmentation for realloc speed.
3. **Accept the trade-off** (`R10_2...:353-356`): for workloads that do
   not realloc heavily in the 256 KiB–1 MiB range, the regression is
   irrelevant.

**Neither #1 nor #2 is designed here, and neither is any of the five
existing mechanisms.** R18-2 §7.1's own refined recommendation states this
explicitly: *"closing it would require one of R10-2 §5's mitigations
(in-place medium-class grow within a segment, or growth headroom /
over-allocation within the medium class) — none of which R17-4 or R18-3
implemented."* This design adds nothing to that set. A separate design task
(analogous to this one, but for the in-place-medium-grow mechanism) is the
prerequisite to any path that actually flips the R10-2 verdict; this
document deliberately does NOT scope that work.

**Scope boundary, stated plainly:** the unified `LargePolicy` this document
proposes is about **COORDINATION of the existing levers**, not about
**SOLVING the structural problem** that keeps the R10-2 gate RED. Those are
two different problems; conflating them would repeat exactly the
isolated-gate overclaim pattern (§2) this task exists to correct.

---

## 7. What this design does NOT do

- **No `src/` change.** No code is written or modified.
- **No `Cargo.toml` change.** `production = [...]` (`Cargo.toml:399`) is
  untouched; no feature is promoted, demoted, added, or aliased.
- **No benchmark run.** The §3 matrix is PROPOSED, not executed. The
  already-measured cells (C1, C3 on W1, from R18-2) are cited, not re-run.
- **No new mechanism designed.** The in-place-medium-grow / growth-headroom
  mechanisms (R10-2 §5 #1/#2) are explicitly out of scope (§6).
- **No full runtime unification.** The §5.3 "not feasible without major
  rework" subset is identified, not attempted.

---

## 8. Risks / open questions

1. **C4 may not help the promotion copy (§3.3's central hypothesis).**
   R18-2's mechanism analysis predicts `large-reserved-capacity` cannot
   help the FIRST promotion copy (the copy happens before the fresh
   segment's reserved capacity is set). If C4 confirms this, the
   coordinated matrix's most-likely-to-flip-the-gate cell is a null, and
   the honest conclusion is that the existing three features are already
   well-coordinated (R18-2 measured the best achievable combination) and
   the ONLY remaining lever is R10-2 §5's not-yet-designed mechanism. That
   is a legitimate, valuable null result — it closes the "does headroom
   help promotion?" question definitively rather than leaving it as the
   plan's unstated assumption.
2. **The matrix's workload shapes may miss a real deployment's realloc
   intensity.** R10-2 §5's break-even analysis (`R10_2...:329-336`) shows
   medium-classes is a net win below ~205 reallocs-per-alloc/free-cycle;
   the W1 harness sits well above break-even by design. A deployment in
   the buffer-construction or steady-state-churn profile (R10-2's table)
   would see only the alloc/free wins. The matrix should report the
   break-even framing alongside the kill-gate framing, not just the
   kill-gate — otherwise it reproduces R10-2's own "percentage frame is
   degenerate" caveat (`R10_2...:372-378`) without addressing it.
3. **The runtime `LargePolicy` (§5.3 feasible-now subset) risks adding a
   knob no caller uses.** The codebase has exactly ONE runtime-config type
   today (`LargeCacheConfig`); adding a `LargePolicy` enum that just
   resolves to a `LargeCacheConfig` is only worth it if the named variants
   encode a non-trivial DEFAULT most callers want. If the measurements
   show no single "balanced" default dominates, the enum is ceremony over
   the existing builder. This should be decided AFTER the matrix, not
   before.
4. **`primordial-lazy-commit` is already in `production` (§1.4).** Any
   future matrix row or policy variant that treats it as an opt-in toggle
   is a no-op duplicate. This design excludes it; a future implementer
   who re-includes it (e.g. by copying the plan's literal "five
   mechanisms" list into a harness config) would measure a row identical
   to the baseline and waste a session.
5. **Plan-premise accuracy (flagged, not hidden).** The plan row R18-9
   (`docs/reviews/2026-07-25-r18-plan.md:109`) and the task brief both
   name "five independently-gating feature mechanisms." Per §1.6, the
   accurate count is three opt-in Large features + one runtime knob,
   because (a) `primordial-lazy-commit` is already promoted and is not a
   Large mechanism, and (b) the budget is `large-cache-extended`'s runtime
   dimension, not a separate toggle. This does not invalidate the task —
   three features with non-trivial pairwise interactions is still a real
   coordination problem — but a matrix built on the literal "five" would
   carry two no-op rows. §3.1's matrix is built on the accurate three.

---

## 9. Recommendation / next step

**Next step (a future round, not this one): execute the §3 matrix.**

The single highest-information missing measurement is **C4**
(`production,medium-classes,exact-span-large,large-reserved-capacity` on
W1, the R10-2 realloc-heavy harness). It directly tests whether
reserved-capacity headroom can reduce the structural promotion memcpy —
the open question R18-2 identified but did not measure. It requires no new
harness (reuse `scripts/r10_2_medium_gate.mjs` with a different feature
set, exactly as R18-2 did for C3), and its result is binary-ish: either
the realloc cost drops materially (headroom helps → coordinated policy is
meaningful and the gate may move), or it does not (headroom cannot help
the first copy → the existing features are already at their coordinated
best, and the ONLY path forward is R10-2 §5's new mechanism).

**If C4 is a null (the predicted outcome):** the unified-policy work is
still worth doing for COORDINATION value (getting the cache-extended ×
budget × growth-factor trade-off into one named profile), but it should be
explicitly framed as "best-achievable trade-off among existing levers,"
NOT as the path to clearing R10-2. The path to clearing R10-2 is a
SEPARATE design for in-place medium-class grow (R10-2 §5 #1), which is the
genuine blocker and which no amount of existing-lever coordination
addresses.

**If C4 helps:** the coordinated matrix becomes the evidence base for a
real `LargePolicy` recommendation, and §5.3's feasible-now runtime profile
is worth building in the round after that.

In both branches, the structural-memcpy caveat (§6) stands: a unified
policy coordinates existing levers; it does not invent the new one the
R10-2 gate actually needs.

---

## 10. Files/lines this document is grounded in (for the next round's reader)

**Feature definitions (`Cargo.toml`):**
- `:312` — `exact-span-large = ["alloc-core"]`
- `:357` — `large-reserved-capacity = ["exact-span-large", "aligned-vmem/lazy-commit"]`
- `:371` — `large-cache-extended = ["alloc-decommit"]`
- `:399` — `production = [...]` (includes `primordial-lazy-commit`, NOT the
  three opt-in Large features)
- `:474` — `medium-classes = ["alloc-core"]`
- `:463-469` — `medium-classes`'s `SIZE2CLASS` rodata cost (~16 KiB → ~64 KiB)

**Source constants / call sites:**
- `src/registry/heap_core_free.rs:75` — `MEDIUM_REALLOC_PROMOTION_THRESHOLD`
- `src/registry/heap_core_free.rs:854`/`:863` — promotion call site
- `src/registry/heap_core_free.rs:1074` — `try_promote_to_large`
- `src/alloc_core/alloc_core_large.rs:42` — `LARGE_RESERVED_CAP_BYTES`
- `src/alloc_core/alloc_core_large.rs:89` — `LARGE_RESERVED_CAP_GROWTH_FACTOR` (= 4)
- `src/alloc_core/alloc_core_large.rs:421-422` — growth-factor use site
- `src/alloc_core/large_cache_config.rs:48` — `DEFAULT_HEADROOM_BYTES` (256 MiB)
- `src/alloc_core/large_cache_config.rs:147` — `DEFAULT_EXTENDED_BUDGET_BYTES`
- `src/alloc_core/large_cache_config.rs:177` — `LargeCacheConfig` struct
- `src/alloc_core/large_cache_config.rs:278` — `.budget_bytes()` builder
- `src/alloc_core/large_cache_config.rs:373` — `resolved_budget_bytes()`

**Measurement-methodology precedent (reused, not reinvented):**
- `docs/perf/R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md` §7.1/§10 — R18-2's
  fresh numbers (C1, C3 on W1), the dual-axis + SD/Δ + same-vs-same
  protocol, the structural-memcpy root-cause analysis.
- `docs/perf/R18_2_MEDIUM_REALLOC_GATE_RERUN_summary.csv` — machine-readable
  companion (the shape a future matrix summary would follow).
- `docs/perf/R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` §6 — turnover
  profile (W2), the `large-cache-extended` isolated gate.
- `docs/perf/R14_6_ADAPTIVE_RESERVED_CAPACITY_GATE.md` §1.1/§2.1 —
  doubling-cadence chain (W3), the growth-factor 2x→4x inversion.
- `docs/perf/R12_9_PRIMORDIAL_LAZY_COMMIT.md` §1/§2/§6 — the
  `primordial-lazy-commit` split + already-promoted status (§1.4's basis).
- `docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md` §5 (`:338-356`) — the
  two mitigations + break-even analysis (§6's structural-barrier basis).
- `docs/perf/R17_10_BATCHED_DEFERRED_RECLAIM_DESIGN.md` — the design-only
  format/precedent this document follows, including its own §1.1
  "correcting the plan's premise" discipline.
- `scripts/r10_2_medium_gate.mjs`, `scripts/paired-ab-runner.mjs`,
  `scripts/iai.mjs` — the harnesses the §3 matrix reuses verbatim.
