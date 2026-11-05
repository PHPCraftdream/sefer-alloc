#!/bin/bash
# J6: builds the WG-1 judge (examples/g1_wild_judge.rs) from ONE source tree (a git archive of the measured commit) in WSL,
# `--release --features production`, the repository's alignment RUSTFLAGS, and writes the identity file.
#   J6_TAR=<git archive tar> J6_OUT=<dir for ident.txt> bash build.sh
set -e
source ~/.cargo/env
: "${J6_TAR:?set J6_TAR to the git archive tar of the measured commit}"
: "${J6_OUT:?set J6_OUT to the output dir}"
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
mkdir -p "$J6_OUT"
rm -rf ~/ab/j6src; mkdir -p ~/ab/j6src; tar -xf "$J6_TAR" -C ~/ab/j6src
(cd ~/ab/j6src && CARGO_TARGET_DIR=~/ab/tgt-j6 cargo build -q --release --example g1_wild_judge --features production)
{
  echo "tar $(sha256sum "$J6_TAR" | cut -d' ' -f1)"
  echo "bin $(sha256sum ~/ab/tgt-j6/release/examples/g1_wild_judge | cut -d' ' -f1)"
  rustc -Vv | sed -n '1p;/^host/p;/^LLVM/p'
  grep -m1 'model name' /proc/cpuinfo
  echo "logical cpus: $(nproc)"
  echo "RUSTFLAGS: $RUSTFLAGS"
  echo "features: production (release)"
} > "$J6_OUT/ident.txt"
echo "$HOME/ab/tgt-j6/release/examples/g1_wild_judge"
