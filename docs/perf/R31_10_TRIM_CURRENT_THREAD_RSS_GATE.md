# R31-10 — `trim_current_thread()` RSS gate: GO — measured 128 MiB RSS win in the burst-idle-burst scenario the design exists to fill

**Task #474 (R31-10), Round 31.** AC5 of the design doc
(`docs/design/R30_7_TRIM_SCAVENGE_API_DESIGN.md` §6): the load-bearing
acceptance criterion for `SeferAlloc::trim_current_thread()` — a measured
burst → `trim_current_thread()` → idle → burst sequence reclaims RSS DURING
the idle window that an otherwise-identical burst → idle → burst sequence
(no trim call) does NOT.

**Verdict: GO.** The trim arm reclaims **128.0 MiB RSS** (131.2 MiB → 3.2
MiB) and **144.3 MiB commit** (145.9 MiB → 1.6 MiB) during the idle window;
the no-trim control reclaims **0 KiB** (131.2 MiB → 131.2 MiB RSS, 145.9 →
145.9 MiB commit). Exact, stable, and reproducible across all 3 repetitions
per arm. The 128 MiB RSS win matches the expected 4 × 32 MiB = 128 MiB of
cached large spans the burst deposited. **This is the first measured runtime
improvement this round** — `trim_current_thread()` is a new public API that
changes what ships (not measurement-only work).

**Date:** 2026-07-31. **Base revision measured:** `main` @
`ba5282253918644a11e128d6c6c06d5b4ae30d1a` + working tree, tree SHA
`065f0bc5b8d7b720d56a6316ca29dcac78867a0c` (captured
2026-07-31T13:12:59.269Z, BEFORE this run's measurement binaries were
built — per CLAUDE.md's R29-6 immutable-source-identity rule, form 2: git
tree object SHA via `git write-tree`). Patch identity: sha256
`d1aaa9cb34c9088f14c265f502942d69fd0e9dc789fe0f90f53de88e464d1b76`.
Recover via `git show 065f0bc5b8d7b720d56a6316ca29dcac78867a0c: -- <path>`.
**Platform:** native Windows 10 Pro x86-64, 11th Gen Intel Core i7-11800H @
2.30GHz (8 cores / 16 logical), rustc 1.97.0 — the same host as R31-1/R31-3.
**Feature set:** `production` (`alloc-global + alloc-xthread + alloc-decommit
+ fastbin`).
**Entry point under test:** `SeferAlloc` — direct `GlobalAlloc::alloc` /
`GlobalAlloc::dealloc` trait calls + `SeferAlloc::trim_current_thread()`.
This is the SAME layer as a real `#[global_allocator]` (same TLS resolution,
same `trim_for_recycle` primitive), exercised without system-allocator noise
mixing into the RSS measurement. NOT the `HeapCore`/`HeapRegistry` substrate
(which would bypass the `SeferAlloc` entry point the feature actually ships
at — see CLAUDE.md's entry-point rule).

---

## 0. Headline numbers

### 0.1 RSS and commit at idle (median of 3 reps per arm, subprocess-per-arm)

| arm | rss_burst1 (MiB) | rss_idle (MiB) | rss_burst2 (MiB) | commit_burst1 (MiB) | commit_idle (MiB) | rss_drop (MiB) | commit_drop (MiB) | action_released_delta |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| **TRIM** | 131.2 | **3.2** | 131.2 | 145.9 | **1.6** | **128.0** | **144.3** | **4** |
| **NO_TRIM** | 131.2 | **131.2** | 131.2 | 145.9 | **145.9** | **0.0** | **0.0** | **0** |

Raw log: `docs/perf/_raw_r31_10_trim_rss_gate.log` (6 child runs: 2 arms × 3 reps).
Summary CSV: `docs/perf/R31_10_TRIM_CURRENT_THREAD_RSS_GATE_summary.csv`.

### 0.2 The win, stated with numerator and denominator (CLAUDE.md rule)

- **RSS reclaimed by trim during idle:** 128.0 MiB = `rss_idle_NO_TRIM
  (131.2 MiB) − rss_idle_TRIM (3.2 MiB)`.
- **Commit reclaimed by trim during idle:** 144.3 MiB = `commit_idle_NO_TRIM
  (145.9 MiB) − commit_idle_TRIM (1.6 MiB)`.
- Both wins are the difference between the NO_TRIM arm's idle measurement and
  the TRIM arm's idle measurement — each arm is the other's counterfactual
  (identical workload, identical idle duration, the ONLY difference is the
  single `trim_current_thread()` call in the TRIM arm).

### 0.3 Why RSS and commit differ

RSS (working set) drops by 128.0 MiB — exactly the 4 × 32 MiB payload touched
by the burst. Commit drops by 144.3 MiB — **[CORRECTED 2026-07-31, see §4
below]** each 32 MiB object actually reserves a 36 MiB usable span via
whole-`SEGMENT` rounding (`needed.div_ceil(SEGMENT) * SEGMENT`, 9 × 4 MiB
segments), so 4 objects commit and release 144 MiB, not 128 MiB + a small
fixed per-segment overhead. This is expected: `os::release_segment`
decommits the entire reserved span (all 9 segments), so the commit charge
drops by the full rounded span size, while the working set only included the
touched 32 MiB user payload per object.

---

## 1. Mechanism oracle (CLAUDE.md R30-8 path-activation rule)

Every TRIM arm hard-asserted `action_released_delta > 0` before its numbers
were trusted — `action_released_delta` is the `segments_released_total`
delta measured across the `trim_current_thread()` call itself, proving the
eviction mechanism actually fired (not just that the RSS counter moved for
an unrelated reason). Result: **all 3 TRIM reps passed with
`action_released_delta = 4`** (4 large spans evicted, matching the 4 objects
in the burst).

The NO_TRIM arm reports `action_released_delta = 0` (no trim call, so no
release) and `idle_released_delta = 0` (no release during pure idle either)
— directly confirming the R29-13/R27-3-proven gap this API exists to fill:
**pure idle reclaims exactly 0 KiB without the explicit trim call.**

**Between-arm mechanism delta (CLAUDE.md R30-8 rule):** TRIM
`action_released_delta = 4` vs NO_TRIM `action_released_delta = 0`, delta =
4 — the trim call is the sole source of the eviction, not background decay
or any other mechanism.

---

## 2. Methodology — subprocess-per-arm isolation (R26-4 / R29-6 / R30-8)

### 2.1 Subprocess-per-arm (R26-4 config identity, strongest form)

Each (arm, repetition) cell ran in its OWN freshly-spawned OS process
(`examples/r31_10_trim_rss_gate.rs`'s orchestrator spawns the same binary as
a child with env-var `R31_10_ARM={TRIM|NO_TRIM}`). A fresh process has an
empty heap registry by construction, so cross-arm state contamination is
impossible — no `first-claim-wins` slot reuse (the R25-5/R26-4 defect class),
no stale config, no residual cache from a prior arm.

### 2.2 Allocator entry point under test (CLAUDE.md entry-point rule)

`SeferAlloc` — the `#[global_allocator]` layer. The probe creates a
`SeferAlloc::new()` instance and drives it directly via `GlobalAlloc::alloc`
/ `GlobalAlloc::dealloc` + `SeferAlloc::trim_current_thread()`. This is the
SAME layer as a real `#[global_allocator]` — the `alloc`/`dealloc`/
`trim_current_thread` methods all resolve the current thread's heap via
`current_heap()` → TLS, the same path the global allocator would take. The
probe does NOT use `HeapCore` directly or bypass `SeferAlloc`, so the
measurement reflects the real production call chain.

### 2.3 Workload

- **Burst:** 4 × 32 MiB `GlobalAlloc::alloc` + touch-all-pages (one
  `write_volatile` per 4 KiB page) + `GlobalAlloc::dealloc` of all 4. Under
  `alloc-decommit` with the default unbounded large-cache budget, the freed
  spans are cached (not released), producing ~128 MiB of retained RSS.
- **Trim (TRIM arm only):** `a.trim_current_thread()` — evicts the entire
  large cache + drains the small pool.
- **Idle:** 500 ms `thread::sleep` — long enough for the OS to reclaim
  decommitted pages from the working set.
- **Burst 2:** identical to burst 1 (proves the thread still works after
  trim; the second burst re-materialises the cache from scratch).

### 2.4 RSS measurement

`proc_probe::snapshot()` — reads the process's RSS (working set) and commit
charge at four points: before burst1, after burst1 (cache filled), after
idle (key measurement), after burst2. All measurements are in KiB; the
report converts to MiB for readability (1 MiB = 1024 KiB).

---

## 3. Conclusion

**GO — the design's value proposition is confirmed.** `trim_current_thread()`
delivers exactly what the design doc predicted: a single call at the phase
boundary reclaims the retained RSS that pure idle cannot (R29-13 §0: "not
one byte was reclaimed" across a 2 s idle window at every headroom arm,
36/36 cells). The measured 128 MiB RSS win is the direct, measured closing
of that gap.

**Stability:** the result is exact and stable across all 3 repetitions —
the TRIM arm's `rss_idle` is 3276–3280 KiB (3.2 MiB) in every rep, and the
NO_TRIM arm's `rss_idle` is 134352–134380 KiB (131.2 MiB) in every rep. No
noise, no sign instability.

**Limitations (honestly stated):**

- **Single workload shape.** This gate measures ONE burst size (128 MiB)
  with ONE thread. A production deployment with many threads or mixed
  allocation sizes would see a proportionally larger or smaller win
  depending on how much each thread's heap retains. The mechanism is
  per-thread (CLAUDE.md's `trim_current_thread` naming rule, design §3.2),
  so a multi-threaded server would need each thread to call
  `trim_current_thread()` independently.
- **Windows-specific RSS semantics.** On Windows, `VirtualFree(MEM_DECOMMIT)`
  immediately reduces the commit charge but may briefly lag in the working
  set. The 500 ms idle window is generous enough for the working set to
  converge; on other platforms (Linux `madvise(MADV_DONTNEED)`), the
  behavior may differ in timing but not in final outcome.
- **No headroom-floor variant.** This gate measures the all-the-way-to-empty
  semantics (`evict_all`). A future `trim_current_thread_to_headroom()`
  variant (design §3.4, explicitly out of scope) would retain a configurable
  floor instead of emptying completely.

## 4. CORRECTED 2026-07-31 — §0.3's RSS/commit gap explanation misattributed the mechanism (Round 32 review, finding P2-9)

This section is appended, not a rewrite — every measured number, table, and
verdict in §§0-3 above stays exactly as originally published; only the
NAMED CAUSE in §0.3's prose was wrong, not the numbers.

**P2-9 (misattributed mechanism) — CONFIRMED, fixed.** §0.3 explained the
16.3 MiB gap between the 144.3 MiB commit drop and the 128.0 MiB RSS drop as
"~16 MiB of segment overhead (4 MiB segment headers, metadata pages, guard
pages)". Independently re-derived before accepting the correction, not
merely re-stated from the review: the actual mechanism is whole-`SEGMENT`
rounding, the SAME effect `0a34ba1` (this round's own first commit) added to
CLAUDE.md as a standing evidence rule. For a 32 MiB object at 8-byte
alignment, `AllocCore::alloc_large`'s `needed = hdr_aligned +
align_up(size, align)` (`src/alloc_core/alloc_core_large.rs:144-153`)
rounds to `32 MiB + one page` (the segment header's own footprint), then
`usable = needed.div_ceil(SEGMENT) * SEGMENT` (`:190-192`, `SEGMENT` = 4 MiB,
`src/alloc_core/os.rs:65`) rounds THAT up to **9 segments = 36 MiB** usable
span per object — not the 32 MiB payload alone. **4 objects × 36 MiB = 144
MiB** committed and released, matching the measured 144.3 MiB commit
drop almost exactly (the residual ~0.3 MiB is registry/heap bookkeeping
overhead, not segment padding); only each object's touched 32 MiB PAYLOAD
ever entered the RSS working set, which is why RSS drops 128 MiB while
commit drops 144 MiB. Relatedly, §2.3's workload label "128 MiB/burst"
describes the payload total, not the 144 MiB span footprint each burst
actually reserves — both figures are now named explicitly above rather than
conflated.

*Fix applied:* §0.3's prose corrected in place (the ONE place this
correction touches prose rather than appending only, since the original
sentence stated a specific, now-confirmed-wrong mechanism rather than a bare
number) to name whole-`SEGMENT` rounding instead of "segment headers,
metadata pages, guard pages". No measured value, table, or headline verdict
(§0.1/§0.2/§3's GO verdict) changed — this is a mechanism-explanation
correction only.
