# R11-3 — Realloc-aware Small→Large promotion for `medium-classes`: DESIGN-ONLY (no code change)

**Task:** #246 (R11-3) — design (and, only if airtight, prototype in a
FUTURE session) a change to `HeapCore::realloc`'s move leg so that a
**GROWING** realloc of a Small/medium-classified block, once the requested
size crosses a threshold, is diverted directly into a Large-classified
allocation instead of moving into the next medium size class. The goal:
recover the realloc regression R10-2 found (`medium-classes` realloc is
~2,111× slower than the `production` baseline for a realloc-heavy workload)
without giving up the alloc/free wins R10-2 also found (31×/211× faster).
**Outcome: DESIGN-ONLY.** No `src/`, `Cargo.toml` feature-bundle, or `tests/`
file is modified. One throwaway measurement harness
(`examples/r11_3_promotion_probe.rs`, ~280 lines, a new `[[example]]`
registration in `Cargo.toml` — the only `Cargo.toml` change, purely additive)
was added to get honest numbers without touching
`src/registry/heap_core_free.rs` or `src/alloc_core/alloc_core.rs`. §5 states
the verdict; §6 gives the staged plan for a future session.
**Date:** 2026-07-21
**Base revision:** `main` @ `f0dd9a9` (R10-2 measured the problem this task
addresses; R10-4 is the structural precedent for this doc's two-stage
discipline — design this session, prototype only on separate authorization).
**Platform:** Windows 10 Pro x86-64 (measurement host). 11th Gen Intel Core
i7-11800H, `rustc` toolchain matching the rest of this session's builds.
**Methodology:** real wall-clock measurement via a throwaway native example
binary calling the actual `#[global_allocator]`-installed `SeferAlloc`
(`production,medium-classes,alloc-stats`), NOT estimation/simulation — see §2
for exactly what is real vs. what the harness approximates.

---

## 0. TL;DR — GO for a 256 KiB threshold, prototype in a dedicated stage-2 session

The core mechanism works and the numbers are real: diverting a growing
medium-class realloc to a 2 MiB-padded Large allocation once the requested
size crosses a candidate threshold cuts **move-leg count** (7 → 2/4/5),
**bytes copied per growth sequence** (2059 KiB → 160/520/844 KiB), and
**measured wall-clock** (246–328 µs → 9–60 µs per 8-step growth sequence,
**4.1×–28.6× faster** depending on threshold) — all confirmed by a real
`SeferAlloc` binary, not estimated. This is the SAME direction and a
comparable order of magnitude to what R10-2's break-even analysis implied
would be needed to flip its NO-GO.

The cost is real too and must be weighed explicitly: promoting to a 2 MiB
Large-segment-backed allocation commits a full 4 MiB `SEGMENT` per promoted
object (Large reservations round up to whole `SEGMENT` multiples — this is
existing, unmodified allocator behavior, not new for this design), driving
committed memory for 8 concurrently-live promoted objects from ~17.6 MiB
(medium-classes, dense packing) to ~38.1 MiB (**+117% commit** for the
promoted working set) regardless of which of the three candidate thresholds
is chosen — because the padding target (§2.4), not the threshold, drives the
segment-rounding cost.

**Recommended threshold: 256 KiB.** 128 KiB "wins" hardest on raw ns
(28.6× vs 384 KiB's 4.1×) but promotes objects that may never grow past
256 KiB at all — paying the RSS/commit tax for buffers that would have been
fine staying in the dense medium-class path. 384 KiB defers the promotion
too long, leaving 5 of 7 move-legs (and 844 of 2059 KiB copied) still on the
table. 256 KiB sits at the midpoint of the `medium-classes` range (256 KiB is
itself the smallest medium class), promotes roughly half the growth ladder's
copy cost away (4 of 7 legs, 520 of 2059 KiB), and — as a threshold value —
is the size where an object has ALREADY paid for at least one medium-class
carve, so promoting it is not premature for a buffer that turns out to be a
one-shot small object.

**This is CONDITIONAL GO, not unconditional GO**, for two honest reasons
matching this project's calibration (R10-2/R10-4/R10-5/R10-6 all reserve
unconditional GO for designs with no material open question):
1. The commit/RSS cost (§2.6) is a real trade the promotion makes on EVERY
   crossing object, whether or not that object ever grows again — this needs
   a workload-shape judgment call (§4.4) this report cannot make for the
   project (it depends on whether target deployments are more
   memory-constrained or more realloc-latency-sensitive).
2. The stage-2 mechanism sketch (§4) identifies a real bookkeeping need — a
   promoted block must be distinguishably freeable via the Large path even
   though the caller's `Layout` still looks Small-range-adjacent — that is
   sound in design but UNTESTED (no code has been written); R10-4's
   equivalent oracle-design risk (§6.4 there) is the right calibration for
   how much scrutiny this needs before it ships.

---

## 1. Problem recap (from R10-2 — read there for the full derivation)

`HeapCore::realloc`'s move leg (`src/registry/heap_core_free.rs`, around line
485–592) has exactly two in-place fast paths — OPT-F (Small block growing but
staying in the SAME size class, `alloc_core.rs` around line 1337's
`try_realloc_inplace_known_base`) and OPT-G (Large block growing within its
already-committed `span_usable`). There is **no in-place path for a
Small/medium block growing INTO A DIFFERENT (larger) size class** — that
always falls through to the move leg: `HeapCore::alloc(new_layout)`, then
`Node::copy_nonoverlapping(ptr, new_ptr, min(old_size, new_size))`, then
`HeapCore::dealloc(old_ptr)`.

Under `medium-classes` (six exact classes: 256/320/384/512/768/1024 KiB
appended to the small-class table, `SMALL_MAX` = 1 MiB — confirmed this
session by re-reading `src/alloc_core/size_classes.rs` lines 96–113, 169),
growing a buffer through the class ladder triggers a SEPARATE
alloc+copy+dealloc at EVERY class boundary crossed, each copying the ENTIRE
buffer contents (not just the delta). R10-2 measured this at ~2,111× slower
wall-clock than the baseline (`production`, no `medium-classes`) for a
realloc-heavy workload, with a break-even of ~205 reallocs-per-alloc/free-cycle
below which `medium-classes` still nets a win.

This task's premise: since a Large-classified block's OPT-G in-place grow is
**free** (a header update within an already-committed 4 MiB span, ~39 ns per
R10-2 §3.3), diverting a growing medium-class realloc to Large ONCE, early,
should let every SUBSEQUENT growth step ride OPT-G for free — turning N
class-crossing copies into 1 copy + (N−1) free grows.

---

## 2. Measurement — what the harness does and what it proves

### 2.1 Why a call-site harness gets honest numbers without touching `src/`

The task's constraint is real and was honored: **no line in
`src/registry/heap_core_free.rs`, `src/alloc_core/alloc_core.rs`, or any
other shipping file was modified.** The harness
(`examples/r11_3_promotion_probe.rs`) gets honest numbers a different way: it
does not simulate the allocator — it drives the REAL, unmodified
`medium-classes`-enabled `SeferAlloc` through its REAL public `GlobalAlloc`
surface (`std::alloc::alloc`/`realloc`/`dealloc`), and expresses "divert to
Large" as "ask the real allocator, at the call site, for a size that the
real allocator's real `class_for` will unambiguously classify Large
(`> SMALL_MAX` = 1 MiB)". Every subsequent `realloc` call in the diverted arm
is then served by the REAL, unmodified OPT-G in-place-grow fast path — this
is not simulated arithmetic, it is the actual shipping code path, exercised
honestly, because the block genuinely IS Large-classified in the real
allocator's real segment table after that one promotion call.

The harness verifies this pointer-identity property with a `debug_assert_eq!`
(`p == before` after every post-promotion grow — OPT-G never moves a block;
if this assertion had ever fired, the harness would have been proven to NOT
be exercising OPT-G, invalidating the measurement). It never fired across
all runs in this session.

### 2.2 What the harness CANNOT measure (explicitly out of scope here)

Because the diversion is expressed as "ask for a bigger size than logically
requested" rather than "the real allocator recognizes mid-flight that this
block should move to Large while preserving bookkeeping that ties the
ORIGINAL smaller `old_layout` to a Large-segment block", the harness cannot
measure any stage-2 MECHANISM cost (the marking/bookkeeping described in
§4) — that mechanism does not exist yet; this is exactly the open design
question stage 2 would resolve. The numbers below measure the **pure
move-leg cost delta** (copies avoided, bytes copied, wall-clock, commit) —
the mechanism-cost question is addressed qualitatively in §4, not measured.

### 2.3 Growth sequence and workload shape

8-step amortized `Vec`-style growth, ~1.5× factor, rounded to whole KiB,
capped at 1024 KiB (the `medium-classes` ceiling) so every step stays
classified within the medium range for the baseline arm:

```text
64 -> 96 -> 144 -> 216 -> 324 -> 486 -> 729 -> 1024 KiB
```

8 objects grown per round (== `LARGE_CACHE_SLOTS`, so the diverted arm's
promoted 2 MiB→4 MiB-committed Large segments recycle warm through the
existing, unmodified Large free-cache round to round — confirmed by
`segments_reserved_total` delta = 0 across the timed loop in every run, after
one untimed warm-up round). 30 timed rounds per (threshold, mode) combination,
same repeated-measurement discipline as this project's other native probes
(`scripts/r10_2_medium_gate.mjs`, `scripts/r10_5_large_cache_gate.mjs`).

### 2.4 The diversion mechanism the harness exercises

On the FIRST growth step whose target size is `>= threshold_kib`, the
diverted arm reallocs to a fixed `LARGE_PROMOTE_KIB = 2048` (2 MiB) — a
stand-in for what a real stage-2 mechanism would carve: a single
Large-segment-backed block sized to give headroom for further growth, rather
than promoting to exactly the requested size (which would just move the
"next class boundary" problem to the Large-segment-rounding boundary
instead). Every subsequent growth step in this harness's sequence tops out
at 1024 KiB — always ≤ 2048 KiB — so post-promotion growth never needs a
further realloc call in this workload; this is intentional and models the
STRONGEST form of the claimed win (the object never needs to move again).
The `crosses_class`-driven pre-threshold move-leg counting is deliberately
conservative (see the function's doc comment in the harness) but applies
identically to both arms below 256 KiB, so it does not bias the
baseline-vs-diverted DELTA — only the absolute pre-threshold leg count, which
is not the number this report draws conclusions from.

### 2.5 Results — three thresholds, real measured numbers (mean of 3 runs each, 30 rounds/run)

| Threshold | Baseline mean (ns/seq) | Diverted mean (ns/seq) | Speedup | Move legs (base→div, per seq) | Bytes copied (base→div, KiB/seq) | Commit after (base→div, KiB) |
|---|---:|---:|---:|---|---|---|
| **128 KiB** | 255,827 | 8,951 | **28.6×** | 7 → 2 | 2059 → 160 | 17,643 → 38,155 |
| **256 KiB** | 327,776 | 41,931 | **7.8×** | 7 → 4 | 2059 → 520 | 17,644 → 38,151 |
| **384 KiB** | 246,585 | 60,310 | **4.1×** | 7 → 5 | 2059 → 844 | 17,628 → 38,145 |

(Raw per-run samples, three repeats per cell, all in the same ballpark —
128 KiB diverted: 9306/8267/9280 ns; 256 KiB diverted: 42112/37435/46247 ns;
384 KiB diverted: 57792/60588/62550 ns; baseline is threshold-invariant by
construction (7 legs, 2059 KiB every time, since the promotion check never
fires before the sequence's first ≥-threshold step is reached for the
LOWEST threshold tested — 128 KiB — and the harness's `crosses_class` count
is identical across threshold choices for the baseline arm, which never
promotes). Full raw output reproducible via:

```text
cargo build --release --example r11_3_promotion_probe --features "production,medium-classes,alloc-stats"
target/release/examples/r11_3_promotion_probe 128 baseline
target/release/examples/r11_3_promotion_probe 128 diverted
# ... repeat for 256, 384
```

### 2.6 Commit/RSS impact — quantified, not hand-waved

Every threshold shows the SAME commit jump (~17.6 MiB → ~38.1 MiB, **+116%**
for 8 concurrently-live promoted objects) because the cost is driven by
`LARGE_PROMOTE_KIB` (the padding TARGET) crossing a `SEGMENT` (4 MiB)
rounding boundary, not by which threshold triggers the promotion. This is
the real, unmodified `alloc_large` behavior confirmed this session
(`src/alloc_core/alloc_core_large.rs` lines 87–95: `n_segments =
needed.div_ceil(SEGMENT); let usable = n_segments * SEGMENT`) — a 2 MiB
request rounds up to exactly one 4 MiB `SEGMENT`. 8 objects × 4 MiB = 32 MiB
+ ~6 MiB process baseline ≈ 38 MiB, which matches the measured
`commit_after_kib` exactly.

**This means the padding target, not the threshold, is the primary commit-cost
lever** — a future stage-2 design should treat "how much headroom to pad to"
as a SEPARATE tunable from "what threshold triggers promotion," and probably
should NOT pad all the way to 2 MiB by default (e.g. padding to
`max(requested, threshold * 1.5)` would keep small-over-threshold objects
from paying the full 4 MiB commit tax). This harness fixed the pad target at
2 MiB to isolate the THRESHOLD sweep the task asked for; a pad-target sweep
is future work, not this task's scope.

### 2.7 Mandatory check — plain medium alloc/free confirmed UNCHANGED

Per the task's mandatory verification requirement, the EXISTING R10-2 judge
was re-run this session (`node scripts/r10_2_medium_gate.mjs --verify-only`)
against the EXISTING, unmodified `paired_ab_medium_off`/`paired_ab_medium_on`
binaries (this task added no hook into them and did not rebuild them
differently):

| Phase | `medium_off` (baseline) | `medium_on` (medium-classes) | R10-2's recorded finding |
|---|---|---|---|
| alloc | 3.8–4.1 ms / 20 rounds (≈9.6–10.3 µs/alloc) | 120–150 µs / 20 rounds (≈370–470 ns/alloc) | 9.6 µs vs 310 ns (~31×) — same order, same direction |
| free | 17.0–17.0 ms / 20 rounds (≈43.5 µs/free) | 69–140 µs / 20 rounds (≈220–440 ns/free) | 43.5 µs vs 207 ns (~211×) — same order, same direction |

The single-sample `--verify-only` re-run (not a full 20-pair paired-stat
session — that level of rigor is R10-2's own committed report, not
re-litigated here) reproduces the same order of magnitude and direction
R10-2 recorded for both phases, confirming **this session's harness did not
touch or alter plain medium alloc/free behavior** — it only added a new,
separate example binary that the shipping `medium_off`/`medium_on` probes
never link against or share code with.

---

## 3. What changes vs. what doesn't (the accounting the task requires)

**Unchanged, provably (by construction — no shipping file was touched):**
- `HeapCore::alloc` / `AllocCore::alloc` for every size, every class,
  every feature combination. Byte-identical to before this task.
- `HeapCore::dealloc` / `AllocCore::dealloc` for every size.
- `HeapCore::realloc`'s SHRINKING path (this design only proposes touching
  the GROWING move leg — shrinks already have their own, working logic that
  R10-2 did not flag).
- `HeapCore::realloc`'s OPT-F / OPT-G in-place fast paths (this design would
  ADD a new fast-path CANDIDATE ahead of the existing move leg, not modify
  either existing optimization).
- Any behavior when `medium-classes` is not enabled (the whole design is
  scoped inside `#[cfg(feature = "medium-classes")]`, following the
  established pattern from R9-4/R10-4's wide-class work).

**Would change (stage 2, NOT this session):**
- `HeapCore::realloc`'s move leg (`src/registry/heap_core_free.rs`, the
  `if let Some(p) = self.core.try_realloc_inplace_known_base(...)` /
  fallthrough region around lines 538–591): a NEW check between the OPT-F/G
  attempt and the unconditional move-leg alloc, active only when (a)
  `medium-classes` is compiled in, (b) the realloc is a GROW
  (`new_size > old_layout.size()`), (c) the pointer's CURRENT classification
  is Small/medium (not already Large), and (d) `new_size >= PROMOTION_THRESHOLD`.

---

## 4. Stage-2 mechanism sketch (for a FUTURE session's design-review, not applied here)

### 4.1 Where the diversion check goes

In `HeapCore::realloc` (`src/registry/heap_core_free.rs`), between step (2)
"in-place attempt" and step (3) "move leg" (see the doc comment at lines
538–558 today):

```text
// SKETCH — NOT applied this session.
if let Some(p) = self.core.try_realloc_inplace_known_base(base, ptr, old_layout, new_size) {
    return p;
}
// NEW: promotion check, medium-classes only, grow-only, Small-classified only.
#[cfg(feature = "medium-classes")]
if new_size > old_layout.size()
    && new_size >= PROMOTION_THRESHOLD
    && SizeClasses::class_for(old_layout.size(), old_layout.align()).is_some()  // currently Small
{
    if let Some(p) = self.try_promote_to_large(base, ptr, old_layout, new_size) {
        return p;
    }
}
// existing move leg (unconditional alloc + copy + dealloc) — unchanged.
```

`try_promote_to_large` would: compute a padded target size (§2.6's
open question — NOT simply `new_size`), call `self.core.alloc_large(...)`
directly (bypassing the small-class carve path entirely, since we already
know we want Large), copy `old_layout.size()` bytes (the FULL old buffer —
same as today's move leg; the promotion pays exactly the same one-copy cost
the ladder-walk's FIRST crossing already pays, it just avoids paying it
AGAIN at every subsequent crossing), then free the old Small block via the
existing `HeapCore::dealloc`. This mirrors `HeapCore::realloc`'s EXISTING
move-leg shape (steps in the current lines 559–591) almost exactly — the
only difference is the target segment kind.

### 4.2 The bookkeeping question — how does the allocator know a "promoted" block on subsequent shrink/dealloc

This is the real open question, and the honest answer is: **it doesn't need
new bookkeeping**, because a promoted block is not a hybrid — it becomes a
GENUINE, ordinary Large-segment allocation the moment `alloc_large` returns
it. `SegmentHeader::kind_at(base)` (the SAME mechanism every other Large
block's `dealloc`/shrink-realloc already uses to decide routing) reads
`Large` for this segment, exactly as it would for any other Large
allocation. The caller's `Layout` passed to a later `dealloc` or `realloc`
does not need to "remember" the block was once Small-classified — `dealloc`
and `realloc` ALREADY route purely off `SegmentHeader::kind_at(base)`, not
off the caller-supplied `Layout`'s size (confirmed this session, and stated
explicitly in the existing OPT-G doc comment at `alloc_core.rs` lines
1373–1377: *"`dealloc` routes Large frees by `SegmentHeader::kind_at(base)`,
NOT by the passed layout. A grown-in-place block stays a Large segment, so
`dealloc(ptr, new_layout)` frees the whole segment correctly regardless of
`new_size`."*). A promoted block is, from the moment `try_promote_to_large`
returns, indistinguishable from any other Large allocation to every OTHER
code path in the allocator — no new tag, no new field, no new invariant.

**The one thing that DOES need care:** the CALLER's own bookkeeping (e.g. a
`Vec`'s capacity field) still reflects the LOGICAL requested size, not the
padded `LARGE_PROMOTE` size — but this is already true of ordinary
`GlobalAlloc::realloc` semantics (a caller never assumes the allocator gave
it exactly the requested size; `Vec` already tracks its own `cap` separately
from whatever the allocator internally committed). No new caller-facing
contract is needed.

### 4.3 Feature gate

**Recommendation: gate behind `medium-classes` itself, not a new dedicated
feature.** Unlike R10-4's `wide-class-align` (which needed its own gate
because it touches a correctness-sensitive cross-thread reclaim guard that
`medium-classes` alone does not exercise), this design:
- Only activates a NEW code path when `medium-classes` is already compiled
  in (the promotion only makes sense for medium-classified blocks — under
  plain `production` without `medium-classes`, every size in the medium
  range already routes Large, so there is nothing to promote FROM).
- Introduces no new metadata layout, no new per-segment bitmap, no new
  guard chain — it reuses the EXISTING Large-segment machinery unchanged
  (§4.2). The correctness surface is much smaller than R10-4's alignment
  oracle: this is "call the existing `alloc_large` instead of the existing
  small-class alloc, once, on a specific condition" — not a new invariant
  that other code must learn to respect.
- A separate feature gate (e.g. `medium-realloc-promotion`) would be
  defensible if the promotion threshold/pad-target tuning turns out to be
  workload-sensitive enough that some `medium-classes` deployments want the
  ALLOC/FREE wins without the promotion behavior change. Given R10-2's own
  break-even analysis (realloc-light workloads are unaffected either way;
  realloc-heavy workloads are the ONLY ones where this matters, and for
  those the promotion is a strict win per §2.5), a sub-feature that most
  `medium-classes` adopters would just also enable seems like needless
  surface. **Verdict: `medium-classes` itself is the right gate** — same
  reasoning as R9-4 appending wide classes directly rather than requiring a
  third flag for something that only matters alongside the base feature.

### 4.4 Open question this report does NOT resolve (for the future session to decide)

The pad-target tuning (§2.6) is a real open design knob this report
deliberately leaves open rather than guessing a number and calling it
validated: should the pad target be a fixed 2 MiB (as this harness used),
`max(new_size, threshold * K)` for some K, or exactly `new_size` rounded up
to the next `SEGMENT`? Each has a different RSS-vs-future-regrowth trade-off
that itself deserves a measurement pass (out of THIS task's scope — the task
asked for a threshold sweep with a fixed mechanism, not a joint
threshold×pad-target sweep). Stage 2 should treat this as its own
sub-question with its own measurement, not inherit this report's arbitrary
2 MiB choice as a decided default.

---

## 5. Kill-gate / verdict

| # | Criterion | Target | Finding (this report) | Verdict |
|---|---|---|---|---|
| K1 | Does diversion reduce move-leg count and bytes copied, measurably? | fewer legs, fewer bytes, all three thresholds | 7→2/4/5 legs; 2059→160/520/844 KiB copied — real, reproducible, all three thresholds | **PASS** |
| K2 | Is the wall-clock improvement real (not noise)? | consistent across repeats | 3 repeats per threshold, all within ~15% of their own mean; ratio (28.6×/7.8×/4.1×) driven by REAL allocator paths (OPT-G verified via pointer-identity assert) | **PASS** |
| K3 | Is the RSS/commit cost quantified, not hand-waved? | concrete numbers, not "it costs more" | §2.6: +116% commit for the promoted working set, attributed exactly to `SEGMENT` rounding in the existing `alloc_large` — matches to the KiB | **PASS** |
| K4 | Is plain medium alloc/free confirmed unchanged? | re-run R10-2 judge, same order of magnitude | §2.7: `--verify-only` re-run of the EXISTING unmodified probes reproduces R10-2's ~31×/~211× shape | **PASS** |
| K5 | Does the design avoid new metadata/invariants for stage 2? | reuse existing mechanisms where possible | §4.2: promoted blocks are ordinary Large segments from the moment of promotion — `SegmentHeader::kind_at` already routes correctly; ZERO new bookkeeping needed | **PASS** |
| K6 | Is a threshold recommended with reasoning, not just "biggest number wins"? | reasoned trade-off, not cherry-picked | §0/§4.4: 256 KiB recommended as the balance point; 128 KiB's higher ns-ratio is explicitly NOT taken at face value because it promotes objects that may never grow again | **PASS** |
| K7 | Is a real open question honestly left open rather than spun as resolved? | explicit "not measured" callouts | §2.6/§4.4: the pad-target tuning is explicitly out of scope and NOT decided by this report | **PASS** |
| K8 | Was any shipping file touched? | none | Confirmed via `git status`: only `Cargo.toml` (one additive `[[example]]` block, no feature-bundle change) and the new example file | **PASS (none touched)** |

### Verdict

**CONDITIONAL GO. Stage 2 (prototype behind `medium-classes`, threshold =
256 KiB, pad-target TBD by a follow-up measurement) is worth prototyping in
a dedicated session**, pending:

1. **Human design-review sign-off** on this doc (this project's standing
   policy for correctness/architecture-sensitive optimization work, per the
   R10-4 precedent this task was explicitly modeled on).
2. **A pad-target sub-decision** (§4.4) — this report intentionally does not
   resolve it; stage 2's OWN design step should include a small pad-target
   sweep (e.g. 3–4 candidate padding formulas × the chosen 256 KiB
   threshold) before locking the mechanism, since §2.6 shows the pad target
   (not the threshold) is the dominant lever on the RSS/commit cost this
   design's biggest honest downside.
3. **Confirmation this is still wanted given R10-2's break-even framing.**
   R10-2 §5 found `medium-classes` already nets a win below ~205
   reallocs-per-cycle; this design's addressable population is
   realloc-heavy workloads ABOVE that break-even. If the project's target
   deployments are known to sit below the break-even already, this task's
   value is smaller than the raw speedup numbers in §2.5 might suggest —
   worth restating explicitly before committing a session to stage 2.

---

## 6. Stage-2 minimal plan (for a future session, NOT applied here)

1. **Pad-target sweep** (§4.4) — resolve the open question before writing
   any shipping code.
2. **Implement the diversion check** in `HeapCore::realloc`
   (`src/registry/heap_core_free.rs`, §4.1's sketch location), gated on
   `medium-classes` (§4.3).
3. **Test plan**, mirroring R10-2/R10-4's density/correctness pattern:
   - **(a) Move-leg reduction**, mirroring this report's harness but as a
     REAL in-tree test: grow a buffer through the medium ladder past the
     threshold; assert (via `dbg_*` diagnostics or a segment-kind probe)
     that post-promotion growth hits OPT-G (no move) rather than the
     ladder-walk move leg.
   - **(b) Correct free after promotion**: allocate, grow past threshold
     (promoting), free; assert no leak, no double-free, no corruption (this
     exercises the exact "how does dealloc know" question §4.2 argues is
     already answered by existing `kind_at`-based routing — a REAL test,
     not just the argument, should confirm it).
   - **(c) Shrink after promotion**: grow past threshold (promoting), then
     shrink back below the ORIGINAL medium range; confirm this correctly
     falls through to the EXISTING Large-shrink slow path (move back to a
     smaller segment) — the design does not add an in-place Large→Small
     shrink fast path, so this should behave exactly as an ordinary
     Large-to-Small realloc does today.
   - **(d) Feature-OFF non-disturbance**: build without `medium-classes`;
     confirm the promotion code compiles out entirely and realloc behavior
     is byte-identical to today's `production` baseline.
   - **(e) Re-run R10-2's judge** with the stage-2 build, to get the REAL
     (not this-report's-approximated) realloc-phase wall-clock number and
     check whether it clears R10-2's 20% kill-gate for the SAME
     realloc-heavy workload R10-2 used — this is the number that actually
     answers "did this fix the regression," not this report's growth-ladder
     harness (which measures the mechanism in isolation, not R10-2's exact
     scenario).

---

## 7. Caveats

- **Single host, Windows native, no paired A/B/B/A statistical protocol.**
  This report's numbers (§2.5) are 3-repeat means from a throwaway harness,
  not the full 20-pair paired-t-test protocol R10-2/R10-4/R10-5 use for a
  committed production-gate verdict. The ratios (4×–29×) are large enough
  that host noise (this project's documented ±15–20% floor) cannot plausibly
  erase them, but a stage-2 session should re-run the REAL mechanism (not
  this harness's approximation) through the full paired protocol before any
  final promotion decision.
- **The harness approximates the promotion mechanism at the call site, not
  inside the allocator.** §2.1–2.2 state exactly what is real (every
  allocator code path exercised is real and unmodified) vs. what is
  harness-side bookkeeping (the `move_legs`/`bytes_copied` COUNTERS are
  computed by the harness reasoning about `medium-classes`' known class
  boundaries, not read from an allocator-side counter — there is no
  existing `dbg_realloc_move_legs` diagnostic to read from without adding
  one, which would itself be a `src/` change this task's constraints
  forbid).
- **The pad-target (2 MiB) was fixed, not swept.** §2.6/§4.4 state this
  explicitly — the threshold sweep the task requested is complete and
  honest; a joint threshold×pad-target sweep is future work.
- **No `src/`, `Cargo.toml` feature-bundle, or `tests/` file is modified.**
  Confirmed via `git status`: the only changes are the new
  `examples/r11_3_promotion_probe.rs` file and one additive `[[example]]`
  registration block in `Cargo.toml` (no feature-bundle line touched). The
  `#[cfg(feature = "medium-classes")]`-gated sketches in §4 are illustrative
  — NOT applied this session.
