#!/bin/bash
# ev_ (b4-events stage 1): Windows wrapper — snapshot a worktree and run the
# event-cost callgrind layout in WSL.
# Usage: bash .rush/wsl/ev_prof_win.sh <worktree-name> <label> [larson|mstress]
set -e
R=$REPO
(cd $R/worktrees/$1 && git ls-files -co --exclude-standard -z | tar --null -T - -cf $R/.rush/wsl/ev_$2.tar)
wsl.exe -e bash -lc "bash $REPO_WSL/.rush/wsl/ev_prof.sh $2 ${3:-larson}"
