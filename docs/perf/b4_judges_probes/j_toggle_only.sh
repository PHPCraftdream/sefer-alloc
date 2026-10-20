#!/bin/bash
# Re-run selected toggle-collect globs reusing an existing probe dir/build.
# Usage (WSL): bash j_toggle_only.sh <label> <alloc> [workload]
set -e
L=$1; A=$2; W=${3:-larson}
D=~/ab/$L
BIN=~/ab/tgti-$L/release/examples/zz_larson_probe
cd ~/ab
for g in "$@"; do :; done
for g in flush_class_internal refill_magazine_slow; do
  for k in 50000 100000; do
    timeout 600 valgrind --tool=callgrind --toggle-collect="*$g*" --callgrind-out-file=pji.$L.$A.$W.$g.$k.out $BIN $A $W $k >/dev/null 2>&1
  done
  t1=$(callgrind_annotate pji.$L.$A.$W.$g.50000.out 2>/dev/null | awk '/PROGRAM TOTALS/{gsub(",","");print $1;exit}')
  t2=$(callgrind_annotate pji.$L.$A.$W.$g.100000.out 2>/dev/null | awk '/PROGRAM TOTALS/{gsub(",","");print $1;exit}')
  if [ -z "$t1" ] || [ -z "$t2" ]; then echo "TOGGLE $A $W $g marginal Ir/op: NOMATCH"; else
  echo "TOGGLE $A $W $g marginal Ir/op: $(( (t2 - t1) / 50000 ))"; fi
done
