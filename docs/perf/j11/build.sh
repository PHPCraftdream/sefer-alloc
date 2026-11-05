#!/bin/bash
# J11: builds the big-block judge (examples/g1_big_judge.rs) in WSL from ONE source tree (a git archive of the measured commit),
# `--release --features production`, the repository's alignment RUSTFLAGS, and writes the identity file.
#   J11_TAR=<git archive tar> J11_OUT=<dir for ident.txt> bash build.sh
set -e
source ~/.cargo/env
: "${J11_TAR:?set J11_TAR to the git archive tar of the measured commit}"
: "${J11_OUT:?set J11_OUT to the output dir}"
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
mkdir -p "$J11_OUT"
rm -rf ~/ab/j11src; mkdir -p ~/ab/j11src; tar -xf "$J11_TAR" -C ~/ab/j11src
(cd ~/ab/j11src && CARGO_TARGET_DIR=~/ab/tgt-j11 cargo build -q --release --example g1_big_judge --features production)
{
  echo "tar $(sha256sum "$J11_TAR" | cut -d' ' -f1)"
  echo "bin $(sha256sum ~/ab/tgt-j11/release/examples/g1_big_judge | cut -d' ' -f1)"
  rustc -Vv | sed -n '1p;/^host/p;/^LLVM/p'
  grep -m1 'model name' /proc/cpuinfo
  echo "logical cpus: $(nproc)"
  echo "RUSTFLAGS: $RUSTFLAGS"
  echo "features: production (release)"
} > "$J11_OUT/ident.txt"
echo "$HOME/ab/tgt-j11/release/examples/g1_big_judge"
