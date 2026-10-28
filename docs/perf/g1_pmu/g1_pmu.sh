#!/bin/bash
# G1 contemplation: PMU counters, sefer vs mimalloc, on probe shapes of the
# J1′ kernels (WSL side). Usage: bash g1_pmu.sh <label> <identity>
# G1_BASE: directory holding <label>.tar; results go to <label>.log there.
set -e
L=$1; ID=${2:-unknown}
BASE=${G1_BASE:?set G1_BASE to the directory holding <label>.tar}
P=${PERF:-/usr/lib/linux-tools-6.8.0-147/perf}
D=~/ab/$L
rm -rf "$D"; mkdir -p "$D"; tar -xf $BASE/$L.tar -C "$D"
(cd "$D" && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgt-$L cargo build -q --release --example b4_pmu_probe --features production)
BIN=~/ab/tgt-$L/release/examples/b4_pmu_probe
LOG=$BASE/$L.log
{ echo "identity $ID"; echo "bin_sha256 $(sha256sum $BIN | cut -c1-64)"; echo "features production"; echo "cpu $(lscpu | grep 'Model name' | sed 's/.*: *//')"; } > "$LOG"
E1=cycles:u,instructions:u,cycle_activity.stalls_total:u,cycle_activity.stalls_l1d_miss:u,cycle_activity.stalls_mem_any:u,exe_activity.bound_on_stores:u
E2=cycles:u,mem_load_retired.l1_miss:u,mem_load_retired.l2_miss:u,l2_rqsts.rfo_miss:u,branch-misses:u,resource_stalls.sb:u
SC=(
  "bulk16k|bulk @ 16384 64 300"
  "bulk64|bulk @ 64 64 80000"
  "pair16|@ 1 16 20000000"
  "churn16|@ 256 16 20000000"
  "mixed16_64|mixed @ 256 16 64 128 20000000"
  "mixed256_1024|mixed @ 256 256 1024 48 10000000"
)
for r in 1 2 3; do
  for sc in "${SC[@]}"; do
    name=${sc%%|*}; args=${sc#*|}
    for a in sefer mi; do
      cmd=${args/@/$a}
      for g in 1 2; do
        ev=E$g
        out=$($P stat -x, -e ${!ev} -- taskset -c 2 $BIN $cmd 2>&1 >/tmp/g1_stdout.txt)
        echo "RUN $r $name $a $g $(cat /tmp/g1_stdout.txt)" >> "$LOG"
        echo "$out" | awk -F, -v r=$r -v n=$name -v a=$a -v g=$g 'NF>=3 && $3!="" {print "EV", r, n, a, g, $3, $1}' >> "$LOG"
      done
    done
  done
done
echo "== $LOG =="
