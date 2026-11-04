#!/bin/bash
# NL-9: the default flip. ONE source tree (a git archive of the measured commit) built twice:
# arm `on` = `--features "production misuse-checks"` (the former default), arm `off` =
# `--features production` (the new default). bulk 1024 x {16,64}: callgrind Ir and perf stat
# cycles/instructions, timing in ABBA order; larson/mstress T=1 and T=4: callgrind Ir, Mops
# (ABBA) and peak RSS (/usr/bin/time %M). Usage:
#   NL9_TAR=<git archive tar> NL9_OUT=<raw output dir> bash nl9.sh
set -e
source ~/.cargo/env
: "${NL9_TAR:?set NL9_TAR to the git archive tar of the measured commit}"
: "${NL9_OUT:?set NL9_OUT to the raw output dir}"
P=${NL9_PERF:-$(ls /usr/lib/linux-tools-*/perf | head -1)}
HERE=$(cd "$(dirname "$0")" && pwd)
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
mkdir -p "$NL9_OUT"; : > "$NL9_OUT/identity.txt"
rm -rf ~/ab/nl9src; mkdir -p ~/ab/nl9src; tar -xf "$NL9_TAR" -C ~/ab/nl9src
cp "$HERE/g1_mt_one.rs" ~/ab/nl9src/examples/g1_mt_one.rs
! grep -q '^production = .*misuse-checks' ~/ab/nl9src/Cargo.toml
sha256sum "$NL9_TAR" >> "$NL9_OUT/identity.txt"
rustc -Vv >> "$NL9_OUT/identity.txt"
declare -A FEAT=( [on]="production misuse-checks" [off]="production" )
for arm in on off; do
  (cd ~/ab/nl9src && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgt-nl9$arm cargo build -q --release --example b4_pmu_probe --example g1_mt_one --features "${FEAT[$arm]}")
  echo "$arm: --features ${FEAT[$arm]}" >> "$NL9_OUT/identity.txt"
  sha256sum ~/ab/tgt-nl9$arm/release/examples/b4_pmu_probe ~/ab/tgt-nl9$arm/release/examples/g1_mt_one >> "$NL9_OUT/identity.txt"
done
bin() { echo ~/ab/tgt-nl9$1/release/examples/$2; }
cd /tmp
for arm in on off; do
  for sz in 16 64; do
    valgrind --tool=callgrind --callgrind-out-file=/tmp/nl9.$arm.bulk$sz.out $(bin $arm b4_pmu_probe) bulk sefer 1024 $sz 400 > /dev/null 2>&1
    callgrind_annotate /tmp/nl9.$arm.bulk$sz.out 2>/dev/null | grep -E "PROGRAM TOTALS" | tr -d ',' > "$NL9_OUT/ir.$arm.bulk$sz.txt"
  done
  for w in larson mstress; do for t in 1 4; do
    valgrind --tool=callgrind --callgrind-out-file=/tmp/nl9.$arm.$w$t.out $(bin $arm g1_mt_one) $w sefer $t 100000 > /dev/null 2>&1
    callgrind_annotate /tmp/nl9.$arm.$w$t.out 2>/dev/null | grep -E "PROGRAM TOTALS" | tr -d ',' > "$NL9_OUT/ir.$arm.$w$t.txt"
  done; done
done
# timing: 5 reps of the order on,off,off,on (ABBA); sample index = position in the order
for rep in 1 2 3 4 5; do
  i=0
  for arm in on off off on; do
    i=$((i+1))
    for sz in 16 64; do
      $P stat -e cycles:u,instructions:u -x, -o "$NL9_OUT/stat.$arm.bulk$sz.$rep.$i.txt" $(bin $arm b4_pmu_probe) bulk sefer 1024 $sz 4000 > "$NL9_OUT/run.$arm.bulk$sz.$rep.$i.stdout"
    done
    for w in larson mstress; do for t in 1 4; do
      /usr/bin/time -f '%M' -o "$NL9_OUT/rss.$arm.$w$t.$rep.$i.txt" $(bin $arm g1_mt_one) $w sefer $t 1000000 > "$NL9_OUT/run.$arm.$w$t.$rep.$i.stdout"
    done; done
  done
done
echo ok
