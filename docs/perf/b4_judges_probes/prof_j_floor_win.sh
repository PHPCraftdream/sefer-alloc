#!/bin/bash
# Windows Git Bash wrapper for the floor probe.
# Usage: bash .rush/wsl/prof_j_floor_win.sh <worktree-name> <label>
set -e
R=${REPO:?set REPO to the repo root}
(cd $R/worktrees/$1 && git ls-files -co --exclude-standard -z | tar --null -T - -cf $R/.rush/wsl/$2.tar)
wsl.exe -e bash -lc "bash ${REPO_WSL:?set REPO_WSL to the repo root as seen from WSL}/.rush/wsl/prof_j_floor.sh $2"
