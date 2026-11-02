## 3. Reading the tables

* **Fault ratios in the summary.** Never-hint over default in grow64: 331,728 / 69,166 = 4.8; default over mimalloc: 69,166 / 33,432 = 2.1; never over
  mimalloc: 331,728 / 33,432 = 9.9; threshold 64 over mimalloc: 118,078 / 33,432 = 3.5 (all from the page-fault table; denominators named).
* **Thresholds 1 and 4 against the default.** 53,882 and 56,937 against 69,166 faults in grow64 (22 % and 18 % fewer; numerator the difference,
  denominator the default); the time column does not separate them from the default, and `wild` peak RSS is 110 and 100 MiB against 62.
* **Threshold 1024 behaves like 0** in every scenario here (fewer than 1024 small segments exist), which is also the "never" documented value.
* **Why the big shapes do not move in RSS.** They allocate whole segments, so every hinted segment is fully touched either way.

## 4. Limits

* Linux only, one thread per scenario, six fill-then-free or phase shapes; WSL2 kernel with the `madvise` mode. A system with the mode `never`
  ignores the hint, one with `always` backs segments by huge pages regardless of the setting.
* The page-fault counts are the reproducible measurement; wall-clock medians carry the noise of a shared machine.
* No default changed, so no `bench:table` / `iai` refresh is owed; the cold path gained one relaxed atomic load per small-segment reservation
  (a reservation is one per 4 MiB of small-block growth).

## 5. Reproduction

* Judge sources: tree in `cf3/tree.txt`. Scripts `cf3/build.sh`, `cf3/sweep.sh 9 1`, `cf3/mech.sh 3` (they read the directory in `CF3_DIR`; the binaries'
  hashes are in `cf3/ident.txt`).
* Tables and CSVs: `node scripts/cf3_report.mjs cf3/raw.jsonl cf3/evidence.jsonl cf3/mech.jsonl docs/perf`, then
  `node scripts/cf3_report.mjs --assemble docs/perf` rebuilds this file from `cf3/doc_head.md`, `cf3/cf3_tables.md` and `cf3/doc_tail.md`.
* Summary files: `docs/perf/CF3_HUGE_PAGE_THRESHOLD_summary.csv` (time, faults, RSS, huge-page bytes) and
  `docs/perf/CF3_HUGE_PAGE_THRESHOLD_mech_summary.csv` (hinted-segment counter per threshold). Source identity: commit
  `f76b0914f40a2767402b0ddc9eff104f681c6f45`.
