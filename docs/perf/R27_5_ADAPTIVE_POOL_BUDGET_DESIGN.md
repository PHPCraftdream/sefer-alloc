# R27-5 — Adaptive (process-wide) small-pool growth budget: DESIGN-ONLY

**Task:** R27-5 (task #423), Round 27, P1. **DESIGN-ONLY — no `src/`, `Cargo.toml`,
`tests/`, or `benches/` file is modified.** This document turns the review's
adaptive-pool-growth sketch (`docs/reviews/2026-07-28-r26-readonly-review.md`'s
"Required retention gate" follow-on) into a real design with concrete data
structures, algorithms, acceptance criteria, and — critically — an honest
critique of where R27-3's measured data breaks it. It re-opens and re-answers
the adaptive/process-wide pool-budget question that R26-9 (task #418) closed on
the now-refuted premise that "there is no cap-specific RSS cost to manage."

**Outcome (§6): RECOMMEND OPTION 1 (keep the paired 4/16 MiB default; document an
explicit 8/32 MiB throughput recipe) over Option 2 (promote 8/32) and Option 3
(this adaptive design).** The adaptive design's principal claimed benefit — bound
aggregate RSS while granting hot heaps the latency win — is, under the
uniform-pressure workloads R27-3/R27-4 actually measured, either equivalent to
cap-8-for-all (the budget is never the binding constraint) or it splits the win
unevenly (some heaps stay slow). The single hardest sub-problem — shrinking a
grown heap back during idle — is **unsolved within this project's constraints**
(R27-3 proved idle does not decay the pool; the project has a documented
anti-precedent against background threads). The complexity cost (a new growth
heuristic + global token accounting + a decay/scavenge mechanism + reset-on-
recycle) is not justified by a benefit the measured data does not demonstrate.
This is a design that earns a CONDITIONAL-GO-on-paper / RECOMMEND-AGAINST-
SHIPPING verdict — it is written out fully so a future round that DOES find a
real uneven-pressure victim has a concrete starting point, not so it ships now.

**Style precedent:** `R17_10_BATCHED_DEFERRED_RECLAIM_DESIGN.md` and
`R25_8_RUN_ENCODED_FREE_BATCH_DESIGN.md` — a real design with concrete
pseudocode, explicit invariants, an honest scope of what it does NOT claim, and
a verdict section that may be "do not build this yet."

**Date:** 2026-07-29. **Base revision analyzed:** `main` @ `7d60ee4` (R27-4
landed). No measurement performed in this task — it synthesizes R27-3 (task
#421) and R27-4 (task #422)'s already-measured data, exactly as scoped. Line
numbers cited are current as of `7d60ee4`.

---

## 0. TL;DR — the design is sound on paper, but its headline benefit is illusory under the measured workloads, and its hardest sub-problem is unsolved within this project's constraints

Three findings collapse the recommendation to "keep the safe default + document
the recipe," despite the design itself being logically sound:

1. **The latency win is BINARY, not graduated.** R27-4's 22% win comes entirely
   from eliminating the decommit cliff (9→0 decommits/run). A heap is either at
   effective cap 8 (zero decommits, full win) or at cap 4 (9 decommits, no win).
   There is no "cap 6 gives half the win." A global token budget that lets only
   *some* heaps grow therefore gives *some* heaps the full win and leaves the
   rest with zero win — under a uniform-pressure workload (every heap saturates
   cap 4), this is strictly worse than cap-8-for-all on the aggregate, and
   indistinguishable from the status quo for the declined heaps. The budget only
   helps when pressure is UNEVEN (a few hot heaps among many cold ones) — and no
   measured workload in this project's history exhibits that shape (§4.1–4.2).

2. **Idle does not shrink a grown heap — and this project will not add a thread
   to do it.** R27-3 §3 proved the small-pool decay is event-driven (fires only
   on `reserve_small_segment`, `src/alloc_core/alloc_core_small.rs:1874`; no
   background thread). A heap that grew to cap 8 and then went idle STAYS at
   cap 8's retention until its owning thread exits (`trim_for_recycle` drains
   it). Shrinking the *growth state itself* during pure idle would require a
   timer/background thread this project has a documented, repeated anti-precedent
   against (`src/alloc_core/alloc_core.rs:135` "no background thread is needed";
   `large_cache_config.rs:330`; `large_cache_mode.rs:14`; the `background-
   scavenger` `LargeCacheMode` variant reserved `#[non_exhaustive]` but
   "deferred indefinitely"). So an adaptive design cannot deliver its
   post-burst-RSS-bounding promise without a mechanism the project has
   repeatedly declined to build (§3.5, §4.3).

3. **The complexity is a whole new subsystem for a benefit the data does not
   show.** Growth-trigger heuristic + rolling window, global token accounting (a
   process-wide atomic + a new cross-heap coordination surface this project has
   never had for the small pool), a decay/scavenge mechanism, and reset-on-
   recycle semantics — all to reach a result that, under the measured
   uniform-pressure workloads, is either "cap 8 for everyone" (= Option 2 with
   extra steps) or "partial win" (worse aggregate). Option 1 (document the
   recipe) captures the latency win for the users who want it at near-zero
   complexity cost and keeps the safety-first default untouched (§6).

The design is nevertheless written out in full (§3) so the analysis is
auditable and so a future round that finds a genuine uneven-pressure victim has
a concrete, critiqued starting point rather than re-deriving it from scratch.

---

## 1. Where this task comes from — and why now

Task #418 (R26-9, commit `8940b17`) closed the adaptive/process-wide pool-budget
design with the recorded condition: *"design (not implement) an adaptive/
process-wide pool budget, ONLY if R26-1's corrected RSS gate exposes a real
cap-8 memory penalty."* R26-1's data was RSS-neutral at peak-live-set under a
lower-pressure batch-50 shape that never proved victim activation, so R26-9
reasoned "there is no cap-specific RSS cost to manage" and closed.

That premise is now refuted at two levels:

- **R27-2 (task #420):** R26-1's RSS axis never proved victim activation (batch
  50 ≈ 12.5 MiB fits inside the 4-segment retention region) and R26-3's own
  committed raw log showed cap8 retaining ~+4,100 KiB post-teardown.
- **R27-3 (task #421):** the proper retention gate, at the pressure-producing
  batch 120, with victim activation HARD-ASSERTED for both arms — cap 4
  provably saturates (`decommit_delta` = 274/1,226/1,446 at 1/8/32 threads;
  `pooled_hw_max` = 4) and cap 8 provably retains beyond cap 4's bound
  (`pooled_hw_max` = 6, `decommit_delta` = 0). The cap8−cap4 post-teardown RSS
  delta is **~+8 MiB per materialised heap (~2 segments)**, scaling **linearly**
  to ~+255 MiB at 32 heaps, decomposing into ~+4 MiB pooled/drainable and ~+4
  MiB committed-non-pooled, and it does NOT decay during idle.
- **R27-4 (task #422):** the latency win survives at the REAL paired byte cap
  (16/32 MiB, not the 256 MiB measurement ceiling) through the real un-bypassed
  `#[global_allocator]` — cap8 ~22% faster (t=8.114, sign 19/20), decommit cliff
  eliminated (9→0, deterministic across 40 launches).

R26-9's closure condition ("ONLY if a corrected RSS gate exposes a real cap-8
memory penalty") is therefore demonstrably MET: there IS a cap-specific,
per-heap, linearly-scaling retention cost (~+8 MiB/heap) that an adaptive/
process-wide budget could, in principle, bound while letting individual hot
heaps exceed 4 segments. This document is the design that closure condition
asked for — and its honest conclusion (§6) is that the design, while sound, is
not worth building relative to Option 1, given what the data actually shows.

---

## 2. The measured facts this design must respect

Everything below is cited from R27-3 (`docs/perf/R27_3_POOL_RETENTION_GATE.md`)
and R27-4 (`docs/perf/R27_4_REAL_DEFAULT_AB_GATE.md`) — this task does not
re-measure anything.

### 2.1 The latency axis (R27-4) — the win to capture

| metric | cap 4 (current default) | cap 8 (paired candidate) |
|---|---|---|
| mean `elapsed_ns` (20 A/B/B/A blocks) | 96.71 ms | 75.31 ms |
| Δ (cap4 − cap8) | — | **+21.40 ms ≈ 22% faster** |
| statistical significance | — | t=8.114 ≫ crit 2.101; sign 19/20 |
| `decommit_calls_total` (deterministic, all 40 launches) | **9** | **0** |
| `segments_reserved_total` | 16 | 8 |
| same-vs-same control (cap4 vs cap4) | t=−0.434, 11/9 split | — |

**The win's mechanism:** the batch-120@1024B workload's ~6-segment peak demand
overflows a 4-segment pool, forcing emptied segments through
decommit→re-reserve churn (9 decommits/run); an 8-segment pool absorbs the
demand with headroom, zero decommits. The decommit cliff is the cliff — the win
is *binary* (cap ≥ demand-absorbing ⇒ 0 decommits; cap < demand ⇒ N decommits),
not graduated. This is the single most load-bearing fact for §4's critique.

### 2.2 The retention axis (R27-3) — the cost to bound

| metric | cap 4 (16 MiB) | cap 8 (32 MiB) | Δ (cap8−cap4) |
|---|---|---|---|
| post-teardown RSS, 1 thread | 26,652 KiB | 34,748 KiB | **+8,096 KiB** |
| post-teardown RSS, 8 threads | 190,296 KiB | 255,856 KiB | **+65,560 KiB** |
| post-teardown RSS, 32 threads | 745,936 KiB | 1,007,360 KiB | **+261,424 KiB (~255 MiB)** |
| per-heap Δ (8T / 32T) | — | — | +8,195 / +8,170 KiB |
| pooled high-water (per heap) | 4 | **6** | +2 |
| pooled final (per heap) | 4 | **5** | +1 |
| `decommit_delta` (1T / 8T / 32T) | 274 / 1,226 / 1,446 | 0 / 0 / 0 | — |

**Decomposition of the ~+8 MiB/heap:** ~+4 MiB is pooled (5 vs 4 segments,
drainable via `HeapCore::dbg_drain_small_pool` — proven by the post-drain RSS
drop), ~+4 MiB is committed-non-pooled (segments cap 8 never decommits that cap
4's churn releases; reclaimable only by thread-exit / recycle). The pooled tier
is bounded by `pool_cap`; the committed-non-pooled tier is the residual that
makes the total ~+8 MiB rather than ~+4 MiB (R27-3 §2).

**Decay (R27-3 §3, confirmed by reading source):** the small-pool decay shares
the large-cache 1000 ms interval (`DEFAULT_DECAY_INTERVAL_MS`,
`src/alloc_core/large_cache_config.rs:51`) but is **event-driven** — it fires
inline on the `reserve_small_segment` cold path (`maybe_decay_small_pool`,
`src/alloc_core/alloc_core_small_pool.rs:516`, called at
`alloc_core_small.rs:1874`), evicting one FIFO-oldest pooled segment per tick.
**No background thread.** Pure idle (no allocations) does NOT decay the pool:
RSS and `dbg_pooled_count` are flat across R27-3's 2 s idle window. The
retention persists until the heap does more allocation work (triggering decay
ticks) or until explicit drain / thread-exit / recycle.

### 2.3 The lifecycle facts (read from source this task)

- **`pool_cap` is set once at materialization** (`src/alloc_core/alloc_core.rs:836-839`,
  `pool_cap = min(pool_segments, pool_byte_cap / SEGMENT)`) and is an `AllocCore`
  field (`:660-672`), not re-derived per allocation. `SEGMENT = 4 MiB`.
- **`claim_with_config` is first-claim-wins** (`src/registry/heap_registry.rs:247-299`):
  a re-claim of an already-materialised slot keeps the slot's ORIGINAL config
  silently; on mismatch it bumps the process-wide `CONFIG_CONFLICTS` counter
  (`:72`, Relaxed) and `debug_assert!`s in debug (compiled out of `--release`).
- **`recycle`** (`heap_registry.rs:342`) returns the slot to the `free_slots`
  Treiber stack; `pick_slot` (`:316-322`) pops recycled slots first.
- **`trim_for_recycle`** (`src/registry/heap_core_ownership.rs:252-265`) flushes
  all tcaches then calls `AllocCore::drain_small_pool()`
  (`alloc_core_small_pool.rs:636-650`) — **it DRAINS the pool to zero**
  (releasing every pooled segment to the OS) but does NOT reset `pool_cap` (a
  plain field, untouched by drain). `MAX_HEAPS = 4096` (`src/registry/bootstrap.rs:465`).
- **Background-thread anti-precedent:** this project has NEVER spawned a
  background/daemon thread for reclamation. Every decay is event-driven. The
  `LargeCacheMode` enum reserves a `background-scavenger` variant
  (`#[non_exhaustive]`) but it is explicitly "deferred indefinitely"
  (`docs/checkpoints/2026-06-28-numa-complete-perf-investigation.md`).
- **`DECOMMIT_CALLS`** (`src/alloc_core/alloc_core.rs:221`) is a process-wide
  relaxed `AtomicU64`, incremented inside `decommit_empty_segment_impl`
  (`alloc_core_small_pool.rs:745`) — already a cold-path diagnostic, the natural
  template for any new growth-event counter.

---

## 3. Design sketch — per-heap adaptive growth with a process-wide token budget

This section develops the review's sketch faithfully, including the parts the
data later breaks (§4). The goal is a real, implementable design, not an idea
list.

### 3.1 HARD CONSTRAINT (non-negotiable): no new shared state on the hot path

> No new shared atomic or lock on the per-allocation or per-free hot path.

This project has **four consecutive NO-GOs** in the adjacent magazine-overflow
region (R24-3 `flush_magazine_class`, R24-4 bulk-mask, R25-3 `FLUSH_N`, R26-7
lazy stage array — all "added per-block/per-event bookkeeping costs more than
the one-time savings it enables"). A shared atomic touched on every `alloc`/
`dealloc` would be a fifth by construction and worse (cross-core contention,
not just added instructions). **All growth/token logic below lives ONLY on the
already-cold "cap-full, about to decommit/reserve" path** — specifically
`release_or_pool_empty_segment` (`alloc_core_small_pool.rs:236`) and
`reserve_small_segment` (`alloc_core_small.rs:1848`), neither of which is on the
scalar `alloc`/`dealloc`/`dealloc_batch_small` fast path.

### 3.2 New per-heap state (AllocCore fields, `alloc-decommit`-gated)

```text
// SKETCH — illustrative, NOT applied.
struct AllocCore {
    // ... existing pool_head/pool_tail/pooled_count/pool_cap ...
    /// The CURRENTLY-effective pool cap for THIS heap. Starts at BASE_POOL_CAP
    /// (4) and may grow toward MAX_POOL_CAP (8) after demonstrated sustained
    /// pressure. Replaces `pool_cap` as the value `release_or_pool_empty_segment`
    /// compares against (`pooled_count < effective_pool_cap`). `pool_cap`
    /// (the materialization-time configured cap) is retained as the hard
    /// ceiling `effective_pool_cap` may never exceed.
    effective_pool_cap: usize,          // BASE_POOL_CAP (4) ..= pool_cap_or_max

    /// Count of "cap-full release" events (a segment emptied, pool was already
    /// full => released instead of pooled) within the current pressure window.
    /// Incremented ONLY on the cold release leg of release_or_pool_empty_segment.
    cap_full_events_in_window: u32,

    /// Start of the current pressure-measurement window (Instant). None => no
    /// window active (reset when a window closes or the heap is trimmed).
    growth_window_start: Option<Instant>,
}

// Process-wide (one static, Relaxed — touched only on cold paths):
/// Remaining growth tokens. Each heap that grows effective_pool_cap from 4 toward
/// 8 consumes ONE token (regardless of the step size). Trim/recycle returns it.
/// Initialized to GROWTH_TOKEN_BUDGET (see §3.4 for calibration).
static GROWTH_TOKENS_REMAINING: AtomicU32 = AtomicU32::new(GROWTH_TOKEN_BUDGET);
```

No new type, no new module — three `usize`/`u32`/`Option<Instant>` fields on
`AllocCore` plus one process-wide `AtomicU32`, all behind the existing
`alloc-decommit` gate. The `Instant` is only ever constructed on the cold path
(mirroring `maybe_decay_small_pool`'s own `Instant::now()` at
`alloc_core_small_pool.rs:521`, which is already cold-path-only by the
`pooled_count == 0` fast-exit at `:518`).

### 3.3 The growth trigger (cold path only)

Two cold sites cooperate:

**(A) The "cap-full release" detector** — inside
`release_or_pool_empty_segment` (`alloc_core_small_pool.rs:236`), on the branch
where the pool is full (`pooled_count >= effective_pool_cap`, the release leg at
`:274-285`). This is the precise, already-existing code location where a segment
empties and is RELEASED rather than pooled because the cap was hit — i.e. the
direct signal that this heap is experiencing cap-driven churn:

```text
// SKETCH — in the release leg, BEFORE release_empty_segment_now.
self.cap_full_events_in_window += 1;
```

This is owner-side, single-threaded (the owner thread is the sole writer of its
`AllocCore`), so the increment is a plain field write — **no atomic, no lock.**
It runs once per cap-full release event, which under the measured batch-120
workload is 274/1,226/1,446 times across the whole 1.5 s pressure window at
1/8/32 threads (R27-3 §0) — i.e. tens-to-hundreds of times per heap per second
of sustained pressure, and zero times when the heap is idle or fits in cap 4.

**(B) The growth decision** — in `reserve_small_segment`
(`alloc_core_small.rs:1848`), AFTER the existing `maybe_decay_small_pool()` call
at `:1874` (itself cold — only reached when no registered/pooled segment has a
free block of the requested class). This is the established "small churn is
happening but the pool did not help" clock edge the decay mechanism already
piggybacks on:

```text
// SKETCH — after maybe_decay_small_pool(), before the OS reservation.
if self.effective_pool_cap < self.pool_cap.max(MAX_POOL_CAP) {
    // Open / check the rolling window.
    let now = Instant::now();
    let window_start = self.growth_window_start.get_or_insert(now);
    if now.duration_since(*window_start) >= GROWTH_WINDOW {
        // Window elapsed: evaluate pressure, then roll a fresh window.
        let events = core::mem::take(&mut self.cap_full_events_in_window);
        self.growth_window_start = Some(now);
        if events >= GROWTH_THRESHOLD && self.effective_pool_cap < MAX_POOL_CAP {
            // Try to claim a process-wide token (cold path: at most once per
            // growth step per heap, not per alloc/free).
            if GROWTH_TOKENS_REMAINING.fetch_update(
                Relaxed, Relaxed,
                |t| if t > 0 { Some(t - 1) } else { None }
            ).is_ok() {
                self.effective_pool_cap = MAX_POOL_CAP;  // grow straight to 8
            }
            // else: budget exhausted — heap stays at current cap (§3.4).
        }
    }
}
```

**Calibration against R27-3's real numbers.** The window and threshold must be
chosen so that *genuine sustained pressure* (the shape that earns the latency
win) triggers growth, but *transient blips* do not:

- `GROWTH_WINDOW = 200 ms` (one-fifth of the decay interval; short enough to
  react within a real pressure burst, long enough to reject a handful of
  stray events).
- `GROWTH_THRESHOLD`: cap 4 produces 274/1,226/1,446 cap-full events over a 1.5 s
  window at 1/8/32 threads — per-heap that is 274 / ~153 / ~45 events/1.5s. In
  a 200 ms window, the per-heap rate is ~37 / ~20 / ~6 events. A threshold of
  **`GROWTH_THRESHOLD = 4`** (cap-full events in 200 ms) would trigger at all
  three thread counts while sitting well above zero (idle produces 0). This is
  deliberately a low bar: the consequence of a false-positive growth is bounded
  RSS (§3.4), and the consequence of a false-negative is leaving the latency win
  uncaptured — the asymmetry favors triggering. A future implementation task
  must re-derive this from a stage-1 counter measurement (R17-10 §5.1's
  "measure-before-implementing" discipline), not trust this calibration blindly.

The growth is straight to `MAX_POOL_CAP` (8), not stepped (4→6→8), because §2.1
showed the win is binary — an intermediate cap of 6 would still saturate on a
7-segment demand and earn nothing. Stepping would only delay the win and add
states to test.

### 3.4 Global token accounting — the budget and its exhaustion

**Shared state:** a single process-wide `static GROWTH_TOKENS_REMAINING:
AtomicU32` (`alloc_core.rs`, sibling of `DECOMMIT_CALLS`). Touched ONLY on the
cold growth path (`fetch_update` once per heap per growth step) and on
trim/recycle (`fetch_add` once per heap exit). Never on `alloc`/`dealloc`.

**Contention model:** `fetch_update` is a CAS loop, but it executes at most
~(number of heaps that ever grow) times across the whole process lifetime —
typically tens of times, not per-operation. Even under contention from 32
threads growing simultaneously, each does one CAS that either succeeds (token
taken) or fails (budget gone, heap stays at 4). There is no spin, no blocking.
This is strictly less contention than `DECOMMIT_CALLS.fetch_add`, which already
fires on every decommit without measurable cost.

**Budget calibration.** The budget bounds the WORST-CASE aggregate RSS growth
attributable to adaptive growth. Each heap that grows from 4→8 adds ~+8 MiB
retention (R27-3 §0). So:

```text
GROWTH_TOKEN_BUDGET = (desired_aggregate_RSS_overhead_MiB) / 8
```

For a 128 MiB aggregate-growth ceiling: `GROWTH_TOKEN_BUDGET = 16`. At 32
concurrent heaps, at most 16 may grow to cap 8 (the other 16 stay at 4); the
worst-case extra retention is 16 × 8 MiB = 128 MiB, vs cap-8-for-all's 256 MiB.
**Note `MAX_HEAPS = 4096` is NOT the right calibration** — that would permit
all heaps to grow (budget never binds), making the token accounting cosmetic.

**Budget exhaustion behavior: decline, do not block or steal.** When
`fetch_update` fails (budget == 0), the heap stays at its current
`effective_pool_cap` and returns to the normal `reserve_small_segment` flow —
i.e. it takes a fresh OS segment and continues with cap-4 decommit churn. No
blocking (that would add latency to the cold path), no stealing from idle heaps
(stealing would require reading/clearing ANOTHER heap's `effective_pool_cap` — a
cross-heap coordination on the cold path that adds a second shared structure and
a reclamation-of-tokens-from-decayed-heaps problem that is strictly harder than
the idle-decay problem §3.5 already fails to solve). Decline is simple, safe,
and keeps the design's contention surface to one atomic.

### 3.5 Shrink-back / decay — the hardest part (and where the design strains against the project's constraints)

The review identified three options for reclaiming growth after a burst:

**(a) A genuinely new timer/background mechanism.** This is the only option that
reclaims retention during *pure idle* (no allocations). It is **REJECTED** by
this project's documented, repeated anti-precedent: every existing decay is
event-driven ("no background thread is needed," `src/alloc_core/alloc_core.rs:135`;
"no background thread," `large_cache_config.rs:330`, `large_cache_mode.rs:14`).
The reserved `background-scavenger` `LargeCacheMode` variant is "deferred
indefinitely." Adding a background thread for the small pool alone would
introduce a reclamation-thread lifecycle (start/stop/join, TSan validation,
`atexit` ordering) this project has never carried, to solve a problem Option 1
does not have. **Not recommended.**

**(b) Piggyback on the next allocation-driven event after idle.** Reuse the
existing `maybe_decay_small_pool` trigger (`reserve_small_segment`). The growth
state would decay by one step per decay tick when the window shows NO cap-full
events. **This is the only option compatible with the project's constraints, and
it has a fundamental limitation R27-3 §3 exposes:** a heap that went idle after
growing NEVER calls `reserve_small_segment` (it is not allocating), so it NEVER
gets a decay tick, so its `effective_pool_cap` and pooled segments STAY grown.
The decay only fires once the heap resumes allocating — and if it resumes under
pressure, you do not want to shrink it. **Net: under (b), a once-grown heap
retains its growth until thread-exit.** This is the honest, unavoidable
consequence.

**(c) An explicit scavenge call site** (e.g. a periodic `SeferAlloc::scavenge()`
the embedder opts into). This shifts the idle-reclamation burden to the
application and is functionally equivalent to the existing
`HeapCore::dbg_drain_small_pool` / a future `trim` API — it does not solve
automatic idle reclamation, it just names a manual hook. Useful as an escape
hatch, not as the primary mechanism.

**Chosen: (b), with (c) as an explicit escape hatch, and an honest acceptance
that idle retention persists until thread-exit.** Concretely:

```text
// SKETCH — in reserve_small_segment, in the window-evaluation block from §3.3.
if events == 0 && self.effective_pool_cap > BASE_POOL_CAP {
    // No pressure this window AND we are currently grown => decay one step.
    // (Only fires when the heap is ACTIVELY allocating again but no longer
    // under pressure — the narrow "pressure ceased but heap still alive" case.)
    self.effective_pool_cap -= GROWTH_STEP;  // or straight back to BASE_POOL_CAP
    GROWTH_TOKENS_REMAINING.fetch_add(1, Relaxed);  // return the token
}
```

**What this does NOT do:** it does NOT shrink a heap that is *truly idle* (zero
allocations). That heap keeps its grown cap and its pooled segments until
`trim_for_recycle` runs at thread-exit. §4.3 states the RSS consequence
explicitly.

### 3.6 Thread turnover / recycling

Because `claim_with_config` is first-claim-wins (`heap_registry.rs:247-299`) and
`trim_for_recycle` (`heap_core_ownership.rs:252`) drains the pool but does not
reset `pool_cap`, an adaptively-grown heap's state needs explicit handling at
two lifecycle points:

1. **`trim_for_recycle` (thread exit, before `recycle`):** if
   `effective_pool_cap > BASE_POOL_CAP`, **return the token**
   (`GROWTH_TOKENS_REMAINING.fetch_add(1, Relaxed)`) and **reset
   `effective_pool_cap = BASE_POOL_CAP`**, `cap_full_events_in_window = 0`,
   `growth_window_start = None`. This ensures a recycled slot starts fresh — the
   next claimant re-earns growth from scratch. This is the ONE place the design
   deliberately violates first-claim-wins for the *growth state* (the configured
   `pool_cap` still wins, as today); the growth state is runtime-earned, not
   configured, so resetting it on recycle is correct, not a config conflict.

2. **A recycled slot re-claimed by a new thread** starts at `effective_pool_cap
   = BASE_POOL_CAP` (because of the reset above), `pooled_count = 0` (drained),
   with the slot's original `pool_cap`. No stale growth leaks across owners.

If the reset at (1) were omitted, a hot thread that grew and exited would leave
its successor at cap 8 with `pooled_count = 0` — the successor would re-grow the
pool to 8 on its first pressure burst WITHOUT consuming a new token (its
`effective_pool_cap` is already 8), silently exceeding the budget by one heap.
The reset is therefore load-bearing for the budget's correctness, not cosmetic.

### 3.7 Hot-path Ir preservation — how to verify the constraint holds

The constraint (§3.1) is verifiable, not merely asserted. A future
implementation task must prove the scalar hot path is untouched:

- **`dealloc_batch_small` / scalar `alloc` / scalar `dealloc`** must be
  byte-identical in Ir before/after, measured via the project's existing iai
  harness (`npm run iai`, matching R24-2/R24-3/R25-3's Ir-gate discipline). The
  growth fields are read/written ONLY in `release_or_pool_empty_segment` (cold)
  and `reserve_small_segment` (cold); neither is on the scalar path. The iai
  reference arms (`dealloc_free_only_*`, R25-3's retained infra) must not move.
- **The `GROWTH_TOKENS_REMAINING` atomic** is never named outside the two cold
  sites + trim; a grep audit (`grep -rn GROWTH_TOKENS_REMAINING src/`) must show
  exactly those references, mirroring the `DECOMMIT_CALLS` audit pattern.
- **A counterfactual test:** a workload that fits in cap 4 (never saturates)
  must show `cap_full_events_in_window == 0`, `effective_pool_cap == BASE`,
  and identical Ir to the current cap-4 build — proving the growth machinery is
  inert for non-pressured heaps.

---

## 4. Honest critique — where R27-3's data breaks the design

This section is the reason the recommendation (§6) is "keep the safe default,"
not "build this." Each subsection names a fact that would disprove the critique
and checks it against R27-3/R27-4's ground truth.

### 4.1 The win is binary — there is no "half-grown, half-win" state

R27-4 §2 shows the latency win comes ENTIRELY from the decommit cliff
(9→0 decommits). Cap 8 absorbs the ~6-segment demand; cap 4 does not. There is
no measurement of a cap-6 or cap-7 arm, but the mechanism makes the outcome
certain: if the demand is ~6 segments and the cap is 6, the pool saturates on
any transient 7th segment and decommits; if the cap is 7, same at 8. Only a cap
≥ the sustained peak demand gives zero decommits. **So a heap is either "grown
enough" (full win) or "not grown enough" (no win).** A global budget that gives
cap 8 to only *some* heaps produces a bimodal fleet: grown heaps at 75 ms/run,
declined heaps at 97 ms/run. For a workload whose aggregate progress is the sum
(or the max) of per-heap runtimes, that is strictly worse than cap-8-for-all.

*Fact that would disprove this:* a measured cap-6 arm showing ~half the latency
win. R27-4 has no such arm; the mechanism forbids it. **Not disproven.**

### 4.2 The global budget is cosmetic under uniform pressure — the measured workloads are uniform

R27-3 §0 shows EVERY heap saturates cap 4 at every thread count (decommit_delta
= 274/1,226/1,446; per-heap ~274/~153/~45). Under a workload where all heaps
experience the same pressure, a budget smaller than the heap count forces some
heaps to decline — and §4.1 says declined heaps get no win. So:

- **Budget ≥ heap count:** every heap grows ⇒ the design is cap-8-for-all ⇒
  Option 2 with a growth heuristic, a token atomic, and a reset-on-recycle path
  layered on top, for zero additional benefit. The "bound" never binds.
- **Budget < heap count:** some heaps decline ⇒ the fleet is bimodal (§4.1) ⇒
  worse aggregate than cap-8-for-all, and the declined heaps are unchanged from
  today's cap-4 default.

The budget ONLY helps when pressure is UNEVEN: a few hot heaps among many cold
ones, where the budget routes the scarce growth tokens to the heaps that
actually decommit. **No workload in this project's measurement history exhibits
that shape** — R25-5/R26-1/R26-3/R27-3/R27-4 all use uniform per-thread
churn-with-teardown. Until an uneven-pressure victim is measured, the budget's
benefit is hypothetical.

*Fact that would disprove this:* a measured workload where a minority of heaps
saturate cap 4 and the majority never do. None exists in the committed record.
**Not disproven — the precondition for the design's headline benefit is unmet.**

### 4.3 Idle stickiness — a grown heap stays grown until thread-exit, defeating the post-burst RSS bound

R27-3 §3 proved the small-pool decay is event-driven and idle does not reclaim.
§3.5 chose option (b) (piggyback on the next alloc event) because the project
will not add a background thread. The unavoidable consequence: **a heap that
grows during a burst and then goes idle retains its ~+8 MiB (pooled +
committed-non-pooled) until its owning thread exits.** The design's promise to
"bound post-teardown RSS for a workload that does NOT sustain pressure" is
therefore only honored at thread-exit granularity, not at burst-end granularity.

For a long-lived thread pool (the common server deployment: N threads live for
the process lifetime, each serving bursts then idling), EVERY thread that ever
grew stays grown for the whole process lifetime — which is *exactly* the
cap-8-for-all retention footprint (~+8 MiB × N heaps) the design was supposed to
improve on. The adaptive machinery adds complexity to reach the same RSS as
Option 2 for the deployment that matters most (long-lived pools).

*Fact that would disprove this:* either (a) a background-decay mechanism this
project accepts (it has repeatedly declined), or (b) evidence that bursty
long-lived-pool workloads decay their growth via the event-driven path (they do
not — idle heaps do not allocate, so `reserve_small_segment` never fires).
**Not disproven — the hardest sub-problem is unsolved within the constraints.**

### 4.4 The committed-non-pooled tier is not bounded by `effective_pool_cap` alone

R27-3 §2 decomposes the ~+8 MiB/heap into ~+4 MiB pooled (bounded by the cap)
and ~+4 MiB committed-non-pooled (segments cap 8 never decommits that cap 4's
churn would have released). Growing `effective_pool_cap` to 8 lets the heap
RETAIN those segments instead of churning them — which is precisely the latency
mechanism (no decommit) — but it means the growth decision simultaneously
unlocks BOTH tiers. The token budget bounds the *count* of grown heaps, and each
grown heap contributes ~+8 MiB, so the aggregate bound is `tokens × 8 MiB`. This
is consistent, but it means the budget's RSS arithmetic must use the FULL ~+8
MiB/heap figure (not ~+4 MiB), or it under-bounds. §3.4's calibration
(`GROWTH_TOKEN_BUDGET = desired_overhead_MiB / 8`) already does this correctly;
this subsection exists so a future implementer does not naively use the pooled-
only ~+4 MiB figure and under-provision the budget.

---

## 5. Acceptance criteria — checklist a future implementation task must satisfy

These are stated as a verifiable checklist. An implementation that cannot tick
every box is not shippable.

- [ ] **AC1 — latency win reproduces for a sustained-pressure workload.** A
  workload that DOES sustain pressure (R27-4's batch-120 shape) must reproduce
  ~22% faster / 9→0 decommits for heaps that grew, measured through the real
  `#[global_allocator]` with the paired-ab runner (20 pairs, same-vs-same
  control, matching R27-4's protocol). Evidence: a `paired_ab_runs/` provenance
  JSON with t ≫ crit and sign ≈ all-favoring-grown.
- [ ] **AC2 — post-teardown RSS is bounded for a non-sustaining workload.** A
  workload that bursts then goes idle must show aggregate post-teardown RSS ≤
  `(GROWTH_TOKEN_BUDGET × 8 MiB) + cap4_baseline`, measured via R27-3's
  subprocess-per-arm protocol with config self-verification
  (`dbg_pool_cap` / `config_conflicts_total`). **Caveat §4.3:** for long-lived
  thread pools this bound holds only at thread-exit, not burst-end — the test
  must use a thread-per-burst shape (threads exit after the burst) for this AC
  to be meaningful, and the doc must say so, not hide it.
- [ ] **AC3 — correct behavior at 1/8/32 concurrent heaps.** The token budget
  must actually bound aggregate growth: at 32 heaps with budget 16, at most 16
  heaps report `effective_pool_cap == 8`; the other 16 report `== 4`; the
  `GROWTH_TOKENS_REMAINING` final value is `budget − grown_count`. Verified via
  a per-heap `dbg_effective_pool_cap()` accessor (new, `bench-internals`-gated,
  observation-only — matching the R25-1 safe-hook rule). **Honest disclosure
  required in the gate report:** state whether the budget was the binding
  constraint or whether all heaps grew (budget ≥ heap count ⇒ cosmetic, §4.2).
- [ ] **AC4 — burst-then-long-idle behavior is characterized, not hand-waved.**
  The gate report MUST explicitly state, with measurements, that a grown-then-
  idle heap retains its growth until thread-exit (§4.3), and quantify the
  retention over a ≥2 s idle window (matching R27-3 §3's flat-RSS finding). If
  the test claims idle shrink-back, it must show the mechanism (there is none
  under option (b) for truly-idle heaps — so this AC is really "prove the
  limitation is real and documented").
- [ ] **AC5 — thread turnover correctness.** A heap that grew, whose thread
  exits (`trim_for_recycle`), must (a) return its token
  (`GROWTH_TOKENS_REMAINING` increments by 1), (b) reset
  `effective_pool_cap = BASE_POOL_CAP`, (c) drain pooled segments (existing
  `drain_small_pool` behavior). A successor thread claiming the recycled slot
  starts at `effective_pool_cap == 4` and re-earns growth. Verified by a test
  that grows a heap, recycles it, re-claims, and asserts the reset.
- [ ] **AC6 — no regression to scalar hot-path Ir.** `npm run iai` on
  `dealloc_batch_small` / scalar `alloc` / scalar `dealloc` and the R25-3
  reference arms (`dealloc_free_only_*`) must be byte-identical before/after
  (the growth fields are cold-path-only). A grep audit shows
  `GROWTH_TOKENS_REMAINING` referenced ONLY in the two cold sites + trim. The
  counterfactual (cap-4-fitting workload ⇒ `cap_full_events_in_window == 0`,
  identical Ir) passes.
- [ ] **AC7 — feature gating.** All new state is `alloc-decommit`-gated (it is
  meaningless without the pool). The `dbg_effective_pool_cap()` accessor is
  `bench-internals`-gated (R25-1 rule: no new safe `pub fn` touching allocator
  metadata in `production`). No new feature enters the `production` composition
  without its own promotion gate.

---

## 6. Recommendation among the three product directions

The three options for the pool-policy default question:

1. **Keep the paired default `(4, 16 MiB)`; document an explicit `(8, 32 MiB)`
   (or `(16, 64 MiB)`) throughput recipe.** Zero `src/` default change; the
   recipe is builder guidance (`SmallSegmentPoolConfig::new().pool_segments(8)
   .pool_byte_cap(32*1024*1024)`), optionally a named preset constant if a later
   task adds one. Near-zero complexity.
2. **Promote the paired default to `(8, 32 MiB)` for everyone.** One-line
   constant change (`DEFAULT_POOL_SEGMENTS` 4→8,
   `DEFAULT_POOL_BYTE_CAP` 16→32 MiB). Captures the 22% latency win by default
   at the cost of ~+8 MiB/heap retention (linear to ~+255 MiB at 32 heaps).
3. **This adaptive design.** A new subsystem (growth heuristic, token budget,
   decay/scavenge, reset-on-recycle) intended to grant the win to hot heaps
   while bounding aggregate RSS.

**Recommendation: OPTION 1.** Reasoning, earned from the data and the
complexity trade-off (not from sophistication):

- **Option 3's headline benefit is unproven by the measured data (§4.2).** The
  budget only helps under uneven pressure, and no committed workload exhibits
  that shape. Under the uniform-pressure workloads this project has measured
  (every heap saturates cap 4), Option 3 is either cap-8-for-all (= Option 2
  with a subsystem layered on) or a bimodal fleet (worse aggregate). Its hardest
  sub-problem — idle shrink-back — is unsolved within the project's no-background-
  thread constraint (§4.3), so for the long-lived-thread-pool deployment that
  matters most it converges to cap-8-for-all's retention footprint anyway. The
  complexity (new fields, a process-wide atomic, growth/reset semantics, a new
  accessor, new tests) is not justified by a benefit the data does not show.
  This matches the CONDITIONAL-GO convention (R17-10, R25-8): sound on paper,
  deferred until a real uneven-pressure victim materializes and a stage-1
  counter measurement (à la R17-10 §5.1) justifies the heuristic's calibration.

- **Option 2 vs Option 1 is the genuine judgment call,** and for a project whose
  CLAUDE.md frames its identity as "safety-first / general-purpose," the
  RSS-conservative default is the more defensible *default* — it works for the
  widest deployment set, including memory-constrained and high-thread-count
  deployments where ~+255 MiB of non-idle-decaying retention is material. The
  22% latency win is real and large (R27-4), but it is a *throughput* win
  available to the users who want it via a one-line builder call with
  fully-measured numbers (R27-3 + R27-4 supply the exact RSS/latency trade
  table a recipe doc needs). A default that raises every deployment's retention
  floor to capture a win many deployments would not measure is the less
  general-purpose choice. **Document the `(8, 32 MiB)` recipe prominently**
  (README throughput-tuning section + a `docs/perf/` note citing R27-3/R27-4)
  so the win is discoverable, not hidden.

- **The counterargument to Option 1** is that if the *common* general-purpose
  workload is in fact throughput-sensitive (most real programs do batch
  allocation and would benefit from 22% lower latency), then Option 2's default
  is the better "general-purpose" choice and the RSS cost is an acceptable
  universal price. This is a reasonable position; it is not the one recommended
  here, because (a) the retention does not self-decay (R27-3 §3), so it is a
  permanent floor, not a transient peak; (b) at 32 heaps it is ~255 MiB, which
  is large enough to matter in containerized/edge deployments that are
  themselves "general-purpose"; and (c) Option 1's recipe makes the opt-in
  cost one line, so the downside of staying conservative is small. A future
  round with evidence that the dominant real workload is throughput-bound and
  RSS-insensitive could re-open Option 2 — that re-opening should rest on
  deployment evidence, not on the current measurement record.

**Net:** Option 1 now; Option 3 deferred (CONDITIONAL-GO, gated on a measured
uneven-pressure victim + a stage-1 counter calibration); Option 2 held as a
credible alternative if future deployment evidence shows throughput dominates
the general-purpose case.

---

## 7. What this document does NOT claim

- **No measurement was performed.** Every number is cited from R27-3/R27-4; the
  growth-threshold calibration in §3.3 is derived from R27-3's published
  decommit counts, not re-measured, and a future implementation must re-derive
  it from a stage-1 counter run (AC6's counterfactual discipline).
- **No claim that the adaptive design is unsound.** It is logically sound
  (§3); the verdict is that its benefit is unproven under the measured
  workloads and its hardest sub-problem is unsolved within the constraints, not
  that it is incorrect. This is a CONDITIONAL-GO / defer, matching R17-10's and
  R25-8's convention, not a NO-GO-on-paper.
- **No claim about cap 6/7.** The binary-win argument (§4.1) is from the
  decommit-cliff mechanism, not from a measured cap-6 arm; a future round COULD
  measure one, but the mechanism makes a graduated outcome implausible.
- **No `src/` change, no default change.** `DEFAULT_POOL_SEGMENTS` /
  `DEFAULT_POOL_BYTE_CAP` remain `4` / `16 MiB`. This is a design document.
- **The recipe documentation (Option 1's mechanism) is itself a separate task.**
  This document recommends it; it does not write it. A follow-up task would add
  the README/`docs/perf/` throughput-tuning note citing R27-3/R27-4's trade
  table.

---

## 8. Files changed

| file | change |
|---|---|
| `docs/perf/R27_5_ADAPTIVE_POOL_BUDGET_DESIGN.md` | this design document (new) |
| `docs/perf/OPEN_ITEMS.md` | item 13 "Next trigger" bullet updated + new dated paragraph appended (append-only convention) |

**No production source file changed.** No `src/`, `examples/`, `benches/`, or
`tests/` file touched. No commit made — tree left unstaged for personal
zero-trust review, per this task's explicit instruction.
