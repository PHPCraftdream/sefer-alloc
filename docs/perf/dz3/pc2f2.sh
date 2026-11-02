#!/bin/bash
# DZ-3 final measurement (tree in dz3_tree.txt): base main vs the change vs mimalloc.
# (1) counters (hit / miss) with counting builds, (2) callgrind program totals (extra Ir per free = (total(order) - total(seq)) / n),
# (3) time per free, interleaved processes
source ~/.cargo/env
W=<scratch dir>
S=$HOME/w2m
O=$W/pc2/final2; mkdir -p $O
rm -rf $S/dz3; mkdir -p $S/dz3; tar -xf $W/pc2/dz3.tar -C $S/dz3
cp $S/base/examples/g1_rhythm_probe.rs $S/dz3/examples/g1_rhythm_probe.rs
F_TIME="production"
F_CNT="production bench-internals internals alloc-stats"
(cd $S/dz3 && CARGO_TARGET_DIR=$S/tgt-dz3 cargo build -q --release --example g1_rhythm_probe --features "$F_TIME") || exit 1
(cd $S/dz3 && CARGO_TARGET_DIR=$S/tgt-dz3c cargo build -q --release --example g1_rhythm_probe --features "$F_CNT") || exit 1
BASE=$S/tgt-base/release/examples/g1_rhythm_probe
NEW=$S/tgt-dz3/release/examples/g1_rhythm_probe
BASEC=$S/tgt-basec/release/examples/g1_rhythm_probe
NEWC=$S/tgt-dz3c/release/examples/g1_rhythm_probe
{ echo "tree $(cat $W/pc2/dz3_tree.txt)"; echo "base commit 6b8dd8e3 (main; the pin cache is unchanged since c0ee7dfb)"; sha256sum $BASE $NEW $BASEC $NEWC | sed "s#$S/##"; rustc -vV | head -3; } > $O/ident.txt
: > $O/counters.txt
for rep in 1 2; do
  for m in 4 5 8 16 64; do
    for o in cyclic seq random; do
      echo "base $($BASEC pincycle $m $o)" >> $O/counters.txt
      echo "new $($NEWC pincycle $m $o)" >> $O/counters.txt
    done
  done
done
C=$S/cg; cd $C
: > $O/ir.txt
for arm in base new; do
  if [ $arm = base ]; then B=$BASE; else B=$NEW; fi
  for m in 4 5 8 16 64; do
    for o in cyclic seq random; do
      valgrind --tool=callgrind --callgrind-out-file=$C/pf.out $B pincycle $m $o > /dev/null 2>&1
      echo "$arm $m $o total=$(callgrind_annotate $C/pf.out 2>/dev/null | grep 'PROGRAM TOTALS' | awk '{print $1}' | tr -d ,)" >> $O/ir.txt
    done
  done
done
: > $O/time.txt
for i in 1 2 3 4 5 6 7 8 9; do
  for m in 4 5 8 16 64; do
    for o in cyclic seq random; do
      $BASE pincycle $m $o | sed 's/^{/{"arm":"base",/' >> $O/time.txt
      $NEW pincycle $m $o | sed 's/^{/{"arm":"new",/' >> $O/time.txt
      $BASE pincycle $m $o mi | sed 's/^{/{"arm":"mi",/' >> $O/time.txt
    done
  done
done
echo pc2f2-done
