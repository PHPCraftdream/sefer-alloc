#!/bin/bash
# P220: callgrind cache simulation of growmix, sefer vs mi, Linux judge from a git archive of the measured commit,
# `--release --features production`, the repository's alignment RUSTFLAGS plus line tables (recipe of docs/perf/j10/j10.sh).
#   P220_TAR=<git archive tar> P220_OUT=<raw dir> [P220_SCALE=0.25] [CARGO_BUILD_JOBS=2] bash cachesim.sh
set -e
source ~/.cargo/env
: "${P220_TAR:?set P220_TAR}"
: "${P220_OUT:?set P220_OUT}"
SCALE=${P220_SCALE:-0.25}
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
rm -rf ~/ab/p220src; mkdir -p ~/ab/p220src "$P220_OUT"; tar -xf "$P220_TAR" -C ~/ab/p220src
(cd ~/ab/p220src && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgt-p220 nice -n 19 cargo build -q --release --example g1_wild_judge --features production)
BIN=~/ab/tgt-p220/release/examples/g1_wild_judge
{ echo "bin $(sha256sum "$BIN" | cut -d' ' -f1)"; echo "tar $(sha256sum "$P220_TAR" | cut -d' ' -f1)"; echo "scale $SCALE"; rustc -V; valgrind --version; } > "$P220_OUT/cg_ident.txt"
cd /tmp
for arm in sefer mi; do
  "$BIN" growmix $arm 1 "$SCALE" | tail -1 > "$P220_OUT/cg_run.growmix.$arm.json"
  nice -n 19 valgrind --tool=callgrind --cache-sim=yes --dump-line=yes --compress-strings=no --compress-pos=no \
    --callgrind-out-file=/tmp/p220.growmix.$arm.out "$BIN" growmix $arm 1 "$SCALE" > /dev/null 2>&1
  sed -E 's#/home/[^/]+/#<home>/#g' /tmp/p220.growmix.$arm.out | gzip -9n > "$P220_OUT/cg_growmix.$arm.out.gz"
  callgrind_annotate --show=Ir,Dr,Dw,D1mr,D1mw,DLmr,DLmw /tmp/p220.growmix.$arm.out 2>/dev/null | sed -E 's#/home/[^/]+/#<home>/#g' | sed -n '/PROGRAM TOTALS/,$p' | head -45 > "$P220_OUT/cg_fn.growmix.$arm.txt"
done
echo ok
