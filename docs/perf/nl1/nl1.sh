#!/bin/bash
# NL-1: `production` (misuse-checks on) against the same bundle without it, ONE source tree
# for both arms (a git archive of the measured commit). bulk 1024 x {16,64}: callgrind Ir and
# perf stat cycles, timing in ABBA order; larson/mstress T=1 and T=4: callgrind Ir and Mops
# (ABBA). Usage: NL1_TAR=<git archive tar> NL1_OUT=<raw output dir> bash nl1.sh
set -e
source ~/.cargo/env
: "${NL1_TAR:?set NL1_TAR to the git archive tar of the measured commit}"
: "${NL1_OUT:?set NL1_OUT to the raw output dir}"
P=${NL1_PERF:-$(ls /usr/lib/linux-tools-*/perf | head -1)}
HERE=$(cd "$(dirname "$0")" && pwd)
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
mkdir -p "$NL1_OUT"; : > "$NL1_OUT/identity.txt"
# The probes require the literal feature `production`, so the off arm is the same tree with
# `misuse-checks` removed from the `production` line of Cargo.toml (recorded in identity.txt).
for arm in on off; do
  rm -rf ~/ab/nl1src-$arm; mkdir -p ~/ab/nl1src-$arm; tar -xf "$NL1_TAR" -C ~/ab/nl1src-$arm
  cp "$HERE/g1_mt_one.rs" ~/ab/nl1src-$arm/examples/g1_mt_one.rs
done
sed -i 's/"primordial-lazy-commit", "misuse-checks"]/"primordial-lazy-commit"]/' ~/ab/nl1src-off/Cargo.toml
grep -q '^production = .*misuse-checks' ~/ab/nl1src-on/Cargo.toml
! grep -q '^production = .*misuse-checks' ~/ab/nl1src-off/Cargo.toml
sha256sum "$NL1_TAR" >> "$NL1_OUT/identity.txt"
diff -u ~/ab/nl1src-on/Cargo.toml ~/ab/nl1src-off/Cargo.toml >> "$NL1_OUT/identity.txt" || true
for arm in on off; do
  (cd ~/ab/nl1src-$arm && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgt-nl1$arm      cargo build -q --release --example b4_pmu_probe --example g1_mt_one --features production)
  echo "$arm: --features production" >> "$NL1_OUT/identity.txt"
  sha256sum ~/ab/tgt-nl1$arm/release/examples/b4_pmu_probe ~/ab/tgt-nl1$arm/release/examples/g1_mt_one >> "$NL1_OUT/identity.txt"
done
bin() { echo ~/ab/tgt-nl1$1/release/examples/$2; }
cd /tmp
for arm in on off; do
  for sz in 16 64; do
    valgrind --tool=callgrind --callgrind-out-file=/tmp/nl1.$arm.bulk$sz.out $(bin $arm b4_pmu_probe) bulk sefer 1024 $sz 400 > /dev/null 2>&1
    callgrind_annotate /tmp/nl1.$arm.bulk$sz.out 2>/dev/null | grep -E "PROGRAM TOTALS" | tr -d ',' > "$NL1_OUT/ir.$arm.bulk$sz.txt"
  done
  for w in larson mstress; do for t in 1 4; do
    valgrind --tool=callgrind --callgrind-out-file=/tmp/nl1.$arm.$w$t.out $(bin $arm g1_mt_one) $w sefer $t 100000 > /dev/null 2>&1
    callgrind_annotate /tmp/nl1.$arm.$w$t.out 2>/dev/null | grep -E "PROGRAM TOTALS" | tr -d ',' > "$NL1_OUT/ir.$arm.$w$t.txt"
  done; done
done
# timing: 5 reps of the order on,off,off,on (ABBA); sample index = position in the order
for rep in 1 2 3 4 5; do
  i=0
  for arm in on off off on; do
    i=$((i+1))
    for sz in 16 64; do
      $P stat -e cycles:u,instructions:u -x, -o "$NL1_OUT/stat.$arm.bulk$sz.$rep.$i.txt" $(bin $arm b4_pmu_probe) bulk sefer 1024 $sz 4000 > "$NL1_OUT/run.$arm.bulk$sz.$rep.$i.stdout"
    done
    for w in larson mstress; do for t in 1 4; do
      $(bin $arm g1_mt_one) $w sefer $t 1000000 > "$NL1_OUT/run.$arm.$w$t.$rep.$i.stdout"
    done; done
  done
done
echo ok
