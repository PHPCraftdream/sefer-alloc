#!/bin/bash
# NL-0 ceiling: callgrind + perf stat of b4_pmu_probe bulk 1024 x {16,64} B for base / A / B trees.
set -e
source ~/.cargo/env
G=${NL0_SCRATCH:?scratch dir holding nl0/{base,a,b}.tar}; P=/usr/lib/linux-tools-6.8.0-147/perf
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
O=$G/nl0/out; mkdir -p $O
for L in base a b; do
  [ -d ~/ab/nl0$L ] || { mkdir -p ~/ab/nl0$L; tar -xf $G/nl0/$L.tar -C ~/ab/nl0$L; }
  (cd ~/ab/nl0$L && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgt-nl0$L cargo build -q --release --example b4_pmu_probe --features production)
  BIN=~/ab/tgt-nl0$L/release/examples/b4_pmu_probe
  sha256sum $BIN >> $O/identity.txt
  for sz in 16 64; do
    cd /tmp
    valgrind --tool=callgrind --callgrind-out-file=/tmp/nl0.$L.$sz.out $BIN bulk sefer 1024 $sz 400 > /dev/null 2>&1
    callgrind_annotate /tmp/nl0.$L.$sz.out 2>/dev/null | grep -E "PROGRAM TOTALS" | tr -d ',' > $O/ir.$L.$sz.txt
    for r in 1 2 3 4 5; do
      $P stat -e cycles:u,instructions:u -x, -o $O/stat.$L.$sz.$r.txt $BIN bulk sefer 1024 $sz 4000 > $O/run.$L.$sz.$r.stdout
    done
  done
done
echo ok
