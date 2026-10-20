#!/bin/bash
# B4 X-mix deterministic profile (j- prefix): larson-like churn (768 live)
# with the EXACT larson pick_size size distribution (~97% 16..512 B, ~3%
# 512 B..8 KiB) — examples/zz_xmix, wrappers #[inline(never)]. Two passes per
# wrapper: Ir (marginal Ir/op over 50k/100k) and cache-sim (D1/LL misses +
# EstimatedCycles). Usage (WSL): bash prof_j_xmix.sh <label> [sefer|mimalloc]
set -e
L=$1; A=${2:-sefer}
D=~/ab/$L; rm -rf $D; mkdir -p $D; tar -xf ${REPO_WSL:?set REPO_WSL to the repo root as seen from WSL}/.rush/wsl/$L.tar -C $D
cp ${REPO_WSL:?set REPO_WSL to the repo root as seen from WSL}/.rush/wsl/j_xmix_probe.rs $D/examples/zz_xmix.rs
printf '\n[[example]]\nname = "zz_xmix"\nrequired-features = ["alloc-global"]\n' >> $D/Cargo.toml
(cd $D && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgtao-$L cargo build -q --release --example zz_xmix --features production)
cd ~/ab
for w in w_alloc w_free; do
  for k in 50000 100000; do timeout 600 valgrind --tool=callgrind --toggle-collect="*zz_xmix::$w*" --callgrind-out-file=pa.$L.$A.$w.$k.out ~/ab/tgtao-$L/release/examples/zz_xmix $A $k >/dev/null 2>&1; done
  t1=$(callgrind_annotate pa.$L.$A.$w.50000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//'); t2=$(callgrind_annotate pa.$L.$A.$w.100000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//')
  echo "== j-xmix $A $w marginal Ir/op: $(( (t2 - t1) / 50000 ))"
  for k in 50000 100000; do callgrind_annotate --auto=yes --show-percs=no --threshold=100 pa.$L.$A.$w.$k.out 2>/dev/null | awk '/-- Auto-annotated source: /{src=$NF; next} /^ *[0-9,]+ /{ if(src !~ /^\?\?\?/ && $0 !~ /=>/){ v=$1; gsub(",","",v); line=$0; sub(/^ *[0-9,]+ +/,"",line); s[src" | "line]+=v } } END{for(k in s) print s[k]"\t"k}' > paxj.$k.txt; done
  awk -F'\t' 'NR==FNR{a[$2]=$1;next}{d=$1-a[$2]; if(d>25000) printf "%6.1f\t%s\n", d/50000, $2}' paxj.50000.txt paxj.100000.txt | sort -rn | sed "s|$L/||; s|$HOME/ab/||" | grep -v "events annotated\|PROGRAM TOTALS" | head -30
  for k in 50000 100000; do timeout 600 valgrind --tool=callgrind --cache-sim=yes --toggle-collect="*zz_xmix::$w*" --callgrind-out-file=pc.$L.$A.$w.$k.out ~/ab/tgtao-$L/release/examples/zz_xmix $A $k >/dev/null 2>&1; done
  c1=$(callgrind_annotate pc.$L.$A.$w.50000.out 2>/dev/null | grep -m1 TOTALS | sed 's/([0-9.,%]*)//g; s/TOTALS//; s/,//g')
  c2=$(callgrind_annotate pc.$L.$A.$w.100000.out 2>/dev/null | grep -m1 TOTALS | sed 's/([0-9.,%]*)//g; s/TOTALS//; s/,//g')
  read -r ir d1 ll <<<"$(awk -v a="$c1" -v b="$c2" 'BEGIN{split(a,A);split(b,B);
    ir=(B[1]-A[1])/50000; d1=(B[4]+B[5]-A[4]-A[5])/50000; ll=(B[6]+B[7]-A[6]-A[7])/50000;
    printf "%.2f %.2f %.2f", ir, d1, ll}')"
  est=$(awk -v i="$ir" -v d="$d1" -v l="$ll" 'BEGIN{printf "%.1f", i + 10*d + 100*l}')
  echo "== j-xmix $A $w cache: D1m/op=$d1 LLm/op=$ll EstCycles/op=$est"
done
