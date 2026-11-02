#!/bin/bash
# RP-2: does a larger empty-segment pool remove the post-wave cost? sefer (pool 4) vs sefer8 (Throughput: pool 8) vs mi
source ~/.cargo/env
W=<scratch dir>
S=$HOME/w2m
cp $W/rp1/g1_rp1_probe.rs $S/base/examples/
(cd $S/base && CARGO_TARGET_DIR=$S/tgt-base cargo build -q --release --example g1_rp1_probe --features production) || exit 1
BIN=$S/tgt-base/release/examples/g1_rp1_probe
O=$W/rp1/out4; mkdir -p $O
: > $O/raw.txt
for cfg in "4 64 3" "8 64 3" "16 64 3"; do
  set -- $cfg
  n=$(( $1 * (4194304 / $2) * 9 / 10 ))
  for i in 1 2 3 4 5 6 7; do
    for who in sefer sefer8 mi; do
      $BIN $who $1 $2 $3 $n >> $O/raw.txt
    done
  done
done
echo rp1b-done
