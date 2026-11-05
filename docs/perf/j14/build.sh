#!/bin/bash
# J14: builds the big-block judge (examples/g1_big_judge.rs) in WSL from two git archives: base (`--features production`) and x
# (`--features "production large-cache-adaptive"`), the repository's alignment RUSTFLAGS; base2 is a byte-identical copy of base.
# xt (J14 + the J13 hint, tag archive/g1-j14-xt) is built from J14_XT_TAR when set.
#   J14_BASE_TAR=<tar> J14_X_TAR=<tar> [J14_XT_TAR=<tar>] J14_OUT=<dir for ident.txt> bash build.sh
set -e
source ~/.cargo/env
: "${J14_BASE_TAR:?}"; : "${J14_X_TAR:?}"; : "${J14_OUT:?}"
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
mkdir -p "$J14_OUT" ~/ab/j14bin
build() { # variant tar features
  rm -rf ~/ab/j14src-$1; mkdir -p ~/ab/j14src-$1; tar -xf "$2" -C ~/ab/j14src-$1
  (cd ~/ab/j14src-$1 && CARGO_TARGET_DIR=~/ab/tgt-j14-$1 cargo build -q --release --example g1_big_judge --features "$3")
  cp ~/ab/tgt-j14-$1/release/examples/g1_big_judge ~/ab/j14bin/$1
}
build base "$J14_BASE_TAR" "production"
build x "$J14_X_TAR" "production large-cache-adaptive"
[ -n "${J14_XT_TAR:-}" ] && build xt "$J14_XT_TAR" "production large-cache-adaptive"
cp ~/ab/j14bin/base ~/ab/j14bin/base2
{
  echo "tar-base $(sha256sum "$J14_BASE_TAR" | cut -d' ' -f1)"
  echo "bin-base $(sha256sum ~/ab/j14bin/base | cut -d' ' -f1)"
  echo "tar-x $(sha256sum "$J14_X_TAR" | cut -d' ' -f1)"
  echo "bin-x $(sha256sum ~/ab/j14bin/x | cut -d' ' -f1)"
  echo "bin-base2 $(sha256sum ~/ab/j14bin/base2 | cut -d' ' -f1)"
  [ -n "${J14_XT_TAR:-}" ] && { echo "tar-xt $(sha256sum "$J14_XT_TAR" | cut -d' ' -f1)"; echo "bin-xt $(sha256sum ~/ab/j14bin/xt | cut -d' ' -f1)"; }
  echo "features-base: production; features-x: production large-cache-adaptive (release)"
  rustc -Vv | sed -n '1p;/^host/p;/^LLVM/p'
  grep -m1 'model name' /proc/cpuinfo
  echo "logical cpus: $(nproc)"
  echo "RUSTFLAGS: $RUSTFLAGS"
  echo "thp: $(cat /sys/kernel/mm/transparent_hugepage/enabled)"
} > "$J14_OUT/ident.txt"
echo "$HOME/ab/j14bin"
