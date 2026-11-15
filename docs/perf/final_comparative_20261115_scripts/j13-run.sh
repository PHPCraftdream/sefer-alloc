#!/bin/bash
# Runs one phase of the registered J13 study inside the WSL2 guest. Usage: j13-run.sh build|smoke|full
set -euo pipefail
MODE="$1"
SCRIPTS=/mnt/d/dev/rust/sefer-alloc/worktrees/j13b-budget-20261106/.rush/stdin/j13-local
export PATH="$HOME/tools/bin:$HOME/.cargo/bin:$PATH"
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS
cd "$HOME/work/sefer-j13"
export RUN_ID="local-20261115-$MODE"
export RUN_ATTEMPT=1
case "$MODE" in
  build)
    export RUNNER_TEMP="$HOME/work/j13-build"
    rm -rf "$RUNNER_TEMP"; mkdir -p "$RUNNER_TEMP"
    bash "$SCRIPTS/j13-build.sh"
    echo BUILD-DONE ;;
  smoke|full)
    export RUNNER_TEMP="$HOME/work/j13-$MODE"
    rm -rf "$RUNNER_TEMP"; mkdir -p "$RUNNER_TEMP"
    cp -r "$HOME/work/j13-build/g1-big-study-build" "$RUNNER_TEMP/g1-big-study-build"
    if [ "$MODE" = smoke ]; then export SMOKE=true; else export SMOKE=false; fi
    bash "$SCRIPTS/j13-measure.sh"
    echo "MEASURE-DONE $MODE" ;;
  *) echo "unknown mode $MODE"; exit 2 ;;
esac
