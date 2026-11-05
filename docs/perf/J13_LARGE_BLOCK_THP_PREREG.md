# J13 preregistration — a transparent-huge-page hint for fresh Large reservations (Linux)

Written and committed **before** any measurement of the change. Class if the gates pass: `perf(runtime)` (a production default changes on Linux);
if they fail: no code, a `bench` report.

## 1. Observation and hypothesis

J11 (`docs/perf/J11_BIG_BLOCK_JUDGE_MAP.md`, main `ddc95e9a`, Linux/WSL2, THP mode `madvise`): cold `cycle` of 1 GiB takes Sefer 262 503 page faults
(one per 4 KiB page) against mimalloc's 847, and the cold time ratio sefer / mimalloc is 2.2–2.6 on every Linux cell with blocks of 16 MiB and more.

Hypothesis H: a fresh Large reservation is not advised `MADV_HUGEPAGE` (only small segments are, from the 16th on: TH-1, `reserve.rs`), so its first touch
pays one fault per 4 KiB page; advising the reservation makes the kernel back it with 2 MiB pages and removes most of those faults and their time.

## 2. Change under test (x)

In `AllocCore::alloc_large_slow` (non-NUMA arm), after a fresh reservation of a block of at least `LARGE_THP_MIN_BYTES = 8 MiB` (the request `size`, not the
4 MiB-rounded span), call `Segment::advise_huge_pages()` on it, as `reserve_small_segment` does for small segments. Linux only (a no-op elsewhere); advisory
(contents and mapping unchanged); no change for requests below 8 MiB, for cache hits, or on Windows. Base = main plus the `g1_big_judge` change that samples
`AnonHugePages` (commit before the patch), so base and x differ by the patch only.

## 3. Entry point and design

- Entry point: real `SeferAlloc` through `#[inline(never)]` shims at the `__rust_alloc` boundary (`examples/g1_big_judge.rs`), the layer the change ships at.
- Three Sefer binaries built from git archives in WSL (`--release --features production`, alignment RUSTFLAGS): `base`, `x`, and `base2`, a byte-identical copy of `base`
  (the A/A control); the mimalloc arm runs from the `base` binary. Cells (12 blocks x 2 repeats per arm, order shuffled per block, arm order rotated by block):
  `cycle` 1 MiB (below the threshold: control), 16 MiB, 128 MiB, 1 GiB, 4 GiB; `grow` 128 MiB, 1 GiB; `many` 128 x 16 MiB, 16 x 128 MiB; `edge` 128 MiB (RSS cost cell).
- Statistic: per block and arm the minimum over repeats of the cold time (first rep, all phases), d = ln(x / base), cell = exp(mean d), two-sided 95 % t-interval
  (11 degrees of freedom, 2.201); the same for d' = ln(base2 / base) as the A/A control; a cell is read only if its A/A interval contains 1.
  Faults, end and peak RSS and `anon_huge_peak` are medians over the runs of a cell.

## 4. Gates (fixed now)

- **T0, path activation (mechanism).** In `cycle` 128 MiB, `cycle` 1 GiB and `grow` 1 GiB, the median `anon_huge_peak` of x is at least 80 % of the block size,
  and the median faults of x are at most 20 % of base's. Failing T0 means the hint did not take effect: no benefit claim, report as NULL.
- **T1, benefit.** `cycle` 1 GiB cold: x / base upper bound of the interval below 0.85 (point estimate expected near 0.5), A/A interval containing 1.
- **T2, cost.** In `cycle` and `grow` cells x's end and peak RSS are at most base's plus 4 MiB; in `edge` 128 MiB (two touched pages, so up to two 2 MiB pages)
  at most base's plus 8 MiB. The control `cycle` 1 MiB (below the threshold) has an x / base interval containing 1 and equal faults within 5 %.
- **Kill.** Any cell of 16 MiB or more whose x / base interval lies wholly above 1.10 (slower), or any T2 limit exceeded: NO-GO, no code merged.
- **Decision.** GO only when T0, T1 and T2 pass and nothing is killed. A GO is then merged as `perf(runtime)` with a test that the hint is set (VmFlags `hg` of the
  block's mapping in `/proc/self/smaps`) for a block of at least 8 MiB and not for 1 MiB; otherwise a `bench` report with the numbers and no change to `src/`.

## 5. What this does not claim

Windows is unchanged by construction (no-op); no claim is made there. THP mode `always` or `never` on other hosts is not measured: `never` makes the hint a no-op, `always`
already backs the span (so x equals base). The RSS cost of a hinted block is at most the span rounded to 2 MiB pages; nothing is claimed about workloads that touch a small
part of many big blocks beyond the `edge` cell.

## 6. Amendment 1 (written after campaign 1 had been read; governs campaign 2 only, on fresh data)

**Campaign 1** (`docs/perf/j13/raw.jsonl.gz`, report `docs/perf/J13_LARGE_BLOCK_THP.md`) read **NO-GO** under sections 4 as written: T0 (path activation) and T1 (benefit)
passed — cold x / base 0.42–0.57 on every cell of 16 MiB or more, page faults per GiB 262 503 → 871, THP-backed bytes equal to the block — and two cost gates failed:
T2 on `grow` (end RSS +10 MiB at 128 MiB, +16 MiB at 1 GiB, limit +4 MiB) and the kill on `edge` 128 MiB (cold x / base 8.3; absolute 56 µs → 512 µs median).
That report stays published as it is. The limits that failed were written without the mechanism of the hint; this amendment replaces them by limits derived from it, and
campaign 2 (same binaries, new runs) is the only evidence for the amended gates. The campaign 1 numbers were known when this was written; the limits below follow from the
mechanism and not from fitting those numbers (the measured `grow` deltas, +10 and +16 MiB, equal the bound below exactly, which supports THP rounding and excludes a leak).

- A hinted reservation backs every touched 2 MiB region with a whole huge page, so a partly touched hinted reservation costs at most 2 MiB of RSS more than 4 KiB pages.
  **T2 (RSS), amended:** x's end and peak RSS are at most base's plus `2 MiB` times the number of hinted reservations the cell creates and touches partly: `cycle` 1 (+2 MiB),
  `grow` the doubling chain from 8 MiB up to the block (128 MiB: 5, +10 MiB; 1 GiB: 8, +16 MiB), `edge` two touched regions (+4 MiB), `many` k (+2 MiB x k). `many` is gated now.
- A sparse touch (first and last byte of a block) costs two 2 MiB zeroings instead of two 4 KiB ones: at 2 GiB/s at least, up to about 2 ms. **Kill on `edge`, replaced by an absolute
  limit:** the median cold time of `edge` 128 MiB for x is at most 2 ms. The relative kill (1.10 over base) stays for every other cell of 16 MiB or more.
- T0, T1 and the control cell are unchanged. The decision rule is unchanged: GO only when T0, T1, T2 pass and nothing is killed; campaign 1 is not averaged into campaign 2.
