# R17-10 — Batched deferred reclaim: design (NOT implementation)

**Task:** R17-10 (task #327, P3 "design → prototype"). **DESIGN-ONLY.** No
`src/` change. This document proposes a design for a follow-up round's
implementation + gate; it does not implement or benchmark anything itself.

**Date:** 2026-07-25. **Base revision:** `main` @ `1117198` (R17-9 just
landed).

---

## 0. Where this task comes from

The Round 17 plan (`docs/reviews/2026-07-24-r17-plan.md`, row R17-10) names
this "candidate #1 for a real production speedup," and its wording — *"one
`sync_directory_for_segment_classes` per segment, batched directory/decommit
transition at recycle"* — is a near-verbatim restatement of
`docs/perf/R14_3_CLASS_AWARE_DIRTY_FIXED_WORK_AB.md` §6's own
"future-optimization note" (not implemented there, explicitly flagged for a
later round):

> "a plausible future target is batching/amortizing that reclaim (e.g.
> coalescing `sync_directory_for_segment_classes` calls per-segment rather
> than per-block, or batching the recycle-time drain instead of doing it
> inline) so the work shrinks in TOTAL, not just moves within the round."

R14-3 found that class-aware-dirty's dramatic sub-window win (~5.5×–21.7×
depending on host/run) does not survive at nearly that magnitude on the
full-round axis (~11%–35% depending on run, and NOT statistically
significant in three of four process-level fixed-work re-measurements —
R14-3 §2.2, R17-7 §2.4). The open question this task's plan entry is
reacting to: is there real, uncaptured batching headroom in the
drain/reclaim/recycle path itself (independent of class-aware routing),
which would show up on the full-round axis rather than just moving cost
around within it?

**Finding up front, stated honestly (see §1.1):** part of the plan's own
premise — *"per each reclaimed block, directory-sync is called separately"*
— is **not accurate as read from current code**. `sync_directory_for_segment_
classes` has been batched to one call **per segment per drain visit** since
R8-1 (task #214, pre-dates this whole round). The batching gap that
genuinely exists is narrower and in a different place: the **decommit-check
call**, and the complete absence of any **cross-segment** batching within one
`drain_dirty_segments` sweep. Both are documented precisely below, each
against the exact lines that show the current (non-batched) behavior and an
already-shipped, already-proven sibling mechanism this design proposes to
reuse rather than invent from scratch.

---

## 1. Problem

### 1.1 What is ALREADY batched (correcting the plan's premise)

`AllocCore::drain_dirty_segments`
(`src/alloc_core/alloc_core_small.rs:2521-2691`) is the drain this whole
class-aware-dirty line of work (R9-6/R12-7/R13-9/R14-3) is about. Its
segment-visit loop (`for (w, ds_word) in scan_source.iter().enumerate()` →
`while bits != 0`, lines 2580-2688) does, for each dirty segment bit it
finds:

1. `ring.drain(|off| { ... })` (line 2628) — **one call that drains the
   segment's ENTIRE `RemoteFreeRing` in one pass**, whatever its occupancy.
   Inside the closure, `changed_classes: u64` (line 2627) accumulates
   `1u64 << entry_class_idx(off)` for every successfully reclaimed entry —
   this is already exactly a **per-segment class presence mask**, gathered
   during the drain, one bit per size class (≤64 classes fit one `u64`;
   `SMALL_CLASS_COUNT = 49` default, `src/alloc_core/size_classes.rs:165`,
   58 under `medium-classes` — both ≤ 64).
2. **One** `self.sync_directory_for_segment_classes(base, sid,
   changed_classes)` call (line 2654), AFTER the ring is fully drained —
   `src/alloc_core/alloc_core_small.rs:2399-2423`'s doc comment states this
   explicitly: *"inspecting ONLY the classes whose bit is set in
   `changed_classes`... instead of sweeping all `SMALL_CLASS_COUNT`
   classes... O(popcount(changed_classes)) reads instead of O(SMALL_CLASS_
   COUNT)."* This was R8-1 (task #214), landed well before R17.

So the "per-segment class mask, one sync call per segment" the plan's
wording asks for **already exists** — it is not new design surface, and the
task's assumption that directory-sync fires once per reclaimed *block* is
refuted by reading the code: it fires once per *segment* per drain visit,
gated on a bitmask accumulated across that segment's whole ring. The same
`drain_heap_overflow` overflow-ring path
(`src/registry/heap_core_xthread.rs:616-645`, `:676-687`) is the one
directory-sync call site that genuinely IS per-entry rather than
per-segment — but that ring is a cross-segment MPSC queue where one drain
call routes entries to many different `base`s, so a single accumulated mask
is not applicable there the same way (see that file's own comment at
line 618-622 acknowledging the asymmetry: *"a per-entry immediate sync ...
instead of a batched bitmask, because `HeapOverflow` is a cross-segment MPSC
ring"*). This design does not touch that path.

### 1.2 What is genuinely NOT batched — decommit-check per reclaimed block

Inside the same `ring.drain` closure (`alloc_core_small.rs:2628-2650`), each
successfully reclaimed entry calls the **per-block** decommit check:

```text
if reclaimed {
    #[cfg(feature = "alloc-decommit")]
    if Self::dec_live_and_maybe_decommit(base, small_cur) {
        decommit_happened = true;
    }
    changed_classes |= 1u64 << entry_class_idx(off);
}
```

`dec_live_and_maybe_decommit` (`src/alloc_core/alloc_core_small_pool.rs:78-113`)
decrements the segment's owner-only `live_count` by 1 and, only on the
transition to `live == 0` (plus `base != small_cur`, not-already-decommitted,
kind `Small`), returns `true` (the caller then calls
`release_or_pool_empty_segment`, line 2671). Called once per reclaimed
entry — for a ring holding N entries this is N separate `dec_live`
decrement+compare sequences instead of one.

**This is not hypothetical waste with no known fix.** The exact same file
already carries a proven-identical batched sibling,
`dec_live_batch_and_maybe_decommit`
(`src/alloc_core/alloc_core_small_pool.rs:115-159`, "E3, task W4"), built for
a DIFFERENT call site — `flush_run`'s same-segment magazine-flush batch
(`src/alloc_core/alloc_core_small_magazine.rs:586-670`). Its own doc comment
(lines 120-132) gives the exact correctness argument this design needs:

> "within a same-segment run `live` can only reach 0 at the LAST accepted
> block... The final `live_count` is identical: `sub_live(k)` == `k`
> `dec_live`s. Decommit fires at most once, on the SAME transition... under
> the SAME proviso... Checking the proviso ONCE on the post-`sub_live`
> value therefore reproduces the loop exactly."

`drain_dirty_segments`'s ring-drain closure does not use this sibling — it
still calls the per-block `dec_live_and_maybe_decommit` inline. Switching it
to accumulate an `accepted_count: u32` in the closure (mirroring
`flush_run`'s own `accepted_count` at `alloc_core_small_magazine.rs:615/658`)
and calling `dec_live_batch_and_maybe_decommit(base, accepted_count,
small_cur)` once, AFTER `ring.drain(...)` returns (same place the
`sync_directory_for_segment_classes` call already sits), reuses an
ALREADY-PROVEN-correct primitive with no new correctness argument to invent
— only the wiring changes.

### 1.3 What is genuinely NOT batched — cross-segment finalization within one sweep

`drain_dirty_segments` finalizes a newly-emptied segment **inline**, the
instant it is discovered (`release_or_pool_empty_segment(base)` at line
2671, followed immediately by `continue` to the next dirty bit). If a single
drain sweep (one `drain_dirty_segments` call) empties several segments — a
realistic case under a bursty mixed-class free storm — each one gets its own
immediate pool-admission-or-release decision, with no opportunity to look at
the whole batch together (e.g., admitting the K found-empty-this-sweep
segments to the pool in one pass, or doing the `SegmentTable::recycle`
bookkeeping — `hash_remove`, `own_cache_clear`, `free_list_push`,
`src/alloc_core/segment_table.rs:337-403` — for several segments back to
back rather than interleaved with the rest of the per-segment drain body).

**A near-identical deferred-batch pattern already exists elsewhere in this
codebase**, for a structurally similar (but not identical) reason:
`HeapCore::drain_heap_overflow` (`src/registry/heap_core_xthread.rs:497-733`,
R11-2/R12-6) collects emptied bases into a bounded `emptied_bases:
[*mut u8; EMPTIED_BASES_CAP]` array (`EMPTIED_BASES_CAP = 64`, line 586)
DURING its drain closure instead of finalizing inline, then runs
`release_or_pool_empty_segment` for each collected base in one loop AFTER
the drain returns (lines 713-720), with a bounded-tail fallback
(`finalize_orphaned_empty_segments`, `alloc_core_small_pool.rs:287-356`) for
the rare case for more than 64 distinct bases empty in one pass. **That
precedent's motivating reason is different from this design's**: for
`drain_heap_overflow`, deferral is load-bearing for *correctness* (its own
comment, lines 520-530: draining a SECOND entry for a base already finalized
by an EARLIER entry in the SAME pass would touch freed/decommitted memory,
because that ring can carry multiple entries for the same base in one
drain). `drain_dirty_segments`'s per-segment dirty-bitmap loop does **not**
have that hazard — each dirty bit (hence each `base`) is visited **at most
once** per sweep by construction (the bitmap word is `swap(0, Acquire)`'d
once, line 2584, and a segment's dirty bit cannot be set twice in the same
word-snapshot). So a deferred-batch scheme here is a **pure locality/hot-path
shape** proposal, not a correctness requirement — it must be justified on
its own measured merits (§5), not on the strength of the `drain_heap_
overflow` precedent's different justification.

### 1.4 Cost accounting, current code (per drain-visited segment, N entries in its ring)

| Step | Current cost | Already batched? |
|---|---|---|
| Ring drain (`ring.drain`) | 1 call, whole ring | Yes (always has been) |
| `changed_classes` accumulation | 1 bitmask, O(N) OR-ops inside the ring-drain closure | Yes (R8-1) |
| Directory sync | 1 call, O(popcount(changed_classes)) reads | **Yes (R8-1)** — plan's premise about this step is refuted |
| Decommit-check (`dec_live_and_maybe_decommit`) | N calls, O(1) each (decrement + 3 comparisons) | **No** — proven-identical batched sibling exists, unused here |
| Pool-admit-or-release (`release_or_pool_empty_segment`) | 1 call per newly-emptied segment, called inline as discovered | Partially — per-segment already, but NOT batched ACROSS multiple segments emptied in the same sweep |
| `SegmentTable::recycle` bookkeeping (release leg only) | 1 call per released segment, inline | Same as above |

The realistic per-sweep win from closing the two genuine gaps (row 4, row 5)
is bounded by: (a) row 4 — `(N-1)` avoided decrement+compare sequences per
segment whose ring holds N>1 entries (small, since each is O(1) already —
this is a constant-factor tightening, not an algorithmic class change,
unlike the O(D)→O(D_class) win class-aware-dirty already delivered), and
(b) row 5 — avoided redundant `SegmentTable` structural-mutation overhead
when K>1 segments empty in the same sweep (K is expected to be small in
most workloads; R9-6's own worst-case bench workload empties zero segments
per sweep by construction — its blocks stay live for the whole round, see
`tests/r9_6_class_aware_dirty_judge.rs` step 1-2). **This is explicitly a
smaller, more speculative win than class-aware-dirty's O(D)→O(D_class) — see
§5 for why the measurement plan must establish this BEFORE any
implementation commitment, exactly the same discipline R9-6 applied before
R12-7 built anything.**

---

## 2. Proposal

Two independent, separately-gateable sub-designs — do NOT bundle them into
one all-or-nothing change, so each can be measured and kept/reverted on its
own merit (mirroring this project's own "13.4a bitmap guard, 13.4b two-list,
keep 13.4b ONLY if the bench shows improvement" discipline,
`docs/PHASE13_4_DEALLOC_DESIGN.md` §2).

### 2.1 Sub-design A — reuse `dec_live_batch_and_maybe_decommit` in `drain_dirty_segments`

Change `drain_dirty_segments`'s ring-drain closure
(`alloc_core_small.rs:2628-2650`) to:

1. Track `accepted_count: u32` (mirrors `flush_run`'s `accepted_count`,
   `alloc_core_small_magazine.rs:615/658`) instead of calling
   `dec_live_and_maybe_decommit` inline per reclaimed entry.
2. After `ring.drain(...)` returns (same point the existing
   `sync_directory_for_segment_classes` call sits, line 2654), call
   `Self::dec_live_batch_and_maybe_decommit(base, accepted_count, small_cur)`
   once, replacing the per-block call.

**No new type, no new field.** This is wiring an EXISTING, already-proven
(by the E3/task-W4 doc comment's own byte-identical argument, §1.2) function
into a second call site it was not written for but is provably correct at
(the argument in `alloc_core_small_pool.rs:120-132` is call-site-agnostic —
it is about `live_count` transition arithmetic, not about `flush_run`'s
particular shape). The order of operations changes (decommit decision is
made once, after the whole ring is drained, instead of possibly mid-drain)
but the OBSERVABLE outcome is identical per that same proof: decommit can
only fire at the LAST accepted entry that brings `live` to 0, which is
unaffected by whether the check happens immediately at that entry or once
at the end — the segment does not get a chance to un-empty between "the
last entry drained" and "the deferred check runs," because nothing else
touches `live_count` for this `base` between those two points (the owner is
mid-drain, single-threaded on this path).

### 2.2 Sub-design B — deferred cross-segment finalization within one `drain_dirty_segments` sweep

Collect newly-emptied bases from `drain_dirty_segments`'s segment loop into
a small bounded on-stack buffer (same shape as `drain_heap_overflow`'s
`emptied_bases: [*mut u8; EMPTIED_BASES_CAP]`, `heap_core_xthread.rs:586-589`,
though a MUCH smaller cap likely suffices here — unlike the overflow ring, a
segment can appear **at most once** per sweep in this loop, so no dedup scan
is needed, only a fixed cap and a "buffer full → finalize immediately as a
fallback for this one segment" tail case), instead of calling
`release_or_pool_empty_segment` inline at line 2671. Finalize the whole
collected batch in one pass AFTER the `for (w, ds_word) in
scan_source.iter()...` loop fully returns.

**Sizing:** `drain_dirty_segments` visits at most `popcount` bits across
`DIRTY_BITMAP_WORDS` words (`MAX_SEGMENTS / 64 = 64` words, `MAX_SEGMENTS =
4096`, `src/alloc_core/segment_table.rs:64`) in one call — a MUCH smaller
practical cap than `drain_heap_overflow`'s 64 (which defends against up to
`HEAP_OVERFLOW_CAP = 2048` distinct native entries in one MPSC drain). A cap
in the 8-16 range is very likely generous for this path's realistic case
(most sweeps empty 0 segments — R9-6's own bench workload never does, by
construction of its "blocks stay live" harness step); this needs to be
confirmed empirically before picking a number, not guessed here.

**Do NOT bundle with sub-design A's `accepted_count` change mechanically** —
sub-design B changes WHEN `release_or_pool_empty_segment` runs relative to
the rest of the sweep (still within `drain_dirty_segments`, just after
instead of during), not HOW the decommit decision itself is computed
(sub-design A's concern). They compose (A decides "does this segment go
empty," B decides "when do we act on that"), but should be introduced and
gated as two independent changes so a regression in one does not obscure the
other's own effect.

### 2.3 Explicitly rejected for this round: per-segment class-mask STORAGE change

The plan wording could be read as also asking for a NEW per-segment stored
class-mask structure (as opposed to the transient `changed_classes: u64`
local this design's §1.1 shows already exists). This design does **not**
propose adding one: the transient, drain-call-scoped `changed_classes`
local already IS the per-segment class mask the directory-sync step needs,
and the class-aware-dirty feature's separate, ALREADY-SHIPPED
`PerClassDirty` sidecar (`src/alloc_core/dirty_by_class.rs`,
`WORDS_PER_CLASS = MAX_SEGMENTS / 64 = 64`,
`src/alloc_core/segment_directory.rs:172`, 8.0 KiB page-rounded per
materialised heap — R13-9 §5.1's corrected figure) already IS the
persistent per-(segment,class) presence structure that drives WHICH segments
get visited in the first place. Adding a third, redundant class-mask
structure would duplicate state two mechanisms already provide with no
stated new capability — out of scope.

---

## 3. Invariants to preserve

This is the section the plan brief specifically asked to ground in
`docs/RACE_DRAIN_RECLAIM.md` and the class-aware-dirty lost-wakeup protocol
(`docs/perf/R12_7_CLASS_AWARE_DIRTY_ROUTING_GATE.md` §3.2). Neither
sub-design changes the SET of entries reclaimed or the SET of directory bits
published in a drain pass — both are pure reorderings of WHEN existing,
unchanged computations run, not WHAT they compute. Each invariant below is
checked against that claim specifically.

### 3.1 Lost-wakeup protocol (R12-7 §3.2) — untouched

R12-7's protocol decision: *"the ring is drained UNCONDITIONALLY and
COMPLETELY once a segment is visited — the per-class bit is a VISIT HINT
only, never a partial-drain filter."* Neither sub-design touches `ring.drain`
itself, its closure's entry-acceptance logic, or which segments get visited
(that is governed entirely by the existing `scan_source` bitmap selection,
`alloc_core_small.rs:2574-2578`, unchanged by this proposal). Sub-design A
only changes when the `live_count`/decommit ARITHMETIC that already runs
once per entry gets EXECUTED (batched to once after the same fully-drained
ring, versus once per entry during); it does not skip any entry, and does
not change which classes get published (`changed_classes` accumulation is
untouched — still gathered per-entry inside the SAME `ring.drain` closure).
Sub-design B only changes when the ALREADY-COMPUTED "this segment just
emptied" fact gets ACTED on (deferred to after the sweep's segment loop
instead of immediately); it does not change the fact itself or which
segments produce it. **The lost-wakeup guarantee — "a stale or redundant bit
costs at most one wasted visit, never a silently-skipped ring entry" — is
unaffected because both changes operate strictly AFTER the point in the
existing code where every ring entry for a visited segment has already been
drained and classified.**

### 3.2 §11-13 class-carried-in-ring-entry invariant (`RACE_DRAIN_RECLAIM.md`)

The root fix this file documents (§13: *"carry the class through the ring...
NEVER `page_map` for the class"*) is orthogonal to both sub-designs — neither
touches `reclaim_offset`/`reclaim_offset_checked`'s class derivation
(`remote_free_ring::entry_class_idx(off)`, still called exactly where it is
today, inside the SAME `ring.drain` closure). Sub-design A/B do not add a
new reclaim path or a new class-derivation site.

### 3.3 M6 decommit-without-epoch proof (`PHASE35_DECOMMIT_DESIGN.md` §1)

The proof's four numbered points (decommit only at `live_count == 0`; a late
cross-thread free at `live==0` is a double-free caught by the `is_free`
bitmap guard before any write; `reclaim_offset` on a stale entry never
touches the decommitted page; reclaim and decommit are both owner-side,
serialized) all depend on **the FINAL `live_count` value and the fact that
the decommit decision is made by the OWNER, synchronously, before any other
owner action can observe or act on the segment**. Sub-design A's own
correctness argument (§2.1, quoting `dec_live_batch_and_maybe_decommit`'s
doc comment) is precisely that the batched and per-block forms produce the
IDENTICAL final `live_count` and fire the decommit transition at the
IDENTICAL logical point (the entry that brings `live` to 0) — so the M6
proof's four points hold **verbatim**, unchanged, because the thing they are
proofs ABOUT (final `live_count`, single-owner serialization, decommit only
at true zero) does not change. Sub-design B defers WHEN
`release_or_pool_empty_segment` (which performs the actual
`os::decommit_pages`/reservation-release) runs relative to the REST of the
sweep, but still strictly AFTER the segment's own ring has been fully
drained and its `live_count` has settled at 0 (sub-design A or the current
per-block check, either way, already ran before a base is queued for
deferred finalization) — so no OTHER segment's ring-drain in the SAME sweep
can be mid-flight against an ALREADY-finalized base's now-decommitted
memory, because (§1.3) a base appears at most once in this loop's dirty-bit
scan, so there is no "entry #3 of 5 against an already-freed base" hazard
`drain_heap_overflow`'s OWN deferral exists to prevent (§1.3) — that hazard
structurally cannot arise here, which is exactly why sub-design B is
optional/performance-only rather than a correctness requirement.

**One new invariant sub-design B MUST establish (not automatically true —
requires its own explicit check, not just inherited from A/§1.3):** while a
base sits in the deferred-finalization buffer (discovered-empty but not yet
released/pooled), no code within the REST of the same `drain_dirty_segments`
call may read or write that segment's payload — only true if nothing later
in the SAME sweep can re-visit an ALREADY-processed dirty bit for that base
(true, since `scan_source.swap(0, Acquire)` clears each word before
scanning its bits, and the outer `for (w, ds_word)` loop never revisits a
word) — **and** no OTHER field of `AllocCore` read later in the SAME call
(after the segment loop, before the deferred-finalization pass runs) assumes
every discovered-empty-this-sweep segment has ALREADY been released/pooled
(this needs an explicit audit of what runs between "segment loop ends" and
"deferred finalization pass runs" in the modified function — trivial if the
finalization pass is placed immediately at the end of `drain_dirty_segments`
with nothing else between, which is the design's stated plan, but must be
checked against the ACTUAL diff at implementation time, not assumed from
this document).

### 3.4 R17-4's segment-leak class of bug — explicit cross-check

**Correction to the plan's premise:** R17-4 is task **#321**, not #329 as an
earlier draft of this section stated, and it is **already resolved** —
landed in commit `1b761f4` ("route promoted Large segments by kind on
dealloc under medium-classes, fixing a segment leak"), which predates this
document's own `1117198` base revision (R17-9). It is not an open item this
design needs to wait on. R17-4's fix touched exactly `src/registry/
heap_core_free.rs` (`HeapCore::dealloc_own_thread_with_base`'s fastbin
magazine dispatch, keyed on `class_for(layout.size())` instead of segment
`kind`) plus its own new regression test — it did **not** touch
`alloc_core_small_pool.rs` at all (confirmed via `git show --stat 1b761f4`),
so there is no adjacent-code overlap with this design's proposed changes.
R17-4's defect class was a MIS-ROUTING bug (wrong class/kind used in a
dealloc-routing comparison), not a batching-timing bug, and is structurally
unrelated to either sub-design here: sub-design A reuses
`dec_live_batch_and_maybe_decommit`'s EXISTING `kind_at(base) ==
SegmentKind::Small` check (`alloc_core_small_pool.rs:152-154`, identical to
`dec_live_and_maybe_decommit`'s own check at lines 97-99 — same code,
verbatim, already shared by both functions today, unchanged by this
proposal), and sub-design B does not touch kind/class routing at all — it
only changes the ORDER of already-kind-and-class-correct calls. **This
design carries no new surface for the R17-4 bug class, and no dependency on
it** — the implementation task (§7) does not need to wait for or re-run
anything R17-4-specific beyond the project's normal full test suite.

---

## 4. API / data-structure changes

Deliberately minimal — no new persistent state, per §2.3's rejection of a
redundant stored class-mask:

- **Sub-design A:** no new function. `drain_dirty_segments`'s ring-drain
  closure (`alloc_core_small.rs:2628-2650`) changes its captured-state shape:
  replace the per-entry `#[cfg(feature = "alloc-decommit")] let mut
  decommit_happened = false;` (line 2622) + inline `dec_live_and_maybe_
  decommit` call (line 2635) with an `accepted_count: u32` accumulator
  (`+= 1` wherever `reclaimed` is true, mirroring
  `alloc_core_small_magazine.rs:658`) and a single post-drain call to the
  EXISTING `Self::dec_live_batch_and_maybe_decommit(base, accepted_count,
  small_cur)` (already `pub(super)`, `alloc_core_small_pool.rs:137`, visible
  from `alloc_core_small.rs` — same crate module tree, no visibility change
  needed). `decommit_happened` becomes `dec_live_batch_and_maybe_decommit`'s
  own bool return, used exactly as `decommit_happened` is used today (line
  2670's `if decommit_happened { self.release_or_pool_empty_segment(base);
  ... }`).
- **Sub-design B:** one new bounded local buffer INSIDE
  `drain_dirty_segments` (stack-allocated, function-scoped — not a new
  `AllocCore` field, not a new persistent type), sized by a new `const`
  (name TBD at implementation time, e.g. `DIRTY_DRAIN_EMPTIED_CAP`), mirroring
  `drain_heap_overflow`'s `EMPTIED_BASES_CAP` local (`heap_core_xthread.rs:586`)
  but WITHOUT that function's dedup scan (§2.2 — not needed here, since a
  base cannot repeat within one `drain_dirty_segments` sweep) and WITHOUT a
  separate "overflowed" fallback-sweep call (a buffer-full segment can
  simply fall back to calling `release_or_pool_empty_segment` inline,
  right there, exactly as today — no new fallback function needed, since
  unlike `drain_heap_overflow`'s tail case this is not a correctness gap,
  just "did not get the batching benefit this one time").

No change to `PerClassDirty`, `SegmentDirectory`, `RemoteFreeRing`,
`HeapSlotRemote`, or any wire/packed-entry format. No new feature flag is
proposed — both sub-designs are internal-only changes to code already gated
`#[cfg(feature = "alloc-decommit")]` (both `dec_live_and_maybe_decommit` and
its batched sibling already carry that gate; `drain_dirty_segments` itself
is gated `alloc-xthread + alloc-segment-directory + not(numa-aware)`, and the
decommit-specific lines within it are ALREADY additionally
`#[cfg(feature = "alloc-decommit")]`-gated today, e.g. line 2621/2634/2669) —
this is a same-feature-surface, internal-only reshaping, not a new opt-in
axis to add to the feature matrix.

---

## 5. Measurement methodology for the next round

Per CLAUDE.md's "Phased delivery" wall-clock-gate rule (added specifically
because of this same class-aware-dirty investigation, R14-3 §5): *"a
wall-clock gate must report both the sub-window metric and the full-round
criterion time for the same harness... any material gap between the two
axes is itself a result requiring explanation."* This design's own §1.4
already predicts the likely SHAPE of the result — a smaller, more
constant-factor effect than class-aware-dirty's algorithmic O(D)→O(D_class)
win — so the measurement plan must be capable of DISTINGUISHING "small real
effect" from "no effect, noise" from the start, not merely capable of
reporting a headline multiplier.

### 5.1 Gate structure (two-stage, same discipline as R9-6→R12-7)

**Stage 1 (measure first, no `src/` risk):** before implementing either
sub-design, build a counter-level judge analogous to R9-6's
`WASTED_DIRTY_DRAINS` — a diagnostic-only counter (gated
`#[cfg(feature = "alloc-stats")]`, Relaxed ordering, no behavior change)
counting, across a representative mixed-class churn workload:
(a) how many `dec_live_and_maybe_decommit` calls happen per drain-visited
segment (to quantify sub-design A's N-per-segment→1-per-segment reduction
directly, in the same units R9-6 used for its own waste ratio), and
(b) how many segments empty per `drain_dirty_segments` call, and the
DISTRIBUTION of that count across many calls (to establish whether
sub-design B's K>1-per-sweep scenario is common or a rare tail in realistic
workload shapes — §1.4 already flags this as the open empirical question
that decides whether sub-design B is worth building at all). **If stage 1
shows segments essentially never empty >1-at-a-time in the target workload
shapes, sub-design B should be downgraded to NO-GO before any wall-clock
bench is written** — same "measure before implementing" discipline R9-6
applied, which is exactly what avoided over-building class-aware-dirty on
guesswork.

**Stage 2 (only if stage 1 shows a non-trivial ratio):** implement the
sub-design(s) stage 1 justified, behind existing feature gates (no new
flag, per §4), and run the SAME dual-axis wall-clock protocol R13-9/R14-3/
R17-7 already established for class-aware-dirty:

1. **Criterion sub-window + full-round, same harness shape as
   `benches/r12_7_class_aware_dirty_wallclock.rs`** — reuse its dual-`Instant`
   pattern (R14-3 change #1: one timer for the existing narrow window, one
   OUTER timer spanning the full `run_round`, both printed on every sweep
   line). Do NOT report the sub-window number alone as a headline under any
   circumstance — R14-3's whole point was that doing so once already
   produced a "21.71×" figure that overstated the full-round effect by
   roughly 2 orders of magnitude.
2. **Fixed-work, process-level A/B/B/A judge with in-process warm-up
   rounds** — reuse `examples/_shared/paired_ab_class_aware_dirty_workload.rs`'s
   established shape and `scripts/paired-ab-runner.mjs`, following R17-7's
   own correction to R14-3's original single-round design (`PAIRED_AB_
   WARMUP_ROUNDS`, default 3, discarded before the single measured round —
   `examples/paired_ab_class_aware_dirty_{off,on}.rs`'s established
   pattern). 20 pairs (80 process launches) per comparison, TWO independent
   repeats minimum (R14-3's run 1 and run 2 disagreed on significance; R17-7's
   two warm-up runs also needed to be compared against each other before
   drawing a conclusion — a single run is not sufficient evidence either
   way, per this project's own accumulated experience with this exact
   harness family).
3. **Environment-quality disclosure BEFORE measuring, not after** — per
   R17-7's own methodology fix: check `wmic cpu get loadpercentage` (or the
   host's equivalent) and report it alongside the numbers, not silently.
   R17-7's own measurement ran at 80-100% background load and still reported
   the numbers with that caveat rather than discarding or re-chasing a
   "clean" run — this design's stage 2 should do the same, not retry until a
   flattering number appears.
4. **Same-vs-same control** (off vs off) run alongside every A/B comparison,
   exactly as R14-3 §2.2/R17-7 §2.4 did — this is the harness-sanity check
   that distinguishes "the harness itself has no significant self-difference"
   from "the treatment has no significant effect," and both R14-3 and R17-7
   needed it to interpret their own inconclusive results honestly.
5. **iai/Callgrind deterministic instruction count** on the 12 pre-existing
   `benches/perf_gate_iai.rs` single-thread benches — confirms (per R13-9 §2)
   that neither sub-design regresses the NON-remote hot path (both changes
   live entirely inside `#[cfg(feature = "alloc-decommit")]`-gated code
   already only reached from cross-thread reclaim paths, so a clean iai
   result here would be the same "feature compiled in, path unreached on
   these benches" signature R13-9 §2 already established for class-aware-dirty
   — worth re-confirming explicitly rather than assuming it transfers).

### 5.2 What would make this a GO vs a documented NO-GO

Given §1.4's own prediction (constant-factor tightening, not an algorithmic
class change), the honest bar this design sets for itself: a full-round,
fixed-work, process-level effect that reaches statistical significance
(paired t, same threshold R14-3/R17-7 used) in AT LEAST two independent
repeats, with the same-vs-same control confirming the harness itself has no
spurious self-difference in the same runs. If — as happened to
class-aware-dirty's OWN full-round measurement across R14-3's two runs and
R17-7's two runs (4 process-level measurements, 0 reaching significance in
the winning direction) — this design's sub-window number looks appealing but
the full-round, fixed-work number does not reach significance across
repeated independent measurement, the honest conclusion is the same one
R17-7 reached for class-aware-dirty: keep the change ONLY if it has an
independent justification (code simplicity from reusing an existing proven
primitive, for sub-design A specifically — §2.1's "no new correctness
argument to invent" IS such a justification on its own, independent of
wall-clock), not solely on an unconfirmed wall-clock win.

---

## 6. Risks / open questions

1. **Sub-design A's win may be too small to separate from noise.** §1.4
   already frames this honestly: the per-block decommit check today is O(1)
   (a decrement + 3 comparisons), so batching N of them into 1 saves
   (N-1)×(one decrement + up to 3 comparisons) — likely single-digit
   nanoseconds per segment at realistic N. This is smaller than
   class-aware-dirty's own algorithmic win, and class-aware-dirty's
   full-round effect ALREADY proved hard to separate from host noise across
   four independent process-level measurements (R14-3, R17-7). Sub-design A
   may land as a code-quality improvement (reuses an existing proven
   primitive, removes an inline/batched inconsistency between two call
   sites doing conceptually the same thing) with an UNPROVEN or
   statistically-inconclusive wall-clock effect — that would still be a
   legitimate, honestly-reported outcome per §5.2, not a failed task.
2. **Sub-design B's value is entirely conditional on §5.1 stage 1's
   empirical finding.** If realistic mixed-class churn workloads essentially
   never empty more than one segment per `drain_dirty_segments` sweep (very
   plausible — R9-6's OWN worst-case bench workload empties zero, by
   harness construction), sub-design B has no target to batch and should be
   dropped before implementation, not built speculatively. This must be
   checked BEFORE writing sub-design B's code, exactly as §5.1 stage 1
   specifies.
3. **Deferring `release_or_pool_empty_segment` changes the pool's
   observed admission order within one sweep.** Today, if segment X empties
   before segment Y within the same `drain_dirty_segments` call, X is
   admitted to the pool (or released) before Y is even discovered.
   Sub-design B would discover X and Y, THEN admit/release both — if
   `pool_cap` is nearly exhausted, the ORDER admission happens in can change
   which of X/Y ends up pooled vs. released (pool admission is a strict
   `pooled_count < pool_cap` gate, `alloc_core_small_pool.rs:257`). This is
   not a correctness bug (both orderings are valid outcomes — the pool has
   no documented FIFO/fairness contract between segments emptying in the
   same sweep), but it IS an observable behavior change worth flagging
   explicitly for the implementation task's own test suite to cover (does
   any existing test assert a SPECIFIC segment ends up pooled after a
   multi-empty sweep, in a way that would spuriously break under
   reordering? — an audit question for §7, not answered here).
4. **Interaction with the `class-aware-dirty` coarse-only latch
   (R13-1).** Neither sub-design reads or writes `sidecar_oom_latch` — both
   operate strictly after the scan-source (per-segment vs per-class bitmap)
   selection has already happened (§3.1) — but the implementation task
   should still run the existing `class-aware-dirty` test/loom suites
   (`tests/class_aware_dirty_routing.rs`, `tests/loom_class_aware_dirty.rs`)
   against this design's diff, since they are the most direct existing
   regression net for `drain_dirty_segments`'s behavior and would catch an
   accidental behavior change even though none is intended.
5. **R17-4 is already resolved, not open (correction — see §3.4).** Per §3.4,
   R17-4 (task #321, commit `1b761f4`) landed before this document's own
   base revision and touched only `heap_core_free.rs`, disjoint from this
   design's `alloc_core_small_pool.rs`/`alloc_core_small.rs` surface. No
   dependency or wait-on-resolution risk remains here.
6. **"Batched directory/decommit transition at recycle" (the plan's exact
   phrase) may refer to something this design does NOT cover.** The plan
   text could plausibly be read as asking about `HeapRegistry::recycle`
   (`src/registry/heap_registry.rs:342-375`) — but that function is a
   heap-SLOT-level CAS (`STATE_LIVE → STATE_FREE`) + free-stack push; it does
   **not** drain rings, sync directories, or touch segments at all (verified
   by reading its full body, quoted above). The "recycle-time drain" R14-3
   §6 speculated about is therefore not a literal cost inside
   `HeapRegistry::recycle` itself — the deferred cost R14-3 observed
   (§14-3's own §2.3: "where the ~17ms of window-axis savings actually
   goes") more likely lands on the FIRST alloc of a heap's NEXT owner after
   reuse (that owner's OWN `find_segment_with_free_impl` → `drain_dirty_
   segments` call, paying whatever drain work the PREVIOUS owner deferred
   by not being visited before the round's timed window ended) — i.e., the
   SAME `drain_dirty_segments` this design already targets, just observed
   from a different point in the round's timeline, not a separate mechanism
   living in `recycle` proper. This document flags the discrepancy rather
   than silently reinterpreting the plan's wording; the next round's
   implementer should re-confirm this reading against the actual bench
   trace (e.g., instrument `run_round`'s phases directly) before assuming
   it, since it is inference from reading code, not a directly observed
   trace in this design task's own scope.

---

## 7. Next-round plan (implementation + gate — NOT done here)

1. **Stage 1 judge** (§5.1): add the two diagnostic counters (decommit-check
   calls per drain-visited segment; segments-emptied-per-sweep distribution)
   behind `alloc-stats`, on the SAME workload-shape family
   `tests/r9_6_class_aware_dirty_judge.rs` already established (reuse, do
   not reinvent, its N∈{1,2,4,8} producer-class harness — but note its
   "blocks stay live for the whole round" step 1-2 construction means it
   will read K=0 for the segments-emptied metric by design; a SECOND
   workload variant that actually frees enough of a class to empty segments
   is needed to get a non-degenerate reading for sub-design B's question).
2. **Decision gate on sub-design B** based on stage 1's segments-emptied
   distribution (§5.1/§6.2) — proceed to implement it ONLY if a
   non-negligible fraction of sweeps empty >1 segment in the chosen
   representative workload(s).
3. **Implement sub-design A** (§2.1) — small, mechanical, reuses an
   existing proven primitive; independent of the sub-design B decision.
4. **Implement sub-design B** (§2.2) — only if step 2's gate passes.
5. **Regression suite**: full existing `class-aware-dirty`/`alloc-decommit`
   test and loom suites (§6.4), plus a NEW regression test asserting
   sub-design A's `live_count`/decommit-transition outcome is IDENTICAL to
   the pre-change per-block behavior on a workload that empties a segment
   mid-ring-drain (the direct counterfactual: assert the same segment ends
   up released/pooled at the same logical point, not just "eventually")."
6. **Dual-axis wall-clock gate** (§5) — both sub-window and full-round,
   with the fixed-work process-level A/B/B/A + same-vs-same control +
   environment-load disclosure, at least two independent repeats, written up
   as `docs/perf/R1X_Y_BATCHED_DEFERRED_RECLAIM_GATE.md` following this same
   file's sibling reports' format, with raw logs under
   `docs/perf/_raw_r1x_y_*.log` per the raw-log/summary-CSV conventions
   already established (CLAUDE.md "Raw perf logs" / "machine-readable
   summary" rules).
7. **Promotion recommendation, not decision** — per this project's
   established practice (R13-9 §7's own framing), the gate report
   recommends GO/CONDITIONAL-GO/NO-GO; the orchestrator/user decides whether
   either sub-design lands, exactly as `class-aware-dirty`'s own promotion
   was handled.

---

## 8. Files/lines this document is grounded in (for the next round's reader)

- `src/alloc_core/alloc_core_small.rs:2380-2423` — `sync_directory_for_
  segment_classes` (already-batched, per-segment, R8-1).
- `src/alloc_core/alloc_core_small.rs:2521-2691` — `drain_dirty_segments`
  (the drain this whole design is about; scan-source selection at
  2544-2578; per-segment loop at 2580-2688; per-block decommit check at
  2621-2635; per-segment sync call at 2651-2655; per-segment pool/release at
  2668-2679).
- `src/alloc_core/alloc_core_small_pool.rs:78-113` — `dec_live_and_maybe_
  decommit` (per-block, current call site's function).
- `src/alloc_core/alloc_core_small_pool.rs:115-159` — `dec_live_batch_and_
  maybe_decommit` (E3/task W4 — the already-proven batched sibling this
  design proposes reusing).
- `src/alloc_core/alloc_core_small_pool.rs:236-285` — `release_or_pool_
  empty_segment` (pool-admit-or-release-and-recycle decision).
- `src/alloc_core/alloc_core_small_pool.rs:287-356` — `finalize_orphaned_
  empty_segments` (R12-6 fallback-sweep precedent for a bounded dedup
  buffer overflowing).
- `src/alloc_core/alloc_core_small_magazine.rs:586-670` — `flush_run` (the
  ORIGINAL call site `dec_live_batch_and_maybe_decommit` was built for).
- `src/registry/heap_core_xthread.rs:497-733` — `drain_heap_overflow`
  (R11-2/R12-6 — the existing deferred cross-entry finalization pattern for
  the OTHER, cross-segment overflow ring; motivating reason differs from
  this design's, §1.3).
- `src/alloc_core/segment_table.rs:337-403` — `SegmentTable::recycle` (slot
  release bookkeeping: hash_remove, own_cache_clear, OS release, free-list
  push).
- `src/registry/heap_registry.rs:342-375` — `HeapRegistry::recycle` (heap
  SLOT recycle — confirmed NOT a drain; see §6 point 6).
- `src/alloc_core/dirty_by_class.rs`, `src/alloc_core/segment_directory.rs:170-172`
  — the existing `PerClassDirty` sidecar / `WORDS_PER_CLASS` (§2.3's
  rejected-alternative discussion).
- `src/registry/heap_core_xthread.rs:334-...` — `set_dirty_bit_for_segment`
  (producer side, unaffected by this design).
- `docs/RACE_DRAIN_RECLAIM.md` §11-14 — class-carried-in-ring-entry root fix
  (§3.2).
- `docs/PHASE35_DECOMMIT_DESIGN.md` §1 — decommit-without-epoch proof (§3.3).
- `docs/perf/R9_6_CLASS_AWARE_DIRTY_ROUTING_JUDGE.md`,
  `docs/perf/R12_7_CLASS_AWARE_DIRTY_ROUTING_GATE.md`,
  `docs/perf/R13_9_CLASS_AWARE_DIRTY_PRODUCTION_GATE.md`,
  `docs/perf/R14_3_CLASS_AWARE_DIRTY_FIXED_WORK_AB.md` (including its R17-7
  §2.4 addendum) — the measurement-methodology precedent this design's §5
  reuses in full.
- `docs/PHASE13_4_DEALLOC_DESIGN.md` — the "ship sub-part A, measure, keep
  sub-part B only if it helps" phased-delivery precedent (§2).
