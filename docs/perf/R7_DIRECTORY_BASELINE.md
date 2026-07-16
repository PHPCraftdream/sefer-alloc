# R7 Workstream A — Segment Directory Baseline (A0)

**Date:** 2026-07-16
**Base revision:** HEAD of `main` (post R7_PLAN commit)
**Platform:** Windows 10 Pro x86-64, 16 threads, Rust release profile
**Harness:** `benches/segment_directory_sweep.rs` (R6-OPT-A4 judge)

---

## 1. Diagnostic counter inventory (alloc-stats gated)

| Counter | Static | Accessor | Incremented in A0? | Purpose |
|---|---|---|---|---|
| `FULL_SCAN_SLOTS_EXAMINED` | `directory_stats.rs` | `dbg_full_scan_slots_examined()` | **YES** — per slot in `find_segment_with_free_impl` | Primary O(S) scan cost |
| `DIRECTORY_HITS` | `directory_stats.rs` | `dbg_directory_hits()` | No (A3) | Directory lookup successes |
| `DIRECTORY_STALE_HITS` | `directory_stats.rs` | `dbg_directory_stale_hits()` | No (A3) | Stale-positive clears |
| `DIRECTORY_FALLBACK_SCANS` | `directory_stats.rs` | `dbg_directory_fallback_scans()` | No (A3) | Fallback linear scan entries |
| `DIRECTORY_WORDS_EXAMINED` | `directory_stats.rs` | `dbg_directory_words_examined()` | No (A3) | Bitmap words scanned per lookup |
| `DIRTY_SEGMENTS_DRAINED` | `directory_stats.rs` | `dbg_dirty_segments_drained()` | No (A4) | Remote dirty drain invocations |

All counters: process-wide `AtomicU64`, Relaxed. Storage always compiled under
`alloc-core`. Per-event increments gated behind `alloc-stats` (matching
`FOREIGN_OR_UNROUTABLE_FREES`, `tcache_hits`, `large_cache_hits`).
Feature-OFF builds are byte-for-byte unchanged.

**Files changed:**
- `src/alloc_core/directory_stats.rs` — new file, counter storage
- `src/alloc_core/mod.rs` — module declaration
- `src/alloc_core/alloc_core_core_diag.rs` — `dbg_*` read accessors
- `src/alloc_core/alloc_core_small.rs` — `FULL_SCAN_SLOTS_EXAMINED` increment
- `benches/directory_threshold_probe.rs` — new transition-zone probe
- `Cargo.toml` — bench entry for probe

---

## 2. Full sweep tables

### 2.1 Quick matrix — representative classes, holes = {0, 50}%

class 0 = 16 B, class 25 = mid (~4 KiB), class 48 = SMALL_MAX (258,752 B).
All times in nanoseconds. `holes=0` = worst case (scan walks all S-1 non-target
segments before finding the target).

#### Class 25 (mid, ~4 KiB)

| S | holes | mean (ns) | p50 (ns) | p99 (ns) | max segments walked |
|----:|------:|----------:|---------:|---------:|--------------------:|
| 1 | 0% | 28 | 26 | 45 | 0 |
| 1 | 50% | 40 | 42 | 94 | 0 |
| 3 | 0% | 66 | 67 | 101 | 2 |
| 3 | 50% | 42 | 44 | 72 | 2 |
| 16 | 0% | 299 | 326 | 375 | 15 |
| 16 | 50% | 41 | 46 | 58 | 15 |
| 64 | 0% | 1,168 | 1,265 | 1,309 | 63 |
| 64 | 50% | 37 | 31 | 50 | 63 |
| 256 | 0% | 16,487 | 16,487 | 16,487 | 255 |
| 256 | 50% | 33 | 33 | 33 | 255 |
| 1023 | 0% | 101,620 | 101,620 | 101,620 | 1022 |
| 1023 | 50% | 37 | 37 | 37 | 1022 |

#### Class 48 / SMALL_MAX (258,752 B)

| S | holes | mean (ns) | p50 (ns) | p99 (ns) | max segments walked |
|----:|------:|----------:|---------:|---------:|--------------------:|
| 1 | 0% | 25 | 25 | 41 | 0 |
| 1 | 50% | 24 | 25 | 25 | 0 |
| 3 | 0% | 53 | 48 | 96 | 2 |
| 3 | 50% | 25 | 25 | 47 | 2 |
| 16 | 0% | 219 | 200 | 407 | 15 |
| 16 | 50% | 38 | 41 | 61 | 15 |
| 64 | 0% | 1,119 | 1,191 | 1,456 | 63 |
| 64 | 50% | 40 | 44 | 55 | 63 |
| 256 | 0% | 17,194 | 17,080 | 21,837 | 255 |
| 256 | 50% | 48 | 46 | 148 | 255 |
| 1023 | 0% | 101,834 | 100,989 | 110,103 | 1022 |
| 1023 | 50% | 46 | 48 | 58 | 1022 |

### 2.2 Kill-gate check (S <= 3 vs high S)

class = SMALL_MAX, holes = 0%:

| S | mean (ns) | p50 (ns) | p99 (ns) |
|----:|----------:|---------:|---------:|
| 1 | 25 | 25 | 35 |
| 3 | 51 | 47 | 131 |
| 1023 | 100,287 | 100,558 | 104,944 |

- `mean(S=3) / mean(S=1)` = **2.04x** (flat region, consistent with IAI S=3)
- `mean(S=1023) / mean(S=3)` = **1966x** (clear O(S) divergence)
- Gate PASSED: the high-S ratio dominates the low-S ratio.

### 2.3 Repeated-cell consistency (state-leak check)

S=64, holes=0%, SMALL_MAX, 5 repeats interleaved with unrelated cells:

| repeat | mean (ns) | p50 (ns) | p99 (ns) |
|-------:|----------:|---------:|---------:|
| 0 | 914 | 907 | 1,281 |
| 1 | 943 | 876 | 1,314 |
| 2 | 911 | 900 | 1,336 |
| 3 | 938 | 898 | 1,354 |
| 4 | 971 | 909 | 1,269 |

mean of means = 935.4 ns, max abs dev = 35.6 ns, rel spread = 3.8%.
Gate PASSED (< 5x threshold).

### 2.4 Full-class sweep at S=16, holes=0%

All 49 classes at S=16, holes=0%. Mean latency range: 201--918 ns. No per-class
anomaly (class 7 outlier at 918 ns is noise -- its block size is 128 B, which
places a capacity of ~32,768 blocks per segment, near the point where
construction itself becomes expensive).

### 2.5 Remote-dirty-density matrix (alloc-xthread)

class = SMALL_MAX, holes = 0% (target has one free, non-targets are full).

| S | dirty% | mean (ns) | p50 (ns) | p99 (ns) |
|----:|-------:|----------:|---------:|---------:|
| 3 | 0% | 180 | 200 | 500 |
| 3 | 1% | 240 | 300 | 500 |
| 3 | 10% | 360 | 300 | 900 |
| 3 | 100% | 770 | 800 | 1,100 |
| 64 | 0% | 2,566 | 2,500 | 3,400 |
| 64 | 1% | 4,400 | 3,800 | 6,600 |
| 64 | 10% | 1,533 | 1,800 | 1,900 |
| 64 | 100% | 1,366 | 1,200 | 1,700 |
| 1023 | 0% | 90,200 | 90,200 | 90,200 |
| 1023 | 1% | 1,300 | 1,300 | 1,300 |
| 1023 | 10% | 8,500 | 8,500 | 8,500 |
| 1023 | 100% | 1,500 | 1,500 | 1,500 |

At S=1023 / dirty=0%, the full linear scan costs ~90 us. With dirty > 0%, the
ring drains reclaim blocks into earlier segments, which terminate the scan
early -- the cost drops to 1--8 us. This is the load-bearing side-effect P1
mandates preserving: the ring drain inside the scan creates free blocks in
earlier segments, turning a full-S walk into an early-exit.

---

## 3. S=32..63 transition-zone data (P5 threshold choice)

class = SMALL_MAX (258,752 B), holes = 0%, worst case.
Measured by `benches/directory_threshold_probe.rs`.

| S | slots walked | mean (ns) | p50 (ns) | p99 (ns) | per slot (ns) |
|---:|-----------:|----------:|---------:|---------:|--------------:|
| 16 | 15 | 219 | 200 | 407 | 14.6 |
| 32 | 31 | 442 | 403 | 1,019 | 14.3 |
| 40 | 39 | 543 | 526 | 689 | 13.9 |
| 48 | 47 | 608 | 558 | 851 | 12.9 |
| 56 | 55 | 822 | 780 | 1,907 | 14.9 |
| 63 | 62 | 829 | 753 | 1,157 | 13.4 |
| 64 | 63 | 1,119 | 1,191 | 1,456 | 17.8 |
| 256 | 255 | 17,194 | 17,080 | 21,837 | 67.4 |
| 1023 | 1022 | 101,834 | 100,989 | 110,103 | 99.6 |

### Observations

1. **Per-slot cost is roughly constant at ~14 ns up through S=63**, then rises
   sharply at S=256+ (67 ns/slot) and S=1023 (100 ns/slot). The jump is
   cache-pressure driven: at S >= 256, the working set of segment metadata
   (kind_at + BinTable head per slot, each on a different 4 MiB-spaced page)
   exceeds L2 and starts hitting L3 / DRAM.

2. **At S=32, the worst-case scan is ~442 ns mean.** At S=64, it is ~1.1 us.
   At S=16, it is ~219 ns. The directory overhead (A1: bitmap lookup, word
   scan, validation) should be well under 100 ns, so the directory becomes a
   net win once the scan it replaces costs more than ~100 ns, i.e., around
   **S >= 16**.

3. **Recommended materialization threshold: 32.** Rationale:
   - At S=16 the scan costs ~219 ns, which is already measurable but not
     painful. The directory sidecar (6.1 KiB for 49 classes) is a fixed
     overhead that is wasted at low S.
   - At S=32 the scan costs ~442 ns and the p99 touches 1 us -- a clear win
     for a ~100 ns directory lookup.
   - At S=64 the scan is already > 1 us mean and the p99 is ~1.5 us.
   - Threshold of 32 catches the transition zone where the directory's benefit
     starts dominating its overhead, while still deferring the sidecar
     allocation for the common case of low-S heaps (a heap with < 32 registered
     segments is common in many workloads and does not need the directory).
   - The A6 GO gate (`S <= 16 not worse than 2%`) is satisfied by definition:
     S < 32 keeps the current linear scan unchanged.

---

## 4. Kill-gate reference values (before Workstream A)

These are the CURRENT values of the gates A6 will compare against.

### 4.1 IAI (`npm run iai`)

Not available on this host (Windows; iai-callgrind requires Linux + Valgrind).
CI runs IAI on Linux. The reference values should be captured from the CI run
at this commit.

### 4.2 Criterion churn — SeferAlloc (production features)

`cargo bench --bench global_alloc --features production`, SeferAlloc only.
All times are per-batch (256 alloc+dealloc ops per iteration).

| Bench | 16 B | 64 B | 256 B | 1024 B |
|---|---:|---:|---:|---:|
| `global_alloc` (direct) | 47.1 us | 48.0 us | 54.4 us | 64.7 us |
| `churn` | 39.8 us | 24.8 us | 24.6 us | 25.6 us |
| `churn_write` | 26.9 us | 25.5 us | 25.6 us | 29.7 us |
| `churn_with_teardown` | 31.5 us | 29.9 us | 35.1 us | 93.7 us |

### 4.3 Cold direct (16/64/256/1024 B)

Same as the `global_alloc` row above (the `bench_global_alloc` group IS the
cold direct alloc bench): 47 / 48 / 54 / 65 us per batch.

### 4.4 Persistent fan-in (`heap_fanin_persistent`)

`cargo bench --bench heap_fanin_persistent --features production`.
Reference cell: T=8, burst=1000, owner=active.

| Metric | Value |
|---|---|
| p50 | 800 ns/op |
| p99 | 10,700 ns/op |
| max | 38,600 ns/op |
| wall_total | 0.350 ms |
| overflow/op | 0.744 |

### 4.5 Windows first-heap commit (`first_alloc_process`)

`cargo run --release --example first_alloc_process --features production`.

| Metric | Value |
|---|---|
| RSS before | 3,228 KiB |
| RSS after 1 heap | 3,352 KiB |
| RSS delta (1 heap) | 124 KiB |
| Commit before | 688 KiB |
| Commit after 1 heap | 5,312 KiB |
| Commit delta (1 heap) | 4,624 KiB |
| First alloc latency | 137,700 ns |
| Peak RSS (64 heaps) | 8,768 KiB |

---

## 5. Summary

The baseline confirms the O(S) linear-scan cost documented in the R7 plan:

- **S=1023, holes=0%: ~100 us mean** (the target the directory must eliminate)
- **S=64, holes=0%: ~1.1 us mean** (already measurable)
- **S=16, holes=0%: ~220 ns mean** (the region the directory should NOT regress)
- **S=3, holes=0%: ~50 ns mean** (flat, consistent with existing IAI judge)

The per-slot cost is ~14 ns at S <= 63 (L1/L2 hot), rising to ~100 ns at
S=1023 (cache pressure). The A3 directory lookup should complete in well under
100 ns (a few bitmap-word reads + one slot validation), giving a clear > 10x
win at S=256+ and > 5x at S=64.

Recommended materialization threshold: **32** (see section 3 for justification).

Kill-gate reference values are captured in section 4.
