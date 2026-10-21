#!/bin/bash
# oc_rem2_tar.sh (WSL): build a snapshot tar for a D1-prime-port2 shoulder.
# Usage: bash oc_rem2_tar.sh <a|b> <out.tar>
set -e
SH=$1; OUT=$2
R=${REPO_WSL:?set REPO_WSL to the repo path inside WSL}; W=$R/.rush/wsl
if [ "$SH" = a ]; then
  cd $W/ocport-a2
  git ls-files -co --exclude-standard -z | tar --null -T - -cf $OUT
else
  cd $R/worktrees/b4-oneclass
  # oc_rem2_b.list: git ls-files -co --exclude-standard -z (built with GIT_DIR
  # set — WSL git cannot follow this worktree's Windows-style .git pointer),
  # NUL-excluding .git.ocport-bak if it ever appears.
  tar --null -T $W/oc_rem2_b.list -cf $OUT
fi
echo "TAR_OK $SH $(sha256sum $OUT | cut -d' ' -f1)"
