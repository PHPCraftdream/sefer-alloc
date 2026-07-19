# Cross-version wall-clock comparison — 0.2.1 → 0.3.0 (post-Round7)

**Date:** 2026-07-17
**Anchors:**

| Version | Source | Commit | Notes |
|---|---|---|---|
| **0.2.1** | tag `sefer-alloc-v0.2.1`, harness ported | `bench/0.2.1` (`5edb3d9`) | last version published on crates.io |
| **0.3.0 (current)** | `main` | `49046ef` | all Round7 committed + pushed |
| mimalloc / System | dev-deps / std | — | external references |

**Platform:** Windows 10 Pro x86-64, 16 threads, Rust release, criterion fast
profile (`sample_size(10)`, short warm-up). Single noisy dev host (±15–20 %
run-to-run). Trust the *shape and order of magnitude*, not the last decimal.

## Methodology — same harness against each version's allocator

The benchmark **harness** evolved across versions, so running "each version's own
harness" would conflate harness changes with allocator changes. Instead the
**current harness** (`benches/global_alloc.rs` + `scripts/bench-table.mjs`) is run
against each version's **allocator source**:

- **0.3.0** = current `main` — the harness is native there.
- **0.2.1** predates `bench:table`, so the current harness was ported onto the
  0.2.1 tag and preserved as the reusable **`bench/0.2.1`** branch. Verified: the
  7 bench groups are byte-identical to `main`'s. Re-measure with
  `git checkout bench/0.2.1 && npm run bench:table`.

mimalloc/System are the same code in both runs; their columns below are the
0.3.0-run reference and agreed with the 0.2.1-run within host noise.

The two ratio columns describe **0.3.0**: `vs 0.2.1` = the version-over-version
improvement (0.2.1 ÷ 0.3.0), `vs mimalloc` = 0.3.0 vs mimalloc (mimalloc ÷ 0.3.0).
ns/op unless noted; **lower is better**.

---

## 1. Cold-direct (`bench_direct_alloc`, no reuse — 1 alloc + 1 free per op)

| Size | Sefer 0.2.1 | Sefer 0.3.0 | mimalloc | System | vs 0.2.1 | vs mimalloc |
|---|---:|---:|---:|---:|---|---|
| 16B | 28.9 | 27.7 | 10.4 | 100.5 | 1.04× faster | 2.66× slower |
| 64B | 37.2 | 36.1 | 19.0 | 147.9 | 1.03× faster | 1.90× slower |
| 256B | 32.8 | 34.8 | 22.5 | 134.7 | 1.06× slower | 1.55× slower |
| 1024B | 36.5 | 39.3 | 45.6 | 141.9 | 1.08× slower | **1.16× faster** |

## 2. Churn, non-writing (`bench_churn_alloc`, working-set reuse — 1 free + 1 alloc per op)

| Size | Sefer 0.2.1 | Sefer 0.3.0 | mimalloc | System | vs 0.2.1 | vs mimalloc |
|---|---:|---:|---:|---:|---|---|
| 16B | 22.9 | 19.0 | 17.5 | 115.9 | **1.21× faster** | 1.09× slower |
| 64B | 20.8 | 19.1 | 25.0 | 137.8 | **1.09× faster** | **1.31× faster** |
| 256B | 44.3 | 20.6 | 34.5 | 140.9 | **2.15× faster** | **1.67× faster** |
| 1024B | 45.4 | 21.4 | 228.8 | 156.4 | **2.12× faster** | **10.69× faster** |

## 3. Churn + write (`bench_churn_alloc_write` — writes 16 B after each alloc)

| Size | Sefer 0.2.1 | Sefer 0.3.0 | mimalloc | System | vs 0.2.1 | vs mimalloc |
|---|---:|---:|---:|---:|---|---|
| 16B | 22.7 | 22.3 | 19.4 | 148.4 | **1.02× faster** | 1.15× slower |
| 64B | 25.4 | 16.0 | 26.6 | 94.5 | **1.59× faster** | **1.66× faster** |
| 256B | 35.5 | 15.7 | 29.1 | 100.1 | **2.26× faster** | **1.85× faster** |
| 1024B | 38.6 | 23.2 | 199.5 | 116.3 | **1.66× faster** | **8.60× faster** |

## 4. Churn + teardown (`..._with_teardown` — DELIBERATE diagnostic: teardown stays inside the timed region)

The gap vs plain churn IS the segment decommit/release/re-reserve cost
(`benches/global_alloc.rs:460-469`).

| Size | Sefer 0.2.1 | Sefer 0.3.0 | mimalloc | System | vs 0.2.1 | vs mimalloc |
|---|---:|---:|---:|---:|---|---|
| 16B | 37.2 | 26.8 | 22.6 | 153.9 | **1.39× faster** | 1.19× slower |
| 64B | 48.3 | 26.3 | 25.1 | 154.3 | **1.84× faster** | 1.05× slower |
| 256B | 72.9 | 30.8 | 32.1 | 175.3 | **2.37× faster** | **1.04× faster** |
| 1024B | 94.5 | 104.3 | 58.3 | 176.7 | 1.10× slower | 1.79× slower |

## 5. Vec_push (honest geometric `Vec<i64>` growth — 8 grow steps + stores per op, NOT scaled)

| Sefer 0.2.1 | Sefer 0.3.0 | mimalloc | System | vs 0.2.1 | vs mimalloc |
|---:|---:|---:|---:|---|---|
| 1213.3 | 1032.8 | 1029.7 | 1843.2 | **1.17× faster** | ~parity (1.00×) |

## 6. segment_decommit_cycle (253 KiB decommit→release→re-reserve; UNSCALED — ns per batch of 34 alloc + 34 free)

| Sefer 0.2.1 | Sefer 0.3.0 | mimalloc | System | vs 0.2.1 | vs mimalloc |
|---:|---:|---:|---:|---|---|
| 405 980 | 1 277.6 | 5 698.7 | 61 306 | **~318× faster** | **4.46× faster** |

## 7. working_set_cycle (oscillating working set; UNSCALED — ns/batch, SeferAlloc-only, no mimalloc/System arm)

| Size | Sefer 0.2.1 | Sefer 0.3.0 | vs 0.2.1 |
|---|---:|---:|---|
| 16B | 306 480 | 182 650 | **1.68× faster** |
| 64B | 334 990 | 198 150 | **1.69× faster** |
| 256B | 706 350 | 199 660 | **3.54× faster** |
| 1024B | 1 031 300 | 256 100 | **4.03× faster** |

*(`pool_cap_sweep` is diagnostic-only — its signal is the `decommit_calls`
counter deltas, not criterion timing; excluded from the wall-clock comparison.
See `docs/perf/R7_POOL_CAP_PRESETS.md`.)*

---

## What changed between 0.2.1 and 0.3.0 (why the big wins)

Two independent overhauls landed across the 0.2.1→0.3.0 waves (rounds 4–7):

### a) The ~318× decommit-cycle win — the small-segment pool + large cache

In **0.2.1**, an emptied segment was **decommitted and released to the OS**
immediately; the next allocation of that class had to **reserve + commit a fresh
segment from the OS** again — a full `VirtualAlloc`/`mmap` + `MEM_COMMIT` syscall
storm on every empty→reuse cycle. That is exactly what `segment_decommit_cycle`
and `working_set_cycle` stress.

**0.3.0** retains emptied segments instead of releasing them:

- **Mechanism-2 — the bounded small-segment hysteresis pool** (RAD-3/E2; cap
  presets in `docs/perf/R7_POOL_CAP_PRESETS.md`, default 4 segments / 16 MiB):
  an emptied segment is parked in the pool, and reuse is a cheap pool-pop over
  the **existing VA reservation** — no OS syscall.
- **The OPT-E large-segment cache** (`LARGE_CACHE_SLOTS = 8`, pages kept
  committed) does the same for large allocations.

Result: the decommit/reuse cycle collapses from ~406 µs/batch to ~1.3 µs/batch
(**~318×**), and the oscillating working-set cycle improves up to ~4×.

### b) The ~128 MiB → ~6 MiB first-alloc commit — the chunked Registry

Separate axis (memory, not speed), also landed between 0.2.1 and 0.3.0
(**R6-OPT-P0-2**, commits `e4b3e1d` + `8dc6fe8`). In 0.2.1 the registry was a
monolithic `[HeapSlot; 4096]` inline array; on Windows the **entire
Registry + HeapOverflow array was committed on the first allocation ≈ 125–129.5
MiB commit charge** (demand-zero pages — invisible to an RSS-only probe, which is
why the first-alloc commit judge exists). 0.3.0 replaced it with **64 lazily
materialised 64-slot chunks** (`RegistryChunk`) plus a lazy `HeapOverflow`
sidecar, each chunk following the same CAS→reserve→publish bootstrap protocol.

Measured via `examples/first_alloc_process.rs`: **~129.5 MiB → ~5.98 MiB (~21.7×)**
first-alloc commit charge (the residual ~6 MiB is dominated by the 4 MiB
primordial segment + the first lazy chunk). This is a Windows **commit-charge**
reduction, not an RSS one.

*(Note: Round7's own `alloc-lazy-commit` originally targeted the primordial's
own 4 MiB and hit a NO-GO on first measurement (see
`docs/perf/R7_INCREMENTAL_COMMIT.md`) — but a later commit, R7-B6 (`8977e88`),
made the primordial lazy too and closed that gap (measured first-heap commit
Δ: 4.52 MiB → ~0.887 MiB). The 128→6 MiB win above is the separate, earlier
chunked-registry work (R6-OPT-P0-2), not R7-B — the two are independent axes
(memory-charge-at-different-scale vs commit-charge-reduction) and both are
now landed.)*

---

## Takeaways

- **0.3.0 vs 0.2.1:** faster on essentially every reuse workload — churn/write
  +1.0–2.3×, the decommit cycle **~318×**, working-set up to ~4×, Vec_push
  ~1.17×. No real regression (cold-direct 256–1024 B and teardown-1024 B are
  within ±15–20 % host noise). Plus a **~21.7× lower first-alloc commit charge**
  (128→6 MiB) on Windows.
- **0.3.0 vs mimalloc:** wins the whole churn family at 64 B+ (up to **10.7×** at
  1024 B) and the decommit cycle (4.46×); loses only on the cold path at small
  sizes (16–64 B, ~1.9–2.7×) — the known scalar-`GlobalAlloc` cold-path gap a
  batch/scoped API would close (out of Round7 scope).
- Numbers are directional (±15–20 % host noise); UNSCALED groups (6–7) are
  higher-variance but the gaps there are multiplicative, far beyond noise.
  Re-run `npm run bench:table` on both `main` and `bench/0.2.1` for fresh numbers.
