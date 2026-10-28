#!/bin/bash
# B4-25 ceiling: callgrind per-file self Ir, mstress/larson at T=1,2,4 (WSL side).
# Usage: bash b425_prof.sh <label> <identity>
set -e
L=$1; ID=${2:-unknown}
BASE=${B425_BASE:?set B425_BASE to the directory holding <label>.tar}
D=~/ab/$L
rm -rf "$D"; mkdir -p "$D"; tar -xf $BASE/$L.tar -C "$D"
(cd "$D" && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgt-$L cargo build -q --release --example b425_mt_probe --features production)
BIN=~/ab/tgt-$L/release/examples/b425_mt_probe
LOG=$BASE/$L.log
{ echo "identity $ID"; echo "bin_sha256 $(sha256sum $BIN | cut -c1-64)"; echo "features production"; } > "$LOG"
cd ~/ab
for wl in mstress larson; do
  for t in 1 2 4; do
    for mode in sefer mimalloc; do
      tag="$L.$mode.$wl.t$t"
      timeout 1800 valgrind --tool=callgrind --callgrind-out-file=$tag.out "$BIN" $mode $wl 50000 $t > $tag.stdout 2>/dev/null
      tot=$(callgrind_annotate $tag.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//')
      echo "IR_TOTAL $mode $wl $t $tot" >> "$LOG"
      [ $mode = sefer ] || continue
      callgrind_annotate --auto=yes --show-percs=no --threshold=100 $tag.out > $tag.annot-auto 2>/dev/null || true
      awk -v tag="$mode $wl $t" '
        /-- Auto-annotated source: / { cur=$0; sub(/.*Auto-annotated source: /,"",cur); sub(/.*\/src\//,"src/",cur); next }
        cur == "" { next }
        /=>/ { next }
        /events annotated/ { next }
        /^[ \t]*[0-9]/ { n=$1; gsub(",","",n); sum[cur]+=n; next }
        END { for (f in sum) if (sum[f] > 0) print "FILE_IR", tag, f, sum[f] }
      ' $tag.annot-auto | sort -k6 -n -r >> "$LOG"
      callgrind_annotate --threshold=99 $tag.out | grep -E "^ *[0-9,]+ " | head -40 | sed "s/^/FN_SELF $mode $wl $t /" >> "$LOG"
    done
  done
done
cp $L.*.out $BASE/ 2>/dev/null || true
echo "== $LOG =="
