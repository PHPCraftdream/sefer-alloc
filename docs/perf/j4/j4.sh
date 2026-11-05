#!/bin/bash
# J4: paired multi-thread table, SeferAlloc vs mimalloc, on ONE source tree (a git archive of the measured commit).
# One binary (g1_mt_one) carries both allocators and picks one by argument, so both arms are the same build.
# Cells: larson/mstress x T in {1,2,4,8}. Timing: 5 repetitions of the order sefer, mi, mi, sefer (ABBA),
# 10 samples per arm and cell; the position in the order is part of the file name.
#   J4_TAR=<git archive tar> J4_OUT=<raw output dir> bash j4.sh
set -e
source ~/.cargo/env
: "${J4_TAR:?set J4_TAR to the git archive tar of the measured commit}"
: "${J4_OUT:?set J4_OUT to the raw output dir}"
HERE=$(cd "$(dirname "$0")" && pwd)
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
mkdir -p "$J4_OUT"; : > "$J4_OUT/identity.txt"
rm -rf ~/ab/j4src; mkdir -p ~/ab/j4src; tar -xf "$J4_TAR" -C ~/ab/j4src
cp "$HERE/../nl9/g1_mt_one.rs" ~/ab/j4src/examples/g1_mt_one.rs
! grep -q '^production = .*misuse-checks' ~/ab/j4src/Cargo.toml
sha256sum "$J4_TAR" >> "$J4_OUT/identity.txt"
rustc -Vv >> "$J4_OUT/identity.txt"
grep -m1 'model name' /proc/cpuinfo >> "$J4_OUT/identity.txt"
echo "logical cpus: $(nproc)" >> "$J4_OUT/identity.txt"
(cd ~/ab/j4src && CARGO_TARGET_DIR=~/ab/tgt-j4 cargo build -q --release --example g1_mt_one --features production)
echo "--features production" >> "$J4_OUT/identity.txt"
BIN=~/ab/tgt-j4/release/examples/g1_mt_one
sha256sum "$BIN" >> "$J4_OUT/identity.txt"
for rep in 1 2 3 4 5; do
  i=0
  for arm in sefer mi mi sefer; do
    i=$((i+1))
    for w in larson mstress; do for t in 1 2 4 8; do
      "$BIN" $w $arm $t 1000000 > "$J4_OUT/run.$arm.$w$t.$rep.$i.stdout"
    done; done
  done
done
echo ok
