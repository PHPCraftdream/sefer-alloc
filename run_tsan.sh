#!/bin/bash
set -e
cd /mnt/d/dev/rust/sefer-alloc
export RUSTFLAGS="-Zsanitizer=thread"
export CARGO_TARGET_DIR=/tmp/sefer-tsan
for t in race_repro race_norecycle global_alloc_mt heap_cross_thread; do
  echo ""
  echo "=== TSan: $t ==="
  cargo +nightly test \
    -Zbuild-std \
    --target x86_64-unknown-linux-gnu \
    --features "alloc-global alloc-xthread" \
    --test "$t" 2>&1 | tail -8
done
