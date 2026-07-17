# Round7 — benchmark results (what we won + general comparison)

**Date:** 2026-07-17
**Base revision:** `49046ef` (HEAD of `main`, all Round7 committed + pushed)
**Platform:** Windows 10 Pro x86-64, 16 threads, Rust release profile
**Harnesses:**
- `benches/segment_directory_sweep.rs` — the refill-miss (find-segment-with-free)
  judge, run directory-OFF vs directory-ON (feature `alloc-segment-directory`).
- `npm run bench:table` (`scripts/bench-table.mjs` + `benches/global_alloc.rs`) —
  the canonical 3-arm wall-clock comparison (SeferAlloc vs mimalloc vs System),
  fixed ns/op units, fixed bench set.

**Host-noise caveat (read first).** Criterion runs the fast profile
(`sample_size(10)`, short warm-up/measurement) on a shared Windows host with
±15–20 % run-to-run wall-clock variance. Treat the ns/op numbers as *directional
ratios*, not precise measurements — the relative order of the three arms is
stable, the third decimal is not. The "Performance has regressed/improved"
verdicts in `bench:table`'s run-over-run appendix are almost all host noise (both
mimalloc and System "regressed" +20–46 % on the same run), not real changes.

---

## 1. What we won — the segment directory (Round7 Workstream A, **GO**)

The R7-A directory replaces the O(S) linear scan over all registered segments in
`find_segment_with_free` with an O(1)-ish per-class bitmap lookup (opt-in feature
`alloc-segment-directory`, materialised lazily at ≥32 segments). The sweep below
is the refill-miss judge at `holes=0%` (the worst case — the scan walks every
segment). "S" = number of registered segments; times are per find-segment call.

### class = 48 (`SMALL_MAX` = 258 752 B)

| S (segments) | OFF — linear scan | ON — directory | speedup |
|---:|---:|---:|---:|
| 1 | 25 ns | 21 ns | ~1.2× |
| 3 | 24 ns | 25 ns | ~parity |
| 16 | 78 ns | 54 ns | ~1.4× |
| 64 | 228 ns | 53 ns | **~4.3×** |
| 256 | 5 038 ns | 128 ns | **~39×** |
| 1023 | 67 515 ns | 400 ns | **~166×** |

### class = 25

| S | OFF | ON | speedup |
|---:|---:|---:|---:|
| 64 | 193 ns | 47 ns | ~4.1× |
| 256 | 3 555 ns | 123 ns | **~29×** |
| 1023 | 67 515 ns | 376 ns | **~180×** |

**Reading it.** The linear scan blows up O(S) — 67 µs per refill-miss at S=1023.
The directory collapses that to sub-microsecond (~0.4 µs), a **~166–180×** win at
high segment counts and **~29–39×** at S=256, while staying at **parity for S≤3**
(no regression on small heaps — the directory isn't even materialised there).
This confirms the A6 GO verdict. The feature is opt-in and off by default; the
guarded linear-scan fallback is retained as the correctness oracle and the
OOM-degradation path. Documented trade-off (from A6): at high remote-dirty
density the directory-drain-first path can be slower than OFF — the gate measures
dirty=0 %, and the feature is opt-in precisely so that workloads choose it.

---

## 2. General comparison — SeferAlloc vs mimalloc vs System

Canonical `npm run bench:table` output (ns per operation; lower is better). All
51 expected bench ids present, 78 parsed.

### Cold-direct (`bench_direct_alloc`, no reuse — 1 alloc + 1 free per op)

| Size | SeferAlloc | mimalloc | System | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 27.2 | 10.2 | 98.5 | 2.67× slower |
| 64B | 27.8 | 13.7 | 107.2 | 2.03× slower |
| 256B | 29.4 | 17.8 | 97.6 | 1.65× slower |
| 1024B | 30.6 | 34.5 | 109.4 | **1.13× faster** |

### Churn, non-writing (`bench_churn_alloc`, working-set reuse — 1 free + 1 alloc per op)

| Size | SeferAlloc | mimalloc | System | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 18.3 | 19.7 | 119.8 | **1.08× faster** |
| 64B | 18.9 | 25.6 | 124.7 | **1.35× faster** |
| 256B | 17.8 | 43.0 | 141.2 | **2.42× faster** |
| 1024B | 21.3 | 215.9 | 150.5 | **10.15× faster** |

### Churn + write (`bench_churn_alloc_write` — writes 16 B after each alloc)

| Size | SeferAlloc | mimalloc | System | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 16.7 | 15.9 | 91.4 | 1.05× slower |
| 64B | 15.7 | 18.7 | 93.0 | **1.19× faster** |
| 256B | 16.1 | 35.6 | 100.4 | **2.21× faster** |
| 1024B | 22.9 | 160.1 | 128.1 | **6.99× faster** |

### Churn + teardown (`..._with_teardown` — DELIBERATE diagnostic: teardown stays inside the timed region)

The gap vs plain `churn` at the same size IS the segment decommit/release/re-reserve
cost (`benches/global_alloc.rs:460-469`). SeferAlloc is intentionally slower here —
it does real decommit work that mimalloc's cache avoids in this micro-shape.

| Size | SeferAlloc | mimalloc | System | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 24.7 | 20.0 | 140.7 | 1.23× slower |
| 64B | 25.1 | 23.4 | 146.2 | 1.07× slower |
| 256B | 30.7 | 29.2 | 161.9 | 1.05× slower |
| 1024B | 99.2 | 56.4 | 126.3 | 1.76× slower |

### Vec_push (honest geometric `Vec<i64>` growth — 8 grow steps + stores per op, NOT scaled)

| SeferAlloc | mimalloc | System | Sefer vs mimalloc |
|---:|---:|---:|---:|
| 797.7 | 1083.3 | 1801.3 | **1.36× faster** |

### Additional groups (outside the canonical 4-table headline)

`segment_decommit_cycle` (253 KiB small-segment decommit→release→re-reserve;
ns per batch of 34 alloc + 34 free):

| SeferAlloc | mimalloc | System | Sefer vs mimalloc |
|---:|---:|---:|---:|
| 1232.4 | 5084.9 | 50415.0 | **4.13× faster** |

`working_set_cycle` (Mechanism-2 decommit/reuse judge, SeferAlloc-only, ns/batch):

| Size | SeferAlloc |
|---|---:|
| 16B | 184 280 |
| 64B | 180 090 |
| 256B | 192 560 |
| 1024B | 230 200 |

(`pool_cap_sweep` is diagnostic-only — its signal is the `decommit_calls` counter
deltas, not criterion timing; see `benches/global_alloc.rs` and
`docs/perf/R7_POOL_CAP_PRESETS.md`.)

---

## 3. Takeaways

- **The refill-miss win is the Round7 headline** — up to ~180× at high segment
  counts, parity below the materialisation threshold. Opt-in, fallback-guarded.
- **Steady-state churn is SeferAlloc's strength**: 1.08–10.15× faster than
  mimalloc (the advantage grows with block size — 10× at 1024 B), and 5–8×
  faster than the System allocator across the board.
- **Cold-direct (no reuse) at small sizes is the weak spot**: 2–2.7× slower than
  mimalloc at 16–64 B (mimalloc's cold path is extremely tuned), crossing over to
  faster at 1024 B. This is the known scalar-`GlobalAlloc` cold-path gap; a
  batch/scoped API (out of Round7 scope) is the path to close it.
- **Decommit-heavy micro-shapes** (`..._with_teardown`) are intentionally slower —
  SeferAlloc does real page reclaim there; on the honest full decommit cycle
  (`segment_decommit_cycle`) it is 4.13× faster than mimalloc.
- Numbers are directional (±15–20 % host noise); the arm ordering is what's
  stable. Re-run `npm run bench:table` on a quiet machine for publication figures.
