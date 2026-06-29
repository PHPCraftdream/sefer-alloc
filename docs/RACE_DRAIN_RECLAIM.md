# Cross-thread-free drain-reclaim UAF — RESOLVED

**Status:** RESOLVED (task #33/#36) — see §14 for the shipped fix and its
two-platform verification. The TRUE root cause is in §13 (the `page_map` class is
wrong for mixed-class pages; the cross-thread reclaim must carry the class from
the freer's `Layout`, not derive it from `page_map`). §1–§12 are the
investigation record (several earlier "root causes" were layers peeled off by
zero-trust verification — intrusive-word §8 → ring-ABA → header-race §11 → the
true class-derivation bug §13). Reclaim now works (no more discard-leak).

The history below (OPEN-era text) is retained as the diagnostic trail.

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
  churn" caveat from `docs/ALLOC_BENCH.md` and mark the FINDINGS remainder
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

## 10. Disentangle verdict (task #36) — ABA confirmed, ring exonerated

The `/oxx` "truth before beauty" protocol was executed: **isolate the ring
first, then read the surviving crash** — do not bolt a generation-tag onto an
undiagnosed bug.

### Evidence

1. **The ring is clean — proven in isolation.** `tests/remote_ring_unit.rs`
   drives `RemoteFreeRing` over a plain `Box<[u8]>` (NO allocator, NO segment,
   NO recycled offsets): 4 producers push 8 000 UNIQUE offsets through `CAP=256`
   (multi-wrap), 1 consumer drains. Asserts exactly-once + exact overflow
   accounting + `reclaimed + overflow == attempted`. **GREEN.** The drain fix
   `while h < t` → `while h != t` (wrap-correct cursors) is in place. So the
   ring neither loses nor duplicates an offset *as a data structure*.

2. **The crash survives the clean ring.** Re-enabling reclaim (via the
   `find_segment_with_free` per-segment ring drain → `reclaim_offset`),
   `tests/race_repro.rs::drain_reclaim_uaf_repro_long_lived_consumer` still
   corrupts: a free-list node's first word is not a valid `next` pointer
   (`attempt to subtract with overflow` in the free-list walk; `next` outside
   the segment).

3. **The instrumented trace names the cause.** A reclaim-time probe printing
   `(thread, off, first_word, seg_owner_id)` on the anomalous add shows, e.g.:
   `off=16864 first_word=0x7578 seg_owner_id=5 class=0` — and the first word is
   an **incrementing application counter** (`0x7578, 0x759a, 0x75bc, …`), i.e.
   LIVE user data. The block is simultaneously on the free list AND in the app's
   hands.

### What each hypothesis turned out to be

| Hypothesis | Verdict | Why |
|---|---|---|
| Ring bug (`h<t` / missing clear / missing break) | **REFUTED** | isolated ring GREEN, exactly-once over 8 000 offsets |
| Dual-ownership (two threads on one segment) | **REFUTED** | trace: reclaimer `ThreadId` ≡ segment `owner_id` always |
| Stale aliased `BinTable` view | **REFUTED** | `BinTable` is a write-through view (`head`/`set_head` hit segment memory; no cache) |
| **ABA across block lifetimes via slot-recycle** | **CONFIRMED** | reclaim adds a block whose first word is live, incrementing app data |

### The setting: whole-heap-reuse on slot recycle

`drain_reclaim_uaf_repro_long_lived_consumer` spawns **producer threads that
exit each wave** (their heap slots recycle) while one **long-lived consumer**
frees their boxes cross-thread (→ the producer segment's `RemoteFreeRing`).
`HeapRegistry::claim` **inherits a recycled slot's `HeapCore` as-is** (the core
is materialised only on first claim, `new_gen == 1`; later claims "reuse the
already-live HeapCore").

> **CORRECTION (self-review, task #37).** An earlier draft of this section
> attributed the resurrection to thread exit `abandon`ing segments while leaving
> them in the inherited table, so a recycled slot's new owner drains rings of a
> *previous* incarnation. **That mechanism is refuted by the code:**
> `HeapRegistry::abandon_segments` is **NOT called on the hot path** — the
> Phase-12.5 shard model has `AbandonGuard` call `recycle` only, leaving the
> `HeapCore` whole (`heap_registry.rs:234`). So segments are never pushed to the
> abandoned stack on the hot path and there is no `OWNED→ABANDONED→OWNED`
> incarnation change during the stress test; ownership of a segment is
> **continuous** (the slot owns it; only the bound *thread* changes on recycle).
> The corruption therefore occurs **within a single continuous ownership**, on
> the concurrency seam between a Remote's channel publish and the Owner's
> drain/reclaim/reuse — NOT across an incarnation boundary. The `ThreadId ≡
> owner_id` trace is consistent with this (each event is the current owner of
> its own slot). The exact interleaving has repeatedly defeated static
> reasoning; that is the signal to **model-check it with loom against an explicit
> state-machine spec** (`docs/CROSS_THREAD_STATE_MACHINES.md`, task #37) rather
> than hand-argue or hand-patch it further. A concrete suspect surfaced while
> grounding the spec: `dealloc_routing` reads `hdr.owner_thread_free`
> **non-atomically** (`SegmentHeader::read_at`) while `stamp_segment_owner`
> writes it non-atomically — a Remote may observe a stale value and mis-route a
> free. loom is to confirm or refute this.

The recurring Phase-12 theme remains **whole-heap reuse on slot recycle** as the
*setting*, the one deviation from `ShardedRegion` — but the failing transition is
within continuous ownership, not across incarnations.

### Consequence for the fix

The ring is the wrong place to add machinery — it is already correct in
isolation. The remaining work is to **specify the protocol as a system of state
machines and model-check it with loom** (`docs/CROSS_THREAD_STATE_MACHINES.md`),
because the within-ownership interleaving that violates I-BLOCK-1 has defeated
repeated static reasoning. The loom model must reproduce the
`LIVE ∧ LOCAL_FREE` violation *without* the fix (non-vacuity), then the fix is
whatever makes the model green — candidates: an atomic `owner_thread_free`
handoff (if the mis-route suspect holds), and/or the boundary discipline of the
spec. The earlier "per-block epoch / generation-tag" recommendation is **held**
pending the loom verdict — it is not adopted blind.

## 11. ROOT CAUSE (task #37, research complete) — data race on `SegmentHeader`

Research isolated every component and proved each correct **alone**:

| Component | Test | Verdict |
|---|---|---|
| Ring (data structure) | `tests/remote_ring_unit.rs` | clean (8000 offs, exactly-once) |
| `reclaim_offset` (owner logic) | `tests/reclaim_offset_unit.rs` | clean (50×200 single-threaded) |
| Single-shard protocol | `tests/loom_xthread_protocol.rs` | loom-green + non-vacuous counterfactual |

So the bug is **not logic** — it is **concurrency**, and the non-deterministic
manifestation (panic at alloc_core.rs:516 *or* :261, or `STATUS_ACCESS_VIOLATION`,
varying run to run) is the signature of **undefined behaviour from a data race**.

**The race:** `SegmentHeader` packs an owner-mutated field (`bump`, rewritten on
every `carve_block` via `write_header` — a full-struct read-modify-write) in the
same struct as the cross-thread-read fields (`magic`, `kind`,
`owner_thread_free`). The Remote's `dealloc_routing` reads the **whole** header
non-atomically (`SegmentHeader::read_at` = `Node::read_struct::<SegmentHeader>`)
to get `owner_thread_free`/`kind`/`magic`. The happens-before from the mpsc
`send`/`recv` only covers the owner's writes **up to the send** of the freed
block; the owner keeps carving **after** that send, and each later carve rewrites
the header concurrently with the Remote's read. Concurrent non-atomic
read+write of the same memory = **data race = UB**.

Why every prior analysis hit a wall: the *logic* (single-owner free list, clean
ring, correct reclaim) is genuinely correct; the corruption is injected by UB,
which no amount of logical reasoning about the protocol can predict.

### Fix (delegated to implementation)

Decouple the cross-thread-read fields from the owner-mutated `bump`:

1. **`carve_block` must update only `bump`**, not rewrite the whole header
   (`write_header` of the full struct). `bump` is owner-only (no Remote reads
   it), so a field-specific owner write is race-free.
2. **`dealloc_routing` must read only `magic` / `kind` / `owner_thread_free`**
   via field-specific accessors (cf. the existing `kind_at`), never the whole
   mutable header. Those fields are written once at segment init and then only
   read, so field reads do not race.

Gate: `race_repro` ×5 green + `remote_ring_unit` + `reclaim_offset_unit` +
`loom_xthread_protocol` + full config matrix + clippy. Audit for any OTHER
cross-thread `read_at`/`write_header` overlap.

## 12. CONTROL EXPERIMENT (task #33) — recycle REFUTED as the cause

`tests/race_norecycle.rs` runs the same cross-thread-free reclaim stress but with
**long-lived producer threads** (spawned once, never exit mid-test → slots are
NEVER recycled). It **STILL crashes** (`alloc_core.rs:653`, subtract overflow in
the `free_list_contains` guard — a free-list node's `next` is outside its
segment).

This **refutes** three prior hypotheses:
- crush's "ABA via re-carve across slot-recycle" (the #33 fix-run diagnosis),
- §10's "slot-recycle ABA",
- the entire boundary-discipline direction (`CROSS_THREAD_STATE_MACHINES.md` §5
  Q/E) as the fix for THIS crash.

**The bug is steady-state**: stable producer (owner) + stable consumer
(cross-freer), no lifetime boundary. This is exactly the scenario the loom model
`tests/loom_xthread_protocol.rs::protocol_single_owner_never_resurrects` proves
correct. Therefore the **implementation deviates from the proven protocol** in a
concrete concurrent detail that NO isolated test covered:
- `remote_ring_unit` exercised ring push/drain concurrency but did NOT reclaim
  into a real `BinTable` (the consumer just counted offsets);
- `reclaim_offset_unit` exercised reclaim into a real `BinTable` but
  single-threaded;
- the loom model is an abstraction (1-slot channel, 1-block free list).

The untested seam: **concurrent ring-push (consumer) while the owner
drains→reclaim_offset→writes BinTable AND allocates (pop/carve)**, with real
offset reuse across alloc/free cycles. The header data race (§11) was real UB and
is fixed, but fixing it did not change the symptom — so it was not (the sole)
cause of THIS crash.

### Status of the §11 header-race fix

KEPT (it removes genuine UB): `carve_block` now writes only `bump` via
`SegmentMeta::bump_of`/`set_bump`; `dealloc_routing` reads only
`magic_at`/`owner_thread_free_at` (disjoint from `bump`). Sound improvement,
independent of this crash.

### Next (research-directed)

A race detector (TSan, Linux/WSL — unavailable on this Windows host) is the
proven tool for the remaining heisenbug. Absent that: a minimal stress that
reproduces with ONE owner + ONE remote on ONE segment, bisecting the concurrent
push/drain/reclaim/alloc seam; or ship the sound discard (no UAF, bounded leak)
as the Phase-12 MT floor and defer reclaim behind a CI race-detector gate.

## 13. TRUE ROOT CAUSE (task #33) — page_map class is wrong for mixed-class pages

Found by ThreadSanitizer (which proved there is NO data race in our code — only
a harness `Arc` refcount artifact) plus an in-process free-list audit on a
RELIABLE LINUX REPRO (`tests/race_norecycle.rs` crashes on Linux too — NOT
Windows-specific; the os-seam mmap/VirtualAlloc difference is irrelevant).

The audit pinned two smoking guns, both in `reclaim_offset` (called from
`drain_thread_free` → `RemoteFreeRing::drain`):

```
FREELIST-CORRUPT after reclaim-add: class=1 node_off=42976 next=0xa0b9 (= small data, outside segment)
BAD-OFFSET: reclaim off=43232 NOT aligned to block_size=768 (class=13)
```

`43232` is a multiple of 16 (class 1) but NOT of 768 (class 13). So the block is
class 1, yet `page_map.class_of(43232/PAGE)` returned class 13. **`page_map` gives
the wrong class.**

Why: a segment has ONE bump cursor shared by ALL size classes (`carve_block`
advances the segment header's single `bump`). Consecutive carves of different
classes are therefore adjacent in memory and **share pages**. The
page-dedication rule (`carve` sets a page's class only `if class_of().is_none()`)
records only the FIRST class to touch a page; later blocks of OTHER classes in
the same page are mis-attributed. Pages are **mixed-class**, so `page_map` is
unreliable as a class oracle.

This never mattered before Phase 12 because the **own-thread** free path derives
the class from the caller's `Layout` (`AllocCore::dealloc` → `classify(layout)`)
— always correct. Only the **cross-thread** reclaim (`reclaim_offset`,
`dealloc_small_by_segment`) has no `Layout` and falls back to `page_map` → wrong
class → wrong `block_size` → it links the free-list `next` at a mis-aligned
address, corrupting a neighbouring block (whose first word, read later as a
`next`, points outside the segment → the `subtract with overflow` / UAF).

Consistent with everything: TSan-clean (it is a logic bug, not a race);
single-thread `reclaim_offset_unit` GREEN (it used one class only → no page
mixing); reproduces on Linux and Windows; non-deterministic crash site (the
corrupt node is tripped by whichever later walk reaches it first).

### Fix

The cross-thread freer **has the `Layout`** (`HeapCore::dealloc(ptr, layout)` →
`dealloc_routing(ptr, layout)`). Carry the class to the owner instead of making
the owner guess from `page_map`:

- pack the class into the ring entry: `u32 = offset | (class_idx << 22)`
  (`offset < SEGMENT = 2^22`; `class_idx < SMALL_CLASS_COUNT ≪ 2^10`);
- `dealloc_routing` computes `class_idx = classify(layout)` and pushes the packed
  value;
- `reclaim_offset` unpacks the class and uses it directly — NEVER `page_map` for
  the class. (Keep a sanity check that `offset` is `block_size`-aligned.)
- audit `dealloc_small_by_segment` for the same `page_map`-class reliance.

(A deeper, Phase-13 option is true per-class page dedication — a separate bump
per class / mimalloc-style pages — but carrying the class is the minimal correct
fix and uses information the freer already holds.)

## 14. RESOLVED (task #33/#36) — fix shipped, verified on Windows + Linux

Two changes eliminated the cross-thread-free reclaim corruption:

1. **Carry the size class through the ring (the §13 root fix).** The cross-thread
   freer has the `Layout`, so `dealloc_routing` packs `class_idx` into the ring
   entry (`u32 = offset | class_idx << 22`, `pack_entry`/`unpack_entry` in
   `remote_free_ring`); `reclaim_offset` unpacks and uses that class instead of
   the unreliable `page_map` (whose per-page class is wrong for the mixed-class
   pages a shared bump cursor produces). `reclaim_offset` keeps a `block_size`
   alignment sanity check.

2. **Removed the eager per-alloc `drain_thread_free`.** Reclaim is now LAZY,
   solely inside `find_segment_with_free` (the alloc-slow-path drains each owned
   segment's ring → `reclaim_offset`) — the original `ShardedRegion` 7b
   discipline. The eager every-alloc drain was a redundant deviation; under the
   installed `#[global_allocator]` serving libtest's own cross-thread frees it
   corrupted the free list (a single-thread-churn regression), while the lazy
   path handles the *identical* workload correctly. Reclaim completeness is
   preserved (the owner drains a segment's ring the moment it needs a free block
   from it; until then frees sit in the bounded ring → bounded leak).

### Verification (non-vacuous: every gate test was RED before, GREEN after)

- `race_repro` ×5 (Windows) + ×N (Linux nightly) — green (was a non-deterministic
  `STATUS_ACCESS_VIOLATION` / subtract-overflow).
- `race_norecycle` (the reliable Linux repro that crashed every run) — green.
- `remote_ring_unit`, `reclaim_offset_unit`, `loom_xthread_protocol` (+
  counterfactual), `loom_remote_ring` (+ counterfactuals) — green.
- Full Windows suite (differential, invariants, reentrancy, concurrent_stress,
  global_alloc, global_alloc_installed, compaction, …) — 0 failed.
- `clippy` 0 warnings; feature matrix builds clean.
- ThreadSanitizer (Linux) — no data race in allocator code (the bug was a
  class-derivation logic error, not a race), confirming the diagnosis.

The cross-thread-free reclaim is sound and reclaims (no more discard-leak).
