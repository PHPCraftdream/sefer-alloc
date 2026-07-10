# Perf plan — beat mimalloc on small/medium allocations (0.3.x)

**Goal:** overtake `mimalloc` on the two fronts where 0.3.0 loses, **without
surrendering a single correctness guarantee** (M2 exact double/foreign-free
no-op, D1 live-count accuracy, A1 cross-thread reclaim, xthread soundness,
`#![forbid(unsafe_code)]` by default; `production` = `#![deny(unsafe_code)]`
+ 8 named seams). Every speedup here removes a
*tautology*, never a *guard*.

Tasks: **#144 (P0) → #145 (P1) → #146 (P2) → #147 (P3) → #148 (P4) →
#149 (P5)**, chained `blockedBy`. Each phase: @sx implements → main-session
line-by-line zero-trust → personal counterfactual → full hardening sweep →
commit between phases. Perf phases start AFTER the push/CI work settles
(P0 touches `.github/`).

---

## Measured gap (0.3.0, criterion, one noisy Windows host, SeferAlloc called
directly via GlobalAlloc vs mimalloc 0.1; ratios are the signal)

Cold direct (alloc+free, no working-set reuse — "first touch"):

| size | Sefer | mimalloc | gap |
|---|---|---|---|
| 16 B | ~28 µs | ~11 µs | **2.6× slower** |
| 64 B | ~29 µs | ~14 µs | **2.0× slower** |
| 256 B | ~28 µs | ~19 µs | 1.5× slower |
| 1024 B | ~29 µs | ~35 µs | 1.2× faster |

Churn (working-set reuse of 256 live blocks — the real pattern, hits the
magazine):

| size | Sefer | mimalloc | gap |
|---|---|---|---|
| 16 B | ~29 µs | ~37 µs | 1.26× faster |
| 64 B | ~31 µs | ~38 µs | 1.23× faster |
| 256 B | ~28 µs | ~23 µs | **1.25× slower** |
| 1024 B | ~28 µs | ~161 µs | 5.8× faster |

**Two fronts to beat mimalloc:**
- **(A) Cold first-touch of tiny blocks (16–64 B)** — mimalloc 2–2.6× faster.
  This is the carve/refill path (magazine empty, fresh segment), NOT the hit.
- **(B) 256 B churn** — the only size where mimalloc leads even on reuse.

## Core diagnosis (why the ceiling is ours, not his)

Sefer is **flat ~28 µs across all sizes** in both cold and churn →
**instruction-bound, not page-fault-bound**. mimalloc grows with size in cold
(11→14→19→35 µs — it pays for bytes/page-faults); we pay for *ceremony*.
Ceremony compresses; bytes don't. So the headroom is ours.

Two anatomies (from reading the hot path line-by-line):
- **Churn pair (magazine hit):** TLS-load → resolver (2 branches: TORN, null)
  → `class_for` → count-- → slot-load → **`lock xadd`** ‖ free: mask →
  **hash-probe** → `class_for` (second time!) → count++ → slot-store.
- **Cold pair (carve):** every fresh block does a *pilgrimage* through the
  BinTable — carve → write_next into the block body → bitmap RMW → head-store
  → *(immediately back)* → read_next (dependent load!) → bitmap RMW →
  head-store → live_count++. ~8 metadata touches to hand out a virgin bump
  slice. mimalloc bump-links a fresh page once, then alloc = one pop.

---

## The five eurekas (guarantees untouched — we delete tautologies, not guards)

### Э1 — "A virgin block is already in the right state" (front A, main lever)
M2 bitmap invariant: `bit 0 = allocated`. A block freshly carved from the
bump cursor **already has bit 0**. The whole BinTable round-trip is moving it
to "free" and instantly back to "allocated" — a tautology. Remove it with a
**bump-direct fast path**: magazine empty AND freelist empty → carve a batch
straight from bump into the magazine slots (`bump += n*block_size`,
`live_count += n`), **without touching the bitmap** (bit 0 is already
correct). ~6–8 instructions/block instead of ~40. M2 byte-identical: a
double-free of such a block → `mark_free` → the second free sees "already
free" → no-op, exactly as today. D1 exact (same batch inc). Bonus: the P7
alloc-side bulk-bypass becomes unnecessary — the bump path *is* the ideal
bulk.

### Э2 — "Two sentinels, one branch" (both fronts, hottest path)
After #129 every alloc compares `p == TORN` and `p == null` — two branches
for a once-per-thread teardown case. But `null = 0`, `TORN = usize::MAX` are
the range ends: one compare catches both —
`p.addr().wrapping_sub(1) < usize::MAX - 1` → fast (Own); else cold split
(`0 → bind_slow`, `MAX → Fallback`). Semantics identical (same #129
counterfactual test), minus a branch on the process's hottest path.
Math: 0 → wrap MAX, not `< MAX-1` ✓; MAX → MAX-1, not `< MAX-1` ✓.

### Э3 — "The tail of the proof" (free half)
The `contains_base` hash-probe on every free re-proves what we proved a moment
ago. Generalize `last_stamped_segment` (OPT-C) into a tiny 2–4-word
direct-mapped **own-cache**, filled ONLY from a won hash-probe (the cache
*remembers proven*, never *asserts*); invalidated in the exact points a
segment leaves us (unregister/recycle/register-reuse — the #135 sites). A hit
GUARANTEES mapped+ours. Miss → full probe. Not one bit of the guarantee
weakened — the path to it is shortened. Honest: modest (2–6%; contains_base is
already O(1) hash) — the win is skipping the probe arithmetic + a likely
primordial-line L1 miss.

### Э4 — "Classify once" (both fronts)
`class_for` is computed 2–3× per alloc and 2× per free (confirmed in code).
Class is a pure function of (size, align); thread `c` through the path →
minus 1–2 loads from the 16 KiB SIZE2CLASS table + branches per op. Boring,
lawful, free.

### Э5 — "A counter that doesn't count" (front B + all churn)
`tcache_hits.fetch_add` is a `lock xadd` per hit (#133 removed the
*contention*, not the *lock prefix*). Owner is the sole writer:
`load(Relaxed); store(+1, Relaxed)` — same atomic visibility for `stats()`,
zero lock. (Or honestly: an `alloc-stats` feature, off by default — diagnostics
should not live in the production tick. Separate product decision, not in P1.)
Both forms TSan/miri-clean.

Garnish: overflow-flush of 8 blocks — they are neighbours in one bitmap word →
merge the RMWs per word + splice the freelist chain locally (one head-splice
instead of eight). And S3: `alloc_zeroed` on bump-virgin blocks must NOT
memset zeros over OS zeros (guarded by a poison counterfactual).

## Honest ceiling
The M2 bitmap on the *real* free path stays — that is the price of an exact
guarantee mimalloc does not offer, and we do **not** pay it one bit less.
Everything above is removal of tautologies, not of guards. Fully catching
mimalloc's free path while keeping M2 on every substrate free is not possible
without a feature-gated guard (`fast`/`hardened`) — a separate product
decision, not now (P9/0.4+, §9 of the research report, rejected here).

---

## Phased implementation

### P0 — measurement foundation (#144, S; front A is blind without it)
Add to `benches/perf_gate_iai.rs` (Linux-gated, stub-main elsewhere):
`cold_alloc_free_1024x16b`, `cold_alloc_free_1024x64b` (front A, like
`bench_direct_alloc`), `churn_256b` (front B). Wall-clock baseline already
recorded (2026-07-03 tables). After push: `workflow_dispatch` perf-gate,
record the first Ir numbers. Accept: bench compiles on Windows (stub); CI
dispatch shows Ir.

### P1 — four quick wins (#145, S, risk ~0 → beat mimalloc on 256 B churn)
Э5 (counter load+store) + Э4 (single class_for; open `AllocCore::
alloc_small_class(c)` and call `core.dealloc_small(base,ptr,c)` directly) +
Э2 (one-branch resolver in all three tls_heap resolvers) + exact **256 B
class** (SMALL_CLASS_COUNT 48→49; public type already a slice, #136 — not
breaking). Gate: production suite ×2, clippy (prod + --all-features) 0, fmt,
doc 0/0, **TSan** (atomics), miri-quick (torn test), iai churn Ir↓.
Expect: churn pair −25–35% → 256 B ~28 → ~20–22 µs vs mimalloc 23 = **overtake**.

### P2 — own-segment cache (#146, S/M, honestly modest 2–6%)
Э3. 2–4-word direct-mapped cache in `AllocCore` next to the table; filled only
from a won probe; invalidated inside unregister/recycle/register-reuse. Free
routing: register compare first, miss → `contains_base`. CRITICAL: complete
invalidation — a stale hit on an unmapped segment = UB (M2 class). Test: force
decommit/recycle of a cached segment → stale free = M2 no-op, not a
false-positive own-route. Counterfactual: break invalidation → red. Gate: miri
decommit + segment_table_o1 + M2 proptest.

### P3 — bump-direct carve: front A's main lever (#147, M, full zero-trust)
Э1. New `AllocCore::refill_class_bump(c, out) -> usize`. SOURCE ORDER
PRESERVED: freelist / ring-drain (`find_segment_with_free`) BEFORE bump-carve
— else freed blocks go stale (RSS drift, breaks xthread ring-reclaim
expectations). Bump only when genuinely empty (the cold storm is exactly
that). Retire the P7 alloc-side bypass (heap_core.rs ~587–594); keep the
dealloc-side bulk-flush. Rewrite `dbg_alloc_streak` tests deliberately (streak
semantics change). M2 not weakened a bit. Guarantee checklist: D1 batch inc
(decommit-soak green), M2 proptest, differential, region_invariants under
miri, xthread suite, TSan. Perf counterfactual: iai cold Ir before/after on CI
(expect −50%+) + local wall-clock cold 16/64 B. New regression:
cold-storm → free-storm → churn correctness. Expect: cold 16–64 B ~28 →
~14–18 µs (parity/overtake).

### P4 — polish (#148, optional, one commit per item)
(a) flush word-merge + chain-splice (free-storm 2–5%); (b) S3 `alloc_zeroed`
virgin-skip — **NO-GO (honest-reject, 2026-07-10)**, see
`docs/checkpoints/2026-07-10-alloc-zeroed-virgin-skip-reject.md`. Short reason:
the plan's own precondition ("iron virgin flag") does not exist and cannot be
had cheaply — there is no per-block virgin state, only a segment-level
decommit flag, and virgin-ness of a *specific block inside an already-committed
segment* is a finer question the plan underestimated. The extractable win is
narrow (only the first `alloc_zeroed` touch of a genuinely fresh, never-reused
bump slice), and the recycled/decommit→recommit path *legitimately* carries
non-zero garbage on macOS/XNU/*BSD (`MADV_DONTNEED` is advisory+lazy, no
zero-fill guarantee — vmem `lib.rs` §decommit note, lines ~522–526), so an
unconditional skip would be a correctness bug, not an optimization. Cost of the
required per-block virgin metadata + invariant is not justified without real
profiling (itself a separate task). Reconsider only if P0/P5 Ir data proves the
memset is a measurable bootstrap cost AND a segment-level virgin signal can
gate it without new per-block state. (c) TCACHE_CAP/FLUSH_N sweep — by
measurement only.

### P5 — final measurement + honest verdict (#149)
Re-run criterion tables; dispatch perf-gate → Ir vs P0 baseline (deterministic
proof); update README#Performance + docs/ALLOC_BENCH.md + CHANGELOG. State
plainly where we overtook mimalloc, where a gap remains and why (M2 bitmap on
real free — the price of the guarantee, paid in full). If a target is not met,
say so with numbers.

## Verifiability
Э1–Э5 are all instruction changes → visible in `perf_gate_iai` (Ir). Э2/Э4/Э5
are covered by the existing `small_churn_16b`; Э1 (cold carve) needs the new
`cold_alloc` iai bench (P0) or it stays wall-clock-only on a noisy host. The
256 B class and TCACHE_CAP tuning are wall-clock + RSS (iai insensitive).
