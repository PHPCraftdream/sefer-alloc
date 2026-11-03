#!/bin/bash
# NL-0 part 2: prototype C vs B vs base. bulk 1024 x {16,64} callgrind + perf stat; larson/mstress T=1 and T=4 callgrind + Mops.
set -e
source ~/.cargo/env
G=${NL0_SCRATCH:?set NL0_SCRATCH to the scratch dir}; P=/usr/lib/linux-tools-6.8.0-147/perf
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
O=$G/nl0/outc; mkdir -p $O; : > $O/identity.txt
for L in base b c; do
  [ -d ~/ab/nl0$L ] || { mkdir -p ~/ab/nl0$L; tar -xf $G/nl0/$L.tar -C ~/ab/nl0$L; }
  cp $G/nl0/g1_mt_one.rs ~/ab/nl0$L/examples/g1_mt_one.rs
  (cd ~/ab/nl0$L && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgt-nl0$L cargo build -q --release --example b4_pmu_probe --example g1_mt_one --features production)
  BIN=~/ab/tgt-nl0$L/release/examples/b4_pmu_probe; MT=~/ab/tgt-nl0$L/release/examples/g1_mt_one
  sha256sum $BIN $MT >> $O/identity.txt
  cd /tmp
  for sz in 16 64; do
    valgrind --tool=callgrind --callgrind-out-file=/tmp/nl0c.$L.bulk$sz.out $BIN bulk sefer 1024 $sz 400 > /dev/null 2>&1
    callgrind_annotate /tmp/nl0c.$L.bulk$sz.out 2>/dev/null | grep -E "PROGRAM TOTALS" | tr -d ',' > $O/ir.$L.bulk$sz.txt
    for r in 1 2 3 4 5; do
      $P stat -e cycles:u,instructions:u -x, -o $O/stat.$L.bulk$sz.$r.txt $BIN bulk sefer 1024 $sz 4000 > $O/run.$L.bulk$sz.$r.stdout
    done
  done
  for w in larson mstress; do for t in 1 4; do
    valgrind --tool=callgrind --callgrind-out-file=/tmp/nl0c.$L.$w$t.out $MT $w sefer $t 100000 > /dev/null 2>&1
    callgrind_annotate /tmp/nl0c.$L.$w$t.out 2>/dev/null | grep -E "PROGRAM TOTALS" | tr -d ',' > $O/ir.$L.$w$t.txt
    for r in 1 2 3 4 5; do $MT $w sefer $t 1000000 > $O/run.$L.$w$t.$r.stdout; done
  done; done
done
echo ok
