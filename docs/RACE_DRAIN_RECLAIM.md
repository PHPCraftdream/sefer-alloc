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
