#!/bin/bash
# NL-11: ceiling of dropping the producer-side class check in SidecarBitmap::publish (misuse-only: it drops a
# foreign free of a never-issued granule). ONE source tree (a git archive of the measured commit), two arms
# that differ only by patches/nl11.patch (the check behind `misuse-checks` or debug assertions); both build
# `--features production` (the new default, no misuse-checks). Cells: xfree_probe (4096 cross-thread frees,
# callgrind Ir between the producer markers), larson/mstress T=2 and T=4 (callgrind Ir, Mops in ABBA order).
#   NL11_TAR=<git archive tar> NL11_OUT=<raw output dir> bash nl11.sh
set -e
source ~/.cargo/env
: "${NL11_TAR:?set NL11_TAR to the git archive tar of the measured commit}"
: "${NL11_OUT:?set NL11_OUT to the raw output dir}"
HERE=$(cd "$(dirname "$0")" && pwd)
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
mkdir -p "$NL11_OUT"; : > "$NL11_OUT/identity.txt"
sha256sum "$NL11_TAR" >> "$NL11_OUT/identity.txt"
rustc -Vv >> "$NL11_OUT/identity.txt"
for arm in base proto; do
  rm -rf ~/ab/nl11src-$arm; mkdir -p ~/ab/nl11src-$arm; tar -xf "$NL11_TAR" -C ~/ab/nl11src-$arm
  cp "$HERE/g1_mt_one.rs" ~/ab/nl11src-$arm/examples/g1_mt_one.rs
  cp "$HERE/xfree_probe.rs" ~/ab/nl11src-$arm/examples/nl11_xfree.rs
  ! grep -q '^production = .*misuse-checks' ~/ab/nl11src-$arm/Cargo.toml
  if [ $arm = proto ]; then
    (cd ~/ab/nl11src-$arm && patch -p1 -s < "$HERE/patches/nl11.patch")
    echo "proto: patch nl11 sha256 $(sha256sum "$HERE/patches/nl11.patch" | cut -c1-64)" >> "$NL11_OUT/identity.txt"
  fi
  (cd ~/ab/nl11src-$arm && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgt-nl11$arm cargo build -q --release --example nl11_xfree --example g1_mt_one --features production)
  echo "$arm: --features production" >> "$NL11_OUT/identity.txt"
  sha256sum ~/ab/tgt-nl11$arm/release/examples/nl11_xfree ~/ab/tgt-nl11$arm/release/examples/g1_mt_one >> "$NL11_OUT/identity.txt"
done
bin() { echo ~/ab/tgt-nl11$1/release/examples/$2; }
cd /tmp
for arm in base proto; do
  valgrind --tool=callgrind --toggle-collect='*producer*' --callgrind-out-file=/tmp/nl11.$arm.xfree.out $(bin $arm nl11_xfree) > /dev/null 2>&1
  callgrind_annotate /tmp/nl11.$arm.xfree.out 2>/dev/null | grep -E "PROGRAM TOTALS" | tr -d ',' > "$NL11_OUT/ir.$arm.xfree.txt"
  for w in larson mstress; do for t in 2 4; do
    valgrind --tool=callgrind --callgrind-out-file=/tmp/nl11.$arm.$w$t.out $(bin $arm g1_mt_one) $w sefer $t 100000 > /dev/null 2>&1
    callgrind_annotate /tmp/nl11.$arm.$w$t.out 2>/dev/null | grep -E "PROGRAM TOTALS" | tr -d ',' > "$NL11_OUT/ir.$arm.$w$t.txt"
  done; done
done
# timing: 5 reps of the order base,proto,proto,base (ABBA); sample index = position in the order
for rep in 1 2 3 4 5; do
  i=0
  for arm in base proto proto base; do
    i=$((i+1))
    for w in larson mstress; do for t in 2 4; do
      $(bin $arm g1_mt_one) $w sefer $t 1000000 > "$NL11_OUT/run.$arm.$w$t.$rep.$i.stdout"
    done; done
  done
done
echo ok
