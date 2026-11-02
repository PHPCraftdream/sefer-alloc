## 4. Reading the tables

* **What the pool can and cannot do.** The pool keeps empty small segments of the same size class so that a later
  allocation of that class re-uses them without a fresh OS reservation. The judge shapes never refill what they emptied, so the
  pool has nothing to serve: `seg_reserved` is identical at every size (except `phase`, which has a small refill). The cost side
  is certain and linear: every segment kept is committed memory that stays in the process.
* **Where a gain was measured elsewhere.** R27-4 measured a gain at pool 8 on a different shape (1024-byte allocate/free
  batches of 120 with teardown, Windows, single thread; `docs/perf/R27_4_REAL_DEFAULT_AB_GATE.md`): that is a workload that does
  cross a segment boundary repeatedly. The judge shapes here do not repeat that pattern, so neither result contradicts the other.
* **What pool 1024 shows on Windows.** It never releases a segment (`seg_released` = 0 in grow64, grow1k, regrow), so the time of
  releasing segments in `free` disappears: the WG-M0 ceiling again, now with the grow1k and regrow numbers repeated with the
  control arm. The price is the whole burst staying committed (1172 MiB left after grow64, against 29 MiB at the default).
  That trade is the opt-in "Retain" policy under owner decision (OWN-1 / RET-1); this sweep does not add an argument for a
  bounded size in between.
* **Linux time.** Not decisive: the layout noise floor (the `sefer`/`p4` column) is as large as the largest per-size
  difference. The decision for Linux rests on the counters and fault counts, which do not depend on layout.

## 5. Limits

* One thread per scenario (the judge is single-threaded); the pool is per heap, so the retained size scales with the number of
  heaps (threads that own a heap): N segments of 4 MiB per heap.
* Seven fill-then-free shapes. A workload that re-fills what it empties is the case where a larger pool can pay; use the
  counters (`seg_reserved` / `seg_released` through `dbg_current_pool_cap()` and the judge fields) on the real workload before
  raising the size.
* Windows time was measured with 7 repetitions per cell, Linux with 9; the Windows control spread is the reference for what is
  distinguishable there.
* No default changed, so no `bench:table` / `iai` refresh is owed.

## 6. Reproduction

* Judge sources: tree in `cf1/linux/tree.txt`. Linux: `cf1/linux/build.sh`, `sweep.sh 9 1`, `mech.sh 3`; Windows:
  `cf1/windows/win_build.sh`, `win_build_ev.sh`, `win_sweep.mjs 7 1`, `win_mech.mjs 3` (they expect the paths of the author's
  scratch directory; the binaries' hashes are in the `ident.txt` files).
* Tables and CSVs: `node scripts/cf1_report.mjs cf1/linux/raw.jsonl cf1/linux/evidence.jsonl cf1/linux/mech.jsonl docs/perf linux`
  (same for `windows`), then `node scripts/cf1_report.mjs --assemble docs/perf` rebuilds this file from `cf1/doc_head.md`, the two
  generated table files and `cf1/doc_tail.md`.
* Summary files (time, RSS, ratios): `docs/perf/CF1_POOL_SEGMENTS_linux_summary.csv`,
  `docs/perf/CF1_POOL_SEGMENTS_windows_summary.csv`; mechanism counters and faults:
  `docs/perf/CF1_POOL_SEGMENTS_linux_mech_summary.csv`, `docs/perf/CF1_POOL_SEGMENTS_windows_mech_summary.csv`.
  Source identity: commit `f15038ecb050eae071f89e113671df99308b5b9c` (recorded before measurement).
