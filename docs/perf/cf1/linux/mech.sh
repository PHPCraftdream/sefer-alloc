#!/bin/bash
# CF-1 mechanism oracle: segment reserve/release counters, resolved cap and minor page faults per pool size
# (deterministic, layout-insensitive). One process per run, scale 1, REPS repetitions.
O=${CF1_DIR:?set CF1_DIR}/out
REPS=$1
: > $O/mech.jsonl
for sc in grow64 grow1k growmix regrow large wild phase; do
  for ((rep=0; rep<REPS; rep++)); do
    for N in 0 4 8 16 32 64 1024; do
      j=$( { /usr/bin/time -f "%R" $O/bin/ev_$N $sc sefer_pool 1 1 | tail -1; } 2>$O/_t ); f=$(tail -1 $O/_t)
      echo "{\"want\":$N,\"rep\":$rep,\"faults\":$f,\"run\":$j}" >> $O/mech.jsonl
    done
  done
done
echo mech-done
