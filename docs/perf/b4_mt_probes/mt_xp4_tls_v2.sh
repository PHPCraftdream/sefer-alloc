#!/bin/bash
# mt_xp4_tls_v2.sh — rerun wrapper of mt_xp4_tls.sh (NOT modified; new mt_* copy
# as required). Fixes the logging path: output goes through tee to a file INSIDE
# WSL (~/ab/mt-xp4v3-tls.log), not to a /mnt path (the old wrapper's tee to
# /mnt failed, which forced a hand-assembled log).
# First lines of the log are the source identity, computed BY THE SCRIPT
# (sha256sum output echoed into the same log before the run — no hand typing):
#   - snapshot tar sha256 (mt-xp3-rss-a.tar = git archive of base A 2730d02a)
#   - probe file sha256 (mt_xp4_tls_probe.rs)
# Changes for the mt-xp4v3 rerun:
#   - `set -euo pipefail`: the exit code of this script is the exit code of the
#     probe (the `grep` in the pipe can no longer mask a probe failure).
#   - BEFORE each run the wrapper prints a configuration line
#       steps=<N> workload=<w> T=<t>   # probe hardcodes working_set=768 mstress_blocks=512
#     (<steps> and <T> are the probe's argv; working_set / mstress_blocks are
#     hardcoded INSIDE the probe, so they are recorded as a comment — this is
#     run configuration, not a hand-written measurement).
#   - all /mnt-absolute paths printed into the log are sanitized to $REPO_WSL
#     BY THE SCRIPT (no post-hoc hand editing of the log).
# REPO_WSL must be set by the caller (WSL path to the repository root).
set -euo pipefail
: "${REPO_WSL:?set REPO_WSL to the WSL path of the repo root, e.g. \$REPO_WSL}"
LP=mt-xp4v3
TAR=$REPO_WSL/.rush/wsl/mt-xp3-rss-a.tar
PROBE=$REPO_WSL/.rush/wsl/mt_xp4_tls_probe.rs
LOG=~/ab/mt-xp4v3-tls.log
SAN="s|$REPO_WSL|\$REPO_WSL|g"
{
  echo "# mt-xp4v3 identity (computed by this script from sha256sum, paths sanitized to \$REPO_WSL):"
  echo "# snapshot commit: 2730d02a5824128804f847d006206bda75a9fc84 (base A; tar = \$REPO_WSL/.rush/wsl/mt-xp3-rss-a.tar)"
  sha256sum "$TAR" | sed -e "$SAN"
  sha256sum "$PROBE" | sed -e "$SAN"
} > "$LOG"
D=~/ab/$LP; rm -rf $D; mkdir -p $D
tar -xf "$TAR" -C $D
cp "$PROBE" $D/examples/zz_mt_xp4_tls.rs
printf '\n[[example]]\nname = "zz_mt_xp4_tls"\nrequired-features = ["alloc-global"]\n' >> $D/Cargo.toml
(cd $D && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgtxp4-$LP \
  cargo build -q --release --example zz_mt_xp4_tls --features production,internals,bench-internals) 2>&1 | tee -a "$LOG"
BIN=~/ab/tgtxp4-$LP/release/examples/zz_mt_xp4_tls
for wl in larson mstress; do
  for t in 2 4; do
    echo "steps=100000 workload=$wl T=$t # probe hardcodes working_set=768 mstress_blocks=512" | tee -a "$LOG"
    echo "== base A $wl T=$t ==" | tee -a "$LOG"
    $BIN sefer $wl 100000 $t 2>&1 | grep -E 'PINS|error' | tee -a "$LOG"
  done
done
echo "DONE mt-xp4v3" | tee -a "$LOG"
