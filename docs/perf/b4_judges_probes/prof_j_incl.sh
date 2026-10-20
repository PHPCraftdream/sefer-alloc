#!/bin/bash
# Inclusive-cost probe (b4-judges step 4): larson T=1.
# callgrind_annotate here resolves costs to SOURCE LINES, so inclusive
# per-function costs are measured with --toggle-collect on the event
# functions: marginal Ir/op (50k->100k) while inside the matched functions
# = inclusive event cost x events/op (per-op contribution directly).
# Also dumps one untoggled profile aggregated per source file (self costs)
# for the harness-inflation check (larson lib.rs vs floor bumpa lib.rs).
# Usage (WSL): bash prof_j_incl.sh <label> [sefer|mimalloc] [larson|mstress]
set -e
L=$1; A=${2:-sefer}; W=${3:-larson}
D=~/ab/$L; rm -rf $D; mkdir -p $D; tar -xf ${REPO_WSL:?set REPO_WSL to the repo root as seen from WSL}/.rush/wsl/$L.tar -C $D
cp ${REPO_WSL:?set REPO_WSL to the repo root as seen from WSL}/.rush/wsl/larson_probe.rs $D/examples/zz_larson_probe.rs
printf '\n[[example]]\nname = "zz_larson_probe"\nrequired-features = ["alloc-global"]\n' >> $D/Cargo.toml
(cd $D && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgti-$L cargo build -q --release --example zz_larson_probe --features production)
cd ~/ab
BIN=~/ab/tgti-$L/release/examples/zz_larson_probe
# 1) toggle-collect per event function
for g in refill_magazine_slow finish_magazine_refill flush_class_internal alloc_with_class dealloc_own_thread_with_base; do
  for k in 50000 100000; do
    timeout 600 valgrind --tool=callgrind --toggle-collect="*$g*" --callgrind-out-file=pji.$L.$A.$W.$g.$k.out $BIN $A $W $k >/dev/null 2>&1
  done
  t1=$(callgrind_annotate pji.$L.$A.$W.$g.50000.out 2>/dev/null | awk '/PROGRAM TOTALS/{gsub(",","");print $1;exit}')
  t2=$(callgrind_annotate pji.$L.$A.$W.$g.100000.out 2>/dev/null | awk '/PROGRAM TOTALS/{gsub(",","");print $1;exit}')
  echo "TOGGLE $A $W $g marginal Ir/op: $(( (t2 - t1) / 50000 ))"
done
# 2) untoggled per-file self-cost aggregation (marginal 50k->100k)
for k in 50000 100000; do
  timeout 600 valgrind --tool=callgrind --callgrind-out-file=pji.$L.$A.$W.full.$k.out $BIN $A $W $k >/dev/null 2>&1
  callgrind_annotate --auto=no --threshold=100 pji.$L.$A.$W.full.$k.out > /dev/null 2>&1 || true
  callgrind_annotate --auto=yes --show-percs=no --threshold=100 pji.$L.$A.$W.full.$k.out 2>/dev/null | awk '/-- Auto-annotated source: /{src=$NF; next} /^ *[0-9,]+ /{ if(src !~ /^\?\?\?/ && $0 !~ /=>/){ v=$1; gsub(",","",v); s[src]+=v } } END{for(k in s) print s[k]"\t"k}' > pji.files.$k.txt
done
tot1=$(grep -m1 TOTALS pji.$L.$A.$W.full.50000.out >/dev/null; callgrind_annotate pji.$L.$A.$W.full.50000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//')
tot2=$(callgrind_annotate pji.$L.$A.$W.full.100000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//')
echo "FULLTOTAAL $A $W marginal Ir/op: $(( (tot2 - tot1) / 50000 ))"
paste pji.files.50000.txt pji.files.100000.txt | awk -F'\t' '{d=$3-$1; if(d>0) printf "SELFFILE %s marginal Ir/op: %.3f\n", $4, d/50000}' | sort -t: -k2 -rn | head -25
