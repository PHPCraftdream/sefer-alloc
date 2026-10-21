#!/bin/bash
: "${REPO_WSL:?set REPO_WSL to the repository root}"
# ut_prof_u31.sh — B4-29/U31 trigger growth16 callgrind profile (WSL side).
# Usage: bash ut_prof_u31.sh <label>   (label is prefixed ut- by the _win wrapper)
# Expects .rush/wsl/<label>.tar (working-tree snapshot of <checkout>,
# made by ut_prof_u31_win.sh). Unpacks to ~/ab/<label>, adds
# examples/ut_u31_probe.rs (copy of .rush/wsl/ut_u31_probe.rs), builds it with
# production + bench-internals + internals (release + line tables), then for
# each of the six arms (sefer|mimalloc x 32768|65536|131072) runs callgrind
# once. Also runs the sefer arms WITHOUT valgrind to read the B4_* observer
# counters (discovery oracle). Attribution: callgrind_annotate --inclusive=yes
# totals plus --auto=yes line annotation, filtered for
# src/alloc_core/small/alloc_core_small/find_segment/scan.rs and
# src/alloc_core/segment/remote_bitmap/bitmap_scan.rs.
# All artifacts: ~/ab/ut-*.{out,txt} copied back to
# ${REPO_WSL}/.rush/wsl/ as ut-*. {out,annot,annot-auto,counters,log}
set -e
L=$1; DIFFHASH=${2:-unknown}
BASE=${REPO_WSL}/.rush/wsl
WT=${REPO_WSL}
D=~/ab/$L
rm -rf "$D"; mkdir -p "$D"; tar -xf $BASE/$L.tar -C "$D"
cp $BASE/ut_u31_probe.rs "$D/examples/ut_u31_probe.rs"
printf '\n[[example]]\nname = "ut_u31_probe"\nrequired-features = ["alloc-global", "bench-internals", "internals"]\n' >> "$D/Cargo.toml"
DIFFHASH=$DIFFHASH
(cd "$D" && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgt-$L cargo build -q --release --example ut_u31_probe --features production,bench-internals,internals)
BIN=~/ab/tgt-$L/release/examples/ut_u31_probe
LOG=$BASE/$L.log
{
  echo "git_diff_sha256 $DIFFHASH"
  echo "bin $BIN"
} > "$LOG"
cd ~/ab
for mode in sefer mimalloc; do
  for n in 32768 65536 131072; do
    tag="$L.$mode.n$n"
    timeout 1800 valgrind --tool=callgrind --callgrind-out-file=$tag.out "$BIN" $mode $n >/dev/null 2>&1
    tot=$(callgrind_annotate $tag.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//')
    echo "IR_TOTAL $mode $n $tot" >> "$LOG"
    callgrind_annotate --inclusive=yes --threshold=100 $tag.out > $tag.annot 2>/dev/null || true
    # per-line annotation of the two discovery-scan files (raw artifact; NOTE:
    # auto-annotated per-line sums double-count inlined frames across files —
    # the SCAN_IR numbers below are from the self-Ir section instead).
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
  done
done
# Discovery oracle: counters around the same sefer bodies (no valgrind needed;
# counters are plain Relaxed increments).
for n in 32768 65536 131072; do
  echo "COUNTERS $($BIN sefer $n --counters)" >> "$LOG"
done
cp $L.*.out $L.*.annot $L.*.annot-auto $L.*.flat "$LOG" $BASE/ 2>/dev/null || true
echo "== $LOG =="
cat "$LOG"
