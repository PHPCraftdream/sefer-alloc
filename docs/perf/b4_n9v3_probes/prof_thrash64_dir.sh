#!/bin/bash
# Usage: bash .rush/wsl/prof_thrash64_dir.sh <abs-dir-with-git-tree> <label> [sefer|mimalloc]
set -e
R=${REPO}
(cd "$1" && git ls-files -co --exclude-standard -z | tar --null -T - -cf $R/.rush/wsl/$2.tar)
wsl.exe -e bash -lc "bash ${REPO_WSL}/.rush/wsl/prof_thrash64.sh $2 ${3:-sefer}"
