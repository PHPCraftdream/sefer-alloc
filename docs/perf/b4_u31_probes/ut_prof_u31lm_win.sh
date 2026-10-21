#!/bin/bash
: "${REPO:?set REPO to the repository root}"
# Windows Git Bash wrapper: snapshot the b4-u31trig worktree working tree and
# callgrind-profile one malloc-bench-rs workload (larson|mstress) at T=1 in WSL.
# Usage: bash .rush/wsl/ut_prof_u31lm_win.sh <label> <larson|mstress> [steps]
set -e
R=${REPO}
L="ut-$1"
W=$2
(cd $R && git ls-files -co --exclude-standard -z | tar --null -T - -cf $R/.rush/wsl/$L.tar)
(cd $R && git diff | sha256sum | awk '{print "git_diff_sha256 "$1}' > $R/.rush/wsl/$L.diffhash)
cat $R/.rush/wsl/$L.diffhash
wsl.exe -e bash -lc "bash ${REPO_WSL}/.rush/wsl/ut_prof_u31lm.sh $L $W ${3:-100000}"
