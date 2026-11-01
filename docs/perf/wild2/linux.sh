#!/bin/bash
source ~/.cargo/env
W=<dir with base.tar, x.tar, linux_schedule.txt>; S=$HOME/w2
rm -rf $S/b $S/x; mkdir -p $S/b $S/x
tar -xf $W/base.tar -C $S/b; tar -xf $W/x.tar -C $S/x
for L in b x; do (cd $S/$L && CARGO_TARGET_DIR=$S/tgt-$L cargo build -q --release --example g1_wild_judge --features "production internals") || exit 1; done
B=$S/tgt-b/release/examples/g1_wild_judge; cp $B $S/base2; X=$S/tgt-x/release/examples/g1_wild_judge
{ echo "base: git archive of c0ee7dfb; x: git archive of the g1-wild2 head (sha256 of tars: $(sha256sum $W/base.tar | cut -c1-64) $(sha256sum $W/x.tar | cut -c1-64))"; rustc -V; uname -sr; sha256sum $B $S/base2 $X | sed "s#$S/##"; } > $W/linux_ident.txt
: > $W/linux_raw.jsonl
tr -d '\r' < $W/linux_schedule.txt | while read b arm sc; do
  case $arm in base) E=$B;; base2) E=$S/base2;; x) E=$X;; esac
  case $sc in grow64|grow1k|regrow|growmix) SC=0.5;; *) SC=1;; esac
  r=$($E $sc sefer 1 $SC) || exit 1
  echo "{\"block\":$b,\"arm\":\"$arm\",\"r\":$r}" >> $W/linux_raw.jsonl
done
echo linux-done
# C5: callgrind Ir (deterministic), sefer arm
: > $W/callgrind_totals.txt
cg() { valgrind --tool=callgrind --callgrind-out-file=$S/cg.out $1 $2 sefer 1 $3 > /dev/null 2>&1; callgrind_annotate $S/cg.out 2>/dev/null | grep 'PROGRAM TOTALS' | awk '{print $1}' | tr -d ,; }
for sc in "wild 0.3" "phase 0.05" "burst 0.1"; do
  set -- $sc
  echo "$1 base $(cg $B $1 $2)" >> $W/callgrind_totals.txt
  echo "$1 x $(cg $X $1 $2)" >> $W/callgrind_totals.txt
done
cat $W/callgrind_totals.txt
# supporting (not a gate): perf stat, user mode, 5 runs per arm and scenario
P=/usr/lib/linux-tools-6.8.0-147/perf
: > $W/perf_stat_raw.txt
for i in 1 2 3 4 5; do
  for arm in base x; do
    case $arm in base) E=$B;; x) E=$X;; esac
    for sc in wild phase burst; do
      out=$($P stat -x, -e cycles:u,instructions:u,L1-dcache-load-misses:u -- $E $sc sefer 1 1 2>&1 >/dev/null | tr '\n' ' ')
      echo "run$i $sc $arm $out" >> $W/perf_stat_raw.txt
    done
  done
done
echo linux-all-done
