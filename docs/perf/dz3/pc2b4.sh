source ~/.cargo/env
S=$HOME/w2m; W=<scratch dir>; O=$W/pc2/final; C=$S/cg
rm -rf $S/dz3b; cp -r $S/dz3 $S/dz3b
sed -i 's/^const SWEEP_BUDGET: usize = 8;/const SWEEP_BUDGET: usize = 4;/' $S/dz3b/src/registry/heap_core_xthread/route_pin_cache.rs
grep -c "SWEEP_BUDGET: usize = 4" $S/dz3b/src/registry/heap_core_xthread/route_pin_cache.rs
(cd $S/dz3b && CARGO_TARGET_DIR=$S/tgt-dz3b cargo build -q --release --example g1_rhythm_probe --features production) || exit 1
BASE=$S/tgt-base/release/examples/g1_rhythm_probe; NEW=$S/tgt-dz3/release/examples/g1_rhythm_probe; NB=$S/tgt-dz3b/release/examples/g1_rhythm_probe
cd $C
: > $O/b4_ir.txt
for m in 16 64; do for o in cyclic seq random; do
  valgrind --tool=callgrind --callgrind-out-file=$C/pb.out $NB pincycle $m $o > /dev/null 2>&1
  echo "b4 $m $o total=$(callgrind_annotate $C/pb.out 2>/dev/null | grep 'PROGRAM TOTALS' | awk '{print $1}' | tr -d ,)" >> $O/b4_ir.txt
done; done
: > $O/b4_time.txt
for i in 1 2 3 4 5 6 7 8 9; do for m in 16 64; do for o in cyclic seq random; do
  for arm in base new b4; do
    case $arm in base) B=$BASE;; new) B=$NEW;; b4) B=$NB;; esac
    $B pincycle $m $o | sed "s/^{/{\"arm\":\"$arm\",/" >> $O/b4_time.txt
  done
done; done; done
echo done
