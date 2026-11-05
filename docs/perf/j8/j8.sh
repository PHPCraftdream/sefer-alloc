#!/bin/bash
# J8: ceiling of skipping all-zero directory words in blocks of 8 (docs/perf/J8_DIRECTORY_ZERO_WORD_SKIP_PREREG.md).
# ONE source tree (a git archive of the base commit), two arms that differ only by patches/j8.patch; both build
# `--release --features production` with the repository's alignment RUSTFLAGS and a line-table-free release profile.
#   J8_TAR=<git archive tar> J8_OUT=<raw output dir> bash j8.sh
# Output (J8_OUT): identity.txt; run.<sc>.<arm>.json + ir.<sc>.<arm>.txt (callgrind totals and function table of the judge at scale 0.25);
# fn.<arm>.txt (self Ir and call counts of the discovery function on grow64 at scale 0.5); bulk.<arm>.<size>.txt (b4_pmu_probe Ir).
set -e
source ~/.cargo/env
: "${J8_TAR:?set J8_TAR to the git archive tar of the base commit}"
: "${J8_OUT:?set J8_OUT to the raw output dir}"
HERE=$(cd "$(dirname "$0")" && pwd)
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
mkdir -p "$J8_OUT"; : > "$J8_OUT/identity.txt"
echo "tar $(sha256sum "$J8_TAR" | cut -d' ' -f1)" >> "$J8_OUT/identity.txt"
rustc -Vv | sed -n '1p;/^host/p;/^LLVM/p' >> "$J8_OUT/identity.txt"
for arm in base patch; do
  rm -rf ~/ab/j8src-$arm; mkdir -p ~/ab/j8src-$arm; tar -xf "$J8_TAR" -C ~/ab/j8src-$arm
  if [ $arm = patch ]; then
    (cd ~/ab/j8src-$arm && patch -p1 -s < "$HERE/patches/j8.patch")
    echo "patch j8.patch sha256 $(sha256sum "$HERE/patches/j8.patch" | cut -d' ' -f1)" >> "$J8_OUT/identity.txt"
  fi
  (cd ~/ab/j8src-$arm && CARGO_TARGET_DIR=~/ab/tgt-j8$arm cargo build -q --release --example g1_wild_judge --example b4_pmu_probe --features production)
  echo "$arm: --features production" >> "$J8_OUT/identity.txt"
  sha256sum ~/ab/tgt-j8$arm/release/examples/g1_wild_judge ~/ab/tgt-j8$arm/release/examples/b4_pmu_probe | sed 's#  .*/#  #' | sed "s#^#$arm bin #" >> "$J8_OUT/identity.txt"
done
bin() { echo ~/ab/tgt-j8$1/release/examples/$2; }
cd /tmp
for arm in base patch; do
  for sc in grow64 grow1k growmix regrow wild phase; do
    "$(bin $arm g1_wild_judge)" $sc sefer 1 0.25 | tail -1 > "$J8_OUT/run.$sc.$arm.json"
    valgrind --tool=callgrind --callgrind-out-file=/tmp/j8.$sc.$arm.out "$(bin $arm g1_wild_judge)" $sc sefer 1 0.25 > /dev/null 2>&1
    callgrind_annotate /tmp/j8.$sc.$arm.out 2>/dev/null | sed -e 's#/home/[^/]*/#<scratch>/#g' | sed -n '/PROGRAM TOTALS/,$p' | head -60 > "$J8_OUT/ir.$sc.$arm.txt"
  done
  # discovery function on grow64 at scale 0.5: absolute positions so call counts can be read
  valgrind --tool=callgrind --compress-strings=no --compress-pos=no --callgrind-out-file=/tmp/j8.fn.$arm.out "$(bin $arm g1_wild_judge)" grow64 sefer 1 0.5 > /dev/null 2>&1
  callgrind_annotate /tmp/j8.fn.$arm.out 2>/dev/null | sed -e 's#/home/[^/]*/#<scratch>/#g' | sed -n '/PROGRAM TOTALS/,$p' | head -40 > "$J8_OUT/fn.$arm.txt"
  grep -E "^calls=|^cfn=" /tmp/j8.fn.$arm.out | awk '/^cfn=/{c=$0; next} /^calls=/{split($0,a,"[= ]"); n[c]+=a[2]} END{for(k in n) print n[k], k}' | grep -E "find_segment_with_free_impl|try_carve_batch|process_ingress" > "$J8_OUT/calls.$arm.txt"
  for sz in 16 64; do
    valgrind --tool=callgrind --callgrind-out-file=/tmp/j8.bulk$sz.$arm.out "$(bin $arm b4_pmu_probe)" bulk sefer 1024 $sz 400 > /dev/null 2>&1
    callgrind_annotate /tmp/j8.bulk$sz.$arm.out 2>/dev/null | grep -E "PROGRAM TOTALS" | tr -d ',' > "$J8_OUT/bulk.$arm.$sz.txt"
  done
done
echo ok
