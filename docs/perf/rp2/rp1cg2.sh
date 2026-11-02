#!/bin/bash
# RP-1/RP-2: callgrind of the owner phase without harness statistics: functions of (allocs=N) minus (allocs=0), 16 segments x 64 B, 3 workers
source ~/.cargo/env
W=<scratch dir>
S=$HOME/w2m; BIN=$S/tgt-base/release/examples/g1_rp1_probe
cp $W/rp1/g1_rp1_probe.rs $S/base/examples/
(cd $S/base && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=$S/tgt-base cargo build -q --release --example g1_rp1_probe --features production) || exit 1
O=$W/rp1/out2; mkdir -p $O; C=$S/cg; mkdir -p $C; cd $C
N=943718
export RP1_NOSTATS=1
: > $O/totals.txt
for who in sefer mi; do
  for n in 0 $N; do
    valgrind --tool=callgrind --callgrind-out-file=$C/rp2.$who.$n.out $BIN $who 16 64 3 $n > /dev/null 2>&1
    echo "$who allocs=$n $(callgrind_annotate $C/rp2.$who.$n.out 2>/dev/null | grep 'PROGRAM TOTALS' | awk '{print $1}')" >> $O/totals.txt
    callgrind_annotate --threshold=99.9 $C/rp2.$who.$n.out 2>/dev/null | grep -E '^ *[0-9,]+ ' | sed -e 's#\[[^]]*\]##' | cut -c1-230 > $O/fn.$who.$n.txt
  done
done
echo rp2cg-done
