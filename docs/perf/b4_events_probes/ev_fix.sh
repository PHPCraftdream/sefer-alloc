#!/bin/bash
# ev_ (b4-events stage 1): fix-up pass — robust toggles + call-count parse.
# Usage (WSL): bash ev_fix.sh <label> [larson|mstress]
# Reuses the build from ev_prof.sh (~/ab/ev<label>, ~/ab/tgte-<label>).
set -e
L=$1; W=${2:-larson}
cd ~/ab
BIN=~/ab/tgte-$L/release/examples/ev_larson_probe
T=1800
if [ ! -x $BIN ]; then
  BASE=$REPO_WSL/.rush/wsl
  D=~/ab/ev$L; tar -xf $BASE/ev_$L.tar -C $D || true
  (cd $D && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgte-$L cargo build -q --release --example ev_larson_probe --features production)
fi
echo "== TOGGLES (fix) $W T=1 =="
for g in refill_magazine_slow refill_magazine_slow_virgin finish_magazine_refill \
         flush_class_internal flush_run try_drain_freelist_batch \
         find_segment_with_free batch_drain_from_class_cursor carve_batch reserve_small_segment; do
  for k in 50000 100000; do
    timeout $T valgrind --tool=callgrind --toggle-collect="*$g*" \
      --callgrind-out-file=ev.$L.$W.$g.$k.out $BIN sefer $W $k >/dev/null 2>&1
  done
  t1=$(callgrind_annotate ev.$L.$W.$g.50000.out 2>/dev/null | awk '/PROGRAM TOTALS/{gsub(",","");print $1;exit}')
  t2=$(callgrind_annotate ev.$L.$W.$g.100000.out 2>/dev/null | awk '/PROGRAM TOTALS/{gsub(",","");print $1;exit}')
  case "$t1$t2" in ''|*[!0-9]*) echo "TOGGLE $W $g marginal Ir/op: NOMATCH";;
    *) echo "TOGGLE $W $g marginal Ir/op: $(( (t2 - t1) / 50000 ))";; esac
done
echo "== CALLS (fix, 100k trace) $W =="
# callgrind compressed format: 'fn=(id) name' defines id; later 'fn=(id)';
# 'calls=<' appears after 'cfn=(id)' or 'cfn=NAME'.
awk -v FS=" " '
  /^(c|c)fn=/ { v=$0; sub(/^(c)?fn=/,"",v); cur=v; next }
  /^(c|c)fn=\(/ { v=$0; sub(/^(c)?fn=\(/,"",v); sub(/\).*/,"",v); cur=nm[v]; next }
  /^fn=/ { v=$0; sub(/^fn=\(/,"",v); id=v; sub(/\).*/,"",id); n=$0; sub(/^[^ ]* /,"",n); nm[id]=n; next }
  /^cfn=\(/ { v=$0; sub(/^cfn=\(/,"",v); sub(/\).*/,"",v); cur=nm[v]; next }
  /^cfn=/ { v=$0; sub(/^cfn=/,"",v); cur=v; next }
  /^calls=/ { if (cur!="") { c[cur]+=$2 } }
  END { for (f in c) printf "%d\t%s\n", c[f], f }
' ev.$L.$W.full.100000.out | sort -rn | head -30
