#!/bin/bash
# MT1: hottest annotated instructions (share of all cycles:u samples), sefer vs mi, mstress and larson
S=$HOME/w2m; B=$S/tgt-base/release/examples/g1_mt_one
O=<scratch dir>/mt1c
P=/usr/lib/linux-tools-6.8.0-147/perf
cd /tmp
for w in mstress larson; do
  for a in sefer mi; do
    [ -f /tmp/mt1r.$w.$a.data ] || $P record -q -e cycles:u -F 20000 -o /tmp/mt1r.$w.$a.data -- $B $w $a 1 3000000 > /dev/null 2>&1
    $P report -i /tmp/mt1r.$w.$a.data --stdio --no-children --sort symbol 2>/dev/null | grep -v '^#' | grep -v '^$' | head -10 | cut -c1-110 > $O/perf_sym2.$w.$a.txt
    $P annotate -i /tmp/mt1r.$w.$a.data --stdio 2>/dev/null > /tmp/mt1r.$w.$a.ann
    grep -E '^ +[0-9]+\.[0-9]+ +:' /tmp/mt1r.$w.$a.ann | awk '$1>=0.8' | sort -rn | head -30 > $O/perf_ann2.$w.$a.txt
  done
done
echo mt1r2-done
