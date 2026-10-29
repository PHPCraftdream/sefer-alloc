#!/bin/sh
# G1-PC counting run (PC_PREREG.md): every point twice, one process per run.
set -e
cd "$(dirname "$0")/../../.."
export CARGO_TARGET_DIR=target/g1scan RUSTC_WRAPPER= RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
F="production bench-internals internals alloc-stats"
cargo build --release -q --example g1_rhythm_probe --features "$F"
BIN=target/g1scan/release/examples/g1_rhythm_probe
R=docs/perf/g1_scan/raw
mkdir -p $R
{ git rev-parse HEAD; sha256sum $BIN.exe 2>/dev/null || sha256sum $BIN; rustc -vV; echo "features: $F"; echo "RUSTFLAGS: $RUSTFLAGS"; } > docs/perf/g1_scan/run_identity_pc.txt
for rep in 1 2; do
  for m in 4 5 8 16 64; do
    for o in cyclic seq random; do
      $BIN pincycle $m $o >> $R/pincycle.jsonl
    done
  done
done
node scripts/g1_scan_report.mjs $R
