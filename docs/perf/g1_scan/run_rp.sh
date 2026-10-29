#!/bin/sh
# G1-RP run (RP_PREREG.md). Counting build: one process per point (oracle and
# phase counters). Timing build (`production` only, no diagnostic increments):
# 10 processes per point and side, sides alternate.
set -e
cd "$(dirname "$0")/../../.."
export RUSTC_WRAPPER= RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
FC="production bench-internals internals alloc-stats"
CARGO_TARGET_DIR=target/g1scan cargo build --release -q --example g1_rhythm_probe --features "$FC"
CARGO_TARGET_DIR=target/g1time cargo build --release -q --example g1_rhythm_probe --features production
BC=target/g1scan/release/examples/g1_rhythm_probe
BT=target/g1time/release/examples/g1_rhythm_probe
R=docs/perf/g1_scan/raw
mkdir -p $R
{ git rev-parse HEAD; sha256sum $BC* | head -1; sha256sum $BT* | head -1; rustc -vV; echo "counting features: $FC"; echo "timing features: production"; echo "RUSTFLAGS: $RUSTFLAGS"; } > docs/perf/g1_scan/run_identity_rp.txt
for s in 8 32 64; do
  fill=$((s * 65536))
  for frac in 16 4 1; do
    $BC reclaim $fill $((fill / frac)) >> $R/reclaim_counting.jsonl
    for rep in 1 2 3 4 5 6 7 8 9 10; do
      if [ $((rep % 2)) = 1 ]; then
        $BT reclaim $fill $((fill / frac)) >> $R/reclaim.jsonl
        $BT reclaim $fill $((fill / frac)) mi >> $R/reclaim.jsonl
      else
        $BT reclaim $fill $((fill / frac)) mi >> $R/reclaim.jsonl
        $BT reclaim $fill $((fill / frac)) >> $R/reclaim.jsonl
      fi
    done
  done
done
node scripts/g1_scan_report.mjs $R
