
## 3. Gate by gate (PREREG.md section 3)

| id | result | reading |
|---|---|---|
| T1 Windows judge `wild` | UNREADABLE | campaign 1 A/A excludes 1; pilot at HIGH priority no better; confirmation on Windows not run (Amendment 2) |
| T2 Linux judge `wild` | PASS in the confirmation campaign (0.963, 0.941-0.986); FAIL in campaign 1 (0.972, 0.941-1.004) | the one confirmation campaign of the preregistration was spent here, no third |
| T3 activation | PASS, 63.7 % (2 424 451 / 3 804 795) | threshold 40 % |
| C1 Windows controls | KILL by the letter on `phase` (1.083) and `burst` (1.030); `grow1k` unreadable; the rest HOLD/ok | the KILLs sit inside A/A-sized noise and in scenarios whose executed path is instruction-identical |
| C2 Linux controls | all ok in the confirmation campaign; `growmix` 1.029 is the closest to the line | campaign 1: `growmix` unreadable (A/A excludes 1) |
| C3 `bench:table` | set s1 GO, set s2 NO-GO by one noisy cell | noise floor printed under each set; UNREADABLE cells are neither passed nor failed |
| C4 MT | cells flagged by the spread rule | see the MT table |
| C5 callgrind Ir | PASS | `wild` -2.77 %, `phase` +0.0005 %, `burst` +0.002 % (limits: negative, <= +0.5 %, +-0.3 %) |
| C6 memo-hit path | PASS | disassembly identical up to one extra cold stub |
| C7 CI | open | see section 4 |

By the preregistration a gate that is UNREADABLE is neither passed nor failed, and a KILL by the letter stands in
the table. The honest summary is therefore **HOLD**: the evidence that exists says "a small, real gain on `wild`
(about 3-4 % of its run time on Linux, 2.8 % of its instructions), no cost on the paths that do not miss the memo,
no detectable regression elsewhere on Linux", while the Windows judge, the `bench:table` and MT controls could not
resolve effects of this size on a loaded shared machine.

## 4. What is left before this could merge

- **Evidence registry.** CI marks the row R-Ph3b stale because `dealloc_fast.rs` changed; it must be re-verified
  on CI (the `gate-run` mechanism) once the code is final, then the temporary config and label removed.
- **Owner decision** on the HOLD: merge as a small `perf(runtime)` gain with the limits above, or reject.
- **Not part of this change:** a larger `own_cache`. In `wild` 29 % of the lane frees still pass through the hash
  probe in the chain; with 64 entries that falls to 4.9 % (WILD-1 probe), but 64 entries exceed the `HeapCore`
  size assert under `--all-features` (9208 B measured, limit 9216 B) and would need a stack-budget decision or an
  out-of-line cache. `growmix` (1 382 444 hash probes against 73 015 second-chance accepts) is where that matters.

## 5. Limits

- One machine, shared; the Windows numbers of this report are noise-limited and say so; Linux (WSL2) numbers come
  from a different compiler than the Windows ones.
- The noise rules for C3 and C4 (Amendment 1 point 4) were written after seeing campaign 1 and are post hoc.
- `regrow` looked like a gain in campaign 1 on both platforms (0.911 and 0.937) and reads neutral in the Linux
  confirmation campaign (1.003, 0.948-1.061); its deterministic Ir is -1.41 %. It is not claimed.
- The WILD-1 report cites the callgrind per-call cost (91.7 Ir) from Linux at scale 0.3 next to the 88.8 % memo-miss
  rate from Windows at scale 1; the miss rate at the callgrind scale on Linux is lower (278 028 entries of the
  chain), which this report's per-function table (entries counted from the entry instruction) makes explicit.

## 6. Reproduction

- Builds at the commits of `ident.txt` / `linux_ident.txt` (`cargo build --release --example g1_wild_judge --features
  "production internals"`; the activation build adds `bench-internals`); campaigns:
  `node docs/perf/wild2/run.mjs --scenarios ... --blocks N --repeats K --base ... --base2 ... --x ... --out ...`
  (`--emit` prints the schedule used by `linux.sh`, `linux2.sh`).
- `bench:table`: `node scripts/bench-table-paired.mjs --features production --raw <prefix> --csv <file>` with the
  aligned-code `RUSTFLAGS` of the earlier campaigns, order base, x, x, base per set; MT: `malloc_macro` x 3 ABBA.
- Linux tools output (callgrind per function, perf stat, disassembly diff) is in `docs/perf/wild2/linux_tools/`,
  private paths replaced by `<scratch>` and `~`.
- Tables and CSV: `node scripts/wild2_report.mjs` (writes `docs/perf/wild2/tables.md`,
  `docs/perf/WILD2_LANE_SECOND_CHANCE_summary.csv` and this file from `doc_head.md`, `doc_tail.md` and the tables).
