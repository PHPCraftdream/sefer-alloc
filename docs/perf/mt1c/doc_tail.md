
## 3. Reading

- **mstress T=1.** The deterministic gap is 6 743 489 Ir over 300 000 steps. The measured alloc/free pairs explain
  the share printed in section 0 item 2; the cold magazine events explain 0.084 of it. `perf stat` shows the same
  gap in cycles at a slightly better IPC, so on Linux this is a path-length gap, not a stall. The `free` side costs
  more than twice what mimalloc's `mi_free` costs per call, the `alloc` side costs less; a change that lengthens
  `alloc` to shorten `free` is not visible in this data.
- **larson T=1.** The instruction gap (1.071) is smaller than the cycle gap (1.166). Branch misses, L1I, dTLB and
  L1D misses are not higher for sefer, so the extra cycles are neither of those. About 0.28 of the cycle gap is
  attributed (by `perf record` share times `perf stat` cycles, a hint with skid) to the std `mpmc` receiver, the
  harness' channel, whose nodes are allocated by the allocator under test; cycles of that function differ between
  arms although its instruction count is identical in both (`callgrind`: 7 500 049 + 5 700 039 Ir in both).
  That is a candidate for placement or dependency effects of the memory the allocator returns, not a measured cause.
- **WILD-2.** Not relevant to T=1 mstress / larson (-130 and +142 Ir).
- **Windows.** The LM-1 ratios (1.098, 1.239) are larger than the Linux ones (1.071 / 1.106 in Ir, 1.166 / 1.078
  in cycles); this report does not explain the difference and does not measure Windows.

## 4. Limits

- One machine, shared with other sessions; Linux (WSL2) only. callgrind rows are deterministic; `perf stat` rows
  are medians of 5-7 interleaved runs and the larson cycles of sefer span 211.4-228.8 M.
- `perf record` shares are sampling-based (skid); they are used only as an attribution hint in section 3.
- The call-tree extract attributes inlined callees by file and line; the mimalloc `_mi_page_retire` row is the
  inclusive Ir as printed and its call count is not used.
- The "49 instructions" of the R123 accept path is a static count of the hit path; the per-call Ir of this report
  is a dynamic inclusive count that includes the shim frame and the cold events. They are different quantities.

## 5. What is left

- Larson: find the cause of the cycles that are not instructions (placement / dependency of the memory returned to
  the channel nodes); a first test is `perf record` with annotation of the receiver function for both arms.
- Mstress: any further cut of the `free` path must keep every safety check and pass `scripts/g1_magnitude_verdict.mjs`.

## 6. Reproduction

- `mt1c.sh` (builds base and WILD-2 from `git archive` tars, registers `g1_mt_one.rs.txt` as an example, runs
  callgrind), `mt1p.sh` (cycles / instructions / L1D), `mt1b.sh` (branches, L1I), `mt1t.sh` (dTLB, loads),
  `mt1r2.sh` (`perf record` shares), `mt1calls.sh` and `mt1fc.sh` (call-tree extracts). They are kept as run, with
  the scratch directory replaced by `<scratch dir>`; they need WSL2 with valgrind and a `perf` binary matching the
  kernel.
- Tables and CSV: `node scripts/mt1_t1_report.mjs` (writes `docs/perf/mt1c/tables.md`,
  `docs/perf/MT1_T1_DECOMPOSITION_summary.csv` and this file from `doc_head.md`, `doc_tail.md` and the tables).
