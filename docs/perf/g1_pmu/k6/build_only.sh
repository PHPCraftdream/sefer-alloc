#!/bin/bash
# build_only.sh <label>: unpack <label>.tar and build the probe (no runs)
set -e
L=$1; BASE=${G1_BASE:?set G1_BASE to the directory holding <label>.tar}; D=~/ab/$L
rm -rf "$D"; mkdir -p "$D"; tar -xf $BASE/$L.tar -C "$D"
(cd "$D" && find . -name "*.rs" -exec touch {} + && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgt-$L cargo build -q --release --example b4_pmu_probe --features production)
echo built ~/ab/tgt-$L/release/examples/b4_pmu_probe
