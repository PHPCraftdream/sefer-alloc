#!/bin/bash
# Features: production.
# Судья: batchl — large 258/round alloc/free Ir/блок.
# Запуск ВЕЗДЕ с явным `timeout 600` и `CARGO_TARGET_DIR` вне репо (изолированный target); переменные: REPO (корень репо, Windows-путь), REPO_WSL (тот же корень под /mnt/<drive>/… в WSL), AB_DIR (scratch, по умолчанию $HOME/ab), SCRATCH (каталог логов вне репо).
# Batch-probe deterministic profile: rounds of 256 fixed-size allocs + 2 Large (1 MiB) alloc/free per round
# LIFO free-all (examples/zz_batch_large_probe), to exercise magazine overflow.
# Usage (WSL): bash prof_batch.sh <label> [sefer|mimalloc]   (expects .rush/wsl/<label>.tar)
AB_DIR=${AB_DIR:-$HOME/ab}
set -e
L=$1; A=${2:-sefer}
D=$AB_DIR/$L; rm -rf $D; mkdir -p $D; tar -xf ${REPO_WSL}/.rush/wsl/$L.tar -C $D
cp ${REPO_WSL}/.rush/wsl/batch_large_probe.rs $D/examples/zz_batch_large_probe.rs
printf '\n[[example]]\nname = "zz_batch_large_probe"\nrequired-features = ["alloc-global"]\n' >> $D/Cargo.toml
(cd $D && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=$AB_DIR/tgtao-$L cargo build -q --release --example zz_batch_large_probe --features production)
cd $AB_DIR
for w in w_alloc w_free; do
  for k in 50000 100000; do valgrind --tool=callgrind --toggle-collect="*zz_batch_large_probe::$w*" --callgrind-out-file=pa.$L.$A.$w.$k.out $AB_DIR/tgtao-$L/release/examples/zz_batch_large_probe $A $k 16 >/dev/null 2>&1; done
  t1=$(callgrind_annotate pa.$L.$A.$w.50000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//'); t2=$(callgrind_annotate pa.$L.$A.$w.100000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//')
  echo "== $A $w marginal Ir/call (258/round): $(( (t2 - t1) / 50000 / 258 ))"
  for k in 50000 100000; do callgrind_annotate --auto=yes --show-percs=no --threshold=100 pa.$L.$A.$w.$k.out 2>/dev/null | awk '/-- Auto-annotated source: /{src=$NF; next} /^ *[0-9,]+ /{ if(src !~ /^\?\?\?/ && $0 !~ /=>/){ v=$1; gsub(",","",v); line=$0; sub(/^ *[0-9,]+ +/,"",line); s[src" | "line]+=v } } END{for(k in s) print s[k]"\t"k}' > pax.$k.txt; done
  awk -F'\t' 'NR==FNR{a[$2]=$1;next}{d=$1-a[$2]; if(d>25000) printf "%6.1f\t%s\n", d/50000, $2}' pax.50000.txt pax.100000.txt | sort -rn | sed "s|$L/||; s|$AB_DIR/||" | grep -v "events annotated\|PROGRAM TOTALS" | head -30
done
