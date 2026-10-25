#!/bin/bash
set -euo pipefail
root=$1
out="$root/.rush/own-free-context/alloc-sanity"
mkdir -p "$out"
for phase in before after; do
  if [ "$phase" = before ]; then
    bin="$root/.rush/own-free-split/baseline/target/release/examples/own_free_profile"
  else
    bin="$root/.rush/own-free-context/candidate/target/release/examples/own_free_profile"
  fi
  sha256sum "$bin" > "$out/$phase.binary.sha256"
  for mode in rt 16 64; do
    for n in 50000 100000; do
      stem="$out/$phase-$mode-$n"
      valgrind --tool=callgrind --collect-atstart=no --toggle-collect='*own_free_profile::w_alloc*' --callgrind-out-file="$stem.callgrind" "$bin" sefer "$mode" "$n" > "$stem.stdout" 2> "$stem.stderr"
      callgrind_annotate --threshold=100 "$stem.callgrind" > "$stem.annotate"
    done
  done
done
printf 'Alloc sanity complete on same immutable benchmark binaries.\n'
