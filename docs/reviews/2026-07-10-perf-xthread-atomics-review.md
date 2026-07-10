# Performance review — xthread/atomics overhead on the own-thread fast path (2026-07-10)

**Scope:** whether cross-thread-safety bookkeeping (the xthread mechanism
that had a real UB bug, H1, fixed earlier this session) imposes an
avoidable tax on the pure single-thread alloc/dealloc fast path. **Method:**
fxx (Fable-5, effort=max) research agent, read-only investigation of
`src/registry/heap_core.rs`, `src/alloc_core/remote_free_ring.rs`,
`src/alloc_core/deferred_large/`, `src/concurrent/{hand,epoch_handle,
epoch_region,pinning}.rs`, `src/registry/tagged_ptr.rs`. Scope verified
against the `production` feature set (`alloc-global + alloc-xthread +
alloc-decommit + fastbin`, `alloc-stats`/`hardened` off). No source files
were modified; no findings below have been implemented.

## Headline result

The pure own-thread fast path contains **zero atomic RMWs and zero
fences, and (on the magazine-hit path) zero atomic loads**. The 2x gap vs
mimalloc on 16B cold-direct is **not** caused by atomics/fences from the
xthread machinery firing per-op; it is caused by memory-touch breadth
(number of distinct cache lines per free — see the companion fast-path and
layout reviews) plus a bounded amount of xthread bookkeeping that fires
only on the refill/Large slow paths.

### Path-by-path atomic inventory

- **Alloc, magazine hit** (`HeapCore::alloc`, `heap_core.rs:626-756`): TLS
  `Cell` load, one `class_for` static-table load, one branch, array pop.
  The `alloc-stats` counter and `hardened` gen-bump are compiled out in
  production. **No atomics at all.**
- **Dealloc, own-thread small** (`dealloc_routing` →
  `dealloc_own_thread_with_base`, `heap_core.rs:1285-1109`): pointer mask,
  `contains_base` (direct-mapped `own_cache` — one plain load+compare on
  hit), `class_for`, magazine double-free scan (plain compares,
  branchless), `bump_of` plain header load, `alloc_bitmap().is_free` plain
  load, two plain stores. **No atomics at all.**
- The epoch/hazard machinery (`src/concurrent/hand.rs`, `epoch_handle.rs`,
  `epoch_region.rs`, `pinning.rs`) is **not on the alloc path at all** — it
  belongs to the experimental `Region<T>`/`Handle<T>` face (gated
  `experimental`) and the Kani proofs. No pin/unpin per alloc/dealloc.
- `registry/tagged_ptr.rs` is used only by the `free_slots` claim/recycle
  stack — once per thread lifetime, never per-op.

## Ranked findings

### 1. Per-free M2 oracle memory-touch breadth (NOT xthread-caused, cross-referenced here) — HIGH confidence, HIGH risk

Every own-thread small free touches ~4 distinct cache lines (segment-table
`own_cache`, magazine `slots[c]`/`count`, segment header `bump`/stale-free
guard, alloc-bitmap `is_free`). mimalloc's free touches the page header and
writes `next` into the block body (a line the caller usually just
touched). This is plain (non-atomic) but *dependent, potentially cold*
loads — more expensive than any relaxed atomic. Cannot be removed without
weakening the M2 double-free guarantee; a `hardened`-style inversion is a
product-policy decision, not a bug fix. **This finding is covered in
depth by `2026-07-10-perf-fastpath-review.md` finding 1 — same root
cause, different angle of arrival.**

### 2. Refill-path ring-drain walk runs unconditionally even when every ring is permanently empty, and `is_empty()` is dead code — HIGH confidence, MEDIUM risk

- **File:** `src/alloc_core/alloc_core.rs:2808-2842`
  (`find_segment_with_free_impl`) → `src/alloc_core/remote_free_ring.rs:606-656`
  (`RemoteFreeRing::drain`).
- Runs on every free-list miss inside every magazine refill; per owned
  small segment, unconditionally executes `ring.drain(..)`: `tail.load(Acquire)`
  + `head.load(Relaxed)` + closure setup + an **unconditional
  `head.store(h, Release)` even when nothing was drained**.
  `RemoteFreeRing::is_empty()` (`:663`) exists exactly for this and is
  `#[allow(dead_code)]` — wired to nothing. In a single-threaded process
  the rings are empty forever, yet every refill pays ~3 atomic ops × N
  segments.
- **Fix direction:** guard the drain with a cheap Relaxed `tail` vs cached
  `head` compare before entering `drain()` — skipping the Release
  head-store when `h` did not advance is sound (the stored value would
  equal what producers already observe). Because the owner is the ring's
  single consumer, the owner may cache its own last-published `head`
  value in owner-private state (no atomic head load at all needed for the
  empty-check; only the remote-written `tail` must be loaded).
- **Invariant to prove:** single-consumer identity across the slot
  release→claim handshake (already argued at `remote_free_ring.rs:610-621`
  for the existing Relaxed head load — the same argument covers a cached
  copy, but it must be re-established on slot re-claim).
- **Risk: medium** — same MPSC protocol family as the H1 bug; needs a
  loom/miri pass on the wrap and re-claim cases. Expected win: moderate on
  cold-storm/churn benches, ~zero on magazine-hit steady state.

### 3. Per-Large-alloc `drain_large_deferred_free` Acquire load + stamp load — HIGH confidence it's already near-minimal, HIGH risk to touch, recommend leaving alone

- **File:** `src/alloc_core/deferred_large/drain.rs:49-52` (empty-check
  Acquire load of the slot-resident `thread_free` word) +
  `heap_core.rs:1556-1572` (`stamp_segment_owner`'s Relaxed `owner_state`
  load, OPT-C cache hit).
- Both are owner-exclusive when no cross-thread frees occur — ~2 cheap
  loads per Large op only. You cannot know "no remote pushed" without
  reading something a remote writes, so the load is already the minimal
  form. **This is literally the H1 word** (the field the earlier UB fix
  this session hoisted to a slot-resident `'static`). Recommend: **leave
  alone.**

### 4. `dealloc_routing` ownership determination is already near-optimal — HIGH confidence, no change recommended

The "is this mine" test is one masked compare against the direct-mapped
`own_cache` (a 1-entry-per-index proven-present cache) — no atomic, no
header read, safe against unmapped foreign bases. Folding it into the
header line the free path touches anyway (mimalloc-style owner-id-in-
header compare) would be unsound for the foreign/unmapped case
`contains_base` exists to protect — the hash-table check is what makes
reading the header safe at all.

### 5. Minor: duplicate classification on the Large leg — HIGH confidence, trivial impact, LOW risk

`HeapCore::alloc` computes `class` (prior OPT-C/E9 hoist), then falls
through to `AllocCore::alloc` which recomputes `Self::classify(size,
align)` (`alloc_core.rs:489-496`). Large-path-only, one redundant table
lookup + compare.

## Summary recommendation

Wire the dead `RemoteFreeRing::is_empty()` logic into
`find_segment_with_free_impl`'s per-segment loop as a drain-guard: before
constructing the drain closure, do a Relaxed `tail` load and compare
against the consumer's cached `head`; if equal, skip the drain entirely —
including the currently-unconditional `head.store(h, Release)` that today
dirties the ring's cursor line on every scanned segment of every free-list
miss even in a process that never performs a single cross-thread free.
Semantics are preserved (a remote push landing after the empty-check is
exactly as deferred as one landing after today's drain finishes — the
"later drain picks it up" contract already documented in the module).
This is the one place cross-thread plumbing demonstrably charges the pure
single-thread path for traffic that never exists; it reuses an already-
written primitive and touches the ring's consumer side only — not the H1
`thread_free` word — but given this mechanism's history it should still
land with a loom test covering the empty-check-vs-concurrent-push race
and the slot re-claim boundary before being trusted.
