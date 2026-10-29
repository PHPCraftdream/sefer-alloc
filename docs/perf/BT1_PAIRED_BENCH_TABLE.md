# BT-1: paired short-sample `bench:table` (measurement only)

No runtime code changed. This report adds a second way to produce the canonical
SeferAlloc / mimalloc / System table and records what it shows next to the
criterion table (`npm run bench:table`).

## Method

`examples/bench_table_paired.rs`, driven by `scripts/bench-table-paired.mjs`
(`npm run bench:table:paired`).

- Entry point: each allocator's `GlobalAlloc` impl called directly
  (`SeferAlloc` static, `mimalloc::MiMalloc`, `std::alloc::System`), the same
  layer `benches/global_alloc.rs` measures. None is the process allocator.
- Bodies: those of `benches/global_alloc.rs` — `direct` (alloc 1024, free
  1024), `churn` and `churn_write` (working set 256, 1024 random free+alloc
  steps, seed `0xCAFE`; `churn_write` stores two volatile words after each
  alloc).
- Pairing: per round and per (group, size), all three arms are set up
  untimed, then their three timed samples run back to back in a rotated
  order, so one triplet sees one noise state. A sample is 2 calls (2048
  pairs, tens of µs).
- Statistic: per process, the minimum ns/pair over 200 rounds; a table cell
  is the median of three processes' minimums, with their range in
  parentheses. The range is reported because process-level state (address
  layout) is not removed by a minimum over rounds: in an earlier
  three-process run one process measured churn 64 B at 9.3 against 6.0 in
  the other two.
- Churn regime (`--batch`): `1` = one working set prefilled before the
  sample (the steady state of a long-lived set); `16` = sixteen freshly
  prefilled sets per sample, the shape criterion's `iter_batched`
  (`BatchSize::SmallInput`) produces.

## Data

Every table below is the script's own output (`*.md`, derived from the
gzipped per-sample raw files; `*.csv` is the summary; `*.manifest.json`
holds commit `91223e36`, clean tree, binary sha256 and `rustc -vV`).
Re-derive with
`node scripts/bench-table-paired.mjs --from docs/perf/bt1_paired/<cfg>.p1.jsonl.gz,<…p2>,<…p3>`.

| Configuration | Features | `--batch` | Table |
|---|---|---:|---|
| `prod_b1` | `production` | 1 | [prod_b1.md](bt1_paired/prod_b1.md) |
| `prod_b16` | `production` | 16 | [prod_b16.md](bt1_paired/prod_b16.md) |
| `bi_b1` | `production internals bench-internals` (the `bench:table` set) | 1 | [bi_b1.md](bt1_paired/bi_b1.md) |

Windows 10 dev host, the same host as the 2026-10-29 criterion table.

## Findings

1. **The criterion table measures a slower SeferAlloc than ships.**
   `bench:table` needs `internals bench-internals` (for
   `dbg_trim_current_thread`), and those features compile diagnostic counters
   into the hot path. Paired, same commit and host: SeferAlloc warm bulk 16 B
   is 9.6 ns/pair with them and 6.8 without; churn 16 B 7.5 against 5.9.
   mimalloc and System do not change (they have no such counters). The
   README's "2.46× slower" warm bulk 16 B is therefore, for `production`,
   1.49× slower in the paired measurement (64 B: 1.44× → 1.04×).
2. **mimalloc's churn cost depends on the regime; SeferAlloc's does not.**
   Churn 16 B, SeferAlloc 5.9 in both regimes; mimalloc 14.8 in steady state
   and 5.5 over freshly prefilled sets. Likely mechanism (inferred from
   mimalloc's design, not measured here): frees go to the page's
   `local_free` list, so once a page's `free` list is used up every alloc
   takes the generic path, while a fresh prefill leaves a long `free` list. So
   churn 16 B is 2.51× faster in steady state and 1.07× slower over fresh
   sets; both are reported.
3. **The paired table is tight.** For SeferAlloc and mimalloc, 53 of 72
   cells (three configurations × 24) have their three process minimums
   within 0.3 ns of each other; the widest is churn_write 64 B SeferAlloc in
   `prod_b16`, 6.100–6.516 around 6.140 (6.8 % of the cell). The criterion
   runs carry a ±15–20 % band on this host.
4. **Cross-group state is visible.** `direct` is the same body in both
   regimes, yet SeferAlloc's 1024 B warm bulk is 8.7 in `prod_b1` and 14.7 in
   `prod_b16`, and System's 174.0 against 51.4: the preceding churn groups
   (one set vs sixteen sets per sample) leave different heap state behind.
   Not investigated further here.

## Not concluded

- The remaining gap between the criterion absolute numbers and the paired
  ones (e.g. SeferAlloc churn 16 B 17.1 criterion against 7.5 paired with
  the same features) is not explained by features or batching alone; the
  criterion run is a single long sweep on a noisy host.
- No GO/NO-GO: this changes how the table is measured, not the allocator.
