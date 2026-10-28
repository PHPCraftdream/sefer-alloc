#!/bin/bash
# G1-K3: frontend / uop PMU counters, sefer arm only, base vs variant (WSL side).
# Usage: bash g1_fe.sh <label> <identity>   (G1_BASE holds <label>.tar)
set -e
L=$1; ID=${2:-unknown}
BASE=${G1_BASE:?set G1_BASE to the directory holding <label>.tar}
P=${PERF:-/usr/lib/linux-tools-6.8.0-147/perf}
D=~/ab/$L
rm -rf "$D"; mkdir -p "$D"; tar -xf $BASE/$L.tar -C "$D"
(cd "$D" && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgt-$L cargo build -q --release --example b4_pmu_probe --features production)
BIN=~/ab/tgt-$L/release/examples/b4_pmu_probe
LOG=$BASE/$L.fe.log
{ echo "identity $ID"; echo "bin_sha256 $(sha256sum $BIN | cut -c1-64)"; echo "features production"; echo "cpu $(lscpu | grep 'Model name' | sed 's/.*: *//')"; } > "$LOG"
E1=cycles:u,instructions:u,cycle_activity.stalls_total:u,cycle_activity.stalls_mem_any:u,exe_activity.bound_on_stores:u,branch-misses:u
E2=cycles:u,idq.dsb_uops:u,idq.mite_uops:u,dsb2mite_switches.penalty_cycles:u,icache_data.stalls:u,decode.lcp:u
E3=cycles:u,uops_issued.any:u,uops_retired.slots:u,int_misc.recovery_cycles:u,ld_blocks.store_forward:u,resource_stalls.sb:u
SC=("bulk16k|bulk @ 16384 64 300" "bulk64|bulk @ 64 64 80000" "mixed256_1024|mixed @ 256 256 1024 48 10000000")
for r in 1 2 3 4 5; do
  for sc in "${SC[@]}"; do
    name=${sc%%|*}; args=${sc#*|}; cmd=${args/@/sefer}
    for g in 1 2 3; do
      ev=E$g
      out=$($P stat -x, -e ${!ev} -- taskset -c 2 $BIN $cmd 2>&1 >/tmp/g1fe_stdout.txt)
      echo "RUN $r $name sefer $g $(cat /tmp/g1fe_stdout.txt)" >> "$LOG"
      echo "$out" | awk -F, -v r=$r -v n=$name -v g=$g 'NF>=3 && $3!="" {print "EV", r, n, "sefer", g, $3, $1}' >> "$LOG"
    done
  done
done
echo "== $LOG =="
