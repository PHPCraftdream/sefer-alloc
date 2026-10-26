#!/bin/bash
P=/usr/lib/linux-tools-6.8.0-147/perf
B=~/ab/tgt-pmu/release/examples/b4_pmu_probe
EV=cycles,instructions,branches,branch-misses,L1-dcache-loads,L1-dcache-load-misses,L1-dcache-stores,dTLB-load-misses,cache-misses
for a in sefer mi; do for L in 1 64 256 4096; do
  for N in 0 20000000; do
    $P stat -x, -e $EV -- taskset -c 2 $B $a $L 16 $N 2>&1 >/dev/null | awk -F, -v a=$a -v L=$L -v N=$N '{print a, L, N, $3, $1}'
  done
done; done
