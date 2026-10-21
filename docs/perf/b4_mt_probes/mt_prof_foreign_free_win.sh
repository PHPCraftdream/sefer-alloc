#!/bin/bash
# Windows Git Bash wrapper for mt_prof_foreign_free.sh (new mt_* file; models
# prof_win.sh without modifying it). Snapshots the worktree working tree and
# runs the WSL-side profiler.
# Usage: bash mt_prof_foreign_free_win.sh <worktree-name> <label> <foreign|own> <locality|capacity>
set -e
R=$REPO
(cd $R/worktrees/$1 && git ls-files -co --exclude-standard -z | tar --null -T - -cf $R/.rush/wsl/$2.tar)
wsl.exe -e bash -lc "bash $REPO_WSL/.rush/wsl/mt_prof_foreign_free.sh $1 $2 ${3:-foreign} ${4:-locality}"
