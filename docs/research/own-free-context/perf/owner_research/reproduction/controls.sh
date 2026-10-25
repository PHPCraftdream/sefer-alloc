#!/bin/bash
set -euo pipefail
repo=$1
out=$2
mkdir -p "$out"
cd "$repo"
CARGO_PROFILE_RELEASE_DEBUG=2 CARGO_TARGET_DIR="$out/target" cargo build --locked --release --example own_free_wall --features production > "$out/build.log" 2>&1
bin="$out/target/release/examples/own_free_wall"
sha256sum "$bin" > "$out/binary.sha256"
for pass in 1 2; do
  for arm in sefer mimalloc; do
    for mode in bulk64 bulk16k; do
      for n in 100 200; do
        stem="$out/p${pass}-${arm}-${mode}-${n}"
        valgrind --tool=callgrind --collect-atstart=no --toggle-collect='*own_free_wall::w_free*' --callgrind-out-file="$stem.callgrind" "$bin" "$arm" "$mode" "$n" > "$stem.stdout" 2> "$stem.stderr"
        callgrind_annotate --threshold=100 "$stem.callgrind" > "$stem.annotate"
      done
    done
  done
done
printf 'Overflow controls complete: %s\n' "$out"
