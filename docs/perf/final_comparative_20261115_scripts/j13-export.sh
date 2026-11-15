#!/bin/bash
# Copies the registered study evidence out of the guest; files above 200 KiB are gzip-compressed (repo raw-artifact policy).
set -euo pipefail
SRC="$HOME/work/j13-full/j13-limited-evidence"
DST=/mnt/d/dev/rust/sefer-alloc/worktrees/j13b-budget-20261106/docs/perf/j13_final_20261115
rm -rf "$DST"; mkdir -p "$DST"
cd "$SRC"
find . -type f | while read -r f; do
  mkdir -p "$DST/$(dirname "$f")"
  if [ "$(stat -c %s "$f")" -gt 204800 ]; then gzip -9 -c "$f" > "$DST/$f.gz"; else cp "$f" "$DST/$f"; fi
done
# cpuinfo/meminfo/raw preflight stay as copied; sizes:
du -sh "$DST" | tail -1
find "$DST" -type f | wc -l
