#!/bin/bash
# MT1: perf stat user-mode cycles/instructions/L1D misses, g1_mt_one T=1, sefer vs mi, interleaved, 7 rounds
S=$HOME/w2m; B=$S/tgt-base/release/examples/g1_mt_one
O=<scratch dir>/mt1c/perf_stat_raw.txt
P=/usr/lib/linux-tools-6.8.0-147/perf
: > $O
for i in 1 2 3 4 5 6 7; do
  for w in mstress larson; do
    for a in sefer mi; do
      out=$($P stat -x, -e cycles:u,instructions:u,L1-dcache-load-misses:u -- $B $w $a 1 3000000 2>&1 >/dev/null | tr '\n' ' ')
      echo "run$i $w $a $out" >> $O
    done
  done
done
echo mt1p-done
