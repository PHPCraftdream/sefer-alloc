#!/bin/bash
# J10 step 1: line-level callgrind of the growmix free/refill side of the Linux judge (sefer arm), from ONE source tree (a git archive of
# the measured commit), `--release --features production`, the repository's alignment RUSTFLAGS plus line tables.
#   J10_TAR=<git archive tar> J10_OUT=<raw dir> [J10_SCALE=0.25] bash j10.sh
set -e
source ~/.cargo/env
: "${J10_TAR:?set J10_TAR}"
: "${J10_OUT:?set J10_OUT}"
SCALE=${J10_SCALE:-0.25}
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
rm -rf ~/ab/j10src; mkdir -p ~/ab/j10src "$J10_OUT"; tar -xf "$J10_TAR" -C ~/ab/j10src
(cd ~/ab/j10src && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgt-j10 cargo build -q --release --example g1_wild_judge --features production)
BIN=~/ab/tgt-j10/release/examples/g1_wild_judge
echo "bin $(sha256sum "$BIN" | cut -d' ' -f1)" > "$J10_OUT/ident.txt"
echo "tar $(sha256sum "$J10_TAR" | cut -d' ' -f1)" >> "$J10_OUT/ident.txt"
"$BIN" growmix sefer 1 "$SCALE" | tail -1 > "$J10_OUT/run.growmix.sefer.json"
cd /tmp
valgrind --tool=callgrind --dump-line=yes --compress-strings=no --compress-pos=no --callgrind-out-file=/tmp/j10.growmix.out "$BIN" growmix sefer 1 "$SCALE" > /dev/null 2>&1
sed -e 's#/home/[^/]*/#<scratch>/#g' /tmp/j10.growmix.out | gzip -9 > "$J10_OUT/growmix.sefer.lines.out.gz"
callgrind_annotate /tmp/j10.growmix.out 2>/dev/null | sed -e 's#/home/[^/]*/#<scratch>/#g' | sed -n '/PROGRAM TOTALS/,$p' | head -40 > "$J10_OUT/ir.growmix.sefer.txt"
echo "scale $SCALE" > "$J10_OUT/scale.txt"
echo ok
