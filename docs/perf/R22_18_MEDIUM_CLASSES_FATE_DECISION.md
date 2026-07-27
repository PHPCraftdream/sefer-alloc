# R22-18 — Product fate of `medium-classes`: decision record

**Task:** R22-18 (task #369). **DECISION-ONLY, docs-only.** No `src/` change,
no `Cargo.toml` change, no CI change, no test change. This document reads the
full evidentiary trail (5 cited reports below, all read in full, not
excerpted from memory) and records a recommendation for the human
orchestrator to accept or override.

**Date:** 2026-07-26. **Base revision:** `main` @ `ff48029` (R22-16, task
#367, the immediately preceding commit; R22-17, task #368, is queued but not
yet landed at the time of this reading).

---

## 0. Headline recommendation

**Recommend (b) — formally document `medium-classes` as a named opt-in
workload profile, not (a) ship-in-production and not (c) reject-and-remove.**
Full reasoning in §3. The short version: the alloc/free win is real, large,
reproduced across 3+ independent measurements, and has *never once* been
contradicted; the realloc loss is equally real and has failed to clear its
kill-gate in **4 independent attempts across 3 rounds** using 4 structurally
different mitigation strategies (cache-size scaling, destination headroom,
in-place-grow preconditions, and now — closed-form, not even empirically —
OS-level remap). At some point a decision-maker has to conclude the realloc
axis is not merely "still open," it is **structurally closed** for the
dense-packing design `medium-classes` uses, and stop paying the
re-measurement tax every round. But "structurally closed on realloc" is not
the same claim as "the whole feature is worthless" — the alloc/free win
targets a real, different, non-realloc-heavy workload class, and the honest
disposition for a feature with a real win AND a real, permanent, documented
limitation is a documented profile, not silent removal of the win alongside
the loss.

---

## 1. Evidence summary — each number re-verified against its source, not paraphrased from the task prompt

### 1.1 The original alloc/free win — `R10_2_MEDIUM_CLASSES_NATIVE_GATE.md` (task #228, 2026-07-21)

Process-level A/B/B/A paired wall-clock judge, 240 independent process
launches (20 pairs × 4 launches × 3 phases), `production` (Large path) vs
`production,medium-classes` (small/medium path), 16-simultaneously-live
256 KiB–1 MiB objects (2× `LARGE_CACHE_SLOTS = 8`, so the baseline cannot
hide behind a warm cache):

| Phase | A (baseline) | B (medium-classes) | Ratio | Statistics |
|---|---:|---:|---:|---|
| **Alloc** | 9.6 µs/alloc | 310 ns/alloc | **~31× faster** | t=55.758 (crit 2.101), sign 20/20 |
| **Free** | 43.5 µs/free | 207 ns/free | **~211× faster** | t=88.289, sign 20/20 |
| **Realloc** | 39 ns/realloc | 82.3 µs/realloc | **~2,111× SLOWER** | t=−53.607, sign 20/20 |

Segment-count density win: 329 (baseline) → 11 (medium-classes) for the same
working set. §5 of that report derives a break-even: medium-classes' alloc+
free savings (~16.9 ms per alloc/free cycle) divide by its realloc cost delta
(~82.3 µs/op) to give **~205 reallocs-per-cycle** as the break-even point —
below that, medium-classes wins net; above it, baseline wins net. This
break-even number is the load-bearing artifact for option (b)'s "break-even
curve" ask (§3.2 below) — it already exists, from the very first gate
report, and none of the 3 subsequent re-measurements changed the mechanism
that produces it (only the realloc side moved, via cache-size and headroom
experiments; the alloc/free side was never touched by any of them).

**This win has never been contradicted.** Every subsequent report (R14-4,
R18-2, R20-2, R21-2) that also measured alloc/free phases reproduced it
within measurement noise (~31×/~211× reappear in R14-4 §0 rows 2-3; R18-2
§10.3's alloc/free deltas are t≥50, sign 20/20 every time; R20-2 §3's alloc/
free rows are t≥52, sign 20/20 every time). This is the single most
reproduced result in this entire trail.

### 1.2 R18-2's re-run — embedded in `R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md` §7.1/§10 (task #331, 2026-07-26)

There is no separate `R18_2_*.md` file — R18-2's re-run is §7.1/§10 of
`R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md` (confirmed by grep: the only R18-2
artifacts are `R18_2_MEDIUM_REALLOC_GATE_RERUN_summary.csv` +
`_raw_r18_2_*.log`, all cited from and folded into R14-4's own document).
Re-run on post-R17-4-leak-fix, post-R18-3-`kind_at`-narrowing code (`main` @
`912740f`), same exact harness (`scripts/r10_2_medium_gate.mjs --pairs 20`),
three feature compositions:

| Arm B (treatment) | realloc mean Δ (A−B) | realloc per-op (B) | **B/A ratio** | segments (B) | commit (B) | kill-gate (20%) |
|---|---:|---:|---:|---:|---:|:---:|
| `production,medium-classes` | −66.06 ms | 67.6 µs/realloc | **~1,180×** | 172 | 49 MiB | **FAIL (RED)** |
| `production,medium-classes,large-cache-extended` | −19.38 ms | 19.6 µs/realloc | **~380×** | 20 | 81 MiB | **FAIL (RED)** |
| control (production vs production) | +0.0006 ms | — | — | 329 | 34 MiB | n/a (harness-honesty PASS, t=0.364≪2.101) |

The task prompt's "~1,180x" figure is confirmed exactly (§7.1's table, row
1). `large-cache-extended` (8→40 cache slots) cuts the gap ~3.5× but the
residual is confirmed structural (§10.7): "the per-promotion cost is still
(alloc_large... ) + a 256 KiB `copy_nonoverlapping`... the time regression is
essentially unchanged." Root cause: the leak fix (R17-4) only fixed COMMIT,
never TIME — the promotion memcpy's cost was never touched by any code
change up to this point.

### 1.3 R20-2 — `R20_2_C4_RESERVED_CAPACITY_HEADROOM_GATE.md` (task #347, 2026-07-26)

Tests whether `large-reserved-capacity` (geometric growth headroom on the
freshly-promoted Large segment) reduces the promotion memcpy. Decisive test
— direct, load-matched, paired C1 (`production,medium-classes`) vs C4
(`production,medium-classes,exact-span-large,large-reserved-capacity`):

| | mean Δ (C1−C4) | SD | t | crit (p<0.05) | sign (C1/C4) | significant? |
|---|---:|---:|---:|---:|:---:|:---:|
| realloc phase, 20 pairs | +967 µs | 3.577 ms | **1.209** | 2.101 | 10/20 · 10/20 | **NO** |

**Verdict: NULL.** C1 per-op 49.6 µs vs C4 48.6 µs — a ~2% gap, SD is 370% of
the delta (unresolvable), sign test dead-even. Confirmed mechanistically
(§6.2): the promotion memcpy happens *before* the fresh Large segment's
reserved-capacity headroom is ever consulted, so headroom can only help a
*subsequent* grow, never the copy that created the promoted block. A real
but orthogonal win was found on **commit charge** (not RSS, not realloc
speed): `exact-span-large` roughly halves commit (C1 ≈50.5 MiB → C4 ≈23.9
MiB) — this does not move the kill-gate at all.

### 1.4 R21-2 — `R21_2_OPT_H_STAGE1_HIT_RATE.md` (task #351, 2026-07-26)

Diagnostic-only Stage-1 hit-rate counters for OPT-H (a proposed in-place
tail-of-segment cross-class grow — the design's actual name for what the
task prompt calls "in-place medium grow"), run against two harnesses,
**zero behavior change**, counters only:

| Harness | attempts | hits | hit rate |
|---|---:|---:|---:|
| R10-2's 16-object adversarial harness | 320 | 0 | **0%** |
| R21-1's single-hot-buffer harness (the "friendliest" case OPT-H was designed for) | 20 | 0 | **0%** |

**0/320 and 0/20 confirmed exactly as the task prompt states.** Both are
structural zeros, root-caused (§4): every round's only OPT-H-eligible
attempt lands at offset 262144 (256 KiB), and `262144 % 327680 (320 KiB) ≠ 0`
— precondition 4 (new-class alignment) fails identically, every round, on
both harnesses, because the harness's reset mechanics always re-promote from
the same non-alignment-friendly carve position. Design's own trigger
("material majority... most grows... take the fast path") explicitly NOT
met. Recommendation: **NO-GO for implementing OPT-H's real grow action.**

### 1.5 R22-6 — closed-form LCM proof, folded into `R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` §2.1 addendum + `OPEN_ITEMS.md`'s "Recently resolved" trail (task #357, 2026-07-26)

Not a re-measurement — a closed-form arithmetic proof that OPT-H's own two
preconditions (tail-adjacency + new-class alignment) jointly force the carve
offset to be a multiple of `lcm(block_size(old_class), block_size(new_class))`
for the six medium classes (256/320/384/512/768/1024 KiB). Verified
independently in this reading (re-derived from `OPEN_ITEMS.md`'s own table,
cross-checked against `size_classes.rs`'s stated `EXTRAS`):

| transition | lcm | legal offsets in one 4 MiB segment |
|---|---|---:|
| 256K→320K | 1.25 MiB | 2 |
| 320K→384K | 1.875 MiB | 1 |
| 384K→512K | 1.5 MiB | 2 |
| 512K→768K | 1.5 MiB | 2 |
| 768K→1M | 3.0 MiB | 1 |

Chaining across all six classes needs `lcm(4,5,6,8,12,16) = 240` units =
**15 MiB**, far exceeding the 4 MiB segment — no single offset supports a
full ladder walk, and even 3 consecutive stages (256→320→384) fail
precondition 5 (segment capacity) at the only surviving offset. **Structural
conclusion: the medium ladder allows at most one cross-class hop per segment
lifetime, from a small fixed set of carve positions, independent of harness
design** — this is a mathematical bound, not an empirical one; no future
harness redesign can move it. This closes item 1 as NO-GO on geometric
grounds specifically for the medium ladder (the sub-16 KiB geometric ladder
has friendlier ratios and is left as a separate, low-priority, unrelated
item — `OPEN_ITEMS.md`'s `[L]` tier item 11 — not a medium-classes lever).

### 1.6 R22-16 — `R22_16_PROMOTION_REMAP_DESIGN.md` (task #367, 2026-07-26)

Design-only investigation of whether an OS-level VA-remap primitive
(`mremap` / Windows placeholder-VA) could eliminate the promotion memcpy
entirely, rather than working around it. **Verdict: NO-GO for remap-in-place
under the current segment model, on two independent architectural
blockers**, either alone sufficient:

1. No promotion-time mechanism exists (or is cheap to build) to verify "am I
   the sole live occupant of these exact pages right now" — required by both
   OS primitives, and unlike OPT-H's tail-adjacency check (a single cheap
   comparison against the current bump cursor), this would need to hold not
   just at carve time but persistently until promotion time, with no
   existing structure (`BinTable`/`PageMap`/free-list) tracking it (§2.4).
2. `segment_base_of_ptr`'s O(1) bitmask identity and `SegmentTable`'s
   base-pointer-keyed hash table both assume a segment's base address is
   stable for its entire lifetime — remapping breaks every OTHER live
   pointer sharing the segment, not just the promoted object (§3.1-3.3).

The one surviving direction — **MediumExtent**, a Large-like
one-object-per-segment kind applied at first-alloc time — is
CONDITIONAL-GO as a *separate future design*, explicitly not a continuation
of this mechanism, and explicitly trades away the density win that makes
`medium-classes` attractive in the first place (every candidate object would
pay a full OS reservation up front, the opposite of the whole point). This
is the "last untried asymptotic lever" the task prompt references, and it,
too, closes NO-GO for the mechanism actually asked about.

**2026-07-27 correction pointer (task #373, R23-4):** blocker 1 above
(the promotion-time neighbor-liveness check) has since been retracted as a
flawed premise — `carve_block`/`carve_batch`'s bump-monotonicity plus
`decommit_empty_segment_impl`'s empty-only reset gate together prove a live
medium block's byte range is exclusive for its whole lifetime, with no
runtime check needed. Blocker 2 (base-address stability) is confirmed
independent and unaffected. Revised: Linux sub-region remap is
CONDITIONAL-GO pending a correctness prototype (an unbuilt `mremap` FFI
surface plus a still-missing "never free-list-push a remap-vacated offset"
discipline); Windows and whole-segment remap remain NO-GO. This does not
change §0/§3's ship decision for `medium-classes` itself (the realloc axis
is still RED, unmeasured by any new evidence) — see §5's falsifiability
clause update for the precise scope of what did and did not change. Full
derivation: `R22_16_PROMOTION_REMAP_DESIGN.md` §10 (original §0-§9 preserved
verbatim there per the same convention).

### 1.7 Cross-check: is anyone depending on `medium-classes` today?

`medium-classes` is not in `production` (confirmed: `Cargo.toml:474`,
`medium-classes = ["alloc-core"]`, a standalone opt-in feature never listed
in any `production = [...]` composition). Grepped for consumers: no
`examples/`, `benches/`, or `README.md` usage claims this feature is used by
an external/downstream consumer — every reference found is this project's
own perf-gate infrastructure (34 files under `tests/`, 15 under `examples/`,
10 under `src/`, ~87 lines across `docs/perf/*.md`). Since this feature has
never shipped in a default build, there is no known external migration cost
either way.

---

## 2. The three options, weighed honestly

### (a) Ship in `production` — REJECTED, confirmed by the numbers, not merely asserted

The realloc kill-gate is real and has never come close to clearing:
~1,180×/~380× (R18-2), NULL on the one lever that might have helped (R20-2),
0% hit rate on the one mechanism designed to intercept it in place (R21-2),
and now a closed-form proof that the mechanism *cannot* be made to work on
this ladder no matter how it's tuned (R22-6), plus a design-level NO-GO on
even bypassing the copy at the OS level (R22-16). A consumer whose workload
reallocs medium-sized (256 KiB–1 MiB) objects even moderately often would
see a regression of two to three orders of magnitude on that path if this
shipped by default. This is not a marginal or noise-level concern — every
measurement's t-statistic is 38-154, sign tests are unanimous or
near-unanimous, and the SD/Δ resolvability checks (the R17-7 methodological
safeguard this project adopted) confirm every one of these effects is real,
not host jitter. **(a) is not defensible. Confirmed, not merely asserted.**

### (b) Formally document as a named opt-in workload profile

**What already exists that this option can lean on, so it is cheap:**
- The break-even curve is already derived (§1.1): ~205 reallocs-per-cycle
  is the tipping point between medium-classes winning and losing net, for
  the specific 16-object/8-slot-cache/256→768 KiB shape R10-2 measured. This
  is a real number from a real, reproducible harness — not invented for this
  decision.
- R10-2 §5's own workload-profile table already sketches three named
  archetypes ("buffer construction," "steady-state alloc/free churn,"
  "realloc-heavy steady state") with a directional verdict for each. This is
  the seed of the "named workload profile" doc's structure.
- A real consumer benchmark does **not** yet exist — no `docs/perf/*.md`
  or `benches/*` file runs a benchmark modeled on an actual downstream
  consumer's allocation pattern (only the project's own synthetic adversarial
  harnesses exist: the 16-object N-live-simultaneously harness and the
  single-hot-buffer harness, both purpose-built to interrogate the realloc
  axis specifically, not to represent a real application). Building one
  cheaply, reusing existing harnesses per this project's "speed: short
  scenario by default" convention, would mean: take
  `examples/_shared/paired_ab_medium_workload.rs` (already exists, already
  wired into `scripts/r10_2_medium_gate.mjs`) and parameterize its realloc
  frequency (currently fixed at 48 reallocs/round/object-population) as a
  sweep variable, then plot wall-clock ratio vs. realloc-rate to produce the
  break-even curve as an actual measured curve rather than the closed-form
  arithmetic estimate §1.1 already gives. This is a genuinely cheap
  follow-up (the harness and runner already exist; only a parameterization
  and a sweep script are new), but it is explicitly **not done by this
  task** (docs-only, decision-only) — it is named here as (b)'s natural next
  step, in §4's stub.
- **RSS/commit budget**: R20-2 §6.3 already gives concrete numbers for one
  configuration (`exact-span-large`: commit ~50.5 MiB → ~23.9 MiB, RSS
  ~3.17 MiB → ~9.58 MiB relative to plain `production`) — real numbers
  already measured, ready to be cited in a workload-profile doc's budget
  section.

**Cost of (b):** ongoing maintenance of a profile doc that must stay
accurate as the codebase evolves (a real but bounded cost — the profile
doc, unlike the recurring "should we ship this" question, does not need to
be re-litigated every round, only updated if the underlying mechanism
changes). Non-zero risk that a profile doc, once written, still gets treated
as "someday" and never gets an actual consumer benchmark attached — mitigated
by making the "new evidence to reopen" bar explicit (§5) so silence is not
mistaken for endorsement.

### (c) Formally reject and remove

**Cost:** genuinely loses the real, measured, thrice-reproduced alloc/free
win (~31×/~211×) with zero mitigation, for a feature that: (1) has no known
external consumer (§1.7 — not in `production`, so removal cost is
"infrastructure only," not "break someone's build"), and (2) whose removal
would touch a large surface: 34 `tests/` files, 15 `examples/` files
(including 3 shared workload harnesses reused by multiple gate reports), 10
`src/` files, ~87 lines across `docs/perf/*.md` design docs, plus the two
CI job steps named in §5 of this document's companion checklist. This is a
genuinely large diff for a feature whose problem is narrower than "the whole
thing is bad" — it is specifically the realloc axis, cleanly separable from
the alloc/free axis, that is bad.

**Benefit:** stops the recurring re-measurement cost the R22 plan synthesis
flagged (`docs/reviews/2026-07-26-r22-plan.md` §5 item 4 / §2.3 item 2 of
the underlying review) — 4 rounds (R18-2, R20-2, R21-2, R22-6/R22-16) spent
non-trivial measurement/design effort on the SAME question (does anything
make medium-classes' realloc axis acceptable) and got NO 4 times. That
recurring cost is real and this decision's whole point is to stop paying it
either way — but (b) stops it too, at a much smaller price (a documentation
commitment vs. a large deletion), by making the "closed until new evidence"
declaration explicit rather than by deleting the evidence of why it's
closed.

**Why (c) loses to (b) here:** the realloc axis being un-clearable is not
evidence the *whole feature* should go — it is evidence the feature has a
**scope**, and the alloc/free win within that scope is not merely
undisputed, it is the single most-reproduced result in this entire trail
(reappearing, at consistent magnitude, in every one of R10-2/R14-4/R18-2/
R20-2). Removing a real, reproduced win because a *different, separable*
axis of the same feature has a permanent limitation is a worse trade than
documenting the limitation and scoping the feature to where it wins. (c)
would be the right call if the alloc/free win were itself in doubt, or if
`medium-classes` had no way to be used safely (e.g. if the realloc axis were
silently reachable with no way for a consumer to know they were on thin
ice) — neither is true here: the win is solid and the danger is fully
diagnosable and documentable (a consumer can be told exactly what their
realloc rate needs to stay under).

---

## 3. Recommendation and reasoning (restated plainly)

**Recommend (b).** Reasoning, compressed:

1. (a) is closed by direct evidence — not a close call.
2. Between (b) and (c): the deciding fact is that `medium-classes` is not
   one mechanism with one verdict, it is two independently-measured axes
   (alloc/free: consistently WINS; realloc: consistently LOSES) that
   happen to share a feature flag. (c) treats them as inseparable and
   throws both away; (b) treats them as what the evidence actually shows —
   separable — and keeps the win while formally bounding the loss.
3. The recurring-cost problem (b)/(c) both solve is specifically "nobody
   should have to re-measure this without new evidence" — (b) solves that
   as fully as (c) does, via §5's explicit reopen-bar, at a fraction of the
   deletion cost, while preserving the win for the workload where it is
   real.
4. This is not "whichever sounds more constructive" — (c) was worked
   through seriously (§2, the removal checklist in §5 is fully enumerated,
   not hand-waved), and (b) wins on the merits: the asymmetry between "a
   large, disruptive removal that also deletes a genuine win" and "a
   bounded documentation commitment that keeps the win and closes the
   question just as durably" is the actual basis for the call, not a
   default preference.

---

## 4. Stub structure for the named workload profile doc (per (b)'s own next step — NOT written in full here)

A future round implementing (b) would create
`docs/perf/MEDIUM_CLASSES_WORKLOAD_PROFILE.md` with (section headers only,
no content — this is explicitly out of scope for this decision task):

```text
# medium-classes — workload profile: allocation-heavy, low-realloc consumers

## 0. Headline: who should turn this feature on
## 1. The win this profile is built on (cite R10-2, restate the ~31x/~211x
##    numbers, do not re-measure)
## 2. The loss this profile explicitly excludes (cite R18-2/R20-2/R21-2/
##    R22-6/R22-16, restate the ~1,180x/~380x numbers and the "structurally
##    closed" verdict)
## 3. The break-even curve
##    3.1 Closed-form estimate (R10-2 §5, ~205 reallocs/cycle) — already exists
##    3.2 Measured curve (NEW — parameterize paired_ab_medium_workload.rs's
##        realloc frequency, sweep it, plot ratio vs. rate) — not yet built
## 4. RSS / commit budget for this profile
##    (cite R20-2 §6.3's exact-span-large numbers; state which sibling
##    feature combination this profile recommends, if any)
## 5. Is there a real consumer benchmark?
##    (answer: not yet — name what it would take: a benchmark built from an
##    actual downstream access pattern, not a synthetic adversarial harness)
## 6. How to tell if your workload qualifies
##    (a short decision checklist: measure your own realloc-rate-in-the-
##    256KiB-1MiB-range and compare against §3's break-even)
## 7. What this profile does NOT claim
##    (does not claim medium-classes is safe for realloc-heavy workloads;
##    does not claim production-readiness; still opt-in, still not in
##    `production`)
```

---

## 5. This decision retires the recurring re-measurement cost — falsifiability clause

**Per this task's own framing:** this record closes the "should
`medium-classes` ship, in any form?" question for future rounds. A future
round should **not** reopen this question, spend a task re-running R18-2's
harness, or re-litigate the realloc axis, **without NEW evidence**, defined
narrowly as one of:

1. **A change to the segment/carving model** that alters the LCM arithmetic
   R22-6 derived (§1.5) — e.g. a redesigned medium-class ladder with
   friendlier size ratios, or a fundamentally different carve discipline
   (such as R22-16's MediumExtent, §1.6, IF it is built and separately
   measured) that removes the shared-segment neighbor-sharing constraint
   R22-16 found. A change to `large-cache-extended`'s slot count alone does
   NOT qualify — R18-2 already measured that lever (8→40 slots, ~3.5×
   improvement, still RED) and it is folded into this record.
2. **A real downstream consumer** whose actual measured realloc rate in the
   256 KiB–1 MiB range is at or below the break-even threshold (§1.1,
   ~205 reallocs/cycle for R10-2's exact harness shape — a different
   working-set size or cache configuration would need its own break-even
   recomputed, not this exact number reused unchecked) — i.e. new evidence
   that the "does anyone actually have a low-realloc, allocation-heavy
   workload" question (currently unanswered, §1.7/§2(b)) has a concrete
   yes.
3. **A new OS-level primitive or platform capability** not covered by
   R22-16's investigation (e.g. a future Windows/Linux kernel feature that
   removes the page-isolation precondition §1.6 found missing) — narrower
   than "someone has a new idea for remap," specifically a capability that
   falsifies one of R22-16 §2.4/§3.1-3.3's two architectural blockers.

   **2026-07-27 update (task #373, R23-4) — trigger 3 PARTIALLY satisfied,
   not fully, and by a different mechanism than this trigger anticipated.**
   This trigger was worded for a NEW OS/platform capability arriving from
   outside; what actually happened is narrower and already-landed: an
   independent re-verification of R22-16's OWN reasoning (not a new OS
   capability) found §2.4's "promotion-time neighbor-liveness check"
   blocker was based on a flawed premise — reading `carve_block`/
   `carve_batch`'s bump-monotonicity and `decommit_empty_segment_impl`'s
   empty-only reset gate directly (not trusting §2.4's original claim)
   shows a live medium block's byte range is provably exclusive for its
   whole lifetime, with no runtime check needed. §2.4 is retracted;
   §3.1-3.3's whole-segment base-address-stability blocker is confirmed
   independent and UNAFFECTED (this correction falsifies only ONE of the
   two named architectural blockers, not both). **Practical effect on THIS
   falsifiability clause: Linux sub-region remap moved from "blocked by two
   independent architectural arguments" to "CONDITIONAL-GO pending a
   correctness prototype"** — a real, material change in status, but
   explicitly NOT "the feature now works" — no prototype has been built,
   no `mremap` FFI exists yet, and a genuinely new bookkeeping discipline
   (never free-list-push a remap-vacated offset — the "permanent hole"
   question, only partially resolved by monotonicity, see
   `R22_16_PROMOTION_REMAP_DESIGN.md` §10.3) remains unbuilt. This does
   **not** reopen the "should `medium-classes` ship" decision itself (§0/§3
   above stand: the realloc axis is still RED today, with no code change
   and no measurement showing otherwise) — it reopens exactly one design
   sub-question (is sub-region remap worth prototyping) that this record's
   §1.6 had marked fully closed. Tracked as a still-open, not-yet-satisfied
   engineering item in `docs/perf/OPEN_ITEMS.md` item 6, not as evidence
   that clears this record's own bar. Full derivation:
   `R22_16_PROMOTION_REMAP_DESIGN.md` §10.

Absent one of these three (or their equivalent partial-satisfaction update
above), the correct action for a future round that encounters
`medium-classes`' realloc axis again is to **cite this record**
(and the 4 reports it summarizes), not to re-measure. This is the
falsifiable form of "closed until new evidence" — it names what would count,
so silence past this point is a deliberate closure, not an oversight the way
R14-4's un-cleared "re-run once R14-5 lands" item was (the original
motivating case for `OPEN_ITEMS.md`'s own existence).

---

## 6. Option (c)'s removal checklist — NOT executed, for a future round if this decision is ever overridden toward (c)

**Explicitly not performed by this task** (design/decision only, per the
task's own "identify, do not remove" instruction). Enumerated so a future
round has a concrete starting point if the recommendation in §0/§3 is
overridden:

### 6.1 CI rows (`.github/workflows/ci.yml`)
- `test-hardened` job, step `test (--features "hardened medium-classes")`
  (line 197; `cargo test --features "hardened medium-classes" --no-fail-fast`,
  line 199).
- `test-feature-isolation` job, step
  `test (--features "production medium-classes")` (line 389;
  `cargo test --features "production medium-classes" --test
  r14_4_promotion_move_leg_reduction --no-fail-fast`, line 391) and step
  `test (--features "production medium-classes exact-span-large")`
  (line 392; `cargo test --features "production medium-classes
  exact-span-large" --no-fail-fast`, line 395).
- The surrounding explanatory comment blocks in both jobs (lines ~188-196,
  ~364-388) that document why these specific feature combinations were
  chosen — would need removal or substantial rewrite, not just the `run:`
  lines.

### 6.2 `Cargo.toml`
- `medium-classes = ["alloc-core"]` (line 474) and its preceding doc comment
  block.
- `medium-classes-wide = ["medium-classes"]` (line 506) and its doc comment
  — depends on `medium-classes`, would need removal in the same pass.
  Any other feature whose `Cargo.toml` doc comment references
  `medium-classes` as a dependency or interaction (`large-reserved-capacity`,
  `exact-span-large`'s interaction notes, etc. — a full-file grep would be
  needed at execution time, not enumerated exhaustively here).
- `[[example]]` entries whose `required-features` include `medium-classes`
  (at minimum: `paired_ab_medium_off`/`_on`, `paired_ab_hot_buffer_off`/`_on`,
  `r11_3_promotion_probe`, `r13_7_large_cache_extended_hit_rate_measure`,
  `r13_8_medium_working_set_judge`, `r14_4_pad_target_probe`,
  `r21_2_opt_h_stage1_probe` — confirmed present via grep at the time of
  this reading; re-grep at execution time since this list can drift).

### 6.3 `src/` (10 files reference `medium-classes` per this reading's grep)
- Every `#[cfg(feature = "medium-classes")]`-gated block would need either
  deletion (if the code exists solely to serve this feature, e.g. the
  `EXTRAS` medium-class-ladder entries in `size_classes.rs`, the promotion
  call site in `heap_core_free.rs`) or careful un-gating review (if the
  surrounding function also serves non-medium-classes code paths). A
  precise file list needs a fresh `grep -rl 'medium-classes' src/` at
  execution time (this reading counted 10 files but did not enumerate each
  one, since this task's own scope is decision-only).

### 6.4 `tests/` (34 files reference `medium-classes` per this reading's grep)
- Includes at minimum the R14-4 promotion test suite
  (`tests/r14_4_promotion_*.rs`, 4 files), the R21-2 OPT-H precondition probe
  test, and every hand-written `#[cfg(feature = "medium-classes")]` or
  `medium_promotion_reachable!`-gated test — a fresh enumeration would be
  needed at execution time.

### 6.5 `examples/` (15 files reference `medium-classes` per this reading's grep)
- Includes the 3 shared workload harnesses
  (`examples/_shared/paired_ab_medium_workload.rs`,
  `paired_ab_hot_buffer_workload.rs`,
  `paired_ab_large_cache_extended_turnover_workload.rs`) that MULTIPLE gate
  reports (R10-2, R14-4, R18-2, R20-2, R21-2) depend on for reproducibility
  — removing these would break the reproduction commands cited in every one
  of those reports' own text, an explicit cost this checklist flags for a
  future executor to weigh (a removal could keep the harnesses as
  historical/archived artifacts even while dropping the feature, if
  reproducibility of the historical record is valued independently of the
  feature's own fate).

### 6.6 Design docs' "still active" status (`docs/perf/*.md`, ~87 lines across the corpus per this reading's grep)
- `docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` (OPT-H design, already
  marked closed/NO-GO by R22-6 — would need a terminal "retired alongside
  medium-classes" note added, not a rewrite).
- `docs/perf/R22_16_PROMOTION_REMAP_DESIGN.md` (remap design, MediumExtent's
  §4a CONDITIONAL-GO would need an explicit "moot if medium-classes itself
  is removed" note, since MediumExtent's whole premise is being a *variant*
  of medium-classes' segment model).
- `docs/perf/R10_4_RUN_ORIGIN_ORACLE_DESIGN.md` (already conditioned on
  `medium-classes-wide` being re-opened, per `OPEN_ITEMS.md` item 5 — would
  become permanently moot rather than conditionally deferred).
- `docs/perf/OPEN_ITEMS.md` itself — the `[L]`-tier item 11 (sub-16 KiB
  geometric-ladder OPT-H probe) and item 10 (R11-3 joint threshold×pad-target
  sweep) are both conditioned on `medium-classes`/`medium-classes-wide` — both
  would move from "deferred, conditional" to "permanently moot," a distinct
  disposition from either their current state or from closure-by-evidence.

**This checklist is intentionally not exhaustive to the byte** (per the
task's own instruction: identify, do not remove) — several sub-items above
explicitly note "a fresh grep at execution time" is needed, because this
decision task's own scope was reading and deciding, not cataloguing to
completion a change this document recommends AGAINST making.

---

## 7. `docs/perf/OPEN_ITEMS.md` pointer

A one-line pointer to this decision has been added to `OPEN_ITEMS.md`'s
"Recently resolved" trail (see that file's own diff) — closing, per this
task's own instruction, the "should medium-classes ship at all" question the
R22 plan synthesis (`docs/reviews/2026-07-26-r22-plan.md` §5 item 4) flagged
as never having been asked directly.

---

## 8. Files/lines this document is grounded in

- `docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md` — read in FULL (447
  lines). §3.1-3.3 (the three-phase results), §4 (kill-gate), §5 (break-even
  derivation, workload-profile table).
- `docs/perf/R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md` — read in FULL (726
  lines), including its embedded R18-2 re-run (§7.1, §10). Confirmed: there
  is no separate `R18_2_*.md` report file; `R18_2_MEDIUM_REALLOC_GATE_RERUN_
  summary.csv` and the three `_raw_r18_2_*.log` files are this report's own
  companions.
- `docs/perf/R20_2_C4_RESERVED_CAPACITY_HEADROOM_GATE.md` — read in FULL
  (450 lines). §3 (C4 gate), §4/§4.1 (the decisive C1-vs-C4 direct
  comparison), §6 (verdict and discussion).
- `docs/perf/R21_2_OPT_H_STAGE1_HIT_RATE.md` — read in FULL (430 lines).
  §3.1/§3.2 (the two 0% measurements), §4 (root-cause trace), §5 (does this
  meet the CONDITIONAL-GO trigger — no).
- `docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` — read the relevant
  sections in FULL (§2 OPT-H proposal + R22-6 addendum at line 204, §5
  honest scope assessment). Confirmed the closed-form LCM proof text and
  cross-checked its arithmetic independently.
- `docs/perf/R22_16_PROMOTION_REMAP_DESIGN.md` — read in FULL (813 lines).
  §0 (headline), §2 (neighbor-sharing blocker), §3 (segment-identity
  blocker), §4 (MediumExtent comparison), §6 (honest verdict).
- `docs/perf/OPEN_ITEMS.md` — read the "Recently resolved" trail's OPT-H/
  LCM closure entry and the R18-9/C4 closure entry in full, cross-checked
  the LCM table's numbers against this document's own re-derivation.
- `docs/reviews/2026-07-26-r22-plan.md` — §2.3 item 4 (the Russian-language
  synthesis flagging "should medium-classes ship at all" as never having
  been asked) and the P3-tier task-queue entry for R22-18 itself.
- `Cargo.toml` — `medium-classes`/`medium-classes-wide` feature definitions
  (lines 467-506), confirmed neither is part of any `production` composition.
- `.github/workflows/ci.yml` — grepped for every `medium-classes` CI row;
  confirmed exact line numbers for §6.1's checklist (`test-hardened` line
  197/199; `test-feature-isolation` lines 389/391/392/395).
- Grep counts (not exhaustively enumerated, per §6's own caveat): 34
  `tests/*.rs` files, 15 `examples/**/*.rs` files, 10 `src/**/*.rs` files,
  and ~87 lines across `docs/perf/*.md` reference `medium-classes` as of
  this reading (`main` @ `ff48029`).
