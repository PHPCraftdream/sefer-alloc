
## 3. Reading the tables

1. **Windows `wild` / `phase`.** Small LIFO bursts are 79 % of the `wild` difference (54.2 / 68.9: 42.5 free +
   11.7 alloc) and 83 % of the `phase` difference (65.5 / 78.9). Realloc contributes about 0 ms in `wild`
   (0.1 + 0.1 ms) and 5.3 ms in `phase` (2.3 + 3.0 ms). Large calls and the teardown add 2.5 + 4.1 + 3.6 ms in
   `wild`; the teardown is 0.2 ms in mimalloc against 3.8 ms in sefer.
2. **Burst alloc is slower in sefer everywhere, a little** (1.09..1.15 per block in the burst table, on a clean
   heap too); that part is the D16-R family (bulk small blocks), it is not specific to `wild`.
3. **Burst free is where the heap context matters.** The ratio per freed block grows from about 1.2..1.33 (clean
   heap, `phase`) to 2.37 (Windows) and 3.06 (Linux) in `wild`. The probe counters give the first link of the
   cause: 88.8 % of the frees in `wild` leave the memo lane; of those, 2 554 708 hit `own_cache` and 1 256 610 need
   the hash probe (29.3 % of all lane frees, median run; between 1.08 M and 1.35 M in the five runs because the
   segment layout differs from run to run). The chain behind the miss costs 91.7 instructions per call in the
   callgrind profile (the lane's own hit path is not part of that count).
4. **Instruction count and misses both contribute.** Instructions are +21.7 % (perf stat) and L1D load misses
   +73.8 %; the cache simulation (callgrind `--cache-sim`, D1 48 KiB 12-way) does not reproduce the extra misses
   (sefer 640 666 against mimalloc 779 909 simulated D1 read misses), so this report does not attribute the
   miss excess to a cause. The instruction excess is attributed: `lane_miss`.
5. **`large`.** The gap is the free of Large blocks: sefer 1.26 us per call against 0.21 us (4.9 ms and 0.8 ms for
   3889 calls, Windows) and the teardown; allocation of Large blocks is faster in sefer. This points at what a
   Large free does in sefer (decommit/release/cache admission) and was not investigated further. On Linux the
   whole `large` scenario is faster in sefer (14.7 ms against 118.4 ms) because mimalloc's `large_alloc` is
   105.5 ms slower.
6. **Correction of an earlier reading.** LM-1 recorded `sefer_pool` as unable to attribute `wild`/`large`; this
   report confirms it was the wrong lever for them (the dominant part is the small-block free, section 3.3).

## 4. What this suggests (not measured here)

- **WILD-2, a cheaper path for the memo miss.** The lane that accepts an own-thread small free currently returns
  to the unchanged chain on any memo mismatch. A second chance inside the cold exit (probe the direct-mapped
  `own_cache`, then the same acceptance steps) would keep the hit path of the memo untouched. The M1 assessment
  (`docs/perf/M1_SEGMENT_MAP_ASSESSMENT.md`) gave as its return trigger "a target workload with a live set larger
  than one segment and out-of-order frees where the judge shows a gap on free": `wild` is that workload (and
  `growmix`, `regrow` are candidates). A larger `own_cache` is a separate, size-limited lever (the 256-entry
  table does not fit the `HeapCore` size assert; 64 entries did build). Both need a preregistration, a paired
  judge campaign on Windows and Linux and a `bench:table` control before any decision.
- **Large free in `large`.** A separate, smaller item (4.2 ms of 4.0 ms in a 13 ms scenario).

## 5. Limits

- The probe counters are a throwaway build; timing of that build is not used anywhere. Counts differ between runs
  (segment layout); medians of 5 are reported.
- Per-call timers in the judge perturb the small operations they wrap (large blocks, realloc); the bursts are
  timed as units. `rest` contains everything not timed, including the first-touch writes of small blocks outside
  bursts.
- Linux numbers come from WSL2 on the same machine; the Windows and Linux builds use different compilers.
- One machine, one seed (1); the 12 blocks are repeats of the same op sequence, not different workloads.

## 6. Reproduction

- Judge build at the commits above: `cargo build --release --example g1_wild_judge --features "production internals"`;
  campaign: `WJ_PROF=1 node docs/perf/wild1/run.mjs --exe <judge> --out prof_raw.jsonl --blocks 12 --repeats 3 --scenarios wild,phase,large`
  and `--scenarios burst` for the clean-heap campaign (raw files are committed gzipped).
- Linux: `docs/perf/wild1/linux.sh` shows the build-and-run recipe (paths are placeholders); `perf stat` and
  `callgrind` outputs are in `docs/perf/wild1/linux_tools/` (private paths replaced by `<scratch>` and `~`).
- Probe: apply `docs/perf/wild1/probe.patch` on `12a01097`, build with `--features "production internals"` (LANE
  line) or `"production internals bench-internals"` (TIER1 line), run `g1_wild_judge <scenario> sefer 1 1`.
- Tables and CSVs: `node scripts/wild1_report.mjs` (writes `docs/perf/wild1/tables.md`,
  `docs/perf/WILD1_GAP_DECOMPOSITION_summary.csv`, `docs/perf/WILD1_GAP_DECOMPOSITION_bursts_summary.csv` and this
  file from `doc_head.md`, `doc_tail.md` and the tables).
