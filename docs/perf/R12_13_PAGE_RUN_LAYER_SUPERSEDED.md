# R12-13 — Page-run layer (R11-7 design): re-evaluation verdict — SUPERSEDED, NO-GO

**Task:** #264 (R12-13, P2) — before implementing anything from
`docs/perf/R11_7_PAGE_RUN_LAYER_DESIGN.md` (CONDITIONAL GO, stage-2-ready
since R11-7), re-evaluate whether R12-3 (`exact-span-large`, commit
`2593d30`) and R12-4 (`large-reserved-capacity`, commit `fc155c9`) already
closed the niche the page-run layer was designed for. Implementation is
authorized only if the re-evaluation shows a real gap remains.
**Outcome: SUPERSEDED — NO-GO on implementing the page-run layer.** No
`src/`, `Cargo.toml`, or `tests/` file is touched by this task. This
document is the deliverable; `R11_7_PAGE_RUN_LAYER_DESIGN.md` is annotated
with a pointer to this verdict (design content otherwise left intact, for a
future revisit if the facts below change).
**Date:** 2026-07-22.
**Base revision:** `main` @ `f0dd9a9` plus R12-1..R12-12 (`89b6ce2..a7db75a`).

---

## 1. What R11-7 actually targeted (re-read in full, not skimmed)

`R11_7_PAGE_RUN_LAYER_DESIGN.md` is explicit that its target is **density
for many simultaneously-live 1.25–2.0 MiB objects sharing one arena**, not
merely "one big object wastes committed bytes." Its own framing (§2.6):

> "a workload would need to be allocating enormous volumes of 1.25-2 MiB
> objects to have many arenas live simultaneously — each 8 MiB arena holds
> ~3-5 blocks, so 100 live 1.5 MiB objects need ~25 arenas, NOT 100"

and its §0 summary states the win as **multi-block packing per arena**
(density 5/4/3/3 for an 8 MiB page-run arena vs today's 2/1/1/1 for a plain
4 MiB segment) — i.e. the problem it solves is *N objects sharing 1 arena*,
not *1 object wasting space in its own arena*. The design's own §2.6 also
flags the mechanism this task is asked to check: a page-run arena, under
its recommended parallel-`PageRunTable` design, **does not consume a
`SegmentTable` slot at all**, specifically so that "many-thousands-of-live"
workloads do not exhaust `MAX_SEGMENTS = 1024`.

Two distinct sub-problems are therefore bundled under "the density gap":

- **(a) RSS/committed-bytes waste per object** — a single 1.25–1.75 MiB
  object sitting in a 4 MiB segment wastes megabytes of *committed* memory
  it never touches.
- **(b) `SegmentTable`-slot / OS-reservation-syscall pressure at high
  live-object counts** — each object (whether via Small-class carve or a
  dedicated Large segment) that needs its own `SegmentTable` registration
  and/or its own `vmem::reserve_aligned` call pays a fixed per-object tax
  independent of committed bytes, and is bounded by `MAX_SEGMENTS = 1024`
  for anything going through the Large path.

R11-7's density arithmetic is about (a)-shaped waste turning into (b)-shaped
capacity via the "5/4/3/3 blocks share one arena, so 25 arenas instead of
100" argument — but the workload it needs to matter (thousands of live
medium objects) is asserted, not measured, anywhere in this codebase (see
§3).

---

## 2. What R12-3/R12-4 measured, and what they explicitly leave alone

### 2.1 R12-3 (`exact-span-large`, `2593d30`) — closes (a), not (b)

R12-3's own commit message states the mechanism precisely: it changes
**only the physical `usable` (committed-byte) computation** for a Large
segment, from `round_up(header + size, SEGMENT)` to
`round_up(header + size, PAGE)`. Measured RSS amplification, reproduced
personally by the committing agent (not just claimed by the delegate):

| Request size | Before (whole-SEGMENT rounding) | After (`exact-span-large`) |
|---|---:|---:|
| 260 KiB | 15.78x | 1.05x |
| 512 KiB | 8.00x | 1.01x |
| 1 MiB | 4.00x | 1.00x |
| 1.75 MiB | 2.29x | 1.00x |
| 4 MiB | 2.00x | 1.00x |

This is problem **(a)** — per-object committed-byte waste — closed almost
completely (residual amplification ~1.00–1.05x, i.e. essentially exact).

R12-3's commit message is explicit about what it does **not** touch:
*"Scope is deliberately minimal... only the physical `usable` computation
changes."* The segment's **alignment stays `SEGMENT` (4 MiB) unconditionally**
(so `segment_base_of_ptr`'s masking is unaffected), and — critically for
this task's question — the registration call
(`self.table.register(slot.base)`, `src/alloc_core/alloc_core_large.rs:294`)
is completely untouched. **Every Large allocation, exact-span or not, still
consumes exactly one `SegmentTable` slot and pays exactly one
`vmem::reserve_aligned`/`reserve_aligned_lazy` OS call.** `exact-span-large`
never claimed otherwise and the R11-7 design doc's own §2.6 concern (slot
pressure at high live-object counts) is not mentioned anywhere in the R12-3
or R12-4 commit messages — because neither touches it.

### 2.2 R12-4 (`large-reserved-capacity`, `fc155c9`) — closes a *different*
### side effect of (a), also not (b)

R12-4 exists purely to counteract exact-span-large's own side effect: with
committed span shrunk to the exact request, OPT-G's in-place-grow fast path
lost its committed headroom, so realloc growth mostly fell to the slow
alloc+copy+free path. R12-4 reserves (not commits) a geometric 2x VA span
up front and commits incrementally on growth. This is a **latency**
mitigation for realloc chains on the *same* object — it has nothing to do
with the number of distinct live objects or `SegmentTable` slot pressure;
`reserved_capacity` is a per-segment field carried in the *same* one
`SegmentTable` slot the segment already had. Confirms: R12-4 does not touch
(b) either.

---

## 3. Does the (b)-shaped gap (`MAX_SEGMENTS` / syscall frequency) actually
## bite on a real workload here?

`MAX_SEGMENTS = 1024` (`src/alloc_core/segment_table.rs:64`) is confirmed
unchanged by R12-3/R12-4 — a hard, compile-time cap on live Large (and
Small-segment) registrations, extensively guarded by existing regression
tests (`tests/regression_large_align_no_segment_exhaustion.rs`,
`tests/segment_table_recycle.rs`, `tests/regression_own_thread_large_no_leak.rs`,
etc., all asserting correct behavior *at* or *beyond* 1024 concurrent
segments — i.e. the crate already treats hitting this cap as an expected,
handled edge case, not a silent failure mode).

Two facts narrow whether this cap is the real bottleneck R11-7 imagined for
the 1.25–2.0 MiB range specifically:

1. **Under `medium-classes-wide` (already shipped, R9-4), `SMALL_MAX` is
   1.75 MiB** (`src/alloc_core/size_classes.rs:37,169`: wide classes take
   `SMALL_MAX` from 1 MiB to 1.75 MiB). That means **1.25/1.5/1.75 MiB
   objects already route through the Small-class carve path, not Large**,
   and Small-class carve does **not** consume one `SegmentTable` slot per
   *object* — it consumes one slot per *segment*, shared by however many
   same-class blocks fit (density 2/1/1 in a plain 4 MiB segment per R9-4's
   own numbers). Only the **2.0 MiB class** — which was never shipped
   (R9-4 explicitly excluded it: `floor(4Mi/2Mi)-1 = 1`, no density win in
   a 4 MiB segment) and does not exist in `SIZE_CLASS_TABLE` today — would
   fall to the Large path and hit the one-slot-per-object tax page-run was
   meant to fix. R11-7's own target range is thus **already mostly not a
   Large-path/`MAX_SEGMENTS` problem** for three of its four listed classes.
2. **No workload, test, benchmark, or example in this repository
   demonstrates or exercises "many-thousands-of-simultaneously-live
   1.25–2.0 MiB objects."** A repo-wide search (`grep` across
   `tests/`, `benches/`, `examples/`, `docs/perf/*.md`) for this pattern
   finds nothing outside R11-7's own hypothetical framing (`docs/perf/R11_7_PAGE_RUN_LAYER_DESIGN.md:438`,
   its illustrative "100 live 1.5 MiB objects" arithmetic). The
   `medium_classes_wide_correctness.rs` test suite exercises single-digit
   object counts per class (correctness/density spot-checks), not scale.
   `MAX_SEGMENTS`-exhaustion tests in this codebase exist for **Small/Large
   segments in general** (proving the recycle mechanism works correctly at
   or past 1024), not specifically for wide-medium-class-sized objects —
   i.e. the crate already has tooling and tests proving `MAX_SEGMENTS`
   exhaustion is handled gracefully (recycling, `None` return, no leak/UB)
   for the general case; there is no evidence this specific size range is
   a distinguished pain point beyond that general, already-tested ceiling.

Only the never-shipped 2.0 MiB class would need page-run's arena-sharing to
become density-viable at all — and shipping it is explicitly **gated on
page-run existing** per R11-7 §6.2, i.e. it is not an independent pressure
today; nothing currently allocates 2.0 MiB objects through this crate's
Large path in volume.

---

## 4. Verdict

**Exact-span-large (R12-3) closed the RSS/committed-bytes side of the
density gap (problem (a)) essentially completely (~1.00–1.05x residual
amplification, down from 2–15.8x) for the 1.25/1.5/1.75/2.0 MiB range, and
R12-4 closed the realloc-latency side effect that fix introduced. Neither
touches, nor was ever advertised to touch, the `SegmentTable`-slot /
OS-reservation-syscall pressure (problem (b)) that page-run's `PageRunTable`
design specifically existed to avoid. But problem (b) does not have a
demonstrated victim in this codebase**: three of the four target classes
(1.25/1.5/1.75 MiB) already route through the Small-class path under
shipped `medium-classes-wide` (one `SegmentTable` slot per *segment*, not
per *object*, since R9-4), the fourth (2.0 MiB) was never shipped and its
own design doc gates its shipping on page-run existing, and no test,
benchmark, or workload anywhere in this repository exercises or motivates
"many-thousands-of-live medium objects" as a real scenario for this size
range specifically.

**Decision: NO-GO on implementing the page-run layer now.** R12-3 addressed
the actual, measured pain (RSS amplification up to 15.8x on the objects
that route through Large) at a small fraction of the design/correctness
cost R11-7 itself quantified (§0/§4: "six of eleven cross-cutting mechanisms
need genuinely new, parallel code," "closer to a second subsystem," a
multi-phase mini-project comparable to the original `medium-classes`/
`medium-classes-wide` build-out). Spending that budget now, against a
`MAX_SEGMENTS`-pressure scenario with zero demonstrated occurrence in this
project's tests, benchmarks, or examples, fails this project's own
consistent pattern (R9-4, R10-4, R11-3, R12-3) of gating heavyweight new
subsystems on measured pain, not hypothetical pain. This matches the task
brief's own framing of NO-GO/defer as the legitimate, likely outcome of
this specific re-evaluation.

**This is not a permanent close.** If a future need materializes —
e.g. a real workload that allocates thousands of simultaneously-live
1.25–2.0 MiB (or larger, uniform-size) objects and is measured to be
`MAX_SEGMENTS`-bound or OS-reservation-syscall-bound specifically (not
RSS-bound, since that part is now solved) — `R11_7_PAGE_RUN_LAYER_DESIGN.md`
remains a complete, reusable CONDITIONAL-GO design for that scenario and
should be the starting point, re-validated against whatever
`MAX_SEGMENTS`/class-table state exists at that time.

---

## 5. Caveats

- No code, test, or benchmark was written or run this session — this is a
  re-evaluation against already-measured R12-3/R12-4 numbers (reproduced
  personally by the committing agent per `2593d30`'s and `fc155c9`'s commit
  messages) and already-shipped `medium-classes-wide` constants
  (`src/alloc_core/size_classes.rs`), not new measurement.
- "No demonstrated victim" (§3 point 2) is an absence-of-evidence finding
  from this repository's own tests/benches/examples/docs, not a proof that
  no such workload could ever exist for a downstream user of this crate.
  The decisive point per the task's own framing is that this codebase has
  no data motivating the investment now, which is the standard this
  project has consistently applied (R9-4 dropped 1.5/1.75 MiB "not yet
  needed" tuning; R10-4 and R11-7 themselves both reached only CONDITIONAL,
  not unconditional, GO absent real measurement).
