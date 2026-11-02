#!/bin/bash
# DZ-3 experiment (scratch): PIN_CACHE_SLOTS 4 -> 16 / 64; Ir per foreign free (callgrind) and time per free, vs base 4 and mimalloc
source ~/.cargo/env
W=<scratch dir>
S=$HOME/w2m
O=$W/pc2/oute; mkdir -p $O
for n in 16 64; do
  rm -rf $S/pin$n; mkdir -p $S/pin$n; tar -xf $W/base.tar -C $S/pin$n
  sed -i "s/^const PIN_CACHE_SLOTS: usize = 4;/const PIN_CACHE_SLOTS: usize = $n;/" $S/pin$n/src/registry/heap_core_xthread/route_pin_cache.rs
  grep -c "PIN_CACHE_SLOTS: usize = $n" $S/pin$n/src/registry/heap_core_xthread/route_pin_cache.rs
  cp $S/base/examples/g1_rhythm_probe.rs $S/pin$n/examples/g1_rhythm_probe.rs
  (cd $S/pin$n && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=$S/tgt-pin$n cargo build -q --release --example g1_rhythm_probe --features production) || exit 1
done
B4=$S/tgt-base/release/examples/g1_rhythm_probe
B16=$S/tgt-pin16/release/examples/g1_rhythm_probe
B64=$S/tgt-pin64/release/examples/g1_rhythm_probe
C=$S/cg; cd $C
: > $O/ir.txt
for slots in 4 16 64; do
  eval B=\$B$slots
  for m in 5 8 16 64; do
    for o in cyclic seq random; do
      valgrind --tool=callgrind --callgrind-out-file=$C/pe.$slots.$m.$o.out $B pincycle $m $o > /dev/null 2>&1
      callgrind_annotate --inclusive=yes --tree=caller $C/pe.$slots.$m.$o.out 2>/dev/null > $C/pe.tree.txt
      pf=$(grep -E '^ *[0-9,]+ \(.*\)\s+\*\s+.*publish_foreign' $C/pe.tree.txt | sed -e 's#\[[^]]*\]##' | head -1 | awk '{print $1}')
      echo "$slots $m $o publish_foreign_incl=$pf" >> $O/ir.txt
    done
  done
done
: > $O/time.txt
for i in 1 2 3 4 5 6 7 8 9; do
  for m in 5 8 16 64; do
    for o in cyclic seq random; do
      for slots in 4 16 64; do
        eval B=\$B$slots
        $B pincycle $m $o | sed "s/^{/{\"slots\":$slots,/" >> $O/time.txt
      done
    done
  done
done
echo pc2e-done
