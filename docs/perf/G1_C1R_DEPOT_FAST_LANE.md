# G1 C1R: the WB-3 C1 depot fast lanes on top of the R123 free lane — `perf(runtime)`

Runtime change to plain `--features production`: the two magazine
boundaries of the `#[global_allocator]` path get the frameless depot
dispatchers of WB-3 C1 (`free_magazine_park` on a full magazine,
`refill_magazine_miss` on an empty one). C1 was measured on 2026-10-29 at
`100d4197` and rejected below the old threshold (card 136,
`docs/perf/WB3_C1_DEPOT_FAST_LANE_PREREG.md`). After R123 the depot is most
of what remains of the gap to mimalloc on warm bulk frees (D16: park/reload
0.58 of 0.69 ns per pair). The owner reopened C1 on 2026-11-01 and asked for
the magnitude rule as the judge. Verdict: **GO** in both paired sets, no
losing cell.

Entry point under test: the real `#[global_allocator]` path
(`SeferAlloc::alloc`/`dealloc` → `HeapCore` → magazine boundary), through
`malloc-bench-rs`'s `inline(never)` shims, the same boundary as
`__rust_alloc`/`__rust_dealloc`. C1 changes only that layer's cold calls.

## 1. Change

| Part | Before | After |
|---|---|---|
| C1 (cherry-pick of `100d4197`) | full magazine → `free_magazine_full`; empty magazine → `refill_magazine_slow` | `free_magazine_park` / `refill_magazine_miss` take the floor or the overflow ring's steady case (`DepotOverflow::park_simple` / `reload_simple`) with no call in between; every other case tail-calls the unchanged generic path |
| Joint with R123 | the AS-1 lane's `free_magazine_full_fast` called `free_magazine_full` directly | it calls `free_magazine_park`, so the lane reaches the fast park like the guarded push path |

Free-path rejects, the state-byte protocol and the depot's owner-only access
are unchanged; C1's Kani equivalences (И-F1/И-F2), its model test
`tests/wb3_depot_fast_lane_model.rs` and its two P12 ghost rules come with
the cherry-pick.

## 2. Instruction counts (callgrind, WSL)

Totals from `docs/perf/c1r/callgrind_totals.txt` (`g1_mt_one <w> sefer 1
300000`; `b4_pmu_probe bulk sefer 1024 <s> 400`).

| Workload | base | C1R | Δ |
|---|---:|---:|---:|
| mstress T=1 | 71,682,195 | 70,462,308 | −1.7 % |
| larson T=1 | 69,130,624 | 68,773,384 | −0.5 % |
| bulk 1024 × 16 B | 54,458,420 | 51,867,933 | −4.8 % |
| bulk 1024 × 64 B | 54,461,794 | 51,869,234 | −4.8 % |

## 3. Paired bench:table (Windows, judge of the magnitude rule)

Two sets, order base, x, x, base each, aligned code
(`-align-all-functions=6 -align-all-nofallthru-blocks=5`). Tables derived
by `node scripts/g1_magnitude_verdict.mjs docs/perf/c1r s{1,2}base s{1,2}c1r s{1,2}`;
the summary CSV `G1_C1R_DEPOT_FAST_LANE_summary.csv` is written by the same
script (`MAG_CSV`).

| Set | sefer time gm | wins / losses (pp) | largest loss | ratio to mimalloc gm | new LOSS cells | Verdict |
|---|---:|---:|---:|---:|---:|---|
| s1 | −1.8 % | 27.3 / 0.0 | +0.0 % | −0.3 % | 0 | GO |
| s2 | −2.1 % | 18.1 / 0.0 | +0.0 % | −2.0 % | 0 | GO |

Largest cells (sefer time, set 1 / set 2): direct 64 B −13.9 % / −7.8 %,
direct 256 B −4.0 % / −5.5 %, direct 16 B −4.1 % / +0.0 %, direct 1024 B
−3.4 % / −2.9 %. Still a loss against mimalloc: direct 16 B (sefer/mi
1.296 → 1.256 / 1.263 → 1.251). In set 1 the ratio column shows churn
1024 B +16.8 % in both pairs while sefer's own time there is −1.9 % / +7.9 %
(agreed 0): the mimalloc arm moved, not sefer.

## 4. MT control (malloc_macro, Windows)

ABBA × 6 (12 runs per arm), table by
`node scripts/as1_mt_table.mjs docs/perf/c1r/mt_malloc_macro_raw.txt` (the
script labels the second arm `as1`; here it is C1R). Higher is better.

| Workload | T | sefer before (M ops/s) | after | Δ | sefer/mi before | after |
|---|---:|---:|---:|---:|---:|---:|
| larson | 1 | 56.12 | 55.90 | −0.4 % | 0.937 | 0.964 |
| larson | 2 | 61.30 | 61.81 | +0.8 % | 1.292 | 1.278 |
| larson | 4 | 103.48 | 108.90 | +5.2 % | 1.366 | 1.349 |
| mstress | 1 | 64.50 | 66.50 | +3.1 % | 0.890 | 0.948 |
| mstress | 2 | 101.47 | 108.84 | +7.3 % | 1.054 | 1.092 |
| mstress | 4 | 154.28 | 147.41 | −4.5 % | 0.968 | 0.951 |

mstress T=4 is the one lower median. Its single runs span 119–205 M ops/s
for sefer in both arms (`mt_malloc_macro_raw.txt`), so the −4.5 % median
difference is inside the run-to-run spread; it is reported, not counted as
a win or a loss.

## 5. Identity

- Base: `main` at `f05ffb6763f058fb9d878309675abcb2d6d3b53f` (R123 landed).
- Measured tree: commit `d9b056637a961717bffc47f93332263cab9af108`
  (branch `g1-c1r`, tree `3aca44a075553e11d9aae29a9e53e2df1c769d46`),
  committed before the measurement.
- Linux lab binaries (sha256 prefix): `g1_mt_one` base `475b82a5f2e14e85`,
  C1R `e303236f92fc2b78`; `b4_pmu_probe` base `ff77cca32d2e0fe9`, C1R
  `3b7d5034e40f1073`.
- Host: Windows 10 x86_64 (bench:table, MT), WSL2 Ubuntu (callgrind).

## 6. Not done

- The overflow ring's park/reload still copies 16 slot pointers per event;
  a design without that copy is a separate candidate.
- The Win64 free lane still saves `rsi`/`rdi`. Three asm experiments
  (kind compare without `LANE_PS`, MAG write before the cap check, the
  header's `payload_offset` as the bound) did not remove them; none was
  measured, so no claim is made.
