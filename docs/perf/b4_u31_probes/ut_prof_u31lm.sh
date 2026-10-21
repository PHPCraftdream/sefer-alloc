#!/bin/bash
: "${REPO_WSL:?set REPO_WSL to the repository root}"
# ut_prof_u31lm.sh — B4-29/U31 chunk 2: callgrind larson|mstress at T=1 (WSL).
# Usage: bash ut_prof_u31lm.sh <label> <larson|mstress> [steps]
# Expects .rush/wsl/<label>.tar (working-tree snapshot of <checkout>,
# made by ut_prof_u31lm_win.sh). Adds examples/ut_larson_probe2.rs, builds it
# with production + bench-internals + internals (release + line tables), runs
# callgrind once at T=1 (steps per thread, default 100000), then once without
# valgrind with --counters for the B4_* observer deltas.
# Attribution: same method as ut_prof_u31.sh run3 — self-Ir from the flat
# "Ir file:function" section of callgrind_annotate (partitions PROGRAM TOTALS;
# no --auto double counting of inlined frames), for
# src/alloc_core/small/alloc_core_small/find_segment/scan.rs and
# src/alloc_core/segment/remote_bitmap/bitmap_scan.rs, plus the inclusive
# find_segment_with_free row. Scan share = scan self-Ir / PROGRAM TOTALS.
set -e
L=$1; W=$2; STEPS=${3:-100000}
BASE=${REPO_WSL}/.rush/wsl
D=~/ab/$L
rm -rf "$D"; mkdir -p "$D"; tar -xf $BASE/$L.tar -C "$D"
cp $BASE/ut_larson_probe2.rs "$D/examples/ut_larson_probe2.rs"
printf '\n[[example]]\nname = "ut_larson_probe2"\nrequired-features = ["alloc-global", "bench-internals", "internals"]\n' >> "$D/Cargo.toml"
(cd "$D" && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgt-$L cargo build -q --release --example ut_larson_probe2 --features production,bench-internals,internals)
BIN=~/ab/tgt-$L/release/examples/ut_larson_probe2
LOG=$BASE/$L.log
{
  cat $BASE/$L.diffhash
  echo "bin $BIN"
  echo "workload $W threads 1 steps $STEPS"
} > "$LOG"
cd ~/ab
tag="$L.$W.t1.s$STEPS"
timeout 1800 valgrind --tool=callgrind --callgrind-out-file=$tag.out "$BIN" sefer $W $STEPS >/dev/null 2>&1
tot=$(callgrind_annotate $tag.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//')
echo "IR_TOTAL $W t1 $tot" >> "$LOG"
callgrind_annotate --inclusive=yes --threshold=100 $tag.out > $tag.annot 2>/dev/null || true
    callgrind_annotate --auto=yes --show-percs=no --threshold=100 $tag.out > $tag.annot-auto 2>/dev/null || true
    # Self-Ir attribution over the --auto=yes line annotation: each per-line
    # cost belongs to the file named in its section header (inlined code is
    # printed under its own inline-source file section). Aggregate per file;
    # skip inclusive "=>" rows and zero-cost "." rows. The flat file:function
    # rows at the top of the output are BEFORE the first Auto-annotated
    # section and are excluded (cur == "").
    attrf=$(mktemp)
    awk '
      /-- Auto-annotated source: / { cur=$0; sub(/.*Auto-annotated source: /,"",cur); next }
      cur == "" { next }
      /=>/ { next }
      /events annotated/ { next }
      /^[ 	]*[0-9]/ { n=$1; gsub(",","",n); sum[cur]+=n; tot+=n; next }
      END { for (f in sum) print f, sum[f]; print "ANNOT_TOTAL", tot }
    ' $tag.annot-auto > $attrf
    scan_ir=$(awk '/find_segment\/scan\.rs / {s+=$2} END{print s+0}' $attrf)
    bm_ir=$(awk '/remote_bitmap\/bitmap_scan\.rs / {s+=$2} END{print s+0}' $attrf)
    annot_tot=$(awk '/^ANNOT_TOTAL/ {print $2}' $attrf)
    uncover=$((tot - annot_tot))
    # Exact equality with PROGRAM TOTALS is unreachable when glibc/startup
    # sources are absent (callgrind lists them as "could not be found"); the
    # residual is reported as UNCOVERED Ir and the verdict uses the per-file
    # sums as a LOWER BOUND. RESIDUAL_OK = every .rs section annotated.
    st=RESIDUAL
    [ "$annot_tot" -eq "$tot" ] && st=ok
    echo "SELF_IR scan.rs=$scan_ir bitmap_scan.rs=$bm_ir sum=$((scan_ir + bm_ir)) total=$tot" >> "$LOG"
    echo "ATTR_TOTALS $st annotated=$annot_tot program=$tot uncovered=$uncover" >> "$LOG"
    rm -f $attrf
# Inclusive find_segment_with_free (and its inlined children) from the
# inclusive annotate.
grep -m3 'find_segment_with_free' $tag.annot >> "$LOG" || echo "find_segment_with_free: no inclusive row" >> "$LOG"
# Counters (no valgrind; plain Relaxed increments).
timeout 600 "$BIN" sefer $W $STEPS --counters > $tag.counters 2>&1
echo "COUNTERS $(tail -1 $tag.counters)" >> "$LOG"
cp $tag.out $tag.annot $tag.flat $tag.annot-auto $tag.counters "$LOG" $BASE/ 2>/dev/null || true
echo "== $LOG =="
cat "$LOG"
