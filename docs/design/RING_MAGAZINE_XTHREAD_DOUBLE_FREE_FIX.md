# Closing the ring↔magazine cross-thread double-free residual (task #164 F)

**Status:** DESIGN ONLY. This document evaluates candidate fixes and picks one.
It changes no source. Implementation is a separate future arc.

**NOT a 0.3.0 blocker.** The hole is honestly documented (`src/registry/heap_core.rs:848-885`,
`docs/FASTBIN_DESIGN.md:241-257`), pinned RED-and-`#[ignore]`d
(`tests/regression_xthread_double_free_residual.rs:97-99`), and modelled under loom
(`tests/loom_magazine_ring_compose.rs`). It PRE-EXISTS the magazine — it lives in
the 0.2.x fastbin too — and is only reachable via a genuine USER double-free (UB by
contract, as any double-free is). Э6 neither opened nor closed it. Ship 0.3.0 with
the residual documented; land this fix on its own arc.

---

## 1. Problem statement — why NEITHER own-thread oracle fires

### 1.1 The two exact oracles (own-thread free path)

`HeapCore::dealloc_own_thread_with_base` (`src/registry/heap_core.rs:783`) runs, on
every small own-thread free, exactly two M2 double-free oracles and touches no block
body:

1. **In-magazine scan** (`heap_core.rs:908-926`): a branchless chunked scan of
   `tcache.slots[c][0..cnt]`. Catches a block freed-but-not-yet-flushed (still queued
   in this class's magazine).
2. **BinTable `is_free` bitmap** (`heap_core.rs:951-953`): `meta.alloc_bitmap().is_free(off)`.
   Catches a block that was flushed back to a segment free list.

A live block is in neither → it is pushed to the magazine (`heap_core.rs:955-959`).

### 1.2 The third, transient resting place the oracles cannot see

A block P whose **cross-thread free is still in-flight** sits in its segment's
`RemoteFreeRing` (`src/alloc_core/remote_free_ring.rs`), pushed by a remote thread via
`dealloc_routing`'s Variant-2 push, and **not yet drained** by the owner. This state
sets NEITHER oracle:

- P is **not in `slots`** → the in-magazine scan (`heap_core.rs:908-926`) cannot see it.
- The bitmap **still reads "allocated"** → the ring push deliberately does not touch
  the bitmap; only the owner-side drain `AllocCore::reclaim_offset` → `mark_free`
  (`src/alloc_core/alloc_core.rs:718`, the `is_free` transition happens deep in that
  path) sets the free bit. So `is_free(off)` returns false (`heap_core.rs:951`).

So an OWN-thread free of P, concurrent-with / after a remote free of the same P that is
still queued, passes both oracles (`heap_core.rs:951` falls through) and P is pushed into
the magazine (`heap_core.rs:957`). P is now BOTH magazine-resident AND pending in the ring.

### 1.3 The corruption, once drained

A later owner drain — `find_segment_with_free` → `reclaim_offset` (`alloc_core.rs:1911`,
`alloc_core.rs:718`) — finds the stale ring entry for P. P is still carved
(`off < bump`, `is_free` still 0, magic/kind/align all pass, `alloc_core.rs:730-749`),
so `reclaim_offset` does `Node::write_next(P, old_head)` and links P onto the BinTable +
`dec_live`. If the magazine already re-issued P to the user, that `write_next` clobbers
P's live word0 and P is now double-issuable (magazine copy + BinTable free-list copy).
Under `alloc-decommit` it can even decommit+unmap a magazine-resident segment.

### 1.4 Two-thread timeline

```
 OWNER thread (heap H owns P's segment S)     REMOTE thread R
 ─────────────────────────────────────────    ───────────────────────────
 P = alloc()   (P live, bitmap=alloc)
 ... user hands P to R (or R frees it) ...
                                               free(P):  dealloc_routing
                                               owner_tf(S) != R  → ring.push(off(P),c)
                                               (bitmap of P UNTOUCHED, live unchanged)
 free(P)  [own-thread double-free]
   in-magazine scan(P):  MISS  (P not in slots)
   is_free(off(P)):      false (ring push left bitmap=alloc)
   → push P to tcache.slots[c]        ← BUG: P now in magazine AND in ring
 q = alloc() → pops P (LIFO)          ← P re-issued, user owns P again
 *(P) = user data
 ... refill miss → find_segment_with_free(c)
     → reclaim_offset(S, off(P))
       magic/kind/align/off<bump/is_free  ALL PASS (P still carved)
       write_next(P, old_head)          ← CLOBBERS P's live word0
       mark_free + dec_live             ← P now ALSO on BinTable free list
   ⇒ double-issue + freelist corruption
```

This is inherently a cross-thread × own-thread interleaving: the ring leg is remote, the
magazine leg is own. The `tests/regression_xthread_double_free_residual.rs` repro
collapses it to a deterministic single thread via `dbg_push_to_ring` /
`dbg_drain_all_rings` (`heap_core.rs:1428`, `1438`), asserting the CORRECT behaviour
(sentinel word0 survives, P never double-issued) — RED today, `#[ignore]`d.

---

## 2. The layering obstacle

The magazine (`Tcache`) lives in **`HeapCore`** (per-thread, has `&mut self.tcache`;
decision recorded in `FASTBIN_DESIGN.md:78`). The ring drain lives in **`AllocCore`**:
`AllocCore::find_segment_with_free` → `AllocCore::reclaim_offset`
(`alloc_core.rs:1911`, `718`). `AllocCore` is the pure segment substrate — it has **no
`&Tcache`, no per-thread caching policy, no visibility of the magazine at all**. So at
the exact moment `reclaim_offset` is about to `write_next` a drained block, it CANNOT ask
"is this block currently sitting in the owner's magazine?" — the layer that owns that
answer is one level up, and `find_segment_with_free` is called FROM `HeapCore::alloc`'s
refill (`heap_core.rs` refill path, ~line 617-654) with `self.core` borrowed mutably,
so the magazine is not even reachable through the current borrow.

That is the crux: **the drain and the magazine are mutually blind, and they sit on
opposite sides of the `HeapCore`/`AllocCore` boundary.** Any real fix must put ONE of
them in view of the OTHER (drain sees magazine, or magazine-push sees ring), and — per
the loom result below — must handle BOTH temporal legs.

---

## 3. Why the naive fix fails (both loom legs)

`tests/loom_magazine_ring_compose.rs` models the three resting bits
(`in_magazine`, `bitmap_free`, `ring`) with the invariant `!(in_magazine && bitmap_free)`.

- `compose_finds_double_issue_hole_today` (`:152`) — TODAY's rules (own-free blind to
  ring): loom finds `ring.store → own_free sets in_magazine → drain sets bitmap_free`
  → violation. `#[should_panic]` counterfactual proving the hole is real.
- `compose_naive_ring_check_still_holed` (`:201`) — the OBVIOUS fix "make own_free also
  read the ring" (`own_free(true)`, `:217`) STILL panics. loom finds the **symmetric
  leg**: own_free runs BEFORE the remote publishes (`ring` empty → own_free sets
  `in_magazine`), THEN remote publishes and owner drains (→ sets `bitmap_free`) → the
  SAME violation.

**Conclusion the design must honour:** a ring-read on the own-free path closes only the
leg where the ring entry is already visible when own_free runs. The other leg
(own-free-then-remote-publish-then-drain) needs the **drain to see the magazine** (or a
per-block conflict record consulted at drain time). The correct fix lives at the DRAIN,
not (only) at own_free.

---

## 4. Candidate evaluation

Kill criterion (absolute, from CLAUDE.md speed rules + the churn perf ledger in
`FASTBIN_DESIGN.md` §P6): **the fix must add ZERO cost to the alloc/dealloc magazine hot
path.** Conflicts are provably rare (only on a real user double-free), so all cost must
live on the cold drain / refill-miss path, never on the per-op push/pop.

### (a) Drain-with-magazine-visibility

Lift the ring-drain loop from `AllocCore` up to `HeapCore` where `&self.tcache` exists.
`AllocCore` exposes iteration over pending ring entries; `HeapCore` runs the drain, and
before each `reclaim_offset` it cross-checks the magazine `slots` scan for the unusual
case (bitmap says allocated AND the entry is a drain candidate AND the block is in
`slots[c]`). If the block is magazine-resident, the drain must NOT `reclaim_offset` it
(and must resolve the double-free: keep exactly one copy — see §5).

- **Correctness — both legs?** YES. The check happens at DRAIN time, so it fires
  regardless of whether own_free ran before or after the remote publish. When the drain
  processes P's stale entry it consults the live magazine; if P is in the magazine, the
  ring entry is a duplicate free and is dropped instead of linked. This is exactly the
  "drain sees the magazine" the loom companion demands.
- **Hot-path cost:** ZERO. The refactor moves an architectural boundary; the per-op
  push/pop and the common refill (miss with an empty ring) are untouched. The
  cross-check runs only per ring entry during a drain, which is already a cold path.
- **Blast radius:** LARGE. `find_segment_with_free` (`alloc_core.rs:1911`) is the
  central alloc-slow-path and is entangled with NUMA fallback, `alloc-decommit`
  recycle-during-scan (`alloc_core.rs:1929`, the unbounded-recycle #126 redo), and the
  single-writer field-specific reads (`alloc_core.rs:1958-1968`). Hoisting the drain
  means either duplicating that loop in `HeapCore` or threading a `&Tcache` (or a
  `FnMut` "is this block magazine-resident?" callback) down into `AllocCore` — the
  latter is the smaller, lower-risk shape and keeps `AllocCore` `unsafe`-free.
- **Interaction:** must preserve the RACE_DRAIN_RECLAIM class-packing fix
  (`alloc_core.rs:700-709`), the decommit bump-guard (`alloc_core.rs:751-759`), and
  A1 deferred-large (untouched — large bypasses the magazine). The magazine-resident
  check must run under owner-only access (it is — the drain is owner-side).

### (b) Per-heap epoch/bloom "recently pushed to magazine"

`reclaim_offset` skips (and counts) allocated-bitmap entries whose `off` hits a per-heap
bloom filter of "offsets recently pushed to the magazine"; false positives → deferred
reclaim on the next drain. The bloom is updated on every magazine push and cleared on
flush.

- **Correctness — both legs?** YES in principle (the filter is consulted at drain time,
  so both temporal legs are covered), BUT it is APPROXIMATE: false positives defer a
  legitimate reclaim (acceptable — retried next drain), and the filter must be sized so
  it never yields a false NEGATIVE for a genuinely-magazine-resident block within the
  window, which is delicate.
- **Hot-path cost:** **NON-ZERO — this VIOLATES the kill criterion.** A bloom update
  (~2-3 instructions: hash `off`, set bits) runs on EVERY magazine push
  (`heap_core.rs:957`) — the exact hot free path the whole magazine exists to keep bare
  (the design fought to remove even the word1 key write, `heap_core.rs:807-818`; re-adding
  a per-push write is a regression of that hard-won result). Plus a clear on flush.
- **Verdict:** REJECT on the kill criterion. It re-taxes the hot path to fix a
  rare-conflict-only problem.

### (d) Hybrid — drain returns a conflict list, `HeapCore` decides

The drain (still in `AllocCore`, minimally changed) collects the entries whose bitmap
reads "allocated" into a small **conflict list** and returns it up to `HeapCore`.
`HeapCore` cross-checks each against the magazine `slots` and decides per entry: reclaim
(genuine cross-thread free — mark_free + link) vs. skip (block is magazine-resident →
the ring entry is the duplicate free → drop it, keep the magazine copy). The normal case
(bitmap says free, no conflict) never produces a conflict-list entry and costs nothing.

- **Correctness — both legs?** YES — identical drain-time visibility to (a) (the
  decision is made in `HeapCore` with the magazine in hand), so both temporal legs are
  covered.
- **Hot-path cost:** NEAR-ZERO. Conflicts are produced only when the bitmap reads
  allocated for a pending ring entry — i.e. only on a real user double-free (or a stale
  entry that the existing bump-guard already handles). The push/pop and the normal
  refill-miss drain (all entries bitmap-free) are untouched. No per-push write (unlike b).
- **Blast radius:** SMALLER than (a). `AllocCore`'s drain loop keeps its structure; it
  gains a "don't `write_next`; instead append `off` to an out-param conflict list when
  the bitmap already reads allocated" branch. `HeapCore` gains a short post-drain loop
  that scans `slots[c]` per conflict (bounded by CAP=16 and by conflict count, which is
  ~0). `find_segment_with_free`'s NUMA / recycle / single-writer logic is otherwise
  intact.
- **Interaction:** the conflict list is owner-private stack storage (bounded — ring
  capacity is fixed), no allocation (M5-clean). Decommit bump-guard and RACE_DRAIN
  class-packing unchanged. Deferred-large / A1 unaffected.

### (c) Oracle-(3) bounded ring-peek on free — REJECTED (recorded)

Make own_free peek the segment ring on every free. **Rejected** for two independent
reasons: (i) **perf** — worst-case 256 atomic loads per free (ring capacity) kills churn,
directly violating the kill criterion; (ii) **correctness** — even ignoring cost, loom's
`compose_naive_ring_check_still_holed` (`:201`) proves the ring-read-on-own-free approach
is STILL holed on the symmetric (own-free-before-publish) leg. Do not revive.

---

## 5. Recommendation — (d) Hybrid conflict-list, drain-side resolution

**Pick (d).** Justification against the constraint "churn/cold iai Ir not worse":

- **(d) is the only candidate that is both correct on BOTH loom legs AND zero-cost on the
  hot path.** (a) is also correct-and-hot-path-free but has a larger blast radius through
  `find_segment_with_free`'s entangled NUMA/decommit/recycle logic; (b) taxes the hot
  path (killed); (c) is killed twice (perf + still-holed).
- The conflict is produced only on a real double-free, so the churn and cold-recycle
  microbenches (`bench_churn_alloc`, `bench_direct_alloc`) and the Linux `iai` Ir counts
  see NO new instruction on any path they exercise (they never double-free) — the added
  code is on a branch that is not taken in any perf workload.
- It keeps `AllocCore` `unsafe`-free and per-thread-policy-free (the magazine decision
  stays in `HeapCore`), honouring the layering intent of `FASTBIN_DESIGN.md:78-95`.

If (d)'s drain refactor proves awkward to thread the conflict list out of
`find_segment_with_free`, fall back to (a) with a `&Tcache` (or `is_in_magazine`
closure) passed down — same correctness, slightly larger surface. Both are acceptable;
(d) first.

---

## 6. Implementation plan (separate future arc)

**Functions to refactor**

1. `AllocCore::reclaim_offset` (`src/alloc_core/alloc_core.rs:718`) — split the "block is
   still carved (bitmap allocated) and passes guards" branch: instead of unconditionally
   `write_next`+`mark_free`, when the entry is a drain candidate whose bitmap reads
   allocated, append its `off` (+ class) to an out-param **conflict list** and do NOT
   link it. (The existing magic/kind/align/`off<bump` defence stays first.)
2. `AllocCore::find_segment_with_free` (`alloc_core.rs:1911`) and the internal drain
   helper — thread the conflict-list out-param through, preserving the
   recycle-during-scan and NUMA fallback logic untouched.
3. `HeapCore::alloc` refill path (`src/registry/heap_core.rs:~617-654`) and/or a new
   `HeapCore`-level drain wrapper — after the drain returns, for each conflict entry scan
   `self.tcache.slots[c]`: if found → the ring entry is a duplicate free of a
   magazine-resident block → **drop the ring entry, keep exactly the magazine copy**
   (M2 no-op honoured, no double-issue); if not found → it was a genuine still-live
   cross-thread free → perform the reclaim (link + mark_free + dec_live) that
   `reclaim_offset` deferred.
4. Wire the same resolution through the debug seams `HeapCore::dbg_drain_all_rings`
   (`heap_core.rs:1438`) so the regression test exercises the production decision.

**Making the pinning test go green WITHOUT inverting its counterfactual**

`tests/regression_xthread_double_free_residual.rs` already asserts the CORRECT
behaviour: (a) P's sentinel word0 survives the drain (`:147-153`) and (b) P is never
double-issued (`:166-171`). Under (d), step (5)'s `dbg_drain_all_rings` will find P's
ring entry, see the bitmap-allocated conflict, scan the magazine, find P resident, and
DROP the ring entry — so `write_next` never runs (sentinel survives) and P never lands
on the BinTable (no double-issue). Both assertions pass **unchanged**. Then remove the
`#[ignore]` (`:98`) and the `#[test]` runs in the default suite. Do NOT weaken or invert
either assertion — they are the target correctness, not the pinned bug.

**Making the loom model go green WITHOUT removing its counterfactual**

Per the file's own instructions (`loom_magazine_ring_compose.rs:60-63`, `:196-200`):
replace `compose_naive_ring_check_still_holed` with a GREEN invariant test whose `drain`
transition, when `ring` is nonempty AND `in_magazine` is set, does NOT set `bitmap_free`
(it drops the ring entry) — modelling (d)'s "drain sees the magazine" rule. The invariant
`!(in_magazine && bitmap_free)` then HOLDS on both legs (assert, no `#[should_panic]`).
Retire `compose_finds_double_issue_hole_today` (`:152`) as documenting the now-fixed
today-rules, OR keep it renamed as a historical counterfactual gated so CI stays green.
Register the green model in `scripts/loom.mjs`'s runner (the note at `:72-75` says to
wire it in only once #164 flips it green — do so then). The key point: the new model must
encode the DRAIN-side resolution, not an own_free ring-read, so it closes the symmetric
leg the companion test proved open.

**Perf gates that must hold**

- Churn wall (`benches/global_alloc.rs::bench_churn_alloc`, all sizes) — unchanged within
  noise (±0.3× ratio per `FASTBIN_DESIGN.md` P2+ methodology).
- Cold-recycle / bulk wall (`bench_direct_alloc`) — unchanged.
- **Linux `iai` Ir** on the recycle/alloc/free counters
  (`recycle_alloc_free_256x16b`, task #159) — byte-for-byte unchanged: the new code is on
  a branch taken only on a double-free, which no `iai` bench exercises.
- larson/mstress (`examples/malloc_macro`) — no regression (no per-op change).
- `--features production` full suite green; non-`fastbin` build unaffected (all new code
  `#[cfg]`-gated with the existing fastbin/xthread gates).
- Re-run TSan on `soak_xthread` + the loom green model; miri on the new drain-resolution
  unit test.

**Why (d) avoids the symmetric hole (restated).** The resolution is made at DRAIN time in
`HeapCore` with the magazine in hand — not on the own_free path. So it is indifferent to
whether own_free ran before or after the remote publish: whenever the drain processes P's
entry, it looks at the live magazine and, finding P there, refuses to link it. That is
precisely the "drain must also see the magazine" the loom companion
(`loom_magazine_ring_compose.rs:196-200`) says a correct fix requires.

---

## 7. Non-blocker restatement

This is NOT a 0.3.0 release blocker. The residual is honestly documented in code
(`heap_core.rs:848-885`) and design (`FASTBIN_DESIGN.md:241-257`), pinned
(`regression_xthread_double_free_residual.rs`, RED+`#[ignore]`) and modelled
(`loom_magazine_ring_compose.rs`). It pre-exists in the 0.2.x fastbin, is reachable only
via a genuine user cross-thread double-free (UB by contract for any allocator), and is
mirrored by the released-Large-segment residual note in `dealloc_routing`. Ship 0.3.0;
land this fix on its own arc.

---

## 8. Implementation postscript (2026-07-05)

### 8.1 Two errors found in the design (sections 4(d) and 6)

1. **"Conflicts are rare" is wrong.** Every genuine cross-thread free drains with
   bitmap reading "allocated" — that is the NORMAL state (the ring push deliberately
   leaves the bitmap untouched). Conflicts are not rare; they are universal. The cost
   claim survives only because drains are cold (refill-miss path) and iai benches never
   populate rings — so the per-entry predicate check adds zero Ir to every perf-gated
   bench.

2. **The pinned-test-goes-green claim (section 6) contradicts the test's own step 4.**
   The test does alloc(P) -> ring_push -> dealloc(P) (enters magazine) -> alloc() (pops P)
   -> drain. Section 6 says the drain "will find P resident" in the magazine — but P was
   popped at step 4 (LIFO). At drain time P is NOT in the magazine; it is a live user
   block. The magazine scan finds nothing; under (d)'s "if not found -> reclaim" rule the
   drain would `write_next(P, ...)`, clobbering live data. The test STAYS RED.

### 8.2 The impossibility: re-issue-before-drain

The re-issue-before-drain state is information-theoretically identical to a delayed
remote free of the current lifetime:

| signal            | re-issue-before-drain (UB)     | delayed genuine xfree (correct) |
|-------------------|--------------------------------|---------------------------------|
| `bitmap`          | "allocated"                    | "allocated"                     |
| `in_magazine`     | false (popped)                 | false (never pushed)            |
| ring entry        | `(off, class)`                 | `(off, class)`                  |

No distinguishing state exists without per-block generations.

`mark_free`-on-push + `mark_alloc`-on-pop ALSO fails this case: the pop restores
"allocated", making the drain state identical to a genuine free. It is strictly dominated
by the zero-cost drain-side check (which covers the in-magazine leg for free and leaves
the re-issue leg identically uncovered).

> **Scope note (added 2026-07-06, task R1 / retro C1).** The impossibility above
> covers ONLY the re-issue-before-drain leg — the block has already been popped
> from the magazine into the user's hands when the drain runs. The X-arc
> adversarial retrospective (§C1) found a SECOND, **decidable** leg that this
> theorem does NOT cover and that the X2 fix as originally shipped left open:
> the **refill-window in-out-buffer** leg. `refill_class_bump_impl` pulls
> freelist blocks into the caller-owned `out[0..filled]` buffer BEFORE draining
> rings; the predicate's `if k == c { return false; }` shortcut is blind to
> those magazine-destined blocks (they are not yet in `tcache.slots`). A stale
> ring note for such a block is information-theoretically DISTINGUISHABLE — the
> block IS visible, just in `out` rather than in the magazine — so no
> generations are needed. Task R1 closed this leg by wrapping the predicate
> with an out-membership guard (`is_in_magazine(ptr,k) || (k == c &&
> out[..filled].contains(ptr))`). The taxonomy is now THREE legs:
> 1. in-magazine-at-drain — closed by X2 (task #164);
> 2. **in-refill-out-buffer-at-drain — closed by R1 (task R1, this fix);**
> 3. re-issue-before-drain (block in user's hands) — the impossibility above,
>    **closed UNDER HARDENED by X7 (Ф1–Ф5, 2026-07-06); see §8.4 below.**

### 8.3 Implemented shape: section 5's fallback (a)-closure, narrowed

The implemented fix uses the section 5 fallback (a) shape: a `&dyn Fn(*mut u8, usize) ->
bool` predicate is threaded from `HeapCore` (where `&self.tcache` is accessible via split
borrows on disjoint fields `core` / `tcache`) down through `find_segment_with_free` /
`dbg_drain_all_rings` into `reclaim_offset_checked`. The predicate scans
`tcache.slots[class][0..count[class]]` (at most 16 compares, ~0 in practice).

The predicate is consulted AFTER all existing guards (magic / kind / align / off<bump /
is_free) and IMMEDIATELY BEFORE `write_next`. If the block IS magazine-resident, the ring
entry is dropped (return false without linking). If NOT resident, the link proceeds exactly
as before. All guard and link logic stays in one function (`reclaim_offset_checked`).

**Coverage:** the in-magazine leg (P still in `tcache.slots` when drain runs) is closed
on ALL production drain paths: (1) `refill_magazine_slow` via `refill_class_bump_checked`
and `find_segment_with_free_checked`; (2) `HeapCore::realloc` via `try_realloc_inplace`
miss routing through `HeapCore::alloc` (magazine-aware); (3) `dbg_drain_all_rings_checked`
(test seam). `AllocCore::alloc_small`'s unchecked `find_segment_with_free` is unreachable
from `HeapCore` under production features (fastbin routes small allocs through the
magazine; the `self.core.alloc` fallthrough is Large-only; realloc's slow path routes
through `HeapCore::alloc`). The re-issue-before-drain leg (P popped before drain) remains
a documented UB residual.

**Kill criterion:** ZERO new cost on the alloc/dealloc magazine hot path. The predicate is
invoked only per ring entry during a drain (cold, refill-miss path), on the branch where
bitmap reads "allocated". No per-push or per-pop write. iai Ir byte-identical.

### 8.4 The costed full fix (task X7, hardened-only) — IMPLEMENTED 2026-07-06 (Ф1–Ф5)

> **Status: IMPLEMENTED.** The X7 arc (Ф1–Ф5, task #188 umbrella) landed this
> fix under `--features hardened`. Commits: Ф1 `cdc3361` (gen table in segment
> metadata), Ф2 `345a2ce` (hardened ring-entry repack `[gen:8|class:6|off16:18]`),
> Ф3 `d1e91ff` (the three touches: bump-at-issue / stamp-at-remote-free /
> compare-at-drain + the success-criterion test), Ф4 `3b0ed2c` (lifecycle-seam
> tests: decommit-reset / recycle / adopt), Ф5 (this phase: hardened costs in
> the ledger, wrap-1/256 boundary test, docs sync, TSan/miri/loom final runs).
> The full phased account is in
> [`X7_GENERATIONAL_RING_PLAN.md`](X7_GENERATIONAL_RING_PLAN.md); this section
> is retained as the original costed sketch.
>
> **Residual taxonomy after X7:** leg 1 (in-magazine-at-drain) — closed by X2
> (#164); leg 2 (refill-window in-out-buffer) — closed by R1; leg 3 (re-issue-
> before-drain) — **closed UNDER HARDENED** by X7 (the stamp/compare guard drops
> a stale note whose generation no longer matches the block's current life). The
> only remaining leak is the **1/256 wrap**: a stale note whose stamped
> generation coincidentally equals the current generation modulo 256 (≥256
> re-issues without an intervening drain) is wrongly honoured — a probabilistic
> residual-of-the-residual, accepted by design (plan §2.5 rejected doubling the
> ring footprint for a `u64` note to close a leak that only fires under
> adversarial cross-thread-free timing on an already-UB program class). Pinned to
> its exact 256-modulus by `tests/regression_gen_wrap_boundary.rs` (Ф5). The
> production hot path is byte-for-byte untouched (every X7 code path is behind
> `#[cfg(feature = "hardened")]`; the Ф1–Ф4 production-judge gates confirmed
> 11/11 byte-identical Ir at every phase, and Ф5 re-confirms it as the closure
> gate). The hardened-tier cost is published in
> [`docs/perf/IAI_BASELINE.md`](../perf/IAI_BASELINE.md) ("Hardened-tier costs
> (X7)" section): +0.2–0.8% Ir marginal on the magazine hot path (the per-issue
> `bump_gen` RMW), +2.6% on refill-miss paths, plus a one-time ~262k Ir
> bootstrap per heap-claim (gen-table zeroing) — the published price of the
> defence-in-depth feature.

The ring `u32` entry currently packs `off:22 + class:10` (22 offset bits, 10 class bits).
Only 6 class bits are needed (`SMALL_CLASS_COUNT = 49 < 64`). This frees 4 bits. Combined
with a 4-bit reduction in offset precision (storing `off / 16` instead of `off`, valid
because block sizes are multiples of `MIN_BLOCK = 16`), 8 bits are available for a
per-block generation counter:

    off/16 : 18 bits  +  class : 6 bits  +  gen : 8 bits  =  32 bits

A per-block generation side-table (~1 byte per block; at `MIN_BLOCK = 16`,
`SEGMENT = 4 MiB`, that is `4 MiB / 16 = 256K` bytes = 6.25% per segment) bumps the
generation on every alloc. The ring push captures the current gen; the drain compares. A
mismatch (gen wrapped or advanced) means the block was re-allocated since the ring push —
the entry is stale and must be dropped. Wrap-around at 256 (`2^8`) gives a false-negative
window of 1/256 per block per ABA cycle — acceptable for a hardened-mode guard.

Per-alloc cost: one byte load + increment + store (~1-3 Ir). This is the unavoidable
minimum for per-block generation tracking. Gated behind the `hardened` feature so the
production hot path is unaffected.
