#!/bin/bash
# Windows Git Bash wrapper for the unified runtime/const probe.
# Usage: bash .rush/wsl/prof_j_uniconst_win.sh <worktree-name> <label> [sefer|mimalloc]
set -e
R=${REPO:?set REPO to the repo root}
(cd $R/worktrees/$1 && git ls-files -co --exclude-standard -z | tar --null -T - -cf $R/.rush/wsl/$2.tar)
wsl.exe -e bash -lc "bash ${REPO_WSL:?set REPO_WSL to the repo root as seen from WSL}/.rush/wsl/prof_j_uniconst.sh $2 ${3:-sefer}"
