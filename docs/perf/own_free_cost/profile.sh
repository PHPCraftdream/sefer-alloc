#!/bin/bash
set -euo pipefail
repo=$1
out=$2
mkdir -p "$out"
cd "$repo"
{
  rustc -Vv
  cargo -V
  uname -a
  valgrind --version
} > "$out/toolchain.log"
CARGO_PROFILE_RELEASE_DEBUG=2 CARGO_TARGET_DIR="$out/target" cargo build --locked --release --example own_free_profile --features production > "$out/build.log" 2>&1
bin="$out/target/release/examples/own_free_profile"
sha256sum "$bin" > "$out/binary.sha256"
nm -C "$bin" > "$out/symbols.txt"
for pass in 1 2; do
  for arm in sefer mimalloc; do
    for mode in rt 16 64; do
      for n in 50000 100000; do
        stem="$out/p${pass}-${arm}-${mode}-${n}"
        printf 'profile pass=%s arm=%s mode=%s n=%s\n' "$pass" "$arm" "$mode" "$n"
        valgrind --tool=callgrind --collect-atstart=no --toggle-collect='*own_free_profile::w_free*' --callgrind-out-file="$stem.callgrind" "$bin" "$arm" "$mode" "$n" > "$stem.stdout" 2> "$stem.stderr"
        callgrind_annotate --auto=yes --threshold=100 "$stem.callgrind" > "$stem.annotate"
      done
    done
  done
done
printf 'Profile complete: 24 fresh processes, identical production binary.\n'
