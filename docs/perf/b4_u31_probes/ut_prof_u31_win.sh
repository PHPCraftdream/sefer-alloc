#!/bin/bash
: "${REPO:?set REPO to the repository root}"
# Windows Git Bash wrapper: snapshot the b4-u31trig worktree working tree and
# profile the six B4-29/U31 growth16 arms in WSL with callgrind.
# Usage: bash .rush/wsl/ut_prof_u31_win.sh <label>
#   (worktree is fixed to b4-u31trig; label is suffixed ut-)
set -e
R=${REPO}
L="ut-$1"
(cd $R && git ls-files -co --exclude-standard -z | tar --null -T - -cf $R/.rush/wsl/$L.tar)
DIFFHASH=$(cd $R && git diff | sha256sum | awk '{print $1}')
wsl.exe -e bash -lc "bash ${REPO_WSL}/.rush/wsl/ut_prof_u31.sh $L $DIFFHASH"
