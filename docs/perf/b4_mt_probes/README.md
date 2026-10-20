# mt_ probes (b4-mt: X-notown / X-pin / mt-xp4)

Set `REPO` (the repository root as seen from Git Bash on Windows) and
`REPO_WSL` (the same root as seen from WSL, under `/mnt/<drive>/...`) before
running these scripts. All scripts are read-only for the worktrees and
build only inside WSL snapshots (`~/ab/...`); no git write, no new unsafe.

## Full reproduction from a clean checkout

Everything below is derivable from the committed tree alone: the probes and
the wrappers live in this directory (`docs/perf/b4_mt_probes/`), and the
wrapper copies read the probes from `$REPO_WSL/.rush/wsl/` — step (b) below
puts them there. No file outside the repository is assumed to exist.

Blocks (a)–(b) run in **Git Bash** from the repository root; the mt-xp4 run
in the list below is launched from Git Bash via `wsl.exe`.

```bash
# (0) directories the steps below write into
mkdir -p "$REPO/.rush/wsl"
wsl.exe -e bash -lc "mkdir -p ~/ab"

# (a) snapshot tars (read-only: git archive / git ls-files, NO git write)
#   base A tar used by the mt-xp4 (v2/v3) and RSS-arm-A runs:
git archive 2730d02a -o "$REPO/.rush/wsl/mt-xp3-rss-a.tar"
#   worktree snapshot tars for the judge runs are made by the *_win.sh
#   wrappers THEMSELVES (git ls-files -co --exclude-standard | tar), so no
#   separate creation step is needed for them — e.g. mt_prof_foreign_free_win.sh
#   writes $REPO/.rush/wsl/<label>.tar, mt_rss_pin_win.sh writes
#   $REPO/.rush/wsl/<lp>-{a,b}.tar (arm a is git archive 2730d02a, re-created
#   by the wrapper the same way as line (a) above).

# (b) copy the probes AND the wrappers into $REPO_WSL/.rush/wsl/ (the
#     wrapper copies source the probes from there):
cp docs/perf/b4_mt_probes/mt_*.rs "$REPO/.rush/wsl/"
cp docs/perf/b4_mt_probes/mt_*.sh "$REPO/.rush/wsl/"

# (c) environment variables for every wrapper invocation:
#   REPO        — Git-Bash path of the repository root
#   REPO_WSL    — the same root as seen from WSL (/mnt/<drive>/...)
```

### Wrapper invocations (ALL arguments, verbatim)

- `bash mt_prof_foreign_free_win.sh <worktree-name> <label> <foreign|own> <locality|capacity>`
  — `<worktree-name>` is a directory under `$REPO/worktrees/` (e.g. `b4-mt`);
  `<label>` names both the tar (`$REPO/.rush/wsl/<label>.tar`, created by the
  wrapper) and the WSL build dir; the 3rd/4th args select the foreign/own mode
  and the locality/capacity trace. Produced `_raw_b4_mt_base.log` (labels
  `mt-base-*`), `_raw_b4_mt_xn.log` (`mt-xn-*`), `_raw_b4_mt_xp.log`
  (`mt-xp-a-*`/`mt-xp-ab-*`), `_raw_b4_mt_xp3_{f,fcap,own}.log` (label
  `mt-xp3`), `_raw_b4_mt_xp3_b1{,b}.log` (labels `mt-xp3-b1`, `mt-xp3-b1b`).
- `bash mt_prof_foreign_free_win.sh <worktree> <label> foreign locality` —
  same script; the oracle block inside prints `ORACLE ... hit_frac=`.
- `_raw_b4_mt_xp3_larson.log`, `_raw_b4_mt_xp3_mstress.log` — the standard
  `bash .rush/wsl/prof_win.sh <worktree-name> <label> <larson|mstress> sefer`
  on the mt-xp3 diff; `_raw_b4_mt_xp3_thrash64.log` — standard
  `bash .rush/wsl/prof_thrash64_dir.sh <abs-dir-with-git-tree> <label> sefer`.
  These two standard callgrind profilers (and the WSL-side `prof.sh` /
  `prof_thrash64.sh` they call) are repository-local tools that are NOT
  committed — a pre-existing reproducibility gap shared by the other B4
  reports. The decisive NO-GO evidence (mt-xp4) does not depend on them.
- `bash mt_rss_pin_win.sh <label-prefix>` — e.g. `mt-xp3-rss`; creates
  `$REPO/.rush/wsl/<lp>-a.tar` (= `git archive 2730d02a`) and
  `<lp>-b.tar` (working tree) itself, then runs `mt_rss_pin.sh <lp>` in WSL.
  Produced `_raw_b4_mt_xp3_rss.log`.
- `_raw_b4_mt_xp4_tls.log` (mt-xp4v3) —
  `wsl.exe -e bash -lc "REPO_WSL=<REPO_WSL> bash $REPO_WSL/.rush/wsl/mt_xp4_tls_v2.sh"`
  (no further arguments; the wrapper hardcodes the `steps=100000`/`T∈{2,4}`
  loop and prints a `steps=<N> workload=<w> T=<t>` configuration line before
  each run; `set -euo pipefail` — its exit code is the probe's exit code).
  Requires `$REPO_WSL/.rush/wsl/mt-xp3-rss-a.tar` (step (a)) and
  `$REPO_WSL/.rush/wsl/mt_xp4_tls_probe.rs` + `mt_xp4_tls_v2.sh` (step (b));
  the log is tee-ed INSIDE WSL to `~/ab/mt-xp4v3-tls.log` and copied into the
  repo as-is (only `/mnt/<drive>/…` paths are sanitized to `$REPO_WSL`, by
  the script itself). NOTE: the vmem verifier may fail with ENOENT on a stale
  `heap_route_pin_cache.rs` git-index entry until the orchestrator commits;
  no green vmem is promised before that commit.

## Sources (verbatim copies; private paths replaced by `$REPO` / `$REPO_WSL`)

- `mt_foreign_free_probe.rs` — judge probe: real foreign-free path through
  `GlobalAlloc::dealloc` (locality / capacity / own modes), callgrind
  `w_free` window.
- `mt_prof_foreign_free.sh` / `mt_prof_foreign_free_win.sh` — WSL-side profiler
  and its Git-Bash wrapper (snapshot the worktree, callgrind marginal
  Ir/free + bench-internals oracle).
- `mt_xp4_tls_probe.rs` — decisive mt-xp4 probe: hit/miss counters of the
  EXISTING R1 TLS route pin cache on real larson/mstress traces at T=2/T=4
  (counters-only build; Ir is never taken from this build). The probe
  hardcodes `working_set: 768` and `mstress_blocks: 512` internally (argv is
  only `<sefer|mimalloc> <larson|mstress> <steps> <threads>`).
- `mt_xp4_tls.sh` — original mt-xp4 wrapper (models the base A snapshot run).
- `mt_xp4_tls_v2.sh` — mt-xp4v3 rerun wrapper: `set -euo pipefail` (exit code
  = probe's exit code); tee inside WSL (`~/ab/mt-xp4v3-tls.log`); writes the
  source identity (snapshot commit + `sha256sum` of the snapshot tar and the
  probe file, `/mnt`-paths sanitized to `$REPO_WSL`) AND a
  `steps=<N> workload=<w> T=<t>` configuration line (with the probe's
  hardcoded `working_set=768 mstress_blocks=512` recorded as a comment) into
  the same log BEFORE each run — no hand-assembled log lines.
- `mt_rss_pin_probe.rs` + `mt_rss_pin.sh` / `mt_rss_pin_win.sh` — R26-4
  subprocess RSS probe (two snapshot arms, delta idle RSS, threshold
  ≤ +1024 KiB).

## Which command produced which cited raw log (`docs/perf/_raw_b4_mt_*`)

See the "Wrapper invocations" list above — every `_raw_b4_mt_*.log` maps to
exactly one invocation with its full argument list.
