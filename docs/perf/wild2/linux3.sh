#!/bin/bash
source ~/.cargo/env
W=<dir with linux2_schedule.txt>; S=$HOME/w2
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
for L in b x; do (cd $S/$L && CARGO_TARGET_DIR=$S/tgta-$L cargo build -q --release --example g1_wild_judge --features "production internals") || exit 1; done
unset RUSTFLAGS
B=$S/tgta-b/release/examples/g1_wild_judge; X=$S/tgta-x/release/examples/g1_wild_judge; cp $B $S/base2a
{ echo "aligned-code builds (RUSTFLAGS align-all-functions=6, align-all-nofallthru-blocks=5) of the same trees as linux_ident.txt"; sha256sum $B $S/base2a $X | sed "s#$S/##"; } > $W/linux3_ident.txt
: > $W/linux3_raw.jsonl
tr -d '\r' < $W/linux2_schedule.txt | while read b arm sc; do
  case $arm in base) E=$B;; base2) E=$S/base2a;; x) E=$X;; esac
  case $sc in regrow|growmix) SC=0.5;; *) SC=1;; esac
  r=$($E $sc sefer 1 $SC) || exit 1
  echo "{\"block\":$b,\"arm\":\"$arm\",\"r\":$r}" >> $W/linux3_raw.jsonl
done
P=/usr/lib/linux-tools-6.8.0-147/perf
: > $W/perf_stat_aligned_raw.txt
for i in 1 2 3 4 5; do
  for arm in base x; do
    case $arm in base) E=$B;; x) E=$X;; esac
    for sc in wild phase burst; do
      out=$($P stat -x, -e cycles:u,instructions:u,L1-dcache-load-misses:u -- $E $sc sefer 1 1 2>&1 >/dev/null | tr '\n' ' ')
      echo "run$i $sc $arm $out" >> $W/perf_stat_aligned_raw.txt
    done
  done
done
echo linux3-done
