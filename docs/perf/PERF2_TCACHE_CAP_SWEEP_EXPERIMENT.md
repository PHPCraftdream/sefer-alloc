# X-arc PERF-2 honest-reject — TCACHE_CAP sweep (32 / 64 / 128)

**REJECT (all three candidates).** Sweeping `TCACHE_CAP` up from the default
16 to 32, 64, and 128 was measured against the 11-bench iai reference AND the
wall-clock `global_alloc` criterion bench (the cold-storm shape the experiment
targeted). Every candidate regressed **every** bench, including the explicit
targets (cold / recycle / the `global_alloc` storm), and the regressions grew
**super-linearly** with CAP. The "larger magazine amortizes refill/flush
orchestration on storm patterns" hypothesis (task #206 / PERF-2, from `fxx`
research) is **refuted by direct measurement** in both judges. CAP=32 reproduces
the X4-A reject (2026-07-05) within binary-layout noise; CAP=64 and CAP=128 are
strictly worse and never before measured. Final tree after PERF-2 = pristine
(zero diff to `src/`; this doc is the only new file).

Recorded per the project's reject-with-numbers precedent (X4/X5/X6 ledger
entries above), so the next reader does not re-run the same sweep blind.

## Setup and method

- **Source under test:** `src/registry/tcache.rs:48` (`pub(crate) const
  TCACHE_CAP: usize`). The constant governs (a) the physical `slots[c]` array
  size per class, (b) the small-class refill amount (via `refill_n_for_class`,
  which clamps `REFILL_BYTE_BUDGET / block_size` to `[1, TCACHE_CAP]`), and (c)
  `FLUSH_N = TCACHE_CAP / 2` (the half-flush hysteresis, auto-scales — no
  independent edit needed).
- **`REFILL_BYTE_BUDGET = 64 KiB` is CAP-independent.** For small classes
  (16 B / 64 B), `64 KiB / 16 = 4096 ≫ CAP`, so `refill_n = CAP` — bumping CAP
  directly scales the small-class refill batch size (the experiment's premise).
  For the largest small class (~253 KiB), `64 KiB / 253 KiB = 0 → clamp to 1`
  regardless of CAP, so the D3 byte budget keeps large-class refill RSS bounded
  independent of the raw CAP ceiling. **Bumping CAP alone is a coherent
  experiment; no companion constant needs adjusting.** (`FLUSH_N` scales
  automatically; the byte budget stays at 64 KiB.)
- **Judge 1 — iai (instruction count):** `npm run iai` (WSL + valgrind
  callgrind, `production` features, 11 bench fns in `perf_gate_iai.rs`).
  Deterministic Ir + cache columns (L1/L2/RAM/EstCycles). The cold/recycle iai
  benches use `COLD_BATCH = 256` (not the criterion bench's 1024), so at CAP=16
  a cold bench pays 256/16 = 16 refills — still a meaningful refill-storm
  signal, just smaller than `global_alloc`.
- **Judge 2 — wall-clock criterion:** `cargo bench --features production --bench
  global_alloc -- "global_alloc/"`. The `global_alloc` group (`bench_direct_alloc`)
  does `OPS = 1024` alloc-then-dealloc-all of distinct blocks through the
  `GlobalAlloc` face — the exact cold-storm shape the research hypothesis names.
  Windows wall-clock is noisy (sample_size 10), but the small-size sefer-vs-
  mimalloc gap is wide enough to read through the noise.
- **Per candidate:** temp-edit CAP → run iai → run `cargo test --features
  production` → revert. Each candidate measured against a clean baseline (no
  accumulation across candidates). `git diff` empty at the end (verified).

## Baseline (CAP=16) — measured 2026-07-07

`npm run iai` at the current `TCACHE_CAP = 16`. **Confirms the Post-X1+X2+X3
reference table** (the CURRENT 11-bench table, above) within binary-layout
noise: every Ir is within +27 (churn family) / +139 (cold) / +162 (recycle) of
the reference — a uniform <0.15 % drift consistent with the X3 note's
"adding/shifting a bench shifts every bench's Ir via pure binary layout." The
source under test is byte-identical to the reference's source (last touch to
`tcache.rs` is `62740d1`, pre-X-arc; the five commits since the reference are
all docs/CI). The drift is deterministic layout jitter from unrelated commits,
NOT a code change. The measured table below is the correct "current baseline"
to diff candidates against (same binary-layout environment for all four runs).

| bench                       |        Ir |    L1 hits | L2 hits | RAM hits | Est. Cycles |
| --------------------------- | --------: | ---------: | ------: | -------: | ----------: |
| small_churn_16b             |    81,423 |    142,745 |     161 |    5,219 |     326,215 |
| aligned_churn_640b_a128     |    81,439 |    142,771 |     159 |    5,219 |     326,231 |
| large_alloc_free_cycle      |    73,011 |    132,644 |     162 |    5,221 |     316,189 |
| realloc_grow                |   561,971 |  1,174,078 |   3,988 |   74,960 |   3,817,618 |
| cold_alloc_free_256x16b     |   125,354 |    195,737 |     171 |    5,325 |     382,967 |
| cold_alloc_free_256x64b     |   125,357 |    195,561 |     173 |    5,505 |     389,101 |
| recycle_alloc_free_256x16b  |   179,180 |    260,773 |     174 |    5,335 |     448,368 |
| recycle_alloc_free_256x64b  |   179,183 |    260,585 |     176 |    5,527 |     454,910 |
| churn_256b                  |    81,423 |    142,741 |     161 |    5,223 |     326,351 |
| churn_write_256b            |    81,551 |    142,997 |     161 |    5,223 |     326,607 |
| multiseg_cold_256k          |   111,662 |    189,164 |     189 |    5,517 |     383,204 |

Wall-clock `global_alloc` (criterion, Windows, median of [low..mean..high]):

| bench                     |   SeferAlloc |   mimalloc |    System |
| ------------------------- | -----------: | ---------: | --------: |
| global_alloc/…/16B        |   29.457 µs  |  11.789 µs | 178.47 µs |
| global_alloc/…/64B        |   57.785 µs  |  25.100 µs | 208.29 µs |
| global_alloc/…/256B       |   50.796 µs  |  34.751 µs | 207.80 µs |
| global_alloc/…/1024B      |   61.455 µs  |  68.225 µs | 305.14 µs |

Baseline gap vs mimalloc: sefer is **2.5× slower** at 16 B, 2.3× at 64 B, 1.5×
at 256 B, and ~even at 1024 B (sefer slightly ahead, within noise). This is the
gap PERF-2 aimed to narrow.

## Candidate A — CAP=32

| bench                       |   base Ir |   CAP=32 Ir |    Δ Ir |  Δ % |
| --------------------------- | --------: | ----------: | ------: | ---: |
| small_churn_16b             |    81,423 |     104,282 | +22,859 | +28% |
| aligned_churn_640b_a128     |    81,439 |     104,311 | +22,872 | +28% |
| large_alloc_free_cycle      |    73,011 |      91,830 | +18,819 | +26% |
| realloc_grow                |   561,971 |     584,079 | +22,108 |  +4% |
| cold_alloc_free_256x16b     |   125,354 |     151,117 | +25,763 | +21% |
| cold_alloc_free_256x64b     |   125,357 |     151,120 | +25,763 | +21% |
| recycle_alloc_free_256x16b  |   179,180 |     211,959 | +32,779 | +18% |
| recycle_alloc_free_256x64b  |   179,183 |     211,962 | +32,779 | +18% |
| churn_256b                  |    81,423 |     104,286 | +22,863 | +28% |
| churn_write_256b            |    81,551 |     104,414 | +22,863 | +28% |
| multiseg_cold_256k          |   111,662 |     130,481 | +18,819 | +17% |

**Reproduces X4-A within layout noise** (X4-A reported recycle +32,305 / churn
+22.3k / cold +25.3k; this run +32,779 / +22,863 / +25,763). Every bench
regressed, including the explicit targets. Mechanism (X4-A's, re-confirmed):
each refill/flush doubled in size (bigger carve/flush batches, larger `Tcache`
zero-init at heap claim, longer M2 in-magazine scan); the benches don't
refill-miss enough to amortize the larger batches. `cargo test --features
production`: all green except `tests/regression_tcache_byte_budget.rs::
small_class_refill_unaffected_by_byte_budget`, which hardcodes `const
TCACHE_CAP: usize = 16` (line 57) as a local mirror and asserts the literal —
a **test-infrastructure artifact, not an allocator correctness regression**
(the allocator correctly returns 32, the new full cap; the companion
byte-budget-clamp test `large_small_class_refill_bounded_by_byte_budget`
passes, proving the D3 clamp still engages for large classes at CAP=32). A
future real change to CAP would need to update that test's mirror constant.

## Candidate B — CAP=64

| bench                       |   base Ir |   CAP=64 Ir |    Δ Ir |  Δ % |
| --------------------------- | --------: | ----------: | ------: | ---: |
| small_churn_16b             |    81,423 |     137,456 | +56,033 | +69% |
| aligned_churn_640b_a128     |    81,439 |     137,511 | +56,072 | +69% |
| large_alloc_free_cycle      |    73,011 |     116,924 | +43,913 | +60% |
| realloc_grow                |   561,971 |     614,396 | +52,425 |  +9% |
| cold_alloc_free_256x16b     |   125,354 |     192,235 | +66,881 | +53% |
| cold_alloc_free_256x64b     |   125,357 |     192,238 | +66,881 | +53% |
| recycle_alloc_free_256x16b  |   179,180 |     268,129 | +88,949 | +50% |
| recycle_alloc_free_256x64b  |   179,183 |     268,132 | +88,949 | +50% |
| churn_256b                  |    81,423 |     137,468 | +56,045 | +69% |
| churn_write_256b            |    81,551 |     137,596 | +56,045 | +69% |
| multiseg_cold_256k          |   111,662 |     155,575 | +43,913 | +39% |

Strictly worse than CAP=32 on every bench (regression is monotonic in CAP).
`cargo test`: all green (same single test skipped, same reason).

## Candidate C — CAP=128

| bench                       |   base Ir |  CAP=128 Ir |     Δ Ir |   Δ % |
| --------------------------- | --------: | ----------: | -------: | ----: |
| small_churn_16b             |    81,423 |     206,070 | +124,647 | +153% |
| aligned_churn_640b_a128     |    81,439 |     194,036 | +112,597 | +138% |
| large_alloc_free_cycle      |    73,011 |     167,138 |  +94,127 | +129% |
| realloc_grow                |   561,971 |     672,944 | +110,973 |  +20% |
| cold_alloc_free_256x16b     |   125,354 |     277,193 | +151,839 | +121% |
| cold_alloc_free_256x64b     |   125,357 |     277,202 | +151,845 | +121% |
| recycle_alloc_free_256x16b  |   179,180 |     385,329 | +206,149 | +115% |
| recycle_alloc_free_256x64b  |   179,183 |     385,338 | +206,155 | +115% |
| churn_256b                  |    81,423 |     206,098 | +124,675 | +153% |
| churn_write_256b            |    81,551 |     206,226 | +124,675 | +153% |
| multiseg_cold_256k          |   111,662 |     205,797 |  +94,135 |  +84% |

**Super-linear regression.** The `Tcache` struct is now `49 × 128 × 8 B = 50.2
KiB` for `slots` alone (vs 6.27 KiB at CAP=16) — it spills L1, visible in the
L2-hit column jumping from ~160 (CAP=16) to ~1000 (CAP=128): the magazine
metadata itself stopped being L1-resident. `cargo test`: all green (same single
test skipped).

### Wall-clock confirmation at CAP=128 (the decisive signal)

`cargo bench --features production --bench global_alloc -- "global_alloc/"` at
CAP=16 vs CAP=128. The `global_alloc` group is the **exact storm shape** the
research hypothesis targeted (1024 distinct-block allocs, then 1024 frees).
CAP=128 makes sefer **dramatically worse** on every size, and the gap vs
mimalloc **widens** instead of narrowing:

| bench                  | CAP=16 sefer | CAP=128 sefer |   Δ wall |   mimalloc | gap@16 | gap@128 |
| ---------------------- | -----------: | ------------: | -------: | ---------: | -----: | ------: |
| global_alloc/…/16B     |   29.457 µs  |    95.697 µs  |  +224 %  |  11.789 µs | 2.50×  |  **4.9×** |
| global_alloc/…/64B     |   57.785 µs  |   128.66 µs   |  +123 %  |  25.100 µs | 2.30×  |  **5.1×** |
| global_alloc/…/256B    |   50.796 µs  |   136.55 µs   |  +169 %  |  34.751 µs | 1.46×  |  **3.9×** |
| global_alloc/…/1024B   |   61.455 µs  |   135.45 µs   |  +120 %  |  68.225 µs | 0.90×  |  **2.0×** |

(The `change:` field on the CAP=128 run vs the CAP=16 criterion cache reported
+14 % to +50 % across rows — criterion's own statistical signal, independent of
the absolute medians above.) This refutes the core premise: at 1024 ops /
CAP=128 = 8 refills per iteration, the per-refill cost grew faster (8× larger
carve batch + L1-spill) than the refill count shrank (64 → 8). The storm
hypothesis's arithmetic ("1024/16 = 64 refills → 1024/128 = 8 refills, an 8×
amortization win") is overwhelmed by the per-refill cost growth. **mimalloc's
advantage is NOT a deeper magazine — it is a structurally cheaper refill
(`mmap`/page free list with no per-refill orchestration equivalent), which a
larger CAP cannot replicate and in fact punishes.**

## Other-bench regression check (the research hypothesis's predictions)

- **`churn_256b` / `small_churn_16b` (working-set-bounded reuse):** the research
  predicted these should be CAP-insensitive "as long as CAP doesn't exceed the
  working set size." **REFUTED by measurement** — churn regressed monotonically
  and steeply at every CAP (small_churn_16b: +28 % / +69 % / +153 %). The
  reasoning missed that the *first* alloc of each bench iteration triggers a
  full refill (magazine starts empty), and a larger CAP means a larger refill
  batch (more carve work) + a larger `Tcache` zero-init at heap claim. The
  "working set fits in CAP" argument ignores the refill *cost*, which scales
  with CAP. Churn is NOT CAP-insensitive; it is CAP-*monotonic*.
- **`large_alloc_free_cycle`:** regressed too (+26 % / +60 % / +129 %) despite
  doing NO small-block magazine work — pure binary-layout + larger `Tcache`
  zero-init at heap claim (the `Tcache` is heap-claim-time zero-initialized
  regardless of whether the bench uses it). This is the cleanest decomposition
  of the "fixed cost per heap claim" component of the regression.

## RSS / worst-case-parking math (per candidate CAP)

Two distinct RSS effects, only one of which is CAP-sensitive:

1. **Refill-time parked bytes (D3 byte-budget clamp):** `refill_n_for_class` is
   `clamp(64 KiB / block_size, 1, CAP)`, so for any class where `block_size >
   64 KiB / CAP`, the clamp engages and refill parks ≤ `64 KiB` per class per
   thread **independent of CAP**.
   - CAP=16: clamp engages for `block_size > 4 KiB`.
   - CAP=32: clamp engages for `block_size > 2 KiB`.
   - CAP=64: clamp engages for `block_size > 1 KiB`.
   - CAP=128: clamp engages for `block_size > 512 B`.
   - **The refill-time RSS ceiling is unchanged (64 KiB / class / thread) at all
     four CAP values** — the D3 byte budget does its job. This is NOT a
     CAP-driven risk.
2. **`Tcache` struct footprint per thread (the real CAP-driven cost):**
   `slots: [[*mut u8; CAP]; 49]` + `count: [u16; 49]`.
   - CAP=16:  49 × 16 × 8 + 49 × 2 = **6.27 KiB + 98 B ≈ 6.4 KiB/thread** (L1-resident).
   - CAP=32:  49 × 32 × 8 + 98 B ≈ **12.6 KiB/thread** (L1-resident).
   - CAP=64:  49 × 64 × 8 + 98 B ≈ **25.1 KiB/thread** (L1-pressure).
   - CAP=128: 49 × 128 × 8 + 98 B ≈ **50.2 KiB/thread** (spills L1 — observed:
     L2 hits 161 → 998 on small_churn_16b).
   The struct is zero-initialized at heap claim and resident for the thread's
   lifetime. At CAP=128 a single thread's `Tcache` is ~50 KiB before it
   allocates a single block — and it no longer fits in a 32 KiB L1d, which is
   the mechanism behind the super-linear Ir regression (magazine push/pop start
   missing L1). The 50 KiB is *physical RSS per thread* the moment a heap is
   claimed; for a server with many threads this is `N_threads × 50 KiB` of
   metadata alone (vs 6.4 KiB at CAP=16).
3. **Worst-case full-magazine RSS (push-side accumulation to CAP):** a magazine
   can hold up to `CAP` blocks of its class before an overflow flush. For the
   largest small class (~253 KiB): CAP=16 → ≈3.95 MiB; CAP=128 → ≈31.6 MiB per
   class per thread. This is gated by the refill path (which only refills
   `min(64KiB/bs, CAP)`), so reaching a full CAP-deep mag of large blocks needs
   many frees without draining allocs — unusual but not impossible. For small
   classes (16 B) it is negligible (CAP=128 → 2 KiB). **This is the one RSS
   ceiling that genuinely rises with CAP**, but only in the large-class +
   free-heavy-without-alloc corner.

## Verdict

**No candidate is a win, mixed trade-off, or even marginal.** All three are
uniform regressions on all 11 iai benches AND on the wall-clock storm shape the
experiment existed to improve. The regression is monotonic and super-linear in
CAP. The research hypothesis ("larger magazine amortizes refill/flush
orchestration on storm patterns") is refuted in both judges: at CAP=128 the
`global_alloc/16B` sefer-vs-mimalloc gap *widened* from 2.5× to 4.9× instead of
narrowing. mimalloc's advantage is a structurally cheaper refill (page free
list, no per-refill orchestration), not a deeper magazine — a larger CAP cannot
replicate it and actively punishes the refill path. The companion predictions
(churn CAP-insensitivity; refill amortization) also failed against measurement.

**Recommendation: honest-reject.** Do not pursue a CAP bump as a real change.
The cold-first-touch gap vs mimalloc has a different cause (likely the
per-refill orchestration cost itself — `find_segment_with_free` /
`drain_freelist_batch` / latch logic — not the *frequency* of refills). The
shape that could win is a **cheaper refill**, not a **rarer refill**: e.g.
batching the `find_segment_with_free` scan, or hoisting work out of the
per-refill path (the X5 per-class segment-queue idea, honest-rejected at n=3
segments but structurally sound at large n, is the right *family* of attack —
reduce per-refill cost, don't enlarge the batch). The CAP parameter is already
at its optimum (16); this sweep confirms X4-A's verdict and extends it to the
two never-before-measured values (64, 128), which are strictly worse.

Final tree after PERF-2 = pristine (zero diff to `src/`; this doc is the only
new file).
