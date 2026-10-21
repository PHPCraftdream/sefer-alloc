#!/bin/bash
# oc_rem2_perf.sh (Windows side): sequential D1-prime-port2 perf judges for one shoulder.
# Usage: bash oc_rem2_perf.sh <a|b>
set -u
SH=${1:-a}
R=${REPO:?set REPO to the repo path}
W=$R/.rush/wsl
DOCS=$R/worktrees/b4-oneclass/docs/perf
WSLW=${REPO_WSL:?set REPO_WSL to the repo path inside WSL}/.rush/wsl
A3=1ff7e14000bb078836d7e455732322b857192bcf69439d2e95cbb03c97c9a1ce
if [ "$SH" = a ]; then
  TAG=base
  TREE=086180efab1faf2bfa9889f4194355bb83cda9a69dbec83a5e2b9e9f992c89b3
  IDENT="identity: shoulder=a base=8da6402d patch=oc_mem_hooks_a3.patch(sha256=$A3) write-tree=02db40ed426458ebdb351babdcf6e263c9b3cd36 source-manifest-sha256=$TREE"
else
  TAG=b
  TREE=58f83d6b71a7e8a16341cae2f1f9c6ccb65af351508659e7cfc85af9bf1e5fc0
  IDENT="identity: shoulder=b base=8da6402d (origin/main) merged with 294ff32a (b4-oneclass D1-prime, mid-merge resolved) write-tree=19aa0fcf11fa955a7a245f0d338141891ac38d92 source-manifest-sha256=$TREE tree-diff-sha256(src+crates)=$TREE"
fi
run_cell() { # cell
  local cell=$1
  local label=ocport2-$TAG-$cell
  local log=$DOCS/_raw_b4_oneclass_port2-$TAG-$cell.log
  local cmd
  case $cell in
    mstress)  cmd="bash $WSLW/prof_f.sh ocport2 $label mstress sefer";;
    larson)   cmd="bash $WSLW/prof_f.sh ocport2 $label larson sefer";;
    ao)       cmd="bash $WSLW/prof_ao.sh $label sefer";;
    aoc)      cmd="bash $WSLW/prof_ao_c.sh $label sefer production";;
    batch)    cmd="bash $WSLW/prof_batch.sh $label sefer";;
    batchc)   cmd="bash $WSLW/prof_batch_c.sh $label sefer production";;
    batchl)   cmd="bash $WSLW/prof_batchl.sh $label sefer";;
    foreign)  cmd="bash $WSLW/mt_prof_foreign_free.sh ocport2 $label foreign locality";;
    thrash64) cmd="bash $WSLW/prof_thrash64.sh $label sefer";;
    *) echo "unknown cell $cell"; return 1;;
  esac
  echo "=== CELL $cell start $(date -u +%T)"
  wsl.exe -e bash -lc "bash $WSLW/oc_rem2_tar.sh $SH $WSLW/$label.tar"
  { echo "tree-diff-sha256 $TREE"
    echo "$IDENT"
    echo "features: production"
    echo "cmd: $cmd (via $WSLW tar snapshot $label.tar)"
  } > "$log"
  wsl.exe -e bash -lc "$cmd" >> "$log" 2>&1
  echo "RC=$?" >> "$log"
  echo "=== CELL $cell RC=$?"
}
for c in mstress larson ao aoc batch batchc batchl foreign thrash64; do run_cell $c; done
echo "ALL_CELLS_DONE $TAG"
