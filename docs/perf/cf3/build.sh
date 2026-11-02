#!/bin/bash
# CF-3: build the judge once per huge-page threshold (sefer_pool arm, WJ_THP), time and evidence builds.
source ~/.cargo/env
W=${CF3_DIR:?set CF3_DIR}
S=$HOME/w2m
O=$W/out; mkdir -p $O/bin
rm -rf $S/cf3; mkdir -p $S/cf3
tar -xf $W/tree.tar -C $S/cf3
EX=release/examples/g1_wild_judge
for N in 0 1 4 16 64 1024; do
  (cd $S/cf3 && WJ_POOL=4 WJ_THP=$N CARGO_TARGET_DIR=$S/tgt-cf3 cargo build -q --release --example g1_wild_judge --features production) || exit 1
  cp $S/tgt-cf3/$EX $O/bin/time_$N
  (cd $S/cf3 && WJ_POOL=4 WJ_THP=$N CARGO_TARGET_DIR=$S/tgt-cf3e cargo build -q --release --example g1_wild_judge --features "production internals bench-internals") || exit 1
  cp $S/tgt-cf3e/$EX $O/bin/ev_$N
done
{ echo "tree $(cat $W/tree.txt)"; (cd $O/bin && sha256sum *); rustc -vV | head -3; uname -r; lscpu | grep 'Model name'; cat /sys/kernel/mm/transparent_hugepage/enabled; } > $O/ident.txt
echo built
