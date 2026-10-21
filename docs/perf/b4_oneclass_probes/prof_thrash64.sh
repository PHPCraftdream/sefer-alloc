#!/bin/bash
# Features: production.
# Судья: thrash64 — alloc/free Ir/op + EstCycles; NO-GO-гейт ≤ +1 %.
# Запуск ВЕЗДЕ с явным `timeout 600` и `CARGO_TARGET_DIR` вне репо (изолированный target); переменные: REPO (корень репо, Windows-путь), REPO_WSL (тот же корень под /mnt/<drive>/… в WSL), AB_DIR (scratch, по умолчанию $HOME/ab), SCRATCH (каталог логов вне репо).
# Allocator-only deterministic profile: larson-like churn (768 live, 16..512 B)
# with alloc/dealloc behind #[inline(never)] wrappers (examples/zz_thrash64).
# Usage (WSL): bash prof_ao.sh <label> [sefer|mimalloc]   (expects .rush/wsl/<label>.tar)
# Two passes per wrapper: Ir (with per-line marginal markup) and cache-sim
# (D1/LL misses + EstimatedCycles, iai-style).
AB_DIR=${AB_DIR:-$HOME/ab}
set -e
L=$1; A=${2:-sefer}
D=$AB_DIR/$L; rm -rf $D; mkdir -p $D; tar -xf ${REPO_WSL}/.rush/wsl/$L.tar -C $D
cp ${REPO_WSL}/.rush/wsl/thrash64_probe.rs $D/examples/zz_thrash64.rs
printf '\n[[example]]\nname = "zz_thrash64"\nrequired-features = ["alloc-global"]\n' >> $D/Cargo.toml
(cd $D && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=$AB_DIR/tgt64-$L cargo build -q --release --example zz_thrash64 --features production)
echo "== activation: $($AB_DIR/tgt64-$L/release/examples/zz_thrash64 $A 10 2>&1 | head -2)"
cd $AB_DIR
for w in w_alloc w_free; do
  for k in 50000 100000; do valgrind --tool=callgrind --toggle-collect="*zz_thrash64::$w*" --callgrind-out-file=p64a.$L.$A.$w.$k.out $AB_DIR/tgt64-$L/release/examples/zz_thrash64 $A $k >/dev/null 2>&1; done
  t1=$(callgrind_annotate p64a.$L.$A.$w.50000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//'); t2=$(callgrind_annotate p64a.$L.$A.$w.100000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//')
  echo "== $A $w marginal Ir/op: $(( (t2 - t1) / 50000 ))"
  for k in 50000 100000; do callgrind_annotate --auto=yes --show-percs=no --threshold=100 p64a.$L.$A.$w.$k.out 2>/dev/null | awk '/-- Auto-annotated source: /{src=$NF; next} /^ *[0-9,]+ /{ if(src !~ /^\?\?\?/ && $0 !~ /=>/){ v=$1; gsub(",","",v); line=$0; sub(/^ *[0-9,]+ +/,"",line); s[src" | "line]+=v } } END{for(k in s) print s[k]"\t"k}' > p64x.$k.txt; done
  awk -F'\t' 'NR==FNR{a[$2]=$1;next}{d=$1-a[$2]; if(d>25000) printf "%6.1f\t%s\n", d/50000, $2}' p64x.50000.txt p64x.100000.txt | sort -rn | sed "s|$L/||; s|$AB_DIR/||" | grep -v "events annotated\|PROGRAM TOTALS" | head -30
  for k in 50000 100000; do valgrind --tool=callgrind --cache-sim=yes --toggle-collect="*zz_thrash64::$w*" --callgrind-out-file=p64c.$L.$A.$w.$k.out $AB_DIR/tgt64-$L/release/examples/zz_thrash64 $A $k >/dev/null 2>&1; done
  c1=$(callgrind_annotate p64c.$L.$A.$w.50000.out 2>/dev/null | grep -m1 TOTALS | sed 's/([0-9.,%]*)//g; s/TOTALS//; s/,//g')
  c2=$(callgrind_annotate p64c.$L.$A.$w.100000.out 2>/dev/null | grep -m1 TOTALS | sed 's/([0-9.,%]*)//g; s/TOTALS//; s/,//g')
  read -r ir d1 ll <<<"$(awk -v a="$c1" -v b="$c2" 'BEGIN{split(a,A);split(b,B);
    ir=(B[1]-A[1])/50000; d1=(B[4]+B[5]-A[4]-A[5])/50000; ll=(B[6]+B[7]-A[6]-A[7])/50000;
    printf "%.2f %.2f %.2f", ir, d1, ll}')"
  est=$(awk -v i="$ir" -v d="$d1" -v l="$ll" 'BEGIN{printf "%.1f", i + 10*d + 100*l}')
  echo "== $A $w cache: D1m/op=$d1 LLm/op=$ll EstCycles/op=$est"
done
