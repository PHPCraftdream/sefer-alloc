# R13-8 — Judge: 256-2048 simultaneously live 260 KiB - 2 MiB objects

**Task:** #278 (R13-8). **MEASUREMENT ONLY — not a design/implementation
decision.** This document reports what was measured against the task
brief's own two questions; no `src/` behavior changes. New artifacts:
`examples/r13_8_medium_working_set_judge.rs` (throwaway harness,
`required-features = ["alloc-core", "alloc-decommit"]`) and this document.

**Date:** 2026-07-23. **Base revision:** `main` @ `df636ff` (R13-1..R13-7
landed; R13-8 is the next task in queue). **Platform measured:** Windows 10
Pro x86-64 (native), ~48 GiB total RAM / ~17 GiB free at measurement time.
No Linux/macOS/NUMA hardware available to this session (same limitation
R13-6 §8 already documents).

---

## 0. Headline answers

**Question 1 — does the extended (40-slot) Large cache help a 256-2048
simultaneously-LIVE-object working set?** **No, and this is not really the
right question for that scenario — the cache holds only FREED/reusable
segments, and a static live set never deposits anything into it until
teardown.** Measured `dbg_large_cache_hits()` at peak-live across all four
scale points (256/512/1024/2048), in every feature arm: **0**. RSS,
commit, and per-op wall-clock at peak-live are statistically identical
between `large-cache-extended` on and off (see §2). The cache DOES matter —
decisively — for a different, TURNOVER-shaped workload in this exact size
range (24 distinct sizes, repeated alloc/free cycles): base 8 slots give
33.33% hit rate (69,216 ns/op); the 40-slot extension gives 100% hit rate
(249 ns/op) — a ~278x wall-clock win (§4). The honest reformulation: **the
extended cache is not the right tool for "hold N objects alive
simultaneously" capacity; it is the right tool for "cycle through N
distinct sizes with churn," and it already delivers a large, real win
there** (consistent with R13-7's own design intent).

**Question 2 — is `MAX_SEGMENTS` (1024) a real ceiling for this scenario,
justifying a page-run layer or an expandable `SegmentTable`?** **Yes — a
concrete, precisely located, 100%-reproducible ceiling, not a hypothetical
one.** Every arm (baseline `production`, `+exact-span-large`,
`+exact-span-large+large-cache-extended`, and `--all-features`) hits
**exactly 1023** live Large objects before `alloc()` starts returning null
— never fewer, never more, across every feature combination and every
`target` in `{1024, 2048}` (§3, §5). The 1024th slot is occupied by the
primordial segment (`bootstrap.rs` seeds `SegmentTable::from_primordial(...,
1, ...)`), so `MAX_SEGMENTS - 1 = 1023` is the exact usable capacity for
Large objects. Alloc/dealloc per-op cost stays flat (13-25 µs/op alloc,
27-95 µs/op dealloc) all the way up to the ceiling — **no non-linear
degradation approaching capacity**, only a hard wall at 1023. Whether this
ceiling justifies page-run/expandable-`SegmentTable` complexity is a
separate cost/benefit call (§6) — this task answers "is the ceiling real",
not "should it be raised."

---

## 1. Methodology

### 1.1 Harness

`examples/r13_8_medium_working_set_judge.rs`, three parts:

- **Part A — scale sweep.** For each `target` in `{256, 512, 1024, 2048}`:
  allocate up to `target` Large objects round-robin over a runtime-computed
  size ladder (see §1.2), touch first/last byte of each (genuine residency,
  not just reservation), snapshot RSS/commit/segment-reserve-count/table-
  count/cache-hits at peak-live, then deallocate all and record dealloc
  wall-clock. A FRESH `AllocCore` per `target` (no cross-contamination
  between scale points).
- **Part B — exact ceiling probe.** One run targeting `MAX_SEGMENTS + 64`
  (1088) live objects, to find precisely where `alloc()` starts returning
  null, with a hard assertion that `achieved <= MAX_SEGMENTS` (would fail
  loudly if some path let more live objects through than the table allows —
  it did not, in any arm).
- **Part C — turnover judge.** 24 distinct sizes (batched, not round-robin —
  see the module doc's note reusing R13-7's own self-caught methodology
  fix), 200 alloc-all/dealloc-all cycles (4800 total pairs), exact hit-rate
  via `AllocCore::dbg_large_cache_hits()`. This directly answers "does the
  cache matter *anywhere* in this size range," separating that from
  Question 1's "does it matter for the *static live-set* scenario."

### 1.2 Size ladder — computed, not hardcoded (lesson from task #277)

`large_size_ladder(n)` reads `AllocCore::dbg_small_class_count()` /
`dbg_block_size()` at runtime and picks `n` sizes strictly above the
CURRENT build's Small/Large boundary, up to 2 MiB. Verified directly (own
run, not assumed):

| Feature set | `small_max` | Boundary |
|---|---:|---|
| `production` alone | 258,752 B | 252.69 KiB |
| `--all-features` (`medium-classes-wide`) | 1,835,008 B | 1792.00 KiB (1.75 MiB) |

This means the harness's size ladder starts at ~266 KiB under `production`
but ~1849 KiB under `--all-features` — both runs still exercise genuinely
Large objects in whichever build they run under, rather than the
`--all-features` run silently degrading to a Small-class workload (the
exact failure mode task #277's brief warned about). Confirmed by direct
`--all-features` run (§5): all headline numbers (1023-object ceiling, 0
cache hits at peak-live, 100% turnover hit rate) hold unchanged.

### 1.3 Memory-budget honesty

Worst case: 2048 objects x up to 4 MiB committed span each (whole-`SEGMENT`
rounding, the case WITHOUT `exact-span-large`) = up to 8 GiB. This host had
~17 GiB free at measurement time (`systeminfo`/`Get-CimInstance
Win32_OperatingSystem`, checked before running). The harness carries an
explicit, loud `assert!` bail (`MAX_LIVE_BUDGET_MIB = 12_288`) with a
message naming the actual computed requirement — not a silent
`eprintln!`+return — per the task brief's explicit instruction. In
practice this bail never fired: `MAX_SEGMENTS` capped every run at 1023
objects (≤ 4 GiB committed even in the worst-case arm) well before the
budget ceiling would matter (§0 Question 2 — the real, empirically hit
limit was the table, not physical memory).

### 1.4 Diagnostics used

All pre-existing `#[doc(hidden)]` `dbg_*` hooks, no new instrumentation
added to `src/`:

- `AllocCore::dbg_segments_reserved_total()` / `dbg_segments_released_total()`
  — process-wide OS reservation/release counters (NOT gated on
  `alloc-stats` — always compiled, confirmed by direct source read of
  `alloc_core_core_diag.rs`'s doc comment).
- `AllocCore::dbg_table_count()` / `AllocCore::dbg_max_segments()` — live
  `SegmentTable` occupancy vs. the compile-time cap.
- `AllocCore::dbg_large_cache_hits()` / `dbg_large_cache_slot_sizes()` /
  `dbg_large_cache_extended_slot_sizes()` / `dbg_large_cache_extension_materialised()`
  / `dbg_large_cache_total_slots()` — cache occupancy/hit introspection
  (same set R13-6/R13-7 used). `alloc-stats` IS required for
  `dbg_large_cache_hits()`'s increment to be non-zero (same caveat R13-6
  §1.4 documents) — every measured arm below includes `alloc-stats`.
- `proc_probe::snapshot()` (the same `proc-probe`/`proc-memstat` crate
  `first_alloc_process.rs` uses) for RSS (`WorkingSetSize`) and commit
  charge (`PagefileUsage`) on this Windows host.

---

## 2. Part A — scale sweep results

All four arms measured: `production+alloc-stats` (baseline, no
`exact-span-large`), `+exact-span-large`, `+exact-span-large
+large-cache-extended`, and `--all-features` (which includes both plus
`medium-classes-wide`, `numa-aware`, etc.). Raw logs:
`docs/perf/_raw_r13_8_baseline_production.log`,
`docs/perf/_raw_r13_8_exact_span.log`,
`docs/perf/_raw_r13_8_exact_span_cache_extended.log`,
`docs/perf/_raw_r13_8_all_features.log`.

### 2.1 `production` baseline (no `exact-span-large`) — idle rss=2992 KiB, commit=648 KiB

| target | achieved | stopped_by_null | rss Δ (KiB) | commit Δ (KiB) | alloc µs/op | dealloc µs/op | cache hits at peak |
|---:|---:|---|---:|---:|---:|---:|---:|
| 256 | 256 | false | 3,152 | 1,051,008 | 19.3 | 93.8 | 0 |
| 512 | 512 | false | 6,260 | 2,101,668 | 20.0 | 85.5 | 0 |
| 1024 | **1023** | **true** | 12,412 | 4,198,864 | 19.5 | 90.2 | 0 |
| 2048 | **1023** | **true** | 12,416 | 4,198,864 | 25.3 | 89.2 | 0 |

Commit charge scales almost exactly linearly with achieved count until the
wall (≈4099 KiB commit per object — whole-4-MiB-`SEGMENT` rounding, as
expected without `exact-span-large`): 1023 objects x 4 MiB ≈ 4092 MiB ≈
4,190,208 KiB, matching the measured 4,198,864 KiB delta within header/
alignment overhead.

### 2.2 `+exact-span-large` — idle rss=2996 KiB, commit=644 KiB

| target | achieved | stopped_by_null | rss Δ (KiB) | commit Δ (KiB) | alloc µs/op | dealloc µs/op | cache hits at peak |
|---:|---:|---|---:|---:|---:|---:|---:|
| 256 | 256 | false | 3,148 | 299,448 | 13.0 | 29.1 | 0 |
| 512 | 512 | false | 6,256 | 598,552 | 14.2 | 36.9 | 0 |
| 1024 | **1023** | **true** | 12,404 | 1,194,680 | 15.2 | 31.6 | 0 |
| 2048 | **1023** | **true** | 12,408 | 1,194,680 | 16.7 | 34.8 | 0 |

`exact-span-large` cuts commit charge by ~3.5x at the 1023-object ceiling
(4,198,864 KiB -> 1,194,680 KiB) — exactly reproducing R12-3/R13-6's
already-measured win, now confirmed at THIS scale (hundreds of live
objects, not one). Dealloc cost also drops ~3x (90.2 -> 31.6 µs/op at 1024)
— consistent with far less committed-page teardown work per object.
**Neither RSS delta nor the 1023-object ceiling itself moves.**

### 2.3 `+exact-span-large+large-cache-extended` — idle rss=2976 KiB, commit=620 KiB

| target | achieved | stopped_by_null | rss Δ (KiB) | commit Δ (KiB) | alloc µs/op | dealloc µs/op | cache hits at peak |
|---:|---:|---|---:|---:|---:|---:|---:|
| 256 | 256 | false | 3,140 | 299,448 | 15.0 | 26.5 | 0 |
| 512 | 512 | false | 6,264 | 598,556 | 13.9 | 27.5 | 0 |
| 1024 | **1023** | **true** | 12,416 | 1,194,692 | 16.1 | 29.9 | 0 |
| 2048 | **1023** | **true** | 12,424 | 1,194,696 | 14.8 | 29.7 | 0 |

**Statistically identical to §2.2** (differences are within normal run-to-
run noise, e.g. 12 KiB RSS jitter, 1-3 µs/op jitter) — `large-cache-
extended` produces **zero measurable effect** on a static live-object
scale sweep, exactly as its own design predicts (it widens the FREE-segment
cache; nothing is ever freed until after the peak-live snapshot in this
harness). Cache hits at peak-live: 0 in every row, confirming the cache
never even activates during this phase of the workload.

### 2.4 `--all-features` (adds `medium-classes-wide`, `numa-aware`, etc.) — idle rss=3008 KiB, commit=652 KiB

| target | achieved | stopped_by_null | rss Δ (KiB) | commit Δ (KiB) | alloc µs/op | dealloc µs/op | cache hits at peak |
|---:|---:|---|---:|---:|---:|---:|---:|
| 256 | 256 | false | 3,432 | 512,568 | 13.5 | 41.6 | 0 |
| 512 | 512 | false | 6,588 | 1,021,060 | 14.0 | 42.7 | 0 |
| 1024 | **1023** | **true** | 12,824 | 2,035,956 | 17.4 | 43.8 | 0 |
| 2048 | **1023** | **true** | 12,908 | 2,036,156 | 15.9 | 42.2 | 0 |

Same 1023-object ceiling under `--all-features`, confirming the ceiling is
independent of `medium-classes-wide`'s boundary shift (expected — the
ceiling is `SegmentTable`-slot-based, and every Large object in this size
range still routes through the Large path in every feature combination
tested, per `R12_13_PAGE_RUN_LAYER_DEFERRED.md` §3's already-established
finding). Commit-per-object here is higher than §2.2/§2.3 (≈2034 KiB vs.
≈1195 KiB) because the size ladder itself starts at a higher floor (~1.75
MiB vs. ~260 KiB) under `medium-classes-wide` — this is the ladder's honest
size shift (§1.2), not a regression.

**No non-linear wall-clock degradation observed anywhere in any arm**
approaching the 1023 ceiling: alloc µs/op and dealloc µs/op both stay in a
narrow band (13-25 / 27-95 µs/op respectively) across every scale point in
every arm — the ceiling is a hard, binary wall (null return), not a
gradual slowdown.

---

## 3. Part B — exact `MAX_SEGMENTS` ceiling probe

One run per arm, `probe_target = MAX_SEGMENTS + 64 = 1088`:

| Arm | achieved | stopped_by_null | table_count at stop |
|---|---:|---|---:|
| `production` baseline | 1023 | true | 1024 |
| `+exact-span-large` | 1023 | true | 1024 |
| `+exact-span-large+large-cache-extended` | 1023 | true | 1024 |
| `--all-features` | 1023 | true | 1024 |

**Identical in every arm: exactly 1023.** The harness's own hard assertion
(`achieved <= MAX_SEGMENTS`, would fail loudly if any path let more live
objects through than the table nominally allows) held in every run —
confirming `R12_13_PAGE_RUN_LAYER_DEFERRED.md` §2.1's finding ("every Large
allocation... still consumes exactly one `SegmentTable` slot") is not just
true in principle but **exactly, reproducibly true in practice at this
scale**, with no off-by-more-than-one surprises. The "-1" (1023 rather than
1024) is explained by `bootstrap.rs`'s `SegmentTable::from_primordial(...,
1, ...)` — the primordial segment permanently occupies slot 0.

---

## 4. Part C — turnover judge (Question 1's real answer)

24 distinct Large sizes in the 260 KiB - 2 MiB range (computed ladder, same
as Parts A/B), 200 batch alloc-all/dealloc-all cycles = 4800 total
alloc/dealloc pairs:

| Arm | hit rate | wall-clock/op |
|---|---:|---:|
| `production` baseline (base 8 slots) | 33.33% | 69,215.5 ns |
| `+exact-span-large` (base 8 slots) | 33.33% | 27,161.9 ns |
| `+exact-span-large+large-cache-extended` (40 slots) | **100.00%** | **249.3 ns** |
| `--all-features` (40 slots, wider size floor) | **100.00%** | **222.7 ns** |

With only 8 base slots and 24 distinct sizes in active rotation, exactly
8/24 (33.3%) can stay resident at once — matching the measured 33.33% hit
rate exactly (an internal consistency check the harness's own output
confirms: `base slot occupancy (8): 8` in the 8-slot arms). Extending to 40
slots comfortably covers the 24-size working set (`extension slots used:
16`, `total slots: 40`), yielding a 100.00% hit rate and a **~278x**
wall-clock improvement (69,215.5 ns -> 249.3 ns per op) — the same order of
magnitude win R13-7's own hit-rate judge already found for a similar-width
working set, now confirmed specifically inside the 260 KiB - 2 MiB range
the task brief names.

**This is the honest reformulation of Question 1**: the extended cache is
irrelevant to "hold N objects alive simultaneously" (§2 — 0 hits, no
measurable effect), but delivers a large, real, reproducible win for
"repeatedly allocate/free a working set of >8 distinct sizes in this
range" — a DIFFERENT, equally realistic shape of workload in the same size
band. Both are true at once; they are not in tension.

---

## 5. `--all-features` regression check

Both the scale sweep (§2.4) and the turnover judge (§4, `--all-features`
column) were run under `--all-features` specifically to catch the class of
mistake task #277's brief warned about (a hardcoded size silently
reclassifying as Small under `medium-classes-wide`). Confirmed via direct
run: `small_max = 1,835,008 B (1792.00 KiB)` under `--all-features` vs.
`258,752 B (252.69 KiB)` under `production` alone — the harness's runtime-
computed `large_size_ladder` adapted correctly in both cases (no panics, no
silently-degraded-to-Small-path measurements, no assertion failures). All
headline findings (1023-object ceiling, 0 cache hits at peak-live, 100%
turnover hit rate with the extension) are IDENTICAL in shape under
`--all-features`, differing only in the expected, explained ways (higher
per-object commit charge because the size floor itself is higher).

---

## 6. Does the demonstrated ceiling justify page-run / an expandable
## `SegmentTable`?

This task's brief is explicit that Question 2 is "is there a real victim,"
not "should we build the fix" — but the numbers earn a brief honest note
since they materially update `R12_13_PAGE_RUN_LAYER_DEFERRED.md`'s own
central claim:

- `R12_13_PAGE_RUN_LAYER_DEFERRED.md` §3 point 2 stated: *"No workload,
  test, benchmark, or example in this repository demonstrates or exercises
  'many-thousands-of-simultaneously-live 1.25-2.0 MiB objects.'"* **This
  task is exactly that demonstration for the 260 KiB - 2 MiB band the
  task brief names** — not "many thousands," but a precisely measured,
  100%-reproducible wall at 1023 objects, well within the "256-2048" range
  the task brief itself asks about (2048 asked-for, 1023 achievable).
- The wall is **binary and total**, not a soft degradation: at 1024
  requested, the workload gets exactly 1023 live objects and then every
  further `alloc()` call returns null (graceful OOM, not a panic or UB —
  `alloc_core_large.rs`'s "no-panic" comment on `table.register`'s `None`
  branch, confirmed by direct source read) until something is freed. A
  caller needing >1023 simultaneously live Large objects in this size band
  — **on a build using `production`'s actual shipping feature set today,
  with or without the opt-in `exact-span-large`/`large-cache-extended`
  additions** — has no workaround inside this crate short of raising
  `MAX_SEGMENTS` (a compile-time constant) or restructuring around a
  page-run-style shared-arena design that does not spend one table slot
  per object.
- **This does NOT retroactively validate page-run's original arena-sharing
  motivation** (density inside one arena for MANY objects, `R11_7...md`'s
  own "5/4/3/3 vs 2/1/1/1 blocks per arena" argument) — `exact-span-large`
  already solves the RSS/commit side of that problem almost completely
  (§2.2/§2.3's ~3.5x commit reduction, matching R12-3/R13-6's prior
  finding), and this task's own numbers show **no non-linear wall-clock
  degradation** approaching the ceiling (§2, closing remark) — the
  remaining gap is purely `MAX_SEGMENTS`'s slot-count arithmetic, which a
  **much smaller fix than page-run** (simply raising `MAX_SEGMENTS`, or a
  simpler expandable/chained `SegmentTable` that keeps the existing O(1)
  `segment_of(ptr)` masking scheme) could plausibly close without needing
  page-run's "six of eleven cross-cutting mechanisms need genuinely new,
  parallel code" complexity budget (`R11_7...md`'s own estimate, quoted in
  `R12_13...md` §4).
- Recommendation left to the orchestrator/user per this task's own
  "measurement only" framing: the demonstrated victim is real and
  precisely located (§0 Question 2), but the SIMPLEST closing move —
  raising `MAX_SEGMENTS` (currently 1024, a `pub(crate) const`,
  `src/alloc_core/segment_table.rs:64`) and re-measuring the same 256-2048
  sweep — has not been tried and would be a natural, low-risk next probe
  before reaching for page-run's much larger design.

---

## 7. Runs performed (per-task-brief requirement)

- `examples/r13_8_medium_working_set_judge.rs` run personally, 4 arms,
  output inspected directly (not just exit code): `production alloc-stats`
  (`docs/perf/_raw_r13_8_baseline_production.log`), `+exact-span-large`
  (`docs/perf/_raw_r13_8_exact_span.log`),
  `+exact-span-large+large-cache-extended`
  (`docs/perf/_raw_r13_8_exact_span_cache_extended.log`), and
  `--all-features` (`docs/perf/_raw_r13_8_all_features.log`).
- `cargo fmt --check` — clean.
- `cargo clippy --all-targets --all-features -- -D warnings` — clean.
- `cargo test --release --features production` — all green (0 failures),
  confirming the new example/Cargo.toml entry does not disturb the
  existing suite.

---

## 8. Caveats

- Single-host measurement (Windows 10 Pro x86-64, ~48 GiB RAM); no
  Linux/macOS/NUMA cross-check performed this session (same limitation
  R13-6 §8 already documents for this feature family).
- `MAX_SEGMENTS`'s exact usable count (1023, not 1024) is specific to this
  codebase's current bootstrap (`SegmentTable::from_primordial(..., 1,
  ...)` reserving slot 0 for the primordial segment) — a detail worth
  keeping in mind if `MAX_SEGMENTS` is ever raised, since the usable count
  will always be `MAX_SEGMENTS - 1`, not `MAX_SEGMENTS`.
- This harness measures Large-path-only objects (round-robin over a
  runtime-computed size ladder, no Small-class objects mixed in) — a
  workload that mixes Small and Large simultaneously was out of this
  task's scope (the brief's own size range, 260 KiB - 2 MiB, is
  Large-path-only in every feature combination tested).
- §6's "raising `MAX_SEGMENTS` is the simplest closing move" is an
  observation, not a recommendation to implement — per the task brief,
  that decision is explicitly left to the orchestrator/user.
