# sefer-region: `DenseRegion<T>` and `ShardedSyncRegion<T>` — design notes

Status: **design note only — not implemented.** Closes tasks #799 and #800
(perf design notes flagged in
`docs/reviews/2026-08-09-sefer-region-static-release-audit.md`, "what can
be sped up" items 1 and 3). Both were blocked on task #802 (the F2
domain-aware `Handle<T>` redesign); #802 landed in commit `9741388`
(`region_id: NonZeroU64` on both `Region<T>` and `Handle<T>`), so both
design notes below are written against that final shape.

Neither design in this document has a follow-up implementation task filed
yet. Landing either requires an explicit maintainer decision (this note's
job is to make that decision cheap, not to make it for the maintainer).

## 1. `DenseRegion<T>` — for holey-iteration workloads

### Motivation

`Region<T>` iterates over `slotmap::SlotMap`'s slot array, which is NOT
compacted after removals — tombstone holes remain until the slot is
reused by a later `insert`. The static-release-audit's own measurement:
sweeping 1000 live values in a `SlotMap` that has accumulated 90%
tombstone holes costs ~11.482 µs, versus ~1.319 µs with zero holes (a
~8.7x cliff). This is a **ceiling on possible improvement**, not proof
that a new type automatically achieves it — the real win depends on how
much of that ~11.482 µs is holes-skipped-per-iteration versus other
per-call overhead, which this note does not re-measure.

### Design shape

A **separate** `DenseRegion<T>` type over `slotmap::DenseSlotMap`, NOT a
change to `Region<T>`'s own backing store. Reasons this must be additive,
not a swap:

- `DenseSlotMap` has a real, already-documented cost on the OTHER two
  axes this crate optimizes for: this crate's own docs (see
  `Region`'s module doc, "the lookup/churn axis it was benchmarked to
  win") note `SlotMap` was chosen specifically because lookup is faster
  and churn (repeated insert/remove) is cheaper than `DenseSlotMap`'s
  compacting behavior. Swapping the default backing would trade a
  ~8.7x-ceiling win on one axis (hole-heavy iteration) for a real
  regression on the two axes `SlotMap` was picked for in the first
  place — churn-heavy or lookup-heavy callers would get strictly worse
  service by default.
- `DenseRegion<T>` is therefore an **opt-in alternative** for
  iteration-heavy, hole-heavy workloads specifically — not a
  replacement for `Region<T>`.

### Handle identity, post-F2

`DenseRegion<T>` needs its own `Handle`-shaped type (or can reuse
`Handle<T>` verbatim if `slotmap::DenseSlotMap`'s key type is also
`DefaultKey` — needs verification against the installed `slotmap`
version before implementation, not assumed here). Either way, the F2
redesign's `region_id: NonZeroU64` pattern applies unchanged: a
`DenseRegion<T>`-minted handle must carry its own instance id and be
rejected by any other `Region<T>`/`DenseRegion<T>` instance, including
one of the other type. If `DenseRegion<T>` reuses `Handle<T>` directly,
its `region_id` counter must draw from the SAME `NEXT_REGION_ID` static
in `region.rs` (not a second independent counter) — two independent
`AtomicU64` counters both starting at 1 would let a `Region<T>` and a
`DenseRegion<T>` mint colliding `(key, region_id)` pairs, defeating I6
across the type boundary. If `DenseRegion<T>` instead gets its own
handle type (e.g. `DenseHandle<T>`), the type system already prevents
cross-type confusion the way `Handle<Foo>` vs `Handle<Bar>` does today,
and a shared counter is not required — this becomes an implementation
choice to make explicitly when #799 is picked up, not before.

### What must be measured before landing

1. A real (not holes-synthesized-then-immediately-swept) benchmark of
   `DenseRegion<T>` iteration under the SAME hole-accumulation shape the
   audit's ~8.7x number was measured under, to confirm the ceiling is
   actually reachable, not just theoretically available.
2. Insert/remove throughput regression versus `Region<T>` on a churny
   workload, since `DenseSlotMap` remove is a swap-and-truncate that must
   fix up the moved element's key — real cost, not assumed from this
   crate's own docs' general "~2.8x churn" figure (that figure describes
   `slotmap`'s own benchmarks, not this crate's specific insert/remove
   pattern; would need re-measurement here per this repo's own
   cost-and-benefit-same-regime rule).
3. Whether `DenseRegion<T>` needs its own `SyncRegion`-equivalent
   (`SyncDenseRegion<T>`?) or can share `SyncRegion<T>`'s implementation
   generically over a backing-store trait — a real design fork, not
   resolved here.

## 2. `ShardedSyncRegion<T>` — for independent-key concurrent workloads

### Motivation

`SyncRegion<T>`'s single `RwLock<Region<T>>` globally serializes every
write and forces even independent-key reads to contend on the same lock
(and, underneath it, the same handful of cache lines the lock's state
lives on). For a workload whose reads/writes are naturally
key-independent (no cross-key invariant to preserve), sharding could
reduce contention significantly — but "could" is doing real work in that
sentence: this has NOT been measured against a real mixed read/write
workload, only asserted as plausible from the structural bottleneck.

### Design shape

A separate `ShardedSyncRegion<T>` type, most likely N independent
`SyncRegion<T>`-shaped shards (or N independent `RwLock<Region<T>>`s
directly) selected by some shard key derived from the `Handle<T>` itself
at lookup time — which means the shard identifier must be encoded IN the
handle at insert time, not recomputed from the value (the value isn't
available at `get`/`remove` time before the shard is known).

### Coordinating shard identity with F2's region_id — the one open design question

This is the crux this design note exists to flag, per #800's own
description: the shard id and the F2 region-identity id should share
"one coherent encoding" rather than being bolted on separately. Two
concrete shapes, neither implemented, both requiring a decision before
#800 becomes buildable:

**Shape A — shard id is a SEPARATE field.** `ShardedHandle<T>` (or
whatever it's named) carries `region_id: NonZeroU64` (identical role to
today's `Handle<T>`, rejecting cross-instance confusion) PLUS a
`shard_id: u32` (or however many shards are supported) stamped at
insert time based on which shard's `SlotMap` actually received the
value. Simple, but grows the handle again (already 16 bytes post-F2;
this would push it to 24 bytes with a naive layout, or 16 bytes if
`shard_id` fits in `region_id`'s otherwise-unused high bits — see Shape
B).

**Shape B — shard id is PACKED into region_id's bit space.** Since
`region_id: NonZeroU64` is a process-wide monotonic counter and a real
process will never mint anywhere close to 2^64 `Region`/`ShardedSyncRegion`
instances, the top N bits of `region_id` could encode the shard index
directly (e.g. top 8 bits = shard index 0-255, bottom 56 bits = the
actual monotonic instance counter — still enormous headroom, and still
satisfies `NonZeroU64`'s non-zero requirement as long as the counter
portion is stamped correctly). This keeps `Handle<T>`'s layout unchanged
(no new field), at the cost of a slightly more intricate encode/decode
step in every accessor and a hard cap on shard count baked into the bit
width chosen. This shape needs its own dedicated design pass if chosen
— it is sketched here as a real option, not a recommendation.

Neither shape is picked here. This is exactly the kind of "maintainer
judgment fork" the crate's F2 decision itself was (patch vs. redesign) —
it should go through the same explicit-decision protocol before any
implementation task is filed, not be silently decided by whichever
implementer picks up #800 first.

### What must be measured before landing

1. A real mixed read/write benchmark (not a synthetic single-op
   microbenchmark) against `SyncRegion<T>`'s current single-lock
   baseline, on a workload whose key access pattern is genuinely
   independent across "shards" (an adversarial workload where every
   access lands in the same shard should show NO improvement — that's
   the correct, expected outcome, and the benchmark should assert
   against regressing that case, not just the favorable one).
2. Shard count sensitivity (a sweep, not one arbitrary N) — this
   repo's own convention for a runtime-configuration sweep
   (see CLAUDE.md's "config-sweep row" evidence requirements) would
   apply once this becomes a measured gate report, not just a design
   note.
3. Whether iteration (`iter`/`iter_mut`) over a sharded store needs a
   cross-shard-locking iterator (locks every shard for the duration of
   iteration — defeats the whole point under concurrent writers) or is
   simply not offered on `ShardedSyncRegion<T>` at all (a real API
   surface decision, not a detail).

## Summary

| | `DenseRegion<T>` | `ShardedSyncRegion<T>` |
|---|---|---|
| Backing | `slotmap::DenseSlotMap` | N × `Region<T>` (or N × `RwLock<Region<T>>`) |
| Default or opt-in | Opt-in (SlotMap stays default) | Opt-in (SyncRegion stays default) |
| Handle identity impact | Reuses F2's `region_id` pattern; shared counter if handle type is reused, type-level separation if not (open choice) | Needs shard id coherently encoded alongside `region_id` — Shape A (separate field) or Shape B (packed bits), both sketched, neither chosen |
| Blocking measurement | Real hole-accumulation iteration benchmark; churn regression check | Real mixed read/write benchmark against single-lock baseline; shard-count sweep |
| Status | Design note only | Design note only |

Both remain **not implemented**. Filing an implementation task for
either requires: (1) the shard/handle-identity shape decided explicitly
by the maintainer (Shape A vs B for #800; shared-counter vs
separate-handle-type for #799), and (2) the "what must be measured
before landing" benchmarks above run and showing a real (not
ceiling-only) win under the actual regime each type targets — per this
repo's cost-and-benefit-same-regime rule in `CLAUDE.md`.
