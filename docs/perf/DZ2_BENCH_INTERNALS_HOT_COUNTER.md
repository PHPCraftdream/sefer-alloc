# DZ-2 — `bench-internals` hot-path counter: isolation and fix

`fix(perf)`, measurement-infrastructure fix. No `production` code or default
changed: the counters exist only under `bench-internals`.

## Question

BT-1 (`docs/perf/BT1_PAIRED_BENCH_TABLE.md`, OPEN_ITEMS 132) found the
`bench:table` build (`production internals bench-internals`) 1.3–3.4 ns/pair
slower than `production` for SeferAlloc and at most 0.6 ns slower for
mimalloc. Which feature causes it, and by what mechanism?

## Entry point and protocol

The real `#[global_allocator]` path, `SeferAlloc` vs `mimalloc::MiMalloc`, in
`examples/bench_table_paired.rs`, through `scripts/bench-table-paired.mjs`.
That harness rotates through all 6 arm orders, takes the per-process minimum
over 200 rounds, and reports the median over 3 processes. Builds use aligned
code (`-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5`)
on an i7-11800H under Windows 10. Each run's manifest records the commit,
dirty flag, exe sha256, rustc, CPU and OS.

- **Before:** commit `3782939c`, clean, four feature sets: `production`, `+ internals`,
  `+ bench-internals`, `+ internals bench-internals`.
- **After:** commit `9f8d1e62` (tree `2b6c8dee`). The manifests say
  `c4b1ceeb+dirty` because the binaries were built from the working tree
  just before that commit. The commit holds exactly that tree, nothing else.
  Two feature sets: `production` and `+ bench-internals`.

The `production internals bench-internals` run in `before/` was noisy
(spread of per-process minimums up to 109 %, all three arms). It is
kept as raw data but not used.

## Isolation

`internals` costs nothing (≤ 0.1 ns/pair in every cell). The whole
overhead belongs to `bench-internals`. Two follow-up measurements in WSL
(`docs/perf/dz2/wsl_before/`, Linux build of `examples/b4_pmu_probe`
`bulk sefer 1024 <16|64> <passes>`, same source tree):

- **callgrind:** instructions differ by +2.2 Ir/pair (157.0 → 159.2,
  +1.4 %). The overhead is not instruction count.
- **perf stat:** cycles go 1.77 G → 2.82 G (+59 %) at 16 B with the same
  +1.4 % instructions, which is ≈ 20.5 cycles/pair. perf report puts it
  all in `sefer_dealloc`: 42 % → 61 % of samples, while `sefer_alloc` keeps
  the same absolute cycles. perf annotate (`annotate_bi16_lock.txt`) shows
  `lock incq` on the free path at the memo-hit return of
  `SegmentTable::canonical_base_of_mut`: `CONTAINS_BASE_TIER1_HITS.fetch_add`
  (plus the own_cache-hit and miss sites). One locked RMW per free costs
  ≈ 20 cycles on this core, which is the measured delta.

Mechanism: `fetch_add(Relaxed)` still compiles to `lock inc` on x86. The
counter's doc said it "cannot … add overhead"; that held only for
`production`, not for the measurement builds.

## Fix

`counters::tier1_bump` — a Relaxed `load` + `store` (no `lock`), used at the
three Tier-1 sites. It is exact for a single thread and can lose counts only
under concurrent bumps from different threads. Every reader (7 integration
tests, 2 examples) serializes and reads its own delta on the same thread.
Those tests are the counterfactual: a bump that did nothing would turn them
red. The new ordering sites are claimed by inventory scope `counter/tier1-bump`.

## Result (derived by `node scripts/dz2_verdict.mjs docs/perf/dz2 docs/perf/DZ2_BENCH_INTERNALS_HOT_COUNTER_summary.csv`)

SeferAlloc ns/pair (cell = median over 3 processes of the per-process
minimum). Δ = `+ bench-internals` minus `production` in the same run.

| Cell | before: production | before: + internals | before: + bench-internals | Δ before | after: production | after: + bench-internals | Δ after | Δ removed |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| direct 16 B | 7.18 | 7.18 | 10.06 | 2.88 | 7.23 | 7.32 | 0.10 | 97 % |
| direct 64 B | 6.88 | 6.98 | 9.86 | 2.98 | 6.93 | 7.18 | 0.24 | 92 % |
| direct 256 B | 7.71 | 7.76 | 10.74 | 3.03 | 7.67 | 7.81 | 0.15 | 95 % |
| direct 1024 B | 10.01 | 10.06 | 13.72 | 3.71 | 9.96 | 10.25 | 0.29 | 92 % |
| churn 16 B | 5.66 | 5.66 | 7.57 | 1.90 | 5.66 | 5.76 | 0.10 | 95 % |
| churn 64 B | 5.66 | 5.57 | 7.47 | 1.81 | 5.62 | 5.76 | 0.15 | 92 % |
| churn 256 B | 5.71 | 5.66 | 7.52 | 1.81 | 5.71 | 5.81 | 0.10 | 95 % |
| churn 1024 B | 6.05 | 5.96 | 7.86 | 1.81 | 5.81 | 6.15 | 0.34 | 81 % |
| churn_write 16 B | 5.96 | 5.86 | 7.67 | 1.71 | 5.86 | 5.91 | 0.05 | 97 % |
| churn_write 64 B | 5.86 | 5.81 | 7.86 | 2.00 | 5.81 | 6.01 | 0.20 | 90 % |
| churn_write 256 B | 6.10 | 6.05 | 8.25 | 2.15 | 6.01 | 6.40 | 0.39 | 82 % |
| churn_write 1024 B | 6.54 | 6.35 | 8.89 | 2.34 | 6.40 | 6.93 | 0.54 | 77 % |

bench-internals overhead removed, summed over 12 cells: 25.49 of 28.12 ns/pair (91 %)

After the fix, the Linux perf stat (`wsl_after/fg.txt`) shows the
`bench-internals` build no longer above `production` (cycles 1.82–1.87 G
vs 2.05–2.15 G at 16 B). Instruction counts still differ by +2.2 Ir/pair,
so these unlocked bumps cost almost nothing. That run was noisier than the
before run (`production` 2.05 G vs 1.77 G), so the Linux cycle numbers
show the sign of the change, not its size.

## Remaining

`bench-internals` still costs 0.05–0.54 ns/pair (the Δ-after column). The
largest values are in the 1024 B churn+write and 256 B cells. Candidates are
the per-refill counters (`S1_REFILL_EVENTS`, `S1_MAG_BYTE_WRITES`; locked,
≈ 1 per 32 operations) or layout. One follow-up was measured and **not kept**: commit `5988bdcd` (tag
`archive/dz2-refill-bumps-measured`) also moved `S1_REFILL_EVENTS`,
`S1_MAG_BYTE_WRITES` and `R1_01_CAP_ROUTE_FREES` to the unlocked bump. Its
paired run (`docs/perf/dz2/after2/`; manifests say `dirty` only because the
report files were still untracked) left the residual unchanged: the
`bench-internals` overhead summed over the 12 cells was 2.98 ns/pair, against
2.64 in `after/`, so the difference is inside run-to-run noise. Those counters
fire about once per 32 operations; their locked RMW is not measurable here.
Whatever remains is left to FG-1 (flamegraph campaign), and only if it ever
affects a judge. Production numbers are unaffected.

## Reproduce

- Paired table: `node scripts/bench-table-paired.mjs --features "<set>" --raw <prefix> --csv <prefix>.csv`.
  Re-derive a summary from the raw data with `--from <prefix>`. The raw
  data is stored gzipped: unpack `*.jsonl.gz` first.
- WSL: `docs/perf/dz2/dz2_cg.sh` (callgrind, both builds) and `dz2_fg.sh` (perf
  stat, perf record/flamegraph, top symbols). Set `REPO` to the repo root as
  seen from WSL, `PERF` to the perf binary, and provide the source as
  `git archive -o .rush/wsl/dz2_tree.tar <commit>`.
