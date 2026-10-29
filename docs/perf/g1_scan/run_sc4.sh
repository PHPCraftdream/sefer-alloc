#!/bin/sh
# G1-SC-4 timing run (SC4_PREREG.md): growth, sefer vs mimalloc, ABBA quads.
set -e
cd "$(dirname "$0")/../../.."
export CARGO_TARGET_DIR=target/g1sc4 RUSTC_WRAPPER= RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
F="production"
cargo build --release -q --example g1_rhythm_probe --features "$F"
BIN=target/g1sc4/release/examples/g1_rhythm_probe
R=docs/perf/g1_scan/raw
mkdir -p $R
[ ! -s $R/sc4_growth.jsonl ] || { echo "abort: $R/sc4_growth.jsonl not empty"; exit 2; }
{ git rev-parse HEAD; git status --porcelain --untracked-files=no | wc -l | sed 's/^/dirty_files: /'; sha256sum $BIN.exe 2>/dev/null || sha256sum $BIN; rustc -vV; echo "features: $F"; echo "RUSTFLAGS: $RUSTFLAGS"; } > docs/perf/g1_scan/run_identity_sc4.txt
for size in 64 1024; do
  per=$((size == 64 ? 65536 : 4096))
  for s in 8 16 32 64 128 256; do
    n=$((s * per))
    q=1
    while [ $q -le 10 ]; do
      $BIN growth $size $n >> $R/sc4_growth.jsonl
      $BIN growth $size $n mi >> $R/sc4_growth.jsonl
      $BIN growth $size $n mi >> $R/sc4_growth.jsonl
      $BIN growth $size $n >> $R/sc4_growth.jsonl
      q=$((q + 1))
    done
  done
done
node scripts/g1_sc4_report.mjs $R/sc4_growth.jsonl --csv docs/perf/G1_SC4_GROWTH_TIME_summary.csv
