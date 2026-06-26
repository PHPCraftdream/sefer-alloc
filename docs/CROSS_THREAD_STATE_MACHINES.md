# Cross-thread free as a system of state machines (the spec)

**Status:** design (task #37). Written *before* implementation, deliberately.
This document is the authoritative specification of the cross-thread-free
protocol. Implementation must match it; the loom model checks it; both the §8
intrusive-word race and the Phase-12.6 ring-ABA are shown below to be the **same
invariant violation**, so fixing the invariant fixes both.

Why this exists: Phases 8–12 drifted from the project's founding discipline
("dangerous memory → proven tools, don't improvise") into hand-rolled lock-free
machinery, because a `#[global_allocator]` cannot put a heap-allocating crate
(crossbeam-epoch, a concurrent queue) on its own path (reentrancy). When you
*must* hand-roll, the proven substitute is not "a cleverer data structure" —
it is **a written state machine with invariants you can model-check**. That is
what this is.

---

## 0. The actors and what they may touch

| Actor | Identity | May write |
|---|---|---|
| **Owner** | the one thread bound to a heap slot, for one slot-lifetime | that heap's segments' `BinTable`s; collects their channels |
| **Remote** | any other thread freeing a block it holds | only an atomic *publish* into the block's segment channel |
| **Adopter** | a thread claiming an `ABANDONED` segment | the segment's `owner_state` via CAS, then becomes Owner |

The single discipline: **the Owner is the sole mutator of free-list state; the
only cross-thread write is a Remote's atomic publish into a channel.** Everything
below makes that precise and says what happens at the boundaries where the Owner
identity changes.

---

## 1. SM-BLOCK — the allocation atom

Scope: one `(segment, offset)` *within a single segment incarnation* (see
SM-SEGMENT for "incarnation"). This is the machine whose invariant the bugs broke.

States:
- `UNCARVED` — the bump cursor has not reached this offset; not yet a block.
- `LIVE` — handed to the application.
- `LOCAL_FREE` — on the Owner's `BinTable` free list (free; Owner-only reachable).
- `REMOTE_FREED` — freed by a Remote; published into the segment channel; **not
  yet collected; NOT allocatable.**

Transitions (and the sole actor permitted to drive each):
```
UNCARVED   --carve (Owner)----------------> LIVE
LIVE       --dealloc by owner (Owner)-----> LOCAL_FREE
LIVE       --dealloc by non-owner (Remote)-> REMOTE_FREED   // the only x-thread edge
REMOTE_FREED --collect (Owner)-----------> LOCAL_FREE
LOCAL_FREE --alloc (Owner)----------------> LIVE
```

Invariants:
- **I-BLOCK-1 (mutual exclusion).** A block is in *exactly one* state at any
  instant. The crash signature (a free-list node holding live app data, `next`
  outside the segment) is a witnessed `LIVE ∧ LOCAL_FREE`.
- **I-BLOCK-2 (no resurrection).** For each block-life, its `REMOTE_FREED →
  LOCAL_FREE` (collect) must *happen-before* the next `LOCAL_FREE → LIVE`
  (alloc) of the same address. Equivalently: **a remote free queued for life N
  must be collected within the owner-context of life N — never applied to a
  later incarnation that reused the address.**
- **I-BLOCK-3 (single free per life).** Each life takes exactly one `LIVE →
  {LOCAL_FREE | REMOTE_FREED}` edge (no double free).

---

## 2. SM-SEGMENT — ownership / incarnation

> **Hot-path reality (verified, task #37).** In the shipped Phase-12.5 shard
> model, `abandon_segments` is **not called on the hot path** — thread exit does
> `recycle` only and leaves the `HeapCore` whole (whole-heap reuse;
> `heap_registry.rs:234`). So on the hot path a segment is **continuously
> `OWNED`** by its slot; only the bound *thread* changes on recycle. The
> `OWNED→ABANDONED→OWNED(g+1)` edges below are the **adoption substrate**
> (loom-proven, retained for a future decommit-when-empty policy), NOT the
> stress-path. **The observed corruption is therefore a within-continuous-
> ownership violation** (Owner drain/reclaim/reuse racing a Remote publish),
> which is what the loom model in §6 targets first; the incarnation edges are
> modelled second, for the decommit policy.

Scope: one segment base. `owner_state` packs `(state, owner_id, generation)`.

States:
- `UNINIT`
- `OWNED(H, g)` — heap `H` is the sole `BinTable` writer and sole channel
  collector. `g` is the **incarnation number** (bumped on each adoption).
- `ABANDONED(g)` — the prior Owner released the slot/exited; **no live owner**,
  yet blocks may still be `LIVE` in app hands and Remotes may still publish.

Transitions:
```
UNINIT      --first alloc (Owner)----------> OWNED(H, g)
OWNED(H,g)  --abandon on exit (Owner)------> ABANDONED(g)
ABANDONED(g)--adopt, CAS (Adopter)---------> OWNED(H', g+1)
```

Invariants:
- **I-SEG-1 (single owner).** At most one `OWNED` owner; owner = sole free-list
  writer + sole collector.
- **I-SEG-2 (collector continuity / the boundary rule).** A segment's channel
  may be collected only by its *current* `OWNED` incarnation. Across an
  `OWNED → ABANDONED → OWNED(g+1)` boundary, a publish made for incarnation `g`
  must NOT be collected into incarnation `g+1`'s free list. This is I-BLOCK-2
  lifted to the segment.

---

## 3. SM-SLOT — registry HeapSlot

Scope: one registry slot index. `HeapCore` is materialised on first claim and
**inherited as-is** on later claims (see `HeapRegistry::claim`).

States: `FREE --claim(gen+1)--> LIVE(gen) --recycle--> FREE`.

Invariant:
- **I-SLOT-1.** The inherited `HeapCore`'s segment table must not let a new
  slot-lifetime collect channel entries that belong to a prior lifetime's
  blocks. (This is the concrete path by which I-BLOCK-2 was violated: producer
  threads exit, slots recycle, the inheritor drains channels of segments whose
  blocks were handed out in the previous lifetime.)

---

## 4. SM-CHANNEL — the cross-thread handoff (per segment)

This is where representation is usually argued (intrusive word vs offset ring).
The state machine shows the argument is **secondary** — what matters is that a
channel entry is *bound to a block-life*, so it can never be applied to a
different life.

Abstract entry lifecycle: `EMPTY → PUBLISHED(block, life-epoch) → COLLECTED`.

- **Intrusive (mimalloc) representation:** the block *is* the node; "in channel"
  ⟺ `REMOTE_FREED`. A block in the channel is not allocatable; collect swaps the
  whole list to the Owner's `local_free`. I-BLOCK-2 holds *by construction* —
  you cannot alloc what is in the channel, and collect is the only path to
  `LOCAL_FREE`. **Provided** the boundary rule (I-SEG-2) seals/quiesces the
  channel at abandon so a post-boundary owner never reads a pre-boundary node.
- **Non-intrusive (our ring) representation:** the entry is an offset, *decoupled
  from the block's life*. An offset published in life N can be drained in life
  N+1 → I-BLOCK-2 violated. To restore the binding the entry must carry the
  **life-epoch** (the segment generation captured at the block's *alloc*),
  checked at collect against the segment's current generation.

### The unification (why this whole exercise pays off)

Both historical failures are **one** violation:

- **§8 intrusive-word race:** at slot reuse, the block's first word is contended
  between "Remote writing the channel-next (REMOTE_FREED for life N)" and "Owner
  reusing it (LIVE/LOCAL_FREE for life N+1)" → `LIVE ∧ REMOTE_FREED` =
  **I-BLOCK-1/2 broken at the boundary.**
- **Phase-12.6 ring-ABA:** an offset published in life N is collected in life
  N+1 after the block was re-carved → `LIVE ∧ LOCAL_FREE` =
  **I-BLOCK-1/2 broken at the boundary.**

Same invariant, same boundary. So the fix is **not** "intrusive vs ring"; it is
**enforcing I-BLOCK-2 / I-SEG-2 at the OWNED↔ABANDONED↔OWNED boundary.**

---

## 5. The boundary discipline (the actual fix surface)

Exactly one of these must hold; both are valid, pick by cost:

- **(Q) Quiesce at abandon.** `OWNED → ABANDONED` may occur only after the Owner
  has drained its channel AND no block of this segment is still `LIVE` in app
  hands. If live blocks remain, the segment is not abandoned (it is retained/
  leaked-bounded until quiescent), so no Remote free can target a re-incarnated
  address. Simple; cost: retains segments with stragglers.
- **(E) Life-epoch on the entry.** Every channel publish carries the segment
  generation captured at the block's *alloc*. Collect applies an entry only if
  its epoch == the segment's current generation; otherwise the entry is a stale
  cross-incarnation free and is dropped (sound; bounded leak only at the
  boundary). Cost: wider entry (offset+gen) / a per-block alloc-epoch stamp.

Either makes I-BLOCK-2 hold. (E) keeps full reclaim within a lifetime; (Q)
trades some retention for a simpler channel. **Recommendation: model both in
loom, ship the one whose loom model is smaller and whose leak bound is
acceptable.** The decision is now a measured one, not a guess.

---

## 6. Verification plan (verification-first)

1. Encode SM-BLOCK + SM-SEGMENT + SM-CHANNEL as a loom model over loom atomics
   (NOT the real allocator): a small number of blocks, 1 Owner that
   alloc/free/collect/abandons, 1 Adopter, ≥1 Remote that frees across the
   boundary. Assert I-BLOCK-1/2/3 and I-SEG-1/2 on every interleaving
   (`preemption_bound = 3`).
2. **Counterfactuals (non-vacuity):** the model WITHOUT the boundary discipline
   (no quiesce / no epoch check) must make loom find the `LIVE ∧ LOCAL_FREE`
   interleaving (`#[should_panic]`). This reproduces the §8/ABA bug *in the
   model*, proving the model has teeth and the discipline is what removes it.
3. Only then implement to match, and re-run `tests/race_repro.rs` (×5 under
   reclaim) + `tests/remote_ring_unit.rs` + the full gate.

---

## 7. What this replaces

- The ad-hoc Variant-2 ring stays *only* as the channel representation **if** it
  carries the life-epoch (option E); otherwise it is replaced by the intrusive
  channel under the boundary rule (option Q). Either way the `generation-tag`
  idea is no longer a "crutch bolted on" — it is `I-SEG-2` made executable, with
  a loom proof.
- The transient subtract-overflow guards / diagnostic probes are deleted (they
  masked I-BLOCK-1 instead of preventing it).

---

## 8. Open questions for the implementation phase

- (E) needs the block's alloc-epoch at *collect* time. Where is it stored — in
  the channel entry (widen to u64 `offset|gen`), or read from the segment
  generation and compared to a per-block stamp? The loom model decides.
- (Q) needs "no block of this segment still LIVE" — a per-segment live-count, or
  the existing bump/free accounting. Cheap to add; confirm it is M5-clean.
- Interaction with M11 epoch-guard (#35) for M6 decommit: the same segment
  generation should serve both (decommit-safety and collect-safety are the same
  "don't touch a re-incarnated address" property). Unify, don't duplicate.
