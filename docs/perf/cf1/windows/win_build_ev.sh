#!/bin/bash
set -e
W=${CF1_DIR:?set CF1_DIR}
cd $W/winsrc
for N in 0 4 8 16 32 64 1024; do
  WJ_POOL=$N CARGO_TARGET_DIR=$W/wintgt_ev cargo build -q --release --example g1_wild_judge --features "production internals bench-internals"
  cp $W/wintgt_ev/release/examples/g1_wild_judge.exe $W/winout/bin/ev_$N.exe
done
(cd $W/winout/bin && sha256sum ev_*) >> $W/winout/ident.txt
echo built-ev
