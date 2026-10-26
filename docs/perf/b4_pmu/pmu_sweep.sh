#!/bin/bash
B=~/ab/tgt-pmu/release/examples/b4_pmu_probe
for rep in 1 2 3; do
  for L in 1 4 16 64 256 1024 4096; do
    for a in sefer mi; do taskset -c 2 $B $a $L 16 20000000; done
  done
done
