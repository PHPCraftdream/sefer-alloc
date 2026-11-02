#!/bin/bash
# CF-1: build the judge once per pool size (sefer_pool arm), time builds (production) and evidence builds.
source ~/.cargo/env
W=${CF1_DIR:?set CF1_DIR}
S=$HOME/w2m
O=$W/out; mkdir -p $O/bin
rm -rf $S/cf1; mkdir -p $S/cf1
tar -xf $W/tree.tar -C $S/cf1
EX=release/examples/g1_wild_judge
for N in 0 4 8 16 32 64 1024; do
  (cd $S/cf1 && WJ_POOL=$N CARGO_TARGET_DIR=$S/tgt-cf1 cargo build -q --release --example g1_wild_judge --features production) || exit 1
  cp $S/tgt-cf1/$EX $O/bin/time_$N
  (cd $S/cf1 && WJ_POOL=$N CARGO_TARGET_DIR=$S/tgt-cf1e cargo build -q --release --example g1_wild_judge --features "production internals bench-internals") || exit 1
  cp $S/tgt-cf1e/$EX $O/bin/ev_$N
done
{ echo "tree $(cat $W/tree.txt)"; (cd $O/bin && sha256sum *); rustc -vV | head -3; uname -r; lscpu | grep 'Model name'; } > $O/ident.txt
echo built
