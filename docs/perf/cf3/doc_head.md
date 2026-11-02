# CF-3 — the transparent-huge-page threshold becomes a setting; sweep and default (`bench`, measurement-only, default unchanged)

**Class:** the setting itself (`LargeCacheConfig::huge_page_after_segments`, commit `f76b0914f40a2767402b0ddc9eff104f681c6f45`; tree `58771673f7fa11977c6239896e3087db53f17b54`, unchanged by the later reword of its subject) is
`perf(opt-in)`: default unchanged (16), one relaxed load added to the cold small-segment reservation path. This report is `bench`:
it only measures the setting (judge arm `sefer_pool` takes `WJ_THP=<n>` at build time). Tables and both summary CSVs are generated
by `scripts/cf3_report.mjs` from `docs/perf/cf3/`; the prose cites them. Platform: Linux (WSL2) only — the hint is a no-op elsewhere.

**Entry point under test:** a real `#[global_allocator]` — `SeferAlloc::with_config(LargeCacheConfig::DEFAULT.pool(...).huge_page_after_segments(n))`
inside the process judge `examples/g1_wild_judge.rs` (arm `sefer_pool`), because the setting ships at that layer.

## 0. Summary

1. **The setting.** `.huge_page_after_segments(n)` on `LargeCacheConfig`: from the n-th small segment (4 MiB each, counted over
   the whole process) on, a newly reserved small segment is advised `MADV_HUGEPAGE` (Linux). `0` never hints; default 16 (64 MiB of
   small segments, the TH-1 value). Process-wide, first config wins; out-of-range values (above `u32::MAX`) mean "never"; no value can
   index anything.
2. **What the hint buys (page faults, deterministic).** Minor page faults of the process in the 1 GiB growth shape `grow64`:
   threshold 0 (never) 331,728; 1: 53,882; 4: 56,937; 16 (default): 69,166; 64: 118,078; mimalloc 33,432. The other growth shapes
   show the same order (tables below). The hinted-segment counter follows the threshold exactly (grow64: 273 / 270 / 258 / 210
   segments for thresholds 1 / 4 / 16 / 64).
3. **What it costs (memory).** In the small shapes the threshold decides how much memory is backed by huge pages: `wild` peak RSS 41
   MiB at 0 and 64 and above, 62 at 16, 100 at 4 and 110 at 1; `phase` 185 MiB at 0, 295 at 16, 307 at 4, 310 at 1. The big growth
   shapes are not affected (grow64 peak RSS 1296–1300 MiB at every threshold: they fill whole segments).
4. **Time.** Medians are in the direction of the faults — never hinting is slower in the three growth shapes (ratio to the default
   1.203, 1.726, 1.560 in grow64, growmix, regrow) — but the per-repetition ranges are wide on the shared machine (the control
   `sefer`, the same configuration in the same binary, spans 0.60–1.32 in grow64), so the time rows are supporting evidence and the
   page-fault counts are the measurement. Hinting earlier than 16 (thresholds 4 and 1) removes 18 % and 22 % of the default's faults in grow64 (numerator: 69,166 minus 56,937 and
   53,882; denominator: the default's 69,166), without a time difference above the noise and with 38 and 48 MiB more peak RSS in `wild`.
5. **Verdict:** the default stays 16. In grow64 it has 2.1× the page faults of mimalloc (69,166 / 33,432), against 9.9× with the hint
   off (331,728 / 33,432) and 3.5× at threshold 64 (118,078 / 33,432), while the small shapes keep their 4 KiB-page RSS (`wild` 62 MiB
   against 110 MiB at threshold 1). The README section "Transparent huge pages (Linux)" documents the setting and the numbers.

## 1. What was measured

* **Judge and identity.** `examples/g1_wild_judge.rs` at commit `f76b0914f40a2767402b0ddc9eff104f681c6f45` (`cf3/tree.txt`), release;
  `--features production` for time, `--features "production internals bench-internals"` for the counters; binary hashes, `rustc`, CPU, kernel
  and the system's transparent-huge-page mode (`madvise`: the hint is honoured) are in `cf3/ident.txt`. Linux WSL2, i7-11800H.
* **Arms.** `t<N>` = `sefer_pool` built with `WJ_THP=<N>` for N in 0, 1, 4, 16, 64, 1024 and the default pool (`WJ_POOL=4`: pool 4 segments, byte cap 16 MiB —
  the first build of this sweep used the CF-1 default of the arm, 1024, and was discarded before any data was taken); `sefer` =
  `SeferAlloc::new()`; `mi` = mimalloc. `sefer` and `t16` run the same binary with the same effective configuration: their spread is the
  noise floor. Six scenarios (`grow64`, `grow1k`, `growmix`, `regrow`, `wild`, `phase`; `large` allocates no small segments), seed 1,
  scale 1, one process per run.
* **Rotation.** Per scenario and repetition the eight arms run in a cyclic rotation, 9 repetitions.
* **Config evidence (R26-4).** For every threshold: requested (`thp_req`), resolved (`thp_res`, read with `AllocCore::dbg_huge_page_after_segments()`
  from the allocator itself), pool requested and resolved (4 / 4), `config_conflicts` = 0; the script aborts on any mismatch
  (`cf3/evidence.jsonl`, `cf3/mech.jsonl`). Subprocess isolation per run.
* **Mechanism evidence (R30-8).** The hinted-segment counter (`THP_HINTED_SMALL_SEGMENTS`) and the huge-page-backed bytes at the live
  peak (`AnonHugePages` from `/proc`) per threshold; the script checks that threshold 0 hints nothing and that the counter never rises with the threshold.
* **Statistics.** Time: median of per-repetition ratios against `t16` of the same repetition, with the minimum and maximum of those
  ratios; faults and RSS: medians.

