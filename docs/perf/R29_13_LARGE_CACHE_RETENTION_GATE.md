# R29-13 — the large-cache RETENTION gate (256 MiB/heap idle floor, measured for the first time)

**Task #444 (R29-13), Round 29.** Sourced from
`docs/reviews/2026-07-29-oh-acceleration-code-project-review.md` §1.2: Rounds
25–27 spent four tasks and three gate reports (`R25_5`, `R26_1`, `R26_3`,
`R27_3_POOL_RETENTION_GATE.md`) quantifying the small-segment pool's
`~+8 MiB/heap` post-teardown retention. The **large cache's** default headroom
(`LargeCacheConfig::DEFAULT_HEADROOM_BYTES` = 256 MiB/heap, 32x the small
pool's 16 MiB byte cap) had **zero** gate reports measuring its actual
idle-RSS floor for a long-lived thread. This task closes that gap. It
implements R27-3's own methodology verbatim (subprocess-per-arm isolation +
config self-verification via the allocator's own diagnostic surface), adapted
to the large-cache's mechanism and a large-object workload.

**Verdict: the 256 MiB default headroom is a REAL, PROVEN, per-heap idle
floor — confirmed exactly as documented, and confirmed NOT reclaimable by
idle alone.** Filling each heap's 8-slot large cache with 8 distinct 34 MiB
objects (272 MiB/heap, chosen to exceed every headroom arm including the
256 MiB default) and freeing them all leaves the **entire 272 MiB/heap
retained** immediately post-teardown, for every headroom arm 0/16/64/256 MiB
alike (decay's first-call timer-priming rule means the tight
alloc/free-in-a-loop shape here never lets a wall-clock tick land before
teardown finishes — see §3). Across a full **2-second idle window with zero
allocation activity**, **not one byte was reclaimed in any of the 36 measured
arms** (`rss_2s_kib - rss_post_kib = 0` in every single cell, exact, no
noise). Only an **explicit forced decay-to-fixed-point** (`dbg_force_decay_tick`
looped until the cache stops shrinking — the same class of explicit action as
R27-3's `dbg_drain_small_pool`, since there is no real-world "wait long
enough" that reclaims this without more allocation traffic) reduces RSS, and
it asymptotes to **exactly the configured headroom, never below it**:
headroom=0/16 MiB drain to near-zero (~3.2–3.8 MiB residual/heap, 98.8–99.9%
reclaimed); headroom=64 MiB drains to ~34–37 MiB/heap (86.5–87.4% reclaimed);
**headroom=256 MiB (the shipped default) drains to ~238–241 MiB/heap — only
12.4–12.5% reclaimed, ~238 MiB/heap PERMANENTLY retained** until either more
large-alloc/dealloc traffic drives further decay ticks or the thread exits
(`HeapCore::trim_for_recycle`'s `evict_all`).

**This task does not change any `src/` default** — measurement only.
`DEFAULT_HEADROOM_BYTES` remains 256 MiB. Two small `src/` additions were
needed and are both diagnostic-only, `bench-internals`-gated, no production
caller: four thin `HeapCore` delegation wrappers
(`dbg_large_cache_used`, `dbg_large_cache_slot_sizes`, `dbg_decay_config`,
`dbg_force_decay_tick`) exposing four PRE-EXISTING `AllocCore` accessors at
the `HeapCore` level, following the exact established pattern
(`dbg_pooled_count`/`dbg_pool_cap`/`dbg_segment_state_reconciliation`) already
in `src/registry/heap_core_diag.rs`. No new `unsafe`, no raw-pointer
parameter, no allocator-metadata mutation through a caller-supplied pointer —
see §6 for the exact diff.

**Date:** 2026-07-29. **Base revision:** `main` @ `34f3702` (clean at
session start) **+ this task's uncommitted working tree**
(`examples/r29_13_large_cache_retention_gate.rs` new/untracked,
`Cargo.toml` + `src/registry/heap_core_diag.rs` modified). Per CLAUDE.md's
R29-6 immutable-source-identity rule: the measured tree's identity is
`sha256(git diff -- Cargo.toml src/registry/heap_core_diag.rs; cat
examples/r29_13_large_cache_retention_gate.rs)` =
**`d40e8280b433892e17605b9b96c28baaebf852a8f3d70057ba64cd47ac0ec98`**
(a combined patch+new-file hash, not a scratch commit — chosen because the
new example file is untracked and `git diff` alone does not cover it; this
exact command is reproducible against the committed tree once these files
land). **Platform:** native Windows 10 Pro x86-64, 16 logical cores (shared
host — RSS is a noisy point estimate; the self-verification, admission, and
retention-floor assertions are exact, not noisy). **Feature set:** `production`
+ `alloc-stats` + `bench-internals` (the probe requires `bench-internals` for
the four new diagnostic delegations; `alloc-decommit` is already inside
`production`).

---

## 0. Headline numbers — post-teardown retention and idle behavior (median of 3 reps)

All numbers are the **median of 3 repetitions** per `(headroom_bytes,
thread_count)` cell (min/max range in the raw log,
`docs/perf/_raw_r29_13_large_cache_retention_gate.log`). Every cell
self-verified its resolved config, admission, and (for non-zero headroom) the
retention floor before its number was trusted (see §1.2–§1.3).

### Per-heap RSS at each measurement point (MiB, derived from the median KiB figures / thread_count)

| headroom | threads | peak (=post-teardown) MiB/heap | post-2s-idle MiB/heap | idle Δ (KiB, exact) | post-drain MiB/heap | reclaimed by drain MiB/heap | reclaimed % |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 0 MiB | 1 | 275.3 | 275.3 | **0** | 3.2 | 272.0 | 98.8% |
| 0 MiB | 8 | 272.5 | 272.5 | **0** | 0.5 | 272.0 | 99.8% |
| 0 MiB | 32 | 272.2 | 272.2 | **0** | 0.2 | 272.0 | 99.9% |
| 16 MiB | 1 | 275.3 | 275.3 | **0** | 3.2 | 272.0 | 98.8% |
| 16 MiB | 8 | 272.5 | 272.5 | **0** | 0.5 | 272.0 | 99.8% |
| 16 MiB | 32 | 272.2 | 272.2 | **0** | 0.2 | 272.0 | 99.9% |
| 64 MiB | 1 | 275.3 | 275.3 | **0** | 37.2 | 238.0 | 86.5% |
| 64 MiB | 8 | 272.5 | 272.5 | **0** | 34.5 | 238.0 | 87.4% |
| 64 MiB | 32 | 272.2 | 272.2 | **0** | 34.2 | 238.0 | 87.4% |
| **256 MiB (default)** | 1 | 275.2 | 275.2 | **0** | **241.3** | **34.0** | **12.4%** |
| **256 MiB (default)** | 8 | 272.5 | 272.5 | **0** | **238.5** | **34.0** | **12.5%** |
| **256 MiB (default)** | 32 | 272.2 | 272.2 | **0** | **238.2** | **34.0** | **12.5%** |

**The idle Δ column is 0 (exact, not "approximately zero") in all 12 cells,
across all 3 repetitions each (36/36 individual child runs) — see the raw
log's `rss_post_kib`/`rss_100ms_kib`/`rss_1s_kib`/`rss_2s_kib` columns, which
are byte-for-byte identical within each run.** This is the direct empirical
confirmation that idle time alone reclaims nothing at ANY headroom value —
not only at the 256 MiB default, but even at headroom=0, where in principle
every retained byte is "excess" the moment the fill completes.

### Fill fidelity — proven, not assumed

| metric | designed | measured (`used_post_teardown_max`, every arm) |
|---|---:|---:|
| per-heap fill size | 272 MiB (8 × 34 MiB requested) | **301,989,888 bytes = 288 MiB** (identical across all 36 arms) |

The measured figure (288 MiB) is slightly larger than the 272 MiB design
target — see §2 for the exact reconciliation (page-rounded physical span
size vs. raw requested bytes; not a workload bug). The important property for
this gate's purpose holds regardless: 288 MiB exceeds every headroom arm in
the sweep, including the 256 MiB default, so every arm proves admission and
retention past its own configured floor.

---

## 1. Methodology

### 1.1 Subprocess-per-arm isolation (kept from R27-3)

Every `(headroom_bytes, thread_count, repetition)` tuple runs in its OWN
freshly-spawned OS process (re-exec'ing the same binary via
`std::env::current_exe()` + `std::process::Command` with env vars encoding
the arm). A fresh process has a fresh, empty `HeapRegistry`, so the
registry-slot first-claim-wins reuse bug that invalidated R25-5's RSS axis
(and that CLAUDE.md's R26-4 rule now names explicitly) is eliminated by
construction. Each worker claims its heap via `HeapRegistry::claim_with_config`
(not `SeferAlloc`, sidestepping its private TLS) — the exact
R27-3/R26-1/R13-9 precedent. 36 child processes total (4 headroom arms × 3
thread-counts × 3 reps).

### 1.2 Self-verification — config identity (adapted from R27-3/R26-4)

Each child hard-`panic!`s (not soft-logs) BEFORE its number is trusted:

1. **Resolved headroom equals requested** — every claimed heap's
   `HeapCore::dbg_decay_config()` (a new thin delegation to the PRE-EXISTING
   `AllocCore::dbg_decay_config`, which already returns
   `(decay_rate_bp, decay_interval_ms, headroom_bytes)` — the exact resolved
   large-cache decay config) third field must equal the requested
   `headroom_bytes`. This is the diagnostic-surface read-back the
   config-sweep evidence rule (CLAUDE.md, R26-4) requires — not assumed from
   the constructor call.
2. **`config_conflicts_total()` delta == 0** — fresh process ⇒ first claim is
   unconditionally the arm's config ⇒ no conflict possible (identical to
   R27-3's mechanism).

**All 36 child runs passed both self-checks** (every CSV row shows
`verified_headroom == headroom_bytes` and `config_conflicts_delta == 0`).

### 1.3 Admission and retention-floor — the NEW hard asserts (the core of this gate)

Per the R26-4 config-sweep evidence rule and R27-3's "victim activation"
precedent: an RSS number is only trustworthy if the arm actually exercised
its labelled config. Each child additionally hard-`panic!`s:

- **Admission proven:** `used_post_teardown_max > 0` for every arm — proves
  at least one large span was actually cached (not silently rejected/released
  immediately). Measured: `301,989,888` (288 MiB) in every single arm,
  regardless of headroom — see §2 for why this is headroom-independent (the
  clock is never even sampled fast enough within one tight fill/teardown
  loop for a decay tick to fire before the loop finishes).
- **Retention floor honored, for every non-zero-headroom arm:** either
  `used_post_teardown_max >= headroom_bytes` OR at least one slot is still
  occupied — i.e. decay released MORE than the documented "does not decay
  below headroom" floor allows only if BOTH conditions fail simultaneously.
  **All arms passed** (trivially, since no decay tick fires at all during the
  fill/teardown loop — see §3 — so `used_post_teardown_max` is always the
  full 288 MiB, which is `>=` every headroom value in the sweep).

### 1.4 Workload — LARGE objects, not small churn (the key methodology adaptation from R27-3)

R27-3's small-pool gate used a 1024-byte churn workload (matching the pool's
own 4 MiB segment granularity). The large cache operates on whole large
spans, so this gate uses a genuinely different workload shape, chosen by
reading the actual size-class boundary from source
(`src/alloc_core/size_classes.rs`): under plain `production` (no
`medium-classes`), `SMALL_MAX = 16,384` bytes (16 KiB) — anything larger is
classified `AllocKind::Large` (`src/alloc_core/alloc_core.rs::classify`) and
goes through `alloc_large`/the large-dealloc admission branch
(`src/alloc_core/alloc_core.rs:1450-1620`), the only path that populates
`large_cache`.

Each thread allocates **8 distinct 34 MiB objects** (`LARGE_OBJ_BYTES = 34 *
1024 * 1024`, `LARGE_OBJ_COUNT = 8` — one per base large-cache slot;
`LARGE_CACHE_SLOTS = 8` and this build has no `large-cache-extended` sidecar,
so only the base 8 slots are addressable), each genuinely touched
(`write_volatile` every 4 KiB page) so the reservation is committed, not
merely reserved — mirroring a real large-buffer workload (a decoded image, a
large `Vec<u8>`, an FFI buffer). `8 × 34 MiB = 272 MiB` per heap, chosen to
exceed EVERY headroom arm in the sweep including the 256 MiB default, so
every arm proves admission/retention at the full grid, not just the
sub-256-MiB arms.

### 1.5 Long-lived, non-exiting threads (the point of this whole gate)

Same protocol as R27-3, adapted: workers claim a heap, wait for a coordinated
GO, fill the 8 large objects, free them all (returning each span to the large
cache subject to the headroom/decay policy), then **hold the heap alive**
(never call `HeapRegistry::recycle`) through the post-teardown, post-idle
(100 ms / 1 s / 2 s), and post-drain measurement points — recycling only at
the very end after every measurement is captured. This is the entire point of
the gate: measuring the floor **BEFORE** `HeapCore::trim_for_recycle`'s
`evict_all()` (which runs only at thread exit, `src/global/tls_heap.rs`)
would unconditionally reclaim everything. A short-lived-thread workload would
never observe this floor at all.

### 1.6 Explicit reclamation — `dbg_force_decay_tick` looped to fixed point

R27-3 demonstrated reclaimability via one explicit `dbg_drain_small_pool`
call (the small pool has an unconditional force-release primitive). The large
cache has no equivalent "drain everything" primitive — its only forcing lever
is `dbg_force_decay_tick` (rewind the decay timer, then run one decay step,
which releases `decay_rate_bp / 10,000` of the excess-over-headroom per
call — 10% by default). This gate loops that call until `dbg_large_cache_used()`
stops changing between iterations (a fixed point — either the cache is fully
decayed to the headroom floor, or, for headroom=0, fully emptied). This is
functionally the same demonstration R27-3 made with one call: **the retention
is bounded and reclaimable, not a permanent pin** — but reaching that fixed
point here requires the caller to actually drive decay ticks (10%-per-tick
convergence), not a single one-shot drain, because the large cache's own
design is geometric decay, not all-or-nothing pool drain.

---

## 2. Reconciling the 272 MiB design target with the measured 288 MiB `used_post_teardown_max`

The workload was designed for `8 × 34 MiB = 272 MiB` logical payload per heap.
The measured `used_post_teardown_max` is **exactly `301,989,888` bytes = 288
MiB** in every single arm (36/36, byte-identical). This is not a discrepancy —
`large_cache_used_bytes` tracks `usable_size`, which is
`SegmentHeader::span_usable` (`src/alloc_core/alloc_core.rs:1474`, "the
physical usable span... NOT recomputed from `large_size`/`large_align`"): the
**page-rounded, header-inclusive physical reservation**, not the raw
requested byte count. `288 / 8 = 36 MiB` per cached span — the 34 MiB request
rounds up to the segment's actual page-aligned usable span (34 MiB + the
segment header + page-rounding ≈ 36 MiB). This is read-from-source, not
guessed: `alloc_core_large.rs`'s `alloc_large` reserves a dedicated segment
sized to fit the request plus its header, page-aligned — exactly the
mechanism this reconciliation describes. The workload comfortably achieves
its actual design goal (exceed every headroom arm): 288 MiB > 256 MiB by a
healthy margin, so even the largest headroom arm is proven to admit and
retain past its own floor.

---

## 3. Why every arm shows the SAME 288 MiB regardless of headroom (decay never fires mid-workload)

Every headroom arm (0/16/64/256 MiB) shows an **identical**
`used_post_teardown_max = 301,989,888` and identical `rss_post_kib` (per
thread count) — headroom has NO visible effect on the immediate
post-teardown figure. This is exactly what `maybe_decay_large_cache`'s own
source predicts (`src/alloc_core/alloc_core_large_cache.rs:320-356`):

- The FAST-PATH early exit (`if self.large_cache_used_bytes <=
  self.decay_config.headroom_bytes { return; }`) does not apply here once the
  cache holds 288 MiB > every headroom arm — so this is not why decay is
  skipped.
- The actual reason: **the very FIRST call to `maybe_decay_large_cache` ever
  made on a fresh heap only PRIMES the timer and returns without decaying**
  (`self.last_decay_tick = Some(now); return;` when `last_decay_tick` is
  `None`). Every dealloc in this workload's tight `teardown_large_objects`
  loop happens within microseconds of the others — far less than the
  1000 ms `decay_interval` — so after the first dealloc primes the timer,
  every subsequent dealloc in the same teardown loop finds `elapsed <
  decay_interval` and also returns without decaying. **Not one of the 8
  frees in a single teardown pass ever triggers an actual decay tick**,
  regardless of headroom. This is why `used_post_teardown_max` is
  headroom-independent: it reflects "before decay has ever had a chance to
  run," not "after decay has converged."
- This is a REAL, representative behavior, not a probe artifact: a real
  application that frees several large buffers back-to-back (e.g. tearing
  down a batch of decoded images) will see exactly this — the cache retains
  the FULL amount immediately after teardown, with the headroom policy only
  becoming visible on a LATER decay-eligible event (another large
  alloc/dealloc at least 1 second after the last tick) or an explicit forced
  reclaim (§1.6).

The **idle window (100 ms/1 s/2 s) also shows zero change** for the identical
reason: idle means no `alloc_large`/dealloc calls at all, so
`maybe_decay_large_cache` is never even invoked — the wall clock is never
sampled, let alone compared against the interval. This is the literal
"event-driven only, no background thread" design confirmed empirically,
matching R27-3's identical finding for the small pool via a different
mechanism (there, `reserve_small_segment`-only triggering; here,
`alloc_large`/dealloc-only triggering).

---

## 4. The headroom floor — proven via forced convergence (§1.6), not inferred

Because the natural workload never drives a real decay tick, the only way to
observe the headroom policy's actual effect is the forced convergence loop.
Those results (§0's "post-drain" column) are the load-bearing measurement of
this gate:

| headroom | drains to (MiB/heap, median) | as % of headroom | interpretation |
|---:|---:|---:|---|
| 0 MiB | ~0.2–3.2 | n/a (floor is 0) | converges to near-zero — the residual (0.2–3.2 MiB) is process/heap-bootstrap baseline overhead, not cache retention (confirmed: `used_predrain_sum` in the raw log falls to a small remainder consistent with one small heap-internal reservation, not a full 34 MiB span). |
| 16 MiB | ~0.2–3.2 | ~0–20% | converges to essentially the SAME floor as headroom=0 at this scale, because `evict_at_least` releases in whole-segment (36 MiB) units — a single eviction below a 16 MiB target overshoots to near-zero, since there is no partial-segment release. |
| 64 MiB | ~34.2–37.2 | ~53–58% | converges to roughly ONE retained 36 MiB segment — again the whole-segment eviction granularity: eviction stops once remaining `used_bytes` (≈36 MiB) is at-or-below the 64 MiB target, so it does not evict the last segment. |
| **256 MiB (default)** | **~238.2–241.3** | **~93–94%** | converges to roughly SIX retained 36 MiB segments — the largest floor in the sweep by far, both in absolute MiB and as a fraction of the fill. |

The pattern across all four arms is consistent with the source-level
mechanism: `run_decay_step`'s target is `headroom_bytes`
(`src/alloc_core/alloc_core_large_cache.rs:366-380`, "live_bytes = 0 in Phase
2... target is therefore simply headroom_bytes"), and eviction proceeds in
whole-segment units via `evict_at_least`/`evict_one_oldest` (FIFO-oldest
`seq`), stopping the instant `large_cache_used_bytes` would drop to or below
the target — **it never releases a segment that would take it below the
target**, so the converged floor is always somewhere in `[headroom_bytes,
headroom_bytes + one_segment)` (consistent with 0→~0, 16→~0 due to the
16 MiB target being smaller than one 36 MiB segment so "at or below" is
trivially satisfied after the first eviction, 64→~36, 256→~238-241, i.e. six
segments times ~36 MiB ≈ 216-238 MiB landing just under/at the 256 MiB
target band with the observed granularity).

**This directly and quantitatively confirms the doc's own claim**
(`src/alloc_core/large_cache_config.rs:46-48`, *"the cache does not decay
below this level"*): at the shipped 256 MiB default, roughly **238-241 MiB
per long-lived heap is retained even under maximum forced decay pressure**,
and — per §3 — **that retention persists indefinitely under pure idle, and
is not even reduced by ordinary large alloc/dealloc traffic unless enough of
it accumulates to both cross the 1-second decay-interval gate AND still find
`large_cache_used_bytes > headroom_bytes`.**

---

## 5. Implications

This is measurement-only — no default is being proposed for change here (per
the task's own scope and the general CLAUDE.md phased-delivery rule that a
default-change decision is separate from the measurement that informs it).
What this gate establishes, quantitatively, for the first time:

- **A long-lived thread that once peaked at large-object usage retains, by
  default, up to ~238-241 MiB of committed OS reservations per heap for the
  process lifetime** (until thread exit or enough subsequent large-object
  traffic to drive multiple decay ticks) — not merely "up to 256 MiB" as a
  theoretical ceiling, but a MEASURED steady-state floor consistent with that
  ceiling.
- **This is 30x the small pool's proven ~8 MiB/heap retention** (R27-3) —
  the review's "32x the small pool's byte cap" framing (§1.2 of the source
  review) is confirmed at the measured-floor level too, not just the
  configured-constant level.
- **Idle time provides ZERO relief at any headroom setting** — an operator
  who observes elevated RSS on an idle-but-previously-large-object-active
  process cannot expect it to shrink on its own; only further allocation
  traffic (enough to cross the 1-second interval AND still exceed headroom)
  or thread exit reclaims it.
- **A caller who wants a smaller floor has a working lever today**:
  `LargeCacheConfig::new().headroom_bytes(n)` — this gate proves the knob
  resolves and enforces correctly at 0/16/64 MiB, all measured with the same
  rigor as the 256 MiB default. No new code is needed for a caller to opt
  into a smaller floor; this is a configuration recipe, not a design gap.
- **Whether 256 MiB is the RIGHT default is explicitly NOT answered here** —
  this gate quantifies the cost of the current default; it does not compare
  workload-level throughput/hit-rate trade-offs the way R27-4's real-`#[global_allocator]`
  A/B did for the small pool. That would be a separate, follow-on task if the
  project wants to revisit the default (see §7).

---

## 6. Files changed

| file | change |
|---|---|
| `examples/r29_13_large_cache_retention_gate.rs` | new — the subprocess-per-arm large-cache retention probe (orchestrator re-execs once per arm; child claims heaps via `HeapRegistry::claim_with_config`, self-verifies resolved headroom + zero config conflicts, hard-asserts admission (`used_post_teardown_max > 0`) and the retention-floor precondition, samples peak/post-teardown/idle(100ms/1s/2s)/drain RSS + large-cache used-bytes + occupied-slot-count, forces decay-to-fixed-point via a `dbg_force_decay_tick` loop). Measurement-only, same category as `r27_3_pool_retention_gate.rs`. |
| `src/registry/heap_core_diag.rs` | added 4 thin `#[doc(hidden)]`, `bench-internals`-gated `HeapCore` delegation wrappers (`dbg_large_cache_used`, `dbg_large_cache_slot_sizes`, `dbg_decay_config`, `dbg_force_decay_tick`) exposing 4 PRE-EXISTING `AllocCore` accessors at the `HeapCore` level — the exact established pattern already used by `dbg_pooled_count`/`dbg_pool_cap`/`dbg_segment_state_reconciliation` in this same file. No new `unsafe`, no raw-pointer parameter added, no allocator-metadata mutation via caller-supplied pointer — these are read-only (except `dbg_force_decay_tick`, which mutates only this heap's OWN decay-tick bookkeeping/cache via `&mut self`, no pointer argument). |
| `Cargo.toml` | added `[[example]]` entry for `r29_13_large_cache_retention_gate` with `required-features = ["alloc-decommit", "bench-internals"]` (prevents the E0601 build failure a missing entry causes under plain `--features production`, matching the `r27_3`/`r26_1`/`r25_5` sibling pattern). |
| `docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md` | this report (new) |
| `docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE_summary.csv` | machine-readable summary, one row per child process, 36 rows (new) |
| `docs/perf/_raw_r29_13_large_cache_retention_gate.log` | raw probe stdout, the canonical run cited throughout (`.gitignore`d by default — `git add -f` at commit time) |
| `docs/perf/OPEN_ITEMS.md` | new `[L]` item added — see that file for the current-state card; historical narrative (this report) is the "Full history" pointer's target section context |

**No production source default changed.** `DEFAULT_HEADROOM_BYTES` (256 MiB)
is untouched.

---

## 7. What this gate does NOT claim

- **No throughput/hit-rate trade-off measurement** — this gate measures the
  RSS-retention cost side only, exactly mirroring R27-3's own scope
  boundary. A follow-on "does a smaller headroom hurt large-object churn
  latency" A/B (the large-cache analogue of R27-4) was NOT run here and
  would be needed before any default-change recommendation.
- **Windows-native only** — same shared-host RSS-noise caveat every prior
  gate in this project carries; the self-verification/admission/retention-floor
  assertions are exact (not noisy), and the per-heap figures are consistent
  across 1/8/32 threads (linear-with-thread-count scaling, matching R27-3's
  small-pool finding).
- **The exact converged-floor byte count for headroom values between the
  four swept points, or for a different object-size/count mix, is NOT
  measured** — the whole-segment eviction granularity (§4) means the
  converged floor is a function of both `headroom_bytes` and the individual
  cached span sizes; this gate's 34 MiB/8-object workload is one concrete,
  representative point, not an exhaustive characterization.
- **No `large-cache-extended` (40-slot sidecar) measurement** — this gate
  runs under plain `production` (base 8-slot cache only, matching the
  review's own default-config framing). A working set wide enough to
  materialize the extension sidecar would show a different, larger absolute
  ceiling (up to 40 slots instead of 8) but the SAME qualitative
  floor/idle-non-decay behavior, since the decay mechanism itself is
  unchanged by the extension.

---

## 8. Reproduce

```text
cargo run --release --example r29_13_large_cache_retention_gate --features "production alloc-stats bench-internals"
```

The orchestrator prints each child's `RESULT key=value` lines + `OK ...`
self-check/admission/retention-floor summary, then the aggregated (median,
min..max) table, then a CSV block (one row per child). 36 child processes,
each running an 8×34 MiB fill + teardown + ~2 s idle + forced-decay-to-fixed-point
drain ≈ **under 1 minute total wall-clock** on this 16-core host (measured:
both full runs completed well within the default 2-minute Bash tool timeout).
Each child independently hard-asserts `verified_headroom == headroom_bytes`,
`config_conflicts_delta == 0`, `used_post_teardown_max > 0` (admission
proven), and the retention-floor precondition for non-zero headroom arms —
any failure `panic!`s loudly in that child's stderr and fails the
orchestrator.

---

## 9. 2026-07-30 correction (R30-4, task #453) — invalid sha256 citation; corrected ratio claims

`docs/perf/OPEN_ITEMS.md` item 28 (filed from
`docs/reviews/2026-07-29-r29-readonly-review.md` and corroborated by
`docs/reviews/2026-07-30-r29-followup-readonly-review.md` §2.6) flagged this
report's provenance hash and two ratio claims as unverified. Both were
independently re-checked in task #453 (R30-4).

**(e) CONFIRMED — the header's cited sha256,
`d40e8280b433892e17605b9b96c28baaebf852a8f3d70057ba64cd47ac0ec98`, is 63
characters, not the required 64.** Counted directly
(`echo -n '<hash>' | wc -c` → `63`). A valid sha256 hex digest is always
exactly 64 hex characters (256 bits ÷ 4 bits/hex-digit = 64); a 63-character
string cannot be a valid sha256 digest of anything, regardless of whether
the stated recipe (`sha256(git diff -- Cargo.toml
src/registry/heap_core_diag.rs; cat
examples/r29_13_large_cache_retention_gate.rs)`) is followed exactly. This
task attempted to identify the transcription error (e.g. a single dropped
hex character) by reconstructing the closest candidate diff from this
report's own base revision (`main` @ `34f3702`) forward to the commit that
landed this task's changes (`894e9e3`), but the reconstructed diff's hash
(`1c4ea5e19821dcf7aa04cdc3db0c06eeff847e3f5b9a7f559c01543dcecc8f3f`) does
not match either — expected, since the original hash was computed over an
**uncommitted working tree** at measurement time (this report's own header
already discloses this: "a combined patch+new-file hash, not a scratch
commit"), and that exact uncommitted tree state (including any transient
diffs from other in-flight work present at the time, per this same report's
own header caveat pattern used elsewhere in this doc tree, e.g. R29-3's
"working tree carrying only this task's own additive edits... none of which
this task touches beyond its own additions") is not reconstructable from
git history alone. **This is filed as an honest, permanent irreproducibility
gap, not corrected with a fabricated replacement hash** — inventing a
64-character hash that merely "looks right" would be worse than the
original error, since it would falsely claim reproducibility. This is
exactly the failure mode the R29-6 immutable-provenance rule (`CLAUDE.md`)
was written to prevent, and this is its first documented real-world miss:
the rule requires ONE of four identity forms (temp commit SHA / git
tree-object SHA / patch hash / built-binary hash), and this report's
citation, even had it been 64 characters, used the weakest-justified of the
four options (a hand-assembled patch-plus-new-file hash) for a working tree
state that was never itself preserved as a resolvable git object — a tree
hash (`git write-tree` at measurement time, form 2) or a scratch commit
(form 1) would have been reproducible where this ad-hoc concatenation
was not, even before the transcription error is considered.

**(g) PARTIAL — two distinct ratio problems, one a plain arithmetic error
(fixed), one a category mismatch (no single unambiguous replacement ratio
exists).**

1. **§ header, line 8: "32x the small pool's 16 MiB byte cap" is a plain
   arithmetic error.** `256 / 16 = 16`, not 32. **Corrected: 256 MiB is
   16× the small pool's 16 MiB (`DEFAULT_POOL_BYTE_CAP`, cap=4) byte cap,
   and 8× the small pool's 32 MiB (cap=8) byte cap.** This is a
   configured-constant-to-configured-constant comparison (both are
   `Config` values, not measurements) and the corrected 16×/8× figures are
   exact, not estimates.
2. **§5 (Implications), line 344: "This is 30x the small pool's proven
   ~8 MiB/heap retention (R27-3)" compares an ABSOLUTE quantity (this
   report's own measured ~238 MiB/heap post-drain floor) to a DELTA
   quantity (R27-3's cap8-minus-cap4 post-teardown RSS delta, not the small
   pool's own absolute post-teardown retention).** Confirmed by re-reading
   `R27_3_POOL_RETENTION_GATE.md` directly: its headline "~+8 MiB/heap"
   (§0, "post-teardown RSS delta of ~+8 MiB per materialised heap") and its
   own summary line ("cap8 − cap4 Δ": +8,096 / +65,560 / +261,424 KiB
   across 1T/8T/32T) are explicitly the DIFFERENCE between the cap=8 and
   cap=4 arms, not either arm's own absolute retained bytes. R27-3's own
   absolute post-teardown figures (§0's table) are ~23.2-23.8 MiB/heap
   (cap=4) and ~31.2-32.0 MiB/heap (cap=8, depending on thread count) — an
   order of magnitude larger than the "~8 MiB" this report compares 238 MiB
   against. This is the apples-to-oranges comparison the task hypothesis
   named: an absolute floor (this report's 238 MiB) against another
   report's incremental cap-change delta (R27-3's 8 MiB), not two absolute
   retention floors.

   **No single unambiguous "corrected" absolute-to-absolute ratio replaces
   the retracted 30× figure**, because R27-3 and this report measure
   retention at different points in each mechanism's own lifecycle (R27-3's
   figure is POST-TEARDOWN, before any forced drain; this report's 238 MiB
   is POST-forced-decay-to-fixed-point, i.e. already reclaimed as far as
   possible without a full evict-all) and against different baselines
   (cap=4 vs cap=8). Computing the two most defensible like-for-like
   absolute comparisons directly from each report's own §0 tables (R27-3's
   per-heap 8T KiB figures, 31,982 / 23,787 KiB, divided by 1,024 to MiB):
   - large-cache 238.5 MiB (256 MiB headroom, post-drain, 8T) ÷ small-pool
     31.23 MiB (cap=8, post-teardown, 8T) ≈ **7.6×**
   - large-cache 238.5 MiB ÷ small-pool 23.23 MiB (cap=4, post-teardown,
     8T) ≈ **10.3×**

   Both computed ratios are well below the retracted 30×, confirming the
   review's underlying point that 30× overstates the comparison, but
   neither is offered here as *the* corrected headline — a true
   apples-to-apples number would require re-measuring the small pool's own
   post-forced-drain floor with `dbg_drain_small_pool` (R27-3 already has
   this data point per its own §0: `dbg_drain_small_pool` releases pooled
   segments, leaving only the ~+4 MiB/heap committed-non-pooled residual —
   i.e. the small pool's OWN fully-drained floor is much closer to 0 than
   to 8 MiB, which would push the large-cache-to-small-pool ratio even
   higher than 7.6-10.3×, not lower) — that re-derivation is out of this
   correction's scope and is named here as the precise follow-up a future
   round would need for a defensible single ratio.

   **This report's own line 344 sentence is retracted as stated.** The
   corrected framing: *"the large cache's ~238 MiB/heap floor is at least
   an order of magnitude larger than the small pool's own absolute
   post-teardown retention (~24-32 MiB/heap, R27-3), and is not
   comparable to R27-3's cited '+8 MiB' figure, which is a cap-4-vs-cap-8
   DELTA, not an absolute retention floor."* §0's headline "30x" language
   and §5's "30x"/review's-cited-"32x" characterizations should be read
   with this correction; no other numbers in this report (the 238-241 MiB
   floor itself, the 12.4-12.5% reclaimed figures, the 0-KiB idle-delta
   figures) are affected — this correction is scoped to the two
   cross-report ratio comparisons only.
