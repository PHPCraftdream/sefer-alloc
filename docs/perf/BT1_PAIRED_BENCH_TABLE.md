# BT-1: paired short-sample `bench:table` (measurement only)

No runtime code changed. This report adds a second way to produce the canonical
SeferAlloc / mimalloc / System table and records what it shows next to the
criterion table (`npm run bench:table`).

## Method

`examples/bench_table_paired.rs`, driven by `scripts/bench-table-paired.mjs`
(`npm run bench:table:paired`).

- Entry point: each allocator's `GlobalAlloc` impl called directly
  (`SeferAlloc` static, `mimalloc::MiMalloc`, `std::alloc::System`), the same
  layer `benches/global_alloc.rs` measures and the README table reports.
  None is the process allocator; this is not the `#[global_allocator]`
  judge (J1).
- Bodies: those of `benches/global_alloc.rs` — `direct` (alloc 1024, free
  1024), `churn` and `churn_write` (working set 256, 1024 random free+alloc
  steps, seed `0xCAFE`; `churn_write` stores two volatile words after each
  alloc).
- Pairing: per round and per (group, size), all three arms are set up
  untimed, then their three timed samples run back to back, so one triplet
  sees one noise state. The order of a cell walks all six permutations of
  the arms, shifted by one per round.
- Sample size: `direct` and `steady` churn — 2 bodies (2048 pairs);
  `fresh:16` churn — one body on each of 16 sets (16 384 pairs). Each
  table states its own pairs per sample.
- Statistic: per process, the minimum ns/pair over 200 rounds; a table cell
  is the median of three processes' minimums, with their range in
  parentheses. The range is shown because process-level state (address
  layout) is not removed by a minimum over rounds.
- Churn regimes: `steady` — one working set per (group, size, arm),
  prefilled once and kept alive for the whole run; `fresh:16` — every
  sample prefills 16 new sets and runs one body on each, the shape of
  criterion's `iter_batched` (one body per setup result).

## Data

Every table is the script's own output (`*.md`), derived from the gzipped
per-sample raw files; `*.csv` is the summary (commit and features on every
row); `*.manifest.json` holds commit `73241d28`, clean tree, binary sha256,
`rustc -vV`, CPU (i7-11800H, 16 logical) and OS (Windows 10 Pro
10.0.19045), written before measuring, plus `raw_sha256` of the three raw
files, which the script checks on `--from` (for these three runs the
hashes were added after capture, when the check was introduced). Re-derive any of them byte for byte with
`node scripts/bench-table-paired.mjs --from docs/perf/bt1_paired/<cfg>`.

| Configuration | Features | Churn regime | Table |
|---|---|---|---|
| `prod_steady` | `production` | `steady` | [prod_steady.md](bt1_paired/prod_steady.md) |
| `prod_fresh16` | `production` | `fresh:16` | [prod_fresh16.md](bt1_paired/prod_fresh16.md) |
| `bi_steady` | `production internals bench-internals` (the `bench:table` set) | `steady` | [bi_steady.md](bt1_paired/bi_steady.md) |

## Findings

1. **Under `production`, warm bulk 16 B is the one clear loss.** SeferAlloc
   against mimalloc, `prod_steady` (`prod_fresh16` in brackets): warm bulk
   16 B 7.3 against 4.7, 1.54× slower (1.47×); 64 B 7.0 against 6.3, 1.12×
   slower (1.02×); 256 B a tie (1.27× faster); 1024 B 1.62× faster
   (1.02×). Churn and churn+write at 16/64 B lie between 1.08× slower and
   1.29× faster; at 256 B SeferAlloc is 1.34–1.56× faster, at 1024 B
   2.75–9.87× faster.
2. **The `bench:table` feature set measures a slower SeferAlloc than ships.**
   Same commit and host, `steady`: with `internals bench-internals` every
   SeferAlloc cell is 1.3 to 3.4 ns/pair higher (warm bulk 16 B 10.1
   against 7.3; churn 16 B 7.5 against 5.7). mimalloc's cells differ by at
   most 0.6 ns; System's by up to 3.4 ns in either direction (System's own
   process spread reaches 31 %). The likely cause is the diagnostic
   counters those features compile into SeferAlloc's hot path — a
   hypothesis consistent with this comparison, not an isolated
   measurement. With those features the paired table shows warm bulk 16 B
   at 2.12× slower; the README criterion table shows 2.46×.
3. **mimalloc's 1024 B churn depends on the regime.** mimalloc churn
   1024 B: 17.0 (`steady`) against 69.3 (`fresh:16`); SeferAlloc 6.2
   against 7.0. At 16 B mimalloc is 5.8 against 5.5.
4. **Spread.** The script prints, per arm, how many cells have their three
   process minimums within 0.3 ns and the widest relative spread. For
   `prod_steady`: SeferAlloc 8/12 cells, widest direct 64 B (21.5 %, one
   process at 8.4 against 6.9); mimalloc 7/12, widest churn 1024 B
   (19.5 %). These are spreads of process minimums, not the same quantity
   as the criterion runs' ±15–20 % noise band.

## Superseded first harness

The first version of the harness (commits `5bd2aa29`, `91223e36`) was
reviewed before landing and replaced: the arm order of a cell never changed
across rounds (the rotation index advanced by 12 per round, 12 mod 3 = 0),
and its "steady" regime prefilled a fresh set every round and ran two
bodies on it. In that regime mimalloc churn 16 B measured about 15 ns
against 5.8 in the real steady state, which made SeferAlloc look 2.5×
faster at churn 16 B. Those numbers are not used anywhere.

## Not concluded

- The criterion table's absolute numbers are generally higher (e.g.
  SeferAlloc churn 16 B 17.1 there against 7.5 here with the same
  features), with exceptions — System warm bulk 1024 B is 109.2 there
  against 259.6 here. Not explained; the criterion run is one long sweep
  on a noisy host.
- No GO/NO-GO: this changes how the table is measured, not the allocator.
