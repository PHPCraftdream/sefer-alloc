#!/bin/sh
# SC-6 prototype diff, growth (SC-4 shape): base (origin/main production) vs
# proto (this branch) vs mimalloc, order base proto mi mi proto base, 5 rounds.
# Usage (from this worktree root): sh docs/perf/g1_scan/sc6_diff/run_growth.sh <base-worktree>
set -e
BASE_WT=$1
export RUSTC_WRAPPER= RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
(cd "$BASE_WT" && CARGO_TARGET_DIR=target/sc6base cargo build --release -q --example g1_rhythm_probe --features production)
CARGO_TARGET_DIR=target/sc6proto cargo build --release -q --example g1_rhythm_probe --features production
B="$BASE_WT/target/sc6base/release/examples/g1_rhythm_probe"
P=target/sc6proto/release/examples/g1_rhythm_probe
O=docs/perf/g1_scan/sc6_diff
: > $O/growth.jsonl
{ echo "proto $(git rev-parse HEAD)"; echo "base $(git -C "$BASE_WT" rev-parse HEAD)"; sha256sum $B.exe $P.exe 2>/dev/null || sha256sum $B $P; rustc -vV | head -1; echo "RUSTFLAGS: $RUSTFLAGS"; } > $O/identity_growth.txt
tag() { sed "s/^{/{\"arm\":\"$1\",/" ; }
for size in 64 1024; do
  per=$((size == 64 ? 65536 : 4096))
  for s in 8 32 64 128 256; do
    n=$((s * per))
    r=1
    while [ $r -le 5 ]; do
      $B growth $size $n | tag base >> $O/growth.jsonl
      $P growth $size $n | tag proto >> $O/growth.jsonl
      $P growth $size $n mi | tag mi >> $O/growth.jsonl
      $B growth $size $n mi | tag mi >> $O/growth.jsonl
      $P growth $size $n | tag proto >> $O/growth.jsonl
      $B growth $size $n | tag base >> $O/growth.jsonl
      r=$((r + 1))
    done
  done
done
echo done
