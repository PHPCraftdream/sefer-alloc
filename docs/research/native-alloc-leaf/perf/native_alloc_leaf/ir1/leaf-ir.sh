#!/bin/bash
set -euo pipefail
repo=$1
out=$2
mkdir -p "$out"
cd "$repo"
CARGO_PROFILE_RELEASE_DEBUG=2 CARGO_TARGET_DIR="$out/target" cargo build --locked --release --features production --example own_free_profile --example own_free_wall > "$out/build.log" 2>&1
for example in own_free_profile own_free_wall; do
  sha256sum "$out/target/release/examples/$example" >> "$out/binaries.sha256"
done
for arm in sefer mimalloc; do
  for mode in rt 16 64 bulk64 bulk16k; do
    example=own_free_profile
    counts='50000 100000'
    if [[ "$mode" = bulk* ]]; then example=own_free_wall; counts='100 200'; fi
    bin="$out/target/release/examples/$example"
    for axis in alloc free pair; do
      for n in $counts; do
        stem="$out/$arm-$mode-$axis-$n"
        options=(--collect-atstart=yes)
        if [ "$axis" != pair ]; then options=(--collect-atstart=no "--toggle-collect=*$example::w_$axis*"); fi
        valgrind --tool=callgrind "${options[@]}" --callgrind-out-file="$stem.callgrind" "$bin" "$arm" "$mode" "$n" > "$stem.stdout" 2> "$stem.stderr"
        callgrind_annotate --threshold=0 "$stem.callgrind" > "$stem.annotate"
      done
    done
  done
 done
sha256sum --check "$out/binaries.sha256" > "$out/binary-check.log"
printf 'Leaf Ir profiles complete\n'
