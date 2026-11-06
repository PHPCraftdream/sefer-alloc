# J8 pre-registration — skip all-zero directory words in blocks of 8 (`perf(runtime)` candidate; written before any measurement of the patch)

## Question

J7 (`docs/perf/J7_GAP_SPLIT.md`) put ~350 of the ~605 Ir of one `find_segment_with_free_impl` call (one call per magazine refill, grow64 at scale 0.5) into the
walk over the class's 64 directory words (`scan_directory_candidates`, `src/alloc_core/small/alloc_core_small/find_segment/scan.rs`,
`os::read_directory_class_words` copies `[u64; WORDS_PER_CLASS]` by value, then a word-by-word loop with `if bits == 0 { continue }`). While the heap only grows,
almost every word is zero. Does testing eight words at a time remove that cost on the growth shapes without touching anything else?

## The change (identical behaviour)

The word loop becomes a loop over `words.chunks_exact(8)`. A chunk whose OR is zero is skipped at once; under `alloc-stats` the counter
`DIRECTORY_WORDS_EXAMINED` advances by 8 for it, exactly as eight zero words did. A non-zero chunk runs the former word loop on its eight words with the same body,
same order, same early return. `WORDS_PER_CLASS` is a multiple of 8 (asserted at compile time). No atomic, no ordering, no layout, no new field: the read stays by value.

## Acceptance (fixed now)

- **Source identity:** one tree (a git archive of the base commit), arm `base` = the tree, arm `patch` = the tree plus `docs/perf/j8/patches/j8.patch`; both `--release --features production`
  with the repository's alignment RUSTFLAGS; the judge `examples/g1_wild_judge.rs` (Linux, WSL2) and `examples/b4_pmu_probe` for bulk.
- **Path activation (hard assert before any claim):** callgrind Ir per `find_segment_with_free_impl` call on grow64 at scale 0.5 falls by at least 200 Ir (the walk was ~350); if it does not, the
  chunk skip did not take effect and the run is void.
- **Ir (deterministic judge):** whole-process Ir per op on grow64, growmix, regrow (scale 0.25, `docs/perf/j7/j7.sh`): GO needs at least -2 % on grow64 (opt-in/runtime threshold) and no cell above +0.5 %;
  `bulk 1024 x {16, 64}` (`b4_pmu_probe`) must not rise by more than 0.5 %.
- **Time (only with the Ir above in the same cell):** the J6 paired judge campaign (12 blocks x 3 repeats, arms base and patch in one session, order alternating by block, the
  J6 statistic): a time claim for a scenario needs its Ir delta >= 0.5 % in the same scenario and non-overlapping 95 % intervals of the base/patch ratio against 1; otherwise "no time claim".
- **Safety:** the existing directory tests (R7-A0, R13-01 counters, `dbg_words_per_class` users) stay green on CI; a new test compares the chunked walk with the reference word-by-word walk on random bitmaps.
- **NO-GO:** path activation fails, or grow64 Ir gain under 2 %, or any control above its limit. A NO-GO is recorded with the same artifacts and the patch is not merged.

## Not in scope

Directory layout, the table scan, the ingress path, the kernel-time share of the gap (Retain), the Windows `large` loss.
