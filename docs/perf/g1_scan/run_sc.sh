#!/bin/sh
# G1-SC counting run (SC_PREREG.md): every point twice, one process per run.
set -e
cd "$(dirname "$0")/../../.."
export CARGO_TARGET_DIR=target/g1scan RUSTC_WRAPPER= RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
F="production bench-internals internals alloc-stats"
cargo build --release -q --example g1_rhythm_probe --features "$F"
BIN=target/g1scan/release/examples/g1_rhythm_probe
R=docs/perf/g1_scan/raw
mkdir -p $R
{ git rev-parse HEAD; sha256sum $BIN.exe 2>/dev/null || sha256sum $BIN; rustc -vV; echo "features: $F"; echo "RUSTFLAGS: $RUSTFLAGS"; } > docs/perf/g1_scan/run_identity_sc.txt
for rep in 1 2; do
  for s in 8 16 32 64; do
    $BIN growth 64 $((s * 65536)) >> $R/growth64.jsonl
    $BIN growth 1024 $((s * 4096)) >> $R/growth1k.jsonl
  done
  for s in 8 32; do
    for k in 1 2 3 8; do
      $BIN trickle 64 $((s * 65536)) $k >> $R/trickle64.jsonl
      $BIN trickle 64 $((s * 65536)) $k control >> $R/trickle64.jsonl
    done
  done
  for s in 8 32 64; do
    $BIN bigheap $((s * 65536)) 65536 >> $R/bigheap.jsonl
  done
done
node scripts/g1_scan_report.mjs $R --csv docs/perf/G1_SCAN_COUNT_summary.csv
