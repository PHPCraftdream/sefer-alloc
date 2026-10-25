#!/bin/bash
set -euo pipefail
repo=$1
out=$2
stage=$3
mkdir -p "$out/tmp"
cd "$repo"
export CARGO_TARGET_DIR="$out/target"
export TMPDIR="$out/tmp"
export RUSTC_WRAPPER=
export CARGO_BUILD_RUSTC_WRAPPER=
case "$stage" in
  build)
    cargo bench --locked --bench perf_gate_iai --features 'production bench-internals internals' --no-run --message-format=json > "$out/build.jsonl" 2> "$out/build.stderr"
    ;;
  measure)
    export CARGO_MANIFEST_DIR="$repo"
    "${4:?exact built executable}" > "$out/measure.stdout" 2> "$out/measure.stderr"
    ;;
  *) exit 2 ;;
esac
printf 'IAI %s complete: %s\n' "$stage" "$repo"
