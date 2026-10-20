#!/bin/bash
# ev_ (b4-events stage 1): callgrind layout of refill/flush event costs.
# Usage (WSL): bash ev_prof.sh <label> [larson|mstress]
# Expects .rush/wsl/ev_<label>.tar (made by ev_prof_win.sh) — a working-tree
# snapshot of worktrees/b4-events. Builds examples/ev_larson_probe
# (production, release + line tables), then:
#   1) --toggle-collect per event/phase function, marginal Ir/op 50k->100k
#   2) untoggled full profile: marginal Ir/op + call counts (event sizes,
#      discovery/carve split)
#   3) --cache-sim=yes run: D1 miss marginal/op
#   4) line annotation (--auto=yes) of key allocator files
set -e
L=$1; W=${2:-larson}
BASE=$REPO_WSL/.rush/wsl
D=~/ab/ev$L; rm -rf $D; mkdir -p $D; tar -xf $BASE/ev_$L.tar -C $D
cp $BASE/larson_probe.rs $D/examples/ev_larson_probe.rs
printf '\n[[example]]\nname = "ev_larson_probe"\nrequired-features = ["alloc-global"]\n' >> $D/Cargo.toml
(cd $D && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgte-$L cargo build -q --release --example ev_larson_probe --features production)
cd ~/ab
BIN=~/ab/tgte-$L/release/examples/ev_larson_probe
T=1800
echo "== EVENT LAYOUT $W T=1 (label $L) =="
# 1) toggles: marginal Ir/op while inside matched fn (+callees/inlines)
for g in refill_magazine_slow refill_magazine_slow_virgin finish_magazine_refill \
         flush_class_internal flush_run try_drain_freelist_batch \
         find_segment_with_free batch_drain_from_class_cursor carve_batch reserve_small_segment; do
  for k in 50000 100000; do
    timeout $T valgrind --tool=callgrind --toggle-collect="*$g*" \
      --callgrind-out-file=ev.$L.$W.$g.$k.out $BIN sefer $W $k >/dev/null 2>&1
  done
  t1=$(callgrind_annotate ev.$L.$W.$g.50000.out 2>/dev/null | awk '/PROGRAM TOTALS/{gsub(",","");print $1;exit}')
  t2=$(callgrind_annotate ev.$L.$W.$g.100000.out 2>/dev/null | awk '/PROGRAM TOTALS/{gsub(",","");print $1;exit}')
  if [ -z "$t1" ] || [ -z "$t2" ]; then echo "TOGGLE $W $g marginal Ir/op: NOMATCH"; else
  echo "TOGGLE $W $g marginal Ir/op: $(( (t2 - t1) / 50000 ))"; fi
done
# 2) full untoggled profile, marginal Ir/op + call counts
for k in 50000 100000; do
  timeout $T valgrind --tool=callgrind --callgrind-out-file=ev.$L.$W.full.$k.out $BIN sefer $W $k >/dev/null 2>&1
done
tot1=$(callgrind_annotate ev.$L.$W.full.50000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//')
tot2=$(callgrind_annotate ev.$L.$W.full.100000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//')
echo "FULL $W marginal Ir/op: $(( (tot2 - tot1) / 50000 ))"
# call counts in the 100k trace (callgraph: sum calls= lines following cfn of name)
for g in refill_magazine_slow refill_magazine_slow_virgin flush_class_internal flush_run \
         try_drain_freelist_batch find_segment_with_free batch_drain_from_class_cursor \
         carve_batch reserve_small_segment finish_magazine_refill; do
  n=$(awk -v g="$g" '
    /^cfn=/ { cfn=$0; sub(/^cfn=/,"",cfn); inc=(index(cfn,g)>0); next }
    inc && /^calls=/ { split($0,a," "); s+=a[2]; inc=0 }
    END { print s+0 }' ev.$L.$W.full.100000.out)
  echo "CALLS $W $g: $n"
done
# 3) cache-sim run (100k only)
timeout $T valgrind --tool=callgrind --cache-sim=yes --callgrind-out-file=ev.$L.$W.cache.100000.out $BIN sefer $W $k >/dev/null 2>&1
echo "CACHE TOTALS (100k): $(callgrind_annotate ev.$L.$W.cache.100000.out 2>/dev/null | grep -m1 -E 'TOTALS' )"
callgrind_annotate ev.$L.$W.cache.100000.out 2>/dev/null | grep -E 'Dr:|D1mr:|ILmr:' | head -5
# 4) line annotation of the full 100k profile
timeout $T true
callgrind_annotate --auto=yes --show-percs=no --threshold=100 ev.$L.$W.full.100000.out > ev.$L.$W.auto.txt 2>/dev/null || true
echo "== LINE ANNOTATION (key files) =="
for f in alloc_core_small_impl.rs alloc_core_small_magazine.rs hot.rs class_cursor.rs scan.rs; do
  awk -v pat="$f" '/-- Auto-annotated source: /{src=$0} src ~ pat && /^ *[0-9]/ {print}' ev.$L.$W.auto.txt | grep -v '=>' | head -40
done | head -150
