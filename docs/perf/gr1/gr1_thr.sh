#!/bin/bash
# GR-1(c): DIRECTORY_MATERIALIZE_THRESHOLD sweep by callgrind program totals (base tree m4, threshold variants).
source ~/.cargo/env
G=${G1_DIR:?set G1_DIR}
O=$G/gr1/thr; mkdir -p $O
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
for T in 4 8 16 32; do
  D=~/ab/m4t$T
  if [ ! -d $D ]; then mkdir -p $D; tar -xf $G/m4.tar -C $D; sed -i "s/DIRECTORY_MATERIALIZE_THRESHOLD: u32 = 32;/DIRECTORY_MATERIALIZE_THRESHOLD: u32 = $T;/" $D/src/alloc_core/segment/segment_directory/segment_directory_impl.rs; fi
  grep -c "THRESHOLD: u32 = $T;" $D/src/alloc_core/segment/segment_directory/segment_directory_impl.rs
  (cd $D && CARGO_TARGET_DIR=~/ab/tgt-m4t$T cargo build -q --release --example g1_wild_judge --features production) || exit 1
  cp ~/ab/tgt-m4t$T/release/examples/g1_wild_judge $O/judge_$T
done
sha256sum $O/judge_* > $O/ident.txt
: > $O/ir.csv
for T in 32 16 8 4; do
  for SC in grow64 grow1k growmix regrow phase wild; do
    valgrind --tool=callgrind --callgrind-out-file=/tmp/thr.out $O/judge_$T $SC sefer 1 0.1 > /dev/null 2>/tmp/thr.err
    ir=$(grep -E "Collected" /tmp/thr.err | grep -oE '[0-9]+$')
    echo "$T,$SC,$ir" >> $O/ir.csv
  done
done
cat $O/ir.csv
