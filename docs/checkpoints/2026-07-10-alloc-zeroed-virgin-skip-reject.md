# Checkpoint — 2026-07-10 [alloc-zeroed virgin-skip: NO-GO / honest-reject]

## Decision

**NO-GO.** The perf-plan item **P4(b) / S3** ("`alloc_zeroed` on bump-virgin
blocks must NOT memset zeros over OS zeros, guarded by a poison
counterfactual") is **rejected honestly**, not implemented. The unconditional
zeroing in both `alloc_zeroed` sites and in the bitmap init stays exactly as
it is — that is the safe, correct behavior. No code was changed; this is a
documentation-and-ledger action closing a dangling plan thread flagged by
`docs/reviews/2026-07-09-performance-review.md` finding **F6 (Low)**.

## Scope reviewed (the three sites in the "family")

- `src/registry/heap_core.rs:794` `HeapCore::alloc_zeroed` — unconditional
  `Node::zero(ptr, size.max(MIN_BLOCK))` after `alloc`.
- `src/alloc_core/alloc_core.rs:516` `AllocCore::alloc_zeroed` — same,
  unconditional `Node::zero`.
- `src/alloc_core/alloc_bitmap.rs:80` `AllocBitmap::init_in_place` — writes
  `FOOTPRINT` zero bytes (default `SEGMENT/MIN_BLOCK/8 = 32 KiB`, 8 pages) over
  the fresh segment's bitmap region on every `reserve_small_segment`.

## The plan's precondition, and why it fails

The plan (`docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md`, P4(b) line ~171
and S3 garnish line ~115) gates the optimization on **"an iron virgin flag +
mandatory poison counterfactual, else reject honestly."** That precondition is
not met and cannot be met cheaply:

1. **No per-block virgin state exists.** The only "freshness" signal in the
   allocator is *segment-level*: the `alloc-decommit` machinery tracks whether a
   whole segment is decommitted/recommitted. A decommit/recommit resets the
   ENTIRE segment; it says nothing about whether a *specific block inside an
   already-committed segment* has been handed out and freed before. Virgin-ness
   of an individual block within a live segment is a strictly finer question
   than any flag we keep — the plan underestimates this (it speaks of "a virgin
   block" as if the bump cursor gave us a durable per-block bit; it does not —
   once a block is freed and recycled it is bit-for-bit indistinguishable, at
   the `alloc_zeroed` call site, from a virgin one without new metadata).

2. **The recycled path legitimately holds non-zero garbage — cross-platform.**
   OS pages are physically zero on first commit (`VirtualAlloc(MEM_COMMIT)` /
   fresh `mmap`), so the *first* touch of a virgin bump slice is indeed being
   re-zeroed redundantly. BUT a block that was allocated (poisoned with user
   data), freed, and re-served via the freelist/magazine — WITHOUT any decommit
   cycle — still carries that old data. `alloc_zeroed` MUST zero it. And even
   the decommit→recommit route does not save us: per `crates/vmem/src/lib.rs`
   (decommit doc + the platform note ~lines 522–526), `MADV_DONTNEED` on
   macOS/XNU and the *BSDs is **advisory and lazy** — it carries NO zero-fill
   guarantee. The vmem code comments explicitly: "every `alloc_zeroed` path
   zeroes explicitly (`Node::zero` in the callers), so nothing relies on the
   kernel zeroing decommitted pages." So an unconditional virgin-skip that
   trusts "OS pages are zero" would be a **correctness bug** (returning dirty
   memory from `alloc_zeroed`), not an optimization — precisely the failure the
   plan's mandated poison-counterfactual test is designed to catch.

3. **The extractable win is narrow.** The redundant memset is pure overhead
   ONLY on the genuinely-first touch of a never-reused bump slice. In the churn
   / working-set-reuse pattern (the real hot pattern per the plan's own gap
   table) blocks are reused, so `alloc_zeroed` there is NOT operating over
   OS-zeroed pages and cannot be skipped. The win therefore lives entirely in a
   cold, first-touch, `alloc_zeroed`-specific corner (note: the churn/cold
   benches measure plain `alloc`, not `alloc_zeroed`), which is a small slice of
   an already-narrow front.

## Cost/benefit verdict

Implementing this honestly requires **new per-block virgin metadata** plus a
new invariant that is easy to break (a stale "virgin" bit on a reused block =
`alloc_zeroed` silently returns dirty memory — a latent data-disclosure /
correctness bug of the M2 severity class). Paying that metadata + invariant
cost is not justified for a narrow, `alloc_zeroed`-only, first-touch win that
has **never been shown by profiling** to be a measurable bootstrap cost. The
profiling itself (the plan's P0 iai foundation is scoped to plain
`alloc`/churn, not `alloc_zeroed`) is a separate task. Structurally: the
allocator is diagnosed as *instruction-bound, not page-fault-bound* (plan §Core
diagnosis) — the memset is a byte-copy cost that grows with size, i.e. exactly
the kind of work that does NOT dominate the flat ~28 µs ceiling the campaign is
actually fighting.

## What would flip this to GO (reconsider criteria)

- A deterministic Ir/wall-clock measurement (own `alloc_zeroed` cold bench)
  proving the redundant memset is a real, non-trivial bootstrap cost; AND
- A way to gate the skip on an EXISTING segment-level virgin signal (never a
  new per-block bit) that provably cannot be true for any block that has ever
  been freed/recycled/recommitted on ANY target OS; AND
- The mandated poison-counterfactual test green: write non-zero poison into a
  block, free it, re-`alloc_zeroed`, assert the result is still all-zero — i.e.
  the test must go RED if virgin-tracking is ever broken such that dirty memory
  leaks through `alloc_zeroed`.

Absent all three, unconditional zeroing remains the correct and shipped
behavior.

## Files touched this task

- `docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md` — P4(b) line marked
  **NO-GO (honest-reject, 2026-07-10)** with the technical reason and a pointer
  to this checkpoint.
- `docs/checkpoints/2026-07-10-alloc-zeroed-virgin-skip-reject.md` — this file.
- **No source code changed.** `alloc_zeroed` (both sites) and
  `AllocBitmap::init_in_place` retain unconditional zeroing.
