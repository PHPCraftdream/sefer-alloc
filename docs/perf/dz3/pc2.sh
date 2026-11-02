#!/bin/bash
# PC-2: callgrind Ir of the foreign free (publish_foreign inclusive) in the pincycle probe, cyclic / seq / random, m = 4, 5, 8, 16, 64
source ~/.cargo/env
W=<scratch dir>
S=$HOME/w2m
(cd $S/base && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=$S/tgt-base cargo build -q --release --example g1_rhythm_probe --features production) || exit 1
BIN=$S/tgt-base/release/examples/g1_rhythm_probe
O=$W/pc2/out; mkdir -p $O; C=$S/cg; cd $C
sha256sum $BIN | sed "s#$S/##" > $O/ident.txt
: > $O/rows.txt
for m in 4 5 8 16 64; do
  for o in cyclic seq random; do
    valgrind --tool=callgrind --callgrind-out-file=$C/pc2.$m.$o.out $BIN pincycle $m $o > /dev/null 2>&1
    tot=$(callgrind_annotate $C/pc2.$m.$o.out 2>/dev/null | grep 'PROGRAM TOTALS' | awk '{print $1}')
    callgrind_annotate --inclusive=yes --tree=caller $C/pc2.$m.$o.out 2>/dev/null > $C/pc2.$m.$o.tree.txt
    pf=$(grep -E '^ *[0-9,]+ \(.*\)\s+\*\s+.*publish_foreign' $C/pc2.$m.$o.tree.txt | sed -e 's#\[[^]]*\]##' | head -1 | awk '{print $1}')
    calls=$(grep -E '<.*publish_foreign.*\(([0-9,]+)x\)' $C/pc2.$m.$o.tree.txt | grep -oE '\(([0-9,]+)x\)' | tr -d '(),x' | sort -n | tail -1)
    echo "$m $o total=$tot publish_foreign_incl=$pf calls=$calls" >> $O/rows.txt
  done
done
echo pc2-done
