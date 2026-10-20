#!/bin/bash
# mt_rss_pin_win.sh — Windows Git Bash wrapper for the R26-4 subprocess RSS
# probe (X-pin review blocker 2). Builds TWO snapshot arms and runs ONE
# PROCESS per arm; computes delta RSS. Read-only for the worktrees.
#   Arm A: snapshot of base commit 2730d02a (git archive — NO git write)
#   Arm B: snapshot of the current working tree of worktrees/b4-mt
# Usage: bash mt_rss_pin_win.sh <label-prefix>
set -e
R=$REPO
WT=$R/worktrees/b4-mt
LP=${1:-mt-xp3-rss}
# Arm A: archive the base commit into a temp dir, tar it.
rm -rf /tmp/mt-rss-a; mkdir -p /tmp/mt-rss-a
(cd $WT && git archive 2730d02a | tar -x -C /tmp/mt-rss-a)
(cd /tmp/mt-rss-a && tar -cf $R/.rush/wsl/$LP-a.tar .)
# Arm B: snapshot of the working tree.
(cd $WT && git ls-files -co --exclude-standard -z | tar --null -T - -cf $R/.rush/wsl/$LP-b.tar)
wsl.exe -e bash -lc "bash $REPO_WSL/.rush/wsl/mt_rss_pin.sh $LP"
