#!/bin/bash
# J13: builds the big-block judge (examples/g1_big_judge.rs) in WSL from two git archives, base and x, `--release --features production`,
# the repository's alignment RUSTFLAGS; base2 is a byte-identical copy of base (the A/A control). Writes the identity file.
#   J13_BASE_TAR=<tar> J13_X_TAR=<tar> J13_OUT=<dir for ident.txt> bash build.sh
set -e
source ~/.cargo/env
: "${J13_BASE_TAR:?}"; : "${J13_X_TAR:?}"; : "${J13_OUT:?}"
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
mkdir -p "$J13_OUT" ~/ab/j13bin
for v in base x; do
  tar_var="J13_$(echo $v | tr a-z A-Z)_TAR"; tar_path="${!tar_var}"
  rm -rf ~/ab/j13src-$v; mkdir -p ~/ab/j13src-$v; tar -xf "$tar_path" -C ~/ab/j13src-$v
  (cd ~/ab/j13src-$v && CARGO_TARGET_DIR=~/ab/tgt-j13-$v cargo build -q --release --example g1_big_judge --features production)
  cp ~/ab/tgt-j13-$v/release/examples/g1_big_judge ~/ab/j13bin/$v
done
cp ~/ab/j13bin/base ~/ab/j13bin/base2
{
  for v in base x; do
    tar_var="J13_$(echo $v | tr a-z A-Z)_TAR"
    echo "tar-$v $(sha256sum "${!tar_var}" | cut -d' ' -f1)"
    echo "bin-$v $(sha256sum ~/ab/j13bin/$v | cut -d' ' -f1)"
  done
  echo "bin-base2 $(sha256sum ~/ab/j13bin/base2 | cut -d' ' -f1)"
  rustc -Vv | sed -n '1p;/^host/p;/^LLVM/p'
  grep -m1 'model name' /proc/cpuinfo
  echo "logical cpus: $(nproc)"
  echo "RUSTFLAGS: $RUSTFLAGS"
  echo "features: production (release)"
  echo "thp: $(cat /sys/kernel/mm/transparent_hugepage/enabled)"
} > "$J13_OUT/ident.txt"
echo "$HOME/ab/j13bin"
