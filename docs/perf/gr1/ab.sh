#!/bin/bash
# paired ABBA: judge_32 (A) vs judge_16 (B) vs judge_8 (C), arm sefer, rotated per rep
O=${G1_DIR:?set G1_DIR}/gr1/thr
: > $O/ab.jsonl
for sc in grow64 grow1k growmix; do
  for ((rep=0; rep<15; rep++)); do
    arr=(32 16 8); 
    for ((k=0;k<3;k++)); do
      t=${arr[$(( (k+rep) % 3 ))]}
      j=$($O/judge_$t $sc sefer 1 0.2 | tail -1)
      echo "{\"t\":$t,\"rep\":$rep,\"run\":$j}" >> $O/ab.jsonl
    done
  done
done
echo ab-done
