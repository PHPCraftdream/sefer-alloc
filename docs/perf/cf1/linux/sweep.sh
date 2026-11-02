#!/bin/bash
# CF-1 sweep: one process per run, arm order rotated per rep (cyclic Latin square), all in one session.
# usage: sweep.sh REPS SCALE [scenarios...]
W=${CF1_DIR:?set CF1_DIR}
O=$W/out
REPS=$1; SCALE=$2; shift 2
SCEN=${@:-grow64 grow1k growmix regrow large wild phase}
LABELS=(sefer p0 p4 p8 p16 p32 p64 p1024 mi)
: > $O/evidence.jsonl
for N in 0 4 8 16 32 64 1024; do
  echo "{\"want\":$N,\"out\":$($O/bin/ev_$N regrow sefer_pool 1 0.1 | tail -1)}" >> $O/evidence.jsonl
done
: > $O/raw.jsonl
n=${#LABELS[@]}
for sc in $SCEN; do
  for ((rep=0; rep<REPS; rep++)); do
    for ((k=0; k<n; k++)); do
      l=${LABELS[$(( (k+rep) % n ))]}
      case $l in
        sefer) b=time_1024; a=sefer;;
        mi) b=time_1024; a=mi;;
        p*) b=time_${l#p}; a=sefer_pool;;
      esac
      j=$($O/bin/$b $sc $a 1 $SCALE | tail -1) || { echo "FAIL $sc $l" >&2; continue; }
      echo "{\"label\":\"$l\",\"rep\":$rep,\"run\":$j}" >> $O/raw.jsonl
    done
  done
  echo "done $sc"
done
echo sweep-done
