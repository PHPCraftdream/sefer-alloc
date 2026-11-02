#!/bin/bash
# CF-2: route pin cache slot count sweep. Arms: base (main @ 3aab9681, SeferAlloc::new, 32 slots, no mask)
# and the change (tree in tree.txt) built with PC_SLOTS = 4 8 16 32 64; mimalloc through the same probe.
# (1) counters (hit rate, resolved slots) with counting builds, (2) callgrind program totals (hit-path cost),
# (3) time per foreign free, interleaved processes.
source ~/.cargo/env
W=<scratch dir>
S=$HOME/w2m
O=$W/out; mkdir -p $O/bin
rm -rf $S/cf2new $S/cf2base; mkdir -p $S/cf2new $S/cf2base
tar -xf $W/new.tar -C $S/cf2new
tar -xf $W/base.tar -C $S/cf2base
F_TIME="production"
F_CNT="production bench-internals internals alloc-stats"
EX=release/examples/g1_rhythm_probe
for N in 4 8 16 32 64; do
  (cd $S/cf2new && PC_SLOTS=$N CARGO_TARGET_DIR=$S/tgt-cf2 cargo build -q --release --example g1_rhythm_probe --features "$F_TIME") || exit 1
  cp $S/tgt-cf2/$EX $O/bin/time_$N
  (cd $S/cf2new && PC_SLOTS=$N CARGO_TARGET_DIR=$S/tgt-cf2c cargo build -q --release --example g1_rhythm_probe --features "$F_CNT") || exit 1
  cp $S/tgt-cf2c/$EX $O/bin/cnt_$N
done
(cd $S/cf2base && CARGO_TARGET_DIR=$S/tgt-cf2b cargo build -q --release --example g1_rhythm_probe --features "$F_TIME") || exit 1
cp $S/tgt-cf2b/$EX $O/bin/time_base
(cd $S/cf2base && CARGO_TARGET_DIR=$S/tgt-cf2bc cargo build -q --release --example g1_rhythm_probe --features "$F_CNT") || exit 1
cp $S/tgt-cf2bc/$EX $O/bin/cnt_base
{ echo "tree $(cat $W/tree.txt)"; echo "base commit 3aab9681"; (cd $O/bin && sha256sum *); rustc -vV | head -3; uname -r; lscpu | grep 'Model name'; } > $O/ident.txt
echo built
ARMS="base 4 8 16 32 64"
MS="4 5 8 16 32 64"
: > $O/counters.txt
for rep in 1 2; do
  for m in $MS; do
    for o in cyclic seq random; do
      for a in $ARMS; do
        echo "$a $($O/bin/cnt_$a pincycle $m $o)" >> $O/counters.txt
      done
    done
  done
done
echo counters-done
C=$HOME/w2m/cg; mkdir -p $C
: > $O/ir.txt
for a in $ARMS; do
  for m in 4 16 32 64; do
    for o in seq cyclic; do
      valgrind --tool=callgrind --callgrind-out-file=$C/cf2.out $O/bin/time_$a pincycle $m $o > /dev/null 2>&1
      echo "$a $m $o total=$(callgrind_annotate $C/cf2.out 2>/dev/null | grep 'PROGRAM TOTALS' | awk '{print $1}' | tr -d ,)" >> $O/ir.txt
    done
  done
done
echo ir-done
# Time phase: in repetition i the 7 arms (base, 4 8 16 32 64, mimalloc) run in the order rotated by i (a Latin square),
# so no arm owns a fixed position inside a cell.
: > $O/time.txt
TARMS=(base 4 8 16 32 64 mi)
for i in 0 1 2 3 4 5 6 7 8; do
  for m in 4 8 16 32 64; do
    for o in cyclic seq random; do
      for k in 0 1 2 3 4 5 6; do
        a=${TARMS[$(( (i + k) % 7 ))]}
        if [ $a = mi ]; then
          $O/bin/time_32 pincycle $m $o mi | sed 's/^{/{"arm":"mi",/' >> $O/time.txt
        else
          $O/bin/time_$a pincycle $m $o | sed "s/^{/{\"arm\":\"$a\",/" >> $O/time.txt
        fi
      done
    done
  done
done
echo cf2sweep-done
