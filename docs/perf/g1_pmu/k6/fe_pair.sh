#!/bin/bash
# fe_pair.sh <base-label> <var-label> <reps>: PMU, sefer arm, prebuilt probes,
# base/variant interleaved per (rep, scenario, group) -> <label>.fep.log
set -e
A=$1; V=$2; R=${3:-7}
BASE=${G1_BASE:?set G1_BASE to the directory holding the logs}
P=/usr/lib/linux-tools-6.8.0-147/perf
E1=cycles:u,instructions:u,exe_activity.bound_on_stores:u,resource_stalls.sb:u
E2=cycles:u,cycle_activity.stalls_total:u,cycle_activity.stalls_mem_any:u,ld_blocks.store_forward:u
SC=("bulk16k|bulk sefer 16384 64 300" "bulk64|bulk sefer 64 64 80000" "mixed256_1024|mixed sefer 256 256 1024 48 10000000" "mixed16_64|mixed sefer 256 16 64 128 20000000" "pair16|sefer 1 16 20000000")
for L in $A $V; do
  B=~/ab/tgt-$L/release/examples/b4_pmu_probe
  { echo "identity $L"; echo "bin_sha256 $(sha256sum $B | cut -c1-64)"; echo "features production"; echo "cpu $(lscpu | grep 'Model name' | sed 's/.*: *//')"; } > $BASE/$L.fep.log
done
for r in $(seq 1 $R); do
  for sc in "${SC[@]}"; do
    name=${sc%%|*}; cmd=${sc#*|}
    for g in 1 2; do
      ev=E$g
      if [ $((r % 2)) = 1 ]; then ORD="$A $V"; else ORD="$V $A"; fi
      for L in $ORD; do
        B=~/ab/tgt-$L/release/examples/b4_pmu_probe
        out=$($P stat -x, -e ${!ev} -- taskset -c 2 $B $cmd 2>&1 >/tmp/fep_stdout.txt)
        echo "RUN $r $name sefer $g $(cat /tmp/fep_stdout.txt)" >> $BASE/$L.fep.log
        echo "$out" | awk -F, -v r=$r -v n=$name -v g=$g 'NF>=3 && $3!="" {print "EV", r, n, "sefer", g, $3, $1}' >> $BASE/$L.fep.log
      done
    done
  done
done
echo done
