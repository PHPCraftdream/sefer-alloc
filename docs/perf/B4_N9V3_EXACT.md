# B4-31 — N9 v3: last-segment memo armed on own_cache hits only (`perf(runtime)`)

N9 v3 from the B4-M micro-pack (`docs/perf/B4_MICRO_PACK.md` §5) was split out of PR #89
because its local verdict rested on an integer-precision base. This report closes it with
exact numbers on both arms.

- **Change:** `SegmentTable::canonical_base_of_mut` arms the one-entry memo only on an
  exact-base `own_cache` hit; a won `hash_find` probe fills `own_cache` alone
  (`src/alloc_core/segment/segment_table/segment_table_impl.rs`). Pinned by
  `tests/b4_micro_n9_memo_fill_policy.rs` (red if the won-probe fill returns).
- **Entry point:** `SeferAlloc` as `GlobalAlloc` (thrash64 probe `examples/zz_thrash64`
  built from `docs/perf/b4_n9v3_probes/thrash64_probe.rs` by `prof_thrash64.sh` there, wrappers
  `w_alloc`/`w_free` behind `#[inline(never)]`; the same probe the micro-pack used) and the
  CI iai judge `perf_gate_iai`.
- **Identity:** A = main `2f066096`, B = `f8b15f97` (this PR's code commit); both measured
  from clean trees (empty `git diff` hash recorded in the raw log).
- **Raw:** `_raw_b4_n9v3_thrash64.log` (local callgrind, exact TOTALS), `_raw_b4_n9v3_iai_ab.csv`
  + `_raw_b4_n9v3_iai_ab_verdict.log` (r20 gate-run 39541547971). Tables and verdict:
  `node scripts/b4_n9v3_table.mjs` (asserts: same SHAs in both sources, clean trees,
  mimalloc controls unchanged, TOTALS arithmetic, thrash64 activation ≥ 64 segments);
  CSV `B4_N9V3_EXACT_summary.csv` (`--csv`).

## Result (output of `node scripts/b4_n9v3_table.mjs`, verbatim)

A = `2f066096` (main), B = `f8b15f97` (N9 v3); both trees clean (diff sha256 = empty).

Target — thrash64, local callgrind, exact (TOTALS(100k) − TOTALS(50k)) / 50k, `--features production`:

| metric | A | B | Δ | pre-registered | result |
|---|---:|---:|---:|---|---|
| free Ir/op | 84.3607 | 81.4298 | -2.9309 | ≤ -2 | met |
| alloc Ir/op | 22.0000 | 22.0000 | 0.0000 | ≤ 0 | ok |
| free EstCycles/op | 93.3 | 90.3 | -3.0 | — | — |

Activation oracle: distinct 16 B-window segments 64 / 64 (≥ 64 in both arms).

Non-regression — CI iai A/B (`production bench-internals internals`): of 77 cells with A > 100 Ir, 21 better, 43 worse (36 of them exactly +6 Ir), 13 equal; mimalloc controls: 13, all Δ = 0.
Largest regression: `dealloc_contains_base_probe_only_16b` 51514 → 51586 (0.140 %), threshold ≤ 0.2 %.

| cell (largest gains) | A Ir | B Ir | Δ | Δ % |
|---|---:|---:|---:|---:|
| `r19_prf01_publication` | 150265 | 149922 | -343 | -0.228 % |
| `large_cache_eviction_hit_only_41x4mib` | 234497 | 234005 | -492 | -0.210 % |
| `large_cache_eviction_prefill_only_41x4mib` | 156348 | 156102 | -246 | -0.157 % |
| `large_cache_free_slot_search_cycle_only` | 73440 | 73336 | -104 | -0.142 % |
| `seg_cycle_decommit_256k` | 1676348 | 1674385 | -1963 | -0.117 % |
| `large_cache_prefill_only_4mib` | 56453 | 56397 | -56 | -0.099 % |
| `large_cache_hit_only_4mib` | 57885 | 57830 | -55 | -0.095 % |
| `large_cache_free_slot_search_prefill_only` | 61287 | 61239 | -48 | -0.078 % |

**Verdict N9 v3: GO** (target met; non-regression held).

## Reading

The CI regressions are almost all exactly +6 Ir per cell regardless of the number of
operations in the cell (`dealloc_free_only_16b_n8` and `_n32` both +6), i.e. a fixed cost in
the measured window, not per operation; the largest relative one,
`dealloc_contains_base_probe_only_16b`, is +0.14 %. The target workload (64 live segments,
miss-heavy) gains 2.93 Ir per free and 3.0 EstCycles per free.
