# R7 Workstream C1 -- TCACHE_CAP re-sweep (16 / 32 / 64)

**Date:** 2026-07-17
**Base revision:** `e53f1f7` (HEAD of `main`, post-R7 plan)
**Platform:** Windows 10 Pro x86-64, 16 threads, Rust release profile
**Harness:** `npm run bench:table` (criterion wall-clock), `npm run iai`
(deterministic Ir via WSL + callgrind), `node scripts/first-alloc-bench.mjs`
(process-per-sample commit/RSS), nightly `-Zprint-type-sizes` (compile-time
struct sizing)

---

## 1. Motivation

The prior NO-GO (2026-07-07, `PERF2_TCACHE_CAP_SWEEP_EXPERIMENT.md`) predates
MagazineBitmap / RAD-5 (2026-07-12), which removed the O(count) M2
in-magazine duplicate scan that penalized larger caps. The R7 plan (C1)
mandates a re-sweep to test whether the changed cost model makes a higher
TCACHE_CAP viable.

## 2. What TCACHE_CAP controls

`TCACHE_CAP` (`src/registry/tcache.rs:48`) sizes the physical `slots[c]`
array in each `PerClass`, which appears 49 times (once per small class) in
every `Tcache`, inside every `HeapCore`, inside every `HeapSlot`, inside each
64-slot `RegistryChunk`. It also controls:

- `FLUSH_N = TCACHE_CAP / 2` (auto-scales, no independent edit)
- `refill_n_for_class = clamp(REFILL_BYTE_BUDGET / block_size, 1, TCACHE_CAP)`
  -- small classes (16 B) refill at exactly CAP; large small-classes clamp
  below CAP via the byte budget (64 KiB, unchanged)

## 3. Compile-time type sizes (nightly `-Zprint-type-sizes`)

| Type | CAP=16 | CAP=32 | CAP=64 |
|---|---:|---:|---:|
| `PerClass` | 136 B | 264 B | 520 B |
| `Tcache` (49 classes) | 6,664 B | 12,936 B | 25,480 B |
| `HeapCore` | 7,320 B | 13,592 B | 26,136 B |
| `HeapSlot` (align 64) | 8,256 B | 14,528 B | 27,072 B |
| `RegistryChunk` (64 slots) | 528,384 B (516 KiB) | 929,792 B (908 KiB) | 1,732,608 B (1,692 KiB) |
| **Delta per HeapSlot** | -- | **+6,272 B** | **+18,816 B** |
| **Delta per chunk (64 slots)** | -- | **+392 KiB** | **+1,176 KiB** |

## 4. First-chunk commit guard (process-per-sample, 10 samples each)

| Metric (median) | CAP=16 | CAP=32 | CAP=64 |
|---|---:|---:|---:|
| Commit Delta 1 heap | 4,624 KiB (4.52 MiB) | 5,032 KiB (4.91 MiB) | 5,844 KiB (5.71 MiB) |
| **Delta from baseline** | -- | **+408 KiB (+8.8%)** | **+1,220 KiB (+26.4%)** |
| RSS Delta 1 heap | 128 KiB | 136 KiB | 172 KiB |
| RSS Delta 8 heaps | 814 KiB | 976 KiB | 1,312 KiB |
| RSS Delta 64 heaps | 5,562 KiB (5.43 MiB) | 6,758 KiB (6.60 MiB) | 9,174 KiB (8.96 MiB) |
| First-alloc latency (median) | 109.4 us | 96.3 us | 140.4 us |

**Workstream B (R7-B) targets a REDUCTION of first-heap commit from 4.52 MiB
to <=0.9 MiB.** CAP=32 grows it to 4.91 MiB (+8.8%), CAP=64 grows it to
5.71 MiB (+26.4%) -- both move in the WRONG direction. CAP=64 adds 1.15 MiB
to the first chunk, exactly as the R7 plan predicted ("cap=64 adds ~1.2 MiB to
the first chunk; if it eats the R6 commit-charge win, NO-GO regardless of a
small wall-clock win").

## 5. IAI (deterministic Ir) -- the authoritative judge

Ir/op = (Ir - bootstrap_proxy) / ops, marginal instruction count per
allocator operation. Deterministic run-to-run, comparable across CAP values.

| Bench | CAP=16 Ir/op | CAP=32 Ir/op | CAP=64 Ir/op | CAP=32 delta | CAP=64 delta |
|---|---:|---:|---:|---:|---:|
| small_churn_16b | 74.3 | 84.1 | 102.9 | **+13.2%** | **+38.5%** |
| churn_256b | 74.3 | 84.2 | 103.1 | **+13.3%** | **+38.8%** |
| churn_write_256b | 78.3 | 88.2 | 107.1 | **+12.6%** | **+36.8%** |
| cold_alloc_free_256x16b | 186.0 | 173.9 | 162.8 | -6.5% | -12.5% |
| cold_alloc_free_256x64b | 186.0 | 173.9 | 162.8 | -6.5% | -12.5% |
| recycle_alloc_free_256x16b | 187.7 | 175.6 | 162.4 | -6.4% | -13.5% |
| recycle_alloc_free_256x64b | 187.7 | 175.7 | 162.4 | -6.4% | -13.5% |

**Raw Ir (total, not per-op):**

| Bench | CAP=16 Ir | CAP=32 Ir | CAP=64 Ir | CAP=32 delta | CAP=64 delta |
|---|---:|---:|---:|---:|---:|
| small_churn_16b | 34,051 | 60,789 | 87,800 | +78.5% | +157.8% |
| large_alloc_free_cycle (bootstrap) | 29,299 | 55,405 | 81,216 | +89.1% | +177.2% |
| cold_alloc_free_256x16b | 76,910 | 99,926 | 122,897 | +29.9% | +59.8% |
| realloc_grow | 518,611 | 548,440 | 580,267 | +5.8% | +11.9% |

**Estimated Cycles (callgrind cache simulation):**

| Bench | CAP=16 | CAP=32 | CAP=64 |
|---|---:|---:|---:|
| small_churn_16b | 84,975 | 171,626 | 272,043 |
| churn_256b | 85,009 | 171,628 | 272,099 |
| cold_alloc_free_256x16b | 141,278 | 223,437 | 317,874 |

Churn estimated cycles more than doubled at CAP=32 and tripled at CAP=64,
driven by L1 misses on the larger PerClass struct.

## 6. Wall-clock criterion (noisy, directional only)

SeferAlloc ns/op, `sample_size(10)`, Windows host. Noisy -- interpret as
directional signal only; IAI (section 5) is the authoritative judge.

### Cold-direct (1 alloc + 1 free per op, no reuse)

| Size | CAP=16 | CAP=32 | CAP=64 | CAP=32 delta | CAP=64 delta |
|---|---:|---:|---:|---:|---:|
| 16B | 29.3 | 29.4 | 29.5 | +0.3% | +0.7% |
| 64B | 30.3 | 29.7 | 31.6 | -2.0% | +4.3% |
| 256B | 31.2 | 32.3 | 31.0 | +3.5% | -0.6% |
| 1024B | 31.7 | 34.3 | 34.0 | +8.2% | +7.3% |

### Churn (1 free + 1 alloc per op, working-set reuse)

| Size | CAP=16 | CAP=32 | CAP=64 | CAP=32 delta | CAP=64 delta |
|---|---:|---:|---:|---:|---:|
| 16B | 26.2 | 14.9 | 15.4 | -43.1% | -41.2% |
| 64B | 24.0 | 14.7 | 15.9 | -38.8% | -33.8% |
| 256B | 25.1 | 16.1 | 16.6 | -35.9% | -33.9% |
| 1024B | 27.6 | 16.8 | 16.0 | -39.1% | -42.0% |

### Write-churn (1 free + 1 alloc + 16B write per op)

| Size | CAP=16 | CAP=32 | CAP=64 | CAP=32 delta | CAP=64 delta |
|---|---:|---:|---:|---:|---:|
| 16B | 16.2 | 15.4 | 16.0 | -4.9% | -1.2% |
| 64B | 15.7 | 16.5 | 16.2 | +5.1% | +3.2% |
| 256B | 16.1 | 17.7 | 17.7 | +9.9% | +9.9% |
| 1024B | 18.9 | 21.1 | 19.4 | +11.6% | +2.6% |

**Caveat -- wall-clock vs IAI discrepancy on churn.** The wall-clock churn
bench shows a dramatic improvement (~40%) at CAP=32, while IAI's deterministic
Ir/op shows a +13% regression for the same bench shape. The wall-clock
result is unreliable here: criterion `sample_size(10)` on a noisy Windows
host with thermal throttling and background load. The IAI Ir count is
deterministic and reproducible. The run-to-run "change" percentages in the
criterion output show several +45%..+85% swings for mimalloc and System on
the same host during this session, confirming heavy noise. **The IAI judge
governs the GO/NO-GO verdict; wall-clock is supplementary signal.**

## 7. GO/NO-GO verdict per criterion

### CAP=32

| Gate | Criterion | Measured | Verdict |
|---|---|---|---|
| Cold direct >= -10% | Cold Ir/op: -6.5% (improved) | PASS |
| Churn not worse than 2% | Churn Ir/op: **+13.2%** | **FAIL** |
| First-heap commit not up beyond agreed limit | +408 KiB (+8.8%) | FAIL (moves opposite to B target) |
| No growth of remote/magazine correctness surface | Same code, same surface | PASS |

**CAP=32: NO-GO.** Churn Ir/op +13% hard-fails the <=2% gate. First-chunk
commit grows by 392 KiB, opposing Workstream B's reduction goal.

### CAP=64

| Gate | Criterion | Measured | Verdict |
|---|---|---|---|
| Cold direct >= -10% | Cold Ir/op: -12.5% (improved) | PASS |
| Churn not worse than 2% | Churn Ir/op: **+38.8%** | **FAIL** |
| First-heap commit not up beyond agreed limit | +1,220 KiB (+26.4%) | **FAIL** (1.15 MiB growth) |
| No growth of remote/magazine correctness surface | Same code, same surface | PASS |

**CAP=64: NO-GO.** Churn Ir/op +39% catastrophically fails the <=2% gate.
First-chunk commit grows by 1.15 MiB, exactly as the R7 plan predicted,
eating into the R6 commit win.

## 8. Root cause analysis (why the re-sweep confirms the prior reject)

The RAD-5 MagazineBitmap removed the O(count) in-magazine duplicate scan from
the dealloc path, which was the old rationale for why larger caps were
expensive. However, the dominant cost at larger CAP is NOT the M2 scan but
the **struct size scaling of `PerClass`**: at CAP=32, each `PerClass` grows
from 136 B to 264 B (nearly 2x), and `Tcache` from 6.6 KiB to 12.9 KiB.
This causes:

1. **More instructions for zero-init** of the larger magazine arrays on heap
   bootstrap (HeapCore::new) -- the bootstrap Ir proxy grows +89% (CAP=32).
2. **Wider cache footprint per class** -- a magazine hit/push/pop that
   previously fit in one cache line now spans two, increasing L1 misses.
   EstimatedCycles doubles at CAP=32 for churn.
3. **First-chunk commit growth** -- 49 classes x 64 slots x (delta per
   PerClass) = significant commit charge per registry chunk.

The M2 scan removal was necessary but NOT sufficient: the structural cost of
larger PerClass arrays dominates. **The prior reject stands.**

## 9. "Flush-incoming-directly-when-full" policy variant

The R7 plan (C1) optionally includes a "flush incoming directly (no cap
increase)" policy. This variant would not change TCACHE_CAP (keeping the
struct-size cost at zero) but instead, on a full magazine, would flush the
incoming block directly back to the segment free list rather than triggering
a half-flush. This is a dealloc-path policy change, not a cap change.

**Not tested in this sweep** because:
- The variant does not change TCACHE_CAP, so the struct-size and commit-charge
  gates are trivially PASS (no change).
- The policy is an independent experiment (different hypothesis: "avoid
  half-flush overhead on overflow") that does not interact with the cap sweep.
- Measuring it properly requires a separate sweep with its own IAI baseline.
- The churn bench's tight push-pop loop would make this variant strictly worse
  (every overflow incurs a full flush-one instead of an amortized half-flush),
  so it is unlikely to pass the churn gate. If pursued, it should be a
  separate experiment.

## 10. Conclusion

**All candidates NO-GO. TCACHE_CAP stays at 16.**

The MagazineBitmap (RAD-5) did remove one cost factor (O(count) M2 scan),
but the dominant costs of a larger CAP -- struct-size scaling, zero-init
overhead, cache footprint, and first-chunk commit growth -- remain. The prior
reject (PERF2, 2026-07-07) is confirmed with fresh, post-RAD-5 data. The
re-sweep is a valid documented NO-GO with numbers.

## 11. Gate compliance

| Gate | Result |
|---|---|
| `cargo fmt --check` | PASS |
| `cargo clippy --all-targets -- -D warnings` | PASS |
| `cargo clippy --all-targets --features experimental -- -D warnings` | PASS |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| `cargo clippy --lib --features production -- -D warnings` | PASS |
| Production code change? | None (CAP restored to 16) |

No test files added, no production code changed. Zero diff to `src/`.
