#!/bin/bash
# J7 step 2: callgrind Ir of the growth scenarios of the WG-1 judge, sefer vs mimalloc, from the Linux judge binary of J6
# (docs/perf/j6/build.sh; the same main 875ddcd6 build). Per scenario and arm one callgrind run at scale SCALE; the judge's own
# `ops` field (alloc + free calls, from a plain run at the same scale) turns totals into Ir per operation.
# Raw output: one callgrind_annotate table per run and the judge's JSON line.
#   J7_BIN=<judge binary> J7_OUT=<raw dir> [J7_SCALE=0.25] bash j7.sh
set -e
: "${J7_BIN:?set J7_BIN to the judge binary}"
: "${J7_OUT:?set J7_OUT to the raw output dir}"
SCALE=${J7_SCALE:-0.25}
mkdir -p "$J7_OUT"
cd /tmp
for sc in grow64 growmix regrow; do
  for arm in sefer mi; do
    "$J7_BIN" $sc $arm 1 $SCALE | tail -1 > "$J7_OUT/run.$sc.$arm.json"
    valgrind --tool=callgrind --callgrind-out-file=/tmp/j7.$sc.$arm.out "$J7_BIN" $sc $arm 1 $SCALE > /dev/null 2>&1
    callgrind_annotate /tmp/j7.$sc.$arm.out 2>/dev/null | sed -e 's#/home/[^/]*/#<scratch>/#g' | sed -n '/PROGRAM TOTALS/,$p' | head -40 > "$J7_OUT/ir.$sc.$arm.txt"
  done
done
echo "scale $SCALE" > "$J7_OUT/scale.txt"
echo ok
