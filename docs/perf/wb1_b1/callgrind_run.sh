#!/bin/bash
# WB-1/B-1a prereg check 1: callgrind Ir, base (40da85c3) vs b1 (040a9106).
set -o pipefail
source ~/.cargo/env 2>/dev/null; export PATH=$HOME/.cargo/bin:$PATH RUSTC_WRAPPER=
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
O=~/ab/wb1b1; mkdir -p $O
for arm in base b1; do
  D=~/abw/worktrees/wb1b1_$arm; rm -rf $D; mkdir -p $D; cd $D || exit 2
  tar -xf /mnt/d/dev/rust/sefer-alloc/.rush/wsl/wb1b1_$arm.tar || exit 3
  CARGO_TARGET_DIR=~/ab/tgt-wb1b1-$arm timeout 1800 cargo build --release -q --example b4_pmu_probe --features production > $O/build_$arm.log 2>&1 || { echo "build $arm failed"; exit 4; }
  sha256sum ~/ab/tgt-wb1b1-$arm/release/examples/b4_pmu_probe | sed "s|^|$arm |" >> $O/identity.txt
done
total() { callgrind_annotate --auto=no "$1" 2>/dev/null | grep -m1 'PROGRAM TOTALS' | awk '{gsub(",","",$1); print $1}'; }
run() { # arm label args...
  arm=$1; lab=$2; shift 2
  f=$O/cg.$arm.$lab.out
  valgrind --tool=callgrind --callgrind-out-file=$f ~/ab/tgt-wb1b1-$arm/release/examples/b4_pmu_probe "$@" > $f.stdout 2>/dev/null
  echo "$arm $lab $(total $f)"
}
for arm in base b1; do
  for sz in 16 64; do for p in 50 100; do run $arm bulk$sz.$p bulk sefer 1024 $sz $p; done; done
  for spec in "pair16 1 16" "churn16 256 16" "churn64 256 64"; do set -- $spec
    for n in 200000 400000; do run $arm $1.$n sefer $2 $3 $n; done; done
done
