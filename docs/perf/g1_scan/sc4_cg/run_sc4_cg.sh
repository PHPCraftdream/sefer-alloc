#!/bin/bash
# G1-SC-4 mechanism share: callgrind of growth 64 B at S = 8 and 64, sefer and mi.
set -o pipefail
source ~/.cargo/env 2>/dev/null; export PATH=$HOME/.cargo/bin:$PATH RUSTC_WRAPPER= CARGO_PROFILE_RELEASE_DEBUG=line-tables-only
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
D=~/abw/worktrees/sc4; rm -rf $D; mkdir -p $D; cd $D || exit 2
tar -xf /mnt/d/dev/rust/sefer-alloc/.rush/wsl/sc4_tree.tar || exit 3
CARGO_TARGET_DIR=~/ab/tgt-sc4 timeout 1800 cargo build --release -q --example g1_rhythm_probe --features production > ~/ab/sc4_build.log 2>&1 || { echo build failed; tail ~/ab/sc4_build.log; exit 4; }
B=~/ab/tgt-sc4/release/examples/g1_rhythm_probe; O=~/ab/sc4; mkdir -p $O
sha256sum $B
for s in 8 64; do for a in sefer mi; do
  n=$((s * 65536)); f=$O/cg.$a.$s.out
  valgrind --tool=callgrind --callgrind-out-file=$f $B growth 64 $n $([ $a = mi ] && echo mi) > /dev/null 2>&1
  tot=$(callgrind_annotate --auto=no $f 2>/dev/null | grep -m1 'PROGRAM TOTALS' | awk '{gsub(",","",$1); print $1}')
  ls=$(callgrind_annotate --auto=no --inclusive=yes $f 2>/dev/null | grep -m1 'linear_scan' | awk '{gsub(",","",$1); print $1}')
  echo "$a S=$s n=$n total_Ir=$tot linear_scan_incl_Ir=${ls:-0}"
done; done
