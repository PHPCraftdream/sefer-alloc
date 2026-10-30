#!/bin/sh
# SC-6 prototype diff, MT price: ph6b_mt_ab (larson/mstress, T=1,2,4,8, every
# 16th free cross-thread), base vs proto, order ABBA ABBA.
# Usage (from this worktree root): sh docs/perf/g1_scan/sc6_diff/run_mt.sh <base-worktree>
set -e
BASE_WT=$1
export RUSTC_WRAPPER= RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
F="production internals"
(cd "$BASE_WT" && CARGO_TARGET_DIR=target/sc6base cargo build --release -q --example ph6b_mt_ab --features "$F")
CARGO_TARGET_DIR=target/sc6proto cargo build --release -q --example ph6b_mt_ab --features "$F"
B="$BASE_WT/target/sc6base/release/examples/ph6b_mt_ab"
P=target/sc6proto/release/examples/ph6b_mt_ab
O=docs/perf/g1_scan/sc6_diff
: > $O/mt.txt
{ echo "proto $(git rev-parse HEAD)"; echo "base $(git -C "$BASE_WT" rev-parse HEAD)"; sha256sum $B.exe $P.exe 2>/dev/null || sha256sum $B $P; echo "features: $F"; } > $O/identity_mt.txt
for arm in $(for q in $(seq 1 25); do printf "base proto proto base "; done); do
  if [ $arm = base ]; then X=$B; else X=$P; fi
  $X 2>/dev/null | grep '^RESULT mt_ns' | sed "s/^RESULT/RESULT arm=$arm/" >> $O/mt.txt
done
echo done
