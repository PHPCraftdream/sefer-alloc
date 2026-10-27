#!/bin/bash
source ~/.cargo/env 2>/dev/null || true
# U31 peak-RSS check: growth16 probe and ph6b_mt_ab, base vs variant, ABBA x 20.
set -e
OUT=$1; : > $OUT
for q in $(seq 0 99); do
  if [ $((q % 2)) -eq 0 ]; then order="A B B A"; else order="B A A B"; fi
  for arm in $order; do
    t=ut-u31base0; [ $arm = B ] && t=ut-u31v3
    /usr/bin/time -f %M -o /tmp/u31m ~/ab/tgt-$t/release/examples/ph6b_mt_ab >/dev/null 2>&1; m=$(cat /tmp/u31m)
    echo "{\"quad\":$q,\"arm\":\"$arm\",\"mt_maxrss_kib\":$m}" >> $OUT
  done
done
