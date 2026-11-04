#!/bin/bash
# NL-4/5/6: what do the remaining guard classes cost with misuse-checks OFF? ONE source tree
# (a git archive of the measured commit) for every arm; the arms differ only by a patch that puts
# one guard class behind the same cfg as the state byte. base = no patch. All arms build
# `production` minus `misuse-checks` (the probes require the literal feature `production`).
# bulk 1024 x {16,64}: callgrind Ir and perf stat cycles; larson/mstress T=1 and T=4: callgrind Ir
# and Mops. Timing order per repetition: base nl4 nl5 nl6 all all nl6 nl5 nl4 base (palindrome),
# so 5 repetitions give 10 samples per arm. Usage:
#   NL46_TAR=<git archive tar> NL46_OUT=<raw output dir> bash nl46.sh
set -e
source ~/.cargo/env
: "${NL46_TAR:?set NL46_TAR to the git archive tar of the measured commit}"
: "${NL46_OUT:?set NL46_OUT to the raw output dir}"
P=${NL46_PERF:-$(ls /usr/lib/linux-tools-*/perf | head -1)}
HERE=$(cd "$(dirname "$0")" && pwd)
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
ARMS=(base nl4 nl5 nl6 all)
patches_of() { case $1 in base) ;; nl4) echo nl4;; nl5) echo nl5;; nl6) echo nl6;; all) echo nl4 nl5 nl6;; esac; }
mkdir -p "$NL46_OUT"; : > "$NL46_OUT/identity.txt"
sha256sum "$NL46_TAR" >> "$NL46_OUT/identity.txt"
for arm in "${ARMS[@]}"; do
  rm -rf ~/ab/nl46src-$arm; mkdir -p ~/ab/nl46src-$arm; tar -xf "$NL46_TAR" -C ~/ab/nl46src-$arm
  cp "$HERE/g1_mt_one.rs" ~/ab/nl46src-$arm/examples/g1_mt_one.rs
  sed -i 's/"primordial-lazy-commit", "misuse-checks"]/"primordial-lazy-commit"]/' ~/ab/nl46src-$arm/Cargo.toml
  ! grep -q '^production = .*misuse-checks' ~/ab/nl46src-$arm/Cargo.toml
  for p in $(patches_of $arm); do
    (cd ~/ab/nl46src-$arm && patch -p1 -s < "$HERE/patches/$p.patch")
    echo "$arm: patch $p sha256 $(sha256sum "$HERE/patches/$p.patch" | cut -c1-64)" >> "$NL46_OUT/identity.txt"
  done
  (cd ~/ab/nl46src-$arm && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgt-nl46$arm cargo build -q --release --example b4_pmu_probe --example g1_mt_one --features production)
  echo "$arm: --features production (misuse-checks off)" >> "$NL46_OUT/identity.txt"
  sha256sum ~/ab/tgt-nl46$arm/release/examples/b4_pmu_probe ~/ab/tgt-nl46$arm/release/examples/g1_mt_one >> "$NL46_OUT/identity.txt"
done
bin() { echo ~/ab/tgt-nl46$1/release/examples/$2; }
cd /tmp
for arm in "${ARMS[@]}"; do
  for sz in 16 64; do
    valgrind --tool=callgrind --callgrind-out-file=/tmp/nl46.$arm.bulk$sz.out $(bin $arm b4_pmu_probe) bulk sefer 1024 $sz 400 > /dev/null 2>&1
    callgrind_annotate /tmp/nl46.$arm.bulk$sz.out 2>/dev/null | grep -E "PROGRAM TOTALS" | tr -d ',' > "$NL46_OUT/ir.$arm.bulk$sz.txt"
  done
  for w in larson mstress; do for t in 1 4; do
    valgrind --tool=callgrind --callgrind-out-file=/tmp/nl46.$arm.$w$t.out $(bin $arm g1_mt_one) $w sefer $t 100000 > /dev/null 2>&1
    callgrind_annotate /tmp/nl46.$arm.$w$t.out 2>/dev/null | grep -E "PROGRAM TOTALS" | tr -d ',' > "$NL46_OUT/ir.$arm.$w$t.txt"
  done; done
done
ORDER=(base nl4 nl5 nl6 all all nl6 nl5 nl4 base)
for rep in 1 2 3 4 5; do
  i=0
  for arm in "${ORDER[@]}"; do
    i=$((i+1))
    for sz in 16 64; do
      $P stat -e cycles:u,instructions:u -x, -o "$NL46_OUT/stat.$arm.bulk$sz.$rep.$i.txt" $(bin $arm b4_pmu_probe) bulk sefer 1024 $sz 4000 > "$NL46_OUT/run.$arm.bulk$sz.$rep.$i.stdout"
    done
    for w in larson mstress; do for t in 1 4; do
      $(bin $arm g1_mt_one) $w sefer $t 1000000 > "$NL46_OUT/run.$arm.$w$t.$rep.$i.stdout"
    done; done
  done
done
echo ok
