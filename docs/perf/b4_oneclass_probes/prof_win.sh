#!/bin/bash
# Windows Git Bash wrapper: snapshot a worktree's working tree and profile it in WSL.
# Usage: bash .rush/wsl/prof_win.sh <worktree-name> <label> [larson|mstress] [sefer|mimalloc]
set -e
R=${REPO:?set REPO to the repo path}
(cd $R/worktrees/$1 && git ls-files -co --exclude-standard -z | tar --null -T - -cf $R/.rush/wsl/$2.tar)
wsl.exe -e bash -lc "bash ${REPO_WSL:?set REPO_WSL to the repo path inside WSL}/.rush/wsl/prof.sh $1 $2 ${3:-larson} ${4:-sefer}"
