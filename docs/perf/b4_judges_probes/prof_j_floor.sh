#!/bin/bash
# Floor probe (examples/zz_floor_probe from j_floor_probe.rs): larson and
# mstress at T=1 for sefer / mimalloc / bump-always / bump-never.
# Prints MARGINAL Ir/step at k=50k->100k, twice per combo (repro check: the
# two passes must agree exactly; callgrind is deterministic).
# Usage (WSL): bash prof_j_floor.sh <label>   (expects .rush/wsl/<label>.tar)
set -e
L=$1
D=~/ab/$L; rm -rf $D; mkdir -p $D; tar -xf ${REPO_WSL:?set REPO_WSL to the repo root as seen from WSL}/.rush/wsl/$L.tar -C $D
cp ${REPO_WSL:?set REPO_WSL to the repo root as seen from WSL}/.rush/wsl/j_floor_probe.rs $D/examples/zz_floor_probe.rs
printf '\n[[example]]\nname = "zz_floor_probe"\nrequired-features = ["alloc-global"]\n' >> $D/Cargo.toml
(cd $D && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgtf-$L cargo build -q --release --example zz_floor_probe --features production)
cd ~/ab
BIN=~/ab/tgtf-$L/release/examples/zz_floor_probe
for pass in 1 2; do
  for w in larson mstress; do
    for a in sefer mimalloc bumpa bumpn; do
      for k in 50000 100000; do
        valgrind --tool=callgrind --callgrind-out-file=pjf.$L.$pass.$w.$a.$k.out $BIN $a $w $k >/dev/null 2>&1
      done
      t1=$(callgrind_annotate pjf.$L.$pass.$w.$a.50000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//')
      t2=$(callgrind_annotate pjf.$L.$pass.$w.$a.100000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//')
      echo "FLOOR pass=$pass $w $a marginal Ir/step: $(( (t2 - t1) / 50000 ))"
    done
  done
done
