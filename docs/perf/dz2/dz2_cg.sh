#!/bin/bash
# DZ-2: Ir of the bulk kernel under production vs production bench-internals (same source).
set -o pipefail
source ~/.cargo/env 2>/dev/null; export PATH=$HOME/.cargo/bin:$PATH RUSTC_WRAPPER= CARGO_PROFILE_RELEASE_DEBUG=line-tables-only
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
D=~/abw/worktrees/dz2; rm -rf $D; mkdir -p $D; cd $D || exit 2
tar -xf ${REPO:?set REPO to the repo root seen from WSL}/.rush/wsl/dz2_tree.tar || exit 3
O=~/ab/dz2; mkdir -p $O
for v in prod bi; do
  f="production"; [ $v = bi ] && f="production bench-internals"
  CARGO_TARGET_DIR=~/ab/tgt-dz2-$v timeout 1800 cargo build --release -q --example b4_pmu_probe --features "$f" > $O/build.$v.log 2>&1 || { echo build $v failed; tail $O/build.$v.log; exit 4; }
  B=~/ab/tgt-dz2-$v/release/examples/b4_pmu_probe; sha256sum $B
  for sz in 16 64; do for p in 50 100; do
    valgrind --tool=callgrind --callgrind-out-file=$O/cg.$v.$sz.$p.out $B bulk sefer 1024 $sz $p > /dev/null 2>&1
    echo "$v $sz $p $(callgrind_annotate --auto=no $O/cg.$v.$sz.$p.out 2>/dev/null | grep -m1 'PROGRAM TOTALS')"
  done; done
done
