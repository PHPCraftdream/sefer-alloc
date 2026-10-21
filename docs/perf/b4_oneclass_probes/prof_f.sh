#!/bin/bash
# Features: production (переменная FEAT, 5-й аргумент).
# Судья: mstress / larson — MARGINAL Ir/step (callgrind, 50k→100k).
# Запуск ВЕЗДЕ с явным `timeout 600` и `CARGO_TARGET_DIR` вне репо (изолированный target); переменные: REPO (корень репо, Windows-путь), REPO_WSL (тот же корень под /mnt/<drive>/… в WSL), AB_DIR (scratch, по умолчанию $HOME/ab), SCRATCH (каталог логов вне репо).
# Deterministic callgrind profile of one malloc-bench-rs workload at T=1.
# Usage (from Windows Git Bash):
#   wsl.exe -e bash -lc "bash ${REPO_WSL}/.rush/wsl/prof.sh <worktree-name> <label> [larson|mstress] [sefer|mimalloc]"
# Expects .rush/wsl/<label>.tar made by prof_win.sh (working tree snapshot).
# ${REPO}/worktrees/<worktree-name> into $AB_DIR/<label>, builds
# examples/zz_larson_probe (production features, release + line tables), runs
# callgrind at 50k and 100k ops and prints the MARGINAL Ir per op (total and by
# source file and by source line, top entries). Read-only for the worktree.
AB_DIR=${AB_DIR:-$HOME/ab}
set -e
WT=${REPO}/worktrees/$1; L=$2; W=${3:-larson}; A=${4:-sefer}; FEAT="${5:-production}"
D=$AB_DIR/$L; rm -rf $D; mkdir -p $D; tar -xf ${REPO_WSL}/.rush/wsl/$L.tar -C $D
cp ${REPO_WSL}/.rush/wsl/larson_probe.rs $D/examples/zz_larson_probe.rs
printf '\n[[example]]\nname = "zz_larson_probe"\nrequired-features = ["alloc-global"]\n' >> $D/Cargo.toml
(cd $D && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=$AB_DIR/tgtp-$L cargo build -q --release --example zz_larson_probe --features "$FEAT")
cd $AB_DIR
for k in 50000 100000; do valgrind --tool=callgrind --callgrind-out-file=pf.$L.$A.$W.$k.out $AB_DIR/tgtp-$L/release/examples/zz_larson_probe $A $W $k >/dev/null 2>&1; done
t1=$(callgrind_annotate pf.$L.$A.$W.50000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//'); t2=$(callgrind_annotate pf.$L.$A.$W.100000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//')
echo "MARGINAL Ir/op ($A $W T=1): $(( (t2 - t1) / 50000 ))"
for k in 50000 100000; do callgrind_annotate --inclusive=no --threshold=100 pf.$L.$A.$W.$k.out 2>/dev/null | grep -E '^ *[0-9,]+ \(' | grep -v "=>" | sed -E 's/^ *([0-9,]+) \([^)]*\) +/\1\t/' | tr -d ',' > pfs.$k.txt; done
echo "--- by source file (Ir/op)"
for k in 50000 100000; do awk -F'\t' '{split($2,p,":"); s[p[1]]+=$1} END{for(f in s) print s[f]"\t"f}' pfs.$k.txt > pff.$k.txt; done
awk -F'\t' 'NR==FNR{a[$2]=$1;next}{d=$1-a[$2]; if(d>2500) printf "%7.1f\t%s\n", d/50000, $2}' pff.50000.txt pff.100000.txt | sort -rn | sed "s|$D/||" | head -25
