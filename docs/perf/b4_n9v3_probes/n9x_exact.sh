#!/bin/bash
# Exact (fractional) marginal Ir/op from the callgrind outs left by prof_thrash64.sh.
cd ~/ab
for L in "$@"; do
  for w in w_alloc w_free; do
    t1=$(callgrind_annotate p64a.$L.sefer.$w.50000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//; s/PROGRAMTOTALS//')
    t2=$(callgrind_annotate p64a.$L.sefer.$w.100000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//; s/PROGRAMTOTALS//')
    awk -v l="$L" -v w="$w" -v a="$t1" -v b="$t2" 'BEGIN{printf "EXACT %s %s Ir/op = %.4f (TOTALS %s -> %s)\n", l, w, (b-a)/50000, a, b}'
  done
done
