# thrash64 exact probe (B4-31, N9 v3)

`thrash64_probe.rs` + the WSL callgrind wrappers that produced
`docs/perf/_raw_b4_n9v3_thrash64.log`. Set `REPO` (repository root in Git Bash)
and `REPO_WSL` (same root under `/mnt/<drive>/...` in WSL), copy these files to
`$REPO/.rush/wsl/`, then for each arm (a clean checkout/worktree of the SHA):

```bash
bash .rush/wsl/prof_thrash64_dir.sh <abs-path-of-arm-checkout> n9x-a sefer   # A = main
bash .rush/wsl/prof_thrash64_dir.sh <abs-path-of-arm-checkout> n9x-b sefer   # B = N9 v3
wsl.exe -e bash -lc "bash $REPO_WSL/.rush/wsl/n9x_exact.sh n9x-a n9x-b"
```

`n9x_exact.sh` prints the exact (fractional) marginal Ir/op from the callgrind
TOTALS the wrapper leaves under `~/ab`; `prof_thrash64.sh` itself rounds Ir/op
down to an integer, which is why the exact pass exists.
