#!/bin/bash
source ~/.cargo/env
W=<dir with linux2_schedule.txt>; S=$HOME/w2
B=$S/tgt-b/release/examples/g1_wild_judge; X=$S/tgt-x/release/examples/g1_wild_judge
cmp $B $S/base2 || exit 1
: > $W/linux2_raw.jsonl
tr -d '\r' < $W/linux2_schedule.txt | while read b arm sc; do
  case $arm in base) E=$B;; base2) E=$S/base2;; x) E=$X;; esac
  case $sc in regrow|growmix) SC=0.5;; *) SC=1;; esac
  r=$($E $sc sefer 1 $SC) || exit 1
  echo "{\"block\":$b,\"arm\":\"$arm\",\"r\":$r}" >> $W/linux2_raw.jsonl
done
cg() { valgrind --tool=callgrind --callgrind-out-file=$S/cg.out $1 $2 sefer 1 $3 > /dev/null 2>&1; callgrind_annotate $S/cg.out 2>/dev/null | grep 'PROGRAM TOTALS' | awk '{print $1}' | tr -d ,; }
echo "regrow base $(cg $B regrow 0.25)" >> $W/callgrind_totals.txt
echo "regrow x $(cg $X regrow 0.25)" >> $W/callgrind_totals.txt
echo linux2-done
