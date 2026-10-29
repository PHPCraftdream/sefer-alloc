#!/bin/bash
set -euo pipefail
repo=$1
out=$2
mkdir -p "$out"
cd "$repo"
CARGO_PROFILE_RELEASE_DEBUG=2 CARGO_TARGET_DIR="$out/target" cargo build --locked --release --features production --example native_mixed_sefer --example native_mixed_mimalloc > "$out/build.log" 2>&1
for arm in sefer mimalloc; do
  bin="$out/target/release/examples/native_mixed_$arm"
  sha256sum "$bin" >> "$out/binaries.sha256"
  for repeats in 120 241; do
    stem="$out/$arm-$repeats"
    valgrind --tool=callgrind --collect-atstart=yes --callgrind-out-file="$stem.callgrind" "$bin" "$repeats" > "$stem.stdout" 2> "$stem.stderr"
    callgrind_annotate --inclusive=yes --threshold=100 "$stem.callgrind" > "$stem.annotate"
  done
 done
sha256sum --check "$out/binaries.sha256" > "$out/binary-check.log"
printf 'Installed workload attribution complete\n'
