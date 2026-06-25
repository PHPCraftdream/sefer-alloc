# Open race: cross-thread-free drain-reclaim UAF (NOT yet eliminated)

**Status:** OPEN. Phase 12 ships a SOUND workaround (the bounded-leak *discard*),
not a fix. The underlying data race is still present and is the reason
`drain_thread_free` discards instead of reclaiming. Tracked by task **#33**
(per-segment generation-tag drain-guard). Decommit (M6/M11, task **#35**) shares
the same generation mechanism.

Related: `docs/FINDINGS_PHASE12.md` §8 (the falsification record).

---

## 1. What is and isn't true right now

- ✅ **No UAF / no corruption ships.** The committed allocator (`abe5610`) is
  sound: own-thread free reclaims normally; cross-thread free pushes to the
  per-slot atomic TFS; the owner *drains* the TFS but **DISCARDS** the drained
  blocks (`heap_core.rs::drain_thread_free` = `swap(null)` + drop the chain)
  rather than returning them to the `BinTable`.
- ❌ **The race is NOT eliminated.** Returning drained blocks to the `BinTable`
  (the "reclaim") is what races. We *avoid* the race by leaking, not by fixing
  it. RSS therefore grows under sustained cross-thread-free churn (bounded by
  the live cross-thread-freed footprint); the single-thread `Heap` path
  (`heap::thread_free`) is unaffected and fully reclaims.

## 2. The race (precise)

Unit of ownership is the **slot** (a `HeapSlot` in the global registry). A slot
holds a whole `HeapCore` (its segments + an inline `AtomicPtr` TFS head). The
slot's `state` token (FREE/LIVE) transfers ownership between threads on
thread-exit (`recycle`) and bind (`claim`). `owner_thread_free` in each segment
header points at the slot's **inline** TFS head — a stable address for the
process lifetime.

The contended object is the **block's first word** (its intrusive `next`
pointer), which is reused for three different roles over the block's life:
free-list `next` (in a `BinTable`), TFS `next` (while queued cross-thread), and
**user data** (while the block is live/allocated).

Interleaving that corrupts (observed as `STATUS_ACCESS_VIOLATION` in the
`global_allocator_cross_thread_free` MT test when reclaim is naively restored):

```
  thread C (cross-thread freer)         owner B (current holder of the slot)
  ----------------------------          -----------------------------------
  read hdr.owner_thread_free            ... allocating/freeing on this heap ...
    = &slot.inline_TFS
  (about to push block X to the TFS)
                                        drain TFS  (swap null) — does NOT see X yet
                                        pop X from BinTable (X was free)   ──┐
                                        return X to app; app writes X.first ─┘  X.first = user data
  push X to TFS:
    X.next = TFS head     ← OVERWRITES nothing dangerous (X already user data,
                            but C believes X is dead and links it)
                                        later: drain TFS → sees X
                                        read X.next (= whatever C/app left)
                                        treat X.next as a free-list pointer
                                        → out-of-segment deref → UAF / fault
```

The key point the naive "single-writer" argument missed: the owner is the sole
**`BinTable` writer**, but the **block's intrusive word** is contended between
the cross-thread pusher (C) and the owner's reuse (B) across the
release→claim / drain→reuse boundary. A block can be simultaneously "in flight
to the TFS" (per C) and "reused and live" (per B). Reclaiming such a block reads
user data as a `next` pointer.

(The `/oxx` hypothesis — "the 12.5 leak is a scar; just restore drain, no epoch
needed" — was TESTED and FALSIFIED by exactly this: naive restore segfaults; see
FINDINGS §8.)

## 3. Why discard is sound (the current shipped choice)

If the owner never *reads* a drained block's `next` and never *links* it into a
`BinTable`, the block's word is only ever written (by C's push, by the app) and
read by no one as a pointer. `swap(null)` still establishes the happens-before
that lets later pushers proceed. The block stays mapped and simply unused →
bounded leak, zero corruption.

## 4. The fix to build (task #33): per-segment generation-tag drain-guard

Give each segment (or each slot) a **generation** that bumps on the
release→claim boundary (and/or whenever the BinTable is reused in a way that
could alias an in-flight TFS entry). The cross-thread freer records the
generation it observed when it read `owner_thread_free`; the drain accepts a
block into the `BinTable` only if the segment's generation **still matches**.

- **Match** → no reuse happened across the freer's push; the block is genuinely
  dead and safe to reclaim → push to `BinTable`.
- **Mismatch** → a release→claim/reuse boundary was crossed while the freer's
  push was in flight; the block may have been reused → **skip** it (leak that one
  block) or re-route. Correctness over completeness.

Design questions to resolve during #33:
- **Where the generation lives.** Per-segment header field (`AtomicU32`) is the
  natural home; bump it on `recycle`/`claim` of the slot that owns the segment.
- **Where the freer records its observed generation.** Options: tag it into the
  TFS entry (pack into spare low/aligned bits of the pushed pointer, like the
  abandoned-segs tag), or a side word. Must not need an extra cross-thread write
  to the contended block word.
- **Granularity.** Per-slot generation may be coarser but cheaper than
  per-segment; evaluate.

This is the same generation/epoch family as the **M11 decommit guard (#35)** —
build the generation mechanism once and use it for both reclaim (#33) and
decommit-safety (#35).

## 5. Verification gate for #33 (non-vacuous)

- Restore `drain_thread_free` to return blocks to the `BinTable` **behind the
  generation-guard**, then run the committed `global_allocator_cross_thread_free`
  MT test — it MUST pass (no `STATUS_ACCESS_VIOLATION`) where the naive restore
  fails.
- A **closed-form cross-thread stress** test that does NOT hold any mutex across
  an alloc/free (the prior attempt deadlocked on exactly that lock-order
  hazard), bounded to terminate in seconds, no per-iteration thread spawn/join;
  assert checksum + bounded RSS over many repeats.
- `loom` counterfactual: the guard-less drain double-owns / reads a stale word →
  loom catches it; the guarded drain does not.
- After green: remove the discard, drop the "bounded leak under cross-thread
  churn" caveat from `docs/MALLOC_BENCH.md` and mark the FINDINGS remainder
  resolved.

## 6. Do NOT

- Do NOT re-attempt the naive `drain→BinTable` restore without the guard — it
  segfaults (proven).
- Do NOT hold a `Mutex`/`SpinLock` across an allocation or free in any stress
  test (lock-order deadlock with the fallback spinlock).

---

## 7. "If we have shards, where does the race come from?" (honest root-cause status)

A fair challenge: in a CLEAN shard model — one owner writes each `BinTable`, the
only cross-thread channel is the atomic TFS (loom-proven, Phase 10) — a
single-owner `drain→BinTable` MUST be sound. It segfaults, so **isolation is
violated somewhere**. This was NOT rigorously root-caused; §2's "intrusive-word
handoff" is a HYPOTHESIS, not a proven cause. **#33 must root-cause FIRST**
(minimal repro identifying the exact double-owner / double-free / stale-pointer
path) before choosing a fix — the fix may be far simpler than a generation-tag.

Leading suspects, in order:

1. **The fallback heap is a SHARED (non-sharded) heap.** It is process-global,
   used by multiple threads (serialized by a spinlock), with its own inline TFS;
   its segments are stamped `owner_thread_free = &FALLBACK.TFS`. It is the only
   heap that is NOT a per-thread shard. A block allocated from the fallback and
   freed cross-thread routes to the fallback TFS; the single-owner-at-a-time
   guarantee rests entirely on the spinlock — verify nothing drains/reuses a
   fallback block outside that lock, and that own-thread vs the global drain
   cannot interleave.
2. **Intrusive TFS word at slot reuse.** The original `ShardedRegion` remote-free
   queue stored freed *indices* (numbers) — it never reinterpreted an object's
   own bytes as a link. Our TFS is *intrusive* (the block's first word is the
   queue `next`). Across a slot release→claim, `owner_thread_free` is stable but
   the owner changed; a block can be reachable as "in-flight TFS entry" and
   "reused by the new owner" — the contended word.
3. **Plain implementation bug** in the restored `drain_thread_free` /
   `dealloc_small_by_segment` routing (e.g. a drained block pushed to the wrong
   segment's `BinTable`, or `free_list_contains` walking a list mutated by a
   path that should not touch it).

**Method for #33 (root-cause before fix):** instrument the restored drain to
record, for the faulting block, (a) which thread/slot allocated it, (b) which
freed it and via which path (own/cross/fallback), (c) the segment's owner at
drain time. Reproduce the `STATUS_ACCESS_VIOLATION` deterministically (small
thread count, `--test-threads=1` won't show it — needs real concurrency; use a
tight 2-thread producer/consumer with NO mutex held across alloc/free). Identify
which of (1)/(2)/(3) it is. THEN fix the actual cause — only fall back to the
generation-tag (§4) if the cause is genuinely the §2/(2) fundamental handoff
race, not a fixable isolation leak.

---

## 8. ROOT CAUSE CONFIRMED (instrumented repro) — suspect #2

Root-caused with a deterministic, instrumented repro (`tests/race_repro.rs`).
**Verdict: suspect #2 (intrusive TFS word at slot-reuse handoff) — CONFIRMED.
Suspects #1 (fallback) and #3 (routing bug) REFUTED.**

Captured trace (reclaim drain enabled):

```
[XPUSH tid=6 our_slot=3 block=0x..57a0 base=0x..400000 target_head=...078 (foreign)]
[DRAIN tid=2 slot=1 block=0x..57a0 next=0x0 base=0x..400000 our_head=...078]
[POP!! tid=2 seg=0x..400000 class=2] block 0x..57c0 next=0x6c64657463657078 OUTSIDE segment
[FLC!! ...] block 0x..57c0 on the free list has next=0x6c64657463657078 OUTSIDE segment
```

`0x6c64657463657078` = little-endian ASCII (`xpectdl`) — **user data written by the
app into a reused block, read back as a free-list `next` pointer**. The signature
of the UAF.

**Counterfactual (my own run + the agent's):**

| `drain_thread_free` | `tests/race_repro.rs` |
|---|---|
| **reclaim** (swap+walk+`dealloc_small_by_segment`) | **STATUS_ACCESS_VIOLATION** (deterministic) |
| **discard** (swap only — shipped) | **3/3 OK** |

`tests/race_repro.rs` is committed as the **counterfactual gate** for any future
fix: it passes under the shipped discard, and MUST keep passing under a future
guarded reclaim (and would segfault under an unguarded one). `heap_cross_thread`
(the single-heap path) is unaffected (3/3 OK) — the race is exclusively in the
registry cross-thread reclaim.

Why it is fundamental (not a missing lock): the block's first word is contended
between "in-flight TFS entry" (per the freer) and "reused by the new slot owner"
(per the owner), because the TFS head is a STABLE per-slot address (the 12.5
inline-`AtomicPtr` choice) — a freer's push and a *different* (post-reuse) owner's
drain meet on the same atomic, and the single-writer BinTable owner has no way to
know a drained block was already reused.

## 9. Fix direction (open decision)

- **Variant 1 — generation-tag (surgical, keeps intrusive TFS).** Caveat found
  while scoping: a 32-bit generation does NOT fit in a block pointer's spare
  bits (`MIN_BLOCK=16` → only 4 aligned low bits). So the observed generation
  cannot simply be packed into the pushed pointer; it needs side storage or a
  coarser scheme (e.g. a per-segment generation checked at drain against a
  generation the freer stamped into the *segment header* at push time — but two
  freers racing on one header field is itself contended). Needs careful design.
- **Variant 2 — non-intrusive TFS (architecturally cleanest; matches the
  original `ShardedRegion` 7b, which queued segment-relative OFFSETS, never
  reinterpreting object bytes).** Removes the contended word entirely. Cost: the
  queue node needs storage that is NOT the block's own first word — either a
  side array indexed per segment, or a small bounded ring, M5-clean (no
  `std::alloc` on the path).

Decision pending (this is the same class of architectural choice as the sharding
inversion). Variant 2 is the more faithful "fix the shards" — it restores the
original discipline (queues carry references/indices, never poison the object).
