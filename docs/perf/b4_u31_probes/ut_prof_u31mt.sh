#!/bin/bash
: "${REPO_WSL:?set REPO_WSL to the repository root}"
# ut_prof_u31mt.sh — B4-29/U31 chunk 3: counters-only MT run larson|mstress at
# T in {2,4} (WSL). Usage: bash ut_prof_u31mt.sh <label> <larson|mstress> <threads> [steps]
# Expects .rush/wsl/<label>.tar (snapshot by ut_prof_u31mt_win.sh). Builds
# examples/ut_u31mt_probe.rs with production + internals + bench-internals,
# runs ONE counters pass (no valgrind; Relaxed counters, not wall-clock).
set -e
L=$1; W=$2; T=$3; STEPS=${4:-100000}
BASE=${REPO_WSL}/.rush/wsl
D=~/ab/$L
rm -rf "$D"; mkdir -p "$D"; tar -xf $BASE/$L.tar -C "$D"
cp $BASE/ut_u31mt_probe.rs "$D/examples/ut_u31mt_probe.rs"
printf '\n[[example]]\nname = "ut_u31mt_probe"\nrequired-features = ["alloc-global", "bench-internals", "internals"]\n' >> "$D/Cargo.toml"
(cd "$D" && CARGO_TARGET_DIR=~/ab/tgt-$L cargo build -q --release --example ut_u31mt_probe --features production,bench-internals,internals)
BIN=~/ab/tgt-$L/release/examples/ut_u31mt_probe
LOG=$BASE/$L.log
{
  cat $BASE/$L.diffhash
  echo "workload $W threads $T steps $STEPS"
} > "$LOG"
cd ~/ab
tag="$L.$W.t$T.s$STEPS"
timeout 600 "$BIN" sefer $W $T $STEPS --counters > $tag.counters 2>&1 || true
cat $tag.counters >> "$LOG"
cp $tag.counters $BASE/ 2>/dev/null || true
cp $LOG $BASE/ 2>/dev/null || true
echo "== $LOG =="
cat "$LOG"
