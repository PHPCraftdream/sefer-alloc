# ev_ probes (b4-events, stage 1)

Set `REPO` (Windows path to the repository checkout, e.g. `/c/src/sefer-alloc`) and
`REPO_WSL` (same repository as seen from WSL, e.g. `/mnt/c/src/sefer-alloc`) before
sourcing/running these scripts.

- `ev_prof_win.sh <worktree-name> <label> [larson|mstress]` — Windows wrapper: snapshots the
  worktree into `$REPO/.rush/wsl/ev_<label>.tar` and invokes `ev_prof.sh` in WSL.
- `ev_prof.sh <label> [larson|mstress]` — WSL-side: unpacks, builds `examples/ev_larson_probe`
  (production features, release, line tables) and runs callgrind:
  toggle-collect marginal Ir/op per event/phase function, full profile with call counts,
  a cache-sim run, and line annotation of key allocator files.
- `ev_larson_probe.rs` — probe program (one malloc-bench-rs workload at T=1, sefer allocator).
