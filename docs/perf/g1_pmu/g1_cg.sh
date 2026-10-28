#!/bin/bash
# G1 contemplation: callgrind self Ir per function, probe shapes (WSL side).
# Usage: bash g1_cg.sh <label> [set2]   (binary built by g1_pmu.sh <label>)
# Default set -> <label>.cg.log (top 30); set2 (bulk64, pair16) -> <label>.cg2.log (top 24).
set -e
L=$1; BASE=${G1_BASE:?}
BIN=~/ab/tgt-$L/release/examples/b4_pmu_probe
cd ~/ab
if [ "$2" = set2 ]; then
  LOG=$BASE/$L.cg2.log; TOP=24
  SC=("bulk64|bulk @ 64 64 6000" "pair16|@ 1 16 400000")
else
  LOG=$BASE/$L.cg.log; TOP=30
  SC=(
    "bulk16k|bulk @ 16384 64 20"
    "mixed16_64|mixed @ 256 16 64 128 400000"
    "mixed256_1024|mixed @ 256 256 1024 48 200000"
  )
fi
echo "bin_sha256 $(sha256sum $BIN | cut -c1-64)" > "$LOG"
for sc in "${SC[@]}"; do
  name=${sc%%|*}; args=${sc#*|}
  for a in sefer mi; do
    tag="$L.cg.$name.$a"
    timeout 1800 valgrind --tool=callgrind --callgrind-out-file=$tag.out "$BIN" ${args/@/$a} > $tag.stdout 2>/dev/null
    echo "RUN $name $a $(cat $tag.stdout)" >> "$LOG"
    callgrind_annotate --threshold=99.5 $tag.out | grep -E "^ *[0-9,]+ " | head -$TOP | sed "s/^/FN $name $a /" >> "$LOG"
  done
done
echo "== $LOG =="
