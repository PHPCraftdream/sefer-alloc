#!/bin/bash
set -euo pipefail
root=$1
out="$root/.rush/own-free-split/sanity"
mkdir -p "$out"
for phase in before after; do
  bin="$root/.rush/own-free-split/control-$phase/target/release/examples/own_free_wall"
  nm -S -C "$bin" > "$out/$phase.symbols"
  sha256sum "$bin" > "$out/$phase.binary.sha256"
  for n in 100 200; do
    stem="$out/$phase-$n"
    valgrind --tool=callgrind --callgrind-out-file="$stem.callgrind" "$bin" sefer bulk16k "$n" > "$stem.stdout" 2> "$stem.stderr"
    callgrind_annotate --threshold=100 "$stem.callgrind" > "$stem.annotate"
  done
done
printf 'Full-program bulk16k sanity complete; no toggle filtering.\n'
