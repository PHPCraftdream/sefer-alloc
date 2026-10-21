# ut_ probes (B4-29: U31 return-trigger measurement)

Probes and wrappers that produced the raw data in `docs/perf/b4_u31_raw/`
(report `docs/perf/B4_U31_TRIGGER.md`, tables by
`scripts/b4_u31_trigger_table.mjs`). Measurement only — no runtime change;
the counters they read are `bench-internals`-gated observers.

Set `REPO` (repository root as seen from Git Bash on Windows) and
`REPO_WSL` (the same root as seen from WSL, `/mnt/<drive>/...`). The
wrappers snapshot the checkout with `git ls-files | tar` (read-only), build
inside WSL under `~/ab/`, and write their logs to `$REPO/.rush/wsl/`.

```bash
mkdir -p "$REPO/.rush/wsl" && cp docs/perf/b4_u31_probes/* "$REPO/.rush/wsl/"
# T1/T3: growth16 arms (Sefer and mimalloc) under callgrind
bash .rush/wsl/ut_prof_u31_win.sh u31run4
# T1 reference: larson / mstress, T=1
bash .rush/wsl/ut_prof_u31lm_win.sh u31lar2 larson 100000
bash .rush/wsl/ut_prof_u31lm_win.sh u31ms2 mstress 100000
# T2: MT counters, 4 cells x 3 repeats (labels as in b4_u31_raw/)
bash .rush/wsl/ut_prof_u31mt_win.sh u31mt3-larson-t2-r1b larson 2 100000
#   ... and likewise for larson-t2-r{2,3}, larson-t4-r{1,2,3},
#   mstress-t2-r{1,2,3}, mstress-t4-r{1,2,3}
```

The committed `.annot` files are truncated to the lines the table script
reads (marker inside each file); the `.log`/`.counters` files are verbatim
apart from home/repository paths replaced by `~/` and `<repo>`.
