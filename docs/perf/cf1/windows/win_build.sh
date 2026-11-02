#!/bin/bash
# CF-1 Windows: judge binaries per pool size (time builds: production), from the committed tree in tree.tar.
set -e
W=${CF1_DIR:?set CF1_DIR}
S=$W/winsrc
rm -rf $S; mkdir -p $S $W/winout/bin
tar -xf $W/tree.tar -C $S
cd $S
for N in 0 4 8 16 32 64 1024; do
  WJ_POOL=$N CARGO_TARGET_DIR=$W/wintgt cargo build -q --release --example g1_wild_judge --features production
  cp $W/wintgt/release/examples/g1_wild_judge.exe $W/winout/bin/time_$N.exe
done
{ echo "tree $(cat $W/tree.txt)"; (cd $W/winout/bin && sha256sum *); rustc -vV | head -3; wmic cpu get name | sed -n 2p; cmd //c ver; } > $W/winout/ident.txt
echo built
