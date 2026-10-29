#!/bin/bash
# DZ-2: perf stat (cycles/instructions) + flamegraphs of the bulk kernel, production vs production bench-internals.
source ~/.cargo/env 2>/dev/null; export PATH=$HOME/.cargo/bin:$PATH
PERF=${PERF:-perf}
O=~/ab/dz2; W=${REPO:?set REPO to the repo root seen from WSL}/.rush/g1/dz2/fg; mkdir -p $O $W
for v in prod bi; do
  B=~/ab/tgt-dz2-$v/release/examples/b4_pmu_probe; [ -x $B ] || { echo missing $B; exit 2; }
  for sz in 16 64; do
    for r in 1 2 3; do
      echo "$v $sz r$r $($PERF stat -x, -e cycles:u,instructions:u $B bulk sefer 1024 $sz 50000 2>&1 >/dev/null | awk -F, '{printf "%s=%s ", $3, $1}')"
    done
    $PERF record -q -F 4999 -g --call-graph dwarf,16384 -o $O/perf.$v.$sz.data $B bulk sefer 1024 $sz 50000 >/dev/null 2>&1
    $PERF script -i $O/perf.$v.$sz.data 2>/dev/null | inferno-collapse-perf > $O/folded.$v.$sz.txt
    inferno-flamegraph --title "DZ-2 $v bulk sefer 1024x${sz}B" < $O/folded.$v.$sz.txt > $W/fg.$v.$sz.svg
    cp $O/folded.$v.$sz.txt $W/
    $PERF report -i $O/perf.$v.$sz.data --no-children --stdio --sort sym 2>/dev/null | grep -E '^ +[0-9]' | head -15 > $W/top.$v.$sz.txt
  done
done
for sz in 16 64; do inferno-diff-folded $O/folded.prod.$sz.txt $O/folded.bi.$sz.txt | inferno-flamegraph --title "DZ-2 diff prod->bi ${sz}B" > $W/fg.diff.$sz.svg; done
echo done
