#!/bin/bash
# CF-3 mechanism oracle: hint counter, resolved threshold, huge-page bytes, faults per threshold (bench-internals builds).
O=${CF3_DIR:?set CF3_DIR}/out
REPS=$1
: > $O/mech.jsonl
for sc in grow64 grow1k growmix regrow wild phase; do
  for ((rep=0; rep<REPS; rep++)); do
    for N in 0 1 4 16 64 1024; do
      j=$( { /usr/bin/time -f "%R" $O/bin/ev_$N $sc sefer_pool 1 1 | tail -1; } 2>$O/_t ); f=$(tail -1 $O/_t)
      echo "{\"want\":$N,\"rep\":$rep,\"faults\":$f,\"run\":$j}" >> $O/mech.jsonl
    done
  done
done
echo mech-done
