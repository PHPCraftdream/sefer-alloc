#!/bin/bash
# CF-3 sweep: one process per run, arm order rotated per rep (cyclic Latin square). usage: sweep.sh REPS SCALE
O=${CF3_DIR:?set CF3_DIR}/out
REPS=$1; SCALE=$2
SCEN="grow64 grow1k growmix regrow wild phase"
LABELS=(sefer t0 t1 t4 t16 t64 t1024 mi)
: > $O/evidence.jsonl
for N in 0 1 4 16 64 1024; do
  echo "{\"want\":$N,\"out\":$($O/bin/ev_$N regrow sefer_pool 1 0.1 | tail -1)}" >> $O/evidence.jsonl
done
: > $O/raw.jsonl
n=${#LABELS[@]}
for sc in $SCEN; do
  for ((rep=0; rep<REPS; rep++)); do
    for ((k=0; k<n; k++)); do
      l=${LABELS[$(( (k+rep) % n ))]}
      case $l in
        sefer) b=time_16; a=sefer;;
        mi) b=time_16; a=mi;;
        t*) b=time_${l#t}; a=sefer_pool;;
      esac
      j=$( { /usr/bin/time -f "%R" $O/bin/$b $sc $a 1 $SCALE | tail -1; } 2>$O/_t ); f=$(tail -1 $O/_t)
      echo "{\"label\":\"$l\",\"rep\":$rep,\"faults\":$f,\"run\":$j}" >> $O/raw.jsonl
    done
  done
  echo "done $sc"
done
echo sweep-done
