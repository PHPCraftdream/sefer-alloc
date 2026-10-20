#!/bin/bash
# Unified runtime/const probe (examples/zz_uniconst from j_uniconst_probe.rs).
# One binary, one run: --toggle-collect per wrapper for
# w_alloc, w_free (runtime Layout, 768 live, 16..512 B) and
# w_alloc16/w_free16/w_alloc64/w_free64 (const Layout, 16-slot window).
# Prints marginal Ir/op for each wrapper at 50k->100k steps, twice.
# Usage (WSL): bash prof_j_uniconst.sh <label> [sefer|mimalloc]
set -e
L=$1; A=${2:-sefer}
D=~/ab/$L; rm -rf $D; mkdir -p $D; tar -xf ${REPO_WSL:?set REPO_WSL to the repo root as seen from WSL}/.rush/wsl/$L.tar -C $D
cp ${REPO_WSL:?set REPO_WSL to the repo root as seen from WSL}/.rush/wsl/j_uniconst_probe.rs $D/examples/zz_uniconst.rs
printf '\n[[example]]\nname = "zz_uniconst"\nrequired-features = ["alloc-global"]\n' >> $D/Cargo.toml
(cd $D && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgtu-$L cargo build -q --release --example zz_uniconst --features production)
cd ~/ab
BIN=~/ab/tgtu-$L/release/examples/zz_uniconst
for pass in 1 2; do
  for w in w_alloc_rt w_free_rt w_alloc16 w_free16 w_alloc64 w_free64; do
    for k in 50000 100000; do
      valgrind --tool=callgrind --toggle-collect="*zz_uniconst::$w*" --callgrind-out-file=pu.$L.$A.$pass.$w.$k.out $BIN $A $k >/dev/null 2>&1
    done
    t1=$(callgrind_annotate pu.$L.$A.$pass.$w.50000.out | grep TOTALS | head -1 | tr -d ', ' | grep -oE '^[0-9]+')
    t2=$(callgrind_annotate pu.$L.$A.$pass.$w.100000.out | grep TOTALS | head -1 | tr -d ', ' | grep -oE '^[0-9]+')
    echo "UNICONST pass=$pass $A $w marginal Ir/op: $(( (t2 - t1) / 50000 ))"
  done
done
