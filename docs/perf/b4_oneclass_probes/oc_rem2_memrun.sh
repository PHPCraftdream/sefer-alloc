#!/bin/bash
# oc_rem2_memrun.sh (WSL): unpack a shoulder tarball (A tar already contains
# the a3 hook patch), inject the oc_mem_probe example, build it (release,
# production + internals + bench-internals), run ONE process for the given
# mode, write MEM output + RC to the FINAL log. Modeled on oc_remeas_memrun.sh.
# Usage: bash oc_rem2_memrun.sh <label> <tar-file> <mode> <final-log> <identity-line>
set -e
L=$1; TAR=$2; MODE=${3:-full}; FLOG=$4; IDENT=$5
[ -f "$TAR" ] || { echo "oc_rem2_memrun: tar not found: $TAR" >&2; exit 1; }
BASE=${REPO_WSL:?set REPO_WSL to the repo path inside WSL}/.rush/wsl
D=~/ab/$L
rm -rf "$D"; mkdir -p "$D"
tar -xf "$TAR" -C "$D"
cp $BASE/oc_mem_probe.rs "$D/examples/zz_oc_mem_probe.rs"
grep -q 'zz_oc_mem_probe' "$D/Cargo.toml" || printf '\n[[example]]\nname = "zz_oc_mem_probe"\nrequired-features = ["alloc-global", "alloc-decommit", "internals", "bench-internals"]\n' >> "$D/Cargo.toml"
cd "$D"
{
  echo "$IDENT"
  echo "features: production,internals,bench-internals"
  echo "build-cmd: cargo build --release --example zz_oc_mem_probe --features production,internals,bench-internals (CARGO_TARGET_DIR isolated)"
} >> "$FLOG"
CARGO_TARGET_DIR=~/ab/tgt4-$L timeout 600 cargo build -q --release --example zz_oc_mem_probe \
  --features "production internals bench-internals" >> "$FLOG" 2>&1
BUILD_RC=$?
echo "BUILD_RC=$BUILD_RC" >> "$FLOG"
if [ "$BUILD_RC" != 0 ]; then echo "RUN_RC=skipped" >> "$FLOG"; exit "$BUILD_RC"; fi
~/ab/tgt4-$L/release/examples/zz_oc_mem_probe "$MODE" >> "$FLOG" 2>&1
RUN_RC=$?
echo "RUN_RC=$RUN_RC" >> "$FLOG"
echo "mem $L $MODE: BUILD_RC=$BUILD_RC RUN_RC=$RUN_RC"
