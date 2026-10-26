#!/bin/bash
P=/usr/lib/linux-tools-6.8.0-147/perf
B=~/ab/tgt-pmu/release/examples/b4_pmu_probe
cd ~/ab
taskset -c 2 $B inter 256 16 128 30000 0 64
taskset -c 2 $B inter 256 16 128 30000 0 0
$P record -q -o ~/ab/mix.data -e cycles:u -F 20000 -- taskset -c 2 $B inter 256 16 128 60000 0 64 > /dev/null 2>&1
$P report -i ~/ab/mix.data --no-children --sort symbol --stdio 2>/dev/null | grep -v '^#' | grep -v '^$' | head -40
