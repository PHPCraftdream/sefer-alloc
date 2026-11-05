#!/bin/bash
# NL-2 ceiling: the free lane's `g >= bump >> 4` guard replaced by the constant bound `g >= FOOTPRINT` (one dependent
# load and a shift fewer) in a build that KEEPS misuse-checks. ONE source tree (git archive of the measured commit), two arms
# that differ by patches/nl2.patch; both build `--features "production misuse-checks"`. bulk 1024 x {16,64}: callgrind Ir and
# perf stat cycles; larson/mstress T=1: callgrind Ir and Mops. Timing: 5 repetitions of base, proto, proto, base (ABBA).
#   NL2_TAR=<git archive tar> NL2_OUT=<raw output dir> bash nl2.sh
set -e
source ~/.cargo/env
: "${NL2_TAR:?set NL2_TAR to the git archive tar of the measured commit}"
: "${NL2_OUT:?set NL2_OUT to the raw output dir}"
P=${NL2_PERF:-$(ls /usr/lib/linux-tools-*/perf | head -1)}
HERE=$(cd "$(dirname "$0")" && pwd)
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
FEAT="production misuse-checks"
mkdir -p "$NL2_OUT"; : > "$NL2_OUT/identity.txt"
sha256sum "$NL2_TAR" >> "$NL2_OUT/identity.txt"
rustc -Vv >> "$NL2_OUT/identity.txt"
for arm in base proto; do
  rm -rf ~/ab/nl2src-$arm; mkdir -p ~/ab/nl2src-$arm; tar -xf "$NL2_TAR" -C ~/ab/nl2src-$arm
  cp "$HERE/../nl9/g1_mt_one.rs" ~/ab/nl2src-$arm/examples/g1_mt_one.rs
  if [ $arm = proto ]; then
    (cd ~/ab/nl2src-$arm && patch -p1 -s < "$HERE/patches/nl2.patch")
    echo "proto: patch nl2 sha256 $(sha256sum "$HERE/patches/nl2.patch" | cut -c1-64)" >> "$NL2_OUT/identity.txt"
  fi
  (cd ~/ab/nl2src-$arm && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgt-nl2$arm cargo build -q --release --example b4_pmu_probe --example g1_mt_one --features "$FEAT")
  echo "$arm: --features $FEAT" >> "$NL2_OUT/identity.txt"
  sha256sum ~/ab/tgt-nl2$arm/release/examples/b4_pmu_probe ~/ab/tgt-nl2$arm/release/examples/g1_mt_one >> "$NL2_OUT/identity.txt"
done
bin() { echo ~/ab/tgt-nl2$1/release/examples/$2; }
cd /tmp
for arm in base proto; do
  for sz in 16 64; do
    valgrind --tool=callgrind --callgrind-out-file=/tmp/nl2.$arm.bulk$sz.out $(bin $arm b4_pmu_probe) bulk sefer 1024 $sz 400 > /dev/null 2>&1
    callgrind_annotate /tmp/nl2.$arm.bulk$sz.out 2>/dev/null | grep -E "PROGRAM TOTALS" | tr -d ',' > "$NL2_OUT/ir.$arm.bulk$sz.txt"
  done
  for w in larson mstress; do for t in 1 4; do
    valgrind --tool=callgrind --callgrind-out-file=/tmp/nl2.$arm.$w$t.out $(bin $arm g1_mt_one) $w sefer $t 100000 > /dev/null 2>&1
    callgrind_annotate /tmp/nl2.$arm.$w$t.out 2>/dev/null | grep -E "PROGRAM TOTALS" | tr -d ',' > "$NL2_OUT/ir.$arm.$w$t.txt"
  done; done
done
for rep in 1 2 3 4 5; do
  i=0
  for arm in base proto proto base; do
    i=$((i+1))
    for sz in 16 64; do
      $P stat -e cycles:u,instructions:u -x, -o "$NL2_OUT/stat.$arm.bulk$sz.$rep.$i.txt" $(bin $arm b4_pmu_probe) bulk sefer 1024 $sz 4000 > "$NL2_OUT/run.$arm.bulk$sz.$rep.$i.stdout"
    done
    for w in larson mstress; do for t in 1 4; do
      $(bin $arm g1_mt_one) $w sefer $t 1000000 > "$NL2_OUT/run.$arm.$w$t.$rep.$i.stdout"
    done; done
  done
done
echo ok
